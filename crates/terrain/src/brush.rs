//! Sculpting and painting brushes.
//!
//! A brush stroke is a series of stamps: [`Stroke`] spaces them evenly
//! along the mouse's path (so a fast drag leaves no gaps) and repeats them
//! while the mouse is held still. Each stamp ([`apply`]) changes only the
//! samples under the brush and returns that rectangle, so whatever draws
//! the terrain rebuilds only those chunks.
//!
//! [`EditRecorder`] keeps a copy of each chunk the first time a stroke
//! changes it, so the whole stroke undoes as one step without copying the
//! whole terrain. Foliage stamps scatter plants and props the same way.

use std::collections::BTreeMap;
use std::sync::Arc;

use axcenity_core::{
    CHUNK_CELLS, FoliageInstance, FoliageSource, MAX_FOLIAGE_INSTANCES, MAX_TERRAIN_LAYERS,
    PaintWeights, SampleRect, TerrainData, TerrainRegion,
};
use glam::{Vec2, Vec3};

use crate::Placement;
use crate::noise::{Noise, Rng, mix, smoothstep};

/// How much Raise, Lower and Sculpt move the ground in one stamp at full
/// strength, as a fraction of the brush's radius.
const RAISE_PER_STAMP: f32 = 0.05;
/// How far Flatten and Smooth go in one stamp at full strength.
const BLEND_PER_STAMP: f32 = 0.5;
/// The most stamps one call to [`Stroke::advance`] makes, so a huge jump
/// (the mouse leaving and coming back far away) can't freeze the editor.
const MAX_STAMPS_PER_STEP: usize = 64;
/// The most plants or props one foliage stamp adds, so a huge cell or a
/// high density can't freeze the editor.
const MAX_FOLIAGE_PER_STAMP: usize = 256;

/// How the brush's edge fades, from its inner part to its rim.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Curve {
    /// An S curve: gentle at both ends.
    #[default]
    Smooth,
    Linear,
    /// Falls quickly past the inner part, for pointed shapes.
    Sharp,
    /// Stays strong almost to the rim.
    Soft,
}

impl Curve {
    pub const ALL: [Curve; 4] = [Curve::Smooth, Curve::Linear, Curve::Sharp, Curve::Soft];

    pub fn label(self) -> &'static str {
        match self {
            Curve::Smooth => "Smooth",
            Curve::Linear => "Linear",
            Curve::Sharp => "Sharp",
            Curve::Soft => "Soft",
        }
    }

    /// `u` from 0 (the rim) to 1 (the inner part) as a weight.
    fn shape(self, u: f32) -> f32 {
        match self {
            Curve::Smooth => smoothstep(0.0, 1.0, u),
            Curve::Linear => u,
            Curve::Sharp => u * u,
            Curve::Soft => u.sqrt(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Shape {
    #[default]
    Circle,
    Square,
}

/// Stamps mirrored across the terrain's middle too.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Symmetry {
    #[default]
    None,
    /// Mirrored left to right (across the line along Z through the middle).
    X,
    /// Mirrored front to back.
    Z,
    /// Mirrored both ways: four stamps.
    Both,
}

impl Symmetry {
    pub const ALL: [Symmetry; 4] = [Symmetry::None, Symmetry::X, Symmetry::Z, Symmetry::Both];

    pub fn label(self) -> &'static str {
        match self {
            Symmetry::None => "None",
            Symmetry::X => "Mirror X",
            Symmetry::Z => "Mirror Z",
            Symmetry::Both => "Mirror both",
        }
    }
}

/// The brush's size and feel, shared by every tool.
#[derive(Clone, Debug, PartialEq)]
pub struct Brush {
    /// Studs from the middle to the rim.
    pub radius: f32,
    /// 0 to 1.
    pub strength: f32,
    /// 0 to 1: how much of the radius fades out (0 is a hard edge).
    pub falloff: f32,
    pub curve: Curve,
    pub shape: Shape,
    /// The distance between stamps along a stroke, as a fraction of the radius.
    pub spacing: f32,
    pub symmetry: Symmetry,
    /// Only change ground between these heights (studs above the
    /// terrain's bottom); Raise and Lower also stop at them.
    pub height_limits: Option<(f32, f32)>,
    /// Only change ground whose slope (degrees) is between these.
    pub slope_limits: Option<(f32, f32)>,
}

impl Default for Brush {
    fn default() -> Self {
        Self {
            radius: 12.0,
            strength: 0.5,
            falloff: 0.6,
            curve: Curve::Smooth,
            shape: Shape::Circle,
            spacing: 0.25,
            symmetry: Symmetry::None,
            height_limits: None,
            slope_limits: None,
        }
    }
}

impl Brush {
    /// How strongly the brush acts `offset` studs (X, Z) from its middle:
    /// 1 inside the inner part, fading to 0 at the rim.
    pub fn weight(&self, offset: Vec2) -> f32 {
        let radius = self.radius.max(0.01);
        let distance = match self.shape {
            Shape::Circle => offset.length(),
            Shape::Square => offset.x.abs().max(offset.y.abs()),
        } / radius;
        if distance >= 1.0 {
            return 0.0;
        }
        let falloff = self.falloff.clamp(0.0, 1.0);
        let inner = 1.0 - falloff;
        if distance <= inner || falloff == 0.0 {
            return 1.0;
        }
        self.curve.shape(1.0 - (distance - inner) / falloff)
    }

    /// Studs between stamps.
    pub fn spacing_studs(&self) -> f32 {
        (self.radius * self.spacing.clamp(0.02, 2.0)).max(0.05)
    }
}

/// Sculpt's shapes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SculptMode {
    /// Raises rounded hills.
    #[default]
    Hill,
    /// Digs rounded hollows.
    Basin,
    /// Raises a sharp crest along the stroke.
    Ridge,
    /// Cuts a V-shaped valley along the stroke.
    Valley,
    /// Pulls the ground into flat steps `step` studs apart, making cliffs.
    Terrace,
}

