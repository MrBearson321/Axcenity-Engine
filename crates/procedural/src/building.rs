//! What a BuildingGenerator makes (procedural worlds PW6): a building from
//! a footprint, as a list of [`Block`]s that `lib.rs` turns into parts.
//!
//! The footprint is the generator's own box standing free (Position,
//! Size, its turn round Y), or the corners of the Spline it's in. On a
//! foundation it stacks `Floors` storeys: each side of the footprint is
//! cut into a whole number of equal bays about `BayWidth` wide between
//! corner posts, each bay a window or plain wall (picked by `Seed`, one
//! random number per bay, so the same seed gives the same building), with
//! a doorway in the middle of the first side on the ground floor. A floor
//! slab tops each storey, and a flat roof or (over a rectangle) a gable
//! of two wedges tops the building.
//!
//! A slab over a footprint that isn't a rectangle is cut into triangles
//! (ear clipping), each made of two wedge parts: a wedge's triangular
//! sides are a right triangle, and any triangle is two right triangles
//! split at the foot of its tallest height.

use axcenity_core::{ClassName, Color3, InstanceId, Material, Value, World};
use glam::{Mat3, Quat, Vec2, Vec3};

use crate::rng::Rng;
use crate::{rotation_from_orientation, v3};

/// Parts one building makes, at most: past this the rest are left out.
pub const MAX_BLOCKS: usize = 6000;
/// Storeys, at most.
pub const MAX_FLOORS: usize = 50;
/// Studs a gable roof reaches past the walls.
const OVERHANG: f32 = 1.0;
/// Studs thick: floor slabs and a flat roof.
const SLAB: f32 = 1.0;

/// A roof's shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Roof {
    Flat,
    Gable,
}

/// A BuildingGenerator's settings.
#[derive(Clone, Debug, PartialEq)]
pub struct BuildingSettings {
    pub seed: u64,
    pub floors: usize,
    pub floor_height: f32,
    pub roof: Roof,
    pub roof_height: f32,
    pub bay_width: f32,
    pub window_chance: f32,
    pub wall_thickness: f32,
    /// Model files for a plain bay, a window, the door and a corner.
    pub wall_model: Option<String>,
    pub window_model: Option<String>,
    pub door_model: Option<String>,
    pub corner_model: Option<String>,
    pub wall_material: Material,
    pub wall_color: Color3,
    pub trim_color: Color3,
    pub roof_color: Color3,
}

impl BuildingSettings {
    /// The settings of the BuildingGenerator `instance`, kept in range.
    pub fn of(instance: &axcenity_core::Instance) -> BuildingSettings {
        let number = |name, default: f64| {
            instance
                .property(name)
                .and_then(Value::as_number)
                .filter(|n| n.is_finite())
                .unwrap_or(default) as f32
        };
        let model = |name| {
            instance
                .property(name)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|id| !id.is_empty())
                .map(str::to_owned)
        };
        let color = |name, default| match instance.property(name) {
            Some(Value::Color3(c)) => *c,
            _ => default,
        };
        let roof = match instance.property("RoofStyle") {
            Some(Value::EnumItem(item)) if item.item == "Flat" => Roof::Flat,
            _ => Roof::Gable,
        };
        BuildingSettings {
            seed: number("Seed", 1.0) as i64 as u64,
            floors: (number("Floors", 2.0).round() as usize).clamp(1, MAX_FLOORS),
            floor_height: number("FloorHeight", 10.0).clamp(4.0, 100.0),
            roof,
            roof_height: number("RoofHeight", 6.0).clamp(0.5, 100.0),
            bay_width: number("BayWidth", 8.0).clamp(2.0, 1000.0),
            window_chance: number("WindowChance", 0.7).clamp(0.0, 1.0),
            wall_thickness: number("WallThickness", 1.0).clamp(0.2, 10.0),
            wall_model: model("WallModel"),
            window_model: model("WindowModel"),
            door_model: model("DoorModel"),
            corner_model: model("CornerModel"),
            wall_material: match instance.property("WallMaterial") {
                Some(Value::Material(material)) => *material,
                _ => Material::Brick,
            },
            wall_color: color("WallColor", Color3::new(0.86, 0.78, 0.66)),
            trim_color: color("TrimColor", Color3::new(0.55, 0.52, 0.48)),
            roof_color: color("RoofColor", Color3::new(0.45, 0.24, 0.2)),
        }
    }

    /// The module model files it uses, in a set order.
    pub fn models(&self) -> Vec<String> {
        [
            &self.wall_model,
            &self.window_model,
            &self.door_model,
            &self.corner_model,
        ]
        .into_iter()
        .flatten()
        .cloned()
        .collect()
    }
}

