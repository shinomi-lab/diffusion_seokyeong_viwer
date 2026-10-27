// 計算は Web Worker の中で動かし、メインスレッドを止めない（8.2）。ここにモデルの処理は書かない。
import init, { Simulator } from '../wasm/sim_wasm.js';
import type { OpName, Ops, Req, Res, StepResult } from './protocol';

type Handler<K extends OpName> = (sim: Simulator, a: Ops[K]['args']) => Ops[K]['res'];

/** 過去のラウンドのデータを、JS の値に読み出して WASM 側のオブジェクトを解放する。 */
function readRoundData(s: Simulator, r: number): Ops['roundData']['res'] {
  const d = s.roundData(r);
  try {
    return {
      summaryJson: d.summaryJson,
      events: { wave: d.wave, node: d.node, from: d.from, nArr: d.nArr, uInt: d.uInt, uExt: d.uExt, flags: d.flags },
      everBefore: d.everBefore,
      everAfter: d.everAfter,
    };
  } finally {
    d.free();
  }
}
type Handlers = { [K in OpName]: Handler<K> };

const handlers: Handlers = {
  loadGraph: (s, a) => s.loadGraph(a.bytes, JSON.stringify(a.opts)),
  paramSpecs: (s) => s.paramSpecs(),
  recParamSpecs: (s) => s.recParamSpecs(),
  recSim: (s) => s.recSim(),
  recWaveTrace: (s) => s.recWaveTrace(),
  createScenario: (s, a) => s.createScenario(a.configJson),
  centralityRanking: (s, a) => s.centralityRanking(a.metric, a.top, a.vcCsv),
  setIp: (s, a) => s.setIp(a.node),
  ringLayout: (s) => ({ angle: s.ringAngles(), dist: s.ringDist() }),
  step: (s, a): StepResult => {
    const summaryJson = s.step(a.choice);
    return {
      summaryJson,
      events: {
        wave: s.eventWave(),
        node: s.eventNode(),
        from: s.eventFrom(),
        nArr: s.eventNArr(),
        uInt: s.eventUInt(),
        uExt: s.eventUExt(),
        flags: s.eventFlags(),
      },
      ever: s.everFlags(),
    };
  },
  kinds: (s, a) => s.kinds(a.intMode),
  everFlags: (s) => s.everFlags(),
  nodeDetail: (s, a) => s.nodeDetail(a.node),
  groupSeries: (s, a) => s.groupSeries(a.sel, a.atRound, a.intMode, a.customNodes),
  sectorGrid: (s, a) => s.sectorGrid(a.intMode),
  selectNodes: (s, a) => s.selectNodes(a.ringLo, a.ringHi, a.angLo, a.angHi),
  selectUnreachable: (s) => s.selectUnreachable(),
  edgesAmong: (s, a) => s.edgesAmong(a.nodes, a.cap),
  runMc: (s, a) => s.runMc(a.seq, a.trials, a.seed),
  exportScenarioJson: (s) => s.exportScenarioJson(),
  exportRoundsCsv: (s) => s.exportRoundsCsv(),
  exportEventsCsv: (s, a) => s.exportEventsCsv(a.round),
  reset: (s) => {
    s.reset();
    return null;
  },
  branchTree: (s) => s.branchTree(),
  rewindTo: (s, a) => {
    s.rewindTo(a.round);
    return null;
  },
  switchTo: (s, a) => s.switchTo(a.id),
  roundData: (s, a) => readRoundData(s, a.r),
  importRecord: (s, a) => s.importRecord(a.recordJson),
};

const post = (m: Res, transfer: Transferable[] = []) =>
  (self as unknown as { postMessage(m: unknown, t: Transferable[]): void }).postMessage(m, transfer);

/** 結果に含まれる型付き配列のバッファを集める（コピーせず所有権を移す）。 */
function collectBuffers(v: unknown, out: Transferable[] = []): Transferable[] {
  if (ArrayBuffer.isView(v)) out.push(v.buffer as ArrayBuffer);
  else if (v && typeof v === 'object') for (const x of Object.values(v)) collectBuffers(x, out);
  return out;
}

const ready = init();
/** 比較（F13）の A・B で別々の Simulator を持つ。 */
const sims: Simulator[] = [];
const simFor = async (slot: number): Promise<Simulator> => {
  await ready;
  return (sims[slot] ??= new Simulator());
};

self.onmessage = async (ev: MessageEvent<Req>) => {
  const { id, op, args, slot = 0 } = ev.data;
  try {
    const sim = await simFor(slot);
    const h = handlers[op] as (s: Simulator, a: unknown) => unknown;
    const res = h(sim, args);
    post({ id, ok: true, res }, collectBuffers(res));
  } catch (e) {
    post({ id, ok: false, error: typeof e === 'string' ? e : e instanceof Error ? e.message : String(e) });
  }
};
