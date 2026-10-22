//! 画像・連番 PNG・動画の書き出し（F15）。egui の画面スクリーンショットから領域を切り出して PNG にする。
//! 動画（MP4）は、連番 PNG を `ffmpeg`（PATH にあるとき）でまとめる。ユーザーが押したときだけ実行する。

use std::path::{Path, PathBuf};
use std::process::Command;

use eframe::egui::{ColorImage, Rect};

/// スクリーンショット（画面全体）の `rect`（egui のポイント）を PNG で保存する。
pub fn save_crop(
    image: &ColorImage,
    rect: Rect,
    ppp: f32,
    path: &Path,
) -> Result<(u32, u32), String> {
    let [w, h] = image.size;
    let x0 = ((rect.min.x * ppp).round().max(0.0) as usize).min(w);
    let y0 = ((rect.min.y * ppp).round().max(0.0) as usize).min(h);
    let x1 = ((rect.max.x * ppp).round().max(0.0) as usize).min(w);
    let y1 = ((rect.max.y * ppp).round().max(0.0) as usize).min(h);
    if x1 <= x0 || y1 <= y0 {
        return Err("切り出す範囲が空です".into());
    }
    let raw = image.as_raw();
    let mut buf = Vec::with_capacity((x1 - x0) * (y1 - y0) * 4);
    for y in y0..y1 {
        buf.extend_from_slice(&raw[(y * w + x0) * 4..(y * w + x1) * 4]);
    }
    image::save_buffer(
        path,
        &buf,
        (x1 - x0) as u32,
        (y1 - y0) as u32,
        image::ColorType::Rgba8,
    )
    .map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(((x1 - x0) as u32, (y1 - y0) as u32))
}

pub fn frame_path(dir: &Path, n: usize) -> PathBuf {
    dir.join(format!("frame_{n:04}.png"))
}

pub fn ffmpeg_available() -> bool {
    Command::new("ffmpeg")
        .arg("-version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// `dir/frame_0001.png`… を MP4 にする。`fps` は 1 秒あたりのフレーム数（ウェーブ）。
pub fn make_mp4(dir: &Path, out: &Path, fps: u32) -> Result<(), String> {
    let o = Command::new("ffmpeg")
        .args(["-y", "-v", "error", "-framerate", &fps.to_string(), "-i"])
        .arg(dir.join("frame_%04d.png"))
        .args([
            "-vf",
            "pad=ceil(iw/2)*2:ceil(ih/2)*2",
            "-pix_fmt",
            "yuv420p",
        ])
        .arg(out)
        .output()
        .map_err(|e| format!("ffmpeg を実行できません: {e}"))?;
    if o.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&o.stderr).trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{Color32, pos2};

    #[test]
    fn crops_the_requested_region_in_pixels() {
        // 4x3 ピクセル。(1,1) だけ赤
        let mut img = ColorImage::filled([4, 3], Color32::WHITE);
        img.pixels[4 + 1] = Color32::RED;
        let dir = std::env::temp_dir().join(format!("viewer_crop_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("c.png");
        // ppp = 2: ポイントの (0.5, 0.5)〜(2, 1.5) = ピクセルの (1,1)〜(4,3)
        let (w, h) = save_crop(
            &img,
            Rect::from_min_max(pos2(0.5, 0.5), pos2(2.0, 1.5)),
            2.0,
            &p,
        )
        .unwrap();
        assert_eq!((w, h), (3, 2));
        let out = image::open(&p).unwrap().to_rgba8();
        assert_eq!(out.dimensions(), (3, 2));
        assert_eq!(out.get_pixel(0, 0).0, [255, 0, 0, 255]);
        assert_eq!(out.get_pixel(1, 0).0, [255, 255, 255, 255]);
        // 範囲外は切り詰める／空ならエラー
        assert!(
            save_crop(
                &img,
                Rect::from_min_max(pos2(10.0, 10.0), pos2(20.0, 20.0)),
                1.0,
                &p
            )
            .is_err()
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn frame_names_are_zero_padded_and_sequential() {
        assert_eq!(
            frame_path(Path::new("/x"), 1),
            PathBuf::from("/x/frame_0001.png")
        );
        assert_eq!(
            frame_path(Path::new("/x"), 123),
            PathBuf::from("/x/frame_0123.png")
        );
    }
}
