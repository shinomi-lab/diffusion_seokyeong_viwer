import { describe, expect, it } from 'vitest';
import { clickAction, DX, DY, PAD, placeTree } from './tree';
import type { BranchTreeJson } from '../worker/protocol';

// 根 0 → 1(r1,'0') → 2(r2,'3') → 3(r3,'5')。2 から分岐して 4(r3,'2')（別の段）
const tree: BranchTreeJson = {
  nodes: [
    { id: 0, parent: null, round: 0, choice: '', lane: 0, n_received: 0, cum_xact: 0 },
    { id: 1, parent: 0, round: 1, choice: '0', lane: 0, n_received: 4, cum_xact: 0 },
    { id: 2, parent: 1, round: 2, choice: '3', lane: 0, n_received: 9, cum_xact: 1 },
    { id: 3, parent: 2, round: 3, choice: '5', lane: 0, n_received: 12, cum_xact: 3 },
    { id: 4, parent: 2, round: 3, choice: '2', lane: 1, n_received: 7, cum_xact: 2 },
  ],
  active_path: [1, 2, 3],
  current: 3,
  n_lanes: 2,
};

describe('placeTree', () => {
  it('ラウンドが横、段が縦。現在の枝と現在位置が分かる', () => {
    const p = placeTree(tree);
    const n = (id: number) => p.nodes.find((x) => x.id === id)!;
    expect([n(3).x, n(3).y]).toEqual([PAD + 3 * DX, PAD]);
    expect([n(4).x, n(4).y]).toEqual([PAD + 3 * DX, PAD + DY]);
    expect(n(3).onActive && n(3).current).toBe(true);
    expect(n(4).onActive).toBe(false);
    expect(p.edges).toHaveLength(4);
    expect(p.edges.filter((e) => e.active)).toHaveLength(3);
    expect(p.width).toBe(PAD * 2 + 3 * DX);
    expect(p.height).toBe(PAD * 2 + DY);
  });
});

describe('clickAction', () => {
  it('現在位置は何もしない／現在の枝なら巻き戻し／別の枝なら切り替え／根は 0 ラウンドへ', () => {
    expect(clickAction(tree, 3)).toEqual({ kind: 'none' });
    expect(clickAction(tree, 1)).toEqual({ kind: 'rewind', round: 1 });
    expect(clickAction(tree, 0)).toEqual({ kind: 'rewind', round: 0 });
    expect(clickAction(tree, 4)).toEqual({ kind: 'switch', id: 4 });
    expect(clickAction(tree, 99)).toEqual({ kind: 'none' });
  });
});
