//! 見た目のテーマ（`spec/ui-tokens.json` の `chrome`）。配置・寸法は変えず、色・枠線・角丸・影だけを決める。
//! 配色はダーク専用で、OS がライトでもダークで表示する。
//! 中立色の段階で UI を作り、アクセントの青は操作できる要素（選択中・押下・フォーカス）だけに使う。
//! 性能: 影はポップアップとウィンドウだけに小さく付ける（パネルには付けず、塗りと枠線だけ）。

use eframe::egui::{self, Color32, CornerRadius, Frame, Margin, Shadow, Stroke, Theme, Visuals};

use crate::tokens::{Chrome, Tokens};

pub fn c32(c: [f32; 3]) -> Color32 {
    Color32::from_rgb(
        (c[0] * 255.0).round() as u8,
        (c[1] * 255.0).round() as u8,
        (c[2] * 255.0).round() as u8,
    )
}

fn visuals(t: &Tokens) -> Visuals {
    let c: &Chrome = &t.chrome;
    let mut v = Visuals::dark();
    // egui の部品は文字が小さく、角丸 7px だとチェックボックスが円に見えるので 4px に抑える
    let r_ctl = CornerRadius::same((t.radius_control as u8).min(4));
    let r_card = CornerRadius::same(t.radius_card as u8);
    v.panel_fill = c32(c.card);
    v.window_fill = c32(c.card);
    v.faint_bg_color = c32(c.ui);
    v.extreme_bg_color = c32(c.card);
    v.code_bg_color = c32(c.ui);
    v.window_stroke = Stroke::new(1.0, c32(c.border));
    v.window_corner_radius = r_card;
    v.menu_corner_radius = r_ctl;
    let shadow_color = Color32::from_black_alpha(90);
    v.window_shadow = Shadow {
        offset: [0, 2],
        blur: 8,
        spread: 0,
        color: shadow_color,
    };
    v.popup_shadow = Shadow {
        offset: [0, 1],
        blur: 4,
        spread: 0,
        color: shadow_color,
    };
    v.hyperlink_color = c32(c.accent);
    v.selection.bg_fill = c32(c.accent);
    v.selection.stroke = Stroke::new(1.0, c32(c.on_accent));
    v.slider_trailing_fill = true;
    v.handle_shape = egui::style::HandleShape::Circle;
    // 文字: 本文は段階 12、ラベル等の弱い文字は段階 11
    let text = c32(c.text);
    let muted = c32(c.muted);
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = c32(c.card);
    w.noninteractive.weak_bg_fill = c32(c.card);
    w.noninteractive.bg_stroke = Stroke::new(1.0, c32(c.border));
    w.noninteractive.fg_stroke = Stroke::new(1.0, text);
    w.noninteractive.corner_radius = r_ctl;
    w.inactive.bg_fill = c32(c.border);
    w.inactive.weak_bg_fill = c32(c.ui);
    w.inactive.bg_stroke = Stroke::new(1.0, c32(c.border_strong));
    w.inactive.fg_stroke = Stroke::new(1.0, text);
    w.inactive.corner_radius = r_ctl;
    w.hovered.bg_fill = c32(c.hover);
    w.hovered.weak_bg_fill = c32(c.hover);
    w.hovered.bg_stroke = Stroke::new(1.0, muted);
    w.hovered.fg_stroke = Stroke::new(1.5, text);
    w.hovered.corner_radius = r_ctl;
    w.hovered.expansion = 0.0;
    w.active.bg_fill = c32(c.accent);
    w.active.weak_bg_fill = c32(c.accent_soft);
    w.active.bg_stroke = Stroke::new(1.0, c32(c.accent));
    w.active.fg_stroke = Stroke::new(1.5, text);
    w.active.corner_radius = r_ctl;
    w.active.expansion = 0.0;
    w.open.weak_bg_fill = c32(c.accent_soft);
    w.open.bg_stroke = Stroke::new(1.0, c32(c.accent));
    w.open.corner_radius = r_ctl;
    v
}

/// ダーク専用の見た目を設定する。OS の設定に関係なくダークに固定し、念のためライト側にも同じ見た目を入れる。起動時に 1 回呼ぶ。
pub fn apply(ctx: &egui::Context, t: &Tokens) {
    ctx.set_theme(Theme::Dark);
    for theme in [Theme::Light, Theme::Dark] {
        ctx.set_visuals_of(theme, visuals(t));
    }
}

/// 操作パネルの地（画面の地 = 段階 1）。節はこの上にカードとして置く。
pub fn app_frame(t: &Tokens) -> Frame {
    let c = &t.chrome;
    Frame::new()
        .fill(c32(c.app))
        .inner_margin(Margin::symmetric(8, 8))
        .stroke(Stroke::new(1.0, c32(c.border)))
}

/// カード（段階 2 の塗り + 段階 6 の枠線 + 角丸）。影は付けない（毎フレームの形状を増やさない）。
pub fn card<R>(ui: &mut egui::Ui, t: &Tokens, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let c = &t.chrome;
    let r = Frame::new()
        .fill(c32(c.card))
        .stroke(Stroke::new(1.0, c32(c.border)))
        .corner_radius(CornerRadius::same(t.radius_card as u8))
        .inner_margin(Margin {
            left: 8,
            right: 8,
            top: 4,
            bottom: 6,
        })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner;
    ui.add_space(6.0);
    r
}

/// ツールバー: ボタンを枠なし（hover で淡く塗る）にする。このパネルの中だけに効く。
pub fn ghost_buttons(ui: &mut egui::Ui) {
    let w = &mut ui.style_mut().visuals.widgets;
    w.inactive.weak_bg_fill = Color32::TRANSPARENT;
    w.inactive.bg_stroke = Stroke::NONE;
}

/// アクセントで塗った主ボタン（「発信」など）。
pub fn primary_button(t: &Tokens, text: egui::RichText) -> egui::Button<'static> {
    let c = &t.chrome;
    egui::Button::new(text.strong().color(c32(c.on_accent)))
        .fill(c32(c.accent))
        .stroke(Stroke::new(1.0, c32(c.accent)))
}
