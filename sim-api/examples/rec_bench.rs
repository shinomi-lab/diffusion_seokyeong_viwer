//! T-R11・T-V10・T-W10: 推薦の時間を測る（`recommender-design.md` 11 章、v3 は方式 wave も）。
//! 実行: cargo run --release --example rec_bench -p sim-api -- グラフ.txt [ラウンド数]
use std::sync::Arc;
use std::time::Instant;

use sim_api::config::ScenarioConfig;
use sim_api::graph_io::*;
use sim_api::log::Choice;
use sim_api::session::{Scenario, Session};

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("グラフのパスを指定してください");
    let rounds: u32 = args.next().map_or(10, |s| s.parse().unwrap());
    let bytes = std::fs::read(&path).unwrap();
    let src = GraphSource {
        bytes,
        format: GraphFormat::EdgeList,
        gzip: false,
        directed: true,
        transposed: true,
    };
    use sim_api::config::RecMethod;
    for (name, rec, method) in [
        ("推薦 off", false, RecMethod::Baseline),
        ("推薦 on・基準（θ=0.15）", true, RecMethod::Baseline),
        (
            "推薦 on・CF＋グラフ（θ=0.15, α=0.5）",
            true,
            RecMethod::Hybrid,
        ),
        ("推薦 on・Rモデル（ε=1）", true, RecMethod::Wave),
    ] {
        let g = load_graph(&src).unwrap();
        let n = g.n_nodes;
        let mut cfg = ScenarioConfig::default();
        cfg.recommender.enabled = rec;
        cfg.recommender.method = method;
        let scn = Arc::new(Scenario::new(g, cfg).unwrap());
        let mut s = Session::new(scn, 0).unwrap();
        let t = Instant::now();
        let mut recv = 0u64;
        for r in 0..rounds {
            let o = s.step(Choice::Msg((r % 6) as u8)).unwrap();
            recv += o.summary.rec_received as u64;
        }
        let ms = t.elapsed().as_secs_f64() * 1e3 / rounds as f64;
        println!("{name}: {n} ノード、1 ラウンド平均 {ms:.1} ms、推薦で受信 合計 {recv} 人");
    }
}
