// 画面に出す状態（kinds と pulse）。最新ラウンドのイベント列と再生位置 k から stateAtWave で作る。
import { useMemo } from 'react';
import { emptyEvents, stateAtWave } from '../logic/frames';
import type { EventCols } from '../worker/protocol';
import { usePlayback } from './playback';
import { useSession, useSessionB } from './session';
import { useView } from './view';

export interface Display {
  kinds: Uint8Array;
  pulse: Uint8Array;
  /** 0 = なし・フォロー、1 = 推薦、2 = 推薦由来の連鎖（stateAtWave）。 */
  via: Uint8Array;
  events: EventCols;
  k: number;
}

/** `slot`: 比較（F13）の A = 0 / B = 1。再生位置は共有し、各スロットは自分の最後のウェーブで止まる。 */
export function useDisplay(slot: 0 | 1 = 0): Display | null {
  const store = slot === 1 ? useSessionB : useSession;
  const rounds = store((s) => s.rounds);
  const ip = store((s) => s.ip);
  const positions = store((s) => s.positions);
  const intMode = useView((s) => s.intMode);
  const recMode = useView((s) => s.recMode);
  const k = usePlayback((s) => s.k);
  return useMemo(() => {
    if (!positions || ip === null) return null;
    const n = positions.xy.length / 2;
    const cur = rounds.length > 0 ? rounds[rounds.length - 1] : null;
    const before = cur ? cur.everBefore : new Uint8Array(n);
    const ev = cur ? cur.events : emptyEvents();
    const last = cur ? cur.summary.n_waves - 1 : -1;
    const kk = Math.min(k, last);
    return { ...stateAtWave(before, ev, kk, intMode, ip, recMode === 0), events: ev, k: kk };
  }, [rounds, ip, positions, intMode, recMode, k]);
}
