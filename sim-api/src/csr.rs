//! 辺の順序を保った高速な隣接表（7.3.2）。

use graph_lib::prelude::{Graph, GraphB};

/// 後続（拡散の向き）と先行の CSR。`succ` は `GraphB::successors` と同じ昇順。
#[derive(Clone, Debug)]
pub struct Csr {
    s_off: Vec<u32>,
    s_tgt: Vec<u32>,
    p_off: Vec<u32>,
    p_tgt: Vec<u32>,
}

impl Csr {
    pub fn from_graph(g: &GraphB) -> Self {
        let n = g.node_count();
        let build = |f: &dyn Fn(usize) -> Vec<u32>| {
            let mut off = Vec::with_capacity(n + 1);
            let mut tgt = Vec::new();
            off.push(0u32);
            for i in 0..n {
                tgt.extend(f(i));
                off.push(tgt.len() as u32);
            }
            (off, tgt)
        };
        let (s_off, s_tgt) = build(&|i| g.successors(i).map(|&j| j as u32).collect());
        let (p_off, p_tgt) = build(&|i| g.predecessors(i).map(|&j| j as u32).collect());
        Self {
            s_off,
            s_tgt,
            p_off,
            p_tgt,
        }
    }

    #[inline]
    pub fn n_nodes(&self) -> usize {
        self.s_off.len() - 1
    }

    #[inline]
    pub fn n_edges(&self) -> usize {
        self.s_tgt.len()
    }

    #[inline]
    pub fn succ(&self, i: u32) -> &[u32] {
        let i = i as usize;
        &self.s_tgt[self.s_off[i] as usize..self.s_off[i + 1] as usize]
    }

    #[inline]
    pub fn pred(&self, i: u32) -> &[u32] {
        let i = i as usize;
        &self.p_tgt[self.p_off[i] as usize..self.p_off[i + 1] as usize]
    }

    #[inline]
    pub fn out_degree(&self, i: u32) -> usize {
        self.succ(i).len()
    }
}
