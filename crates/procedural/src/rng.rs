//! A small seeded random number generator (SplitMix64), the same on every
//! platform. Each spot gets its own from the seed and its number, so adding
//! a spot at the end doesn't move the ones before it.

pub struct Rng(u64);

impl Rng {
    /// The generator for spot `index` of a layout seeded with `seed`.
    pub fn new(seed: u64, index: u64) -> Self {
        Rng(mix(
            seed ^ mix(index.wrapping_add(0x51_7C_C1_B7_27_22_0A_95))
        ))
    }

    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        mix(self.0)
    }

    /// A number from 0 up to (not including) 1.
    pub fn unit(&mut self) -> f32 {
        (self.next() >> 40) as f32 / (1u64 << 24) as f32
    }

    /// A number between `low` and `high`.
    pub fn range(&mut self, low: f32, high: f32) -> f32 {
        low + (high - low) * self.unit()
    }

    /// A whole number from 0 up to (not including) `count`, which isn't 0.
    pub fn below(&mut self, count: usize) -> usize {
        (self.next() % count as u64) as usize
    }
}

fn mix(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}
