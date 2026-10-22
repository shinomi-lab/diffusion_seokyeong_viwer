//! MC の別スレッド実行（10.1）。`rayon` で試行を並列に回し（`McMode::Independent` と同じ乱数の使い方）、
//! 結果を `mpsc` で返して `ctx.request_repaint()` を呼ぶ。UI は固まらない。

use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};

use eframe::egui;
use rayon::prelude::*;
use sim_api::engine::Scratch;
use sim_api::log::Choice;
use sim_api::mc::{
    McResult, TrialSeries, independent_rng, run_trial, summarize, validate_sequence,
};
use sim_api::session::Scenario;

pub fn spawn_mc(
    ctx: egui::Context,
    scn: Arc<Scenario>,
    ip: u32,
    seq: Vec<Choice>,
    trials: u32,
    seed: u64,
) -> Result<Receiver<McResult>, String> {
    validate_sequence(&scn, ip, &seq).map_err(|e| e.to_string())?;
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let n = scn.graph.n_nodes;
        let all: Vec<TrialSeries> = (0..trials)
            .into_par_iter()
            .map_init(
                || Scratch::new(n),
                |scratch, k| {
                    let mut rng = independent_rng(seed, k);
                    let mut t = TrialSeries::default();
                    run_trial(&scn, ip, &seq, &mut rng, scratch, &mut t);
                    t
                },
            )
            .collect();
        let _ = tx.send(summarize(&all, seq.len()));
        ctx.request_repaint();
    });
    Ok(rx)
}
