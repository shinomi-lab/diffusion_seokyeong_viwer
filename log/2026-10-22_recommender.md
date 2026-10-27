# 2026-10-22 推薦の実装（recommender-design.md R-1〜R-8）

## 依頼と決定
- 「この通りに実装してください」: `recommender-design.md` を Web 版・デスクトップ版の両方に実装する。配置とデザインは変えず、追加は操作パネルの「推薦」の節・色の縁取り・時系列の 2 系列・ノード詳細の表だけ。
- 決定済みの方針（設計書 13 章）: アイテム = メッセージの種類、ラウンド 1 から on/off 可、目的 = シェアと外部行動の平均（w_s 0.5）、推薦は連鎖のあと、閾値 θ 以上の全員、受信は色だけで示す、off では履歴を更新しない、推薦の乱数は任意の固定の塩（v1 では引かない）。

## 実施
- sim-api: `RecommenderConfig`・登録簿（R-1）、履歴 R と `recommend.rs`（R-2）、`run_round_rec`（① → ② → ③、`via`、R-3）、セッション・ログ・MC・CSV・バッチの拒否（R-4）。テスト `tests/recommend.rs`（T-R04〜R08）、単体（T-R02・R03）。
- sim-wasm: `recParamSpecs`、`nodeDetail` に `rec`・`received[].via`、ゴールデン `golden_rec.json`（R-5）。
- Web: `RecommenderEditor`（A・B）、`stateAtWave` の `via`、シェーダの縁取り（pulse の bit1・bit2）、凡例、時系列 2 系列、ノード詳細、`e2e:rec`（R-6）。
- デスクトップ: 操作パネル（A・B）、`state_at_wave` の `via`、`nodes.wgsl` の縁取り、凡例、時系列・SVG、ノード詳細、Parquet の `via` 列、`--rec THETA`、GPU の画素テスト（R-7）。
- 色: `#C2711A`（橙）を色覚シミュレーションで選定。太さは px。
- 文書: 15.3-notes、web/desktop の read_me、CLAUDE.md に 1 行、設計書の版の行（R-8）。

## 直した不具合
- Web: 凡例でフックを `||` で短絡させ、推薦 on で画面全体が落ちた → 両方を毎回呼ぶ。
- 縁取りが半径比 0.4 だと小さいノードで縁のアンチエイリアスに隠れた → px 指定にし、ノードの縁を縁取りの上に重ねる。
- デスクトップの Parquet 書き出しが `from = u32::MAX` でパニックする → `from_label = -1`。

## 確認
- `cargo fmt -p sim-api -p sim-wasm`、`cargo clippy -p sim-api -p sim-wasm -p desktop --all-targets --no-deps -- -D warnings`、`cargo test --workspace --release`（131 件）、`wasm-pack test --node sim-wasm`（off・balanced・rec）。
- ゴールデン off・balanced は、追加の 0 の項目を除いて再生成前とビット一致（推薦 off で既存と同じ）。
- `npm test`（20 件）・`npm run build`・`e2e:rec`・`e2e:align`・`e2e:p3`・`e2e:media` すべて通過。
- デスクトップ `--demo --rec 0.05 --sequence 00000 --screenshot`（`docs/rec-shots/desktop_rec.png`）、Web（`docs/rec-shots/rec_network.png`）で縁取り・凡例・系列・表を目視確認。
- T-R11: Twitter（81,306 ノード）で 1 ラウンド 12〜15 ms、推薦の段階による遅れは見えない。

## 次
- θ の既定（仮置き 0.15）を 3 水準の比較で決める（U-R1）。初期は全員が全体の率だけで同じスコアになり、θ をまたぐと一斉に推薦される点の扱い（v2 の確率的な配信で `rec_rng` を使う案）。
