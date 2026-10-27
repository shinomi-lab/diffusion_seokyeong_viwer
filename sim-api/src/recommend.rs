//! 推薦（`recommender-design.md` 6.2、`recommender-v2-design.md` 6.1）。アイテム = メッセージの種類。
//! - 基準（v1）: スコア = w_s·P̂シェア + (1 − w_s)·P̂外部行動。P̂ は本人の同じ種類の履歴を全体の率 q̄ へ寄せて推定する。
//! - CF＋グラフ（v2）: f = α·f_CF + (1 − α)·f_G。f_CF はアイテムベースの協調フィルタリング、f_G は社会的証明。
//!
//! - Rモデル（v3、`recommender-v3-design.md` 6.1）: 各ウェーブのあとに、反応率に応じた数だけフォロー外へ押し出す。
//!   スコアは持たず、エンジン（`engine::cascade`）が `phi`・`two_step_walk` を使って枠を配る。
//!
//! INV-R3・INV-V3: 四則演算と sqrt だけを使い、和の順序を固定して、Web（WASM）とデスクトップで同じ値・同じ選択にする。
//! v1・v2 は乱数を使わない。Rモデルだけが `rec_rng` を引く（INV-W2・W3）。

use rand::{Rng, SeedableRng};

use crate::config::{RecMethod, RecommenderConfig};
use crate::csr::Csr;
use crate::population::Population;
use crate::rng::SimRng;

/// 推薦専用の乱数の塩（任意に選んだ固定値。2026-10-22 決定 D-R8）。
pub const REC_SALT: u64 = 0x5EC0_7A1D_3B9F_2E41;

/// 推薦専用の乱数（Rモデルの抽選用。v1・v2 では引かない）。
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

/// 種類 m に対する u の推定確率 (P̂シェア, P̂外部行動)。
pub fn estimate(pop: &Population, u: u32, m: u8, n0: f64) -> (f64, f64) {
    let l = pop.rec_l;
    let [n, s, x] = pop.rec_hist[u as usize * l + m as usize];
    let [tn, ts, tx] = pop.rec_total[m as usize];
    let (qs, qx) = if tn == 0 {
        (0.0, 0.0)
    } else {
        (ts as f64 / tn as f64, tx as f64 / tn as f64)
    };
    let (n, s, x) = (n as f64, s as f64, x as f64);
    let denom = n + n0;
    // n = 0 かつ n0 = 0 のときは 0/0 を避けて全体の率を使う
    let ps = if denom == 0.0 {
        qs
    } else {
        (s + n0 * qs) / denom
    };
    let px = if pop.states[u as usize].ext_done {
        0.0 // 外部行動は生涯 1 回
    } else if denom == 0.0 {
        qx
    } else {
        (x + n0 * qx) / denom
    };
    (ps, px)
}

pub fn score(ps: f64, px: f64, w_share: f64) -> f64 {
    w_share * ps + (1.0 - w_share) * px
}

/// 種類ごとの全体の率 (b_s, b_x)。受信が 0 の種類は (0, 0)。
pub fn base_rates(pop: &Population) -> Vec<(f64, f64)> {
    pop.rec_total
        .iter()
        .map(|&[tn, ts, tx]| {
            if tn == 0 {
                (0.0, 0.0)
            } else {
                (ts as f64 / tn as f64, tx as f64 / tn as f64)
            }
        })
        .collect()
}

