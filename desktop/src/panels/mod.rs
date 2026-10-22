pub mod charts;
pub mod control;
pub mod detail;
pub mod legend;
pub mod media;
pub mod network;
pub mod toolbar;
pub mod tree;

use eframe::egui::Color32;

/// トークンの色（0〜1）→ egui の色。
pub fn col(c: [f32; 3]) -> Color32 {
    Color32::from_rgb(
        (c[0] * 255.0).round() as u8,
        (c[1] * 255.0).round() as u8,
        (c[2] * 255.0).round() as u8,
    )
}

pub fn col_a(c: [f32; 3], a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(
        (c[0] * 255.0).round() as u8,
        (c[1] * 255.0).round() as u8,
        (c[2] * 255.0).round() as u8,
        (a * 255.0).round() as u8,
    )
}
