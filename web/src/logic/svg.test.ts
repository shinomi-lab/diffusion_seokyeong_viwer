import { describe, expect, it } from 'vitest';
import { escapeXml, lineChartSvg, niceTicks } from './svg';

const base = {
  x: [1, 2, 3, 4],
  series: [
    { label: '平均', color: '#0072B2' },
    { label: '平均 p10', color: '#0072B2', hidden: true },
    { label: '平均 p90', color: '#0072B2', hidden: true },
    { label: 'B 平均', color: '#009E73', dash: [6, 4] },
  ],
  cols: [[0.1, 0.2, 0.4, 0.5], [0.0, 0.1, 0.3, 0.4], [0.2, 0.3, 0.5, 0.6], [0.1, NaN, 0.3, 0.2]],
  bands: [[3, 2, '#0072B2']] as [number, number, string][],
};

describe('lineChartSvg', () => {
  it('SVG の体裁・系列・帯・凡例を出す（隠した系列は線にも凡例にも出さない）', () => {
    const s = lineChartSvg({ ...base, title: 'ラウンド別', xLabel: 'ラウンド' });
    expect(s.startsWith('<svg xmlns="http://www.w3.org/2000/svg"')).toBe(true);
    expect(s.trimEnd().endsWith('</svg>')).toBe(true);
    // 線: 平均 1 + B 平均が NaN で 2 本に分かれず 1 つの path（M と L の組）
    expect((s.match(/<path /g) ?? []).length).toBe(2);
    expect((s.match(/<polygon /g) ?? []).length).toBe(1);
    expect(s).toContain('stroke-dasharray="6 4"');
    expect(s).toContain('>平均<');
    expect(s).not.toContain('平均 p10');
    expect(s).not.toContain('NaN');
  });

  it('NaN で線を切る（B 平均は 2 つの部分に分かれる）', () => {
    const s = lineChartSvg(base);
    const b = s.split('\n').find((l) => l.includes('#009E73') && l.includes('<path'))!;
    expect((b.match(/M/g) ?? []).length).toBe(2);
  });

  it('文字をエスケープする', () => {
    expect(escapeXml('A<B&"C">\'')).toBe('A&lt;B&amp;&quot;C&quot;&gt;&apos;');
    const s = lineChartSvg({ ...base, title: 'x<y&z' });
    expect(s).toContain('x&lt;y&amp;z');
  });

  it('縦軸は値に合わせて広がる（1 を超えても切らない）', () => {
    const s = lineChartSvg({ x: [0, 1, 2], series: [{ label: '効用', color: '#000' }], cols: [[0, 1, 1.73]] });
    const ticks = [...s.matchAll(/text-anchor="end"[^>]*>([-\d.]+)</g)].map((m) => Number(m[1]));
    expect(Math.max(...ticks)).toBeGreaterThanOrEqual(1.5);
    expect(s).not.toContain('NaN');
  });

  it('データが空・1 点でも壊れない', () => {
    expect(lineChartSvg({ x: [], series: [], cols: [] })).toContain('</svg>');
    expect(lineChartSvg({ x: [1], series: [{ label: 'a', color: '#000' }], cols: [[2]] })).not.toContain('NaN');
  });
});

describe('niceTicks', () => {
  it('きりのよい目盛りで範囲を覆う', () => {
    const t = niceTicks(-0.43, 1.73, 5);
    expect(t[0]).toBeLessThanOrEqual(-0.43);
    expect(t[t.length - 1]).toBeGreaterThanOrEqual(1.73);
    expect(niceTicks(0, 10, 5)).toEqual([0, 2, 4, 6, 8, 10]);
    expect(niceTicks(3, 3)).toEqual([2, 3, 4]);
  });
});
