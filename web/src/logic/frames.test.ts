import { describe, expect, it } from 'vitest';
import { emptyEvents, stateAtWave } from './frames';
import { KIND, kindOf } from './kinds';
import type { EventCols } from '../worker/protocol';

function events(rows: { wave: number; node: number; from: number; flags: number }[]): EventCols {
  const e = emptyEvents();
  return {
    ...e,
    wave: Uint16Array.from(rows.map((r) => r.wave)),
    node: Uint32Array.from(rows.map((r) => r.node)),
    from: Uint32Array.from(rows.map((r) => r.from)),
    nArr: Uint8Array.from(rows.map(() => 1)),
    uInt: new Float32Array(rows.length),
    uExt: new Float32Array(rows.length),
    flags: Uint8Array.from(rows.map((r) => r.flags)),
  };
}

describe('kindOf', () => {
  it('分類の規則（累積／このラウンドのみ）', () => {
    expect(kindOf(0, false, false, 0)).toBe(KIND.Unreceived);
    expect(kindOf(1, false, false, 0)).toBe(KIND.ReceivedOnly);
    expect(kindOf(1 | 2, false, false, 0)).toBe(KIND.Internal);
    expect(kindOf(1 | 2, false, false, 1)).toBe(KIND.ReceivedOnly);
    expect(kindOf(1 | 2, true, false, 1)).toBe(KIND.Internal);
    expect(kindOf(1 | 4, false, false, 1)).toBe(KIND.External);
    expect(kindOf(1 | 2 | 4, false, false, 0)).toBe(KIND.Both);
    expect(kindOf(1 | 2 | 4, false, false, 1)).toBe(KIND.External);
    expect(kindOf(0, false, true, 0)).toBe(KIND.Ip);
  });
});

describe('stateAtWave', () => {
  // IP=0。wave0: 1（内部）, 2（受信のみ）。wave1: 3（外部）, 4（両方）
  const ev = events([
    { wave: 0, node: 1, from: 0, flags: 1 | 4 },
    { wave: 0, node: 2, from: 0, flags: 0 },
    { wave: 1, node: 3, from: 1, flags: 2 },
    { wave: 1, node: 4, from: 1, flags: 1 | 2 | 4 },
  ]);
  const before = new Uint8Array(6);

  it('k=-1 は開始前、wave ごとに進む', () => {
    const s = stateAtWave(before, ev, -1, 0, 0);
    expect([...s.kinds]).toEqual([5, 0, 0, 0, 0, 0]);
    expect(s.pulse.every((x) => x === 0)).toBe(true);
    const s0 = stateAtWave(before, ev, 0, 0, 0);
    expect([...s0.kinds]).toEqual([5, 2, 1, 0, 0, 0]);
    expect([...s0.pulse]).toEqual([0, 1, 1, 0, 0, 0]);
    const s1 = stateAtWave(before, ev, 1, 0, 0);
    expect([...s1.kinds]).toEqual([5, 2, 1, 3, 4, 0]);
    expect([...s1.pulse]).toEqual([0, 0, 0, 1, 1, 0]);
  });

  it('このラウンドのみ: 前のラウンドの内部は数えず、外部は累積', () => {
    const b = new Uint8Array(6);
    b[5] = 1 | 2 | 4; // 前のラウンドで内部・外部が成立済み
    const s = stateAtWave(b, ev, 1, 1, 0);
    expect(s.kinds[5]).toBe(KIND.External);
    expect(s.kinds[1]).toBe(KIND.Internal);
  });

  it('入力を書き換えない', () => {
    const b = new Uint8Array(6);
    stateAtWave(b, ev, 1, 0, 0);
    expect(b.every((x) => x === 0)).toBe(true);
  });
});

describe('stateAtWave の推薦の経路（recommender-design.md 8.2）', () => {
  // IP=0。wave0: 1（フォロー）。wave1: 2（推薦、シェア）。wave2: 3（推薦由来）
  const ev = events([
    { wave: 0, node: 1, from: 0, flags: 0 },
    { wave: 1, node: 2, from: 0xffffffff, flags: 1 | 4 | 8 },
    { wave: 2, node: 3, from: 2, flags: 16 },
  ]);
  it('表示中のウェーブまでの経路だけを返す', () => {
    const b = new Uint8Array(4);
    expect([...stateAtWave(b, ev, 0, 0, 0).via]).toEqual([0, 0, 0, 0]);
    expect([...stateAtWave(b, ev, 1, 0, 0).via]).toEqual([0, 0, 1, 0]);
    expect([...stateAtWave(b, ev, 2, 0, 0).via]).toEqual([0, 0, 1, 2]);
  });
});

describe('stateAtWave の推薦の累積表示', () => {
  // ノード 1 は前のラウンドで推薦で受け取った（EVER_REC = 16）。このラウンドはノード 2 が推薦由来、ノード 3 が推薦
  const ev = events([
    { wave: 0, node: 2, from: 0, flags: 16 },
    { wave: 1, node: 3, from: 0xffffffff, flags: 8 },
  ]);
  const before = Uint8Array.from([0, 1 | 16, 0, 0]);
  it('累積なら前のラウンドの推薦も 1、このラウンドのみなら今回の分だけ', () => {
    expect([...stateAtWave(before, ev, 1, 0, 0, true).via]).toEqual([0, 1, 2, 1]);
    expect([...stateAtWave(before, ev, 1, 0, 0, false).via]).toEqual([0, 0, 2, 1]);
    expect([...stateAtWave(before, ev, -1, 0, 0, true).via]).toEqual([0, 1, 0, 0]);
  });
});
