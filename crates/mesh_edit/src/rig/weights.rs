//! Skin weights: automatic (bone heat), by distance, and painted.
//!
//! **Bone heat** (Baran and Popović's Pinocchio, as Blender's Automatic
//! Weights): each bone's weight is the steady temperature of the surface
//! when the bone is a heat source, `(L + A·H) w = A·H·p`. `L` is the
//! cotangent Laplacian (how the surface spreads heat), `A` each vertex's
//! area, `H` how strongly a vertex is held to the bone nearest it that it
//! can see without the mesh in the way (1 / distance²), and `p` 1 for that
//! bone (shared between bones as near as each other). Each bone's system
//! is solved by conjugate gradients; then each vertex keeps its 4
//! strongest, adding up to 1 (cut off continuously: see `Skin::limit`).

use glam::Vec3;

use super::{Armature, MAX_INFLUENCES, Skin, nearest_points};
use crate::half_edge::EditMesh;
use crate::paint::BrushFalloff;
use crate::reference::Reference;

/// Weights under this are dropped after bone heat.
const SMALLEST: f32 = 0.01;

/// The deforming bones as segments: each bone's index, head and tail.
fn segments(armature: &Armature) -> Vec<(usize, Vec3, Vec3)> {
    armature
        .bones
        .iter()
        .enumerate()
        .filter(|(_, bone)| bone.deform)
        .map(|(k, bone)| (k, bone.head, bone.tail()))
        .collect()
}

/// The point of the segment from `a` to `b` nearest `p`.
fn closest_on_segment(p: Vec3, a: Vec3, b: Vec3) -> Vec3 {
    let ab = b - a;
    let t = if ab.length_squared() > 1e-12 {
        ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0)
    } else {
        0.0
    };
    a + ab * t
}

/// The mesh's faces cut into triangles, by vertex.
fn triangles(mesh: &EditMesh) -> Vec<[u32; 3]> {
    (0..mesh.face_count() as u32)
        .flat_map(|face| mesh.triangles(face))
        .map(|triangle| triangle.map(|e| mesh.half_edge(e).vertex))
        .collect()
}

/// A sparse symmetric matrix by rows.
struct Sparse {
    /// Each row's other columns and values.
    rows: Vec<Vec<(u32, f64)>>,
    diagonal: Vec<f64>,
}

impl Sparse {
    fn times(&self, x: &[f64], out: &mut [f64]) {
        for (i, row) in self.rows.iter().enumerate() {
            let mut sum = self.diagonal[i] * x[i];
            for &(j, value) in row {
                sum += value * x[j as usize];
            }
            out[i] = sum;
        }
    }
}

/// The cotangent Laplacian (positive on the diagonal) and each vertex's
/// area (a third of its triangles').
fn laplacian(positions: &[Vec3], triangles: &[[u32; 3]]) -> (Sparse, Vec<f64>) {
    let n = positions.len();
    let mut edges: std::collections::BTreeMap<(u32, u32), f64> = Default::default();
    let mut area = vec![0.0f64; n];
    for &[a, b, c] in triangles {
        let [pa, pb, pc] = [a, b, c].map(|v| positions[v as usize].as_dvec3());
        let twice = (pb - pa).cross(pc - pa).length();
        if twice < 1e-18 {
            continue;
        }
        for v in [a, b, c] {
            area[v as usize] += twice / 6.0;
        }
        // The angle at each corner weighs the edge across from it.
        for (corner, (u, w)) in [(pa, (b, c)), (pb, (c, a)), (pc, (a, b))] {
            let (pu, pw) = (
                positions[u as usize].as_dvec3(),
                positions[w as usize].as_dvec3(),
            );
            let (e1, e2) = (pu - corner, pw - corner);
            let cot = e1.dot(e2) / e1.cross(e2).length().max(1e-18);
            let key = (u.min(w), u.max(w));
            *edges.entry(key).or_insert(0.0) += cot / 2.0;
        }
    }
    let mut rows = vec![Vec::new(); n];
    let mut diagonal = vec![0.0; n];
    for ((u, w), weight) in edges {
        // Obtuse triangles can make an edge's weight negative; held just
        // above 0, the matrix stays one whose solutions stay in 0 to 1.
        let weight = weight.clamp(1e-3, 1e3);
        rows[u as usize].push((w, -weight));
        rows[w as usize].push((u, -weight));
        diagonal[u as usize] += weight;
        diagonal[w as usize] += weight;
    }
    (Sparse { rows, diagonal }, area)
}

