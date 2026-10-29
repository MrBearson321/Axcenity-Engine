//! More detail to sculpt: a voxel remesh, and subdividing every face.
//!
//! [`voxel_remesh`] samples how far each point of a grid `size` apart is
//! from the surface, inside or out, and puts an even surface of quads
//! through it (surface nets: a vertex in each cell the surface crosses, at
//! the middle of where it crosses the cell's edges, and a quad across
//! each crossed grid edge). The grid is centred on the mesh's box, so a
//! mesh mirrored across its middle comes out mirrored too.

use glam::{DVec2, Vec3};

use crate::half_edge::{EditMesh, Polygon};
use crate::modifiers::{ModifierKind, Purpose};

/// Grid points a voxel remesh may sample, at most.
pub const MAX_VOXELS: usize = 160 * 160 * 160;

/// Grid points round the mesh's box on each side.
const PAD: usize = 2;

/// `mesh`'s surface rebuilt as quads about `size` apart (closed, every
/// face smooth, in slot 0, without uvs). An error if `mesh` has no faces
/// or `size` would take more than [`MAX_VOXELS`] grid points.
pub fn voxel_remesh(mesh: &EditMesh, size: f32) -> Result<EditMesh, String> {
    let triangles: Vec<[Vec3; 3]> = (0..mesh.face_count() as u32)
        .flat_map(|face| mesh.triangles(face))
        .map(|corners| corners.map(|e| mesh.position(mesh.half_edge(e).vertex)))
        .collect();
    if triangles.is_empty() {
        return Err("There's no surface to remesh".to_owned());
    }
    if !(size > 0.0 && size.is_finite()) {
        return Err("The voxel size must be more than 0".to_owned());
    }
    let (min, max) = triangles.iter().flatten().fold(
        (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
        |(min, max), &p| (min.min(p), max.max(p)),
    );
    let center = (min + max) / 2.0;
    let extent = max - min;
    let n: [usize; 3] = std::array::from_fn(|a| (extent[a] / size).ceil() as usize + 1 + 2 * PAD);
    let total = n[0] as u64 * n[1] as u64 * n[2] as u64;
    if total > MAX_VOXELS as u64 {
        return Err(format!(
            "A voxel size of {size} needs {total} voxels, more than {MAX_VOXELS}: make it bigger"
        ));
    }
    let grid = Grid {
        n,
        origin: Vec3::new(
            center.x - (n[0] - 1) as f32 * size / 2.0,
            center.y - (n[1] - 1) as f32 * size / 2.0,
            center.z - (n[2] - 1) as f32 * size / 2.0,
        ),
        size,
    };
    let field = signed_distance(&grid, &triangles);
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

    /// The range of grid steps along `axis` within `[low, high]`.
    fn span(&self, axis: usize, low: f32, high: f32) -> std::ops::Range<usize> {
        let from = ((low - self.origin[axis]) / self.size).ceil().max(0.0) as usize;
        let to = (((high - self.origin[axis]) / self.size).floor() + 1.0).max(0.0) as usize;
        from.min(self.n[axis])..to.min(self.n[axis])
    }
}

/// How far each grid point is from the surface: negative inside. Points
/// more than two steps away get two steps.
fn signed_distance(grid: &Grid, triangles: &[[Vec3; 3]]) -> Vec<f32> {
    let band = grid.size * 2.0;
    let count = grid.n[0] * grid.n[1] * grid.n[2];
    let mut field = vec![band; count];
    for triangle in triangles {
        let low = triangle[0].min(triangle[1]).min(triangle[2]) - Vec3::splat(band);
        let high = triangle[0].max(triangle[1]).max(triangle[2]) + Vec3::splat(band);
        let [si, sj, sk] = std::array::from_fn(|a| grid.span(a, low[a], high[a]));
        for k in sk {
            for j in sj.clone() {
                for i in si.clone() {
                    let p = grid.point(i, j, k);
                    let d = p.distance(closest_on_triangle(p, *triangle));
                    let slot = &mut field[grid.index(i, j, k)];
                    *slot = slot.min(d);
                }
            }
        }
    }
    // Inside: an odd number of crossings along the row in -X.
    let rows = grid.n[1] * grid.n[2];
    let mut crossings: Vec<Vec<f32>> = vec![Vec::new(); rows];
    for triangle in triangles {
        let flat = triangle.map(|p| DVec2::new(p.y as f64, p.z as f64));
        let (a, mut b, mut c) = (0, 1, 2);
        let area = orient(flat[a], flat[b], flat[c]);
        if area == 0.0 {
            continue;
        }
        if area < 0.0 {
            std::mem::swap(&mut b, &mut c);
        }
        let area = area.abs();
        let low = triangle[0].min(triangle[1]).min(triangle[2]);
        let high = triangle[0].max(triangle[1]).max(triangle[2]);
        for k in grid.span(2, low.z, high.z) {
            for j in grid.span(1, low.y, high.y) {
                let p = grid.point(0, j, k);
                let at = DVec2::new(p.y as f64, p.z as f64);
                let weights = [(b, c), (c, a), (a, b)].map(|(u, v)| {
                    let w = orient(flat[u], flat[v], at);
                    (w, w > 0.0 || (w == 0.0 && owns(flat[v] - flat[u])))
                });
                if weights.iter().all(|(_, inside)| *inside) {
                    let x = (weights[0].0 * triangle[a].x as f64
                        + weights[1].0 * triangle[b].x as f64
                        + weights[2].0 * triangle[c].x as f64)
                        / area;
                    crossings[j + grid.n[1] * k].push(x as f32);
                }
            }
        }
    }
    for k in 0..grid.n[2] {
        for j in 0..grid.n[1] {
            let row = &mut crossings[j + grid.n[1] * k];
            row.sort_unstable_by(f32::total_cmp);
            let mut passed = 0;
            for i in 0..grid.n[0] {
                let x = grid.point(i, j, k).x;
                while passed < row.len() && row[passed] < x {
                    passed += 1;
                }
                if passed % 2 == 1 {
                    let slot = &mut field[grid.index(i, j, k)];
                    *slot = -*slot;
                }
            }
        }
    }
    field
}

/// Twice the signed area of `u v p`: positive when counter-clockwise.
fn orient(u: DVec2, v: DVec2, p: DVec2) -> f64 {
    (v.x - u.x) * (p.y - u.y) - (v.y - u.y) * (p.x - u.x)
}

/// Whether a point exactly on an edge running along `d` is inside: of the
/// two triangles sharing an edge (running opposite ways), exactly one owns
/// it, so a row through the edge crosses the surface once.
fn owns(d: DVec2) -> bool {
    d.y > 0.0 || (d.y == 0.0 && d.x < 0.0)
}

/// The point of the triangle nearest `p` (Ericson, Real-Time Collision
/// Detection, 5.1.5).
fn closest_on_triangle(p: Vec3, [a, b, c]: [Vec3; 3]) -> Vec3 {
    let (ab, ac, ap) = (b - a, c - a, p - a);
    let (d1, d2) = (ab.dot(ap), ac.dot(ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = p - b;
    let (d3, d4) = (ab.dot(bp), ac.dot(bp));
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return a + ab * (d1 / (d1 - d3));
    }
    let cp = p - c;
    let (d5, d6) = (ab.dot(cp), ac.dot(cp));
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return a + ac * (d2 / (d2 - d6));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        return b + (c - b) * ((d4 - d3) / ((d4 - d3) + (d5 - d6)));
    }
    let denominator = 1.0 / (va + vb + vc);
    a + ab * (vb * denominator) + ac * (vc * denominator)
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
