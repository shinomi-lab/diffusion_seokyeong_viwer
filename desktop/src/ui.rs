//! `eframe::App` の実装: 11.2 のレイアウト、キー操作、MC 結果の受け取り、デモ・ベンチ。

use std::time::Instant;

use eframe::egui;
use eframe::egui_wgpu;

use crate::app::{App, BenchState};
use crate::camera::Camera;
use crate::gpu;
use crate::panels::col;
use crate::playback::Playback;

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let now = Instant::now();
        let started = now;
        let dt = now.duration_since(self.last_frame).as_secs_f32();
        self.last_frame = now;

        if self.bench.is_some() {
            self.update_bench(ctx);
            return;
        }

        if let Some(rx) = &self.slots[0].mc_rx {
            if let Ok(r) = rx.try_recv() {
                self.slots[0].mc_result = Some(r);
                self.slots[0].mc_running = false;
                self.slots[0].mc_rx = None;
            }
        }
        self.poll_mp4();
        self.handle_screenshot(ctx);
        self.drive_record(ctx);
        self.handle_keys(ctx);
        let was_playing = self.playback.playing;
        self.playback.advance(dt.min(0.1));
        if was_playing || self.playback.playing || self.slots[0].mc_running {
            ctx.request_repaint();
        }
        self.drive_demo(ctx);

        // 11.2: ツールバー 48px / 操作パネル 280px（全高）/ 下段 280px / ノード詳細 340px / 中央
        egui::TopBottomPanel::top("toolbar")
            .exact_height(48.0)
            .show(ctx, |ui| self.ui_toolbar(ui));
        egui::SidePanel::left("control")
            .frame(crate::theme::app_frame(&self.tokens))
            .exact_width(280.0)
            .resizable(false)
            .show(ctx, |ui| self.ui_control(ui));
        egui::TopBottomPanel::bottom("bottom")
            .exact_height(280.0)
            .show(ctx, |ui| {
                let w = ui.available_width();
                ui.horizontal_top(|ui| {
                    ui.allocate_ui(
                        egui::vec2(w * 2.0 / 3.0 - 6.0, ui.available_height()),
                        |ui| self.ui_round_charts(ui),
                    );
                    ui.separator();
                    ui.allocate_ui(
                        egui::vec2(ui.available_width(), ui.available_height()),
                        |ui| {
                            ui.vertical(|ui| self.ui_tree(ui, ctx));
                        },
                    );
                });
            });
        egui::SidePanel::right("detail")
            .exact_width(340.0)
            .resizable(false)
            .show(ctx, |ui| {
                let w = ui.available_width();
                ui.set_max_width(w);
                self.ui_detail(ui)
            });
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.spacing_mut().item_spacing.y = 3.0;
            self.ui_network(ui)
        });
        // 1 フレームの UI 処理にかかった時間（待機中の間隔ではなく、実際の処理時間）
        self.frame_ms = self.frame_ms * 0.9 + started.elapsed().as_secs_f32() * 1000.0 * 0.1;
    }
}

impl App {
    /// キーボード: 1〜6 でメッセージ選択、Enter で発信、Space で再生／一時停止、←→ で 1 ウェーブ戻る／進む（11.3）。
    fn handle_keys(&mut self, ctx: &egui::Context) {
        if ctx.wants_keyboard_input() {
            return;
        }
        let n = self.n_messages();
        let keys = [
            egui::Key::Num1,
            egui::Key::Num2,
            egui::Key::Num3,
            egui::Key::Num4,
            egui::Key::Num5,
            egui::Key::Num6,
            egui::Key::Num7,
            egui::Key::Num8,
            egui::Key::Num9,
        ];
        let (pressed, enter, space, left, right) = ctx.input(|i| {
            (
                keys.iter().position(|&k| i.key_pressed(k)),
                i.key_pressed(egui::Key::Enter),
                i.key_pressed(egui::Key::Space),
                i.key_pressed(egui::Key::ArrowLeft),
                i.key_pressed(egui::Key::ArrowRight),
            )
        });
        if let Some(i) = pressed {
            if i < n {
                self.palette = i as i32;
            }
        }
        if enter {
            self.send();
        }
        if space {
            self.playback.toggle();
        }
        if left {
            self.playback.step_wave(-1);
        }
        if right {
            self.playback.step_wave(1);
        }
    }

    /// `--demo`: 数ラウンド発信して、画面を PNG に保存して終了する（目視確認・スモークテスト用）。
    fn drive_demo(&mut self, ctx: &egui::Context) {
        // 借用の都合で、状態を一度取り出して処理し、あとで戻す
        let Some(mut demo) = self.demo.take() else {
            return;
        };
        self.drive_demo_inner(ctx, &mut demo);
        self.demo = Some(demo);
    }

