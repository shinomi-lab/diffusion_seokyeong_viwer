# 情報拡散ライブビューア 設計書（Claude Code 実装用）

| 項目 | 内容 |
| --- | --- |
| 版 | 1.1（2026-10-20。拡張機構を M1 のみに整理。15.4 参照。1.0 は 2026-10-02） |
| 読み手 | ターミナルの Claude Code。人間が読んでもよい |
| 元資料 | 「情報拡散ライブビューア 企画・要件定義書」（Claude Docs, rev 57）。要件 ID（F01〜F19、N01〜N09、M1〜M6）は同書のものをそのまま使う。**ただし M2〜M5 は 2026-10-20 に廃止・保留にした（15.4）。同書の 6.5・6.6 の M2〜M5 の記述は無効** |
| 位置づけ | 要件書は「何を作るか」、本書は「どう作るか」。両者が食い違うときは **15.1 の差分表 → 本書 → 要件書** の順に優先する |
| 検証状況 | 本書の ✔検証済み と書いた項目は、サンドボックスで実コードを動かして確かめた（付録 B）。それ以外は設計上の判断であり、実装時にテストで確かめること |

---

## 0. AI への指示（最初に読む）

1. 作業を始める前に、**3 章（不変条件）と 14 章（タスク表）**を読む。ほかの章は、取り組むタスクの「参照」列にある節だけ読めばよい。
2. 1 回の作業は 14 章の **1 タスク**とする。完了条件（DoD）のコマンドがすべて通ってから、次のタスクへ進む。
3. 仕様が足りない、または矛盾していると気づいたら、推測で実装しない。**既定の案を採り、コードに `// 仮定: …` を付け、15.3 に項目を追記**してからタスクを続ける。変更の影響が大きいとき（不変条件に触れるときなど）は、作業を止めて人間に質問する。
4. **既存の `core` / `runner` / `graph-lib` / `util` / `src` は変更しない**（INV-01）。例外は、ルート `Cargo.toml` の `members` への追加だけである。
5. 言語の規則は、コード内のコメントとドキュメントコメントを**日本語**、識別子を英語、UI の文言を日本語とする（既存コードに合わせる）。
6. コミットは 1 タスク 1 コミットを基本にする。メッセージは日本語でよい。
7. 毎回の完了時に次を実行する。

```bash
cargo fmt --all
cargo clippy -p sim-api -p sim-wasm -- -D warnings   # desktop は P2 以降に追加
cargo test --workspace
```

8. **最初のタスクの前に必ず確認すること**（この設計書を書いた時点でリポジトリのルートの `Cargo.toml` は見えていなかった）。

```bash
cat Cargo.toml                       # [workspace] members と [workspace.dependencies] の実際の内容
cat rust-toolchain.toml 2>/dev/null  # あれば固定バージョン
rustc --version                      # edition 2024 には 1.85 以上が必要
cargo tree -p runner --depth 1       # rand / rand_distr / toml の実バージョン
git -C graph-lib status 2>&1 | head  # graph-lib が git submodule / 別リポジトリか
ls test/                             # runner のテストが参照するグラフ（congress 等）の場所
```

確認の結果が本書の前提（4 章）と違うときは、本書ではなく確認の結果を正とし、15.3 に差分を書く。

---

## 1. 作るものの要約

- **目的**: 現行の Rust 拡散シミュレーションに拡張機構 M1（購入者の口コミ）を重ね、「IP を選ぶ → 用意したメッセージを選ぶ → 発信 → 伝播を再生して見る」を 1 ラウンドずつ手で回せる探索・可視化アプリを作る。
- **成果物は 2 つ**: Web 版（Rust→WASM＋TypeScript）とデスクトップ版（Rust＋egui）。計算の窓口 `sim-api` と、画面の仕様（11 章）を共有し、画面の実装は別々に行う。
- **やらない**: M2〜M4（廃止）、M5（検討中。コードはコメントアウト）、M6（適応方策）、系列の全探索、ログイン・サーバ、実 SNS への接続。
- **この版の最重要要件**: 拡張機構をすべて無効にしたとき、既存の Rust バッチ（`core::diffusion`）と**ビット単位で同じ結果**になること（N01）。

```
                 ┌────────────────────┐        ┌──────────────────────┐
                 │ web/ (TS+React)    │        │ desktop/ (eframe)    │
                 │ WebGL2 / uPlot     │        │ wgpu / egui_plot     │
                 └─────────┬──────────┘        └──────────┬───────────┘
        Worker メッセージ  │                              │ 関数の直接呼び出し
                 ┌─────────▼──────────┐                   │
                 │ sim-wasm (cdylib)  │                   │
                 └─────────┬──────────┘                   │
                           └──────────────┬────────────────┘
                                 ┌────────▼─────────┐
                                 │ sim-api (lib)    │  M1 / トレース / 状態ログ / MC
                                 └───┬────────┬─────┘
                       sim-core(=core)│        │graph-lib
                       ┌─────────────▼┐  ┌────▼────────┐
                       │ core（既存） │  │ graph-lib    │   runner / util も既存・変更しない
                       └──────────────┘  └─────────────┘
```

---

## 2. 用語集（本書の語とコードの対応）

| 用語 | 意味 | コード上の名前 |
| --- | --- | --- |
| ラウンド | IP が 1 つのメッセージを発信し、独立カスケードが終わるまで。`Session::step` 1 回に相当 | `round`（1 始まり。0 は初期状態） |
| ウェーブ | カスケードの 1 段。`curr` に入っているノードをすべて処理して `next` へ渡す 1 周。再生（F06）の単位 | `wave`（0 始まり。IP→隣人が wave 0） |
| リング／ホップ距離 | IP からのグラフ上の**最短距離**（辺の確率を無視した静的な値）。全体図（F11）の単位 | `Rings.dist`、`core::diffusion::shortest_distances_from` |
| ★ウェーブとリングは別物 | 伝播は確率的なので、距離 2 のノードが wave 4 で受信することも、受信しないこともある | — |
| 内部活性 | 受信時の内部効用が正（シェアできる状態）。何度でも成立しうる | `int_fired` / `EVER_INT` |
| 外部活性 | 外部効用が初めて正になった（外部行動）。生涯 1 回 | `ext_fired` / `EVER_EXT` |
| 強さ 0／無効値 | 機構を無効にする値（M1: κ=0）。このとき現行モデルに戻る | `Mech::off()` |
| 系列文字列 | メッセージ番号を 16 進 1 桁で並べた文字列。**左端がラウンド 1** | `Sequence::to_string` |
| 較正 T | 反応性の分布の scale（`1/(β·T)`）に使う T。実行 T とは別の設定項目 | `calibration_rounds`（既定 5） |
| 実行 T | 実際に回すラウンド数の上限 | `max_rounds`（既定 50） |
| シナリオ | 変更されない設定の束（グラフ、設定、初期母集団） | `Scenario`（`Arc` で共有） |
| セッション | シナリオ＋IP＋乱数＋ラウンドごとの状態の履歴 | `Session` |
| IP | 情報提供者。受信せず、強制的にシェアする。状態は更新しない | `ip` |
| s̄（平均刺激） | 反応性の重み w を逆算するときの平均刺激。**メッセージ空間の平均** | `mean_stimulus` |

---

## 3. 不変条件（Invariants）★最重要

違反すると研究結果の再現性が壊れる。実装中に疑問が出たら、まずこの表に戻る。

| ID | 規則 | 理由・補足 | 検査 |
| --- | --- | --- | --- |
| INV-01 | `core/` `runner/` `graph-lib/` `util/` `src/` の既存ファイルを変更しない。ルート `Cargo.toml` の `members` 追加のみ許す | バッチ結果の再現性。graph-lib は独立リポジトリの可能性がある | `git diff --stat` に既存ファイルが出ない |
| INV-02 | 機構を無効にしたとき、`sim-api` の結果は `core::diffusion::monte_carlo_average` と**ビット一致**する（合計値・効用・乱数列の位置まで） | N01 | T-01, T-02, T-07 |
| INV-03 | `sim-api` の乱数は `SimRng = rand_xoshiro::Xoshiro256PlusPlus` だけを使う。`SmallRng` / `StdRng` / `thread_rng` / `usize` の乱数抽出（`random_range` 等）を使わない | `SmallRng` は 32bit（WASM）で別アルゴリズム（Xoshiro128++）になる。✔検証済み：Xoshiro256PlusPlus は 64bit の `SmallRng` と乱数列が一致する | T-04 |
| INV-04 | 辺ごとに**必ず 1 回** `rng.random::<f64>()` を引く。受信済みの隣人に対しても引く（`&&` による短絡禁止）。（M3 を廃止したので、追加で引く乱数はない） | 乱数列がずれると過去の結果と一致しない（既存コードの NOTE と同じ理由） | T-01 |
| INV-05 | 処理順を固定する。`curr` は `Vec` を `pop`（後入れ先出し）、`successors` は昇順、`next` への追加順は**初回受信の順** | RNG の対応関係が変わらないようにする | T-01 |
| INV-06 | 効用更新は `w * u + (1.0 - w) * s` の評価順のまま書く。`mul_add` を使わない。状態は `f64`（`f32` は表示用ログだけ） | 浮動小数点の丸めを既存と一致させる | T-01 |
| INV-07 | 発火判定は `if relative_eq!(&u, &0.0) { false } else { u > 0.0 }`（`approx` の既定許容）。M3 を廃止したので、常にこの閾値判定 | 既存の `DecisionBase::decided` と同じ | T-01 |
| INV-08 | 新しい crate は `core` を `sim-core = { package = "core", path = "../core" }` と**名前を変えて**依存する。`core = …` のまま依存しない | ✔検証済み：パッケージ名 `core` をそのまま依存すると、`thiserror` 等のマクロが `::core::fmt` を解決できずコンパイルエラーになる | ビルド |
| INV-09 | ノード番号は graph-lib と同じ「ラベルの昇順ソートの添字」。元のラベルとの対応表を必ず持つ。系列文字列は左端＝ラウンド 1 | 過去の混乱（LSD-first、出現順 ID）の再発防止 | T-05 |
| INV-10 | `sim-api` は `wasm32-unknown-unknown` でビルドできること。`tokio` `rayon` `parquet` `std::fs` `std::time` `std::thread` を使わない（desktop 側で使う） | Web 版のため | CI |
| INV-11 | `HashMap` / `HashSet` の反復順序に依存しない。必要なら `BTreeMap` か `Vec` を使う | 決定性 | レビュー |
| INV-12 | 較正 T（`calibration_rounds`）と実行 T（`max_rounds`）を別の設定にする。反応性の scale は `1/(β·calibration_rounds)` | T を変えると集団が変わる落とし穴（研究メモ） | T-03 |
| INV-13 | IP は受信しない。IP の状態は更新しない。IP は常にシェアする（1 ウェーブ目の送信元） | 既存仕様 | T-01 |
| INV-14 | 機構や反応性の設定を変えたら、セッションを**作り直す**（履歴は消える旨を UI で確認する）。途中で強さだけ変えない | 履歴が現在の設定と矛盾するのを防ぐ | T-15 |
| INV-15 | 状態の効用に上限や丸めを設けない（1 を超えてよい）。ログや表示用の `f32` 変換でも上限を付けない | 要件 F10・F19 | T-13 |

---

## 4. 既存コードの事実（読んで確認済み）

| 場所 | 事実 | 本設計への影響 |
| --- | --- | --- |
| ルート `Cargo.toml` | **未確認**（資料に含まれていない）。`core` `util` `graph-lib` `rand` `rand_distr` `serde` `serde_with` `anyhow` `clap` が `workspace = true` で参照されている | 0 章の確認コマンドで読む |
| `core/Cargo.toml` | パッケージ名は **`core`**。edition 2024。依存は `approx` `graph-lib` `rand`（workspace、既定 feature）`serde` | INV-08。`rand` の既定 feature が `getrandom` を引くので、WASM では 8.2 の設定が要る |
| `core/src/diffusion.rs` | `diffuse(graph:&GraphB, prop_prob, ws:&mut Workspace, ip, message:&Message, rng) -> DiffusionResult{num_xact, num_share, total_step}`。`curr`/`next` は `Vec`、`pop()`で後入れ先出し。受信判定は `(rcvd[j]==false) & (prop_prob > rng.random())`。IP は `rcvd=true` で状態を更新しない。`num_share` は「内部が成立し、かつ後続を持つ」ノードの数。`num_xact` は外部が成立したノードの数 | 7.3 の参照実装が同じ動作をする |
| 同上 | `monte_carlo_average` は試行ごとに初期状態へ戻し、**1 本の rng を試行・ラウンド全体で使い続ける**。`multiple_diffuse` は系列の各メッセージ（=各ラウンド）で `diffuse` を呼ぶ。状態はラウンドをまたいで持ち越し、`rcvd` はラウンドごとにリセット | 1 回の再生は「`sample_size=1` の試行」と同じ乱数の使い方にする（T-02） |
| 同上 | `shortest_distances_from(graph, ip) -> Vec<Option<usize>>`（BFS、`successors` を使う） | リング計算の正解として使う（T-14） |
| `core/src/decision.rs` | `DecisionBase{weight, utility}` のフィールドと `update` は**非公開**。式は `u = w*u + (1.0-w)*s`。`DecisionAny` は毎回再評価、`DecisionOnce` は生涯 1 回。`weight()`/`utility()` の公開アクセサは `DecisionAny` と `DecisionBase` にあり、`DecisionOnce` には**無い** | 拡張機構のために効用を直接触る必要があるので、`sim-api` は**独自の状態型**を持つ（7.2）。式は複製する |
| `core/src/user.rs` | `UserState{internal_decision: DecisionAny, external_decision: DecisionOnce}`。`contact` は内部、外部の順に更新 | 同上 |
| `core/src/message.rs` | `Message{internal_stimulus, external_stimulus}`（`Deserialize` 済み）。`MessageSpace::try_from(Vec<Message>)` は L≤16 と刺激∈[-1,1] を検証。`internal_mean_stimulus()` 等で平均を返す。系列は基数 L の桁分解で、**桁 0＝ラウンド 1**、文字列は左端がラウンド 1 | `Message` と `MessageSpace` を再利用する。m∅（発信しない）は 2026-10-20 に廃止した |
| `runner/src/brute_force/config.rs` | `DecisionParameters{initial_utility, beta, gamma}`（フィールド非公開、`Deserialize`）。`sample_iter(num_rounds, num_nodes, mean_stimulus, rng:&mut SmallRng)`：`scale = 1.0/(beta*(num_rounds as f64))`、`shape = gamma/(beta-gamma)`、`Pareto::new(scale, shape)`、`weight = (mean/(mean-u0)).powf(responsiveness)`。`validate()` は `gamma<beta`、`beta>0`、`gamma>0`、`u0∈[-1,1]` を要求（非公開） | 同じ式を `sim-api` に複製し、runner の `sample_iter` と一致するテストを置く（T-03） |
| `runner/src/brute_force.rs` | ユーザ状態の乱数は `user_sampling.sample_with(\|rng\| (SmallRng::from_rng(rng), SmallRng::from_rng(rng)))`。第 k 番目のユーザ状態は k 番目の (内部 rng, 外部 rng) の組から作る。カスケードの rng は `SmallRng::seed_from_u64(0)`（`diffusion_seed: 0` 固定）から系列ごとに `from_rng` で派生 | `user_index` と `user_seed` で同じ母集団を再現できるようにする（7.6） |
| `util/src/lib.rs` | `GraphConfig` はファイルパスが必須でフィールド非公開。`RngSampling{size, seed}` | Web ではパスが使えないので、グラフの読み込みは `sim-api` が独自に持つ |
| `graph-lib/src/graph.rs` | `GraphB::{Di(DiGraphB), Ud(UndiGraphB)}`。`DiGraphB` は `Vec<BTreeSet<usize>>`（`successors` は昇順）。`From<&[(L,L)]>` はラベルを**昇順ソート・重複除去して添字化**。無向は両向きを持つ | INV-09。ラベル対応表は自前で保持する |
| `graph-lib/src/io.rs` | 構文エラーは **`panic!` / `unwrap`**。コメントは edgelist が `#`,`%`、csv/tsv が `#`。区切りは edgelist が空白・タブ、csv が `,`、tsv がタブ。`ParseOption{transposed, compressed}`。gz は `flate2` | WASM では panic が致命的なので、構文検査を先に自前で行う（7.10） |
| `graph-lib/Cargo.toml` | `flate2` と `serde` は常に有効。任意の `alg` feature を有効にすると `rayon`・`num-traits` と `algorithm`・`metric` モジュールが入る | `sim-api` は `alg` を有効にしない（rayon が WASM に入るため） |
| 既存の設定例 `config.toml` | `message_space` 4 個、`num_rounds=5`、`receipt_probability=0.5`、`beta=0.8, gamma=0.6, initial_utility=-1.0`、`graph={path=…, directed=true, transposed=true, format="csv"}`、`ip_sampling`、`user_sampling`、`diffusion_sample_size` | `ScenarioConfig` は同じキー名を使い、未知のキーを無視する（12 章） |
| 既知の落とし穴（研究メモ） | `num_rounds` は系列長と較正 T を兼ねる。メッセージ番号は外部刺激の昇順ではない（今回の 6 個では m0 が最大、m1 が最小）。有向と無向の数値は比較しない。txt は「A→B＝A が B をフォロー」で、拡散には `transposed=true` が要る | INV-12、11 章の既定値、8・9 章の読み込み画面 |

