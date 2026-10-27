//! 操作パネル（9.3）: グラフ設定 → シナリオ設定 → 機構 M1（`param_specs()` から自動生成）→ 推薦 → プリセット → IP 選択 → メッセージ → 発信。

use eframe::egui::{self, RichText};
use sim_api::config::{MechanismConfig, RecMethod, RecommenderConfig, ScenarioConfig};
use sim_api::graph_io::GraphFormat;
use sim_api::params::{
    ParamKind, ParamSpec, get_rec_value, param_specs, rec_param_specs, rec_param_used,
};

use crate::app::{App, Metric};

/// `ParamSpec.id`（例 `m1.kappa`）に対応する値の読み書き。
fn get_f(c: &MechanismConfig, id: &str) -> f64 {
    sim_api::params::get_value(c, id).unwrap_or(0.0)
}

// M5 を戻すときに腕を足すので、腕が 1 本でも match のままにする
#[allow(clippy::single_match)]
fn set_f(c: &mut MechanismConfig, id: &str, v: f64) {
    match id {
        "m1.kappa" => c.m1.kappa = v,
        // M5（検討中のためコメントアウト）
        // "m5.eta" => c.m5.eta = v,
        // "m5.n_max" => c.m5.n_max = v.round().max(1.0) as u32,
        _ => {}
    }
}

fn enabled_mut<'a>(c: &'a mut MechanismConfig, m: &str) -> Option<&'a mut bool> {
    Some(match m {
        "m1" => &mut c.m1.enabled,
        // "m5" => &mut c.m5.enabled,
        _ => return None,
    })
}

/// 拡張機構（M1）の編集（プリセットと、`param_specs()` の登録簿から自動生成したスライダー。N07）。A・B のどちらにも使う。
fn mechanism_editor(ui: &mut egui::Ui, mech: &mut MechanismConfig) {
    ui.horizontal_wrapped(|ui| {
        if ui.button("現行（すべて無効）").clicked() {
            *mech = MechanismConfig::off();
        }
        if ui.button("既定").clicked() {
            *mech = MechanismConfig::balanced();
        }
        if ui.button("既定の2倍").clicked() {
            *mech = MechanismConfig::scaled(2.0);
        }
    });
    let specs: Vec<ParamSpec> = param_specs().to_vec();
    let mut last_mech = String::new();
    for s in &specs {
        let mid = s.id.split('.').next().unwrap_or("");
        if s.mechanism != last_mech {
            last_mech = s.mechanism.to_string();
            if let Some(en) = enabled_mut(mech, mid) {
                ui.checkbox(en, RichText::new(s.mechanism).strong());
            }
        }
        let on = enabled_mut(mech, mid).map(|b| *b).unwrap_or(false);
        ui.add_enabled_ui(on, |ui| match s.kind {
            // 真偽値の項目（旧 M4 の m∅）は廃止。登録簿に足したらここで扱う
            ParamKind::Bool => {}
            _ => {
                let mut v = get_f(mech, s.id);
                // ラベルは折り返せるように、スライダーとは別に置く
                ui.add(egui::Label::new(s.label).wrap());
                let mut sl = egui::Slider::new(&mut v, s.min.unwrap_or(0.0)..=s.max.unwrap_or(1.0));
                if let Some(st) = s.step {
                    sl = sl.step_by(st);
                }
                if ui
                    .add(sl)
                    .on_hover_text(format!("無効値: {}", s.off))
                    .changed()
                {
                    set_f(mech, s.id, v);
                }
            }
        });
    }
}

