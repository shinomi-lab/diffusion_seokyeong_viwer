//! カメラ（パン・ズーム）。ワールド⇄画面⇄NDC の変換の**唯一の定義**。
//!
//! 画面座標は「ビュー矩形の左上が原点、y が下向き」（egui の座標系。単位は egui のポイント）。
//! ワールドは y が上向き。GPU 側へは `ndc_transform` の結果をユニフォームで渡すだけで、
//! シェーダ側で y を反転しない（Web 版で、ここの不一致がノードとエッジのずれを起こした）。

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub cx: f32,
    pub cy: f32,
    /// ワールド 1 単位あたりのポイント数。
    pub zoom: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            cx: 0.0,
            cy: 0.0,
            zoom: 30.0,
        }
    }
}

pub const MIN_ZOOM: f32 = 0.5;
pub const MAX_ZOOM: f32 = 2000.0;

#[derive(Clone, Copy, Debug)]
pub struct Bounds {
    pub min_x: f32,
    pub min_y: f32,
    pub max_x: f32,
    pub max_y: f32,
}

/// `ndc = world * scale + offset`（x, y とも）。NDC は y が上向き。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NdcTransform {
    pub scale: [f32; 2],
    pub offset: [f32; 2],
}

impl Camera {
    /// ワールド → 画面（ビュー矩形内の座標、左上原点、y 下向き）。
    pub fn to_screen(&self, size: [f32; 2], p: [f32; 2]) -> [f32; 2] {
        [
            (p[0] - self.cx) * self.zoom + size[0] / 2.0,
            -(p[1] - self.cy) * self.zoom + size[1] / 2.0,
        ]
    }

    pub fn to_world(&self, size: [f32; 2], s: [f32; 2]) -> [f32; 2] {
        [
            (s[0] - size[0] / 2.0) / self.zoom + self.cx,
            -(s[1] - size[1] / 2.0) / self.zoom + self.cy,
        ]
    }

    /// GPU 用: ワールド → NDC。`to_screen` と同じ位置になる（`ndc_pixel` で画面座標に戻して照合できる）。
    pub fn ndc_transform(&self, size: [f32; 2]) -> NdcTransform {
        let sx = 2.0 * self.zoom / size[0];
        let sy = 2.0 * self.zoom / size[1];
        NdcTransform {
            scale: [sx, sy],
            offset: [-self.cx * sx, -self.cy * sy],
        }
    }

    /// 指定の領域が収まるカメラ。
    pub fn fit(b: Bounds, size: [f32; 2], margin: f32) -> Self {
        let bw = (b.max_x - b.min_x).max(0.5);
        let bh = (b.max_y - b.min_y).max(0.5);
        let zoom = ((size[0] - margin * 2.0) / bw)
            .min((size[1] - margin * 2.0) / bh)
            .max(1.0);
        Self {
            cx: (b.min_x + b.max_x) / 2.0,
            cy: (b.min_y + b.max_y) / 2.0,
            zoom,
        }
    }