    fn drive_demo_inner(&mut self, ctx: &egui::Context, demo: &mut crate::app::DemoState) {
        ctx.request_repaint();
        // 別の用途（連番 PNG の記録など）が進んでいる間は、デモを止めて待つ
        if self.record.is_some() || self.shot.is_some() || self.mp4_rx.is_some() {
            return;
        }
        if self.slots[0].session.is_none() {
            demo.wait_frames += 1;
            if demo.wait_frames == 3 {
                if self.slots[0].scenario.is_none() {
                    let p = concat!(env!("CARGO_MANIFEST_DIR"), "/../test/100random.txt");
                    self.graph_opts.transposed = false;
                    self.load_graph_path(p);
                }
                if let Some(&top) = self.ranking.first() {
                    let ip = self.cli.ip.unwrap_or(top);
                    self.choose_ip(ip);
                }
            }
            return;
        }
        let seq: Vec<i32> = self
            .cli
            .demo_sequence
            .clone()
            .unwrap_or_else(|| "0351".into())
            .chars()
            .map(|c| c.to_digit(16).map(|d| d as i32).unwrap_or(-1))
            .collect();
        if self.playback.playing {
            return;
        }
        // 2 画面比較（F13）: B は機構を「既定」にして、A と同じ IP・同じ選択で並べる
        if self.cli.compare && !demo.compare_started {
            demo.compare_started = true;
            self.set_compare(true);
            if self.slots.len() > 1 {
                self.slots[1].form.mechanisms = sim_api::config::MechanismConfig::balanced();
                self.apply_b();
            }
            return;
        }
        if demo.step < seq.len() {
            self.palette = seq[demo.step];
            demo.step += 1;
            self.send();
            demo.step_ms.push(self.slots[0].last_step_ms);
            self.playback.skip_to_end();
            return;
        }
        if let (Some(ring), false) = (self.cli.expand_ring, demo.expanded) {
            demo.expanded = true;
            self.expand_ring(ring, None);
            return;
        }
        // 巻き戻し → 別のメッセージで分岐 → 元の枝へ戻る（F09）。元と一致することを確かめる
        if self.cli.branch && demo.branch_stage < 3 {
            match demo.branch_stage {
                0 => {
                    demo.branch_orig = self.slots[0]
                        .rounds
                        .iter()
                        .map(|r| serde_json::to_string(&r.summary).unwrap_or_default())
                        .collect();
                    demo.branch_tail = self.slots[0]
                        .session
                        .as_ref()
                        .and_then(|s| s.active_path().last().copied())
                        .unwrap_or(0);
                    let alt = if self.slots[0].choices.get(2) == Some(&sim_api::log::Choice::Msg(4))
                    {
                        2
                    } else {
                        4
                    };
                    self.rewind_all(2);
                    self.palette = alt;
                    self.send();
                    self.playback.skip_to_end();
                    demo.branch_stage = 1;
                }
                1 => {
                    let forked = self.slots[0]
                        .session
                        .as_ref()
                        .map(|s| s.tree_view().n_lanes)
                        .unwrap_or(0);
                    self.switch_all(demo.branch_tail);
                    let now: Vec<String> = self.slots[0]
                        .rounds
                        .iter()
                        .map(|r| serde_json::to_string(&r.summary).unwrap_or_default())
                        .collect();
                    println!(
                        "BRANCH lanes_after_fork={forked} back_rounds={} identical_to_original={}",
                        now.len(),
                        now == demo.branch_orig
                    );
                    demo.branch_stage = 3;
                }
                _ => {}
            }
            return;
        }
        // 連番 PNG（と MP4）の書き出し（F15）
        if let Some(dir) = self.cli.record.clone() {
            if !demo.record_started {
                demo.record_started = true;
                self.start_record(std::path::PathBuf::from(dir));
                return;
            }
            if let (Some(out), false) = (self.cli.mp4.clone(), demo.mp4_done) {
                demo.mp4_done = true;
                if let Some((d, n)) = self.last_record.clone() {
                    println!("RECORD frames={n} dir={}", d.display());
                }
                if self.ffmpeg_ok() {
                    self.start_mp4(ctx, std::path::PathBuf::from(out));
                } else {
                    println!("（ffmpeg が無いので MP4 は省略）");
                }
                return;
            } else if self.cli.mp4.is_none() && !demo.mp4_done {
                demo.mp4_done = true;
                if let Some((d, n)) = self.last_record.clone() {
                    println!("RECORD frames={n} dir={}", d.display());
                }
            }
        }
        // MC を別スレッドで回し、その間もフレームが進む（UI が固まらない）ことを数える
        if let Some(trials) = self.cli.mc {
            match &mut demo.mc_started {
                None => {
                    if let (Some(scn), Some(ip)) = (self.slots[0].scenario.clone(), self.ip) {
                        match crate::worker::spawn_mc(
                            ctx.clone(),
                            scn,
                            ip,
                            self.slots[0].choices.clone(),
                            trials,
                            1,
                        ) {
                            Ok(rx) => {
                                self.slots[0].mc_rx = Some(rx);
                                self.slots[0].mc_running = true;
                                demo.mc_started = Some((Instant::now(), 0));
                            }
                            Err(e) => self.error = Some(e),
                        }
                    }
                    return;
                }
                Some((t0, frames)) => {
                    if self.slots[0].mc_running {
                        *frames += 1;
                        return;
                    }
                    if *frames != u32::MAX {
                        let mc = self.slots[0].mc_result.as_ref();
                        println!(
                            "MC trials={trials} elapsed={:.0}ms frames_during={} total_xact_mean={:.3} total_share_mean={:.3}",
                            t0.elapsed().as_secs_f32() * 1000.0,
                            frames,
                            mc.map(|m| m.total_xact_mean).unwrap_or(f64::NAN),
                            mc.map(|m| m.total_share_mean).unwrap_or(f64::NAN)
                        );
                        *frames = u32::MAX;
                    }
                }
            }
        }
        if let (Some(path), false, Some(s)) = (
            self.cli.export_parquet.clone(),
            demo.exported,
            &self.slots[0].session,
        ) {
            demo.exported = true;
            match crate::export_parquet::write_events_parquet(std::path::Path::new(&path), s) {
                Ok(n) => println!("PARQUET rows={n} path={path}"),
                Err(e) => eprintln!("Parquet を書き出せません: {e}"),
            }
        }
        // クリックの当たり判定の確認: IP の描画位置をクリックすると、そのノードが選ばれる
        if self.cli.screenshot.is_some()
            && self.view.mode == crate::state::Mode::Expanded
            && self.view.selected.is_none()
            && self.canvas_rect.width() > 10.0
        {
            if let (Some(ip), Some(pos)) = (self.ip, &self.slots[0].positions) {
                let size = self.view_size;
                let s = self
                    .view
                    .camera
                    .to_screen(size, [pos.xy[2 * ip as usize], pos.xy[2 * ip as usize + 1]]);
                let p = eframe::egui::Pos2::new(
                    self.canvas_rect.min.x + s[0],
                    self.canvas_rect.min.y + s[1],
                );
                self.canvas_click(self.canvas_rect, size, p);
                println!(
                    "CLICK ip={ip} at=({:.1},{:.1}) selected={:?} {}",
                    p.x,
                    p.y,
                    self.view.selected,
                    if self.view.selected == Some(ip) {
                        "OK"
                    } else {
                        "FAIL"
                    }
                );
                self.view.selected = None;
            }
        }
        // 最後のラウンドで受信したノードを選んで、ノード詳細も写す
        if self.view.selected.is_none() {
            self.view.selected = self.slots[0]
                .rounds
                .last()
                .and_then(|r| r.events.node.first().copied());
        }
        demo.wait_frames += 1;
        if demo.wait_frames > 12 && !demo.screenshot_requested {
            demo.screenshot_requested = true;
            let n = self.slots[0]
                .scenario
                .as_ref()
                .map(|s| s.graph.n_nodes)
                .unwrap_or(0);
            println!(
                "DEMO nodes={n} step_ms={:?} {}",
                demo.step_ms
                    .iter()
                    .map(|v| format!("{v:.1}"))
                    .collect::<Vec<_>>(),
                self.status.clone().unwrap_or_default()
            );
            self.shot = Some(crate::panels::media::ShotTask::Final);
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        }
    }

