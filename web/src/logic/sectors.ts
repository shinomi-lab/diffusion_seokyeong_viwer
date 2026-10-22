// 全体図の扇形セルの集計（sim-api の overview::aggregate と同じ）。再生中は Worker を呼ばずに画面側で集計する。
// 配列は [ring][sector] ごとに n, n_recv, n_int, n_ext, n_both の 5 値（sectorGrid と同じ並び）。
import { KIND } from './kinds';
import { N_SECTORS } from './units';

const TAU_F32 = Math.fround(2 * Math.PI);
export const UNREACHABLE = 0xffffffff;

/** Rust の `sector_of`（f32 の演算）と同じ。 */
export function sectorOf(angle: number): number {
  const v = Math.fround(Math.fround(angle / TAU_F32) * N_SECTORS);
  return Math.min(Math.floor(v), N_SECTORS - 1);
}

export function ringCount(dist: Uint32Array): number {
  let m = 0;
  for (const d of dist) if (d !== UNREACHABLE && d > m) m = d;
  return m + 1;
}

export function aggregateSectors(kinds: Uint8Array, dist: Uint32Array, angle: Float32Array): Uint32Array {
  const cells = new Uint32Array(ringCount(dist) * N_SECTORS * 5);
  for (let i = 0; i < dist.length; i++) {
    const d = dist[i];
    if (d === UNREACHABLE) continue;
    const o = (d * N_SECTORS + sectorOf(angle[i])) * 5;
    cells[o]++;
    switch (kinds[i]) {
      case KIND.ReceivedOnly: cells[o + 1]++; break;
      case KIND.Internal: cells[o + 1]++; cells[o + 2]++; break;
      case KIND.External: cells[o + 1]++; cells[o + 3]++; break;
      case KIND.Both: cells[o + 1]++; cells[o + 4]++; break;
      default: break;
    }
  }
  return cells;
}

export interface RingTotals {
  n: number;
  recv: number;
  int: number;
  ext: number;
  both: number;
}

/** リングごとの合計（リング表示用）。 */
export function ringTotals(cells: Uint32Array, rings: number): RingTotals[] {
  const out: RingTotals[] = [];
  for (let r = 0; r < rings; r++) {
    const t = { n: 0, recv: 0, int: 0, ext: 0, both: 0 };
    for (let s = 0; s < N_SECTORS; s++) {
      const o = (r * N_SECTORS + s) * 5;
      t.n += cells[o]; t.recv += cells[o + 1]; t.int += cells[o + 2]; t.ext += cells[o + 3]; t.both += cells[o + 4];
    }
    out.push(t);
  }
  return out;
}