    /// カーソル位置（画面座標）を固定してズームする。
    pub fn zoom_at(&self, size: [f32; 2], s: [f32; 2], factor: f32) -> Self {
        let zoom = (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        let w = self.to_world(size, s);
        Self {
            zoom,
            cx: w[0] - (s[0] - size[0] / 2.0) / zoom,
            cy: w[1] + (s[1] - size[1] / 2.0) / zoom,
        }
    }

    /// 画面上で `d`（ポイント）だけドラッグしたときのパン。
    pub fn panned(&self, d: [f32; 2]) -> Self {
        Self {
            cx: self.cx - d[0] / self.zoom,
            cy: self.cy + d[1] / self.zoom,
            zoom: self.zoom,
        }
    }
}

#[cfg(test)]
impl NdcTransform {
    pub fn apply(&self, p: [f32; 2]) -> [f32; 2] {
        [
            p[0] * self.scale[0] + self.offset[0],
            p[1] * self.scale[1] + self.offset[1],
        ]
    }
}

/// NDC → ビュー内の画面座標（左上原点、y 下向き）。テスト用。
#[cfg(test)]
pub fn ndc_to_screen(ndc: [f32; 2], size: [f32; 2]) -> [f32; 2] {
    [
        (ndc[0] + 1.0) / 2.0 * size[0],
        (1.0 - ndc[1]) / 2.0 * size[1],
    ]
}

pub fn bounds_of(xy: &[f32], nodes: Option<&[u32]>) -> Bounds {
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (
        f32::INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
    );
    let mut visit = |i: usize| {
        let (x, y) = (xy[2 * i], xy[2 * i + 1]);
        min_x = min_x.min(x);
        max_x = max_x.max(x);
        min_y = min_y.min(y);
        max_y = max_y.max(y);
    };
    match nodes {
        Some(ns) => ns.iter().for_each(|&i| visit(i as usize)),
        None => (0..xy.len() / 2).for_each(&mut visit),
    }
    if !min_x.is_finite() {
        return Bounds {
            min_x: -1.0,
            min_y: -1.0,
            max_x: 1.0,
            max_y: 1.0,
        };
    }
    Bounds {
        min_x,
        min_y,
        max_x,
        max_y,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: [f32; 2], b: [f32; 2]) -> bool {
        (a[0] - b[0]).abs() < 1e-3 && (a[1] - b[1]).abs() < 1e-3
    }

    #[test]
    fn screen_world_round_trip() {
        let c = Camera {
            cx: 1.5,
            cy: -0.4,
            zoom: 54.0,
        };
        let size = [820.0, 512.0];
        for p in [[0.0, 0.0], [3.0, 2.0], [-4.0, 1.0], [2.5, -6.0]] {
            assert!(close(c.to_world(size, c.to_screen(size, p)), p));
        }
    }

    /// GPU 用の NDC 変換が、egui 側の `to_screen` と同じ位置になる（y の向きを含む）。Web 版の縦反転バグの再発防止。
    #[test]
    fn ndc_matches_screen_including_y_direction() {
        for c in [
            Camera {
                cx: 0.0,
                cy: 0.0,
                zoom: 30.0,
            },
            Camera {
                cx: 1.4,
                cy: 0.09,
                zoom: 54.0,
            },
            Camera {
                cx: -3.0,
                cy: 7.0,
                zoom: 2.0,
            },
        ] {
            let size = [820.0, 512.0];
            let t = c.ndc_transform(size);
            for p in [[0.0, 0.0], [3.0, 2.0], [-4.0, 1.0], [2.5, -6.0], [0.1, 5.0]] {
                let a = ndc_to_screen(t.apply(p), size);
                let b = c.to_screen(size, p);
                assert!(close(a, b), "{c:?} {p:?}: {a:?} != {b:?}");
            }
        }
        // 上（y が大きい）ほど画面の上（y が小さい）に来る
        let c = Camera::default();
        let size = [100.0, 100.0];
        assert!(c.to_screen(size, [0.0, 1.0])[1] < c.to_screen(size, [0.0, 0.0])[1]);
        assert!(
            ndc_to_screen(c.ndc_transform(size).apply([0.0, 1.0]), size)[1]
                < ndc_to_screen(c.ndc_transform(size).apply([0.0, 0.0]), size)[1]
        );
    }

    #[test]
    fn zoom_keeps_cursor_point_fixed() {
        let c = Camera {
            cx: 2.0,
            cy: 1.0,
            zoom: 40.0,
        };
        let size = [800.0, 500.0];
        let cursor = [123.0, 310.0];
        let before = c.to_world(size, cursor);
        let z = c.zoom_at(size, cursor, 1.7);
        assert!(close(z.to_world(size, cursor), before));
        assert!(z.zoom > c.zoom);
    }

    #[test]
    fn pan_moves_content_with_the_pointer() {
        let c = Camera {
            cx: 0.0,
            cy: 0.0,
            zoom: 10.0,
        };
        let size = [200.0, 200.0];
        let p = [1.0, 1.0];
        let before = c.to_screen(size, p);
        let moved = c.panned([30.0, -20.0]).to_screen(size, p);
        assert!(close(moved, [before[0] + 30.0, before[1] - 20.0]));
    }

    #[test]
    fn fit_contains_bounds() {
        let b = Bounds {
            min_x: -5.0,
            min_y: -2.0,
            max_x: 5.0,
            max_y: 2.0,
        };
        let size = [800.0, 500.0];
        let c = Camera::fit(b, size, 40.0);
        for p in [[b.min_x, b.min_y], [b.max_x, b.max_y]] {
            let s = c.to_screen(size, p);
            assert!(s[0] >= 0.0 && s[0] <= size[0] && s[1] >= 0.0 && s[1] <= size[1]);
        }
    }
}
