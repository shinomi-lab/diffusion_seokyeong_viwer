# 2026-10-20 拡張機構の整理（M1 のみ・M5 はコメントアウト）

## 依頼と決定
- 根拠文献と本研究の意思決定モデルを照合した結果、ユーザーが判断: M1 は残す、M5 はコメントアウトして今後検討、ほか（M2・M3・M4）は廃止。M4 は忘却・投稿予算・m∅ をすべて廃止（質問で確認）。

## 実施
- sim-api: config・params・node・engine・session・mc・log・export。テストを更新（M1 の検査、M5 無効時は経路数で更新が変わらないこと、旧設定の読み込み、`-` が不正な文字になること）。
- sim-wasm: シナリオ情報から budget/allow_rest を削除、ゴールデン再生成。
- Web: 機構は M1 のみ、m∅ ボタン・キー 0・投稿予算表示・計画ラウンド数（A・B）を削除。e2e:p3 の予算に関する確認を削除。
- デスクトップ: 同上。
- 文書: CLAUDE.md、15.3-notes、web/desktop の read_me、docs/p1-compare を更新。

## 確認
- cargo clippy（全ターゲット、-D warnings）、cargo test --workspace --release、wasm-pack test、npm test / build / e2e:align / e2e:p3 / e2e:media すべて通過。
- デスクトップの比較＋MC のスクリーンショットで、機構欄が M1 のみ、m∅ と計画ラウンド数が無いことを確認。

## 追記: 設計書・要件書の更新
- `viewer-design.md` を版 1.1 に（15.4 を新設。付録 B は当時の記録として注記のみ）。要件書は Claude Docs 側を更新（M2〜M4 の行を削除、M5 は検討中と付記、M1 のみ）。
- リポジトリ内の要件書 PDF は 2026-10-02 の写しのまま（古い）。

## 次
- M5 の再検討（複数到達をどう扱うか: 刺激の強化か、到達回数ぶんの更新か、ラウンドをまたぐ累積か）。
