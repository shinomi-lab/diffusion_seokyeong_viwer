// 系列・分岐ツリー（F09）と MC の併記（F12）。
import { useState } from 'react';
import { coverage } from '../logic/mc';
import { clickAction, placeTree } from '../logic/tree';
import { sequenceString } from '../logic/units';
import { rewindAll, runMcAll, switchAll } from '../store/actions';
import { usePlayback } from '../store/playback';
import { useSession } from '../store/session';

export function SeriesPanel() {
  const choices = useSession((s) => s.choices);
  const tree = useSession((s) => s.tree);
  const rounds = useSession((s) => s.rounds);
  const mc = useSession((s) => s.mc);
  const playing = usePlayback((s) => s.playing);
  const [trials, setTrials] = useState(200);
  const placed = tree ? placeTree(tree) : null;

  const click = (id: number) => {
    if (!tree || playing) return;
    const a = clickAction(tree, id);
    if (a.kind === 'rewind') void rewindAll(a.round);
    else if (a.kind === 'switch') void switchAll(a.id);
  };

  const cov = mc.result
    ? {
        cum: coverage(rounds.map((r) => r.summary.cum_xact), mc.result.cum_xact),
        share: coverage(rounds.map((r) => r.summary.n_share), mc.result.new_share),
      }
    : null;

  return (
    <section className="branch" aria-label="系列と分岐ツリー">
      <h3>選んだ系列</h3>
      {choices.length === 0 ? <p className="hint">まだ発信していません。</p> : <p className="seq" data-testid="sequence">{sequenceString(choices)}</p>}
      {placed && placed.nodes.length > 1 && (
        <div className="tree-scroll">
          <svg width={placed.width} height={placed.height} data-testid="branch-tree" role="img" aria-label="分岐ツリー">
            {placed.edges.map((e, i) => (
              <path key={i} d={`M${e.x1},${e.y1} L${e.x1},${e.y2} L${e.x2},${e.y2}`} fill="none" stroke={e.active ? 'var(--accent)' : 'var(--line)'} strokeWidth={e.active ? 2 : 1.5} />
            ))}
            {placed.nodes.map((n) => (
              <g key={n.id} transform={`translate(${n.x},${n.y})`} onClick={() => click(n.id)} className="tree-node" data-node-id={n.id} data-round={n.round}>
                <title>{n.id === 0 ? '初期状態（ラウンド 0）' : `ラウンド ${n.round}: m${n.choice === '-' ? '∅' : n.choice}（累積外部 ${n.cum_xact}${n.cum_rec > 0 ? `、推薦で受信 ${n.rec_received}・累積 ${n.cum_rec}` : ''}）`}</title>
                <circle r={7.5} fill={n.onActive ? 'var(--accent)' : 'var(--bg)'} stroke={n.current ? 'var(--warn)' : n.onActive ? 'var(--accent)' : 'var(--muted)'} strokeWidth={n.current ? 3 : 1.5} />
                <text textAnchor="middle" dy="3.5" fontSize="9" fill={n.onActive ? 'var(--on-accent)' : 'var(--fg)'} pointerEvents="none">{n.id === 0 ? '0' : n.choice}</text>
              </g>
            ))}
          </svg>
        </div>
      )}
      <p className="note">節をクリックすると、そのラウンドへ巻き戻します（別の枝の節なら、その枝へ切り替えます）。巻き戻して別のメッセージを発信すると、枝ができます。</p>
      <h3>MC 平均の併記</h3>
      <div className="mc-row">
        <label>試行数 <input type="number" min={10} max={5000} step={10} value={trials} onChange={(e) => setTrials(Number(e.target.value))} style={{ width: 70 }} /></label>
        <button disabled={choices.length === 0 || mc.running || playing} onClick={() => void runMcAll(trials)} data-testid="run-mc">この系列を MC で回す</button>
        {mc.running && <span className="note">計算中…</span>}
      </div>
      {mc.result && cov && (
        <p className="note" data-testid="mc-result">
          {mc.result.trials} 試行（{Math.round(mc.ms)} ms）: 合計外部行動者数の平均 {mc.result.total_xact_mean.toFixed(2)}、合計シェア回数の平均 {mc.result.total_share_mean.toFixed(2)}。
          1 回の実現が 10〜90% の帯に入っているラウンド — 累積外部行動 {cov.cum.inside}/{cov.cum.total}、新規シェア {cov.share.inside}/{cov.share.total}。
        </p>
      )}
    </section>
  );
}
