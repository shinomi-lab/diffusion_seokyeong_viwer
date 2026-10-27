//! モンテカルロ（7.12）。同じ系列を N 回回す。イベントは記録しない（`NoTrace`）。

use rand::SeedableRng;
use serde::Serialize;

use crate::engine::{NoTrace, Scratch, run_round};
use crate::groups::Band;
use crate::log::{Choice, quantile_sorted};
use crate::node::EVER_RECV;
use crate::rng::SimRng;
use crate::session::{Scenario, SessionError};

#[derive(Clone, Copy, Debug)]
pub enum McMode {
    /// 1 本の rng を試行をまたいで使い続ける（バッチ照合用）。
    Sequential { seed: u64 },
    /// 試行ごとに独立な rng（並列・画面用）。
    Independent { seed: u64 },
}

#[derive(Clone, Copy, Debug)]
pub struct McSpec {
    pub trials: u32,
    pub mode: McMode,
}

/// 1 試行のラウンド別の値。
#[derive(Clone, Debug, Default)]
pub struct TrialSeries {
    pub new_xact: Vec<u32>,
    pub new_share: Vec<u32>,
    /// 受信済みノードの平均効用。
    pub mean_u_int: Vec<f32>,
    pub mean_u_ext: Vec<f32>,
    /// 推薦で受け取った人数（このラウンド）と、推薦で受け取ったことのある人数（累積）。推薦 off では 0。
    pub new_rec: Vec<u32>,
    pub cum_rec: Vec<u32>,
}

