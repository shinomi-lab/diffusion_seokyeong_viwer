#![allow(
    clippy::needless_range_loop,
    clippy::field_reassign_with_default,
    clippy::type_complexity
)]
//! T-15: 巻き戻しと分岐の決定性。巻き戻して同じ選択を再生すると、状態・ログ・イベント列・要約・乱数が**ビット一致**する。

use std::sync::Arc;

use rand::RngCore;
use sim_api::config::{MechanismConfig, ScenarioConfig};
use sim_api::export::{GraphMeta, ScenarioRecord, scenario_json};
use sim_api::graph_io::*;
use sim_api::log::Choice;
use sim_api::session::{Scenario, Session, SessionError};

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

fn scenario(mech: bool) -> Arc<Scenario> {
    let mut cfg = ScenarioConfig::default();
    cfg.receipt_probability = 0.7;
    cfg.user_seed = 5;
    cfg.diffusion_seed = 11;
    if mech {
        cfg.mechanisms = MechanismConfig::balanced();
    }
    Arc::new(Scenario::new(graph(), cfg).unwrap())
}

fn session(scn: &Arc<Scenario>) -> Session {
    Session::new(Arc::clone(scn), 3).unwrap()
}

/// 20 ラウンドの系列。
fn seq20() -> Vec<Choice> {
    use Choice::*;
    vec![
        Msg(0),
        Msg(3),
        Msg(5),
        Msg(2),
        Msg(1),
        Msg(1),
        Msg(4),
        Msg(2),
        Msg(0),
        Msg(5),
        Msg(4),
        Msg(3),
        Msg(3),
        Msg(0),
        Msg(4),
        Msg(1),
        Msg(2),
        Msg(5),
        Msg(0),
        Msg(3),
    ]
}

fn run(s: &mut Session, seq: &[Choice]) {
    for &c in seq {
        s.step(c).unwrap();
    }
}

/// 状態のすべて（ビット単位）。
#[derive(PartialEq, Debug)]
struct Snap {
    round: u32,
    pop: Vec<(u64, u64, u64, u64, bool, u8, u32)>,
    log: Vec<Vec<u32>>,
    summaries: Vec<String>,
    events: Vec<String>,
    ever: Vec<u8>,
    next_rng: u64,
    path: Vec<u32>,
}

fn snap(s: &Session) -> Snap {
    let pop = s
        .population()
        .states
        .iter()
        .enumerate()
        .map(|(k, st)| {
            (
                st.w_int.to_bits(),
                st.w_ext.to_bits(),
                st.u_int.to_bits(),
                st.u_ext.to_bits(),
                st.ext_done,
                st.flags,
                s.population().recv_round[k],
            )
        })
        .collect();
    let log = (0..s.log().n_rows())
        .map(|r| {
            let mut row: Vec<u32> = s.log().row_u_int(r).iter().map(|v| v.to_bits()).collect();
            row.extend(s.log().row_u_ext(r).iter().map(|v| v.to_bits()));
            row.extend(s.log().row_flags(r).iter().map(|&f| f as u32));
            row
        })
        .collect();
    let summaries = s
        .summaries()
        .map(|x| serde_json::to_string(x).unwrap())
        .collect();
    let events = (1..=s.round())
        .map(|r| format!("{:?}", s.event_columns(r).unwrap()))
        .collect();
    let mut rng = s.rng().clone();
    Snap {
        round: s.round(),
        pop,
        log,
        summaries,
        events,
        ever: s.ever_flags(),
        next_rng: rng.next_u64(),
        path: s.active_path().to_vec(),
    }
}

#[test]
fn rewind_then_replay_is_bit_identical() {
    for mech in [false, true] {
        let scn = scenario(mech);
        let seq = seq20();
        let mut full = session(&scn);
        run(&mut full, &seq);
        let full_snap = snap(&full);
        // チェックポイントの境界（8, 16）とその前後、先頭・末尾付近
        for k in [0u32, 1, 7, 8, 9, 15, 16, 17, 19] {
            let mut x = session(&scn);
            run(&mut x, &seq);
            x.rewind_to(k).unwrap();
            // 最初の k ラウンドだけを新しく回したセッションと、全く同じ状態になる
            let mut reference = session(&scn);
            run(&mut reference, &seq[..k as usize]);
            assert_eq!(
                snap(&x),
                snap(&reference),
                "mech={mech} k={k}: 巻き戻し直後の状態"
            );
            // 残りを再生すると、元の最終状態に戻る
            run(&mut x, &seq[k as usize..]);
            assert_eq!(snap(&x), full_snap, "mech={mech} k={k}: 再生後の状態");
        }
    }
}