/// Solves `matrix · x = b` from `x` by conjugate gradients with each
/// row's diagonal as the preconditioner.
fn conjugate_gradients(matrix: &Sparse, b: &[f64], x: &mut [f64]) {
    let n = b.len();
    let inverse: Vec<f64> = matrix
        .diagonal
        .iter()
        .map(|&d| if d > 0.0 { 1.0 / d } else { 1.0 })
        .collect();
    let mut r = vec![0.0; n];
    matrix.times(x, &mut r);
    for i in 0..n {
        r[i] = b[i] - r[i];
    }
    let norm_b = b.iter().map(|v| v * v).sum::<f64>().sqrt().max(1e-30);
    let mut z: Vec<f64> = (0..n).map(|i| r[i] * inverse[i]).collect();
    let mut p = z.clone();
    let mut rz: f64 = (0..n).map(|i| r[i] * z[i]).sum();
    let mut ap = vec![0.0; n];
    for _ in 0..4 * n.max(100) {
        let norm_r = r.iter().map(|v| v * v).sum::<f64>().sqrt();
        if norm_r <= 1e-7 * norm_b {
            break;
        }
        matrix.times(&p, &mut ap);
        let pap: f64 = (0..n).map(|i| p[i] * ap[i]).sum();
        if pap.abs() < 1e-300 {
            break;
        }
        let alpha = rz / pap;
        for i in 0..n {
            x[i] += alpha * p[i];
            r[i] -= alpha * ap[i];
            z[i] = r[i] * inverse[i];
        }
        let next: f64 = (0..n).map(|i| r[i] * z[i]).sum();
        let beta = next / rz;
        rz = next;
        for i in 0..n {
            p[i] = z[i] + beta * p[i];
        }
    }
}

/// Automatic Weights: bone heat from the armature's deforming bones onto
/// `mesh`. A vertex that no bone reaches takes its nearest bone.
pub fn automatic_weights(mesh: &EditMesh, armature: &Armature) -> Skin {
    let positions = mesh.positions();
    let n = positions.len();
    let bones = segments(armature);
    if bones.is_empty() || n == 0 {
        return Skin::new(mesh, vec![Vec::new(); n]);
    }
    let triangles = triangles(mesh);
    let surface = Reference::new(mesh);
    let step = 1e-3 * surface.size().max(1e-6);

    // Each vertex's nearest visible bones (shared alike) and its heat.
    let mut heat = vec![0.0f64; n];
    let mut source: Vec<Vec<(usize, f64)>> = vec![Vec::new(); n];
    let mut nearest_any = vec![0usize; n];
    for (vertex, &p) in positions.iter().enumerate() {
        let mut found: Vec<(usize, f32)> = Vec::new();
        let mut closest = (f32::INFINITY, 0);
        for (k, &(_, a, b)) in bones.iter().enumerate() {
            let at = closest_on_segment(p, a, b);
            let distance = p.distance(at);
            if distance < closest.0 {
                closest = (distance, k);
            }
            let Some(direction) = (at - p).try_normalize() else {
                found.push((k, 0.0));
                continue;
            };
            let limit = distance - 2.0 * step;
            if limit <= 0.0 || !surface.occluded(p + direction * step, direction, limit) {
                found.push((k, distance));
            }
        }
        nearest_any[vertex] = closest.1;
        let Some(best) = found.iter().map(|&(_, d)| d).reduce(f32::min) else {
            continue;
        };
        let near: Vec<usize> = found
            .iter()
            .filter(|&&(_, d)| d <= best * 1.0001 + 1e-9)
            .map(|&(k, _)| k)
            .collect();
        let share = 1.0 / near.len() as f64;
        source[vertex] = near.into_iter().map(|k| (k, share)).collect();
        heat[vertex] = 1.0 / f64::from(best.max(step)).powi(2);
    }

    let (mut matrix, area) = laplacian(positions, &triangles);
    for vertex in 0..n {
        matrix.diagonal[vertex] += area[vertex] * heat[vertex];
    }
    // Each bone's temperature, solved on threads.
    let solve = |k: usize| -> Vec<f64> {
        let mut b = vec![0.0; n];
        let mut x = vec![0.0; n];
        for vertex in 0..n {
            for &(bone, share) in &source[vertex] {
                if bone == k {
                    b[vertex] = area[vertex] * heat[vertex] * share;
                    x[vertex] = share;
                }
            }
        }
        if b.iter().all(|&v| v == 0.0) {
            return x;
        }
        conjugate_gradients(&matrix, &b, &mut x);
        x
    };
    let threads = std::thread::available_parallelism().map_or(1, |t| t.get().min(16));
    let per = bones.len().div_ceil(threads);
    let solved: Vec<Vec<f64>> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..bones.len())
            .collect::<Vec<_>>()
            .chunks(per.max(1))
            .map(|chunk| {
                let chunk = chunk.to_vec();
                let solve = &solve;
                scope.spawn(move || chunk.into_iter().map(solve).collect::<Vec<_>>())
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|handle| handle.join().expect("a solve doesn't panic"))
            .collect()
    });

    let weights = (0..n)
        .map(|vertex| {
            let list: Vec<(u16, f32)> = bones
                .iter()
                .zip(&solved)
                .map(|(&(bone, _, _), temperature)| {
                    (bone as u16, temperature[vertex].clamp(0.0, 1.0) as f32)
                })
                .filter(|&(_, w)| w >= SMALLEST)
                .collect();
            if list.is_empty() {
                vec![(bones[nearest_any[vertex]].0 as u16, 1.0)]
            } else {
                list
            }
        })
        .collect();
    finish(mesh, weights)
}

