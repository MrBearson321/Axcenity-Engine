//! Make Low Poly (R15, from Phase 61): a lighter copy of a high-poly
//! object to bake its detail onto. [`decimate_to`] cuts it to a number of
//! triangles and [`relax`] evens the result out on the high-poly surface.

use std::collections::BTreeSet;

use glam::Vec3;

use crate::half_edge::EditMesh;
use crate::modifiers::reduce;
use crate::reference::Reference;

/// Evens out `mesh` on `reference`: `times` over, each vertex moves
/// halfway to the middle of its neighbours (along the faces' edges) and
/// back onto the surface. A decimated mesh's thin triangles, and those
/// folded across a ridge, open out. Open borders stay where they are.
pub fn relax(mesh: &mut EditMesh, reference: &Reference, times: usize) {
    let count = mesh.vertex_count();
    let mut neighbours: Vec<BTreeSet<u32>> = vec![BTreeSet::new(); count];
    let mut border = vec![false; count];
    for (index, half) in mesh.half_edges().iter().enumerate() {
        let (a, b) = mesh.ends(index as u32);
        neighbours[a as usize].insert(b);
        neighbours[b as usize].insert(a);
        if half.twin.is_none() {
            border[a as usize] = true;
            border[b as usize] = true;
        }
    }
    let vertices: Vec<u32> = (0..count as u32)
        .filter(|&v| !border[v as usize] && !neighbours[v as usize].is_empty())
        .collect();
    for _ in 0..times {
        let places: Vec<Vec3> = vertices
            .iter()
            .map(|&v| {
                let around = &neighbours[v as usize];
                let middle =
                    around.iter().map(|&n| mesh.position(n)).sum::<Vec3>() / around.len() as f32;
                reference.snap(mesh.position(v).lerp(middle, 0.5))
            })
            .collect();
        mesh.set_positions(&vertices, &places);
    }
}

/// `mesh` cut to about `triangles` triangles (or fewer) by meshoptimizer,
/// as the Decimate modifier does; uv seams and material borders stay.
pub fn decimate_to(mesh: &EditMesh, triangles: usize) -> EditMesh {
    let count: usize = (0..mesh.face_count() as u32)
        .map(|face| mesh.triangles(face).len())
        .sum();
    if count == 0 || triangles >= count {
        return mesh.clone();
    }
    let mut ratio = triangles as f32 / count as f32;
    let mut result = reduce::decimate(mesh, ratio);
    // meshoptimizer stops short of the target where it can't collapse
    // more without breaking the surface: aim lower until under it.
    for _ in 0..4 {
        let made = result.face_count();
        if made <= triangles {
            break;
        }
        ratio *= triangles as f32 / made as f32 * 0.98;
        result = reduce::decimate(mesh, ratio);
    }
    result
}
