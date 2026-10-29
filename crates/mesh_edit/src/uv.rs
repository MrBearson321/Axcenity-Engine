//! UV unwrapping (Phase 58): seams, unwrapping by angle (LSCM), Smart UV
//! Project, cube, cylinder, sphere and view projections, packing islands,
//! averaging their scale, stretch, and the UV editor's edits of corners
//! (moves, stitching, aligning).
//!
//! Each works on one uv channel of an [`EditMesh`] (0, or 1 for the second
//! uvs a lightmap would use) and changes only corners' uvs, so vertex,
//! face and half-edge numbers stay put.
//!
//! Uvs are glTF's: (0, 0) is the top left of the texture and v grows down
//! it. The flattening works with v growing up (a face seen from outside
//! goes counter-clockwise, as in 3D) and turns v over when it writes, so a
//! texture isn't mirrored on the faces.
//!
//! - A **chart** is a set of faces flattened together, with its corners
//!   joined into **uv vertices**: corners at one vertex reached round it
//!   without crossing a cut. Unwrap cuts at seams ([`EditMesh::seams`]);
//!   islands in the UV editor are cut where the uvs on the two sides of an
//!   edge differ.

mod lscm;
mod pack;
mod project;
mod sparse;

use std::collections::{BTreeMap, BTreeSet};

use glam::{Affine2, Mat4, Vec2, Vec3};

use crate::half_edge::EditMesh;

pub use project::Projection;

/// Uvs closer than this count as one place.
const SAME_UV: f32 = 1e-5;

/// Faces flattened together: its faces, and its corners (half-edges)
/// joined into uv vertices.
#[derive(Clone, Debug, Default)]
pub(crate) struct Chart {
    pub faces: Vec<u32>,
    /// Each corner of its faces and the uv vertex it's in.
    pub corners: Vec<(u32, usize)>,
    /// How many uv vertices there are.
    pub vertices: usize,
}

impl Chart {
    /// Its corners' half-edges.
    pub fn half_edges(&self) -> impl Iterator<Item = u32> + '_ {
        self.corners.iter().map(|&(corner, _)| corner)
    }
}

/// How bent a face is in uv space: `angle`, how far its corners' angles
/// moved (0 none, 1 as far as they were big), and `area`, how much its
/// share of the uv area differs from its share of the surface (0 the same,
/// near 1 squashed to nothing or blown up).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Stretch {
    pub angle: f32,
    pub area: f32,
}

/// Which way Align lines picked uvs up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlignAxis {
    /// The same u: a line down the texture.
    U,
    /// The same v: a line across it.
    V,
}

/// `faces` joined into charts across the edges `joined` accepts (a
/// half-edge and its twin, both in `faces`), in order of their first face.
pub(crate) fn charts(
    mesh: &EditMesh,
    faces: &[u32],
    joined: impl Fn(u32, u32) -> bool,
) -> Vec<Chart> {
    let mut in_set = vec![false; mesh.face_count()];
    for &face in faces {
        if let Some(slot) = in_set.get_mut(face as usize) {
            *slot = true;
        }
    }
    let edges = mesh.half_edges();
    let mut face_root = UnionFind::new(mesh.face_count());
    let mut corner_root = UnionFind::new(edges.len());
    for (index, edge) in edges.iter().enumerate() {
        let index = index as u32;
        let Some(twin) = edge.twin else {
            continue;
        };
        if index > twin
            || !in_set[edge.face as usize]
            || !in_set[edges[twin as usize].face as usize]
            || !joined(index, twin)
        {
            continue;
        }
        face_root.join(edge.face, edges[twin as usize].face);
        // The twin runs back: its next corner is at this one's vertex.
        corner_root.join(index, edges[twin as usize].next);
        corner_root.join(edge.next, twin);
    }
    let mut by_root: BTreeMap<u32, usize> = BTreeMap::new();
    let mut charts: Vec<Chart> = Vec::new();
    let mut seen = BTreeSet::new();
    for &face in faces {
        if face as usize >= mesh.face_count() || !seen.insert(face) {
            continue;
        }
        let root = face_root.root(face);
        let chart = *by_root.entry(root).or_insert_with(|| {
            charts.push(Chart::default());
            charts.len() - 1
        });
        charts[chart].faces.push(face);
    }
    for chart in &mut charts {
        let mut vertex_of: BTreeMap<u32, usize> = BTreeMap::new();
        for &face in &chart.faces {
            for corner in mesh.face_half_edges(face) {
                let root = corner_root.root(corner);
                let next = vertex_of.len();
                let vertex = *vertex_of.entry(root).or_insert(next);
                chart.corners.push((corner, vertex));
            }
        }
        chart.vertices = vertex_of.len();
    }
    charts
}

