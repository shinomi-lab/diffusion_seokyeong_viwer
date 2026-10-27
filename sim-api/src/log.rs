//! 状態ログ・ノード分類・ラウンド要約・イベント列（7.5, 7.7）。

use serde::Serialize;

use crate::engine::{Observer, ReceiveEvent};

/// UtilityLog の各ラウンドの行に格納する 1 バイトのフラグ。
pub const F_RECV_NOW: u8 = 1;
pub const F_INT_NOW: u8 = 2;
pub const F_EXT_NOW: u8 = 4;
pub const F_EVER_RECV: u8 = 8;
pub const F_EVER_INT: u8 = 16;
pub const F_EVER_EXT: u8 = 32;
/// このラウンドに推薦で受信した（`recommender-design.md` 6.5）。
pub const F_REC_NOW: u8 = 64;
/// 一度でも推薦で受け取った（累積）。推薦由来の連鎖の「このラウンド」はイベント列（`EV_RECCHAIN`）で分かる。
pub const F_EVER_REC: u8 = 128;
/// `Session::ever_flags()` の推薦（累積）のビット（`F_EVER_REC >> 3`）。1・2・4 は EVER_RECV・INT・EXT と同じ。
pub const EVER_FLAG_REC: u8 = F_EVER_REC >> 3;
/// `ever_flags()` に出す累積のビット（このラウンドの印 `F_REC_NOW` は含めない）。
pub const EVER_FLAGS_MASK: u8 = 1 | 2 | 4 | EVER_FLAG_REC;

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum NodeKind {
    Unreceived = 0,
    ReceivedOnly = 1,
    Internal = 2,
    External = 3,
    Both = 4,
    Ip = 5,
}

impl NodeKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Unreceived => "unreceived",
            Self::ReceivedOnly => "received",
            Self::Internal => "internal",
            Self::External => "external",
            Self::Both => "both",
            Self::Ip => "ip",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntMode {
    /// これまでに 1 回でも成立（既定。要件書 9.1）。
    Cumulative,
    /// そのラウンドで成立したものだけ。
    ThisRoundOnly,
}

impl IntMode {
    pub fn from_u8(v: u8) -> Self {
        if v == 0 {
            Self::Cumulative
        } else {
            Self::ThisRoundOnly
        }
    }
}

/// 外部は常に累積。内部は `int_mode` に従う。
pub fn kind_of(flags: u8, int_mode: IntMode, is_ip: bool) -> NodeKind {
    if is_ip {
        return NodeKind::Ip;
    }
    if flags & F_EVER_RECV == 0 {
        return NodeKind::Unreceived;
    }
    let int = match int_mode {
        IntMode::Cumulative => flags & F_EVER_INT != 0,
        IntMode::ThisRoundOnly => flags & F_INT_NOW != 0,
    };
    let ext = flags & F_EVER_EXT != 0;
    match (int, ext) {
        (false, false) => NodeKind::ReceivedOnly,
        (true, false) => NodeKind::Internal,
        (false, true) => NodeKind::External,
        (true, true) => NodeKind::Both,
    }
}

/// ラウンド優先のログ。行 r の長さ = n_nodes。行 0 = 初期状態。
#[derive(Clone)]
pub struct UtilityLog {
    n_nodes: usize,
    u_int: Vec<f32>,
    u_ext: Vec<f32>,
    flags: Vec<u8>,
}

impl UtilityLog {
    pub fn new(n_nodes: usize) -> Self {
        Self {
            n_nodes,
            u_int: Vec::new(),
            u_ext: Vec::new(),
            flags: Vec::new(),
        }
    }

    pub fn n_nodes(&self) -> usize {
        self.n_nodes
    }

    /// 行数（= 現在のラウンド + 1）。
    pub fn n_rows(&self) -> usize {
        self.flags.len() / self.n_nodes.max(1)
    }

    /// 効用は丸めず上限も付けない（INV-15。f32 への変換のみ）。
    pub fn push_row(
        &mut self,
        u_int: impl Iterator<Item = f64>,
        u_ext: impl Iterator<Item = f64>,
        flags: &[u8],
    ) {
        self.u_int.extend(u_int.map(|v| v as f32));
        self.u_ext.extend(u_ext.map(|v| v as f32));
        self.flags.extend_from_slice(flags);
        debug_assert_eq!(self.u_int.len(), self.flags.len());
    }

    pub fn truncate_rows(&mut self, rows: usize) {
        let len = rows * self.n_nodes;
        self.u_int.truncate(len);
        self.u_ext.truncate(len);
        self.flags.truncate(len);
    }

    #[inline]
    fn range(&self, r: usize) -> std::ops::Range<usize> {
        r * self.n_nodes..(r + 1) * self.n_nodes
    }

    pub fn row_u_int(&self, r: usize) -> &[f32] {
        &self.u_int[self.range(r)]
    }

    pub fn row_u_ext(&self, r: usize) -> &[f32] {
        &self.u_ext[self.range(r)]
    }

