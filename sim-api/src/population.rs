//! 母集団の生成（7.6）。バッチ（runner::brute_force）と同じ乱数の使い方で、
//! 同じ `user_seed`・`user_index` から同じ母集団を作る。

use rand::SeedableRng;
use rand_distr::{Distribution, Pareto};

use crate::config::{ConfigError, DecisionParams, ScenarioConfig};
use crate::node::NodeState;
use crate::rng::SimRng;

#[derive(Clone)]
pub struct Population {
    pub states: Vec<NodeState>,
    /// そのノードが最後に受信したラウンド。0 = 未受信。
    pub recv_round: Vec<u32>,
    /// 初期効用（M4 の忘却の戻り先）。
    pub u0_int: f64,
    pub u0_ext: f64,
    /// 推薦の履歴 R: ノード × 種類（`rec_l` 個）× {受信, シェア, 外部行動}。添字 = node * rec_l + m。
    /// 推薦 off では空のまま（INV-R5）。
    pub rec_hist: Vec<[u16; 3]>,
    /// 種類ごとの全体の合計 {受信, シェア, 外部行動}（q̄ の計算用）。
    pub rec_total: Vec<[u32; 3]>,
    /// 種類の数 L（推薦 off では 0）。
    pub rec_l: usize,
}

impl Population {
    /// 推薦の履歴を確保する（推薦 on のシナリオでだけ呼ぶ）。
    pub fn enable_rec_history(&mut self, l: usize) {
        self.rec_l = l;
        self.rec_hist = vec![[0; 3]; self.states.len() * l];
        self.rec_total = vec![[0; 3]; l];
    }
}

/// runner の `DecisionParameters::sample_iter` と同じ式・評価順で重みを引く。
pub fn sample_weights(
    p: &DecisionParams,
    calibration_rounds: u32,
    n: usize,
    mean: f64,
    rng: &mut SimRng,
) -> Vec<f64> {
    let scale = 1.0 / (p.beta * (calibration_rounds as f64));
    let shape = p.gamma / (p.beta - p.gamma);
    Pareto::<f64>::new(scale, shape)
        .expect("scale / shape の妥当性は validate で検証済み")
        .sample_iter(rng)
        .take(n)
        .map(|responsiveness| (mean / (mean - p.initial_utility)).powf(responsiveness))
        .collect()
}

/// `user_index` 番目の (内部 rng, 外部 rng) の組。バッチの `user_sampling.sample_with` と同じ派生。
pub fn user_rng_pair(user_seed: u64, user_index: u32) -> (SimRng, SimRng) {
    let mut master = SimRng::seed_from_u64(user_seed);
    let mut pair = None;
    for _ in 0..=user_index {
        pair = Some((SimRng::from_rng(&mut master), SimRng::from_rng(&mut master)));
    }
    pair.expect("0..=user_index は空にならない")
}

pub fn sample_population(
    cfg: &ScenarioConfig,
    n_nodes: usize,
    mean_int: f64,
    mean_ext: f64,
) -> Result<Population, ConfigError> {
    cfg.internal_parameters
        .validate("internal_parameters", mean_int)?;
    cfg.external_parameters
        .validate("external_parameters", mean_ext)?;
    let (mut irng, mut erng) = user_rng_pair(cfg.user_seed, cfg.user_index);
    let wi = sample_weights(
        &cfg.internal_parameters,
        cfg.calibration_rounds,
        n_nodes,
        mean_int,
        &mut irng,
    );
    let we = sample_weights(
        &cfg.external_parameters,
        cfg.calibration_rounds,
        n_nodes,
        mean_ext,
        &mut erng,
    );
    let (u0_int, u0_ext) = (
        cfg.internal_parameters.initial_utility,
        cfg.external_parameters.initial_utility,
    );
    let states = wi
        .iter()
        .zip(&we)
        .map(|(&a, &b)| NodeState::new(a, b, u0_int, u0_ext))
        .collect();
    Ok(Population {
        states,
        recv_round: vec![0; n_nodes],
        u0_int,
        u0_ext,
        rec_hist: Vec::new(),
        rec_total: Vec::new(),
        rec_l: 0,
    })
}
