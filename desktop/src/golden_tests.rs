//! P2-4 / P2-5 の DoD: Web 版と同じ設定・シードで、desktop が表示に使う値がゴールデン（`spec/fixtures/golden_*.json`、
//! ネイティブで作成し WASM でも一致を確認済み。T-17）と一致する。
//! desktop と同じ経路（`sim-api` の `Scenario`/`Session` を直接呼ぶ。`playback::state_at_wave` で表示状態を作る）で再計算する。
//! また、MC を別スレッドで回した結果が、同じ試行を 1 本ずつ回した結果とビット一致する。

use std::sync::Arc;

use serde_json::{Value, json};
use sim_api::config::ScenarioConfig;
use sim_api::graph_io::{GraphFormat, GraphSource, load_graph};
use sim_api::log::{Choice, EventColumns, IntMode};
use sim_api::session::{Scenario, Session};
use sim_wasm::golden::{GoldenCase, cases};

use crate::playback::state_at_wave;

const GRAPH: &[u8] = include_bytes!("../../spec/fixtures/random300.txt");

fn arr<T: Into<Value> + Copy>(v: &[T]) -> Value {
    Value::Array(v.iter().map(|&x| x.into()).collect())
}

fn events_value(e: &EventColumns) -> Value {
    json!({
        "wave": arr(&e.wave), "node": arr(&e.node), "from": arr(&e.from), "n_arr": arr(&e.n_arr),
        "u_int": arr(&e.u_int), "u_ext": arr(&e.u_ext), "flags": arr(&e.flags),
    })
}

/// desktop の `App::send` と同じ流れ（`Session::step` → 要約・イベント・everFlags）で、ラウンドごとの値を作る。
/// `kinds` は desktop の表示ロジック（`state_at_wave` の最終ウェーブ）で作る。
fn desktop_rounds(c: &GoldenCase) -> Vec<Value> {
    let g = load_graph(&GraphSource {
        bytes: c.graph.to_vec(),
        format: GraphFormat::EdgeList,
        gzip: false,
        directed: true,
        transposed: false,
    })
    .unwrap();
    let cfg = ScenarioConfig::from_json(c.config_json).unwrap();
    let scn = Arc::new(Scenario::new(g, cfg).unwrap());
    let mut s = Session::new(scn, c.ip).unwrap();
    let mut ever_before = s.ever_flags();
    let mut out = Vec::new();
    for &ch in c.sequence {
        let choice = Choice::from_i32(ch).unwrap();
        let o = s.step(choice).unwrap();
        let summary: Value =
            serde_json::from_str(&serde_json::to_string(&o.summary).unwrap()).unwrap();
        let events = o.events.clone();
        let last = o.summary.n_waves as i32 - 1;
        let ever = s.ever_flags();
        let shown = state_at_wave(&ever_before, &events, last, IntMode::Cumulative, c.ip);
        // 表示用の kinds は Session::kinds と一致する（画面は常にこちらを使う）
        let engine: Vec<u8> = s
            .kinds(IntMode::Cumulative)
            .into_iter()
            .map(|k| k as u8)
            .collect();
        assert_eq!(
            shown.kinds, engine,
            "state_at_wave の最終ウェーブが Session::kinds と一致する"
        );
        out.push(json!({
            "summary": summary,
            "events": events_value(&events),
            "ever_flags": arr(&ever),
            "kinds": arr(&shown.kinds),
        }));
        ever_before = ever;
    }
    out
}

fn check(name: &str, golden: &str) {
    let c = cases(GRAPH).into_iter().find(|c| c.name == name).unwrap();
    let got = desktop_rounds(&c);
    let want: Value = serde_json::from_str(golden).unwrap();
    let want = want["rounds"].as_array().unwrap();
    assert_eq!(got.len(), want.len());
    for (i, (a, b)) in got.iter().zip(want).enumerate() {
        for key in ["summary", "events", "ever_flags", "kinds"] {
            assert_eq!(a[key], b[key], "{name} ラウンド {} の {key}", i + 1);
        }
    }
}

#[test]
fn golden_off_matches() {
    check("off", include_str!("../../spec/fixtures/golden_off.json"));
}

#[test]
fn golden_balanced_matches() {
    check(
        "balanced",
        include_str!("../../spec/fixtures/golden_balanced.json"),
    );
}

#[test]
fn threaded_mc_matches_single_threaded_trials() {
    use sim_api::engine::Scratch;
    use sim_api::mc::{TrialSeries, independent_rng, run_trial, summarize};
    let g = load_graph(&GraphSource {
        bytes: GRAPH.to_vec(),
        format: GraphFormat::EdgeList,
        gzip: false,
        directed: true,
        transposed: false,
    })
    .unwrap();
    let scn = Arc::new(
        Scenario::new(
            g,
            ScenarioConfig {
                receipt_probability: 0.6,
                ..Default::default()
            },
        )
        .unwrap(),
    );
    let seq: Vec<Choice> = [0u8, 3, 5, 1, 0].iter().map(|&i| Choice::Msg(i)).collect();
    let rx = crate::worker::spawn_mc(
        eframe::egui::Context::default(),
        Arc::clone(&scn),
        4,
        seq.clone(),
        64,
        9,
    )
    .unwrap();
    let threaded = rx
        .recv_timeout(std::time::Duration::from_secs(60))
        .expect("MC が終わらない");
    let mut sc = Scratch::new(scn.graph.n_nodes);
    let all: Vec<TrialSeries> = (0..64)
        .map(|k| {
            let mut t = TrialSeries::default();
            run_trial(&scn, 4, &seq, &mut independent_rng(9, k), &mut sc, &mut t);
            t
        })
        .collect();
    let single = summarize(&all, seq.len());
    assert_eq!(
        threaded.total_xact_mean.to_bits(),
        single.total_xact_mean.to_bits()
    );
    assert_eq!(
        threaded.total_share_mean.to_bits(),
        single.total_share_mean.to_bits()
    );
    assert_eq!(threaded.cum_xact.mean, single.cum_xact.mean);
}
