//! 画面がスライダーを自動生成するための登録簿（12.3、N07）。
//! 新しい機構は `param_specs()` に 1 行足し、`get_value` に対応を足す。

use serde::Serialize;

use crate::config::MechanismConfig;

#[derive(Clone, Copy, Debug, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ParamKind {
    F64,
    U32,
    Bool,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct ParamSpec {
    pub id: &'static str,
    pub mechanism: &'static str,
    pub label: &'static str,
    pub kind: ParamKind,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub step: Option<f64>,
    pub default: f64,
    /// 無効値（bool は 0/1）。
    pub off: f64,
}

#[allow(clippy::too_many_arguments)]
const fn f(
    id: &'static str,
    mechanism: &'static str,
    label: &'static str,
    min: f64,
    max: f64,
    step: f64,
    default: f64,
    off: f64,
) -> ParamSpec {
    ParamSpec {
        id,
        mechanism,
        label,
        kind: ParamKind::F64,
        min: Some(min),
        max: Some(max),
        step: Some(step),
        default,
        off,
    }
}

pub fn param_specs() -> &'static [ParamSpec] {
    const SPECS: &[ParamSpec] = &[
        f(
            "m1.kappa",
            "M1 購入者の口コミ",
            "κ（外部行動時に内部効用へ加える値）",
            0.0,
            0.5,
            0.01,
            0.05,
            0.0,
        ),
        // M5 複合感染（検討中のためコメントアウト。2026-10-20）
        // f("m5.eta", "M5 複合感染", "η（経路 1 本ごとの強化）", 0.0, 1.0, 0.05, 0.25, 0.0),
        // ParamSpec { id: "m5.n_max", mechanism: "M5 複合感染", label: "数える経路の上限", kind: ParamKind::U32,
        //     min: Some(1.0), max: Some(8.0), step: Some(1.0), default: 3.0, off: 1.0 },
        // M2・M3・M4 は 2026-10-20 に廃止。
    ];
    SPECS
}

/// 数値の現在値（範囲検査用）。bool は None。
pub fn get_value(c: &MechanismConfig, id: &str) -> Option<f64> {
    Some(match id {
        "m1.kappa" => c.m1.kappa,
        // "m5.eta" => c.m5.eta,
        // "m5.n_max" => c.m5.n_max as f64,
        _ => return None,
    })
}
