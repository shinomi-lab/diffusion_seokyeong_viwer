//! ノード詳細（F10・F19）。「ノード」「ノード群」のタブ。効用は 1 を超えても上限なしで表示する。

use eframe::egui::{self, Color32, RichText};
use egui_plot::{Line, Plot, PlotPoints};
use sim_api::groups::GroupSel;

use crate::app::App;
use crate::panels::charts::{band_lines, points};
use crate::panels::col;
use crate::panels::legend::{LegendEntry, fixed_plot, is_visible, legend_row};
use crate::state::{GroupKind, Tab};

impl App {
    pub fn ui_detail(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.view.tab, Tab::Node, "ノード");
            ui.selectable_value(&mut self.view.tab, Tab::Group, "ノード群");
        });
        ui.separator();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| match self.view.tab {
                Tab::Node => self.ui_single_node(ui),
                Tab::Group => self.ui_group(ui),
            });
    }

    fn ui_single_node(&mut self, ui: &mut egui::Ui) {
        let (Some(node), Some(session)) = (self.view.selected, &self.slots[0].session) else {
            ui.add(egui::Label::new("ネットワーク上のノードをクリックすると、効用の推移・受信したラウンドと受信元・活性になったラウンドを表示します。").wrap());
            return;
        };
        let stale = !matches!(&self.slots[0].detail_cache, Some((n, r, _)) if *n == node && *r == self.slots[0].rounds.len());
        if stale {
            self.slots[0].detail_cache =
                Some((node, self.slots[0].rounds.len(), session.node_detail(node)));
        }
        let Some((_, _, d)) = &self.slots[0].detail_cache else {
            return;
        };
        ui.horizontal(|ui| {
            ui.heading(format!("ノード {}（ラベル {}）", d.node, d.label));
            if d.is_ip {
                ui.label(
                    RichText::new("IP")
                        .strong()
                        .color(Color32::WHITE)
                        .background_color(col(self.tokens.series_internal)),
                );
            }
        });
        let (c_u_int, c_u_ext) = (
            col(self.tokens.series_internal),
            col(self.tokens.series_external),
        );
        let hidden = self.hidden_series.clone();
        fixed_plot(Plot::new("node_util"))
            .height(150.0)
            .x_axis_label("ラウンド")
            .show(ui, |p| {
                if is_visible(&hidden, "内部効用") {
                    p.line(
                        Line::new("内部効用", points(&d.u_int, 0.0))
                            .color(c_u_int)
                            .width(1.8),
                    );
                }
                if is_visible(&hidden, "外部効用") {
                    p.line(
                        Line::new("外部効用", points(&d.u_ext, 0.0))
                            .color(c_u_ext)
                            .width(1.8),
                    );
                }
            });
        legend_row(
            ui,
            &[
                LegendEntry::new("内部効用", c_u_int, false),
                LegendEntry::new("外部効用", c_u_ext, false),
            ],
            &mut self.hidden_series,
        );
        let max = |v: &[f32]| v.iter().copied().fold(f32::MIN, f32::max);
        ui.add(egui::Label::new(RichText::new(format!("縦軸は値に合わせて自動で広がります（1 を超えても切りません）。最大値: 内部 {:.3} / 外部 {:.3}", max(&d.u_int), max(&d.u_ext))).weak()).wrap());
        egui::Grid::new("node_kv").num_columns(2).show(ui, |ui| {
            ui.label("内部が成立したラウンド");
            ui.label(if d.int_rounds.is_empty() {
                "—".to_string()
            } else {
                d.int_rounds
                    .iter()
                    .map(|r| r.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            });
            ui.end_row();
            ui.label("外部行動のラウンド");
            ui.label(d.ext_round.map(|r| r.to_string()).unwrap_or("—".into()));
            ui.end_row();
        });
        egui::Grid::new("node_recv").striped(true).show(ui, |ui| {
            for h in ["受信ラウンド", "ウェーブ", "受信元（ラベル）", "経路数"]
            {
                ui.strong(h);
            }
            ui.end_row();
            if d.received.is_empty() {
                ui.label("まだ受信していません");
                ui.end_row();
            }
            for r in &d.received {
                ui.label(r.round.to_string());
                ui.label((r.wave + 1).to_string());
                ui.label(r.from_label.to_string());
                ui.label(r.n_arr.to_string());
                ui.end_row();
            }
        });
        if !d.is_ip
            && ui
                .button("このノードを IP にする（履歴は消えます）")
                .clicked()
        {
            let n = d.node;
            self.choose_ip(n);
        }
    }

    fn ui_group(&mut self, ui: &mut egui::Ui) {
        egui::ComboBox::from_label("群の種類")
            .selected_text(
                GroupKind::ALL
                    .iter()
                    .find(|(k, _)| *k == self.view.group_kind)
                    .map(|(_, n)| *n)
                    .unwrap_or(""),
            )
            .show_ui(ui, |ui| {
                for (k, name) in GroupKind::ALL {
                    ui.selectable_value(&mut self.view.group_kind, k, name);
                }
            });
        let Some(session) = &self.slots[0].session else {
            ui.label("IP を選ぶと表示されます。");
            return;
        };
        let sel = match self.view.group_kind {
            GroupKind::IntOnly => GroupSel::IntOnly,
            GroupKind::ExtOnly => GroupSel::ExtOnly,
            GroupKind::Both => GroupSel::Both,
            GroupKind::ReceivedOnly => GroupSel::ReceivedOnly,
            GroupKind::Custom => {
                let Some(g) = &self.view.group else {
                    ui.add(egui::Label::new("Shift を押しながらネットワーク上をドラッグすると範囲選択で群を作れます。全体図ではリングの「展開」から作れます。").wrap());
                    return;
                };
                GroupSel::Custom(g.clone())
            }
        };
        let gs = session.group_series(&sel, self.slots[0].rounds.len() as u32, self.view.int_mode);
        ui.add(egui::Label::new(RichText::new(format!("表示中のラウンド（{}）の時点で群を決め、メンバー {} ノードを固定して過去へさかのぼります。", gs.at_round, gs.members)).weak()).wrap());
        let (c_int, c_ext, c_both) = (
            col(self.tokens.series_internal),
            col(self.tokens.series_external),
            col(self.tokens.series_both),
        );
        let alpha = self.tokens.band_alpha;
        let hidden = self.hidden_series.clone();
        fixed_plot(Plot::new("group_util"))
            .height(170.0)
            .x_axis_label("ラウンド")
            .show(ui, |p| {
                band_lines(p, "内部 平均", &gs.int, 0.0, c_int, alpha, &hidden);
                band_lines(p, "外部 平均", &gs.ext, 0.0, c_ext, alpha, &hidden);
            });
        legend_row(
            ui,
            &[
                LegendEntry::new("内部 平均", c_int, false),
                LegendEntry::new("外部 平均", c_ext, false),
            ],
            &mut self.hidden_series,
        );
        ui.strong("群の大きさの推移");
        fixed_plot(Plot::new("group_size"))
            .height(100.0)
            .x_axis_label("ラウンド")
            .show(ui, |p| {
                p.line(
                    Line::new(
                        "群の大きさ",
                        PlotPoints::new(
                            gs.size_by_round
                                .iter()
                                .enumerate()
                                .map(|(i, &v)| [i as f64, v as f64])
                                .collect(),
                        ),
                    )
                    .color(c_both)
                    .width(1.6),
                );
            });
    }
}