---

## 5. リポジトリ構成（追加するもの）

```
(repo root)
├─ Cargo.toml              # members に sim-api, sim-wasm, desktop を追加（既存の記述は変えない）
├─ .cargo/config.toml      # 新規 or 追記: wasm32 の getrandom 設定（8.2）
├─ core/ graph-lib/ runner/ util/ src/     # 既存。変更禁止（INV-01）
├─ sim-api/                # 新規 lib。WASM でもビルドできる純 Rust（INV-10）
│  ├─ Cargo.toml
│  ├─ src/
│  │   ├─ lib.rs
│  │   ├─ rng.rs           # SimRng
│  │   ├─ config.rs        # ScenarioConfig, MechanismConfig
│  │   ├─ params.rs        # ParamSpec 登録簿・プリセット
│  │   ├─ graph_io.rs      # グラフ読み込み（構文検査つき）
│  │   ├─ csr.rs           # Csr（辺の順序を保つ）
│  │   ├─ node.rs          # NodeState, contact（M1。M5 はコメントアウト）
│  │   ├─ population.rs    # Population, 母集団の生成（バッチと同じ乱数の使い方）
│  │   ├─ engine.rs        # run_round, rest_round, Observer
│  │   ├─ session.rs       # Session, Choice, RoundSummary, BranchTree
│  │   ├─ log.rs           # UtilityLog, フラグ, NodeKind
│  │   ├─ overview.rs      # リング, 放射状レイアウト, 扇形セル集計, 範囲選択
│  │   ├─ groups.rs        # ノード群の平均推移
│  │   ├─ centrality.rs    # 次数, PageRank（IP 候補の順位用）
│  │   ├─ mc.rs            # モンテカルロ
│  │   └─ export.rs        # JSON / CSV / バッチ用 TOML
│  ├─ tests/               # parity_core.rs, parity_runner.rs, golden.rs など（13 章）
│  └─ examples/make_golden.rs
├─ sim-wasm/               # 新規 cdylib（wasm-bindgen）。ロジックは持たず、sim-api の薄い包み（8 章）
├─ desktop/                # 新規 bin（eframe）。P2 で作る（10 章）
├─ web/                    # 新規 Vite + React + TypeScript。P1 で作る（9 章）
├─ spec/
│  ├─ ui-tokens.json       # 両版共通の色・形・寸法（11 章）
│  └─ fixtures/            # 小さなグラフ、設定、ゴールデン出力
└─ claude/viewer-design.md # 本書
```

---

## 6. 依存関係と Cargo の設定

### 6.1 `sim-api/Cargo.toml`

```toml
[package]
name = "sim-api"
version = "0.1.0"
edition = "2024"

[dependencies]
sim-core     = { package = "core", path = "../core" }   # INV-08: 名前を変えて依存する
graph-lib    = { workspace = true }                     # alg feature は有効にしない（rayon が入る）
rand         = { workspace = true }
rand_xoshiro = "0.7"                                    # ✔ rand 0.9 と組み合わせて検証済み
rand_distr   = { workspace = true }
serde        = { workspace = true }
serde_json   = "1"
toml         = "0.9"
flate2       = "1"                                      # gz の展開（graph-lib と同じ）
approx       = "0.5"                                    # INV-07
# libm = "0.2"                                  # M3 の exp 用だった。M3 の廃止で不要（Cargo.toml に残っていてもよい）
thiserror    = "2"

[dev-dependencies]
runner = { path = "../runner" }                         # T-03（母集団の一致）用。WASM のビルドには入らない
```

- ルートの `[workspace.dependencies]` に `rand_distr` がない、または `serde` に `derive` がないときは、0 章の確認結果に合わせて直す。
- バージョン番号は 0 章の `cargo tree` の結果を優先する。

### 6.2 WASM の設定（`sim-wasm`）

`core` は `rand` を既定の feature で使っており、`getrandom` が引かれる。`wasm32-unknown-unknown` では `getrandom` の裏方（バックエンド）を明示しないとコンパイルできない。`getrandom` 0.3 のソースで、`getrandom_backend="wasm_js"` の cfg と `wasm_js` feature の組が使われることは確認した。**WASM ターゲットはサンドボックスに入れられず、実際のビルドは未検証**なので、P0-1 で必ずビルドして確かめること。

```toml
# sim-wasm/Cargo.toml
[package]
name = "sim-wasm"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
sim-api = { path = "../sim-api" }
wasm-bindgen = "0.2"
console_error_panic_hook = "0.1"
getrandom = { version = "0.3", features = ["wasm_js"] }
serde_json = "1"
```

```toml
# .cargo/config.toml（ルートに新規作成。既にあれば追記）
[target.wasm32-unknown-unknown]
rustflags = ["--cfg", 'getrandom_backend="wasm_js"']
```

ビルド: `wasm-pack build sim-wasm --target web --release --out-dir ../web/src/wasm`。

### 6.3 `desktop/Cargo.toml`（P2 で作る）

`eframe`（wgpu feature）、`egui_plot`、`rfd`、`rayon`、`sim-api`、`serde_json`、`parquet`＋`arrow-array`＋`arrow-schema`（runner と同じ 57 系）。バージョンは P2-1 の時点の最新の互換な組を採る。

---

## 7. sim-api の設計

### 7.1 モジュール一覧

| モジュール | 役割 | 主な公開物 | 担当タスク |
| --- | --- | --- | --- |
| `rng` | 乱数の型を 1 か所で決める | `SimRng`, `rng_from_seed` | P0-2 |
| `config` | 設定の型と検証 | `ScenarioConfig`, `MechanismConfig`, `Mech` | P0-3 |
| `params` | 画面が自動生成するための登録簿とプリセット | `ParamSpec`, `param_specs()`, `MechanismConfig::{off, balanced, scaled}` | P0-3 |
| `graph_io` | グラフの読み込み。構文エラーを `Result` で返す | `GraphSource`, `LoadedGraph`, `load_graph` | P0-4 |
| `csr` | 辺の順序を保った高速な隣接表 | `Csr` | P0-4 |
| `node`, `population` | ノード状態、`contact`、母集団の生成 | `NodeState`, `Population`, `sample_population` | P0-5 |
| `engine` | 1 ラウンドのカスケード | `run_round`, `rest_round`, `Observer`, `Scratch` | P0-6 |
| `session`, `log` | 対話の 1 セッション、状態ログ | `Session`, `Choice`, `RoundOutput`, `UtilityLog`, `NodeKind` | P0-7 |
| `overview` | リング、レイアウト、集計 | `Rings`, `RadialLayout`, `SectorGrid` | P0-8 |
| `groups` | ノード群の推移 | `GroupSel`, `GroupSeries` | P0-8 |
| `centrality` | IP 候補の順位 | `out_degree`, `pagerank_reverse` | P0-8 |
| `mc` | 同じ系列を N 回回す | `run_mc`, `McSpec`, `McResult` | P0-9 |
| `export` | JSON・CSV・バッチ用 TOML | `ScenarioRecord`, `round_rows_csv`, `batch_toml` | P0-10 |

### 7.2 主要な型（Rust）

```rust
// rng.rs  — INV-03
pub type SimRng = rand_xoshiro::Xoshiro256PlusPlus;
pub fn rng_from_seed(seed: u64) -> SimRng { SimRng::seed_from_u64(seed) }

// node.rs
pub const EVER_RECV: u8 = 1; pub const EVER_INT: u8 = 2; pub const EVER_EXT: u8 = 4;

#[derive(Clone)]
pub struct NodeState {
    pub w_int: f64, pub w_ext: f64,    // 効用更新の重み（母集団の生成時に決まり、不変）
    pub u_int: f64, pub u_ext: f64,    // 現在の効用（上限なし。INV-15）
    pub ext_done: bool,                // 外部行動を済ませたか（生涯 1 回）
    pub flags: u8,                     // EVER_RECV | EVER_INT | EVER_EXT
}

// population.rs
#[derive(Clone)]
pub struct Population {
    pub states: Vec<NodeState>,
    pub recv_round: Vec<u32>,          // そのノードが最後に受信したラウンド。0 = 未受信
    pub u0_int: f64, pub u0_ext: f64,  // 初期効用
}

// config.rs
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DecisionParams { pub initial_utility: f64, pub beta: f64, pub gamma: f64 }   // runner と同じキー名

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ScenarioConfig {
    pub message_space: Vec<Message>,           // sim_core::message::Message（既存）
    pub receipt_probability: f64,
    pub calibration_rounds: u32,               // 既定 5   ← INV-12
    pub max_rounds: u32,                       // 既定 50
    pub internal_parameters: DecisionParams,
    pub external_parameters: DecisionParams,
    pub mean_stimulus: MeanStimulus,           // Auto（既定）| Manual { internal, external }
    pub user_seed: u64,                        // 母集団の生成シード（バッチの user_sampling.seed）
    pub user_index: u32,                       // 母集団の番号（バッチの us_id）
    pub diffusion_seed: u64,                   // カスケード乱数のシード（バッチは 0 固定）
    pub mechanisms: MechanismConfig,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MechanismConfig {
    pub m1: M1 /* enabled, kappa */,
    // pub m5: M5 /* enabled, eta, n_max */,   // 検討中のためコメントアウト（15.4）
}

/// 実効値。無効の機構は無効値（κ=0）になる。
#[derive(Clone, Copy, Debug)]
pub struct Mech { pub kappa: f64 /*, pub eta: f64, pub n_max: u8（M5。検討中） */ }
impl MechanismConfig { pub fn effective(&self) -> Mech { /* ... */ } }
```

- 旧版（1.0）にあった `m2`〜`m4`、`ScenarioConfig.horizon`（計画ラウンド数）、投稿予算の整数計算は、2026-10-20 に廃止した（15.4）。古い設定・記録の JSON にこれらのキーが残っていても、知らないキーとして無視して読める。
- `MeanStimulus::Auto` は `MessageSpace::internal_mean_stimulus()` / `external_mean_stimulus()` と**同じ計算**を使う（6 個なら 0.8/6 ≈ 0.1333…）。要件書の「0.133」は丸めた表示で、実装では丸めない。

### 7.3 カスケード（`engine::run_round`）

#### 7.3.1 方針

- 既存の `diffuse` を**そのまま写した**うえで、「受信した瞬間に状態を更新する」ところだけを「**ウェーブの終わりにまとめて更新する**」に変える。もとは M5（同じウェーブで届いた経路の数 n を数える）のために必要な変更だった。M5 は検討中だが、経路数 `n_arr` の記録はノード詳細の表示のために残してある。
- ✔**検証済み**（付録 B-3）: この方式は、機構なしのとき、既存の `diffuse`（の処理を書き写したもの）と合計値・全ノードの効用・外部行動フラグ・乱数列の位置まで**ビット一致**する（ランダムグラフ 60 種 × 受信確率 3 種 × 5 シード × 8 ラウンド）。**実物の `core::diffusion::diffuse` との照合は T-01 で行う**（P0-6 の完了条件）。理由は、(a) 乱数の引き方（辺ごとに 1 回）が同じ、(b) ノード同士の状態更新は独立、(c) `next` への追加順が「初回受信の順」で同じ、の 3 点である。

#### 7.3.2 アルゴリズム（擬似コード）