impl SculptMode {
    pub const ALL: [SculptMode; 5] = [
        SculptMode::Hill,
        SculptMode::Basin,
        SculptMode::Ridge,
        SculptMode::Valley,
        SculptMode::Terrace,
    ];

    pub fn label(self) -> &'static str {
        match self {
            SculptMode::Hill => "Hill",
            SculptMode::Basin => "Basin",
            SculptMode::Ridge => "Ridge",
            SculptMode::Valley => "Valley",
            SculptMode::Terrace => "Terrace",
        }
    }
}

/// Which way Flatten may move the ground.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FlattenMode {
    #[default]
    Both,
    /// Only fills in what's below the target.
    RaiseOnly,
    /// Only cuts away what's above it.
    LowerOnly,
}

impl FlattenMode {
    pub const ALL: [FlattenMode; 3] = [
        FlattenMode::Both,
        FlattenMode::RaiseOnly,
        FlattenMode::LowerOnly,
    ];

    pub fn label(self) -> &'static str {
        match self {
            FlattenMode::Both => "Both ways",
            FlattenMode::RaiseOnly => "Raise only",
            FlattenMode::LowerOnly => "Lower only",
        }
    }
}

/// The built-in stamp shapes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StampShape {
    #[default]
    Mountain,
    Crater,
    Plateau,
    Ridge,
    Depression,
}

impl StampShape {
    pub const ALL: [StampShape; 5] = [
        StampShape::Mountain,
        StampShape::Crater,
        StampShape::Plateau,
        StampShape::Ridge,
        StampShape::Depression,
    ];

    pub fn label(self) -> &'static str {
        match self {
            StampShape::Mountain => "Mountain",
            StampShape::Crater => "Crater",
            StampShape::Plateau => "Plateau",
            StampShape::Ridge => "Ridge",
            StampShape::Depression => "Depression",
        }
    }

    /// The shape's height at (`u`, `v`), each -1 to 1 across the stamp,
    /// from about -1 to 1.
    pub fn height(self, u: f32, v: f32) -> f32 {
        let r = (u * u + v * v).sqrt();
        match self {
            StampShape::Mountain => {
                // A peak with a few shoulders, the same every time.
                let shoulders = Noise::new(11).fbm(u * 2.5, v * 2.5, 3, 0.5, 2.0) * 0.25;
                ((1.0 - r).max(0.0).powf(1.6) * (1.0 + shoulders)).max(0.0)
            }
            StampShape::Crater => {
                // A bowl below the ground with a raised rim at 0.75.
                let rim = (-((r - 0.75) / 0.15).powi(2)).exp() * 0.6;
                let bowl = -(1.0 - smoothstep(0.0, 0.8, r)) * 0.8;
                (rim + bowl) * (1.0 - smoothstep(0.9, 1.0, r))
            }
            StampShape::Plateau => 1.0 - smoothstep(0.55, 0.95, r),
            StampShape::Ridge => {
                let across = 1.0 - (v.abs() * 2.5).min(1.0);
                let along = 1.0 - smoothstep(0.6, 1.0, u.abs());
                across.powf(1.5) * along
            }
            StampShape::Depression => -(1.0 - smoothstep(0.2, 1.0, r)),
        }
    }
}

