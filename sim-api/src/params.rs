//! 画面がスライダーを自動生成するための登録簿（12.3、N07）。
//! 新しい機構は `param_specs()` に 1 行足し、`get_value` に対応を足す。

use serde::Serialize;

use crate::config::{MechanismConfig, RecMethod, RecommenderConfig};

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

/// 推薦のスライダーの登録簿（`recommender-design.md` 5 章）。`mechanism` は画面の見出し。
pub fn rec_param_specs() -> &'static [ParamSpec] {
    const SPECS: &[ParamSpec] = &[
        f(
            "rec.threshold",
            "推薦",
            "閾値 θ（これ以上のスコアの人に推薦）",
            0.0,
            1.0,
            0.01,
            0.15,
            0.0,
        ),
        f(
            "rec.w_share",
            "推薦",
            "シェアの重み（外部行動は 1 − この値）",
            0.0,
            1.0,
            0.05,
            0.5,
            0.5,
        ),
        f(
            "rec.prior_n0",
            "推薦",
            "平滑化の強さ λ（記録の少ない人を全体の率に寄せる）",
            0.0,
            100.0,
            0.5,
            2.0,
            2.0,
        ),
        f(
            "rec.alpha",
            "推薦",
            "CF の比率 α（1 で協調フィルタリングだけ、0 でグラフだけ）",
            0.0,
            1.0,
            0.05,
            0.5,
            0.5,
        ),
        ParamSpec {
            id: "rec.min_common",
            mechanism: "推薦",
            label: "似かたの最小人数 c（両方の種類を受け取った人がこれ未満なら似かた 0）",
            kind: ParamKind::U32,
            min: Some(1.0),
            max: Some(1000.0),
            step: Some(1.0),
            default: 5.0,
            off: 5.0,
        },
        // Rモデル（`recommender-v3-design.md` 5 章）
        f(
            "rec.eps",
            "推薦",
            "推薦の強さ ε（0 で推薦なし）",
            0.0,
            5.0,
            0.1,
            1.0,
            0.0,
        ),
        f(
            "rec.delta",
            "推薦",
            "フォロー外の露出の割引 δ（1 で割引なし）",
            0.0,
            1.0,
            0.05,
            1.0,
            1.0,
        ),
        f(
            "rec.near",
            "推薦",
            "近くへの推薦 θ（2 歩先へ配る確率）",
            0.0,
            1.0,
            0.05,
            0.5,
            0.5,
        ),
        f(
            "rec.tau",
            "推薦",
            "しきい値 τ（反応率が基準の何倍で推薦するか）",
            0.0,
            3.0,
            0.1,
            0.0,
            0.0,
        ),
        f(
            "rec.phi_max",
            "推薦",
            "倍率の上限 φ_max",
            1.0,
            10.0,
            0.5,
            3.0,
            3.0,
        ),
        f(
            "rec.k",
            "推薦",
            "擬似カウント k（何人分様子を見るか）",
            1.0,
            100.0,
            1.0,
            10.0,
            10.0,
        ),
        f(
            "rec.mu",
            "推薦",
            "アカウント記憶 μ（1 で記憶なし）",
            0.0,
            1.0,
            0.05,
            1.0,
            1.0,
        ),
    ];
    SPECS
}

/// 方式ごとに使う推薦の項目（画面の有効・無効。`recommender-v3-design.md` 5 章）。
pub fn rec_param_used(method: RecMethod, id: &str) -> bool {
    match id {
        "rec.threshold" | "rec.w_share" | "rec.prior_n0" => method != RecMethod::Wave,
        "rec.alpha" | "rec.min_common" => method == RecMethod::Hybrid,
        _ => method == RecMethod::Wave,
    }
}

/// 推薦の設定の現在値（範囲検査用）。
pub fn get_rec_value(c: &RecommenderConfig, id: &str) -> Option<f64> {
    Some(match id {
        "rec.threshold" => c.threshold,
        "rec.w_share" => c.w_share,
        "rec.prior_n0" => c.prior_n0,
        "rec.alpha" => c.alpha,
        "rec.min_common" => c.min_common as f64,
        "rec.eps" => c.eps,
        "rec.delta" => c.delta,
        "rec.near" => c.near,
        "rec.tau" => c.tau,
        "rec.phi_max" => c.phi_max,
        "rec.k" => c.k,
        "rec.mu" => c.mu,
        _ => return None,
    })
}