```rust
pub struct Scratch {            // ラウンドをまたいで使い回す作業領域
    rcvd: Vec<bool>, recv_wave: Vec<u32> /* NONE=u32::MAX */, arr: Vec<u8>,
    curr: Vec<u32>, next: Vec<u32>, newly: Vec<(u32, u32)>,   // newly = (受信ノード, 初回の送信元)
}
pub struct RoundCounts { pub n_received: u32, pub n_share: u32, pub n_xact: u32, pub n_waves: u32 }

pub trait Observer {
    fn on_receive(&mut self, ev: &ReceiveEvent);
}
pub struct NoTrace;
impl Observer for NoTrace { #[inline(always)] fn on_receive(&mut self, _: &ReceiveEvent) {} }  // MC 用。コストゼロ

pub struct ReceiveEvent {
    pub wave: u32, pub node: u32, pub from: u32, pub n_arr: u8,
    pub u_int: f64, pub u_ext: f64,                     // 更新後の効用
    pub int_fired: bool, pub ext_fired: bool, pub enqueued: bool,   // enqueued = 次のウェーブでシェアする
}

pub fn run_round<O: Observer>(
    csr: &Csr, p: f64, pop: &mut Population, sc: &mut Scratch,
    ip: u32, _msg_id: u8, msg: &Message, mech: &Mech, round: u32,
    rng: &mut SimRng, obs: &mut O,
) -> RoundCounts {
    sc.rcvd.fill(false); sc.recv_wave.fill(NONE);
    sc.curr.clear(); sc.next.clear(); sc.curr.push(ip); sc.rcvd[ip as usize] = true;   // INV-13
    let (mut n_recv, mut n_share, mut n_xact, mut wave) = (0, 0, 0, 0u32);

    while !sc.curr.is_empty() {
        sc.newly.clear();
        while let Some(i) = sc.curr.pop() {                          // INV-05: 後入れ先出し
            for &j in csr.succ(i) {                                  // INV-05: 昇順
                let hit = p > rng.random::<f64>();                   // INV-04: 必ず引く（短絡しない）
                if hit {
                    let ju = j as usize;
                    if !sc.rcvd[ju] {                                // 初回の受信
                        sc.rcvd[ju] = true; sc.recv_wave[ju] = wave; sc.arr[ju] = 1;
                        sc.newly.push((j, i));
                    } else if sc.recv_wave[ju] == wave {             // 同じウェーブでの 2 本目以降（経路数。M5 の検討用・表示用）
                        sc.arr[ju] = sc.arr[ju].saturating_add(1);
                    }                                                // IP や前のウェーブの受信者への命中は無視
                }
            }
        }
        for k in 0..sc.newly.len() {                                 // 初回受信の順に状態を更新（INV-05）
            let (j, from) = sc.newly[k];
            let n_arr = sc.arr[j as usize];
            let st = &mut pop.states[j as usize];
            let (int_fired, ext_fired) = st.contact(msg, n_arr, mech);   // 7.4。n_arr は M5 の検討用（いまは使わない）
            pop.recv_round[j as usize] = round;
            let enqueued = int_fired && csr.out_degree(j) > 0;       // 既存: successors(j).len() > 0
            if ext_fired { n_xact += 1; }
            if enqueued { n_share += 1; sc.next.push(j); }
            n_recv += 1;
            obs.on_receive(&ReceiveEvent { wave, node: j, from, n_arr,
                u_int: st.u_int, u_ext: st.u_ext, int_fired, ext_fired, enqueued });
        }
        std::mem::swap(&mut sc.curr, &mut sc.next);
        wave += 1;
    }
    RoundCounts { n_received: n_recv, n_share, n_xact, n_waves: wave }
}
```

- `n_share` は `core` の `DiffusionResult.num_share`、`n_xact` は `num_xact`、`n_waves` は `total_step` に一致しなければならない（T-01）。
- `Csr` は `GraphB` の `successors` を**同じ順序で**並べて作る（BTreeSet の昇順）。

#### 7.3.3 ラウンドの後処理（廃止）

旧版にあった `apply_forgetting`（M4 の忘却）と `rest_round`（m∅）は、2026-10-20 に廃止した（15.4）。`Session::step` は `run_round` を呼ぶだけである。

### 7.4 `NodeState::contact`（M1 の置き場所）

処理の順序は**この順番で固定**する。

```rust
impl NodeState {
    #[inline]
    pub fn contact(&mut self, msg: &Message, _n_arr: u8, m: &Mech) -> (bool, bool) {
        // M5 複合感染（検討中のためコメントアウト）: 同じウェーブで届いた経路の数 n（上限 n_max）で刺激を強める。
        // let scale = if m.eta != 0.0 { 1.0 + m.eta * (_n_arr.min(m.n_max) as f64 - 1.0) } else { 1.0 };
        // let (s_i, s_e) = (msg.internal_stimulus * scale, msg.external_stimulus * scale);
        let (s_i, s_e) = (msg.internal_stimulus, msg.external_stimulus);
        // 効用更新（INV-06: 評価順を変えない）
        self.u_int = self.w_int * self.u_int + (1.0 - self.w_int) * s_i;
        self.u_ext = self.w_ext * self.u_ext + (1.0 - self.w_ext) * s_e;
        // 外部の判定（生涯 1 回）
        let mut ext_fired = false;
        if !self.ext_done && decide(self.u_ext) { self.ext_done = true; ext_fired = true; }
        // M1 口コミ: 外部行動が成立した瞬間に 1 回だけ、内部効用へ κ を加える（内部の判定より前）
        if ext_fired && m.kappa != 0.0 { self.u_int += m.kappa; }
        // 内部の判定（毎回）
        let int_fired = decide(self.u_int);
        self.flags |= EVER_RECV | if int_fired { EVER_INT } else { 0 } | if ext_fired { EVER_EXT } else { 0 };
        (int_fired, ext_fired)
    }
}

/// INV-07: 既存の DecisionBase::decided と同じ。乱数は引かない。
#[inline]
pub fn decide(u: f64) -> bool {
    if approx::relative_eq!(&u, &0.0) { false } else { u > 0.0 }
}
```

| 機構 | 無効値 | 注意（実装時に確認） |
| --- | --- | --- |
| M1 κ | 0 | 要件書は「外部決定の成立時に内部効用へ κ を加える」としか書いていない。本書は **内部の判定の前**に加えると決めた（購入者がその場でシェアできる。ユーザー確認済み、15.3 の Q5）。κ は 1 回だけ（外部行動は生涯 1 回） |
| M5 η, n_max（検討中） | 0, 1 | コメントアウト中。`n_arr` はその**ウェーブ内**で、そのノードに届いた（確率で成功した）送信元の数。再検討するときは、複数到達の扱い（刺激の強化か、到達回数ぶんの更新か、ラウンドをまたぐ累積か）から決める（15.4） |

M1 の κ で、効用が刺激の定義域 [-1, 1] を超えうるが、`sim-api` の更新式は範囲を検査しない（既存の `debug_assert` は `core` 側の型の話で、ここでは使わない）。

### 7.5 ノード状態の分類（`log.rs`）

```rust
#[repr(u8)] pub enum NodeKind { Unreceived = 0, ReceivedOnly = 1, Internal = 2, External = 3, Both = 4, Ip = 5 }

/// UtilityLog の各ラウンドの行に格納する 1 バイトのフラグ。
pub const F_RECV_NOW: u8 = 1;  pub const F_INT_NOW: u8 = 2;  pub const F_EXT_NOW: u8 = 4;
pub const F_EVER_RECV: u8 = 8; pub const F_EVER_INT: u8 = 16; pub const F_EVER_EXT: u8 = 32;
```

`kind_of(flags, int_mode)`:

| 条件（上から順に判定） | 結果 |
| --- | --- |
| IP | `Ip` |
| `F_EVER_RECV` が無い | `Unreceived` |
| `int` = (`int_mode=Cumulative` なら `F_EVER_INT`、`ThisRoundOnly` なら `F_INT_NOW`)、`ext` = `F_EVER_EXT`（外部は常に累積） | `(false,false)→ReceivedOnly`、`(true,false)→Internal`、`(false,true)→External`、`(true,true)→Both` |

既定は `Cumulative`（要件書 9.1。画面で切り替え可能）。

### 7.6 母集団の生成（`population::sample_population`）

バッチ（`runner::brute_force`）と**同じ乱数の使い方**で、同じ `user_seed`・`user_index` から同じ母集団を作る。

```rust
pub fn sample_population(cfg: &ScenarioConfig, n_nodes: usize, mean_int: f64, mean_ext: f64) -> Result<Population, ConfigError> {
    // 1. バッチの user_sampling.sample_with と同じ: 親 rng から (内部 rng, 外部 rng) の組を user_index 番目まで順に作る
    let mut master = SimRng::seed_from_u64(cfg.user_seed);
    let mut pair = None;
    for _ in 0..=cfg.user_index { pair = Some((SimRng::from_rng(&mut master), SimRng::from_rng(&mut master))); }
    let (mut irng, mut erng) = pair.unwrap();
    // 2. 各ノードの w を Pareto から引く（式と評価順を runner と同じにする）
    //    scale = 1.0 / (beta * (calibration_rounds as f64)),  shape = gamma / (beta - gamma)
    //    responsiveness ~ Pareto::<f64>::new(scale, shape).sample_iter(&mut rng).take(n_nodes)
    //    weight = (mean / (mean - u0)).powf(responsiveness)
    // 3. NodeState{ w_int, w_ext, u_int=u0_int, u_ext=u0_ext, ext_done=false, flags=0 }
}
```

- 検証（`validate`）は runner と同じ: `beta>0`、`gamma>0`、`gamma<beta`、`initial_utility∈[-1,1]`。`mean` と `u0` の関係で `mean − u0 = 0` になる設定（0 除算）も弾く。
- `powf` は標準の `f64::powf` を使う（バッチと揃えるため）。WASM とネイティブで最後の 1 ビットが異なる可能性が残るのは、ここ（初期の重み）だけである（15.1 の D6）。

### 7.7 セッション（`session.rs`）

```rust
pub enum Choice { Msg(u8) }   // m∅（Rest）は 2026-10-20 に廃止

pub struct Scenario {            // 変更されない。Arc で共有
    pub graph: LoadedGraph, pub cfg: ScenarioConfig, pub mech: Mech,
    pub mean_int: f64, pub mean_ext: f64, pub initial: Population,
}

pub struct Session { /* scenario: Arc<Scenario>, ip, pop, rng, round, log, history, scratch, trace */ }

impl Session {
    /// 乱数は `SimRng::seed_from_u64(cfg.diffusion_seed)`。★バッチの乱数とは別物（下の注）
    pub fn new(scn: Arc<Scenario>, ip: u32) -> Result<Self, SessionError>;
    /// 乱数を外から渡す（バッチの 1 行の再現・テスト用。7.12 の `batch_row_rng`）
    pub fn with_rng(scn: Arc<Scenario>, ip: u32, rng: SimRng) -> Result<Self, SessionError>;
    pub fn step(&mut self, choice: Choice) -> Result<&RoundOutput, SessionError>;
    pub fn round(&self) -> u32;
    pub fn kinds(&self, int_mode: IntMode) -> Vec<NodeKind>;               // 現在のラウンド終了時の全ノード
    pub fn node_detail(&self, node: u32) -> NodeDetail;                    // F10
    pub fn group_series(&self, sel: GroupSel, at_round: u32, int_mode: IntMode) -> GroupSeries;   // F19
    pub fn log(&self) -> &UtilityLog;
    // P3
    pub fn rewind_to(&mut self, round: u32) -> Result<(), SessionError>;
}

pub enum SessionError { RoundLimit, InvalidMessage(u8), InvalidIp(u32) }   // ほか: 巻き戻し・節の誤りなど
```

> **注（バッチの乱数との関係）**: バッチ（`runner::brute_force`）のカスケード用の乱数は、`SmallRng::seed_from_u64(0)` を親にして、(us_id, IP の位置, 系列番号) の順に `from_rng` で子を派生させ、**系列ごとに別の子**を使う。`Session::new` の乱数（`seed_from_u64(diffusion_seed)`）はこれとは別物なので、`Session::new` の結果はバッチの特定の行とは一致しない。バッチの 1 行を画面で再現したいときは、`batch_row_rng`（7.12）で子の乱数を作り、`with_rng` に渡す。その行の MC 平均の 1 試行目が、画面の 1 回の再生に一致する。

`step` の手順:

1. 検査: `round == max_rounds` → `RoundLimit`。`Choice::Msg(id)` で `id >= L` → `InvalidMessage`。
2. `round += 1`。`rng` の複製を `RoundRecord.rng_before` に保存する（巻き戻し用、P3）。
3. `run_round(.., obs = &mut TraceRecorder)` を呼ぶ。
4. （旧版の `apply_forgetting`・予算の計数は廃止。）
5. `UtilityLog` に今ラウンドの行（全ノードの `u_int`・`u_ext`（`f32`）とフラグ）を追加する。フラグの `F_*_NOW` は、そのラウンドの受信・発火から作る。
6. `RoundSummary` を作る（下表）。

```rust
pub struct RoundSummary {
    pub round: u32, pub choice: Choice,
    pub n_received: u32, pub n_share: u32, pub new_xact: u32, pub cum_xact: u32, pub n_waves: u32,
    pub n_int_active_now: u32,                              // このラウンドで内部が成立した受信者数
    pub recv: UtilityStats, pub all: UtilityStats,          // 受信済みノード / IP 以外の全ノード
}
pub struct UtilityStats { pub n: u32, pub mean_int: f32, pub mean_ext: f32, pub q_int: [f32; 3], pub q_ext: [f32; 3] } // [p10, p50, p90]
```

- 分位点は、昇順に並べて添字 `round((n-1)·q)` の値を採る（線形補間はしない。両版で同じ値になるよう、計算は Rust だけで行う）。
- `n_share`・`new_xact` は `core` の `num_share`・`num_xact` と同じ数え方（T-01）。

```rust
pub struct RoundOutput { pub summary: RoundSummary, pub events: EventColumns }

/// 列指向（WASM の TypedArray にそのまま渡せる）。長さはすべて同じ。
pub struct EventColumns {
    pub wave: Vec<u16>, pub node: Vec<u32>, pub from: Vec<u32>, pub n_arr: Vec<u8>,
    pub u_int: Vec<f32>, pub u_ext: Vec<f32>,
    pub flags: Vec<u8>,   // bit0 = int_fired, bit1 = ext_fired, bit2 = enqueued
}
```

#### 状態ログ（F10・F19 の元データ）

```rust
pub struct UtilityLog {
    n_nodes: usize,
    u_int: Vec<f32>, u_ext: Vec<f32>, flags: Vec<u8>,   // ラウンド優先: 行 r の長さ = n_nodes。行 0 = 初期状態
}
```

