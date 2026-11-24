#![allow(clippy::field_reassign_with_default, clippy::needless_range_loop)]
//! 推薦（Rモデル。`recommender-v3-design.md` 10 章）の T-W02〜T-W06・T-W11。
//! （2026-10-27 に、推薦の v1（基準）・v2（CF＋グラフ）と、そのテスト T-R04〜T-R08・T-V04〜T-V06 は廃止した。）

use std::collections::HashSet;
use std::sync::Arc;

use rand::RngCore;
use sim_api::config::{RecommenderConfig, ScenarioConfig};
use sim_api::engine::*;
use sim_api::graph_io::*;
use sim_api::log::*;
use sim_api::mc::{McMode, McSpec, run_mc};
use sim_api::rng::rng_from_seed;
use sim_api::session::{Scenario, Session};

fn graph() -> LoadedGraph {
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

fn run(c: ScenarioConfig, ip: u32, seq: &[u8]) -> Session {
    let scn = Arc::new(Scenario::new(graph(), c).unwrap());
    let mut s = Session::new(scn, ip).unwrap();
    for &m in seq {
        s.step(Choice::Msg(m)).unwrap();
    }
    s
}

#[derive(Default)]
struct Rec(Vec<ReceiveEvent>);
impl Observer for Rec {
    fn on_receive(&mut self, ev: &ReceiveEvent) {
        self.0.push(*ev);
    }
}

/// 推薦が起きる系列（m0 を多めにして反応率を育てる）。
const SEQ: [u8; 12] = [0, 0, 1, 0, 2, 0, 0, 3, 0, 1, 0, 0];

fn base_cfg() -> ScenarioConfig {
    let mut c = ScenarioConfig::default();
    c.receipt_probability = 0.4;
    c.user_seed = 3;
    c.diffusion_seed = 9;
    c
}

fn wave_cfg(eps: f64, delta: f64, mu: f64) -> ScenarioConfig {
    let mut c = base_cfg();
    c.recommender = RecommenderConfig {
        enabled: true,
        eps,
        delta,
        mu,
        ..RecommenderConfig::default()
    };
    c
}

/// 1 ラウンドを方式 wave で回す。戻り値は (結果, イベント)。
#[allow(clippy::too_many_arguments)]
fn wave_round(
    scn: &Scenario,
    pop: &mut sim_api::population::Population,
    ip: u32,
    msg_id: u8,
    rng: &mut sim_api::rng::SimRng,
    rec_rng: &mut sim_api::rng::SimRng,
    a_t: f64,
    ebar0: f64,
) -> (RoundCounts, Vec<ReceiveEvent>) {
    let mut sc = Scratch::new(scn.graph.n_nodes);
    let mut ev = Rec::default();
    let c = run_round_rec(
        &scn.graph.csr,
        scn.cfg.receipt_probability,
        pop,
        &mut sc,
        ip,
        &scn.messages[msg_id as usize],
        &scn.mech,
        Some(&scn.cfg.recommender),
        Some(WaveCtx {
            rng: rec_rng,
            a_t,
            ebar0,
        }),
        1,
        rng,
        &mut ev,
    );
    (c, ev.0)
}

#[test]
fn wave_recommendation_actually_happens() {
    let s = run(wave_cfg(1.0, 1.0, 1.0), 5, &SEQ);
    let rec: u32 = s.summaries().map(|r| r.rec_received).sum();
    assert!(rec > 0, "推薦で届いた人がいる");
    // ラウンド 1 から推薦が起きうる（履歴を使わない）
    assert!(!s.rec_wave_trace().is_empty());
}

/// T-W02: ε = 0 または δ = 0 なら、推薦 off と結果・拡散の乱数・rec_rng の状態まで一致する。
#[test]
fn t_w02_zero_budget_matches_off() {
    for (eps, delta) in [(0.0, 1.0), (1.0, 0.0)] {
        let scn = Scenario::new(graph(), wave_cfg(eps, delta, 1.0)).unwrap();
        let off = Scenario::new(graph(), base_cfg()).unwrap();
        let (mut pa, mut pb) = (scn.initial.clone(), off.initial.clone());
        let (mut ra, mut rb) = (rng_from_seed(7), rng_from_seed(7));
        let mut rec = sim_api::recommend::rec_rng(7);
        let before = rec.clone();
        for m in [0u8, 1, 0] {
            let (a, ea) = wave_round(&scn, &mut pa, 5, m, &mut ra, &mut rec, scn.ebar0, scn.ebar0);
            let mut sb = Scratch::new(off.graph.n_nodes);
            let mut eb = Rec::default();
            let b = run_round(
                &off.graph.csr,
                off.cfg.receipt_probability,
                &mut pb,
                &mut sb,
                5,
                &off.messages[m as usize],
                &off.mech,
                1,
                &mut rb,
                &mut eb,
            );
            assert_eq!(
                (a.n_received, a.n_share, a.n_xact, a.n_waves, a.rec_received),
                (b.n_received, b.n_share, b.n_xact, b.n_waves, 0)
            );
            assert_eq!(ea.len(), eb.0.len());
            for (x, y) in ea.iter().zip(&eb.0) {
                assert_eq!(
                    (x.node, x.wave, x.u_int.to_bits()),
                    (y.node, y.wave, y.u_int.to_bits())
                );
            }
        }
        assert_eq!(ra.next_u64(), rb.next_u64(), "拡散の乱数が同じ");
        assert_eq!(rec, before, "rec_rng を引かない");
    }
}

/// T-W03: 推薦の宛先は 2 ホップ先だけ。0 → 1 → 2 で、1 は受信してもシェアしないので、フォロー経由は 1 までしか届かない。
/// 推薦は 0 の 2 ホップ先の 2 にだけ届く。受信済み・IP には届かず、空振りを数える。
#[test]
fn t_w03_two_hop_reaches_beyond_a_non_sharer_without_double_receipt() {
    let g = load_graph(&GraphSource {
        bytes: b"0 1\n1 2\n".to_vec(),
        format: GraphFormat::EdgeList,
        gzip: false,
        directed: true,
        transposed: false,
    })
    .unwrap();
    let mut c = wave_cfg(5.0, 1.0, 1.0);
    c.receipt_probability = 1.0;
    let scn = Scenario::new(g, c).unwrap();
    for seed in 0..30 {
        // 受け取れば必ず両方が成立する母集団（w = 0 なので u' = 刺激 > 0）。ただしノード 1 は内部の w = 1 でシェアしない
        let mut pop = scn.initial.clone();
        for (i, s) in pop.states.iter_mut().enumerate() {
            *s = sim_api::node::NodeState::new(if i == 1 { 1.0 } else { 0.0 }, 0.0, -1.0, -1.0);
        }
        let mut rng = rng_from_seed(seed);
        let mut rec = sim_api::recommend::rec_rng(seed);
        let (k, ev) = wave_round(&scn, &mut pop, 0, 0, &mut rng, &mut rec, 1.0, 1.0);
        let nodes: HashSet<u32> = ev.iter().map(|e| e.node).collect();
        assert_eq!(nodes.len(), ev.len(), "seed {seed}: 1 人 1 回");
        assert!(!nodes.contains(&0), "IP は受信しない");
        assert_eq!(k.n_xact, k.n_received);
        assert_eq!(k.rec_xact, k.rec_received);
        assert!(
            k.rec_miss >= 3,
            "枠が宛先より多いので空振りが出る: {}",
            k.rec_miss
        );
        // フォロー経由で 1、推薦で 2 に 1 回だけ届く
        assert_eq!(nodes, HashSet::from([1, 2]), "seed {seed}");
        assert_eq!(k.rec_received, 1);
        for e in &ev {
            assert_eq!(e.via == VIA_REC, e.from == FROM_REC);
        }
    }
}

/// T-W04: 拡散の乱数を引くのはフォロー経由の辺の抽選だけ（推薦は rec_rng だけを引く）。
/// 引いた回数 = IP と、次のウェーブへ送った人の出次数の和。
#[test]
fn t_w04_diffusion_rng_only_for_follow_edges() {
    let scn = Scenario::new(graph(), wave_cfg(2.0, 1.0, 1.0)).unwrap();
    let csr = &scn.graph.csr;
    let ip = 5;
    for seed in 0..10u64 {
        let mut pop = scn.initial.clone();
        let mut rng = rng_from_seed(seed);
        let mut rec = sim_api::recommend::rec_rng(seed);
        let (k, ev) = wave_round(
            &scn, &mut pop, ip, 0, &mut rng, &mut rec, scn.ebar0, scn.ebar0,
        );
        let draws: usize = csr.out_degree(ip)
            + ev.iter()
                .filter(|e| e.enqueued)
                .map(|e| csr.out_degree(e.node))
                .sum::<usize>();
        let mut expect = rng_from_seed(seed);
        for _ in 0..draws {
            let _ = rand::Rng::random::<f64>(&mut expect);
        }
        assert_eq!(rng.next_u64(), expect.next_u64(), "seed {seed}");
        if seed == 0 {
            assert!(k.rec_received > 0, "推薦が起きる設定であること");
        }
    }
}

/// T-W06: R3 ありで、巻き戻し・分岐・MC がビット一致する。
#[test]
fn t_w06_rewind_and_mc_are_bit_identical() {
    let c = wave_cfg(1.0, 1.0, 0.5);
    let snap = |s: &Session| -> (Vec<String>, Vec<u64>, u64, String) {
        (
            s.summaries()
                .map(|x| serde_json::to_string(x).unwrap())
                .collect(),
            s.population()
                .states
                .iter()
                .map(|x| x.u_int.to_bits())
                .collect(),
            s.rec_prior().to_bits(),
            serde_json::to_string(s.rec_wave_trace()).unwrap(),
        )
    };
    let full = run(c.clone(), 5, &SEQ);
    let want = snap(&full);
    assert_ne!(
        full.rec_prior(),
        full.scenario().ebar0,
        "R3 の a_t が動いている"
    );
    for k in [0u32, 3, 7, 8, 9, 11] {
        let mut s = run(c.clone(), 5, &SEQ);
        s.rewind_to(k).unwrap();
        let reference = run(c.clone(), 5, &SEQ[..k as usize]);
        let (a, b) = (snap(&s), snap(&reference));
        assert_eq!((&a.0, &a.1, a.2), (&b.0, &b.1, b.2), "k={k}: 巻き戻し直後");
        for &m in &SEQ[k as usize..] {
            s.step(Choice::Msg(m)).unwrap();
        }
        assert_eq!(snap(&s), want, "k={k}: 再生後");
    }
    let scn = Scenario::new(graph(), c).unwrap();
    let seq: Vec<Choice> = SEQ.iter().map(|&m| Choice::Msg(m)).collect();
    let spec = McSpec {
        trials: 20,
        mode: McMode::Independent { seed: 4 },
    };
    let a = run_mc(&scn, 5, &seq, &spec).unwrap();
    let b = run_mc(&scn, 5, &seq, &spec).unwrap();
    assert_eq!(a.total_xact_mean.to_bits(), b.total_xact_mean.to_bits());
}

/// T-W11: 受信の経路・送信元・ウェーブ・イベントの不変条件と、ログ・累積の記録（推薦で受信した人の累積）。
#[test]
fn t_w11_via_from_wave_and_event_invariants() {
    let s = run(wave_cfg(1.0, 1.0, 1.0), 5, &SEQ);
    let mut saw_rec = false;
    let mut saw_chain = false;
    for r in 1..=s.round() {
        let ev = s.event_columns(r).unwrap();
        let mut seen = HashSet::new();
        let first_rec_wave = (0..ev.len())
            .filter(|&k| ev.flags[k] & EV_REC != 0)
            .map(|k| ev.wave[k])
            .min();
        for k in 0..ev.len() {
            assert!(seen.insert(ev.node[k]), "1 ラウンドで同じノードは 1 件まで");
            assert_ne!(ev.node[k], 5, "IP のイベントは無い");
            if ev.flags[k] & EV_ENQ != 0 {
                assert!(ev.flags[k] & EV_INT != 0);
            }
            if ev.flags[k] & EV_REC != 0 {
                saw_rec = true;
                assert_eq!(ev.from[k], FROM_REC);
            }
            if ev.flags[k] & EV_RECCHAIN != 0 {
                saw_chain = true;
                assert_ne!(ev.from[k], FROM_REC);
                // 推薦由来は、推薦で受信した人（またはその下流）からフォロー経由で届くので、最初の推薦より後のウェーブ
                assert!(ev.wave[k] > first_rec_wave.expect("推薦由来の前に推薦がある"));
            }
        }
        // ログのフラグとも一致
        let flags = s.log().row_flags(r as usize);
        for k in 0..ev.len() {
            let f = flags[ev.node[k] as usize];
            assert_eq!(f & F_REC_NOW != 0, ev.flags[k] & EV_REC != 0);
            // 推薦で受け取ったら累積の印も立つ（一度立ったら消えない）
            if ev.flags[k] & EV_REC != 0 {
                assert!(f & F_EVER_REC != 0);
            }
        }
    }
    assert!(saw_rec && saw_chain);
    // 累積: ever_flags の推薦のビットと cum_rec が、これまでに推薦で受け取った人と一致する
    let mut ever_rec = HashSet::new();
    for r in 1..=s.round() {
        let ev = s.event_columns(r).unwrap();
        for k in 0..ev.len() {
            if ev.flags[k] & EV_REC != 0 {
                ever_rec.insert(ev.node[k]);
            }
        }
        assert_eq!(
            s.summaries().nth(r as usize - 1).unwrap().cum_rec as usize,
            ever_rec.len()
        );
    }
    let ef = s.ever_flags();
    for (u, &f) in ef.iter().enumerate() {
        assert_eq!(
            f & EVER_FLAG_REC != 0,
            ever_rec.contains(&(u as u32)),
            "ノード {u}"
        );
        assert_eq!(
            f & !EVER_FLAGS_MASK,
            0,
            "ever_flags に「このラウンド」の印が混ざらない"
        );
    }
    // ノード詳細の受信元
    let d = (0..300)
        .map(|u| s.node_detail(u))
        .find(|d| d.received.iter().any(|x| x.via == 1))
        .unwrap();
    let r = d.received.iter().find(|x| x.via == 1).unwrap();
    assert_eq!((r.from, r.from_label), (FROM_REC, -1));
}
