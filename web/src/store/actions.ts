// 複数のスロット（比較の A・B。F13）にまたがる操作。比較がオフのときは A だけに作用する。
// B は A と同じグラフ・同じ IP・同じ選択を受け取り、機構などの設定だけが違う（同じシードなので決定論的に比べられる）。
import { create } from 'zustand';
import type { GraphOpts } from '../worker/protocol';
import { usePlayback } from './playback';
import { useSession, useSessionB } from './session';
import { useView } from './view';

interface CompareState {
  enabled: boolean;
  setEnabled(v: boolean): void;
}

export const useCompare = create<CompareState>((set) => ({ enabled: false, setEnabled: (enabled) => set({ enabled }) }));

type Store = typeof useSession;

export function activeStores(): Store[] {
  return useCompare.getState().enabled ? [useSession, useSessionB] : [useSession];
}

/** 最後のウェーブ番号に合わせて再生を準備する（A・B の長い方。短い方は終了後の状態のまま）。 */
function syncPlaybackToEnd() {
  const last = Math.max(...activeStores().map((s) => {
    const r = s.getState().rounds;
    return r.length ? r[r.length - 1].summary.n_waves - 1 : -1;
  }));
  usePlayback.getState().setLast(last);
}

export async function loadGraphAll(bytes: Uint8Array, name: string, opts?: Partial<GraphOpts>) {
  await useSession.getState().loadGraph(bytes, name, opts);
  if (useCompare.getState().enabled) await useSessionB.getState().loadGraph(bytes, name, { ...useSession.getState().graphOpts });
  usePlayback.getState().setLast(-1);
}

export async function reloadGraphAll() {
  const a = useSession.getState();
  if (!a.graphBytes) return;
  await loadGraphAll(a.graphBytes, a.graphOpts.name, a.graphOpts);
}

export async function applyAll() {
  for (const s of activeStores()) await s.getState().applyForm();
  usePlayback.getState().setLast(-1);
}

export async function chooseIpAll(node: number) {
  for (const s of activeStores()) await s.getState().chooseIp(node);
  usePlayback.getState().setLast(-1);
}

/**
 * 選んだメッセージで 1 ラウンド発信し、再生を始める。比較中は A・B が必ず同じ選択を受け取る:
 * どちらかが発信できない（実行ラウンド数の上限など）ときは、先に進んだ側も取り消して揃える。
 */
export async function stepAll(choice: number) {
  if (usePlayback.getState().playing) return;
  const stores = activeStores();
  const before = stores.map((s) => s.getState().rounds.length);
  let nWaves = 0;
  for (let i = 0; i < stores.length; i++) {
    const rec = await stores[i].getState().step(choice);
    if (!rec) {
      for (let j = 0; j < i; j++) await stores[j].getState().rewind(before[j]);
      if (i > 0) useSessionB.setState({ error: useSessionB.getState().error ?? 'B では発信できません。A への発信も取り消しました' });
      syncPlaybackToEnd();
      return;
    }
    nWaves = Math.max(nWaves, rec.summary.n_waves);
  }
  usePlayback.getState().start(nWaves);
}

export async function restartAll() {
  for (const s of activeStores()) await s.getState().restart();
  usePlayback.getState().setLast(-1);
}

export async function rewindAll(round: number) {
  for (const s of activeStores()) await s.getState().rewind(round);
  syncPlaybackToEnd();
}

export async function switchAll(id: number) {
  for (const s of activeStores()) await s.getState().switchTo(id);
  syncPlaybackToEnd();
}

export async function runMcAll(trials: number) {
  await Promise.all(activeStores().map((s) => s.getState().runMc(trials)));
}

/**
 * 比較モードのオン・オフ。オンにすると、B に A と同じグラフを読み込み、A の設定（機構など）を複製して、
 * 同じ IP・同じ現在の枝の選択を再現する（B の設定は、そのあと画面で変える）。
 */
export async function setCompare(on: boolean) {
  useCompare.getState().setEnabled(on);
  if (!on) return;
  const a = useSession.getState();
  const b = useSessionB.getState();
  if (!a.graphBytes) return;
  b.setForm(JSON.parse(JSON.stringify(a.form)));
  b.setGraphOpts({ ...a.graphOpts });
  await b.loadGraph(a.graphBytes, a.graphOpts.name, { ...a.graphOpts });
  if (a.ip !== null) {
    await useSessionB.getState().chooseIp(a.ip);
    for (const c of a.choices) await useSessionB.getState().step(c);
  }
  useView.getState().resetView(useView.getState().mode);
  syncPlaybackToEnd();
}

/** B の設定を適用する: B のシナリオを作り直し、A と同じ IP・同じ現在の枝の選択を再現する。 */
export async function applyB() {
  const a = useSession.getState();
  await useSessionB.getState().applyForm();
  if (useSessionB.getState().error) return;
  if (a.ip !== null) {
    await useSessionB.getState().chooseIp(a.ip);
    for (let i = 0; i < a.choices.length; i++) {
      const rec = await useSessionB.getState().step(a.choices[i]);
      if (!rec) {
        const why = useSessionB.getState().error ?? '';
        useSessionB.setState({ error: `B は A の ${i + 1} ラウンド目の選択を再現できません（${why}）。B の実行ラウンド数の上限などを確認してください` });
        break;
      }
    }
  }
  syncPlaybackToEnd();
}