/// A heightmap image used as a stamp: values from 0 to 1, row by row.
#[derive(Clone, Debug, PartialEq)]
pub struct StampImage {
    pub width: usize,
    pub height: usize,
    pub values: Vec<f32>,
}

impl StampImage {
    /// The value at (`u`, `v`), each -1 to 1 across the image, blended.
    fn sample(&self, u: f32, v: f32) -> f32 {
        let x = ((u + 1.0) / 2.0 * (self.width - 1) as f32).clamp(0.0, (self.width - 1) as f32);
        let y = ((v + 1.0) / 2.0 * (self.height - 1) as f32).clamp(0.0, (self.height - 1) as f32);
        let (x0, y0) = (x.floor() as usize, y.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(self.width - 1), (y0 + 1).min(self.height - 1));
        let at = |x: usize, y: usize| self.values[y * self.width + x];
        let (tx, ty) = (x - x0 as f32, y - y0 as f32);
        let top = at(x0, y0) + (at(x1, y0) - at(x0, y0)) * tx;
        let bottom = at(x0, y1) + (at(x1, y1) - at(x0, y1)) * tx;
        top + (bottom - top) * ty
    }
}

/// How a stamp combines with the ground.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StampMode {
    /// Adds the shape's height.
    #[default]
    Add,
    /// Raises the ground to the shape where it's below it.
    Max,
    /// Lowers the ground to the shape where it's above it.
    Min,
}

impl StampMode {
    pub const ALL: [StampMode; 3] = [StampMode::Add, StampMode::Max, StampMode::Min];

