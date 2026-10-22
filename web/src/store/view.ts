// 表示の状態（モード、選択、カメラ、タブ）。
import { create } from 'zustand';

export type DetailTab = 'node' | 'group';

export interface Camera {
  cx: number;
  cy: number;
  /** ワールド 1 単位あたりの CSS ピクセル数。 */
  zoom: number;
}

interface ViewState {
  /** overview: リング・扇形の全体図。expanded: 個別ノード。 */
  mode: 'overview' | 'expanded';
  /** 展開中のノード集合。null は全ノード。 */
  expanded: Uint32Array | null;
  expandedLabel: string;
  /** 全体図から展開したか（「全体図に戻る」を出す）。 */
  fromOverview: boolean;
  selectedNode: number | null;
  /** リング・扇形・範囲選択で作った群。 */
  groupNodes: Uint32Array | null;
  tab: DetailTab;
  /** 0 = 累積、1 = このラウンドのみ（F07）。 */
  intMode: 0 | 1;
  camera: Camera;
  /** 描画する辺が上限で打ち切られたか。 */
  edgesTruncated: boolean;
  notice: string | null;

  setMode(mode: 'overview' | 'expanded'): void;
  expand(nodes: Uint32Array, label: string): void;
  backToOverview(): void;
  selectNode(n: number | null): void;
  setGroup(nodes: Uint32Array | null): void;
  setTab(t: DetailTab): void;
  setIntMode(m: 0 | 1): void;
  setCamera(c: Camera): void;
  setEdgesTruncated(v: boolean): void;
  setNotice(s: string | null): void;
  resetView(mode: 'overview' | 'expanded'): void;
}

export const useView = create<ViewState>((set) => ({
  mode: 'expanded',
  expanded: null,
  expandedLabel: '',
  fromOverview: false,
  selectedNode: null,
  groupNodes: null,
  tab: 'node',
  intMode: 0,
  camera: { cx: 0, cy: 0, zoom: 30 },
  edgesTruncated: false,
  notice: null,

  setMode: (mode) => set({ mode }),
  // 展開したリング・扇形は、そのまま「選択した群」としても使える（F19）
  expand: (nodes, label) => set({ mode: 'expanded', expanded: nodes, expandedLabel: label, fromOverview: true, groupNodes: nodes }),
  backToOverview: () => set({ mode: 'overview', expanded: null, expandedLabel: '', fromOverview: false }),
  selectNode: (n) => set({ selectedNode: n, ...(n !== null ? { tab: 'node' as const } : {}) }),
  setGroup: (nodes) => set({ groupNodes: nodes, ...(nodes ? { tab: 'group' as const } : {}) }),
  setTab: (tab) => set({ tab }),
  setIntMode: (intMode) => set({ intMode }),
  setCamera: (camera) => set({ camera }),
  setEdgesTruncated: (edgesTruncated) => set({ edgesTruncated }),
  setNotice: (notice) => set({ notice }),
  resetView: (mode) =>
    set({ mode, expanded: null, expandedLabel: '', fromOverview: false, selectedNode: null, groupNodes: null, notice: null }),
}));
