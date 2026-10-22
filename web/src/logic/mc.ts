// MC の併記（F12）。1 回の実現が、MC の 10〜90% の帯の中にあるかを確かめる。
import type { Band } from '../worker/protocol';

export interface Coverage {
  inside: number;
  total: number;
}

/** ラウンドごとの実現値が、帯（p10〜p90、両端を含む）に入っているラウンド数。 */
export function coverage(realized: number[], band: Pick<Band, 'p10' | 'p90'>): Coverage {
  const total = Math.min(realized.length, band.p10.length);
  let inside = 0;
  for (let i = 0; i < total; i++) if (realized[i] >= band.p10[i] && realized[i] <= band.p90[i]) inside++;
  return { inside, total };
}