    pub fn label(self) -> &'static str {
        match self {
            StampMode::Add => "Add",
            StampMode::Max => "Raise to",
            StampMode::Min => "Lower to",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Stamp {
    pub shape: StampShape,
    /// A heightmap image instead of `shape`.
    pub image: Option<Arc<StampImage>>,
    /// Degrees around the up axis.
    pub rotation: f32,
    /// Studs at the shape's highest point (negative digs).
    pub height: f32,
    pub mode: StampMode,
}

impl Default for Stamp {
    fn default() -> Self {
        Self {
            shape: StampShape::Mountain,
            image: None,
            rotation: 0.0,
            height: 24.0,
            mode: StampMode::Add,
        }
    }
}

/// How painting puts a layer down.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PaintMode {
    /// Blends toward the layer, more with each stamp.
    #[default]
    Blend,
    /// Puts the layer down fully wherever the brush reaches.
    Replace,
}

/// What one stamp does.
#[derive(Clone, Debug, PartialEq)]
pub enum Operation {
    Raise,
    Lower,
    /// Evens out bumps, a few samples across at a time, so large shapes stay.
    Smooth,
    /// Moves the ground toward `target` (world Y).
    Flatten {
        target: f32,
        mode: FlattenMode,
    },
    Sculpt {
        mode: SculptMode,
        /// Terrace's step, in studs.
        step: f32,
    },
    /// Adds bumps from noise `scale` studs across, up to `amplitude` studs
    /// (negative turns them upside down).
    Noise {
        amplitude: f32,
        scale: f32,
        seed: u64,
    },
    Stamp(Stamp),
    /// Paints layer `layer`. Layers marked in `locked` keep their paint.
    Paint {
        layer: usize,
        mode: PaintMode,
        locked: [bool; MAX_TERRAIN_LAYERS],
    },
    /// Takes paint away, back to the automatic rules.
    Erase {
        locked: [bool; MAX_TERRAIN_LAYERS],
    },
    /// Scatters `source` at `density` plants per square stud, or removes
    /// that source when `erase` is set. `collide` is stored on each new
    /// instance.
    Foliage {
        source: FoliageSource,
        density: f32,
        erase: bool,
        collide: bool,
    },
    /// Cuts the cells under the brush out of the surface (procedural
    /// worlds PW7), or fills them back in when `fill` is set.
    Holes {
        fill: bool,
    },
}

impl Operation {
    /// True for operations that change heights (the rest change paint or foliage).
    pub fn sculpts(&self) -> bool {
        !matches!(
            self,
            Operation::Paint { .. }
                | Operation::Erase { .. }
                | Operation::Foliage { .. }
                | Operation::Holes { .. }
        )
    }
}

/// Applies one stamp of `operation` at `center` (world), and its mirror
/// images, and returns the samples it changed.
pub fn apply(
    data: &mut TerrainData,
    placement: &Placement,
    brush: &Brush,
    operation: &Operation,
    center: Vec3,
) -> SampleRect {
    let mut changed = SampleRect::default();
    for (center, flip) in mirrored(placement, brush.symmetry, center) {
        let rect = apply_one(data, placement, brush, operation, center, flip);
        changed = changed.union(rect);
    }
    changed
}

/// `center` and its mirror images, each with whether it's flipped (for a
/// stamp's turn).
fn mirrored(placement: &Placement, symmetry: Symmetry, center: Vec3) -> Vec<(Vec3, bool)> {
    let middle = placement.position;
    let flip_x = Vec3::new(2.0 * middle.x - center.x, center.y, center.z);
    let flip_z = Vec3::new(center.x, center.y, 2.0 * middle.z - center.z);
    let flip_both = Vec3::new(flip_x.x, center.y, flip_z.z);
    match symmetry {
        Symmetry::None => vec![(center, false)],
        Symmetry::X => vec![(center, false), (flip_x, true)],
        Symmetry::Z => vec![(center, false), (flip_z, true)],
        Symmetry::Both => vec![
            (center, false),
            (flip_x, true),
            (flip_z, true),
            (flip_both, false),
        ],
    }
}

/// The samples one stamp at `center` may change: its footprint and its
/// mirror images' (see [`Brush::symmetry`]).
pub fn affected_rects(
    data: &TerrainData,
    placement: &Placement,
    brush: &Brush,
    center: Vec3,
) -> Vec<SampleRect> {
    mirrored(placement, brush.symmetry, center)
        .into_iter()
        .map(|(center, _)| footprint(data, placement, Vec2::new(center.x, center.z), brush.radius))
        .collect()
}

/// The samples within `radius` studs of `center` (X, Z), on the grid.
pub fn footprint(
    data: &TerrainData,
    placement: &Placement,
    center: Vec2,
    radius: f32,
) -> SampleRect {
    let (size_x, size_z) = data.size();
    let cell = placement.cell_size(data);
    let grid = placement.to_grid(data, center.x, center.y);
    let reach = Vec2::new(radius / cell.x, radius / cell.y);
    let low = (grid - reach).floor();
    let high = (grid + reach).ceil() + Vec2::ONE;
    let clamp = |value: f32, size: usize| value.clamp(0.0, size as f32) as usize;
    SampleRect::new(
        clamp(low.x, size_x),
        clamp(low.y, size_z),
        clamp(high.x, size_x),
        clamp(high.y, size_z),
    )
}

fn apply_one(
    data: &mut TerrainData,
    placement: &Placement,
    brush: &Brush,
    operation: &Operation,
    center: Vec3,
    flipped: bool,
) -> SampleRect {
    let center_xz = Vec2::new(center.x, center.z);
    let rect = footprint(data, placement, center_xz, brush.radius);
    if rect.is_empty() {
        return rect;
    }
    let (size_x, _) = data.size();
    let cell = placement.cell_size(data);
    // Every weight first, from the heights as they were, so changing one
    // sample doesn't affect its neighbours' slopes or smoothing.
    let mut weights = Vec::with_capacity(rect.width() * rect.depth());
    for z in rect.z0..rect.z1 {
        for x in rect.x0..rect.x1 {
            let at = placement.to_world_xz(data, Vec2::new(x as f32, z as f32));
            let mut weight = brush.weight(at - center_xz);
            if weight > 0.0 {
                weight *= limits_mask(data, placement, brush, x, z);
            }
            weights.push(weight);
        }
    }
    if weights.iter().all(|&weight| weight <= 0.0) {
        return SampleRect::default();
    }

    if let Operation::Foliage {
        source,
        density,
        erase,
        collide,
    } = operation
    {
        apply_foliage(
            data, placement, brush, rect, &weights, source, *density, *erase, *collide, center_xz,
        );
        return rect;
    }

    if let Operation::Holes { fill } = operation {
        // Every cell whose corner the brush reaches, whatever its strength.
        let holes = data.holes_mut_in(rect);
        let mut index = 0;
        for z in rect.z0..rect.z1 {
            for x in rect.x0..rect.x1 {
                if weights[index] > 0.0 {
                    holes[z * size_x + x] = !fill;
                }
                index += 1;
            }
        }
        return rect;
    }

    if !operation.sculpts() {
        let paint = data.paint_mut_in(rect);
        let mut index = 0;
        for z in rect.z0..rect.z1 {
            for x in rect.x0..rect.x1 {
                let weight = weights[index];
                index += 1;
                if weight > 0.0 {
                    let sample = &mut paint[z * size_x + x];
                    *sample = paint_sample(*sample, operation, weight * brush.strength);
                }
            }
        }
        return rect;
    }

    let to_height = 1.0 / placement.size.y;
    let raise = brush.strength * brush.radius * RAISE_PER_STAMP * to_height;
    let blend = (brush.strength * BLEND_PER_STAMP).clamp(0.0, 1.0);
    let limits = brush
        .height_limits
        .map(|(low, high)| (low * to_height, high * to_height));
    // Smoothing reads the heights as they were, one sample around.
    let before = data.read_region(rect.grown(2, data.size().0, data.size().1));
    let old = |x: usize, z: usize| {
        let x = x.clamp(before.rect.x0, before.rect.x1 - 1);
        let z = z.clamp(before.rect.z0, before.rect.z1 - 1);
        before.heights[(z - before.rect.z0) * before.rect.width() + (x - before.rect.x0)]
    };
    let noise_of = |seed: u64| Noise::new(seed);
    let heights = data.heights_mut_in(rect);
    let mut index = 0;
    for z in rect.z0..rect.z1 {
        for x in rect.x0..rect.x1 {
            let weight = weights[index];
            index += 1;
            if weight <= 0.0 {
                continue;
            }
            let height = old(x, z);
            let world = placement.world_xz_at(cell, x, z);
            let local = (world - center_xz) / brush.radius.max(0.01);
            let new = match operation {
                Operation::Raise => height + raise * weight,
                Operation::Lower => height - raise * weight,
                Operation::Smooth => {
                    // A 3 × 3 blur: removes bumps a few samples across, and
                    // only slowly wears down bigger shapes.
                    let mut sum = 0.0;
                    let mut total = 0.0;
                    for dz in -1i32..=1 {
                        for dx in -1i32..=1 {
                            let k = if dx == 0 && dz == 0 {
                                4.0
                            } else if dx == 0 || dz == 0 {
                                2.0
                            } else {
                                1.0
                            };
                            let nx = (x as i32 + dx).max(0) as usize;
                            let nz = (z as i32 + dz).max(0) as usize;
                            sum += old(nx, nz) * k;
                            total += k;
                        }
                    }
                    height + (sum / total - height) * blend * weight
                }
                Operation::Flatten { target, mode } => {
                    let target = placement.height_of(*target);
                    let moved = height + (target - height) * blend * weight;
                    match mode {
                        FlattenMode::Both => moved,
                        FlattenMode::RaiseOnly => moved.max(height),
                        FlattenMode::LowerOnly => moved.min(height),
                    }
                }
                Operation::Sculpt { mode, step } => {
                    let distance = local.length().min(1.0);
                    match mode {
                        SculptMode::Hill => height + raise * weight * weight,
                        SculptMode::Basin => height - raise * weight * weight,
                        SculptMode::Ridge => {
                            height + raise * (1.0 - distance).powi(2) * 2.0 * weight
                        }
                        SculptMode::Valley => {
                            height - raise * (1.0 - distance).powi(2) * 2.0 * weight
                        }
                        SculptMode::Terrace => {
                            let step = (step.max(0.1)) * to_height;
                            let level = (height / step).round() * step;
                            height + (level - height) * blend * weight
                        }
                    }
                }
                Operation::Noise {
                    amplitude,
                    scale,
                    seed,
                } => {
                    let noise = noise_of(*seed).fbm(
                        world.x / scale.max(0.1),
                        world.y / scale.max(0.1),
                        4,
                        0.5,
                        2.0,
                    );
                    height + noise * amplitude * to_height * brush.strength * weight * 0.25
                }
                Operation::Stamp(stamp) => {
                    let turn = if flipped {
                        -stamp.rotation
                    } else {
                        stamp.rotation
                    }
                    .to_radians();
                    let (sin, cos) = turn.sin_cos();
                    let (u, v) = (
                        local.x * cos + local.y * sin,
                        -local.x * sin + local.y * cos,
                    );
                    let shape = match &stamp.image {
                        Some(image) => image.sample(u, v),
                        None => stamp.shape.height(u, v),
                    };
                    let lift = shape * stamp.height * to_height * brush.strength.max(0.05) * 2.0;
                    match stamp.mode {
                        StampMode::Add => height + lift * weight,
                        StampMode::Max => {
                            let level = placement.height_of(center.y) + lift;
                            height + (level.max(height) - height) * weight
                        }
                        StampMode::Min => {
                            let level = placement.height_of(center.y) + lift;
                            height + (level.min(height) - height) * weight
                        }
                    }
                }
                Operation::Paint { .. }
                | Operation::Erase { .. }
                | Operation::Foliage { .. }
                | Operation::Holes { .. } => height,
            };
            let new = match limits {
                // Raising stops at the top limit and lowering at the bottom;
                // ground already past one isn't pulled back.
                Some((_, high)) if new > height => new.min(high.max(height)),
                Some((low, _)) if new < height => new.max(low.min(height)),
                _ => new,
            };
            heights[z * size_x + x] = new;
        }
    }
    data.clamp_heights_in(rect);
    rect
}

impl Placement {
    /// The world X and Z of sample (`x`, `z`), with the cell size already known.
    fn world_xz_at(&self, cell: Vec2, x: usize, z: usize) -> Vec2 {
        Vec2::new(
            self.position.x - self.size.x / 2.0 + x as f32 * cell.x,
            self.position.z - self.size.z / 2.0 + z as f32 * cell.y,
        )
    }
}

/// 1 where the brush's height and slope limits let it act on sample
/// (`x`, `z`), 0 where not.
fn limits_mask(
    data: &TerrainData,
    placement: &Placement,
    brush: &Brush,
    x: usize,
    z: usize,
) -> f32 {
    if let Some((low, high)) = brush.height_limits {
        let studs = data.height(x as isize, z as isize) * placement.size.y;
        if studs < low || studs > high {
            return 0.0;
        }
    }
    if let Some((low, high)) = brush.slope_limits {
        let slope = placement.slope_degrees(data, x, z);
        if slope < low || slope > high {
            return 0.0;
        }
    }
    1.0
}

#[allow(clippy::too_many_arguments)] // Stamp inputs: brush, samples, and what to scatter.
fn apply_foliage(
    data: &mut TerrainData,
    placement: &Placement,
    brush: &Brush,
    rect: SampleRect,
    weights: &[f32],
    source: &FoliageSource,
    density: f32,
    erase: bool,
    collide: bool,
    center: Vec2,
) {
    if erase {
        let drop: Vec<bool> = data
            .foliage()
            .iter()
            .map(|instance| {
                if instance.source != *source || !instance.in_rect(rect) {
                    return false;
                }
                let at = placement.to_world_xz(data, Vec2::new(instance.x, instance.z));
                brush.weight(at - center) > 0.0
            })
            .collect();
        if drop.iter().any(|&drop| drop) {
            let mut index = 0;
            data.foliage_mut().retain(|_| {
                let gone = drop[index];
                index += 1;
                !gone
            });
        }
        return;
    }
    let density = density.max(0.0);
    if density <= 0.0 {
        return;
    }
    let (size_x, size_z) = data.size();
    let cell = placement.cell_size(data);
    let cell_area = cell.x * cell.y;
    let seed = mix(source.name().len() as u64
        ^ (center.x.to_bits() as u64)
        ^ (center.y.to_bits() as u64).rotate_left(32));
    let mut rng = Rng::new(seed);
    let mut added = Vec::new();
    let mut index = 0;
    for z in rect.z0..rect.z1 {
        for x in rect.x0..rect.x1 {
            let weight = weights[index] * brush.strength;
            index += 1;
            if weight <= 0.0 {
                continue;
            }
            let mut expected = density * cell_area * weight;
            while expected > 0.0 && added.len() < MAX_FOLIAGE_PER_STAMP {
                let chance = expected.min(1.0);
                expected -= 1.0;
                if rng.next_f32() >= chance {
                    continue;
                }
                let jitter_x = (rng.next_f32() - 0.5) * 0.9;
                let jitter_z = (rng.next_f32() - 0.5) * 0.9;
                let gx = (x as f32 + jitter_x).clamp(0.0, (size_x - 1) as f32);
                let gz = (z as f32 + jitter_z).clamp(0.0, (size_z - 1) as f32);
                let at = placement.to_world_xz(data, Vec2::new(gx, gz));
                if brush.weight(at - center) <= 0.0 {
                    continue;
                }
                added.push(FoliageInstance {
                    source: source.clone(),
                    x: gx,
                    z: gz,
                    yaw: rng.next_f32() * std::f32::consts::TAU,
                    scale: 0.85 + rng.next_f32() * 0.3,
                    collide,
                    lean: 0.0,
                    sink: 0.0,
                });
            }
            if added.len() >= MAX_FOLIAGE_PER_STAMP {
                break;
            }
        }
        if added.len() >= MAX_FOLIAGE_PER_STAMP {
            break;
        }
    }
    if added.is_empty() {
        return;
    }
    let foliage = data.foliage_mut();
    let room = MAX_FOLIAGE_INSTANCES.saturating_sub(foliage.len());
    foliage.extend(added.into_iter().take(room));
}

/// One sample's paint after painting or erasing with `amount` (0 to 1).
/// Paint adds up to at most 255; painting takes from the unlocked layers
/// and from what's left to the automatic rules.
pub fn paint_sample(sample: PaintWeights, operation: &Operation, amount: f32) -> PaintWeights {
    let amount = amount.clamp(0.0, 1.0);
    let mut weights = sample.map(|w| w as f32 / 255.0);
    match operation {
        Operation::Paint {
            layer,
            mode,
            locked,
        } => {
            let layer = (*layer).min(MAX_TERRAIN_LAYERS - 1);
            let amount = match mode {
                PaintMode::Blend => amount,
                PaintMode::Replace => {
                    if amount > 0.0 {
                        1.0
                    } else {
                        0.0
                    }
                }
            };
            for (index, weight) in weights.iter_mut().enumerate() {
                if index != layer && !locked[index] {
                    *weight *= 1.0 - amount;
                }
            }
            let others: f32 = (0..MAX_TERRAIN_LAYERS)
                .filter(|&index| index != layer)
                .map(|index| weights[index])
                .sum();
            let room = (1.0 - others).max(0.0);
            weights[layer] = (weights[layer] * (1.0 - amount) + amount).min(room);
        }
        Operation::Erase { locked } => {
            for (index, weight) in weights.iter_mut().enumerate() {
                if !locked[index] {
                    *weight *= 1.0 - amount;
                }
            }
        }
        _ => return sample,
    }
    quantize(weights)
}

/// Weights from 0 to 1 as bytes adding up to at most 255.
fn quantize(weights: [f32; MAX_TERRAIN_LAYERS]) -> PaintWeights {
    let mut bytes = weights.map(|w| (w.clamp(0.0, 1.0) * 255.0).round() as u8);
    loop {
        let total: u32 = bytes.iter().map(|&b| u32::from(b)).sum();
        if total <= 255 {
            return bytes;
        }
        // Rounding up went over: take one from the biggest.
        if let Some(biggest) = bytes.iter_mut().max() {
            *biggest -= 1;
        }
    }
}

/// Spaces a stroke's stamps evenly along the mouse's path.
#[derive(Clone, Debug, Default)]
pub struct Stroke {
    last: Option<Vec3>,
    /// Studs moved since the last stamp.
    travelled: f32,
    /// Seconds since the last stamp.
    waited: f32,
}

impl Stroke {
    pub fn new() -> Self {
        Self::default()
    }

