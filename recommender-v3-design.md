# 推薦付きライブビューア 設計書 v3（Rモデル：波ごとの押し出し推薦、Claude Code 実装用）

| 項目 | 内容 |
| --- | --- |
| 版 | 3.0（2026-10-26）。**W-1〜W-8 実装済み（2026-10-26）**。実装時の仮定・設計からの変更・実測は `docs/15.3-notes.md` の「推薦 v3」節 |
| 位置づけ | `recommender-design.md`（v1）・`recommender-v2-design.md`（v2）の**追補**。本書に書いていないことは v1・v2 の設計書と `viewer-design.md` 1.1 に従う |
| 元資料 | `../reccomend_system/推薦システム単純.md` の3節（数理モデルの定義。記号はそちらに従う） |
| 参照実装 | `../diffusion_seokyeong/core/src/{recommend.rs,diffusion.rs}`（core 0.3.0、2026-10-26 実装。仕様は `../diffusion_seokyeong/研究リソースとか/推薦チャネル_実装仕様.md`） |
| 対象 | `sim-api`・`sim-wasm`・Web 版・デスクトップ版の**両方** |
| 方針 | 3つ目の方式「Rモデル」（`wave`）を足す。v1・v2 と違い、**推薦は各ウェーブのフォロー経由の処理のあとに入り、推薦専用の乱数 `rec_rng` を引く**。方式「基準」「CF＋グラフ」と推薦 off は、今の実装と完全に同じ結果のまま |

---

## 0. AI への指示（最初に読む）
1. `viewer-design.md` の 3 章（INV-01〜15）、`recommender-design.md` の 3 章（INV-R1〜R5）、`recommender-v2-design.md` の 3 章（INV-V1〜V5）、本書の 3 章（INV-W1〜W6）を読んでから作業する。
2. `core/` `runner/` `util/` は変えない（`diffusion_seokyeong` の新しい版をコピーし直すこともしない。ビット一致の基準点を動かさないため）。`cargo fmt --all` を使わない（`cargo fmt -p <crate>`）。
3. **推薦 off・方式「基準」・方式「CF＋グラフ」は、今の実装と完全に同じ結果**にする。既存のテストとゴールデン（`golden_off`・`golden_balanced`・`golden_rec`・`golden_hybrid`）は変えずに通す。
4. 1 回の作業は 11 章の 1 タスク。完了条件のコマンドがすべて通ってから次へ進む。
5. 仕様が足りないときは既定案で進め、`// 仮定:` を付け、`docs/15.3-notes.md` に追記する。
6. コメントと UI の文言は日本語、識別子は英語。完了ごとに `log/` に開発ログを新規作成する。

---

## 1. 要約
- 方式「Rモデル」（`wave`）では、各ウェーブ s で、フォロー経由の受信と状態の更新（`apply_wave`）が終わったあとに、次の数だけフォロー外へ届ける。

```latex
B_s = \varepsilon\,\delta\,\varphi\!\left(\frac{\hat e_s}{\bar e_0}\right) p \sum_{i\in S_s} d_i,
\qquad
\hat e_s = \frac{\bar H + k\,a_t}{\bar C + k},
\qquad
\varphi(x) = \begin{cases} 0 & (x<\tau) \\ \min\{x,\varphi_{\max}\} & (x\ge\tau) \end{cases}
```

