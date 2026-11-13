//! Mesh filters (roadmap R15.1): one change over the whole sculpt at once
//! (Smooth, Sharpen, Inflate, Scale, Random), as Blender's Mesh Filter. Each
//! works out every vertex's move from where the vertices are before it, then
//! moves them, held back by the mask as brushes are; hidden parts don't move.

use glam::Vec3;

use super::Sculpt;

/// What a mesh filter does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MeshFilter {
    /// Each vertex toward its neighbours' middle.
    #[default]
    Smooth,
    /// Each vertex away from its neighbours' middle: details stand out.
    Sharpen,
    /// Each vertex out along its own normal (a negative amount: in).
    Inflate,
    /// Each vertex away from the middle of the whole (negative: toward it).
    Scale,
    /// Each vertex along its normal by a random amount, the same for the
    /// same mesh, for a rough surface.
    Random,
}

impl MeshFilter {
    pub const ALL: [MeshFilter; 5] = [
        MeshFilter::Smooth,
        MeshFilter::Sharpen,
        MeshFilter::Inflate,
        MeshFilter::Scale,
        MeshFilter::Random,
    ];

    pub fn label(self) -> &'static str {
        match self {
            MeshFilter::Smooth => "Smooth",
            MeshFilter::Sharpen => "Sharpen",
            MeshFilter::Inflate => "Inflate",
            MeshFilter::Scale => "Scale",
            MeshFilter::Random => "Random",
        }
    }

    pub fn tip(self) -> &'static str {
        match self {
            MeshFilter::Smooth => "Even out the whole surface",
            MeshFilter::Sharpen => "Make every detail stand out more",
            MeshFilter::Inflate => "Push the whole surface out along its normals (negative: in)",
            MeshFilter::Scale => "Grow the whole object from its middle (negative: shrink)",
            MeshFilter::Random => "Roughen the whole surface by random amounts",
        }
    }
}

/// How far Inflate and Random move a vertex at amount 1, as a share of the
/// sculpt's size (its box's diagonal).
const PUSH: f32 = 0.01;
/// How much Scale grows the whole at amount 1.
const GROW: f32 = 0.1;

/// A number from -1 to 1 for vertex `v`, the same every time.
fn noise(v: u32) -> f32 {
    let mut x = v.wrapping_mul(0x9E37_79B9) ^ 0x85EB_CA6B;
    x ^= x >> 16;
    x = x.wrapping_mul(0x7FEB_352D);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846C_A68B);
    x ^= x >> 16;
    (x >> 8) as f32 / (1u32 << 23) as f32 - 1.0
}

impl Sculpt {
    /// Runs `filter` over every shown vertex by `amount` (about -1 to 1;
    /// Smooth and Sharpen take the share of the way, the others a size).
    /// True if anything moved.
    pub fn filter(&mut self, filter: MeshFilter, amount: f32) -> bool {
        if amount == 0.0 || !amount.is_finite() || self.positions.is_empty() {
            return false;
        }
        // Shown vertices: on any triangle that isn't hidden.
        let mut shown = vec![false; self.positions.len()];
        for (t, corners) in self.triangles.iter().enumerate() {
            if !self.hidden[t] {
                for &v in corners {
                    shown[v as usize] = true;
                }
            }
        }
        let (mut low, mut high) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
        for &p in &self.positions {
            low = low.min(p);
            high = high.max(p);
        }
        let size = (high - low).length().max(1e-6);
        let middle = (low + high) / 2.0;
        let mut moves = Vec::new();
        for v in 0..self.positions.len() as u32 {
            let free = 1.0 - self.mask[v as usize];
            if !shown[v as usize] || free <= 0.0 {
                continue;
            }
            let p = self.positions[v as usize];
            let mean = || {
                let around = self.neighbors.of(v);
                (!around.is_empty()).then(|| {
                    around
                        .iter()
                        .map(|&n| self.positions[n as usize])
                        .sum::<Vec3>()
                        / around.len() as f32
                })
            };
            let step = match filter {
                MeshFilter::Smooth => match mean() {
                    Some(mean) => (mean - p) * amount.clamp(-1.0, 1.0),
                    None => continue,
                },
                MeshFilter::Sharpen => match mean() {
                    Some(mean) => (p - mean) * (amount.clamp(-1.0, 1.0) * 0.5),
                    None => continue,
                },
                MeshFilter::Inflate => self.normals[v as usize] * (amount * size * PUSH),
                MeshFilter::Scale => (p - middle) * (amount * GROW),
                MeshFilter::Random => self.normals[v as usize] * (noise(v) * amount * size * PUSH),
            };
            let step = step * free;
            if step != Vec3::ZERO && step.is_finite() {
                moves.push((v, step));
            }
        }
        let moved: Vec<u32> = moves.iter().map(|&(v, _)| v).collect();
        for (v, step) in moves {
            self.positions[v as usize] += step;
        }
        self.after_move_all(&moved);
        !moved.is_empty()
    }
}
