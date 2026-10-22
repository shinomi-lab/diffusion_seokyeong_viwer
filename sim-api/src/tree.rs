//! 分岐ツリー（F09、7.7）。ラウンドごとの選択を節にし、巻き戻して別の選択をすると枝ができる。
//! 節は `{id, parent, round, choice, summary}` だけを持つ（ノード単位のログは現在の枝の分だけ。`Session` が持つ）。

use serde::Serialize;

use crate::log::{Choice, RoundSummary};

#[derive(Clone, Debug)]
pub struct BranchNode {
    pub id: u32,
    pub parent: Option<u32>,
    /// 根は 0。
    pub round: u32,
    pub choice: Option<Choice>,
    pub summary: Option<RoundSummary>,
}

#[derive(Clone, Debug)]
pub struct BranchTree {
    nodes: Vec<BranchNode>,
}

impl Default for BranchTree {
    fn default() -> Self {
        Self::new()
    }
}

impl BranchTree {
    pub fn new() -> Self {
        Self {
            nodes: vec![BranchNode {
                id: 0,
                parent: None,
                round: 0,
                choice: None,
                summary: None,
            }],
        }
    }

    pub fn nodes(&self) -> &[BranchNode] {
        &self.nodes
    }

    pub fn get(&self, id: u32) -> Option<&BranchNode> {
        self.nodes.get(id as usize)
    }

    pub fn child(&self, parent: u32, choice: Choice) -> Option<u32> {
        self.nodes
            .iter()
            .find(|n| n.parent == Some(parent) && n.choice == Some(choice))
            .map(|n| n.id)
    }

    /// 親の下に節を作る（同じ選択の子があればそれを返し、要約を更新する）。
    pub fn add_or_get(&mut self, parent: u32, choice: Choice, summary: RoundSummary) -> u32 {
        if let Some(id) = self.child(parent, choice) {
            self.nodes[id as usize].summary = Some(summary);
            return id;
        }
        let id = self.nodes.len() as u32;
        let round = self.nodes[parent as usize].round + 1;
        self.nodes.push(BranchNode {
            id,
            parent: Some(parent),
            round,
            choice: Some(choice),
            summary: Some(summary),
        });
        id
    }

    /// 根を除いた、根から `id` までの節の並び（ラウンド 1 から順）。
    pub fn path_ids(&self, id: u32) -> Option<Vec<u32>> {
        self.get(id)?;
        let mut v = Vec::new();
        let mut cur = id;
        while let Some(p) = self.nodes[cur as usize].parent {
            v.push(cur);
            cur = p;
        }
        v.reverse();
        Some(v)
    }

    /// 枝の並び（段）。最初の子は親と同じ段、2 番目以降は新しい段。両版で同じ配置にするため Rust 側で決める。
    pub fn lanes(&self) -> Vec<u32> {
        let n = self.nodes.len();
        let mut children: Vec<Vec<u32>> = vec![Vec::new(); n];
        for node in &self.nodes {
            if let Some(p) = node.parent {
                children[p as usize].push(node.id);
            }
        }
        let mut lane = vec![0u32; n];
        let mut next = 1u32;
        let mut stack = vec![0u32];
        while let Some(v) = stack.pop() {
            for (k, &c) in children[v as usize].iter().enumerate() {
                lane[c as usize] = if k == 0 {
                    lane[v as usize]
                } else {
                    next += 1;
                    next - 1
                };
            }
            // 子の処理順は id の昇順にそろえる
            for &c in children[v as usize].iter().rev() {
                stack.push(c);
            }
        }
        lane
    }
}

/// 画面・JSON 用の表現。
#[derive(Clone, Debug, Serialize)]
pub struct BranchNodeView {
    pub id: u32,
    pub parent: Option<u32>,
    pub round: u32,
    /// 系列文字列の 1 文字（m∅ は `-`。根は空）。
    pub choice: String,
    pub lane: u32,
    pub n_received: u32,
    pub cum_xact: u32,
}

#[derive(Clone, Debug, Serialize)]
pub struct BranchTreeView {
    pub nodes: Vec<BranchNodeView>,
    /// 根を除いた、現在の枝の節（ラウンド 1 から順）。
    pub active_path: Vec<u32>,
    /// 現在位置の節（巻き戻し直後はラウンド r の節。何も発信していなければ 0）。
    pub current: u32,
    pub n_lanes: u32,
}

impl BranchTree {
    pub fn view(&self, active_path: &[u32]) -> BranchTreeView {
        let lanes = self.lanes();
        let nodes: Vec<BranchNodeView> = self
            .nodes
            .iter()
            .map(|n| BranchNodeView {
                id: n.id,
                parent: n.parent,
                round: n.round,
                choice: n
                    .choice
                    .map(|c| c.to_char().to_string())
                    .unwrap_or_default(),
                lane: lanes[n.id as usize],
                n_received: n.summary.as_ref().map(|s| s.n_received).unwrap_or(0),
                cum_xact: n.summary.as_ref().map(|s| s.cum_xact).unwrap_or(0),
            })
            .collect();
        let n_lanes = lanes.iter().copied().max().unwrap_or(0) + 1;
        BranchTreeView {
            nodes,
            active_path: active_path.to_vec(),
            current: active_path.last().copied().unwrap_or(0),
            n_lanes,
        }
    }
}