/// `faces` in charts cut at seams, as Unwrap flattens them.
pub(crate) fn seam_charts(mesh: &EditMesh, faces: &[u32]) -> Vec<Chart> {
    charts(mesh, faces, |edge, _| {
        let (a, b) = mesh.ends(edge);
        !mesh.is_seam(a, b)
    })
}

/// `faces` in islands: charts cut wherever the uvs in `channel` on the two
/// sides of an edge differ.
pub(crate) fn uv_charts(mesh: &EditMesh, faces: &[u32], channel: usize) -> Vec<Chart> {
    charts(mesh, faces, |edge, twin| {
        let next = mesh.half_edge(edge).next;
        let twin_next = mesh.half_edge(twin).next;
        mesh.uv(edge, channel).distance(mesh.uv(twin_next, channel)) < SAME_UV
            && mesh.uv(next, channel).distance(mesh.uv(twin, channel)) < SAME_UV
    })
}

/// The uv islands of `faces` in `channel`, each as its faces.
pub fn uv_islands(mesh: &EditMesh, faces: &[u32], channel: usize) -> Vec<Vec<u32>> {
    uv_charts(mesh, faces, channel)
        .into_iter()
        .map(|chart| chart.faces)
        .collect()
}

/// Unwrap (U): `faces` cut open at the seams and each piece flattened by
/// least squares conformal maps, which keeps angles as close as it can.
/// Pinned vertices keep their uvs; pieces with none are then packed into
/// the texture with `margin` between them. The new uvs go in `channel`.
pub fn unwrap(
    mesh: &mut EditMesh,
    faces: &[u32],
    margin: f32,
    channel: usize,
) -> Result<(), String> {
    let charts = seam_charts(mesh, faces);
    if charts.is_empty() {
        return Err("Pick faces to unwrap".to_owned());
    }
    let mut free = Vec::new();
    for chart in charts {
        let pinned = pinned(mesh, &chart, channel);
        let guess = lscm::plane_guess(mesh, &chart, None);
        let Some(uvs) = lscm::flatten(mesh, &chart, &pinned, &guess) else {
            continue;
        };
        write(mesh, &chart, &uvs, channel);
        if pinned.is_empty() {
            free.push(chart);
        }
    }
    pack::pack(mesh, &free, margin, true, channel);
    Ok(())
}

/// Smart UV Project: `faces` grouped by which way they face (a new group
/// for each face more than `angle` degrees from every group so far), each
/// group's connected pieces flattened (by LSCM, from their projection, so
/// slanted faces aren't squashed), scaled alike and packed with `margin`.
pub fn smart_project(
    mesh: &mut EditMesh,
    faces: &[u32],
    angle: f32,
    margin: f32,
    channel: usize,
) -> Result<(), String> {
    let charts = project::smart_charts(mesh, faces, angle.to_radians());
    if charts.is_empty() {
        return Err("Pick faces to project".to_owned());
    }
    for (chart, direction) in &charts {
        let guess = lscm::plane_guess(mesh, chart, Some(*direction));
        let uvs = lscm::flatten(mesh, chart, &[], &guess)
            .filter(|uvs| !lscm::flips(mesh, chart, uvs))
            .unwrap_or(guess);
        write(mesh, chart, &uvs, channel);
    }
    let charts: Vec<Chart> = charts.into_iter().map(|(chart, _)| chart).collect();
    average_scale(mesh, &charts, channel);
    pack::pack(mesh, &charts, margin, true, channel);
    Ok(())
}

/// Cube, cylinder, sphere or view projection of `faces` into `channel`.
pub fn project(
    mesh: &mut EditMesh,
    faces: &[u32],
    projection: Projection,
    channel: usize,
) -> Result<(), String> {
    if faces.is_empty() {
        return Err("Pick faces to project".to_owned());
    }
    project::project(mesh, faces, projection, channel);
    Ok(())
}

