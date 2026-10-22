# 2026-10-03 デスクトップのグラフ固定・凡例分離、1 万ノード超の個別表示

## 依頼
- デスクトップの時系列グラフを Web 版のように固定し、凡例をグラフの下へ分離。
- 1 万ノード以上でもネットワークを表示できるように。

## 実施
- `panels/legend.rs` 新規（固定プロット・凡例行・トグル。単体テスト 2 件）。`charts.rs`・`detail.rs` を差し替え。
- `EXPAND_CAP` 200,000。`lod.rs` 新規（半径・視野内の辺の間引き。テスト 3 件）。`app.rs::refresh_view_edges`、`gpu::Uniforms::with_radius`。
- Web: `units.ts` の上限・`effectiveRadius`、`gl.ts`・`NetworkView.tsx` に反映。

## 確認
- cargo test --workspace --release / clippy -D warnings / wasm-pack test / npm test / npm run build / e2e:align すべて通過。
- 実機（M2）スクリーンショット: 比較＋MC で凡例が下に分離され、非表示指定の系列が消える。Twitter（81,306 ノード）が最初から個別表示。

## 限界・次
- Web は辺の視野内間引き未実装、実 GPU の fps 未測定。デスクトップの Twitter でのフレーム時間は未記録（前回計測は 95 fps）。
- 個別表示 ⇄ 全体図の常時切替ボタンは未追加（現状は IP 選択時にノード数で自動決定）。