/// Where a building stands: its corners seen from above (world X and Z,
/// in order round it), the top of its foundation and how thick that is.
#[derive(Clone, Debug, PartialEq)]
pub struct Footprint {
    pub corners: Vec<Vec2>,
    pub base: f32,
    pub foundation: f32,
}

impl Footprint {
    /// The footprint of the BuildingGenerator `generator`: the points of
    /// the Spline it's in (at least 3), or its own box.
    pub fn of(world: &World, generator: InstanceId) -> Option<Footprint> {
        let instance = world.get(generator)?;
        let vector = |name| {
            instance
                .property(name)
                .and_then(Value::as_vector3)
                .map(v3)
                .filter(|v| v.is_finite())
        };
        let size = vector("Size")?.abs();
        let foundation = size.y.max(0.0);
        let spline = instance
            .parent()
            .and_then(|parent| world.get(parent))
            .filter(|parent| parent.class() == ClassName::Spline);
        if let Some(spline) = spline {
            let points: Vec<Vec3> = spline
                .children()
                .iter()
                .filter_map(|&child| world.get(child))
                .filter(|child| child.class() == ClassName::SplinePoint)
                .filter_map(|point| point.property("Position")?.as_vector3().map(v3))
                .filter(|p| p.is_finite())
                .collect();
            let mut corners: Vec<Vec2> = Vec::new();
            for p in &points {
                let corner = Vec2::new(p.x, p.z);
                if corners
                    .last()
                    .is_none_or(|last| last.distance(corner) > 0.01)
                {
                    corners.push(corner);
                }
            }
            if corners.len() > 1 && corners[0].distance(corners[corners.len() - 1]) <= 0.01 {
                corners.pop();
            }
            if corners.len() < 3 || area(&corners).abs() < 1.0 {
                return None;
            }
            let lowest = points.iter().map(|p| p.y).fold(f32::INFINITY, f32::min);
            return Some(Footprint {
                corners,
                base: lowest + foundation,
                foundation,
            });
        }
        let center = vector("Position")?;
        let turn = vector("Orientation").map_or(0.0, |o| o.y);
        let rotation = rotation_from_orientation(Vec3::new(0.0, turn, 0.0));
        let (w, d) = (size.x / 2.0, size.z / 2.0);
        if w < 0.5 || d < 0.5 {
            return None;
        }
        // The front side (-Z, where the door goes) first.
        let corners = [(-w, -d), (w, -d), (w, d), (-w, d)]
            .into_iter()
            .map(|(x, z)| {
                let p = center + rotation * Vec3::new(x, 0.0, z);
                Vec2::new(p.x, p.z)
            })
            .collect();
        Some(Footprint {
            corners,
            base: center.y + size.y / 2.0,
            foundation,
        })
    }

    /// Whether its corners make a rectangle.
    pub fn is_rectangle(&self) -> bool {
        let c = &self.corners;
        c.len() == 4
            && (0..4).all(|i| {
                let a = (c[(i + 1) % 4] - c[i]).normalize_or_zero();
                let b = (c[(i + 2) % 4] - c[(i + 1) % 4]).normalize_or_zero();
                a.dot(b).abs() < 0.01
            })
    }
}

/// What a block is drawn as.
#[derive(Clone, Debug, PartialEq)]
pub enum Look {
    /// A built-in part: a block, or a wedge.
    Part {
        wedge: bool,
        material: Material,
        color: Color3,
        transparency: f32,
    },
    /// A module model, stretched to the block.
    Model(String),
}

/// One part of a building: its name, box and look.
#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    pub name: &'static str,
    pub center: Vec3,
    pub rotation: Quat,
    pub size: Vec3,
    pub look: Look,
}

