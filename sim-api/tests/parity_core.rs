//! T-01 / T-02 / T-07 / T-12 / T-16: 既存 `core::diffusion` とのビット一致と、エンジンの不変条件。
#![allow(clippy::needless_range_loop, clippy::field_reassign_with_default)]

use rand::{Rng, SeedableRng, rngs::SmallRng};
use sim_api::config::{Mech, MechanismConfig, ScenarioConfig};
use sim_api::engine::*;
use sim_api::graph_io::*;
use sim_api::node::NodeState;
use sim_api::population::{Population, sample_population};
use sim_api::rng::rng_from_seed;
use sim_core::decision::{DecisionAny, DecisionBase, DecisionOnce};
use sim_core::diffusion::{Workspace, monte_carlo_average};
use sim_core::message::Message;
use sim_core::user::UserState;

pub fn load_edges(edges: &[(usize, usize)], n: usize) -> LoadedGraph {
    // 全ノードが現れるよう、孤立ノードにも自己ループ以外の目印は付けず、ラベル順＝番号になる連番グラフを作る
    let mut s = String::new();
    for &(a, b) in edges {
        s.push_str(&format!("{a} {b}\n"));
    }
    let g = load_graph(&GraphSource {
        bytes: s.into_bytes(),
        format: GraphFormat::EdgeList,
        gzip: false,
        directed: true,
        transposed: false,
    })
    .unwrap();
    assert_eq!(g.n_nodes, n, "テスト用グラフは全ノードに辺を持たせる");
    g
}

fn random_graph(seed: u64, n: usize, deg: usize) -> LoadedGraph {
    let mut r = SmallRng::seed_from_u64(seed);
    let mut edges = Vec::new();
    for i in 0..n {
        // 全ノードを出現させるため、環状の 1 辺を必ず入れる
        edges.push((i, (i + 1) % n));
        for _ in 0..deg {
            let j = r.random_range(0..n);
            if j != i {
                edges.push((i, j));
            }
        }
    }
    load_edges(&edges, n)
}

fn default_msgs() -> Vec<Message> {
    ScenarioConfig::default().messages()
}

fn user_states(pop: &Population) -> Vec<UserState> {
    pop.states
        .iter()
        .map(|s| {
            UserState::new(
                DecisionAny::from(DecisionBase::new(s.w_int, pop.u0_int)),
                DecisionOnce::from(DecisionBase::new(s.w_ext, pop.u0_ext)),
            )
        })
        .collect()
}

fn run_parity(mech_cfg: &MechanismConfig) -> (usize, usize) {
    let msgs = default_msgs();
    let (mut events, mut multi) = (0usize, 0usize);
    let mech = mech_cfg.effective();
    assert_eq!(mech, Mech::off(), "T-01 は機構が無効値のときに限る");
    for gseed in 0..60u64 {
        let n = 40 + (gseed as usize % 5) * 60;
        let deg = 1 + (gseed as usize % 6);
        let g = random_graph(1000 + gseed, n, deg);
        let cfg = ScenarioConfig {
            user_seed: gseed,
            ..Default::default()
        };
        let (mi, me) = cfg.mean_stimuli().unwrap();
        let init = sample_population(&cfg, n, mi, me).unwrap();
        let states = user_states(&init);
        for &p in &[0.3, 0.5, 1.0] {
            for rseed in 0..5u64 {
                let mut pop = init.clone();
                let mut sc = Scratch::new(n);
                let mut rng = rng_from_seed(rseed);
                let seq: Vec<usize> = (0..8)
                    .map(|round| (round * 5 + rseed as usize) % 6)
                    .collect();
                let ip = ((rseed as usize * 7) % n) as u32;
                let (mut cx, mut cs) = (0u64, 0u64);
                for round in 0..8usize {
                    let id = seq[round];
                    let mut rec = Counter::default();
                    let c = run_round(
                        &g.csr,
                        p,
                        &mut pop,
                        &mut sc,
                        ip,
                        id as u8,
                        &msgs[id],
                        &mech,
                        round as u32 + 1,
                        &mut rng,
                        &mut rec,
                    );
                    cx += c.n_xact as u64;
                    cs += c.n_share as u64;
                    events += rec.n;
                    multi += rec.multi;

                    // 既存側: 同じシードの SmallRng で、系列の先頭 round+1 個を 1 試行だけ回す（累積合計が返る）
                    let prefix: Vec<Message> = seq[..=round].iter().map(|&i| msgs[i]).collect();
                    let mut ws = Workspace::new(n);
                    let mut srng = SmallRng::seed_from_u64(rseed);
                    let agg = monte_carlo_average(
                        &g.graph,
                        p,
                        &states,
                        ip as usize,
                        &prefix,
                        1,
                        &mut srng,
                        &mut ws,
                    );
                    assert_eq!(
                        agg.mean_num_xact, cx as f64,
                        "xact g{gseed} p{p} r{rseed} round{round}"
                    );
                    assert_eq!(
                        agg.mean_num_share, cs as f64,
                        "share g{gseed} p{p} r{rseed} round{round}"
                    );
                    // 乱数列の位置まで一致
                    let mut a = rng.clone();
                    assert_eq!(
                        rand::RngCore::next_u64(&mut a),
                        rand::RngCore::next_u64(&mut srng),
                        "rng g{gseed} p{p} r{rseed} round{round}"
                    );
                }
            }
        }
    }
    (events, multi)
}

