//! Chunk meshes: what the renderer draws for each chunk of a terrain.
//!
//! Each chunk is built on its own (so an edit rebuilds only its chunks) at
//! one of [`LOD_LEVELS`] levels of detail: level `n` uses every `2^n`th
//! sample. Neighbouring chunks at different levels don't quite meet, so
//! every chunk hangs a skirt down from its edges, deep enough to hide the gap.
//!
//! Corners are relative to the terrain's `Position`. Normals come from the
//! whole heightfield, so they match across chunk edges. Each cell is split
//! along the same diagonal as parry's heightfield (from (x, z+1) to
//! (x+1, z)), so at full detail the drawn surface is exactly the one
//! physics collides with.

use axcenity_core::{CHUNK_CELLS, TerrainData};
use glam::Vec3;

use crate::Placement;

/// Full detail, then 1/2, 1/4, 1/8 and 1/16 of the samples.
pub const LOD_LEVELS: u32 = 5;

/// One chunk's triangles, with texture coordinates into its splat
/// textures (one texel per sample, texel centres on the samples).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChunkMesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
}

/// The level of detail for a chunk whose middle is `distance` studs from
/// the camera: full detail nearby, halving every time the distance
/// doubles past `detail_distance`.
pub fn lod_for_distance(distance: f32, detail_distance: f32) -> u32 {
    if !distance.is_finite() || distance <= detail_distance {
        return 0;
    }
    let level = (distance / detail_distance.max(1.0)).log2().floor() as u32 + 1;
    level.min(LOD_LEVELS - 1)
}

/// The mesh of chunk (`cx`, `cz`) at level of detail `lod`.
pub fn chunk_mesh(
    data: &TerrainData,
    placement: &Placement,
    cx: usize,
    cz: usize,
    lod: u32,
) -> ChunkMesh {
    let rect = data.chunk_rect(cx, cz);
    // A chunk with holes is always at full detail, so its holes keep
    // their shape far away.
    let holes = data.chunk_has_holes(cx, cz);
    let lod = if holes { 0 } else { lod };
    let step = 1usize << lod.min(LOD_LEVELS - 1);
    // The samples used along each side: every `step`th, and always the last.
    let along = |start: usize, end: usize| {
        let mut samples: Vec<usize> = (start..end).step_by(step).collect();
        if samples.last() != Some(&(end - 1)) {
            samples.push(end - 1);
        }
        samples
    };
    let xs = along(rect.x0, rect.x1);
    let zs = along(rect.z0, rect.z1);
    let (texels_x, texels_z) = (rect.width() as f32, rect.depth() as f32);
    let cell = placement.cell_size(data);
    let half = placement.size / 2.0;
    let point = |x: usize, z: usize| {
        Vec3::new(
            -half.x + x as f32 * cell.x,
            data.height(x as isize, z as isize) * placement.size.y,
            -half.z + z as f32 * cell.y,
        )
    };
    let uv = |x: usize, z: usize| {
        [
            ((x - rect.x0) as f32 + 0.5) / texels_x,
            ((z - rect.z0) as f32 + 0.5) / texels_z,
        ]
    };

    let mut mesh = ChunkMesh::default();
    for &z in &zs {
        for &x in &xs {
            mesh.positions.push(point(x, z).to_array());
            mesh.normals.push(placement.normal(data, x, z).to_array());
            mesh.uvs.push(uv(x, z));
        }
    }
    let columns = xs.len();
    let corner = |i: usize, j: usize| (j * columns + i) as u32;
    for (j, &z) in zs.iter().enumerate().take(zs.len() - 1) {
        for (i, &x) in xs.iter().enumerate().take(columns - 1) {
            let (a, b, c, d) = (
                corner(i, j),
                corner(i, j + 1),
                corner(i + 1, j),
                corner(i + 1, j + 1),
            );
            // A hole's cell isn't drawn (at full detail, a corner is a cell).
            if holes && data.is_hole(x, z) {
                continue;
            }
            // Counter-clockwise from above, split from (x, z+1) to (x+1, z).
            mesh.indices.extend_from_slice(&[a, b, c, b, d, c]);
        }
    }

    // Skirts: each edge's corners again, lowered, joined to the edge.
    let edges: [Vec<(usize, usize)>; 4] = [
        xs.iter().map(|&x| (x, zs[0])).collect(),
        zs.iter().map(|&z| (*xs.last().unwrap(), z)).collect(),
        xs.iter().rev().map(|&x| (x, *zs.last().unwrap())).collect(),
        zs.iter().rev().map(|&z| (xs[0], z)).collect(),
    ];
    for edge in edges {
        // Deep enough to cover any gap: the gap is never more than the
        // edge's own rise and fall.
        let heights: Vec<f32> = edge
            .iter()
            .map(|&(x, z)| data.height(x as isize, z as isize))
            .collect();
        let (low, high) = heights
            .iter()
            .fold((f32::MAX, f32::MIN), |(low, high), &h| {
                (low.min(h), high.max(h))
            });
        let depth = (high - low) * placement.size.y + cell.max_element() * step as f32;
        let first = mesh.positions.len() as u32;
        for &(x, z) in &edge {
            let top = point(x, z);
            mesh.positions.push(top.to_array());
            mesh.positions
                .push((top - Vec3::new(0.0, depth, 0.0)).to_array());
            let normal = placement.normal(data, x, z).to_array();
            mesh.normals.extend_from_slice(&[normal, normal]);
            mesh.uvs.extend_from_slice(&[uv(x, z), uv(x, z)]);
        }
        // The edges go round the chunk anticlockwise seen from above, so
        // these face outward.
        for k in 0..edge.len() as u32 - 1 {
            let (top, bottom) = (first + k * 2, first + k * 2 + 1);
            let (next_top, next_bottom) = (top + 2, bottom + 2);
            mesh.indices
                .extend_from_slice(&[top, next_top, bottom, bottom, next_top, next_bottom]);
        }
    }
    mesh
}

