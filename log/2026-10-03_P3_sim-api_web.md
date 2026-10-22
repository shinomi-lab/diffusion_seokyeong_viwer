# 開発ログ: P3（sim-api・sim-wasm・Web）巻き戻し・分岐 / MC 併記 / 動画・画像 / 2 画面比較

- 日付: 2026-10-03
- 段階: P3-1〜P3-4 の sim-api・sim-wasm・Web 版が完了（デスクトップ版は `log/2026-10-03_P3_desktop.md`）
- 前段階: `log/2026-10-03_fix-unreachable-block.md`
- ユーザー決定: P3-1〜P3-4 を Web・デスクトップ両方（F13 も両方）、P3-5（`verify`）は今回作らない
- 根拠: `viewer-design.md` 14 章 P3

## 作ったもの
- **sim-api**: 保存点（8 ラウンドごと）、`rewind_to`、`BranchTree`、`switch_to`、`load_tree`、`ever_flags_at`、`summary_at`。記録 JSON の `branch_tree` を実際の木に。
- **sim-wasm**: `rewindTo`・`switchTo`・`branchTree`・`roundData`・`importRecord`。
- **Web**: ストアのスロット化（A/B）と `actions.ts`、分岐ツリー（SVG）、MC の帯・実現値の帯判定、2 画面比較（B の機構・計画ラウンド数）、PNG・SVG・WebM の書き出し、記録の読み込み。

## 検証
- T-15（巻き戻しと分岐のビット一致）が通る（機構なし/あり、保存点の境界 8・16 の前後）。`cargo test --workspace --release` 117 件、`wasm-pack test`、`cargo clippy`（sim-api・sim-wasm・desktop）、`npm test` 19 件が通る。
- 実ブラウザ（puppeteer + Chrome）: `npm run e2e:p3`（巻き戻し → 分岐 → 元の枝へ戻ると 5 ラウンドの要約が元とビット単位で一致、MC、比較）、`npm run e2e:media`（PNG・SVG・WebM。ffprobe で VP9・約 50 フレーム。フレームを取り出して目視）、`npm run e2e:align`（位置の一致は崩れていない）。スクリーンショットで比較画面を目視。

## つまずきと対処
- 比較モードのチェックボックスが折りたたまれた領域に隠れて押せなかった → 最初から開いた状態に。
- B に「既定」を適用すると、投稿予算（T=5 の 3 回）で A の選択を再現できず、A と B がずれた → 発信を A・B でそろえて行う（片方が失敗したら取り消す）、B の計画ラウンド数の入力、再現できない理由を B に表示。e2e で確認。
- MediaRecorder の WebM は長さ（duration）を持たず ffprobe の長さが取れない → フレーム数で確認する方式に。
- `niceTicks` が範囲の上端を覆わない（1.5 までで 1.73 を含まない）バグを単体テストで発見して修正（1 を超える効用が切れないことが SVG でも成り立つ）。

## 限界・未実施
- P3-5（`verify`）は未実施（必要になった時点で方式を相談）。
- WebM に長さの情報が無い。比較モードで Twitter 規模の Web 表示はメモリが 2 倍（全体図のみ確認）。

## 次にやること
`log/2026-10-03_P3_desktop.md` のデスクトップ版の結果を参照。ユーザーが実際に触っての不具合報告を受けて修正。
