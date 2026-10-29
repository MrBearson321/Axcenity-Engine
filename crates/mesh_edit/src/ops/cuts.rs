//! Cutting faces into more: loop cut (Ctrl+R), edge slide (G G), the
//! knife (K) and subdivide.

use std::collections::{BTreeSet, HashMap, HashSet};

use glam::{Vec2, Vec3};

use super::{Input, KnifePoint, Picked, Soup, SoupFace, key};
use crate::half_edge::EditMesh;

/// A ring of quads to cut across: each rung (an edge the cut crosses) as
/// (where its cuts are measured from, the other end), and each quad with
/// the corner its first rung starts at.
struct Ring {
    rungs: Vec<(u32, u32)>,
    quads: Vec<(usize, usize)>,
}

/// The ring of quads the edge `a`–`b` crosses, both ways from it, stopping
/// at a face that isn't a quad or an open border.
fn ring(soup: &Soup, a: u32, b: u32) -> Option<Ring> {
    let directed = soup.directed();
    let quad = |face: usize| soup.faces[face].corners.len() == 4;
    let corner = |face: usize, k: usize| soup.faces[face].corners[k % 4];
    let mut rungs = vec![(a, b)];
    let mut quads = Vec::new();
    let mut seen = HashSet::new();
    // Forward: in each quad from the edge (x→y) measured from x, the far
    // edge (z→w) is measured from w; the next quad has w→z.
    let mut at = directed.get(&(a, b)).copied();
    let mut closed = false;
    while let Some((face, k)) = at {
        if !quad(face) || !seen.insert(face) {
            break;
        }
        quads.push((face, k));
        let (z, w) = (corner(face, k + 2), corner(face, k + 3));
        if (w, z) == rungs[0] {
            closed = true;
            break;
        }
        rungs.push((w, z));
        at = directed.get(&(w, z)).copied();
    }
    if !closed {
        // Backward: from b→a (measured from its end, a), the far edge
        // (r→s) is measured from r; the next quad has s→r.
        let mut back_rungs = Vec::new();
        let mut back_quads = Vec::new();
        let mut at = directed.get(&(b, a)).copied();
        while let Some((face, k)) = at {
            if !quad(face) || !seen.insert(face) {
                break;
            }
            back_quads.push((face, k));
            let (r, s) = (corner(face, k + 2), corner(face, k + 3));
            back_rungs.push((r, s));
            at = directed.get(&(s, r)).copied();
        }
        back_rungs.reverse();
        back_quads.reverse();
        back_rungs.extend(rungs);
        back_quads.extend(quads);
        rungs = back_rungs;
        quads = back_quads;
    }
    (!quads.is_empty()).then_some(Ring { rungs, quads })
}

/// Ctrl+R: `cuts` loops across the ring of quads the edge `edge` crosses,
/// evenly spaced and slid by `slide` (−1 to 1).
pub(super) fn loop_cut(
    soup: &mut Soup,
    edge: [u32; 2],
    cuts: u32,
    slide: f32,
) -> Result<Picked, String> {
    let [a, b] = edge;
    let Some(ring) = ring(soup, a, b) else {
        return Err("Loop Cut needs an edge between quads".to_owned());
    };
    let cuts = cuts.clamp(1, 128) as usize;
    let slide = slide.clamp(-0.99, 0.99);
    let fractions: Vec<f32> = (1..=cuts)
        .map(|k| (k as f32 + slide) / (cuts + 1) as f32)
        .collect();
    // The points on each rung, from its smaller vertex.
    let mut points: HashMap<[u32; 2], Vec<u32>> = HashMap::new();
    for &(from, to) in &ring.rungs {
        let (p, q) = (soup.position(from), soup.position(to));
        let mut ids: Vec<u32> = fractions.iter().map(|&t| soup.add(p.lerp(q, t))).collect();
        if from > to {
            ids.reverse();
        }
        points.insert(key(from, to), ids);
    }
    let along = |points: &HashMap<[u32; 2], Vec<u32>>, from: u32, to: u32| {
        let mut ids = points[&key(from, to)].clone();
        if from > to {
            ids.reverse();
        }
        ids
    };
    let mut slices = Vec::new();
    let ring_faces: HashSet<usize> = ring.quads.iter().map(|&(face, _)| face).collect();
    let mut picked = Picked::default();
    for &(face, k) in &ring.quads {
        let polygon = soup.faces[face].clone();
        let c = |i: usize| polygon.corners[(k + i) % 4];
        let uv = |i: usize| polygon.uvs[(k + i) % 4];
        // Along the first rung from c0 to c1, and the far one from c3 to c2.
        let mut side_a = vec![c(0)];
        side_a.extend(along(&points, c(0), c(1)));
        side_a.push(c(1));
        let mut side_b = vec![c(3)];
        side_b.extend(along(&points, c(3), c(2)));
        side_b.push(c(2));
        let uv_a = |j: usize| uv(0).lerp(uv(1), j as f32 / (cuts + 1) as f32);
        let uv_b = |j: usize| uv(3).lerp(uv(2), j as f32 / (cuts + 1) as f32);
        for j in 0..=cuts {
            slices.push(SoupFace::like(
                &polygon,
                vec![side_a[j], side_a[j + 1], side_b[j + 1], side_b[j]],
                vec![uv_a(j), uv_a(j + 1), uv_b(j + 1), uv_b(j)],
            ));
        }
        for j in 1..=cuts {
            picked.edges.push([side_a[j], side_b[j]]);
        }
    }
    soup.insert_edge_points(&points, &ring_faces);
    let mut index = 0;
    soup.faces.retain(|_| {
        index += 1;
        !ring_faces.contains(&(index - 1))
    });
    soup.faces.extend(slices);
    Ok(picked)
}

