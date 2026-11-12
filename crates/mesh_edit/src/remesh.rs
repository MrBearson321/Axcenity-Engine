//! Surfaces from distance fields, and subdividing every face.
//!
//! [`field_mesh`] samples a distance function on a grid `size` apart and
//! puts an even surface of quads through it (surface nets: a vertex in each
//! cell the surface crosses, at the middle of where it crosses the cell's
//! edges, and a quad across each crossed grid edge). The grid is centred
//! on the box, so a body mirrored across its middle comes out mirrored too.
//! Rigging's Humanoid template builds its body this way. Sculpting's voxel
//! remesh, which also used it, was removed in Phase 90.

use glam::Vec3;

use crate::half_edge::{EditMesh, Polygon};
use crate::modifiers::{ModifierKind, Purpose};

/// Grid points a field may be sampled at, at most.
pub const MAX_VOXELS: usize = 160 * 160 * 160;

/// Grid points round the mesh's box on each side.
const PAD: usize = 2;

/// The surface where `distance` (negative inside) crosses 0, sampled
/// `size` apart over the box from `min` to `max` (padded). An error past
/// [`MAX_VOXELS`].
pub(crate) fn field_mesh(
    min: Vec3,
    max: Vec3,
    size: f32,
    distance: impl Fn(Vec3) -> f32,
) -> Result<EditMesh, String> {
    let center = (min + max) / 2.0;
    let extent = max - min;
    let n: [usize; 3] = std::array::from_fn(|a| (extent[a] / size).ceil() as usize + 1 + 2 * PAD);
    if n.iter().map(|&k| k as u64).product::<u64>() > MAX_VOXELS as u64 {
        return Err("too many voxels".to_owned());
    }
    let grid = Grid {
        n,
        origin: center
            - Vec3::new((n[0] - 1) as f32, (n[1] - 1) as f32, (n[2] - 1) as f32) * size / 2.0,
        size,
    };
    let mut field = vec![0.0; n[0] * n[1] * n[2]];
    for k in 0..n[2] {
        for j in 0..n[1] {
            for i in 0..n[0] {
                field[grid.index(i, j, k)] = distance(grid.point(i, j, k));
            }
        }
    }
    Ok(surface_nets(&grid, &field))
}

/// Every face cut into four (Catmull-Clark, one level), smoothing the
/// surface on the way.
pub fn subdivide(mesh: &EditMesh) -> Result<EditMesh, String> {
    ModifierKind::Subdivision {
        viewport: 1,
        export: 1,
    }
    .run(mesh, Purpose::Viewport)
}

/// Points `size` apart from `origin`, `n` along each axis.
struct Grid {
    n: [usize; 3],
    origin: Vec3,
    size: f32,
}

impl Grid {
    fn index(&self, i: usize, j: usize, k: usize) -> usize {
        i + self.n[0] * (j + self.n[1] * k)
    }

    fn point(&self, i: usize, j: usize, k: usize) -> Vec3 {
        self.origin + Vec3::new(i as f32, j as f32, k as f32) * self.size
    }

    fn cell(&self, i: usize, j: usize, k: usize) -> usize {
        i + (self.n[0] - 1) * (j + (self.n[1] - 1) * k)
    }
}

/// The surface where `field` crosses 0, as quads facing out.
fn surface_nets(grid: &Grid, field: &[f32]) -> EditMesh {
    let [nx, ny, nz] = grid.n;
    let inside = |i: usize, j: usize, k: usize| field[grid.index(i, j, k)] < 0.0;
    // A vertex in each cell the surface crosses.
    let mut cell_vertex = vec![u32::MAX; (nx - 1) * (ny - 1) * (nz - 1)];
    let mut positions = Vec::new();
    const EDGES: [([usize; 3], [usize; 3]); 12] = [
        ([0, 0, 0], [1, 0, 0]),
        ([0, 1, 0], [1, 1, 0]),
        ([0, 0, 1], [1, 0, 1]),
        ([0, 1, 1], [1, 1, 1]),
        ([0, 0, 0], [0, 1, 0]),
        ([1, 0, 0], [1, 1, 0]),
        ([0, 0, 1], [0, 1, 1]),
        ([1, 0, 1], [1, 1, 1]),
        ([0, 0, 0], [0, 0, 1]),
        ([1, 0, 0], [1, 0, 1]),
        ([0, 1, 0], [0, 1, 1]),
        ([1, 1, 0], [1, 1, 1]),
    ];
    for k in 0..nz - 1 {
        for j in 0..ny - 1 {
            for i in 0..nx - 1 {
                let (mut sum, mut count) = (Vec3::ZERO, 0);
                for (from, to) in EDGES {
                    let (a, b) = (
                        [i + from[0], j + from[1], k + from[2]],
                        [i + to[0], j + to[1], k + to[2]],
                    );
                    let fa = field[grid.index(a[0], a[1], a[2])];
                    let fb = field[grid.index(b[0], b[1], b[2])];
                    if (fa < 0.0) == (fb < 0.0) {
                        continue;
                    }
                    let (pa, pb) = (grid.point(a[0], a[1], a[2]), grid.point(b[0], b[1], b[2]));
                    sum += pa + (pb - pa) * (fa / (fa - fb));
                    count += 1;
                }
                if count > 0 {
                    cell_vertex[grid.cell(i, j, k)] = positions.len() as u32;
                    positions.push(sum / count as f32);
                }
            }
        }
    }
    // A quad across each grid edge the surface crosses, from the four
    // cells round it, counter-clockwise seen from outside.
    let mut polygons = Vec::new();
    let mut quad = |cells: [usize; 4], outward: bool| {
        let mut corners = cells.map(|cell| cell_vertex[cell]);
        if !outward {
            corners.reverse();
        }
        polygons.push(Polygon {
            corners: corners.to_vec(),
            uvs: Vec::new(),
            uvs2: Vec::new(),
            normals: Vec::new(),
            material: 0,
            smooth: true,
        });
    };
    for k in 1..nz - 1 {
        for j in 1..ny - 1 {
            for i in 0..nx - 1 {
                if inside(i, j, k) != inside(i + 1, j, k) {
                    let cells = [
                        grid.cell(i, j - 1, k - 1),
                        grid.cell(i, j, k - 1),
                        grid.cell(i, j, k),
                        grid.cell(i, j - 1, k),
                    ];
                    quad(cells, inside(i, j, k));
                }
            }
        }
    }
    for k in 1..nz - 1 {
        for j in 0..ny - 1 {
            for i in 1..nx - 1 {
                if inside(i, j, k) != inside(i, j + 1, k) {
                    let cells = [
                        grid.cell(i - 1, j, k - 1),
                        grid.cell(i - 1, j, k),
                        grid.cell(i, j, k),
                        grid.cell(i, j, k - 1),
                    ];
                    quad(cells, inside(i, j, k));
                }
            }
        }
    }
    for k in 0..nz - 1 {
        for j in 1..ny - 1 {
            for i in 1..nx - 1 {
                if inside(i, j, k) != inside(i, j, k + 1) {
                    let cells = [
                        grid.cell(i - 1, j - 1, k),
                        grid.cell(i, j - 1, k),
                        grid.cell(i, j, k),
                        grid.cell(i - 1, j, k),
                    ];
                    quad(cells, inside(i, j, k));
                }
            }
        }
    }
    EditMesh::new(positions, &polygons)
}
