// spec/ui-tokens.json（Web・デスクトップ共通の色・形・寸法。11.1）を読む。色や寸法はここ以外にハードコードしない。
import tokens from '../../spec/ui-tokens.json';

export type Tokens = typeof tokens;
export const T: Tokens = tokens;

/** `#RRGGBB` → [r, g, b]（0〜1）。 */
export function hexToRgb(hex: string): [number, number, number] {
  const n = parseInt(hex.slice(1), 16);
  return [((n >> 16) & 255) / 255, ((n >> 8) & 255) / 255, (n & 255) / 255];
}

export const KIND_LABELS = ['未受信', '受信のみ', '内部活性', '外部活性', '両方活性', 'IP'] as const;

/**
 * UI の枠の色（`chrome`）を CSS 変数として `:root` に流し込む。CSS には色を直書きしない（色の出どころは ui-tokens.json だけ）。
 * 配色はダーク専用で、OS のライト／ダークの設定には従わない。
 */
export function applyChromeVars(): void {
  const c = T.chrome;
  const vars: Record<string, string> = {
    '--app': c.app, '--card': c.card, '--ui': c.ui, '--hover': c.hover,
    '--line': c.border, '--line-strong': c.borderStrong,
    '--muted': c.muted, '--fg': c.text, '--grid': c.grid,
    '--accent': c.accent, '--accent-soft': c.accentSoft, '--on-accent': c.onAccent,
    '--net': T.background,
    '--r-card': `${c.radiusCard}px`, '--r-ctl': `${c.radiusControl}px`,
  };
  const r = document.documentElement.style;
  for (const [k, v] of Object.entries(vars)) r.setProperty(k, v);
}
