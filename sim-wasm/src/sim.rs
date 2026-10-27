//! `sim-api` を JS から使う窓口の実体（純 Rust。ネイティブでもテストできる）。
//! モデルの処理は持たず、`sim-api` の呼び出しと状態の保持だけを行う。エラーは日本語の文字列。

use std::sync::Arc;

use serde::Deserialize;
use sim_api::centrality;
use sim_api::config::ScenarioConfig;
use sim_api::export::{self, GraphMeta};
use sim_api::graph_io::{GraphFormat, GraphSource, LoadedGraph, load_graph};
use sim_api::groups::GroupSel;
use sim_api::log::{Choice, EventColumns, IntMode};
use sim_api::mc::{McMode, McSpec, run_mc};
use sim_api::overview::{self, N_SECTORS, RadialLayout, Rings};
use sim_api::params::{param_specs, rec_param_specs};
use sim_api::session::{Scenario, Session};

/// JS で扱える整数の上限（シード用）。
const MAX_SEED: f64 = 9_007_199_254_740_992.0;

#[derive(Deserialize)]
#[serde(default)]
struct GraphOpts {
    format: String,
    gzip: bool,
    directed: bool,
    transposed: bool,
    name: String,
}

impl Default for GraphOpts {
    fn default() -> Self {
        // 既定は transposed = true（入力が「A→B＝A が B をフォロー」の形式のため。9.7）
        Self {
            format: "edgelist".into(),
            gzip: false,
            directed: true,
            transposed: true,
            name: String::new(),
        }
    }
}

/// 過去のラウンド 1 つぶんのデータ。
pub struct RoundData {
    pub summary_json: String,
    pub events: EventColumns,
    pub ever_before: Vec<u8>,
    pub ever_after: Vec<u8>,
}

struct IpView {
    rings: Rings,
    layout: RadialLayout,
}

#[derive(Default)]
pub struct Sim {
    source: Option<(GraphSource, GraphMeta)>,
    graph_info: Option<(usize, usize, u64)>,
    scenario: Option<Arc<Scenario>>,
    session: Option<Session>,
    view: Option<IpView>,
}

fn json<T: serde::Serialize>(v: &T) -> Result<String, String> {
    serde_json::to_string(v).map_err(|e| format!("JSON の作成に失敗しました: {e}"))
}

fn check_seed(v: f64, name: &str) -> Result<u64, String> {
    if !(0.0..MAX_SEED).contains(&v) || v.fract() != 0.0 {
        return Err(format!(
            "{name} は 0 以上 2^53 未満の整数である必要があります"
        ));
    }
    Ok(v as u64)
}

impl Sim {
    pub fn new() -> Self {
        Self::default()
    }

    fn scn(&self) -> Result<&Arc<Scenario>, String> {
        self.scenario
            .as_ref()
            .ok_or("シナリオが未作成です（createScenario を先に呼んでください）".to_string())
    }

    fn sess(&self) -> Result<&Session, String> {
        self.session
            .as_ref()
            .ok_or("IP が未選択です（setIp を先に呼んでください）".to_string())
    }

    pub fn load_graph(&mut self, bytes: &[u8], opts_json: &str) -> Result<String, String> {
        let o: GraphOpts = if opts_json.trim().is_empty() {
            GraphOpts::default()
        } else {
            serde_json::from_str(opts_json)
                .map_err(|e| format!("グラフ設定を読み込めません: {e}"))?
        };
        let format = match o.format.as_str() {
            "edgelist" | "txt" => GraphFormat::EdgeList,
            "csv" => GraphFormat::Csv,
            "tsv" => GraphFormat::Tsv,
            f => return Err(format!("未知のグラフ形式です: {f}")),
        };
        let src = GraphSource {
            bytes: bytes.to_vec(),
            format,
            gzip: o.gzip,
            directed: o.directed,
            transposed: o.transposed,
        };
        let g = load_graph(&src).map_err(|e| e.to_string())?;
        let mut warnings = Vec::new();
        if g.n_nodes > sim_api::overview::EXPAND_CAP {
            warnings.push(format!(
                "ノード数が {} を超えています。全体図から始まり、個別展開は {} ノードまでです",
                sim_api::overview::EXPAND_CAP,
                sim_api::overview::EXPAND_CAP
            ));
        }
        if !o.directed {
            warnings.push("有向と無向の結果は比較できません".to_string());
        }
        let info = serde_json::json!({ "n_nodes": g.n_nodes, "n_edges": g.n_edges, "hash": g.hash, "directed": o.directed, "transposed": o.transposed, "warnings": warnings });
        self.graph_info = Some((g.n_nodes, g.n_edges, g.hash));
        self.source = Some((
            src,
            GraphMeta {
                source_name: o.name,
                directed: o.directed,
                transposed: o.transposed,
            },
        ));
        self.scenario = None;
        self.session = None;
        self.view = None;
        Ok(info.to_string())
    }

