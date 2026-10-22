// ノードの分類（7.5 と同じ規則）。番号は NodeKind と同じ。
export const KIND = { Unreceived: 0, ReceivedOnly: 1, Internal: 2, External: 3, Both: 4, Ip: 5 } as const;

export const EVER_RECV = 1;
export const EVER_INT = 2;
export const EVER_EXT = 4;

/** イベントの flags のビット。 */
export const EV_INT = 1;
export const EV_EXT = 2;
export const EV_ENQ = 4;

/** `ever` は EVER_* のビット。外部は常に累積。内部は `intNow`（このラウンドで成立）か累積。 */
export function kindOf(ever: number, intNow: boolean, isIp: boolean, intMode: 0 | 1): number {
  if (isIp) return KIND.Ip;
  if ((ever & EVER_RECV) === 0) return KIND.Unreceived;
  const int = intMode === 0 ? (ever & EVER_INT) !== 0 : intNow;
  const ext = (ever & EVER_EXT) !== 0;
  if (int && ext) return KIND.Both;
  if (int) return KIND.Internal;
  if (ext) return KIND.External;
  return KIND.ReceivedOnly;
}