    /// `--bench N`: ランダム配置の N ノードを描いてフレーム時間を測る（10.4）。再生中と同じく毎フレーム状態を更新する。
    fn update_bench(&mut self, ctx: &egui::Context) {
        let (n, started) = {
            let b = self.bench.as_ref().expect("bench");
            (b.n, b.frames > 0)
        };
        if !started || self.slots[0].scene.pos.len() != n * 2 {
            self.setup_bench_scene(n);
        }
        let b: &mut BenchState = self.bench.as_mut().expect("bench");
        let now = Instant::now();
        if b.frames > 0 {
            b.frame_ms
                .push(now.duration_since(b.last).as_secs_f32() * 1000.0);
        }
        b.last = now;
        b.frames += 1;
        let elapsed = b.start.elapsed().as_secs_f32();
        // 再生中を模して、毎フレーム kind を変える（GPU への転送を含めて測る）
        let tick = b.frames;
        let attrs: Vec<u32> = (0..n)
            .map(|i| ((i as u32 + tick) % 5) | ((i as u32 + tick).is_multiple_of(7) as u32) << 8)
            .collect();
        self.slots[0].scene.attrs = std::sync::Arc::new(attrs);
        self.slots[0].scene.attr_ver += 1;
        let size_f = self.view_size;
        let cam = self.view.camera;
        let bg = self.tokens.bg;
        let uniforms = gpu::Uniforms::new(&self.tokens, &cam, size_f, bg, self.srgb_target);
        let frame = gpu::Frame {
            uniforms,
            pos: self.slots[0].scene.pos.clone(),
            pos_ver: self.slots[0].scene.pos_ver,
            attrs: self.slots[0].scene.attrs.clone(),
            attr_ver: self.slots[0].scene.attr_ver,
            edges: self.slots[0].scene.edges.clone(),
            edge_ver: self.slots[0].scene.edge_ver,
        };
        let mut new_size = size_f;
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
                let (rect, _) = ui.allocate_exact_size(ui.available_size(), egui::Sense::hover());
                new_size = [rect.width(), rect.height()];
                ui.painter_at(rect).rect_filled(rect, 0.0, col(bg));
                ui.painter_at(rect)
                    .add(egui_wgpu::Callback::new_paint_callback(
                        rect,
                        crate::panels::network::NetCallback { frame, slot: 0 },
                    ));
            });
        if (new_size[0] - size_f[0]).abs() > 1.0 || (new_size[1] - size_f[1]).abs() > 1.0 {
            self.view_size = new_size;
            self.view.camera = Camera::fit(
                crate::camera::bounds_of(&self.slots[0].scene.pos, None),
                new_size,
                20.0,
            );
        }
        ctx.request_repaint();
        if elapsed > 6.0 && !self.bench.as_ref().is_some_and(|b| b.done) {
            let b = self.bench.as_mut().expect("bench");
            b.done = true;
            let b = &*b;
            let mut v: Vec<f32> = b.frame_ms.iter().skip(10).copied().collect(); // ウォームアップを除く
            v.sort_by(f32::total_cmp);
            let avg = v.iter().sum::<f32>() / v.len().max(1) as f32;
            let pct = |q: f32| {
                v.get(((v.len().saturating_sub(1)) as f32 * q) as usize)
                    .copied()
                    .unwrap_or(0.0)
            };
            println!(
                "BENCH nodes={} edges={} frames={} avg={:.2}ms ({:.0} fps) p50={:.2}ms p95={:.2}ms max={:.2}ms",
                n,
                self.slots[0].scene.edges.len() / 4,
                v.len(),
                avg,
                1000.0 / avg.max(0.001),
                pct(0.5),
                pct(0.95),
                v.last().copied().unwrap_or(0.0)
            );
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn setup_bench_scene(&mut self, n: usize) {
        // 円盤状のランダム配置（決定的）。辺は 3N 本（上限 EDGE_CAP）
        let mut s: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut rnd = move || {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            (s >> 11) as f32 / (1u64 << 53) as f32
        };
        let radius = (n as f32).sqrt() / 8.0;
        let mut xy = Vec::with_capacity(n * 2);
        for _ in 0..n {
            let r = rnd().sqrt() * radius;
            let a = rnd() * std::f32::consts::TAU;
            xy.push(r * a.cos());
            xy.push(r * a.sin());
        }
        let m = (3 * n).min(crate::app::EDGE_CAP);
        let mut edges = Vec::with_capacity(m * 4);
        for _ in 0..m {
            let (a, b) = (
                (rnd() * n as f32) as usize % n,
                (rnd() * n as f32) as usize % n,
            );
            edges.extend_from_slice(&[xy[2 * a], xy[2 * a + 1], xy[2 * b], xy[2 * b + 1]]);
        }
        self.slots[0].scene.pos = std::sync::Arc::new(xy);
        self.slots[0].scene.pos_ver += 1;
        self.slots[0].scene.edges = std::sync::Arc::new(edges);
        self.slots[0].scene.edge_ver += 1;
        self.slots[0].scene.attrs = std::sync::Arc::new(vec![0; n]);
        self.slots[0].scene.attr_ver += 1;
        self.view.camera = Camera::fit(
            crate::camera::bounds_of(&self.slots[0].scene.pos, None),
            self.view_size,
            20.0,
        );
        let _ = Playback::default();
    }
}
