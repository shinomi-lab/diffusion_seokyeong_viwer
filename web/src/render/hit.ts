// ヒットテスト: 画面上でクリック位置に最も近いノードを、半径の 1.5 倍以内で拾う（9.4）。
import type { Camera } from '../store/view';
import { toScreen } from './camera';

/** `nodes` は描画中のノード番号、`xy` は全ノードの座標。見つからなければ -1。 */
export function hitTest(xy: Float32Array, nodes: Uint32Array, cam: Camera, w: number, h: number, sx: number, sy: number, radiusPx: number): number {
  let best = -1;
  let bestD = Math.max(radiusPx * 1.5, 8) ** 2; // 小さいノードでも最低 8px は拾う
  for (let k = 0; k < nodes.length; k++) {
    const i = nodes[k];
    const [px, py] = toScreen(cam, w, h, xy[2 * i], xy[2 * i + 1]);
    const d = (px - sx) ** 2 + (py - sy) ** 2;
    if (d <= bestD) {
      bestD = d;
      best = i;
    }
  }
  return best;
}
