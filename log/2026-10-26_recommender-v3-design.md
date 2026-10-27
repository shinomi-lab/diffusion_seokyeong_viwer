# 2026-10-26 推薦 v3（Rモデル：波ごとの押し出し推薦）の設計書の作成

## 依頼と決定
- 依頼: 推薦チャネル（Rモデル、`../reccomend_system/推薦システム単純.md` の3節）を diffusion_seokyeong の Rust に入れ、ビューアにも入れるための設計仕様を作る（ビューアは今回は設計だけ）。
- 決定（設計書 12 章）: 3つ目の方式 `RecMethod::Wave`（D-W1）、① の各ウェーブの `apply_wave` のあとに推薦を入れる（D-W2）、推薦の抽選は `rec_rng` から引く（D-W3）、起点と1歩目は出辺1回の乱数でまとめる（D-W4、参照実装と同じ）、新規の既定方式は Hybrid のまま（D-W5）。

## 実施
- `recommender-v3-design.md` を作成（v2 と同じ構成: 0 章 AI への指示 〜 12 章 決定事項。ID は INV-W1〜W6・T-W01〜T-W10・W-1〜W-8・D-W1〜W6・U-W1〜W4）。
- 既存コードの事実を確認して 4 章に書いた: 差し込み位置は `engine.rs` の `cascade` の `apply_wave` と `swap` のあいだ、`rec_rng` は `Session` と保存点に持っているがどこからも引いていない、MC（`mc.rs`）には `rec_rng` が無い。
- `docs/15.3-notes.md` に「推薦 v3」の節を追加。

## 確認
- コードは変更していないので、テストは実行していない。
- 参照実装（`diffusion_seokyeong`）側は実装済みで、`cargo test --workspace` 32件成功、推薦 off で既存の parquet とビット一致（`../diffusion_seokyeong/log/2026-10-26_recommender.md`）。

## 気づいたこと
- `docs/15.3-notes.md` の「推薦による受信の記録（2026-10-22）」の節で、本文が2回繰り返されている（既存の重複。今回は触っていない）。
- v1 の閾値 θ（`rec.threshold`）と、Rモデルの「近くへの推薦の確率 θ」は記号がぶつかる。設計書では後者を `rec.near`（画面は「近くへの推薦 θ」）とした。

## 次
- W-1 から順に実装する。最後の W-8 で、参照実装との統計的な一致（T-W09）を確かめる。
