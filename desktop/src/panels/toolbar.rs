//! ツールバー: 設定の保存・読み込み、書き出し、現在ラウンド／T の表示（F01・F14）。

use eframe::egui;
use sim_api::config::ScenarioConfig;
use sim_api::export::{self, GraphMeta};

use crate::app::App;

impl App {
    pub fn ui_toolbar(&mut self, ui: &mut egui::Ui) {
        crate::theme::ghost_buttons(ui);
        ui.horizontal_centered(|ui| {
            ui.heading("情報拡散ライブビューア");
            ui.separator();
            if ui.button("設定を保存").clicked() {
                self.save_config();
            }
            if ui.button("設定を読み込む").clicked() {
                self.load_config();
            }
            ui.separator();
            let has = self.slots[0].session.is_some() && !self.slots[0].rounds.is_empty();
            if ui
                .add_enabled(has, egui::Button::new("シナリオ記録 (JSON)"))
                .clicked()
            {
                self.export_text("scenario.json", "JSON", &["json"], |s, m| {
                    export::scenario_json(s, m).map_err(|e| e.to_string())
                });
            }
            if ui
                .add_enabled(has, egui::Button::new("ラウンド別 (CSV)"))
                .clicked()
            {
                self.export_text("rounds.csv", "CSV", &["csv"], |s, _| {
                    Ok(export::round_rows_csv(s))
                });
            }
            if ui
                .add_enabled(has, egui::Button::new("ノード別イベント (CSV)"))
                .clicked()
            {
                self.export_text("events.csv", "CSV", &["csv"], |s, _| {
                    Ok(export::events_csv(s, None))
                });
            }
            if ui
                .add_enabled(has, egui::Button::new("ノード別イベント (Parquet)"))
                .clicked()
            {
                if let (Some(s), Some(p)) = (
                    &self.slots[0].session,
                    rfd::FileDialog::new()
                        .set_file_name("events.parquet")
                        .add_filter("Parquet", &["parquet"])
                        .save_file(),
                ) {
                    match crate::export_parquet::write_events_parquet(&p, s) {
                        Ok(n) => {
                            self.status = Some(format!("{} に {n} 行を書き出しました", p.display()))
                        }
                        Err(e) => self.error = Some(e),
                    }
                }
            }
            ui.separator();
            let ctx = ui.ctx().clone();
            if ui.add_enabled(self.slots[0].session.is_some(), egui::Button::new("画像 (PNG)")).on_hover_text("ネットワーク図を PNG で保存（比較中は A と B を左右に並べる）").clicked() {
                if let Some(p) = rfd::FileDialog::new().set_file_name("network.png").add_filter("PNG", &["png"]).save_file() {
                    let rect = self.net_rect;
                    self.request_crop(&ctx, rect, p);
                }
            }
            if ui.add_enabled(has && self.record.is_none(), egui::Button::new("連番 PNG…")).on_hover_text("いまのラウンドの再生を、ウェーブごとに PNG にしてフォルダへ保存").clicked() {
                if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                    self.start_record(dir);
                }
            }
            let can_mp4 = self.last_record.is_some() && self.mp4_rx.is_none() && self.ffmpeg_ok();
            if ui.add_enabled(can_mp4, egui::Button::new("MP4 にする")).on_hover_text("直前に書き出した連番 PNG を ffmpeg で MP4 にまとめる（ffmpeg が PATH にあるとき）").clicked() {
                if let Some(p) = rfd::FileDialog::new().set_file_name("diffusion.mp4").add_filter("MP4", &["mp4"]).save_file() {
                    self.start_mp4(&ctx, p);
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.strong(format!(
                    "ラウンド {} / {}",
                    self.slots[0].rounds.len(),
                    self.max_rounds()
                ));
            });
        });
    }

    fn graph_meta(&self) -> GraphMeta {
        GraphMeta {
            source_name: self.graph_opts.name.clone(),
            directed: self.graph_opts.directed,
            transposed: self.graph_opts.transposed,
        }
    }

    fn export_text(
        &mut self,
        name: &str,
        label: &str,
        ext: &[&str],
        f: impl Fn(&sim_api::session::Session, &GraphMeta) -> Result<String, String>,
    ) {
        let Some(s) = &self.slots[0].session else {
            return;
        };
        let Some(p) = rfd::FileDialog::new()
            .set_file_name(name)
            .add_filter(label, ext)
            .save_file()
        else {
            return;
        };
        match f(s, &self.graph_meta())
            .and_then(|t| std::fs::write(&p, t).map_err(|e| e.to_string()))
        {
            Ok(()) => self.status = Some(format!("{} を書き出しました", p.display())),
            Err(e) => self.error = Some(e),
        }
    }

    fn save_config(&mut self) {
        let Some(p) = rfd::FileDialog::new()
            .set_file_name("scenario-config.json")
            .add_filter("JSON", &["json"])
            .save_file()
        else {
            return;
        };
        match serde_json::to_string_pretty(&self.slots[0].form)
            .map_err(|e| e.to_string())
            .and_then(|t| std::fs::write(&p, t).map_err(|e| e.to_string()))
        {
            Ok(()) => self.status = Some(format!("設定を {} に保存しました", p.display())),
            Err(e) => self.error = Some(e),
        }
    }

    fn load_config(&mut self) {
        let Some(p) = rfd::FileDialog::new()
            .add_filter("設定（JSON / TOML）", &["json", "toml"])
            .pick_file()
        else {
            return;
        };
        let r = std::fs::read_to_string(&p)
            .map_err(|e| e.to_string())
            .and_then(|t| {
                if p.extension().is_some_and(|e| e == "toml") {
                    ScenarioConfig::from_toml(&t).map_err(|e| e.to_string())
                } else {
                    // シナリオ記録 JSON（config キーを持つ）も読める
                    let v: serde_json::Value =
                        serde_json::from_str(&t).map_err(|e| e.to_string())?;
                    let cfg = v.get("config").cloned().unwrap_or(v);
                    ScenarioConfig::from_json(&cfg.to_string()).map_err(|e| e.to_string())
                }
            });
        match r {
            Ok(cfg) => {
                self.slots[0].form = cfg;
                self.status = Some("設定を読み込みました（「設定を適用」で反映）".to_string());
            }
            Err(e) => self.error = Some(format!("設定を読み込めません: {e}")),
        }
    }
}
