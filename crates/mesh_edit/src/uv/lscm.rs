//! Least squares conformal maps (Lévy et al., 2002): a chart flattened so
//! each triangle keeps its angles as well as it can. For a triangle laid
//! flat in its own plane at complex points z₁, z₂, z₃, a map to uvs
//! U₁, U₂, U₃ (as complex numbers) keeps its shape exactly when
//! Σ (z₍ⱼ₊₂₎ − z₍ⱼ₊₁₎) Uⱼ = 0; the unwrap makes the sum of those, squared
//! and divided by the triangle's size, as small as it can, with two uv
//! vertices pinned so it can't shrink to a point. Its real and imaginary
//! parts are two rows each of a sparse least-squares problem
//! (`sparse.rs`).

use std::collections::BTreeSet;

use glam::{DVec2, Vec2, Vec3};

use super::sparse::{Rows, least_squares};
use super::{Chart, chart_positions};
use crate::half_edge::EditMesh;

/// `chart`'s uv vertices laid flat on a plane square to `direction` (or
/// to its faces' average normal), v up, with up in the model up the
/// texture where it can be.
pub(crate) fn plane_guess(mesh: &EditMesh, chart: &Chart, direction: Option<Vec3>) -> Vec<Vec2> {
    let normal = direction.unwrap_or_else(|| {
        let sum: Vec3 = chart
            .faces
            .iter()
            .map(|&face| face_area_normal(mesh, face))
            .sum();
        let largest = chart
            .faces
            .iter()
            .map(|&face| face_area_normal(mesh, face))
            .max_by(|a, b| a.length().total_cmp(&b.length()))
            .unwrap_or(Vec3::Y);
        sum.try_normalize()
            .or_else(|| largest.try_normalize())
            .unwrap_or(Vec3::Y)
    });
    let (u, v) = plane_axes(normal);
    chart_positions(mesh, chart)
        .into_iter()
        .map(|p| Vec2::new(p.dot(u), p.dot(v)))
        .collect()
}

/// Right and up on a plane facing `normal`, seen from its front: up is
/// the model's up where it can be (and back, -Z, for a plane facing up).
pub(crate) fn plane_axes(normal: Vec3) -> (Vec3, Vec3) {
    let right = Vec3::Y.cross(normal).try_normalize().unwrap_or(Vec3::X);
    (right, normal.cross(right).normalize_or(Vec3::Y))
}

/// `face`'s normal, as long as twice its area.
fn face_area_normal(mesh: &EditMesh, face: u32) -> Vec3 {
    mesh.triangles(face)
        .into_iter()
        .map(|t| {
            let [a, b, c] = t.map(|e| mesh.position(mesh.half_edge(e).vertex));
            (b - a).cross(c - a)
        })
        .sum()
}

/// `chart`'s triangles, each as its three uv vertices.
fn triangles(mesh: &EditMesh, chart: &Chart) -> Vec<[usize; 3]> {
    let mut vertex_of = vec![usize::MAX; mesh.half_edges().len()];
    for &(corner, vertex) in &chart.corners {
        vertex_of[corner as usize] = vertex;
    }
    chart
        .faces
        .iter()
        .flat_map(|&face| mesh.triangles(face))
        .map(|t| t.map(|e| vertex_of[e as usize]))
        .collect()
}

