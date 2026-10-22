// シナリオ設定のフォーム状態と、機構のプリセット（12 章）。キー名は sim-api の ScenarioConfig と同じ。
import type { ParamSpec } from '../worker/protocol';

// M2〜M4 は 2026-10-20 に廃止。M5 は検討中のためコメントアウト（戻すときは 'm5' を足す）。
export type MechId = 'm1'; // | 'm5'
export const MECH_IDS: MechId[] = ['m1']; // , 'm5'

export type MechValues = { enabled: boolean } & Record<string, number | boolean>;
export type Mechanisms = Record<MechId, MechValues>;

export interface DecisionForm {
  beta: number;
  gamma: number;
  initial_utility: number;
}

export interface ScenarioForm {
  receipt_probability: number;
  calibration_rounds: number;
  max_rounds: number;
  user_seed: number;
  user_index: number;
  diffusion_seed: number;
  internal_parameters: DecisionForm;
  external_parameters: DecisionForm;
  message_space: { internal_stimulus: number; external_stimulus: number }[];
  mechanisms: Mechanisms;
}

// 試作の既定（要件書 6.2・12.1）。Rust 側の ScenarioConfig::default と一致する（logic/config.test.ts で確認）。
export const DEFAULT_MESSAGES = [
  { internal_stimulus: 0.2, external_stimulus: 0.2 },
  { internal_stimulus: 0.2, external_stimulus: 0.05 },
  { internal_stimulus: 0.15, external_stimulus: 0.1 },
  { internal_stimulus: 0.1, external_stimulus: 0.1 },
  { internal_stimulus: 0.1, external_stimulus: 0.15 },
  { internal_stimulus: 0.05, external_stimulus: 0.2 },
];

/** 機構の既定値（要件書 6.6）。ParamSpec の default と一致させる。 */
export function mechanismsFrom(specs: ParamSpec[], enabled: boolean, scale = 1): Mechanisms {
  const out = {} as Mechanisms;
  for (const m of MECH_IDS) out[m] = { enabled };
  for (const s of specs) {
    const [m, key] = s.id.split('.') as [MechId, string];
    let v = s.kind === 'bool' ? s.default === 1 : s.default;
    if (typeof v === 'number' && scale !== 1) {
      // 「既定の2倍」: κ を k 倍（M5 を戻すときは η も k 倍。n_max は変えない）
      if (['m1.kappa' /* , 'm5.eta' */].includes(s.id)) v = v * scale;
    }
    out[m][key] = v;
  }
  return out;
}

export type PresetName = 'off' | 'balanced' | 'double';
export const PRESET_LABELS: Record<PresetName, string> = { off: '現行（すべて無効）', balanced: '既定', double: '既定の2倍' };

export function presetMechanisms(name: PresetName, specs: ParamSpec[]): Mechanisms {
  if (name === 'off') return mechanismsFrom(specs, false);
  return mechanismsFrom(specs, true, name === 'double' ? 2 : 1);
}

export function defaultForm(specs: ParamSpec[]): ScenarioForm {
  return {
    receipt_probability: 0.5,
    calibration_rounds: 5,
    max_rounds: 50,
    user_seed: 0,
    user_index: 0,
    diffusion_seed: 0,
    internal_parameters: { beta: 0.8, gamma: 0.6, initial_utility: -1 },
    external_parameters: { beta: 0.8, gamma: 0.6, initial_utility: -1 },
    message_space: DEFAULT_MESSAGES.map((m) => ({ ...m })),
    mechanisms: presetMechanisms('off', specs),
  };
}

export function toConfigJson(f: ScenarioForm): string {
  return JSON.stringify({
    receipt_probability: f.receipt_probability,
    calibration_rounds: f.calibration_rounds,
    max_rounds: f.max_rounds,
    user_seed: f.user_seed,
    user_index: f.user_index,
    diffusion_seed: f.diffusion_seed,
    internal_parameters: f.internal_parameters,
    external_parameters: f.external_parameters,
    message_space: f.message_space,
    mean_stimulus: { mode: 'auto' },
    mechanisms: f.mechanisms,
  });
}

/** 保存した設定（シナリオ記録 JSON の config など）をフォームに戻す。足りないキーは現在の値のまま。 */
export function formFromConfig(base: ScenarioForm, cfg: Record<string, unknown>): ScenarioForm {
  const c = cfg as Partial<ScenarioForm>;
  const mech = { ...base.mechanisms };
  const cm = (c.mechanisms ?? {}) as Partial<Mechanisms>;
  for (const m of MECH_IDS) if (cm[m]) mech[m] = { ...mech[m], ...cm[m] } as MechValues;
  return { ...base, ...c, mechanisms: mech } as ScenarioForm;
}