/// Pack Islands: the islands of `faces` turned (with `rotate`) to fit
/// their smallest boxes, scaled alike, and laid out in the texture with
/// `margin` (a share of its width) between them. Islands with a pinned
/// vertex stay where they are.
pub fn pack_islands(
    mesh: &mut EditMesh,
    faces: &[u32],
    margin: f32,
    rotate: bool,
    channel: usize,
) -> Result<(), String> {
    let charts: Vec<Chart> = uv_charts(mesh, faces, channel)
        .into_iter()
        .filter(|chart| {
            !chart
                .half_edges()
                .any(|corner| mesh.pins().contains(&mesh.half_edge(corner).vertex))
        })
        .collect();
    if charts.is_empty() {
        return Err("No unpinned islands to pack".to_owned());
    }
    pack::pack(mesh, &charts, margin, rotate, channel);
    Ok(())
}

/// Average Island Scale: each island of `faces` scaled round its middle so
/// a stud of surface takes the same room in the texture on all of them.
pub fn average_island_scale(
    mesh: &mut EditMesh,
    faces: &[u32],
    channel: usize,
) -> Result<(), String> {
    let charts = uv_charts(mesh, faces, channel);
    if charts.is_empty() {
        return Err("Pick faces to scale".to_owned());
    }
    average_scale(mesh, &charts, channel);
    Ok(())
}

fn average_scale(mesh: &mut EditMesh, charts: &[Chart], channel: usize) {
    let areas: Vec<(f32, f32)> = charts
        .iter()
        .map(|chart| {
            chart.faces.iter().fold((0.0, 0.0), |(surface, uv), &face| {
                let (s, u) = face_areas(mesh, face, channel);
                (surface + s, uv + u)
            })
        })
        .collect();
    let (surface, uv) = areas
        .iter()
        .fold((0.0, 0.0), |(s, u), &(cs, cu)| (s + cs, u + cu));
    if surface <= 0.0 || uv <= 0.0 {
        return;
    }
    let ratio = uv / surface;
    for (chart, &(surface, uv)) in charts.iter().zip(&areas) {
        if surface <= 0.0 || uv <= 0.0 {
            continue;
        }
        let scale = (ratio * surface / uv).sqrt();
        let (min, max) = bounds(mesh, chart, channel);
        let middle = (min + max) / 2.0;
        let matrix = Affine2::from_translation(middle)
            * Affine2::from_scale(Vec2::splat(scale))
            * Affine2::from_translation(-middle);
        for corner in chart.half_edges() {
            let uv = mesh.uv(corner, channel);
            mesh.set_uv(corner, channel, matrix.transform_point2(uv));
        }
    }
}

/// How bent each of `faces` is in `channel` ([`Stretch`]), one each.
pub fn stretch(mesh: &EditMesh, faces: &[u32], channel: usize) -> Vec<Stretch> {
    let areas: Vec<(f32, f32)> = faces
        .iter()
        .map(|&face| face_areas(mesh, face, channel))
        .collect();
    let (surface, uv) = areas
        .iter()
        .fold((0.0, 0.0), |(s, u), &(fs, fu)| (s + fs, u + fu));
    let ratio = if surface > 0.0 { uv / surface } else { 0.0 };
    faces
        .iter()
        .zip(&areas)
        .map(|(&face, &(surface, uv))| {
            let area = if surface <= 0.0 || ratio <= 0.0 {
                0.0
            } else {
                let share = uv / surface / ratio;
                1.0 - share.min(1.0 / share).clamp(0.0, 1.0)
            };
            Stretch {
                angle: angle_stretch(mesh, face, channel),
                area,
            }
        })
        .collect()
}

/// The surface area of `face` and the area its uvs cover.
fn face_areas(mesh: &EditMesh, face: u32, channel: usize) -> (f32, f32) {
    mesh.triangles(face)
        .into_iter()
        .fold((0.0, 0.0), |(surface, uv), [a, b, c]| {
            let p = |e: u32| mesh.position(mesh.half_edge(e).vertex);
            let (pa, pb, pc) = (p(a), p(b), p(c));
            let (ua, ub, uc) = (
                mesh.uv(a, channel),
                mesh.uv(b, channel),
                mesh.uv(c, channel),
            );
            (
                surface + (pb - pa).cross(pc - pa).length() / 2.0,
                uv + ((ub - ua).perp_dot(uc - ua) / 2.0).abs(),
            )
        })
}

