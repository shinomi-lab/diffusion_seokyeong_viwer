# 情報拡散ライブビューア（独立ワークスペース）

- 設計書: `viewer-design.md`、要件書: `情報拡散ライブビューア 企画・要件定義書.pdf`。作業前に設計書の 0 章・3 章（不変条件）・14 章（タスク表）を読む。実装メモ・設計書との差分は `docs/15.3-notes.md`。
- `core/` `runner/` `util/` は `diffusion_seokyeong` からのコピー。**改変しない**（ビット一致の基準点）。`cargo fmt --all` も使わない（`cargo fmt -p sim-api`）。
- 乱数は `sim-api` では `SimRng`（Xoshiro256++）だけ。`SmallRng` 禁止。辺ごとに乱数を必ず 1 回引く（`&&` で短絡しない）。
- 新しい crate は `core` を `sim-core = { package = "core", path = "../core" }` と名前を変えて依存する。
- 拡張機構は M1（購入者の口コミ）だけ。M2〜M4 は廃止、M5 は検討中でコメントアウト（2026-10-20）。M1 を無効にしたとき `core::diffusion` とビット一致（T-01・T-02・T-07: `sim-api/tests/parity_core.rs`）。
- 仕様が足りないときは既定案で進め `// 仮定:` を付け、`docs/15.3-notes.md` に追記する。
- 完了ごとに: `cargo fmt -p sim-api -p sim-wasm && cargo clippy -p sim-api -p sim-wasm --no-deps -- -D warnings && cargo test --workspace`
- コメントと UI の文言は日本語、識別子は英語。
- WASM: `wasm-pack test --node sim-wasm`（T-17。ゴールデンは `cargo run --release --example make_golden -p sim-wasm` で再生成）、`wasm-pack build sim-wasm --target web --release --out-dir ../web/src/wasm`。
- Web: `cd web && npm run wasm`（sim-wasm を web/src/wasm に出力）→ `npm run dev`（`?sample=1` で test/100random.txt を自動読み込み）/ `npm run build` / `npm test`（vitest。wasm の照合を含む）/ `npm run compare`（3 水準の記録を docs/p1-compare/ に書く）。
- 色・寸法は `spec/ui-tokens.json` だけに置く（Web・デスクトップ共通）。
- Desktop: `cargo run --release -p desktop`（`--graph PATH --ip N --demo --screenshot out.png --bench N`）、`cargo test -p desktop`（実 GPU でのピクセル検証を含む）、`cargo clippy -p desktop --no-deps -- -D warnings`。eframe 0.33 / egui_plot 0.34（rustc 1.91 nightly の制約。最新の 0.36 は rustc 1.95 が必要）。ワールド→画面→NDC の変換は `desktop/src/camera.rs` の 1 か所だけ。
- 推薦（`recommender-design.md`）: `sim-api/src/recommend.rs`、`ScenarioConfig.recommender`（既定 off。off で既存とビット一致）。v1・v2 の 1 ラウンド = フォロー経由の連鎖 → 推薦（スコア ≥ θ の未受信者）→ 推薦からの連鎖。方式は `RecMethod::{Baseline（v1）, Hybrid（v2: アイテムベース CF＋社会的証明、`recommender-v2-design.md`）, Wave（v3: Rモデル、`recommender-v3-design.md`）}`。`method` キーの無い記録は Baseline。v1・v2 は乱数を引かない。**Wave は ① の各ウェーブの `apply_wave` のあとに押し出し、推薦専用の `rec_rng` だけを引く**（拡散の乱数は引かない。R3 の a_t は `Session.rec_a` と保存点に持つ）。参照実装は `../diffusion_seokyeong`（core 0.3.0）で、一致は統計的に確認（`sim-api/examples/wave_crosscheck.rs` ↔ `runner/examples/rec_crosscheck.rs`）。確認: `cargo test -p sim-api --test recommend`、ゴールデン `golden_rec.json`・`golden_hybrid.json`・`golden_wave.json`、`cd web && npm run e2e:rec`、デスクトップ `--demo --rec 0.2 [--rec-baseline]`・`--demo --rec-wave 1.0`。
- P3: 巻き戻し・分岐は `sim-api` の `Session::{rewind_to, switch_to, tree_view}`（保存点 = 8 ラウンドごと。T-15: `sim-api/tests/rewind.rs`）。Web・デスクトップとも比較（F13）は「スロット」（A=0 / B=1）で別々の `Simulator` / `Slot` を持つ。確認: `cd web && npm run e2e:p3 && npm run e2e:media`、デスクトップは `--demo --branch / --compare / --record DIR`。