    pub fn param_specs(&self) -> Result<String, String> {
        json(&param_specs())
    }

    /// 推薦のスライダーの登録簿（`recommender-design.md` 5 章）。
    pub fn rec_param_specs(&self) -> Result<String, String> {
        json(&rec_param_specs())
    }

    pub fn create_scenario(&mut self, config_json: &str) -> Result<String, String> {
        let (src, _) = self.source.as_ref().ok_or("グラフが未読み込みです")?;
        let cfg = ScenarioConfig::from_json(config_json).map_err(|e| e.to_string())?;
        // Scenario はグラフを所有するので、保持している元データから作り直す
        let g: LoadedGraph = load_graph(src).map_err(|e| e.to_string())?;
        let scn = Scenario::new(g, cfg).map_err(|e| e.to_string())?;
        let summary = serde_json::json!({
            "config": scn.cfg,
            "mean_int": scn.mean_int,
            "mean_ext": scn.mean_ext,
            "n_nodes": scn.graph.n_nodes,
            "n_messages": scn.messages.len(),
        });
        self.scenario = Some(Arc::new(scn));
        self.session = None;
        self.view = None;
        Ok(summary.to_string())
    }

    pub fn centrality_ranking(
        &self,
        metric: &str,
        top: usize,
        vc_csv: Option<&str>,
    ) -> Result<Vec<u32>, String> {
        let scn = self.scn()?;
        let csr = &scn.graph.csr;
        let scores: Vec<f64> = match metric {
            "degree" => centrality::out_degree(csr)
                .iter()
                .map(|&d| d as f64)
                .collect(),
            "pagerank" => centrality::pagerank_reverse(csr),
            "vc" => {
                let text = vc_csv.ok_or("VC の CSV が必要です")?;
                centrality::metric_from_csv(text, &scn.graph.labels, "vc")?
            }
            m => return Err(format!("未知の指標です: {m}")),
        };
        Ok(centrality::ranking(&scores, top))
    }

    pub fn set_ip(&mut self, node: u32) -> Result<String, String> {
        let scn = Arc::clone(self.scn()?);
        let session = Session::new(Arc::clone(&scn), node).map_err(|e| e.to_string())?;
        let rings = overview::compute_rings(&scn.graph.csr, node);
        let layout = overview::radial_layout(&scn.graph.csr, &rings, node);
        let out = serde_json::json!({ "ring_sizes": rings.ring_sizes, "unreachable": rings.unreachable, "ip": node, "ip_label": scn.graph.labels[node as usize] });
        self.session = Some(session);
        self.view = Some(IpView { rings, layout });
        Ok(out.to_string())
    }

    pub fn reset(&mut self) -> Result<(), String> {
        let ip = self.sess()?.ip();
        let scn = Arc::clone(self.scn()?);
        self.session = Some(Session::new(scn, ip).map_err(|e| e.to_string())?);
        Ok(())
    }

    fn view(&self) -> Result<&IpView, String> {
        self.view.as_ref().ok_or("IP が未選択です".to_string())
    }

    pub fn ring_angles(&self) -> Result<Vec<f32>, String> {
        Ok(self.view()?.layout.angle.clone())
    }

    pub fn ring_dist(&self) -> Result<Vec<u32>, String> {
        Ok(self.view()?.rings.dist.clone())
    }

    /// `choice`: 0..L-1 = メッセージ。
    pub fn step(&mut self, choice: i32) -> Result<String, String> {
        let c = Choice::from_i32(choice).ok_or_else(|| format!("不正な選択です: {choice}"))?;
        let s = self.session.as_mut().ok_or("IP が未選択です")?;
        let out = s.step(c).map_err(|e| e.to_string())?;
        json(&out.summary)
    }

    pub fn last_events(&self) -> Result<&EventColumns, String> {
        self.sess()?
            .last_output()
            .map(|o| &o.events)
            .ok_or_else(|| "まだ発信していません".to_string())
    }

