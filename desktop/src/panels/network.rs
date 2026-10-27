//! ネットワーク表示（F06・F07・F11）。ノード・辺は wgpu、リング・扇形・強調・縮小地図は egui の 2D で重ねる。
//! 座標変換は `camera.rs` の 1 か所だけ（GPU 側も同じ変換のユニフォームを使う）。

use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Shape, Stroke, vec2};
use eframe::egui_wgpu;
use sim_api::log::IntMode;
use sim_api::overview::N_SECTORS;

use crate::app::App;
use crate::camera::Camera;
use crate::gpu;
use crate::layout::unreachable_rect;
use crate::panels::{col, col_a};
use crate::playback::SPEEDS;
use crate::state::Mode;

const MINI: f32 = 150.0;
const CONTROLS_H: f32 = 62.0;

pub struct NetCallback {
    pub frame: gpu::Frame,
    /// 比較（F13）の A = 0 / B = 1。スロットごとの描画器（バッファ）を使う。
    pub slot: usize,
}

impl egui_wgpu::CallbackTrait for NetCallback {
    fn prepare(
        &self,
        device: &eframe::wgpu::Device,
        queue: &eframe::wgpu::Queue,
        _screen: &egui_wgpu::ScreenDescriptor,
        _enc: &mut eframe::wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<eframe::wgpu::CommandBuffer> {
        if let Some(r) = resources
            .get_mut::<gpu::Renderers>()
            .and_then(|r| r.0.get_mut(self.slot))
        {
            r.prepare(device, queue, &self.frame);
        }
        Vec::new()
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        pass: &mut eframe::wgpu::RenderPass<'static>,
        resources: &egui_wgpu::CallbackResources,
    ) {
        if let Some(r) = resources
            .get::<gpu::Renderers>()
            .and_then(|r| r.0.get(self.slot))
        {
            r.paint(pass);
        }
    }
}

fn to_pos(rect: Rect, s: [f32; 2]) -> Pos2 {
    Pos2::new(rect.min.x + s[0], rect.min.y + s[1])
}

impl App {
    pub fn ui_network(&mut self, ui: &mut egui::Ui) {
        let avail = ui.available_size();
        let canvas = vec2(avail.x, (avail.y - CONTROLS_H).max(100.0));
        let (rect, resp) = ui.allocate_exact_size(canvas, Sense::click_and_drag());
        // 比較（F13）: 左右に A・B を並べる。カメラ・選択・再生位置は共有し、状態（ノードの色）だけがスロットごとに違う
        let n = self.slots.len();
        let gap = 4.0;
        let w = (rect.width() - gap * (n as f32 - 1.0)) / n as f32;
        let rects: Vec<Rect> = (0..n)
            .map(|i| {
                Rect::from_min_size(
                    rect.min + vec2(i as f32 * (w + gap), 0.0),
                    vec2(w, rect.height()),
                )
            })
            .collect();
        let size = [w, rect.height()];
        self.view_size = size;
        self.canvas_rect = rects[0];
        self.net_rect = rect;
        self.cur = 0;
        let bg = self.tokens.bg;
        if self.slots[0].positions.is_none() {
            ui.painter_at(rect).rect_filled(rect, 0.0, col(bg));
            ui.painter_at(rect).text(
                rect.center(),
                Align2::CENTER_CENTER,
                "グラフを読み込み、IP を選ぶと、ここに拡散が表示されます。",
                FontId::proportional(14.0),
                Color32::GRAY,
            );
            self.ui_playbar(ui);
            return;
        }
        if self.view.refit {
            self.fit_camera(size);
        }
        for i in 0..n {
            self.update_display(i);
        }
        // ポインタがある側（A または B）に対して入力を処理する
        let pointer = resp.interact_pointer_pos().or(resp.hover_pos());
        let active = pointer
            .and_then(|p| rects.iter().position(|r| r.contains(p)))
            .unwrap_or(0);
        self.cur = active;
        self.handle_canvas_input(ui, &resp, rects[active], size);
        let cam = self.view.camera; // パン・ズームを反映した最新のカメラ

        for i in 0..n {
            self.cur = i;
            let rect_i = rects[i];
            let painter = ui.painter_at(rect_i);
            painter.rect_filled(rect_i, 0.0, col(bg));
            let max_dist = self.slots[self.cur]
                .positions
                .as_ref()
                .map(|p| p.max_dist)
                .unwrap_or(1);
            match self.view.mode {
                Mode::Overview => {
                    self.ensure_sectors();
                    self.paint_sectors(&painter, rect_i, &cam, size);
                    self.paint_rings(&painter, rect_i, &cam, size, max_dist);
                    self.paint_unreachable(&painter, rect_i, &cam, size, max_dist);
                }
                Mode::Expanded => {
                    self.paint_rings(&painter, rect_i, &cam, size, max_dist);
                    self.refresh_view_edges(ui.ctx(), i, &cam, size);
                    if !self.slots[self.cur].scene.pos.is_empty() {
                        let uniforms =
                            gpu::Uniforms::new(&self.tokens, &cam, size, bg, self.srgb_target)
                                .with_edge_count(self.slots[self.cur].scene.edges.len() / 4)
                                .with_radius(self.slots[self.cur].scene.radius_px);
                        let sc = &self.slots[self.cur].scene;
                        let frame = gpu::Frame {
                            uniforms,
                            pos: sc.pos.clone(),
                            pos_ver: sc.pos_ver,
                            attrs: sc.attrs.clone(),
                            attr_ver: sc.attr_ver,
                            edges: sc.edges.clone(),
                            edge_ver: sc.edge_ver,
                        };
                        painter.add(egui_wgpu::Callback::new_paint_callback(
                            rect_i,
                            NetCallback { frame, slot: i },
                        ));
                    }
                    self.paint_overlay(&painter, rect_i, &cam, size);
                }
            }
            if i == active {
                self.paint_rubber(&painter, rect_i);
            }
            self.paint_minimap(&painter, rect_i, &cam, size, max_dist);
            if i == 0 {
                self.paint_overview_ui(ui, rect_i, &resp, &cam, size);
            }
            if n > 1 {
                // 比較: A・B の札と、B で起きたエラー
                let label = if i == 0 { "A" } else { "B" };
                let r = Rect::from_min_size(
                    Pos2::new(rect_i.max.x - 34.0, rect_i.min.y + 6.0),
                    vec2(28.0, 20.0),
                );
                painter.rect_filled(r, 4.0, col(self.tokens.series_internal));
                painter.text(
                    r.center(),
                    Align2::CENTER_CENTER,
                    label,
                    FontId::proportional(14.0),
                    Color32::WHITE,
                );
                if let Some(e) = self.slots[i].error.clone() {
                    painter.text(
                        rect_i.center_top() + vec2(0.0, 30.0),
                        Align2::CENTER_TOP,
                        e,
                        FontId::proportional(12.0),
                        Color32::from_rgb(200, 60, 60),
                    );
                }
            }
        }
        self.cur = 0;

        // 左下: 処理時間と、直近の操作の結果（ツールバーが混むのでここに出す）
        let status = self.status.clone().unwrap_or_default();
        ui.painter_at(rect).text(
            rect.left_bottom() + vec2(8.0, -6.0),
            Align2::LEFT_BOTTOM,
            format!("UI {:.1} ms/フレーム　{status}", self.frame_ms),
            FontId::proportional(10.0),
            Color32::GRAY,
        );
        self.ui_playbar(ui);
    }