/// 種類どうしの似かた（L×L、行優先）。両方の種類を受け取ったことがある人について、
/// 「本人の反応率 − 全体の率」（シェアと外部行動の 2 成分）のコサイン類似度（adjusted cosine）。
/// 共通の受信者が `min_common` 人未満・分母 0 の組は 0、負は 0 に切る。対角は 1。INV-V4: 保存せず毎回計算する。
pub fn item_sim(pop: &Population, min_common: u32) -> Vec<f64> {
    let l = pop.rec_l;
    let base = base_rates(pop);
    let (mut dot, mut na, mut nb) = (
        vec![0.0f64; l * l],
        vec![0.0f64; l * l],
        vec![0.0f64; l * l],
    );
    let mut cnt = vec![0u32; l * l];
    let mut d: Vec<Option<(f64, f64)>> = vec![None; l];
    for u in 0..pop.states.len() {
        for (m, dm) in d.iter_mut().enumerate() {
            let [n, s, x] = pop.rec_hist[u * l + m];
            *dm = (n > 0).then(|| {
                let n = n as f64;
                (s as f64 / n - base[m].0, x as f64 / n - base[m].1)
            });
        }
        for m in 0..l {
            let Some((sm, xm)) = d[m] else { continue };
            for (k, dk) in d.iter().enumerate().skip(m + 1) {
                let Some((sk, xk)) = *dk else { continue };
                let i = m * l + k;
                dot[i] += sm * sk + xm * xk;
                na[i] += sm * sm + xm * xm;
                nb[i] += sk * sk + xk * xk;
                cnt[i] += 1;
            }
        }
    }
    let mut sim = vec![0.0f64; l * l];
    for m in 0..l {
        sim[m * l + m] = 1.0;
        for k in m + 1..l {
            let i = m * l + k;
            let v = if cnt[i] >= min_common && na[i] > 0.0 && nb[i] > 0.0 {
                (dot[i] / (na[i].sqrt() * nb[i].sqrt())).max(0.0)
            } else {
                0.0
            };
            sim[i] = v;
            sim[k * l + m] = v;
        }
    }
    sim
}

/// CF の推定 (P̂シェア, P̂外部行動)。ベースライン（全体の率）に、本人が受け取ったことのある種類の
/// 「反応率 − 全体の率」を似かたで重み付けして足す（Koren 型）。0〜1 に切る。外部行動が済んだ人は P̂x = 0。
pub fn estimate_cf(
    pop: &Population,
    u: u32,
    m: u8,
    sim: &[f64],
    base: &[(f64, f64)],
    lambda: f64,
) -> (f64, f64) {
    let l = pop.rec_l;
    let mi = m as usize;
    let (mut num_s, mut num_x, mut den) = (0.0f64, 0.0f64, 0.0f64);
    for k in 0..l {
        let [n, s, x] = pop.rec_hist[u as usize * l + k];
        let w = sim[mi * l + k];
        if n == 0 || w == 0.0 {
            continue;
        }
        let n = n as f64;
        num_s += w * (s as f64 / n - base[k].0);
        num_x += w * (x as f64 / n - base[k].1);
        den += w;
    }
    let (bs, bx) = base[mi];
    let d = den + lambda;
    // 分母 0（λ = 0 かつ寄与する種類なし）のときは全体の率
    let (ps, px) = if d == 0.0 {
        (bs, bx)
    } else {
        (bs + num_s / d, bx + num_x / d)
    };
    let px = if pop.states[u as usize].ext_done {
        0.0 // 外部行動は生涯 1 回
    } else {
        px.clamp(0.0, 1.0)
    };
    (ps.clamp(0.0, 1.0), px)
}

/// 社会的証明 f_G: フォロー先（u に送りうる人 = `csr.pred(u)`）のうち、このラウンドの ① で送った人の割合。
pub fn graph_score(csr: &Csr, social: &[u32], u: u32) -> f64 {
    let k = csr.pred(u).len();
    if k == 0 {
        0.0
    } else {
        social[u as usize] as f64 / k as f64
    }
}

