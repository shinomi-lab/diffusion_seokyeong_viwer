//! ラウンド別の時系列（F08）。新規シェア数・新規外部行動者数・累積外部行動者数、効用の平均と分位点、MC の帯（F12）。
//! 縦軸は値に合わせて自動で広がる（上限を設けない。INV-15）。

use eframe::egui::{self, Color32, Stroke};
use egui_plot::{Line, LineStyle, Plot, PlotPoints, Polygon};
use sim_api::groups::Band;
use std::collections::HashSet;

use crate::app::App;
use crate::panels::legend::{LegendEntry, fixed_plot, is_visible, legend_height, legend_row};
use crate::panels::{col, col_a};

/// x = 0, 1, … の系列を、`offset` から始まる x 座標で点列にする。
pub fn points(v: &[f32], x0: f64) -> PlotPoints<'static> {
    PlotPoints::new(
        v.iter()
            .enumerate()
            .map(|(i, &y)| [x0 + i as f64, y as f64])
            .collect(),
    )
}

/// 10〜90% の帯（上側を前向き、下側を後ろ向きにたどる多角形）。
pub fn band_polygon(
    name: &str,
    hi: &[f32],
    lo: &[f32],
    x0: f64,
    color: Color32,
) -> Polygon<'static> {
    let mut pts: Vec<[f64; 2]> = hi
        .iter()
        .enumerate()
        .map(|(i, &y)| [x0 + i as f64, y as f64])
        .collect();
    pts.extend(
        lo.iter()
            .enumerate()
            .rev()
            .map(|(i, &y)| [x0 + i as f64, y as f64]),
    );
    Polygon::new(name, PlotPoints::new(pts))
        .fill_color(color)
        .stroke(Stroke::NONE)
}

/// 平均の線と 10〜90% の帯。凡例で非表示にされた系列は描かない（帯も同じ系列に属する）。
pub fn band_lines(
    plot_ui: &mut egui_plot::PlotUi,
    name: &str,
    b: &Band,
    x0: f64,
    color: Color32,
    alpha: f32,
    hidden: &HashSet<String>,
) {
    if !is_visible(hidden, name) {
        return;
    }
    plot_ui.add(band_polygon(
        &format!("{name} 10〜90%"),
        &b.p90,
        &b.p10,
        x0,
        col_a_c(color, alpha),
    ));
    plot_ui.line(Line::new(name, points(&b.mean, x0)).color(color).width(1.6));
}

fn col_a_c(c: Color32, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (a * 255.0) as u8)
}

impl App {
    /// 「ラウンド別の人数」の凡例（A・B・MC の系列。帯用の補助系列は出さない）。
    fn counts_legend(&self) -> Vec<LegendEntry> {
        let t = &self.tokens;
        let (c_int, c_ext, c_new, c_both) = (
            col(t.series_internal),
            col(t.series_external),
            col(t.series_new_xact),
            col(t.series_both),
        );
        let mut v = vec![
            LegendEntry::new("新規シェア", c_int, false),
            LegendEntry::new("新規外部行動", c_new, false),
            LegendEntry::new("累積外部行動", c_ext, false),
        ];
        if self.rec_on(0) {
            // 推薦（recommender-design.md 10 章）。推薦 off では出さない
            v.extend([
                LegendEntry::new("推薦経由の外部行動", col(t.series_rec_xact), false),
                LegendEntry::new("推薦で受信してシェア", col(t.series_rec_share), true),
                LegendEntry::new("推薦で受信", col(t.series_rec_xact), true),
                LegendEntry::new("推薦で受信（累積人数）", col(t.series_rec_xact), true),
            ]);
        }
        if self.compare {
            v.extend([
                LegendEntry::new("B 新規シェア", c_int, true),
                LegendEntry::new("B 新規外部行動", c_new, true),
                LegendEntry::new("B 累積外部行動", c_ext, true),
            ]);
        }
        if self.slots[0].mc_result.is_some() {
            v.extend([
                LegendEntry::new("MC 累積外部行動（平均）", c_both, false),
                LegendEntry::new(
                    "MC 新規シェア（平均）",
                    col_a(t.series_internal, 0.6),
                    false,
                ),
            ]);
            if self.rec_on(0) {
                v.push(LegendEntry::new(
                    "MC 推薦で受信・累積（平均）",
                    col(t.series_rec_xact),
                    true,
                ));
            }
        }
        v
    }

