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
        rounds.push(json!({
            "summary": summary,
            "events": events,
            "ever_flags": arr(&sim.ever_flags()?),
            "kinds": arr(&sim.kinds(0)?),
        }));
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
    ]
}
