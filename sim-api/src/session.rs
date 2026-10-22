//! 対話の 1 セッション（7.7）。

use std::sync::Arc;

use rand::SeedableRng;
use sim_core::message::Message;

use crate::config::{ConfigError, Mech, ScenarioConfig};
use crate::engine::{NoTrace, Observer, RoundCounts, Scratch, run_round};
use crate::graph_io::LoadedGraph;
use crate::log::*;
use crate::node::{EVER_EXT, EVER_INT, EVER_RECV};
use crate::population::{Population, sample_population};
use crate::rng::SimRng;
use crate::tree::{BranchTree, BranchTreeView};

/// 変更されない設定の束。`Arc` で共有する。
pub struct Scenario {
    pub graph: LoadedGraph,
    pub cfg: ScenarioConfig,
    pub mech: Mech,
    pub messages: Vec<Message>,
    pub mean_int: f64,
    pub mean_ext: f64,
    pub initial: Population,
}

impl Scenario {
    pub fn new(graph: LoadedGraph, cfg: ScenarioConfig) -> Result<Self, ConfigError> {
        cfg.validate(Some(graph.n_nodes))?;
        let (mean_int, mean_ext) = cfg.mean_stimuli()?;
        let initial = sample_population(&cfg, graph.n_nodes, mean_int, mean_ext)?;
        let mech = cfg.mech();
        let messages = cfg.messages();
        Ok(Self {
            graph,
            cfg,
            mech,
            messages,
            mean_int,
            mean_ext,
            initial,
        })
    }
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum SessionError {
    #[error("実行ラウンド数の上限（max_rounds）に達しました")]
    RoundLimit,
    #[error("メッセージ番号 {0} は存在しません")]
    InvalidMessage(u8),
    #[error("IP のノード番号 {0} は存在しません")]
    InvalidIp(u32),
    #[error("ラウンド {0} には戻れません（現在より先、または存在しません）")]
    InvalidRound(u32),
    #[error("ツリーの節 {0} は存在しません")]
    InvalidNode(u32),
    #[error("分岐ツリーを読み込めません: {0}")]
    InvalidTree(String),
}

pub struct RoundOutput {
    pub summary: RoundSummary,
    pub events: EventColumns,
}

struct RoundRecord {
    choice: Choice,
    out: RoundOutput,
}

/// 巻き戻し用の保存点。ラウンド 0 と `CHECKPOINT_EVERY` ラウンドごとに、母集団・乱数・数え上げを複製して持つ。
struct Checkpoint {
    round: u32,
    pop: Population,
    rng: SimRng,
    cum_xact: u32,
}

const CHECKPOINT_EVERY: u32 = 8;

pub struct Session {
    scenario: Arc<Scenario>,
    ip: u32,
    pop: Population,
    rng: SimRng,
    round: u32,
    cum_xact: u32,
    log: UtilityLog,
    history: Vec<RoundRecord>,
    scratch: Scratch,
    checkpoints: Vec<Checkpoint>,
    tree: BranchTree,
    /// 現在の枝の節（ラウンド 1 から順。長さ = round）。
    path: Vec<u32>,
}

impl Session {
    /// 乱数は `seed_from_u64(diffusion_seed)`。バッチの乱数とは別物（15.1 D16）。
    pub fn new(scn: Arc<Scenario>, ip: u32) -> Result<Self, SessionError> {
        let rng = SimRng::seed_from_u64(scn.cfg.diffusion_seed);
        Self::with_rng(scn, ip, rng)
    }

    pub fn with_rng(scn: Arc<Scenario>, ip: u32, rng: SimRng) -> Result<Self, SessionError> {
        let n = scn.graph.n_nodes;
        if ip as usize >= n {
            return Err(SessionError::InvalidIp(ip));
        }
        let pop = scn.initial.clone();
        let checkpoints = vec![Checkpoint {
            round: 0,
            pop: pop.clone(),
            rng: rng.clone(),
            cum_xact: 0,
        }];
        let mut log = UtilityLog::new(n);
        log.push_row(
            pop.states.iter().map(|s| s.u_int),
            pop.states.iter().map(|s| s.u_ext),
            &vec![0u8; n],
        );
        Ok(Self {
            ip,
            pop,
            rng,
            round: 0,
            cum_xact: 0,
            log,
            history: Vec::new(),
            scratch: Scratch::new(n),
            checkpoints,
            tree: BranchTree::new(),
            path: Vec::new(),
            scenario: scn,
        })
    }

