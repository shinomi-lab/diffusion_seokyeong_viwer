// 小さな共通の定数・関数。
/** 個別展開できる上限（N03。sim-api の EXPAND_CAP と同じ）。 */
export const EXPAND_CAP = 200_000;

/** 描くノード数に応じた半径（px）。多いほど点が重なるので縮める（デスクトップの lod.rs と同じ式）。 */
export function effectiveRadius(base: number, nDraw: number): number {
  const FULL = 3_000;
  if (nDraw <= FULL) return base;
  return Math.min(base, Math.max(1.2, base * Math.sqrt(FULL / nDraw)));
}
export const N_SECTORS = 72;
/** 辺の描画の上限（9.4）。 */
export const EDGE_CAP = 50_000;

/** メッセージ番号 → 表示名（m0〜）。 */
export function msgName(choice: number): string {
  return `m${choice}`;
}

/** 系列文字列（左端がラウンド 1）。 */
export function sequenceString(choices: number[]): string {
  return choices.map((c) => c.toString(16)).join('');
}

export function parseSequence(s: string): number[] {
  return [...s].map((ch) => {
    const v = parseInt(ch, 16);
    if (Number.isNaN(v)) throw new Error(`系列文字列の文字 '${ch}' が不正です`);
    return v;
  });
}

/** Blob を保存する（外部サーバには何も送らない。N06）。 */
export function downloadText(name: string, text: string, mime = 'text/plain'): void {
  const url = URL.createObjectURL(new Blob([text], { type: `${mime};charset=utf-8` }));
  const a = document.createElement('a');
  a.href = url;
  a.download = name;
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

/** Blob（画像・動画）を保存する。 */
export function downloadBlob(name: string, blob: Blob): void {
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = name;
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 2000);
}
