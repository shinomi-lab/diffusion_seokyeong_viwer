# 2026-10-04 見た目の改良（Web・デスクトップ）

## 依頼
- 使う側にとって見た目と雰囲気を良くする。配置・UI は維持し、色・背景など見た目だけ。
- 評価の高い事例を調べて適用。性能に悪影響を出さない。

## 決定
- 方向性: 落ち着いた研究ツール風（ユーザー選択）。
- 参考事例: Linear、Radix Colors、ダッシュボードの 60-30-10、IBM Carbon、Datawrapper、Cosmograph / Gephi Lite。
- ノード・系列の色は意味と色覚配慮があるので変えない。

## 実施
- `spec/ui-tokens.json`: chrome・edges・rings の暗色版・background。
- Web: `styles.css` 全面、`tokens.ts::applyChromeVars`、`main.tsx`、`Plot.tsx`（軸）、`Toolbar.tsx`・`overlay.ts`・`NetworkView.tsx`・`SeriesPanel.tsx`（直書きの色をトークンへ）、`scripts/shots.mjs`（前後比較のスクリーンショットとフレーム時間）。
- デスクトップ: 新規 `theme.rs`、`tokens.rs`（Chrome）、`app.rs`（起動時に適用）、`ui.rs`（操作パネルの地）、`panels/control.rs`（節のカード・発信ボタン）、`panels/toolbar.rs`、`panels/network.rs`・`gpu/mod.rs`（輪・辺の色）、`panels/legend.rs`（横の格子線のみ）。

## 確認
- cargo test --workspace --release、clippy -D warnings、npm run build / npm test / e2e:align / e2e:p3 / e2e:media すべて通過。
- スクリーンショット（変更前後、ライト/ダーク、通常/比較）で配置が同じことを確認。
- 性能: 上記 docs/15.3-notes.md の通り、悪化なし。

## つまずき
- egui の角丸 7px でチェックボックスが円に見えた → 部品は 4px に。
- ツールバーのボタンの枠が重かった → ツールバー内だけ枠なしに。
- 計算でダークのアクセント上の白文字が 2.3:1 → 暗い文字に変更。

## 限界・次
- SVG 書き出しのグラフの配色（`logic/svg.ts`・`desktop/src/svg.rs`）は白地の資料用のまま。
- 実 GPU での Web のフレーム時間は未測定（ヘッドレスは swiftshader）。
