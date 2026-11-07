//! A SplineMesh's pieces: its model bent along the curve, repeated to fill
//! it.
//!
//! The model's length runs along its Z: its back (+Z) is where a piece
//! starts and its front (-Z) where it ends, as a part faces -Z. Its width
//! (X) goes to the curve's right and its height (Y) up from the curve, so
//! the curve runs along the middle of the bottom of each piece. A whole
//! number of pieces fills the curve, each stretched a little to fit, and
//! where two meet both take the curve's frame at the same distance, so
//! there's no gap. Each piece is first cut across into slices about a stud
//! long, so a long straight face (a box's side) bends with the curve too.

use axcenity_assets::{Model, Primitive};
use axcenity_core::spline::SplineCurve;
use glam::{Vec2, Vec3};

use crate::v3;

/// Pieces along one curve, at most.
pub const MAX_PIECES: usize = 1000;
/// Cuts across one piece, at most.
const MAX_SLICES: usize = 64;
/// Studs of curve in a slice, at most (fewer when there'd be too many).
const SLICE_LENGTH: f32 = 1.0;

/// One bent piece: its model, fitted as a loaded one is, and where the
/// middle of its box is in the world. Its MeshPart is unturned, with the
/// model's `size` as its Size.
pub struct Piece {
    pub model: Model,
    pub center: Vec3,
}

/// The curve's frame `distance` along it: the point, then its right, up
/// and forward directions. Upright like `GetCFrameAt`: never rolled.
pub fn frame(curve: &SplineCurve, distance: f32) -> (Vec3, Vec3, Vec3, Vec3) {
    let point = v3(curve.position_at_distance(distance));
    let forward = v3(curve.direction_at_distance(distance));
    let right = forward.cross(Vec3::Y).try_normalize().unwrap_or(Vec3::X);
    let up = right.cross(forward).normalize_or(Vec3::Y);
    (point, right, up, forward)
}

/// How many pieces of `length` studs fill `curve` (at least one).
pub fn piece_count(curve: &SplineCurve, length: f32) -> usize {
    let length = if length > 0.01 { length } else { 1.0 };
    ((curve.length() / length).round() as usize).clamp(1, MAX_PIECES)
}

/// A corner of a triangle being cut: where it is in the model's 1×1×1
/// cube, its normal there, and its texture coordinates.
#[derive(Clone, Copy)]
struct Corner {
    at: Vec3,
    normal: Vec3,
    uv: Vec2,
}

impl Corner {
    fn lerp(self, other: Corner, t: f32) -> Corner {
        Corner {
            at: self.at.lerp(other.at, t),
            normal: self.normal.lerp(other.normal, t),
            uv: self.uv.lerp(other.uv, t),
        }
    }
}

/// `source` bent along `curve` as pieces of `size` (width, height and
/// length in studs). None with no curve to follow.
pub fn bend(curve: &SplineCurve, source: &Model, size: Vec3) -> Vec<Piece> {
    let total = curve.length();
    if total <= 0.0 {
        return Vec::new();
    }
    let count = piece_count(curve, size.z);
    let length = total / count as f32;
    let slices = ((length / SLICE_LENGTH).ceil() as usize).clamp(1, MAX_SLICES);
    // The model's triangles, cut into slices once for every piece.
    let sliced: Vec<Vec<[Corner; 3]>> = source
        .primitives
        .iter()
        .map(|primitive| slice(primitive, slices))
        .collect();
    // Normals in the stretched piece: a stretch tips them the other way.
    let stretch = Vec3::new(size.x, size.y, length).max(Vec3::splat(1e-4));
    (0..count)
        .map(|i| {
            let start = i as f32 * length;
            let origin = frame(curve, start + length / 2.0).0;
            let primitives = source
                .primitives
                .iter()
                .zip(&sliced)
                .filter(|(_, triangles)| !triangles.is_empty())
                .map(|(primitive, triangles)| {
                    let mut positions = Vec::with_capacity(triangles.len() * 3);
                    let mut normals = Vec::with_capacity(triangles.len() * 3);
                    let mut uvs = Vec::new();
                    for corner in triangles.iter().flatten() {
                        let along = (0.5 - corner.at.z) * length;
                        let (point, right, up, forward) = frame(curve, start + along);
                        let x = corner.at.x * size.x;
                        let y = (corner.at.y + 0.5) * size.y;
                        positions.push((point + right * x + up * y - origin).to_array());
                        let n = (corner.normal / stretch).normalize_or_zero();
                        let normal = (right * n.x + up * n.y - forward * n.z).normalize_or_zero();
                        normals.push(normal.to_array());
                        if !primitive.uvs.is_empty() {
                            uvs.push(corner.uv.to_array());
                        }
                    }
                    let indices = (0..positions.len() as u32).collect();
                    Primitive::new(positions, normals, uvs, indices, primitive.material.clone())
                })
                .collect();
            let model = Model::from_primitives(primitives).ok()?;
            let center = origin + Vec3::from(model.center);
            Some(Piece { model, center })
        })
        .collect::<Option<Vec<_>>>()
        .unwrap_or_default()
}

/// `primitive`'s triangles, each cut where it crosses one of the planes
/// that split the model's length into `slices`.
fn slice(primitive: &Primitive, slices: usize) -> Vec<[Corner; 3]> {
    let corner = |k: u32| -> Option<Corner> {
        let k = k as usize;
        Some(Corner {
            at: Vec3::from(*primitive.positions.get(k)?),
            normal: primitive
                .normals
                .get(k)
                .map_or(Vec3::Y, |&normal| Vec3::from(normal)),
            uv: primitive
                .uvs
                .get(k)
                .map_or(Vec2::ZERO, |&uv| Vec2::from(uv)),
        })
    };
    let mut triangles: Vec<[Corner; 3]> = primitive
        .indices
        .as_chunks::<3>()
        .0
        .iter()
        .filter_map(|&[a, b, c]| Some([corner(a)?, corner(b)?, corner(c)?]))
        .collect();
    for k in 1..slices {
        let plane = 0.5 - k as f32 / slices as f32;
        let mut cut = Vec::with_capacity(triangles.len());
        for triangle in triangles {
            split(triangle, plane, &mut cut);
        }
        triangles = cut;
    }
    triangles
}

/// Adds `triangle` to `out`, cut in two along the plane Z = `plane` if it
/// crosses it. The pieces keep its winding.
fn split(triangle: [Corner; 3], plane: f32, out: &mut Vec<[Corner; 3]>) {
    const NEAR: f32 = 1e-6;
    let side = triangle.map(|corner| corner.at.z - plane);
    if side.iter().all(|&s| s >= -NEAR) || side.iter().all(|&s| s <= NEAR) {
        out.push(triangle);
        return;
    }
    let mut front: Vec<Corner> = Vec::with_capacity(4);
    let mut back: Vec<Corner> = Vec::with_capacity(4);
    for i in 0..3 {
        let j = (i + 1) % 3;
        let (a, sa, sb) = (triangle[i], side[i], side[j]);
        if sa >= 0.0 {
            front.push(a);
        }
        if sa <= 0.0 {
            back.push(a);
        }
        if (sa > 0.0 && sb < 0.0) || (sa < 0.0 && sb > 0.0) {
            let middle = a.lerp(triangle[j], sa / (sa - sb));
            front.push(middle);
            back.push(middle);
        }
    }
    for polygon in [front, back] {
        for k in 1..polygon.len().saturating_sub(1) {
            out.push([polygon[0], polygon[k], polygon[k + 1]]);
        }
    }
}