- 1 ノード 1 ラウンドあたり 9 バイト。Twitter 規模（81,306 ノード）× 50 ラウンドで約 37 MB。要件書の「約 32MB」は効用だけの見積もりで、フラグを足すと約 37 MB になる。
- 確保の上限: `n_nodes × (max_rounds + 1) × 9 ≤ 1 GiB`。超える設定は `ConfigError::LogTooLarge` で拒否する。Web 版は 256 MB 超で警告を出す。
- 状態そのもの（`Population`）は `f64` で持ち、ログだけ `f32` にする（INV-06・INV-15。丸め・上限なし）。

#### 巻き戻しと分岐（P3。P0 ではデータの用意だけ）

- 方式は **チェックポイント＋再生**。ラウンド 0 と、8 ラウンドごとに `Population` の複製・`SimRng` の複製・`sent` を保存する。
- `rewind_to(r)`: `r` 以下で最大のチェックポイントを復元し、記録済みの選択を再生して `r` まで進める。乱数を復元しているので**ビット一致で再現**する（T-15）。ログと履歴は `r` で切り詰める。
- 分岐: `rewind_to(r)` のあとに別の `Choice` で `step` すると、`BranchTree` に新しい枝ができる。枝の切り替えは「共通の祖先まで巻き戻し → その枝の選択列を再生」で行う。ノード単位のログは**現在の枝の分だけ**持つ（枝ごとに 37 MB を持たない）。`BranchTree` の節は `{id, parent, round, choice, summary}` だけを持つ。

### 7.8 全体図（`overview.rs`）

```rust
pub struct Rings {
    pub dist: Vec<u32>,            // IP からの最短距離。u32::MAX = 到達不能
    pub ring_sizes: Vec<u32>,      // 添字 = 距離。[0] = 1（IP）
    pub unreachable: u32,
}
pub fn compute_rings(csr: &Csr, ip: u32) -> Rings;     // BFS。core の shortest_distances_from と一致（T-14）

pub struct RadialLayout { pub angle: Vec<f32>, pub parent: Vec<u32> }   // 到達可能なノードだけ有効。angle ∈ [0, 2π)
pub fn radial_layout(csr: &Csr, rings: &Rings, ip: u32) -> RadialLayout;

pub const N_SECTORS: u32 = 72;                         // 5 度刻み
pub struct RingSectorCell { pub n: u32, pub n_recv: u32, pub n_int: u32, pub n_ext: u32, pub n_both: u32 }
pub struct SectorGrid { pub cells: Vec<RingSectorCell> }   // 添字 = ring * N_SECTORS + sector
pub fn aggregate(rings: &Rings, layout: &RadialLayout, kinds: &[NodeKind]) -> SectorGrid;

pub const EXPAND_CAP: usize = 10_000;                  // 個別展開できる上限（N03）
pub fn select_nodes(rings: &Rings, layout: &RadialLayout, ring_lo: u32, ring_hi: u32,
                    ang_lo: f32, ang_hi: f32) -> Result<Vec<u32>, TooMany>;
```

**放射状レイアウトのアルゴリズム**（初版は素朴でよい。O(N+E)）:

1. IP から BFS（FIFO、`successors` は昇順）して、各ノードの親（最初に見つけたノード）を決める。
2. 親子関係から子のリストを作り（子は添字の昇順）、IP を根として**深さ優先の行きがけ順**に番号 `pos` を振る（再帰せず、明示的なスタックを使う）。
3. `angle(v) = 2π · pos(v) / n_reachable`。同じ部分木のノードが連続した角度に並ぶ。
4. 描画の座標は `(R·ring·cos(angle), R·ring·sin(angle))`（`R` はリング間隔）。重なりを減らすため、半径に `+ 0.3·R·(frac(node·0.6180339887) − 0.5)` の決定的なずらしを入れてよい（これは描画側の仕事で、`sim-api` は角度だけ返す）。
5. **到達不能**のノードは座標を持たない。全体図では 1 つの枠にまとめ、展開するときは格子状に並べる（上限は `EXPAND_CAP`）。

### 7.9 ノード群の推移（`groups.rs`）

```rust
pub enum GroupSel { IntOnly, ExtOnly, Both, ReceivedOnly, Custom(Vec<u32>) }

pub struct Band { pub mean: Vec<f32>, pub p10: Vec<f32>, pub p50: Vec<f32>, pub p90: Vec<f32> }   // 長さ = at_round + 1
pub struct GroupSeries {
    pub at_round: u32, pub members: u32,
    pub int: Band, pub ext: Band,           // 内部・外部の効用。メンバーは at_round の時点で固定（要件書 9.3）
    pub size_by_round: Vec<u32>,            // 各ラウンドでそのつど分類し直したときの群の大きさ
}
```

- 分類は `kind_of` と同じ規則で、`at_round` の行のフラグから行う。`IntOnly` = Internal、`ExtOnly` = External、`Both` = Both、`ReceivedOnly` = ReceivedOnly。
- 平均推移は、`at_round` で決まった**メンバーを固定**して、ラウンド 0〜`at_round` の各行の `u_int`・`u_ext` を集計する（「いま両方活性のノードは、それ以前にどう推移したか」）。
- `size_by_round[t]` は、各ラウンド `t` の行のフラグでそのつど分類した大きさ（群の大きさの推移）。
- `Custom` は、リング・扇形・範囲選択で作った任意の群。

### 7.10 グラフの読み込み（`graph_io.rs`）

```rust
pub enum GraphFormat { EdgeList, Csv, Tsv }
pub struct GraphSource { pub bytes: Vec<u8>, pub format: GraphFormat, pub gzip: bool, pub directed: bool, pub transposed: bool }
pub struct LoadedGraph {
    pub graph: GraphB,          // sim_core が使う型（graph_lib::prelude::GraphB）
    pub csr: Csr,
    pub labels: Vec<i64>,       // 添字 = ノード番号、値 = 元のラベル（昇順）
    pub n_nodes: usize, pub n_edges: usize,
    pub hash: u64,              // 正規化した辺リストの FNV-1a 64bit（記録 JSON に入れる）
}
pub fn load_graph(src: &GraphSource) -> Result<LoadedGraph, GraphError>;
```

手順:

1. `gzip` なら `flate2::read::GzDecoder` で展開する。
2. 1 行ずつ、graph-lib と**同じ規則**で解析する（コメント、区切り、空行の扱いは 4 章の表）。**自前で解析して `Result` を返す**（graph-lib は構文エラーで panic する。WASM では致命的）。エラーには行番号を入れる（`GraphError::Parse { line, text }`）。
3. `transposed` なら各辺を `(v, u)` に反転する。
4. ラベルを昇順に並べて重複を除き（`labels`）、辺を添字に直す。`GraphB::Di(DiGraphB::from(&edges[..]))`、無向なら `GraphB::Ud(UndiGraphB::from(&edges[..]))` で組み立てる（`graph-lib` の `From<&[(L,L)]>` は `L = i64` でも使える）。ラベルの並びと添字の対応は、graph-lib 自身が使う方法（ソート＋重複除去）と同じなので一致する。
5. `Csr::from_graph` で `successors` を昇順のまま並べる。

空のグラフ、辺が 0 本、ノード数が `u32` を超える場合は `GraphError`。自己ループは許す（`BTreeSet` が重複を除く）。**graph-lib の `ParseBuilder` は使わない**が、結果が一致することを T-05 で確かめる。

### 7.11 IP 候補の順位（`centrality.rs`）

| 指標 | 計算 | 備考 |
| --- | --- | --- |
| 次数（フォロワー数） | 拡散グラフの `successors` の数（出次数） | 研究での「入次数」（入力ファイルの向きでの入次数）と同じ量。VC との順位相関 ρ≈0.90 |
| PageRank | **拡散グラフの逆向き**（`predecessors` をたどる）で、減衰 0.85、50 反復、行き止まりは一様に分配 | 入力ファイルの向きでの PageRank（過去の Python 実装 `reverse_edges=False`）に一致。VC との順位相関 ρ≈0.99 |
| VC（Viral Centrality） | **アプリ内では計算しない**。事前計算した CSV（`node, degree, pagerank, vc`）があれば読み込んで順位に使う | 計算が重い。CSV の `node` は元のラベル（添字ではない） |

### 7.12 モンテカルロ（`mc.rs`）

```rust
pub enum McMode { Sequential { seed: u64 }, Independent { seed: u64 } }
pub struct McSpec { pub trials: u32, pub mode: McMode }
pub struct McResult {
    pub rounds: u32,
    pub new_xact: Band, pub cum_xact: Band, pub new_share: Band,       // 試行間の mean / p10 / p50 / p90（ラウンドごと）
    pub mean_u_int: Band, pub mean_u_ext: Band,                        // 受信済みノードの平均効用
    pub total_xact_mean: f64, pub total_share_mean: f64,               // 系列全体の合計の平均（バッチの mean_num_* と同じ量）
}
pub fn run_mc(scn: &Scenario, ip: u32, seq: &[Choice], spec: &McSpec) -> McResult;
pub fn run_trial(scn: &Scenario, ip: u32, seq: &[Choice], rng: &mut SimRng, scratch: &mut Scratch, out: &mut TrialSeries);
```

- `Sequential`: **1 本の rng を試行をまたいで使い続ける**。`seed` から `SimRng` を作り、各試行は `scn.initial` から母集団を戻してから系列を回す。機構を無効にしたとき、`core::diffusion::monte_carlo_average`（同じ rng・同じ試行数）と `total_*_mean` がビット一致する（T-02）。バッチとの照合用。
- `Independent`: 試行 k の rng を `SimRng::seed_from_u64(seed.wrapping_add(k as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15))` で作る。試行どうしが独立なので、desktop 版は `rayon` の `par_iter` で `run_trial` を並列に呼べる（`sim-api` は `rayon` を使わない: INV-10）。画面の MC 併記（F12）はこちらを使う。
- MC は `NoTrace` で回す（イベントを記録しない）。

**バッチの 1 行の再現**（乱数の対応づけ）:

```rust
/// runner::brute_force の乱数の割り当てを再現する。
/// 親は SimRng::seed_from_u64(diffusion_seed)。バッチは (us_id → IP の位置 → 系列番号) の三重ループで
/// 系列ごとに `from_rng(&mut 親)` で子を 1 つずつ派生させる。したがって第 k 番目の子が対象の行の乱数:
///   k = (us_id * num_ips + ip_pos) * num_seqs + seq_index
/// 親から k 個の子を順に捨てて、次の子を返す（チャンク分割は派生の順序を変えない）。
pub fn batch_row_rng(diffusion_seed: u64, num_ips: usize, num_seqs: u64,
                     us_id: usize, ip_pos: usize, seq_index: u64) -> SimRng;
```

- 返った乱数を `run_mc(Sequential)` に渡して 100 試行（バッチの `diffusion_sample_size`）回すと、その行の `mean_num_xact`・`mean_num_share` に一致する（T-02b）。
- `num_seqs` は `MessageSpace::seq_size(num_rounds)`（`L^T`）、`ip_pos` は `ip_sampling` が返した IP の並びでの位置。IP の並びは `(0..n).choose_multiple(rng, size)`（`usize` の乱数抽出）で決まるので、**64bit ネイティブでしか再現できない**。画面側は IP の位置をユーザーに入力させる（Could。P3）。

### 7.13 出力（`export.rs`）

| 出力 | 形式 | 内容 |
| --- | --- | --- |
| シナリオ記録 | JSON | `{ "version": 1, "graph": {hash, n_nodes, n_edges, directed, transposed, source_name}, "config": <ScenarioConfig>, "ip": {id, label}, "branch_tree": [{id, parent, round, choice}], "active_path": [id…], "sequence": "03102" }`。`sequence` は左端がラウンド 1 |
| ラウンド別結果 | CSV | 列（この順）: `round, msg, new_share, new_xact, cum_xact, received, mean_u_int, mean_u_ext, p10_u_int, p50_u_int, p90_u_int, p10_u_ext, p50_u_ext, p90_u_ext, mean_u_int_all, mean_u_ext_all`。`mean_*`・分位点は受信済みノードについて、`*_all` は IP 以外の全ノードについて。`msg` は `0`〜`5` または `-` |
| ノード別イベント | CSV（desktop は Parquet も） | 列: `round, wave, node, node_label, from, from_label, u_int, u_ext, kind`（`kind` は `NodeKind` の名前） |
| バッチ用 TOML | TOML | 機構がすべて無効のときだけ、既存の `runner brute-force` が読める形式で出す（`message_space`、`num_rounds`、`receipt_probability`、`internal_parameters`、`external_parameters`、`graph`、`ip_sampling`、`user_sampling`、`diffusion_sample_size`）。**機構が 1 つでも有効なら `Err(BatchIncompatible)`**。有効なときの検証は P3-5 の `verify` サブコマンドで行う（15.1 の D2） |

Parquet は `sim-api` では作らない（INV-10）。`desktop` が `arrow-array`・`parquet` で行に変換する。

---

## 8. sim-wasm の設計（Web 版の計算の窓口）

`sim-wasm` は**ロジックを持たない**。`sim-api` の関数を `wasm-bindgen` で包むだけの薄い層とする（ここにモデルの処理を書かない）。

### 8.1 公開 API（`#[wasm_bindgen] pub struct Simulator`）

文字列で返すものは JSON、`Uint*/Float*Array` で返すものは列指向の配列である。エラーは日本語のメッセージ付きの JS 例外（`Err(JsValue)`）にする。64bit の乱数シードは JS の数値で扱える範囲（2^53 未満）に制限し、超える入力は検証で弾く。

