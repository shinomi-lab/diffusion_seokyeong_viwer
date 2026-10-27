//! `spec/ui-tokens.json`（Web・デスクトップ共通の色・形・寸法。11.1）を読む。色や寸法はここ以外にハードコードしない。

use serde_json::Value;

const RAW: &str = include_str!("../../spec/ui-tokens.json");

pub fn hex(s: &str) -> [f32; 3] {
    let n = u32::from_str_radix(s.trim_start_matches('#'), 16).unwrap_or(0);
    [
        ((n >> 16) & 255) as f32 / 255.0,
        ((n >> 8) & 255) as f32 / 255.0,
        (n & 255) as f32 / 255.0,
    ]
}

#[derive(Clone, Debug)]
pub struct KindStyle {
    pub label: String,
    pub square: bool,
    pub fill: Option<[f32; 3]>,
    pub stroke: Option<[f32; 3]>,
    pub stroke_width: f32,
    pub radius_scale: f32,
}

/// UI の枠の色（`chrome`）。中立色の段階（地・カード・部品・hover・枠線・補助文字・本文）とアクセント。
#[derive(Clone, Debug)]
pub struct Chrome {
    pub app: [f32; 3],
    pub card: [f32; 3],
    pub ui: [f32; 3],
    pub hover: [f32; 3],
    pub border: [f32; 3],
    pub border_strong: [f32; 3],
    pub muted: [f32; 3],
    pub text: [f32; 3],
    pub accent: [f32; 3],
    pub accent_soft: [f32; 3],
    pub on_accent: [f32; 3],
    pub grid: [f32; 3],
}

impl Chrome {
    fn load(v: &Value) -> Self {
        let c = |k: &str| hex(v[k].as_str().unwrap_or("#000000"));
        Self {
            app: c("app"),
            card: c("card"),
            ui: c("ui"),
            hover: c("hover"),
            border: c("border"),
            border_strong: c("borderStrong"),
            muted: c("muted"),
            text: c("text"),
            accent: c("accent"),
            accent_soft: c("accentSoft"),
            on_accent: c("onAccent"),
            grid: c("grid"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Tokens {
    pub kinds: Vec<KindStyle>,
    pub split: [[f32; 3]; 2],
    pub wave: [f32; 3],
    pub selected: [f32; 3],
    pub halo_scale: f32,
    /// 推薦で受信（黄色の塗り）・推薦由来の連鎖（暗い黄色の塗り）の色と、内部活性の青い縁取りの太さ px。
    pub rec: [f32; 3],
    pub rec_chain: [f32; 3],
    pub rec_ring_px: f32,
    pub radius_px: f32,
    pub series_internal: [f32; 3],
    pub series_external: [f32; 3],
    pub series_both: [f32; 3],
    pub series_new_xact: [f32; 3],
    pub series_rec_xact: [f32; 3],
    pub series_rec_share: [f32; 3],
    pub band_alpha: f32,
    pub ring_stroke: [f32; 3],
    pub ring_dash: [f32; 2],
    pub ring_label: [f32; 3],
    /// ネットワーク図の地。配色はダーク専用（OS のライト／ダークに従わない）。
    pub bg: [f32; 3],
    pub edge: [f32; 3],
    pub chrome: Chrome,
    pub radius_card: f32,
    pub radius_control: f32,
}

impl Tokens {
    pub fn load() -> Self {
        let v: Value = serde_json::from_str(RAW).expect("spec/ui-tokens.json を解析できません");
        let c = |p: &Value| hex(p.as_str().unwrap_or("#000000"));
        let kinds = (0..6)
            .map(|i| {
                let k = &v["kinds"][i.to_string()];
                KindStyle {
                    label: k["label"].as_str().unwrap_or("").to_string(),
                    square: k["shape"] == "square",
                    fill: k["fill"].as_str().map(hex),
                    stroke: k["stroke"].as_str().map(hex),
                    stroke_width: k["strokeWidth"].as_f64().unwrap_or(0.0) as f32,
                    radius_scale: k["radiusScale"].as_f64().unwrap_or(1.0) as f32,
                }
            })
            .collect();
        let split = &v["kinds"]["4"]["fillSplit"];
        Self {
            kinds,
            split: [c(&split[0]), c(&split[1])],
            wave: c(&v["highlight"]["currentWave"]),
            selected: c(&v["highlight"]["selected"]),
            halo_scale: v["highlight"]["haloScale"].as_f64().unwrap_or(1.35) as f32,
            rec: c(&v["highlight"]["recommended"]),
            rec_chain: c(&v["highlight"]["recChain"]),
            rec_ring_px: v["highlight"]["recRingPx"].as_f64().unwrap_or(2.0) as f32,
            radius_px: v["node"]["radiusPx"].as_f64().unwrap_or(4.0) as f32,
            series_internal: c(&v["series"]["internal"]),
            series_external: c(&v["series"]["external"]),
            series_both: c(&v["series"]["both"]),
            series_new_xact: c(&v["series"]["newXact"]),
            series_rec_xact: c(&v["series"]["recXact"]),
            series_rec_share: c(&v["series"]["recShare"]),
            band_alpha: v["series"]["bandAlpha"].as_f64().unwrap_or(0.18) as f32,
            ring_stroke: c(&v["rings"]["stroke"]),
            ring_dash: [
                v["rings"]["dash"][0].as_f64().unwrap_or(3.0) as f32,
                v["rings"]["dash"][1].as_f64().unwrap_or(4.0) as f32,
            ],
            ring_label: c(&v["rings"]["label"]),
            bg: c(&v["background"]),
            edge: c(&v["edges"]),
            chrome: Chrome::load(&v["chrome"]),
            radius_card: v["chrome"]["radiusCard"].as_f64().unwrap_or(10.0) as f32,
            radius_control: v["chrome"]["radiusControl"].as_f64().unwrap_or(7.0) as f32,
        }
    }
}

/// 0〜1 の色 → `#RRGGBB`（SVG 用）。
pub fn to_hex(c: [f32; 3]) -> String {
    format!(
        "#{:02X}{:02X}{:02X}",
        (c[0] * 255.0).round() as u8,
        (c[1] * 255.0).round() as u8,
        (c[2] * 255.0).round() as u8
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_the_shared_tokens() {
        let t = Tokens::load();
        assert_eq!(t.kinds.len(), 6);
        assert_eq!(t.kinds[3].label, "外部活性");
        assert!(t.kinds[3].square && t.kinds[4].square && !t.kinds[2].square);
        assert_eq!(t.kinds[5].radius_scale, 2.4);
        assert_eq!(t.radius_px, 4.0);
        assert_eq!(hex("#0072B2"), [0.0, 114.0 / 255.0, 178.0 / 255.0]);
        assert_eq!(t.split[0], hex("#3A8EEA"));
        assert_eq!(t.bg, hex("#10131A"));
    }
}