/// The weights each vertex keeps: its 4 strongest, adding up to 1.
fn finish(mesh: &EditMesh, weights: Vec<Vec<(u16, f32)>>) -> Skin {
    let mut skin = Skin::new(mesh, weights);
    skin.limit(MAX_INFLUENCES);
    skin.normalize();
    skin
}

/// Weights by Distance: each vertex by its nearest deforming bones, each
/// counting as 1 / distance⁴, its 4 strongest kept, adding up to 1.
pub fn weights_by_distance(mesh: &EditMesh, armature: &Armature) -> Skin {
    let bones = segments(armature);
    let size = Reference::new(mesh).size().max(1e-6);
    let weights = mesh
        .positions()
        .iter()
        .map(|&p| {
            let raw: Vec<(u16, f32)> = bones
                .iter()
                .map(|&(bone, a, b)| {
                    let d = p.distance(closest_on_segment(p, a, b)).max(1e-4 * size);
                    (bone as u16, 1.0 / d.powi(4))
                })
                .collect();
            let strongest = raw.iter().map(|&(_, w)| w).fold(0.0, f32::max);
            raw.into_iter()
                .filter(|&(_, w)| w >= SMALLEST * strongest)
                .collect()
        })
        .collect();
    finish(mesh, weights)
}

/// What a weight brush does to the active bone's weights.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WeightTool {
    /// Toward 1.
    #[default]
    Add,
    /// Toward 0.
    Subtract,
    /// The active bone's weight toward its neighbours' average.
    Smooth,
    /// Every bone's weight toward its neighbours' average.
    Blur,
}

impl WeightTool {
    pub const ALL: [WeightTool; 4] = [
        WeightTool::Add,
        WeightTool::Subtract,
        WeightTool::Smooth,
        WeightTool::Blur,
    ];

    pub fn label(self) -> &'static str {
        match self {
            WeightTool::Add => "Add",
            WeightTool::Subtract => "Subtract",
            WeightTool::Smooth => "Smooth",
            WeightTool::Blur => "Blur",
        }
    }
}

/// A weight brush's settings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WeightBrush {
    pub tool: WeightTool,
    /// In points on screen (the Studio turns it into the model's units).
    pub radius: f32,
    pub strength: f32,
    pub falloff: BrushFalloff,
    /// After each dab, a touched vertex's other bones are scaled so all
    /// its weights add up to 1 again.
    pub auto_normalize: bool,
}

impl Default for WeightBrush {
    fn default() -> Self {
        WeightBrush {
            tool: WeightTool::Add,
            radius: 40.0,
            strength: 0.5,
            falloff: BrushFalloff::Smooth,
            auto_normalize: true,
        }
    }
}

/// A mesh's vertices and who's next to whom, for weight brushes.
pub struct WeightPainter {
    positions: Vec<Vec3>,
    neighbours: Vec<Vec<u32>>,
}

impl WeightPainter {
    pub fn new(mesh: &EditMesh) -> WeightPainter {
        let mut neighbours = vec![Vec::new(); mesh.vertex_count()];
        for [a, b, c] in triangles(mesh) {
            for (u, w) in [(a, b), (b, c), (c, a)] {
                if !neighbours[u as usize].contains(&w) {
                    neighbours[u as usize].push(w);
                }
                if !neighbours[w as usize].contains(&u) {
                    neighbours[w as usize].push(u);
                }
            }
        }
        WeightPainter {
            positions: mesh.positions().to_vec(),
            neighbours,
        }
    }

