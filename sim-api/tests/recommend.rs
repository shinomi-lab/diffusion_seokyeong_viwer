#![allow(clippy::field_reassign_with_default, clippy::needless_range_loop)]
//! 推薦（`recommender-design.md`）の T-R04〜T-R08（方式「基準」）と、v2（`recommender-v2-design.md`）の T-V04〜T-V06。

use std::collections::HashSet;
use std::sync::Arc;

use rand::RngCore;
use sim_api::config::{RecMethod, RecommenderConfig, ScenarioConfig};
use sim_api::csr::Csr;
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

fn cfg_m(rec: bool, threshold: f64, method: RecMethod) -> ScenarioConfig {
    let mut c = ScenarioConfig::default();
    c.receipt_probability = 0.4;
    c.user_seed = 3;
    c.diffusion_seed = 9;
    c.recommender = RecommenderConfig {
        enabled: rec,
        threshold,
        method,
        ..RecommenderConfig::default()
    };
    c
}

/// v1 のテスト用（方式「基準」）。
fn cfg(rec: bool, threshold: f64) -> ScenarioConfig {
    cfg_m(rec, threshold, RecMethod::Baseline)
}

/// m0 を繰り返して全体の率 q̄ を育て、推薦が起きる系列。
const SEQ: [u8; 12] = [0, 0, 1, 0, 2, 0, 0, 3, 0, 1, 0, 0];

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

#[test]
fn recommendation_actually_happens() {
    let s = run(cfg(true, 0.05), 5, &SEQ);
    let rec: u32 = s.summaries().map(|r| r.rec_received).sum();
    assert!(rec > 0, "推薦で届いた人がいる");
    // ラウンド 1 は全体の履歴が無いので 0 人（設計書 6.2）
    assert_eq!(s.summaries().next().unwrap().rec_received, 0);
}

#[test]
fn t_r04_follow_stage_and_rng_unchanged_by_recommendation() {
    // 同じ母集団・同じ乱数で、推薦 on/off の ① の部分が同じ
    let scn = Scenario::new(graph(), cfg(true, 0.0)).unwrap();
    let mut pop = scn.initial.clone();
    // 全体の履歴を作っておく（θ = 0 なら未受信者全員が候補）
    sim_api::recommend::update_history(
        &mut pop,
        0,
        [(1, true, false), (2, false, false)].into_iter(),
    );
    let n = scn.graph.n_nodes;
    let ip = 5;
    let (mut pa, mut pb) = (pop.clone(), pop.clone());
    let (mut ra, mut rb) = (rng_from_seed(7), rng_from_seed(7));
    let (mut sa, mut sb) = (Scratch::new(n), Scratch::new(n));
    let (mut ea, mut eb) = (Rec::default(), Rec::default());
    let msg = scn.messages[0];
    let off = run_round_rec(
        &scn.graph.csr,
        0.4,
        &mut pa,
        &mut sa,
        ip,
        0,
        &msg,
        &scn.mech,
        None,
        None,
        1,
        &mut ra,
        &mut ea,
    );
    let on = run_round_rec(
        &scn.graph.csr,
        0.4,
        &mut pb,
        &mut sb,
        ip,
        0,
        &msg,
        &scn.mech,
        Some(&scn.cfg.recommender),
        None,
        1,
        &mut rb,
        &mut eb,
    );
    let k = ea.0.len();
    assert!(eb.0.len() > k, "推薦で追加の受信がある");
    for i in 0..k {
        let (a, b) = (&ea.0[i], &eb.0[i]);
        assert_eq!(
            (a.wave, a.node, a.from, a.via),
            (b.wave, b.node, b.from, b.via)
        );
        assert_eq!(a.u_int.to_bits(), b.u_int.to_bits());
        assert_eq!(b.via, VIA_FOLLOW);
    }
    // ① の後に推薦が来る。候補は IP と ① の受信者を含まない
    let first: HashSet<u32> = ea.0.iter().map(|e| e.node).collect();
    for e in &eb.0[k..] {
        if e.via == VIA_REC {
            assert!(!first.contains(&e.node) && e.node != ip);
        }
    }
    assert_eq!(
        (
            off.n_received,
            on.n_received - on.rec_received - on.chain_received
        ),
        (k as u32, k as u32)
    );
    // 推薦 off（None）と、推薦の無効な設定は、乱数の位置まで同じ
    let (mut pc, mut rc, mut scc) = (pop.clone(), rng_from_seed(7), Scratch::new(n));
    let off2 = run_round(
        &scn.graph.csr,
        0.4,
        &mut pc,
        &mut scc,
        ip,
        0,
        &msg,
        &scn.mech,
        1,
        &mut rc,
        &mut NoTrace,
    );
    assert_eq!(off, off2);
    let mut ra2 = rng_from_seed(7);
    let (mut pd, mut sd) = (pop.clone(), Scratch::new(n));
    run_round_rec(
        &scn.graph.csr,
        0.4,
        &mut pd,
        &mut sd,
        ip,
        0,
        &msg,
        &scn.mech,
        None,
        None,
        1,
        &mut ra2,
        &mut NoTrace,
    );
    assert_eq!(rc.next_u64(), ra2.next_u64());
}

