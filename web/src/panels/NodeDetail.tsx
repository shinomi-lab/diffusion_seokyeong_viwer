// ノード詳細（F10・F19）。「ノード」「ノード群」のタブ。効用は 1 を超えても上限なしで表示する。
import { useEffect, useMemo, useState } from 'react';
import { chooseIpAll } from '../store/actions';
import { useSession } from '../store/session';
import { useView } from '../store/view';
import { T } from '../tokens';
import { sim } from '../worker/client';
import type { GroupSeriesJson, NodeDetailJson } from '../worker/protocol';
import { UPlotChart } from './Plot';

const GROUPS = [
  { id: 0, label: '内部のみ' },
  { id: 1, label: '外部のみ' },
  { id: 2, label: '両方' },
  { id: 3, label: '受信のみ' },
  { id: 4, label: '選択した群（リング・範囲）' },
] as const;

export function NodeDetail() {
  const tab = useView((s) => s.tab);
  return (
    <aside className="detail" aria-label="ノード詳細">
      <div className="tabs" role="tablist">
        <button role="tab" aria-selected={tab === 'node'} className={tab === 'node' ? 'on' : ''} onClick={() => useView.getState().setTab('node')}>ノード</button>
        <button role="tab" aria-selected={tab === 'group'} className={tab === 'group' ? 'on' : ''} onClick={() => useView.getState().setTab('group')}>ノード群</button>
      </div>
      {tab === 'node' ? <SingleNode /> : <GroupNodes />}
    </aside>
  );
}

function SingleNode() {
  const node = useView((s) => s.selectedNode);
  const rounds = useSession((s) => s.rounds);
  const ip = useSession((s) => s.ip);
  const [d, setD] = useState<NodeDetailJson | null>(null);

  useEffect(() => {
    if (node === null || ip === null) {
      setD(null);
      return;
    }
    let cancel = false;
    sim.call('nodeDetail', { node }).then((s) => !cancel && setD(JSON.parse(s))).catch(() => undefined);
    return () => {
      cancel = true;
    };
  }, [node, rounds, ip]);

  const data = useMemo(() => (d ? ([d.u_int.map((_, i) => i), d.u_int, d.u_ext] as [number[], number[], number[]]) : null), [d]);
  if (node === null) return <p className="hint">ネットワーク上のノードをクリックすると、効用の推移・受信したラウンドと受信元・活性になったラウンドを表示します。</p>;
  if (!d || !data) return <p className="hint">読み込み中…</p>;
  return (
    <div className="detail-body">
      <h3>ノード {d.node}（ラベル {d.label}）{d.is_ip && <span className="badge">IP</span>}</h3>
      <UPlotChart
        data={data}
        xLabel="ラウンド"
        series={[{ label: '内部効用', color: T.series.internal }, { label: '外部効用', color: T.series.external }]}
      />
      <p className="note">縦軸は値に合わせて自動で広がります（1 を超えても切りません）。最大値: 内部 {Math.max(...d.u_int).toFixed(3)} / 外部 {Math.max(...d.u_ext).toFixed(3)}</p>
      <dl className="kv">
        <dt>内部が成立したラウンド</dt><dd>{d.int_rounds.length ? d.int_rounds.join(', ') : '—'}</dd>
        <dt>外部行動のラウンド</dt><dd>{d.ext_round ?? '—'}</dd>
      </dl>
      <table className="mini">
        <thead><tr><th>受信ラウンド</th><th>ウェーブ</th><th>受信元（ラベル）</th><th>経路数</th></tr></thead>
        <tbody>
          {d.received.length === 0 && <tr><td colSpan={4}>まだ受信していません</td></tr>}
          {d.received.map((r) => <tr key={r.round}><td>{r.round}</td><td>{r.wave + 1}</td><td>{r.from_label}</td><td>{r.n_arr}</td></tr>)}
        </tbody>
      </table>
      {!d.is_ip && <button onClick={() => void chooseIpAll(d.node)}>このノードを IP にする（履歴は消えます）</button>}
    </div>
  );
}

function GroupNodes() {
  const group = useView((s) => s.groupNodes);
  const intMode = useView((s) => s.intMode);
  const rounds = useSession((s) => s.rounds);
  const ip = useSession((s) => s.ip);
  const [sel, setSel] = useState<number>(2);
  const [g, setG] = useState<GroupSeriesJson | null>(null);
  const [err, setErr] = useState<string | null>(null);

  useEffect(() => {
    if (ip === null) return;
    let cancel = false;
    sim
      .call('groupSeries', { sel, atRound: rounds.length, intMode, customNodes: sel === 4 ? (group ?? new Uint32Array(0)).slice() : undefined })
      .then((s) => {
        if (!cancel) {
          setG(JSON.parse(s));
          setErr(null);
        }
      })
      .catch((e: Error) => !cancel && setErr(e.message));
    return () => {
      cancel = true;
    };
  }, [sel, rounds, intMode, group, ip]);

  const util = useMemo(() => {
    if (!g) return null;
    const x = g.int.mean.map((_, i) => i);
    return [x, g.int.mean, g.int.p10, g.int.p90, g.ext.mean, g.ext.p10, g.ext.p90] as [number[], ...number[][]];
  }, [g]);
  const sizes = useMemo(() => (g ? ([g.size_by_round.map((_, i) => i), g.size_by_round] as [number[], number[]]) : null), [g]);

  return (
    <div className="detail-body">
      <label className="row">群の種類
        <select value={sel} onChange={(e) => setSel(Number(e.target.value))}>
          {GROUPS.map((o) => <option key={o.id} value={o.id}>{o.label}{o.id === 4 ? `（${group?.length ?? 0}）` : ''}</option>)}
        </select>
      </label>
      {err && <p className="error">{err}</p>}
      {g && util && sizes && (
        <>
          <p className="note">表示中のラウンド（{g.at_round}）の時点で群を決め、メンバー {g.members} ノードを固定して過去へさかのぼります。</p>
          <UPlotChart
            data={util}
            xLabel="ラウンド"
            series={[
              { label: '内部 平均', color: T.series.internal },
              { label: '内部 p10', color: T.series.internal, hidden: true, width: 0 },
              { label: '内部 p90', color: T.series.internal, hidden: true, width: 0 },
              { label: '外部 平均', color: T.series.external },
              { label: '外部 p10', color: T.series.external, hidden: true, width: 0 },
              { label: '外部 p90', color: T.series.external, hidden: true, width: 0 },
            ]}
            bands={[[3, 2, T.series.internal], [6, 5, T.series.external]]}
          />
          <h4>群の大きさの推移</h4>
          <UPlotChart data={sizes} height={110} xLabel="ラウンド" series={[{ label: '群の大きさ', color: T.series.both }]} />
        </>
      )}
      {sel === 4 && !group && <p className="hint">Shift を押しながらネットワーク上をドラッグすると範囲選択で群を作れます。全体図ではリングの「展開」から作れます。</p>}
    </div>
  );
}