/// G G: each picked edge's ends slid along the edges beside them, toward
/// the faces on one side (`factor` > 0) or the other.
pub(super) fn edge_slide(mesh: &mut EditMesh, input: &Input, factor: f32) -> Result<(), String> {
    if input.edges.is_empty() {
        return Err("Edge Slide needs edges picked".to_owned());
    }
    let factor = factor.clamp(-1.0, 1.0);
    let soup = Soup::of(mesh);
    let directed = soup.directed();
    let corner = |face: usize, k: isize| {
        let corners = &soup.faces[face].corners;
        corners[k.rem_euclid(corners.len() as isize) as usize]
    };
    // The picked edges in chains, each run one way so "left" is the same
    // side all along.
    let mut at_vertex: HashMap<u32, Vec<[u32; 2]>> = HashMap::new();
    for &edge in &input.edges {
        for v in edge {
            at_vertex.entry(v).or_default().push(edge);
        }
    }
    let mut oriented: Vec<(u32, u32)> = Vec::new();
    let mut done: HashSet<[u32; 2]> = HashSet::new();
    for &edge in &input.edges {
        if done.contains(&edge) {
            continue;
        }
        // Walk back to an end of the chain (or round a loop), then forward.
        let mut start = edge[0];
        let mut came = edge;
        let mut steps = 0;
        while let Some(&prev) = at_vertex[&start].iter().find(|&&e| e != came) {
            start = if prev[0] == start { prev[1] } else { prev[0] };
            came = prev;
            steps += 1;
            if start == edge[0] || steps > input.edges.len() {
                break;
            }
        }
        let mut from = start;
        let mut next = at_vertex[&start]
            .iter()
            .copied()
            .find(|e| !done.contains(e));
        while let Some(e) = next {
            done.insert(e);
            let to = if e[0] == from { e[1] } else { e[0] };
            oriented.push((from, to));
            from = to;
            next = at_vertex[&from].iter().copied().find(|e| !done.contains(e));
        }
    }
    // Each end's rail on each side: the face to the left of a→b has
    // a→b; to the right, b→a.
    let mut left: HashMap<u32, u32> = HashMap::new();
    let mut right: HashMap<u32, u32> = HashMap::new();
    let picked: HashSet<u32> = input.edges.iter().flatten().copied().collect();
    let rail = |side: &mut HashMap<u32, u32>, vertex: u32, other: u32| {
        if !input.edges.contains(&key(vertex, other)) {
            side.entry(vertex).or_insert(other);
        }
    };
    for &(a, b) in &oriented {
        if let Some(&(face, k)) = directed.get(&(a, b)) {
            let k = k as isize;
            rail(&mut left, a, corner(face, k - 1));
            rail(&mut left, b, corner(face, k + 2));
        }
        if let Some(&(face, k)) = directed.get(&(b, a)) {
            let k = k as isize;
            rail(&mut right, a, corner(face, k + 2));
            rail(&mut right, b, corner(face, k - 1));
        }
    }
    let side = if factor >= 0.0 { &left } else { &right };
    let mut vertices: Vec<u32> = picked.iter().copied().collect();
    vertices.sort_unstable();
    let moved: Vec<(u32, Vec3)> = vertices
        .iter()
        .filter_map(|&v| {
            let to = *side.get(&v)?;
            Some((v, mesh.position(v).lerp(mesh.position(to), factor.abs())))
        })
        .collect();
    let (which, positions): (Vec<u32>, Vec<Vec3>) = moved.into_iter().unzip();
    mesh.set_positions(&which, &positions);
    Ok(())
}

