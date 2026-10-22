#![allow(clippy::needless_range_loop, clippy::field_reassign_with_default)]

use sim_api::config::*;
use sim_api::params::param_specs;

const DEFAULT_TOML: &str = r#"
receipt_probability = 0.5
calibration_rounds  = 5
max_rounds          = 50
user_seed           = 0
user_index          = 0
diffusion_seed      = 0
message_space = [
  { internal_stimulus = 0.20, external_stimulus = 0.20 },
  { internal_stimulus = 0.20, external_stimulus = 0.05 },
  { internal_stimulus = 0.15, external_stimulus = 0.10 },
  { internal_stimulus = 0.10, external_stimulus = 0.10 },
  { internal_stimulus = 0.10, external_stimulus = 0.15 },
  { internal_stimulus = 0.05, external_stimulus = 0.20 },
]
internal_parameters = { beta = 0.8, gamma = 0.6, initial_utility = -1.0 }
external_parameters = { beta = 0.8, gamma = 0.6, initial_utility = -1.0 }
[mean_stimulus]
mode = "auto"
[mechanisms.m1]
enabled = true
kappa = 0.05
[mechanisms.m2]
enabled = true
rho = 0.90
[mechanisms.m3]
enabled = true
sigma = 0.05
[mechanisms.m4]
enabled = true
lambda = 0.10
budget_ratio = 0.60
allow_rest = true
[mechanisms.m5]
enabled = true
eta = 0.25
n_max = 3
"#;

/// 旧形式（M2〜M5 のキーを含む）の設定も読める。廃止した機構のキーは無視される。
#[test]
fn default_toml_parses_and_validates() {
    let c = ScenarioConfig::from_toml(DEFAULT_TOML).unwrap();
    c.validate(Some(300)).unwrap();
    assert_eq!(c.mechanisms, MechanismConfig::balanced());
    assert_eq!(c.mech().kappa, 0.05);
    let (mi, me) = c.mean_stimuli().unwrap();
    assert!((mi - 0.8 / 6.0).abs() < 1e-15 && (me - 0.8 / 6.0).abs() < 1e-15);
}

#[test]
fn runner_style_config_ignores_unknown_keys() {
    let s = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../spec/fixtures/config.toml"
    ))
    .unwrap();
    let c = ScenarioConfig::from_toml(&s).unwrap();
    // num_rounds 等の未知キーは無視され、欠けたキーは既定になる
    assert_eq!(c.max_rounds, 50);
}

#[test]
fn off_gives_neutral_values() {
    assert_eq!(MechanismConfig::off().effective(), Mech::off());
    // enabled=true でも値が無効値なら effective は無効値と同じ
    let mut c = MechanismConfig::balanced();
    c.m1.kappa = 0.0;
    assert_eq!(c.effective(), Mech::off());
}

#[test]
fn presets() {
    assert!(MechanismConfig::balanced().m1.enabled);
    let s = MechanismConfig::scaled(2.0);
    assert!((s.m1.kappa - 0.10).abs() < 1e-15);
}

#[test]
fn only_m1_is_registered() {
    let ids: Vec<&str> = param_specs().iter().map(|p| p.id).collect();
    assert_eq!(ids, ["m1.kappa"]);
}

#[test]
fn validation_errors_are_japanese() {
    let mut c = ScenarioConfig::default();
    c.internal_parameters.beta = 0.6;
    c.internal_parameters.gamma = 0.6;
    assert!(
        c.validate(None)
            .unwrap_err()
            .to_string()
            .contains("gamma < beta")
    );

    let mut c = ScenarioConfig::default();
    c.receipt_probability = 1.5;
    assert!(
        c.validate(None)
            .unwrap_err()
            .to_string()
            .contains("receipt_probability")
    );

    let mut c = ScenarioConfig::default();
    c.mechanisms.m1.kappa = 0.9;
    assert!(
        c.validate(None)
            .unwrap_err()
            .to_string()
            .contains("m1.kappa")
    );

    let mut c = ScenarioConfig::default();
    c.user_seed = 1 << 53;
    assert!(c.validate(None).is_err());

    let c = ScenarioConfig::default();
    assert!(matches!(
        c.validate(Some(100_000_000)),
        Err(ConfigError::LogTooLarge { .. })
    ));

    let mut c = ScenarioConfig::default();
    c.message_space[0].internal_stimulus = 1.5;
    assert!(c.validate(None).is_err());
}

#[test]
fn registry_covers_all_numeric_params() {
    let c = MechanismConfig::balanced();
    for s in param_specs() {
        if s.min.is_some() {
            let v = sim_api::params::get_value(&c, s.id).unwrap();
            assert!((s.min.unwrap()..=s.max.unwrap()).contains(&v), "{}", s.id);
        }
    }
}