    fn util_legend(&self) -> Vec<LegendEntry> {
        let t = &self.tokens;
        let (c_int, c_ext) = (col(t.series_internal), col(t.series_external));
        let mut v = vec![
            LegendEntry::new("内部", c_int, false),
            LegendEntry::new("外部", c_ext, false),
        ];
        if self.compare {
            v.extend([
                LegendEntry::new("B 内部 平均", c_int, true),
                LegendEntry::new("B 外部 平均", c_ext, true),
            ]);
        }
        if self.slots[0].mc_result.is_some() {
            v.extend([
                LegendEntry::new("MC 内部 平均", c_int, true),
                LegendEntry::new("MC 外部 平均", c_ext, true),
            ]);
        }
        v
    }

    pub fn ui_round_charts(&mut self, ui: &mut egui::Ui) {
        let t = &self.tokens;
        let (c_int, c_ext, c_new, c_both) = (
            col(t.series_internal),
            col(t.series_external),
            col(t.series_new_xact),
            col(t.series_both),
        );
        let band_alpha = t.band_alpha;
        let counts_entries = self.counts_legend();
        let util_entries = self.util_legend();
        let legend_h = legend_height(counts_entries.len().max(util_entries.len()));
        // 見出し + グラフ + 凡例。グラフの高さは固定（凡例の分を引く）
        let h = (ui.available_height() - 28.0 - legend_h).max(80.0);
        let hidden = self.hidden_series.clone();
        // 書き出しの要求（SVG / PNG）と、凡例の操作。描画の借用が終わってから処理する
        let mut export: Option<(usize, bool)> = None;
        let mut chart_rects = [egui::Rect::ZERO; 2];
        let mut new_hidden = hidden.clone();
        ui.columns(2, |cols| {
            cols[0].horizontal(|ui| {
                ui.strong("ラウンド別の人数");
                if ui
                    .small_button("SVG")
                    .on_hover_text("時系列を SVG で保存")
                    .clicked()
                {
                    export = Some((0, true));
                }
                if ui
                    .small_button("PNG")
                    .on_hover_text("時系列を PNG で保存")
                    .clicked()
                {
                    export = Some((0, false));
                }
            });
            fixed_plot(Plot::new("round_counts"))
                .height(h)
                .x_axis_label("ラウンド")
                .show(&mut cols[0], |p| {
                    let x: Vec<f64> = self.slots[0]
                        .rounds
                        .iter()
                        .map(|r| r.summary.round as f64)
                        .collect();
                    let line =
                        |name: &str, f: &dyn Fn(&crate::state::RoundData) -> f64, c: Color32| {
                            Line::new(
                                name.to_string(),
                                PlotPoints::new(
                                    x.iter()
                                        .zip(&self.slots[0].rounds)
                                        .map(|(&x, r)| [x, f(r)])
                                        .collect(),
                                ),
                            )
                            .color(c)
                            .width(1.6)
                            // 線の下を淡く塗る（面グラフ風。人数は 0 以上なので 0 まで）
                            .fill(0.0)
                            .fill_alpha(0.12)
                        };
                    if is_visible(&hidden, "新規シェア") {
                        p.line(line("新規シェア", &|r| r.summary.n_share as f64, c_int));
                    }
                    if is_visible(&hidden, "新規外部行動") {
                        p.line(line("新規外部行動", &|r| r.summary.new_xact as f64, c_new));
                    }
                    if is_visible(&hidden, "累積外部行動") {
                        p.line(line("累積外部行動", &|r| r.summary.cum_xact as f64, c_ext));
                    }
                    if self.rec_on(0) {
                        let c = col(self.tokens.series_rec_xact);
                        if is_visible(&hidden, "推薦経由の外部行動") {
                            p.line(line(
                                "推薦経由の外部行動",
                                &|r| (r.summary.rec_xact + r.summary.chain_xact) as f64,
                                c,
                            ));
                        }
                        if is_visible(&hidden, "推薦で受信してシェア") {
                            p.line(
                                line(
                                    "推薦で受信してシェア",
                                    &|r| r.summary.rec_share as f64,
                                    col(self.tokens.series_rec_share),
                                )
                                .style(LineStyle::dotted_dense()),
                            );
                        }
                        if is_visible(&hidden, "推薦で受信") {
                            p.line(
                                line("推薦で受信", &|r| r.summary.rec_received as f64, c)
                                    .style(LineStyle::dashed_loose()),
                            );
                        }
                        if is_visible(&hidden, "推薦で受信（累積人数）") {
                            p.line(
                                line("推薦で受信（累積人数）", &|r| r.summary.cum_rec as f64, c)
                                    .style(LineStyle::dashed_dense()),
                            );
                        }
                    }
                    if self.compare {
                        // 比較（F13）: B を破線で重ねる
                        let bx: Vec<f64> = self.slots[1]
                            .rounds
                            .iter()
                            .map(|r| r.summary.round as f64)
                            .collect();
                        let bl = |name: &str,
                                  f: &dyn Fn(&crate::state::RoundData) -> f64,
                                  c: Color32| {
                            Line::new(
                                name.to_string(),
                                PlotPoints::new(
                                    bx.iter()
                                        .zip(&self.slots[1].rounds)
                                        .map(|(&x, r)| [x, f(r)])
                                        .collect(),
                                ),
                            )
                            .color(c)
                            .width(1.6)
                            .style(LineStyle::dashed_dense())
                        };
                        if is_visible(&hidden, "B 新規シェア") {
                            p.line(bl("B 新規シェア", &|r| r.summary.n_share as f64, c_int));
                        }
                        if is_visible(&hidden, "B 新規外部行動") {
                            p.line(bl("B 新規外部行動", &|r| r.summary.new_xact as f64, c_new));
                        }
                        if is_visible(&hidden, "B 累積外部行動") {
                            p.line(bl("B 累積外部行動", &|r| r.summary.cum_xact as f64, c_ext));
                        }
                    }
                    if let Some(mc) = &self.slots[0].mc_result {
                        band_lines(
                            p,
                            "MC 累積外部行動（平均）",
                            &mc.cum_xact,
                            1.0,
                            c_both,
                            band_alpha,
                            &hidden,
                        );
                        band_lines(
                            p,
                            "MC 新規シェア（平均）",
                            &mc.new_share,
                            1.0,
                            col_a(self.tokens.series_internal, 0.6),
                            band_alpha,
                            &hidden,
                        );
                        if self.rec_on(0) {
                            band_lines(
                                p,
                                "MC 推薦で受信・累積（平均）",
                                &mc.cum_rec,
                                1.0,
                                col(self.tokens.series_rec_xact),
                                band_alpha,
                                &hidden,
                            );
                        }
                    }
                });
            legend_row(&mut cols[0], &counts_entries, &mut new_hidden);
            chart_rects[0] = cols[0].min_rect();
            cols[1].horizontal(|ui| {
                ui.strong("効用（受信済みノードの平均と 10〜90% の帯）");
                if ui
                    .small_button("SVG")
                    .on_hover_text("時系列を SVG で保存")
                    .clicked()
                {
                    export = Some((1, true));
                }
                if ui
                    .small_button("PNG")
                    .on_hover_text("時系列を PNG で保存")
                    .clicked()
                {
                    export = Some((1, false));
                }
            });
            fixed_plot(Plot::new("round_util"))
                .height(h)
                .x_axis_label("ラウンド")
                .show(&mut cols[1], |p| {
                    let r: Vec<&crate::state::RoundData> = self.slots[0].rounds.iter().collect();
                    let x: Vec<f64> = r.iter().map(|r| r.summary.round as f64).collect();
                    let series = |f: &dyn Fn(&sim_api::log::UtilityStats) -> f32| -> Vec<[f64; 2]> {
                        x.iter()
                            .zip(&r)
                            .map(|(&x, r)| [x, f(&r.summary.recv) as f64])
                            .collect()
                    };
                    // 面の塗りの基準（グラフの底）: 表示する値の最小
                    let floor = r
                        .iter()
                        .flat_map(|r| [r.summary.recv.q_int[0], r.summary.recv.q_ext[0]])
                        .filter(|v| v.is_finite())
                        .fold(f32::INFINITY, f32::min);
                    let floor = if floor.is_finite() { floor } else { 0.0 };
                    for (name, c, mean, lo, hi) in [
                        (
                            "内部",
                            c_int,
                            series(&|s| s.mean_int),
                            series(&|s| s.q_int[0]),
                            series(&|s| s.q_int[2]),
                        ),
                        (
                            "外部",
                            c_ext,
                            series(&|s| s.mean_ext),
                            series(&|s| s.q_ext[0]),
                            series(&|s| s.q_ext[2]),
                        ),
                    ] {
                        if !is_visible(&hidden, name) {
                            continue;
                        }
                        let mut pts: Vec<[f64; 2]> = hi.clone();
                        pts.extend(lo.iter().rev().copied());
                        p.add(
                            Polygon::new(format!("{name} 10〜90%"), PlotPoints::new(pts))
                                .fill_color(col_a_c(c, band_alpha))
                                .stroke(Stroke::NONE),
                        );
                        p.line(
                            Line::new(format!("{name} 平均"), PlotPoints::new(mean))
                                .color(c)
                                .width(1.6)
                                .fill(floor)
                                .fill_alpha(0.10),
                        );
                    }
                    if self.compare {
                        // 比較（F13）: B の効用の平均を破線で重ねる
                        let b: Vec<(f64, &sim_api::log::UtilityStats)> = self.slots[1]
                            .rounds
                            .iter()
                            .map(|r| (r.summary.round as f64, &r.summary.recv))
                            .collect();
                        let bi: Vec<[f64; 2]> =
                            b.iter().map(|(x, st)| [*x, st.mean_int as f64]).collect();
                        let be: Vec<[f64; 2]> =
                            b.iter().map(|(x, st)| [*x, st.mean_ext as f64]).collect();
                        if is_visible(&hidden, "B 内部 平均") {
                            p.line(
                                Line::new("B 内部 平均", PlotPoints::new(bi))
                                    .color(c_int)
                                    .width(1.6)
                                    .style(LineStyle::dashed_dense()),
                            );
                        }
                        if is_visible(&hidden, "B 外部 平均") {
                            p.line(
                                Line::new("B 外部 平均", PlotPoints::new(be))
                                    .color(c_ext)
                                    .width(1.6)
                                    .style(LineStyle::dashed_dense()),
                            );
                        }
                    }
                    if let Some(mc) = &self.slots[0].mc_result {
                        if is_visible(&hidden, "MC 内部 平均") {
                            p.line(
                                Line::new("MC 内部 平均", points(&mc.mean_u_int.mean, 1.0))
                                    .color(c_int)
                                    .style(LineStyle::dashed_dense()),
                            );
                        }
                        if is_visible(&hidden, "MC 外部 平均") {
                            p.line(
                                Line::new("MC 外部 平均", points(&mc.mean_u_ext.mean, 1.0))
                                    .color(c_ext)
                                    .style(LineStyle::dashed_dense()),
                            );
                        }
                    }
                });
            legend_row(&mut cols[1], &util_entries, &mut new_hidden);
            chart_rects[1] = cols[1].min_rect();
        });
        self.hidden_series = new_hidden;
        if let Some((which, svg)) = export {
            let (name, rect) = if which == 0 {
                ("round-counts", chart_rects[0])
            } else {
                ("round-utility", chart_rects[1])
            };
            let ctx = ui.ctx().clone();
            if svg {
                if let Some(p) = rfd::FileDialog::new()
                    .set_file_name(format!("{name}.svg"))
                    .add_filter("SVG", &["svg"])
                    .save_file()
                {
                    let text = if which == 0 {
                        self.counts_svg()
                    } else {
                        self.util_svg()
                    };
                    match std::fs::write(&p, text) {
                        Ok(()) => self.status = Some(format!("{} に保存しました", p.display())),
                        Err(e) => self.error = Some(e.to_string()),
                    }
                }
            } else if let Some(p) = rfd::FileDialog::new()
                .set_file_name(format!("{name}.png"))
                .add_filter("PNG", &["png"])
                .save_file()
            {
                self.request_crop(&ctx, rect, p);
            }
        }
    }
}
