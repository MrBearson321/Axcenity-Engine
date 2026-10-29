//! Modifiers that add copies of the mesh: Mirror, Array and Solidify.

use std::collections::{HashMap, HashSet};

use glam::{IVec3, Vec3};

use super::Axis;
use crate::half_edge::EditMesh;
use crate::ops::{Soup, SoupFace};

/// `mesh` and its reflection across the plane square to `axis` through
/// the origin. With `merge`, a vertex within `distance` of the plane is
/// put on it and shared with its reflection; a face lying in the plane
/// would be back to back with its own, so both go.
pub(super) fn mirror(mesh: &EditMesh, axis: Axis, merge: bool, distance: f32) -> EditMesh {
    let i = axis.index();
    let mut soup = Soup::of(mesh);
    let count = soup.positions.len();
    let on_plane: Vec<bool> = soup
        .positions
        .iter()
        .map(|p| merge && p[i].abs() <= distance)
        .collect();
    let mut reflection = Vec::with_capacity(count);
    for (vertex, &on) in on_plane.iter().enumerate() {
        if on {
            soup.positions[vertex][i] = 0.0;
            reflection.push(vertex as u32);
        } else {
            let mut position = soup.positions[vertex];
            position[i] = -position[i];
            reflection.push(soup.add(position));
        }
    }
    let reflect = |mut normal: Vec3| {
        normal[i] = -normal[i];
        normal
    };
    let mut faces = Vec::with_capacity(soup.faces.len() * 2);
    for mut face in std::mem::take(&mut soup.faces) {
        let on = |corner: &u32| on_plane[*corner as usize];
        if face.corners.iter().all(on) {
            continue;
        }
        // Faces at the seam are shaded again, so it doesn't show.
        if face.corners.iter().any(on) {
            face.normals.clear();
        }
        // A reflection turns a face over; reversed, it faces out again.
        let mut copy = face.clone();
        copy.corners = face
            .corners
            .iter()
            .rev()
            .map(|&c| reflection[c as usize])
            .collect();
        copy.uvs.reverse();
        copy.normals = face.normals.iter().rev().map(|&n| reflect(n)).collect();
        faces.push(face);
        faces.push(copy);
    }
    soup.faces = faces;
    soup.build(false).mesh
}

/// `count` copies of `mesh`, each `offset` times its size from the last.
/// With `merge`, each copy's vertices within `distance` of the last
/// copy's are joined to them, and faces left back to back go.
pub(super) fn array(
    mesh: &EditMesh,
    count: u32,
    offset: Vec3,
    merge: bool,
    distance: f32,
) -> EditMesh {
    let base = Soup::of(mesh);
    let (min, max) = bounds(&base.positions);
    let step = offset * (max - min);
    let joins = if merge && count > 1 && step.length() > distance {
        joins(&base.positions, step, distance)
    } else {
        vec![None; base.positions.len()]
    };
    let mut soup = Soup {
        positions: Vec::new(),
        faces: Vec::new(),
        auto_smooth: base.auto_smooth,
        ..Soup::default()
    };
    let mut last: Vec<u32> = Vec::new();
    for copy in 0..count {
        let shift = step * copy as f32;
        let mut joined = vec![false; base.positions.len()];
        let index: Vec<u32> = (0..base.positions.len())
            .map(|vertex| match joins[vertex] {
                Some(onto) if copy > 0 => {
                    joined[vertex] = true;
                    last[onto as usize]
                }
                _ => soup.add(base.positions[vertex] + shift),
            })
            .collect();
        for face in &base.faces {
            let mut face = face.clone();
            if face.corners.iter().any(|&c| joined[c as usize]) {
                face.normals.clear();
            }
            for corner in &mut face.corners {
                *corner = index[*corner as usize];
            }
            soup.faces.push(face);
        }
        last = index;
    }
    if merge {
        drop_back_to_back(&mut soup);
    }
    soup.build(false).mesh
}

