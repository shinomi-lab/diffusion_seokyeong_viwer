import { describe, expect, it } from 'vitest';
import { coverage } from './mc';

describe('coverage', () => {
  it('実現値が p10〜p90（両端を含む）に入るラウンド数を数える', () => {
    const band = { p10: [0, 1, 2, 3], p90: [2, 3, 4, 5] };
    expect(coverage([1, 3, 5, 4], band)).toEqual({ inside: 3, total: 4 });
    expect(coverage([0, 1, 2, 3], band)).toEqual({ inside: 4, total: 4 });
    expect(coverage([9, 9, 9, 9], band)).toEqual({ inside: 0, total: 4 });
    // 長さが違うときは短い方に合わせる
    expect(coverage([1, 3], band)).toEqual({ inside: 2, total: 2 });
  });
});
