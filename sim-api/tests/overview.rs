//! T-14: 全体図、T-13 の群の部分、IP 順位。
#![allow(clippy::needless_range_loop, clippy::field_reassign_with_default)]

use std::sync::Arc;
use std::time::Instant;

use rand::{Rng, SeedableRng, rngs::SmallRng};
use sim_api::centrality::*;
use sim_api::config::{MechanismConfig, ScenarioConfig};
use sim_api::graph_io::*;
use sim_api::groups::*;
use sim_api::log::*;
use sim_api::overview::*;
use sim_api::session::*;
use sim_core::diffusion::shortest_distances_from;

fn graph_from(edges: &[(usize, usize)]) -> LoadedGraph {
    let s: String = edges.iter().map(|(a, b)| format!("{a} {b}\n")).collect();
    load_graph(&GraphSource {
        bytes: s.into_bytes(),
        format: GraphFormat::EdgeList,
        gzip: false,
        directed: true,
        transposed: false,
    })
    .unwrap()
}

fn fixture() -> LoadedGraph {
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

fn random_big(n: usize, deg: usize, seed: u64) -> LoadedGraph {
    let mut r = SmallRng::seed_from_u64(seed);
    let mut e = Vec::new();
    for i in 0..n {
        e.push((i, (i + 1) % n));
        for _ in 0..deg {
            let j = r.random_range(0..n);
            if j != i {
                e.push((i, j));
            }
        }
    }
    graph_from(&e)
}

#[test]
fn rings_match_core_shortest_distances() {
    for g in [
        fixture(),
        graph_from(&[(0, 1), (1, 2), (3, 4), (2, 0), (4, 3), (5, 5)]),
        random_big(500, 2, 3),
    ] {
        for ip in [0u32, (g.n_nodes / 2) as u32] {
            let rings = compute_rings(&g.csr, ip);
            let reference = shortest_distances_from(&g.graph, ip as usize);
            for k in 0..g.n_nodes {
                match reference[k] {
                    Some(d) => assert_eq!(rings.dist[k], d as u32),
                    None => assert_eq!(rings.dist[k], u32::MAX),
                }
            }
            assert_eq!(
                rings.ring_sizes.iter().sum::<u32>() + rings.unreachable,
                g.n_nodes as u32
            );
            assert_eq!(rings.ring_sizes[0], 1);
        }
    }
    let g = graph_from(&[(0, 1), (1, 2), (3, 4)]);
    assert_eq!(compute_rings(&g.csr, 0).unreachable, 2);
}

#[test]
fn radial_layout_properties() {
    let g = graph_from(&[(0, 1), (0, 2), (1, 3), (1, 4), (2, 5), (6, 7)]);
    let rings = compute_rings(&g.csr, 0);
    let lay = radial_layout(&g.csr, &rings, 0);
    let n_reach = (g.n_nodes as u32 - rings.unreachable) as f32;
    assert_eq!(lay.angle[0], 0.0);
    for k in 0..g.n_nodes {
        if rings.dist[k] != u32::MAX {
            assert!(lay.angle[k] >= 0.0 && lay.angle[k] < std::f32::consts::TAU);
        }
    }
    // 行きがけ順: 0,1,3,4,2,5 → pos = 0,1,4,2,3,5
    let pos = |k: usize| (lay.angle[k] / std::f32::consts::TAU * n_reach).round() as u32;
    assert_eq!(
        [pos(0), pos(1), pos(3), pos(4), pos(2), pos(5)],
        [0, 1, 2, 3, 4, 5]
    );
    // 決定的
    let lay2 = radial_layout(&g.csr, &rings, 0);
    assert_eq!(lay.angle, lay2.angle);
}

#[test]
fn radial_layout_subtrees_are_contiguous() {
    let g = random_big(800, 3, 9);
    let rings = compute_rings(&g.csr, 5);
    let lay = radial_layout(&g.csr, &rings, 5);
    let n = g.n_nodes;
    let nr = n as f32;
    let pos: Vec<u32> = (0..n)
        .map(|k| (lay.angle[k] / std::f32::consts::TAU * nr).round() as u32)
        .collect();
    let mut seen = vec![false; n];
    assert!(pos.iter().all(|&p| (p as usize) < n));
    for &p in &pos {
        assert!(!seen[p as usize], "pos は一意");
        seen[p as usize] = true;
    }
    // 部分木の大きさ（親の配列から）
    let mut size = vec![1u32; n];
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by_key(|&k| std::cmp::Reverse(rings.dist[k]));
    for &k in &order {
        let p = lay.parent[k];
        if p != u32::MAX {
            size[p as usize] += size[k];
        }
    }
    for k in 0..n {
        // 子孫の pos は [pos(k), pos(k)+size) に入る
        let mut cur = k as u32;
        while lay.parent[cur as usize] != u32::MAX {
            let anc = lay.parent[cur as usize] as usize;
            assert!(pos[k] >= pos[anc] && pos[k] < pos[anc] + size[anc]);
            cur = anc as u32;
        }
    }
}

#[test]
fn aggregate_and_select() {
    let g = fixture();
    let mut cfg = ScenarioConfig::default();
    cfg.receipt_probability = 0.7;
    let scn = Arc::new(Scenario::new(g, cfg).unwrap());
    let mut s = Session::new(Arc::clone(&scn), 0).unwrap();
    for r in 0..5u8 {
        s.step(Choice::Msg(r)).unwrap();
    }
    let rings = compute_rings(&scn.graph.csr, 0);
    let lay = radial_layout(&scn.graph.csr, &rings, 0);
    let kinds = s.kinds(IntMode::Cumulative);
    let grid = aggregate(&rings, &lay, &kinds);
    for (ring, &size) in rings.ring_sizes.iter().enumerate() {
        let tot: u32 = (0..N_SECTORS as usize)
            .map(|sec| grid.cells[ring * N_SECTORS as usize + sec].n)
            .sum();
        assert_eq!(tot, size);
    }
    let recv_total: u32 = grid.cells.iter().map(|c| c.n_recv).sum();
    let expect = kinds
        .iter()
        .filter(|k| !matches!(k, NodeKind::Unreceived | NodeKind::Ip))
        .count() as u32;
    assert_eq!(recv_total, expect);
    for c in &grid.cells {
        assert!(c.n_int + c.n_ext + c.n_both <= c.n_recv);
    }
    // 全範囲の選択 = 到達可能な全ノード
    let all = select_nodes(&rings, &lay, 0, 100, 0.0, std::f32::consts::TAU).unwrap();
    assert_eq!(all.len() as u32, 300 - rings.unreachable);
    let part = select_nodes(&rings, &lay, 1, 1, 0.0, std::f32::consts::PI).unwrap();
    assert!(
        part.iter()
            .all(|&k| rings.dist[k as usize] == 1 && lay.angle[k as usize] < std::f32::consts::PI)
    );
}

#[test]
fn select_nodes_rejects_over_cap() {
    let e: Vec<(usize, usize)> = (1..=EXPAND_CAP + 500).map(|i| (0, i)).collect();
    let g = graph_from(&e);
    let rings = compute_rings(&g.csr, 0);
    let lay = radial_layout(&g.csr, &rings, 0);
    let r = select_nodes(&rings, &lay, 1, 1, 0.0, std::f32::consts::TAU);
    assert!(matches!(r, Err(TooMany(n)) if n > EXPAND_CAP));
    assert!(select_nodes(&rings, &lay, 0, 0, 0.0, std::f32::consts::TAU).is_ok());
}

#[test]
fn radial_layout_speed_at_twitch_scale() {
    let g = random_big(7000, 5, 1);
    let t = Instant::now();
    let rings = compute_rings(&g.csr, 0);
    let lay = radial_layout(&g.csr, &rings, 0);
    let ms = t.elapsed().as_secs_f64() * 1000.0;
    println!("radial_layout(7000 nodes): {ms:.2} ms");
    assert_eq!(lay.angle.len(), 7000);
    if !cfg!(debug_assertions) {
        assert!(ms < 100.0, "{ms} ms");
    }
}

#[test]
fn group_series_members_and_sizes() {
    let g = fixture();
    let mut cfg = ScenarioConfig::default();
    cfg.receipt_probability = 0.8;
    cfg.mechanisms = MechanismConfig::balanced();
    let scn = Arc::new(Scenario::new(g, cfg).unwrap());
    let mut s = Session::new(scn, 0).unwrap();
    for r in 0..8u8 {
        s.step(Choice::Msg(r % 6)).unwrap();
    }
    let log = s.log();
    for (sel, kind) in [
        (GroupSel::IntOnly, NodeKind::Internal),
        (GroupSel::ExtOnly, NodeKind::External),
        (GroupSel::Both, NodeKind::Both),
        (GroupSel::ReceivedOnly, NodeKind::ReceivedOnly),
    ] {
        let gs = s.group_series(&sel, 8, IntMode::Cumulative);
        let expect = s
            .kinds(IntMode::Cumulative)
            .iter()
            .filter(|&&k| k == kind)
            .count() as u32;
        assert_eq!(gs.members, expect);
        assert_eq!(gs.int.mean.len(), 9);
        assert_eq!(gs.size_by_round.len(), 9);
        // 各ラウンドのそのつどの分類の大きさ
        for r in 0..=8usize {
            let n = log
                .row_flags(r)
                .iter()
                .enumerate()
                .filter(|&(k, &f)| kind_of(f, IntMode::Cumulative, k == 0) == kind)
                .count() as u32;
            assert_eq!(gs.size_by_round[r], n);
        }
        assert_eq!(*gs.size_by_round.last().unwrap(), gs.members);
        if gs.members > 0 {
            for r in 0..=8 {
                assert!(gs.int.p10[r] <= gs.int.p50[r] && gs.int.p50[r] <= gs.int.p90[r]);
            }
        }
    }
    // メンバーを固定してさかのぼる: 第 0 行は全員 u0
    let gs = s.group_series(&GroupSel::Both, 8, IntMode::Cumulative);
    if gs.members > 0 {
        assert_eq!(gs.int.mean[0], -1.0);
        assert_eq!(gs.size_by_round[0], 0);
    }
    let c = s.group_series(&GroupSel::Custom(vec![1, 2, 3]), 4, IntMode::Cumulative);
    assert_eq!(c.members, 3);
    assert_eq!(c.size_by_round, vec![3; 5]);
}

#[test]
fn centrality_rankings() {
    // 0→1, 0→2, 0→3, 1→2
    let g = graph_from(&[(0, 1), (0, 2), (0, 3), (1, 2)]);
    assert_eq!(out_degree(&g.csr), vec![3, 1, 0, 0]);
    assert_eq!(ranking(&[1.0, 5.0, 5.0, 0.0], 3), vec![1, 2, 0]);
    let pr = pagerank_reverse(&g.csr);
    assert!((pr.iter().sum::<f64>() - 1.0).abs() < 1e-9, "確率分布");
    // 逆向きでは 0 へ流れ込む（0 は多くのノードの先行）ので 0 が最大
    assert_eq!(ranking(&pr, 1), vec![0]);
    let csv = "node,degree,pagerank,vc\n10,3,0.5,7.5\n11,1,0.2,1.0\n";
    let g2 = graph_from(&[(10, 11), (11, 12)]);
    let vc = metric_from_csv(csv, &g2.labels, "vc").unwrap();
    assert_eq!(vc[0], 7.5);
    assert_eq!(vc[1], 1.0);
    assert_eq!(vc[2], f64::NEG_INFINITY);
    assert!(metric_from_csv(csv, &g2.labels, "zzz").is_err());
}
