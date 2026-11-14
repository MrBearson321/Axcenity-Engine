//! Roads and paths on the terrain (procedural worlds PW4). A `TerrainPath`
//! inside a Spline says how wide the road is and what it does to the
//! ground: scatter rules keep off its strip, live, while it's there
//! ([`keep_offs`], read by [`terrain_scatter`](crate::terrain_scatter)),
//! and [`apply_path`] flattens and paints the ground under it (the editor's
//! Apply, an undo step, since heights and paint are saved).
//!
//! Distances are seen from above: a strip is every point within half the
//! path's width of the curve, however the curve climbs.

use axcenity_core::spline::SplineCurve;
use axcenity_core::{ClassName, Instance, InstanceId, SampleRect, TerrainData, Value, World};
use axcenity_terrain::Placement;
use axcenity_terrain::brush::{Operation, PaintMode, paint_sample};
use axcenity_terrain::noise::smoothstep;
use glam::{Vec2, Vec3};

use crate::flatten::nearest;
use crate::terrain_scatter::fnv;
use crate::v3;

/// Studs between the points a keep-off strip is followed by.
const KEEP_OFF_SPACING: f32 = 2.0;

/// A TerrainPath's settings.
#[derive(Clone, Debug, PartialEq)]
pub struct PathSettings {
    pub width: f32,
    pub falloff: f32,
    pub flatten: bool,
    /// The TerrainLayer it paints, by name; empty for none.
    pub layer: String,
    pub clear_scatter: bool,
    pub clear_margin: f32,
    /// Route's steepest climb, in degrees.
    pub max_grade: f32,
}

impl PathSettings {
    /// A TerrainPath's settings, or `None` for another class or one that's
    /// switched off.
    pub fn of(instance: &Instance) -> Option<PathSettings> {
        if instance.class() != ClassName::TerrainPath
            || instance.property("Enabled") != Some(&Value::Bool(true))
        {
            return None;
        }
        let number = |name, default: f32| {
            instance
                .property(name)
                .and_then(Value::as_number)
                .map(|n| n as f32)
                .filter(|n| n.is_finite())
                .unwrap_or(default)
        };
        let on = |name| instance.property(name) == Some(&Value::Bool(true));
        Some(PathSettings {
            width: number("Width", 8.0).max(0.0),
            falloff: number("Falloff", 4.0).max(0.0),
            flatten: on("Flatten"),
            layer: instance
                .property("Layer")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_owned(),
            clear_scatter: on("ClearScatter"),
            clear_margin: number("ClearMargin", 1.0).max(0.0),
            max_grade: number("MaxGrade", 8.0).clamp(0.5, 60.0),
        })
    }
}

/// A strip scatter rules keep off: within `reach` studs of `line`, seen
/// from above (world X and Z).
#[derive(Clone, Debug, PartialEq)]
pub struct KeepOff {
    pub line: Vec<Vec2>,
    pub reach: f32,
    /// The strip's box.
    pub min: Vec2,
    pub max: Vec2,
    /// Of the line and reach, to tell when the strip changed.
    pub hash: u64,
}

impl KeepOff {
    /// The strip within `reach` studs of `line`, or `None` without points.
    pub fn new(line: Vec<Vec2>, reach: f32) -> Option<KeepOff> {
        if line.is_empty() {
            return None;
        }
        let (min, max) = line
            .iter()
            .fold((Vec2::INFINITY, Vec2::NEG_INFINITY), |(min, max), &p| {
                (min.min(p), max.max(p))
            });
        let mut bytes = Vec::with_capacity(line.len() * 8 + 4);
        for p in &line {
            bytes.extend(p.x.to_bits().to_le_bytes());
            bytes.extend(p.y.to_bits().to_le_bytes());
        }
        bytes.extend(reach.to_bits().to_le_bytes());
        Some(KeepOff {
            hash: fnv(&bytes),
            min: min - Vec2::splat(reach),
            max: max + Vec2::splat(reach),
            line,
            reach,
        })
    }

    /// Whether world `at` (X, Z) is inside the strip.
    pub fn covers(&self, at: Vec2) -> bool {
        if at.x < self.min.x || at.y < self.min.y || at.x > self.max.x || at.y > self.max.y {
            return false;
        }
        let reach = self.reach * self.reach;
        if self.line.len() == 1 {
            return self.line[0].distance_squared(at) <= reach;
        }
        self.line
            .windows(2)
            .any(|pair| segment_distance_squared(pair[0], pair[1], at) <= reach)
    }

    /// Whether the strip's box overlaps the box from `min` to `max`.
    pub fn overlaps(&self, min: Vec2, max: Vec2) -> bool {
        self.min.x <= max.x && self.min.y <= max.y && self.max.x >= min.x && self.max.y >= min.y
    }
}