/// 推薦の編集（on/off と、`rec_param_specs()` から自動生成したスライダー。`recommender-design.md` 10 章）。A・B のどちらにも使う。
fn recommender_editor(ui: &mut egui::Ui, rec: &mut RecommenderConfig) {
    ui.checkbox(&mut rec.enabled, RichText::new("推薦").strong())
        .on_hover_text("フォロー経由の連鎖のあと、スコアが閾値以上の未受信者にも届ける");
    let on = rec.enabled;
    ui.add_enabled_ui(on, |ui| {
        egui::ComboBox::from_label("方式")
            .selected_text(match rec.method {
                RecMethod::Hybrid => "CF＋グラフ（v2）",
                RecMethod::Baseline => "基準（v1: 個人の反応率）",
                RecMethod::Wave => "Rモデル（v3: 波ごとの押し出し）",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut rec.method, RecMethod::Hybrid, "CF＋グラフ（v2）");
                ui.selectable_value(&mut rec.method, RecMethod::Baseline, "基準（v1: 個人の反応率）");
                ui.selectable_value(&mut rec.method, RecMethod::Wave, "Rモデル（v3: 波ごとの押し出し）");
            })
            .response
            .on_hover_text("基準: 本人の同じ種類の反応率。CF＋グラフ: 種類どうしの似かた（協調フィルタリング）とフォロー先のシェア（社会的証明）。Rモデル: 各ウェーブのあと、反応率に応じた数だけフォロー外へ届ける");
    });
    for s in rec_param_specs() {
        // 方式ごとに使う項目だけを有効にする
        let usable = on && rec_param_used(rec.method, s.id);
        ui.add_enabled_ui(usable, |ui| {
            let mut v = get_rec_value(rec, s.id).unwrap_or(0.0);
            ui.add(egui::Label::new(s.label).wrap());
            let mut sl = egui::Slider::new(&mut v, s.min.unwrap_or(0.0)..=s.max.unwrap_or(1.0));
            if let Some(st) = s.step {
                sl = sl.step_by(st);
            }
            if ui.add(sl).changed() {
                match s.id {
                    "rec.threshold" => rec.threshold = v,
                    "rec.w_share" => rec.w_share = v,
                    "rec.prior_n0" => rec.prior_n0 = v,
                    "rec.alpha" => rec.alpha = v,
                    "rec.min_common" => rec.min_common = v.round().max(1.0) as u32,
                    "rec.eps" => rec.eps = v,
                    "rec.delta" => rec.delta = v,
                    "rec.near" => rec.near = v,
                    "rec.tau" => rec.tau = v,
                    "rec.phi_max" => rec.phi_max = v,
                    "rec.k" => rec.k = v,
                    "rec.mu" => rec.mu = v,
                    _ => {}
                }
            }
        });
    }
}

