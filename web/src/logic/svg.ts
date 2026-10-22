// 時系列の SVG 書き出し（F15）。uPlot の描画と同じデータ・系列・帯の指定から、単体の SVG ファイルを作る。
// 縦軸は値に合わせて広げ、上限・下限を設けない（INV-15: 効用が 1 を超えても切れない）。

import { T as Tok } from '../tokens';

export interface SvgSeries {
  label: string;
  color: string;
  dash?: number[];
  hidden?: boolean;
  width?: number;
}

export interface SvgChart {
  title?: string;
  xLabel?: string;
  x: number[];
  series: SvgSeries[];
  /** series と同じ並びの値（NaN は線を切る）。 */
  cols: number[][];
  /** [上側の系列番号, 下側の系列番号, 色]。系列番号は x を 0 とした番号（UPlotChart と同じ）。 */
  bands?: [number, number, string][];
  width?: number;
  height?: number;
  bg?: string;
  fg?: string;
}

export function escapeXml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;').replace(/'/g, '&apos;');
}

/** 区間 [min, max] を覆う、きりのよい目盛り（およそ n 個）。 */
export function niceTicks(min: number, max: number, n = 5): number[] {
  if (!isFinite(min) || !isFinite(max)) return [0, 1];
  if (min === max) return [min - 1, min, min + 1];
  const raw = (max - min) / n;
  const mag = 10 ** Math.floor(Math.log10(raw));
  const step = [1, 2, 5, 10].map((m) => m * mag).find((s) => s >= raw) ?? raw;
  const out: number[] = [];
  // max 以上になる最初の目盛りまで含める（範囲を必ず覆う）
  for (let k = Math.floor(min / step); ; k++) {
    out.push(k * step);
    if (k * step >= max) break;
  }
  return out.map((v) => Number(v.toPrecision(12)));
}

const fmt = (v: number) => (Number.isInteger(v) ? String(v) : v.toFixed(Math.abs(v) < 1 ? 3 : 2).replace(/0+$/, '').replace(/\.$/, ''));

