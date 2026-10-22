// カメラ（パン・ズーム）。座標は「ワールド → CSS px」。
import type { Camera } from '../store/view';

export const toScreen = (c: Camera, w: number, h: number, x: number, y: number): [number, number] => [
  (x - c.cx) * c.zoom + w / 2,
  -(y - c.cy) * c.zoom + h / 2,
];

export const toWorld = (c: Camera, w: number, h: number, sx: number, sy: number): [number, number] => [
  (sx - w / 2) / c.zoom + c.cx,
  -(sy - h / 2) / c.zoom + c.cy,
];

/** 指定の領域が収まるカメラ。 */
export function fitCamera(b: { minX: number; minY: number; maxX: number; maxY: number }, w: number, h: number, margin = 40): Camera {
  const bw = Math.max(b.maxX - b.minX, 0.5);
  const bh = Math.max(b.maxY - b.minY, 0.5);
  const zoom = Math.max(1, Math.min((w - margin * 2) / bw, (h - margin * 2) / bh));
  return { cx: (b.minX + b.maxX) / 2, cy: (b.minY + b.maxY) / 2, zoom };
}

/** カーソル位置を固定してズームする。 */
export function zoomAt(c: Camera, w: number, h: number, sx: number, sy: number, factor: number): Camera {
  const zoom = Math.min(2000, Math.max(0.5, c.zoom * factor));
  const [wx, wy] = toWorld(c, w, h, sx, sy);
  return { zoom, cx: wx - (sx - w / 2) / zoom, cy: wy + (sy - h / 2) / zoom };
}
