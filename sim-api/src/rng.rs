//! 乱数の型を 1 か所で決める（INV-03）。
//!
//! `SmallRng` は 32bit（WASM）で別アルゴリズムになるため使わない。
//! 64bit の `SmallRng` は `Xoshiro256PlusPlus` なので、同じ列になる。

use rand::SeedableRng;

pub type SimRng = rand_xoshiro::Xoshiro256PlusPlus;

pub fn rng_from_seed(seed: u64) -> SimRng {
    SimRng::seed_from_u64(seed)
}

#[cfg(all(test, target_pointer_width = "64"))]
mod tests {
    use super::*;
    use rand::{Rng, RngCore, rngs::SmallRng};
    use rand_distr::{Distribution, Pareto};

    const SEEDS: [u64; 4] = [0, 1, 42, 123456789];

    #[test]
    fn next_u64_matches_small_rng() {
        for s in SEEDS {
            let mut a = SmallRng::seed_from_u64(s);
            let mut b = rng_from_seed(s);
            for _ in 0..1000 {
                assert_eq!(a.next_u64(), b.next_u64());
            }
        }
    }

    #[test]
    fn derived_children_match() {
        let mut pa = SmallRng::seed_from_u64(7);
        let mut pb = rng_from_seed(7);
        for _ in 0..50 {
            let mut ca = SmallRng::from_rng(&mut pa);
            let mut cb = SimRng::from_rng(&mut pb);
            for _ in 0..20 {
                assert_eq!(ca.next_u64(), cb.next_u64());
            }
        }
    }

    #[test]
    fn random_f64_bits_match() {
        let mut a = SmallRng::seed_from_u64(3);
        let mut b = rng_from_seed(3);
        for _ in 0..1000 {
            assert_eq!(a.random::<f64>().to_bits(), b.random::<f64>().to_bits());
        }
    }

    #[test]
    fn pareto_samples_match() {
        let d = Pareto::<f64>::new(0.25, 3.0).unwrap();
        let mut a = SmallRng::seed_from_u64(5);
        let mut b = rng_from_seed(5);
        let va: Vec<u64> = d.sample_iter(&mut a).take(1000).map(f64::to_bits).collect();
        let vb: Vec<u64> = d.sample_iter(&mut b).take(1000).map(f64::to_bits).collect();
        assert_eq!(va, vb);
    }
}