export function lineChartSvg(c: SvgChart): string {
  const W = c.width ?? 760;
  const H = c.height ?? 360;
  // 画面と同じダーク専用の配色（ui-tokens.json の chrome）
  const bg = c.bg ?? Tok.chrome.card;
  const fg = c.fg ?? Tok.chrome.text;
  const grid = Tok.chrome.grid;
  const L = 58, R = 190, T = c.title ? 34 : 14, B = 44;
  const pw = W - L - R, ph = H - T - B;

  const finite = (v: number) => Number.isFinite(v);
  const xs = c.x.filter(finite);
  let x0 = xs.length ? Math.min(...xs) : 0;
  let x1 = xs.length ? Math.max(...xs) : 1;
  if (x0 === x1) {
    x0 -= 0.5;
    x1 += 0.5;
  }
  // y の範囲: 表示する系列と帯の全値（上限・下限は付けない）
  const ys: number[] = [];
  c.series.forEach((s, i) => { if (!s.hidden) ys.push(...(c.cols[i] ?? []).filter(finite)); });
  for (const [hi, lo] of c.bands ?? []) ys.push(...(c.cols[hi - 1] ?? []).filter(finite), ...(c.cols[lo - 1] ?? []).filter(finite));
  let y0 = ys.length ? Math.min(...ys) : 0;
  let y1 = ys.length ? Math.max(...ys) : 1;
  if (y0 === y1) {
    y0 -= 0.5;
    y1 += 0.5;
  }
  const pad = (y1 - y0) * 0.08;
  y0 -= pad;
  y1 += pad;
  const sx = (x: number) => L + ((x - x0) / (x1 - x0)) * pw;
  const sy = (y: number) => T + ph - ((y - y0) / (y1 - y0)) * ph;

  const o: string[] = [];
  o.push(`<svg xmlns="http://www.w3.org/2000/svg" width="${W}" height="${H}" viewBox="0 0 ${W} ${H}" font-family="system-ui, 'Hiragino Sans', sans-serif" font-size="11">`);
  o.push(`<rect width="${W}" height="${H}" fill="${bg}"/>`);
  if (c.title) o.push(`<text x="${L}" y="20" font-size="13" font-weight="600" fill="${fg}">${escapeXml(c.title)}</text>`);
  // 目盛りと格子
  for (const t of niceTicks(y0, y1, 5)) {
    if (t < y0 || t > y1) continue;
    o.push(`<line x1="${L}" y1="${sy(t).toFixed(1)}" x2="${L + pw}" y2="${sy(t).toFixed(1)}" stroke="${grid}"/>`);
    o.push(`<text x="${L - 6}" y="${(sy(t) + 3.5).toFixed(1)}" text-anchor="end" fill="${fg}">${fmt(t)}</text>`);
  }
  const xt = xs.length <= 12 ? [...new Set(xs)] : niceTicks(x0, x1, 8).filter((t) => t >= x0 && t <= x1);
  for (const t of xt) {
    o.push(`<line x1="${sx(t).toFixed(1)}" y1="${T}" x2="${sx(t).toFixed(1)}" y2="${T + ph}" stroke="${grid}"/>`);
    o.push(`<text x="${sx(t).toFixed(1)}" y="${T + ph + 15}" text-anchor="middle" fill="${fg}">${fmt(t)}</text>`);
  }
  o.push(`<rect x="${L}" y="${T}" width="${pw}" height="${ph}" fill="none" stroke="${fg}" stroke-opacity="0.4"/>`);
  if (c.xLabel) o.push(`<text x="${L + pw / 2}" y="${H - 8}" text-anchor="middle" fill="${fg}">${escapeXml(c.xLabel)}</text>`);
  // 帯（10〜90%）
  for (const [hi, lo, color] of c.bands ?? []) {
    const up = (c.cols[hi - 1] ?? []).map((v, i) => [c.x[i], v] as const).filter(([x, v]) => finite(x) && finite(v));
    const dn = (c.cols[lo - 1] ?? []).map((v, i) => [c.x[i], v] as const).filter(([x, v]) => finite(x) && finite(v)).reverse();
    if (up.length < 2) continue;
    const pts = [...up, ...dn].map(([x, v]) => `${sx(x).toFixed(1)},${sy(v).toFixed(1)}`).join(' ');
    o.push(`<polygon points="${pts}" fill="${color}" fill-opacity="0.18" stroke="none"/>`);
  }
  // 系列（NaN で線を切る）
  c.series.forEach((s, i) => {
    if (s.hidden) return;
    const col = c.cols[i] ?? [];
    let d = '';
    let pen = false;
    col.forEach((v, k) => {
      if (!finite(v) || !finite(c.x[k])) {
        pen = false;
        return;
      }
      d += `${pen ? 'L' : 'M'}${sx(c.x[k]).toFixed(1)},${sy(v).toFixed(1)}`;
      pen = true;
    });
    if (!d) return;
    const dash = s.dash ? ` stroke-dasharray="${s.dash.join(' ')}"` : '';
    o.push(`<path d="${d}" fill="none" stroke="${s.color}" stroke-width="${s.width ?? 1.8}"${dash}/>`);
  });
  // 凡例
  let ly = T + 6;
  for (const s of c.series) {
    if (s.hidden) continue;
    const dash = s.dash ? ` stroke-dasharray="${s.dash.join(' ')}"` : '';
    o.push(`<line x1="${L + pw + 14}" y1="${ly}" x2="${L + pw + 36}" y2="${ly}" stroke="${s.color}" stroke-width="2"${dash}/>`);
    o.push(`<text x="${L + pw + 42}" y="${ly + 3.5}" fill="${fg}">${escapeXml(s.label)}</text>`);
    ly += 16;
  }
  o.push('</svg>');
  return o.join('\n');
}
