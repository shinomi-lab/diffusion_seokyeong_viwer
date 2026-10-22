// 再生（F06）。再生位置 k はウェーブ番号（-1 = ラウンド開始前）。
import { create } from 'zustand';

export const SPEEDS = [0.5, 1, 2, 4, 8, 16] as const;

interface PlaybackState {
  playing: boolean;
  /** 表示中のウェーブ。-1 = ラウンド開始前。 */
  k: number;
  /** ウェーブ/秒。 */
  speed: number;
  /** 最後のウェーブ番号（n_waves - 1）。ラウンドが無いときは -1。 */
  last: number;

  /** 新しいラウンドの再生を始める。 */
  start(nWaves: number): void;
  toggle(): void;
  stepWave(d: 1 | -1): void;
  skipToEnd(): void;
  setSpeed(s: number): void;
  /** 時間の経過で k を進める。戻り値は進んだか。 */
  advance(waves: number): void;
  setLast(last: number): void;
}

export const usePlayback = create<PlaybackState>((set, get) => ({
  playing: false,
  k: -1,
  speed: 4,
  last: -1,

  start: (nWaves) => set({ playing: nWaves > 0, k: -1, last: nWaves - 1 }),
  toggle: () => {
    const { playing, k, last } = get();
    if (!playing && k >= last) set({ k: -1, playing: last >= 0 });
    else set({ playing: !playing });
  },
  stepWave: (d) => {
    const { k, last } = get();
    set({ playing: false, k: Math.max(-1, Math.min(last, k + d)) });
  },
  skipToEnd: () => set({ playing: false, k: get().last }),
  setSpeed: (speed) => set({ speed }),
  advance: (waves) => {
    const { k, last, playing } = get();
    if (!playing) return;
    const nk = Math.min(last, k + waves);
    set({ k: nk, playing: nk < last });
  },
  setLast: (last) => set({ last, k: last, playing: false }),
}));
