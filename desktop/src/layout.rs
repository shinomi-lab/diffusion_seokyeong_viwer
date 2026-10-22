//! 全ノードのワールド座標（リング半径 = ホップ距離、角度 = 放射状レイアウト）。描画側の仕事（7.8 の 4・5）。
//! Web 版の `logic/layout.ts` と同じ式。

pub const UNREACHABLE: u32 = u32::MAX;
/// 個別展開できる上限（N03。sim-api の EXPAND_CAP と同じ）。
pub const EXPAND_CAP: usize = sim_api::overview::EXPAND_CAP;
const PHI: f64 = 0.618_033_988_7;

pub struct Positions {
    /// [x0, y0, x1, y1, …]。到達不能のノードは外側の格子に置く。
    pub xy: Vec<f32>,
    pub max_dist: u32,
}

pub fn compute_positions(angle: &[f32], dist: &[u32]) -> Positions {
    let n = dist.len();
    let mut xy = vec![0.0f32; n * 2];
    let max_dist = dist
        .iter()
        .copied()
        .filter(|&d| d != UNREACHABLE)
        .max()
        .unwrap_or(1)
        .max(1);
    let mut unreach = Vec::new();
    for i in 0..n {
        let d = dist[i];
        if d == UNREACHABLE {
            unreach.push(i);
            continue;
        }
        // 重なりを減らすため、半径に決定的なずらしを入れる（IP は中心）
        let r = if d == 0 {
            0.0
        } else {
            d as f64 + 0.3 * ((i as f64 * PHI) % 1.0 - 0.5)
        };
        let a = angle[i] as f64;
        xy[2 * i] = (r * a.cos()) as f32;
        xy[2 * i + 1] = (r * a.sin()) as f32;
    }
    // 到達不能: 右側（x > max_dist + 2）に格子状に並べる。上限は EXPAND_CAP
    let cols = ((unreach.len().min(EXPAND_CAP) as f64).sqrt().ceil() as usize).max(1);
    for (k, &node) in unreach.iter().enumerate() {
        xy[2 * node] = max_dist as f32 + 3.0 + (k % cols) as f32 * 0.35;
        xy[2 * node + 1] = (k / cols) as f32 * 0.35 - (cols as f32 * 0.35) / 2.0;
    }
    Positions { xy, max_dist }
}

/// 到達不能の枠（全体図）のワールド矩形 (x, y, w, h)。
pub fn unreachable_rect(max_dist: u32) -> (f32, f32, f32, f32) {
    (max_dist as f32 + 2.5, -1.5, 3.0, 3.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ip_at_center_rings_by_distance_and_unreachable_grid() {
        let dist = [0, 1, 1, 2, UNREACHABLE, UNREACHABLE];
        let angle = [0.0, 0.0, std::f32::consts::FRAC_PI_2, 1.0, 0.0, 0.0];
        let p = compute_positions(&angle, &dist);
        assert_eq!(p.max_dist, 2);
        assert_eq!((p.xy[0], p.xy[1]), (0.0, 0.0));
        // 距離 1 のノードは半径 1 付近（±0.15）
        for i in [1usize, 2] {
            let r = (p.xy[2 * i].powi(2) + p.xy[2 * i + 1].powi(2)).sqrt();
            assert!((r - 1.0).abs() < 0.16, "r={r}");
        }
        // 角度 0 は +x 方向、π/2 は +y 方向（y が上向き）
        assert!(p.xy[2] > 0.5 && p.xy[3].abs() < 1e-3);
        assert!(p.xy[5] > 0.5);
        // 到達不能は外側の格子（x > max_dist + 2）
        assert!(p.xy[8] > 4.0 && p.xy[10] > 4.0);
        // 決定的
        assert_eq!(p.xy, compute_positions(&angle, &dist).xy);
    }
}
