//! ネイティブでゴールデンを作る（リポジトリのルートで実行）。出力: spec/fixtures/golden_<name>.json

use sim_wasm::golden::{cases, run_case};

fn main() {
    let graph = std::fs::read("spec/fixtures/random300.txt")
        .expect("先に sim-api の make_fixture を実行してください");
    for c in cases(&graph) {
        let v = run_case(&c).unwrap();
        let path = format!("spec/fixtures/golden_{}.json", c.name);
        std::fs::write(&path, serde_json::to_string_pretty(&v).unwrap()).unwrap();
        println!("wrote {path}");
    }
}