#[derive(Default)]
struct Counter {
    n: usize,
    multi: usize,
}

impl Observer for Counter {
    fn on_receive(&mut self, ev: &ReceiveEvent) {
        self.n += 1;
        if ev.n_arr > 1 {
            self.multi += 1;
        }
    }
}

#[test]
fn t01_cascade_matches_core_with_mechanisms_off() {
    let (events, multi) = run_parity(&MechanismConfig::off());
    assert!(events > 10_000, "受信イベントが少なすぎる: {events}");
    assert!(multi > 0, "n_arr>1 のイベントが一度も無い");
}

#[test]
fn t07_enabled_but_neutral_values_match_core() {
    let mut c = MechanismConfig::balanced();
    c.m1.kappa = 0.0;
    run_parity(&c);
}

/// ノード単位の更新式が core の UserState とビット一致する（Workspace.states が非公開で直接は見えないため）。
#[test]
fn node_contact_matches_core_user_state() {
    let msgs = default_msgs();
    let mut r = SmallRng::seed_from_u64(77);
    let mech = Mech::off();
    for _ in 0..500 {
        let (wi, we): (f64, f64) = (0.1 + 0.8 * r.random::<f64>(), 0.1 + 0.8 * r.random::<f64>());
        let mut ours = NodeState::new(wi, we, -1.0, -1.0);
        let mut theirs = UserState::new(
            DecisionAny::from(DecisionBase::new(wi, -1.0)),
            DecisionOnce::from(DecisionBase::new(we, -1.0)),
        );
        // 外部の効用を見るため、同じ重みの DecisionAny も並走させる
        let mut ext_shadow = DecisionAny::from(DecisionBase::new(we, -1.0));
        for _ in 0..12 {
            let id = r.random_range(0..6usize);
            let m = &msgs[id];
            let (i, e) = ours.contact(m, 1, &mech);
            theirs.contact(m);
            ext_shadow.update(m.external_stimulus);
            assert_eq!(
                ours.u_int.to_bits(),
                theirs.internal_decision.utility().to_bits()
            );
            assert_eq!(ours.u_ext.to_bits(), ext_shadow.utility().to_bits());
            assert_eq!(i, theirs.internal_decision.decided());
            assert_eq!(e, theirs.external_decision.decided());
        }
    }
}

/// T-02 の前提: `monte_carlo_average` の複数試行（1 本の rng を使い続ける）と同じ乱数の使い方。
#[test]
fn t02_sequential_trials_match_core() {
    let g = random_graph(5, 120, 3);
    let cfg = ScenarioConfig::default();
    let (mi, me) = cfg.mean_stimuli().unwrap();
    let init = sample_population(&cfg, 120, mi, me).unwrap();
    let states = user_states(&init);
    let msgs = default_msgs();
    let seq = [0usize, 5, 2, 2, 4, 1];
    let chosen: Vec<Message> = seq.iter().map(|&i| msgs[i]).collect();
    let trials = 50;
    let mut ws = Workspace::new(120);
    let mut srng = SmallRng::seed_from_u64(3);
    let agg = monte_carlo_average(
        &g.graph, 0.5, &states, 4, &chosen, trials, &mut srng, &mut ws,
    );
    let mut rng = rng_from_seed(3);
    let mut sc = Scratch::new(120);
    let (mut tx, mut ts) = (0u64, 0u64);
    let mech = Mech::off();
    for _ in 0..trials {
        let mut pop = init.clone();
        for (r, &id) in seq.iter().enumerate() {
            let c = run_round(
                &g.csr,
                0.5,
                &mut pop,
                &mut sc,
                4,
                id as u8,
                &msgs[id],
                &mech,
                r as u32 + 1,
                &mut rng,
                &mut NoTrace,
            );
            tx += c.n_xact as u64;
            ts += c.n_share as u64;
        }
    }
    assert_eq!(
        agg.mean_num_xact.to_bits(),
        (tx as f64 / trials as f64).to_bits()
    );
    assert_eq!(
        agg.mean_num_share.to_bits(),
        (ts as f64 / trials as f64).to_bits()
    );
}