| メソッド（JS 名） | 引数 | 返り値 | 対応する sim-api |
| --- | --- | --- | --- |
| `loadGraph` | `bytes: Uint8Array`, `optsJson: string`（`format`, `gzip`, `directed`, `transposed`） | `string`（JSON: ノード数、辺数、ハッシュ、警告） | `graph_io::load_graph` |
| `paramSpecs` | — | `string`（JSON: `ParamSpec[]`） | `params::param_specs` |
| `createScenario` | `configJson: string` | `string`（JSON: 検証済み設定、平均刺激、較正値の要約） | `Scenario` の作成 |
| `centralityRanking` | `metric: string`（`degree`/`pagerank`/`vc`）, `top: number`, `vcCsv?: string` | `Uint32Array`（ノード番号を順位順） | `centrality` |
| `setIp` | `node: number` | `string`（JSON: リングの大きさ、到達不能数） | `Session::new`, `compute_rings`, `radial_layout` |
| `ringLayout` | — | `{ angle: Float32Array, dist: Uint32Array }` を 2 回の呼び出しで返す（`ringAngles()`, `ringDist()`） | `overview` |
| `step` | `choice: number`（`0..L-1` = メッセージ） | `string`（JSON: `RoundSummary`） | `Session::step` |
| `eventWave` / `eventNode` / `eventFrom` / `eventNArr` / `eventUInt` / `eventUExt` / `eventFlags` | — | 直前の `step` のイベント列（`Uint16Array` / `Uint32Array` / `Uint32Array` / `Uint8Array` / `Float32Array` / `Float32Array` / `Uint8Array`） | `EventColumns` |
| `everFlags` | — | `Uint8Array`（全ノードの `EVER_*` ビット。現在のラウンド終了時） | `UtilityLog` |
| `kinds` | `intMode: number`（0 = 累積、1 = このラウンドのみ） | `Uint8Array`（`NodeKind`） | `Session::kinds` |
| `nodeDetail` | `node: number` | `string`（JSON: ラベル、各ラウンドの `u_int`・`u_ext`・受信・内部・外部、受信元） | `Session::node_detail` |
| `groupSeries` | `sel: number`, `atRound: number`, `intMode: number`, `customNodes?: Uint32Array` | `string`（JSON: `GroupSeries`） | `groups` |
| `sectorGrid` | `intMode: number` | `Uint32Array`（`[ring][sector]` ごとに `n, n_recv, n_int, n_ext, n_both` の 5 値） | `overview::aggregate` |
| `selectNodes` | `ringLo, ringHi, angLo, angHi` | `Uint32Array`（`EXPAND_CAP` 超過は例外 `TooMany`） | `overview::select_nodes` |
| `edgesAmong` | `nodes: Uint32Array`, `cap: number` | `Uint32Array`（`[from, to, …]`。両端が `nodes` に入る辺） | `Csr` |
| `runMc` | `seq: Int8Array`, `trials: number`, `seed: number` | `string`（JSON: `McResult`） | `mc::run_mc`（`Independent`） |
| `exportScenarioJson` / `exportRoundsCsv` / `exportEventsCsv` | `round?: number` | `string` | `export` |
| `rewindTo`（P3） | `round: number` | — | `Session::rewind_to` |
| `reset` | — | — | `Session` を作り直す |

### 8.2 Worker とのやり取り

計算は **Web Worker** の中で動かし、メインスレッドを止めない。メッセージは `{ id: number, op: string, … }` の要求と `{ id, ok: true, … } | { id, ok: false, error: string }` の応答で往復する。配列は `postMessage(res, [buffer…])` で**所有権を移す**（コピーしない）。操作名は 8.1 のメソッド名と同じにする。型は `web/src/worker/protocol.ts` に TypeScript の判別共用体として 1 か所で定義する。

---

## 9. Web 版の設計（P1）

### 9.1 技術構成

| 項目 | 決定 | 理由 |
| --- | --- | --- |
| ビルド | Vite + TypeScript（厳格モード） | — |
| UI | React 18 + zustand（状態管理） | 状態が画面をまたぐ（選択ノード、再生位置など） |
| ネットワーク描画 | **自前の WebGL2 インスタンス描画**（1 回の描画呼び出しで全ノード）＋ Canvas2D の重ね描き（リングの円弧・ラベル・扇形セル） | 両方活性の「左右半分塗り」とリング表示が必要で、座標がリング配置で決まるため力学レイアウト系のライブラリの利点が小さい。要件書 8.1 の候補（sigma.js / Cosmograph）から変更（15.1 の D3） |
| 時系列 | uPlot（バンドは `bands` オプション） | 軽い。10〜90% 点の帯が描ける |
| 分岐ツリー | SVG（P3） | ノード数が少ない |
| 計算 | `sim-wasm` を Web Worker で実行 | 8 章 |
| テスト | vitest（純粋関数）、Playwright（任意） | — |
| 配布 | `vite build`（`base: './'`）で静的ファイル一式。サーバ不要、オフライン可（N05・N06） | — |

### 9.2 ディレクトリ

```
web/
├─ index.html  vite.config.ts  tsconfig.json  package.json
└─ src/
   ├─ main.tsx  App.tsx
   ├─ wasm/                  # wasm-pack の出力（コミットしない）
   ├─ worker/{sim.worker.ts, protocol.ts, client.ts}
   ├─ store/{session.ts, view.ts, playback.ts}      # zustand
   ├─ render/{NetworkView.tsx, gl.ts, shaders.ts, overlay.ts, hit.ts, camera.ts}
   ├─ panels/{Toolbar.tsx, ControlPanel.tsx, NodeDetail.tsx, RoundChart.tsx, BranchTree.tsx}
   ├─ logic/{kinds.ts, frames.ts, units.ts}         # 純粋関数。vitest の対象
   └─ tokens.ts              # spec/ui-tokens.json を読む
```

### 9.3 画面の部品

| 部品 | 責務 | 主な入力 | 対応 F |
| --- | --- | --- | --- |
| `Toolbar` | グラフの読み込み、設定の保存・読み込み、書き出し、現在ラウンド／T の表示 | — | F01, F14 |
| `ControlPanel` | 上から順に、グラフ設定（形式・gz・有向・`transposed`）、シナリオ設定（β・γ・受信確率・シード・較正 T・実行 T）、機構 M1（オン・オフとスライダー。`paramSpecs` から自動生成）、プリセット（「現行（すべて無効）」「既定」「既定の2倍」）、IP 選択（順位表またはクリック）、メッセージパレット（m0〜m5）、「発信」 | `paramSpecs`, `centralityRanking` | F02〜F05, F16 |
| `NetworkView` | ネットワーク表示、再生、ノード・リング・扇形のクリック、パン・ズーム、縮小地図 | `RadialLayout`, `everFlags`, イベント列 | F06, F07, F11 |
| `NodeDetail` | 「ノード」「ノード群」のタブ。単一ノードの効用の折れ線、群の平均推移と帯 | `nodeDetail`, `groupSeries` | F10, F19 |
| `RoundChart` | ラウンド別の時系列（新規シェア数、新規外部行動者数、累積外部行動者数、効用の平均と分位点） | `RoundSummary[]` | F08 |
| `BranchTree` | 分岐ツリー（P3） | `BranchTree` | F09 |

### 9.4 描画の仕様

**表示モード**（`view.mode`）: `overview`（リング・扇形の全体図）と `expanded`（個別ノード）。

| 条件 | 初期のモード |
| --- | --- |
| ノード数 ≤ 10,000 | `expanded`（全ノードを個別に表示。N03） |
| ノード数 > 10,000 | `overview`。扇形セルまたはリングをクリックすると `selectNodes` で個別展開（`EXPAND_CAP` = 10,000 まで。超えるときは「範囲を狭めてください」と表示） |

**WebGL2 の描画**: 1 回の `drawArraysInstanced` で全ノードを描く。インスタンスごとの属性は `a_pos`（`vec2`、ワールド座標）、`a_kind`（`uint8`）、`a_pulse`（`uint8`、いまのウェーブで受信したら 1）。ノードの半径は画面ピクセルで一定（`tokens.node.radiusPx`）、ズームで変えない。

フラグメントシェーダの規則（`kind` は `NodeKind` と同じ番号）:

| kind | 形 | 塗り |
| --- | --- | --- |
| 0 未受信 | 円（距離 `d = length(p)`） | 白抜き（`0.75 < d ≤ 1` を灰色の線、内側は背景色） |
| 1 受信のみ | 円 | 灰色 |
| 2 内部活性 | 円 | 青 |
| 3 外部活性 | 正方形（`d = max(|p.x|, |p.y|)`） | 緑 |
| 4 両方活性 | 正方形 | `p.x < 0` なら青、そうでなければ緑（左右半分） |
| 5 IP | 円（半径 2.2 倍） | 青＋黒い縁取り |
| `a_pulse=1` | 外側にオレンジの輪（`1 < d ≤ 1.35`） | 現在のウェーブの強調 |

色と寸法は**ハードコードせず** `spec/ui-tokens.json`（11.1）の値をユニフォームで渡す。辺は、展開中のノード集合について `edgesAmong`（上限 50,000 本）の結果を細い線で描き、再生中は現在のウェーブの送信元→受信先の辺だけオレンジで強調する。リングの円弧、リング名（「1ホップ」など）、扇形セルの塗りは Canvas2D で重ねる。

**ヒットテスト**: ノードの画面座標に一様格子（セル幅 = 半径の 4 倍）を作り、クリック位置に最も近いノードを半径の 1.5 倍以内で拾う。

### 9.5 再生（F06）

`logic/frames.ts` の純粋関数で作る。再生は `requestAnimationFrame` で時間を積算して進める。

```ts
// ラウンド開始時の everFlags と、そのラウンドのイベント列から、ウェーブ k の終了時点の状態を作る
function stateAtWave(everBefore: Uint8Array, ev: Events, k: number | -1, intMode: 0 | 1): { kinds: Uint8Array; pulse: Uint8Array }
```

- `k = -1` はラウンド開始前の状態、`k = n_waves - 1` はラウンド終了後の状態（`kinds(intMode)` と一致しなければならない。vitest で確かめる）。
- 規則: イベント（`wave ≤ k`）ごとに、`node` に `EVER_RECV` を立て、`int_fired` なら `EVER_INT`、`ext_fired` なら `EVER_EXT` を立てる。`intMode = 1`（このラウンドのみ）では、内部は「そのラウンドの `int_fired`」だけで判定し、外部は常に累積。`pulse` は `wave == k` のイベントの `node`。
- 操作: 再生・一時停止・コマ送り（±1 ウェーブ）・速度（0.5〜16 ウェーブ/秒、既定 4）・最後までスキップ。再生中は「発信」を押せないようにする（スキップ後に押せる）。

### 9.6 時系列とノード詳細

- **RoundChart**（F08）: 横軸はラウンド。系列は「新規シェア数」「新規外部行動者数」「累積外部行動者数」。効用の平均と分位点（受信済みノード）は別の小さなグラフにする。縦軸は値に合わせて自動で広げる。
- **NodeDetail・単一ノード**（F10）: 内部効用・外部効用の折れ線（**上限なし。1 を超えても切らない**。縦軸を自動で広げる）。受信したラウンド、受信元（元のラベル）、内部・外部が活性になったラウンドを表で示す。
- **NodeDetail・ノード群**（F19）: 選択肢は「内部のみ」「外部のみ」「両方」「受信のみ」と、リング・扇形・範囲選択で作った任意の群。群の内部・外部効用の平均推移を種類ごとに重ね、10〜90% 点の帯を付ける。「群の大きさの推移」（`size_by_round`）は別の小さな折れ線にする。表示中のラウンドの時点で群を決め、メンバーを固定して過去へさかのぼる（7.9）。

### 9.7 ファイルの扱い

- 読み込み: `<input type="file">` とドラッグ＆ドロップ。拡張子（`.txt` `.tsv` `.csv` `.gz`）から形式と gz を推定し、ControlPanel で上書きできる。読み込みの既定は `transposed = true`（入力が「A→B＝A が B をフォロー」の形式のため。ツールチップで説明する）。有向と無向を切り替えたときは「有向と無向の結果は比較できない」と注意を出す。
- 保存: `Blob` ＋ `<a download>`。外部サーバには何も送らない（N06）。
- 大きなグラフ: 10,000 ノードを超えたら、全体図から始まること、個別展開の上限があることを読み込み時に表示する。

### 9.8 P1 の完了判定（要件書 11 章に対応）

「すべて無効」「既定」「既定の2倍」の 3 水準で、同じ IP・同じ系列・同じシードを流し比べられること。比較の記録は、水準ごとにシナリオ記録 JSON とラウンド別 CSV を書き出して残す。これで M1 の既定の強さ（12.1）を確認・調整する。

---

## 10. デスクトップ版の設計（P2）

### 10.1 技術構成

| 項目 | 決定 |
| --- | --- |
| UI | `eframe`（`wgpu` feature）。画面は `egui` の標準部品 |
| ネットワーク描画 | `egui_wgpu` のカスタム描画（`CallbackTrait`）。インスタンスバッファ（`pos: vec2<f32>`、`kind: u32`、`pulse: u32`）、WGSL のシェーダ。**9.4 のフラグメントの規則と同じ**にする。リングの円弧と扇形セルは `egui::Painter` で描く |
| 時系列 | `egui_plot`（帯は塗りつぶしの多角形で描く） |
| ファイル | `rfd`（ネイティブのダイアログ）。Parquet は `arrow-array`・`parquet` で書く |
| 並列 | MC は `rayon` の `par_iter` で `mc::run_trial` を呼ぶ（`McMode::Independent`）。重い処理（MC、書き出し）は別スレッドで行い、`std::sync::mpsc` で結果を返して `ctx.request_repaint()` を呼ぶ |
| セッション | 1 ラウンドの計算は軽い（N02: 約 1 万ノードで 0.5 秒以内）ので UI スレッドで実行してよい。81,306 ノードで 100 ms を超えるなら別スレッドへ移す（P2-2 で実測） |

### 10.2 構成

```
desktop/src/
├─ main.rs  app.rs                 # eframe::App、全体の状態
├─ state.rs                        # 選択ノード、再生位置、表示モード（Web の store に相当）
├─ panels/{toolbar.rs, control.rs, network.rs, detail.rs, charts.rs, tree.rs}
├─ gpu/{mod.rs, nodes.wgsl, camera.rs}
├─ playback.rs                     # frames.ts と同じ規則（stateAtWave）。単体テストを書く
├─ worker.rs                       # MC・書き出しのスレッド
└─ export_parquet.rs               # ノード別イベントの Parquet
```

### 10.3 画面と挙動

画面のレイアウト・色・形・操作は **11 章の共通仕様に従う**。Web 版と違って `sim-api` を同じプロセスで直接呼ぶので、`everFlags` などの配列はコピーなしで参照できる。ノード別の効用ログは全ノード分をメモリに持つ（81,306 × 50 ラウンドで約 37 MB）。

### 10.4 性能の目標と実測（P2-2）

| 項目 | 目標 | 未達のときの代替 |
| --- | --- | --- |
| 10,000 ノードの一括描画 | 60 fps | — |
| 81,306 ノードの一括描画 | 30 fps 以上 | 全体図（扇形セル）の表示を既定にし、個別展開を `EXPAND_CAP` までに制限する |

測定は `desktop` の `--bench` オプション（ランダム配置のノード N 個を描いてフレーム時間を標準出力へ）で行い、結果を 15.3 に書く。

---

## 11. 画面の共通仕様（Web・デスクトップ共通）

### 11.1 `spec/ui-tokens.json`（このとおりに作る）

色は色覚多様性に配慮した Okabe–Ito の配色を使い、内部（青）と外部（緑）は色だけでなく**形（円と四角）**でも区別する（N08）。

