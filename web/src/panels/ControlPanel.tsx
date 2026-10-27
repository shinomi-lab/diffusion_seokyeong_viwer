// 操作パネル（9.3）: グラフ設定 → シナリオ設定 → 機構 M1（paramSpecs から自動生成）→ プリセット → IP 選択 → メッセージ → 発信。
import { useEffect, useState } from 'react';
import type { ScenarioForm } from '../logic/config';
import { msgName } from '../logic/units';
import { usePlayback } from '../store/playback';
import { applyAll, chooseIpAll, loadGraphAll, restartAll, stepAll } from '../store/actions';
import { isDirty, useSession } from '../store/session';
import { MechanismEditor } from './MechanismEditor';
import { RecommenderEditor } from './RecommenderEditor';
import { RecSimTable } from './RecSimTable';
import { RecWaveTable } from './RecWaveTable';
import { ComparePanel } from './ComparePanel';

let lastGraph: { bytes: Uint8Array; name: string } | null = null;

function guessOpts(name: string): { format: 'edgelist' | 'csv' | 'tsv'; gzip: boolean } {
  const gzip = name.toLowerCase().endsWith('.gz');
  const base = gzip ? name.slice(0, -3) : name;
  const ext = base.split('.').pop()?.toLowerCase();
  return { gzip, format: ext === 'csv' ? 'csv' : ext === 'tsv' ? 'tsv' : 'edgelist' };
}

export async function loadGraphFile(file: File): Promise<void> {
  const bytes = new Uint8Array(await file.arrayBuffer());
  lastGraph = { bytes, name: file.name };
  const g = guessOpts(file.name);
  useSession.getState().setGraphOpts(g);
  await loadGraphAll(bytes, file.name, g);
  usePlayback.getState().setLast(-1);
}

export async function loadSample(): Promise<void> {
  const res = await fetch('./samples/100random.txt');
  if (!res.ok) throw new Error('サンプルを読み込めません');
  const bytes = new Uint8Array(await res.arrayBuffer());
  lastGraph = { bytes, name: '100random.txt' };
  // 仮定: サンプルの辺は「メッセージが流れる向き」なので transposed = false で読む
  await loadGraphAll(bytes, '100random.txt', { format: 'edgelist', gzip: false, directed: true, transposed: false });
  usePlayback.getState().setLast(-1);
}

function Num({ label, value, onChange, step = 'any', min, max, title }: { label: string; value: number; onChange: (v: number) => void; step?: string; min?: number; max?: number; title?: string }) {
  return (
    <label className="row" title={title}>
      <span>{label}</span>
      <input type="number" value={Number.isFinite(value) ? value : 0} step={step} min={min} max={max} onChange={(e) => onChange(Number(e.target.value))} />
    </label>
  );
}

