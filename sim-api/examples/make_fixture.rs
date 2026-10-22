//! 決定的な合成グラフ（ノード 300、各ノードから平均 4 本の有向辺、シード 1）を作る。
//! 出力: spec/fixtures/random300.txt（リポジトリのルートで実行する）。

use rand::{Rng, SeedableRng};
use sim_api::rng::SimRng;

fn main() {
    let n = 300usize;
    let mut rng = SimRng::seed_from_u64(1);
    let mut edges: Vec<(usize, usize)> = Vec::new();
    for i in 0..n {
        for _ in 0..4 {
            // 仮定: usize の乱数抽出は 32/64bit で変わるので u64 から作る
            let j = (rng.random::<u64>() % n as u64) as usize;
            if j != i {
                edges.push((i, j));
            }
        }
    }
    edges.sort();
    edges.dedup();
    let mut s = String::from("# random300: 300 nodes, ~4 out-edges per node, seed=1\n");
    for (a, b) in edges {
        s.push_str(&format!("{a} {b}\n"));
    }
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "spec/fixtures/random300.txt".into());
    std::fs::write(&path, s).unwrap();
    println!("wrote {path}");
}
