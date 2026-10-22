// 検証用プロトタイプ（設計書 付録 B-3）。実行: 新規 crate に置いて `cargo run --release`
// Cargo.toml: edition = "2024"; rand = "0.9"; rand_xoshiro = "0.7"; approx = "0.5"
// core/src/diffusion.rs の diffuse を書き写した関数(orig)と、「状態更新をウェーブの終わりにまとめる」方式(newalg)が
// 合計値・全ノードの効用・外部行動フラグ・乱数列までビット一致することを確かめる。機構(M1〜M5)は含まない。
// 注: orig は core の実物ではない(書き写し)。実物との照合は設計書の T-01 で行う。
use approx::relative_eq;
use rand::{Rng, SeedableRng, rngs::SmallRng};
use rand_xoshiro::Xoshiro256PlusPlus;
use std::mem;

#[derive(Clone)]
struct S { wi: f64, ui: f64, we: f64, ue: f64, ext_done: bool }
fn decided(u: f64) -> bool { if relative_eq!(&u, &0.0) { false } else { u > 0.0 } }

// ---- original algorithm (copied logic from core/src/diffusion.rs) ----
fn orig(succ: &[Vec<usize>], p: f64, st: &mut [S], ip: usize, msg: (f64, f64), rng: &mut impl Rng) -> (usize, usize, usize) {
    let n = succ.len();
    let mut rcvd = vec![false; n];
    let (mut curr, mut next) = (vec![ip], Vec::new());
    rcvd[ip] = true;
    let (mut nx, mut ns, mut steps) = (0, 0, 0);
    loop {
        if curr.is_empty() { break; }
        let (mut c_share, mut c_act) = (0, 0);
        while let Some(i) = curr.pop() {
            for &j in &succ[i] {
                if (rcvd[j] == false) & (p > rng.random()) {
                    rcvd[j] = true;
                    let s = &mut st[j];
                    s.ui = s.wi * s.ui + (1.0 - s.wi) * msg.0;
                    s.ue = s.we * s.ue + (1.0 - s.we) * msg.1;
                    let ext = if !s.ext_done { let d = decided(s.ue); if d { s.ext_done = true; } d } else { false };
                    let int = decided(s.ui);
                    if ext { c_act += 1; }
                    if succ[j].len() > 0 && int { c_share += 1; next.push(j); }
                }
            }
        }
        mem::swap(&mut curr, &mut next);
        ns += c_share; nx += c_act; steps += 1;
    }
    (nx, ns, steps)
}

// ---- new algorithm: contact deferred to end of wave, arrivals counted ----
struct Trace { wave: Vec<u16>, node: Vec<u32>, from: Vec<u32>, n_arr: Vec<u8> }
fn newalg(succ: &[Vec<usize>], p: f64, st: &mut [S], ip: usize, msg: (f64, f64), rng: &mut impl Rng, tr: &mut Trace) -> (usize, usize, usize) {
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
                let hit = p > rng.random::<f64>();
                if hit {
                    if !rcvd[j] { rcvd[j] = true; recv_wave[j] = wave; arr[j] = 1; newly.push((j, i)); }
                    else if recv_wave[j] == wave { arr[j] = arr[j].saturating_add(1); }
                }
            }
        }
        for &(j, from) in &newly {
            let s = &mut st[j];
            s.ui = s.wi * s.ui + (1.0 - s.wi) * msg.0;
            s.ue = s.we * s.ue + (1.0 - s.we) * msg.1;
            let ext = if !s.ext_done { let d = decided(s.ue); if d { s.ext_done = true; } d } else { false };
            let int = decided(s.ui);
            if ext { nx += 1; }
            if succ[j].len() > 0 && int { ns += 1; next.push(j); }
            tr.wave.push(wave as u16); tr.node.push(j as u32); tr.from.push(from as u32); tr.n_arr.push(arr[j]);
        }
        mem::swap(&mut curr, &mut next);
        wave += 1;
    }
    (nx, ns, wave as usize)
}

fn main() {
    let mut multi_arr = 0usize; let mut total_ev = 0usize;
    for gseed in 0..60u64 {
        let mut g = SmallRng::seed_from_u64(1000 + gseed);
        let n = 40 + (gseed as usize % 5) * 60;
        let deg = 1 + (gseed as usize % 6);
        let mut succ: Vec<Vec<usize>> = vec![vec![]; n];
        for i in 0..n { for _ in 0..deg { let j = g.random_range(0..n); if j != i { succ[i].push(j); } } succ[i].sort(); succ[i].dedup(); }
        // random user states (weights in [0.1,0.9], u0=-1)
        let init: Vec<S> = (0..n).map(|_| { let a: f64 = g.random(); let b: f64 = g.random();
            S { wi: 0.1 + 0.8 * a, ui: -1.0, we: 0.1 + 0.8 * b, ue: -1.0, ext_done: false } }).collect();
        for &p in &[0.3, 0.5, 1.0] {
            for rseed in 0..5u64 {
                let msgs = [(0.2, 0.2), (0.2, 0.05), (0.15, 0.1), (0.1, 0.1), (0.1, 0.15), (0.05, 0.2)];
                // sequence of 8 rounds, states persist across rounds; SmallRng (orig) vs Xoshiro256++ (new)
                let mut so = init.clone(); let mut sn = init.clone();
                let mut ro = SmallRng::seed_from_u64(rseed); let mut rn = Xoshiro256PlusPlus::seed_from_u64(rseed);
                for round in 0..8 {
                    let m = msgs[(round * 5 + rseed as usize) % 6];
                    let ip = (rseed as usize * 7 + round) % n;
                    let a = orig(&succ, p, &mut so, ip, m, &mut ro);
                    let mut tr = Trace { wave: vec![], node: vec![], from: vec![], n_arr: vec![] };
                    let b = newalg(&succ, p, &mut sn, ip, m, &mut rn, &mut tr);
                    assert_eq!(a, b, "totals differ g{gseed} p{p} r{rseed} round{round}");
                    total_ev += tr.node.len(); multi_arr += tr.n_arr.iter().filter(|&&x| x > 1).count();
                    for k in 0..n { assert_eq!(so[k].ui.to_bits(), sn[k].ui.to_bits()); assert_eq!(so[k].ue.to_bits(), sn[k].ue.to_bits()); assert_eq!(so[k].ext_done, sn[k].ext_done); }
                }
                // RNG state must also stay in lockstep
                assert_eq!(rand::RngCore::next_u64(&mut ro), rand::RngCore::next_u64(&mut rn));
            }
        }
    }
    println!("PARITY OK. receive events checked: {total_ev}, events with n_arr>1 (M5 would act): {multi_arr}");
}