export function ControlPanel() {
  const s = useSession();
  const dirty = isDirty(s);
  const [confirming, setConfirming] = useState(false);
  const form = s.form;
  const setForm = s.setForm;

  const setDec = (which: 'internal_parameters' | 'external_parameters', k: keyof ScenarioForm['internal_parameters'], v: number) =>
    setForm({ [which]: { ...form[which], [k]: v } } as Partial<ScenarioForm>);

  const apply = async () => {
    setConfirming(false);
    await applyAll();
  };

  return (
    <aside className="control" aria-label="操作パネル">
      <details open>
        <summary>グラフ</summary>
        <div className="file-row">
          <input
            type="file"
            accept=".txt,.tsv,.csv,.gz"
            aria-label="グラフファイル"
            onChange={(e) => e.target.files?.[0] && void loadGraphFile(e.target.files[0])}
          />
        </div>
        <button onClick={() => void loadSample()} title="test/100random.txt（100 ノード・400 辺）">サンプル（100random）を読み込む</button>
        <label className="row"><span>形式</span>
          <select value={s.graphOpts.format} onChange={(e) => s.setGraphOpts({ format: e.target.value as 'edgelist' | 'csv' | 'tsv' })}>
            <option value="edgelist">edgelist（空白・タブ）</option><option value="csv">csv</option><option value="tsv">tsv</option>
          </select>
        </label>
        <label className="check"><input type="checkbox" checked={s.graphOpts.gzip} onChange={(e) => s.setGraphOpts({ gzip: e.target.checked })} />gzip</label>
        <label className="check"><input type="checkbox" checked={s.graphOpts.directed} onChange={(e) => s.setGraphOpts({ directed: e.target.checked })} />有向</label>
        <label className="check" title="入力が「A→B＝A が B をフォロー」の形式のときは ON（拡散の向きに反転して読む）">
          <input type="checkbox" checked={s.graphOpts.transposed} onChange={(e) => s.setGraphOpts({ transposed: e.target.checked })} />transposed（フォロー関係を拡散の向きに反転）
        </label>
        {!s.graphOpts.directed && <p className="warn">有向と無向の結果は比較できません。</p>}
        <button disabled={!lastGraph} onClick={() => lastGraph && void loadGraphAll(lastGraph.bytes, lastGraph.name)}>この設定で再読み込み</button>
        {s.graph && (
          <p className="note" data-testid="graph-info">
            {s.graphOpts.name}：{s.graph.n_nodes} ノード / {s.graph.n_edges} 辺
            {s.graph.warnings.map((w) => <span key={w} className="warn block">{w}</span>)}
          </p>
        )}
      </details>

      <details open>
        <summary>シナリオ設定</summary>
        <Num label="受信確率" value={form.receipt_probability} step="0.05" min={0} max={1} onChange={(v) => setForm({ receipt_probability: v })} />
        <Num label="内部 β" value={form.internal_parameters.beta} step="0.05" onChange={(v) => setDec('internal_parameters', 'beta', v)} />
        <Num label="内部 γ" value={form.internal_parameters.gamma} step="0.05" onChange={(v) => setDec('internal_parameters', 'gamma', v)} />
        <Num label="外部 β" value={form.external_parameters.beta} step="0.05" onChange={(v) => setDec('external_parameters', 'beta', v)} />
        <Num label="外部 γ" value={form.external_parameters.gamma} step="0.05" onChange={(v) => setDec('external_parameters', 'gamma', v)} />
        <Num label="較正 T" value={form.calibration_rounds} step="1" min={1} title="反応性の scale = 1/(β·較正T)。実行 T とは別" onChange={(v) => setForm({ calibration_rounds: v })} />
        <Num label="実行 T（上限）" value={form.max_rounds} step="1" min={1} onChange={(v) => setForm({ max_rounds: v })} />
        <Num label="母集団シード" value={form.user_seed} step="1" min={0} onChange={(v) => setForm({ user_seed: v })} />
        <Num label="拡散シード" value={form.diffusion_seed} step="1" min={0} onChange={(v) => setForm({ diffusion_seed: v })} />
      </details>

      <details open>
        <summary>拡張機構（M1）</summary>
        <MechanismEditor store={useSession} />
      </details>

      <details open>
        <summary>推薦</summary>
        <RecommenderEditor store={useSession} />
        <RecSimTable />
        <RecWaveTable />
      </details>

      <ComparePanel />

      <div className="apply">
        {confirming ? (
          <div className="confirm" role="alertdialog">
            <p>設定を変えるとセッションを作り直します。<b>{s.rounds.length} ラウンドの履歴は消えます。</b></p>
            <button className="primary" onClick={() => void apply()}>適用する</button>
            <button onClick={() => setConfirming(false)}>キャンセル</button>
          </div>
        ) : (
          <button className={dirty ? 'primary' : ''} disabled={!s.scenario} onClick={() => (s.rounds.length > 0 ? setConfirming(true) : void apply())}>
            設定を適用（セッションを作り直す）{dirty && ' ●未適用'}
          </button>
        )}
        {s.ip !== null && s.rounds.length > 0 && !confirming && <button onClick={() => void restartAll()}>同じ設定で最初からやり直す</button>}
      </div>

      <details open>
        <summary>IP の選択</summary>
        <IpPicker />
      </details>

      <details open>
        <summary>メッセージと発信</summary>
        <Palette />
      </details>
      {s.error && <p className="error" role="alert" onClick={s.clearError}>{s.error}</p>}
    </aside>
  );
}

