// 画像・動画の書き出し（F15）。ネットワーク図は 3 層（背景・WebGL・重ね描き）のキャンバスを 1 枚に合成して使う。
// 比較モードでは A・B を左右に並べて 1 枚にする。外部サーバには何も送らない（N06）。
import { usePlayback } from '../store/playback';

export interface Layers {
  base: HTMLCanvasElement | null;
  gl: HTMLCanvasElement | null;
  over: HTMLCanvasElement | null;
}

const registry = new Map<number, Layers>();

export function registerLayers(slot: number, layers: Layers): () => void {
  registry.set(slot, layers);
  return () => {
    if (registry.get(slot) === layers) registry.delete(slot);
  };
}

/** 登録されているキャンバスを、左から（A, B の順に）1 枚に合成する。描いたら true。 */
export function compositeInto(dst: HTMLCanvasElement, bg: string): boolean {
  const slots = [...registry.entries()].sort((a, b) => a[0] - b[0]).map(([, l]) => l).filter((l) => l.base && l.gl && l.over);
  if (slots.length === 0) return false;
  const w = slots.reduce((sum, l) => sum + l.base!.width, 0);
  const h = Math.max(...slots.map((l) => l.base!.height));
  if (dst.width !== w || dst.height !== h) {
    dst.width = w;
    dst.height = h;
  }
  const ctx = dst.getContext('2d');
  if (!ctx) return false;
  ctx.fillStyle = bg;
  ctx.fillRect(0, 0, w, h);
  let x = 0;
  for (const l of slots) {
    for (const c of [l.base!, l.gl!, l.over!]) ctx.drawImage(c, x, 0);
    x += l.base!.width;
  }
  return true;
}

export const toBlob = (c: HTMLCanvasElement, type = 'image/png') =>
  new Promise<Blob>((res, rej) => c.toBlob((b) => (b ? res(b) : rej(new Error('画像を作れません'))), type));

export async function networkPng(bg: string): Promise<Blob> {
  const c = document.createElement('canvas');
  if (!compositeInto(c, bg)) throw new Error('ネットワーク表示がありません（IP を選んでください）');
  return toBlob(c);
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

export function recordingSupported(): boolean {
  return typeof MediaRecorder !== 'undefined' && typeof HTMLCanvasElement !== 'undefined' && 'captureStream' in HTMLCanvasElement.prototype;
}

/**
 * 現在のラウンドの再生を、ウェーブ 0 の前から最後まで流して WebM に録画する。再生の速度は画面の設定に従う。
 * 録画中は、合成用キャンバスに毎フレーム描き直して `captureStream` へ渡す。
 */
export async function recordPlayback(bg: string): Promise<Blob> {
  if (!recordingSupported()) throw new Error('このブラウザは録画（MediaRecorder）に対応していません。PNG の書き出しを使ってください');
  const pb = usePlayback.getState();
  if (pb.last < 0) throw new Error('録画するラウンドがありません。先に発信してください');
  const comp = document.createElement('canvas');
  if (!compositeInto(comp, bg)) throw new Error('ネットワーク表示がありません（IP を選んでください）');
  const mime = ['video/webm;codecs=vp9', 'video/webm;codecs=vp8', 'video/webm'].find((m) => MediaRecorder.isTypeSupported(m));
  const rec = new MediaRecorder(comp.captureStream(30), { mimeType: mime, videoBitsPerSecond: 4_000_000 });
  const chunks: Blob[] = [];
  rec.ondataavailable = (e) => {
    if (e.data.size) chunks.push(e.data);
  };
  let running = true;
  const draw = () => {
    if (!running) return;
    compositeInto(comp, bg);
    requestAnimationFrame(draw);
  };
  rec.start(200);
  draw();
  await sleep(300); // 最初のフレーム（ラウンド開始前）を写す
  usePlayback.getState().start(pb.last + 1);
  while (usePlayback.getState().playing) await sleep(50);
  await sleep(900); // 最後の状態を少し見せる
  running = false;
  const stopped = new Promise<void>((r) => {
    rec.onstop = () => r();
  });
  rec.stop();
  await stopped;
  return new Blob(chunks, { type: 'video/webm' });
}
