# デスクトップ版の実行方法（情報拡散ライブビューア）

Rust（egui + wgpu）のネイティブアプリです。`sim-api` を同じプロセスで直接呼びます。サーバへの送信はなく、すべてローカルで動きます。Web 版と同じ画面仕様（`spec/ui-tokens.json`）です。

## 1. 必要なもの
- Rust（edition 2024 が使えるもの。動作確認は rustc 1.91 nightly）。`rustc --version` で確認します。
- macOS（Metal）で動作確認しました。Windows（DX12 / Vulkan）・Linux（Vulkan）向けのバックエンドも指定してありますが、未確認です。
- 日本語フォント: macOS のヒラギノ角ゴシックを自動で読み込みます。見つからないと文字化けします（Windows は游ゴシック・メイリオ、Linux は Noto Sans CJK を探します）。

## 2. 起動
```bash
cd "/Users/hongseokyeong/Desktop/rustモデル/information_diffusion_veiwer"
cargo run --release -p desktop
```
初回はビルドに数分かかります（`--release` を付けないと描画が遅くなります）。

起動後の使い方は Web 版と同じです（`web_read_me.md` の 3 章）。左の「グラフ」で「サンプル（100random）を読み込む」を押し、「IP の選択」で順位表のノードを選び、「メッセージと発信」で発信します。キー操作（`1`〜`6`、`Enter`、`Space`、`←` `→`）、ホイールでズーム、ドラッグでパン、`Shift` + ドラッグで範囲選択も同じです。

### コマンドライン引数
| 引数 | 内容 |
| --- | --- |
| `--graph PATH` | 起動時にグラフを読み込む（`.txt` `.csv` `.tsv`、`.gz` も可） |
| `--ip N` | 起動時に選ぶ IP のノード番号（省略時は次数 1 位。`--demo` のとき） |
| `--demo` | サンプルを読み込み、IP を選び、数ラウンド発信する（既定の系列は `0351`。`--sequence 0351` で変更） |
| `--expand RING` | デモの最後に、そのホップのリングを個別に展開する |
| `--mc N` | デモで、発信した系列を MC で N 試行回す（別スレッド） |
| `--export-parquet PATH` | デモの最後に、ノード別イベントを Parquet に書き出す |
| `--screenshot PATH` | デモのあとに画面を PNG に保存して終了する |
| `--bench N` | ランダム配置の N ノードを描き、フレーム時間を標準出力へ出して終了する（垂直同期なし） |

例: Twitter 規模のグラフで、全体図 → リング展開 → 発信を試す。
```bash
cargo run --release -p desktop -- --graph "../diffusion_seokyeong/test/twitter_dataset.txt" --demo --expand 1 --screenshot out.png
```

## 2.5 巻き戻し・MC・比較・書き出し（P3）
使い方は Web 版（`web_read_me.md` の 3.5）と同じです。デスクトップ版の違い:
- **分岐ツリー**: 下段右の「選んだ系列」に描かれます。節のクリックで巻き戻し／枝の切り替え。
- **2 画面比較**: 左パネルの「2 画面比較（A と B）」で「比較モード」を入れると、中央が左右に分かれます（B の機構を変えて「B に設定を適用」）。
- **画像・連番 PNG・MP4**: ツールバーの「画像 (PNG)」（ネットワーク図）、「連番 PNG…」（いまのラウンドの再生を、ウェーブごとに `frame_0001.png`… としてフォルダへ保存。ラウンド開始前を 1 コマ目とし、枚数 = ウェーブ数 + 1）、「MP4 にする」（`ffmpeg` が PATH にあるとき、直前の連番 PNG をまとめる）。時系列グラフの「SVG」「PNG」でグラフを保存できます。
- 追加の CLI: `--branch`（巻き戻し → 別のメッセージで分岐 → 元の枝へ戻る。元と一致を標準出力に出す）、`--compare`（B を「既定」の機構にして並べる）、`--record DIR [--mp4 PATH]`（連番 PNG と MP4）。

## 3. 書き出し
ツールバーから、シナリオ記録（JSON）、ラウンド別（CSV）、ノード別イベント（CSV / **Parquet**）を保存できます。Parquet は Polars（Python）で読めます。
```python
import polars as pl
pl.read_parquet("events.parquet")   # round, wave, node, node_label, from, from_label, u_int, u_ext, kind
```
MC の併記は、右下の「この系列を MC で回す」を押すと、別スレッド（rayon）で計算され、結果の平均と 10〜90% の帯が時系列に重なります。

## 4. 確認用コマンド
```bash
cargo test -p desktop --release          # 座標変換・再生・位置計算・実 GPU のピクセル検証・ゴールデン一致・MC・Parquet
cargo clippy -p desktop --no-deps -- -D warnings
```
GPU のテストは、ノードと辺が `camera.rs` の位置（上下ミラーでない位置）に描かれていることを、オフスクリーン描画のピクセルで確かめます。GPU が使えない環境では自動でスキップされます。

## 5. うまく動かないとき
| 症状 | 対処 |
| --- | --- |
| 文字が四角（□）になる | 日本語フォントが見つかっていません。`desktop/src/app.rs` の `install_japanese_font` の候補に、手元のフォントのパスを足す |
| ウィンドウが開かない／すぐ落ちる | GPU ドライバ（Metal / Vulkan / DX12）を確認する。ターミナルのエラー出力を見る |
| 「ノード数が 10000 を超えています」 | 全体図から始まります。リング表の「展開」か、扇形セルのクリックで個別に展開します（上限 10,000 ノード） |
| IP の周りだけが表示され、ほかのノードが見えない | IP から**到達できないノード**は、通常は表示しません。画面上部の「IP から到達できないノード N 個は表示していません」の「表示する」で展開できます |
| ビルドが遅い／失敗する | `--release` で実行する。eframe 0.33 を使っているのは rustc 1.91 の制約です（最新の eframe 0.36 は rustc 1.95 が必要） |

## 6. 既知の制限（P2 時点）
- マウス操作（ドラッグ・ホイール・Shift ドラッグ）の自動テストはありません（座標変換とクリックの当たり判定は検証済み）。
- MP4 は `ffmpeg` が必要です（無ければ連番 PNG まで）。
- 比較中は、GPU の描画バッファ・ノード状態が 2 組になります（Twitter 規模でも動きますが、Web 版ほど厳しくありません）。
- 実 GPU での性能（Apple M2）: 10,000 ノードで平均 106 fps、81,306 ノードで平均 95 fps（`docs/15.3-notes.md`）。
