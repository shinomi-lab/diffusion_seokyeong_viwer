// セッション操作に再生位置の調整を添えた窓口。
import type { GraphOpts } from '../worker/protocol';
import { usePlayback } from './playback';
import { useSession } from './session';

/** 最後のウェーブ番号に合わせて再生を準備する。 */
function syncPlaybackToEnd() {
  const r = useSession.getState().rounds;
  usePlayback.getState().setLast(r.length ? r[r.length - 1].summary.n_waves - 1 : -1);
}

export async function loadGraphAll(bytes: Uint8Array, name: string, opts?: Partial<GraphOpts>) {
  await useSession.getState().loadGraph(bytes, name, opts);
  usePlayback.getState().setLast(-1);
}

export async function reloadGraphAll() {
  const a = useSession.getState();
  if (!a.graphBytes) return;
  await loadGraphAll(a.graphBytes, a.graphOpts.name, a.graphOpts);
}

export async function applyAll() {
  await useSession.getState().applyForm();
  usePlayback.getState().setLast(-1);
}

export async function chooseIpAll(node: number) {
  await useSession.getState().chooseIp(node);
  usePlayback.getState().setLast(-1);
}

/** 選んだメッセージで 1 ラウンド発信し、再生を始める。 */
export async function stepAll(choice: number) {
  if (usePlayback.getState().playing) return;
  const rec = await useSession.getState().step(choice);
  if (!rec) {
    syncPlaybackToEnd();
    return;
  }
  usePlayback.getState().start(rec.summary.n_waves);
}

export async function restartAll() {
  await useSession.getState().restart();
  usePlayback.getState().setLast(-1);
}

export async function rewindAll(round: number) {
  await useSession.getState().rewind(round);
  syncPlaybackToEnd();
}

export async function switchAll(id: number) {
  await useSession.getState().switchTo(id);
  syncPlaybackToEnd();
}

export async function runMcAll(trials: number) {
  await useSession.getState().runMc(trials);
}
