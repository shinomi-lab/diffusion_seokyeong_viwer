//! 情報拡散ライブビューア（デスクトップ版）。`sim-api` を同じプロセスで直接呼ぶ（10 章）。

// 入れ子の if を let チェーンにまとめる提案（collapsible_if）は、読みやすさを優先して使わない。
// to_screen などは Copy な小さな構造体だが、座標変換の名前として自然なので &self のまま使う。
// スロット番号 i を、配列の添字とスロット自体の指定の両方に使う箇所は、添字のループのまま書く（needless_range_loop）。
#![allow(
    clippy::collapsible_if,
    clippy::wrong_self_convention,
    clippy::needless_range_loop
)]

mod app;
mod camera;
mod capture;
mod cli;
mod export_parquet;
#[cfg(test)]
mod golden_tests;
mod gpu;
mod layout;
mod lod;
mod panels;
mod playback;
mod state;
mod svg;
mod theme;
mod tokens;
mod ui;
mod worker;

use eframe::egui_wgpu::{WgpuConfiguration, WgpuSetup};
use eframe::wgpu;

fn main() -> eframe::Result {
    let cli = match cli::parse(std::env::args()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    let mut wgpu_options = WgpuConfiguration::default();
    if cli.bench.is_some() {
        // 描画性能を測るので、垂直同期で fps が頭打ちにならないようにする
        wgpu_options.present_mode = wgpu::PresentMode::AutoNoVsync;
    }
    let _ = WgpuSetup::default;
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        wgpu_options,
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("情報拡散ライブビューア")
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([1280.0, 720.0]),
        ..Default::default()
    };
    eframe::run_native(
        "情報拡散ライブビューア",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc, cli)))),
    )
}