fn w0_pop(n: usize) -> Population {
    // w=0, u0=-1: 受信すれば u = 刺激 > 0 で必ず発火
    Population {
        states: (0..n)
            .map(|_| NodeState::new(0.0, 0.0, -1.0, -1.0))
            .collect(),
        recv_round: vec![0; n],
        u0_int: -1.0,
        u0_ext: -1.0,
        rec_hist: Vec::new(),
        rec_total: Vec::new(),
        rec_l: 0,
    }
}

#[derive(Default)]
struct Rec(Vec<ReceiveEvent>);

impl Observer for Rec {
    fn on_receive(&mut self, ev: &ReceiveEvent) {
        self.0.push(*ev);
    }
}

#[test]
fn t12_m5_diamond_counts_arrivals() {
    // 0→1, 0→2, 1→3, 2→3
    let g = load_edges(&[(0, 1), (0, 2), (1, 3), (2, 3)], 4);
    let mut pop = w0_pop(4);
    let mut sc = Scratch::new(4);
    let mut rng = rng_from_seed(0);
    let m = Message {
        internal_stimulus: 0.2,
        external_stimulus: 0.2,
    };
    let mut rec = Rec::default();
    run_round(
        &g.csr,
        1.0,
        &mut pop,
        &mut sc,
        0,
        0,
        &m,
        &Mech::off(),
        1,
        &mut rng,
        &mut rec,
    );
    let e3 = rec.0.iter().find(|e| e.node == 3).unwrap();
    assert_eq!(e3.n_arr, 2);
    assert_eq!(e3.wave, 1);
    assert!(rec.0.iter().filter(|e| e.node != 3).all(|e| e.n_arr == 1));

    // M5（検討中のためコメントアウト）: η>0 なら n_arr=2 で 1.25 倍、n_arr=1 は 1.0
    // let mut mech = Mech::off();
    // mech.eta = 0.25;
    // mech.n_max = 3;
    // let mut pop = w0_pop(4);
    // run_round(&g.csr, 1.0, &mut pop, &mut sc, 0, 0, &m, &mech, 1, &mut rng, &mut NoTrace);
    // assert!((pop.states[3].u_int - 0.2 * 1.25).abs() < 1e-12);
    // assert!((pop.states[1].u_int - 0.2).abs() < 1e-12);

    // 片方だけなら 1
    let g1 = load_edges(&[(0, 1), (0, 2), (1, 3)], 4);
    let mut pop = w0_pop(4);
    let mut rec = Rec::default();
    run_round(
        &g1.csr,
        1.0,
        &mut pop,
        &mut sc,
        0,
        0,
        &m,
        &Mech::off(),
        1,
        &mut rng,
        &mut rec,
    );
    assert_eq!(rec.0.iter().find(|e| e.node == 3).unwrap().n_arr, 1);
}

#[test]
fn t12_n_max_cap_and_ignored_hits() {
    // 0→1,2,3,4 → 全員が 5 に届く。さらに 5→0（IP への命中）、5→1（前のウェーブの受信者への命中）
    let g = load_edges(
        &[
            (0, 1),
            (0, 2),
            (0, 3),
            (0, 4),
            (1, 5),
            (2, 5),
            (3, 5),
            (4, 5),
            (5, 0),
            (5, 1),
        ],
        6,
    );
    let m = Message {
        internal_stimulus: 0.2,
        external_stimulus: 0.2,
    };
    // M5（検討中）: mech.eta = 0.25; mech.n_max = 3; のとき 1.5 倍で頭打ちになる
    let mech = Mech::off();
    let mut pop = w0_pop(6);
    let mut sc = Scratch::new(6);
    let mut rec = Rec::default();
    let mut rng = rng_from_seed(0);
    run_round(
        &g.csr, 1.0, &mut pop, &mut sc, 0, 0, &m, &mech, 1, &mut rng, &mut rec,
    );
    assert_eq!(rec.0.iter().find(|e| e.node == 5).unwrap().n_arr, 4);
    // M5（検討中）: assert!((pop.states[5].u_int - 0.2 * 1.5).abs() < 1e-12, "n_max=3 で頭打ち");
    // M5 が無効なので、経路が 4 本でも刺激は 1 本分
    assert!((pop.states[5].u_int - 0.2).abs() < 1e-12);
    // IP のイベントは無く、1 は 1 件だけ（ウェーブ 2 の 5→1 は数えない）
    assert!(rec.0.iter().all(|e| e.node != 0));
    assert_eq!(rec.0.iter().filter(|e| e.node == 1).count(), 1);
    assert_eq!(rec.0.iter().find(|e| e.node == 1).unwrap().n_arr, 1);
}

