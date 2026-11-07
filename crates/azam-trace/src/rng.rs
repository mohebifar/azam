//! A small, seedable random number generator (PCG32) and the hashes that seed it.

/// PCG32 (O'Neill's permuted congruential generator): fast, small state, good statistics.
#[derive(Clone, Debug)]
pub struct Rng {
    state: u64,
    inc: u64,
}

const MULTIPLIER: u64 = 6_364_136_223_846_793_005;

/// A 64-bit finaliser (`SplitMix64`'s), mixing every input bit into every output bit.
#[must_use]
pub(crate) const fn mix64(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

impl Rng {
    /// A generator for `seed` on stream `stream` (different streams never overlap).
    #[must_use]
    pub fn new(seed: u64, stream: u64) -> Self {
        let mut rng = Self {
            state: 0,
            inc: (stream << 1) | 1,
        };
        rng.next_u32();
        rng.state = rng.state.wrapping_add(seed);
        rng.next_u32();
        rng
    }

    /// The generator for sample `sample` of pixel `pixel` of a render seeded with `seed`.
    #[must_use]
    pub fn for_sample(seed: u64, pixel: u64, sample: u64) -> Self {
        let key = mix64(seed ^ mix64(pixel.wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ mix64(sample)));
        Self::new(key, mix64(key ^ 0x632b_e59b_d9b4_e019))
    }

    /// The next 32 random bits.
    pub fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.state = old.wrapping_mul(MULTIPLIER).wrapping_add(self.inc);
        let xorshifted = (((old >> 18) ^ old) >> 27) as u32;
        let rot = (old >> 59) as u32;
        xorshifted.rotate_right(rot)
    }

    /// A uniform float in `[0, 1)`.
    pub fn next_f32(&mut self) -> f32 {
        // 24 random mantissa bits: exactly representable, never 1.0.
        (self.next_u32() >> 8) as f32 * (1.0 / 16_777_216.0)
    }

    /// Two uniform floats in `[0, 1)`.
    pub fn next_2d(&mut self) -> (f32, f32) {
        (self.next_f32(), self.next_f32())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_sequence() {
        let mut a = Rng::for_sample(7, 100, 3);
        let mut b = Rng::for_sample(7, 100, 3);
        for _ in 0..100 {
            assert_eq!(a.next_u32(), b.next_u32());
        }
        let mut c = Rng::for_sample(7, 101, 3);
        assert_ne!(Rng::for_sample(7, 100, 3).next_u32(), c.next_u32());
    }

    #[test]
    fn floats_are_uniform_in_unit_interval() {
        let mut rng = Rng::new(1, 2);
        let n = 100_000;
        let mut sum = 0.0;
        let mut buckets = [0u32; 10];
        for _ in 0..n {
            let x = rng.next_f32();
            assert!((0.0..1.0).contains(&x));
            sum += f64::from(x);
            buckets[(x * 10.0) as usize] += 1;
        }
        let mean = sum / f64::from(n);
        assert!((mean - 0.5).abs() < 0.005, "{mean}");
        for b in buckets {
            assert!((f64::from(b) / f64::from(n) - 0.1).abs() < 0.005, "{buckets:?}");
        }
    }
}
