//! T-W09: 参照実装（`diffusion_seokyeong` の core 0.3.0）との統計的な一致を確かめる（`recommender-v3-design.md` 10 章）。
//! このビューアで母集団を作って各ノードの重みと初期効用を CSV に書き、同じ母集団・同じ設定で
//! 方式 wave の 1 ラウンドを試行数ぶん回して、平均と標準誤差を出す。
//!
//! 参照実装側は `diffusion_seokyeong/runner/examples/rec_crosscheck.rs` がこの CSV を読んで同じ量を出す。
//! 実行: cargo run --release --example wave_crosscheck -p sim-api -- グラフ.txt 出力.csv [試行数]
use std::fmt::Write;

use sim_api::config::{RecMethod, ScenarioConfig};
use sim_api::engine::{NoTrace, Scratch, WaveCtx, run_round_rec};
use sim_api::graph_io::*;
use sim_api::recommend::rec_rng;
use sim_api::rng::rng_from_seed;
use sim_api::session::Scenario;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("グラフのパスを指定してください");
    let out = args.next().expect("CSV の出力先を指定してください");
    let trials: u64 = args.next().map_or(2000, |s| s.parse().unwrap());
    let g = load_graph(&GraphSource {
        bytes: std::fs::read(&path).unwrap(),
        format: GraphFormat::EdgeList,
        gzip: false,
        directed: true,
        transposed: true,
    })
    .unwrap();
    // 較正 T を 3 にして、初回にシェアする人がいる母集団にする（ē0 > 0）
    let mut cfg = ScenarioConfig {
        receipt_probability: 0.05,
        calibration_rounds: 3,
        ..Default::default()
    };
    cfg.recommender.enabled = true;
    cfg.recommender.method = RecMethod::Wave;
    let scn = Scenario::new(g, cfg).unwrap();
    let csr = &scn.graph.csr;
    // IP は出次数が最大のノード
    let ip = (0..csr.n_nodes() as u32)
        .max_by_key(|&i| csr.out_degree(i))
        .unwrap();
    let m = 1u8; // 既定のメッセージ空間の m1（内部 0.20・外部 0.05）
    let msg = scn.messages[m as usize];

    let mut csv = String::from("w_int,w_ext,u_int,u_ext\n");
    for s in &scn.initial.states {
        let _ = writeln!(
            csv,
            "{:?},{:?},{:?},{:?}",
            s.w_int, s.w_ext, s.u_int, s.u_ext
        );
    }
    std::fs::write(&out, csv).unwrap();

    let r = &scn.cfg.recommender;
    let mut sc = Scratch::new(scn.graph.n_nodes);
    let mut sum = [0f64; 3];
    let mut sq = [0f64; 3];
    for t in 0..trials {
        let mut pop = scn.initial.clone();
        let mut rng = rng_from_seed(t);
        let mut rr = rec_rng(t);
        let c = run_round_rec(
            csr,
            scn.cfg.receipt_probability,
            &mut pop,
            &mut sc,
            ip,
            m,
            &msg,
            &scn.mech,
            Some(r),
            Some(WaveCtx {
                rng: &mut rr,
                a_t: scn.ebar0,
                ebar0: scn.ebar0,
            }),
            1,
            &mut rng,
            &mut NoTrace,
        );
        for (k, v) in [c.n_share, c.n_xact, c.rec_received]
            .into_iter()
            .enumerate()
        {
            sum[k] += v as f64;
            sq[k] += (v as f64).powi(2);
        }
    }
    let n = trials as f64;
    println!(
        "# graph={path} ip={ip} p={} eps={} near={} k={} phi_max={} ebar0={} mean_int={} s_int={} s_ext={} trials={trials}",
        scn.cfg.receipt_probability,
        r.eps,
        r.near,
        r.k,
        r.phi_max,
        scn.ebar0,
        scn.mean_int,
        msg.internal_stimulus,
        msg.external_stimulus
    );
    for (k, name) in ["share", "xact", "rec_rcvd"].iter().enumerate() {
        let mean = sum[k] / n;
        let se = ((sq[k] / n - mean * mean).max(0.0) / n).sqrt();
        println!("{name},{mean},{se}");
    }
}
