//! 画面の状態（Web の `store/view.ts` に相当）。

use crate::camera::Camera;
use sim_api::log::{Choice, EventColumns, IntMode, RoundSummary};

/// ラウンドごとの結果（画面用）。
pub struct RoundData {
    pub summary: RoundSummary,
    pub events: EventColumns,
    /// ラウンド開始時の EVER_* ビット。
    pub ever_before: Vec<u8>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    /// リング・扇形の全体図。
    Overview,
    /// 個別ノード。
    Expanded,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Node,
    Group,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GroupKind {
    IntOnly,
    ExtOnly,
    Both,
    ReceivedOnly,
    Custom,
}

impl GroupKind {
    pub const ALL: [(GroupKind, &'static str); 5] = [
        (GroupKind::IntOnly, "内部のみ"),
        (GroupKind::ExtOnly, "外部のみ"),
        (GroupKind::Both, "両方"),
        (GroupKind::ReceivedOnly, "受信のみ"),
        (GroupKind::Custom, "選択した群（リング・範囲）"),
    ];
}

pub struct View {
    pub mode: Mode,
    /// 展開中のノード集合。None は全ノード。
    pub expanded: Option<Vec<u32>>,
    pub expanded_label: String,
    /// 全体図から展開したか（「全体図に戻る」を出す）。
    pub from_overview: bool,
    pub selected: Option<u32>,
    /// リング・扇形・範囲選択で作った群。
    pub group: Option<Vec<u32>>,
    pub tab: Tab,
    pub group_kind: GroupKind,
    pub int_mode: IntMode,
    pub camera: Camera,
    pub notice: Option<String>,
    /// カメラをノード集合に合わせ直す（次のフレームで、実際の大きさが分かってから）。
    pub refit: bool,
}

impl Default for View {
    fn default() -> Self {
        Self {
            mode: Mode::Expanded,
            expanded: None,
            expanded_label: String::new(),
            from_overview: false,
            selected: None,
            group: None,
            tab: Tab::Node,
            group_kind: GroupKind::Both,
            int_mode: IntMode::Cumulative,
            camera: Camera::default(),
            notice: None,
            refit: true,
        }
    }
}

pub fn sequence_string(choices: &[Choice]) -> String {
    choices.iter().map(|c| c.to_char()).collect()
}
