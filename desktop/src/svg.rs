//! 時系列の SVG 書き出し（F15）。Web 版の `logic/svg.ts` と同じ体裁。縦軸は値に合わせて広げ、上限・下限を設けない（INV-15）。

pub struct SvgSeries {
    pub label: String,
    pub color: String,
    pub dash: Option<[f32; 2]>,
    pub hidden: bool,
    pub values: Vec<f64>,
}

pub struct SvgChart {
    pub title: String,
    pub x_label: String,
    pub x: Vec<f64>,
    pub series: Vec<SvgSeries>,
    /// (上側の系列番号, 下側の系列番号, 色)。系列番号は 0 始まり（series の添字）。
    pub bands: Vec<(usize, usize, String)>,
}

pub fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// 区間 [min, max] を覆う、きりのよい目盛り（およそ n 個）。
pub fn nice_ticks(min: f64, max: f64, n: usize) -> Vec<f64> {
    if !min.is_finite() || !max.is_finite() {
        return vec![0.0, 1.0];
    }
    if min == max {
        return vec![min - 1.0, min, min + 1.0];
    }
    let raw = (max - min) / n as f64;
    let mag = 10f64.powf(raw.log10().floor());
    let step = [1.0, 2.0, 5.0, 10.0]
        .iter()
        .map(|m| m * mag)
        .find(|&s| s >= raw)
        .unwrap_or(raw);
    let mut out = Vec::new();
    let mut k = (min / step).floor() as i64;
    loop {
        let v = k as f64 * step;
        out.push(format!("{v:.12}").parse::<f64>().unwrap_or(v));
        if v >= max {
            break;
        }
        k += 1;
    }
    out
}

