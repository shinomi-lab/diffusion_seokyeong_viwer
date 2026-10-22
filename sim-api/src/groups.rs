//! ノード群の推移（7.9、F19）。

use serde::Serialize;

use crate::log::{IntMode, NodeKind, UtilityLog, kind_of, quantile_sorted};

#[derive(Clone, Debug, PartialEq)]
pub enum GroupSel {
    IntOnly,
    ExtOnly,
    Both,
    ReceivedOnly,
    /// リング・扇形・範囲選択で作った任意の群。
    Custom(Vec<u32>),
}

impl GroupSel {
    /// JS 側の番号: 0 内部のみ / 1 外部のみ / 2 両方 / 3 受信のみ / 4 カスタム。
    pub fn from_u8(v: u8, custom: Vec<u32>) -> Option<Self> {
        Some(match v {
            0 => Self::IntOnly,
            1 => Self::ExtOnly,
            2 => Self::Both,
            3 => Self::ReceivedOnly,
            4 => Self::Custom(custom),
            _ => return None,
        })
    }

    fn kind(&self) -> Option<NodeKind> {
        match self {
            Self::IntOnly => Some(NodeKind::Internal),
            Self::ExtOnly => Some(NodeKind::External),
            Self::Both => Some(NodeKind::Both),
            Self::ReceivedOnly => Some(NodeKind::ReceivedOnly),
            Self::Custom(_) => None,
        }
    }
}

/// 長さ = at_round + 1。
#[derive(Clone, Debug, Default, Serialize, PartialEq)]
pub struct Band {
    pub mean: Vec<f32>,
    pub p10: Vec<f32>,
    pub p50: Vec<f32>,
    pub p90: Vec<f32>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct GroupSeries {
    pub at_round: u32,
    pub members: u32,
    pub int: Band,
    pub ext: Band,
    /// 各ラウンドでそのつど分類し直したときの群の大きさ（Custom は一定）。
    pub size_by_round: Vec<u32>,
}

/// メンバーは `at_round` の行のフラグで決めて固定し、ラウンド 0〜`at_round` の各行の効用を集計する。
pub fn group_series(
    log: &UtilityLog,
    ip: u32,
    sel: &GroupSel,
    at_round: u32,
    int_mode: IntMode,
) -> GroupSeries {
    let at = (at_round as usize).min(log.n_rows() - 1);
    let classify = |r: usize| -> Vec<u32> {
        match sel {
            GroupSel::Custom(v) => v
                .iter()
                .copied()
                .filter(|&k| (k as usize) < log.n_nodes())
                .collect(),
            _ => {
                let want = sel.kind().expect("Custom 以外は種類を持つ");
                log.row_flags(r)
                    .iter()
                    .enumerate()
                    .filter(|&(k, &f)| kind_of(f, int_mode, k as u32 == ip) == want)
                    .map(|(k, _)| k as u32)
                    .collect()
            }
        }
    };
    let members = classify(at);
    let mut int = Band::default();
    let mut ext = Band::default();
    let mut size_by_round = Vec::with_capacity(at + 1);
    for r in 0..=at {
        for (row, band) in [(log.row_u_int(r), &mut int), (log.row_u_ext(r), &mut ext)] {
            let mut v: Vec<f32> = members.iter().map(|&k| row[k as usize]).collect();
            let mean = if v.is_empty() {
                0.0
            } else {
                (v.iter().map(|&x| x as f64).sum::<f64>() / v.len() as f64) as f32
            };
            v.sort_by(f32::total_cmp);
            band.mean.push(mean);
            band.p10.push(quantile_sorted(&v, 0.1));
            band.p50.push(quantile_sorted(&v, 0.5));
            band.p90.push(quantile_sorted(&v, 0.9));
        }
        size_by_round.push(classify(r).len() as u32);
    }
    GroupSeries {
        at_round: at as u32,
        members: members.len() as u32,
        int,
        ext,
        size_by_round,
    }
}
