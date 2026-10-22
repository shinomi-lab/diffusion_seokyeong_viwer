//! 画像・連番 PNG・動画の書き出し（F15）と、スクリーンショットの受け取り。
//! 画面全体のスクリーンショット（`ViewportCommand::Screenshot`）から、ネットワーク図や時系列の領域を切り出して PNG にする。

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};

use eframe::egui::{self, Color32, Rect};

use crate::app::App;
use crate::capture::{ffmpeg_available, frame_path, make_mp4, save_crop};
use crate::svg::{SvgChart, SvgSeries, line_chart_svg};
use crate::tokens::to_hex;

/// 画面のスクリーンショットを待っている用途（同時に 1 つだけ）。
pub enum ShotTask {
    /// `--screenshot`（デモの最終画面。保存して終了）。
    Final,
    /// 領域を切り出して保存する。
    Crop { rect: Rect, path: PathBuf },
    /// 連番 PNG の 1 コマ。
    Frame { rect: Rect, path: PathBuf },
}

/// 連番 PNG の記録（k = -1 のラウンド開始前から最後のウェーブまで、1 ウェーブ 1 コマ）。
pub struct RecordState {
    pub dir: PathBuf,
    pub next_k: i32,
    pub last: i32,
    pub frame_no: usize,
    /// 描画が追いつくのを待つ残りフレーム数。None は、まだ再生位置を動かしていない。
    pub settle: Option<u32>,
    pub saved: usize,
    pub restore_k: i32,
}

impl App {
    /// 現在のラウンドの再生を、ウェーブごとに連番 PNG にする。
    pub fn start_record(&mut self, dir: PathBuf) {
        if self.slots[0].rounds.is_empty() {
            self.error = Some("録画するラウンドがありません。先に発信してください".into());
            return;
        }
        if let Err(e) = std::fs::create_dir_all(&dir) {
            self.error = Some(format!("{}: {e}", dir.display()));
            return;
        }
        let last = self
            .slots
            .iter()
            .map(|s| {
                s.rounds
                    .last()
                    .map(|r| r.summary.n_waves as i32 - 1)
                    .unwrap_or(-1)
            })
            .max()
            .unwrap_or(-1);
        self.record = Some(RecordState {
            dir,
            next_k: -1,
            last,
            frame_no: 1,
            settle: None,
            saved: 0,
            restore_k: self.playback.k,
        });
        self.playback.playing = false;
    }

    /// 毎フレーム呼ぶ。コマごとに再生位置を決め、描画が落ち着いてからスクリーンショットを頼む。
    pub fn drive_record(&mut self, ctx: &egui::Context) {
        if self.shot.is_some() {
            return;
        }
        let Some(rs) = self.record.as_mut() else {
            return;
        };
        ctx.request_repaint();
        match rs.settle {
            None => {
                self.playback.k = rs.next_k;
                self.playback.playing = false;
                rs.settle = Some(3);
            }
            Some(0) => {
                let path = frame_path(&rs.dir, rs.frame_no);
                self.shot = Some(ShotTask::Frame {
                    rect: self.net_rect,
                    path,
                });
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            }
            Some(n) => rs.settle = Some(n - 1),
        }
    }

