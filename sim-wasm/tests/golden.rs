//! T-17: WASM とネイティブの一致。`spec/fixtures/golden_*.json`（ネイティブで作成）を同じ入力から再計算して比べる。
//! 実行: `wasm-pack test --node sim-wasm`。ネイティブでも `cargo test` で同じ比較が走る。

use sim_wasm::golden::{cases, run_case};
use wasm_bindgen_test::*;

const GRAPH: &[u8] = include_bytes!("../../spec/fixtures/random300.txt");
const GOLDEN_OFF: &str = include_str!("../../spec/fixtures/golden_off.json");
const GOLDEN_BALANCED: &str = include_str!("../../spec/fixtures/golden_balanced.json");
const GOLDEN_REC: &str = include_str!("../../spec/fixtures/golden_rec.json");
const GOLDEN_HYBRID: &str = include_str!("../../spec/fixtures/golden_hybrid.json");
const GOLDEN_WAVE: &str = include_str!("../../spec/fixtures/golden_wave.json");

fn check(name: &str, golden: &str) {
    let c = cases(GRAPH).into_iter().find(|c| c.name == name).unwrap();
    let got = run_case(&c).unwrap();
    let want: serde_json::Value = serde_json::from_str(golden).unwrap();
    let (g, w) = (
        got["rounds"].as_array().unwrap(),
        want["rounds"].as_array().unwrap(),
    );
    assert_eq!(g.len(), w.len());
    for (i, (a, b)) in g.iter().zip(w).enumerate() {
        assert_eq!(a["summary"], b["summary"], "{name} round {} summary", i + 1);
        assert_eq!(a["events"], b["events"], "{name} round {} events", i + 1);
        assert_eq!(
            a["ever_flags"],
            b["ever_flags"],
            "{name} round {} ever_flags",
            i + 1
        );
        assert_eq!(a["kinds"], b["kinds"], "{name} round {} kinds", i + 1);
        // 似かたは hybrid だけ（古いゴールデンにはキーが無い）
        if b.get("rec_sim").is_some() {
            assert_eq!(a["rec_sim"], b["rec_sim"], "{name} round {} rec_sim", i + 1);
        }
        // Rモデルの推薦の推移は wave だけ
        assert_eq!(
            a.get("rec_wave"),
            b.get("rec_wave"),
            "{name} round {} rec_wave",
            i + 1
        );
    }
}

#[wasm_bindgen_test]
#[test]
fn golden_off() {
    check("off", GOLDEN_OFF);
}

#[wasm_bindgen_test]
#[test]
fn golden_balanced() {
    check("balanced", GOLDEN_BALANCED);
}

#[wasm_bindgen_test]
#[test]
fn golden_rec() {
    check("rec", GOLDEN_REC);
}

#[wasm_bindgen_test]
#[test]
fn golden_hybrid() {
    check("hybrid", GOLDEN_HYBRID);
}

#[wasm_bindgen_test]
#[test]
fn golden_wave() {
    check("wave", GOLDEN_WAVE);
}
