//! 系列・分岐ツリー（F09）と MC 平均の併記（F12）。

use eframe::egui::{self, Align2, Color32, FontId, Pos2, RichText, Sense, Stroke, vec2};
use sim_api::groups::Band;
use sim_api::tree::BranchTreeView;

use crate::app::App;
use crate::panels::col;
use crate::state::sequence_string;
use crate::worker::spawn_mc;

pub const DX: f32 = 30.0;
pub const DY: f32 = 28.0;
pub const PAD: f32 = 18.0;

/// 節をクリックしたときの動作（Web の `clickAction` と同じ）。
#[derive(Debug, PartialEq, Eq)]
pub enum ClickAction {
    None,
    Rewind(u32),
    Switch(u32),
}

pub fn click_action(t: &BranchTreeView, id: u32) -> ClickAction {
    let Some(node) = t.nodes.iter().find(|n| n.id == id) else {
        return ClickAction::None;
    };
    if id == t.current {
        ClickAction::None
    } else if id == 0 || t.active_path.contains(&id) {
        ClickAction::Rewind(node.round)
    } else {
        ClickAction::Switch(id)
    }
}

/// 節の位置（ラウンドが横、段が縦）。
pub fn node_pos(round: u32, lane: u32) -> [f32; 2] {
    [PAD + round as f32 * DX, PAD + lane as f32 * DY]
}

/// 実現値が、帯（p10〜p90、両端を含む）に入っているラウンド数と、比べたラウンド数。
pub fn coverage(realized: &[f64], band: &Band) -> (usize, usize) {
    let total = realized.len().min(band.p10.len());
    let inside = (0..total)
        .filter(|&i| realized[i] >= band.p10[i] as f64 && realized[i] <= band.p90[i] as f64)
        .count();
    (inside, total)
}