| 部分 | 計算 | 使う情報 |
| --- | --- | --- |
| 反応率 ê | ベータ–二項分布の事後平均。分子はこのラウンドでシェアを決めた人数（フォロワー 0 人も含む）、分母は受信した人数。推薦で届いた人も数える。IP は数えない | このラウンドのここまでの受信 |
| 基準 ē0 | 平均的なメッセージを初めて受け取ったときにシェアする人の割合 | 初期状態（`w_int`・`u_int`）と平均内部刺激 |
| 推薦量 B_s → 枠の数 n_s | ⌊B_s⌋ に、小数部分を確率とする 1 回の抽選を足す | `rec_rng` |
| 宛先 | 確率 θ で「シェア者の出辺から 1 本を一様に選び、その先の後続を 1 人一様に選ぶ」2 歩のウォーク、確率 1−θ で全ノードから一様に選ぶ。受信済み（IP を含む）・行き止まりは空振り | グラフ、`rec_rng` |
| R3 | ラウンドを跨いで a_{t+1} = μ a_t + (1−μ) ê_end（a_1 = ē0） | 前のラウンドの ê |

- 推薦で届いた人は、フォロー経由と**同じ** `NodeState::contact` で受信する（更新則を変えない）。シェアを決め、後続を持てば次のウェーブに入り、その先は推薦由来の連鎖（`VIA_RECCHAIN`）になる。
- v1・v2 の「② 推薦の段階 → ③ 推薦からの連鎖」は、方式 `wave` では行わない。履歴 R も使わない。

---

## 2. 用語
| 用語 | 意味 | コード上の名前 |
| --- | --- | --- |
| 方式 | 推薦のスコアの計算方法。基準（v1）・CF＋グラフ（v2）・Rモデル（v3） | `RecMethod::{Baseline, Hybrid, Wave}` |
| 推薦の強さ ε | 平均的な反応のメッセージで、フォロー外への到達がフォロー経由の何倍になるか | `rec.eps` |
| 露出の割引 δ | フォロー外の投稿をどれだけ割り引いて表示するか（R4）。単一アイテムでは ε との積だけが効く | `rec.delta` |
| 近くへの推薦の確率 θ_near | 2 歩のウォークで宛先を選ぶ確率。**v1 の閾値 θ（`rec.threshold`）とは別物**なので、画面では「近くへの推薦 θ」と書く | `rec.near` |
| しきい値 τ | ê が基準の τ 倍に届かなければ推薦しない（オーディション） | `rec.tau` |
| 上限 φ_max | 推薦量の倍率の上限 | `rec.phi_max` |
| 擬似カウント k | 事前分布の強さ | `rec.k` |
| 記憶の持続 μ | R3。1 で記憶なし | `rec.mu` |
| 空振り | 宛先が受信済み・IP・行き止まりで、届かなかった枠 | `RoundCounts.rec_miss` |

---

## 3. 不変条件（追加）
| ID | 規則 | 理由 | 検査 |
| --- | --- | --- | --- |
| INV-W1 | 推薦 off・方式「基準」「CF＋グラフ」は、今の実装と乱数の位置・結果までビット一致 | 比較の基準を壊さない | 既存テスト全件、ゴールデン 4 つ（T-W01） |
| INV-W2 | 方式 `wave` は、**拡散の乱数 `rng` を引かない**。推薦の抽選はすべて `rec_rng` から引く。したがって ① のフォロー経由の抽選の位置は、推薦の有無で変わらない（INV-R2 を「拡散の乱数を引かない」に読み替える） | 推薦 on/off で拡散の乱数がそろう。比較しやすい | T-W04 |
| INV-W3 | `rec_rng` を引く順序を固定する: 各ウェーブで「丸め 1 回 → 枠ごとに θ の判定 → (ウォーク: 出辺 → 2 歩目 | 一様: 宛先)」。1 歩目の先が行き止まりなら 2 歩目は引かない | WASM とネイティブ、巻き戻しで同じ結果 | T-W06・T-W07 |
| INV-W4 | ε δ = 0 なら推薦の分岐に入らず、`rec_rng` も引かない | 方式 `wave` で ε = 0 は推薦 off と同じ結果 | T-W02 |
| INV-W5 | 計算は四則演算・比較・`min`・`floor` だけ（INV-R3・INV-V3 と同じ）。和はノード番号ではなく**ウェーブのシェア者の処理順**で取る（`curr` を控えた順） | WASM とネイティブで一致 | T-W07 |
| INV-W6 | R3 の a_t はラウンドを跨ぐ状態なので、保存点・巻き戻し・分岐・MC の各試行で正しく持ち回す | 巻き戻しのビット一致 | T-W06 |