#[test]
fn t16_event_invariants() {
    let msgs = default_msgs();
    for gseed in 0..20u64 {
        let n = 100;
        let g = random_graph(300 + gseed, n, 3);
        let cfg = ScenarioConfig {
            user_seed: gseed,
            ..Default::default()
        };
        let (mi, me) = cfg.mean_stimuli().unwrap();
        let mut pop = sample_population(&cfg, n, mi, me).unwrap();
        let mut sc = Scratch::new(n);
        let mut rng = rng_from_seed(gseed);
        let mech = Mech::off();
        for round in 1..=8u32 {
            let ip = 3u32;
            let id = (round as usize) % 6;
            let mut rec = Rec::default();
            let c = run_round(
                &g.csr, 0.6, &mut pop, &mut sc, ip, id as u8, &msgs[id], &mech, round, &mut rng,
                &mut rec,
            );
            assert_eq!(rec.0.len() as u32, c.n_received);
            let mut seen = std::collections::HashSet::new();
            let mut last_wave = 0;
            let mut enq_prev: std::collections::HashSet<u32> = [ip].into_iter().collect();
            let mut enq_cur = std::collections::HashSet::new();
            let mut cur_wave = 0;
            for e in &rec.0 {
                assert!(seen.insert(e.node), "同じノードのイベントは最大 1 件");
                assert_ne!(e.node, ip, "IP のイベントは無い");
                assert!(e.wave >= last_wave, "ウェーブ番号は単調");
                last_wave = e.wave;
                if e.wave != cur_wave {
                    enq_prev = std::mem::take(&mut enq_cur);
                    cur_wave = e.wave;
                }
                assert!(
                    enq_prev.contains(&e.from),
                    "from は直前のウェーブで enqueued（wave0 は IP）"
                );
                if e.enqueued {
                    assert!(e.int_fired);
                    enq_cur.insert(e.node);
                }
                assert!(e.n_arr >= 1);
            }
        }
    }
}

// ---- T-02 / T-02b: MC とバッチの 1 行の再現 ----

mod mc_tests {
    use super::*;
    use sim_api::log::Choice;
    use sim_api::mc::*;
    use sim_api::session::Scenario;
    use sim_core::message::MessageSpace;

    fn scenario() -> Scenario {
        let g = random_graph(11, 150, 3);
        Scenario::new(
            g,
            ScenarioConfig {
                user_seed: 4,
                receipt_probability: 0.5,
                ..Default::default()
            },
        )
        .unwrap()
    }

    fn ref_states(scn: &Scenario) -> Vec<UserState> {
        user_states(&scn.initial)
    }

    #[test]
    fn t02_run_mc_sequential_matches_monte_carlo_average() {
        let scn = scenario();
        let msgs = default_msgs();
        for seq in [
            vec![0usize, 5, 2, 2, 4],
            vec![3, 3, 3],
            vec![1, 0, 5, 4, 3, 2, 1, 0],
        ] {
            let choice: Vec<Choice> = seq.iter().map(|&i| Choice::Msg(i as u8)).collect();
            let chosen: Vec<Message> = seq.iter().map(|&i| msgs[i]).collect();
            for (ip, trials, seed) in [(3u32, 40u32, 0u64), (7, 25, 9)] {
                let res = run_mc(
                    &scn,
                    ip,
                    &choice,
                    &McSpec {
                        trials,
                        mode: McMode::Sequential { seed },
                    },
                )
                .unwrap();
                let mut ws = Workspace::new(150);
                let mut r = SmallRng::seed_from_u64(seed);
                let agg = monte_carlo_average(
                    &scn.graph.graph,
                    0.5,
                    &ref_states(&scn),
                    ip as usize,
                    &chosen,
                    trials as usize,
                    &mut r,
                    &mut ws,
                );
                assert_eq!(res.total_xact_mean.to_bits(), agg.mean_num_xact.to_bits());
                assert_eq!(res.total_share_mean.to_bits(), agg.mean_num_share.to_bits());
                assert_eq!(res.new_xact.mean.len(), seq.len());
            }
        }
    }

