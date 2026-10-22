// 画面側の扇形集計が sim-api の overview::aggregate（sectorGrid）と一致する。
import { existsSync, readFileSync } from 'node:fs';
import { beforeAll, describe, expect, it } from 'vitest';
import { initSync, Simulator } from '../wasm/sim_wasm.js';
import { aggregateSectors } from './sectors';

const wasmPath = new URL('../wasm/sim_wasm_bg.wasm', import.meta.url);
const GRAPH = readFileSync(new URL('../../../test/100random.txt', import.meta.url));

describe.skipIf(!existsSync(wasmPath))('aggregateSectors', () => {
  beforeAll(() => initSync({ module: readFileSync(wasmPath) }));
  it('sectorGrid と一致する（両モード、各ラウンド）', () => {
    const sim = new Simulator();
    sim.loadGraph(GRAPH, JSON.stringify({ format: 'edgelist', directed: true, transposed: false }));
    sim.createScenario(JSON.stringify({ receipt_probability: 0.7 }));
    sim.setIp(8);
    const angle = sim.ringAngles();
    const dist = sim.ringDist();
    for (const c of [0, 3, 5, 1, 0, 2]) {
      sim.step(c);
      for (const m of [0, 1] as const) {
        const mine = aggregateSectors(sim.kinds(m), dist, angle);
        expect([...mine]).toEqual([...sim.sectorGrid(m)]);
      }
    }
  });
});
