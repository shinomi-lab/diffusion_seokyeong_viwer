//! 1 ラウンドのカスケード（7.3）。
//!
//! 既存の `diffuse` の乱数の引き方・処理順をそのまま写し、「受信した瞬間に状態を更新する」ところだけを
//! 「ウェーブの終わりにまとめて更新する」に変える（M5 の経路数 n を数えるため）。
//! 機構なしのとき `core::diffusion::diffuse` とビット一致する（T-01）。

use rand::Rng;
use sim_core::message::Message;

use crate::config::{Mech, RecMethod, RecommenderConfig};
use crate::csr::Csr;
use crate::population::Population;
use crate::rng::SimRng;

const NONE: u32 = u32::MAX;

/// 推薦で受信したときの `from`（`recommender-design.md` 6.3）。
pub const FROM_REC: u32 = u32::MAX;

/// 受信の経路。
pub const VIA_FOLLOW: u8 = 0;
/// 推薦の段階で直接届いた。
pub const VIA_REC: u8 = 1;
/// 推薦で届いた人（またはその先）からフォロー関係をたどって届いた。
pub const VIA_RECCHAIN: u8 = 2;

/// ラウンドをまたいで使い回す作業領域。
pub struct Scratch {
    rcvd: Vec<bool>,
    recv_wave: Vec<u32>,
    arr: Vec<u8>,
    curr: Vec<u32>,
    next: Vec<u32>,
    /// (受信ノード, 初回の送信元)
    newly: Vec<(u32, u32)>,
    /// 受信ノードの経路（VIA_*）。推薦 on のラウンドでだけ使う。
    via: Vec<u8>,
    /// 社会的証明の数: このラウンドの ① で、そのノードに送った人（IP を含む）の数（`recommender-v2-design.md` 6.2）。
    /// 推薦 on・CF＋グラフのラウンドでだけ確保して使う。
    social: Vec<u32>,
    /// Rモデル: そのウェーブのシェア者の控え（`curr` は pop で空になるため）と、その出次数の累積和。
    sharers: Vec<u32>,
    deg_cum: Vec<u32>,
    /// Rモデル: 直近のラウンドのウェーブごとの推薦の推移（表示用。`recommender-v3-design.md` 6.3）。
    wave_trace: Vec<WaveTraceRow>,
}

/// Rモデルの 1 ウェーブ分の推薦の記録。
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize)]
pub struct WaveTraceRow {
    pub wave: u32,
    /// このウェーブの反応率 ê。
    pub e_hat: f64,
    /// 推薦量 B_s（丸める前）。
    pub budget: f64,
    /// 推薦枠の数 n_s。
    pub slots: u32,
    /// 空振りの数。
    pub miss: u32,
}

/// Rモデル（方式 wave）の 1 ラウンド分の文脈（`recommender-v3-design.md` 6.2）。
pub struct WaveCtx<'a> {
    /// 推薦専用の乱数（INV-W2: 拡散の乱数は引かない）。
    pub rng: &'a mut SimRng,
    /// R3 の事前分布の平均 a_t（R3 を使わなければ ē0）。
    pub a_t: f64,
    /// 基準の反応率 ē0。
    pub ebar0: f64,
}

/// Rモデルのラウンド内の累積（ê の分子・分母と空振り）。
#[derive(Default)]
struct WaveAcc {
    /// シェアを決めた人数（フォロワー 0 人も含む。IP は含まない）。
    cum_h: u32,
    /// 受信した人数。
    cum_c: u32,
    miss: u32,
}

impl Scratch {
    /// 直近のラウンドの推薦の推移（Rモデル以外では空）。
    pub fn wave_trace(&self) -> &[WaveTraceRow] {
        &self.wave_trace
    }
}

