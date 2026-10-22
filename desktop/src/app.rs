//! アプリ本体: 状態、グラフ・シナリオ・セッションの操作、描画データの組み立て。
//! 画面の各部は `panels/` にある（`impl App` を分けて書いている）。

use std::sync::Arc;
use std::sync::mpsc::Receiver;
use std::time::Instant;

use eframe::egui;
use sim_api::centrality;
use sim_api::config::ScenarioConfig;
use sim_api::graph_io::{GraphFormat, GraphSource, load_graph};
use sim_api::log::{Choice, IntMode, NodeKind};
use sim_api::mc::McResult;
use sim_api::overview::{self, RadialLayout, Rings};
use sim_api::session::{Scenario, Session};

use crate::camera::{Camera, bounds_of};
use crate::cli::Cli;
use crate::gpu;
use crate::layout::{EXPAND_CAP, Positions, UNREACHABLE, compute_positions};
use crate::playback::{Playback, state_at_wave};
use crate::state::{Mode, RoundData, View};
use crate::tokens::Tokens;

/// 辺の描画の上限（9.4）。
pub const EDGE_CAP: usize = 50_000;

#[derive(Clone, Debug)]
pub struct GraphOpts {
    pub format: GraphFormat,
    pub gzip: bool,
    pub directed: bool,
    /// 既定は true（入力が「A→B＝A が B をフォロー」の形式のため。9.7）。
    pub transposed: bool,
    pub name: String,
}