/// How far `face`'s corner angles moved in uv space, as a share of what
/// they were, on average.
fn angle_stretch(mesh: &EditMesh, face: u32, channel: usize) -> f32 {
    let corners = mesh.face_half_edges(face);
    let count = corners.len();
    let mut total = 0.0;
    for k in 0..count {
        let (prev, here, next) = (
            corners[(k + count - 1) % count],
            corners[k],
            corners[(k + 1) % count],
        );
        let p = |e: u32| mesh.position(mesh.half_edge(e).vertex);
        let surface = (p(next) - p(here)).angle_between(p(prev) - p(here));
        let uv = (mesh.uv(next, channel) - mesh.uv(here, channel))
            .angle_to(mesh.uv(prev, channel) - mesh.uv(here, channel))
            .abs();
        if surface.is_finite() && surface > 1e-4 && uv.is_finite() {
            total += ((uv - surface) / surface).abs().min(1.0);
        } else {
            total += 1.0;
        }
    }
    total / count as f32
}

/// The pairs of islands of `faces` in `channel` whose uvs overlap (by more
/// than touching), each as indices into [`uv_islands`]' list.
pub fn overlapping_islands(mesh: &EditMesh, faces: &[u32], channel: usize) -> Vec<[usize; 2]> {
    let charts = uv_charts(mesh, faces, channel);
    let triangles: Vec<Vec<[Vec2; 3]>> = charts
        .iter()
        .map(|chart| {
            chart
                .faces
                .iter()
                .flat_map(|&face| mesh.triangles(face))
                .map(|t| t.map(|e| mesh.uv(e, channel)))
                .collect()
        })
        .collect();
    let boxes: Vec<(Vec2, Vec2)> = charts
        .iter()
        .map(|chart| bounds(mesh, chart, channel))
        .collect();
    let mut pairs = Vec::new();
    for i in 0..charts.len() {
        for j in i + 1..charts.len() {
            let (a, b) = (boxes[i], boxes[j]);
            if a.1.x <= b.0.x || b.1.x <= a.0.x || a.1.y <= b.0.y || b.1.y <= a.0.y {
                continue;
            }
            let overlap = triangles[i]
                .iter()
                .any(|s| triangles[j].iter().any(|t| triangles_overlap(s, t)));
            if overlap {
                pairs.push([i, j]);
            }
        }
    }
    pairs
}

/// True if two triangles' insides overlap: no edge of either separates
/// them by more than a hair.
pub(crate) fn triangles_overlap(a: &[Vec2; 3], b: &[Vec2; 3]) -> bool {
    const TOUCH: f32 = 1e-6;
    for triangle in [a, b] {
        for k in 0..3 {
            let edge = triangle[(k + 1) % 3] - triangle[k];
            let axis = edge.perp();
            if axis.length_squared() < 1e-20 {
                continue;
            }
            let axis = axis.normalize();
            let range = |t: &[Vec2; 3]| {
                t.iter()
                    .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), p| {
                        let d = p.dot(axis);
                        (lo.min(d), hi.max(d))
                    })
            };
            let (a_lo, a_hi) = range(a);
            let (b_lo, b_hi) = range(b);
            if a_hi <= b_lo + TOUCH || b_hi <= a_lo + TOUCH {
                return false;
            }
        }
    }
    true
}

/// The uv vertex `corner` is in, in the UV editor's sense: every corner at
/// the same vertex with the same uv.
pub fn uv_vertex(mesh: &EditMesh, corner: u32, channel: usize) -> Vec<u32> {
    let edge = mesh.half_edge(corner);
    let uv = mesh.uv(corner, channel);
    (0..mesh.half_edges().len() as u32)
        .filter(|&other| {
            mesh.half_edge(other).vertex == edge.vertex
                && mesh.uv(other, channel).distance(uv) < SAME_UV
        })
        .collect()
}

/// Moves the uvs of `corners` in `channel` by `matrix`.
pub fn transform_uvs(mesh: &mut EditMesh, corners: &[u32], matrix: Affine2, channel: usize) {
    for &corner in corners {
        let uv = mesh.uv(corner, channel);
        mesh.set_uv(corner, channel, matrix.transform_point2(uv));
    }
}