/// The building `settings` make on `footprint`.
pub fn plan(footprint: &Footprint, settings: &BuildingSettings) -> Vec<Block> {
    let mut out = Plan {
        blocks: Vec::new(),
        settings,
    };
    let corners = &footprint.corners;
    let count = corners.len();
    // Which way round the corners go: outward is to the right of each
    // side going round anticlockwise seen from above (with Z down).
    let turn = area(corners).signum();
    let outward = |i: usize| {
        let u = (corners[(i + 1) % count] - corners[i]).normalize_or_zero();
        Vec2::new(u.y, -u.x) * turn
    };
    let s = settings;
    let (height, thick) = (s.floor_height, s.wall_thickness);
    let rectangle = footprint.is_rectangle();
    let trim = out.part(Material::Concrete, s.trim_color);
    // The foundation.
    if footprint.foundation > 0.01 {
        out.slab(
            "Foundation",
            corners,
            footprint.base - footprint.foundation,
            footprint.foundation,
            0.0,
            rectangle,
            &trim,
        );
    }
    let roof_look = out.part(Material::Slate, s.roof_color);
    for floor in 0..s.floors {
        let y = footprint.base + floor as f32 * height;
        // Corner posts.
        for (i, corner) in corners.iter().enumerate() {
            let (prev, next) = (outward((i + count - 1) % count), outward(i));
            let at = *corner - (prev + next) * thick / 2.0;
            let look = match &s.corner_model {
                Some(model) => Look::Model(model.clone()),
                None => trim.clone(),
            };
            out.push(Block {
                name: "Corner",
                center: Vec3::new(at.x, y + height / 2.0, at.y),
                rotation: facing(next),
                size: Vec3::new(thick, height, thick),
                look,
            });
        }
        // Each side's bays.
        for side in 0..count {
            let (from, to) = (corners[side], corners[(side + 1) % count]);
            let along = (to - from).normalize_or_zero();
            let out_dir = outward(side);
            let usable = from.distance(to) - 2.0 * thick;
            if usable < 0.5 {
                continue;
            }
            let bays = ((usable / s.bay_width).round() as usize).max(1);
            let width = usable / bays as f32;
            for bay in 0..bays {
                let middle =
                    from + along * (thick + width * (bay as f32 + 0.5)) - out_dir * thick / 2.0;
                let frame = Bay {
                    middle,
                    along,
                    rotation: facing(out_dir),
                    y,
                    width,
                };
                let door = floor == 0 && side == 0 && bay == bays / 2;
                let index = ((floor as u64) << 40) | ((side as u64) << 20) | bay as u64;
                let window = Rng::new(s.seed, index).unit() < s.window_chance;
                if door {
                    out.door(&frame);
                } else if window {
                    out.window(&frame);
                } else {
                    out.wall(&frame);
                }
            }
        }
        // The slab on top of it: a floor, or under the roof.
        out.slab(
            "Floor",
            corners,
            y + height - SLAB,
            SLAB,
            thick / 2.0,
            rectangle,
            &trim,
        );
    }
    let top = footprint.base + s.floors as f32 * height;
    match (s.roof, rectangle) {
        (Roof::Gable, true) => out.gable(corners, top, &roof_look),
        _ => out.slab(
            "Roof",
            corners,
            top,
            SLAB,
            -OVERHANG / 2.0,
            rectangle,
            &roof_look,
        ),
    }
    out.blocks.truncate(MAX_BLOCKS);
    out.blocks
}

/// One bay of wall: its middle (seen from above, in the wall), the way
/// along the wall, the turn that faces it out, its floor and its width.
struct Bay {
    middle: Vec2,
    along: Vec2,
    rotation: Quat,
    y: f32,
    width: f32,
}

impl Bay {
    /// The point `side` studs along the wall from the bay's middle and
    /// `up` studs above its floor.
    fn at(&self, side: f32, up: f32) -> Vec3 {
        let p = self.middle + self.along * side;
        Vec3::new(p.x, self.y + up, p.y)
    }
}

struct Plan<'a> {
    blocks: Vec<Block>,
    settings: &'a BuildingSettings,
}

