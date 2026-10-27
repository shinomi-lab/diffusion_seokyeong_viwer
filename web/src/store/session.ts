// セッション（グラフ・シナリオ・IP・ラウンドごとの結果）。計算は Worker（sim-wasm）が行い、ここは結果を保持する。
import { create } from 'zustand';
import { defaultForm, formFromConfig, presetMechanisms, toConfigJson, type PresetName, type ScenarioForm } from '../logic/config';
import { computePositions, type Positions } from '../logic/layout';
import { sim } from '../worker/client';
import type { BranchTreeJson, EventCols, GraphInfo, GraphOpts, IpInfo, McResultJson, OpName, Ops, ParamSpec, RoundSummary, ScenarioInfo } from '../worker/protocol';

export interface RoundRecord {
  summary: RoundSummary;
  events: EventCols;
  /** ラウンド開始時の EVER_* ビット。 */
  everBefore: Uint8Array;
  everAfter: Uint8Array;
}

export const DEFAULT_GRAPH_OPTS: GraphOpts = { format: 'edgelist', gzip: false, directed: true, transposed: true, name: '' };

interface SessionState {
  ready: boolean;
  busy: boolean;
  error: string | null;
  specs: ParamSpec[];
  /** 推薦のスライダーの登録簿。 */
  recSpecs: ParamSpec[];
  /** 編集中の設定（適用するまで計算には反映されない）。 */
  form: ScenarioForm;
  /** 適用済みの設定の JSON（編集中との差で「未適用」を判定する）。 */
  appliedJson: string;
  graphOpts: GraphOpts;
  graph: GraphInfo | null;
  scenario: ScenarioInfo | null;
  ranking: { metric: 'degree' | 'pagerank' | 'vc'; nodes: Uint32Array } | null;
  ip: number | null;
  ipInfo: IpInfo | null;
  positions: Positions | null;
  dist: Uint32Array | null;
  angle: Float32Array | null;
  rounds: RoundRecord[];
  choices: number[];
  /** 現在のラウンド終了時の EVER_* ビット。 */
  ever: Uint8Array | null;
  vcCsv: string | null;
  /** 読み込んだグラフのバイト列（比較の B にも同じグラフを読み込むため）。 */
  graphBytes: Uint8Array | null;
  /** 分岐ツリー（F09）。 */
  tree: BranchTreeJson | null;
  /** MC の併記（F12）。 */
  mc: { running: boolean; result: McResultJson | null; ms: number };

  init(): Promise<void>;
  setForm(patch: Partial<ScenarioForm>): void;
  setGraphOpts(patch: Partial<GraphOpts>): void;
  applyPreset(name: PresetName): void;
  loadGraph(bytes: Uint8Array, name: string, opts?: Partial<GraphOpts>): Promise<void>;
  /** 編集中の設定でシナリオを作り直す（履歴は消える。INV-14）。 */
  applyForm(): Promise<void>;
  loadRanking(metric: 'degree' | 'pagerank' | 'vc'): Promise<void>;
  setVcCsv(text: string): void;
  chooseIp(node: number): Promise<void>;
  step(choice: number): Promise<RoundRecord | null>;
  restart(): Promise<void>;
  /** ラウンド `round` の終了時点へ戻る（0 = 初期状態。F09）。 */
  rewind(round: number): Promise<void>;
  /** 木の節の枝へ移る。 */
  switchTo(id: number): Promise<void>;
  refreshTree(): Promise<void>;
  /** 書き出したシナリオ記録 JSON を読み込む（設定・IP・分岐ツリー全体を復元）。 */
  importRecord(json: string): Promise<void>;
  runMc(trials: number): Promise<void>;
  loadConfig(cfg: Record<string, unknown>): void;
  clearError(): void;
}

const parse = <T,>(s: string) => JSON.parse(s) as T;

/** 系列文字列の 1 文字 → 選択。 */
const choiceNum = (c: string) => parseInt(c, 16);

/** 適用済みのシナリオで推薦が on か。 */
export const recOn = (s: { scenario: ScenarioInfo | null }): boolean =>
  ((s.scenario?.config as { recommender?: { enabled?: boolean } } | undefined)?.recommender?.enabled ?? false) === true;

/** 適用済みのシナリオで、推薦が on かつ方式が CF＋グラフか。 */
export const recHybrid = (s: { scenario: ScenarioInfo | null }): boolean =>
  recOn(s) && (s.scenario?.config as { recommender?: { method?: string } }).recommender?.method === 'hybrid';