#[test]
fn t_r05_via_from_wave_and_event_invariants() {
    let s = run(cfg(true, 0.05), 5, &SEQ);
    let mut saw_rec = false;
    for r in 1..=s.round() {
        let ev = s.event_columns(r).unwrap();
        let mut seen = HashSet::new();
        let follow_max = (0..ev.len())
            .filter(|&k| ev.flags[k] & (EV_REC | EV_RECCHAIN) == 0)
            .map(|k| ev.wave[k])
            .max();
        let mut rec_wave = None;
        for k in 0..ev.len() {
            assert!(seen.insert(ev.node[k]), "1 ラウンドで同じノードは 1 件まで");
            assert_ne!(ev.node[k], 5, "IP のイベントは無い");
            if ev.flags[k] & EV_ENQ != 0 {
                assert!(ev.flags[k] & EV_INT != 0);
            }
            if ev.flags[k] & EV_REC != 0 {
                saw_rec = true;
                assert_eq!(ev.from[k], FROM_REC);
                // 推薦はフォロー経由の連鎖が終わったあと（ウェーブ番号はその連鎖のウェーブ数 = n_waves と同じ規則で数える）
                assert!(
                    follow_max.is_none_or(|m| ev.wave[k] > m),
                    "推薦はフォロー経由の連鎖のあと"
                );
                assert!(
                    rec_wave.is_none_or(|w| w == ev.wave[k]),
                    "推薦の受信は同じウェーブ"
                );
                rec_wave = Some(ev.wave[k]);
            }
            if ev.flags[k] & EV_RECCHAIN != 0 {
                assert!(ev.wave[k] > rec_wave.expect("推薦の後"));
                assert_ne!(ev.from[k], FROM_REC);
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
    assert!(saw_rec);
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

#[test]
fn t_r06_history_updated_at_round_end_and_off_is_empty() {
    let s = run(cfg(true, 0.05), 5, &SEQ);
    let pop = s.population();
    let total_recv: u32 = pop.rec_total.iter().map(|t| t[0]).sum();
    let summed: u32 = s.summaries().map(|r| r.n_received).sum();
    assert_eq!(total_recv, summed, "全受信が履歴に入る");
    let total_share: u32 = pop.rec_total.iter().map(|t| t[1]).sum();
    let int_now: u32 = s.summaries().map(|r| r.n_int_active_now).sum();
    assert_eq!(total_share, int_now);
    // 推薦 off では履歴を持たない
    let off = run(cfg(false, 0.05), 5, &SEQ);
    assert!(off.population().rec_hist.is_empty());
    assert!(
        off.summaries()
            .all(|r| r.rec_received == 0 && r.chain_received == 0)
    );
    // ラウンド 1 は履歴が無く全員 0 点。θ = 0 なら 0 ≥ 0 で、① で届かなかった全員に推薦する
    let one = run(cfg(true, 0.0), 5, &[0]);
    assert!(one.summaries().next().unwrap().rec_received > 0);
}

#[test]
fn t_r07_rewind_and_branch_are_bit_identical_with_recommendation() {
    let snap = |s: &Session| -> (Vec<String>, Vec<u64>, Vec<[u16; 3]>) {
        (
            s.summaries()
                .map(|x| serde_json::to_string(x).unwrap())
                .collect(),
            s.population()
                .states
                .iter()
                .map(|x| x.u_int.to_bits())
                .collect(),
            s.population().rec_hist.clone(),
        )
    };
    let full = run(cfg(true, 0.05), 5, &SEQ);
    let want = snap(&full);
    for k in [0u32, 3, 7, 8, 9, 11] {
        let mut s = run(cfg(true, 0.05), 5, &SEQ);
        s.rewind_to(k).unwrap();
        let reference = run(cfg(true, 0.05), 5, &SEQ[..k as usize]);
        assert_eq!(snap(&s), snap(&reference), "k={k}: 巻き戻し直後");
        for &m in &SEQ[k as usize..] {
            s.step(Choice::Msg(m)).unwrap();
        }
        assert_eq!(snap(&s), want, "k={k}: 再生後");
    }
}

#[test]
fn t_r08_mc_is_deterministic_with_recommendation() {
    let scn = Scenario::new(graph(), cfg(true, 0.05)).unwrap();
    let seq: Vec<Choice> = SEQ.iter().map(|&m| Choice::Msg(m)).collect();
    let spec = McSpec {
        trials: 20,
        mode: McMode::Independent { seed: 4 },
    };
    let a = run_mc(&scn, 5, &seq, &spec).unwrap();
    let b = run_mc(&scn, 5, &seq, &spec).unwrap();
    assert_eq!(a.total_xact_mean.to_bits(), b.total_xact_mean.to_bits());
    let off = Scenario::new(graph(), cfg(false, 0.05)).unwrap();
    let c = run_mc(&off, 5, &seq, &spec).unwrap();
    assert_ne!(
        a.total_xact_mean.to_bits(),
        c.total_xact_mean.to_bits(),
        "推薦 on/off で結果が変わる"
    );
}

// ---- v2: CF＋グラフ（recommender-v2-design.md 10 章） ----

fn hybrid(threshold: f64, alpha: f64) -> ScenarioConfig {
    let mut c = cfg_m(true, threshold, RecMethod::Hybrid);
    c.recommender.alpha = alpha;
    c
}

#[test]
fn t_v04_social_proof_counts_follow_stage_senders_only() {
    // 推薦 on・α = 0（グラフだけ）・θ = 0.2: 「フォロー先の 2 割以上が ① で送った人」に届く
    let scn = Scenario::new(graph(), hybrid(0.2, 0.0)).unwrap();
    let csr = &scn.graph.csr;
    let n = scn.graph.n_nodes;
    let ip = 5u32;
    let mut pop = scn.initial.clone();
    let (mut sc, mut rng, mut ev) = (Scratch::new(n), rng_from_seed(7), Rec::default());
    let msg = scn.messages[0];
    run_round_rec(
        csr,
        0.4,
        &mut pop,
        &mut sc,
        ip,
        0,
        &msg,
        &scn.mech,
        Some(&scn.cfg.recommender),
        None,
        1,
        &mut rng,
        &mut ev,
    );
    // ① の送り手 = IP と、推薦によらずに受け取って送った人
    let mut sent = vec![false; n];
    sent[ip as usize] = true;
    for e in &ev.0 {
        if e.via == VIA_FOLLOW && e.enqueued {
            sent[e.node as usize] = true;
        }
    }
    let f_g = |u: u32, csr: &Csr| {
        let p = csr.pred(u);
        if p.is_empty() {
            0.0
        } else {
            p.iter().filter(|&&v| sent[v as usize]).count() as f64 / p.len() as f64
        }
    };
    let follow: HashSet<u32> =
        ev.0.iter()
            .filter(|e| e.via == VIA_FOLLOW)
            .map(|e| e.node)
            .collect();
    let rec: Vec<u32> =
        ev.0.iter()
            .filter(|e| e.via == VIA_REC)
            .map(|e| e.node)
            .collect();
    let want: Vec<u32> = (0..n as u32)
        .filter(|&u| u != ip && !follow.contains(&u) && f_g(u, csr) >= 0.2)
        .collect();
    assert_eq!(
        rec, want,
        "手で数えた f_G ≥ θ の人と一致（② ③ の送り手は数えない）"
    );
    assert!(!rec.is_empty());
}

#[test]
fn t_v05_method_and_alpha_keep_follow_stage_and_rng() {
    let scn_b = Scenario::new(graph(), cfg(true, 0.0)).unwrap();
    let n = scn_b.graph.n_nodes;
    let msg = scn_b.messages[0];
    let mut outs = Vec::new();
    for c in [
        cfg(true, 0.1),
        hybrid(0.1, 0.0),
        hybrid(0.1, 0.5),
        hybrid(0.1, 1.0),
    ] {
        let scn = Scenario::new(graph(), c).unwrap();
        let mut pop = scn.initial.clone();
        let (mut sc, mut rng, mut ev) = (Scratch::new(n), rng_from_seed(7), Rec::default());
        run_round_rec(
            &scn.graph.csr,
            0.4,
            &mut pop,
            &mut sc,
            5,
            0,
            &msg,
            &scn.mech,
            Some(&scn.cfg.recommender),
            None,
            1,
            &mut rng,
            &mut ev,
        );
        let follow: Vec<(u32, u32, u64)> =
            ev.0.iter()
                .filter(|e| e.via == VIA_FOLLOW)
                .map(|e| (e.wave, e.node, e.u_int.to_bits()))
                .collect();
        let rec = ev.0.iter().filter(|e| e.via == VIA_REC).count();
        outs.push((follow, rec));
    }
    for o in &outs[1..] {
        assert_eq!(o.0, outs[0].0, "① の結果は方式・α で変わらない");
    }
    // ラウンド 1: 基準と CF だけ（α = 1）は履歴が無く 0 人。グラフを含むと届く
    assert_eq!((outs[0].1, outs[3].1), (0, 0));
    assert!(outs[1].1 > 0 && outs[2].1 > 0);
}

#[test]
fn t_v06_hybrid_rewind_and_mc_are_bit_identical() {
    let snap = |s: &Session| -> (Vec<String>, Vec<u64>, Vec<[u16; 3]>) {
        (
            s.summaries()
                .map(|x| serde_json::to_string(x).unwrap())
                .collect(),
            s.population()
                .states
                .iter()
                .map(|x| x.u_int.to_bits())
                .collect(),
            s.population().rec_hist.clone(),
        )
    };
    let c = hybrid(0.2, 0.5);
    let full = run(c.clone(), 5, &SEQ);
    assert!(full.summaries().map(|r| r.rec_received).sum::<u32>() > 0);
    let want = snap(&full);
    for k in [0u32, 5, 8, 11] {
        let mut s = run(c.clone(), 5, &SEQ);
        s.rewind_to(k).unwrap();
        for &m in &SEQ[k as usize..] {
            s.step(Choice::Msg(m)).unwrap();
        }
        assert_eq!(snap(&s), want, "k={k}");
    }
    let scn = Scenario::new(graph(), c).unwrap();
    let seq: Vec<Choice> = SEQ.iter().map(|&m| Choice::Msg(m)).collect();
    let spec = McSpec {
        trials: 10,
        mode: McMode::Independent { seed: 4 },
    };
    let (a, b) = (
        run_mc(&scn, 5, &seq, &spec).unwrap(),
        run_mc(&scn, 5, &seq, &spec).unwrap(),
    );
    assert_eq!(a.total_xact_mean.to_bits(), b.total_xact_mean.to_bits());
    // ノード詳細の表と似かた
    let d = full.rec_detail(0);
    assert_eq!(d.len(), 6);
    for r in &d {
        assert!((r.f - (0.5 * r.f_cf + 0.5 * r.f_g)).abs() < 1e-12);
    }
    let sim = full.rec_sim();
    assert_eq!(sim.len(), 36);
    assert!(sim.iter().all(|&x| (0.0..=1.0 + 1e-12).contains(&x)));
}

// ---- v3: Rモデル（recommender-v3-design.md 10 章） ----

fn wave_cfg(eps: f64, delta: f64, mu: f64) -> ScenarioConfig {
    let mut c = cfg_m(true, 0.0, RecMethod::Wave);
    c.recommender.eps = eps;
    c.recommender.delta = delta;
    c.recommender.mu = mu;
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
        msg_id,
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
    // ラウンド 1 から推薦が起きうる（v1 と違い、履歴を使わない）
    assert!(!s.rec_wave_trace().is_empty());
}

/// T-W02: 方式 wave で ε = 0 または δ = 0 なら、推薦 off と結果・拡散の乱数・rec_rng の状態まで一致する。
#[test]
fn t_w02_zero_budget_matches_off() {
    for (eps, delta) in [(0.0, 1.0), (1.0, 0.0)] {
        let scn = Scenario::new(graph(), wave_cfg(eps, delta, 1.0)).unwrap();
        let off = Scenario::new(graph(), cfg(false, 0.0)).unwrap();
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
                m,
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

/// T-W03: フォロー経由では届かない部分にも推薦で届く。受信済み・IP には届かず、空振りを数える。
#[test]
fn t_w03_reaches_outside_without_double_receipt() {
    // 0 → 1 → 2 と、フォロー経由では届かない 3 → 4 → 5 → 3
    let g = load_graph(&GraphSource {
        bytes: b"0 1\n1 2\n3 4\n4 5\n5 3\n".to_vec(),
        format: GraphFormat::EdgeList,
        gzip: false,
        directed: true,
        transposed: false,
    })
    .unwrap();
    let mut c = wave_cfg(5.0, 1.0, 1.0);
    c.recommender.near = 0.0;
    c.receipt_probability = 1.0;
    let scn = Scenario::new(g, c).unwrap();
    let mut reached_outside = false;
    for seed in 0..30 {
        // 受け取れば必ず両方が成立する母集団（w = 0 なので u' = 刺激 > 0）
        let mut pop = scn.initial.clone();
        for s in &mut pop.states {
            *s = sim_api::node::NodeState::new(0.0, 0.0, -1.0, -1.0);
        }
        let mut rng = rng_from_seed(seed);
        let mut rec = sim_api::recommend::rec_rng(seed);
        let (k, ev) = wave_round(&scn, &mut pop, 0, 0, &mut rng, &mut rec, 1.0, 1.0);
        let nodes: HashSet<u32> = ev.iter().map(|e| e.node).collect();
        assert_eq!(nodes.len(), ev.len(), "seed {seed}: 1 人 1 回");
        assert!(!nodes.contains(&0), "IP は受信しない");
        assert_eq!(k.n_xact, k.n_received);
        assert_eq!(k.rec_xact, k.rec_received);
        assert!(k.rec_miss > 0, "枠が宛先より多いので空振りが出る");
        reached_outside |= nodes.iter().any(|&j| j >= 3);
        for e in &ev {
            assert_eq!(e.via == VIA_REC, e.from == FROM_REC);
        }
    }
    assert!(reached_outside, "推薦で 3〜5 に届いたことがある");
}

/// T-W04: 方式 wave でも、拡散の乱数を引くのはフォロー経由の辺の抽選だけ（推薦は rec_rng だけを引く）。
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

/// T-W06: 方式 wave（R3 あり）で、巻き戻し・分岐・MC がビット一致する。
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