    pub fn scenario(&self) -> &Arc<Scenario> {
        &self.scenario
    }

    pub fn ip(&self) -> u32 {
        self.ip
    }

    pub fn round(&self) -> u32 {
        self.round
    }

    pub fn rng(&self) -> &SimRng {
        &self.rng
    }

    pub fn log(&self) -> &UtilityLog {
        &self.log
    }

    pub fn population(&self) -> &Population {
        &self.pop
    }

    pub fn summaries(&self) -> impl Iterator<Item = &RoundSummary> {
        self.history.iter().map(|r| &r.out.summary)
    }

    pub fn choices(&self) -> Vec<Choice> {
        self.history.iter().map(|r| r.choice).collect()
    }

    /// ラウンド `r`（1 始まり）のイベント列。
    pub fn event_columns(&self, r: u32) -> Option<&EventColumns> {
        self.history
            .get((r as usize).checked_sub(1)?)
            .map(|h| &h.out.events)
    }

    pub fn last_output(&self) -> Option<&RoundOutput> {
        self.history.last().map(|r| &r.out)
    }

    /// 系列文字列（左端がラウンド 1。m∅ は `-`）。
    pub fn sequence_string(&self) -> String {
        self.history.iter().map(|r| r.choice.to_char()).collect()
    }

    pub fn step(&mut self, choice: Choice) -> Result<&RoundOutput, SessionError> {
        let scn = Arc::clone(&self.scenario);
        if self.round >= scn.cfg.max_rounds {
            return Err(SessionError::RoundLimit);
        }
        let Choice::Msg(id) = choice;
        if id as usize >= scn.messages.len() {
            return Err(SessionError::InvalidMessage(id));
        }
        self.round += 1;
        let mut rec = TraceRecorder::default();
        let counts = self.apply_round(choice, self.round, &mut rec);

        // ログの行: 今ラウンドの事象（NOW）と累積（EVER）
        let n = self.pop.states.len();
        let mut flags: Vec<u8> = self
            .pop
            .states
            .iter()
            .map(|s| {
                (if s.flags & EVER_RECV != 0 {
                    F_EVER_RECV
                } else {
                    0
                }) | (if s.flags & EVER_INT != 0 {
                    F_EVER_INT
                } else {
                    0
                }) | (if s.flags & EVER_EXT != 0 {
                    F_EVER_EXT
                } else {
                    0
                })
            })
            .collect();
        let mut n_int_now = 0;
        let ev = &rec.cols;
        for k in 0..ev.len() {
            let f = &mut flags[ev.node[k] as usize];
            *f |= F_RECV_NOW;
            if ev.flags[k] & EV_INT != 0 {
                *f |= F_INT_NOW;
                n_int_now += 1;
            }
            if ev.flags[k] & EV_EXT != 0 {
                *f |= F_EXT_NOW;
            }
        }
        self.log.push_row(
            self.pop.states.iter().map(|s| s.u_int),
            self.pop.states.iter().map(|s| s.u_ext),
            &flags,
        );

        // 統計: 受信済み / IP 以外の全ノード
        let r = self.log.n_rows() - 1;
        let (ui, ue) = (self.log.row_u_int(r), self.log.row_u_ext(r));
        let (mut ri, mut re, mut ai, mut ae) =
            (vec![], vec![], Vec::with_capacity(n), Vec::with_capacity(n));
        for k in 0..n {
            if k as u32 == self.ip {
                continue;
            }
            ai.push(ui[k]);
            ae.push(ue[k]);
            if flags[k] & F_EVER_RECV != 0 {
                ri.push(ui[k]);
                re.push(ue[k]);
            }
        }
        let summary = RoundSummary {
            round: self.round,
            choice,
            n_received: counts.n_received,
            n_share: counts.n_share,
            new_xact: counts.n_xact,
            cum_xact: self.cum_xact,
            n_waves: counts.n_waves,
            n_int_active_now: n_int_now,
            recv: stats_of(ri, re),
            all: stats_of(ai, ae),
        };
        // 分岐ツリー: 同じ選択の子があれば再利用し、無ければ新しい枝を作る
        let current = self.path.last().copied().unwrap_or(0);
        let node = self.tree.add_or_get(current, choice, summary.clone());
        self.path.push(node);
        self.history.push(RoundRecord {
            choice,
            out: RoundOutput {
                summary,
                events: rec.cols,
            },
        });
        if self.round.is_multiple_of(CHECKPOINT_EVERY) {
            self.checkpoints.push(Checkpoint {
                round: self.round,
                pop: self.pop.clone(),
                rng: self.rng.clone(),
                cum_xact: self.cum_xact,
            });
        }
        Ok(&self.history.last().expect("直前に追加した").out)
    }