impl App {
    pub fn ui_tree(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.strong("選んだ系列");
        let choices = self.slots[0].choices.clone();
        if choices.is_empty() {
            ui.label("まだ発信していません。");
        } else {
            ui.label(
                RichText::new(sequence_string(&choices))
                    .monospace()
                    .size(18.0),
            );
        }
        let tree = self.slots[0].session.as_ref().map(|s| s.tree_view());
        let mut action = ClickAction::None;
        if let Some(t) = &tree {
            if t.nodes.len() > 1 {
                let max_round = t.nodes.iter().map(|n| n.round).max().unwrap_or(0);
                let size = vec2(
                    PAD * 2.0 + max_round as f32 * DX,
                    PAD * 2.0 + (t.n_lanes.saturating_sub(1)) as f32 * DY,
                );
                egui::ScrollArea::both()
                    .max_height(110.0)
                    .id_salt("tree_scroll")
                    .show(ui, |ui| {
                        let (rect, _) =
                            ui.allocate_exact_size(size.max(vec2(40.0, 40.0)), Sense::hover());
                        let at = |r: u32, l: u32| {
                            let p = node_pos(r, l);
                            Pos2::new(rect.min.x + p[0], rect.min.y + p[1])
                        };
                        let painter = ui.painter_at(rect);
                        let accent = col(self.tokens.series_internal);
                        let line = ui.visuals().widgets.noninteractive.bg_stroke.color;
                        let by_id: std::collections::HashMap<u32, _> =
                            t.nodes.iter().map(|n| (n.id, n)).collect();
                        let active = |id: u32| id == 0 || t.active_path.contains(&id);
                        for n in &t.nodes {
                            if let Some(p) = n.parent.and_then(|p| by_id.get(&p)) {
                                let (a, b) = (at(p.round, p.lane), at(n.round, n.lane));
                                let on = active(n.id) && active(p.id);
                                painter.add(egui::Shape::line(
                                    vec![a, Pos2::new(a.x, b.y), b],
                                    Stroke::new(
                                        if on { 2.0 } else { 1.5 },
                                        if on { accent } else { line },
                                    ),
                                ));
                            }
                        }
                        for n in &t.nodes {
                            let c = at(n.round, n.lane);
                            let on = active(n.id);
                            let resp = ui.interact(
                                egui::Rect::from_center_size(c, vec2(18.0, 18.0)),
                                ui.id().with(("node", n.id)),
                                Sense::click(),
                            );
                            let ring = if n.id == t.current {
                                col(self.tokens.selected)
                            } else if on {
                                accent
                            } else {
                                Color32::GRAY
                            };
                            painter.circle(
                                c,
                                7.5,
                                if on {
                                    accent
                                } else {
                                    ui.visuals().extreme_bg_color
                                },
                                Stroke::new(if n.id == t.current { 3.0 } else { 1.5 }, ring),
                            );
                            painter.text(
                                c,
                                Align2::CENTER_CENTER,
                                if n.id == 0 {
                                    "0".to_string()
                                } else {
                                    n.choice.clone()
                                },
                                FontId::proportional(9.0),
                                if on {
                                    Color32::WHITE
                                } else {
                                    ui.visuals().text_color()
                                },
                            );
                            let tip = if n.id == 0 {
                                "初期状態（ラウンド 0）".to_string()
                            } else {
                                format!(
                                    "ラウンド {}: m{}（累積外部 {}{}）",
                                    n.round,
                                    if n.choice == "-" {
                                        "∅".to_string()
                                    } else {
                                        n.choice.clone()
                                    },
                                    n.cum_xact,
                                    if n.cum_rec > 0 {
                                        format!(
                                            "、推薦で受信 {}・累積 {}",
                                            n.rec_received, n.cum_rec
                                        )
                                    } else {
                                        String::new()
                                    }
                                )
                            };
                            if resp.on_hover_text(tip).clicked() {
                                action = click_action(t, n.id);
                            }
                        }
                    });
            }
        }
        match action {
            ClickAction::Rewind(r) if !self.playback.playing => self.rewind_all(r),
            ClickAction::Switch(id) if !self.playback.playing => self.switch_all(id),
            _ => {}
        }
        ui.add(egui::Label::new(RichText::new("節をクリックすると、そのラウンドへ巻き戻します（別の枝の節ならその枝へ切り替え）。巻き戻して別のメッセージを発信すると枝ができます。").weak()).wrap());
        ui.separator();
        ui.strong("MC 平均の併記（F12）");
        ui.horizontal(|ui| {
            ui.label("試行数");
            ui.add(
                egui::DragValue::new(&mut self.mc_trials)
                    .range(10..=100_000)
                    .speed(10),
            );
            let can = self.slots[0].session.is_some()
                && !self.slots[0].choices.is_empty()
                && !self.slots[0].mc_running;
            if ui
                .add_enabled(can, egui::Button::new("この系列を MC で回す"))
                .clicked()
            {
                if let (Some(scn), Some(ip)) = (self.slots[0].scenario.clone(), self.ip) {
                    match spawn_mc(
                        ctx.clone(),
                        scn,
                        ip,
                        self.slots[0].choices.clone(),
                        self.mc_trials,
                        1,
                    ) {
                        Ok(rx) => {
                            self.slots[0].mc_rx = Some(rx);
                            self.slots[0].mc_running = true;
                        }
                        Err(e) => self.error = Some(e),
                    }
                }
            }
            if self.slots[0].mc_running {
                ui.spinner();
                ui.label("計算中…");
            }
        });
        if let Some(mc) = &self.slots[0].mc_result {
            let cum: Vec<f64> = self.slots[0]
                .rounds
                .iter()
                .map(|r| r.summary.cum_xact as f64)
                .collect();
            let share: Vec<f64> = self.slots[0]
                .rounds
                .iter()
                .map(|r| r.summary.n_share as f64)
                .collect();
            let (c_in, c_all) = coverage(&cum, &mc.cum_xact);
            let (s_in, s_all) = coverage(&share, &mc.new_share);
            ui.add(
                egui::Label::new(format!(
                    "{} 試行: 合計外部行動者数の平均 {:.2}、合計シェア回数の平均 {:.2}。1 回の実現が 10〜90% の帯に入っているラウンド — 累積外部行動 {c_in}/{c_all}、新規シェア {s_in}/{s_all}。",
                    mc.trials, mc.total_xact_mean, mc.total_share_mean
                ))
                .wrap(),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim_api::tree::BranchNodeView;

    fn node(id: u32, parent: Option<u32>, round: u32, lane: u32) -> BranchNodeView {
        BranchNodeView {
            id,
            parent,
            round,
            choice: id.to_string(),
            lane,
            n_received: 0,
            cum_xact: 0,
            rec_received: 0,
            cum_rec: 0,
        }
    }

    // 根 0 → 1 → 2 → 3。2 から分岐して 4（別の段）
    fn tree() -> BranchTreeView {
        BranchTreeView {
            nodes: vec![
                node(0, None, 0, 0),
                node(1, Some(0), 1, 0),
                node(2, Some(1), 2, 0),
                node(3, Some(2), 3, 0),
                node(4, Some(2), 3, 1),
            ],
            active_path: vec![1, 2, 3],
            current: 3,
            n_lanes: 2,
        }
    }

    #[test]
    fn click_actions_match_the_web_version() {
        let t = tree();
        assert_eq!(click_action(&t, 3), ClickAction::None);
        assert_eq!(click_action(&t, 1), ClickAction::Rewind(1));
        assert_eq!(click_action(&t, 0), ClickAction::Rewind(0));
        assert_eq!(click_action(&t, 4), ClickAction::Switch(4));
        assert_eq!(click_action(&t, 99), ClickAction::None);
    }

    #[test]
    fn node_positions_are_round_by_lane() {
        assert_eq!(node_pos(3, 0), [PAD + 3.0 * DX, PAD]);
        assert_eq!(node_pos(3, 1), [PAD + 3.0 * DX, PAD + DY]);
    }

    #[test]
    fn coverage_counts_rounds_inside_the_band() {
        let band = Band {
            mean: vec![0.0; 4],
            p10: vec![0.0, 1.0, 2.0, 3.0],
            p50: vec![0.0; 4],
            p90: vec![2.0, 3.0, 4.0, 5.0],
        };
        assert_eq!(coverage(&[1.0, 3.0, 5.0, 4.0], &band), (3, 4));
        assert_eq!(coverage(&[9.0; 4], &band), (0, 4));
        assert_eq!(coverage(&[1.0, 3.0], &band), (2, 2));
    }
}
