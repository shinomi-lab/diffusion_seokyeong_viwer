// Rモデルの推薦の推移（直近のラウンドのウェーブごと。recommender-v3-design.md 8 章）。方式 wave のときだけ。
import { useEffect, useState } from 'react';
import { recWave, useSession } from '../store/session';
import { sim } from '../worker/client';

interface WaveRow {
  wave: number;
  e_hat: number;
  budget: number;
  slots: number;
  miss: number;
}
interface WaveTrace {
  trace: WaveRow[];
  prior: number;
  ebar0: number;
}

export function RecWaveTable() {
  const on = useSession(recWave);
  const rounds = useSession((s) => s.rounds);
  const ip = useSession((s) => s.ip);
  const [v, setV] = useState<WaveTrace | null>(null);

  useEffect(() => {
    if (!on || ip === null) {
      setV(null);
      return;
    }
    let cancel = false;
    sim
      .call('recWaveTrace', {})
      .then((r) => !cancel && setV(JSON.parse(r) as WaveTrace))
      .catch(() => undefined);
    return () => {
      cancel = true;
    };
  }, [on, rounds, ip]);

  if (!on || !v || v.trace.length === 0) return null;
  return (
    <details data-testid="rec-wave">
      <summary>推薦の推移（直近のラウンド）</summary>
      <table className="mini">
        <thead>
          <tr><th>ウェーブ</th><th>反応率 ê</th><th>推薦量</th><th>枠</th><th>空振り</th></tr>
        </thead>
        <tbody>
          {v.trace.map((r) => (
            <tr key={r.wave}>
              <td>{r.wave}</td>
              <td style={{ textAlign: 'right' }}>{r.e_hat.toFixed(3)}</td>
              <td style={{ textAlign: 'right' }}>{r.budget.toFixed(2)}</td>
              <td style={{ textAlign: 'right' }}>{r.slots}</td>
              <td style={{ textAlign: 'right' }}>{r.miss}</td>
            </tr>
          ))}
        </tbody>
      </table>
      {v.ebar0 === 0 && (
        <p className="note" data-testid="rec-wave-zero">
          基準 ē0 = 0 のため推薦しません（平均的なメッセージを初めて受け取ってシェアする人が母集団にいない）。較正 T や反応性を変えると変わります。
        </p>
      )}
      <p className="note">
        次のラウンドの事前の見込み a = {v.prior.toFixed(3)}（基準 ē0 = {v.ebar0.toFixed(3)}）。推薦量 = ε·δ·φ(ê/ē0)·p·（シェア者のフォロワー数の和）。
      </p>
    </details>
  );
}