export const recWave = (s: { scenario: ScenarioInfo | null }): boolean =>
  recOn(s) && (s.scenario?.config as { recommender?: { method?: string } }).recommender?.method === 'wave';

/** 比較（F13）の A = スロット 0、B = スロット 1。スロットごとに別の Simulator（Worker 内）を使う。 */
export function createSessionStore(slot: 0 | 1) {
  const call = <K extends OpName>(op: K, args: Ops[K]['args']): Promise<Ops[K]['res']> => sim.call(op, args, [], slot);
  return create<SessionState>((set, get) => {
  const run = async <T,>(f: () => Promise<T>): Promise<T | undefined> => {
    set({ busy: true, error: null });
    try {
      return await f();
    } catch (e) {
      set({ error: e instanceof Error ? e.message : String(e) });
      return undefined;
    } finally {
      set({ busy: false });
    }
  };

  const resetSessionState = {
    ip: null, ipInfo: null, positions: null, dist: null, angle: null, rounds: [] as RoundRecord[], choices: [] as number[], ever: null,
    tree: null as BranchTreeJson | null, mc: { running: false, result: null as McResultJson | null, ms: 0 },
  };

  /** ラウンド r（1 始まり）のデータを Worker から取り、画面用の記録にする。 */
  const fetchRound = async (r: number): Promise<RoundRecord> => {
    const d = await call('roundData', { r });
    return { summary: parse<RoundSummary>(d.summaryJson), events: d.events, everBefore: d.everBefore, everAfter: d.everAfter };
  };
  const loadTree = async (): Promise<BranchTreeJson> => parse<BranchTreeJson>(await call('branchTree', {}));

  return {
    ready: false,
    busy: false,
    error: null,
    specs: [],
    recSpecs: [],
    form: defaultForm([]),
    appliedJson: '',
    graphOpts: { ...DEFAULT_GRAPH_OPTS },
    graph: null,
    scenario: null,
    ranking: null,
    ...resetSessionState,
    vcCsv: null,
    graphBytes: null,

    async init() {
      const specs = parse<ParamSpec[]>(await call('paramSpecs', {}));
      const recSpecs = parse<ParamSpec[]>(await call('recParamSpecs', {}));
      set({ specs, recSpecs, form: defaultForm(specs), ready: true });
    },

    setForm(patch) {
      set({ form: { ...get().form, ...patch } });
    },

    setGraphOpts(patch) {
      set({ graphOpts: { ...get().graphOpts, ...patch } });
    },

    applyPreset(name) {
      const { form, specs } = get();
      set({ form: { ...form, mechanisms: presetMechanisms(name, specs) } });
    },

    loadConfig(cfg) {
      set({ form: formFromConfig(get().form, cfg) });
    },

    async loadGraph(bytes, name, opts) {
      const graphOpts = { ...get().graphOpts, ...opts, name };
      set({ graphOpts });
      await run(async () => {
        // 配列は所有権を移すので、コピーを渡す（元のバイト列は保持しない）
        const info = parse<GraphInfo>(await call('loadGraph', { bytes: bytes.slice(), opts: graphOpts }));
        set({ graph: info, graphBytes: bytes, scenario: null, ranking: null, ...resetSessionState });
        await get().applyForm();
        await get().loadRanking('degree');
      });
    },

    async applyForm() {
      if (!get().graph) return;
      await run(async () => {
        const json = toConfigJson(get().form);
        const info = parse<ScenarioInfo>(await call('createScenario', { configJson: json }));
        set({ scenario: info, appliedJson: json, ...resetSessionState });
      });
    },

    async loadRanking(metric) {
      if (!get().scenario) return;
      await run(async () => {
        const nodes = await call('centralityRanking', { metric, top: 50, vcCsv: get().vcCsv ?? undefined });
        set({ ranking: { metric, nodes } });
      });
    },

    setVcCsv(text) {
      set({ vcCsv: text });
    },

    async chooseIp(node) {
      if (!get().scenario) return;
      await run(async () => {
        const info = parse<IpInfo>(await call('setIp', { node }));
        const { angle, dist } = await call('ringLayout', {});
        const ever = await call('everFlags', {});
        set({ ip: node, ipInfo: info, positions: computePositions(angle, dist), dist, angle, rounds: [], choices: [], ever, mc: { running: false, result: null, ms: 0 } });
        await get().refreshTree();
      });
    },

    async step(choice) {
      const { ever, rounds, choices } = get();
      if (get().ip === null || !ever) return null;
      const r = await run(async () => {
        const res = await call('step', { choice });
        const rec: RoundRecord = {
          summary: parse<RoundSummary>(res.summaryJson),
          events: res.events,
          everBefore: ever,
          everAfter: res.ever,
        };
        set({ rounds: [...rounds, rec], choices: [...choices, choice], ever: res.ever, mc: { running: false, result: null, ms: 0 } });
        await get().refreshTree();
        return rec;
      });
      return r ?? null;
    },

    async restart() {
      const ip = get().ip;
      if (ip === null) return;
      await run(async () => {
        await call('reset', {});
        const ever = await call('everFlags', {});
        set({ rounds: [], choices: [], ever, mc: { running: false, result: null, ms: 0 } });
        await get().refreshTree();
      });
    },

    async refreshTree() {
      if (get().ip === null) {
        set({ tree: null });
        return;
      }
      set({ tree: await loadTree() });
    },

    async rewind(round) {
      const { rounds, choices } = get();
      if (get().ip === null || round < 0 || round > rounds.length) return;
      await run(async () => {
        await call('rewindTo', { round });
        const ever = await call('everFlags', {});
        set({ rounds: rounds.slice(0, round), choices: choices.slice(0, round), ever, mc: { running: false, result: null, ms: 0 } });
        await get().refreshTree();
      });
    },

    async switchTo(id) {
      if (get().ip === null) return;
      await run(async () => {
        const res = parse<{ round: number; changed_from: number }>(await call('switchTo', { id }));
        // 共通の祖先までのラウンドは そのまま。その先は新しい枝のデータを取り直す
        const kept = get().rounds.slice(0, res.changed_from);
        for (let r = res.changed_from + 1; r <= res.round; r++) kept.push(await fetchRound(r));
        const tree = await loadTree();
        const byId = new Map(tree.nodes.map((n) => [n.id, n]));
        const ever = await call('everFlags', {});
        set({ rounds: kept, choices: tree.active_path.map((i) => choiceNum(byId.get(i)!.choice)), ever, tree, mc: { running: false, result: null, ms: 0 } });
      });
    },

    async importRecord(json) {
      const rec = JSON.parse(json) as { config?: Record<string, unknown>; ip?: { id: number } };
      if (!get().graph) {
        set({ error: '先にグラフを読み込んでください（記録と同じグラフが必要です）' });
        return;
      }
      await run(async () => {
        const imported = parse<{ scenario: ScenarioInfo; ip: IpInfo }>(await call('importRecord', { recordJson: json }));
        const info = imported.ip;
        if (rec.config) set({ form: formFromConfig(get().form, rec.config) });
        const { angle, dist } = await call('ringLayout', {});
        const tree = await loadTree();
        const byId = new Map(tree.nodes.map((n) => [n.id, n]));
        const rounds: RoundRecord[] = [];
        for (let r = 1; r <= tree.active_path.length; r++) rounds.push(await fetchRound(r));
        const ever = await call('everFlags', {});
        set({
          scenario: imported.scenario, appliedJson: toConfigJson(get().form),
          ip: info.ip, ipInfo: info, positions: computePositions(angle, dist), dist, angle, rounds, ever, tree,
          choices: tree.active_path.map((i) => choiceNum(byId.get(i)!.choice)),
          mc: { running: false, result: null, ms: 0 },
        });
      });
    },

    async runMc(trials) {
      const { ip, choices } = get();
      if (ip === null || choices.length === 0) return;
      set({ mc: { running: true, result: null, ms: 0 }, error: null });
      const t0 = performance.now();
      try {
        const res = await call('runMc', { seq: Int8Array.from(choices), trials, seed: 1 });
        set({ mc: { running: false, result: parse<McResultJson>(res), ms: performance.now() - t0 } });
      } catch (e) {
        set({ mc: { running: false, result: null, ms: 0 }, error: e instanceof Error ? e.message : String(e) });
      }
    },

    clearError() {
      set({ error: null });
    },
  };
  });
}

export const useSession = createSessionStore(0);
export const useSessionB = createSessionStore(1);

/** 編集中の設定が適用済みの設定と違うか。 */
export function isDirty(s: Pick<SessionState, 'form' | 'appliedJson' | 'scenario'>): boolean {
  return !!s.scenario && toConfigJson(s.form) !== s.appliedJson;
}