---

## 4. 既存コードの事実（2026-10-26 時点で確認）
| 事実 | 場所 | v3 での使い方 |
| --- | --- | --- |
| 1 ラウンドは `run_round_rec` で「① `cascade` → ② `select` → ③ `cascade`」 | `sim-api/src/engine.rs:125-197` | 方式 `wave` では ① の `cascade` の中で推薦し、② ③ を飛ばす |
| `cascade` はウェーブごとに `curr` を後入れ先出しで処理し、後続は昇順、辺ごとに乱数を 1 回引く（INV-04・05）。ウェーブの終わりに `apply_wave` → `swap` | `engine.rs:201-256` | **推薦の差し込み位置は `apply_wave` のあと、`swap` の前**（:250-252） |
| `apply_wave` は `sc.newly` の順に `contact` し、`enqueued = int_fired && out_degree > 0` で `sc.next` に入れる。経路は `sc.via`、`RoundCounts.{rec_*, chain_*}` で数える | `engine.rs:258-320` | 推薦で届いた人を `newly` に `(j, FROM_REC)` で入れ、`via = VIA_REC` にしてもう一度呼ぶ。`int_fired` の数（ê の分子）を返せるようにする |
| `rec_rng(seed) = SimRng::seed_from_u64(seed ^ REC_SALT)`。`Session` と保存点に持っているが、**どこからも引いていない** | `recommend.rs:15-20`、`session.rs:84,109,134,329,402` | 初めて引く。`run_round_rec` に `rec_rng: &mut SimRng` を足す |
| MC は `mc.rs` で試行ごとに `run_round_rec` を呼ぶ。`rec_rng` は無い | `sim-api/src/mc.rs:95-117` | 試行ごとに `rec_rng` を作る（6.4） |
| `Csr::succ(i)` は昇順の後続、`out_degree(i)`、`n_nodes()` | `sim-api/src/csr.rs:38-60` | ウォークと出次数 |
| `NodeState { w_int, u_int, … }`、更新は `u ← w·u + (1−w)·s` | `sim-api/src/node.rs:17-52` | ē0 の計算 |
| 登録簿 `rec_param_specs()`・`get_rec_value()`、`RecommenderConfig::validate` は登録簿の範囲で検査 | `params.rs:86-`、`config.rs:270-` | 項目を足す |
| 方式の選択 `RecMethod`（`lowercase`、キーが無ければ Baseline） | `config.rs:220-235` | `Wave` を足す |

---

## 5. 設定（`config.rs`・`params.rs`）

```rust
#[serde(rename_all = "lowercase")]
pub enum RecMethod { Baseline, Hybrid, Wave }

pub struct RecommenderConfig {
    // 既存: enabled, method, threshold, w_share, prior_n0, alpha, min_common
    pub eps: f64,       // 1.0（方式 wave の既定。0 で推薦なし）
    pub delta: f64,     // 1.0
    pub near: f64,      // 0.5（θ_near）
    pub tau: f64,       // 0.0
    pub phi_max: f64,   // 3.0
    pub k: f64,         // 10.0
    pub mu: f64,        // 1.0
}
```

- **互換性**: 新しいキーは `#[serde(default)]` で既定値になるので、v1・v2 の記録はそのまま読める（方式も変わらない）。新規作成の既定の方式は Hybrid のまま（D-W5）。
- 登録簿に 7 行を足す（`mechanism = "推薦"`）:

