// 種類どうしの似かた（6×6、R12）。CF＋グラフのとき、現在の履歴から sim-api が計算した値を濃淡で示す。
import { useEffect, useState } from 'react';
import { recHybrid, useSession } from '../store/session';
import { T } from '../tokens';
import { sim } from '../worker/client';

export function RecSimTable() {
  const on = useSession(recHybrid);
  const rounds = useSession((s) => s.rounds);
  const ip = useSession((s) => s.ip);
  const [v, setV] = useState<Float64Array | null>(null);

  useEffect(() => {
    if (!on || ip === null) {
      setV(null);
      return;
    }
    let cancel = false;
    sim.call('recSim', {}).then((r) => !cancel && setV(r)).catch(() => undefined);
    return () => {
      cancel = true;
    };
  }, [on, rounds, ip]);

  if (!on || !v || v.length === 0) return null;
  const l = Math.round(Math.sqrt(v.length));
  const idx = [...Array(l).keys()];
  return (
    <details data-testid="rec-sim">
      <summary>種類どうしの似かた（協調フィルタリング）</summary>
      <table className="mini sim">
        <thead><tr><th />{idx.map((k) => <th key={k}>m{k}</th>)}</tr></thead>
        <tbody>
          {idx.map((m) => (
            <tr key={m}>
              <th>m{m}</th>
              {idx.map((k) => {
                const x = v[m * l + k];
                const a = Math.round(Math.min(1, Math.max(0, x)) * 255).toString(16).padStart(2, '0');
                // 黄色の塗りの上では暗い文字にする（濃いセルで明るい文字だと読めない）
                const color = x > 0.45 ? T.chrome.onAccent : undefined;
                return <td key={k} style={{ background: `${T.highlight.recommended}${a}`, textAlign: 'right', color }}>{x.toFixed(2)}</td>;
              })}
            </tr>
          ))}
        </tbody>
      </table>
      <p className="note">両方の種類を受け取った人の反応の似かた（0〜1）。濃いほど「同じ人に効く」と学ばれた組。対角は 1。</p>
    </details>
  );
}
