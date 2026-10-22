// Canvas2D の重ね描き: リングの円弧・ラベル、辺、扇形セル、強調、縮小地図。
import { aggregateSectors, ringCount, ringTotals } from '../logic/sectors';
import { EDGE_CAP, N_SECTORS } from '../logic/units';
import { hexToRgb, T } from '../tokens';
import type { Camera } from '../store/view';
import { toScreen } from './camera';

export interface Surface {
  ctx: CanvasRenderingContext2D;
  w: number;
  h: number;
  cam: Camera;
}

const rgba = (hex: string, a: number) => {
  const [r, g, b] = hexToRgb(hex).map((v) => Math.round(v * 255));
  return `rgba(${r},${g},${b},${a})`;
};

export function clear(s: Surface): void {
  s.ctx.clearRect(0, 0, s.w, s.h);
}

/** 同心円のリング（IP からのホップ距離）とラベル。 */
export function drawRings(s: Surface, maxDist: number): void {
  const { ctx, cam } = s;
  const [cx, cy] = toScreen(cam, s.w, s.h, 0, 0);
  ctx.save();
  ctx.strokeStyle = T.rings.stroke;
  ctx.fillStyle = T.rings.label;
  ctx.setLineDash(T.rings.dash);
  ctx.lineWidth = 1;
  ctx.font = '11px system-ui, sans-serif';
  // リング名が重ならないよう、間隔が 16px 未満なら間引く
  const step = Math.max(1, Math.ceil(16 / cam.zoom));
  for (let d = step; d <= maxDist; d += step) {
    const r = d * cam.zoom;
    if (r < 4 || r > 20000) continue;
    ctx.beginPath();
    ctx.arc(cx, cy, r, 0, Math.PI * 2);
    ctx.stroke();
    ctx.setLineDash([]);
    ctx.fillText(`${d}ホップ`, cx + 4, cy - r - 3);
    ctx.setLineDash(T.rings.dash);
  }
  ctx.restore();
}

/** 辺: [from, to, …]。 */
export function drawEdges(s: Surface, xy: Float32Array, edges: Uint32Array): void {
  const { ctx, cam } = s;
  ctx.save();
  ctx.strokeStyle = rgba(T.edges, 0.16);
  ctx.lineWidth = 0.7;
  ctx.beginPath();
  for (let k = 0; k + 1 < Math.min(edges.length, EDGE_CAP * 2); k += 2) {
    const a = edges[k], b = edges[k + 1];
    const [x0, y0] = toScreen(cam, s.w, s.h, xy[2 * a], xy[2 * a + 1]);
    const [x1, y1] = toScreen(cam, s.w, s.h, xy[2 * b], xy[2 * b + 1]);
    ctx.moveTo(x0, y0);
    ctx.lineTo(x1, y1);
  }
  ctx.stroke();
  ctx.restore();
}

/** 現在のウェーブの送信元→受信先の辺だけ強調する。 */
export function drawWaveEdges(s: Surface, xy: Float32Array, from: Uint32Array, node: Uint32Array, wave: Uint16Array, k: number, visible: Uint8Array | null): void {
  if (k < 0) return;
  const { ctx, cam } = s;
  ctx.save();
  ctx.strokeStyle = T.highlight.currentWave;
  ctx.lineWidth = 1.6;
  ctx.beginPath();
  for (let i = 0; i < node.length; i++) {
    if (wave[i] !== k) continue;
    // 展開中の部分集合に入っていないノードへの辺は描かない
    if (visible && (!visible[from[i]] || !visible[node[i]])) continue;
    const [x0, y0] = toScreen(cam, s.w, s.h, xy[2 * from[i]], xy[2 * from[i] + 1]);
    const [x1, y1] = toScreen(cam, s.w, s.h, xy[2 * node[i]], xy[2 * node[i] + 1]);
    ctx.moveTo(x0, y0);
    ctx.lineTo(x1, y1);
  }
  ctx.stroke();
  ctx.restore();
}

export function drawSelected(s: Surface, xy: Float32Array, node: number, kindScale: number): void {
  const { ctx, cam } = s;
  const [x, y] = toScreen(cam, s.w, s.h, xy[2 * node], xy[2 * node + 1]);
  ctx.save();
  ctx.strokeStyle = T.highlight.selected;
  ctx.lineWidth = 2;
  ctx.beginPath();
  ctx.arc(x, y, T.node.radiusPx * kindScale * 1.9, 0, Math.PI * 2);
  ctx.stroke();
  ctx.restore();
}

