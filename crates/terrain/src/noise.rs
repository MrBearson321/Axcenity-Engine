//! Seeded 2D gradient noise and the fractal sums built from it.
//!
//! Hand-written rather than a crate so the numbers never change under us:
//! saved terrains are made again from their seed (see `generate.rs`).

/// A 64-bit mix (SplitMix64's finalizer): every input bit changes about
/// half the output bits, so nearby inputs give unrelated outputs.
pub fn mix(mut x: u64) -> u64 {
    x ^= x >> 30;
    x = x.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

/// A small, fast random number generator (SplitMix64), the same sequence
/// for the same seed on every machine.
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        mix(self.0)
    }

    /// From 0 up to (not including) 1.
    pub fn next_f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }
}

/// Gradient (Perlin-style) noise in 2D, between about -1 and 1, smooth
/// everywhere and 0 at whole-number points.
#[derive(Clone, Copy, Debug)]
pub struct Noise {
    seed: u64,
}

impl Noise {
    pub fn new(seed: u64) -> Self {
        Self { seed: mix(seed) }
    }

    /// The same noise moved somewhere unrelated, for a second independent layer.
    pub fn offset(self, salt: u64) -> Self {
        Self {
            seed: mix(self.seed ^ mix(salt)),
        }
    }

    fn gradient(&self, ix: i32, iz: i32) -> (f32, f32) {
        let hash = mix(self.seed ^ ((ix as u32 as u64) << 32 | iz as u32 as u64));
        // 16 directions around the circle.
        let angle = (hash >> 60) as f32 * (std::f32::consts::TAU / 16.0);
        (angle.cos(), angle.sin())
    }

    pub fn get(&self, x: f32, z: f32) -> f32 {
        let (x0, z0) = (x.floor(), z.floor());
        let (fx, fz) = (x - x0, z - z0);
        let (ix, iz) = (x0 as i32, z0 as i32);
        let corner = |dx: i32, dz: i32| {
            let (gx, gz) = self.gradient(ix + dx, iz + dz);
            gx * (fx - dx as f32) + gz * (fz - dz as f32)
        };
        let (u, v) = (fade(fx), fade(fz));
        let top = lerp(corner(0, 0), corner(1, 0), u);
        let bottom = lerp(corner(0, 1), corner(1, 1), u);
        // Scaled so the result reaches about ±1.
        lerp(top, bottom, v) * std::f32::consts::SQRT_2
    }

    /// Fractal noise: `octaves` layers, each `lacunarity` times finer and
    /// `persistence` times weaker than the one before, about -1 to 1.
    pub fn fbm(&self, x: f32, z: f32, octaves: u32, persistence: f32, lacunarity: f32) -> f32 {
        let (mut sum, mut amplitude, mut frequency, mut total) = (0.0, 1.0, 1.0, 0.0);
        for octave in 0..octaves.max(1) {
            let layer = self.offset(octave as u64);
            sum += layer.get(x * frequency, z * frequency) * amplitude;
            total += amplitude;
            amplitude *= persistence;
            frequency *= lacunarity;
        }
        sum / total
    }

    /// Ridged fractal noise, 0 to about 1: sharp crests where the noise
    /// crosses 0, like mountain ridges. Each octave is weighted by the one
    /// before, so the detail gathers on the ridges.
    pub fn ridged(&self, x: f32, z: f32, octaves: u32, persistence: f32, lacunarity: f32) -> f32 {
        let (mut sum, mut amplitude, mut frequency, mut total) = (0.0, 1.0, 1.0, 0.0);
        let mut weight = 1.0;
        for octave in 0..octaves.max(1) {
            let layer = self.offset(1000 + octave as u64);
            let ridge = 1.0 - layer.get(x * frequency, z * frequency).abs();
            let ridge = ridge * ridge * weight;
            weight = (ridge * 2.0).clamp(0.0, 1.0);
            sum += ridge * amplitude;
            total += amplitude;
            amplitude *= persistence;
            frequency *= lacunarity;
        }
        sum / total
    }
}

fn fade(t: f32) -> f32 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// 0 below `edge0`, 1 above `edge1`, and a smooth S between. Equal edges
/// make a step.
pub fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    if edge1 <= edge0 {
        return if x < edge0 { 0.0 } else { 1.0 };
    }
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_seed_gives_the_same_noise_and_another_seed_doesnt() {
        let a = Noise::new(7);
        let b = Noise::new(7);
        let c = Noise::new(8);
        let points = [(0.3, 0.7), (12.25, -3.5), (-100.1, 55.9)];
        for (x, z) in points {
            assert_eq!(a.fbm(x, z, 5, 0.5, 2.0), b.fbm(x, z, 5, 0.5, 2.0));
        }
        assert!(points.iter().any(|&(x, z)| a.get(x, z) != c.get(x, z)));
        let mut rng = Rng::new(3);
        let first = rng.next_u64();
        assert_eq!(Rng::new(3).next_u64(), first);
    }

    #[test]
    fn noise_stays_in_range_and_is_zero_on_the_grid() {
        let noise = Noise::new(42);
        assert_eq!(noise.get(3.0, -4.0), 0.0);
        let mut rng = Rng::new(1);
        for _ in 0..2000 {
            let (x, z) = (
                rng.next_f32() * 200.0 - 100.0,
                rng.next_f32() * 200.0 - 100.0,
            );
            let value = noise.fbm(x, z, 6, 0.5, 2.0);
            assert!((-1.05..=1.05).contains(&value), "{value}");
            let ridge = noise.ridged(x, z, 6, 0.5, 2.0);
            assert!((0.0..=1.0).contains(&ridge), "{ridge}");
        }
    }

    #[test]
    fn smoothstep_steps_when_its_edges_meet() {
        assert_eq!(smoothstep(1.0, 1.0, 0.5), 0.0);
        assert_eq!(smoothstep(1.0, 1.0, 1.5), 1.0);
        assert_eq!(smoothstep(0.0, 2.0, 1.0), 0.5);
    }
}
