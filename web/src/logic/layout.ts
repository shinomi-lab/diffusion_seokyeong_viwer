// 全ノードのワールド座標（リング半径 = ホップ距離、角度 = 放射状レイアウト）。描画側の仕事（7.8 の 4・5）。
import { EXPAND_CAP } from './units';

const PHI = 0.6180339887;

export interface Positions {
  /** [x0, y0, x1, y1, …]。到達不能のノードは外側の格子に置く。 */
  xy: Float32Array;
  maxDist: number;
}

export function computePositions(angle: Float32Array, dist: Uint32Array): Positions {
  const n = dist.length;
  const xy = new Float32Array(n * 2);
  let maxDist = 1;
  for (let i = 0; i < n; i++) if (dist[i] !== 0xffffffff && dist[i] > maxDist) maxDist = dist[i];
  const unreach: number[] = [];
  for (let i = 0; i < n; i++) {
    const d = dist[i];
    if (d === 0xffffffff) {
      unreach.push(i);
      continue;
    }
    // 重なりを減らすため、半径に決定的なずらしを入れる（IP は中心）
    const r = d === 0 ? 0 : d + 0.3 * (((i * PHI) % 1) - 0.5);
    xy[2 * i] = r * Math.cos(angle[i]);
    xy[2 * i + 1] = r * Math.sin(angle[i]);
  }
  // 到達不能: 右側（x > maxDist + 2）に格子状に並べる。上限は EXPAND_CAP
  const cols = Math.max(1, Math.ceil(Math.sqrt(Math.min(unreach.length, EXPAND_CAP))));
  unreach.forEach((node, k) => {
    xy[2 * node] = maxDist + 3 + (k % cols) * 0.35;
    xy[2 * node + 1] = ((k / cols) | 0) * 0.35 - (cols * 0.35) / 2;
  });
  return { xy, maxDist };
}

export function boundsOf(xy: Float32Array, nodes: Uint32Array | null): { minX: number; minY: number; maxX: number; maxY: number } {
  let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
  const visit = (i: number) => {
    const x = xy[2 * i], y = xy[2 * i + 1];
    if (x < minX) minX = x;
    if (x > maxX) maxX = x;
    if (y < minY) minY = y;
    if (y > maxY) maxY = y;
  };
  if (nodes) for (const i of nodes) visit(i);
  else for (let i = 0; i < xy.length / 2; i++) visit(i);
  if (!isFinite(minX)) return { minX: -1, minY: -1, maxX: 1, maxY: 1 };
  return { minX, minY, maxX, maxY };
}