    /// スクリーンショットが届いたら、頼んだ用途に応じて保存する。
    pub fn handle_screenshot(&mut self, ctx: &egui::Context) {
        let Some(image) = ctx.input(|i| {
            i.events.iter().find_map(|e| {
                if let egui::Event::Screenshot { image, .. } = e {
                    Some(image.clone())
                } else {
                    None
                }
            })
        }) else {
            return;
        };
        let ppp = ctx.pixels_per_point();
        match self.shot.take() {
            Some(ShotTask::Final) => {
                if let Some(path) = &self.cli.screenshot {
                    let [w, h] = image.size;
                    match image::save_buffer(
                        path,
                        image.as_raw(),
                        w as u32,
                        h as u32,
                        image::ColorType::Rgba8,
                    ) {
                        Ok(()) => println!("スクリーンショットを保存しました: {path}（{w}×{h}）"),
                        Err(e) => eprintln!("スクリーンショットを保存できません: {e}"),
                    }
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Some(ShotTask::Crop { rect, path }) => match save_crop(&image, rect, ppp, &path) {
                Ok((w, h)) => {
                    self.status = Some(format!("{} に保存しました（{w}×{h}）", path.display()))
                }
                Err(e) => self.error = Some(e),
            },
            Some(ShotTask::Frame { rect, path }) => {
                if let Err(e) = save_crop(&image, rect, ppp, &path) {
                    self.error = Some(e);
                    self.record = None;
                    return;
                }
                if let Some(rs) = self.record.as_mut() {
                    rs.saved += 1;
                    rs.frame_no += 1;
                    rs.next_k += 1;
                    rs.settle = None;
                    if rs.next_k > rs.last {
                        let (dir, n, k) = (rs.dir.clone(), rs.saved, rs.restore_k);
                        self.record = None;
                        self.playback.k = k.min(self.playback.last);
                        self.last_record = Some((dir.clone(), n));
                        self.status = Some(format!(
                            "連番 PNG を {n} 枚、{} に書き出しました",
                            dir.display()
                        ));
                    }
                }
            }
            None => {}
        }
    }

    /// 領域を PNG で保存してもらう（スクリーンショットを頼む）。
    pub fn request_crop(&mut self, ctx: &egui::Context, rect: Rect, path: PathBuf) {
        self.shot = Some(ShotTask::Crop { rect, path });
        ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
    }

    /// 最後に書き出した連番 PNG を MP4 にする（別スレッド。ffmpeg が PATH にあるとき）。
    pub fn start_mp4(&mut self, ctx: &egui::Context, out: PathBuf) {
        let Some((dir, _)) = self.last_record.clone() else {
            return;
        };
        let (tx, rx): (_, Receiver<Result<PathBuf, String>>) = channel();
        let fps = self.playback.speed.round().max(1.0) as u32;
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let r = make_mp4(&dir, &out, fps).map(|_| out);
            let _ = tx.send(r);
            ctx.request_repaint();
        });
        self.mp4_rx = Some(rx);
        self.status = Some("MP4 を作成中…".into());
    }

    pub fn poll_mp4(&mut self) {
        if let Some(rx) = &self.mp4_rx {
            if let Ok(r) = rx.try_recv() {
                self.mp4_rx = None;
                match r {
                    Ok(p) => self.status = Some(format!("MP4 を保存しました: {}", p.display())),
                    Err(e) => self.error = Some(format!("MP4 を作れません: {e}")),
                }
            }
        }
    }

    pub fn ffmpeg_ok(&mut self) -> bool {
        *self.ffmpeg.get_or_insert_with(ffmpeg_available)
    }

    // ---- 時系列の SVG ----

    fn series_of(
        name: &str,
        color: &str,
        dash: Option<[f32; 2]>,
        hidden: bool,
        values: Vec<f64>,
    ) -> SvgSeries {
        SvgSeries {
            label: name.into(),
            color: color.into(),
            dash,
            hidden,
            values,
        }
    }

