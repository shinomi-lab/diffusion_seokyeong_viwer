//! ノード状態と `contact`（M1 の置き場所。7.4）。
//! M2（摩耗）・M3（S 字反応）・M4（休止・忘却）は 2026-10-20 に廃止。M5（複合感染）は検討中のためコメントアウト。

use sim_core::message::Message;

use crate::config::Mech;

pub const EVER_RECV: u8 = 1;
pub const EVER_INT: u8 = 2;
pub const EVER_EXT: u8 = 4;

#[derive(Clone, Debug)]
pub struct NodeState {
    /// 効用更新の重み（母集団の生成時に決まり、不変）。
    pub w_int: f64,
    pub w_ext: f64,
    /// 現在の効用（上限なし。INV-15）。
    pub u_int: f64,
    pub u_ext: f64,
    /// 外部行動を済ませたか（生涯 1 回）。
    pub ext_done: bool,
    pub flags: u8,
}

impl NodeState {
    pub fn new(w_int: f64, w_ext: f64, u0_int: f64, u0_ext: f64) -> Self {
        Self {
            w_int,
            w_ext,
            u_int: u0_int,
            u_ext: u0_ext,
            ext_done: false,
            flags: 0,
        }
    }

    /// 戻り値は (内部が成立したか, 外部が成立したか)。
    /// `_n_arr` はそのウェーブで届いた経路の数（M5 の検討用に残す。現在は使わない）。
    #[inline]
    pub fn contact(&mut self, msg: &Message, _n_arr: u8, m: &Mech) -> (bool, bool) {
        // M5 複合感染（検討中のためコメントアウト）: 同じウェーブで届いた経路の数 n（上限 n_max）で刺激を強める。
        // let scale = if m.eta != 0.0 { 1.0 + m.eta * (_n_arr.min(m.n_max) as f64 - 1.0) } else { 1.0 };
        // let (s_i, s_e) = if scale == 1.0 {
        //     (msg.internal_stimulus, msg.external_stimulus)
        // } else {
        //     (msg.internal_stimulus * scale, msg.external_stimulus * scale)
        // };
        let (s_i, s_e) = (msg.internal_stimulus, msg.external_stimulus);
        // 効用更新（INV-06: 評価順を変えない。mul_add を使わない）
        self.u_int = self.w_int * self.u_int + (1.0 - self.w_int) * s_i;
        self.u_ext = self.w_ext * self.u_ext + (1.0 - self.w_ext) * s_e;
        // 外部の判定（生涯 1 回）
        let mut ext_fired = false;
        if !self.ext_done && decide(self.u_ext) {
            self.ext_done = true;
            ext_fired = true;
        }
        // M1 口コミ: 外部行動が成立した瞬間に 1 回だけ、内部効用へ κ を加える（内部の判定より前）
        // 決定: D7 / Q5（2026-10-02 ユーザー確認済み）。内部判定の前に加える。
        if ext_fired && m.kappa != 0.0 {
            self.u_int += m.kappa;
        }
        // 内部の判定（毎回）
        let int_fired = decide(self.u_int);
        self.flags |=
            EVER_RECV | if int_fired { EVER_INT } else { 0 } | if ext_fired { EVER_EXT } else { 0 };
        (int_fired, ext_fired)
    }
}

/// 発火判定。INV-07: 既存の `DecisionBase::decided` と同じ（u ≈ 0 は不成立）。
#[inline]
pub fn decide(u: f64) -> bool {
    if approx::relative_eq!(&u, &0.0) {
        false
    } else {
        u > 0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(i: f64, e: f64) -> Message {
        Message {
            internal_stimulus: i,
            external_stimulus: e,
        }
    }

    #[test]
    fn m1_kappa_added_once_before_internal_decision() {
        let mut m = Mech::off();
        m.kappa = 2.0;
        // w_ext=0 なので外部は初回で u_ext = 0.2 > 0 → 成立。w_int=0.5 → u_int = -0.45
        let mut s = NodeState::new(0.5, 0.0, -1.0, -1.0);
        let (i, e) = s.contact(&msg(0.1, 0.2), 1, &m);
        assert!(e);
        // κ は内部判定の前に加わる: -0.45 + 2.0 = 1.55 > 0 → 内部も成立、かつ上限なし
        assert!(i);
        assert!((s.u_int - 1.55).abs() < 1e-12 && s.u_int > 1.0);
        // 2 回目: 外部は済み。κ は再び加わらない（通常の更新のみ）
        let before = s.u_int;
        let (_, e2) = s.contact(&msg(0.1, 0.2), 1, &m);
        assert!(!e2);
        assert_eq!(s.u_int.to_bits(), (0.5 * before + 0.5 * 0.1).to_bits());
        // κ=0 なら加わらない
        m.kappa = 0.0;
        let mut s = NodeState::new(0.5, 0.0, -1.0, -1.0);
        let (i, _) = s.contact(&msg(0.1, 0.2), 1, &m);
        assert!(!i);
    }

    #[test]
    fn arrivals_do_not_change_the_update_while_m5_is_disabled() {
        // M5 はコメントアウト中: 届いた経路の数によらず、更新は同じ
        let m = Mech::off();
        let mut a = NodeState::new(0.0, 0.0, -1.0, -1.0);
        let mut b = a.clone();
        a.contact(&msg(0.2, 0.1), 1, &m);
        b.contact(&msg(0.2, 0.1), 3, &m);
        assert_eq!(a.u_int.to_bits(), b.u_int.to_bits());
        assert_eq!(a.u_ext.to_bits(), b.u_ext.to_bits());
    }

    #[test]
    fn decide_matches_threshold_rule() {
        assert!(decide(0.1));
        assert!(!decide(-0.1));
        assert!(!decide(0.0));
        assert!(!decide(1e-17), "u ≈ 0 は不成立（relative_eq の既定許容）");
    }
}