| id | ラベル | 範囲 | 刻み | 既定 | 無効値 |
| --- | --- | --- | --- | --- | --- |
| `rec.eps` | 推薦の強さ ε | 0〜5 | 0.1 | 1.0 | 0 |
| `rec.delta` | フォロー外の露出の割引 δ | 0〜1 | 0.05 | 1.0 | 1 |
| `rec.near` | 近くへの推薦 θ（2 歩先へ配る確率） | 0〜1 | 0.05 | 0.5 | — |
| `rec.tau` | しきい値 τ（基準の何倍で推薦するか） | 0〜3 | 0.1 | 0 | 0 |
| `rec.phi_max` | 倍率の上限 φ_max | 1〜10 | 0.5 | 3 | — |
| `rec.k` | 擬似カウント k（何人分様子を見るか） | 1〜100 | 1 | 10 | — |
| `rec.mu` | アカウント記憶 μ（1 で記憶なし） | 0〜1 | 0.05 | 1 | 1 |

- 登録簿の範囲検査に加えて、`phi_max >= tau` を `RecommenderConfig::validate` で検査する。
- 方式ごとに使う項目: 基準 = `threshold・w_share・prior_n0`、CF＋グラフ = それに `alpha・min_common`、Rモデル = `eps・delta・near・tau・phi_max・k・mu`。使わない項目は画面で無効表示。

---

## 6. sim-api の設計

### 6.1 推薦モジュール（`recommend.rs` に追加）

```rust
/// 基準の反応率 ē0（元資料 3.1 節）。初期状態の w_int・u_int と平均内部刺激から。ちょうど 0 は「決めない」。
pub fn base_share_rate(initial: &Population, mean_internal: f64) -> f64;

/// φ(x)。
pub fn phi(cfg: &RecommenderConfig, x: f64) -> f64;

/// 2 歩のウォーク。sharers はこのウェーブのシェア者（処理順）、deg_cum はその出次数の累積和。
/// 出辺を random_range(0..Σd) で 1 本選び、行き先 l の後続を random_range(0..d_l) で選ぶ。l が行き止まりなら None。
pub fn two_step_walk(csr: &Csr, sharers: &[u32], deg_cum: &[u32], rec_rng: &mut SimRng) -> Option<u32>;
```

- 参照実装（`diffusion_seokyeong/core/src/recommend.rs`）と同じ手順にする。乱数の型が違う（`SmallRng` と `SimRng`）ので、結果はビット一致しない（T-W09 で統計的に比べる）。
- // 仮定: ē0 = 0 なら推薦しない（参照実装と同じ）。

### 6.2 エンジン（`engine.rs`）
- `run_round_rec` に `rec_rng: &mut SimRng` と `a_t: f64` を足し、戻り値の `RoundCounts` に `rec_miss: u32` と `e_hat_end: f64` を足す。
- `Scratch` に `sharers: Vec<u32>`・`deg_cum: Vec<u32>`・`cum_h: u32`・`cum_c: u32` を足す（ラウンドの始めに `cum_h = cum_c = 0`）。
- `cascade` に `wave_rec: Option<WaveRec>` を足す（`WaveRec { cfg, ebar0, a_t }`。方式 `wave` かつ ε δ > 0 のときだけ `Some`）:
  1. ウェーブの先頭で `sharers ← curr`（`pop` で空になる前に控える）。
  2. いつもどおり辺の抽選と `apply_wave`。`apply_wave` は `int_fired` の数を返し、`cum_h += int_fired の数`、`cum_c += newly.len()`。
  3. `wave_rec` があれば、ê_s・B_s・n_s を計算し、枠ごとに宛先を選ぶ。届いた人は `rcvd = true`、`recv_wave = wave`、`arr = 1`、`via = VIA_REC`、`newly.push((j, FROM_REC))`。全枠のあとに `apply_wave` をもう一度呼ぶ（`cum_h`・`cum_c` に加える）。
  4. `swap` して次のウェーブへ。推薦で届いてシェアした人の先は、既存の規則どおり `VIA_RECCHAIN` になる。
