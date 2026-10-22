//! 1 ラウンドのカスケード（7.3）。
//!
//! 既存の `diffuse` の乱数の引き方・処理順をそのまま写し、「受信した瞬間に状態を更新する」ところだけを
//! 「ウェーブの終わりにまとめて更新する」に変える（M5 の経路数 n を数えるため）。
//! 機構なしのとき `core::diffusion::diffuse` とビット一致する（T-01）。

use rand::Rng;
use sim_core::message::Message;

use crate::config::Mech;
use crate::csr::Csr;
use crate::population::Population;
use crate::rng::SimRng;

const NONE: u32 = u32::MAX;

/// ラウンドをまたいで使い回す作業領域。
pub struct Scratch {
    rcvd: Vec<bool>,
    recv_wave: Vec<u32>,
    arr: Vec<u8>,
    curr: Vec<u32>,
    next: Vec<u32>,
    /// (受信ノード, 初回の送信元)
    newly: Vec<(u32, u32)>,
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
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RoundCounts {
    pub n_received: u32,
    pub n_share: u32,
    pub n_xact: u32,
    pub n_waves: u32,
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
    // M2（摩耗）の廃止で使わなくなった。呼び出し側を変えないために残す
    _msg_id: u8,
    msg: &Message,
    mech: &Mech,
    round: u32,
    rng: &mut SimRng,
    obs: &mut O,
) -> RoundCounts {
    sc.rcvd.fill(false);
    sc.recv_wave.fill(NONE);
    sc.curr.clear();
    sc.next.clear();
    sc.curr.push(ip);
    sc.rcvd[ip as usize] = true; // INV-13: IP は受信せず、状態を更新しない
    let (mut n_recv, mut n_share, mut n_xact, mut wave) = (0u32, 0u32, 0u32, 0u32);

    while !sc.curr.is_empty() {
        sc.newly.clear();
        while let Some(i) = sc.curr.pop() {
            // INV-05: 後入れ先出し、後続は昇順
            for &j in csr.succ(i) {
                // INV-04: 受信済みの隣人に対しても必ず 1 回引く（短絡しない）
                let hit = p > rng.random::<f64>();
                if hit {
                    let ju = j as usize;
                    if !sc.rcvd[ju] {
                        sc.rcvd[ju] = true;
                        sc.recv_wave[ju] = wave;
                        sc.arr[ju] = 1;
                        sc.newly.push((j, i));
                    } else if sc.recv_wave[ju] == wave {
                        // 同じウェーブでの 2 本目以降（経路数。表示用、M5 の検討用）。IP や前のウェーブの受信者への命中は無視
                        sc.arr[ju] = sc.arr[ju].saturating_add(1);
                    }
                }
            }
        }
        // 初回受信の順に状態を更新（INV-05）
        for k in 0..sc.newly.len() {
            let (j, from) = sc.newly[k];
            let n_arr = sc.arr[j as usize];
            let st = &mut pop.states[j as usize];
            let (int_fired, ext_fired) = st.contact(msg, n_arr, mech);
            pop.recv_round[j as usize] = round;
            let enqueued = int_fired && csr.out_degree(j) > 0;
            if ext_fired {
                n_xact += 1;
            }
            if enqueued {
                n_share += 1;
                sc.next.push(j);
            }
            n_recv += 1;
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
            });
        }
        std::mem::swap(&mut sc.curr, &mut sc.next);
        wave += 1;
    }
    RoundCounts {
        n_received: n_recv,
        n_share,
        n_xact,
        n_waves: wave,
    }
}
