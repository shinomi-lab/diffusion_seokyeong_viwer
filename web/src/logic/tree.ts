// 分岐ツリー（F09）の描画用の配置。段（lane）は Rust 側（sim-api の BranchTree::lanes）が決めている。
import type { BranchNodeJson, BranchTreeJson } from '../worker/protocol';

export const DX = 30;
export const DY = 28;
export const PAD = 18;

export interface PlacedNode extends BranchNodeJson {
  x: number;
  y: number;
  onActive: boolean;
  current: boolean;
}

export interface Placed {
  nodes: PlacedNode[];
  edges: { x1: number; y1: number; x2: number; y2: number; active: boolean }[];
  width: number;
  height: number;
}

export function placeTree(t: BranchTreeJson): Placed {
  const active = new Set([0, ...t.active_path]);
  const nodes: PlacedNode[] = t.nodes.map((n) => ({
    ...n,
    x: PAD + n.round * DX,
    y: PAD + n.lane * DY,
    onActive: active.has(n.id),
    current: n.id === t.current,
  }));
  const byId = new Map(nodes.map((n) => [n.id, n]));
  const edges = nodes
    .filter((n) => n.parent !== null)
    .map((n) => {
      const p = byId.get(n.parent as number)!;
      return { x1: p.x, y1: p.y, x2: n.x, y2: n.y, active: n.onActive && p.onActive };
    });
  const maxRound = Math.max(0, ...t.nodes.map((n) => n.round));
  return { nodes, edges, width: PAD * 2 + maxRound * DX, height: PAD * 2 + (t.n_lanes - 1) * DY };
}

/** 節をクリックしたときの動作: 現在位置なら何もしない／現在の枝の上なら巻き戻し／別の枝なら枝を切り替える。 */
export function clickAction(t: BranchTreeJson, id: number): { kind: 'none' } | { kind: 'rewind'; round: number } | { kind: 'switch'; id: number } {
  const node = t.nodes.find((n) => n.id === id);
  if (!node || id === t.current) return { kind: 'none' };
  if (id === 0 || t.active_path.includes(id)) return { kind: 'rewind', round: node.round };
  return { kind: 'switch', id };
}
