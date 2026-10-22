//! グラフの読み込み（7.10）。graph-lib の `ParseBuilder` は構文エラーで panic するので使わず、
//! 同じ規則で自前に解析して `Result` を返す（WASM では panic が致命的なため）。

use std::io::Read;

use graph_lib::prelude::{DiGraphB, Graph, GraphB, UndiGraphB};

use crate::csr::Csr;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphFormat {
    EdgeList,
    Csv,
    Tsv,
}

#[derive(Clone, Debug)]
pub struct GraphSource {
    pub bytes: Vec<u8>,
    pub format: GraphFormat,
    pub gzip: bool,
    pub directed: bool,
    pub transposed: bool,
}

pub struct LoadedGraph {
    pub graph: GraphB,
    pub csr: Csr,
    /// 添字 = ノード番号、値 = 元のラベル（昇順）。
    pub labels: Vec<i64>,
    pub n_nodes: usize,
    pub n_edges: usize,
    /// 正規化した辺リストの FNV-1a 64bit。
    pub hash: u64,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum GraphError {
    #[error("{line} 行目を解析できません: {text}")]
    Parse { line: usize, text: String },
    #[error("gzip の展開に失敗しました: {0}")]
    Gzip(String),
    #[error("文字コードが UTF-8 ではありません")]
    Utf8,
    #[error("辺が 1 本もありません")]
    Empty,
    #[error("ノード数が多すぎます（{0}）")]
    TooManyNodes(usize),
}

fn rules(f: GraphFormat) -> (&'static [&'static str], &'static [char]) {
    match f {
        GraphFormat::EdgeList => (&["#", "%"], &[' ', '\t']),
        GraphFormat::Csv => (&["#"], &[',']),
        GraphFormat::Tsv => (&["#"], &['\t']),
    }
}

/// 解析済みの辺（転置適用後）。
pub fn parse_edges(src: &GraphSource) -> Result<Vec<(i64, i64)>, GraphError> {
    let mut text = Vec::new();
    if src.gzip {
        flate2::read::GzDecoder::new(&src.bytes[..])
            .read_to_end(&mut text)
            .map_err(|e| GraphError::Gzip(e.to_string()))?;
    } else {
        text = src.bytes.clone();
    }
    let text = String::from_utf8(text).map_err(|_| GraphError::Utf8)?;
    let (comments, delims) = rules(src.format);
    let mut edges = Vec::new();
    'outer: for (pos, l) in text.lines().enumerate() {
        let mut s = l.trim();
        for c in comments {
            if let Some(i) = s.find(c) {
                s = s.split_at(i).0.trim();
            }
        }
        if s.is_empty() {
            continue;
        }
        let bad = || GraphError::Parse {
            line: pos + 1,
            text: l.to_string(),
        };
        for &d in delims {
            let mut it = s.split(d);
            let (Some(us), Some(vs)) = (it.next(), it.next()) else {
                continue;
            };
            let u = us.trim().parse::<i64>().map_err(|_| bad())?;
            let v = vs.trim().parse::<i64>().map_err(|_| bad())?;
            edges.push(if src.transposed { (v, u) } else { (u, v) });
            continue 'outer;
        }
        return Err(bad());
    }
    Ok(edges)
}

fn fnv1a(h: &mut u64, bytes: &[u8]) {
    for &b in bytes {
        *h ^= b as u64;
        *h = h.wrapping_mul(0x100_0000_01b3);
    }
}

/// 辺の並び順に依存しないハッシュ。無向は (小, 大) に正規化する。
pub fn edge_hash(edges: &[(i64, i64)], directed: bool) -> u64 {
    let mut v: Vec<(i64, i64)> = edges
        .iter()
        .map(|&(a, b)| if directed || a <= b { (a, b) } else { (b, a) })
        .collect();
    v.sort_unstable();
    v.dedup();
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    fnv1a(&mut h, &[directed as u8]);
    for (a, b) in v {
        fnv1a(&mut h, &a.to_le_bytes());
        fnv1a(&mut h, &b.to_le_bytes());
    }
    h
}

pub fn load_graph(src: &GraphSource) -> Result<LoadedGraph, GraphError> {
    let edges = parse_edges(src)?;
    if edges.is_empty() {
        return Err(GraphError::Empty);
    }
    let hash = edge_hash(&edges, src.directed);
    let graph = if src.directed {
        GraphB::Di(DiGraphB::from(&edges[..]))
    } else {
        GraphB::Ud(UndiGraphB::from(&edges[..]))
    };
    let mut labels: Vec<i64> = edges.iter().flat_map(|&(a, b)| [a, b]).collect();
    labels.sort_unstable();
    labels.dedup();
    let n_nodes = graph.node_count();
    debug_assert_eq!(n_nodes, labels.len());
    if n_nodes > u32::MAX as usize {
        return Err(GraphError::TooManyNodes(n_nodes));
    }
    let csr = Csr::from_graph(&graph);
    Ok(LoadedGraph {
        n_edges: graph.edge_count(),
        n_nodes,
        csr,
        graph,
        labels,
        hash,
    })
}
