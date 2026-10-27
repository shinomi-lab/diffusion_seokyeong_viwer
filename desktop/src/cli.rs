//! コマンドライン引数。
//!   --graph PATH       起動時に読み込むグラフ
//!   --ip N             起動時に選ぶ IP のノード番号（省略時は次数 1 位）
//!   --bench N          ランダム配置の N ノードを描いてフレーム時間を標準出力へ（10.4）
//!   --demo             サンプルを読み込み、IP を選び、数ラウンド発信する（--screenshot と併用）
//!   --expand RING      デモで、発信のあとにそのホップのリングを個別に展開する
//!   --rec THETA        推薦を on（方式 CF＋グラフ）にし、閾値 θ を THETA にして始める（確認用）
//!   --rec-baseline     --rec と併用し、方式を基準（v1）にする
//!   --screenshot PATH  デモの後に画面を PNG に保存して終了する

#[derive(Default, Clone, Debug)]
pub struct Cli {
    pub graph: Option<String>,
    pub ip: Option<u32>,
    pub bench: Option<usize>,
    pub demo: bool,
    pub screenshot: Option<String>,
    /// デモで使うメッセージ系列（既定: 0,3,5,1,0）。
    pub demo_sequence: Option<String>,
    /// デモで、発信のあとに展開するリング（全体図から個別展開の確認用）。
    pub expand_ring: Option<u32>,
    /// デモで、発信した系列を MC で N 試行回し、その間 UI が動き続けることを確かめる。
    pub mc: Option<u32>,
    /// デモの最後に、ノード別イベントを Parquet に書き出す。
    pub export_parquet: Option<String>,
    /// デモで、再生の各ウェーブを連番 PNG（frame_0001.png…）として DIR に書き出す。
    pub record: Option<String>,
    /// `--record` のあと、連番 PNG を MP4 にまとめる（ffmpeg が PATH にあるとき）。
    pub mp4: Option<String>,
    /// デモで、2 画面比較（F13）を使う（B は機構を「既定」にして A と並べる）。
    pub compare: bool,
    /// デモで、巻き戻し → 別のメッセージで分岐 → 元の枝へ戻る（F09）を確かめる。
    pub branch: bool,
    /// 起動時に、凡例で非表示にしておく系列名（カンマ区切り。確認用）。
    pub hide: Option<String>,
    /// 推薦を on にして始めるときの閾値 θ（確認用）。
    pub rec: Option<f64>,
    /// `--rec` の方式を基準（v1）にする。
    pub rec_baseline: bool,
    /// 推薦を方式 wave（Rモデル、v3）・推薦の強さ ε で始める（確認用）。
    pub rec_wave: Option<f64>,
}

pub fn parse(args: impl Iterator<Item = String>) -> Result<Cli, String> {
    let mut cli = Cli::default();
    let mut it = args.skip(1);
    while let Some(a) = it.next() {
        let mut value = |name: &str| it.next().ok_or_else(|| format!("{name} には値が必要です"));
        match a.as_str() {
            "--graph" => cli.graph = Some(value("--graph")?),
            "--ip" => cli.ip = Some(value("--ip")?.parse().map_err(|_| "--ip は整数で指定してください".to_string())?),
            "--bench" => cli.bench = Some(value("--bench")?.parse().map_err(|_| "--bench は整数で指定してください".to_string())?),
            "--screenshot" => cli.screenshot = Some(value("--screenshot")?),
            "--sequence" => cli.demo_sequence = Some(value("--sequence")?),
            "--expand" => cli.expand_ring = Some(value("--expand")?.parse().map_err(|_| "--expand は整数で指定してください".to_string())?),
            "--mc" => cli.mc = Some(value("--mc")?.parse().map_err(|_| "--mc は整数で指定してください".to_string())?),
            "--export-parquet" => cli.export_parquet = Some(value("--export-parquet")?),
            "--record" => cli.record = Some(value("--record")?),
            "--mp4" => cli.mp4 = Some(value("--mp4")?),
            "--compare" => cli.compare = true,
            "--branch" => cli.branch = true,
            "--hide" => cli.hide = Some(value("--hide")?),
            "--demo" => cli.demo = true,
            "--rec-baseline" => cli.rec_baseline = true,
            "--rec-wave" => cli.rec_wave = Some(value("--rec-wave")?.parse().map_err(|_| "--rec-wave は数値で指定してください".to_string())?),
            "--rec" => cli.rec = Some(value("--rec")?.parse().map_err(|_| "--rec は数値で指定してください".to_string())?),
            "-h" | "--help" => return Err("使い方: desktop [--graph PATH] [--ip N] [--bench N] [--demo] [--sequence 0351] [--expand RING] [--mc N] [--export-parquet PATH] [--record DIR [--mp4 PATH]] [--compare] [--branch] [--rec THETA [--rec-baseline]] [--rec-wave EPS] [--screenshot PATH]".into()),
            other => return Err(format!("未知の引数です: {other}")),
        }
    }
    Ok(cli)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(a: &[&str]) -> Result<Cli, String> {
        parse(
            std::iter::once("desktop")
                .chain(a.iter().copied())
                .map(String::from),
        )
    }

    #[test]
    fn parses_options() {
        let c = p(&[
            "--graph",
            "g.txt",
            "--ip",
            "7",
            "--bench",
            "10000",
            "--demo",
            "--screenshot",
            "o.png",
        ])
        .unwrap();
        assert_eq!(
            (
                c.graph.as_deref(),
                c.ip,
                c.bench,
                c.demo,
                c.screenshot.as_deref()
            ),
            (Some("g.txt"), Some(7), Some(10000), true, Some("o.png"))
        );
        assert!(p(&["--ip", "x"]).is_err());
        assert!(p(&["--nope"]).is_err());
        assert!(p(&["--graph"]).is_err());
    }
}