/// Stitch (V): where `corners` include corners of one vertex with
/// different uvs (the two sides of a cut), all of them go to their
/// middle. Returns how many vertices were stitched.
pub fn stitch(mesh: &mut EditMesh, corners: &[u32], channel: usize) -> usize {
    let mut by_vertex: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for &corner in corners {
        by_vertex
            .entry(mesh.half_edge(corner).vertex)
            .or_default()
            .push(corner);
    }
    let mut stitched = 0;
    for corners in by_vertex.values() {
        let uvs: Vec<Vec2> = corners.iter().map(|&c| mesh.uv(c, channel)).collect();
        if uvs.iter().all(|uv| uv.distance(uvs[0]) < SAME_UV) {
            continue;
        }
        let middle = uvs.iter().sum::<Vec2>() / uvs.len() as f32;
        for &corner in corners {
            mesh.set_uv(corner, channel, middle);
        }
        stitched += 1;
    }
    stitched
}

/// Align: the uvs of `corners` lined up on their average u (or v).
pub fn align(mesh: &mut EditMesh, corners: &[u32], axis: AlignAxis, channel: usize) {
    if corners.is_empty() {
        return;
    }
    let middle = corners.iter().map(|&c| mesh.uv(c, channel)).sum::<Vec2>() / corners.len() as f32;
    for &corner in corners {
        let mut uv = mesh.uv(corner, channel);
        match axis {
            AlignAxis::U => uv.x = middle.x,
            AlignAxis::V => uv.y = middle.y,
        }
        mesh.set_uv(corner, channel, uv);
    }
}

/// The pinned uv vertices of `chart`, with their uvs as the flattening
/// works with them (v up).
fn pinned(mesh: &EditMesh, chart: &Chart, channel: usize) -> Vec<(usize, Vec2)> {
    let mut pinned: BTreeMap<usize, Vec2> = BTreeMap::new();
    for &(corner, vertex) in &chart.corners {
        if mesh.pins().contains(&mesh.half_edge(corner).vertex) {
            pinned
                .entry(vertex)
                .or_insert_with(|| flip(mesh.uv(corner, channel)));
        }
    }
    pinned.into_iter().collect()
}

/// Writes `uvs` (one per uv vertex of `chart`, v up) to its corners.
fn write(mesh: &mut EditMesh, chart: &Chart, uvs: &[Vec2], channel: usize) {
    for &(corner, vertex) in &chart.corners {
        mesh.set_uv(corner, channel, flip(uvs[vertex]));
    }
}

/// A uv with v turned over: between glTF's (v down) and the flattening's
/// (v up). Its own inverse.
fn flip(uv: Vec2) -> Vec2 {
    Vec2::new(uv.x, -uv.y)
}

/// The box round `chart`'s uvs in `channel`.
fn bounds(mesh: &EditMesh, chart: &Chart, channel: usize) -> (Vec2, Vec2) {
    chart
        .half_edges()
        .map(|corner| mesh.uv(corner, channel))
        .fold((Vec2::INFINITY, Vec2::NEG_INFINITY), |(min, max), uv| {
            (min.min(uv), max.max(uv))
        })
}

/// The 3D positions of `chart`'s uv vertices (from any of their corners).
fn chart_positions(mesh: &EditMesh, chart: &Chart) -> Vec<Vec3> {
    let mut positions = vec![Vec3::ZERO; chart.vertices];
    for &(corner, vertex) in &chart.corners {
        positions[vertex] = mesh.position(mesh.half_edge(corner).vertex);
    }
    positions
}

/// What a view projection's matrix turns a model point into: its place on
/// screen, x right and y up, -1 to 1.
pub(crate) fn on_screen(view: Mat4, point: Vec3) -> Vec2 {
    let clip = view * point.extend(1.0);
    let w = if clip.w.abs() > 1e-9 { clip.w } else { 1e-9 };
    Vec2::new(clip.x / w, clip.y / w)
}

/// A union-find over numbers, joined by [`join`](Self::join).
struct UnionFind {
    parent: Vec<u32>,
}

impl UnionFind {
    fn new(count: usize) -> UnionFind {
        UnionFind {
            parent: (0..count as u32).collect(),
        }
    }

    fn root(&mut self, mut at: u32) -> u32 {
        while self.parent[at as usize] != at {
            let up = self.parent[self.parent[at as usize] as usize];
            self.parent[at as usize] = up;
            at = up;
        }
        at
    }

    fn join(&mut self, a: u32, b: u32) {
        let (a, b) = (self.root(a), self.root(b));
        // The smaller root wins, so the result doesn't depend on order.
        let (low, high) = (a.min(b), a.max(b));
        self.parent[high as usize] = low;
    }
}