    /// 1 ラウンドぶんの状態更新（`step` と巻き戻しの再生で共通。乱数の引き方・順序は同じ）。
    fn apply_round<O: Observer>(&mut self, choice: Choice, round: u32, obs: &mut O) -> RoundCounts {
        let scn = Arc::clone(&self.scenario);
        let mech = scn.mech;
        let Choice::Msg(id) = choice;
        let counts = run_round(
            &scn.graph.csr,
            scn.cfg.receipt_probability,
            &mut self.pop,
            &mut self.scratch,
            self.ip,
            id,
            &scn.messages[id as usize],
            &mech,
            round,
            &mut self.rng,
            obs,
        );
        self.cum_xact += counts.n_xact;
        counts
    }

    /// ラウンド `round` の終了時点へ戻る（0 = 初期状態）。保存点を復元し、記録済みの選択を再生して進めるので、
    /// 乱数も復元されて**ビット一致で再現**される（T-15）。以降のラウンドの履歴・ログは捨てる（木の節は残る）。
    pub fn rewind_to(&mut self, round: u32) -> Result<(), SessionError> {
        if round > self.round {
            return Err(SessionError::InvalidRound(round));
        }
        if round == self.round {
            return Ok(());
        }
        let cp = self
            .checkpoints
            .iter()
            .rev()
            .find(|c| c.round <= round)
            .expect("ラウンド 0 の保存点は常にある");
        let cp_round = cp.round;
        self.pop = cp.pop.clone();
        self.rng = cp.rng.clone();
        self.cum_xact = cp.cum_xact;
        for r in cp_round + 1..=round {
            let choice = self.history[r as usize - 1].choice;
            self.apply_round(choice, r, &mut NoTrace);
        }
        self.history.truncate(round as usize);
        self.log.truncate_rows(round as usize + 1);
        self.checkpoints.retain(|c| c.round <= round);
        self.path.truncate(round as usize);
        self.round = round;
        Ok(())
    }

    /// 木の節 `id` の枝へ移る。共通の祖先まで巻き戻し、その枝の選択を再生する。
    pub fn switch_to(&mut self, id: u32) -> Result<(), SessionError> {
        let target = self
            .tree
            .path_ids(id)
            .ok_or(SessionError::InvalidNode(id))?;
        let common = self
            .path
            .iter()
            .zip(&target)
            .take_while(|(a, b)| a == b)
            .count();
        self.rewind_to(common as u32)?;
        for &node in &target[common..] {
            let choice = self
                .tree
                .get(node)
                .and_then(|n| n.choice)
                .ok_or(SessionError::InvalidNode(node))?;
            self.step(choice)?;
        }
        Ok(())
    }

    pub fn tree(&self) -> &BranchTree {
        &self.tree
    }

    /// 根を除いた、現在の枝の節（ラウンド 1 から順）。
    pub fn active_path(&self) -> &[u32] {
        &self.path
    }

    pub fn tree_view(&self) -> BranchTreeView {
        self.tree.view(&self.path)
    }