- 方式 `wave` では ② `select` と ③ の `cascade` を呼ばない。`Baseline`・`Hybrid` の経路は 1 行も変えない（INV-W1）。
- ラウンドの終わりに `e_hat_end = (cum_h + k·a_t)/(cum_c + k)`。
- 同じウェーブでの 2 本目以降の命中（`arr` の加算）は、推薦で届いた人には起きない（推薦の受信は 1 回だけ、`arr = 1`）。
- 計算量: ウェーブごとに O(|S_s|)（累積和）＋ O(n_s · log|S_s| + n_s · d)（ウォークの `succ` は配列なので添字で引ける）。

### 6.3 セッション（`session.rs`）
- `Session` に `rec_a: f64`（R3 の a_t）と `ebar0: f64` を足す。ē0 はシナリオの初期状態から 1 回だけ計算する。a_1 = ē0。
- `step` は方式 `wave` のとき `run_round_rec(…, &mut self.rec_rng, self.rec_a, …)` を呼び、終わったら `rec_a ← μ·rec_a + (1−μ)·e_hat_end`。
- 保存点（`Checkpoint`）に `rec_a` を足す（`rec_rng` は既に入っている）。巻き戻し・分岐で復元する（INV-W6）。
- `rec_detail(node)` は方式 `wave` では空（個人のスコアが無いため）。代わりに `rec_wave_trace()`（直近のラウンドのウェーブごとの ê_s・B_s・n_s・空振り）を返す。表示用で、保存はしない（直近ラウンドを数えるときに `Scratch` に溜める）。

### 6.4 ログ・記録・MC
- ~~`RoundSummary` に `rec_miss`・`e_hat_end` を足す。ラウンド別 CSV に同じ 2 列を足す~~ → **実装では足さなかった**（要約はゴールデンに記録されるので、足すと既存のゴールデン 4 つが変わるため）。`RoundCounts` には持ち、表示は `rec_wave_trace` で行う（2026-10-26）。イベント・ログのフラグは v1 のまま（`EV_REC`・`EV_RECCHAIN`・`EVER_REC` をそのまま使う）。
- MC（`mc.rs`）: 試行ごとに `rec_rng = rec_rng(trial_seed)`、`a = ē0` を作って `run_round_rec` に渡す。`trial_seed` は拡散の乱数と同じ派生のしかたにする（試行番号から決まる）。
- `batch_toml` は推薦 on なら v1 と同じく `BatchIncompatible`。**ただし方式 `wave` は `diffusion_seokyeong` の `[recommender]` と項目が対応するので、将来は書き出せる**（U-W3。今回は対象外）。

---

## 7. sim-wasm
| API | 変更 |
| --- | --- |
| `recParamSpecs` | `rec.eps`〜`rec.mu` の 7 項目が加わる |
| `roundSummary` | 変更なし（6.4 の変更を参照） |
| `recWaveTrace()`（新規） | 直近ラウンドのウェーブごとの `{e_hat, budget, slots, miss}` の配列。方式 wave 以外では空 |
| ゴールデン | `spec/fixtures/golden_wave.json` を追加（推薦 on・方式 wave・ε 1・θ_near 0.5・τ 0・μ 0.5 で R3 も通す）。既存のゴールデンは変えない |

---