impl Default for GraphOpts {
    fn default() -> Self {
        Self {
            format: GraphFormat::EdgeList,
            gzip: false,
            directed: true,
            transposed: true,
            name: String::new(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Metric {
    Degree,
    PageRank,
    Vc,
}

/// 現在のウェーブ時点の表示状態。
pub struct Display {
    pub kinds: Vec<u8>,
    pub pulse: Vec<u8>,
    pub key: (usize, i32, bool, u32),
}

/// GPU へ渡す描画データ（版が変わったときだけ GPU に再アップロードされる）。
#[derive(Default)]
pub struct SceneCache {
    pub pos: Arc<Vec<f32>>,
    pub pos_ver: u64,
    pub attrs: Arc<Vec<u32>>,
    pub attr_ver: u64,
    pub edges: Arc<Vec<f32>>,
    pub edge_ver: u64,
    /// GPU に描くノード番号（None = 全ノード）。
    pub nodes: Option<Arc<Vec<u32>>>,
    pub edges_truncated: bool,
    /// 辺を作ったときの表示範囲（ワールド。大きなグラフの視野に応じた間引き用）。
    pub edge_view: Option<[f32; 4]>,
    /// 直近に見た表示範囲と、その範囲になった時刻（カメラが止まったかの判定）。
    pub view_seen: Option<([f32; 4], Instant)>,
    /// 描くノード数に応じた半径（px）。
    pub radius_px: f32,
}

pub struct BenchState {
    pub n: usize,
    pub start: Instant,
    pub frames: u32,
    pub frame_ms: Vec<f32>,
    pub last: Instant,
    pub done: bool,
}

pub struct DemoState {
    pub step_ms: Vec<f32>,
    pub expanded: bool,
    pub mc_started: Option<(Instant, u32)>,
    pub exported: bool,
    pub compare_started: bool,
    pub branch_stage: u8,
    pub branch_orig: Vec<String>,
    pub branch_tail: u32,
    pub record_started: bool,
    pub mp4_done: bool,
    pub step: usize,
    pub wait_frames: u32,
    pub screenshot_requested: bool,
}

/// 比較（F13）の A・B それぞれが持つ、シナリオ・セッション・描画データ。グラフの読み込みと IP・再生・表示範囲は両方で共有する。
pub struct Slot {
    pub form: ScenarioConfig,
    pub applied_json: String,
    pub scenario: Option<Arc<Scenario>>,
    pub session: Option<Session>,
    pub rings: Option<Rings>,
    pub layout: Option<RadialLayout>,
    pub positions: Option<Positions>,
    pub rounds: Vec<RoundData>,
    pub choices: Vec<Choice>,
    pub ever: Vec<u8>,
    pub last_step_ms: f32,
    pub display: Option<Display>,
    pub scene: SceneCache,
    pub sectors: Vec<overview::RingSectorCell>,
    pub sectors_key: Option<(usize, i32, bool, u32)>,
    pub detail_cache: Option<(u32, usize, sim_api::log::NodeDetail)>,
    pub mc_rx: Option<Receiver<McResult>>,
    pub mc_running: bool,
    pub mc_result: Option<McResult>,
    /// このスロットで起きたエラー（B の予算不足など）。
    pub error: Option<String>,
}

impl Slot {
    pub fn new(form: ScenarioConfig) -> Self {
        Self {
            form,
            applied_json: String::new(),
            scenario: None,
            session: None,
            rings: None,
            layout: None,
            positions: None,
            rounds: Vec::new(),
            choices: Vec::new(),
            ever: Vec::new(),
            last_step_ms: 0.0,
            display: None,
            scene: SceneCache::default(),
            sectors: Vec::new(),
            sectors_key: None,
            detail_cache: None,
            mc_rx: None,
            mc_running: false,
            mc_result: None,
            error: None,
        }
    }
}

pub struct App {
    pub tokens: Tokens,
    /// 比較（F13）の A = 0、B = 1。比較がオフのときは長さ 1。
    pub slots: Vec<Slot>,
    pub compare: bool,
    /// いま描画・入力の対象にしているスロット（`ui_network` が A・B を順に切り替える）。
    pub cur: usize,
    /// 凡例で非表示にされた系列名（時系列グラフ。A・B・MC で共通）。
    pub hidden_series: std::collections::HashSet<String>,
    /// ネットワーク図の領域全体（比較中は A・B を含む）。画像・連番 PNG の切り出しに使う。
    pub net_rect: egui::Rect,
    /// スクリーンショットを待っている用途。
    pub shot: Option<crate::panels::media::ShotTask>,
    pub record: Option<crate::panels::media::RecordState>,
    /// 最後に書き出した連番 PNG（フォルダ, 枚数）。
    pub last_record: Option<(std::path::PathBuf, usize)>,
    pub mp4_rx: Option<Receiver<Result<std::path::PathBuf, String>>>,
    pub ffmpeg: Option<bool>,
    pub cli: Cli,

    // グラフ
    pub graph_opts: GraphOpts,
    pub graph_bytes: Option<Vec<u8>>,
    pub graph_label: String,

    // シナリオ・セッション
    pub confirming_apply: bool,
    pub metric: Metric,
    pub ranking: Vec<u32>,
    pub vc_scores: Option<Vec<f64>>,
    pub manual_ip: String,
    pub ip: Option<u32>,

    // 表示
    pub view: View,
    pub playback: Playback,
    pub palette: i32,
    pub view_size: [f32; 2],
    pub canvas_rect: egui::Rect,
    pub rubber: Option<(egui::Pos2, egui::Pos2)>,
    pub srgb_target: bool,

    // 詳細・MC・エラー
    pub mc_trials: u32,
    pub error: Option<String>,
    pub status: Option<String>,

    pub bench: Option<BenchState>,
    pub demo: Option<DemoState>,
    pub frame_ms: f32,
    pub last_frame: Instant,
}

impl App {
    /// 大きなグラフ: カメラが止まったら、表示範囲内の辺だけを上限まで作り直す（拡大するほど細部の辺が出る）。
    pub fn refresh_view_edges(
        &mut self,
        ctx: &egui::Context,
        i: usize,
        cam: &crate::camera::Camera,
        size: [f32; 2],
    ) {
        let slot = &self.slots[i];
        let n_draw = slot.scene.pos.len() / 2;
        if n_draw < crate::lod::EDGE_LOD_MIN_NODES {
            return;
        }
        let (a, b) = (cam.to_world(size, [0.0, 0.0]), cam.to_world(size, size));
        let view = [
            a[0].min(b[0]),
            a[1].min(b[1]),
            a[0].max(b[0]),
            a[1].max(b[1]),
        ];
        let span = (view[2] - view[0]).max(view[3] - view[1]).max(1e-6);
        let near = |o: &[f32; 4]| (0..4).all(|k| (o[k] - view[k]).abs() < span * 0.01);
        if slot.scene.edge_view.as_ref().is_some_and(near) {
            return;
        }
        let now = Instant::now();
        match slot.scene.view_seen {
            Some((v, t)) if near(&v) => {
                if now.duration_since(t).as_millis() < 250 {
                    ctx.request_repaint_after(std::time::Duration::from_millis(100));
                    return;
                }
            }
            _ => {
                self.slots[i].scene.view_seen = Some((view, now));
                ctx.request_repaint_after(std::time::Duration::from_millis(280));
                return;
            }
        }
        let slot = &mut self.slots[i];
        let (Some(pos), Some(scn)) = (&slot.positions, &slot.scenario) else {
            return;
        };
        let margin = span * 0.05;
        let v = [
            view[0] - margin,
            view[1] - margin,
            view[2] + margin,
            view[3] + margin,
        ];
        let (edges, truncated) = crate::lod::edges_in_view(
            &pos.xy,
            slot.scene.nodes.as_deref().map(|n| n.as_slice()),
            &scn.graph.csr,
            v,
            EDGE_CAP,
        );
        slot.scene.edges = Arc::new(edges);
        slot.scene.edge_ver += 1;
        slot.scene.edges_truncated = truncated;
        slot.scene.edge_view = Some(view);
    }

    pub fn new(cc: &eframe::CreationContext<'_>, cli: Cli) -> Self {
        install_japanese_font(&cc.egui_ctx);
        let tokens = Tokens::load();
        crate::theme::apply(&cc.egui_ctx, &tokens);
        if let Some(rs) = cc.wgpu_render_state.as_ref() {
            // 比較（F13）の A・B で別々のバッファを使えるよう、描画器を 2 つ用意する
            let renderers = gpu::Renderers(vec![
                gpu::Renderer::new(&rs.device, rs.target_format),
                gpu::Renderer::new(&rs.device, rs.target_format),
            ]);
            rs.renderer.write().callback_resources.insert(renderers);
        }
        let demo = cli.demo.then_some(DemoState {
            step_ms: Vec::new(),
            expanded: false,
            mc_started: None,
            exported: false,
            compare_started: false,
            branch_stage: 0,
            branch_orig: Vec::new(),
            branch_tail: 0,
            record_started: false,
            mp4_done: false,
            step: 0,
            wait_frames: 0,
            screenshot_requested: false,
        });
        let bench = cli.bench.map(|n| BenchState {
            n,
            start: Instant::now(),
            frames: 0,
            frame_ms: Vec::new(),
            last: Instant::now(),
            done: false,
        });
        let mut app = Self {
            tokens,
            slots: vec![Slot::new(ScenarioConfig::default())],
            compare: false,
            cur: 0,
            hidden_series: cli
                .hide
                .iter()
                .flat_map(|h| h.split(','))
                .map(|n| n.trim().to_string())
                .filter(|n| !n.is_empty())
                .collect(),
            net_rect: egui::Rect::ZERO,
            shot: None,
            record: None,
            last_record: None,
            mp4_rx: None,
            ffmpeg: None,
            cli: cli.clone(),
            graph_opts: GraphOpts::default(),
            graph_bytes: None,
            graph_label: String::new(),
            confirming_apply: false,
            metric: Metric::Degree,
            ranking: Vec::new(),
            vc_scores: None,
            manual_ip: String::new(),
            ip: None,
            view: View::default(),
            playback: Playback::default(),
            palette: 0,
            view_size: [800.0, 500.0],
            canvas_rect: egui::Rect::ZERO,
            rubber: None,
            srgb_target: cc
                .wgpu_render_state
                .as_ref()
                .is_some_and(|r| r.target_format.is_srgb()),
            mc_trials: 200,
            error: None,
            status: None,
            bench,
            demo,
            frame_ms: 0.0,
            last_frame: Instant::now(),
        };
        if let Some(path) = cli.graph.clone() {
            app.load_graph_path(&path);
            if let Some(ip) = cli.ip {
                app.choose_ip(ip);
            } else if cli.demo || cli.screenshot.is_some() {
                if let Some(&top) = app.ranking.first() {
                    app.choose_ip(top);
                }
            }
        }
        app
    }

    // ---- グラフ・シナリオ ----

    pub fn load_graph_path(&mut self, path: &str) {
        match std::fs::read(path) {
            Ok(bytes) => {
                let name = std::path::Path::new(path)
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();
                let lower = name.to_lowercase();
                self.graph_opts.gzip = lower.ends_with(".gz");
                let base = lower.trim_end_matches(".gz");
                self.graph_opts.format = if base.ends_with(".csv") {
                    GraphFormat::Csv
                } else if base.ends_with(".tsv") {
                    GraphFormat::Tsv
                } else {
                    GraphFormat::EdgeList
                };
                self.graph_opts.name = name;
                self.graph_bytes = Some(bytes);
                self.reload_graph();
            }
            Err(e) => self.error = Some(format!("ファイルを読み込めません: {path}: {e}")),
        }
    }

    /// 保持しているバイト列を現在のグラフ設定で読み直し、全スロットのシナリオを作り直す。
    pub fn reload_graph(&mut self) {
        self.clear_session();
        for slot in &mut self.slots {
            slot.scenario = None;
            slot.error = None;
        }
        self.ranking.clear();
        let Some(bytes) = self.graph_bytes.clone() else {
            return;
        };
        let t = Instant::now();
        for i in 0..self.slots.len() {
            match self.build_scenario(bytes.clone(), i) {
                Ok(scn) => {
                    if i == 0 {
                        self.graph_label = format!(
                            "{}：{} ノード / {} 辺",
                            self.graph_opts.name, scn.graph.n_nodes, scn.graph.n_edges
                        );
                        self.status = Some(format!(
                            "グラフとシナリオを作成しました（{:.0} ms）",
                            t.elapsed().as_secs_f32() * 1000.0
                        ));
                    }
                    self.slots[i].applied_json =
                        serde_json::to_string(&self.slots[i].form).unwrap_or_default();
                    self.slots[i].scenario = Some(Arc::new(scn));
                }
                Err(e) => {
                    if i == 0 {
                        self.error = Some(e);
                    } else {
                        self.slots[i].error = Some(e);
                    }
                }
            }
        }
        self.refresh_ranking();
    }

    fn build_scenario(&self, bytes: Vec<u8>, slot: usize) -> Result<Scenario, String> {
        let o = &self.graph_opts;
        let src = GraphSource {
            bytes,
            format: o.format,
            gzip: o.gzip,
            directed: o.directed,
            transposed: o.transposed,
        };
        let g = load_graph(&src).map_err(|e| e.to_string())?;
        Scenario::new(g, self.slots[slot].form.clone()).map_err(|e| e.to_string())
    }

    /// 編集中の設定でシナリオを作り直す（履歴は消える。INV-14）。比較中は A・B の両方。
    pub fn apply_form(&mut self) {
        let ip = self.ip;
        self.reload_graph();
        self.confirming_apply = false;
        if let (Some(ip), true) = (ip, self.slots[0].scenario.is_some()) {
            self.choose_ip(ip);
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.is_dirty_slot(0)
    }

    pub fn is_dirty_slot(&self, i: usize) -> bool {
        self.slots.get(i).is_some_and(|s| {
            s.scenario.is_some()
                && serde_json::to_string(&s.form).unwrap_or_default() != s.applied_json
        })
    }

    pub fn refresh_ranking(&mut self) {
        let Some(scn) = &self.slots[0].scenario else {
            return;
        };
        let csr = &scn.graph.csr;
        let scores: Vec<f64> = match self.metric {
            Metric::Degree => centrality::out_degree(csr)
                .iter()
                .map(|&d| d as f64)
                .collect(),
            Metric::PageRank => centrality::pagerank_reverse(csr),
            Metric::Vc => match &self.vc_scores {
                Some(v) => v.clone(),
                None => centrality::out_degree(csr)
                    .iter()
                    .map(|&d| d as f64)
                    .collect(),
            },
        };
        self.ranking = centrality::ranking(&scores, 50);
    }

    fn clear_session(&mut self) {
        self.ip = None;
        for s in &mut self.slots {
            s.session = None;
            s.rings = None;
            s.layout = None;
            s.positions = None;
            s.rounds.clear();
            s.choices.clear();
            s.ever.clear();
            s.display = None;
            s.detail_cache = None;
            s.mc_result = None;
            s.sectors_key = None;
            s.scene = SceneCache::default();
        }
        self.playback.reset();
        self.view = View::default();
    }

    /// IP を選ぶ（比較中は A・B の両方）。
    pub fn choose_ip(&mut self, node: u32) {
        if self.slots[0].scenario.is_none() {
            return;
        }
        self.clear_session();
        for i in 0..self.slots.len() {
            let Some(scn) = self.slots[i].scenario.clone() else {
                continue;
            };
            match Session::new(Arc::clone(&scn), node) {
                Ok(s) => {
                    let rings = overview::compute_rings(&scn.graph.csr, node);
                    let layout = overview::radial_layout(&scn.graph.csr, &rings, node);
                    let slot = &mut self.slots[i];
                    slot.positions = Some(compute_positions(&layout.angle, &rings.dist));
                    slot.ever = s.ever_flags();
                    slot.session = Some(s);
                    slot.rings = Some(rings);
                    slot.layout = Some(layout);
                    if i == 0 {
                        // 1 万ノード以下は個別表示、超えたら全体図から始める（9.4）
                        self.view.mode = if scn.graph.n_nodes > EXPAND_CAP {
                            Mode::Overview
                        } else {
                            Mode::Expanded
                        };
                    }
                }
                Err(e) => {
                    if i == 0 {
                        self.error = Some(e.to_string());
                    } else {
                        self.slots[i].error = Some(e.to_string());
                    }
                }
            }
        }
        self.ip = Some(node);
        self.view.refit = true;
        for i in 0..self.slots.len() {
            self.rebuild_scene(i);
        }
    }

    pub fn restart(&mut self) {
        if let Some(ip) = self.ip {
            self.choose_ip(ip);
        }
    }

    pub fn n_messages(&self) -> usize {
        self.slots[0]
            .scenario
            .as_ref()
            .map(|s| s.messages.len())
            .unwrap_or(self.slots[0].form.message_space.len())
    }

    pub fn max_rounds(&self) -> u32 {
        self.slots[0]
            .scenario
            .as_ref()
            .map(|s| s.cfg.max_rounds)
            .unwrap_or(self.slots[0].form.max_rounds)
    }

    /// スロット `i` が、いま選ばれているメッセージを発信できるか。
    fn slot_can_send(&self, i: usize) -> bool {
        let slot = &self.slots[i];
        let Some(scn) = &slot.scenario else {
            return false;
        };
        slot.session.is_some() && (slot.rounds.len() as u32) < scn.cfg.max_rounds
    }

    pub fn can_send(&self) -> bool {
        !self.playback.playing && (0..self.slots.len()).all(|i| self.slot_can_send(i))
    }

    pub fn current_choice(&self) -> Choice {
        Choice::Msg(self.palette.max(0) as u8)
    }

    /// 選ばれているメッセージで 1 ラウンド発信し、再生を始める。比較中は A・B が必ず同じ選択を受け取り、
    /// どちらかが発信できないときは、先に進んだ側も取り消して揃える。
    pub fn send(&mut self) {
        if self.playback.playing {
            return;
        }
        if !self.can_send() {
            // 比較中、B だけが発信できない理由を示す
            for i in 1..self.slots.len() {
                if !self.slot_can_send(i) {
                    self.slots[i].error =
                        Some("B では発信できません（実行ラウンド数の上限に達した）".into());
                }
            }
            return;
        }
        let choice = self.current_choice();
        let before: Vec<usize> = self.slots.iter().map(|s| s.rounds.len()).collect();
        let mut n_waves = 0;
        for i in 0..self.slots.len() {
            match self.step_slot(i, choice) {
                Ok(w) => n_waves = n_waves.max(w),
                Err(e) => {
                    for (j, &b) in before.iter().enumerate().take(i) {
                        self.rewind_slot(j, b as u32);
                    }
                    self.slots[i].error = Some(e);
                    return;
                }
            }
        }
        self.playback.start(n_waves);
    }

    fn step_slot(&mut self, i: usize, choice: Choice) -> Result<u32, String> {
        let t = Instant::now();
        let slot = &mut self.slots[i];
        let before = slot.ever.clone();
        let Some(session) = slot.session.as_mut() else {
            return Err("セッションがありません".into());
        };
        let out = session.step(choice).map_err(|e| e.to_string())?;
        let summary = out.summary.clone();
        let events = out.events.clone();
        slot.last_step_ms = t.elapsed().as_secs_f32() * 1000.0;
        slot.ever = session.ever_flags();
        let n_waves = summary.n_waves;
        slot.rounds.push(RoundData {
            summary,
            events,
            ever_before: before,
        });
        slot.choices.push(choice);
        slot.display = None;
        slot.mc_result = None;
        slot.error = None;
        Ok(n_waves)
    }

    // ---- 巻き戻しと分岐（F09） ----

    fn rewind_slot(&mut self, i: usize, round: u32) {
        let slot = &mut self.slots[i];
        let Some(session) = slot.session.as_mut() else {
            return;
        };
        if round as usize > slot.rounds.len() || session.rewind_to(round).is_err() {
            return;
        }
        slot.rounds.truncate(round as usize);
        slot.choices.truncate(round as usize);
        slot.ever = session.ever_flags();
        slot.display = None;
        slot.mc_result = None;
        slot.detail_cache = None;
    }

    /// 再生位置を、全スロットの最後のラウンドの終端に合わせる。
    fn sync_playback_to_end(&mut self) {
        let last = self
            .slots
            .iter()
            .map(|s| {
                s.rounds
                    .last()
                    .map(|r| r.summary.n_waves as i32 - 1)
                    .unwrap_or(-1)
            })
            .max()
            .unwrap_or(-1);
        self.playback.last = last;
        self.playback.k = last;
        self.playback.playing = false;
    }

    /// ラウンド `round` の終了時点へ戻る（0 = 初期状態）。比較中は A・B の両方。
    pub fn rewind_all(&mut self, round: u32) {
        for i in 0..self.slots.len() {
            self.rewind_slot(i, round);
        }
        self.sync_playback_to_end();
    }

    /// 木の節 `id` の枝へ移る。比較中は A・B の両方（同じ操作をするので木は同形）。
    pub fn switch_all(&mut self, id: u32) {
        for i in 0..self.slots.len() {
            let slot = &mut self.slots[i];
            let Some(session) = slot.session.as_mut() else {
                continue;
            };
            let before = session.active_path().to_vec();
            if let Err(e) = session.switch_to(id) {
                slot.error = Some(e.to_string());
                continue;
            }
            let common = before
                .iter()
                .zip(session.active_path())
                .take_while(|(a, b)| a == b)
                .count();
            // 共通のラウンドまでは そのまま。その先は新しい枝のデータを取り直す
            slot.rounds.truncate(common);
            let mut ever_before = if common == 0 {
                session.ever_flags_at(0).unwrap_or_default()
            } else {
                session.ever_flags_at(common as u32).unwrap_or_default()
            };
            for r in common as u32 + 1..=session.round() {
                let events = session.event_columns(r).cloned().unwrap_or_default();
                let summary = session.summary_at(r).cloned().expect("存在するラウンド");
                let after = session.ever_flags_at(r).unwrap_or_default();
                slot.rounds.push(RoundData {
                    summary,
                    events,
                    ever_before: std::mem::replace(&mut ever_before, after),
                });
            }
            slot.choices = session.choices();
            slot.ever = session.ever_flags();
            slot.display = None;
            slot.mc_result = None;
            slot.detail_cache = None;
        }
        self.sync_playback_to_end();
    }

    // ---- 比較（F13） ----

    /// 比較モードのオン・オフ。オンにすると、B に A と同じグラフを読み込み、A の設定を複製して、同じ IP・同じ選択を再現する。
    pub fn set_compare(&mut self, on: bool) {
        if on == self.compare {
            return;
        }
        self.compare = on;
        if !on {
            self.slots.truncate(1);
            self.view.refit = true;
            return;
        }
        let form_b = self.slots[0].form.clone();
        self.slots.push(Slot::new(form_b));
        self.apply_b();
    }

    /// B の設定でシナリオを作り直し、A と同じ IP・同じ現在の枝の選択を再現する（A はそのまま）。
    pub fn apply_b(&mut self) {
        if self.slots.len() < 2 {
            return;
        }
        let Some(bytes) = self.graph_bytes.clone() else {
            return;
        };
        self.slots[1].error = None;
        match self.build_scenario(bytes, 1) {
            Ok(scn) => {
                self.slots[1].applied_json =
                    serde_json::to_string(&self.slots[1].form).unwrap_or_default();
                self.slots[1].scenario = Some(Arc::new(scn));
            }
            Err(e) => {
                self.slots[1].error = Some(e);
                return;
            }
        }
        let Some(ip) = self.ip else { return };
        let choices = self.slots[0].choices.clone();
        let scn = self.slots[1].scenario.clone().expect("直前に作った");
        let session = Session::new(Arc::clone(&scn), ip);
        let rings = overview::compute_rings(&scn.graph.csr, ip);
        let layout = overview::radial_layout(&scn.graph.csr, &rings, ip);
        let b = &mut self.slots[1];
        b.positions = Some(compute_positions(&layout.angle, &rings.dist));
        b.rings = Some(rings);
        b.layout = Some(layout);
        b.rounds.clear();
        b.choices.clear();
        b.display = None;
        b.mc_result = None;
        b.scene = SceneCache::default();
        match session {
            Ok(s) => {
                b.ever = s.ever_flags();
                b.session = Some(s);
            }
            Err(e) => {
                b.error = Some(e.to_string());
                return;
            }
        }
        for (k, c) in choices.iter().enumerate() {
            if let Err(e) = self.step_slot(1, *c) {
                self.slots[1].error = Some(format!(
                    "B は A の {} ラウンド目の選択を再現できません（{e}）。B の実行ラウンド数の上限などを確認してください",
                    k + 1
                ));
                break;
            }
        }
        self.view.refit = true;
        self.rebuild_scene(0);
        self.rebuild_scene(1);
        self.sync_playback_to_end();
    }

    // ---- 表示状態 ----

    /// 現在のウェーブ時点の kinds・pulse を（変化があったときだけ）計算する。
    pub fn update_display(&mut self, i: usize) {
        let Some(ip) = self.ip else { return };
        let (int_mode, k) = (self.view.int_mode, self.playback.k);
        let slot = &mut self.slots[i];
        let Some(pos) = &slot.positions else { return };
        let n = pos.xy.len() / 2;
        let (before, events, last): (Vec<u8>, _, i32) = match slot.rounds.last() {
            Some(r) => (
                r.ever_before.clone(),
                Some(&r.events),
                r.summary.n_waves as i32 - 1,
            ),
            None => (vec![0u8; n], None, -1),
        };
        let k = k.min(last);
        let key = (slot.rounds.len(), k, int_mode == IntMode::ThisRoundOnly, ip);
        if slot.display.as_ref().is_some_and(|d| d.key == key) {
            return;
        }
        let empty = sim_api::log::EventColumns::default();
        let st = state_at_wave(&before, events.unwrap_or(&empty), k, int_mode, ip);
        slot.display = Some(Display {
            kinds: st.kinds,
            pulse: st.pulse,
            key,
        });
        self.rebuild_attrs(i);
    }

    /// GPU に描くノード集合。展開中でないときは、IP から到達できるノードだけを描く。到達不能のノードは格子状に並べると
    /// 大きな塊になって画面を占めるので、通常は出さない（件数を示し、クリックで展開できる）。
    fn draw_nodes(&self, i: usize) -> Option<Arc<Vec<u32>>> {
        if self.view.mode != Mode::Expanded {
            return None;
        }
        if let Some(v) = &self.view.expanded {
            return Some(Arc::new(v.clone()));
        }
        let rings = self.slots[i].rings.as_ref()?;
        (rings.unreachable > 0).then(|| {
            Arc::new(
                rings
                    .dist
                    .iter()
                    .enumerate()
                    .filter(|&(_, &d)| d != UNREACHABLE)
                    .map(|(i, _)| i as u32)
                    .collect(),
            )
        })
    }

    /// ノード集合・モードが変わったとき: 位置と辺を作り直す。
    pub fn rebuild_scene(&mut self, i: usize) {
        let nodes = self.draw_nodes(i);
        let mode = self.view.mode;
        let slot = &mut self.slots[i];
        let (Some(pos), Some(scn)) = (&slot.positions, &slot.scenario) else {
            return;
        };
        slot.scene.pos_ver += 1;
        slot.scene.edge_ver += 1;
        slot.scene.attr_ver += 1;
        if mode == Mode::Overview {
            slot.scene.nodes = None;
            slot.scene.pos = Arc::new(Vec::new());
            slot.scene.edges = Arc::new(Vec::new());
            slot.scene.edges_truncated = false;
        } else {
            let (xy, edges, truncated) = build_geometry(
                &pos.xy,
                nodes.as_deref().map(|v| v.as_slice()),
                &scn.graph.csr,
            );
            slot.scene.pos = Arc::new(xy);
            slot.scene.edges = Arc::new(edges);
            slot.scene.edges_truncated = truncated;
            slot.scene.nodes = nodes;
            slot.scene.edge_view = None;
            slot.scene.view_seen = None;
            let n_draw = slot.scene.pos.len() / 2;
            slot.scene.radius_px = crate::lod::effective_radius(self.tokens.radius_px, n_draw);
        }
        slot.display = None;
        self.view.refit = true;
    }

    fn rebuild_attrs(&mut self, i: usize) {
        let slot = &mut self.slots[i];
        let Some(d) = &slot.display else { return };
        let attrs: Vec<u32> = match &slot.scene.nodes {
            Some(nodes) => nodes
                .iter()
                .map(|&i| d.kinds[i as usize] as u32 | (d.pulse[i as usize] as u32) << 8)
                .collect(),
            None => d
                .kinds
                .iter()
                .zip(&d.pulse)
                .map(|(&k, &p)| k as u32 | (p as u32) << 8)
                .collect(),
        };
        slot.scene.attrs = Arc::new(attrs);
        slot.scene.attr_ver += 1;
    }

    pub fn node_kinds(&self, i: usize) -> Vec<NodeKind> {
        self.slots[i]
            .display
            .as_ref()
            .map(|d| d.kinds.iter().map(|&k| kind_from_u8(k)).collect())
            .unwrap_or_default()
    }

    /// 現在の表示範囲を、ノード集合に合わせる（ビューの大きさが分かってから。A の配置を基準にする）。
    pub fn fit_camera(&mut self, size: [f32; 2]) {
        let Some(pos) = &self.slots[0].positions else {
            return;
        };
        self.view.camera = if self.view.mode == Mode::Overview {
            let r = pos.max_dist as f32 + 0.8;
            let mut b = crate::camera::Bounds {
                min_x: -r,
                min_y: -r,
                max_x: r,
                max_y: r,
            };
            if self.slots[0]
                .rings
                .as_ref()
                .is_some_and(|r| r.unreachable > 0)
            {
                let (x, _, w, _) = crate::layout::unreachable_rect(pos.max_dist);
                b.max_x = x + w + 0.2;
            }
            // 左上のリング表（約 270pt）に重ならないよう、全体図を右へ寄せる
            let usable = [(size[0] - 270.0).max(200.0), size[1]];
            let mut cam = Camera::fit(b, usable, 40.0);
            cam.cx -= 135.0 / cam.zoom;
            cam
        } else {
            Camera::fit(
                bounds_of(
                    &pos.xy,
                    self.slots[0].scene.nodes.as_deref().map(|v| v.as_slice()),
                ),
                size,
                40.0,
            )
        };
        self.view.refit = false;
    }

    pub fn expand_nodes(&mut self, nodes: Vec<u32>, label: String) {
        self.view.group = Some(nodes.clone());
        self.view.expanded = Some(nodes);
        self.view.expanded_label = label;
        self.view.from_overview = true;
        self.view.mode = Mode::Expanded;
        self.view.notice = None;
        for i in 0..self.slots.len() {
            self.rebuild_scene(i);
        }
    }

    pub fn back_to_overview(&mut self) {
        self.view.mode = Mode::Overview;
        self.view.expanded = None;
        self.view.from_overview = false;
        self.view.selected = None;
        for i in 0..self.slots.len() {
            self.rebuild_scene(i);
        }
    }

    pub fn expand_ring(&mut self, ring: u32, sector: Option<u32>) {
        let (Some(rings), Some(layout)) = (&self.slots[0].rings, &self.slots[0].layout) else {
            return;
        };
        let step = std::f32::consts::TAU / overview::N_SECTORS as f32;
        let (lo, hi) = match sector {
            Some(s) => (s as f32 * step, (s + 1) as f32 * step),
            None => (0.0, 7.0),
        };
        match overview::select_nodes(rings, layout, ring, ring, lo, hi) {
            Ok(nodes) => {
                let label = match sector {
                    Some(s) => format!("{ring}ホップ・扇形 {}", s + 1),
                    None => format!("{ring}ホップ"),
                };
                self.expand_nodes(nodes, label);
            }
            Err(e) => self.view.notice = Some(e.to_string()),
        }
    }

    pub fn expand_unreachable(&mut self) {
        let Some(rings) = &self.slots[0].rings else {
            return;
        };
        match overview::select_unreachable(rings) {
            Ok(nodes) => self.expand_nodes(nodes, "到達不能".to_string()),
            Err(e) => self.view.notice = Some(e.to_string()),
        }
    }
}

/// 描画するノードの位置と、両端が集合に入る辺（上限 EDGE_CAP）。
pub fn build_geometry(
    xy: &[f32],
    nodes: Option<&[u32]>,
    csr: &sim_api::csr::Csr,
) -> (Vec<f32>, Vec<f32>, bool) {
    let n = xy.len() / 2;
    let mut inside = vec![nodes.is_none(); n];
    let pos: Vec<f32> = match nodes {
        Some(ns) => {
            let mut v = Vec::with_capacity(ns.len() * 2);
            for &i in ns {
                inside[i as usize] = true;
                v.push(xy[2 * i as usize]);
                v.push(xy[2 * i as usize + 1]);
            }
            v
        }
        None => xy.to_vec(),
    };
    // 全体の辺（視野は無限）。大きなグラフでは、あとからカメラが止まるたびに視野内の辺へ作り直す（`refresh_view_edges`）
    let (edges, truncated) = crate::lod::edges_in_view(
        xy,
        nodes,
        csr,
        [
            f32::NEG_INFINITY,
            f32::NEG_INFINITY,
            f32::INFINITY,
            f32::INFINITY,
        ],
        EDGE_CAP,
    );
    let _ = &inside;
    (pos, edges, truncated)
}

pub fn kind_from_u8(k: u8) -> NodeKind {
    match k {
        0 => NodeKind::Unreceived,
        1 => NodeKind::ReceivedOnly,
        2 => NodeKind::Internal,
        3 => NodeKind::External,
        4 => NodeKind::Both,
        _ => NodeKind::Ip,
    }
}

/// 日本語のフォントを、システムにあるものから探して読み込む（egui の標準フォントには日本語が無い）。
fn install_japanese_font(ctx: &egui::Context) {
    const CANDIDATES: &[&str] = &[
        "/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc",
        "/System/Library/Fonts/Hiragino Sans GB.ttc",
        "/Library/Fonts/Arial Unicode.ttf",
        "C:\\Windows\\Fonts\\YuGothM.ttc",
        "C:\\Windows\\Fonts\\meiryo.ttc",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
    ];
    for path in CANDIDATES {
        if let Ok(bytes) = std::fs::read(path) {
            let mut fonts = egui::FontDefinitions::default();
            fonts
                .font_data
                .insert("jp".to_owned(), Arc::new(egui::FontData::from_owned(bytes)));
            for fam in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                fonts.families.entry(fam).or_default().push("jp".to_owned());
            }
            ctx.set_fonts(fonts);
            return;
        }
    }
    eprintln!("日本語フォントが見つかりません。表示が文字化けします");
}

#[allow(dead_code)]
pub const UNREACHABLE_DIST: u32 = UNREACHABLE;