/// K: the faces cut along the line through `points`, each pair in a row
/// joined across the face they share.
pub(super) fn knife(soup: &mut Soup, points: &[KnifePoint]) -> Result<Picked, String> {
    // The points on each edge, by how far along from its smaller end.
    let mut on_edges: HashMap<[u32; 2], Vec<(f32, u32)>> = HashMap::new();
    let mut path = Vec::new();
    for point in points {
        let (a, b) = (point.a, point.b);
        if a as usize >= soup.positions.len() || b as usize >= soup.positions.len() {
            continue;
        }
        let id = if a == b || point.t <= 1e-4 {
            a
        } else if point.t >= 1.0 - 1e-4 {
            b
        } else {
            let id = soup.add(soup.position(a).lerp(soup.position(b), point.t));
            let from_small = if a < b { point.t } else { 1.0 - point.t };
            on_edges
                .entry(key(a, b))
                .or_default()
                .push((from_small, id));
            id
        };
        if path.last() != Some(&id) {
            path.push(id);
        }
    }
    let edges: HashMap<[u32; 2], Vec<u32>> = on_edges
        .into_iter()
        .map(|(edge, mut along)| {
            along.sort_by(|x, y| x.0.total_cmp(&y.0));
            (edge, along.into_iter().map(|(_, id)| id).collect())
        })
        .collect();
    soup.insert_edge_points(&edges, &HashSet::new());
    let mut picked = Picked::default();
    for pair in path.windows(2) {
        let (u, w) = (pair[0], pair[1]);
        let found = soup.faces.iter().enumerate().find_map(|(index, face)| {
            let iu = face.corners.iter().position(|&c| c == u)?;
            let iw = face.corners.iter().position(|&c| c == w)?;
            let count = face.corners.len();
            let apart = (iu + count - iw) % count;
            (apart != 1 && apart != count - 1).then_some((index, iu, iw))
        });
        let Some((index, iu, iw)) = found else {
            continue;
        };
        let face = soup.faces[index].clone();
        let count = face.corners.len();
        let piece = |from: usize, to: usize| {
            let mut k = from;
            let (mut corners, mut uvs) = (Vec::new(), Vec::new());
            loop {
                corners.push(face.corners[k]);
                uvs.push(face.uvs[k]);
                if k == to {
                    break;
                }
                k = (k + 1) % count;
            }
            SoupFace::like(&face, corners, uvs)
        };
        soup.faces[index] = piece(iu, iw);
        soup.faces.push(piece(iw, iu));
        picked.edges.push([u, w]);
    }
    if picked.edges.is_empty() {
        return Err("The knife's line didn't cross a face".to_owned());
    }
    Ok(picked)
}