## 8. Web 版（配置は不変、追加のみ）
- **型と既定**（`logic/config.ts`）: `RecMethod` に `'wave'`、`RecForm` に 7 項目。`DEFAULT_REC` の既定値は Rust と同じ。`formFromConfig` は欠けた項目を既定値で埋める。
- **操作パネル**（`panels/RecommenderEditor.tsx`）: 方式の選択に「Rモデル（波ごとの押し出し、v3）」を足す。スライダーは登録簿から自動生成し、方式に応じて使わない項目を無効にする（今の `HYBRID_ONLY` を、方式ごとの集合に一般化する）。
- **推薦の推移**: 「推薦」の欄の下に、`<details>` で閉じた小さな表（ウェーブ・ê・推薦量・枠・空振り）。発信・巻き戻しのたびに `recWaveTrace` を取り直す。方式 wave 以外では表示しない。
- **時系列**: 既存の「推薦で受信」の系列はそのまま使える。「空振り」は表示しない（CSV と推移の表だけ）。
- 縁取り・塗り・凡例は v1 から変更（2026-10-26）: 推薦で受信 = 黄色の塗り、推薦由来 = 暗い黄色の塗り（縁取りなし）。`docs/15.3-notes.md` の「推薦の色を黄色に」を参照。

## 9. デスクトップ版（Web と同じ方針）
- `panels/control.rs` の `recommender_editor`: 方式の `ComboBox` に「Rモデル」、スライダーは登録簿から（方式に応じて無効）。A・B の両方（比較で A = CF＋グラフ、B = Rモデルを並べられる）。
- 推薦の推移: 推薦の節の下に `CollapsingHeader`（既定で閉じる）の表。
- CLI: `--rec-wave EPS` を足し、方式 wave・ε = EPS で始められるようにする（確認用）。
- 縁取り・凡例・時系列・Parquet は v1 のまま（Parquet の列も足さない。6.4）。

---

## 10. テスト計画
| ID | 内容 | 場所 |
| --- | --- | --- |
| T-W01 | 推薦 off・基準・CF＋グラフで、既存の全テストとゴールデン 4 つが変更なしで通る | 既存 |
| T-W02 | 方式 wave で ε = 0（または δ = 0）なら、推薦 off と結果・拡散の乱数・`rec_rng` の状態まで一致する | `sim-api/tests/recommend.rs` |
| T-W03 | 小さなグラフ（フォロー経由では届かない部分を持つ）で、推薦で届く・受信済みと IP には届かない・空振りを数える・受信者数と外部行動数の整合（参照実装の S-03 と同じ形） | 同上 |
| T-W04 | 方式 wave でも、① のフォロー経由の抽選（`rng`）の位置は推薦 off と同じ（推薦は `rec_rng` だけを引く） | 同上 |
| T-W05 | `two_step_walk` の起点が出次数に比例する・行き止まりで None。`phi`・`base_share_rate` の手計算 | `recommend.rs` の単体 |
| T-W06 | 方式 wave で、巻き戻し・分岐・MC がビット一致（`rec_rng` と `rec_a` を含む。T-R07・T-R08 と同じ形） | `sim-api/tests/recommend.rs` |
| T-W07 | WASM とネイティブ・デスクトップの一致（`golden_wave.json`） | `sim-wasm/tests/golden.rs`、`desktop/src/golden_tests.rs` |
| T-W08 | 設定: 新しいキーが無い JSON は既定値で読め、方式は変わらない。範囲検査、`phi_max >= tau`。Web の `DEFAULT_REC`・`formFromConfig` が Rust と一致 | `sim-api/tests/config.rs`、`web/src/logic/wasm.test.ts` |
| T-W09 | **参照実装との統計的な一致**: congress（有向、`transposed`）、同じパラメータで、ラウンド 1 の受信者数・推薦経由の受信者数・外部行動数の MC 平均（各 2000 試行）が、`diffusion_seokyeong` の `monte_carlo_average` と標準誤差の 3 倍以内で一致する（M1 off） | `sim-api/examples/wave_crosscheck.rs`（参照実装の値は `diffusion_seokyeong` 側で出した CSV を読む） |
| T-W10 | 画面: Web は `e2e:rec` に、方式 wave の選択・スライダーの有効／無効・推薦の推移の表を追加。デスクトップは `--demo --rec-wave 1.0` のスクリーンショット。計算量は `rec_bench` に方式 wave を足して記録 | `web/scripts/check-rec.mjs`、`sim-api/examples/rec_bench.rs` |