    pub fn ever_flags(&self) -> Result<Vec<u8>, String> {
        Ok(self.sess()?.ever_flags())
    }

    /// 種類どうしの似かた（L×L、行優先）。推薦 off・基準では長さ 0（`recommender-v2-design.md` 7 章）。
    pub fn rec_sim(&self) -> Result<Vec<f64>, String> {
        Ok(self.sess()?.rec_sim())
    }

    /// Rモデル: 直近のラウンドの推薦の推移と、次のラウンドの事前の見込み a・基準 ē0（JSON）。
    /// 方式 wave 以外では `trace` が空（`recommender-v3-design.md` 7 章）。
    pub fn rec_wave_trace(&self) -> Result<String, String> {
        let s = self.sess()?;
        Ok(serde_json::json!({
            "trace": s.rec_wave_trace(),
            "prior": s.rec_prior(),
            "ebar0": s.scenario().ebar0,
        })
        .to_string())
    }

    pub fn kinds(&self, int_mode: u8) -> Result<Vec<u8>, String> {
        Ok(self
            .sess()?
            .kinds(IntMode::from_u8(int_mode))
            .into_iter()
            .map(|k| k as u8)
            .collect())
    }

    pub fn node_detail(&self, node: u32) -> Result<String, String> {
        let s = self.sess()?;
        if node as usize >= s.scenario().graph.n_nodes {
            return Err(format!("ノード番号 {node} は存在しません"));
        }
        // 推薦の表（`rec`、推薦 off では空）を足す
        let mut v = serde_json::to_value(s.node_detail(node)).map_err(|e| e.to_string())?;
        v["rec"] = serde_json::to_value(s.rec_detail(node)).map_err(|e| e.to_string())?;
        Ok(v.to_string())
    }

    pub fn group_series(
        &self,
        sel: u8,
        at_round: u32,
        int_mode: u8,
        custom: &[u32],
    ) -> Result<String, String> {
        let g = GroupSel::from_u8(sel, custom.to_vec())
            .ok_or_else(|| format!("不正な群の種類です: {sel}"))?;
        json(
            &self
                .sess()?
                .group_series(&g, at_round, IntMode::from_u8(int_mode)),
        )
    }

    /// `[ring][sector]` ごとに n, n_recv, n_int, n_ext, n_both の 5 値。
    pub fn sector_grid(&self, int_mode: u8) -> Result<Vec<u32>, String> {
        let v = self.view()?;
        let kinds = self.sess()?.kinds(IntMode::from_u8(int_mode));
        let grid = overview::aggregate(&v.rings, &v.layout, &kinds);
        let _ = N_SECTORS;
        Ok(grid
            .cells
            .iter()
            .flat_map(|c| [c.n, c.n_recv, c.n_int, c.n_ext, c.n_both])
            .collect())
    }

    pub fn select_nodes(
        &self,
        ring_lo: u32,
        ring_hi: u32,
        ang_lo: f32,
        ang_hi: f32,
    ) -> Result<Vec<u32>, String> {
        let v = self.view()?;
        overview::select_nodes(&v.rings, &v.layout, ring_lo, ring_hi, ang_lo, ang_hi)
            .map_err(|e| e.to_string())
    }

    pub fn select_unreachable(&self) -> Result<Vec<u32>, String> {
        overview::select_unreachable(&self.view()?.rings).map_err(|e| e.to_string())
    }