    /// 保存した木（記録 JSON の `branch_tree`）を、この（発信前の）セッションに再生して復元する。
    /// 節は親が先の id 順に作られる。最後に `active_path` の枝へ移る。
    pub fn load_tree(
        &mut self,
        nodes: &[(u32, u32, Choice)],
        active: &[u32],
    ) -> Result<(), SessionError> {
        if self.round != 0 || self.tree.nodes().len() != 1 {
            return Err(SessionError::InvalidTree(
                "発信前のセッションにだけ読み込めます".into(),
            ));
        }
        let mut sorted = nodes.to_vec();
        sorted.sort_by_key(|n| n.0);
        for (id, parent, choice) in sorted {
            if self.tree.get(parent).is_none() {
                return Err(SessionError::InvalidTree(format!(
                    "節 {id} の親 {parent} がありません"
                )));
            }
            self.switch_to(parent)?;
            self.step(choice)?;
            if self.path.last().copied() != Some(id) {
                return Err(SessionError::InvalidTree(format!(
                    "節の番号が一致しません（{id}）"
                )));
            }
        }
        match active.last() {
            Some(&last) => self.switch_to(last),
            None => self.rewind_to(0),
        }
    }

    /// ラウンド `r`（0 = 初期状態）終了時の EVER_* ビット。
    pub fn ever_flags_at(&self, r: u32) -> Option<Vec<u8>> {
        if r as usize >= self.log.n_rows() {
            return None;
        }
        Some(
            self.log
                .row_flags(r as usize)
                .iter()
                .map(|&f| f >> 3)
                .collect(),
        )
    }

    /// ラウンド `r`（1 始まり）の要約。
    pub fn summary_at(&self, r: u32) -> Option<&RoundSummary> {
        self.history
            .get((r as usize).checked_sub(1)?)
            .map(|h| &h.out.summary)
    }

    /// 現在のラウンド終了時の全ノードの分類。
    pub fn kinds(&self, int_mode: IntMode) -> Vec<NodeKind> {
        let r = self.log.n_rows() - 1;
        self.log
            .row_flags(r)
            .iter()
            .enumerate()
            .map(|(k, &f)| kind_of(f, int_mode, k as u32 == self.ip))
            .collect()
    }

    /// 現在のラウンド終了時の EVER_* ビット（EVER_RECV=1, EVER_INT=2, EVER_EXT=4）。
    pub fn ever_flags(&self) -> Vec<u8> {
        let r = self.log.n_rows() - 1;
        self.log.row_flags(r).iter().map(|&f| f >> 3).collect()
    }

    /// F19: ノード群の平均推移。
    pub fn group_series(
        &self,
        sel: &crate::groups::GroupSel,
        at_round: u32,
        int_mode: IntMode,
    ) -> crate::groups::GroupSeries {
        crate::groups::group_series(&self.log, self.ip, sel, at_round, int_mode)
    }

    pub fn node_detail(&self, node: u32) -> NodeDetail {
        let n_rows = self.log.n_rows();
        let k = node as usize;
        let labels = &self.scenario.graph.labels;
        let u_int: Vec<f32> = (0..n_rows).map(|r| self.log.row_u_int(r)[k]).collect();
        let u_ext: Vec<f32> = (0..n_rows).map(|r| self.log.row_u_ext(r)[k]).collect();
        let (mut received, mut int_rounds, mut ext_round) = (vec![], vec![], None);
        for (i, h) in self.history.iter().enumerate() {
            let round = i as u32 + 1;
            let ev = &h.out.events;
            if let Some(p) = ev.node.iter().position(|&x| x == node) {
                received.push(RecvInfo {
                    round,
                    wave: ev.wave[p] as u32,
                    from: ev.from[p],
                    from_label: labels[ev.from[p] as usize],
                    n_arr: ev.n_arr[p],
                });
                if ev.flags[p] & EV_INT != 0 {
                    int_rounds.push(round);
                }
                if ev.flags[p] & EV_EXT != 0 {
                    ext_round = Some(round);
                }
            }
        }
        NodeDetail {
            node,
            label: labels[k],
            u_int,
            u_ext,
            received,
            int_rounds,
            ext_round,
            is_ip: node == self.ip,
        }
    }
}