/// The picked faces cut into smaller ones, each of their edges in
/// `cuts + 1`: quads into a grid, triangles into triangles, and bigger
/// faces into a fan round their middle. Faces next door take the new
/// points along their shared edges.
pub(super) fn subdivide(soup: &mut Soup, input: &Input, cuts: u32) -> Result<Picked, String> {
    let cuts = cuts.clamp(1, 64) as usize;
    let n = cuts + 1;
    let edges: BTreeSet<[u32; 2]> = if input.faces.is_empty() {
        input.edges.clone()
    } else {
        input
            .faces
            .iter()
            .flat_map(|&f| {
                let face = &soup.faces[f];
                (0..face.corners.len()).map(|k| {
                    let (a, b) = face.edge(k);
                    key(a, b)
                })
            })
            .collect()
    };
    if edges.is_empty() {
        return Err("Subdivide needs faces or edges picked".to_owned());
    }
    let mut points: HashMap<[u32; 2], Vec<u32>> = HashMap::new();
    for &[a, b] in &edges {
        let (p, q) = (soup.position(a), soup.position(b));
        let ids = (1..n)
            .map(|k| soup.add(p.lerp(q, k as f32 / n as f32)))
            .collect();
        points.insert([a, b], ids);
    }
    let along = |from: u32, to: u32| {
        let mut ids = points[&key(from, to)].clone();
        if from > to {
            ids.reverse();
        }
        ids
    };
    let mut new_faces = Vec::new();
    for &f in &input.faces {
        let face = soup.faces[f].clone();
        let c = face.corners.clone();
        let count = c.len();
        let sides: Vec<Vec<u32>> = (0..count)
            .map(|i| along(c[i], c[(i + 1) % count]))
            .collect();
        let pos = |soup: &Soup, v: u32| soup.position(v);
        match count {
            4 => {
                // grid[i][j]: i along c0→c1, j along c0→c3.
                let mut grid = vec![vec![u32::MAX; n + 1]; n + 1];
                grid[0][0] = c[0];
                grid[n][0] = c[1];
                grid[n][n] = c[2];
                grid[0][n] = c[3];
                for k in 1..n {
                    grid[k][0] = sides[0][k - 1];
                    grid[n][k] = sides[1][k - 1];
                    grid[n - k][n] = sides[2][k - 1];
                    grid[0][n - k] = sides[3][k - 1];
                }
                let corners = [0, 1, 2, 3].map(|i| pos(soup, c[i]));
                let bilinear = |u: f32, v: f32, at: [Vec3; 4]| {
                    at[0].lerp(at[1], u).lerp(at[3].lerp(at[2], u), v)
                };
                for (i, row) in grid.iter_mut().enumerate().take(n).skip(1) {
                    for (j, cell) in row.iter_mut().enumerate().take(n).skip(1) {
                        *cell =
                            soup.add(bilinear(i as f32 / n as f32, j as f32 / n as f32, corners));
                    }
                }
                let uv_corners = [0, 1, 2, 3].map(|i| face.uvs[i]);
                let uv = |i: usize, j: usize| {
                    let (u, v) = (i as f32 / n as f32, j as f32 / n as f32);
                    uv_corners[0]
                        .lerp(uv_corners[1], u)
                        .lerp(uv_corners[3].lerp(uv_corners[2], u), v)
                };
                for i in 0..n {
                    for j in 0..n {
                        new_faces.push(SoupFace::like(
                            &face,
                            vec![
                                grid[i][j],
                                grid[i + 1][j],
                                grid[i + 1][j + 1],
                                grid[i][j + 1],
                            ],
                            vec![uv(i, j), uv(i + 1, j), uv(i + 1, j + 1), uv(i, j + 1)],
                        ));
                    }
                }
            }
            3 => {
                // tri[i][j] = c0 + (c1−c0)·i/n + (c2−c0)·j/n, i + j ≤ n.
                let mut tri = vec![vec![u32::MAX; n + 1]; n + 1];
                tri[0][0] = c[0];
                tri[n][0] = c[1];
                tri[0][n] = c[2];
                for k in 1..n {
                    tri[k][0] = sides[0][k - 1];
                    tri[n - k][k] = sides[1][k - 1];
                    tri[0][n - k] = sides[2][k - 1];
                }
                let [a, b, cc] = [0, 1, 2].map(|i| pos(soup, c[i]));
                for (i, row) in tri.iter_mut().enumerate().take(n).skip(1) {
                    for (j, cell) in row.iter_mut().enumerate().take(n - i).skip(1) {
                        let (u, v) = (i as f32 / n as f32, j as f32 / n as f32);
                        *cell = soup.add(a + (b - a) * u + (cc - a) * v);
                    }
                }
                let [ua, ub, uc] = [0, 1, 2].map(|i| face.uvs[i]);
                let uv = |i: usize, j: usize| {
                    let (u, v) = (i as f32 / n as f32, j as f32 / n as f32);
                    ua + (ub - ua) * u + (uc - ua) * v
                };
                for i in 0..n {
                    for j in 0..n - i {
                        new_faces.push(SoupFace::like(
                            &face,
                            vec![tri[i][j], tri[i + 1][j], tri[i][j + 1]],
                            vec![uv(i, j), uv(i + 1, j), uv(i, j + 1)],
                        ));
                        if i + j + 2 <= n {
                            new_faces.push(SoupFace::like(
                                &face,
                                vec![tri[i + 1][j], tri[i + 1][j + 1], tri[i][j + 1]],
                                vec![uv(i + 1, j), uv(i + 1, j + 1), uv(i, j + 1)],
                            ));
                        }
                    }
                }
            }
            _ => {
                let middle = soup.add(soup.center(f));
                let middle_uv = face.uvs.iter().sum::<Vec2>() / count as f32;
                let uv_on = |i: usize, k: usize| {
                    face.uvs[i].lerp(face.uvs[(i + 1) % count], k as f32 / n as f32)
                };
                for i in 0..count {
                    let before = (i + count - 1) % count;
                    new_faces.push(SoupFace::like(
                        &face,
                        vec![sides[before][cuts - 1], c[i], sides[i][0], middle],
                        vec![uv_on(before, cuts), face.uvs[i], uv_on(i, 1), middle_uv],
                    ));
                    for k in 1..cuts {
                        new_faces.push(SoupFace::like(
                            &face,
                            vec![sides[i][k - 1], sides[i][k], middle],
                            vec![uv_on(i, k), uv_on(i, k + 1), middle_uv],
                        ));
                    }
                }
            }
        }
    }
    let replaced: HashSet<usize> = input.faces.iter().copied().collect();
    soup.insert_edge_points(&points, &replaced);
    let mut index = 0;
    soup.faces.retain(|_| {
        index += 1;
        !replaced.contains(&(index - 1))
    });
    let mut picked = Picked::default();
    let first = soup.faces.len();
    picked.faces.extend(first..first + new_faces.len());
    soup.faces.extend(new_faces);
    if input.faces.is_empty() {
        picked.vertices = points.values().flatten().copied().collect();
    }
    Ok(picked)
}
