// uPlot の薄い包み。縦軸は値に合わせて自動で広げる（上限を設けない。INV-15）。
import uPlot from 'uplot';
import 'uplot/dist/uPlot.min.css';
import { useEffect, useRef } from 'react';
import { downloadBlob, downloadText } from '../logic/units';
import { lineChartSvg } from '../logic/svg';
import { toBlob } from '../render/capture';
import { T } from '../tokens';

const AXIS_FONT = '11px system-ui, -apple-system, "Hiragino Sans", sans-serif';
const LABEL_FONT = '600 11px system-ui, -apple-system, "Hiragino Sans", sans-serif';

export type PlotSeries = { label: string; color: string; width?: number; dash?: number[]; hidden?: boolean; points?: boolean };

export function UPlotChart({ data, series, bands, height = 150, xLabel, integerX = true, exportName, title }: {
  data: uPlot.AlignedData;
  series: PlotSeries[];
  /** [上側の系列番号, 下側の系列番号, 色]（系列番号は x を 0 とした番号） */
  bands?: [number, number, string][];
  height?: number;
  xLabel?: string;
  integerX?: boolean;
  /** 指定すると PNG・SVG の保存ボタンを出す（F15）。ファイル名の元。 */
  exportName?: string;
  title?: string;
}) {
  const host = useRef<HTMLDivElement>(null);
  const plot = useRef<uPlot | null>(null);
  const key = JSON.stringify([series, bands, height, xLabel]);

  useEffect(() => {
    const el = host.current;
    if (!el) return;
    const css = getComputedStyle(document.documentElement);
    // 軸の数字・ラベルは補助色、格子線は横だけ淡く（縦の格子線は出さない）
    const fg = css.getPropertyValue('--muted').trim() || '#5B6573';
    const grid = css.getPropertyValue('--grid').trim() || 'rgba(0,0,0,0.08)';
    const bandEdges = new Set((bands ?? []).flatMap(([a, b]) => [a, b]));
    const opts: uPlot.Options = {
      width: Math.max(120, el.clientWidth),
      height,
      legend: { show: true, live: false },
      cursor: { drag: { x: false, y: false } },
      scales: { x: { time: false }, y: { range: (_u, min, max) => {
        // 自動で広げる（余白を足すだけで、上限・下限は付けない）
        if (!isFinite(min) || !isFinite(max)) return [0, 1];
        const pad = (max - min || Math.abs(max) || 1) * 0.08;
        return [min - pad, max + pad];
      } } },
      axes: [
        { stroke: fg, font: AXIS_FONT, labelFont: LABEL_FONT, grid: { show: false }, ticks: { stroke: grid }, label: xLabel, values: integerX ? (_u, v) => v.map((x) => (Number.isInteger(x) ? String(x) : '')) : undefined, size: 36 },
        { stroke: fg, font: AXIS_FONT, grid: { stroke: grid, width: 1 }, ticks: { show: false }, size: 44 },
      ],
      series: [
        {},
        ...series.map((s, i) => ({
          label: s.label,
          stroke: s.color,
          width: s.width ?? 1.6,
          dash: s.dash,
          // 実線の系列は、線の色から透明への縦グラデーションでグラフの底まで淡く塗る（面グラフ風）。破線（B・MC）と帯の境界の系列は塗らない
          ...(s.dash || bandEdges.has(i + 1)
            ? {}
            : {
                fill: (u: uPlot) => {
                  // 配置が決まる前（bbox が未確定）は単色で返す
                  if (!Number.isFinite(u.bbox?.top) || !Number.isFinite(u.bbox?.height)) return s.color + '18';
                  const g = u.ctx.createLinearGradient(0, u.bbox.top, 0, u.bbox.top + u.bbox.height);
                  g.addColorStop(0, s.color + '40');
                  g.addColorStop(1, s.color + '00');
                  return g;
                },
                fillTo: (u: uPlot) => u.scales.y.min ?? 0,
              }),
          show: !s.hidden,
          points: { show: s.points ?? data[0].length < 40, size: 4 },
        })),
      ],
      bands: bands?.map(([a, b, color]) => ({ series: [a, b] as [number, number], fill: color + Math.round(T.series.bandAlpha * 255).toString(16).padStart(2, '0') })),
    };
    plot.current = new uPlot(opts, data, el);
    const ro = new ResizeObserver(() => plot.current?.setSize({ width: Math.max(120, el.clientWidth), height }));
    ro.observe(el);
    return () => {
      ro.disconnect();
      plot.current?.destroy();
      plot.current = null;
    };
    // data は別の effect で更新する（系列の構成が変わったときだけ作り直す）
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);

  useEffect(() => {
    plot.current?.setData(data);
  }, [data, key]);

  const savePng = async () => {
    const src = plot.current?.ctx.canvas;
    if (!src || !exportName) return;
    // uPlot のキャンバスは背景が透明なので、画面と同じカードの色の地に重ねて保存する
    const c = document.createElement('canvas');
    c.width = src.width;
    c.height = src.height;
    const ctx = c.getContext('2d')!;
    ctx.fillStyle = T.chrome.card;
    ctx.fillRect(0, 0, c.width, c.height);
    ctx.drawImage(src, 0, 0);
    downloadBlob(`${exportName}.png`, await toBlob(c));
  };
  const saveSvg = () => {
    if (!exportName) return;
    const [x, ...cols] = data as unknown as number[][];
    downloadText(`${exportName}.svg`, lineChartSvg({ title, xLabel, x, series, cols, bands }), 'image/svg+xml');
  };

  return (
    <div className="plot-wrap">
      {exportName && (
        <div className="plot-actions">
          <button onClick={() => void savePng()} title="時系列を PNG で保存" data-testid={`${exportName}-png`}>PNG</button>
          <button onClick={saveSvg} title="時系列を SVG で保存" data-testid={`${exportName}-svg`}>SVG</button>
        </div>
      )}
      <div ref={host} className="plot" />
    </div>
  );
}
