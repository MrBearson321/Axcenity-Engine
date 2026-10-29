//! Subdivision Surface: Catmull-Clark subdivision, as Blender's modifier
//! does by default. Each level cuts every face into a quad per corner and
//! moves the points towards the smooth surface the cage outlines. Open
//! borders stay where they are, smoothed along themselves; texture
//! coordinates are carried along linearly.

use std::collections::HashMap;

use glam::{Vec2, Vec3};

use crate::half_edge::EditMesh;
use crate::ops::{Soup, SoupFace, key};

/// The most faces a subdivision may make.
const MAX_FACES: u64 = 2_000_000;

/// `mesh` subdivided `levels` times (0 leaves it as it is).
pub(super) fn catmull_clark(mesh: &EditMesh, levels: u32) -> Result<EditMesh, String> {
    if levels == 0 {
        return Ok(mesh.clone());
    }
    let mut soup = Soup::of(mesh);
    // The first level makes a quad per corner; each after, four per quad.
    let corners: u64 = soup.faces.iter().map(|f| f.corners.len() as u64).sum();
    let faces = corners.saturating_mul(4u64.saturating_pow(levels - 1));
    if faces > MAX_FACES {
        return Err(format!(
            "{levels} levels would make {faces} faces; use fewer"
        ));
    }
    for _ in 0..levels {
        soup = level(&soup);
    }
    Ok(soup.build(false).mesh)
}

/// One level of Catmull-Clark: the new soup's vertices are the old ones
/// moved, then a point on each edge, then one in each face.
fn level(soup: &Soup) -> Soup {
    let count = soup.positions.len();
    // Each edge once, in the order faces first run along them.
    let mut edge_of: HashMap<[u32; 2], usize> = HashMap::new();
    let mut edges: Vec<([u32; 2], Vec<usize>)> = Vec::new();
    let mut face_edges: Vec<Vec<usize>> = Vec::with_capacity(soup.faces.len());
    for (index, face) in soup.faces.iter().enumerate() {
        let mut around = Vec::with_capacity(face.corners.len());
        for k in 0..face.corners.len() {
            let (a, b) = face.edge(k);
            let edge = *edge_of.entry(key(a, b)).or_insert_with(|| {
                edges.push((key(a, b), Vec::new()));
                edges.len() - 1
            });
            edges[edge].1.push(index);
            around.push(edge);
        }
        face_edges.push(around);
    }
    let face_points: Vec<Vec3> = (0..soup.faces.len()).map(|f| soup.center(f)).collect();
    let middle = |[a, b]: [u32; 2]| (soup.position(a) + soup.position(b)) / 2.0;
    // An edge between two faces takes their points into account; any
    // other (an open border, or one with more faces) stays straight.
    let edge_points: Vec<Vec3> = edges
        .iter()
        .map(|(ends, faces)| match faces.as_slice() {
            [f, g] => (middle(*ends) * 2.0 + face_points[*f] + face_points[*g]) / 4.0,
            _ => middle(*ends),
        })
        .collect();

    let mut face_sum = vec![Vec3::ZERO; count];
    let mut face_count = vec![0u32; count];
    for (index, face) in soup.faces.iter().enumerate() {
        for &corner in &face.corners {
            face_sum[corner as usize] += face_points[index];
            face_count[corner as usize] += 1;
        }
    }
    let mut middle_sum = vec![Vec3::ZERO; count];
    let mut valence = vec![0u32; count];
    let mut border: Vec<Vec<u32>> = vec![Vec::new(); count];
    let mut pinned = vec![false; count];
    for (ends, faces) in &edges {
        let [a, b] = *ends;
        for (here, there) in [(a, b), (b, a)] {
            middle_sum[here as usize] += middle(*ends);
            valence[here as usize] += 1;
            match faces.len() {
                1 => border[here as usize].push(there),
                2 => {}
                _ => pinned[here as usize] = true,
            }
        }
    }
    let mut positions: Vec<Vec3> = (0..count)
        .map(|vertex| {
            let p = soup.positions[vertex];
            let n = valence[vertex] as f32;
            match border[vertex].as_slice() {
                _ if pinned[vertex] || face_count[vertex] == 0 => p,
                [] => {
                    let faces = face_sum[vertex] / face_count[vertex] as f32;
                    let edges = middle_sum[vertex] / n;
                    (faces + edges * 2.0 + p * (n - 3.0)) / n
                }
                [a, b] => p * 0.75 + (soup.position(*a) + soup.position(*b)) * 0.125,
                // A corner where borders meet stays put.
                _ => p,
            }
        })
        .collect();
    positions.extend(edge_points);
    positions.extend(face_points);

    let edge_vertex = |edge: usize| (count + edge) as u32;
    let face_vertex = |face: usize| (count + edges.len() + face) as u32;
    let mut faces = Vec::with_capacity(soup.faces.len() * 4);
    for (index, face) in soup.faces.iter().enumerate() {
        let n = face.corners.len();
        let middle_uv = face.uvs.iter().sum::<Vec2>() / n as f32;
        for k in 0..n {
            let prev = (k + n - 1) % n;
            let next = (k + 1) % n;
            faces.push(SoupFace::like(
                face,
                vec![
                    face.corners[k],
                    edge_vertex(face_edges[index][k]),
                    face_vertex(index),
                    edge_vertex(face_edges[index][prev]),
                ],
                vec![
                    face.uvs[k],
                    (face.uvs[k] + face.uvs[next]) / 2.0,
                    middle_uv,
                    (face.uvs[prev] + face.uvs[k]) / 2.0,
                ],
            ));
        }
    }
    Soup {
        positions,
        faces,
        auto_smooth: soup.auto_smooth,
        ..Soup::default()
    }
}