---

## 11. タスク表（この順に進める）
| ID | 内容 | 参照 | 完了条件 |
| --- | --- | --- | --- |
| W-1 | 設定と登録簿: `RecMethod::Wave`、7 項目、互換の読み方、検証 | 5 章 | T-W08（Rust 側）、`cargo test -p sim-api` |
| W-2 | `base_share_rate`・`phi`・`two_step_walk` | 6.1 | T-W05 |
| W-3 | エンジン: `rec_rng`・`a_t` の受け渡し、`cascade` の波ごとのフック、`apply_wave` の `int_fired` の数、`RoundCounts.{rec_miss, e_hat_end}`、方式 wave で ② ③ を飛ばす | 6.2 | T-W01〜T-W04 |
| W-4 | セッションと MC: `rec_a`・`ebar0`、保存点、`rec_wave_trace`、MC の試行ごとの `rec_rng` | 6.3・6.4 | T-W06、`cargo test --workspace` |
| W-5 | sim-wasm とゴールデン `golden_wave` | 7 章 | T-W07、`wasm-pack test --node sim-wasm` |
| W-6 | Web | 8 章 | `npm run build && npm test && npm run e2e:rec && npm run e2e:p3`、T-W08（Web 側）・T-W10 |
| W-7 | デスクトップ | 9 章 | `cargo clippy -p desktop --all-targets --no-deps -- -D warnings && cargo test -p desktop`、スクリーンショット |
| W-8 | 参照実装との突き合わせ（T-W09）と文書: `docs/15.3-notes.md`、read_me 2 つ、`CLAUDE.md` の推薦の行、`log/`、計算量の実測 | 10 章 | T-W09 が通る、文書の更新 |

各タスクの終わりに: `cargo fmt -p sim-api -p sim-wasm && cargo clippy -p sim-api -p sim-wasm --all-targets --no-deps -- -D warnings && cargo test --workspace`

---

## 12. 決定事項と残る未決
| # | 内容 | 状態 |
| --- | --- | --- |
| D-W1 | 3 つ目の方式「Rモデル」（`wave`）として足し、v1・v2 は残す | 決定（本書） |
| D-W2 | 推薦は ① の `cascade` の各ウェーブの `apply_wave` のあとに入れる（元資料 3.4 節の (a)〜(e)） | 決定（本書） |
| D-W3 | 推薦の抽選は `rec_rng` から引き、拡散の乱数は引かない（INV-R2 の読み替え）。D-R8 の塩をそのまま使う | 決定（本書） |
| D-W4 | 起点と 1 歩目は、シェア者の出辺を 1 回の乱数で選んでまとめる（参照実装と同じ。分布は元資料と同じ） | 決定（2026-10-26、参照実装） |
| D-W5 | 新規作成の既定の方式は Hybrid のまま（v3 は比較用に選んで使う） | 決定（本書） |
| D-W6 | 推薦で届いた人も ê の分子・分母に数える。分子はフォロワー 0 人も含めてシェアを決めた人数 | 決定（2026-10-26、元資料） |
| U-W1 | 既定値 ε 1・θ_near 0.5・k 10・φ_max 3 | 仮置き。参照実装の実験（元資料 8 節）で決める |
| U-W2 | フォロワーのいない IP には推薦が付かない（B_0 = 0）。画面でそれと分かるようにするか（例: 推移の表に「IP のフォロワー 0 人」と出す） | 未決。ē0 = 0 の場合は推移の表に注意を出した（2026-10-26） |
| U-W3 | 方式 wave の設定を `batch_toml` で `diffusion_seokyeong` の `[recommender]` に書き出す | 未決（今回は対象外） |
| U-W4 | M1（購入者の口コミ）と方式 wave を同時に使うときの順序（M1 の受信も ê に数えるか） | 未決。実装時は「数える（受信は経路を区別しない）」を仮定とする |