    /// Moves the stroke to `to` after `seconds`, and returns where to
    /// stamp: the first point, then one every `spacing` studs along the
    /// way, or, held still, one every `repeat` seconds (if given).
    pub fn advance(
        &mut self,
        to: Vec3,
        spacing: f32,
        seconds: f32,
        repeat: Option<f32>,
    ) -> Vec<Vec3> {
        let spacing = spacing.max(0.01);
        let Some(last) = self.last else {
            self.last = Some(to);
            self.waited = 0.0;
            return vec![to];
        };
        let mut stamps = Vec::new();
        let step = to - last;
        let length = Vec2::new(step.x, step.z).length();
        if length > 0.0 {
            // The distance from `last` to the next stamp.
            let mut along = spacing - self.travelled;
            while along <= length && stamps.len() < MAX_STAMPS_PER_STEP {
                stamps.push(last + step * (along / length));
                along += spacing;
            }
            self.travelled = (self.travelled + length) % spacing;
            if stamps.len() == MAX_STAMPS_PER_STEP {
                self.travelled = 0.0;
            }
        }
        self.last = Some(to);
        if stamps.is_empty() {
            self.waited += seconds;
            if let Some(repeat) = repeat
                && self.waited >= repeat
            {
                self.waited = 0.0;
                stamps.push(to);
            }
        } else {
            self.waited = 0.0;
        }
        stamps
    }
}

/// The samples one chunk keeps for undo: its cells, without the edge it
/// shares with the next chunk, so no two chunks keep the same sample.
fn kept_rect(data: &TerrainData, cx: usize, cz: usize) -> SampleRect {
    let (size_x, size_z) = data.size();
    let (chunks_x, chunks_z) = data.chunk_counts();
    let end = |c: usize, chunks: usize, size: usize| {
        if c + 1 == chunks {
            size
        } else {
            (c + 1) * CHUNK_CELLS
        }
    };
    SampleRect::new(
        cx * CHUNK_CELLS,
        cz * CHUNK_CELLS,
        end(cx, chunks_x, size_x),
        end(cz, chunks_z, size_z),
    )
}

/// Collects what a stroke changes, chunk by chunk, for one undo step.
#[derive(Debug, Default)]
pub struct EditRecorder {
    /// Each changed chunk as it was before the stroke first changed it.
    before: BTreeMap<(usize, usize), TerrainRegion>,
}

impl EditRecorder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Call before changing the samples in `rect`: keeps a copy of each
    /// chunk there that isn't kept yet.
    pub fn before_change(&mut self, data: &TerrainData, rect: SampleRect) {
        if rect.is_empty() {
            return;
        }
        let (chunks_x, chunks_z) = data.chunk_counts();
        let first = |sample: usize, chunks: usize| (sample / CHUNK_CELLS).min(chunks - 1);
        for cz in first(rect.z0, chunks_z)..=first(rect.z1 - 1, chunks_z) {
            for cx in first(rect.x0, chunks_x)..=first(rect.x1 - 1, chunks_x) {
                self.before
                    .entry((cx, cz))
                    .or_insert_with(|| data.read_region(kept_rect(data, cx, cz)));
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.before.is_empty()
    }

    /// The finished stroke: each kept chunk before and as it is now.
    /// `None` if nothing actually changed.
    pub fn finish(self, data: &TerrainData) -> Option<TerrainEdit> {
        let mut edit = TerrainEdit::default();
        for region in self.before.into_values() {
            let now = data.read_region(region.rect);
            if now != region {
                edit.before.push(region);
                edit.after.push(now);
            }
        }
        (!edit.before.is_empty()).then_some(edit)
    }
}

/// A stroke's changes: the chunks it changed, before and after.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TerrainEdit {
    pub before: Vec<TerrainRegion>,
    pub after: Vec<TerrainRegion>,
}

impl TerrainEdit {
    pub fn undo(&self, data: &mut TerrainData) {
        for region in &self.before {
            data.write_region(region);
        }
    }

    pub fn redo(&self, data: &mut TerrainData) {
        for region in &self.after {
            data.write_region(region);
        }
    }

    /// Every sample it changes.
    pub fn rect(&self) -> SampleRect {
        self.before
            .iter()
            .fold(SampleRect::default(), |rect, region| {
                rect.union(region.rect)
            })
    }
}

#[cfg(test)]
mod tests;