/// 推薦の段階で届ける人: IP 以外で、このラウンドにまだ受信していない人のうち、スコア ≥ θ の全員（ノード番号の昇順）。
/// `g`: CF＋グラフのときの (グラフ, ① の社会的証明の数)。無ければ f_G = 0。
pub fn select(
    pop: &Population,
    rcvd: &[bool],
    ip: u32,
    m: u8,
    cfg: &RecommenderConfig,
    g: Option<(&Csr, &[u32])>,
) -> Vec<u32> {
    let mut out = Vec::new();
    match cfg.method {
        RecMethod::Baseline => {
            for (u, &got) in rcvd.iter().enumerate() {
                if got || u as u32 == ip {
                    continue;
                }
                let (ps, px) = estimate(pop, u as u32, m, cfg.prior_n0);
                if score(ps, px, cfg.w_share) >= cfg.threshold {
                    out.push(u as u32);
                }
            }
        }
        RecMethod::Hybrid => {
            let a = cfg.alpha;
            // α = 1 なら f_G、α = 0 なら f_CF を計算しない（結果は同じで、速さのため）
            let cf = (a != 0.0).then(|| (item_sim(pop, cfg.min_common), base_rates(pop)));
            for (u, &got) in rcvd.iter().enumerate() {
                if got || u as u32 == ip {
                    continue;
                }
                let u = u as u32;
                let f_cf = cf.as_ref().map_or(0.0, |(sim, base)| {
                    let (ps, px) = estimate_cf(pop, u, m, sim, base, cfg.prior_n0);
                    score(ps, px, cfg.w_share)
                });
                let f_g = match g {
                    Some((csr, social)) if a != 1.0 => graph_score(csr, social, u),
                    _ => 0.0,
                };
                if a * f_cf + (1.0 - a) * f_g >= cfg.threshold {
                    out.push(u);
                }
            }
        }
        // Rモデルは推薦の段階（②）を使わない（エンジンが各ウェーブで配る）。
        RecMethod::Wave => {}
    }
    out
}

/// ラウンドの終わりに R を更新する。`events` は (受信ノード, シェアした, 外部行動した)。
pub fn update_history(
    pop: &mut Population,
    m: u8,
    events: impl Iterator<Item = (u32, bool, bool)>,
) {
    let l = pop.rec_l;
    if l == 0 {
        return; // 推薦 off（INV-R5）
    }
    let mi = m as usize;
    for (u, int_fired, ext_fired) in events {
        let h = &mut pop.rec_hist[u as usize * l + mi];
        h[0] = h[0].saturating_add(1);
        let t = &mut pop.rec_total[mi];
        t[0] += 1;
        if int_fired {
            h[1] = h[1].saturating_add(1);
            t[1] += 1;
        }
        if ext_fired {
            h[2] = h[2].saturating_add(1);
            t[2] += 1;
        }
    }
}

/// MC 用: ラウンドの全受信を集めるだけの観測者（履歴の更新に使う）。
#[derive(Default)]
pub struct HistoryCollector(pub Vec<(u32, bool, bool)>);

impl crate::engine::Observer for HistoryCollector {
    fn on_receive(&mut self, ev: &crate::engine::ReceiveEvent) {
        self.0.push((ev.node, ev.int_fired, ev.ext_fired));
    }
}

/// ノード詳細の表の 1 行。基準では ps・px は v1 の P̂、f_cf = f、f_g = 0。
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize)]
pub struct RecRow {
    pub m: u8,
    pub received: u16,
    pub ps: f64,
    pub px: f64,
    pub f_cf: f64,
    pub f_g: f64,
    pub f: f64,
}

