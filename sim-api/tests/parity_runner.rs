//! T-03: 母集団が runner の `DecisionParameters::sample_iter` とビット一致する。
#![allow(clippy::needless_range_loop, clippy::field_reassign_with_default)]

use rand::{SeedableRng, rngs::SmallRng};
use runner::brute_force::config::{Config, DecisionParameters};
use sim_api::config::{DecisionParams, ScenarioConfig};
use sim_api::population::sample_population;
use sim_core::decision::DecisionAny;

fn runner_params(p: &DecisionParams) -> DecisionParameters {
    // フィールドが非公開なので、runner の設定と同じ TOML 経由で作る
    let s = format!(
        "initial_utility = {:?}\nbeta = {:?}\ngamma = {:?}\n",
        p.initial_utility, p.beta, p.gamma
    );
    toml::from_str(&s).unwrap()
}

#[test]
fn population_weights_match_runner() {
    let _ = std::any::type_name::<Config>();
    let cases = [
        (0u64, 0u32, 0.8, 0.6, 5u32, 0.8 / 6.0),
        (7, 3, 0.8, 0.6, 5, 0.8 / 6.0),
        (42, 1, 0.4, 0.3, 10, 0.125),
        (3, 2, 1.4, 1.05, 20, 0.1),
    ];
    for (seed, idx, beta, gamma, t, mean) in cases {
        let ip = DecisionParams {
            initial_utility: -1.0,
            beta,
            gamma,
        };
        let ep = DecisionParams {
            initial_utility: -0.5,
            beta: beta + 0.2,
            gamma: gamma * 0.9,
        };
        let cfg = ScenarioConfig {
            user_seed: seed,
            user_index: idx,
            calibration_rounds: t,
            internal_parameters: ip.clone(),
            external_parameters: ep.clone(),
            ..Default::default()
        };
        let n = 200;
        let pop = sample_population(&cfg, n, mean, mean).unwrap();

        // runner 側: user_sampling.sample_with と同じ派生（SmallRng）
        let mut master = SmallRng::seed_from_u64(seed);
        let mut pair = None;
        for _ in 0..=idx {
            pair = Some((
                SmallRng::from_rng(&mut master),
                SmallRng::from_rng(&mut master),
            ));
        }
        let (mut ir, mut er) = pair.unwrap();
        let iw: Vec<DecisionAny> = runner_params(&ip)
            .sample_iter::<DecisionAny>(t, n, mean, &mut ir)
            .into_iter()
            .collect();
        let ew: Vec<DecisionAny> = runner_params(&ep)
            .sample_iter::<DecisionAny>(t, n, mean, &mut er)
            .into_iter()
            .collect();
        for k in 0..n {
            assert_eq!(
                pop.states[k].w_int.to_bits(),
                iw[k].weight().to_bits(),
                "w_int {k}"
            );
            assert_eq!(
                pop.states[k].w_ext.to_bits(),
                ew[k].weight().to_bits(),
                "w_ext {k}"
            );
        }
    }
}