impl App {
    pub fn ui_control(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                // 長いラベルでパネルが広がらないよう、幅を固定する（幅は 11.2 の 280px から余白を引いた値）
                let w = ui.available_width();
                ui.set_max_width(w);
                let t = self.tokens.clone();
                crate::theme::card(ui, &t, |ui| self.ui_graph_section(ui));
                crate::theme::card(ui, &t, |ui| self.ui_scenario_section(ui));
                crate::theme::card(ui, &t, |ui| self.ui_mechanism_section(ui));
                crate::theme::card(ui, &t, |ui| self.ui_recommender_section(ui));
                crate::theme::card(ui, &t, |ui| self.ui_compare_section(ui));
                self.ui_apply(ui);
                crate::theme::card(ui, &t, |ui| self.ui_ip_section(ui));
                crate::theme::card(ui, &t, |ui| self.ui_palette_section(ui));
                if let Some(e) = self.error.clone() {
                    if ui
                        .colored_label(egui::Color32::from_rgb(200, 40, 40), format!("⚠ {e}"))
                        .on_hover_text("クリックで消す")
                        .clicked()
                    {
                        self.error = None;
                    }
                }
            });
    }

    fn ui_graph_section(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new(RichText::new("グラフ").strong()).default_open(true).show(ui, |ui| {
            if ui.button("ファイルを選ぶ…").clicked() {
                if let Some(p) = rfd::FileDialog::new().add_filter("グラフ（txt/csv/tsv/gz）", &["txt", "csv", "tsv", "gz", "edgelist"]).pick_file() {
                    self.load_graph_path(&p.to_string_lossy());
                }
            }
            if ui.button("サンプル（100random）を読み込む").clicked() {
                let p = concat!(env!("CARGO_MANIFEST_DIR"), "/../test/100random.txt");
                // 仮定: サンプルの辺は「メッセージが流れる向き」なので transposed = false で読む
                self.graph_opts.transposed = false;
                self.load_graph_path(p);
            }
            egui::ComboBox::from_label("形式").selected_text(match self.graph_opts.format {
                GraphFormat::EdgeList => "edgelist（空白・タブ）",
                GraphFormat::Csv => "csv",
                GraphFormat::Tsv => "tsv",
            }).show_ui(ui, |ui| {
                ui.selectable_value(&mut self.graph_opts.format, GraphFormat::EdgeList, "edgelist（空白・タブ）");
                ui.selectable_value(&mut self.graph_opts.format, GraphFormat::Csv, "csv");
                ui.selectable_value(&mut self.graph_opts.format, GraphFormat::Tsv, "tsv");
            });
            ui.checkbox(&mut self.graph_opts.gzip, "gzip");
            ui.checkbox(&mut self.graph_opts.directed, "有向");
            ui.checkbox(&mut self.graph_opts.transposed, "transposed").on_hover_text("入力が「A→B＝A が B をフォロー」の形式のときは ON");
            ui.add(egui::Label::new(egui::RichText::new("transposed: フォロー関係（A→B＝A が B をフォロー）を拡散の向きに反転して読む").weak()).wrap());
            if !self.graph_opts.directed {
                ui.colored_label(egui::Color32::from_rgb(180, 90, 0), "有向と無向の結果は比較できません。");
            }
            if ui.add_enabled(self.graph_bytes.is_some(), egui::Button::new("この設定で再読み込み")).clicked() {
                self.reload_graph();
            }
            if !self.graph_label.is_empty() {
                ui.label(&self.graph_label);
                if self.slots[0].scenario.as_ref().is_some_and(|s| s.graph.n_nodes > crate::layout::EXPAND_CAP) {
                    ui.colored_label(egui::Color32::from_rgb(180, 90, 0), format!("ノード数が {0} を超えています。全体図から始まり、個別展開は {0} ノードまでです", crate::layout::EXPAND_CAP));
                }
            }
        });
    }

    fn ui_scenario_section(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new(RichText::new("シナリオ設定").strong())
            .default_open(true)
            .show(ui, |ui| {
                let f: &mut ScenarioConfig = &mut self.slots[0].form;
                egui::Grid::new("scn").num_columns(2).show(ui, |ui| {
                    ui.label("受信確率");
                    ui.add(
                        egui::DragValue::new(&mut f.receipt_probability)
                            .speed(0.01)
                            .range(0.0..=1.0),
                    );
                    ui.end_row();
                    ui.label("内部 β");
                    ui.add(egui::DragValue::new(&mut f.internal_parameters.beta).speed(0.01));
                    ui.end_row();
                    ui.label("内部 γ");
                    ui.add(egui::DragValue::new(&mut f.internal_parameters.gamma).speed(0.01));
                    ui.end_row();
                    ui.label("外部 β");
                    ui.add(egui::DragValue::new(&mut f.external_parameters.beta).speed(0.01));
                    ui.end_row();
                    ui.label("外部 γ");
                    ui.add(egui::DragValue::new(&mut f.external_parameters.gamma).speed(0.01));
                    ui.end_row();
                    ui.label("較正 T")
                        .on_hover_text("反応性の scale = 1/(β·較正T)。実行 T とは別");
                    ui.add(egui::DragValue::new(&mut f.calibration_rounds).range(1..=1000));
                    ui.end_row();
                    ui.label("実行 T（上限）");
                    ui.add(egui::DragValue::new(&mut f.max_rounds).range(1..=1000));
                    ui.end_row();
                    ui.label("母集団シード");
                    ui.add(egui::DragValue::new(&mut f.user_seed).range(0..=(1u64 << 53) - 1));
                    ui.end_row();
                    ui.label("拡散シード");
                    ui.add(egui::DragValue::new(&mut f.diffusion_seed).range(0..=(1u64 << 53) - 1));
                    ui.end_row();
                });
            });
    }

    fn ui_mechanism_section(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new(RichText::new("拡張機構（M1）").strong())
            .default_open(true)
            .show(ui, |ui| {
                mechanism_editor(ui, &mut self.slots[0].form.mechanisms);
            });
    }

    fn ui_recommender_section(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new(RichText::new("推薦").strong())
            .default_open(true)
            .show(ui, |ui| {
                recommender_editor(ui, &mut self.slots[0].form.recommender);
                self.ui_rec_sim(ui);
                self.ui_rec_wave_trace(ui);
            });
    }

    /// Rモデルの推薦の推移（直近のラウンドのウェーブごと。`recommender-v3-design.md` 9 章）。方式 wave のときだけ。
    fn ui_rec_wave_trace(&mut self, ui: &mut egui::Ui) {
        let Some(session) = &self.slots[0].session else { return };
        let warn = crate::panels::col(self.tokens.rec);
        let trace = session.rec_wave_trace();
        if trace.is_empty() {
            return;
        }
        egui::CollapsingHeader::new("推薦の推移（直近のラウンド）")
            .default_open(false)
            .show(ui, |ui| {
                egui::Grid::new("rec_wave_trace").striped(true).show(ui, |ui| {
                    for h in ["ウェーブ", "反応率 ê", "推薦量", "枠", "空振り"] {
                        ui.strong(h);
                    }
                    ui.end_row();
                    for r in trace {
                        ui.label(r.wave.to_string());
                        ui.label(format!("{:.3}", r.e_hat));
                        ui.label(format!("{:.2}", r.budget));
                        ui.label(r.slots.to_string());
                        ui.label(r.miss.to_string());
                        ui.end_row();
                    }
                });
                if session.scenario().ebar0 == 0.0 {
                    ui.add(egui::Label::new(egui::RichText::new("基準 ē0 = 0 のため推薦しません（平均的なメッセージを初めて受け取ってシェアする人が母集団にいない）。較正 T や反応性を変えると変わります。").color(warn)).wrap());
                }
                ui.add(egui::Label::new(egui::RichText::new(format!("次のラウンドの事前の見込み a = {:.3}（ē0 = {:.3}）", session.rec_prior(), session.scenario().ebar0)).weak()).wrap());
            });
    }

    /// 種類どうしの似かた（6×6、R12）。CF＋グラフのときだけ。ラウンドが進んだときだけ計算し直す。
    fn ui_rec_sim(&mut self, ui: &mut egui::Ui) {
        let slot = &mut self.slots[0];
        let Some(session) = &slot.session else { return };
        let n = slot.rounds.len();
        if slot.rec_sim_cache.as_ref().is_none_or(|(r, _)| *r != n) {
            slot.rec_sim_cache = Some((n, session.rec_sim()));
        }
        let Some((_, v)) = &slot.rec_sim_cache else {
            return;
        };
        if v.is_empty() {
            return;
        }
        let l = (v.len() as f64).sqrt().round() as usize;
        let c = crate::panels::col(self.tokens.rec);
        egui::CollapsingHeader::new("種類どうしの似かた（協調フィルタリング）")
            .default_open(false)
            .show(ui, |ui| {
                egui::Grid::new("rec_sim").spacing([2.0, 2.0]).show(ui, |ui| {
                    ui.label("");
                    for k in 0..l {
                        ui.strong(format!("m{k}"));
                    }
                    ui.end_row();
                    for m in 0..l {
                        ui.strong(format!("m{m}"));
                        for k in 0..l {
                            let x = v[m * l + k].clamp(0.0, 1.0);
                            let bg = egui::Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (x * 255.0) as u8);
                            // 黄色の塗りの上では暗い文字にする（濃いセルで明るい文字だと読めない）
                            let text = egui::RichText::new(format!("{x:.2}"));
                            let text = if x > 0.45 { text.color(egui::Color32::from_rgb(0x10, 0x13, 0x1A)) } else { text };
                            egui::Frame::new().fill(bg).inner_margin(2.0).show(ui, |ui| {
                                ui.label(text);
                            });
                        }
                        ui.end_row();
                    }
                });
                ui.add(egui::Label::new(egui::RichText::new("両方の種類を受け取った人の反応の似かた（0〜1）。濃いほど「同じ人に効く」と学ばれた組。対角は 1。").weak()).wrap());
            });
    }

    /// 2 画面比較（F13）: 同じグラフ・IP・シード・選択で、設定だけを変えた B を A と並べる。
    fn ui_compare_section(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new(RichText::new("2 画面比較（A と B）").strong())
            .default_open(true)
            .show(ui, |ui| {
                let mut on = self.compare;
                if ui
                    .add_enabled(
                        self.slots[0].scenario.is_some(),
                        egui::Checkbox::new(&mut on, "比較モード"),
                    )
                    .on_hover_text(
                        "同じグラフ・IP・シード・選択で、機構の設定だけを変えた B を左右に並べる",
                    )
                    .changed()
                {
                    self.set_compare(on);
                }
                if !self.compare {
                    return;
                }
                ui.add(
                    egui::Label::new(
                        "B の設定（機構・推薦）。A と同じ IP・同じ選択が B にも適用されます。",
                    )
                    .wrap(),
                );
                mechanism_editor(ui, &mut self.slots[1].form.mechanisms);
                recommender_editor(ui, &mut self.slots[1].form.recommender);
                let dirty = self.is_dirty_slot(1);
                if ui
                    .button(if dirty {
                        "B に設定を適用 ●未適用"
                    } else {
                        "B に設定を適用"
                    })
                    .clicked()
                {
                    self.apply_b();
                }
                if let Some(e) = self.slots[1].error.clone() {
                    ui.colored_label(egui::Color32::from_rgb(200, 40, 40), format!("B: {e}"));
                }
            });
    }

    fn ui_apply(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        if self.confirming_apply {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.colored_label(
                    egui::Color32::from_rgb(180, 90, 0),
                    format!(
                        "設定を変えるとセッションを作り直します。{} ラウンドの履歴は消えます。",
                        self.slots[0].rounds.len()
                    ),
                );
                ui.horizontal(|ui| {
                    if ui.button("適用する").clicked() {
                        self.apply_form();
                    }
                    if ui.button("キャンセル").clicked() {
                        self.confirming_apply = false;
                    }
                });
            });
        } else {
            let dirty = self.is_dirty();
            let label = if dirty {
                "設定を適用（セッションを作り直す） ●未適用"
            } else {
                "設定を適用（セッションを作り直す）"
            };
            if ui
                .add_enabled(self.slots[0].scenario.is_some(), egui::Button::new(label))
                .clicked()
            {
                if self.slots[0].rounds.is_empty() {
                    self.apply_form();
                } else {
                    self.confirming_apply = true;
                }
            }
            if self.ip.is_some()
                && !self.slots[0].rounds.is_empty()
                && ui.button("同じ設定で最初からやり直す").clicked()
            {
                self.restart();
            }
        }
        ui.add_space(4.0);
    }

    fn ui_ip_section(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new(RichText::new("IP の選択").strong())
            .default_open(true)
            .show(ui, |ui| {
                if self.slots[0].scenario.is_none() {
                    ui.label("グラフを読み込んでください。");
                    return;
                }
                let before = self.metric;
                egui::ComboBox::from_label("順位の指標")
                    .selected_text(match self.metric {
                        Metric::Degree => "次数（フォロワー数）",
                        Metric::PageRank => "PageRank",
                        Metric::Vc => "VC（CSV）",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.metric,
                            Metric::Degree,
                            "次数（フォロワー数）",
                        );
                        ui.selectable_value(&mut self.metric, Metric::PageRank, "PageRank");
                        ui.selectable_value(&mut self.metric, Metric::Vc, "VC（CSV）");
                    });
                if self.metric == Metric::Vc && ui.button("VC の CSV を選ぶ…").clicked() {
                    if let Some(p) = rfd::FileDialog::new()
                        .add_filter("CSV", &["csv"])
                        .pick_file()
                    {
                        match std::fs::read_to_string(&p) {
                            Ok(text) => {
                                let labels = self.slots[0]
                                    .scenario
                                    .as_ref()
                                    .map(|s| s.graph.labels.clone())
                                    .unwrap_or_default();
                                match sim_api::centrality::metric_from_csv(&text, &labels, "vc") {
                                    Ok(v) => self.vc_scores = Some(v),
                                    Err(e) => self.error = Some(e),
                                }
                            }
                            Err(e) => self.error = Some(e.to_string()),
                        }
                        self.refresh_ranking();
                    }
                }
                if before != self.metric {
                    self.refresh_ranking();
                }
                let top: Vec<u32> = self.ranking.iter().copied().take(10).collect();
                ui.horizontal_wrapped(|ui| {
                    for (r, n) in top.iter().enumerate() {
                        if ui
                            .selectable_label(self.ip == Some(*n), format!("{}. ノード {n}", r + 1))
                            .clicked()
                        {
                            self.choose_ip(*n);
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("ノード番号");
                    ui.add(egui::TextEdit::singleline(&mut self.manual_ip).desired_width(70.0));
                    if ui
                        .add_enabled(
                            self.manual_ip.trim().parse::<u32>().is_ok(),
                            egui::Button::new("選ぶ"),
                        )
                        .clicked()
                    {
                        if let Ok(n) = self.manual_ip.trim().parse::<u32>() {
                            self.choose_ip(n);
                        }
                    }
                });
                match self.ip {
                    Some(ip) => ui.label(format!("現在の IP: ノード {ip}")),
                    None => ui.label("IP が未選択です"),
                };
            });
    }

    fn ui_palette_section(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new(RichText::new("メッセージと発信").strong())
            .default_open(true)
            .show(ui, |ui| {
                let msgs = self.slots[0]
                    .scenario
                    .as_ref()
                    .map(|s| s.cfg.message_space.clone())
                    .unwrap_or_else(|| self.slots[0].form.message_space.clone());
                ui.horizontal_wrapped(|ui| {
                    for (i, m) in msgs.iter().enumerate() {
                        let tip = format!(
                            "内部 {} / 外部 {}（キー {}）",
                            m.internal_stimulus,
                            m.external_stimulus,
                            i + 1
                        );
                        if ui
                            .selectable_label(self.palette == i as i32, format!(" m{i} "))
                            .on_hover_text(tip)
                            .clicked()
                        {
                            self.palette = i as i32;
                        }
                    }
                });
                let can = self.can_send();
                if ui
                    .add_enabled(
                        can,
                        crate::theme::primary_button(
                            &self.tokens,
                            RichText::new("発信").size(16.0),
                        )
                        .min_size(egui::vec2(ui.available_width(), 32.0)),
                    )
                    .on_hover_text("Enter")
                    .clicked()
                {
                    self.send();
                }
                if self.playback.playing {
                    ui.label("再生中は発信できません（最後までスキップすると押せます）。");
                }
                if self.slots[0].rounds.len() as u32 >= self.max_rounds() {
                    ui.label("実行ラウンド数の上限に達しました。");
                }
            });
    }
}