impl Scratch {
    pub fn new(n: usize) -> Self {
        Self {
            rcvd: vec![false; n],
            recv_wave: vec![NONE; n],
            arr: vec![0; n],
            curr: Vec::new(),
            next: Vec::new(),
            newly: Vec::new(),
            via: vec![VIA_FOLLOW; n],
            social: Vec::new(),
            sharers: Vec::new(),
            deg_cum: Vec::new(),
            wave_trace: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RoundCounts {
    pub n_received: u32,
    pub n_share: u32,
    pub n_xact: u32,
    pub n_waves: u32,
    /// 推薦の段階で直接届いた人数・そのうちシェア（次のウェーブへ送る）・外部行動。
    pub rec_received: u32,
    pub rec_share: u32,
    pub rec_xact: u32,
    /// 推薦由来の連鎖で届いた人数・そのうち外部行動。
    pub chain_received: u32,
    pub chain_xact: u32,
    /// Rモデル: 推薦の空振りの数と、ラウンド終了時点の反応率 ê（R3 の更新に使う）。他の方式では 0。
    pub rec_miss: u32,
    pub e_hat_end: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct ReceiveEvent {
    pub wave: u32,
    pub node: u32,
    pub from: u32,
    pub n_arr: u8,
    /// 更新後の効用。
    pub u_int: f64,
    pub u_ext: f64,
    pub int_fired: bool,
    pub ext_fired: bool,
    /// 次のウェーブでシェアする。
    pub enqueued: bool,
    /// 受信の経路（VIA_FOLLOW / VIA_REC / VIA_RECCHAIN）。
    pub via: u8,
}

pub trait Observer {
    fn on_receive(&mut self, ev: &ReceiveEvent);
}

/// MC 用。コストゼロ。
pub struct NoTrace;

impl Observer for NoTrace {
    #[inline(always)]
    fn on_receive(&mut self, _: &ReceiveEvent) {}
}

#[allow(clippy::too_many_arguments)]
pub fn run_round<O: Observer>(
    csr: &Csr,
    p: f64,
    pop: &mut Population,
    sc: &mut Scratch,
    ip: u32,
    msg_id: u8,
    msg: &Message,
    mech: &Mech,
    round: u32,
    rng: &mut SimRng,
    obs: &mut O,
) -> RoundCounts {
    run_round_rec(
        csr, p, pop, sc, ip, msg_id, msg, mech, None, None, round, rng, obs,
    )
}

/// 推薦付きの 1 ラウンド（`recommender-design.md` 6.3）。
/// ① フォロー経由の連鎖（今と同じ）→ ② 推薦の段階（スコア ≥ θ の未受信者）→ ③ 推薦から始まる連鎖。
/// `rec = None` のときは ① だけで、今の処理と完全に同じ（INV-R1）。推薦は拡散の乱数を引かない（INV-R2・INV-W2）。
/// 方式 wave（Rモデル）では ② ③ を行わず、① の各ウェーブのあとに推薦する。`wave_ctx` はそのときだけ使う。
#[allow(clippy::too_many_arguments)]
pub fn run_round_rec<O: Observer>(
    csr: &Csr,
    p: f64,
    pop: &mut Population,
    sc: &mut Scratch,
    ip: u32,
    msg_id: u8,
    msg: &Message,
    mech: &Mech,
    rec: Option<&RecommenderConfig>,
    wave_ctx: Option<WaveCtx>,
    round: u32,
    rng: &mut SimRng,
    obs: &mut O,
) -> RoundCounts {
    sc.rcvd.fill(false);
    sc.wave_trace.clear();
    sc.recv_wave.fill(NONE);
    sc.curr.clear();
    sc.next.clear();
    sc.curr.push(ip);
    sc.rcvd[ip as usize] = true; // INV-13: IP は受信せず、状態を更新しない
    if rec.is_some() {
        sc.via.fill(VIA_FOLLOW);
    }
    // 社会的証明を数えるのは CF＋グラフのときだけ（基準・off では v1 と同じ処理）
    let count_social = rec.is_some_and(|r| r.method == RecMethod::Hybrid);
    if count_social {
        sc.social.clear();
        sc.social.resize(csr.n_nodes(), 0);
    }
    let mut c = RoundCounts::default();
    let mut wave = 0u32;
    // Rモデル: ε δ = 0 なら推薦の分岐に入らず、rec_rng も引かない（INV-W4）
    let is_wave = rec.is_some_and(|r| r.method == RecMethod::Wave);
    let mut wave_rec = match (rec, wave_ctx) {
        (Some(cfg), Some(ctx)) if is_wave && cfg.eps * cfg.delta > 0.0 => {
            Some((cfg, ctx, WaveAcc::default()))
        }
        _ => None,
    };
    // ① フォロー経由の連鎖（Rモデルでは各ウェーブのあとに推薦する）
    cascade(
        csr,
        p,
        pop,
        sc,
        msg,
        mech,
        round,
        rng,
        obs,
        &mut wave,
        &mut c,
        rec.is_some(),
        count_social,
        wave_rec.as_mut(),
    );
    if let Some((cfg, ctx, acc)) = &wave_rec {
        c.rec_miss = acc.miss;
        c.e_hat_end = (acc.cum_h as f64 + cfg.k * ctx.a_t) / (acc.cum_c as f64 + cfg.k);
    }
    if let Some(cfg) = rec.filter(|_| !is_wave) {
        // ② 推薦の段階: 前のラウンドまでの履歴と、① の社会的証明だけで選ぶ（INV-R5・INV-V5）。乱数は引かない
        let g = count_social.then_some((csr, sc.social.as_slice()));
        let picked = crate::recommend::select(pop, &sc.rcvd, ip, msg_id, cfg, g);
        if !picked.is_empty() {
            sc.newly.clear();
            for &j in &picked {
                let ju = j as usize;
                sc.rcvd[ju] = true;
                sc.recv_wave[ju] = wave;
                sc.arr[ju] = 1;
                sc.via[ju] = VIA_REC;
                sc.newly.push((j, FROM_REC));
            }
            apply_wave(csr, pop, sc, msg, mech, round, obs, wave, &mut c, true);
            std::mem::swap(&mut sc.curr, &mut sc.next);
            wave += 1;
            // ③ 推薦から始まる連鎖（社会的証明は数えない。INV-V5）
            cascade(
                csr, p, pop, sc, msg, mech, round, rng, obs, &mut wave, &mut c, true, false, None,
            );
        }
    }
    c.n_waves = wave;
    c
}

/// `curr` が空になるまでウェーブを進める（今の処理そのまま）。
#[allow(clippy::too_many_arguments)]
fn cascade<O: Observer>(
    csr: &Csr,
    p: f64,
    pop: &mut Population,
    sc: &mut Scratch,
    msg: &Message,
    mech: &Mech,
    round: u32,
    rng: &mut SimRng,
    obs: &mut O,
    wave: &mut u32,
    c: &mut RoundCounts,
    track_via: bool,
    count_social: bool,
    mut wave_rec: Option<&mut (&RecommenderConfig, WaveCtx, WaveAcc)>,
) {
    while !sc.curr.is_empty() {
        sc.newly.clear();
        if wave_rec.is_some() {
            sc.sharers.clear();
            sc.sharers.extend_from_slice(&sc.curr);
        }
        while let Some(i) = sc.curr.pop() {
            // INV-05: 後入れ先出し、後続は昇順
            for &j in csr.succ(i) {
                if count_social {
                    // i（IP か、① でシェアした人）は j のフォロー先。抽選の結果によらず数える
                    // 仮定: 社会的証明の送り手に IP を含める（U-V1）
                    sc.social[j as usize] += 1;
                }
                // INV-04: 受信済みの隣人に対しても必ず 1 回引く（短絡しない）
                let hit = p > rng.random::<f64>();
                if hit {
                    let ju = j as usize;
                    if !sc.rcvd[ju] {
                        sc.rcvd[ju] = true;
                        sc.recv_wave[ju] = *wave;
                        sc.arr[ju] = 1;
                        if track_via {
                            // 送り手が推薦由来なら、その先も推薦由来
                            sc.via[ju] = if sc.via[i as usize] == VIA_FOLLOW {
                                VIA_FOLLOW
                            } else {
                                VIA_RECCHAIN
                            };
                        }
                        sc.newly.push((j, i));
                    } else if sc.recv_wave[ju] == *wave {
                        // 同じウェーブでの 2 本目以降（経路数。表示用、M5 の検討用）。IP や前のウェーブの受信者への命中は無視
                        sc.arr[ju] = sc.arr[ju].saturating_add(1);
                    }
                }
            }
        }
        let h = apply_wave(csr, pop, sc, msg, mech, round, obs, *wave, c, track_via);
        if let Some((cfg, ctx, acc)) = wave_rec.as_deref_mut() {
            acc.cum_h += h;
            acc.cum_c += sc.newly.len() as u32;
            recommend_wave(
                csr, p, pop, sc, msg, mech, round, obs, *wave, c, cfg, ctx, acc,
            );
        }
        std::mem::swap(&mut sc.curr, &mut sc.next);
        *wave += 1;
    }
}

/// Rモデル: このウェーブのフォロー経由の処理のあとに、推薦量 B_s を丸めた数だけ宛先を選んで届ける
/// （元資料 3.4 節の (b)〜(e)、`recommender-v3-design.md` 6.2）。乱数は `ctx.rng` だけを引く（INV-W2・W3）。
#[allow(clippy::too_many_arguments)]
fn recommend_wave<O: Observer>(
    csr: &Csr,
    p: f64,
    pop: &mut Population,
    sc: &mut Scratch,
    msg: &Message,
    mech: &Mech,
    round: u32,
    obs: &mut O,
    wave: u32,
    c: &mut RoundCounts,
    cfg: &RecommenderConfig,
    ctx: &mut WaveCtx,
    acc: &mut WaveAcc,
) {
    let e_hat = (acc.cum_h as f64 + cfg.k * ctx.a_t) / (acc.cum_c as f64 + cfg.k);
    // 仮定: ē0 = 0（平均的なメッセージで誰もシェアしない母集団）なら推薦しない（参照実装と同じ）
    let x = if ctx.ebar0 > 0.0 {
        e_hat / ctx.ebar0
    } else {
        0.0
    };
    sc.deg_cum.clear();
    let mut total = 0u32;
    for &i in &sc.sharers {
        total += csr.out_degree(i) as u32;
        sc.deg_cum.push(total);
    }
    let budget = cfg.eps * cfg.delta * crate::recommend::phi(cfg, x) * p * total as f64;
    let slots = budget.floor() as u32 + (ctx.rng.random::<f64>() < budget.fract()) as u32;
    let n = csr.n_nodes() as u32;
    let miss_before = acc.miss;
    sc.newly.clear();
    for _ in 0..slots {
        let target = if ctx.rng.random::<f64>() < cfg.near {
            crate::recommend::two_step_walk(csr, &sc.sharers, &sc.deg_cum, ctx.rng)
        } else {
            Some(ctx.rng.random_range(0..n))
        };
        // IP は最初から受信済みなので、ここで自動的に除かれる（INV-13）
        match target.filter(|&j| !sc.rcvd[j as usize]) {
            Some(j) => {
                let ju = j as usize;
                sc.rcvd[ju] = true;
                sc.recv_wave[ju] = wave;
                sc.arr[ju] = 1;
                sc.via[ju] = VIA_REC;
                sc.newly.push((j, FROM_REC));
            }
            None => acc.miss += 1,
        }
    }
    if !sc.newly.is_empty() {
        acc.cum_h += apply_wave(csr, pop, sc, msg, mech, round, obs, wave, c, true);
        acc.cum_c += sc.newly.len() as u32;
    }
    sc.wave_trace.push(WaveTraceRow {
        wave,
        e_hat,
        budget,
        slots,
        miss: acc.miss - miss_before,
    });
}

/// 初回受信の順に状態を更新する（INV-05）。
#[allow(clippy::too_many_arguments)]
fn apply_wave<O: Observer>(
    csr: &Csr,
    pop: &mut Population,
    sc: &mut Scratch,
    msg: &Message,
    mech: &Mech,
    round: u32,
    obs: &mut O,
    wave: u32,
    c: &mut RoundCounts,
    track_via: bool,
) -> u32 {
    // 内部が成立した人数（フォロワー 0 人も含む）。Rモデルの ê の分子に使う
    let mut n_int = 0u32;
    for k in 0..sc.newly.len() {
        let (j, from) = sc.newly[k];
        let n_arr = sc.arr[j as usize];
        let st = &mut pop.states[j as usize];
        let (int_fired, ext_fired) = st.contact(msg, n_arr, mech);
        pop.recv_round[j as usize] = round;
        n_int += int_fired as u32;
        let enqueued = int_fired && csr.out_degree(j) > 0;
        if ext_fired {
            c.n_xact += 1;
        }
        if enqueued {
            c.n_share += 1;
            sc.next.push(j);
        }
        c.n_received += 1;
        let via = if track_via {
            sc.via[j as usize]
        } else {
            VIA_FOLLOW
        };
        match via {
            VIA_REC => {
                st.flags |= crate::node::EVER_REC; // 推薦で受け取ったことを累積で記録する
                c.rec_received += 1;
                c.rec_share += enqueued as u32;
                c.rec_xact += ext_fired as u32;
            }
            VIA_RECCHAIN => {
                c.chain_received += 1;
                c.chain_xact += ext_fired as u32;
            }
            _ => {}
        }
        obs.on_receive(&ReceiveEvent {
            wave,
            node: j,
            from,
            n_arr,
            u_int: st.u_int,
            u_ext: st.u_ext,
            int_fired,
            ext_fired,
            enqueued,
            via,
        });
    }
    n_int
}
