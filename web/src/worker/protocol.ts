// Worker とのやり取りの型を 1 か所で定義する（8.2）。操作名は sim-wasm のメソッド名と同じ。
// 配列は postMessage の transfer で所有権を移す（コピーしない）。

export interface GraphOpts {
  format: 'edgelist' | 'csv' | 'tsv';
  gzip: boolean;
  directed: boolean;
  transposed: boolean;
  name: string;
}

export interface EventCols {
  wave: Uint16Array;
  node: Uint32Array;
  from: Uint32Array;
  nArr: Uint8Array;
  uInt: Float32Array;
  uExt: Float32Array;
  /** bit0 = int_fired, bit1 = ext_fired, bit2 = enqueued */
  flags: Uint8Array;
}

export interface StepResult {
  summaryJson: string;
  events: EventCols;
  /** 発信後の EVER_* ビット（1=受信, 2=内部, 4=外部）。 */
  ever: Uint8Array;
}

/** 操作ごとの引数と戻り値。 */
export interface Ops {
  loadGraph: { args: { bytes: Uint8Array; opts: GraphOpts }; res: string };
  paramSpecs: { args: Record<string, never>; res: string };
  createScenario: { args: { configJson: string }; res: string };
  centralityRanking: { args: { metric: 'degree' | 'pagerank' | 'vc'; top: number; vcCsv?: string }; res: Uint32Array };
  setIp: { args: { node: number }; res: string };
  ringLayout: { args: Record<string, never>; res: { angle: Float32Array; dist: Uint32Array } };
  step: { args: { choice: number }; res: StepResult };
  kinds: { args: { intMode: 0 | 1 }; res: Uint8Array };
  everFlags: { args: Record<string, never>; res: Uint8Array };
  nodeDetail: { args: { node: number }; res: string };
  groupSeries: { args: { sel: number; atRound: number; intMode: 0 | 1; customNodes?: Uint32Array }; res: string };
  sectorGrid: { args: { intMode: 0 | 1 }; res: Uint32Array };
  selectNodes: { args: { ringLo: number; ringHi: number; angLo: number; angHi: number }; res: Uint32Array };
  selectUnreachable: { args: Record<string, never>; res: Uint32Array };
  edgesAmong: { args: { nodes: Uint32Array; cap: number }; res: Uint32Array };
  runMc: { args: { seq: Int8Array; trials: number; seed: number }; res: string };
  exportScenarioJson: { args: Record<string, never>; res: string };
  exportRoundsCsv: { args: Record<string, never>; res: string };
  exportEventsCsv: { args: { round?: number }; res: string };
  reset: { args: Record<string, never>; res: null };
  branchTree: { args: Record<string, never>; res: string };
  rewindTo: { args: { round: number }; res: null };
  switchTo: { args: { id: number }; res: string };
  roundData: { args: { r: number }; res: RoundDataRes };
  importRecord: { args: { recordJson: string }; res: string };
}

/** 過去のラウンド 1 つぶん。 */
export interface RoundDataRes {
  summaryJson: string;
  events: EventCols;
  everBefore: Uint8Array;
  everAfter: Uint8Array;
}

export type OpName = keyof Ops;

/** `slot`: 比較（F13）の A = 0 / B = 1。それぞれ別の Simulator を持つ。 */
export type Req = { [K in OpName]: { id: number; slot?: number; op: K; args: Ops[K]['args'] } }[OpName];
export type Res = { id: number; ok: true; res: unknown } | { id: number; ok: false; error: string };

// ---- 画面側で使う型（JSON の形） ----

export interface UtilityStats {
  n: number;
  mean_int: number;
  mean_ext: number;
  q_int: [number, number, number];
  q_ext: [number, number, number];
}

export type ChoiceJson = { Msg: number };

export interface RoundSummary {
  round: number;
  choice: ChoiceJson;
  n_received: number;
  n_share: number;
  new_xact: number;
  cum_xact: number;
  n_waves: number;
  n_int_active_now: number;
  recv: UtilityStats;
  all: UtilityStats;
}

export interface GraphInfo {
  n_nodes: number;
  n_edges: number;
  hash: number;
  directed: boolean;
  transposed: boolean;
  warnings: string[];
}

export interface ScenarioInfo {
  config: Record<string, unknown>;
  mean_int: number;
  mean_ext: number;
  n_nodes: number;
  n_messages: number;
}

export interface IpInfo {
  ring_sizes: number[];
  unreachable: number;
  ip: number;
  ip_label: number;
}

export interface ParamSpec {
  id: string;
  mechanism: string;
  label: string;
  kind: 'f64' | 'u32' | 'bool';
  min: number | null;
  max: number | null;
  step: number | null;
  default: number;
  off: number;
}

export interface RecvInfo {
  round: number;
  wave: number;
  from: number;
  from_label: number;
  n_arr: number;
}

export interface NodeDetailJson {
  node: number;
  label: number;
  u_int: number[];
  u_ext: number[];
  received: RecvInfo[];
  int_rounds: number[];
  ext_round: number | null;
  is_ip: boolean;
}

export interface Band {
  mean: number[];
  p10: number[];
  p50: number[];
  p90: number[];
}

export interface GroupSeriesJson {
  at_round: number;
  members: number;
  int: Band;
  ext: Band;
  size_by_round: number[];
}

// ---- 分岐ツリー・MC（JSON の形） ----

export interface BranchNodeJson {
  id: number;
  parent: number | null;
  round: number;
  /** 系列文字列の 1 文字（根は空）。 */
  choice: string;
  lane: number;
  n_received: number;
  cum_xact: number;
}

export interface BranchTreeJson {
  nodes: BranchNodeJson[];
  active_path: number[];
  current: number;
  n_lanes: number;
}

export interface McResultJson {
  rounds: number;
  trials: number;
  new_xact: Band;
  cum_xact: Band;
  new_share: Band;
  mean_u_int: Band;
  mean_u_ext: Band;
  total_xact_mean: number;
  total_share_mean: number;
}
