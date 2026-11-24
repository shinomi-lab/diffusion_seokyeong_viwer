// ラウンド別の時系列（F08）。新規シェア数・新規外部行動者数・累積外部行動者数、効用の平均と分位点。
// MC の併記（F12: 平均と 10〜90% の帯）を同じ軸に重ねる。
import { useMemo } from 'react';
import { recOn, useSession } from '../store/session';
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
  const rec = useSession(recOn);

  const counts = useMemo(() => {
    const c = empty(A.map((r) => r.summary.round));
    push(c, { label: '新規シェア', color: T.series.newShare }, A.map((r) => r.summary.n_share));
    push(c, { label: '新規外部行動', color: T.series.newXact }, A.map((r) => r.summary.new_xact));
    push(c, { label: '累積外部行動', color: T.series.cumXact }, A.map((r) => r.summary.cum_xact));
    push(c, { label: '累積シェア', color: T.series.cumShare }, A.map((r) => r.summary.cum_share));
    if (rec) {
      // 推薦（Rモデル。recommender-v3-design.md 8 章）。推薦 off では出さない。系列は「推薦で受信」の 2 つだけにしている（2026-10-27）
      push(c, { label: '推薦で受信', color: T.series.recXact, dash: [8, 5] }, A.map((r) => r.summary.rec_received));
      push(c, { label: '推薦で受信（累積人数）', color: T.series.recXact, dash: [14, 4, 3, 4] }, A.map((r) => r.summary.cum_rec));
    }
    if (mc && mc.rounds === A.length) {
      const m = push(c, { label: 'MC 累積外部行動（平均）', color: T.series.both }, mc.cum_xact.mean);
      const lo = push(c, { label: 'MC p10', color: T.series.both, hidden: true, width: 0 }, mc.cum_xact.p10);
      const hi = push(c, { label: 'MC p90', color: T.series.both, hidden: true, width: 0 }, mc.cum_xact.p90);
      c.bands.push([hi, lo, T.series.both]);
      void m;
      if (rec) {
        // 推薦で受信（累積人数）の MC 平均と 10〜90% の帯
        push(c, { label: 'MC 推薦で受信・累積（平均）', color: T.series.recXact, dash: [14, 4, 3, 4] }, mc.cum_rec.mean);
        const rl = push(c, { label: 'MC 推薦 p10', color: T.series.recXact, hidden: true, width: 0 }, mc.cum_rec.p10);
        const rh = push(c, { label: 'MC 推薦 p90', color: T.series.recXact, hidden: true, width: 0 }, mc.cum_rec.p90);
        c.bands.push([rh, rl, T.series.recXact]);
      }
    }
    return c;
  }, [A, mc, rec]);

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
    return c;
  }, [A]);

  return (
    <section className="round-chart" aria-label="ラウンド別時系列">
      <div className="chart-col">
        <h3>ラウンド別の人数</h3>
        <UPlotChart data={[counts.x, ...counts.cols]} xLabel="ラウンド" series={counts.series} bands={counts.bands} exportName="round-counts" title="ラウンド別の人数" />
      </div>
      <div className="chart-col">
        <h3>効用（受信済みノードの平均と 10〜90% の帯）</h3>
        <UPlotChart data={[util.x, ...util.cols]} xLabel="ラウンド" series={util.series} bands={util.bands} exportName="round-utility" title="効用（受信済みノードの平均と 10〜90% の帯）" />
      </div>
    </section>
  );
}