```json
{
  "version": 1,
  "kinds": {
    "0": { "name": "unreceived",  "label": "未受信",   "shape": "circle", "fill": null,      "stroke": "#8A8A8A", "strokeWidth": 1.5 },
    "1": { "name": "received",    "label": "受信のみ", "shape": "circle", "fill": "#9A9A9A", "stroke": null },
    "2": { "name": "internal",    "label": "内部活性", "shape": "circle", "fill": "#0072B2", "stroke": null },
    "3": { "name": "external",    "label": "外部活性", "shape": "square", "fill": "#009E73", "stroke": null },
    "4": { "name": "both",        "label": "両方活性", "shape": "square", "fillSplit": ["#0072B2", "#009E73"], "split": "left-right" },
    "5": { "name": "ip",          "label": "IP",       "shape": "circle", "fill": "#0072B2", "stroke": "#111111", "strokeWidth": 3, "radiusScale": 2.2 }
  },
  "highlight": { "currentWave": "#E69F00", "selected": "#D55E00", "haloScale": 1.35 },
  "node": { "radiusPx": 4.0 },
  "series": { "internal": "#0072B2", "external": "#009E73", "both": "#CC79A7", "newShare": "#0072B2", "newXact": "#D55E00", "cumXact": "#009E73", "bandAlpha": 0.18 },
  "rings": { "stroke": "#B5B5B5", "dash": [3, 4], "label": "#666666" },
  "background": { "light": "#FFFFFF", "dark": "#14161A" }
}
```

### 11.2 レイアウト

最小の画面サイズは 1280×720。

| 領域 | 位置・大きさ | 内容 |
| --- | --- | --- |
| ツールバー | 上、高さ 48 px | 9.3 の `Toolbar` |
| 操作パネル | 左、幅 280 px | `ControlPanel` |
| ネットワーク表示 | 中央（残り全部） | `NetworkView`。下に再生の操作（再生・一時停止・コマ送り・速度）。右下に縮小地図 |
| ノード詳細 | 右、幅 340 px | `NodeDetail` |
| 下段 | 下、高さ 280 px | 左 2/3 が `RoundChart`、右 1/3 が `BranchTree` |

### 11.3 操作

| 操作 | 動作 |
| --- | --- |
| ノードをクリック | 単一ノードとして選択（`NodeDetail` が「ノード」タブに切り替わる） |
| リング名・扇形セルをクリック | そのリング／扇形を個別に展開する（`selectNodes`）。展開後は「全体図に戻る」ボタン |
| ドラッグ（Shift 押しながら） | 範囲選択 → 任意の群を作り、`NodeDetail` が「ノード群」タブに切り替わる |
| `NodeDetail` の群の選択肢 | 「内部のみ」「外部のみ」「両方」「受信のみ」 |
| メッセージのチップを選んで「発信」 | `step`。キーボード: `1`〜`6` でメッセージ選択、`Enter` で発信 |
| 再生 | `Space` で再生／一時停止、`←` `→` で 1 ウェーブ戻る／進む |
| ホイール／ドラッグ | ズーム／パン |
| 内部活性の表示切替 | 「累積」「このラウンドのみ」（既定は累積。要件書 9.1） |

---

## 12. 設定ファイルの仕様（`ScenarioConfig`）

形式は TOML（ファイル）と JSON（Web の API）。キー名は同じ。既存の `runner` の設定ファイルを読んだときは、`graph`・`ip_sampling`・`user_sampling`・`diffusion_sample_size`・`num_rounds` など**未知のキーを無視**する（`serde` の既定の動作。`deny_unknown_fields` を付けない）。欠けているキーは下の既定値になる（T-06）。

### 12.1 試作の既定（6 個のメッセージ＋機構の既定）

```toml
# scenario.toml（試作の既定）
receipt_probability = 0.5
calibration_rounds  = 5       # 反応性の scale = 1/(β·calibration_rounds)。実行 T とは別（INV-12）
max_rounds          = 50
user_seed           = 0
user_index          = 0
diffusion_seed      = 0

message_space = [
  { internal_stimulus = 0.20, external_stimulus = 0.20 },   # m0 両方高
  { internal_stimulus = 0.20, external_stimulus = 0.05 },   # m1 トレードオフ（内部寄り）
  { internal_stimulus = 0.15, external_stimulus = 0.10 },   # m2 トレードオフ
  { internal_stimulus = 0.10, external_stimulus = 0.10 },   # m3 中央一致
  { internal_stimulus = 0.10, external_stimulus = 0.15 },   # m4 トレードオフ
  { internal_stimulus = 0.05, external_stimulus = 0.20 },   # m5 トレードオフ（外部寄り）
]

internal_parameters = { beta = 0.8, gamma = 0.6, initial_utility = -1.0 }
external_parameters = { beta = 0.8, gamma = 0.6, initial_utility = -1.0 }

[mean_stimulus]
mode = "auto"                 # メッセージ空間の平均。"manual" のとき internal / external を指定

[mechanisms.m1]  enabled = true   kappa = 0.05
# M2〜M4 は廃止（2026-10-20）。M5（eta・n_max）は検討中のため、設定項目もコメントアウト
```

プリセット（`MechanismConfig` の関数。画面のボタンに対応）:

| プリセット | 内容 |
| --- | --- |
| 現行（すべて無効） | 5 つとも `enabled = false`（無効値）。バッチと一致する（INV-02） |
| 既定 | 上の TOML の値（要件書 6.6） |
| 既定の2倍 | κ を 2 倍（0.05 → 0.10）（15.1 の D8） |

### 12.2 検証規則（`ScenarioConfig::validate`）

| 項目 | 規則 |
| --- | --- |
| `message_space` | 1〜16 個、各刺激 ∈ [-1, 1]（`MessageSpace::try_from` を使う） |
| `receipt_probability` | [0, 1] |
| `calibration_rounds`・`max_rounds` | ≥ 1 |
| `internal/external_parameters` | `beta>0`、`gamma>0`、`gamma<beta`、`initial_utility ∈ [-1,1]`、平均刺激との差が 0 でない |
| 機構の値 | `ParamSpec` の `min`〜`max` の範囲内 |
| ログの大きさ | `n_nodes × (max_rounds + 1) × 9 B ≤ 1 GiB`（7.7） |
| `user_seed`・`diffusion_seed` | 2^53 未満（JS で扱えるように） |

### 12.3 `ParamSpec` 登録簿（`params::param_specs()` の内容）

画面は、この登録簿から機構のスライダーを**自動生成**する（N07。新しい機構を足すときは、ここに 1 行足すだけで画面に出る）。`off` は無効値、`default` は 6.6 の既定。

| id | 機構 | 表示名 | 型 | min | max | step | default | off |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `m1.kappa` | M1 購入者の口コミ | κ（外部行動時に内部効用へ加える値） | f64 | 0 | 0.5 | 0.01 | 0.05 | 0 |
| （廃止） | M2 摩耗・M3 S 字反応・M4 休止・忘却（`m2.rho`、`m3.sigma`、`m4.lambda`、`m4.budget_ratio`、`m4.allow_rest`） | 2026-10-20 に削除 | — | — | — | — | — | — |
| （検討中） | M5 複合感染（`m5.eta`、`m5.n_max`） | コードはコメントアウト。戻すときは行を足す | — | — | — | — | — | — |

---

## 13. テスト計画

すべて `cargo test --workspace` で動くこと（T-17 の WASM 側だけ別コマンド）。「要件」列は検証する要件・不変条件。

| ID | 内容 | 場所 | 要件 |
| --- | --- | --- | --- |
| T-01 | **カスケードの一致**: ランダムグラフ（ノード 40〜280、出次数 1〜6、60 種）× 受信確率 {0.3, 0.5, 1.0} × シード 5 種 × 8 ラウンドで、`engine::run_round`（機構なし）と `core::diffusion::diffuse` を並べて回し、毎ラウンド、`(n_xact, n_share, n_waves)` と**全ノードの効用（`to_bits`）・外部行動フラグ・乱数の次の出力**が一致する。既存側は `SmallRng`、`sim-api` 側は `SimRng`（64bit ホストでのみ実行）。付録 B-3 のプロトタイプがそのまま雛形 | `sim-api/tests/parity_core.rs` | N01, INV-02〜07 |
| T-02 | **MC の一致**: `run_mc(Sequential)`（機構なし）と `core::diffusion::monte_carlo_average` が、同じ rng・試行数・系列（6 個のメッセージから数通り）で `total_xact_mean`・`total_share_mean` を**ビット一致**で返す | `sim-api/tests/parity_core.rs` | N01 |
| T-02b | **バッチの 1 行の再現**: runner と同じ三重ループ（us_id → IP の位置 → 系列番号。7.12）で子の乱数を順に派生させたものと、`batch_row_rng` の返り値が、数通りの `(us_id, ip_pos, seq_index)` で一致する。その乱数を `run_mc(Sequential)` に渡した結果が、同じ乱数を `monte_carlo_average` に渡した結果と一致する | `sim-api/tests/parity_core.rs` | N01 |
| T-03 | **母集団の一致**: 複数の `(user_seed, user_index, β, γ, 較正 T, 平均刺激)` で、`sample_population` の `w_int`・`w_ext` が、`runner::brute_force::config::DecisionParameters::sample_iter::<DecisionAny>`（`SmallRng`、`user_sampling` と同じ `from_rng` の派生）の `weight()` と**ビット一致**する。外部側も `DecisionAny` を型引数にして `weight()` で比べる（`DecisionOnce` に取得関数がないため） | `sim-api/tests/parity_runner.rs`（`runner` を dev-dependency） | N01, INV-12 |
| T-04 | **乱数の同値**: 64bit ホストで、`SimRng` と `SmallRng` が (a) `seed_from_u64` 後の `next_u64` 1000 個、(b) `from_rng` で派生した子の出力、(c) `random::<f64>()` の `to_bits`、(d) `Pareto::new(0.25, 3.0).sample_iter` の 1000 個で一致する（付録 B-1 と同じ内容を恒久テスト化）。`cfg(target_pointer_width = "64")` で守る | `sim-api/src/rng.rs`（`#[cfg(test)]`） | INV-03 |
| T-05 | **グラフ読み込み**: `graph-lib/dataset/` の `sample_graph.{txt,csv,tsv}` と `.gz`、`simple.txt` を `load_graph` で読み、graph-lib の `ParseBuilder`（i32 ラベル）で読んだ `DiGraphB`／`UndiGraphB` と、ノード数・全ノードの `successors`・`predecessors` が一致する。`labels` が昇順。`transposed` の反転。コメント行・空行・区切りの混在。構文エラーで `GraphError::Parse{line}`（panic しない）。ハッシュが入力の順序に依存しない | `sim-api/tests/graph_io.rs` | INV-09 |
| T-06 | **設定**: 本書 12.1 の TOML が読める。リポジトリの `config.toml`（runner 形式）が未知のキーを無視して読める。12.2 の各規則の違反が日本語のエラーで返る（`gamma >= beta` など）。`MechanismConfig::off()` の `effective()` が無効値になる | `sim-api/tests/config.rs` | F02, F16 |
| T-07 | **強さ 0 の一致**: T-01 を (a) `MechanismConfig::off()`、(b) M1 が `enabled = true` で値だけ無効値（κ=0）の 2 通りで実行し、どちらも既存と一致する | `sim-api/tests/parity_core.rs` | INV-02 |
| T-08 | **M1**: 外部行動が成立した受信でだけ、κ が内部効用に**1 回**加わる。加えた後に内部判定される。κ を大きくしたとき効用が 1 を超える（上限なし） | `sim-api/src/node.rs` | M1, INV-15 |
| T-09〜T-11 | （欠番。M2・M3・M4 の廃止に伴い、2026-10-20 に削除） | — | — |
| T-12 | **経路数**: 2 つの親から同じウェーブで届く菱形グラフで `n_arr = 2`、片方だけなら 1。IP や前のウェーブの受信者への命中は数えない。M5 は無効なので、経路数が何本でも刺激は 1 本分（M5 の検討用に `n_arr` を数え続けている） | `sim-api/tests/parity_core.rs` ほか | （M5 検討用） |
| T-13 | **ログと分類**: 全ラウンド・全ノードで、ログの `EVER_*` が累積に一致し、`F_*_NOW` がそのラウンドの事象に一致する。`kind_of` の 2 つのモード。`group_series` のメンバーが `at_round` の分類と一致し、`size_by_round` が各行の分類と一致する。効用が丸められず 1 を超えて残る（M1 で κ を大きくする） | `sim-api/tests/session.rs` | F10, F19, INV-15 |
| T-14 | **全体図**: `compute_rings` が `core::diffusion::shortest_distances_from` と一致（到達不能は `u32::MAX`）。`ring_sizes` の合計 + `unreachable` = ノード数。`radial_layout`: 角度が [0, 2π)、同じ部分木のノードの `pos` が連続、決定的。`aggregate` の合計がリングの大きさと一致。`select_nodes` が上限を超えると `TooMany` | `sim-api/tests/overview.rs` | F11, N04 |
| T-15 | **巻き戻しの決定性**（P3）: n ラウンド進めてから `rewind_to(k)` し、同じ選択を再生すると、状態・ログ・イベント列が**ビット一致**する。別の選択で分岐しても、元の枝に戻って再生すると元と一致する | `sim-api/tests/session.rs` | F09 |
| T-16 | **イベントの不変条件**: 1 ラウンドで同じノードのイベントは最大 1 件。IP のイベントはない。`from` は直前のウェーブで `enqueued` だったノード（wave 0 は IP）。ウェーブ番号は単調。`enqueued` ⇒ `int_fired`。件数 = `n_received` | `sim-api/tests/parity_core.rs` | F06 |
| T-17 | **WASM と ネイティブの一致**: `spec/fixtures/golden_*.json`（`examples/make_golden.rs` がネイティブで作る: 設定・グラフ・IP・系列と、各ラウンドの `RoundSummary`、イベント列、`everFlags` の抜粋）を、`wasm-pack test --node` で同じ入力から再計算して比べる | `sim-wasm/tests/golden.rs` | N01, INV-03, D6 |
| T-18 | **Web の純粋関数**: `stateAtWave` の最終ウェーブが `kinds()` と一致、`k=-1` が開始前と一致。内部活性の 2 モード。vitest | `web/src/logic/*.test.ts` | F06, F07 |
| T-19 | **デスクトップの純粋ロジック**: `playback.rs` が T-18 と同じ入力で同じ出力。GPU を使わずに実行できるもの | `desktop/src/playback.rs` | F06 |
| T-20 | **書き出し**: ラウンド別 CSV の列が 7.13 の順序どおり。シナリオ記録 JSON を読み戻して同じ系列を再現できる。機構が有効なら `batch_toml` が `BatchIncompatible`、無効なら runner が読める | `sim-api/tests/export.rs` | F14 |

