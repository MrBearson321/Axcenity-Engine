//! Projections (Phase 58): uvs straight from positions, as if a texture
//! were thrown at the faces from round a box, a tube, a ball or the view,
//! and Smart UV Project's grouping of faces by the way they face.

use std::f32::consts::{PI, TAU};

use glam::{Mat4, Vec2, Vec3};

use super::lscm::plane_axes;
use super::{Chart, charts, flip, on_screen};
use crate::half_edge::EditMesh;

/// How [`project`](super::project) throws the texture at the faces.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Projection {
    /// Each face from the side of the box round the faces it faces most,
    /// the box's largest side filling the texture.
    Cube,
    /// Round the up axis through the middle of the faces: u once round,
    /// v from the top to the bottom.
    Cylinder,
    /// Round the middle of the faces: u once round, v from the top pole to
    /// the bottom one.
    Sphere,
    /// As the view sees them, through this view-projection matrix (model
    /// space to clip space), fitted into the texture keeping their shape.
    View(Mat4),
}

/// Sets the uvs of `faces` in `channel` by `projection`.
pub(crate) fn project(mesh: &mut EditMesh, faces: &[u32], projection: Projection, channel: usize) {
    let (min, max) = faces
        .iter()
        .flat_map(|&face| mesh.face_vertices(face))
        .map(|v| mesh.position(v))
        .fold((Vec3::INFINITY, Vec3::NEG_INFINITY), |(min, max), p| {
            (min.min(p), max.max(p))
        });
    let middle = (min + max) / 2.0;
    let size = (max - min).max(Vec3::splat(1e-6));
    match projection {
        Projection::Cube => {
            let side = size.max_element();
            for &face in faces {
                let (u, v) = plane_axes(dominant_axis(mesh.face_normal(face)));
                for corner in mesh.face_half_edges(face) {
                    let p = mesh.position(mesh.half_edge(corner).vertex) - middle;
                    let uv = Vec2::new(p.dot(u), p.dot(v)) / side;
                    mesh.set_uv(corner, channel, flip(uv) + Vec2::splat(0.5));
                }
            }
        }
        Projection::Cylinder | Projection::Sphere => {
            let radius = size.max_element() / 2.0;
            for &face in faces {
                let corners = mesh.face_half_edges(face);
                let center = mesh.face_center(face) - middle;
                let mut uvs: Vec<Vec2> = corners
                    .iter()
                    .map(|&corner| {
                        let p = mesh.position(mesh.half_edge(corner).vertex) - middle;
                        // At a pole (on the axis) the way round is the face's.
                        let round = if Vec2::new(p.x, p.z).length() > 1e-5 * radius {
                            p
                        } else {
                            center
                        };
                        let u = 0.5 + round.x.atan2(round.z) / TAU;
                        let v = match projection {
                            Projection::Cylinder => 0.5 - p.y / size.y,
                            _ => {
                                let up = p.y / p.length().max(1e-9);
                                up.clamp(-1.0, 1.0).acos() / PI
                            }
                        };
                        Vec2::new(u, v)
                    })
                    .collect();
                // A face across the back, where u comes round from 1 to 0,
                // takes its small us round past 1.
                let (lo, hi) = uvs
                    .iter()
                    .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), uv| {
                        (lo.min(uv.x), hi.max(uv.x))
                    });
                if hi - lo > 0.5 {
                    for uv in &mut uvs {
                        if uv.x < 0.5 {
                            uv.x += 1.0;
                        }
                    }
                }
                for (corner, uv) in corners.into_iter().zip(uvs) {
                    mesh.set_uv(corner, channel, uv);
                }
            }
        }
        Projection::View(view) => {
            let points: Vec<(u32, Vec2)> = faces
                .iter()
                .flat_map(|&face| mesh.face_half_edges(face))
                .map(|corner| {
                    let p = mesh.position(mesh.half_edge(corner).vertex);
                    (corner, on_screen(view, p))
                })
                .collect();
            let (lo, hi) = points
                .iter()
                .fold((Vec2::INFINITY, Vec2::NEG_INFINITY), |(lo, hi), &(_, p)| {
                    (lo.min(p), hi.max(p))
                });
            let side = (hi - lo).max_element().max(1e-9);
            let middle = (lo + hi) / 2.0;
            for (corner, p) in points {
                let uv = (p - middle) / side;
                mesh.set_uv(corner, channel, flip(uv) + Vec2::splat(0.5));
            }
        }
    }
}

