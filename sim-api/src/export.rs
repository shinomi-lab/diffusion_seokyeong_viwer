//! 書き出し（7.13）。Parquet は `sim-api` では作らない（INV-10。desktop が行に変換する）。

use std::fmt::Write;

use serde::{Deserialize, Serialize};

use crate::config::ScenarioConfig;
use crate::log::{Choice, EV_REC, EV_RECCHAIN, IntMode, kind_of};
use crate::session::{Scenario, Session};

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ExportError {
    #[error(
        "拡張機構（M1）が有効なため、既存のバッチ実験用の設定は出せません。P3-5 の verify で検証してください"
    )]
    BatchIncompatible,
    #[error("系列文字列を解析できません: {0}")]
    Sequence(String),
    #[error("JSON を処理できません: {0}")]
    Json(String),
    #[error("分岐ツリーを復元できません: {0}")]
    Tree(String),
}

/// グラフの出自（記録 JSON に入れる）。
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct GraphMeta {
    pub source_name: String,
    pub directed: bool,
    pub transposed: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct GraphRecord {
    pub hash: u64,
    pub n_nodes: usize,
    pub n_edges: usize,
    pub directed: bool,
    pub transposed: bool,
    pub source_name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct IpRecord {
    pub id: u32,
    pub label: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BranchRecord {
    pub id: u32,
    pub parent: u32,
    pub round: u32,
    pub choice: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScenarioRecord {
    pub version: u32,
    pub graph: GraphRecord,
    pub config: ScenarioConfig,
    pub ip: IpRecord,
    pub branch_tree: Vec<BranchRecord>,
    pub active_path: Vec<u32>,
    /// 左端がラウンド 1。
    pub sequence: String,
}

pub fn parse_sequence(s: &str) -> Result<Vec<Choice>, ExportError> {
    s.chars()
        .map(|c| {
            c.to_digit(16)
                .map(|d| Choice::Msg(d as u8))
                .ok_or_else(|| ExportError::Sequence(format!("不正な文字 '{c}'")))
        })
        .collect()
}

pub fn scenario_record(s: &Session, meta: &GraphMeta) -> ScenarioRecord {
    let scn = s.scenario();
    let seq = s.sequence_string();
    // 分岐ツリー全体（根を除く）。現在の枝は active_path
    let view = s.tree_view();
    let branch_tree: Vec<BranchRecord> = view
        .nodes
        .iter()
        .filter(|n| n.id != 0)
        .map(|n| BranchRecord {
            id: n.id,
            parent: n.parent.unwrap_or(0),
            round: n.round,
            choice: n.choice.clone(),
        })
        .collect();
    ScenarioRecord {
        version: 1,
        graph: GraphRecord {
            hash: scn.graph.hash,
            n_nodes: scn.graph.n_nodes,
            n_edges: scn.graph.n_edges,
            directed: meta.directed,
            transposed: meta.transposed,
            source_name: meta.source_name.clone(),
        },
        config: scn.cfg.clone(),
        ip: IpRecord {
            id: s.ip(),
            label: scn.graph.labels[s.ip() as usize],
        },
        active_path: view.active_path.clone(),
        branch_tree,
        sequence: seq,
    }
}

pub fn scenario_json(s: &Session, meta: &GraphMeta) -> Result<String, ExportError> {
    serde_json::to_string_pretty(&scenario_record(s, meta))
        .map_err(|e| ExportError::Json(e.to_string()))
}

impl ScenarioRecord {
    pub fn from_json(s: &str) -> Result<Self, ExportError> {
        serde_json::from_str(s).map_err(|e| ExportError::Json(e.to_string()))
    }

    pub fn choices(&self) -> Result<Vec<Choice>, ExportError> {
        parse_sequence(&self.sequence)
    }

    /// 発信前のセッションに、記録した分岐ツリー全体と現在の枝を復元する（F09・F14）。
    pub fn restore_tree(&self, session: &mut Session) -> Result<(), ExportError> {
        let nodes: Vec<(u32, u32, Choice)> = self
            .branch_tree
            .iter()
            .map(|b| {
                let c = parse_sequence(&b.choice)?;
                match c.as_slice() {
                    [one] => Ok((b.id, b.parent, *one)),
                    _ => Err(ExportError::Tree(format!(
                        "節 {} の選択 '{}' が不正です",
                        b.id, b.choice
                    ))),
                }
            })
            .collect::<Result<_, _>>()?;
        session
            .load_tree(&nodes, &self.active_path)
            .map_err(|e| ExportError::Tree(e.to_string()))
    }
}

pub const ROUND_CSV_HEADER: &str = "round,msg,new_share,new_xact,cum_xact,received,mean_u_int,mean_u_ext,p10_u_int,p50_u_int,p90_u_int,p10_u_ext,p50_u_ext,p90_u_ext,mean_u_int_all,mean_u_ext_all,rec_received,rec_share,rec_xact,chain_received,chain_xact,cum_rec";

/// ラウンド別結果。`mean_*`・分位点は受信済みノード、`*_all` は IP 以外の全ノード。
pub fn round_rows_csv(s: &Session) -> String {
    let mut out = String::from(ROUND_CSV_HEADER);
    out.push('\n');
    for r in s.summaries() {
        let (a, b) = (&r.recv, &r.all);
        let _ = writeln!(
            out,
            "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
            r.round,
            r.choice.to_char(),
            r.n_share,
            r.new_xact,
            r.cum_xact,
            r.n_received,
            a.mean_int,
            a.mean_ext,
            a.q_int[0],
            a.q_int[1],
            a.q_int[2],
            a.q_ext[0],
            a.q_ext[1],
            a.q_ext[2],
            b.mean_int,
            b.mean_ext,
            r.rec_received,
            r.rec_share,
            r.rec_xact,
            r.chain_received,
            r.chain_xact,
            r.cum_rec
        );
    }
    out
}

pub const EVENT_CSV_HEADER: &str =
    "round,wave,node,node_label,from,from_label,u_int,u_ext,kind,via";

/// ノード別イベント。`round` を指定するとそのラウンドだけ。`kind` はそのラウンド終了時の分類（累積）。
pub fn events_csv(s: &Session, round: Option<u32>) -> String {
    let labels = &s.scenario().graph.labels;
    let mut out = String::from(EVENT_CSV_HEADER);
    out.push('\n');
    for (i, rs) in s.summaries().enumerate() {
        let r = i as u32 + 1;
        if round.is_some_and(|x| x != r) {
            continue;
        }
        let ev = &s.event_columns(r).expect("存在するラウンド");
        let flags = s.log().row_flags(r as usize);
        for k in 0..ev.len() {
            let node = ev.node[k] as usize;
            let kind = kind_of(flags[node], IntMode::Cumulative, node as u32 == s.ip());
            let _ = writeln!(
                out,
                "{},{},{},{},{},{},{},{},{},{}",
                rs.round,
                ev.wave[k],
                node,
                labels[node],
                ev.from[k],
                if ev.from[k] == crate::engine::FROM_REC {
                    -1
                } else {
                    labels[ev.from[k] as usize]
                },
                ev.u_int[k],
                ev.u_ext[k],
                kind.name(),
                if ev.flags[k] & EV_REC != 0 {
                    "rec"
                } else if ev.flags[k] & EV_RECCHAIN != 0 {
                    "rec_chain"
                } else {
                    "follow"
                }
            );
        }
    }
    out
}

/// TOML の基本文字列。`{:?}` は結合文字を `\u{..}` にしてしまい TOML では不正なので使わない。
fn toml_str(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '\\' => o.push_str("\\\\"),
            '"' => o.push_str("\\\""),
            '\n' => o.push_str("\\n"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                let _ = write!(o, "\\u{:04X}", c as u32);
            }
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

/// 既存の `runner brute-force` が読める設定。機構が 1 つでも有効なら `Err(BatchIncompatible)`（D2）。
// 仮定: バッチは `num_rounds` が系列長と較正 T を兼ねるので、`calibration_rounds` を出す。
// 仮定: バッチは IP をサンプリングで決めるため、IP は指定できない（ip_sampling は size=1）。
pub fn batch_toml(
    scn: &Scenario,
    graph_path: &str,
    meta: &GraphMeta,
    ip_seed: u64,
    diffusion_sample_size: u32,
) -> Result<String, ExportError> {
    let c = &scn.cfg;
    if c.mechanisms.any_enabled() || c.recommender.enabled {
        return Err(ExportError::BatchIncompatible);
    }
    let mut s = String::new();
    s.push_str("# 情報拡散ライブビューアから出力。runner brute-force 用。\n");
    s.push_str("# 注意: num_rounds は系列長と較正 T を兼ねる。ip_sampling はバッチ側で IP を決める（画面で選んだ IP とは別）。\n");
    s.push_str("message_space = [\n");
    for m in &c.message_space {
        let _ = writeln!(
            s,
            "  {{ internal_stimulus = {:?}, external_stimulus = {:?} }},",
            m.internal_stimulus, m.external_stimulus
        );
    }
    s.push_str("]\n");
    let _ = writeln!(s, "num_rounds = {}", c.calibration_rounds);
    let _ = writeln!(s, "receipt_probability = {:?}", c.receipt_probability);
    for (k, p) in [
        ("internal_parameters", &c.internal_parameters),
        ("external_parameters", &c.external_parameters),
    ] {
        let _ = writeln!(
            s,
            "{k} = {{ beta = {:?}, gamma = {:?}, initial_utility = {:?} }}",
            p.beta, p.gamma, p.initial_utility
        );
    }
    let _ = writeln!(
        s,
        "graph = {{ path = {}, directed = {}, transposed = {}, format = \"edgelist\" }}",
        toml_str(graph_path),
        meta.directed,
        meta.transposed
    );
    let _ = writeln!(s, "ip_sampling = {{ size = 1, seed = {ip_seed} }}");
    let _ = writeln!(
        s,
        "user_sampling = {{ size = {}, seed = {} }}",
        c.user_index + 1,
        c.user_seed
    );
    let _ = writeln!(s, "diffusion_sample_size = {diffusion_sample_size}");
    Ok(s)
}