impl TrialSeries {
    fn clear(&mut self) {
        self.new_rec.clear();
        self.cum_rec.clear();
        self.new_xact.clear();
        self.new_share.clear();
        self.mean_u_int.clear();
        self.mean_u_ext.clear();
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct McResult {
    pub rounds: u32,
    pub trials: u32,
    pub new_xact: Band,
    pub cum_xact: Band,
    pub new_share: Band,
    pub mean_u_int: Band,
    pub mean_u_ext: Band,
    /// 推薦で受け取った人数（このラウンド）・推薦で受け取ったことのある人数（累積）。
    pub new_rec: Band,
    pub cum_rec: Band,
    /// 系列全体の合計の平均（バッチの mean_num_xact / mean_num_share と同じ量）。
    pub total_xact_mean: f64,
    pub total_share_mean: f64,
}

/// 系列が `Session::step` の検査を通ることを確かめる。
pub fn validate_sequence(scn: &Scenario, ip: u32, seq: &[Choice]) -> Result<(), SessionError> {
    if ip as usize >= scn.graph.n_nodes {
        return Err(SessionError::InvalidIp(ip));
    }
    for (r, c) in seq.iter().enumerate() {
        if r as u32 >= scn.cfg.max_rounds {
            return Err(SessionError::RoundLimit);
        }
        let Choice::Msg(id) = *c;
        if id as usize >= scn.messages.len() {
            return Err(SessionError::InvalidMessage(id));
        }
    }
    Ok(())
}

/// 1 試行。`scn.initial` から母集団を戻して系列を回す。系列は検証済みであること。
pub fn run_trial(
    scn: &Scenario,
    ip: u32,
    seq: &[Choice],
    rng: &mut SimRng,
    scratch: &mut Scratch,
    out: &mut TrialSeries,
) {
    out.clear();
    let mut pop = scn.initial.clone();
    let mech = scn.mech;
    // Rモデル: 推薦専用の乱数は試行ごとに作る。拡散の乱数の複製から種を取るので、拡散の乱数は消費しない（INV-W2）。
    let rc = &scn.cfg.recommender;
    let is_wave = rc.enabled && rc.method == crate::config::RecMethod::Wave;
    let mut rec_rng = crate::recommend::rec_rng(rand::RngCore::next_u64(&mut rng.clone()));
    let mut rec_a = scn.ebar0;
    for (r, &c) in seq.iter().enumerate() {
        let round = r as u32 + 1;
        let Choice::Msg(id) = c;
        let k = if scn.cfg.recommender.enabled {
            // 推薦 on: セッションと同じく、ラウンドの終わりに履歴を更新する（INV-R5）
            let mut col = crate::recommend::HistoryCollector::default();
            let k = crate::engine::run_round_rec(
                &scn.graph.csr,
                scn.cfg.receipt_probability,
                &mut pop,
                scratch,
                ip,
                id,
                &scn.messages[id as usize],
                &mech,
                Some(&scn.cfg.recommender),
                is_wave.then_some(crate::engine::WaveCtx {
                    rng: &mut rec_rng,
                    a_t: rec_a,
                    ebar0: scn.ebar0,
                }),
                round,
                rng,
                &mut col,
            );
            crate::recommend::update_history(&mut pop, id, col.0.into_iter());
            if is_wave && rc.eps * rc.delta > 0.0 {
                rec_a = rc.mu * rec_a + (1.0 - rc.mu) * k.e_hat_end;
            }
            k
        } else {
            run_round(
                &scn.graph.csr,
                scn.cfg.receipt_probability,
                &mut pop,
                scratch,
                ip,
                id,
                &scn.messages[id as usize],
                &mech,
                round,
                rng,
                &mut NoTrace,
            )
        };
        let (xact, share) = (k.n_xact, k.n_share);
        let (mut si, mut se, mut n, mut cr) = (0.0f64, 0.0f64, 0u32, 0u32);
        for s in &pop.states {
            if s.flags & crate::node::EVER_REC != 0 {
                cr += 1;
            }
            if s.flags & EVER_RECV != 0 {
                si += s.u_int;
                se += s.u_ext;
                n += 1;
            }
        }
        out.new_rec.push(k.rec_received);
        out.cum_rec.push(cr);
        out.new_xact.push(xact);
        out.new_share.push(share);
        out.mean_u_int
            .push(if n == 0 { 0.0 } else { (si / n as f64) as f32 });
        out.mean_u_ext
            .push(if n == 0 { 0.0 } else { (se / n as f64) as f32 });
    }
}

fn band_of(per_trial: &[Vec<f32>], rounds: usize) -> Band {
    let mut b = Band::default();
    let mut col = Vec::with_capacity(per_trial.len());
    for r in 0..rounds {
        col.clear();
        col.extend(per_trial.iter().map(|t| t[r]));
        let mean = (col.iter().map(|&x| x as f64).sum::<f64>() / col.len() as f64) as f32;
        col.sort_by(f32::total_cmp);
        b.mean.push(mean);
        b.p10.push(quantile_sorted(&col, 0.1));
        b.p50.push(quantile_sorted(&col, 0.5));
        b.p90.push(quantile_sorted(&col, 0.9));
    }
    b
}

/// 試行ごとの結果から集計する（desktop が rayon で `run_trial` を並列に呼んだ結果もここへ渡せる）。
pub fn summarize(trials: &[TrialSeries], rounds: usize) -> McResult {
    let t = trials.len();
    let f = |v: &Vec<u32>| v.iter().map(|&x| x as f32).collect::<Vec<f32>>();
    let new_x: Vec<Vec<f32>> = trials.iter().map(|s| f(&s.new_xact)).collect();
    let new_s: Vec<Vec<f32>> = trials.iter().map(|s| f(&s.new_share)).collect();
    let cum_x: Vec<Vec<f32>> = new_x
        .iter()
        .map(|v| {
            let mut acc = 0.0f32;
            v.iter()
                .map(|&x| {
                    acc += x;
                    acc
                })
                .collect()
        })
        .collect();
    let (mut tx, mut ts) = (0u64, 0u64);
    for s in trials {
        tx += s.new_xact.iter().map(|&x| x as u64).sum::<u64>();
        ts += s.new_share.iter().map(|&x| x as u64).sum::<u64>();
    }
    let ui: Vec<Vec<f32>> = trials.iter().map(|s| s.mean_u_int.clone()).collect();
    let ue: Vec<Vec<f32>> = trials.iter().map(|s| s.mean_u_ext.clone()).collect();
    McResult {
        rounds: rounds as u32,
        trials: t as u32,
        new_xact: band_of(&new_x, rounds),
        cum_xact: band_of(&cum_x, rounds),
        new_share: band_of(&new_s, rounds),
        mean_u_int: band_of(&ui, rounds),
        mean_u_ext: band_of(&ue, rounds),
        new_rec: band_of(
            &trials.iter().map(|s| f(&s.new_rec)).collect::<Vec<_>>(),
            rounds,
        ),
        cum_rec: band_of(
            &trials.iter().map(|s| f(&s.cum_rec)).collect::<Vec<_>>(),
            rounds,
        ),
        // バッチと同じく整数で合計してから 1 度だけ割る
        total_xact_mean: tx as f64 / t as f64,
        total_share_mean: ts as f64 / t as f64,
    }
}

/// 試行 k の独立な rng（Independent）。
pub fn independent_rng(seed: u64, k: u32) -> SimRng {
    SimRng::seed_from_u64(
        seed.wrapping_add(k as u64)
            .wrapping_mul(0x9E37_79B9_7F4A_7C15),
    )
}

/// 与えた 1 本の rng を試行をまたいで使い続ける（Sequential）。バッチの 1 行の再現に使う。
pub fn run_mc_with_rng(
    scn: &Scenario,
    ip: u32,
    seq: &[Choice],
    trials: u32,
    rng: &mut SimRng,
) -> Result<McResult, SessionError> {
    validate_sequence(scn, ip, seq)?;
    let mut scratch = Scratch::new(scn.graph.n_nodes);
    let mut all = Vec::with_capacity(trials as usize);
    for _ in 0..trials {
        let mut t = TrialSeries::default();
        run_trial(scn, ip, seq, rng, &mut scratch, &mut t);
        all.push(t);
    }
    Ok(summarize(&all, seq.len()))
}

pub fn run_mc(
    scn: &Scenario,
    ip: u32,
    seq: &[Choice],
    spec: &McSpec,
) -> Result<McResult, SessionError> {
    match spec.mode {
        McMode::Sequential { seed } => {
            let mut rng = SimRng::seed_from_u64(seed);
            run_mc_with_rng(scn, ip, seq, spec.trials, &mut rng)
        }
        McMode::Independent { seed } => {
            validate_sequence(scn, ip, seq)?;
            let mut scratch = Scratch::new(scn.graph.n_nodes);
            let mut all = Vec::with_capacity(spec.trials as usize);
            for k in 0..spec.trials {
                let mut rng = independent_rng(seed, k);
                let mut t = TrialSeries::default();
                run_trial(scn, ip, seq, &mut rng, &mut scratch, &mut t);
                all.push(t);
            }
            Ok(summarize(&all, seq.len()))
        }
    }
}

/// runner::brute_force の乱数の割り当てを再現する。
/// 親は `SimRng::seed_from_u64(diffusion_seed)`。バッチは (us_id → IP の位置 → 系列番号) の三重ループで
/// 系列ごとに `from_rng(&mut 親)` で子を 1 つずつ派生させる。したがって第 k 番目の子が対象の行の乱数:
///   k = (us_id * num_ips + ip_pos) * num_seqs + seq_index
/// （チャンク分割は派生の順序を変えない）。
pub fn batch_row_rng(
    diffusion_seed: u64,
    num_ips: usize,
    num_seqs: u64,
    us_id: usize,
    ip_pos: usize,
    seq_index: u64,
) -> SimRng {
    let k = (us_id as u64 * num_ips as u64 + ip_pos as u64) * num_seqs + seq_index;
    let mut parent = SimRng::seed_from_u64(diffusion_seed);
    for _ in 0..k {
        let _ = SimRng::from_rng(&mut parent);
    }
    SimRng::from_rng(&mut parent)
}
