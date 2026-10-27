//! 再生（F06）。ラウンド開始時の everFlags と、そのラウンドのイベント列から、ウェーブ k の終了時点の状態を作る（9.5）。
//! Web 版の `logic/frames.ts`（`stateAtWave`）と同じ規則。T-19: T-18 と同じ入力で同じ出力。

use sim_api::log::{
    EV_EXT, EV_INT, EV_REC, EV_RECCHAIN, EVER_FLAG_REC, EventColumns, IntMode, NodeKind,
};
use sim_api::node::{EVER_EXT, EVER_INT, EVER_RECV};

pub struct WaveState {
    /// `NodeKind` の番号。
    pub kinds: Vec<u8>,
    /// いまのウェーブ（wave == k）で受信したノードは 1。
    pub pulse: Vec<u8>,
    /// 推薦の印（0 = なし・フォロー、1 = 推薦で受信、2 = 推薦由来の連鎖）。推薦で受信は `rec_cumulative` なら
    /// 前のラウンドまでの分も含む（累積）。推薦由来はこのラウンドのウェーブ k まで。Web の `stateAtWave` と同じ。
    pub via: Vec<u8>,
}

/// 外部は常に累積。内部は `int_now`（このラウンドで成立）か累積。
pub fn kind_of(ever: u8, int_now: bool, is_ip: bool, int_mode: IntMode) -> u8 {
    if is_ip {
        return NodeKind::Ip as u8;
    }
    if ever & EVER_RECV == 0 {
        return NodeKind::Unreceived as u8;
    }
    let int = match int_mode {
        IntMode::Cumulative => ever & EVER_INT != 0,
        IntMode::ThisRoundOnly => int_now,
    };
    let ext = ever & EVER_EXT != 0;
    (match (int, ext) {
        (true, true) => NodeKind::Both,
        (true, false) => NodeKind::Internal,
        (false, true) => NodeKind::External,
        (false, false) => NodeKind::ReceivedOnly,
    }) as u8
}

/// - `k = -1`: ラウンド開始前の状態。`k = n_waves - 1`: ラウンド終了後の状態（`Session::kinds(int_mode)` と一致する）。
/// - 規則: イベント（wave ≤ k）ごとに、`node` へ EVER_RECV、int_fired なら EVER_INT、ext_fired なら EVER_EXT を立てる。
///   `ThisRoundOnly` では、内部は「そのラウンドの int_fired」だけで判定し、外部は常に累積。
pub fn state_at_wave(
    ever_before: &[u8],
    ev: &EventColumns,
    k: i32,
    int_mode: IntMode,
    ip: u32,
    rec_cumulative: bool,
) -> WaveState {
    let n = ever_before.len();
    let mut ever = ever_before.to_vec();
    let mut int_now = vec![false; n];
    let mut pulse = vec![0u8; n];
    let mut via: Vec<u8> = if rec_cumulative {
        ever_before
            .iter()
            .map(|&f| (f & EVER_FLAG_REC != 0) as u8)
            .collect()
    } else {
        vec![0u8; n]
    };
    for i in 0..ev.len() {
        let w = ev.wave[i] as i32;
        if w > k {
            continue;
        }
        let node = ev.node[i] as usize;
        ever[node] |= EVER_RECV;
        if ev.flags[i] & EV_INT != 0 {
            ever[node] |= EVER_INT;
            int_now[node] = true;
        }
        if ev.flags[i] & EV_EXT != 0 {
            ever[node] |= EVER_EXT;
        }
        if w == k {
            pulse[node] = 1;
        }
        if ev.flags[i] & EV_REC != 0 {
            via[node] = 1;
        } else if ev.flags[i] & EV_RECCHAIN != 0 && via[node] == 0 {
            via[node] = 2;
        }
    }
    let kinds = (0..n)
        .map(|j| kind_of(ever[j], int_now[j], j as u32 == ip, int_mode))
        .collect();
    WaveState { kinds, pulse, via }
}

/// 再生の状態（Web の `store/playback.ts` と同じ）。
pub const SPEEDS: [f32; 6] = [0.5, 1.0, 2.0, 4.0, 8.0, 16.0];

pub struct Playback {
    pub playing: bool,
    /// 表示中のウェーブ。-1 = ラウンド開始前。
    pub k: i32,
    /// ウェーブ/秒。
    pub speed: f32,
    /// 最後のウェーブ番号（n_waves - 1）。ラウンドが無いときは -1。
    pub last: i32,
    accum: f32,
}

impl Default for Playback {
    fn default() -> Self {
        Self {
            playing: false,
            k: -1,
            speed: 4.0,
            last: -1,
            accum: 0.0,
        }
    }
}

impl Playback {
    pub fn start(&mut self, n_waves: u32) {
        self.playing = n_waves > 0;
        self.k = -1;
        self.last = n_waves as i32 - 1;
        self.accum = 0.0;
    }

    pub fn reset(&mut self) {
        self.playing = false;
        self.k = -1;
        self.last = -1;
        self.accum = 0.0;
    }

    pub fn toggle(&mut self) {
        if !self.playing && self.k >= self.last {
            self.k = -1;
            self.playing = self.last >= 0;
        } else {
            self.playing = !self.playing;
        }
    }

    pub fn step_wave(&mut self, d: i32) {
        self.playing = false;
        self.k = (self.k + d).clamp(-1, self.last);
    }

    pub fn skip_to_end(&mut self) {
        self.playing = false;
        self.k = self.last;
    }

