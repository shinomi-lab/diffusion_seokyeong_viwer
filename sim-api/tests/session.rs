//! T-11 / T-13: セッション、M4（休止・予算・忘却）、ログと分類。
#![allow(clippy::needless_range_loop, clippy::field_reassign_with_default)]

use std::sync::Arc;

use sim_api::config::{MechanismConfig, ScenarioConfig};
use sim_api::graph_io::*;
use sim_api::log::*;
use sim_api::session::*;

fn fixture() -> LoadedGraph {
    let bytes = std::fs::read(format!(
        "{}/../spec/fixtures/random300.txt",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();
    load_graph(&GraphSource {
        bytes,
        format: GraphFormat::EdgeList,
        gzip: false,
        directed: true,
        transposed: false,
    })
    .unwrap()
}

fn session(cfg: ScenarioConfig) -> Session {
    let scn = Arc::new(Scenario::new(fixture(), cfg).unwrap());
    Session::new(scn, 0).unwrap()
}

#[test]
fn message_and_round_limits() {
    let mut s = session(ScenarioConfig::default());
    assert_eq!(
        s.step(Choice::Msg(6)).err(),
        Some(SessionError::InvalidMessage(6))
    );
    // 失敗したステップではラウンドが進まない
    assert_eq!(s.round(), 0);

    let mut c = ScenarioConfig::default();
    c.max_rounds = 2;
    let mut s = session(c);
    s.step(Choice::Msg(0)).unwrap();
    s.step(Choice::Msg(0)).unwrap();
    assert_eq!(s.step(Choice::Msg(0)).err(), Some(SessionError::RoundLimit));

    let scn = Arc::new(Scenario::new(fixture(), ScenarioConfig::default()).unwrap());
    assert!(matches!(
        Session::new(scn, 300),
        Err(SessionError::InvalidIp(300))
    ));
}

#[test]
fn session_is_deterministic() {
    let run = || {
        let mut s = session(ScenarioConfig {
            mechanisms: MechanismConfig::balanced(),
            ..Default::default()
        });
        for r in 0..3 {
            s.step(Choice::Msg(r)).unwrap();
        }
        s.summaries()
            .map(|x| (x.n_received, x.n_share, x.new_xact))
            .collect::<Vec<_>>()
    };
    assert_eq!(run(), run());
}

#[test]
fn kind_of_modes() {
    let f = F_EVER_RECV | F_EVER_INT;
    assert_eq!(kind_of(0, IntMode::Cumulative, false), NodeKind::Unreceived);
    assert_eq!(
        kind_of(F_EVER_RECV, IntMode::Cumulative, false),
        NodeKind::ReceivedOnly
    );
    assert_eq!(kind_of(f, IntMode::Cumulative, false), NodeKind::Internal);
    assert_eq!(
        kind_of(f, IntMode::ThisRoundOnly, false),
        NodeKind::ReceivedOnly
    );
    assert_eq!(
        kind_of(f | F_INT_NOW, IntMode::ThisRoundOnly, false),
        NodeKind::Internal
    );
    assert_eq!(
        kind_of(F_EVER_RECV | F_EVER_EXT, IntMode::ThisRoundOnly, false),
        NodeKind::External
    );
    assert_eq!(
        kind_of(f | F_EVER_EXT, IntMode::Cumulative, false),
        NodeKind::Both
    );
    assert_eq!(
        kind_of(f | F_EVER_EXT, IntMode::ThisRoundOnly, false),
        NodeKind::External
    );
    assert_eq!(kind_of(0, IntMode::Cumulative, true), NodeKind::Ip);
}

#[test]
fn log_flags_are_consistent_with_cumulative_events() {
    let mut cfg = ScenarioConfig::default();
    cfg.receipt_probability = 0.8;
    cfg.mechanisms = MechanismConfig::balanced();
    let mut s = session(cfg);
    for r in 0..6u8 {
        s.step(Choice::Msg(r % 6)).unwrap();
    }
    let log = s.log();
    assert_eq!(log.n_rows(), 7);
    let mut ever = vec![0u8; 300];
    let mut ext_count = vec![0; 300];
    for r in 1..log.n_rows() {
        let row = log.row_flags(r);
        for k in 0..300 {
            let f = row[k];
            if f & F_RECV_NOW != 0 {
                ever[k] |= F_EVER_RECV;
            }
            if f & F_INT_NOW != 0 {
                ever[k] |= F_EVER_INT;
            }
            if f & F_EXT_NOW != 0 {
                ever[k] |= F_EVER_EXT;
                ext_count[k] += 1;
            }
            assert_eq!(
                f & (F_EVER_RECV | F_EVER_INT | F_EVER_EXT),
                ever[k],
                "r{r} node{k}"
            );
            // 内部・外部が成立したなら受信したラウンドである
            if f & (F_INT_NOW | F_EXT_NOW) != 0 {
                assert!(f & F_RECV_NOW != 0);
            }
        }
    }
    assert!(ext_count.iter().all(|&c| c <= 1), "外部行動は生涯 1 回");
    // kinds は最終行の分類と一致
    let kinds = s.kinds(IntMode::Cumulative);
    assert_eq!(kinds[0], NodeKind::Ip);
    let ef = s.ever_flags();
    for k in 1..300 {
        assert_eq!(ef[k], ever[k] >> 3);
    }
    // 行 0 は初期状態
    assert!(log.row_flags(0).iter().all(|&f| f == 0));
}

#[test]
fn utilities_are_never_clamped() {
    let mut cfg = ScenarioConfig::default();
    cfg.receipt_probability = 1.0;
    for m in cfg.message_space.iter_mut() {
        m.internal_stimulus = 0.9;
        m.external_stimulus = 0.9;
    }
    cfg.mechanisms.m1.enabled = true;
    cfg.mechanisms.m1.kappa = 0.5;
    let mut s = session(cfg);
    for _ in 0..8 {
        s.step(Choice::Msg(0)).unwrap();
    }
    let max = (0..s.log().n_rows())
        .flat_map(|r| s.log().row_u_int(r).iter().copied())
        .fold(f32::MIN, f32::max);
    assert!(max > 1.0, "効用が 1 を超えて残る: {max}");
    let k = (0..300usize)
        .find(|&k| s.log().row_u_int(8)[k] > 1.0)
        .unwrap();
    let d = s.node_detail(k as u32);
    assert!(d.u_int.iter().any(|&v| v > 1.0));
    assert_eq!(d.u_int.len(), 9);
    assert!(d.ext_round.is_some());
    assert!(!d.received.is_empty());
}

#[test]
fn round_summary_stats() {
    let mut s = session(ScenarioConfig::default());
    let out = s.step(Choice::Msg(0)).unwrap();
    let sm = &out.summary;
    assert_eq!(sm.n_received as usize, out.events.len());
    assert_eq!(sm.recv.n, sm.n_received);
    assert_eq!(sm.all.n, 299);
    assert_eq!(sm.cum_xact, sm.new_xact);
    // 分位点は昇順の添字 round((n-1)q)
    assert_eq!(quantile_sorted(&[1.0, 2.0, 3.0, 4.0, 5.0], 0.5), 3.0);
    assert_eq!(quantile_sorted(&[1.0, 2.0], 0.9), 2.0);
}