function annulus(ctx: CanvasRenderingContext2D, cx: number, cy: number, r0: number, r1: number, a0: number, a1: number): void {
  // 画面の y は下向きなので角度は反転して描く
  ctx.beginPath();
  ctx.arc(cx, cy, r1, -a0, -a1, true);
  ctx.arc(cx, cy, r0, -a1, -a0, false);
  ctx.closePath();
}

/** 全体図: 扇形セル。3 本の細い帯（内側から 受信・内部・外部）の濃さで割合を示す。 */
export function drawSectors(s: Surface, cells: Uint32Array, nRings: number): void {
  const { ctx, cam } = s;
  const [cx, cy] = toScreen(cam, s.w, s.h, 0, 0);
  const step = (Math.PI * 2) / N_SECTORS;
  ctx.save();
  ctx.lineWidth = 0.5;
  ctx.strokeStyle = 'rgba(255,255,255,0.12)';
  for (let d = 0; d < nRings; d++) {
    const r0 = Math.max(0, d - 0.5) * cam.zoom, r1 = (d + 0.5) * cam.zoom;
    if (d === 0) {
      ctx.beginPath();
      ctx.arc(cx, cy, 0.5 * cam.zoom, 0, Math.PI * 2);
      ctx.fillStyle = rgba(T.kinds['5'].fill!, 0.9);
      ctx.fill();
      continue;
    }
    const band = (r1 - r0) / 3;
    for (let sec = 0; sec < N_SECTORS; sec++) {
      const o = (d * N_SECTORS + sec) * 5;
      const n = cells[o];
      if (n === 0) continue;
      const a0 = sec * step, a1 = a0 + step;
      annulus(ctx, cx, cy, r0, r1, a0, a1);
      ctx.fillStyle = 'rgba(255,255,255,0.05)';
      ctx.fill();
      ctx.stroke();
      const ratios = [cells[o + 1] / n, (cells[o + 2] + cells[o + 4]) / n, (cells[o + 3] + cells[o + 4]) / n];
      const colors = [T.kinds['1'].fill!, T.kinds['2'].fill!, T.kinds['3'].fill!];
      for (let b = 0; b < 3; b++) {
        if (ratios[b] <= 0) continue;
        annulus(ctx, cx, cy, r0 + band * b, r0 + band * (b + 1), a0, a1);
        ctx.fillStyle = rgba(colors[b], 0.2 + 0.8 * ratios[b]);
        ctx.fill();
      }
    }
  }
  ctx.restore();
}

/** セルの位置から (リング, 扇形) を求める。全体図の外なら null。 */
export function cellAt(cam: Camera, w: number, h: number, sx: number, sy: number, nRings: number): { ring: number; sector: number } | null {
  const [cx, cy] = toScreen(cam, w, h, 0, 0);
  const dx = (sx - cx) / cam.zoom, dy = -(sy - cy) / cam.zoom;
  const ring = Math.round(Math.hypot(dx, dy));
  if (ring < 1 || ring >= nRings) return null;
  let ang = Math.atan2(dy, dx);
  if (ang < 0) ang += Math.PI * 2;
  return { ring, sector: Math.min(N_SECTORS - 1, Math.floor((ang / (Math.PI * 2)) * N_SECTORS)) };
}

/** 到達不能の枠（全体図）。ワールドの矩形を返す。 */
export function unreachableRect(maxDist: number): { x: number; y: number; w: number; h: number } {
  return { x: maxDist + 2.5, y: -1.5, w: 3, h: 3 };
}

export function drawUnreachable(s: Surface, maxDist: number, count: number): void {
  const r = unreachableRect(maxDist);
  const [x0, y0] = toScreen(s.cam, s.w, s.h, r.x, r.y + r.h);
  const [x1, y1] = toScreen(s.cam, s.w, s.h, r.x + r.w, r.y);
  const { ctx } = s;
  ctx.save();
  ctx.setLineDash([4, 3]);
  ctx.strokeStyle = T.rings.stroke;
  ctx.strokeRect(x0, y0, x1 - x0, y1 - y0);
  ctx.setLineDash([]);
  ctx.fillStyle = T.rings.label;
  ctx.font = '11px system-ui, sans-serif';
  ctx.fillText(`到達不能 ${count}`, x0 + 4, y0 + 14);
  ctx.restore();
}

export { ringCount, ringTotals, aggregateSectors };
