// P1-7: 「すべて無効」「既定」「既定の2倍」の 3 水準を、同じ IP・系列・シードで流し比べる（9.8）。
// 通常の `npm test` では記録を書かずに検証だけ行う。`npm run compare` で docs/p1-compare/ に記録を書き出す。
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { beforeAll, describe, expect, it } from 'vitest';
import { initSync, Simulator } from '../wasm/sim_wasm.js';
import { defaultForm, presetMechanisms, toConfigJson, type PresetName } from './config';
import { parseSequence, sequenceString } from './units';
import type { ParamSpec } from '../worker/protocol';

const wasmPath = new URL('../wasm/sim_wasm_bg.wasm', import.meta.url);
const GRAPH = readFileSync(new URL('../../../test/100random.txt', import.meta.url));
const OUT = new URL('../../../docs/p1-compare/', import.meta.url);
const WRITE = process.env.COMPARE === '1';

const SEQUENCE = '0135024501'; // 左端がラウンド 1
const IP = 8;
const LEVELS: PresetName[] = ['off', 'balanced', 'double'];
const GRAPH_OPTS = JSON.stringify({ format: 'edgelist', directed: true, transposed: false, name: '100random.txt' });

describe.skipIf(!existsSync(wasmPath))('3 水準の流し比べ（100random.txt）', () => {
  let specs: ParamSpec[];
  beforeAll(() => {
    initSync({ module: readFileSync(wasmPath) });
    specs = JSON.parse(new Simulator().paramSpecs());
  });

  function run(level: PresetName, configJson?: string) {
    const sim = new Simulator();
    sim.loadGraph(GRAPH, GRAPH_OPTS);
    const form = { ...defaultForm(specs), receipt_probability: 0.6, mechanisms: presetMechanisms(level, specs) };
    sim.createScenario(configJson ?? toConfigJson(form));
    sim.setIp(IP);
    const summaries = parseSequence(SEQUENCE).map((c) => JSON.parse(sim.step(c)));
    return { sim, summaries };
  }

  it('同じ IP・系列・シードで 3 水準を流し、記録を書き出して、読み戻して再現できる', () => {
    const table: Record<string, number[]> = {};
    for (const level of LEVELS) {
      const { sim, summaries } = run(level);
      const json = sim.exportScenarioJson();
      const csv = sim.exportRoundsCsv();
      const rec = JSON.parse(json);
      expect(rec.sequence).toBe(SEQUENCE);
      expect(rec.ip.id).toBe(IP);
      expect(sequenceString(parseSequence(rec.sequence))).toBe(SEQUENCE);
      // 記録 JSON の設定から同じ系列を再現できる（ビット一致するので CSV も同一）
      const again = run(level, JSON.stringify(rec.config));
      expect(again.sim.exportRoundsCsv()).toBe(csv);
      table[level] = summaries.map((s: { cum_xact: number }) => s.cum_xact);
      expect(csv.split('\n')[0]).toBe('round,msg,new_share,new_xact,cum_xact,received,mean_u_int,mean_u_ext,p10_u_int,p50_u_int,p90_u_int,p10_u_ext,p50_u_ext,p90_u_ext,mean_u_int_all,mean_u_ext_all,rec_received,rec_share,rec_xact,chain_received,chain_xact,cum_rec');
      if (WRITE) {
        mkdirSync(OUT, { recursive: true });
        writeFileSync(new URL(`${level}.scenario.json`, OUT), json);
        writeFileSync(new URL(`${level}.rounds.csv`, OUT), csv);
      }
    }
    if (WRITE) {
      const rows = LEVELS.map((l) => `| ${l} | ${table[l].join(' | ')} |`).join('\n');
      const head = `| 水準 | ${SEQUENCE.split('').map((c, i) => `R${i + 1}(m${c})`).join(' | ')} |\n| --- | ${SEQUENCE.split('').map(() => '---').join(' | ')} |`;
      writeFileSync(new URL('summary.md', OUT), `# 累積外部行動者数（IP=ノード ${IP}、系列 ${SEQUENCE}、受信確率 0.6、シード 0）\n\n${head}\n${rows}\n`);
    }
    // 水準が違えば結果が違う（機構が効いている）
    expect(JSON.stringify(table.off)).not.toBe(JSON.stringify(table.balanced));
  });

  it('「すべて無効」は機構なしの設定と一致する（INV-02 の画面側の確認）', () => {
    const a = run('off');
    const b = run('off', JSON.stringify({ receipt_probability: 0.6 }));
    expect(a.sim.exportRoundsCsv()).toBe(b.sim.exportRoundsCsv());
  });
});
