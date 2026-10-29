//! Modifiers that make a mesh of triangles: Decimate (fewer of them,
//! through meshoptimizer) and Triangulate.

use std::collections::HashMap;

use glam::Vec2;

use crate::half_edge::EditMesh;
use crate::ops::{Soup, SoupFace};

/// The smallest share of triangles Decimate keeps.
const MIN_RATIO: f32 = 0.01;

/// `mesh` as triangles, `ratio` as many as it has. Corners with their own
/// uv or material are kept apart, so meshoptimizer keeps seams and the
/// borders between materials where they are. Each triangle is smooth if
/// most faces of its material were.
pub(crate) fn decimate(mesh: &EditMesh, ratio: f32) -> EditMesh {
    if ratio.is_nan() || ratio >= 1.0 {
        return mesh.clone();
    }
    let ratio = ratio.max(MIN_RATIO);
    let mut index_of: HashMap<(u32, [u32; 2], u32), u32> = HashMap::new();
    let mut corners: Vec<(u32, Vec2, u32)> = Vec::new();
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    // Per material: how many faces are smooth, and how many in all.
    let mut smooth: HashMap<u32, (usize, usize)> = HashMap::new();
    for face in 0..mesh.face_count() as u32 {
        let material = mesh.face(face).material;
        let votes = smooth.entry(material).or_default();
        votes.0 += usize::from(mesh.face(face).smooth);
        votes.1 += 1;
        for triangle in mesh.triangles(face) {
            for corner in triangle {
                let edge = mesh.half_edge(corner);
                let key = (edge.vertex, edge.uv.to_array().map(f32::to_bits), material);
                let index = *index_of.entry(key).or_insert_with(|| {
                    corners.push((edge.vertex, edge.uv, material));
                    positions.push(mesh.position(edge.vertex).to_array());
                    (corners.len() - 1) as u32
                });
                indices.push(index);
            }
        }
    }
    let triangles = indices.len() / 3;
    let target = ((triangles as f32 * ratio).round() as usize).max(1) * 3;
    // No error limit: the ratio alone decides, as in Blender.
    let simplified = meshopt::simplify_decoder(
        &indices,
        &positions,
        target,
        1.0,
        meshopt::SimplifyOptions::None,
        None,
    );
    let faces = simplified
        .as_chunks::<3>()
        .0
        .iter()
        .map(|triangle| {
            let material = corners[triangle[0] as usize].2;
            let (smooth_faces, all) = smooth.get(&material).copied().unwrap_or_default();
            SoupFace {
                corners: triangle.map(|k| corners[k as usize].0).to_vec(),
                uvs: triangle.map(|k| corners[k as usize].1).to_vec(),
                uvs2: Vec::new(),
                normals: Vec::new(),
                material,
                smooth: smooth_faces * 2 > all,
            }
        })
        .collect();
    Soup {
        positions: mesh.positions().to_vec(),
        faces,
        auto_smooth: mesh.auto_smooth(),
        seams: mesh.seams().clone(),
        pins: mesh.pins().clone(),
    }
    .build(false)
    .mesh
}

/// Every face of `mesh` cut into triangles, each corner keeping its uv
/// and normal.
pub(super) fn triangulate(mesh: &EditMesh) -> EditMesh {
    let mut faces = Vec::new();
    for face in 0..mesh.face_count() as u32 {
        let like = mesh.face(face);
        for triangle in mesh.triangles(face) {
            let corner = |k: usize| *mesh.half_edge(triangle[k]);
            faces.push(SoupFace {
                corners: (0..3).map(|k| corner(k).vertex).collect(),
                uvs: (0..3).map(|k| corner(k).uv).collect(),
                uvs2: match mesh.has_uv2() {
                    true => (0..3).map(|k| corner(k).uv2).collect(),
                    false => Vec::new(),
                },
                normals: (0..3).map(|k| corner(k).normal).collect(),
                material: like.material,
                smooth: like.smooth,
            });
        }
    }
    Soup {
        positions: mesh.positions().to_vec(),
        faces,
        auto_smooth: mesh.auto_smooth(),
        seams: mesh.seams().clone(),
        pins: mesh.pins().clone(),
    }
    .build(false)
    .mesh
}
