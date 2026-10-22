// 実際の sim-wasm（ビルド済みの web/src/wasm）を Node で動かして、画面側の純粋関数を照合する。
// T-18: stateAtWave の最終ウェーブが kinds() と一致する。プリセットが Rust 側の値と一致する。
import { existsSync, readFileSync } from 'node:fs';
import { beforeAll, describe, expect, it } from 'vitest';
import { initSync, Simulator } from '../wasm/sim_wasm.js';
import { DEFAULT_MESSAGES, defaultForm, presetMechanisms, toConfigJson } from './config';
import { emptyEvents, stateAtWave } from './frames';
import type { EventCols, ParamSpec } from '../worker/protocol';

const wasmPath = new URL('../wasm/sim_wasm_bg.wasm', import.meta.url);
const GRAPH = readFileSync(new URL('../../../test/100random.txt', import.meta.url));
const have = existsSync(wasmPath);

function events(sim: Simulator): EventCols {
  return {
    wave: sim.eventWave(), node: sim.eventNode(), from: sim.eventFrom(), nArr: sim.eventNArr(),
    uInt: sim.eventUInt(), uExt: sim.eventUExt(), flags: sim.eventFlags(),
  };
}

describe.skipIf(!have)('sim-wasm との照合（100random.txt）', () => {
  let specs: ParamSpec[];
  beforeAll(() => {
    initSync({ module: readFileSync(wasmPath) });
    specs = JSON.parse(new Simulator().paramSpecs());
  });

  function setup(mech: 'off' | 'balanced' | 'double', ip = 0) {
    const sim = new Simulator();
    sim.loadGraph(GRAPH, JSON.stringify({ format: 'edgelist', directed: true, transposed: false }));
    const form = { ...defaultForm(specs), receipt_probability: 0.6, mechanisms: presetMechanisms(mech, specs) };
    const info = JSON.parse(sim.createScenario(toConfigJson(form)));
    sim.setIp(ip);
    return { sim, info };
  }

  it('T-18: 最終ウェーブが kinds() と一致し、k=-1 が開始前と一致する（両モード・全機構）', () => {
    for (const mech of ['off', 'balanced'] as const) {
      const { sim } = setup(mech, 3);
      let everBefore: Uint8Array = new Uint8Array(100);
      let prevKinds0 = sim.kinds(0);
      for (const choice of [0, 3, 5, 1, 2, 4, 0]) {
        const summary = JSON.parse(sim.step(choice));
        const ev = events(sim);
        const last = summary.n_waves - 1;
        for (const intMode of [0, 1] as const) {
          expect([...stateAtWave(everBefore, ev, last, intMode, 3).kinds]).toEqual([...sim.kinds(intMode)]);
        }
        // k=-1 は直前のラウンド終了時の状態（累積）と一致
        expect([...stateAtWave(everBefore, emptyEvents(), -1, 0, 3).kinds]).toEqual([...prevKinds0]);
        expect([...stateAtWave(everBefore, ev, -1, 0, 3).kinds]).toEqual([...prevKinds0]);
        everBefore = sim.everFlags();
        prevKinds0 = sim.kinds(0);
      }
    }
  });

  it('既定の文言: メッセージ空間と機構プリセットが Rust 側と一致する', () => {
    const { info } = setup('off');
    expect(info.config.message_space).toEqual(DEFAULT_MESSAGES);
    for (const [name, want] of [
      ['balanced', { m1: { kappa: 0.05 } }],
      ['double', { m1: { kappa: 0.1 } }],
    ] as const) {
      const m = setup(name).info.config.mechanisms;
      for (const [id, vals] of Object.entries(want)) {
        expect(m[id].enabled).toBe(true);
        for (const [k, v] of Object.entries(vals)) expect(m[id][k]).toBeCloseTo(v as number, 12);
      }
    }
    const off = setup('off').info.config.mechanisms;
    expect(Object.values(off).every((v) => (v as { enabled: boolean }).enabled === false)).toBe(true);
  });

  it('「すべて無効」の結果は機構なしの既定設定と同じ（INV-02 の画面側の確認）', () => {
    const a = setup('off').sim;
    const b = new Simulator();
    b.loadGraph(GRAPH, JSON.stringify({ format: 'edgelist', directed: true, transposed: false }));
    b.createScenario(JSON.stringify({ receipt_probability: 0.6 }));
    b.setIp(0);
    for (const c of [0, 2, 5]) expect(a.step(c)).toBe(b.step(c));
  });
});