    pub fn row_flags(&self, r: usize) -> &[u8] {
        &self.flags[self.range(r)]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Choice {
    Msg(u8),
    // m∅（発信しない。M4）は 2026-10-20 に廃止。
}

impl Choice {
    /// 系列文字列の 1 文字（16 進 1 桁）。
    pub fn to_char(self) -> char {
        let Choice::Msg(i) = self;
        char::from_digit(i as u32, 16).unwrap_or('?')
    }

    /// JS 側の表現: 0.. = メッセージ。
    pub fn from_i32(v: i32) -> Option<Self> {
        match v {
            0..=255 => Some(Choice::Msg(v as u8)),
            _ => None,
        }
    }
}

/// [p10, p50, p90]。
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct UtilityStats {
    pub n: u32,
    pub mean_int: f32,
    pub mean_ext: f32,
    pub q_int: [f32; 3],
    pub q_ext: [f32; 3],
}

/// 昇順に並べて添字 `round((n-1)·q)` の値を採る（線形補間しない。両版で同じ値にするため）。
pub fn quantile_sorted(sorted: &[f32], q: f64) -> f32 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = (((sorted.len() - 1) as f64) * q).round() as usize;
    sorted[idx]
}

pub fn stats_of(mut ui: Vec<f32>, mut ue: Vec<f32>) -> UtilityStats {
    let n = ui.len();
    if n == 0 {
        return UtilityStats::default();
    }
    let mean = |v: &[f32]| (v.iter().map(|&x| x as f64).sum::<f64>() / v.len() as f64) as f32;
    let (mi, me) = (mean(&ui), mean(&ue));
    ui.sort_by(f32::total_cmp);
    ue.sort_by(f32::total_cmp);
    let qs = |v: &[f32]| {
        [
            quantile_sorted(v, 0.1),
            quantile_sorted(v, 0.5),
            quantile_sorted(v, 0.9),
        ]
    };
    UtilityStats {
        n: n as u32,
        mean_int: mi,
        mean_ext: me,
        q_int: qs(&ui),
        q_ext: qs(&ue),
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct RoundSummary {
    pub round: u32,
    pub choice: Choice,
    pub n_received: u32,
    pub n_share: u32,
    pub new_xact: u32,
    pub cum_xact: u32,
    pub n_waves: u32,
    /// このラウンドで内部が成立した受信者数。
    pub n_int_active_now: u32,
    /// 受信済みノード。
    pub recv: UtilityStats,
    /// IP 以外の全ノード。
    pub all: UtilityStats,
    /// 推薦（`recommender-design.md` 6.4）。推薦 off では 0。
    pub rec_received: u32,
    pub rec_share: u32,
    pub rec_xact: u32,
    pub chain_received: u32,
    pub chain_xact: u32,
    /// 一度でも推薦で受け取った人数（累積、このラウンドの終わり）。
    pub cum_rec: u32,
}

/// 列指向（WASM の TypedArray にそのまま渡せる）。長さはすべて同じ。
#[derive(Clone, Debug, Default)]
pub struct EventColumns {
    pub wave: Vec<u16>,
    pub node: Vec<u32>,
    pub from: Vec<u32>,
    pub n_arr: Vec<u8>,
    pub u_int: Vec<f32>,
    pub u_ext: Vec<f32>,
    /// bit0 = int_fired, bit1 = ext_fired, bit2 = enqueued, bit3 = 推薦で受信, bit4 = 推薦由来の連鎖
    pub flags: Vec<u8>,
}

impl EventColumns {
    pub fn len(&self) -> usize {
        self.node.len()
    }

    pub fn is_empty(&self) -> bool {
        self.node.is_empty()
    }
}

pub const EV_INT: u8 = 1;
pub const EV_EXT: u8 = 2;
pub const EV_ENQ: u8 = 4;
/// 推薦で受信（直接）。
pub const EV_REC: u8 = 8;
/// 推薦由来の連鎖で受信。
pub const EV_RECCHAIN: u8 = 16;

#[derive(Default)]
pub struct TraceRecorder {
    pub cols: EventColumns,
}

impl Observer for TraceRecorder {
    fn on_receive(&mut self, ev: &ReceiveEvent) {
        let c = &mut self.cols;
        c.wave.push(ev.wave as u16);
        c.node.push(ev.node);
        c.from.push(ev.from);
        c.n_arr.push(ev.n_arr);
        c.u_int.push(ev.u_int as f32);
        c.u_ext.push(ev.u_ext as f32);
        c.flags.push(
            if ev.int_fired { EV_INT } else { 0 }
                | if ev.ext_fired { EV_EXT } else { 0 }
                | if ev.enqueued { EV_ENQ } else { 0 }
                | match ev.via {
                    crate::engine::VIA_REC => EV_REC,
                    crate::engine::VIA_RECCHAIN => EV_RECCHAIN,
                    _ => 0,
                },
        );
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct RecvInfo {
    pub round: u32,
    pub wave: u32,
    pub from: u32,
    /// 推薦で受信したときは -1。
    pub from_label: i64,
    pub n_arr: u8,
    /// 受信の経路（0 = フォロー、1 = 推薦、2 = 推薦由来の連鎖）。
    pub via: u8,
}

/// F10: ノード詳細。
#[derive(Clone, Debug, Serialize)]
pub struct NodeDetail {
    pub node: u32,
    pub label: i64,
    /// 長さ = 現在のラウンド + 1（行 0 = 初期値）。上限なし。
    pub u_int: Vec<f32>,
    pub u_ext: Vec<f32>,
    pub received: Vec<RecvInfo>,
    pub int_rounds: Vec<u32>,
    pub ext_round: Option<u32>,
    pub is_ip: bool,
}
