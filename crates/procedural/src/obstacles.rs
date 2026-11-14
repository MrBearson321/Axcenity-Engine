//! What scatter rules keep off besides paths (procedural worlds PW5):
//! anchored, colliding parts standing on or near the ground, such as a
//! house's walls and floor, a placed rock model or a fence. A rule with
//! `AvoidParts` places nothing whose spot is under such a part (grown by
//! its `PartMargin`) or that would grow into one.
//!
//! [`Obstacles`] keeps the Workspace's parts between frames: the list of
//! BaseParts is read again only when the Workspace's structure changes, and
//! a part's box only when its revision does, so an unchanged world costs a
//! pass over the parts' revisions.

use std::collections::{HashMap, HashSet};

use axcenity_core::{Instance, InstanceId, Value, World};
use glam::{Quat, Vec2, Vec3};

use crate::rotation_from_orientation;
use crate::terrain_scatter::fnv;

/// Studs above the ground a part has to be entirely to let plants grow
/// under it: a part whose bottom is lower than this over a spot keeps
/// plants off it.
pub const CLEARANCE: f32 = 12.0;

/// Studs below the ground the column starts, so a floor laid flush with
/// the ground (its top level with it, give or take rounding in the saved
/// heights) still keeps plants off.
pub const BELOW: f32 = 1.0;

/// One anchored, colliding part's box.
#[derive(Clone, Debug, PartialEq)]
pub struct Obstacle {
    pub center: Vec3,
    pub rotation: Quat,
    pub half: Vec3,
    /// Its box seen from above (world X and Z), without a margin.
    pub min: Vec2,
    pub max: Vec2,
    /// Of its place, turn and size, to tell when it changed.
    pub hash: u64,
}

impl Obstacle {
    /// The box of an anchored, colliding BasePart, or `None` for anything
    /// else.
    pub fn of(instance: &Instance) -> Option<Obstacle> {
        if instance.property("Anchored") != Some(&Value::Bool(true))
            || instance.property("CanCollide") != Some(&Value::Bool(true))
        {
            return None;
        }
        Obstacle::part_box(instance)
    }

    /// Any BasePart's box, anchored or not.
    pub fn part_box(instance: &Instance) -> Option<Obstacle> {
        if !instance.class().is_base_part() {
            return None;
        }
        let vector = |name| {
            let v = instance.property(name).and_then(Value::as_vector3)?;
            let v = Vec3::new(v.x, v.y, v.z);
            v.is_finite().then_some(v)
        };
        let center = vector("Position")?;
        let rotation = rotation_from_orientation(vector("Orientation").unwrap_or(Vec3::ZERO));
        let half = vector("Size")?.abs() / 2.0;
        // The box's corners seen from above.
        let reach = (rotation * Vec3::X * half.x).abs()
            + (rotation * Vec3::Y * half.y).abs()
            + (rotation * Vec3::Z * half.z).abs();
        let bits: Vec<u8> = [center, half]
            .iter()
            .flat_map(|v| v.to_array())
            .chain(rotation.to_array())
            .flat_map(|f| f.to_bits().to_le_bytes())
            .collect();
        Some(Obstacle {
            center,
            rotation,
            half,
            min: Vec2::new(center.x - reach.x, center.z - reach.z),
            max: Vec2::new(center.x + reach.x, center.z + reach.z),
            hash: fnv(&bits),
        })
    }

    /// Whether the column from [`BELOW`] studs under `ground` to
    /// [`CLEARANCE`] studs above it at world `at` (X, Z) passes through the
    /// box grown by `margin` studs across (not up or down).
    pub fn blocks(&self, at: Vec2, ground: f32, margin: f32) -> bool {
        if at.x < self.min.x - margin
            || at.y < self.min.y - margin
            || at.x > self.max.x + margin
            || at.y > self.max.y + margin
        {
            return false;
        }
        // The column in the box's own space, against the box grown
        // sideways by the margin (a slab test).
        let inverse = self.rotation.inverse();
        let from = inverse * (Vec3::new(at.x, ground - BELOW, at.y) - self.center);
        let along = inverse * Vec3::new(0.0, CLEARANCE + BELOW, 0.0);
        let half = self.half + Vec3::new(margin, 0.0, margin);
        let (mut low, mut high) = (0.0_f32, 1.0_f32);
        for axis in 0..3 {
            let (start, step, size) = (from[axis], along[axis], half[axis]);
            if step.abs() < 1e-6 {
                if start.abs() > size {
                    return false;
                }
                continue;
            }
            let (a, b) = ((-size - start) / step, (size - start) / step);
            low = low.max(a.min(b));
            high = high.min(a.max(b));
            if low > high {
                return false;
            }
        }
        true
    }