/// `chart` flattened: a uv (v up) for each of its uv vertices. `pinned`
/// uv vertices stay where they are; with fewer than two, the two farthest
/// apart across `guess` (where the solve starts) are held where it puts
/// them, and the result is moved onto the one pin there is. `None` if it
/// can't be solved.
pub(crate) fn flatten(
    mesh: &EditMesh,
    chart: &Chart,
    pinned: &[(usize, Vec2)],
    guess: &[Vec2],
) -> Option<Vec<Vec2>> {
    let positions = chart_positions(mesh, chart);
    let triangles = triangles(mesh, chart);
    let distinct: BTreeSet<[u32; 2]> = pinned
        .iter()
        .map(|(_, uv)| uv.to_array().map(f32::to_bits))
        .collect();
    let held: Vec<(usize, Vec2)> = if pinned.len() >= 2 && distinct.len() >= 2 {
        pinned.to_vec()
    } else {
        let (a, b) = farthest(guess)?;
        vec![(a, guess[a]), (b, guess[b])]
    };
    let mut column = vec![None; chart.vertices];
    let mut held_uv = vec![None; chart.vertices];
    for &(vertex, uv) in &held {
        held_uv[vertex] = Some(uv.as_dvec2());
    }
    let mut columns = 0;
    for (vertex, slot) in column.iter_mut().enumerate() {
        if held_uv[vertex].is_none() {
            *slot = Some(columns);
            columns += 2;
        }
    }
    let mut a = Rows::new(columns);
    let mut b = Vec::new();
    for triangle in &triangles {
        let [p0, p1, p2] = triangle.map(|v| positions[v].as_dvec3());
        let (e1, e2) = (p1 - p0, p2 - p0);
        let normal = e1.cross(e2);
        let double_area = normal.length();
        if double_area < 1e-14 || e1.length() < 1e-14 {
            continue;
        }
        // The triangle laid flat in its own plane, counter-clockwise.
        let x = e1 / e1.length();
        let y = normal.cross(x) / double_area;
        let z = [
            DVec2::ZERO,
            DVec2::new(e1.length(), 0.0),
            DVec2::new(e2.dot(x), e2.dot(y)),
        ];
        let weight = 1.0 / double_area.sqrt();
        let mut real = Vec::new();
        let mut imaginary = Vec::new();
        let (mut real_b, mut imaginary_b) = (0.0, 0.0);
        for j in 0..3 {
            let w = (z[(j + 2) % 3] - z[(j + 1) % 3]) * weight;
            let vertex = triangle[j];
            // (w.x + i w.y)(u + i v) = (w.x u − w.y v) + i (w.y u + w.x v)
            match (column[vertex], held_uv[vertex]) {
                (Some(c), _) => {
                    real.extend([(c, w.x), (c + 1, -w.y)]);
                    imaginary.extend([(c, w.y), (c + 1, w.x)]);
                }
                (None, Some(uv)) => {
                    real_b -= w.x * uv.x - w.y * uv.y;
                    imaginary_b -= w.y * uv.x + w.x * uv.y;
                }
                (None, None) => unreachable!("each vertex is free or held"),
            }
        }
        a.push(real);
        b.push(real_b);
        a.push(imaginary);
        b.push(imaginary_b);
    }
    if a.len() == 0 {
        return None;
    }
    let mut start = vec![0.0; columns];
    for (vertex, &slot) in column.iter().enumerate() {
        if let (Some(c), Some(uv)) = (slot, guess.get(vertex)) {
            start[c] = f64::from(uv.x);
            start[c + 1] = f64::from(uv.y);
        }
    }
    let steps = (columns * 10 + 100).min(50_000);
    let solved = least_squares(&a, &b, start, 1e-10, steps);
    let mut uvs: Vec<Vec2> = (0..chart.vertices)
        .map(|vertex| match (column[vertex], held_uv[vertex]) {
            (Some(c), _) => DVec2::new(solved[c], solved[c + 1]).as_vec2(),
            (None, Some(uv)) => uv.as_vec2(),
            (None, None) => Vec2::ZERO,
        })
        .collect();
    if let [(vertex, uv)] = pinned {
        let shift = *uv - uvs[*vertex];
        for value in &mut uvs {
            *value += shift;
        }
    }
    uvs.iter().all(|uv| uv.is_finite()).then_some(uvs)
}

/// The two points farthest apart along the longer side of the box round
/// `points`.
fn farthest(points: &[Vec2]) -> Option<(usize, usize)> {
    if points.len() < 2 {
        return None;
    }
    let (min, max) = points
        .iter()
        .fold((Vec2::INFINITY, Vec2::NEG_INFINITY), |(min, max), &p| {
            (min.min(p), max.max(p))
        });
    let axis = if max.x - min.x >= max.y - min.y {
        Vec2::X
    } else {
        Vec2::Y
    };
    let by = |a: &(usize, &Vec2), b: &(usize, &Vec2)| a.1.dot(axis).total_cmp(&b.1.dot(axis));
    let low = points.iter().enumerate().min_by(by)?.0;
    let high = points.iter().enumerate().max_by(by)?.0;
    (low != high).then_some((low, high))
}

/// True if some triangle of `chart` with an area turns over in `uvs` (or
/// shrinks to nothing).
pub(crate) fn flips(mesh: &EditMesh, chart: &Chart, uvs: &[Vec2]) -> bool {
    let positions = chart_positions(mesh, chart);
    triangles(mesh, chart).into_iter().any(|[a, b, c]| {
        let surface = (positions[b] - positions[a])
            .cross(positions[c] - positions[a])
            .length();
        surface > 1e-9 && (uvs[b] - uvs[a]).perp_dot(uvs[c] - uvs[a]) <= 0.0
    })
}