/// How many chunk meshes cover a terrain of `samples` samples a side, for tests.
pub fn chunks_along(samples: usize) -> usize {
    (samples - 1).div_ceil(CHUNK_CELLS).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bumpy() -> (TerrainData, Placement) {
        let mut data = TerrainData::flat(CHUNK_CELLS * 2 + 1, CHUNK_CELLS + 1, 0.0).unwrap();
        let (size_x, _) = data.size();
        let all = data.bounds();
        for (i, h) in data.heights_mut_in(all).iter_mut().enumerate() {
            let (x, z) = ((i % size_x) as f32, (i / size_x) as f32);
            *h = 0.5 + 0.3 * (x * 0.4).sin() * (z * 0.3).cos();
        }
        let placement = Placement {
            position: Vec3::ZERO,
            size: Vec3::new(128.0, 40.0, 64.0),
        };
        (data, placement)
    }

    #[test]
    fn full_detail_has_a_corner_per_sample_and_two_triangles_per_cell() {
        let (data, placement) = bumpy();
        let mesh = chunk_mesh(&data, &placement, 0, 0, 0);
        let samples = (CHUNK_CELLS + 1) * (CHUNK_CELLS + 1);
        let cells = CHUNK_CELLS * CHUNK_CELLS;
        let skirts = 4 * (CHUNK_CELLS + 1) * 2;
        assert_eq!(mesh.positions.len(), samples + skirts);
        assert_eq!(mesh.indices.len(), cells * 6 + 4 * CHUNK_CELLS * 6);
        assert!(
            mesh.indices
                .iter()
                .all(|&i| (i as usize) < mesh.positions.len())
        );
        // The first corner is the terrain's corner, at its height.
        assert_eq!(mesh.positions[0], [-64.0, data.height(0, 0) * 40.0, -32.0]);
        assert_eq!(mesh.uvs[0], [0.5 / 33.0, 0.5 / 33.0]);
    }

    #[test]
    fn surface_triangles_face_up() {
        let (data, placement) = bumpy();
        let mesh = chunk_mesh(&data, &placement, 1, 0, 0);
        let cells = CHUNK_CELLS * CHUNK_CELLS;
        for triangle in mesh.indices[..cells * 6].chunks(3) {
            let [a, b, c] = [0, 1, 2].map(|k| Vec3::from(mesh.positions[triangle[k] as usize]));
            assert!((b - a).cross(c - a).y > 0.0);
        }
    }

    #[test]
    fn neighbouring_chunks_share_their_edge_exactly() {
        let (data, placement) = bumpy();
        let left = chunk_mesh(&data, &placement, 0, 0, 0);
        let right = chunk_mesh(&data, &placement, 1, 0, 0);
        let columns = CHUNK_CELLS + 1;
        for row in 0..columns {
            let a = row * columns + CHUNK_CELLS;
            let b = row * columns;
            assert_eq!(left.positions[a], right.positions[b]);
            assert_eq!(left.normals[a], right.normals[b]);
        }
    }

    #[test]
    fn a_hole_leaves_its_cells_out_at_full_detail_even_far_away() {
        let (mut data, placement) = bumpy();
        let (size_x, _) = data.size();
        let rect = axcenity_core::SampleRect::new(40, 10, 43, 12);
        let holes = data.holes_mut_in(rect);
        for z in 10..12 {
            for x in 40..43 {
                holes[z * size_x + x] = true;
            }
        }
        let cells = CHUNK_CELLS * CHUNK_CELLS;
        let skirts = 4 * CHUNK_CELLS * 6;
        // Chunk (1, 0) holds cells 32 to 63 along X: six are cut out, and
        // asked for at a quarter of the detail it's still full.
        let cut = chunk_mesh(&data, &placement, 1, 0, 2);
        assert_eq!(cut.indices.len(), (cells - 6) * 6 + skirts);
        // The chunk beside it is as it was.
        let whole = chunk_mesh(&data, &placement, 0, 0, 0);
        assert_eq!(whole.indices.len(), cells * 6 + skirts);
    }

    #[test]
    fn lower_detail_uses_fewer_corners_and_keeps_the_edges() {
        let (data, placement) = bumpy();
        let full = chunk_mesh(&data, &placement, 0, 0, 0);
        let quarter = chunk_mesh(&data, &placement, 0, 0, 2);
        assert!(quarter.positions.len() * 8 < full.positions.len());
        // The far corner is still the chunk's corner.
        let columns = CHUNK_CELLS / 4 + 1;
        let last_of_quarter = quarter.positions[columns * columns - 1];
        let last_of_full = full.positions[(CHUNK_CELLS + 1) * (CHUNK_CELLS + 1) - 1];
        assert_eq!(last_of_quarter, last_of_full);
        assert_eq!(lod_for_distance(10.0, 64.0), 0);
        assert_eq!(lod_for_distance(100.0, 64.0), 1);
        assert_eq!(lod_for_distance(300.0, 64.0), 3);
        assert_eq!(lod_for_distance(1e9, 64.0), LOD_LEVELS - 1);
        assert_eq!(chunks_along(CHUNK_CELLS * 2 + 1), 2);
    }
}
