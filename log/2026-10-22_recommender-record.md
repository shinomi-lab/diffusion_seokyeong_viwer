# 2026-10-22 推薦による受信の記録

## 依頼と決定
- 「推薦による受信も記録できるようにしてください」。既に CSV・Parquet の via 列、要約の推薦 5 値、ノード詳細の受信元「推薦」はあったので、何を足すかを確認。ユーザーの選択: 色を累積で残す・推薦で受信した人数の系列・MC と分岐ツリーにも残す。

## 実施
- sim-api: `EVER_REC`（ノードの累積の印）、ログ bit7 を `F_EVER_REC` に、`ever_flags` のマスク、`RoundSummary.cum_rec`、CSV の `cum_rec`、MC の `new_rec`・`cum_rec` の帯、分岐ツリーの節に推薦の人数。
- Web: `stateAtWave` の累積、再生の「推薦: 累積／このラウンドのみ」、時系列 2 系列と MC の帯、ツリーのツールチップ、ノード詳細の「推薦で受け取ったラウンド」。
- デスクトップ: 同上（`state_at_wave` の引数、`View.rec_cumulative`、グラフ・SVG・ツリー・詳細）。

## 確認
- clippy（sim-api・sim-wasm・desktop）、`cargo test --workspace --release`（140 件）、`wasm-pack test --node sim-wasm`、`npm test`（21 件）・`build`・`e2e:rec`・`e2e:align`・`e2e:p3`・`e2e:media`。
- ゴールデンは新しい項目以外変わらないことを確認して再生成。
- デスクトップ `--demo --rec 0.15 --sequence 01305120 --mc 50 --screenshot docs/rec-shots/desktop_record.png` を目視。
