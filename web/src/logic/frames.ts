// 再生（F06）。ラウンド開始時の everFlags と、そのラウンドのイベント列から、ウェーブ k の終了時点の状態を作る（9.5）。
import type { EventCols } from '../worker/protocol';
import { EV_EXT, EV_INT, EVER_EXT, EVER_INT, EVER_RECV, kindOf } from './kinds';

export interface WaveState {
  kinds: Uint8Array;
  /** いまのウェーブ（wave == k）で受信したノードは 1。 */
  pulse: Uint8Array;
}

/**
 * - `k = -1`: ラウンド開始前の状態。`k = n_waves - 1`: ラウンド終了後の状態（`kinds(intMode)` と一致しなければならない）。
 * - 規則: イベント（wave ≤ k）ごとに `node` へ EVER_RECV、int_fired なら EVER_INT、ext_fired なら EVER_EXT を立てる。
 *   `intMode = 1`（このラウンドのみ）では、内部は「そのラウンドの int_fired」だけで判定し、外部は常に累積。
 */
export function stateAtWave(everBefore: Uint8Array, ev: EventCols, k: number, intMode: 0 | 1, ip: number): WaveState {
  const n = everBefore.length;
  const ever = Uint8Array.from(everBefore);
  const intNow = new Uint8Array(n);
  const pulse = new Uint8Array(n);
  for (let i = 0; i < ev.node.length; i++) {
    const w = ev.wave[i];
    if (w > k) continue;
    const node = ev.node[i];
    ever[node] |= EVER_RECV;
    if (ev.flags[i] & EV_INT) {
      ever[node] |= EVER_INT;
      intNow[node] = 1;
    }
    if (ev.flags[i] & EV_EXT) ever[node] |= EVER_EXT;
    if (w === k) pulse[node] = 1;
  }
  const kinds = new Uint8Array(n);
  for (let j = 0; j < n; j++) kinds[j] = kindOf(ever[j], intNow[j] === 1, j === ip, intMode);
  return { kinds, pulse };
}

/** ラウンド開始前の状態（`k = -1`）。 */
export function emptyEvents(): EventCols {
  return {
    wave: new Uint16Array(0), node: new Uint32Array(0), from: new Uint32Array(0), nArr: new Uint8Array(0),
    uInt: new Float32Array(0), uExt: new Float32Array(0), flags: new Uint8Array(0),
  };
}
