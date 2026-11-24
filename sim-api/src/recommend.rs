//! 推薦（Rモデル。`recommender-v3-design.md` 6.1）。アイテム = そのラウンドのメッセージ 1 つ。
//! 各ウェーブのあとに、反応率に応じた数だけフォロー外へ押し出す。スコアは持たず、エンジン（`engine::cascade`）が
//! `phi`・`two_step_walk` を使って枠を配る。
//!
//! INV-W2・W3: 乱数は推薦専用の `rec_rng` だけを引き、拡散の乱数は引かない。
//! INV-W5: 四則演算・比較・`min`・`floor` だけを使い、Web（WASM）とデスクトップで同じ値・同じ選択にする。
//! （2026-10-27 に、推薦の v1（基準）と v2（CF＋グラフ）は廃止した。）

use rand::{Rng, SeedableRng};

use crate::config::RecommenderConfig;
use crate::csr::Csr;
use crate::population::Population;
use crate::rng::SimRng;

/// 推薦専用の乱数の塩（任意に選んだ固定値。2026-10-22 決定 D-R8）。
pub const REC_SALT: u64 = 0x5EC0_7A1D_3B9F_2E41;

/// 推薦専用の乱数（Rモデルの抽選用）。
pub fn rec_rng(diffusion_seed: u64) -> SimRng {
    SimRng::seed_from_u64(diffusion_seed ^ REC_SALT)
}

/// Rモデル: 基準の反応率 ē0（元資料 3.1 節）。平均的なメッセージを初めて受け取ったときにシェアする人の割合。
/// `initial` は母集団の初期状態。判定は `node::decide`（ちょうど 0 は不成立）。
pub fn base_share_rate(initial: &Population, mean_internal: f64) -> f64 {
    let n = initial.states.len();
    if n == 0 {
        return 0.0;
    }
    let k = initial
        .states
        .iter()
        .filter(|s| crate::node::decide(s.w_int * s.u_int + (1.0 - s.w_int) * mean_internal))
        .count();
    k as f64 / n as f64
}

/// Rモデル: 反応の良さ φ(x)。基準の τ 倍に届かなければ 0、届けば x（上限 φ_max）。
pub fn phi(cfg: &RecommenderConfig, x: f64) -> f64 {
    if x < cfg.tau { 0.0 } else { x.min(cfg.phi_max) }
}

/// Rモデル: 近くへの推薦の宛先。シェア者の出辺から 1 本を一様に選び（起点を出次数に比例して選び、
/// その後続を一様に選ぶのと同じ）、行き先 l の後続を 1 人一様に選ぶ。l が行き止まりなら None。
/// `deg_cum[k]` は `sharers[..=k]` の出次数の累積和。乱数は「出辺 → 2 歩目」の順に引く（INV-W3）。
/// 32bit（WASM）と 64bit で同じ値にするため、範囲は u32 で引く。
pub fn two_step_walk(
    csr: &Csr,
    sharers: &[u32],
    deg_cum: &[u32],
    rec_rng: &mut SimRng,
) -> Option<u32> {
    let total = *deg_cum.last()?;
    if total == 0 {
        return None;
    }
    let r = rec_rng.random_range(0..total);
    let idx = deg_cum.partition_point(|&c| c <= r);
    let offset = r - if idx == 0 { 0 } else { deg_cum[idx - 1] };
    let l = csr.succ(sharers[idx])[offset as usize];
    let next = csr.succ(l);
    if next.is_empty() {
        return None;
    }
    Some(next[rec_rng.random_range(0..next.len() as u32) as usize])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::NodeState;

    fn pop(n: usize) -> Population {
        Population {
            states: (0..n)
                .map(|_| NodeState::new(0.5, 0.5, -1.0, -1.0))
                .collect(),
            recv_round: vec![0; n],
            u0_int: -1.0,
            u0_ext: -1.0,
        }
    }

    fn csr_of(edges: &[(usize, usize)]) -> Csr {
        use graph_lib::prelude::{DiGraphB, GraphB};
        Csr::from_graph(&GraphB::Di(DiGraphB::from(edges)))
    }

    /// T-W05: 起点はシェア者の出次数に比例する。シェア者 0 の後続は 2・3・4、シェア者 1 の後続は 5 で、
    /// 2〜5 の後続はそれぞれ 6〜9 の 1 人だけなので、終点から通った出辺が分かる（各 1/4）。
    #[test]
    fn t_w05_walk_origin_proportional_to_out_degree() {
        let csr = csr_of(&[
            (0, 2),
            (0, 3),
            (0, 4),
            (1, 5),
            (2, 6),
            (3, 7),
            (4, 8),
            (5, 9),
        ]);
        let mut rng = rec_rng(0);
        let mut counts = [0u32; 4];
        let trials = 40_000;
        for _ in 0..trials {
            let j = two_step_walk(&csr, &[0, 1], &[3, 4], &mut rng).unwrap();
            counts[j as usize - 6] += 1;
        }
        for c in counts {
            assert!((c as f64 / trials as f64 - 0.25).abs() < 0.01, "{counts:?}");
        }
        // 2 歩目の先が行き止まりなら None
        let line = csr_of(&[(0, 1)]);
        assert_eq!(two_step_walk(&line, &[0], &[1], &mut rng), None);
        // シェア者の出次数が 0 なら None（乱数も引かない）
        let before = rng.clone();
        assert_eq!(two_step_walk(&line, &[1], &[0], &mut rng), None);
        assert_eq!(rng, before);
    }

    /// T-W05: φ と ē0 の手計算。
    #[test]
    fn t_w05_phi_and_base_share_rate() {
        let cfg = RecommenderConfig {
            tau: 1.0,
            phi_max: 2.0,
            ..RecommenderConfig::default()
        };
        assert_eq!(phi(&cfg, 0.99), 0.0);
        assert_eq!(phi(&cfg, 1.5), 1.5);
        assert_eq!(phi(&cfg, 5.0), 2.0);
        // w = 0.05 → 0.05·(−1) + 0.95·0.125 > 0（シェアする）、w = 0.5 → しない。4 人中 1 人
        let mut p = pop(4);
        p.states[0].w_int = 0.05;
        assert_eq!(base_share_rate(&p, 0.125), 0.25);
        // ちょうど 0 は不成立: w·(−1) + (1 − w)·1 = 0 となる w = 0.5、s̄ = 1
        assert_eq!(base_share_rate(&pop(2), 1.0), 0.0);
    }
}