#[test]
fn branching_and_switching_back_reproduces_each_branch() {
    let scn = scenario(true);
    let seq = seq20();
    let mut s = session(&scn);
    run(&mut s, &seq[..12]);
    let orig = snap(&s);
    let orig_last = *s.active_path().last().unwrap();
    s.rewind_to(5).unwrap();
    // 同じ選択を発信し直しても節は増えない
    let nodes_before = s.tree().nodes().len();
    s.step(seq[5]).unwrap();
    assert_eq!(s.tree().nodes().len(), nodes_before);
    s.rewind_to(5).unwrap();
    // 別の選択で分岐
    let alt = [Choice::Msg(2), Choice::Msg(1), Choice::Msg(4)];
    run(&mut s, &alt);
    let branch = snap(&s);
    let branch_last = *s.active_path().last().unwrap();
    assert_eq!(s.tree().nodes().len(), nodes_before + 3);
    assert_ne!(branch.summaries, orig.summaries);
    // 元の枝へ戻る → 元と一致
    s.switch_to(orig_last).unwrap();
    assert_eq!(snap(&s), orig);
    // 分岐した枝へ戻る → 分岐と一致
    s.switch_to(branch_last).unwrap();
    assert_eq!(snap(&s), branch);
    // 木の見え方: 分岐点で 2 本、最初の子は同じ段・2 本目は新しい段
    let v = s.tree_view();
    let lanes: Vec<u32> = v.nodes.iter().map(|n| n.lane).collect();
    assert_eq!(v.n_lanes, 2);
    let fork = s
        .tree()
        .nodes()
        .iter()
        .filter(|n| n.round == 6)
        .map(|n| n.id)
        .collect::<Vec<_>>();
    assert_eq!(fork.len(), 2);
    assert_eq!(lanes[fork[0] as usize], lanes[0]);
    assert_ne!(lanes[fork[1] as usize], lanes[0]);
}

#[test]
fn invalid_rewind_and_node_are_errors() {
    let scn = scenario(false);
    let mut s = session(&scn);
    run(&mut s, &seq20()[..3]);
    assert_eq!(s.rewind_to(4).err(), Some(SessionError::InvalidRound(4)));
    assert_eq!(s.switch_to(999).err(), Some(SessionError::InvalidNode(999)));
}

#[test]
fn scenario_record_round_trips_the_whole_tree() {
    let scn = scenario(true);
    let seq = seq20();
    let mut s = session(&scn);
    run(&mut s, &seq[..10]);
    s.rewind_to(4).unwrap();
    run(&mut s, &[Choice::Msg(2), Choice::Msg(2)]);
    s.rewind_to(8).unwrap_err(); // 現在は 6 ラウンド。先へは戻れない
    s.switch_to(10).unwrap(); // 元の枝の末尾（id 10）
    let meta = GraphMeta {
        source_name: "random300".into(),
        directed: true,
        transposed: false,
    };
    let json = scenario_json(&s, &meta).unwrap();
    let rec = ScenarioRecord::from_json(&json).unwrap();
    assert_eq!(rec.branch_tree.len(), 12);
    assert_eq!(rec.active_path.len(), 10);
    // 発信前のセッションに復元 → 木・現在の枝・状態が一致
    let mut s2 = Session::new(
        Arc::new(Scenario::new(graph(), rec.config.clone()).unwrap()),
        rec.ip.id,
    )
    .unwrap();
    rec.restore_tree(&mut s2).unwrap();
    assert_eq!(snap(&s2), snap(&s));
    assert_eq!(
        serde_json::to_string(&s2.tree_view()).unwrap(),
        serde_json::to_string(&s.tree_view()).unwrap()
    );
}
