//! シナリオ設定の型と検証（7.2・12 章）。

use serde::{Deserialize, Serialize};
use sim_core::message::{Message, MessageSpace, MessageSpaceError};

/// 設定用のメッセージ。`Message` は `Serialize` を持たない（core は改変しない）ので写しを持つ。
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct MessageSpec {
    pub internal_stimulus: f64,
    pub external_stimulus: f64,
}

impl From<MessageSpec> for Message {
    fn from(m: MessageSpec) -> Self {
        Message {
            internal_stimulus: m.internal_stimulus,
            external_stimulus: m.external_stimulus,
        }
    }
}

use crate::params;

/// JS で扱える整数の上限（シード用）。
pub const MAX_SEED: u64 = 1 << 53;
/// 状態ログの確保上限（バイト）。
pub const LOG_LIMIT_BYTES: u64 = 1 << 30;
/// 1 ノード 1 ラウンドあたりのログのバイト数（f32×2 + フラグ 1）。
pub const LOG_BYTES_PER_NODE_ROUND: u64 = 9;

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ConfigError {
    #[error("{0}")]
    Message(String),
    #[error("message_space が不正です: {0}")]
    MessageSpace(MessageSpaceError),
    #[error(
        "状態ログが大きすぎます（{bytes} バイト > 1 GiB）。ノード数か max_rounds を減らしてください"
    )]
    LogTooLarge { bytes: u64 },
}

/// NaN も弾くため、`!(x > 0.0)` ではなく専用の関数にする。
fn is_pos(x: f64) -> bool {
    x > 0.0
}

fn err<T>(s: impl Into<String>) -> Result<T, ConfigError> {
    Err(ConfigError::Message(s.into()))
}

/// runner と同じキー名。
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DecisionParams {
    pub initial_utility: f64,
    pub beta: f64,
    pub gamma: f64,
}

impl Default for DecisionParams {
    fn default() -> Self {
        Self {
            initial_utility: -1.0,
            beta: 0.8,
            gamma: 0.6,
        }
    }
}