fn segment_distance_squared(a: Vec2, b: Vec2, at: Vec2) -> f32 {
    let along = b - a;
    let t = if along.length_squared() > 1e-12 {
        ((at - a).dot(along) / along.length_squared()).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (a + along * t).distance_squared(at)
}

/// The Spline a TerrainPath is in, if its parent is one.
pub fn spline_of(world: &World, path: InstanceId) -> Option<InstanceId> {
    let parent = world.get(path)?.parent()?;
    (world.get(parent)?.class() == ClassName::Spline).then_some(parent)
}

/// The strips scatter rules keep off: every TerrainPath in a Spline that's
/// on and has `ClearScatter`, in tree order, `Width / 2 + ClearMargin`
/// studs either side of its curve.
pub fn keep_offs(world: &World) -> Vec<KeepOff> {
    world
        .all_below(world.root(), |class| class == ClassName::TerrainPath)
        .into_iter()
        .filter_map(|path| {
            let settings = PathSettings::of(world.get(path)?)?;
            if !settings.clear_scatter {
                return None;
            }
            let curve = SplineCurve::of(world, spline_of(world, path)?)?;
            let line = curve
                .polyline(KEEP_OFF_SPACING)
                .into_iter()
                .map(|p| Vec2::new(p.x, p.z))
                .collect();
            KeepOff::new(line, settings.width / 2.0 + settings.clear_margin)
        })
        .collect()
}

/// Flattens and paints `data` under `curve` as `path` says: within half its
/// width fully (the ground at the height of the curve's nearest point,
/// layer `layer` painted on), blending back into the ground over its
/// `Falloff`. `layer` is the layer's place among the terrain's layers;
/// `None` paints nothing. Returns how many samples it changed.
pub fn apply_path(
    data: &mut TerrainData,
    placement: &Placement,
    curve: &SplineCurve,
    path: &PathSettings,
    layer: Option<usize>,
) -> usize {
    if !path.flatten && layer.is_none() {
        return 0;
    }
    let cell = placement.cell_size(data).min_element().max(1e-3);
    let line: Vec<Vec3> = curve
        .polyline((cell / 2.0).max(0.25))
        .into_iter()
        .map(v3)
        .collect();
    let half = path.width / 2.0;
    let reach = half + path.falloff;
    let Some(rect) = rect_around(data, placement, &line, reach) else {
        return 0;
    };
    let (size_x, _) = data.size();
    let paint = layer.map(|layer| Operation::Paint {
        layer,
        mode: PaintMode::Blend,
        locked: [false; axcenity_core::MAX_TERRAIN_LAYERS],
    });
    // Worked out first, so the heights aren't borrowed while reading.
    let mut set = Vec::new();
    for z in rect.z0..rect.z1 {
        for x in rect.x0..rect.x1 {
            let at = placement.to_world_xz(data, Vec2::new(x as f32, z as f32));
            let Some((away, y)) = nearest(&line, at) else {
                continue;
            };
            if away > reach {
                continue;
            }
            let amount = if away <= half {
                1.0
            } else {
                1.0 - smoothstep(half, reach, away)
            };
            let index = z * size_x + x;
            let height = data.heights()[index];
            let height = if path.flatten {
                height + (placement.height_of(y) - height) * amount
            } else {
                height
            };
            let weights = data.paint()[index];
            let weights = match &paint {
                Some(operation) => paint_sample(weights, operation, amount),
                None => weights,
            };
            set.push((index, height, weights));
        }
    }
    if set.is_empty() {
        return 0;
    }
    let (heights, paints) = data.both_mut_in(rect);
    for &(index, height, weights) in &set {
        heights[index] = height;
        paints[index] = weights;
    }
    data.clamp_heights_in(rect);
    set.len()
}

/// The samples of `data` within `reach` studs of `line`'s box, or `None`
/// when that's off the terrain.
pub(crate) fn rect_around(
    data: &TerrainData,
    placement: &Placement,
    line: &[Vec3],
    reach: f32,
) -> Option<SampleRect> {
    if line.is_empty() {
        return None;
    }
    let (min, max) = line
        .iter()
        .fold((Vec2::INFINITY, Vec2::NEG_INFINITY), |(min, max), p| {
            let p = Vec2::new(p.x, p.z);
            (min.min(p), max.max(p))
        });
    let (size_x, size_z) = data.size();
    let low = placement
        .to_grid(data, min.x - reach, min.y - reach)
        .floor();
    let high = placement.to_grid(data, max.x + reach, max.y + reach).ceil();
    let clamp_x = |v: f32| (v.max(0.0) as usize).min(size_x);
    let clamp_z = |v: f32| (v.max(0.0) as usize).min(size_z);
    let rect = SampleRect::new(
        clamp_x(low.x),
        clamp_z(low.y),
        clamp_x(high.x + 1.0),
        clamp_z(high.y + 1.0),
    );
    (!rect.is_empty()).then_some(rect)
}