    /// 両端が `nodes` に入る辺を `[from, to, …]` で返す（`cap` 本まで）。
    pub fn edges_among(&self, nodes: &[u32], cap: usize) -> Result<Vec<u32>, String> {
        let csr = &self.scn()?.graph.csr;
        let mut inside = vec![false; csr.n_nodes()];
        for &n in nodes {
            *inside
                .get_mut(n as usize)
                .ok_or_else(|| format!("ノード番号 {n} は存在しません"))? = true;
        }
        let mut out = Vec::new();
        'o: for &a in nodes {
            for &b in csr.succ(a) {
                if inside[b as usize] {
                    if out.len() / 2 >= cap {
                        break 'o;
                    }
                    out.push(a);
                    out.push(b);
                }
            }
        }
        Ok(out)
    }

    /// 画面の MC 併記（F12）。`seq`: メッセージ番号の列。
    pub fn run_mc(&self, seq: &[i8], trials: u32, seed: f64) -> Result<String, String> {
        let seed = check_seed(seed, "seed")?;
        if trials == 0 {
            return Err("trials は 1 以上である必要があります".into());
        }
        let ip = self.sess()?.ip();
        let choices: Vec<Choice> = seq
            .iter()
            .map(|&v| Choice::from_i32(v as i32).ok_or_else(|| format!("不正な選択です: {v}")))
            .collect::<Result<_, _>>()?;
        let r = run_mc(
            self.scn()?,
            ip,
            &choices,
            &McSpec {
                trials,
                mode: McMode::Independent { seed },
            },
        )
        .map_err(|e| e.to_string())?;
        json(&r)
    }

    /// 分岐ツリー（JSON。`lane` と現在の枝を含む）。
    pub fn branch_tree(&self) -> Result<String, String> {
        json(&self.sess()?.tree_view())
    }

    /// ラウンド `round` の終了時点へ戻る（0 = 初期状態）。
    pub fn rewind_to(&mut self, round: u32) -> Result<(), String> {
        self.session
            .as_mut()
            .ok_or("IP が未選択です")?
            .rewind_to(round)
            .map_err(|e| e.to_string())
    }

    /// 木の節 `id` の枝へ移る。戻り値: `{round, changed_from}`（`changed_from` ラウンド以前は移る前と同じ）。
    pub fn switch_to(&mut self, id: u32) -> Result<String, String> {
        let s = self.session.as_mut().ok_or("IP が未選択です")?;
        let before = s.active_path().to_vec();
        s.switch_to(id).map_err(|e| e.to_string())?;
        let changed_from = before
            .iter()
            .zip(s.active_path())
            .take_while(|(a, b)| a == b)
            .count();
        Ok(serde_json::json!({ "round": s.round(), "changed_from": changed_from }).to_string())
    }

    /// 過去のラウンド `r`（1 始まり）の要約・イベント列・開始前/終了時の everFlags。
    pub fn round_data(&self, r: u32) -> Result<RoundData, String> {
        let s = self.sess()?;
        let summary = s
            .summary_at(r)
            .ok_or_else(|| format!("ラウンド {r} は存在しません"))?;
        Ok(RoundData {
            summary_json: json(summary)?,
            events: s.event_columns(r).ok_or("イベントがありません")?.clone(),
            ever_before: s.ever_flags_at(r - 1).ok_or("ログがありません")?,
            ever_after: s.ever_flags_at(r).ok_or("ログがありません")?,
        })
    }

    /// シナリオ記録 JSON（書き出したもの）を読み込む。グラフが同じ（ハッシュが一致）ことを確かめ、設定・IP・分岐ツリー全体を復元する。
    pub fn import_record(&mut self, record_json: &str) -> Result<String, String> {
        let rec = export::ScenarioRecord::from_json(record_json).map_err(|e| e.to_string())?;
        let hash = self
            .graph_info
            .map(|g| g.2)
            .ok_or("グラフが未読み込みです")?;
        if rec.graph.hash != hash {
            return Err(
                "記録のグラフと、読み込み済みのグラフが違います（ハッシュが一致しません）".into(),
            );
        }
        let scenario =
            self.create_scenario(&serde_json::to_string(&rec.config).map_err(|e| e.to_string())?)?;
        let ip = self.set_ip(rec.ip.id)?;
        let s = self.session.as_mut().ok_or("IP が未選択です")?;
        rec.restore_tree(s).map_err(|e| e.to_string())?;
        // 画面が必要とする、シナリオの要約と IP の情報
        let sc: serde_json::Value = serde_json::from_str(&scenario).map_err(|e| e.to_string())?;
        let ipv: serde_json::Value = serde_json::from_str(&ip).map_err(|e| e.to_string())?;
        Ok(serde_json::json!({ "scenario": sc, "ip": ipv }).to_string())
    }

    pub fn export_scenario_json(&self) -> Result<String, String> {
        let (_, meta) = self.source.as_ref().ok_or("グラフが未読み込みです")?;
        export::scenario_json(self.sess()?, meta).map_err(|e| e.to_string())
    }

    pub fn export_rounds_csv(&self) -> Result<String, String> {
        Ok(export::round_rows_csv(self.sess()?))
    }

    pub fn export_events_csv(&self, round: Option<u32>) -> Result<String, String> {
        Ok(export::events_csv(self.sess()?, round))
    }
}