impl Plan<'_> {
    fn push(&mut self, block: Block) {
        if self.blocks.len() < MAX_BLOCKS {
            self.blocks.push(block);
        }
    }

    fn part(&self, material: Material, color: Color3) -> Look {
        Look::Part {
            wedge: false,
            material,
            color,
            transparency: 0.0,
        }
    }

    fn wall_look(&self) -> Look {
        self.part(self.settings.wall_material, self.settings.wall_color)
    }

    /// A box `size` (along the wall, up, through it) with its middle at
    /// `side` along the bay and `up` above its floor.
    fn piece(&mut self, name: &'static str, bay: &Bay, side: f32, up: f32, size: Vec3, look: Look) {
        if size.min_element() <= 0.01 {
            return;
        }
        self.push(Block {
            name,
            center: bay.at(side, up),
            rotation: bay.rotation,
            size,
            look,
        });
    }

    fn whole(&mut self, name: &'static str, bay: &Bay, model: &str) {
        let s = self.settings;
        self.piece(
            name,
            bay,
            0.0,
            s.floor_height / 2.0,
            Vec3::new(bay.width, s.floor_height, s.wall_thickness),
            Look::Model(model.to_owned()),
        );
    }

    fn wall(&mut self, bay: &Bay) {
        let s = self.settings;
        if let Some(model) = &s.wall_model {
            return self.whole("Wall", bay, model);
        }
        let look = self.wall_look();
        self.piece(
            "Wall",
            bay,
            0.0,
            s.floor_height / 2.0,
            Vec3::new(bay.width, s.floor_height, s.wall_thickness),
            look,
        );
    }

    /// A wall with an opening `wide` by `tall` from `sill` above the floor.
    fn opening(&mut self, name: &'static str, bay: &Bay, wide: f32, sill: f32, tall: f32) {
        let s = self.settings;
        let (height, thick) = (s.floor_height, s.wall_thickness);
        let look = self.wall_look();
        let head = height - sill - tall;
        self.piece(
            name,
            bay,
            0.0,
            sill / 2.0,
            Vec3::new(bay.width, sill, thick),
            look.clone(),
        );
        self.piece(
            name,
            bay,
            0.0,
            height - head / 2.0,
            Vec3::new(bay.width, head, thick),
            look.clone(),
        );
        let pier = (bay.width - wide) / 2.0;
        for side in [-1.0, 1.0] {
            self.piece(
                name,
                bay,
                side * (wide + pier) / 2.0,
                sill + tall / 2.0,
                Vec3::new(pier, tall, thick),
                look.clone(),
            );
        }
    }

    fn window(&mut self, bay: &Bay) {
        let s = self.settings;
        if let Some(model) = &s.window_model {
            return self.whole("Window", bay, model);
        }
        let height = s.floor_height;
        let (wide, sill, tall) = (
            (bay.width * 0.5).clamp(0.5, (bay.width - 0.5).max(0.5)),
            height * 0.3,
            height * 0.4,
        );
        self.opening("Window", bay, wide, sill, tall);
        self.piece(
            "Glass",
            bay,
            0.0,
            sill + tall / 2.0,
            Vec3::new(wide, tall, 0.2),
            Look::Part {
                wedge: false,
                material: Material::Glass,
                color: Color3::new(0.62, 0.76, 0.86),
                transparency: 0.5,
            },
        );
    }

    fn door(&mut self, bay: &Bay) {
        let s = self.settings;
        if let Some(model) = &s.door_model {
            return self.whole("Door", bay, model);
        }
        let wide = (bay.width * 0.5).min(5.0);
        let tall = (s.floor_height * 0.75).min(8.0);
        self.opening("Door", bay, wide, 0.0, tall);
    }

    /// A slab from `bottom` up `thickness` studs over `corners`, brought
    /// in by `inset` studs (out, when negative).
    #[allow(clippy::too_many_arguments)]
    fn slab(
        &mut self,
        name: &'static str,
        corners: &[Vec2],
        bottom: f32,
        thickness: f32,
        inset: f32,
        rectangle: bool,
        look: &Look,
    ) {
        let corners = shrink(corners, inset);
        let middle = bottom + thickness / 2.0;
        if rectangle {
            let (a, b) = (corners[1] - corners[0], corners[2] - corners[1]);
            let center = corners.iter().copied().sum::<Vec2>() / 4.0;
            self.push(Block {
                name,
                center: Vec3::new(center.x, middle, center.y),
                rotation: Quat::from_rotation_y((-a.y).atan2(a.x)),
                size: Vec3::new(a.length(), thickness, b.length()),
                look: look.clone(),
            });
            return;
        }
        for [a, b, c] in triangulate(&corners) {
            let lift = |p: Vec2| Vec3::new(p.x, middle, p.y);
            for (center, rotation, size) in triangle_wedges(lift(a), lift(b), lift(c), thickness) {
                let look = match look {
                    Look::Part {
                        material,
                        color,
                        transparency,
                        ..
                    } => Look::Part {
                        wedge: true,
                        material: *material,
                        color: *color,
                        transparency: *transparency,
                    },
                    other => other.clone(),
                };
                self.push(Block {
                    name,
                    center,
                    rotation,
                    size,
                    look,
                });
            }
        }
    }

    /// A gable roof over the rectangle `corners` from `bottom`: two wedges
    /// back to back, the ridge along the longer side.
    fn gable(&mut self, corners: &[Vec2], bottom: f32, look: &Look) {
        let s = self.settings;
        let (a, b) = (corners[1] - corners[0], corners[2] - corners[1]);
        let (along, across) = if a.length() >= b.length() {
            (a, b)
        } else {
            (b, a)
        };
        let center = corners.iter().copied().sum::<Vec2>() / 4.0;
        let across_dir = Vec3::new(across.x, 0.0, across.y).normalize();
        let half = across.length() / 2.0 + OVERHANG;
        let length = along.length() + 2.0 * OVERHANG;
        let height = s.roof_height;
        let middle = Vec3::new(center.x, bottom + height / 2.0, center.y);
        for side in [-1.0, 1.0] {
            // Its back (+Z) stands at the ridge, so +Z points inward.
            let z = -across_dir * side;
            let x = Vec3::Y.cross(z);
            let rotation = Quat::from_mat3(&Mat3::from_cols(x, Vec3::Y, z));
            let wedge = match look {
                Look::Part {
                    material,
                    color,
                    transparency,
                    ..
                } => Look::Part {
                    wedge: true,
                    material: *material,
                    color: *color,
                    transparency: *transparency,
                },
                other => other.clone(),
            };
            self.push(Block {
                name: "Roof",
                center: middle + across_dir * side * half / 2.0,
                rotation,
                size: Vec3::new(length, height, half),
                look: wedge,
            });
        }
    }
}

