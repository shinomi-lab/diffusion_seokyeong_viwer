# 2026-10-22 推薦 v2（協調フィルタリング＋グラフベース）の実装

## 依頼と決定
- 「推薦はアルゴリズムとしてどちらなのか」→ v1 はベースライン予測で、CF でもグラフベースでもないと回答。ユーザーの依頼で要件書を v2 に改訂（組み合わせ = 重み付きの和、CF = アイテムベース、グラフ = 社会的証明、v1 は基準として残す）。
- 設計書 `recommender-v2-design.md` を作成し、「設計の元に再実装」の依頼で V-1〜V-8 を実装。

## 実施
- sim-api: `RecMethod`・`alpha`・`min_common`（V-1）、`item_sim`・`estimate_cf`・`graph_score`（V-2）、① で社会的証明を数え `select` を方式で分ける（V-3）、`rec_detail` の f_cf・f_g と `rec_sim`（V-4）。
- sim-wasm: `recSim`、`golden_hybrid.json`（V-5）。
- Web: 方式の選択、似かたの表 `RecSimTable`、ノード詳細の列、`formFromConfig` の旧記録の読み方、e2e の確認を追加（V-6）。
- デスクトップ: 方式の ComboBox、似かたのグリッド（ラウンド数で保持）、詳細の列（見出しを短く）、`--rec-baseline`（V-7）。
- 文書: 15.3-notes、read_me 2 つ、CLAUDE.md、設計書の版、説明 PDF に v2 のページ（V-8）。

## 確認
- `cargo clippy`（sim-api・sim-wasm・desktop、全ターゲット、-D warnings）、`cargo test --workspace --release`（139 件）、`wasm-pack test --node sim-wasm`（off・balanced・rec・hybrid）。
- 旧ゴールデン 3 つは空の `rec_sim` 以外ビット一致。
- `npm test`・`npm run build`・`e2e:rec`・`e2e:align`・`e2e:p3`・`e2e:media`。
- デスクトップ `--demo --rec 0.2 --sequence 013051 --screenshot docs/rec-shots/desktop_hybrid.png` を目視。
- Twitter 規模: 基準 12.6 ms、CF＋グラフ 15.0 ms／ラウンド。

## 次
- θ・α・λ・c の既定を比較で決める。f_CF と f_G の分布を見て α を調整する。