    pub fn counts_svg(&self) -> String {
        let t = &self.tokens;
        let a = &self.slots[0].rounds;
        let (c_int, c_new, c_ext, c_both) = (
            to_hex(t.series_internal),
            to_hex(t.series_new_xact),
            to_hex(t.series_external),
            to_hex(t.series_both),
        );
        let mut series = vec![
            Self::series_of(
                "新規シェア",
                &c_int,
                None,
                false,
                a.iter().map(|r| r.summary.n_share as f64).collect(),
            ),
            Self::series_of(
                "新規外部行動",
                &c_new,
                None,
                false,
                a.iter().map(|r| r.summary.new_xact as f64).collect(),
            ),
            Self::series_of(
                "累積外部行動",
                &c_ext,
                None,
                false,
                a.iter().map(|r| r.summary.cum_xact as f64).collect(),
            ),
        ];
        if self.compare {
            let b = &self.slots[1].rounds;
            let pad = |f: &dyn Fn(&crate::state::RoundData) -> f64| -> Vec<f64> {
                (0..a.len())
                    .map(|i| b.get(i).map(f).unwrap_or(f64::NAN))
                    .collect()
            };
            series.push(Self::series_of(
                "B 新規シェア",
                &c_int,
                Some([6.0, 4.0]),
                false,
                pad(&|r| r.summary.n_share as f64),
            ));
            series.push(Self::series_of(
                "B 新規外部行動",
                &c_new,
                Some([6.0, 4.0]),
                false,
                pad(&|r| r.summary.new_xact as f64),
            ));
            series.push(Self::series_of(
                "B 累積外部行動",
                &c_ext,
                Some([6.0, 4.0]),
                false,
                pad(&|r| r.summary.cum_xact as f64),
            ));
        }
        let mut bands = Vec::new();
        if let Some(mc) = &self.slots[0].mc_result {
            if mc.rounds as usize == a.len() {
                let f = |v: &[f32]| v.iter().map(|&x| x as f64).collect::<Vec<f64>>();
                series.push(Self::series_of(
                    "MC 累積外部行動（平均）",
                    &c_both,
                    None,
                    false,
                    f(&mc.cum_xact.mean),
                ));
                series.push(Self::series_of(
                    "MC p10",
                    &c_both,
                    None,
                    true,
                    f(&mc.cum_xact.p10),
                ));
                series.push(Self::series_of(
                    "MC p90",
                    &c_both,
                    None,
                    true,
                    f(&mc.cum_xact.p90),
                ));
                let n = series.len();
                bands.push((n - 1, n - 2, c_both.clone()));
            }
        }
        SvgChart {
            title: "ラウンド別の人数".into(),
            x_label: "ラウンド".into(),
            x: a.iter().map(|r| r.summary.round as f64).collect(),
            series,
            bands,
        }
        .into_svg()
    }

    pub fn util_svg(&self) -> String {
        let t = &self.tokens;
        let a = &self.slots[0].rounds;
        let (c_int, c_ext) = (to_hex(t.series_internal), to_hex(t.series_external));
        let col = |f: &dyn Fn(&sim_api::log::UtilityStats) -> f32| -> Vec<f64> {
            a.iter().map(|r| f(&r.summary.recv) as f64).collect()
        };
        let mut series = Vec::new();
        let mut bands = Vec::new();
        for (name, c, mean, lo, hi) in [
            (
                "内部",
                &c_int,
                col(&|s| s.mean_int),
                col(&|s| s.q_int[0]),
                col(&|s| s.q_int[2]),
            ),
            (
                "外部",
                &c_ext,
                col(&|s| s.mean_ext),
                col(&|s| s.q_ext[0]),
                col(&|s| s.q_ext[2]),
            ),
        ] {
            series.push(Self::series_of(
                &format!("{name} 平均"),
                c,
                None,
                false,
                mean,
            ));
            series.push(Self::series_of(&format!("{name} p10"), c, None, true, lo));
            series.push(Self::series_of(&format!("{name} p90"), c, None, true, hi));
            let n = series.len();
            bands.push((n - 1, n - 2, c.clone()));
        }
        if self.compare {
            let b = &self.slots[1].rounds;
            let pad = |f: &dyn Fn(&sim_api::log::UtilityStats) -> f32| -> Vec<f64> {
                (0..a.len())
                    .map(|i| {
                        b.get(i)
                            .map(|r| f(&r.summary.recv) as f64)
                            .unwrap_or(f64::NAN)
                    })
                    .collect()
            };
            series.push(Self::series_of(
                "B 内部 平均",
                &c_int,
                Some([6.0, 4.0]),
                false,
                pad(&|s| s.mean_int),
            ));
            series.push(Self::series_of(
                "B 外部 平均",
                &c_ext,
                Some([6.0, 4.0]),
                false,
                pad(&|s| s.mean_ext),
            ));
        }
        SvgChart {
            title: "効用（受信済みノードの平均と 10〜90% の帯）".into(),
            x_label: "ラウンド".into(),
            x: a.iter().map(|r| r.summary.round as f64).collect(),
            series,
            bands,
        }
        .into_svg()
    }
}

impl SvgChart {
    fn into_svg(self) -> String {
        line_chart_svg(&self)
    }
}

#[allow(dead_code)]
fn _unused(_: Color32) {}
