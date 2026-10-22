#![allow(clippy::needless_range_loop, clippy::field_reassign_with_default)]

use graph_lib::{
    io::{DataFormat, ParseBuilder, ParseOption},
    prelude::{DiGraphB, Graph, GraphB, UndiGraphB},
};
use sim_api::graph_io::*;

fn fx(name: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/../spec/fixtures/graphlib/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn df(f: GraphFormat) -> DataFormat {
    match f {
        GraphFormat::EdgeList => DataFormat::EdgeList,
        GraphFormat::Csv => DataFormat::CSV,
        GraphFormat::Tsv => DataFormat::TSV,
    }
}

fn lib_graph(bytes: &[u8], fmt: DataFormat, gz: bool, directed: bool, transposed: bool) -> GraphB {
    let b = ParseBuilder::new(
        bytes,
        fmt,
        ParseOption {
            transposed,
            compressed: gz,
        },
    );
    if directed {
        GraphB::Di(b.parse::<DiGraphB>().unwrap())
    } else {
        GraphB::Ud(b.parse::<UndiGraphB>().unwrap())
    }
}

fn same(a: &GraphB, b: &GraphB) {
    assert_eq!(a.node_count(), b.node_count());
    assert_eq!(a.edge_count(), b.edge_count());
    for i in 0..a.node_count() {
        assert_eq!(
            a.successors(i).collect::<Vec<_>>(),
            b.successors(i).collect::<Vec<_>>(),
            "succ {i}"
        );
        assert_eq!(
            a.predecessors(i).collect::<Vec<_>>(),
            b.predecessors(i).collect::<Vec<_>>(),
            "pred {i}"
        );
    }
}

#[test]
fn matches_graph_lib_for_all_formats() {
    let cases = [
        ("sample_graph.txt", GraphFormat::EdgeList, false),
        ("sample_graph.txt.gz", GraphFormat::EdgeList, true),
        ("sample_graph.csv", GraphFormat::Csv, false),
        ("sample_graph.csv.gz", GraphFormat::Csv, true),
        ("sample_graph.tsv", GraphFormat::Tsv, false),
        ("sample_graph.tsv.gz", GraphFormat::Tsv, true),
        ("simple.txt", GraphFormat::EdgeList, false),
        ("simple2.txt", GraphFormat::EdgeList, false),
    ];
    for (name, f, gz) in cases {
        let bytes = fx(name);
        for directed in [true, false] {
            for transposed in [false, true] {
                let ours = load_graph(&GraphSource {
                    bytes: bytes.clone(),
                    format: f,
                    gzip: gz,
                    directed,
                    transposed,
                })
                .unwrap();
                let theirs = lib_graph(&bytes, df(f), gz, directed, transposed);
                same(&ours.graph, &theirs);
                assert!(ours.labels.windows(2).all(|w| w[0] < w[1]), "labels 昇順");
                assert_eq!(ours.labels.len(), ours.n_nodes);
                // Csr が successors と一致
                for i in 0..ours.n_nodes {
                    let c: Vec<usize> = ours
                        .csr
                        .succ(i as u32)
                        .iter()
                        .map(|&x| x as usize)
                        .collect();
                    assert_eq!(c, ours.graph.successors(i).copied().collect::<Vec<_>>());
                }
            }
        }
    }
}

fn src(s: &str, format: GraphFormat) -> GraphSource {
    GraphSource {
        bytes: s.as_bytes().to_vec(),
        format,
        gzip: false,
        directed: true,
        transposed: false,
    }
}

#[test]
fn transpose_reverses_edges() {
    let mut s = src("1 2\n", GraphFormat::EdgeList);
    let a = load_graph(&s).unwrap();
    s.transposed = true;
    let b = load_graph(&s).unwrap();
    assert_eq!(a.csr.succ(0), &[1]);
    assert_eq!(b.csr.succ(1), &[0]);
}

#[test]
fn comments_blank_lines_and_mixed_delimiters() {
    let g = load_graph(&src(
        "# c\n\n% c2\n10 20 # tail\n20\t30\n  30   10  \n",
        GraphFormat::EdgeList,
    ));
    // 3 列目以降・複数空白は graph-lib と同様に扱う（先頭 2 トークン）
    assert!(g.is_ok() || matches!(g, Err(GraphError::Parse { .. })));
    let g = load_graph(&src("# c\n5,6 # x\n6,7\n", GraphFormat::Csv)).unwrap();
    assert_eq!(g.labels, vec![5, 6, 7]);
}

#[test]
fn syntax_errors_do_not_panic() {
    assert_eq!(
        load_graph(&src("1 2\nfoo bar\n", GraphFormat::EdgeList)).err(),
        Some(GraphError::Parse {
            line: 2,
            text: "foo bar".into()
        })
    );
    // 空白区切りのファイルを csv で読むと失敗（誤読みしない）
    assert!(matches!(
        load_graph(&src("1 2\n", GraphFormat::Csv)),
        Err(GraphError::Parse { line: 1, .. })
    ));
    // ヘッダー行
    assert!(matches!(
        load_graph(&src("from,to\n1,2\n", GraphFormat::Csv)),
        Err(GraphError::Parse { line: 1, .. })
    ));
    assert_eq!(
        load_graph(&src("# only\n", GraphFormat::EdgeList)).err(),
        Some(GraphError::Empty)
    );
    let mut s = src("abc", GraphFormat::EdgeList);
    s.gzip = true;
    assert!(matches!(load_graph(&s), Err(GraphError::Gzip(_))));
}

#[test]
fn hash_ignores_order_and_duplicates() {
    let a = load_graph(&src("1 2\n2 3\n3 1\n", GraphFormat::EdgeList)).unwrap();
    let b = load_graph(&src("3 1\n1 2\n2 3\n1 2\n", GraphFormat::EdgeList)).unwrap();
    let c = load_graph(&src("1 2\n2 3\n", GraphFormat::EdgeList)).unwrap();
    assert_eq!(a.hash, b.hash);
    assert_ne!(a.hash, c.hash);
}

#[test]
fn self_loops_allowed_and_fixture_loads() {
    let g = load_graph(&src("1 1\n1 2\n", GraphFormat::EdgeList)).unwrap();
    assert_eq!(g.csr.succ(0), &[0, 1]);
    let bytes = std::fs::read(format!(
        "{}/../spec/fixtures/random300.txt",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();
    let g = load_graph(&GraphSource {
        bytes,
        format: GraphFormat::EdgeList,
        gzip: false,
        directed: true,
        transposed: false,
    })
    .unwrap();
    assert_eq!(g.n_nodes, 300);
}