function IpPicker() {
  const ranking = useSession((s) => s.ranking);
  const ip = useSession((s) => s.ip);
  const scenario = useSession((s) => s.scenario);
  const [manual, setManual] = useState('');
  const labels = useSession((s) => s.graph);
  const [metric, setMetric] = useState<'degree' | 'pagerank' | 'vc'>('degree');
  useEffect(() => {
    if (scenario) void useSession.getState().loadRanking(metric === 'vc' && !useSession.getState().vcCsv ? 'degree' : metric);
  }, [metric, scenario]);
  if (!scenario) return <p className="hint">グラフを読み込んでください。</p>;
  void labels;
  const pick = async (n: number) => {
    await chooseIpAll(n);
  };
  return (
    <div>
      <label className="row"><span>順位の指標</span>
        <select value={metric} onChange={(e) => setMetric(e.target.value as 'degree' | 'pagerank' | 'vc')}>
          <option value="degree">次数（フォロワー数）</option><option value="pagerank">PageRank</option><option value="vc">VC（CSV）</option>
        </select>
      </label>
      {metric === 'vc' && (
        <input type="file" accept=".csv" aria-label="VC の CSV" onChange={async (e) => {
          const f = e.target.files?.[0];
          if (!f) return;
          useSession.getState().setVcCsv(await f.text());
          await useSession.getState().loadRanking('vc');
        }} />
      )}
      <ol className="ranking" data-testid="ranking">
        {ranking && Array.from(ranking.nodes.slice(0, 10)).map((n) => (
          <li key={n}><button className={ip === n ? 'on' : ''} onClick={() => void pick(n)}>ノード {n}</button></li>
        ))}
      </ol>
      <label className="row"><span>ノード番号</span>
        <input type="number" min={0} value={manual} placeholder="0〜" onChange={(e) => setManual(e.target.value)} />
        <button disabled={manual === ''} onClick={() => void pick(Number(manual))}>選ぶ</button>
      </label>
      <p className="note">{ip === null ? 'IP が未選択です。順位表か、ノード詳細の「IP にする」から選べます。' : `現在の IP: ノード ${ip}`}</p>
    </div>
  );
}

export function Palette() {
  const scenario = useSession((s) => s.scenario);
  const form = useSession((s) => s.form);
  const ip = useSession((s) => s.ip);
  const busy = useSession((s) => s.busy);
  const rounds = useSession((s) => s.rounds);
  const playing = usePlayback((s) => s.playing);
  const selected = usePaletteChoice((s) => s.choice);
  const msgs = (scenario?.config.message_space as ScenarioForm['message_space'] | undefined) ?? form.message_space;
  const maxRounds = (scenario?.config.max_rounds as number | undefined) ?? form.max_rounds;
  const full = rounds.length >= maxRounds;
  const canSend = ip !== null && !busy && !playing && !full;
  return (
    <div>
      <div className="chips" role="radiogroup" aria-label="メッセージ">
        {msgs.map((m, i) => (
          <button key={i} role="radio" aria-checked={selected === i} className={selected === i ? 'chip on' : 'chip'} title={`内部 ${m.internal_stimulus} / 外部 ${m.external_stimulus}（キー ${i + 1}）`} onClick={() => usePaletteChoice.getState().set(i)}>
            {msgName(i)}
          </button>
        ))}
      </div>
      <button className="primary send" data-testid="send" disabled={!canSend} onClick={() => void sendSelected()} title="Enter">発信</button>
      {playing && <p className="note">再生中は発信できません（最後までスキップすると押せます）。</p>}
      {full && <p className="note">実行ラウンド数の上限に達しました。</p>}
    </div>
  );
}

import { create } from 'zustand';
interface PaletteState { choice: number; set(n: number): void }
export const usePaletteChoice = create<PaletteState>((set) => ({ choice: 0, set: (choice) => set({ choice }) }));

/** 選ばれているメッセージで 1 ラウンド発信し、再生を始める。 */
export async function sendSelected(): Promise<void> {
  const choice = usePaletteChoice.getState().choice;
  const st = useSession.getState();
  if (st.ip === null || st.busy || usePlayback.getState().playing) return;
  await stepAll(choice);
}
