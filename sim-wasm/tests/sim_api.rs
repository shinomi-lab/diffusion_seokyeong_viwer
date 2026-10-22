//! `Sim`（JS API の実体）の単体テスト。ネイティブで実行する。

use sim_wasm::sim::Sim;

const GRAPH: &[u8] = include_bytes!("../../spec/fixtures/random300.txt");

fn ready() -> Sim {
    let mut s = Sim::new();
    s.load_graph(GRAPH, r#"{"directed":true,"transposed":false}"#)
        .unwrap();
    s.create_scenario("{}").unwrap();
    s.set_ip(0).unwrap();
    s
}

#[test]
fn errors_before_setup_are_japanese() {
    let mut s = Sim::new();
    assert!(s.create_scenario("{}").unwrap_err().contains("グラフ"));
    s.load_graph(GRAPH, "").unwrap();
    assert!(s.set_ip(0).unwrap_err().contains("シナリオ"));
    s.create_scenario("{}").unwrap();
    assert!(s.step(0).unwrap_err().contains("IP"));
}

#[test]
fn load_graph_reports_and_rejects() {
    let mut s = Sim::new();
    let info: serde_json::Value =
        serde_json::from_str(&s.load_graph(GRAPH, r#"{"directed":false}"#).unwrap()).unwrap();
    assert_eq!(info["n_nodes"], 300);
    assert!(
        info["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w.as_str().unwrap().contains("無向"))
    );
    assert!(
        s.load_graph(b"1 2\nfoo bar\n", "")
            .unwrap_err()
            .contains("2 行目")
    );
    assert!(s.load_graph(GRAPH, r#"{"format":"xml"}"#).is_err());
}

#[test]
fn step_events_and_views() {
    let mut s = ready();
    let sm: serde_json::Value = serde_json::from_str(&s.step(0).unwrap()).unwrap();
    let n = sm["n_received"].as_u64().unwrap() as usize;
    assert_eq!(s.last_events().unwrap().len(), n);
    assert_eq!(s.ever_flags().unwrap().len(), 300);
    assert_eq!(s.kinds(0).unwrap()[0], 5, "IP");
    assert_eq!(s.ring_angles().unwrap().len(), 300);
    assert_eq!(s.sector_grid(0).unwrap().len() % (72 * 5), 0);
    // m∅（-1）は廃止したので不正な選択になる
    assert!(s.step(-1).unwrap_err().contains("不正な選択"));
    assert!(s.step(99).unwrap_err().contains("存在しません"));
    assert!(s.node_detail(300).is_err());
    let d: serde_json::Value = serde_json::from_str(&s.node_detail(1).unwrap()).unwrap();
    assert_eq!(d["u_int"].as_array().unwrap().len(), 2);
    s.group_series(2, 1, 0, &[]).unwrap();
    s.reset().unwrap();
    assert!(s.last_events().is_err());
}

#[test]
fn edges_among_selection_and_ranking() {
    let s = ready();
    let all: Vec<u32> = (0..300).collect();
    let e = s.edges_among(&all, 1_000_000).unwrap();
    assert!(e.len() / 2 > 1000);
    assert_eq!(s.edges_among(&all, 10).unwrap().len(), 20, "cap で打ち切る");
    assert!(!s.select_nodes(1, 1, 0.0, 7.0).unwrap().is_empty());
    let top = s.centrality_ranking("degree", 5, None).unwrap();
    assert_eq!(top.len(), 5);
    assert!(s.centrality_ranking("pagerank", 3, None).is_ok());
    assert!(s.centrality_ranking("vc", 3, None).is_err());
    let mc: serde_json::Value =
        serde_json::from_str(&s.run_mc(&[0, 1, 2], 10, 1.0).unwrap()).unwrap();
    assert_eq!(mc["rounds"], 3);
    assert!(s.run_mc(&[0], 10, 9007199254740992.0).is_err());
}

#[test]
fn rewind_branch_switch_and_round_data() {
    let mut s = ready();
    for c in [0, 3, 5, 1] {
        s.step(c).unwrap();
    }
    let t: serde_json::Value = serde_json::from_str(&s.branch_tree().unwrap()).unwrap();
    assert_eq!(t["active_path"].as_array().unwrap().len(), 4);
    assert_eq!(t["nodes"].as_array().unwrap().len(), 5, "根 + 4 ラウンド");
    let orig_last = t["current"].as_u64().unwrap() as u32;
    // 過去のラウンドのデータは、そのラウンドを発信した直後の値と一致する
    let r2 = s.round_data(2).unwrap();
    assert_eq!(r2.ever_before.len(), 300);
    // 巻き戻して別の選択 → 木に枝ができる
    s.rewind_to(2).unwrap();
    s.step(4).unwrap();
    let t2: serde_json::Value = serde_json::from_str(&s.branch_tree().unwrap()).unwrap();
    assert_eq!(t2["nodes"].as_array().unwrap().len(), 6);
    assert_eq!(t2["n_lanes"], 2);
    // 元の枝へ戻る。共通は 2 ラウンドまで
    let sw: serde_json::Value = serde_json::from_str(&s.switch_to(orig_last).unwrap()).unwrap();
    assert_eq!(
        (sw["round"].as_u64(), sw["changed_from"].as_u64()),
        (Some(4), Some(2))
    );
    assert_eq!(s.round_data(2).unwrap().summary_json, r2.summary_json);
    assert!(s.rewind_to(9).is_err());
    assert!(s.round_data(9).is_err());
}

#[test]
fn import_record_restores_tree_and_checks_graph() {
    let mut s = ready();
    for c in [0, 3, 5] {
        s.step(c).unwrap();
    }
    s.rewind_to(1).unwrap();
    s.step(2).unwrap();
    let rec = s.export_scenario_json().unwrap();
    let before = s.branch_tree().unwrap();
    let mut s2 = Sim::new();
    s2.load_graph(GRAPH, r#"{"directed":true,"transposed":false}"#)
        .unwrap();
    s2.import_record(&rec).unwrap();
    assert_eq!(s2.branch_tree().unwrap(), before);
    // 違うグラフには読み込めない
    let mut s3 = Sim::new();
    s3.load_graph(
        b"1 2\n2 3\n3 1\n",
        r#"{"directed":true,"transposed":false}"#,
    )
    .unwrap();
    assert!(s3.import_record(&rec).unwrap_err().contains("グラフ"));
}
