# 2026-10-26 推薦 v3（Rモデル：波ごとの押し出し推薦）の実装（W-1〜W-8）

## 依頼と決定
- 依頼: `recommender-v3-design.md` のとおりに実装する（同じ日の設計書作成のログは `2026-10-26_recommender-v3-design.md`）。
- 決定: 設計どおり。ただし次の 2 点を変えた（`docs/15.3-notes.md` の「推薦 v3」節）。
  - `RoundSummary`・CSV・Parquet に `rec_miss`・`e_hat_end` を足さない（既存のゴールデンを変えないため。推移は `recWaveTrace` で見る）。
  - ē0 = 0 のときは推移の表に「推薦しません」の注意を出す（100random の既定の母集団がこれに当たるため）。

## 実施
- W-1 設定: `RecMethod::Wave`、7 項目、`phi_max >= tau` の検査、登録簿、`params::rec_param_used`。
- W-2 部品: `recommend::{base_share_rate, phi, two_step_walk}`（範囲は u32 で引き、WASM と 64bit で同じ値にする）。
- W-3 エンジン: `run_round_rec` に `wave_ctx`、`cascade` の各ウェーブのあとに `recommend_wave`、`apply_wave` が内部の成立人数を返す、`RoundCounts.{rec_miss, e_hat_end}`。方式 wave は ② ③ を通らない。
- W-4 セッション・MC: `Scenario.ebar0`、`Session.rec_a`（保存点・巻き戻し）、`rec_wave_trace`・`rec_prior`、MC は試行ごとの `rec_rng`。
- W-5 WASM: `recWaveTrace`、ゴールデン `golden_wave.json`（R3 μ 0.5 を通す）。
- W-6 Web: 型・既定・`recParamUsed`、方式の選択肢、`RecWaveTable`（推薦の推移）、worker、`e2e:rec` に 6 項目。
- W-7 デスクトップ: 方式の選択肢、スライダー、推移の表、`--rec-wave EPS`。
- W-8: `sim-api/examples/wave_crosscheck.rs` と `../diffusion_seokyeong/runner/examples/rec_crosscheck.rs` で T-W09、`rec_bench` に方式 wave、文書（15.3-notes・read_me 2 つ・CLAUDE.md・設計書の状態）。

## 直した不具合
- デスクトップのウェーブの矢印が `FROM_REC`（u32::MAX）を添字に使ってパニック（v1 の `--rec` でも起きる既存の不具合）。推薦で受信した人は矢印を描かないようにした。

## 確認
- `cargo test --workspace` 150 件（基準 140 件 + 10）。clippy（sim-api・sim-wasm・desktop）警告なし。
- 既存のゴールデン 4 つは再生成してもバイト単位で同じ（INV-W1）。`wasm-pack test --node sim-wasm` 5 件。
- Web: `npm run build`・`npm test` 21 件・`npm run e2e:rec`（22 項目）・`npm run e2e:p3` すべて OK。
- デスクトップ: 38 件、`--graph spec/fixtures/random300.txt --demo --rec-wave 1.0` のスクリーンショット（`docs/rec-shots/desktop_wave.png`）。
- T-W09: 差は合成標準誤差の 0.54・0.09・0.43 倍（シェア・外部行動・推薦で受信）。ē0 は一致。
- T-W10: Twitter 規模で Rモデル 60.3 ms／ラウンド（off 11.5 ms）。推薦による受信がラウンドあたり約 4.3 万人と多いため。

## 次
- 既定値（ε・θ・k・φ_max）の決定（U-W1）。Rモデルは Twitter 規模で受信が爆発的に増えるので、ε の既定は 1 より小さくする方がよいかもしれない。
- フォロワーのいない IP の扱いの表示（U-W2）、方式 wave の `batch_toml` 書き出し（U-W3）。