    /// How far world `at` (X, Z) is from the box seen from above: 0 under
    /// it. Measured along the box's own sides as they lie seen from above
    /// (exact for an upright or turned box, near enough for a tilted one).
    pub fn distance_from_above(&self, at: Vec2) -> f32 {
        let offset = at - Vec2::new(self.center.x, self.center.z);
        let mut outside = Vec2::ZERO;
        // The box's two sides that lie most flat, as seen from above.
        let mut axes = [
            (self.rotation * Vec3::X, self.half.x),
            (self.rotation * Vec3::Y, self.half.y),
            (self.rotation * Vec3::Z, self.half.z),
        ];
        axes.sort_by(|a, b| a.0.y.abs().total_cmp(&b.0.y.abs()));
        for (index, (axis, half)) in axes.iter().take(2).enumerate() {
            let flat = Vec2::new(axis.x, axis.z);
            let length = flat.length();
            if length < 1e-6 {
                continue;
            }
            // How far the box reaches along this side seen from above,
            // with the third side's slant added.
            let (third, third_half) = (axes[2].0, axes[2].1);
            let reach =
                half * length + third_half * Vec2::new(third.x, third.z).dot(flat / length).abs();
            outside[index] = (offset.dot(flat / length).abs() - reach).max(0.0);
        }
        outside.length()
    }

    /// Its bottom (world Y): the lowest of its corners.
    pub fn bottom(&self) -> f32 {
        let reach = (self.rotation * Vec3::X * self.half.x).y.abs()
            + (self.rotation * Vec3::Y * self.half.y).y.abs()
            + (self.rotation * Vec3::Z * self.half.z).y.abs();
        self.center.y - reach
    }

    /// Whether its box seen from above, grown by `margin`, overlaps the
    /// box from `min` to `max`.
    pub fn overlaps(&self, min: Vec2, max: Vec2, margin: f32) -> bool {
        self.min.x - margin <= max.x
            && self.min.y - margin <= max.y
            && self.max.x + margin >= min.x
            && self.max.y + margin >= min.y
    }
}

/// The Workspace's anchored, colliding parts, kept between frames.
#[derive(Default)]
pub struct Obstacles {
    /// The world and Workspace structure the part list was read for.
    read_for: Option<(u64, u64)>,
    parts: Vec<InstanceId>,
    /// Per part: the revision its box was read at, and the box.
    boxes: HashMap<InstanceId, (u64, Option<Obstacle>)>,
    list: Vec<Obstacle>,
}

impl Obstacles {
    /// The obstacles in `world`, in tree order, read again where parts
    /// changed.
    pub fn update(&mut self, world: &World) -> &[Obstacle] {
        let workspace = world.workspace();
        let key = (world.identity(), world.structure_version_of(workspace));
        let mut changed = false;
        if self.read_for != Some(key) {
            // Another world's (a copy's) parts keep their revisions but
            // may differ: read them all again.
            if self.read_for.map(|(identity, _)| identity) != Some(key.0) {
                self.boxes.clear();
            }
            self.read_for = Some(key);
            self.parts = world.all_below(workspace, |class| class.is_base_part());
            self.parts
                .retain(|&id| !axcenity_core::csg::out_of_world(world, id));
            let parts: HashSet<InstanceId> = self.parts.iter().copied().collect();
            self.boxes.retain(|id, _| parts.contains(id));
            changed = true;
        }
        for &id in &self.parts {
            let Some(instance) = world.get(id) else {
                continue;
            };
            let revision = instance.revision();
            match self.boxes.get(&id) {
                Some((seen, _)) if *seen == revision => {}
                _ => {
                    self.boxes.insert(id, (revision, Obstacle::of(instance)));
                    changed = true;
                }
            }
        }
        if changed {
            self.list = self
                .parts
                .iter()
                .filter_map(|id| self.boxes.get(id).and_then(|(_, b)| b.clone()))
                .collect();
        }
        &self.list
    }
}
