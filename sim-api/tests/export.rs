//! T-20: 書き出し。
#![allow(clippy::needless_range_loop, clippy::field_reassign_with_default)]

use std::sync::Arc;

use runner::brute_force::config::Config;
use sim_api::config::{MechanismConfig, ScenarioConfig};
use sim_api::export::*;
use sim_api::graph_io::*;
use sim_api::log::Choice;
use sim_api::session::{Scenario, Session};

fn fixture_path() -> String {
    format!(
        "{}/../spec/fixtures/random300.txt",
        env!("CARGO_MANIFEST_DIR")
    )
}

fn graph() -> LoadedGraph {
    let bytes = std::fs::read(fixture_path()).unwrap();
    load_graph(&GraphSource {
        bytes,
        format: GraphFormat::EdgeList,
        gzip: false,
        directed: true,
        transposed: false,
    })
    .unwrap()
}

fn meta() -> GraphMeta {
    GraphMeta {
        source_name: "random300.txt".into(),
        directed: true,
        transposed: false,
    }
}

fn run(cfg: ScenarioConfig, seq: &str) -> Session {
    let scn = Arc::new(Scenario::new(graph(), cfg).unwrap());
    let mut s = Session::new(scn, 4).unwrap();
    for c in parse_sequence(seq).unwrap() {
        s.step(c).unwrap();
    }
    s
}

#[test]
fn round_csv_columns_and_rows() {
    let s = run(ScenarioConfig::default(), "0315");
    let csv = round_rows_csv(&s);
    let mut lines = csv.lines();
    assert_eq!(
        lines.next().unwrap(),
        "round,msg,new_share,new_xact,cum_xact,received,mean_u_int,mean_u_ext,p10_u_int,p50_u_int,p90_u_int,p10_u_ext,p50_u_ext,p90_u_ext,mean_u_int_all,mean_u_ext_all"
    );
    let rows: Vec<Vec<&str>> = lines.map(|l| l.split(',').collect()).collect();
    assert_eq!(rows.len(), 4);
    assert!(rows.iter().all(|r| r.len() == 16));
    assert_eq!(rows.iter().map(|r| r[1]).collect::<String>(), "0315");
    assert_eq!(rows[0][0], "1");
}

#[test]
fn events_csv_rows_match_events() {
    let s = run(ScenarioConfig::default(), "01");
    let csv = events_csv(&s, None);
    assert!(csv.starts_with("round,wave,node,node_label,from,from_label,u_int,u_ext,kind\n"));
    let total: u32 = s.summaries().map(|x| x.n_received).sum();
    assert_eq!(csv.lines().count() as u32, total + 1);
    let only2 = events_csv(&s, Some(2));
    assert_eq!(
        only2.lines().count() as u32,
        s.summaries().nth(1).unwrap().n_received + 1
    );
}

#[test]
fn scenario_json_round_trips_and_replays() {
    let mut cfg = ScenarioConfig::default();
    cfg.mechanisms = MechanismConfig::balanced();
    let s = {
        let scn = Arc::new(Scenario::new(graph(), cfg).unwrap());
        let mut s = Session::new(scn, 4).unwrap();
        for c in parse_sequence("031a").unwrap_or_else(|_| vec![]) {
            let _ = s.step(c);
        }
        s
    };
    let json = scenario_json(&s, &meta()).unwrap();
    let rec = ScenarioRecord::from_json(&json).unwrap();
    assert_eq!(rec.version, 1);
    assert_eq!(rec.graph.hash, s.scenario().graph.hash);
    assert_eq!(rec.sequence, s.sequence_string());
    assert_eq!(rec.active_path.len(), rec.branch_tree.len());
    // 読み戻して同じ系列を再現できる
    let scn2 = Arc::new(Scenario::new(graph(), rec.config.clone()).unwrap());
    let mut s2 = Session::new(scn2, rec.ip.id).unwrap();
    for c in rec.choices().unwrap() {
        s2.step(c).unwrap();
    }
    assert_eq!(round_rows_csv(&s), round_rows_csv(&s2));
    assert_eq!(events_csv(&s, None), events_csv(&s2, None));
}

#[test]
fn sequence_parsing() {
    assert_eq!(
        parse_sequence("0af").unwrap(),
        vec![Choice::Msg(0), Choice::Msg(10), Choice::Msg(15)]
    );
    assert!(parse_sequence("0z").is_err());
    // m∅（'-'）は廃止したので不正な文字になる
    assert!(parse_sequence("0-1").is_err());
}

#[test]
fn batch_toml_only_when_mechanisms_off_and_runner_reads_it() {
    let scn = Scenario::new(graph(), ScenarioConfig::default()).unwrap();
    let toml = batch_toml(&scn, &fixture_path(), &meta(), 0, 100).unwrap();
    let cfg: Config = toml::from_str(&toml).unwrap();
    cfg.validate().unwrap();

    let mut c = ScenarioConfig::default();
    c.mechanisms.m1.enabled = true;
    let scn = Scenario::new(graph(), c).unwrap();
    assert_eq!(
        batch_toml(&scn, &fixture_path(), &meta(), 0, 100).err(),
        Some(ExportError::BatchIncompatible)
    );
}