    /// 経過時間 `dt` 秒だけ進める。
    pub fn advance(&mut self, dt: f32) {
        if !self.playing {
            return;
        }
        self.accum += dt * self.speed;
        while self.accum >= 1.0 && self.k < self.last {
            self.accum -= 1.0;
            self.k += 1;
        }
        if self.k >= self.last {
            self.playing = false;
            self.accum = 0.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn events(rows: &[(u16, u32, u32, u8)]) -> EventColumns {
        let mut e = EventColumns::default();
        for &(wave, node, from, flags) in rows {
            e.wave.push(wave);
            e.node.push(node);
            e.from.push(from);
            e.n_arr.push(1);
            e.u_int.push(0.0);
            e.u_ext.push(0.0);
            e.flags.push(flags);
        }
        e
    }

    // T-18（Web の frames.test.ts）と同じ入力・同じ期待値
    fn sample() -> EventColumns {
        // IP=0。flags は bit0=内部成立, bit1=外部成立, bit2=次ウェーブでシェア。
        // wave0: 1（内部）, 2（受信のみ）。wave1: 3（外部）, 4（両方）
        events(&[
            (0, 1, 0, 1 | 4),
            (0, 2, 0, 0),
            (1, 3, 1, 2),
            (1, 4, 1, 1 | 2 | 4),
        ])
    }

    #[test]
    fn kind_of_rules() {
        use IntMode::*;
        assert_eq!(kind_of(0, false, false, Cumulative), 0);
        assert_eq!(kind_of(1, false, false, Cumulative), 1);
        assert_eq!(kind_of(1 | 2, false, false, Cumulative), 2);
        assert_eq!(kind_of(1 | 2, false, false, ThisRoundOnly), 1);
        assert_eq!(kind_of(1 | 2, true, false, ThisRoundOnly), 2);
        assert_eq!(kind_of(1 | 4, false, false, ThisRoundOnly), 3);
        assert_eq!(kind_of(1 | 2 | 4, false, false, Cumulative), 4);
        assert_eq!(kind_of(1 | 2 | 4, false, false, ThisRoundOnly), 3);
        assert_eq!(kind_of(0, false, true, Cumulative), 5);
    }

    #[test]
    fn waves_progress_like_the_web_version() {
        let ev = sample();
        let before = vec![0u8; 6];
        let s = state_at_wave(&before, &ev, -1, IntMode::Cumulative, 0, true);
        assert_eq!(s.kinds, vec![5, 0, 0, 0, 0, 0]);
        assert!(s.pulse.iter().all(|&x| x == 0));
        let s0 = state_at_wave(&before, &ev, 0, IntMode::Cumulative, 0, true);
        assert_eq!(s0.pulse, vec![0, 1, 1, 0, 0, 0]);
        assert_eq!(s0.kinds[1], 2, "wave0 で 1 は内部が成立");
        assert_eq!(s0.kinds[2], 1);
        let s1 = state_at_wave(&before, &ev, 1, IntMode::Cumulative, 0, true);
        assert_eq!(s1.kinds, vec![5, 2, 1, 3, 4, 0]);
        assert_eq!(s1.pulse, vec![0, 0, 0, 1, 1, 0]);
    }

    #[test]
    fn this_round_only_ignores_previous_internal() {
        let ev = sample();
        let mut before = vec![0u8; 6];
        before[5] = 1 | 2 | 4;
        let s = state_at_wave(&before, &ev, 1, IntMode::ThisRoundOnly, 0, true);
        assert_eq!(s.kinds[5], NodeKind::External as u8);
        assert_eq!(s.kinds[1], NodeKind::Internal as u8);
    }

    #[test]
    fn playback_controls() {
        let mut p = Playback::default();
        p.start(3);
        assert!(p.playing && p.k == -1 && p.last == 2);
        p.advance(0.6); // 4 ウェーブ/秒 → 2.4 ウェーブ
        assert_eq!(p.k, 1);
        p.advance(1.0);
        assert!(!p.playing && p.k == 2);
        p.toggle(); // 最後まで行っていれば先頭から再生
        assert!(p.playing && p.k == -1);
        p.step_wave(1);
        assert!(!p.playing && p.k == 0);
        p.step_wave(-5);
        assert_eq!(p.k, -1);
        p.skip_to_end();
        assert_eq!(p.k, 2);
        let mut e = Playback::default();
        e.start(0);
        assert!(!e.playing && e.last == -1);
    }

    #[test]
    fn rec_cumulative_keeps_earlier_recommendations() {
        // Web の frames.test.ts と同じ入力: ノード 1 は前のラウンドに推薦（EVER_FLAG_REC）、
        // このラウンドはノード 2 が推薦由来、ノード 3 が推薦
        let mut ev = EventColumns::default();
        for (w, node, flags) in [(0u16, 2u32, EV_RECCHAIN), (1, 3, EV_REC)] {
            ev.wave.push(w);
            ev.node.push(node);
            ev.from.push(0);
            ev.n_arr.push(1);
            ev.u_int.push(0.0);
            ev.u_ext.push(0.0);
            ev.flags.push(flags);
        }
        let before = vec![0, 1 | EVER_FLAG_REC, 0, 0];
        let c = |k, cum| state_at_wave(&before, &ev, k, IntMode::Cumulative, 0, cum).via;
        assert_eq!(c(1, true), vec![0, 1, 2, 1]);
        assert_eq!(c(1, false), vec![0, 0, 2, 1]);
        assert_eq!(c(-1, true), vec![0, 1, 0, 0]);
    }
}