    fn handle_canvas_input(
        &mut self,
        ui: &egui::Ui,
        resp: &egui::Response,
        rect: Rect,
        size: [f32; 2],
    ) {
        let shift = ui.input(|i| i.modifiers.shift);
        if resp.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                if let Some(p) = resp.hover_pos() {
                    let s = [p.x - rect.min.x, p.y - rect.min.y];
                    self.view.camera = self.view.camera.zoom_at(size, s, (scroll * 0.0015).exp());
                }
            }
        }
        if resp.dragged_by(egui::PointerButton::Primary) {
            if shift && self.view.mode == Mode::Expanded {
                let origin = ui.input(|i| i.pointer.press_origin());
                if let (Some(o), Some(c)) = (origin, resp.interact_pointer_pos()) {
                    self.rubber = Some((o, c));
                }
            } else {
                let d = resp.drag_delta();
                self.view.camera = self.view.camera.panned([d.x, d.y]);
            }
        }
        if resp.drag_stopped() {
            if let Some((a, b)) = self.rubber.take() {
                let r = Rect::from_two_pos(a, b);
                self.select_in_rect(rect, size, r);
            }
        }
        if resp.clicked() {
            if let Some(p) = resp.interact_pointer_pos() {
                if !self.minimap_click(rect, size, p) {
                    self.canvas_click(rect, size, p);
                }
            }
        }
    }

    fn select_in_rect(&mut self, rect: Rect, size: [f32; 2], r: Rect) {
        let Some(pos) = &self.slots[self.cur].positions else {
            return;
        };
        let cam = self.view.camera;
        let nodes: Vec<u32> = self
            .draw_node_ids()
            .into_iter()
            .filter(|&i| {
                let s = cam.to_screen(size, [pos.xy[2 * i as usize], pos.xy[2 * i as usize + 1]]);
                r.contains(to_pos(rect, s))
            })
            .collect();
        self.view.group = Some(nodes);
        self.view.tab = crate::state::Tab::Group;
        self.view.group_kind = crate::state::GroupKind::Custom;
    }

    /// 描画中のノード番号。
    pub fn draw_node_ids(&self) -> Vec<u32> {
        match &self.slots[self.cur].scene.nodes {
            Some(v) => v.to_vec(),
            None => (0..self.slots[self.cur]
                .positions
                .as_ref()
                .map(|p| p.xy.len() / 2)
                .unwrap_or(0) as u32)
                .collect(),
        }
    }

    pub fn canvas_click(&mut self, rect: Rect, size: [f32; 2], p: Pos2) {
        let cam = self.view.camera;
        let s = [p.x - rect.min.x, p.y - rect.min.y];
        match self.view.mode {
            Mode::Expanded => {
                let Some(pos) = &self.slots[self.cur].positions else {
                    return;
                };
                let max_px = (self.slots[self.cur]
                    .scene
                    .radius_px
                    .max(self.tokens.radius_px.min(1.0))
                    * 1.5)
                    .max(4.0);
                let mut best: Option<(f32, u32)> = None;
                for i in self.draw_node_ids() {
                    let q =
                        cam.to_screen(size, [pos.xy[2 * i as usize], pos.xy[2 * i as usize + 1]]);
                    let d = ((q[0] - s[0]).powi(2) + (q[1] - s[1]).powi(2)).sqrt();
                    if d <= max_px && best.is_none_or(|(bd, _)| d < bd) {
                        best = Some((d, i));
                    }
                }
                self.view.selected = best.map(|(_, i)| i);
                if best.is_some() {
                    self.view.tab = crate::state::Tab::Node;
                }
            }
            Mode::Overview => {
                let max_dist = self.slots[self.cur]
                    .positions
                    .as_ref()
                    .map(|p| p.max_dist)
                    .unwrap_or(1);
                let w = cam.to_world(size, s);
                let (ux, uy, uw, uh) = unreachable_rect(max_dist);
                if self.slots[self.cur]
                    .rings
                    .as_ref()
                    .is_some_and(|r| r.unreachable > 0)
                    && w[0] >= ux
                    && w[0] <= ux + uw
                    && w[1] >= uy
                    && w[1] <= uy + uh
                {
                    self.expand_unreachable();
                    return;
                }
                if let Some((ring, sector)) = cell_at(&cam, size, s, max_dist + 1) {
                    self.expand_ring(ring, Some(sector));
                }
            }
        }
    }

    fn minimap_rect(rect: Rect) -> Rect {
        Rect::from_min_size(
            Pos2::new(rect.max.x - MINI - 8.0, rect.max.y - MINI - 8.0),
            vec2(MINI, MINI),
        )
    }

    fn minimap_click(&mut self, rect: Rect, _size: [f32; 2], p: Pos2) -> bool {
        let m = Self::minimap_rect(rect);
        if !m.contains(p) {
            return false;
        }
        let max_dist = self.slots[self.cur]
            .positions
            .as_ref()
            .map(|p| p.max_dist)
            .unwrap_or(1) as f32;
        let sc = (MINI / 2.0 - 6.0) / (max_dist + 1.2);
        self.view.camera.cx = (p.x - m.center().x) / sc;
        self.view.camera.cy = -(p.y - m.center().y) / sc;
        true
    }

    fn ensure_sectors(&mut self) {
        let Some(d) = &self.slots[self.cur].display else {
            return;
        };
        if self.slots[self.cur].sectors_key == Some(d.key) {
            return;
        }
        if let (Some(rings), Some(layout)) =
            (&self.slots[self.cur].rings, &self.slots[self.cur].layout)
        {
            let kinds = self.node_kinds(self.cur);
            self.slots[self.cur].sectors =
                sim_api::overview::aggregate(rings, layout, &kinds).cells;
            self.slots[self.cur].sectors_key = self.slots[self.cur].display.as_ref().map(|d| d.key);
        }
    }

    fn paint_rings(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        cam: &Camera,
        size: [f32; 2],
        max_dist: u32,
    ) {
        let center = to_pos(rect, cam.to_screen(size, [0.0, 0.0]));
        let ring = self.tokens.ring_stroke;
        let stroke = Stroke::new(1.0, col(ring));
        let label = col(self.tokens.ring_label);
        let step = ((16.0 / cam.zoom).ceil() as u32).max(1);
        let mut d = step;
        while d <= max_dist {
            let r = d as f32 * cam.zoom;
            if (4.0..20000.0).contains(&r) {
                let pts: Vec<Pos2> = (0..=96)
                    .map(|k| {
                        let a = k as f32 / 96.0 * std::f32::consts::TAU;
                        center + vec2(a.cos(), a.sin()) * r
                    })
                    .collect();
                painter.extend(Shape::dashed_line(
                    &pts,
                    stroke,
                    self.tokens.ring_dash[0],
                    self.tokens.ring_dash[1],
                ));
                painter.text(
                    center + vec2(4.0, -r - 3.0),
                    Align2::LEFT_BOTTOM,
                    format!("{d}ホップ"),
                    FontId::proportional(11.0),
                    label,
                );
            }
            d += step;
        }
    }

    fn paint_unreachable(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        cam: &Camera,
        size: [f32; 2],
        max_dist: u32,
    ) {
        let Some(rings) = &self.slots[self.cur].rings else {
            return;
        };
        if rings.unreachable == 0 {
            return;
        }
        let (x, y, w, h) = unreachable_rect(max_dist);
        let a = to_pos(rect, cam.to_screen(size, [x, y + h]));
        let b = to_pos(rect, cam.to_screen(size, [x + w, y]));
        let r = Rect::from_two_pos(a, b);
        painter.rect_stroke(
            r,
            0.0,
            Stroke::new(1.0, col(self.tokens.ring_stroke)),
            egui::StrokeKind::Inside,
        );
        painter.text(
            r.min + vec2(4.0, 3.0),
            Align2::LEFT_TOP,
            format!("到達不能 {}（クリックで展開）", rings.unreachable),
            FontId::proportional(11.0),
            Color32::GRAY,
        );
    }

    /// 全体図: 扇形セル。3 本の細い帯（内側から 受信・内部・外部）の濃さで割合を示す。
    fn paint_sectors(&self, painter: &egui::Painter, rect: Rect, cam: &Camera, size: [f32; 2]) {
        let Some(rings) = &self.slots[self.cur].rings else {
            return;
        };
        let n_rings = rings.ring_sizes.len();
        let center = cam.to_screen(size, [0.0, 0.0]);
        let step = std::f32::consts::TAU / N_SECTORS as f32;
        let stroke = Stroke::new(0.5, Color32::from_white_alpha(30));
        let colors = [
            col(self.tokens.kinds[1].fill.unwrap_or([0.6; 3])),
            col(self.tokens.kinds[2].fill.unwrap_or([0.0; 3])),
            col(self.tokens.kinds[3].fill.unwrap_or([0.0; 3])),
        ];
        let pt = |r: f32, a: f32| to_pos(rect, [center[0] + r * a.cos(), center[1] - r * a.sin()]);
        // IP（中心）
        painter.circle_filled(
            to_pos(rect, center),
            0.5 * cam.zoom,
            col(self.tokens.kinds[5].fill.unwrap_or([0.0; 3])),
        );
        for d in 1..n_rings {
            let r0 = (d as f32 - 0.5) * cam.zoom;
            let r1 = (d as f32 + 0.5) * cam.zoom;
            let band = (r1 - r0) / 3.0;
            for sec in 0..N_SECTORS as usize {
                let c = self.slots[self.cur]
                    .sectors
                    .get(d * N_SECTORS as usize + sec)
                    .copied()
                    .unwrap_or_default();
                if c.n == 0 {
                    continue;
                }
                let (a0, a1) = (sec as f32 * step, (sec as f32 + 1.0) * step);
                // 5 度刻みなので、4 点の凸四角形で近似できる
                let quad = |ra: f32, rb: f32| vec![pt(ra, a0), pt(rb, a0), pt(rb, a1), pt(ra, a1)];
                painter.add(Shape::convex_polygon(
                    quad(r0, r1),
                    Color32::from_white_alpha(12),
                    stroke,
                ));
                let n = c.n as f32;
                let ratios = [
                    c.n_recv as f32 / n,
                    (c.n_int + c.n_both) as f32 / n,
                    (c.n_ext + c.n_both) as f32 / n,
                ];
                for b in 0..3 {
                    if ratios[b] <= 0.0 {
                        continue;
                    }
                    let alpha = 0.2 + 0.8 * ratios[b];
                    let [r, g, bl, _] = colors[b].to_array();
                    painter.add(Shape::convex_polygon(
                        quad(r0 + band * b as f32, r0 + band * (b as f32 + 1.0)),
                        Color32::from_rgba_unmultiplied(r, g, bl, (alpha * 255.0) as u8),
                        Stroke::NONE,
                    ));
                }
            }
        }
    }

    /// 現在のウェーブの送信元→受信先の辺、選択中のノードの輪。
    fn paint_overlay(&self, painter: &egui::Painter, rect: Rect, cam: &Camera, size: [f32; 2]) {
        let (Some(pos), Some(d)) = (
            &self.slots[self.cur].positions,
            &self.slots[self.cur].display,
        ) else {
            return;
        };
        let k = d.key.1;
        if let Some(r) = self.slots[self.cur].rounds.last() {
            if k >= 0 {
                let visible = |i: u32| {
                    self.slots[self.cur]
                        .scene
                        .nodes
                        .as_ref()
                        .is_none_or(|v| v.binary_search(&i).is_ok() || v.contains(&i))
                };
                let stroke = Stroke::new(1.6, col(self.tokens.wave));
                let ev = &r.events;
                for i in 0..ev.len() {
                    // 推薦で受信した人（from = FROM_REC）には送り手がいないので、矢印を描かない
                    if ev.wave[i] as i32 != k
                        || ev.from[i] == sim_api::engine::FROM_REC
                        || !visible(ev.node[i])
                        || !visible(ev.from[i])
                    {
                        continue;
                    }
                    let a = to_pos(
                        rect,
                        cam.to_screen(
                            size,
                            [
                                pos.xy[2 * ev.from[i] as usize],
                                pos.xy[2 * ev.from[i] as usize + 1],
                            ],
                        ),
                    );
                    let b = to_pos(
                        rect,
                        cam.to_screen(
                            size,
                            [
                                pos.xy[2 * ev.node[i] as usize],
                                pos.xy[2 * ev.node[i] as usize + 1],
                            ],
                        ),
                    );
                    painter.line_segment([a, b], stroke);
                }
            }
        }
        if let Some(sel) = self.view.selected {
            let c = to_pos(
                rect,
                cam.to_screen(
                    size,
                    [pos.xy[2 * sel as usize], pos.xy[2 * sel as usize + 1]],
                ),
            );
            let scale = if Some(sel) == self.ip { 2.2 } else { 1.0 };
            painter.circle_stroke(
                c,
                self.tokens.radius_px * scale * 1.9,
                Stroke::new(2.0, col(self.tokens.selected)),
            );
        }
    }

    fn paint_rubber(&self, painter: &egui::Painter, _rect: Rect) {
        if let Some((a, b)) = self.rubber {
            let r = Rect::from_two_pos(a, b);
            let c = col(self.tokens.selected);
            painter.rect_filled(r, 0.0, col_a(self.tokens.selected, 0.1));
            painter.rect_stroke(r, 0.0, Stroke::new(1.0, c), egui::StrokeKind::Inside);
        }
    }

    fn paint_minimap(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        cam: &Camera,
        size: [f32; 2],
        max_dist: u32,
    ) {
        let (Some(pos), Some(d)) = (
            &self.slots[self.cur].positions,
            &self.slots[self.cur].display,
        ) else {
            return;
        };
        let m = Self::minimap_rect(rect);
        painter.rect_filled(m, 4.0, crate::theme::c32(self.tokens.chrome.card));
        painter.rect_stroke(
            m,
            4.0,
            Stroke::new(1.0, col(self.tokens.ring_stroke)),
            egui::StrokeKind::Inside,
        );
        let ext = max_dist as f32 + 1.2;
        let sc = (MINI / 2.0 - 6.0) / ext;
        painter.circle_stroke(
            m.center(),
            max_dist as f32 * sc,
            Stroke::new(1.0, col(self.tokens.ring_stroke)),
        );
        let n = pos.xy.len() / 2;
        let step = (n / 3000).max(1);
        for i in (0..n).step_by(step) {
            let kd = d.kinds[i] as usize;
            if kd == 0 {
                continue;
            }
            let (x, y) = (pos.xy[2 * i], pos.xy[2 * i + 1]);
            if x.hypot(y) > ext {
                continue;
            }
            let c = self.tokens.kinds[kd].fill.unwrap_or(self.tokens.split[1]);
            painter.rect_filled(
                Rect::from_center_size(m.center() + vec2(x * sc, -y * sc), vec2(1.5, 1.5)),
                0.0,
                col(c),
            );
        }
        let w0 = cam.to_world(size, [0.0, 0.0]);
        let w1 = cam.to_world(size, size);
        let view = Rect::from_two_pos(
            m.center() + vec2(w0[0] * sc, -w0[1] * sc),
            m.center() + vec2(w1[0] * sc, -w1[1] * sc),
        );
        painter.rect_stroke(
            view.intersect(m),
            0.0,
            Stroke::new(1.5, col(self.tokens.selected)),
            egui::StrokeKind::Inside,
        );
    }

    /// 全体図: リング表（展開ボタン）とセルのツールチップ。
    fn paint_overview_ui(
        &mut self,
        ui: &mut egui::Ui,
        rect: Rect,
        resp: &egui::Response,
        cam: &Camera,
        size: [f32; 2],
    ) {
        if self.view.mode == Mode::Expanded {
            if self.view.from_overview {
                let mut back = false;
                egui::Area::new(egui::Id::new("expand_bar"))
                    .fixed_pos(rect.min + vec2(8.0, 8.0))
                    .show(ui.ctx(), |ui| {
                        egui::Frame::popup(ui.style()).show(ui, |ui| {
                            ui.horizontal(|ui| {
                                if ui.button("← 全体図に戻る").clicked() {
                                    back = true;
                                }
                                ui.label(format!(
                                    "{}（{} ノード）",
                                    self.view.expanded_label,
                                    self.view.expanded.as_ref().map(|v| v.len()).unwrap_or(0)
                                ));
                            });
                        });
                    });
                if back {
                    self.back_to_overview();
                }
            }
            // 到達不能のノードは通常は描かない。件数を示し、クリックで表示する
            let hidden = self.slots[self.cur]
                .rings
                .as_ref()
                .map(|r| r.unreachable)
                .unwrap_or(0);
            if !self.view.from_overview && self.view.expanded.is_none() && hidden > 0 {
                let mut show = false;
                egui::Area::new(egui::Id::new("unreachable_note"))
                    .fixed_pos(rect.min + vec2(8.0, 8.0))
                    .show(ui.ctx(), |ui| {
                        egui::Frame::popup(ui.style()).show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(format!(
                                    "IP から到達できないノード {hidden} 個は表示していません"
                                ));
                                if ui.button("表示する").clicked() {
                                    show = true;
                                }
                            });
                        });
                    });
                if show {
                    self.expand_unreachable();
                }
            }
            if self.slots[self.cur].scene.edges_truncated {
                ui.painter_at(rect).text(
                    rect.left_top() + vec2(8.0, 40.0),
                    Align2::LEFT_TOP,
                    format!("辺は {} 本まで表示しています", crate::app::EDGE_CAP),
                    FontId::proportional(11.0),
                    Color32::GRAY,
                );
            }
        } else {
            // リング表
            let totals = self.ring_totals();
            let mut expand: Option<u32> = None;
            egui::Area::new(egui::Id::new("ring_table"))
                .fixed_pos(rect.min + vec2(8.0, 8.0))
                .show(ui.ctx(), |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .max_height((rect.height() * 0.6).max(120.0))
                            .show(ui, |ui| {
                                egui::Grid::new("rings").striped(true).show(ui, |ui| {
                                    for h in ["リング", "数", "受信", "内部", "外部", "両方", ""]
                                    {
                                        ui.strong(h);
                                    }
                                    ui.end_row();
                                    for (r, t) in totals.iter().enumerate().skip(1) {
                                        ui.label(format!("{r}ホップ"));
                                        for v in [t.0, t.1, t.2, t.3, t.4] {
                                            ui.label(v.to_string());
                                        }
                                        if ui.small_button("展開").clicked() {
                                            expand = Some(r as u32);
                                        }
                                        ui.end_row();
                                    }
                                });
                            });
                    });
                });
            if let Some(r) = expand {
                self.expand_ring(r, None);
            }
            // セルのツールチップ
            if let (Some(p), true) = (resp.hover_pos(), resp.hovered()) {
                let max_dist = self.slots[self.cur]
                    .positions
                    .as_ref()
                    .map(|p| p.max_dist)
                    .unwrap_or(1);
                if let Some((ring, sec)) = cell_at(
                    cam,
                    size,
                    [p.x - rect.min.x, p.y - rect.min.y],
                    max_dist + 1,
                ) {
                    if let Some(c) = self.slots[self.cur]
                        .sectors
                        .get(ring as usize * N_SECTORS as usize + sec as usize)
                    {
                        if c.n > 0 {
                            egui::Tooltip::always_open(ui.ctx().clone(), ui.layer_id(), egui::Id::new("cell_tip"), egui::PopupAnchor::Pointer).show(|ui| {
                                ui.label(format!("{ring}ホップ・扇形 {}: {} ノード / 受信 {} / 内部 {} / 外部 {} / 両方 {}", sec + 1, c.n, c.n_recv, c.n_int, c.n_ext, c.n_both));
                            });
                        }
                    }
                }
            }
        }
        if let Some(n) = self.view.notice.clone() {
            ui.painter_at(rect).text(
                rect.center_top() + vec2(0.0, 12.0),
                Align2::CENTER_TOP,
                n,
                FontId::proportional(13.0),
                Color32::from_rgb(200, 90, 0),
            );
        }
    }

    /// リングごとの (数, 受信, 内部, 外部, 両方)。
    fn ring_totals(&self) -> Vec<(u32, u32, u32, u32, u32)> {
        let Some(rings) = &self.slots[self.cur].rings else {
            return Vec::new();
        };
        let mut out = vec![(0, 0, 0, 0, 0); rings.ring_sizes.len()];
        for (i, c) in self.slots[self.cur].sectors.iter().enumerate() {
            let ring = i / N_SECTORS as usize;
            if let Some(t) = out.get_mut(ring) {
                t.0 += c.n;
                t.1 += c.n_recv;
                t.2 += c.n_int;
                t.3 += c.n_ext;
                t.4 += c.n_both;
            }
        }
        out
    }

    /// 凡例と再生の操作。
    fn ui_playbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            for (i, st) in self.tokens.kinds.clone().iter().enumerate() {
                let (r, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
                let p = ui.painter();
                let c = r.center();
                let fill = st.fill.map(col);
                match (i, st.square) {
                    (4, _) => {
                        p.rect_filled(
                            Rect::from_min_max(r.min, Pos2::new(c.x, r.max.y)),
                            0.0,
                            col(self.tokens.split[0]),
                        );
                        p.rect_filled(
                            Rect::from_min_max(Pos2::new(c.x, r.min.y), r.max),
                            0.0,
                            col(self.tokens.split[1]),
                        );
                    }
                    (_, true) => {
                        p.rect_filled(r.shrink(1.0), 0.0, fill.unwrap_or(Color32::TRANSPARENT));
                    }
                    _ => {
                        if let Some(f) = fill {
                            p.circle_filled(c, 6.0, f);
                        }
                        if let Some(s) = st.stroke {
                            p.circle_stroke(
                                c,
                                6.0,
                                Stroke::new(if i == 5 { 2.0 } else { 1.5 }, col(s)),
                            );
                        }
                    }
                }
                ui.label(st.label.clone());
            }
            // 推薦（A か B のどちらかで on のときだけ）: 推薦で受信 = 黄色の塗り（青い縁 = 内部活性）、推薦由来 = 暗い黄色の塗り
            if self.rec_on(0) || (self.compare && self.rec_on(1)) {
                let t = &self.tokens;
                let (r, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
                ui.painter().circle_filled(r.center(), 6.0, col(t.rec));
                ui.label("推薦で受信");
                let (r, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
                ui.painter().circle_filled(r.center(), 4.5, col(t.rec));
                ui.painter()
                    .circle_stroke(r.center(), 5.5, Stroke::new(2.0, col(t.split[0])));
                ui.label("推薦・内部活性");
                let (r, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
                ui.painter().circle_filled(r.center(), 4.5, col(t.rec_chain));
                ui.label("推薦由来");
            }
        });
        ui.horizontal_wrapped(|ui| {
            let pb = &mut self.playback;
            if ui
                .add_enabled(pb.k > -1, egui::Button::new("◀|"))
                .on_hover_text("コマ戻し (←)")
                .clicked()
            {
                pb.step_wave(-1);
            }
            let play = if pb.playing { "⏸" } else { "▶" };
            if ui
                .add_enabled(pb.last >= 0, egui::Button::new(play))
                .on_hover_text("再生／一時停止 (Space)")
                .clicked()
            {
                pb.toggle();
            }
            if ui
                .add_enabled(pb.k < pb.last, egui::Button::new("|▶"))
                .on_hover_text("コマ送り (→)")
                .clicked()
            {
                pb.step_wave(1);
            }
            if ui
                .add_enabled(pb.k < pb.last, egui::Button::new("⏭"))
                .on_hover_text("最後までスキップ")
                .clicked()
            {
                pb.skip_to_end();
            }
            egui::ComboBox::from_id_salt("speed")
                .selected_text(format!("{} ウェーブ/秒", pb.speed))
                .show_ui(ui, |ui| {
                    for s in SPEEDS {
                        ui.selectable_value(&mut pb.speed, s, format!("{s} ウェーブ/秒"));
                    }
                });
            ui.label(if pb.last < 0 {
                "—".to_string()
            } else {
                format!("ウェーブ {} / {}", pb.k + 1, pb.last + 1)
            });
            ui.separator();
            ui.label("内部活性:");
            let before = self.view.int_mode;
            egui::ComboBox::from_id_salt("intmode")
                .selected_text(if self.view.int_mode == IntMode::Cumulative {
                    "累積"
                } else {
                    "このラウンドのみ"
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.view.int_mode, IntMode::Cumulative, "累積");
                    ui.selectable_value(
                        &mut self.view.int_mode,
                        IntMode::ThisRoundOnly,
                        "このラウンドのみ",
                    );
                });
            if before != self.view.int_mode {
                self.slots[self.cur].display = None;
            }
            // 推薦で受信の色: 累積（一度でも推薦で受け取った人）か、このラウンドのみか
            if self.rec_on(0) || (self.compare && self.rec_on(1)) {
                ui.label("推薦:");
                let before = self.view.rec_cumulative;
                egui::ComboBox::from_id_salt("recmode")
                    .selected_text(if before {
                        "累積"
                    } else {
                        "このラウンドのみ"
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.view.rec_cumulative, true, "累積");
                        ui.selectable_value(
                            &mut self.view.rec_cumulative,
                            false,
                            "このラウンドのみ",
                        );
                    });
                if before != self.view.rec_cumulative {
                    for s in &mut self.slots {
                        s.display = None;
                    }
                }
            }
        });
    }
}

/// 画面座標から (リング, 扇形) を求める。全体図の外なら None。
pub fn cell_at(cam: &Camera, size: [f32; 2], s: [f32; 2], n_rings: u32) -> Option<(u32, u32)> {
    let w = cam.to_world(size, s);
    let ring = w[0].hypot(w[1]).round() as i64;
    if ring < 1 || ring >= n_rings as i64 {
        return None;
    }
    let mut ang = w[1].atan2(w[0]);
    if ang < 0.0 {
        ang += std::f32::consts::TAU;
    }
    Some((
        ring as u32,
        ((ang / std::f32::consts::TAU * N_SECTORS as f32) as u32).min(N_SECTORS - 1),
    ))
}
