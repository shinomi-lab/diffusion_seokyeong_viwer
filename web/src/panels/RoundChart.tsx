// ラウンド別の時系列（F08）。新規シェア数・新規外部行動者数・累積外部行動者数、効用の平均と分位点。
// MC の併記（F12: 平均と 10〜90% の帯）と、比較モード（F13）の B（破線）を同じ軸に重ねる。
import { useMemo } from 'react';
import { useCompare } from '../store/actions';
import { useSession, useSessionB } from '../store/session';
import { T } from '../tokens';
import { UPlotChart, type PlotSeries } from './Plot';

type Cols = { x: number[]; series: PlotSeries[]; cols: number[][]; bands: [number, number, string][] };

function empty(x: number[]): Cols {
  return { x, series: [], cols: [], bands: [] };
}

function push(c: Cols, s: PlotSeries, v: number[]): number {
  c.series.push(s);
  c.cols.push(v);
  return c.series.length; // uPlot の系列番号（x が 0）
}

export function RoundChart() {
  const A = useSession((s) => s.rounds);
  const mc = useSession((s) => s.mc.result);
  const compare = useCompare((c) => c.enabled);
  const B = useSessionB((s) => s.rounds);

  const counts = useMemo(() => {
    const c = empty(A.map((r) => r.summary.round));
    push(c, { label: '新規シェア', color: T.series.newShare }, A.map((r) => r.summary.n_share));
    push(c, { label: '新規外部行動', color: T.series.newXact }, A.map((r) => r.summary.new_xact));
    push(c, { label: '累積外部行動', color: T.series.cumXact }, A.map((r) => r.summary.cum_xact));
    if (compare) {
      const pad = (f: (r: (typeof B)[number]) => number) => A.map((_, i) => (B[i] ? f(B[i]) : NaN));
      push(c, { label: 'B 新規シェア', color: T.series.newShare, dash: [6, 4] }, pad((r) => r.summary.n_share));
      push(c, { label: 'B 新規外部行動', color: T.series.newXact, dash: [6, 4] }, pad((r) => r.summary.new_xact));
      push(c, { label: 'B 累積外部行動', color: T.series.cumXact, dash: [6, 4] }, pad((r) => r.summary.cum_xact));
    }
    if (mc && mc.rounds === A.length) {
      const m = push(c, { label: 'MC 累積外部行動（平均）', color: T.series.both }, mc.cum_xact.mean);
      const lo = push(c, { label: 'MC p10', color: T.series.both, hidden: true, width: 0 }, mc.cum_xact.p10);
      const hi = push(c, { label: 'MC p90', color: T.series.both, hidden: true, width: 0 }, mc.cum_xact.p90);
      c.bands.push([hi, lo, T.series.both]);
      void m;
    }
    return c;
  }, [A, B, compare, mc]);

  const util = useMemo(() => {
    const c = empty(A.map((r) => r.summary.round));
    const add = (name: string, color: string, f: (r: (typeof A)[number]) => [number, number, number], rows: typeof A, dash?: number[]) => {
      push(c, { label: `${name} 平均`, color, dash }, rows.map((r) => f(r)[0]));
      const lo = push(c, { label: `${name} p10`, color, hidden: true, width: 0 }, rows.map((r) => f(r)[1]));
      const hi = push(c, { label: `${name} p90`, color, hidden: true, width: 0 }, rows.map((r) => f(r)[2]));
      c.bands.push([hi, lo, color]);
    };
    add('内部', T.series.internal, (r) => [r.summary.recv.mean_int, r.summary.recv.q_int[0], r.summary.recv.q_int[2]], A);
    add('外部', T.series.external, (r) => [r.summary.recv.mean_ext, r.summary.recv.q_ext[0], r.summary.recv.q_ext[2]], A);
    if (compare && B.length === A.length) {
      push(c, { label: 'B 内部 平均', color: T.series.internal, dash: [6, 4] }, B.map((r) => r.summary.recv.mean_int));
      push(c, { label: 'B 外部 平均', color: T.series.external, dash: [6, 4] }, B.map((r) => r.summary.recv.mean_ext));
    }
    return c;
  }, [A, B, compare]);

  return (
    <section className="round-chart" aria-label="ラウンド別時系列">
      <div className="chart-col">
        <h3>ラウンド別の人数{compare && '（破線 = B）'}</h3>
        <UPlotChart data={[counts.x, ...counts.cols]} xLabel="ラウンド" series={counts.series} bands={counts.bands} exportName="round-counts" title="ラウンド別の人数" />
      </div>
      <div className="chart-col">
        <h3>効用（受信済みノードの平均と 10〜90% の帯）</h3>
        <UPlotChart data={[util.x, ...util.cols]} xLabel="ラウンド" series={util.series} bands={util.bands} exportName="round-utility" title="効用（受信済みノードの平均と 10〜90% の帯）" />
      </div>
    </section>
  );
}
