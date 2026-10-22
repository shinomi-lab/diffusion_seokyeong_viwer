# 2026-10-04 ダーク専用の配色（ダーク・ネットワークダッシュボード風）

## 依頼
- Dribbble の参照画像のように彩色する。試作画像と計画の PDF を作る（`docs/design-proposal/`）。
- 承認後: データの色を変えてよい。ライトモードでもダーク専用と同じに（OS の設定に関係なくダーク）。

## 実施
- 試作: Web の作業用コピーで配色だけを差し替えて撮影。色覚シミュレーションで参照の色を調整（ミントを明るく、ローズ・青を中くらいに）。PDF 5 ページ。
- 本適用: `spec/ui-tokens.json` を 1 組に。Web（tokens.ts・styles.css・NetworkView・overlay・Toolbar・Plot・svg.ts・shots.mjs）、デスクトップ（theme.rs・tokens.rs・gpu/mod.rs・network.rs・ui.rs・app.rs・control.rs・charts.rs・svg.rs・gpu/tests.rs）。
- 時系列の面塗り（Web は縦グラデーション、デスクトップは単色の淡い塗り）。

## 確認
- cargo test --workspace --release、clippy -D warnings、wasm-pack test、npm run build / npm test / e2e:align / e2e:p3 / e2e:media すべて通過。
- スクリーンショット: Web を OS ライトでエミュレートしてもダークで表示。デスクトップ（通常・比較 + MC）。
- 性能: 前回と同程度。

## つまずき
- uPlot の fill 関数が配置前（bbox 未確定）に呼ばれて例外 → 単色を返して保護。
- svg.ts の上余白の変数 `T` と tokens の `T` が衝突 → `Tok` に別名。
- 白地の SVG では新しい色が読めない → 書き出しもダークの地に（notes に仮定として記録）。

## 限界・次
- デスクトップの面塗りはグラデーションではなく単色（egui_plot の制約）。
- Web の実 GPU でのフレーム時間は未測定。