/// `f_g`: 直近のラウンドの社会的証明（呼び出し側が計算する。基準では使わない）。
pub fn detail(pop: &Population, u: u32, cfg: &RecommenderConfig, f_g: f64) -> Vec<RecRow> {
    if cfg.method == RecMethod::Wave {
        return Vec::new(); // Rモデルは個人のスコアを持たない（設計書 6.3）
    }
    let cf =
        (cfg.method == RecMethod::Hybrid).then(|| (item_sim(pop, cfg.min_common), base_rates(pop)));
    (0..pop.rec_l as u8)
        .map(|m| {
            let (ps, px) = match &cf {
                Some((sim, base)) => estimate_cf(pop, u, m, sim, base, cfg.prior_n0),
                None => estimate(pop, u, m, cfg.prior_n0),
            };
            let f_cf = score(ps, px, cfg.w_share);
            let (f_g, f) = match cfg.method {
                RecMethod::Baseline | RecMethod::Wave => (0.0, f_cf),
                RecMethod::Hybrid => (f_g, cfg.alpha * f_cf + (1.0 - cfg.alpha) * f_g),
            };
            RecRow {
                m,
                received: pop.rec_hist[u as usize * pop.rec_l + m as usize][0],
                ps,
                px,
                f_cf,
                f_g,
                f,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::NodeState;

    fn pop(n: usize, l: usize) -> Population {
        let mut p = Population {
            states: (0..n)
                .map(|_| NodeState::new(0.5, 0.5, -1.0, -1.0))
                .collect(),
            recv_round: vec![0; n],
            u0_int: -1.0,
            u0_ext: -1.0,
            rec_hist: Vec::new(),
            rec_total: Vec::new(),
            rec_l: 0,
        };
        p.enable_rec_history(l);
        p
    }

    #[test]
    fn t_r02_estimate_matches_hand_calculation() {
        let mut p = pop(3, 2);
        // 履歴なし・全体も 0 → 0
        assert_eq!(estimate(&p, 0, 0, 2.0), (0.0, 0.0));
        // ノード 0 が種類 0 を 4 回受信、3 回シェア、1 回外部行動。ノード 1 は 4 回受信して何もしない
        update_history(
            &mut p,
            0,
            [
                (0, true, false),
                (0, true, false),
                (0, true, true),
                (0, false, false),
            ]
            .into_iter(),
        );
        update_history(&mut p, 0, (0..4).map(|_| (1, false, false)));
        // 全体: 受信 8, シェア 3, 外部 1 → q̄s = 3/8, q̄x = 1/8
        let (ps, px) = estimate(&p, 0, 0, 2.0);
        assert_eq!(ps, (3.0 + 2.0 * 0.375) / (4.0 + 2.0));
        assert_eq!(px, (1.0 + 2.0 * 0.125) / (4.0 + 2.0));
        // 履歴のないノード 2 は全体の率
        let (ps2, px2) = estimate(&p, 2, 0, 2.0);
        assert_eq!((ps2, px2), (0.375, 0.125));
        // n0 = 0 かつ履歴なし → 全体の率（0/0 を避ける）
        assert_eq!(estimate(&p, 2, 0, 0.0), (0.375, 0.125));
        // 外部行動が済んだ人は P̂x = 0
        p.states[0].ext_done = true;
        assert_eq!(estimate(&p, 0, 0, 2.0).1, 0.0);
        // 種類 1 には履歴がない
        assert_eq!(estimate(&p, 0, 1, 2.0), (0.0, 0.0));
        assert_eq!(score(0.4, 0.2, 0.5), 0.5 * 0.4 + 0.5 * 0.2);
    }

    #[test]
    fn t_r03_threshold_boundary_and_order() {
        let mut p = pop(5, 1);
        // ノード 1: 2 回受信して 2 回シェア。ノード 3: 2 回受信して 1 回シェア
        update_history(
            &mut p,
            0,
            [
                (1, true, false),
                (1, true, false),
                (3, true, false),
                (3, false, false),
            ]
            .into_iter(),
        );
        let cfg = RecommenderConfig {
            enabled: true,
            threshold: 0.0,
            w_share: 1.0,
            prior_n0: 0.0,
            method: RecMethod::Baseline,
            ..RecommenderConfig::default()
        };
        let rcvd = vec![false; 5];
        // θ = 0 なら IP（0）以外の全員
        assert_eq!(select(&p, &rcvd, 0, 0, &cfg, None), vec![1, 2, 3, 4]);
        // n0 = 0: ノード 1 = 1.0、ノード 3 = 0.5、履歴なし = q̄s = 3/4
        // θ = 0.75 ちょうどは選ばれる（履歴なしの 2・4 と、1）
        let c = RecommenderConfig {
            threshold: 0.75,
            ..cfg.clone()
        };
        assert_eq!(select(&p, &rcvd, 0, 0, &c, None), vec![1, 2, 4]);
        // このラウンドに受信済みの人は候補にしない
        let mut got = rcvd.clone();
        got[2] = true;
        assert_eq!(select(&p, &got, 0, 0, &c, None), vec![1, 4]);
        // 同じ入力なら同じ結果
        assert_eq!(
            select(&p, &got, 0, 0, &c, None),
            select(&p, &got, 0, 0, &c, None)
        );
    }

    #[test]
    fn update_is_noop_when_disabled() {
        let mut p = pop(2, 1);
        p.rec_l = 0;
        p.rec_hist.clear();
        update_history(&mut p, 0, [(0, true, true)].into_iter());
        assert!(p.rec_hist.is_empty());
    }

    /// 3 種類の履歴を直接書く（受信, シェア, 外部）。全体の合計も合わせる。
    fn put(p: &mut Population, u: usize, m: usize, h: [u16; 3]) {
        let l = p.rec_l;
        p.rec_hist[u * l + m] = h;
        for (t, &v) in p.rec_total[m].iter_mut().zip(&h) {
            *t += v as u32;
        }
    }

    #[test]
    fn t_v02_item_sim_by_hand() {
        // 3 人 × 2 種類。全体の率: m0 シェア 3/6 = 0.5、m1 シェア 3/6 = 0.5、外部は 0
        let mut p = pop(3, 3);
        put(&mut p, 0, 0, [2, 2, 0]); // d = +0.5
        put(&mut p, 0, 1, [2, 2, 0]); // d = +0.5
        put(&mut p, 1, 0, [2, 0, 0]); // d = −0.5
        put(&mut p, 1, 1, [2, 0, 0]); // d = −0.5
        put(&mut p, 2, 0, [2, 1, 0]); // d = 0
        put(&mut p, 2, 1, [2, 1, 0]); // d = 0
        let s = item_sim(&p, 1);
        // dot = 0.25 + 0.25 = 0.5、na = nb = 0.5 → 1.0（sqrt の丸めで最下位桁がずれうる）
        assert!((s[1] - 1.0).abs() < 1e-12);
        assert_eq!(s[1], s[3], "対称");
        assert_eq!((s[0], s[4], s[8]), (1.0, 1.0, 1.0), "対角は 1");
        assert_eq!(s[2], 0.0, "m2 は誰も受け取っていない");
        // 共通の受信者が c 人未満なら 0
        assert_eq!(item_sim(&p, 4)[1], 0.0);
        // 逆向き（片方でよく反応する人が、もう片方で反応しない）は負 → 0 に切る
        let mut q = pop(2, 2);
        put(&mut q, 0, 0, [1, 1, 0]);
        put(&mut q, 0, 1, [1, 0, 0]);
        put(&mut q, 1, 0, [1, 0, 0]);
        put(&mut q, 1, 1, [1, 1, 0]);
        assert_eq!(item_sim(&q, 1)[1], 0.0);
    }

    #[test]
    fn t_v03_estimate_cf_by_hand() {
        // 要件書 3.3 の例: b_s(m3) = 0.30、b_s(m1) = 0.40、sim(m3, m1) = 0.8、λ = 2、y_s(u, m1) = 1.0
        let mut p = pop(1, 4);
        put(&mut p, 0, 1, [3, 3, 0]);
        let base = vec![(0.0, 0.0), (0.40, 0.0), (0.0, 0.0), (0.30, 0.0)];
        let mut sim = vec![0.0; 16];
        for m in 0..4 {
            sim[m * 4 + m] = 1.0;
        }
        sim[3 * 4 + 1] = 0.8;
        sim[4 + 3] = 0.8;
        let (ps, px) = estimate_cf(&p, 0, 3, &sim, &base, 2.0);
        assert_eq!(ps, 0.30 + 0.8 * (1.0 - 0.40) / (0.8 + 2.0));
        assert_eq!(px, 0.0);
        // λ = 0 で寄与する種類が無い（m2 は似た種類なし）→ 全体の率
        assert_eq!(estimate_cf(&p, 0, 2, &sim, &base, 0.0), (0.0, 0.0));
        // 0〜1 に切る
        let big = vec![(0.0, 0.0), (0.0, 0.0), (0.0, 0.0), (0.9, 0.0)];
        assert_eq!(estimate_cf(&p, 0, 3, &sim, &big, 0.0).0, 1.0);
        // 外部行動が済んだ人は P̂x = 0
        let bx = vec![(0.0, 0.5); 4];
        assert!(estimate_cf(&p, 0, 3, &sim, &bx, 2.0).1 > 0.0);
        p.states[0].ext_done = true;
        assert_eq!(estimate_cf(&p, 0, 3, &sim, &bx, 2.0).1, 0.0);
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
        let mut p = pop(4, 1);
        p.states[0].w_int = 0.05;
        assert_eq!(base_share_rate(&p, 0.125), 0.25);
        // ちょうど 0 は不成立: w·(−1) + (1 − w)·1 = 0 となる w = 0.5、s̄ = 1
        assert_eq!(base_share_rate(&pop(2, 1), 1.0), 0.0);
    }
}