    #[test]
    fn t02b_batch_row_rng_matches_runner_derivation() {
        let (num_ips, num_seqs) = (3usize, 36u64); // 6^2
        let scn = scenario();
        let ms = MessageSpace::try_from(default_msgs()).unwrap();
        assert_eq!(ms.seq_size(2), num_seqs);
        // runner と同じ三重ループで子を順に派生させる
        let mut parent = SmallRng::seed_from_u64(0);
        let mut children = Vec::new();
        for _us in 0..2usize {
            for _ip in 0..num_ips {
                for _seq in 0..num_seqs {
                    children.push(SmallRng::from_rng(&mut parent));
                }
            }
        }
        let msgs = default_msgs();
        for (us, ip_pos, seq_index) in [(0usize, 0usize, 0u64), (0, 2, 35), (1, 1, 7), (1, 2, 35)] {
            let k = ((us * num_ips + ip_pos) as u64 * num_seqs + seq_index) as usize;
            let mut ours = batch_row_rng(0, num_ips, num_seqs, us, ip_pos, seq_index);
            let mut theirs = children[k].clone();
            assert_eq!(
                rand::RngCore::next_u64(&mut ours.clone()),
                rand::RngCore::next_u64(&mut theirs.clone())
            );
            // その乱数での MC がバッチの monte_carlo_average と一致
            let mut ms_buf = Vec::new();
            ms.seq_into(seq_index, 2, &mut ms_buf);
            let chosen: Vec<Message> = ms_buf.iter().map(|m| **m).collect();
            let digits = [(seq_index % 6) as u8, (seq_index / 6 % 6) as u8];
            assert_eq!(
                chosen[0].internal_stimulus,
                msgs[digits[0] as usize].internal_stimulus
            );
            let seq: Vec<Choice> = digits.iter().map(|&d| Choice::Msg(d)).collect();
            let res = run_mc_with_rng(&scn, 5, &seq, 30, &mut ours).unwrap();
            let mut ws = Workspace::new(150);
            let agg = monte_carlo_average(
                &scn.graph.graph,
                0.5,
                &ref_states(&scn),
                5,
                &chosen,
                30,
                &mut theirs,
                &mut ws,
            );
            assert_eq!(res.total_xact_mean.to_bits(), agg.mean_num_xact.to_bits());
            assert_eq!(res.total_share_mean.to_bits(), agg.mean_num_share.to_bits());
        }
    }

    #[test]
    fn independent_mode_is_deterministic_and_validates() {
        let scn = scenario();
        let seq: Vec<Choice> = [0u8, 1, 2].iter().map(|&i| Choice::Msg(i)).collect();
        let spec = McSpec {
            trials: 20,
            mode: McMode::Independent { seed: 5 },
        };
        let a = run_mc(&scn, 0, &seq, &spec).unwrap();
        let b = run_mc(&scn, 0, &seq, &spec).unwrap();
        assert_eq!(a.total_xact_mean.to_bits(), b.total_xact_mean.to_bits());
        assert_eq!(a.cum_xact.p50, b.cum_xact.p50);
        // 累積外部行動者数は単調
        assert!(a.cum_xact.mean.windows(2).all(|w| w[0] <= w[1]));
        assert!(run_mc(&scn, 0, &[Choice::Msg(9)], &spec).is_err());
    }

    #[test]
    fn mc_matches_session_for_single_trial_with_same_rng() {
        use sim_api::session::Session;
        let scn = std::sync::Arc::new(scenario());
        let seq = [Choice::Msg(0), Choice::Msg(3), Choice::Msg(5)];
        let mut s = Session::with_rng(std::sync::Arc::clone(&scn), 2, rng_from_seed(21)).unwrap();
        for &c in &seq {
            s.step(c).unwrap();
        }
        let mut rng = rng_from_seed(21);
        let mut sc = Scratch::new(150);
        let mut t = TrialSeries::default();
        run_trial(&scn, 2, &seq, &mut rng, &mut sc, &mut t);
        let sums: Vec<u32> = s.summaries().map(|x| x.new_xact).collect();
        assert_eq!(sums, t.new_xact);
        let sh: Vec<u32> = s.summaries().map(|x| x.n_share).collect();
        assert_eq!(sh, t.new_share);
        let mu: Vec<f32> = s.summaries().map(|x| x.recv.mean_int).collect();
        assert_eq!(mu, t.mean_u_int);
    }
}