fn fmt(v: f64) -> String {
    if v.fract() == 0.0 {
        format!("{v:.0}")
    } else {
        let s = format!("{:.*}", if v.abs() < 1.0 { 3 } else { 2 }, v);
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

pub fn line_chart_svg(c: &SvgChart) -> String {
    let (w, h) = (760.0f64, 360.0f64);
    let (l, r, t, b) = (
        58.0,
        190.0,
        if c.title.is_empty() { 14.0 } else { 34.0 },
        44.0,
    );
    let (pw, ph) = (w - l - r, h - t - b);
    let xs: Vec<f64> = c.x.iter().copied().filter(|v| v.is_finite()).collect();
    let (mut x0, mut x1) = (
        xs.iter().copied().fold(f64::INFINITY, f64::min),
        xs.iter().copied().fold(f64::NEG_INFINITY, f64::max),
    );
    if !xs.is_empty() && x0 == x1 {
        x0 -= 0.5;
        x1 += 0.5;
    }
    if xs.is_empty() {
        (x0, x1) = (0.0, 1.0);
    }
    let mut ys: Vec<f64> = Vec::new();
    for s in c.series.iter().filter(|s| !s.hidden) {
        ys.extend(s.values.iter().copied().filter(|v| v.is_finite()));
    }
    for (hi, lo, _) in &c.bands {
        for i in [hi, lo] {
            if let Some(s) = c.series.get(*i) {
                ys.extend(s.values.iter().copied().filter(|v| v.is_finite()));
            }
        }
    }
    let (mut y0, mut y1) = if ys.is_empty() {
        (0.0, 1.0)
    } else {
        (
            ys.iter().copied().fold(f64::INFINITY, f64::min),
            ys.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        )
    };
    if y0 == y1 {
        y0 -= 0.5;
        y1 += 0.5;
    }
    let pad = (y1 - y0) * 0.08;
    y0 -= pad;
    y1 += pad;
    let sx = |x: f64| l + (x - x0) / (x1 - x0) * pw;
    let sy = |y: f64| t + ph - (y - y0) / (y1 - y0) * ph;
    // 画面と同じダーク専用の配色（ui-tokens.json の chrome）
    let tk = crate::tokens::Tokens::load();
    let (bg, fg, grid) = (
        crate::tokens::to_hex(tk.chrome.card),
        crate::tokens::to_hex(tk.chrome.text),
        crate::tokens::to_hex(tk.chrome.grid),
    );
    let mut o = Vec::new();
    o.push(format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}" font-family="system-ui, 'Hiragino Sans', sans-serif" font-size="11">"#));
    o.push(format!(r#"<rect width="{w}" height="{h}" fill="{bg}"/>"#));
    if !c.title.is_empty() {
        o.push(format!(
            r#"<text x="{l}" y="20" font-size="13" font-weight="600" fill="{fg}">{}</text>"#,
            escape_xml(&c.title)
        ));
    }
    for tk in nice_ticks(y0, y1, 5) {
        if tk < y0 || tk > y1 {
            continue;
        }
        o.push(format!(
            r#"<line x1="{l}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="{grid}"/>"#,
            sy(tk),
            l + pw,
            sy(tk)
        ));
        o.push(format!(
            r#"<text x="{}" y="{:.1}" text-anchor="end" fill="{fg}">{}</text>"#,
            l - 6.0,
            sy(tk) + 3.5,
            fmt(tk)
        ));
    }
    let mut xt: Vec<f64> = if xs.len() <= 12 {
        xs.clone()
    } else {
        nice_ticks(x0, x1, 8)
            .into_iter()
            .filter(|t| *t >= x0 && *t <= x1)
            .collect()
    };
    xt.dedup();
    for tk in xt {
        o.push(format!(
            r#"<line x1="{:.1}" y1="{t}" x2="{:.1}" y2="{:.1}" stroke="{grid}"/>"#,
            sx(tk),
            sx(tk),
            t + ph
        ));
        o.push(format!(
            r#"<text x="{:.1}" y="{:.1}" text-anchor="middle" fill="{fg}">{}</text>"#,
            sx(tk),
            t + ph + 15.0,
            fmt(tk)
        ));
    }
    o.push(format!(r#"<rect x="{l}" y="{t}" width="{pw}" height="{ph}" fill="none" stroke="{fg}" stroke-opacity="0.4"/>"#));
    if !c.x_label.is_empty() {
        o.push(format!(
            r#"<text x="{:.1}" y="{}" text-anchor="middle" fill="{fg}">{}</text>"#,
            l + pw / 2.0,
            h - 8.0,
            escape_xml(&c.x_label)
        ));
    }
    for (hi, lo, color) in &c.bands {
        let (Some(up), Some(dn)) = (c.series.get(*hi), c.series.get(*lo)) else {
            continue;
        };
        let pts = |s: &SvgSeries| -> Vec<(f64, f64)> {
            s.values
                .iter()
                .enumerate()
                .filter(|&(i, &v)| v.is_finite() && c.x.get(i).is_some_and(|x| x.is_finite()))
                .map(|(i, &v)| (c.x[i], v))
                .collect()
        };
        let (u, mut d) = (pts(up), pts(dn));
        if u.len() < 2 {
            continue;
        }
        d.reverse();
        let poly: Vec<String> = u
            .iter()
            .chain(d.iter())
            .map(|(x, v)| format!("{:.1},{:.1}", sx(*x), sy(*v)))
            .collect();
        o.push(format!(
            r#"<polygon points="{}" fill="{color}" fill-opacity="0.18" stroke="none"/>"#,
            poly.join(" ")
        ));
    }
    for s in c.series.iter().filter(|s| !s.hidden) {
        let mut d = String::new();
        let mut pen = false;
        for (i, &v) in s.values.iter().enumerate() {
            let x = c.x.get(i).copied().unwrap_or(f64::NAN);
            if !v.is_finite() || !x.is_finite() {
                pen = false;
                continue;
            }
            d.push_str(&format!(
                "{}{:.1},{:.1}",
                if pen { "L" } else { "M" },
                sx(x),
                sy(v)
            ));
            pen = true;
        }
        if d.is_empty() {
            continue;
        }
        let dash = s
            .dash
            .map(|[a, b]| format!(r#" stroke-dasharray="{a} {b}""#))
            .unwrap_or_default();
        o.push(format!(
            r#"<path d="{d}" fill="none" stroke="{}" stroke-width="1.8"{dash}/>"#,
            s.color
        ));
    }
    let mut ly = t + 6.0;
    for s in c.series.iter().filter(|s| !s.hidden) {
        let dash = s
            .dash
            .map(|[a, b]| format!(r#" stroke-dasharray="{a} {b}""#))
            .unwrap_or_default();
        o.push(format!(
            r#"<line x1="{}" y1="{ly}" x2="{}" y2="{ly}" stroke="{}" stroke-width="2"{dash}/>"#,
            l + pw + 14.0,
            l + pw + 36.0,
            s.color
        ));
        o.push(format!(
            r#"<text x="{}" y="{}" fill="{fg}">{}</text>"#,
            l + pw + 42.0,
            ly + 3.5,
            escape_xml(&s.label)
        ));
        ly += 16.0;
    }
    o.push("</svg>".to_string());
    o.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(label: &str, color: &str, hidden: bool, dash: Option<[f32; 2]>, v: &[f64]) -> SvgSeries {
        SvgSeries {
            label: label.into(),
            color: color.into(),
            dash,
            hidden,
            values: v.to_vec(),
        }
    }

    fn chart() -> SvgChart {
        SvgChart {
            title: "ラウンド別".into(),
            x_label: "ラウンド".into(),
            x: vec![1.0, 2.0, 3.0, 4.0],
            series: vec![
                s("平均", "#0072B2", false, None, &[0.1, 0.2, 0.4, 0.5]),
                s("平均 p10", "#0072B2", true, None, &[0.0, 0.1, 0.3, 0.4]),
                s("平均 p90", "#0072B2", true, None, &[0.2, 0.3, 0.5, 0.6]),
                s(
                    "B 平均",
                    "#009E73",
                    false,
                    Some([6.0, 4.0]),
                    &[0.1, f64::NAN, 0.3, 0.2],
                ),
            ],
            bands: vec![(2, 1, "#0072B2".into())],
        }
    }

    #[test]
    fn svg_has_series_band_legend_and_no_nan() {
        let out = line_chart_svg(&chart());
        assert!(out.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\""));
        assert!(out.trim_end().ends_with("</svg>"));
        assert_eq!(out.matches("<path ").count(), 2);
        assert_eq!(out.matches("<polygon ").count(), 1);
        assert!(out.contains("stroke-dasharray=\"6 4\""));
        assert!(out.contains(">平均<"));
        assert!(!out.contains("平均 p10"));
        assert!(!out.contains("NaN"));
        // NaN で線が 2 つに分かれる
        let b = out
            .lines()
            .find(|l| l.contains("#009E73") && l.contains("<path"))
            .unwrap();
        assert_eq!(b.matches('M').count(), 2);
    }

    #[test]
    fn escapes_and_does_not_clamp_the_axis() {
        assert_eq!(
            escape_xml("A<B&\"C\">'"),
            "A&lt;B&amp;&quot;C&quot;&gt;&apos;"
        );
        let c = SvgChart {
            title: "x<y".into(),
            x_label: String::new(),
            x: vec![0.0, 1.0, 2.0],
            series: vec![s("効用", "#000", false, None, &[0.0, 1.0, 1.73])],
            bands: vec![],
        };
        let out = line_chart_svg(&c);
        assert!(out.contains("x&lt;y"));
        let ticks: Vec<f64> = out
            .lines()
            .filter(|l| l.contains("text-anchor=\"end\""))
            .filter_map(|l| l.split('>').nth(1)?.split('<').next()?.parse().ok())
            .collect();
        assert!(
            ticks.iter().cloned().fold(f64::MIN, f64::max) >= 1.5,
            "1 を超える値が切れない: {ticks:?}"
        );
    }

    #[test]
    fn empty_and_single_point_are_fine() {
        let c = SvgChart {
            title: String::new(),
            x_label: String::new(),
            x: vec![],
            series: vec![],
            bands: vec![],
        };
        assert!(line_chart_svg(&c).contains("</svg>"));
        let c = SvgChart {
            title: String::new(),
            x_label: String::new(),
            x: vec![1.0],
            series: vec![s("a", "#000", false, None, &[2.0])],
            bands: vec![],
        };
        assert!(!line_chart_svg(&c).contains("NaN"));
    }

    #[test]
    fn ticks_cover_the_range() {
        let t = nice_ticks(-0.43, 1.73, 5);
        assert!(t[0] <= -0.43 && *t.last().unwrap() >= 1.73);
        assert_eq!(
            nice_ticks(0.0, 10.0, 5),
            vec![0.0, 2.0, 4.0, 6.0, 8.0, 10.0]
        );
        assert_eq!(nice_ticks(3.0, 3.0, 5), vec![2.0, 3.0, 4.0]);
    }
}
