//! 全体図（7.8）: IP からのホップ距離のリング、放射状レイアウト、扇形セルの集計、範囲選択。
//! ★リング（静的な最短距離）とウェーブ（確率的な再生の段）は別物。

use std::collections::VecDeque;
use std::f32::consts::TAU;

use crate::csr::Csr;
use crate::log::NodeKind;

pub struct Rings {
    /// IP からの最短距離。`u32::MAX` = 到達不能。
    pub dist: Vec<u32>,
    /// 添字 = 距離。[0] = 1（IP）。
    pub ring_sizes: Vec<u32>,
    pub unreachable: u32,
}

pub fn compute_rings(csr: &Csr, ip: u32) -> Rings {
    let n = csr.n_nodes();
    let mut dist = vec![u32::MAX; n];
    dist[ip as usize] = 0;
    let mut q = VecDeque::with_capacity(n / 2 + 1);
    q.push_back(ip);
    let mut ring_sizes = vec![1u32];
    while let Some(u) = q.pop_front() {
        let d = dist[u as usize];
        for &v in csr.succ(u) {
            if dist[v as usize] == u32::MAX {
                dist[v as usize] = d + 1;
                if ring_sizes.len() <= (d + 1) as usize {
                    ring_sizes.push(0);
                }
                ring_sizes[(d + 1) as usize] += 1;
                q.push_back(v);
            }
        }
    }
    let unreachable = dist.iter().filter(|&&d| d == u32::MAX).count() as u32;
    Rings {
        dist,
        ring_sizes,
        unreachable,
    }
}

/// 到達可能なノードだけ有効。angle ∈ [0, 2π)。
pub struct RadialLayout {
    pub angle: Vec<f32>,
    pub parent: Vec<u32>,
}

/// 初版は素朴（O(N+E)）。BFS で親（最初に見つけたノード）を決め、深さ優先の行きがけ順で番号 pos を振り、
/// `angle = 2π · pos / n_reachable`。同じ部分木のノードが連続した角度に並ぶ。
pub fn radial_layout(csr: &Csr, rings: &Rings, ip: u32) -> RadialLayout {
    let n = csr.n_nodes();
    let mut parent = vec![u32::MAX; n];
    let mut seen = vec![false; n];
    seen[ip as usize] = true;
    let mut q = VecDeque::new();
    q.push_back(ip);
    // 子のリスト（BFS の発見順 = 親ごとに添字の昇順）
    let mut children: Vec<Vec<u32>> = vec![Vec::new(); n];
    while let Some(u) = q.pop_front() {
        for &v in csr.succ(u) {
            if !seen[v as usize] {
                seen[v as usize] = true;
                parent[v as usize] = u;
                children[u as usize].push(v);
                q.push_back(v);
            }
        }
    }
    let n_reach = n as u32 - rings.unreachable;
    let mut angle = vec![0.0f32; n];
    let mut pos = 0u32;
    let mut stack = vec![ip];
    while let Some(v) = stack.pop() {
        angle[v as usize] = TAU * (pos as f32) / (n_reach as f32);
        // 丸めで 2π に達しないようにする
        if angle[v as usize] >= TAU {
            angle[v as usize] = f32::from_bits(TAU.to_bits() - 1);
        }
        pos += 1;
        for &c in children[v as usize].iter().rev() {
            stack.push(c);
        }
    }
    RadialLayout { angle, parent }
}

pub const N_SECTORS: u32 = 72; // 5 度刻み

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RingSectorCell {
    pub n: u32,
    /// 受信済み（受信のみ・内部・外部・両方）の数。
    pub n_recv: u32,
    pub n_int: u32,
    pub n_ext: u32,
    pub n_both: u32,
}

pub struct SectorGrid {
    /// 添字 = ring * N_SECTORS + sector
    pub cells: Vec<RingSectorCell>,
}

pub fn sector_of(angle: f32) -> u32 {
    (((angle / TAU) * N_SECTORS as f32) as u32).min(N_SECTORS - 1)
}

pub fn aggregate(rings: &Rings, layout: &RadialLayout, kinds: &[NodeKind]) -> SectorGrid {
    let mut cells = vec![RingSectorCell::default(); rings.ring_sizes.len() * N_SECTORS as usize];
    for (k, &d) in rings.dist.iter().enumerate() {
        if d == u32::MAX {
            continue;
        }
        let c = &mut cells[(d * N_SECTORS + sector_of(layout.angle[k])) as usize];
        c.n += 1;
        match kinds[k] {
            NodeKind::Unreceived | NodeKind::Ip => {}
            NodeKind::ReceivedOnly => c.n_recv += 1,
            NodeKind::Internal => {
                c.n_recv += 1;
                c.n_int += 1;
            }
            NodeKind::External => {
                c.n_recv += 1;
                c.n_ext += 1;
            }
            NodeKind::Both => {
                c.n_recv += 1;
                c.n_both += 1;
            }
        }
    }
    SectorGrid { cells }
}

/// 個別展開できる上限（N03）。
pub const EXPAND_CAP: usize = 200_000;

#[derive(Debug, thiserror::Error, PartialEq)]
#[error("選択範囲のノード数（{0}）が上限 {EXPAND_CAP} を超えています。範囲を狭めてください")]
pub struct TooMany(pub usize);

/// リング `[ring_lo, ring_hi]` かつ角度 `[ang_lo, ang_hi)` のノード。`ang_lo > ang_hi` なら 0 をまたぐ範囲。
pub fn select_nodes(
    rings: &Rings,
    layout: &RadialLayout,
    ring_lo: u32,
    ring_hi: u32,
    ang_lo: f32,
    ang_hi: f32,
) -> Result<Vec<u32>, TooMany> {
    let in_ang = |a: f32| {
        if ang_lo <= ang_hi {
            a >= ang_lo && a < ang_hi
        } else {
            a >= ang_lo || a < ang_hi
        }
    };
    let mut out = Vec::new();
    for (k, &d) in rings.dist.iter().enumerate() {
        if d != u32::MAX && d >= ring_lo && d <= ring_hi && in_ang(layout.angle[k]) {
            out.push(k as u32);
            if out.len() > EXPAND_CAP {
                return Err(TooMany(out.len()));
            }
        }
    }
    Ok(out)
}

/// 到達不能のノード（全体図では 1 つの枠にまとめる）。
pub fn select_unreachable(rings: &Rings) -> Result<Vec<u32>, TooMany> {
    let v: Vec<u32> = rings
        .dist
        .iter()
        .enumerate()
        .filter(|&(_, &d)| d == u32::MAX)
        .map(|(k, _)| k as u32)
        .collect();
    if v.len() > EXPAND_CAP {
        Err(TooMany(v.len()))
    } else {
        Ok(v)
    }
}