    /// One dab of `brush` on `bone`'s weights in `skin`, round `center`
    /// out to `radius` (the model's units). Every vertex's new weights are
    /// worked out from the old ones before any changes. True if any did.
    pub fn dab(
        &self,
        skin: &mut Skin,
        brush: &WeightBrush,
        bone: usize,
        center: Vec3,
        radius: f32,
    ) -> bool {
        if radius <= 0.0 || skin.weights.len() != self.positions.len() {
            return false;
        }
        let touched: Vec<(usize, f32)> = self
            .positions
            .iter()
            .enumerate()
            .filter_map(|(k, &p)| {
                let d = p.distance(center);
                (d < radius).then(|| (k, brush.strength * brush.falloff.weight(d / radius)))
            })
            .filter(|&(_, f)| f > 0.0)
            .collect();
        let mean = |vertex: usize, bone: usize| {
            let around = &self.neighbours[vertex];
            if around.is_empty() {
                return skin.weight(vertex, bone);
            }
            around
                .iter()
                .map(|&v| skin.weight(v as usize, bone))
                .sum::<f32>()
                / around.len() as f32
        };
        let new: Vec<(usize, Vec<(u16, f32)>)> = touched
            .iter()
            .map(|&(vertex, f)| {
                let mut list = skin.weights[vertex].clone();
                let set = |list: &mut Vec<(u16, f32)>, bone: usize, w: f32| {
                    list.retain(|(b, _)| *b as usize != bone);
                    if w > 1e-4 {
                        list.push((bone as u16, w.clamp(0.0, 1.0)));
                    }
                };
                let old = skin.weight(vertex, bone);
                match brush.tool {
                    WeightTool::Add => set(&mut list, bone, old + (1.0 - old) * f),
                    WeightTool::Subtract => set(&mut list, bone, old * (1.0 - f)),
                    WeightTool::Smooth => {
                        set(&mut list, bone, old + (mean(vertex, bone) - old) * f)
                    }
                    WeightTool::Blur => {
                        let mut bones: Vec<usize> = list.iter().map(|&(b, _)| b as usize).collect();
                        for &v in &self.neighbours[vertex] {
                            bones.extend(skin.weights[v as usize].iter().map(|&(b, _)| b as usize));
                        }
                        bones.sort_unstable();
                        bones.dedup();
                        for b in bones {
                            let w = skin.weight(vertex, b);
                            set(&mut list, b, w + (mean(vertex, b) - w) * f);
                        }
                    }
                }
                if brush.auto_normalize {
                    normalize_keeping(&mut list, (brush.tool != WeightTool::Blur).then_some(bone));
                }
                list.sort_by_key(|&(b, _)| b);
                (vertex, list)
            })
            .collect();
        let mut changed = false;
        for (vertex, list) in new {
            if skin.weights[vertex] != list {
                skin.weights[vertex] = list;
                changed = true;
            }
        }
        changed
    }
}

/// Scales `list`'s weights to add up to 1, keeping `keep`'s own where it
/// can (the others share what's left; with no others, it's left be).
fn normalize_keeping(list: &mut [(u16, f32)], keep: Option<usize>) {
    let kept: f32 = list
        .iter()
        .filter(|(b, _)| Some(*b as usize) == keep)
        .map(|(_, w)| w)
        .sum();
    let others: f32 = list
        .iter()
        .filter(|(b, _)| Some(*b as usize) != keep)
        .map(|(_, w)| w)
        .sum();
    if keep.is_some() && others > 1e-6 {
        let scale = (1.0 - kept).max(0.0) / others;
        for (b, w) in list.iter_mut() {
            if Some(*b as usize) != keep {
                *w *= scale;
            }
        }
    } else if keep.is_none() {
        let total = kept + others;
        if total > 1e-6 {
            for (_, w) in list.iter_mut() {
                *w /= total;
            }
        }
    }
}

/// Mirror weights: each vertex on one side of X = 0 takes the weights of
/// the vertex where its mirror image is (within a thousandth of the
/// model's size), with each bone swapped for its mirror (`.L` ↔ `.R`).
/// `from_positive` copies +X onto −X. Returns how many vertices changed.
pub fn mirror_weights(
    skin: &mut Skin,
    mesh: &EditMesh,
    armature: &Armature,
    from_positive: bool,
) -> usize {
    let positions = mesh.positions();
    if skin.weights.len() != positions.len() {
        return 0;
    }
    let size = Reference::new(mesh).size().max(1e-6);
    let side = if from_positive { -1.0 } else { 1.0 };
    let targets: Vec<usize> = (0..positions.len())
        .filter(|&k| positions[k].x * side > 1e-4 * size)
        .collect();
    let images: Vec<Vec3> = targets
        .iter()
        .map(|&k| positions[k] * Vec3::new(-1.0, 1.0, 1.0))
        .collect();
    let sources = nearest_points(positions, &images);
    let mut changed = 0;
    for ((&target, image), source) in targets.iter().zip(&images).zip(sources) {
        let Some(source) = source.filter(|&s| positions[s].distance(*image) < 1e-3 * size) else {
            continue;
        };
        let list: Vec<(u16, f32)> = skin.weights[source]
            .iter()
            .map(|&(bone, w)| {
                let mirrored = armature.mirror_of(bone as usize).unwrap_or(bone as usize);
                (mirrored as u16, w)
            })
            .collect();
        if skin.weights[target] != list {
            skin.weights[target] = list;
            changed += 1;
        }
    }
    changed
}
