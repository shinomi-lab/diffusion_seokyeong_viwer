//! ゴールデン（T-17）。ネイティブで作った出力を、WASM で同じ入力から再計算して比べる。

use serde_json::{Value, json};

use crate::sim::Sim;

/// ゴールデンの入力。
pub struct GoldenCase<'a> {
    pub name: &'a str,
    pub graph: &'a [u8],
    pub config_json: &'a str,
    pub ip: u32,
    pub sequence: &'a [i32],
}

fn arr<T: Into<Value> + Copy>(v: &[T]) -> Value {
    Value::Array(v.iter().map(|&x| x.into()).collect())
}

/// 各ラウンドの `RoundSummary`・イベント列・`everFlags`・`kinds` を `Value` にまとめる。
pub fn run_case(c: &GoldenCase) -> Result<Value, String> {
    let mut sim = Sim::new();
    sim.load_graph(
        c.graph,
        r#"{"format":"edgelist","directed":true,"transposed":false}"#,
    )?;
    sim.create_scenario(c.config_json)?;
    sim.set_ip(c.ip)?;
    let mut rounds = Vec::new();
    for &ch in c.sequence {
        let summary: Value = serde_json::from_str(&sim.step(ch)?).map_err(|e| e.to_string())?;
        let e = sim.last_events()?;
        let events = json!({
            "wave": arr(&e.wave), "node": arr(&e.node), "from": arr(&e.from), "n_arr": arr(&e.n_arr),
            "u_int": arr(&e.u_int), "u_ext": arr(&e.u_ext), "flags": arr(&e.flags),
        });
        let mut round = json!({
            "summary": summary,
            "events": events,
            "ever_flags": arr(&sim.ever_flags()?),
            "kinds": arr(&sim.kinds(0)?),
            "rec_sim": arr(&sim.rec_sim()?),
        });
        // Rモデルの推薦の推移は方式 wave のときだけ記録する（既存のゴールデンの内容を変えないため）
        let w: Value = serde_json::from_str(&sim.rec_wave_trace()?).map_err(|e| e.to_string())?;
        if !w["trace"].as_array().is_some_and(|t| t.is_empty()) {
            round["rec_wave"] = w;
        }
        rounds.push(round);
    }
    Ok(json!({ "name": c.name, "ip": c.ip, "sequence": c.sequence, "rounds": rounds }))
}

/// ゴールデンの入力の一覧（`make_golden` と WASM のテストで共有する）。
pub fn cases<'a>(graph: &'a [u8]) -> Vec<GoldenCase<'a>> {
    vec![
        GoldenCase {
            name: "off",
            graph,
            config_json: r#"{"receipt_probability":0.5}"#,
            ip: 0,
            sequence: &[0, 5, 2, 2, 4, 1, 3, 0],
        },
        GoldenCase {
            name: "balanced",
            graph,
            config_json: r#"{"receipt_probability":0.6,"user_seed":3,"diffusion_seed":7,
                "mechanisms":{"m1":{"enabled":true}}}"#,
            ip: 17,
            sequence: &[0, 0, 3, 2, 5, 5, 1, 4, 2, 1, 0, 3],
        },
        // 推薦 on（閾値を低くして、推薦の段階と推薦由来の連鎖が起きるようにする。T-R09）
        GoldenCase {
            name: "rec",
            graph,
            config_json: r#"{"receipt_probability":0.6,"user_seed":3,"diffusion_seed":7,
                "mechanisms":{"m1":{"enabled":true}},
                "recommender":{"enabled":true,"threshold":0.05}}"#,
            ip: 17,
            sequence: &[0, 0, 3, 0, 5, 0, 1, 0, 2, 0, 0, 3],
        },
        // 推薦 on・CF＋グラフ（T-V07）。似かたが育つよう種類を混ぜ、ラウンドごとの似かたも記録に入れる
        GoldenCase {
            name: "hybrid",
            graph,
            config_json: r#"{"receipt_probability":0.6,"user_seed":3,"diffusion_seed":7,
                "mechanisms":{"m1":{"enabled":true}},
                "recommender":{"enabled":true,"method":"hybrid","threshold":0.2,"alpha":0.5,"min_common":3}}"#,
            ip: 17,
            sequence: &[0, 1, 3, 0, 5, 1, 1, 0, 2, 3, 0, 1],
        },
        // 推薦 on・Rモデル（T-W07）。R3（μ = 0.5）も通し、ラウンドごとの推薦の推移も記録に入れる
        GoldenCase {
            name: "wave",
            graph,
            config_json: r#"{"receipt_probability":0.6,"user_seed":3,"diffusion_seed":7,
                "mechanisms":{"m1":{"enabled":true}},
                "recommender":{"enabled":true,"method":"wave","eps":1.0,"near":0.5,"mu":0.5}}"#,
            ip: 17,
            sequence: &[0, 1, 3, 0, 5, 1, 1, 0, 2, 3, 0, 1],
        },
    ]
}
