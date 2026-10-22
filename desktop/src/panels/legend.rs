//! グラフの固定と、グラフの下に分離した凡例（系列の表示・非表示の切り替え）。Web 版（uPlot）と同じ体裁。
//! egui_plot 組み込みの凡例は、グラフの上に重なって大きく、線を隠すので使わない。

use std::collections::HashSet;

use eframe::egui::{self, Align2, Color32, FontId, Sense, Stroke, vec2};
use egui_plot::Plot;

/// グラフを固定する（ドラッグ・拡大縮小・スクロール・範囲選択・ダブルクリックのリセットをすべて無効にする）。
/// 軸の範囲はデータに合わせて自動で決まる（上限・下限は付けない。INV-15）。凡例は `legend_row` で別に出す。
/// 格子線は横だけ（縦の格子線は出さない。Datawrapper 等の定石）。
pub fn fixed_plot(p: Plot<'_>) -> Plot<'_> {
    p.show_grid([false, true])
        .allow_drag(false)
        .allow_zoom(false)
        .allow_scroll(false)
        .allow_boxed_zoom(false)
        .allow_double_click_reset(false)
}

#[derive(Clone, Debug, PartialEq)]
pub struct LegendEntry {
    pub name: String,
    pub color: Color32,
    pub dashed: bool,
}

impl LegendEntry {
    pub fn new(name: impl Into<String>, color: Color32, dashed: bool) -> Self {
        Self {
            name: name.into(),
            color,
            dashed,
        }
    }
}

/// 系列を描くか（凡例で非表示にされていないか）。
pub fn is_visible(hidden: &HashSet<String>, name: &str) -> bool {
    !hidden.contains(name)
}

/// 凡例のクリック: 表示 ⇄ 非表示。
pub fn toggle(hidden: &mut HashSet<String>, name: &str) {
    if !hidden.remove(name) {
        hidden.insert(name.to_string());
    }
}

/// 凡例の行数の見積もり（グラフの高さから引く）。
pub fn legend_height(n_entries: usize) -> f32 {
    let rows = n_entries.div_ceil(3).max(1);
    rows as f32 * 19.0 + 4.0
}

/// グラフの下に置く凡例。小さな色の見本（実線・破線）と系列名を並べ、クリックで系列の表示・非表示を切り替える。
pub fn legend_row(ui: &mut egui::Ui, entries: &[LegendEntry], hidden: &mut HashSet<String>) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(10.0, 3.0);
        for e in entries {
            let visible = is_visible(hidden, &e.name);
            let font = FontId::proportional(11.5);
            let galley =
                ui.painter()
                    .layout_no_wrap(e.name.clone(), font.clone(), Color32::PLACEHOLDER);
            let size = vec2(26.0 + galley.size().x + 4.0, 16.0);
            let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
            let alpha = if visible { 1.0 } else { 0.3 };
            let color = e.color.gamma_multiply(alpha);
            let p = ui.painter();
            let y = rect.center().y;
            if e.dashed {
                let mut x = rect.left() + 1.0;
                while x < rect.left() + 21.0 {
                    p.line_segment(
                        [
                            egui::pos2(x, y),
                            egui::pos2((x + 4.0).min(rect.left() + 21.0), y),
                        ],
                        Stroke::new(2.0, color),
                    );
                    x += 7.0;
                }
            } else {
                p.line_segment(
                    [
                        egui::pos2(rect.left() + 1.0, y),
                        egui::pos2(rect.left() + 21.0, y),
                    ],
                    Stroke::new(2.0, color),
                );
            }
            let text_color =
                ui.visuals()
                    .text_color()
                    .gamma_multiply(if visible { 1.0 } else { 0.4 });
            p.text(
                egui::pos2(rect.left() + 26.0, y),
                Align2::LEFT_CENTER,
                &e.name,
                font,
                text_color,
            );
            if resp
                .on_hover_text(if visible {
                    "クリックで非表示"
                } else {
                    "クリックで表示"
                })
                .clicked()
            {
                toggle(hidden, &e.name);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggle_hides_and_shows() {
        let mut h = HashSet::new();
        assert!(is_visible(&h, "新規シェア"));
        toggle(&mut h, "新規シェア");
        assert!(!is_visible(&h, "新規シェア"));
        assert!(is_visible(&h, "累積外部行動"), "ほかの系列には影響しない");
        toggle(&mut h, "新規シェア");
        assert!(is_visible(&h, "新規シェア"));
        assert!(h.is_empty());
    }

    #[test]
    fn legend_height_grows_with_rows() {
        assert!(legend_height(3) < legend_height(4));
        assert!(legend_height(0) > 0.0);
        assert_eq!(legend_height(1), legend_height(3));
    }
}