ゴールデン用のグラフは、`sim-api/examples/make_fixture.rs` が決定的に作る（ノード 300、各ノードから平均 4 本の有向辺、シード 1）。出力は `spec/fixtures/random300.txt`。

---

## 14. 実装タスク（この順に進める）

各タスクの「完了条件（DoD）」がすべて満たされるまで次へ進まない。コマンドは 0 章の共通コマンドに加えて実行するもの。

### P0 共通エンジン（目安 3〜4 週）

| ID | タスク | 参照 | 依存 | 完了条件（DoD） |
| --- | --- | --- | --- | --- |
| P0-1 | **足場**: ルート `Cargo.toml` の `members` に `sim-api` `sim-wasm`（desktop は P2）を追加。`.cargo/config.toml`（6.2）。`sim-api`・`sim-wasm` の空の crate。`.github/workflows/ci.yml`（fmt・clippy・test、WASM ビルド）。0 章の確認コマンドの結果を 15.3 に記録 | 0, 5, 6 | — | `cargo build --workspace` が通る。`cargo build -p sim-api --target wasm32-unknown-unknown` と `wasm-pack build sim-wasm --target web` が通る（ローカルにターゲットがなければ CI で通ることを確認し、結果を 15.3 に書く）。`git diff --stat` に既存ファイルが `Cargo.toml` 以外出ない |
| P0-2 | **乱数**: `rng.rs`（`SimRng`） | INV-03 | P0-1 | T-04 が通る |
| P0-3 | **設定と登録簿**: `config.rs`（型・検証・`effective`・プリセット）、`params.rs`（12.3 の登録簿） | 7.2, 12 | P0-1 | T-06 が通る |
| P0-4 | **グラフ**: `graph_io.rs`、`csr.rs`、`examples/make_fixture.rs`（`random300.txt`） | 7.10 | P0-1 | T-05 が通る。`load_graph` が `graph-lib/dataset/` の全形式を読める |
| P0-5 | **ノードと母集団**: `node.rs`（`contact`、`decide`）、`population.rs`（`sample_population`、`validate`） | 7.4, 7.6 | P0-2, P0-3 | T-03、T-08 が通る |
| P0-6 | **エンジン**: `engine.rs`（`run_round`、`apply_forgetting`、`rest_round`、`Observer`、`NoTrace`、`Scratch`） | 7.3, 付録 B-3 | P0-4, P0-5 | T-01、T-07、T-12、T-16 が通る。**T-01 が通るまで他の機能を足さない** |
| P0-7 | **セッションとログ**: `session.rs`、`log.rs`（`UtilityLog`、`NodeKind`、`RoundSummary`、`EventColumns`、`node_detail`、エラー） | 7.5, 7.7 | P0-6 | T-13 が通る |
| P0-8 | **全体図・ノード群・IP 順位**: `overview.rs`、`groups.rs`、`centrality.rs` | 7.8, 7.9, 7.11 | P0-7 | T-14 が通る。Twitch 規模（約 7,000 ノード）の `radial_layout` が 100 ms 以内（`cargo test --release` で計測して 15.3 に記録） |
| P0-9 | **MC**: `mc.rs`（`run_mc`、`run_trial`、`batch_row_rng`） | 7.12 | P0-7 | T-02、T-02b が通る |
| P0-10 | **書き出し**: `export.rs` | 7.13 | P0-7, P0-9 | T-20 が通る |
| P0-11 | **sim-wasm**: 8.1 の API。ゴールデンの生成と検証 | 8 | P0-10 | T-17 が通る（`wasm-pack test --node`）。`wasm-pack build` の出力を `web/` から読み込める |

**P0 の完了判定**（要件書 11 章）: 機構をすべて無効にすると、既存版と同じシードで合計値が一致する（T-01、T-02、T-07）。WASM とネイティブの両方でビルドが通る（P0-1、P0-11）。

### P1 Web 最小版（目安 2〜4 週）

| ID | タスク | 参照 | 依存 | 完了条件（DoD） |
| --- | --- | --- | --- | --- |
| P1-1 | **足場**: Vite・TypeScript・React・zustand、Worker とプロトコル（`protocol.ts`）、`sim-wasm` の読み込み。Worker 経由で `loadGraph` → `createScenario` → `setIp` → `step` が往復する最小の画面 | 8.2, 9.1, 9.2 | P0-11 | `npm run build` と `npm run test` が通る。`random300.txt` を読み込んで 1 ラウンド回せる |
| P1-2 | **NetworkView**: WebGL2 描画（9.4）、ヒットテスト、パン・ズーム、`tokens.ts`、`spec/ui-tokens.json` の作成 | 9.4, 11.1 | P1-1 | 300 ノードを 5 種の状態で描ける（目視）。1 万ノードで 60 fps（`performance.now()` でフレーム時間を計測して 15.3 に記録） |
| P1-3 | **全体図と展開**: リング・扇形セルの重ね描き、クリックで展開、縮小地図、到達不能の枠 | 7.8, 9.4 | P1-2 | 1 万ノード超のグラフで全体図から始まり、扇形を展開できる。`EXPAND_CAP` 超過の表示 |
| P1-4 | **操作パネル**: グラフ設定、シナリオ設定、機構（`paramSpecs` から自動生成）、プリセット、IP 選択（順位表・クリック）、メッセージパレット、「発信」 | 9.3, 12 | P1-1 | 設定を変えるとセッションが作り直され、その確認が出る（INV-14）。「現行（すべて無効）」で T-01 相当の値が出る（`sim-api` のテストと同じ合計） |
| P1-5 | **再生**: `stateAtWave`、再生・一時停止・コマ送り・速度・スキップ、現在のウェーブの強調 | 9.5 | P1-2, P1-4 | T-18 が通る。再生終了時の画面が `kinds()` と一致 |
| P1-6 | **グラフ類**: `RoundChart`、`NodeDetail`（単一・群）、効用に上限を付けない縦軸 | 9.6 | P1-4 | M1 の κ を大きくして効用が 1 を超えても折れ線が切れない（目視）。群の帯が表示される |
| P1-7 | **書き出しと比較**: シナリオ記録 JSON、ラウンド別 CSV、イベント CSV。「すべて無効／既定／既定の2倍」の流し比べの手順を `docs/p1-compare.md` に書く | 7.13, 9.8 | P1-4 | 3 水準の記録を書き出せる。再読み込みして同じ系列を再現できる（T-20） |

**P1 の完了判定**: 9.8 を満たし、M1 の既定の強さを確認・調整した結果を 15.3 に反映する。

### P2 デスクトップ版（目安 3〜4 週）

| ID | タスク | 参照 | 依存 | 完了条件（DoD） |
| --- | --- | --- | --- | --- |
| P2-1 | **足場**: `desktop` crate（eframe）、11.2 のレイアウト、ファイルダイアログ、設定の読み込み | 10.1, 10.2, 11.2 | P0-10 | `cargo run -p desktop` で空のレイアウトが出る。グラフを読み込める |
| P2-2 | **GPU 描画**: `CallbackTrait`、WGSL（9.4 の規則）、カメラ、ヒットテスト。**`--bench` で性能を実測** | 9.4, 10.4 | P2-1 | 10.4 の目標を測り、結果と判断を 15.3 に書く（未達なら代替を実装する） |
| P2-3 | **操作と再生**: 操作パネル（`paramSpecs` から生成）、IP 選択、メッセージ、再生、全体図と展開 | 9.3〜9.5, 11.3 | P2-2 | T-19 が通る。Twitter 規模（81,306 ノード）で「全体図 → 展開 → 発信」が動く |
| P2-4 | **グラフ類**: `egui_plot` で `RoundChart`・`NodeDetail`（単一・群）。帯、上限なしの縦軸 | 9.6 | P2-3 | Web 版と同じ設定・シードで同じ数値が表示される（T-17 のゴールデンと一致） |
| P2-5 | **書き出しと MC**: JSON・CSV・Parquet、MC 併記（`rayon`、`Independent`）の別スレッド実行 | 10.1, 7.12 | P2-3 | Parquet を既存の Python（Polars）で読める。MC 実行中も UI が固まらない |

**P2 の完了判定**: Twitter 規模で全体図→拡大→発信ができ、Web 版と同じ設定・シードで**集計値とゴールデンの数値が一致**する（15.1 の D6 の基準）。

### P3 研究機能・発表（目安 2〜3 週）

| ID | タスク | 参照 | 依存 | 完了条件（DoD） |
| --- | --- | --- | --- | --- |
| P3-1 | **巻き戻しと分岐**（F09）: `rewind_to`、`BranchTree`、枝の切り替え、ツリーの画面（Web・desktop） | 7.7 | P1, P2 | T-15 が通る |
| P3-2 | **MC の併記**（F12）: 同じ系列を N 回回し、時系列に平均と幅を重ねる | 7.12 | P1, P2 | 1 回の実現が MC の帯の中にあるか確認できる |
| P3-3 | **2 画面比較**（F13）: 同じシードで 2 シナリオを左右に並べて同時再生 | 9, 11.2 | P1, P2 | 設定だけを変えた 2 つが同時に再生される |
| P3-4 | **動画・画像**（F15）: Web は `canvas.captureStream` と `MediaRecorder` で WebM、desktop は PNG の連番（任意で ffmpeg）。時系列は PNG/SVG | — | P1, P2 | ゼミ・発表でデモができる |
| P3-5 | **`runner` の `verify` サブコマンド**: 機構が有効な設定で、**指定した系列のリスト**を `sim-api::mc` で MC 検証し、`mean_num_xact`・`mean_num_share` を既存と同じ Parquet のスキーマで出す（15.1 の D2）。`runner` を変更する唯一の例外タスク。着手前に人間の許可を取る | 7.12, 7.13 | P0-9 | 機構を無効にした設定で、`brute-force` の同じ系列の値と一致する |

### 保留

M6（適応方策、F17）。必要になった時点で別途要件定義を行う。

---

## 15. 要件書との差分・決定・未決事項

### 15.1 要件書との差分（本書の判断）

| ID | 論点 | 要件書の記述 | 本書の決定 | 理由 |
| --- | --- | --- | --- | --- |
| D1 | 拡張機構の実装場所 | 8.3 は `sim-api`、11 章 P0 は `core` と、**食い違っている** | **`sim-api` に実装し、`core` は変更しない** | `core` を変えるとバッチの結果と性能が変わる危険がある。N01 は「強さ 0 でビット一致」のテスト（T-01・T-07）で担保する |
| D2 | バッチ用設定（F14）と機構 | 見つけた系列をバッチで MC 検証する | 既存の `brute-force` は M1 を知らない。機構が無効のときだけバッチ用 TOML を出し、有効なときは **P3-5 の `verify` サブコマンド**で検証する | 検証手段がないと「発見→検証」の流れが成立しない |
| D3 | Web の描画 | sigma.js または Cosmograph | 自前の WebGL2 | 半分塗りノードとリング配置の専用描画が必要で、汎用ライブラリの利点が小さい（9.1） |
| D4 | 乱数 | （記述なし。既存は `SmallRng`） | `Xoshiro256PlusPlus` を明示 | `SmallRng` は WASM（32bit）で別アルゴリズムになる。64bit の `SmallRng` と一致することを確認済み（付録 B-1） |
| D5 | 平均刺激 s̄ | 「0.133 に合わせる」 | **メッセージ空間の平均**（0.8/6 = 0.1333…）をそのまま使う（`mean_stimulus = auto`） | バッチも同じ計算で s̄ を決めている。0.133 は丸めた表示 |
| D6 | Web とデスクトップの一致 | P2 の完了判定「同じ結果が出る」 | **ゴールデンの数値の一致**を基準にする（T-17）。初期の重みの `powf` だけは、WASM とネイティブで最後の 1 ビットが違う可能性が残る。判定が割れたら頻度を測って 15.3 に記録する | M3・M2 は廃止したので、残るのは `powf` だけ |
| D7 | M1 の κ を加えるタイミング | 「外部決定の成立時に内部効用へ κ を加える」 | 外部判定の直後、**内部判定の前**に加える（7.4 (6)） | 購入者がその場でシェアできる。要確認 |
| D8 | 「既定の2倍」 | 「既定の2倍」 | κ を 2 倍（12.1）。M2〜M5 を含んでいた旧版の定義は廃止（15.4） | 旧版は「強さ」の向きが機構ごとに違うため。いまは M1 だけなので単純 |
| D9 | MC の乱数 | MC 平均を併記 | 2 モード: `Sequential`（バッチ照合用）と `Independent`（並列・画面用） | バッチと一致させる検証と、並列化の両立 |
| D10 | メッセージ番号の順序 | — | 6 個の空間では m0 が最大、m1 が最小で、番号は外部刺激の昇順になっていない。**分析で「単調な系列」を数えるときは外部刺激の順に並べ直す** | 研究メモの落とし穴 |
| D11 | m∅ の表記 | — | **廃止**（旧版: 系列文字列では `-`）。いまは `-` を不正な文字として弾く | m∅ の廃止（15.4） |
| D12 | リングとウェーブ | 「ホップ再生」「ホップ距離」 | 2 つを別の概念として扱う（2 章） | 距離はグラフの静的な値、ウェーブは確率的な実現 |
| D13 | 効用の統計と記録量 | 「約 32MB」 | 効用の統計は**受信済みノード**と**IP 以外の全ノード**の 2 系統。記録量はフラグを含めて約 37 MB | 全ノードの平均は未受信（`u0`）に引きずられる |
| D14 | IP 候補の順位 | 次数・PageRank・VC | VC はアプリ内で計算せず、事前計算の CSV から読む | 計算が重い |
| D15 | PageRank の向き | — | 拡散グラフの**逆向き**（入力ファイルの向き）で計算 | 過去の実装（`reverse_edges=False`）と一致させる |
| D16 | 画面の乱数とバッチの乱数 | N01「同じシードならバッチと一致」 | `Session::new` の乱数はバッチの特定の行とは**別物**。バッチの 1 行は `batch_row_rng` ＋ `Session::with_rng` で再現する（7.7・7.12） | バッチは系列ごとに別の子の乱数を使うため。N01 は T-01・T-02・T-02b の 3 段で担保する |

### 15.2 ユーザーが決めたこと（2026-10-02、要件書 12 章より）