/// The upright turn whose front (-Z) faces `out` (seen from above).
fn facing(out: Vec2) -> Quat {
    Quat::from_rotation_y((-out.x).atan2(-out.y))
}

/// Twice the signed area of the polygon `corners` (X, Z): positive when
/// they go anticlockwise seen from above with Z pointing down the screen.
fn area(corners: &[Vec2]) -> f32 {
    let n = corners.len();
    (0..n)
        .map(|i| {
            let (a, b) = (corners[i], corners[(i + 1) % n]);
            a.x * b.y - b.x * a.y
        })
        .sum()
}

/// `corners` moved `by` studs inward (outward when negative), each corner
/// along its bisector, mitred and limited to 4 times `by`.
fn shrink(corners: &[Vec2], by: f32) -> Vec<Vec2> {
    if by.abs() < 1e-6 {
        return corners.to_vec();
    }
    let n = corners.len();
    let turn = area(corners).signum();
    let outward = |i: usize| {
        let u = (corners[(i + 1) % n] - corners[i]).normalize_or_zero();
        Vec2::new(u.y, -u.x) * turn
    };
    (0..n)
        .map(|i| {
            let (a, b) = (outward((i + n - 1) % n), outward(i));
            let bisector = (a + b).normalize_or(b);
            let cos = bisector.dot(b).max(0.25);
            corners[i] - bisector * by / cos
        })
        .collect()
}