impl DecisionParams {
    /// runner の `DecisionParameters::validate` と同じ規則に、0 除算の検査を足したもの。
    pub fn validate(&self, label: &str, mean_stimulus: f64) -> Result<(), ConfigError> {
        if !(-1.0..=1.0).contains(&self.initial_utility) {
            return err(format!(
                "{label}.initial_utility は [-1, 1] である必要があります (実際: {})",
                self.initial_utility
            ));
        }
        if !is_pos(self.beta) {
            return err(format!(
                "{label}.beta は正である必要があります (実際: {})",
                self.beta
            ));
        }
        if !is_pos(self.gamma) {
            return err(format!(
                "{label}.gamma は正である必要があります (実際: {})",
                self.gamma
            ));
        }
        if self.gamma >= self.beta {
            return err(format!(
                "{label}: gamma < beta である必要があります (実際: gamma = {}, beta = {})",
                self.gamma, self.beta
            ));
        }
        if mean_stimulus - self.initial_utility == 0.0 {
            return err(format!(
                "{label}: 平均刺激と initial_utility が等しいため重みを逆算できません"
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "mode", rename_all = "lowercase")]
pub enum MeanStimulus {
    Auto,
    Manual { internal: f64, external: f64 },
}

impl Default for MeanStimulus {
    fn default() -> Self {
        Self::Auto
    }
}

macro_rules! mech_struct {
    ($name:ident { $($f:ident : $t:ty = $d:expr),* }) => {
        #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
        #[serde(default)]
        pub struct $name { pub enabled: bool, $(pub $f: $t),* }
        impl Default for $name {
            fn default() -> Self { Self { enabled: false, $($f: $d),* } }
        }
    };
}

mech_struct!(M1 { kappa: f64 = 0.05 });
// M5 複合感染（検討中のためコメントアウト。2026-10-20）
// mech_struct!(M5 {
//     eta: f64 = 0.25,
//     n_max: u32 = 3
// });
// M2（摩耗）・M3（S 字反応）・M4（休止・忘却・投稿予算・m∅）は 2026-10-20 に廃止。
// 古い記録（JSON）に `m2`〜`m5` のキーが残っていても、知らないキーとして無視して読める。

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct MechanismConfig {
    pub m1: M1,
    // pub m5: M5,
}

/// 実効値。無効の機構は無効値になる。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mech {
    pub kappa: f64,
    // M5（検討中）
    // pub eta: f64,
    // pub n_max: u8,
}

impl Mech {
    /// すべて無効（現行モデル）。
    pub fn off() -> Self {
        Self { kappa: 0.0 }
    }
}

impl MechanismConfig {
    /// 現行（すべて無効）。
    pub fn off() -> Self {
        Self::default()
    }

    /// 既定の強さで有効（要件書 6.6）。
    pub fn balanced() -> Self {
        let mut c = Self::default();
        c.m1.enabled = true;
        // c.m5.enabled = true;
        c
    }

    /// 既定の `k` 倍（κ を k 倍）。
    pub fn scaled(k: f64) -> Self {
        let mut c = Self::balanced();
        c.m1.kappa *= k;
        // c.m5.eta *= k;
        c
    }

    /// 実効値を作る。
    pub fn effective(&self) -> Mech {
        let mut m = Mech::off();
        if self.m1.enabled {
            m.kappa = self.m1.kappa;
        }
        // if self.m5.enabled {
        //     m.eta = self.m5.eta;
        //     m.n_max = self.m5.n_max.clamp(1, 255) as u8;
        // }
        m
    }

    pub fn any_enabled(&self) -> bool {
        self.m1.enabled
    }

    fn validate(&self) -> Result<(), ConfigError> {
        for &spec in params::param_specs() {
            let range = spec.min.zip(spec.max);
            let value = params::get_value(self, spec.id);
            if let (Some((lo, hi)), Some(v)) = (range, value)
                && !(lo..=hi).contains(&v)
            {
                return err(format!(
                    "{} は {lo}〜{hi} の範囲である必要があります (実際: {v})",
                    spec.id
                ));
            }
        }
        Ok(())
    }
}

/// 推薦（`recommender-design.md` 5 章）。拡張機構とは別の設定。既定は off（現行モデル）。
/// 推薦の方式（`recommender-v2-design.md` 5 章）。
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RecMethod {
    /// 基準（v1）: 本人の同じ種類の反応率を全体の率に寄せる。
    Baseline,
    /// CF＋グラフ（v2）: アイテムベースの協調フィルタリングと社会的証明の重み付きの和。
    Hybrid,
    /// Rモデル（v3、`recommender-v3-design.md`）: 各ウェーブのあとに、反応率に応じた数だけフォロー外へ押し出す。
    Wave,
}

impl RecMethod {
    /// キーが無い古い記録（v1 で保存）は基準として読む。
    // 仮定: 古い記録の再現性を優先し、キーが無いときと既定値（Hybrid）を分ける（D-V4）。
    fn legacy() -> Self {
        Self::Baseline
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct RecommenderConfig {
    pub enabled: bool,
    #[serde(default = "RecMethod::legacy")]
    pub method: RecMethod,
    /// 閾値 θ。スコアがこれ以上の人に推薦する。
    // 仮定: U-R1。既定 0.15 は仮置き（3 水準の比較で決める）。
    pub threshold: f64,
    /// シェアの重み w_s。外部行動の重みは 1 − w_s。
    pub w_share: f64,
    /// 平滑化の強さ（基準では n₀、CF＋グラフでは λ）。
    pub prior_n0: f64,
    /// CF の比率 α（1 で CF だけ、0 でグラフだけ）。
    pub alpha: f64,
    /// 種類どうしの似かたを計算する最小の共通受信者数 c。
    pub min_common: u32,
    /// Rモデル: 推薦の強さ ε（0 で推薦なし）。
    pub eps: f64,
    /// Rモデル: フォロー外の露出の割引 δ（1 で割引なし）。
    pub delta: f64,
    /// Rモデル: 近くへの推薦 θ（2 歩先へ配る確率）。v1 の閾値 θ（`threshold`）とは別物。
    pub near: f64,
    /// Rモデル: しきい値 τ（反応率が基準の何倍で推薦するか）。
    pub tau: f64,
    /// Rモデル: 推薦量の倍率の上限 φ_max。
    pub phi_max: f64,
    /// Rモデル: 擬似カウント k（事前分布の強さ）。
    pub k: f64,
    /// Rモデル: アカウント記憶の持続 μ（R3。1 で記憶なし）。
    pub mu: f64,
}

impl Default for RecommenderConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            method: RecMethod::Hybrid,
            threshold: 0.15,
            w_share: 0.5,
            prior_n0: 2.0,
            alpha: 0.5,
            min_common: 5,
            // 仮定: U-W1。Rモデルの既定値は仮置き（参照実装 diffusion_seokyeong と同じ）。
            eps: 1.0,
            delta: 1.0,
            near: 0.5,
            tau: 0.0,
            phi_max: 3.0,
            k: 10.0,
            mu: 1.0,
        }
    }
}

impl RecommenderConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        for &spec in params::rec_param_specs() {
            let v = params::get_rec_value(self, spec.id).unwrap_or(0.0);
            if let (Some(lo), Some(hi)) = (spec.min, spec.max)
                && !(lo..=hi).contains(&v)
            {
                return err(format!(
                    "{} は {lo}〜{hi} の範囲である必要があります (実際: {v})",
                    spec.id
                ));
            }
        }
        if self.phi_max < self.tau {
            return err(format!(
                "rec.phi_max は rec.tau 以上である必要があります (phi_max = {}, tau = {})",
                self.phi_max, self.tau
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ScenarioConfig {
    pub message_space: Vec<MessageSpec>,
    pub receipt_probability: f64,
    /// 反応性の scale = 1/(β·calibration_rounds)。実行 T とは別（INV-12）。
    pub calibration_rounds: u32,
    pub max_rounds: u32,
    pub internal_parameters: DecisionParams,
    pub external_parameters: DecisionParams,
    pub mean_stimulus: MeanStimulus,
    pub user_seed: u64,
    pub user_index: u32,
    pub diffusion_seed: u64,
    pub mechanisms: MechanismConfig,
    /// 推薦（古い記録にはキーが無く、off として読める）。
    pub recommender: RecommenderConfig,
}

fn default_messages() -> Vec<MessageSpec> {
    let m = |i, e| MessageSpec {
        internal_stimulus: i,
        external_stimulus: e,
    };
    vec![
        m(0.20, 0.20),
        m(0.20, 0.05),
        m(0.15, 0.10),
        m(0.10, 0.10),
        m(0.10, 0.15),
        m(0.05, 0.20),
    ]
}

impl Default for ScenarioConfig {
    fn default() -> Self {
        Self {
            message_space: default_messages(),
            receipt_probability: 0.5,
            calibration_rounds: 5,
            max_rounds: 50,
            internal_parameters: DecisionParams::default(),
            external_parameters: DecisionParams::default(),
            mean_stimulus: MeanStimulus::Auto,
            user_seed: 0,
            user_index: 0,
            diffusion_seed: 0,
            mechanisms: MechanismConfig::off(),
            recommender: RecommenderConfig::default(),
        }
    }
}

impl ScenarioConfig {
    pub fn from_toml(s: &str) -> Result<Self, ConfigError> {
        toml::from_str(s)
            .map_err(|e| ConfigError::Message(format!("設定の読み込みに失敗しました: {e}")))
    }

    pub fn from_json(s: &str) -> Result<Self, ConfigError> {
        serde_json::from_str(s)
            .map_err(|e| ConfigError::Message(format!("設定の読み込みに失敗しました: {e}")))
    }

    pub fn mech(&self) -> Mech {
        self.mechanisms.effective()
    }

    pub fn messages(&self) -> Vec<Message> {
        self.message_space.iter().map(|&m| m.into()).collect()
    }

    pub fn message_space(&self) -> Result<MessageSpace, ConfigError> {
        MessageSpace::try_from(self.messages()).map_err(ConfigError::MessageSpace)
    }

    /// 平均刺激（内部, 外部）。Auto は `MessageSpace` と同じ計算（丸めない）。
    pub fn mean_stimuli(&self) -> Result<(f64, f64), ConfigError> {
        Ok(match &self.mean_stimulus {
            MeanStimulus::Auto => {
                let ms = self.message_space()?;
                (ms.internal_mean_stimulus(), ms.external_mean_stimulus())
            }
            MeanStimulus::Manual { internal, external } => (*internal, *external),
        })
    }

    pub fn validate(&self, n_nodes: Option<usize>) -> Result<(), ConfigError> {
        let _ = self.message_space()?;
        if !(0.0..=1.0).contains(&self.receipt_probability) {
            return err(format!(
                "receipt_probability は [0, 1] である必要があります (実際: {})",
                self.receipt_probability
            ));
        }
        if self.calibration_rounds < 1 {
            return err("calibration_rounds は 1 以上である必要があります");
        }
        if self.max_rounds < 1 {
            return err("max_rounds は 1 以上である必要があります");
        }
        if self.user_seed >= MAX_SEED || self.diffusion_seed >= MAX_SEED {
            return err("user_seed・diffusion_seed は 2^53 未満である必要があります");
        }
        let (mi, me) = self.mean_stimuli()?;
        self.internal_parameters
            .validate("internal_parameters", mi)?;
        self.external_parameters
            .validate("external_parameters", me)?;
        self.mechanisms.validate()?;
        self.recommender.validate()?;
        if let Some(n) = n_nodes {
            let bytes = n as u64 * (self.max_rounds as u64 + 1) * LOG_BYTES_PER_NODE_ROUND;
            if bytes > LOG_LIMIT_BYTES {
                return Err(ConfigError::LogTooLarge { bytes });
            }
        }
        Ok(())
    }
}
