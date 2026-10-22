//! 大きなグラフの個別表示（1 万ノード超）のための間引き。点の大きさを密度に合わせて縮め、
//! 辺は表示範囲（ビューポート）内のものを均等に間引いて上限まで描く。拡大するほど細部の辺が出る。

use sim_api::csr::Csr;

/// 点の半径の基準を保つノード数。これを超えると、密度に合わせて縮める。
pub const FULL_SIZE_NODES: usize = 3_000;
/// 点の半径の下限（px）。
pub const MIN_RADIUS_PX: f32 = 1.2;
/// 辺を表示範囲で作り直す（視野に応じた間引き）ようになるノード数。これ以下は全体の辺を一度に作る。
pub const EDGE_LOD_MIN_NODES: usize = 5_000;

/// 描くノード数に応じた半径（px）。`base` は `spec/ui-tokens.json` の基準値（4px）。
pub fn effective_radius(base: f32, n_draw: usize) -> f32 {
    if n_draw <= FULL_SIZE_NODES {
        return base;
    }
    (base * (FULL_SIZE_NODES as f32 / n_draw as f32).sqrt()).clamp(MIN_RADIUS_PX.min(base), base)
}

/// 範囲 `view`（ワールドの [min_x, min_y, max_x, max_y]）の中にある、両端が `nodes`（None = 全ノード）の中にある辺を、
/// `cap` 本までに均等に間引いて返す（[x0, y0, x1, y1, …]）。戻り値の bool は、間引いたか。
pub fn edges_in_view(
    xy: &[f32],
    nodes: Option<&[u32]>,
    csr: &Csr,
    view: [f32; 4],
    cap: usize,
) -> (Vec<f32>, bool) {
    let n = xy.len() / 2;
    let inside = |i: usize| {
        let (x, y) = (xy[2 * i], xy[2 * i + 1]);
        x >= view[0] && x <= view[2] && y >= view[1] && y <= view[3]
    };
    let mut mask = vec![false; n];
    let mut vis: Vec<u32> = Vec::new();
    let mut take = |i: usize| {
        if inside(i) {
            mask[i] = true;
            vis.push(i as u32);
        }
    };
    match nodes {
        Some(ns) => ns.iter().for_each(|&i| take(i as usize)),
        None => (0..n).for_each(&mut take),
    }
    // 1 回目: 候補の辺の数を数える。2 回目: 均等な間隔で選ぶ
    let candidates: usize = vis
        .iter()
        .map(|&a| csr.succ(a).iter().filter(|&&b| mask[b as usize]).count())
        .sum();
    let stride = candidates.div_ceil(cap.max(1)).max(1);
    let mut out = Vec::with_capacity(candidates.min(cap) * 4);
    let mut idx = 0usize;
    for &a in &vis {
        for &b in csr.succ(a) {
            if !mask[b as usize] {
                continue;
            }
            if idx.is_multiple_of(stride) {
                out.extend_from_slice(&[
                    xy[2 * a as usize],
                    xy[2 * a as usize + 1],
                    xy[2 * b as usize],
                    xy[2 * b as usize + 1],
                ]);
            }
            idx += 1;
        }
    }
    (out, stride > 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim_api::graph_io::{GraphFormat, GraphSource, load_graph};

    fn csr(edges: &[(u32, u32)]) -> Csr {
        let s: String = edges.iter().map(|(a, b)| format!("{a} {b}\n")).collect();
        load_graph(&GraphSource {
            bytes: s.into_bytes(),
            format: GraphFormat::EdgeList,
            gzip: false,
            directed: true,
            transposed: false,
        })
        .unwrap()
        .csr
    }

    #[test]
    fn radius_shrinks_with_density_but_stays_in_bounds() {
        assert_eq!(effective_radius(4.0, 100), 4.0);
        assert_eq!(effective_radius(4.0, FULL_SIZE_NODES), 4.0);
        let r10k = effective_radius(4.0, 10_000);
        let r68k = effective_radius(4.0, 68_000);
        assert!(r10k < 4.0 && r68k < r10k && r68k >= MIN_RADIUS_PX);
        assert_eq!(effective_radius(4.0, 100_000_000), MIN_RADIUS_PX);
    }

    #[test]
    fn only_edges_with_both_ends_in_view_are_returned() {
        // ノード 0〜3 は x = 0,1,2,10。視野 x ∈ [-1, 3] には 0,1,2 だけ
        let xy = [0.0, 0.0, 1.0, 0.0, 2.0, 0.0, 10.0, 0.0];
        let c = csr(&[(0, 1), (1, 2), (2, 3), (0, 3)]);
        let (e, truncated) = edges_in_view(&xy, None, &c, [-1.0, -1.0, 3.0, 1.0], 100);
        assert!(!truncated);
        assert_eq!(e, vec![0.0, 0.0, 1.0, 0.0, 1.0, 0.0, 2.0, 0.0]);
        // 視野を広げると全辺が出る
        let (all, _) = edges_in_view(&xy, None, &c, [-1.0, -1.0, 11.0, 1.0], 100);
        assert_eq!(all.len() / 4, 4);
        // 部分集合を指定したときは、その中の辺だけ
        let (sub, _) = edges_in_view(&xy, Some(&[0, 1]), &c, [-1.0, -1.0, 11.0, 1.0], 100);
        assert_eq!(sub.len() / 4, 1);
    }

    #[test]
    fn edges_are_thinned_evenly_up_to_the_cap() {
        // 1 つの星: 0 → 1..=1000。cap 100 で均等に間引く
        let n = 1001usize;
        let mut xy = vec![0.0f32; n * 2];
        for i in 0..n {
            xy[2 * i] = i as f32;
        }
        let edges: Vec<(u32, u32)> = (1..n as u32).map(|b| (0, b)).collect();
        let c = csr(&edges);
        let (e, truncated) = edges_in_view(&xy, None, &c, [-1.0, -1.0, 2000.0, 1.0], 100);
        assert!(truncated);
        let m = e.len() / 4;
        assert!((90..=100).contains(&m), "cap 付近まで出る: {m}");
        // 端点が偏らない（先頭 100 本だけにならない）
        let max_x = e.chunks(4).map(|c| c[2]).fold(0.0f32, f32::max);
        assert!(max_x > 800.0, "遠いノードの辺も含まれる: {max_x}");
    }
}