| 項目 | 決定 |
| --- | --- |
| 今回の範囲 | 現行の Rust シミュレーションを土台に、M1 を適用した探索・可視化用シミュレータを作る（当初は M1〜M5。2026-10-20 に M1 のみへ整理、15.4）。M6 は保留。拡張版は今後 |
| 技術構成 | Web 版（Rust→WASM＋TypeScript）とデスクトップ版（Rust の egui）の両方を作り、それぞれ設計する |
| グラフ | 利用者が自分で設定する。大規模なら全体図から始めて拡大・展開する |
| 全体図 | IP からのホップ距離でまとめる（同心円のリング）。コミュニティ検出は使わない |
| ノードの色分け | 内部活性は青の円、外部活性は緑の四角、両方活性は四角を左右半分ずつ青と緑に塗り分け |
| ノード詳細 | 単一ノードと、活性の種類別（内部のみ・外部のみ・両方）のノード群の 2 モード。効用は 1 を超えても上限なしで表示 |
| メッセージ空間 | 試作は 6 個（12.1）。増やすときはそのつど明記する。平均刺激は両軸とも約 0.133 |
| 反応性 | 現行のパレート分布（β=0.8、γ=0.6、scale=1/(β·T)）をそのまま維持する |
| 拡張機構 | 当初: M1〜M5 を試作から適用。**2026-10-20 に変更: M1 のみ残す。M5 はコメントアウトして今後検討、M2〜M4 は廃止**（15.4）。画面でオン・オフできる。M6 は保留 |

### 15.3 未決事項（AI は実装中に分かったことをここへ追記する）

| # | 内容 | 状態・暫定の扱い |
| --- | --- | --- |
| Q1 | 試作に使う小グラフ（congress 475 ノード、Twitch-ES 4,648 ノードなど） | 未決。開発とテストは `random300.txt`（決定的な合成）で行う。P1 の比較実験の前にユーザーが選ぶ |
| Q2 | 内部活性の既定の表示（累積か、そのラウンドのみか） | **累積を仮置き**。切り替えボタンを付ける |
| Q3 | M1 の既定の強さ（12.3 の `default`） | 仮置き（κ=0.05）。P1-7 の 3 水準の比較で調整する（M2〜M5 の強さは廃止・保留） |
| Q4 | egui で 8 万ノードの一括描画の性能 | P2-2 で実測。結果と判断をここに書く |
| Q5 | M1 の κ を内部判定の前に加えてよいか（D7） | **前を仮置き**。ユーザーに確認する |
| Q6 | `getrandom` の WASM 設定（6.2）で実際にビルドが通るか | **未検証**。P0-1 で確認し、結果をここに書く |
| Q7 | ルート `Cargo.toml` の実際の内容、`graph-lib` が submodule か | **未確認**。0 章のコマンドで確認し、結果をここに書く |
| Q8 | M4 の予算の基準「計画ラウンド数」（`horizon`）の既定 | **消滅**（M4 の廃止に伴い `horizon` と入力欄を削除。15.4） |
| Q9 | VC の CSV の形式（`node` は元のラベル。列名・区切り） | 7.11 のとおり仮置き。実際の事前計算ファイルを見て合わせる |
| Q10 | 「既定の2倍」の定義（D8） | κ を 2 倍に単純化（M2〜M5 の廃止後） |


### 15.4 拡張機構の整理（2026-10-20、版 1.1）

ユーザーの決定: **M1 は残す。M5 はコードをコメントアウトして今後検討する。M2・M3・M4（忘却・投稿予算・m∅・計画ラウンド数）は廃止する。**

| 機構 | 決定 | 理由（根拠文献と本研究の意思決定モデルとの照合） |
| --- | --- | --- |
| M1 購入者の口コミ | **残す** | 内部と外部の 2 軸をつなぐ。研究の問い（拡散を促す投稿と行動を促す投稿の組み合わせ）に直結し、新しい仮定が小さい |
| M2 摩耗 | 廃止 | 元の更新式（刺激の指数移動平均）がすでに反復の頭打ちを含む。M2 が足すのは「下り」だけで、広告の「同じ素材の繰り返し」と、本研究の「訴求の種類を表す刺激ベクトル」は別物。根拠（Schmidt & Eisend 2015）の最大約 10 回に対し、既定の反応性では最大が 4〜7 回目になり、合わない |
| M3 S 字反応 | 廃止 | 個人差は反応性 w、偶然性は受信確率がすでに担う。根拠に挙げた論文（Feinberg 2001）は「連続時間では S 字反応でも周期的なパルシングは最適にならない」と示す論文で、引用が逆向きだった。接触のたびに抽選する形は、外部行動（生涯 1 回）を累積で起きやすくする偏りも生む |
| M4 休止・忘却・予算・m∅ | 廃止 | 記憶は移動平均にすでにある。根拠（Aravindakshan & Naik 2015）は「忘却は遅れて始まる」ことを示すが、実装は直ちに減衰する。戻り先が初期効用 −1 で、1 ラウンド受信しないだけで閾値を割る（u=0.05 → −0.055）。予算と m∅ は IP の行動の選択肢であってユーザーのモデルではない |
| M5 複合感染 | コメントアウト（検討中） | 元の `diffuse` は同じラウンドの 2 本目以降の到達を捨てている（`rcvd` が立つとそれ以上は数えない）。M5 は、その扱いを新しい仮定として足す。再検討するときは、複数到達の扱い（刺激の強化か、到達回数ぶんの更新か、ラウンドをまたぐ累積か）から決める |

- **実装の変更**（`sim-api`）: `MechanismConfig` は `m1` のみ（`m5` はコメント）。`Mech` は `kappa` のみ。`NodeState` から `last_msg`・`wear` を削除。`decide` は閾値判定のみで乱数を引かない。`apply_forgetting`・`rest_round`、`Choice::Rest`、`SessionError::{BudgetExhausted, RestNotAllowed}`、`ScenarioConfig.horizon` を削除。`n_arr`（経路数）の記録はノード詳細の表示と M5 の検討のために残す。
- **互換性**: 古い設定・記録の JSON の `m2`〜`m5`・`horizon` は無視して読める。系列文字列の `-`（m∅）は不正な文字になる。
- **不変条件**: M1 を無効にしたときの `core::diffusion` とのビット一致（T-01・T-02・T-07）は維持する。T-09〜T-11 は欠番、T-12 は経路数の確認に縮小。
- **画面**: 機構の欄は M1 のみ。m∅ のボタン・キー `0`・投稿予算の表示・「計画ラウンド数」の入力欄（A・B とも）を削除。
- **要件書との関係**: 要件書（PDF）の 6.5・6.6 の M2〜M5 の記述、F03（m∅）、F16（M1〜M5 の切り替え）は、この節に従って読み替える。要件書の本体は未改訂。

---

## 付録 A. `CLAUDE.md` に貼る短縮版

リポジトリのルートの `CLAUDE.md` に追記する（既にあれば末尾に追加）。Claude Code は起動時にこのファイルを自動で読むので、本書への入口になる。

````markdown
## 情報拡散ライブビューアの開発

- 設計書: `claude/viewer-design.md`。作業の前に **0 章・3 章（不変条件）・14 章（タスク表）** を読む。1 回の作業は 14 章の 1 タスクとし、完了条件（DoD）を満たしてから次へ進む。
- 既存の `core/` `runner/` `graph-lib/` `util/` `src/` は変更しない（例外は 14 章の P3-5 のみ。着手前に人間の許可を取る）。
- 乱数は `sim-api` では `Xoshiro256PlusPlus`（`SimRng`）だけを使う。`SmallRng` を使わない。辺ごとに乱数を必ず 1 回引く（`&&` で短絡しない）。
- 新しい crate は `core` を `sim-core = { package = "core", path = "../core" }` と名前を変えて依存する。
- 拡張機構は M1 だけ（M2〜M4 は廃止、M5 は検討中でコメントアウト）。M1 を無効にしたとき、既存の `core::diffusion` と**ビット一致**すること（T-01・T-02・T-07）。
- 仕様が足りないときは、推測で実装せず、既定の案を採って `// 仮定:` を付け、設計書 15.3 に追記する。
- 完了ごとに `cargo fmt --all && cargo clippy -p sim-api -p sim-wasm -- -D warnings && cargo test --workspace`。
- コメントと UI の文言は日本語、識別子は英語。
````

最初に Claude Code へ渡す指示の例:

> `claude/viewer-design.md` を読み、0 章の確認コマンドを実行して結果を 15.3 に記録してください。そのあと P0-1 から順に進めます。各タスクの完了条件を満たしたらコミットし、次のタスクに進む前に結果を教えてください。

---

## 付録 B. 検証記録（この設計書を書くときにサンドボックスで実行したもの）

> **注（2026-10-20）**: 以下は 1.0 版を書いたときの記録で、M2〜M5 を含む当時の設計に対するものである。M2〜M4 の廃止、M5 のコメントアウトに伴い、該当する記述（M2 の摩耗列、M3 の確率判定、M4 の忘却、M5 のひし形など）は現行の実装には無い。経緯は 15.4。

Rust 1.97.0（x86_64 Linux）、`rand` 0.9.5、`rand_xoshiro` 0.7、`rand_distr` 0.5.1 で実行した。WASM ターゲット（`wasm32-unknown-unknown`）は、サンドボックスの外部ネットワークの制限で入れられず、**WASM の実ビルドは未検証**である。

### B-1. `SmallRng` と `Xoshiro256PlusPlus`（INV-03・D4）

- `rand` 0.9.5 の `src/rngs/small.rs` に、`SmallRng` が **64bit では `Xoshiro256PlusPlus`、32bit では `Xoshiro128PlusPlus`** を使うと書かれている（`target_pointer_width` で分岐）。WASM（`wasm32`）は 32bit なので、`SmallRng` のままだと WASM とネイティブで乱数列が変わる。
- 64bit ホストで、次がすべて一致した: `seed_from_u64` 後の `next_u64` 1000 個（シード 0・1・42・123456789）、`from_rng` で派生した子 50 個の出力各 20 個、`random::<f64>()` の `to_bits` 1000 個、`Pareto::new(0.25, 3.0).sample_iter` の 1000 個。
- 結論: `SimRng = Xoshiro256PlusPlus` なら、64bit ネイティブの既存バッチ（`SmallRng`）と同じ乱数列になり、WASM でも同じ乱数列になる。

### B-2. パッケージ名 `core`（INV-08）

パッケージ名 `core` の crate を `core = { path = … }` のまま依存した crate で `thiserror` の `#[derive(Error)]` を使うと、`error[E0433]: cannot find 'write' in 'core'`・`cannot find 'fmt' in 'core'` でコンパイルに失敗した。`sim-core = { package = "core", path = … }` と名前を変えると、同じコード（`serde`・`thiserror`・`wasm-bindgen` の derive/マクロ）が通った。

### B-3. ウェーブの終わりにまとめて更新する方式（7.3）

次のプロトタイプで、既存の `diffuse` を**書き写した関数**（`core` の実物を呼んだのではなく、`core/src/diffusion.rs` の処理を `Vec<Vec<usize>>` の隣接表に移したもの）と、**合計値・全ノードの効用（`to_bits`）・外部行動フラグ・乱数の次の出力**がすべて一致した（ランダムグラフ 60 種 × 受信確率 {0.3, 0.5, 1.0} × シード 5 種 × 8 ラウンド、受信イベント 29,311 件を照合。そのうち `n_arr > 1`（M5 が作用する）のイベントが 1,599 件）。既存側は `SmallRng`、プロトタイプ側は `Xoshiro256PlusPlus`。機構（M1〜M5）は含まない。T-01 の雛形としてそのまま使える。**実行できる完全なコードは `claude/viewer-parity-prototype.rs`**（依存は `rand = "0.9"`、`rand_xoshiro = "0.7"`、`approx = "0.5"`、edition 2024。`cargo run --release` で `PARITY OK` が出る）。下はその中心となる関数である。

```rust
// 既存 diffuse と同じ乱数の引き方・処理順で、状態更新だけをウェーブの終わりにまとめる。
fn newalg(succ: &[Vec<usize>], p: f64, st: &mut [S], ip: usize, msg: (f64, f64),
          rng: &mut impl Rng, tr: &mut Trace) -> (usize, usize, usize) {
    let n = succ.len();
    let mut rcvd = vec![false; n];
    let mut recv_wave = vec![u32::MAX; n];
    let mut arr = vec![0u8; n];
    let (mut curr, mut next) = (vec![ip], Vec::new());
    rcvd[ip] = true;
    let (mut nx, mut ns, mut wave) = (0, 0, 0u32);
    while !curr.is_empty() {
        let mut newly: Vec<(usize, usize)> = Vec::new();
        while let Some(i) = curr.pop() {
            for &j in &succ[i] {
                let hit = p > rng.random::<f64>();                 // 必ず 1 回引く
                if hit {
                    if !rcvd[j] { rcvd[j] = true; recv_wave[j] = wave; arr[j] = 1; newly.push((j, i)); }
                    else if recv_wave[j] == wave { arr[j] = arr[j].saturating_add(1); }
                }
            }
        }
        for &(j, from) in &newly {                                   // 初回受信の順
            let s = &mut st[j];
            s.ui = s.wi * s.ui + (1.0 - s.wi) * msg.0;
            s.ue = s.we * s.ue + (1.0 - s.we) * msg.1;
            let ext = if !s.ext_done { let d = decided(s.ue); if d { s.ext_done = true; } d } else { false };
            let int = decided(s.ui);
            if ext { nx += 1; }
            if succ[j].len() > 0 && int { ns += 1; next.push(j); }
            tr.wave.push(wave as u16); tr.node.push(j as u32); tr.from.push(from as u32); tr.n_arr.push(arr[j]);
        }
        std::mem::swap(&mut curr, &mut next);
        wave += 1;
    }
    (nx, ns, wave as usize)
}
```

### B-4. 未検証のもの

- `wasm32-unknown-unknown` での実ビルド（`getrandom` の設定、`wasm-pack`、`wasm-bindgen`）。
- WASM とネイティブの `powf` の最後の 1 ビットの差が、実際に判定を割るか（D6）。
- M1〜M5 を入れた実装の、実グラフ・実 `core` に対する挙動。§7.3・§7.4 のコードは使い捨てクレートでコンパイルし、単体テスト 7 件（機構 OFF で原実装と一致、M5 のひし形で同一波の到着 2 件、トレース不変条件、M2 の摩耗列とリセット、M1 の κ が内部判定の前に 1 回だけ加わること、M4 の忘却式と休み、M3 が u=0 で確率 1/2）が通った。ただし原実装は転記であり、実 `core` との一致は T-01 で確認する。
- 要件書にある性能目標（N02・N03・N04）。P1・P2 で実測する。