/// The axis (with its sign) `normal` is nearest.
fn dominant_axis(normal: Vec3) -> Vec3 {
    let a = normal.abs();
    if a.x >= a.y && a.x >= a.z {
        Vec3::X * normal.x.signum()
    } else if a.y >= a.z {
        Vec3::Y * normal.y.signum()
    } else {
        Vec3::Z * normal.z.signum()
    }
}

/// Smart UV Project's charts: `faces` given ways to face, the biggest
/// faces first, a new way for each face more than `angle` (radians) from
/// all so far; each way moved twice to the area-weighted middle of the
/// faces nearest it; then each face's way is the nearest, and faces with
/// the same way joined across edges (but not seams) make a chart.
pub(crate) fn smart_charts(mesh: &EditMesh, faces: &[u32], angle: f32) -> Vec<(Chart, Vec3)> {
    let mut faces: Vec<u32> = faces
        .iter()
        .copied()
        .filter(|&face| (face as usize) < mesh.face_count())
        .collect();
    faces.sort_unstable();
    faces.dedup();
    let area = |face: u32| {
        mesh.triangles(face)
            .into_iter()
            .map(|t| {
                let [a, b, c] = t.map(|e| mesh.position(mesh.half_edge(e).vertex));
                (b - a).cross(c - a).length() / 2.0
            })
            .sum::<f32>()
    };
    let areas: Vec<f32> = faces.iter().map(|&f| area(f)).collect();
    let normals: Vec<Vec3> = faces.iter().map(|&f| mesh.face_normal(f)).collect();
    let mut order: Vec<usize> = (0..faces.len()).collect();
    order.sort_by(|&a, &b| areas[b].total_cmp(&areas[a]).then(a.cmp(&b)));
    let limit = angle.cos();
    let mut ways: Vec<Vec3> = Vec::new();
    for &k in &order {
        let normal = normals[k];
        if normal != Vec3::ZERO && ways.iter().all(|way| way.dot(normal) < limit) {
            ways.push(normal);
        }
    }
    if ways.is_empty() {
        ways.push(Vec3::Y);
    }
    let nearest = |ways: &[Vec3], normal: Vec3| {
        (0..ways.len())
            .max_by(|&a, &b| {
                ways[a]
                    .dot(normal)
                    .total_cmp(&ways[b].dot(normal))
                    .then(b.cmp(&a))
            })
            .unwrap_or(0)
    };
    for _ in 0..2 {
        let mut sums = vec![Vec3::ZERO; ways.len()];
        for k in 0..faces.len() {
            sums[nearest(&ways, normals[k])] += normals[k] * areas[k];
        }
        for (way, sum) in ways.iter_mut().zip(sums) {
            if let Some(sum) = sum.try_normalize() {
                *way = sum;
            }
        }
    }
    let mut way_of = vec![usize::MAX; mesh.face_count()];
    for (k, &face) in faces.iter().enumerate() {
        way_of[face as usize] = nearest(&ways, normals[k]);
    }
    charts(mesh, &faces, |edge, twin| {
        let (a, b) = mesh.ends(edge);
        way_of[mesh.half_edge(edge).face as usize] == way_of[mesh.half_edge(twin).face as usize]
            && !mesh.is_seam(a, b)
    })
    .into_iter()
    .map(|chart| {
        let way = ways[way_of[chart.faces[0] as usize]];
        (chart, way)
    })
    .collect()
}