/// For each vertex, the one a copy `step` along lands it on (within
/// `distance`; the nearest, then the lowest-numbered).
fn joins(positions: &[Vec3], step: Vec3, distance: f32) -> Vec<Option<u32>> {
    let cell = distance.max(1e-6);
    let key = |p: Vec3| (p / cell).floor().as_ivec3();
    let mut grid: HashMap<IVec3, Vec<u32>> = HashMap::new();
    for (vertex, &position) in positions.iter().enumerate() {
        grid.entry(key(position)).or_default().push(vertex as u32);
    }
    positions
        .iter()
        .map(|&position| {
            let target = position + step;
            let at = key(target);
            let mut best: Option<(f32, u32)> = None;
            for x in -1..=1 {
                for y in -1..=1 {
                    for z in -1..=1 {
                        let Some(near) = grid.get(&(at + IVec3::new(x, y, z))) else {
                            continue;
                        };
                        for &other in near {
                            let d = positions[other as usize].distance(target);
                            if d <= distance && best.is_none_or(|b| (d, other) < b) {
                                best = Some((d, other));
                            }
                        }
                    }
                }
            }
            best.map(|(_, other)| other)
        })
        .collect()
}

/// Takes out faces back to back with another (the same corners the other
/// way round), both of them.
fn drop_back_to_back(soup: &mut Soup) {
    let cycle = |corners: &[u32]| {
        let mut cycle = corners.to_vec();
        let start = (0..cycle.len()).min_by_key(|&k| cycle[k]).unwrap_or(0);
        cycle.rotate_left(start);
        cycle
    };
    let shapes: HashSet<Vec<u32>> = soup.faces.iter().map(|f| cycle(&f.corners)).collect();
    soup.faces.retain(|face| {
        let reversed: Vec<u32> = face.corners.iter().rev().copied().collect();
        !shapes.contains(&cycle(&reversed))
    });
}

/// A second skin `thickness` from the first, along each vertex's normal:
/// `offset` −1 puts it behind the surface, 1 in front (moving the surface
/// back), 0 either side. Open borders are joined by a rim of quads.
pub(super) fn solidify(mesh: &EditMesh, thickness: f32, offset: f32) -> EditMesh {
    let normals = vertex_normals(mesh);
    let count = mesh.vertex_count() as u32;
    let outer = thickness * (offset + 1.0) / 2.0;
    let inner = thickness * (offset - 1.0) / 2.0;
    let base = Soup::of(mesh);
    let mut soup = Soup {
        positions: Vec::with_capacity(count as usize * 2),
        faces: Vec::with_capacity(base.faces.len() * 2),
        auto_smooth: base.auto_smooth,
        ..Soup::default()
    };
    for along in [outer, inner] {
        for (&position, &normal) in base.positions.iter().zip(&normals) {
            soup.add(position + normal * along);
        }
    }
    for face in &base.faces {
        let mut front = face.clone();
        front.normals.clear();
        let mut back = SoupFace::like(
            face,
            face.corners.iter().map(|&c| c + count).collect(),
            face.uvs.clone(),
        );
        back.flip();
        soup.faces.push(front);
        soup.faces.push(back);
    }
    // A border edge a→b of a face gets the quad b, a, a', b' (a' behind a).
    for (index, edge) in mesh.half_edges().iter().enumerate() {
        if edge.twin.is_some() {
            continue;
        }
        let (a, b) = mesh.ends(index as u32);
        let next = mesh.half_edge(edge.next);
        let like = &base.faces[edge.face as usize];
        soup.faces.push(SoupFace::like(
            like,
            vec![b, a, a + count, b + count],
            vec![next.uv, edge.uv, edge.uv, next.uv],
        ));
    }
    soup.build(false).mesh
}

/// Each vertex's normal: the faces round it, weighted by their corners'
/// angles, one unit long (zero for a vertex no face uses).
fn vertex_normals(mesh: &EditMesh) -> Vec<Vec3> {
    let mut normals = vec![Vec3::ZERO; mesh.vertex_count()];
    let faces: Vec<Vec3> = (0..mesh.face_count() as u32)
        .map(|f| mesh.face_normal(f))
        .collect();
    for edge in mesh.half_edges() {
        let here = mesh.position(edge.vertex);
        let next = mesh.position(mesh.half_edge(edge.next).vertex);
        let prev = mesh.position(mesh.half_edge(edge.prev).vertex);
        let angle = (next - here).angle_between(prev - here);
        if angle.is_finite() {
            normals[edge.vertex as usize] += faces[edge.face as usize] * angle;
        }
    }
    normals.into_iter().map(Vec3::normalize_or_zero).collect()
}

/// The smallest and largest corner of the box round `positions`.
pub(crate) fn bounds(positions: &[Vec3]) -> (Vec3, Vec3) {
    let Some(&first) = positions.first() else {
        return (Vec3::ZERO, Vec3::ZERO);
    };
    positions
        .iter()
        .fold((first, first), |(min, max), &p| (min.min(p), max.max(p)))
}