/// The polygon `corners` cut into triangles (ear clipping). A polygon
/// that crosses itself gives what it can.
fn triangulate(corners: &[Vec2]) -> Vec<[Vec2; 3]> {
    let turn = area(corners).signum();
    let mut left: Vec<Vec2> = corners.to_vec();
    let mut out = Vec::new();
    let cross = |a: Vec2, b: Vec2, c: Vec2| (b - a).perp_dot(c - a) * turn;
    while left.len() > 3 {
        let n = left.len();
        let ear = (0..n).find(|&i| {
            let (a, b, c) = (left[(i + n - 1) % n], left[i], left[(i + 1) % n]);
            cross(a, b, c) > 1e-6
                && left.iter().all(|&p| {
                    p == a
                        || p == b
                        || p == c
                        || !(cross(a, b, p) >= 0.0
                            && cross(b, c, p) >= 0.0
                            && cross(c, a, p) >= 0.0)
                })
        });
        let Some(i) = ear else {
            break;
        };
        out.push([left[(i + n - 1) % n], left[i], left[(i + 1) % n]]);
        left.remove(i);
    }
    if left.len() == 3 {
        out.push([left[0], left[1], left[2]]);
    }
    out
}

/// Two wedges filling the flat triangle `a`, `b`, `c` (at their height)
/// `thickness` studs thick: split at the foot of the height onto its
/// longest side, each half is a right triangle, a wedge's side.
fn triangle_wedges(a: Vec3, b: Vec3, c: Vec3, thickness: f32) -> Vec<(Vec3, Quat, Vec3)> {
    // Make `a`-`b` the longest side.
    let (a, b, c) = [(a, b, c), (b, c, a), (c, a, b)]
        .into_iter()
        .max_by(|x, y| x.0.distance(x.1).total_cmp(&y.0.distance(y.1)))
        .unwrap_or((a, b, c));
    let side = b - a;
    if side.length_squared() < 1e-6 {
        return Vec::new();
    }
    let foot = a + side * ((c - a).dot(side) / side.length_squared());
    let tall = c - foot;
    if tall.length() < 1e-3 {
        return Vec::new();
    }
    let mut out = Vec::new();
    for end in [a, b] {
        let base = end - foot;
        if base.length() < 1e-3 {
            continue;
        }
        // The wedge's back (+Z) edge stands at the foot, its top back
        // edge runs through `c` and its bottom front edge through `end`;
        // its width (X) is the slab's thickness.
        let z = -base.normalize();
        let y = tall.normalize();
        let x = y.cross(z);
        out.push((
            foot + tall / 2.0 + base / 2.0,
            Quat::from_mat3(&Mat3::from_cols(x, y, z)),
            Vec3::new(thickness, tall.length(), base.length()),
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_triangle_is_two_wedges_covering_its_area() {
        let (a, b, c) = (
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(10.0, 0.0, 0.0),
            Vec3::new(3.0, 0.0, 4.0),
        );
        let wedges = triangle_wedges(a, b, c, 1.0);
        assert_eq!(wedges.len(), 2);
        let covered: f32 = wedges.iter().map(|(_, _, s)| s.y * s.z / 2.0).sum();
        assert!((covered - 20.0).abs() < 1e-3, "{covered}");
        for (center, rotation, size) in wedges {
            // The wedge's corners: its right angle at the foot (3, 0, 0),
            // the top of its back at `c`.
            let corner = |p: Vec3| center + rotation * (p * size / 2.0);
            assert!(corner(Vec3::new(0.0, -1.0, 1.0)).distance(Vec3::new(3.0, 0.0, 0.0)) < 1e-3);
            assert!(corner(Vec3::new(0.0, 1.0, 1.0)).distance(c) < 1e-3);
            // Its thickness stands up.
            assert!((rotation * Vec3::X).y.abs() > 0.999);
        }
    }

    #[test]
    fn an_l_shape_is_cut_into_four_triangles_of_its_area() {
        let l = [
            Vec2::new(0.0, 0.0),
            Vec2::new(20.0, 0.0),
            Vec2::new(20.0, 10.0),
            Vec2::new(10.0, 10.0),
            Vec2::new(10.0, 20.0),
            Vec2::new(0.0, 20.0),
        ];
        for corners in [l.to_vec(), l.iter().rev().copied().collect()] {
            let triangles = triangulate(&corners);
            assert_eq!(triangles.len(), 4);
            let total: f32 = triangles
                .iter()
                .map(|[a, b, c]| (*b - *a).perp_dot(*c - *a).abs() / 2.0)
                .sum();
            assert!((total - 300.0).abs() < 1e-3, "{total}");
        }
    }
}
