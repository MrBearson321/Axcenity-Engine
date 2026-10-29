//! Joining and removing: merge (M), delete, dissolve, and filling holes:
//! fill (F), grid fill and bridge edge loops.

use std::collections::{BTreeSet, HashMap, HashSet};

use glam::{Vec2, Vec3};

use super::{DeleteKind, DissolveKind, Input, MergeAt, Picked, Soup, SoupFace, key};

/// M: the picked vertices joined into one at their middle or the cursor,
/// or (by distance) those closer than `distance` joined in groups.
pub(super) fn merge(
    soup: &mut Soup,
    input: &Input,
    at: MergeAt,
    cursor: Vec3,
    distance: f32,
) -> Result<Picked, String> {
    if input.vertices.len() < 2 && at != MergeAt::Distance {
        return Err("Merge needs two or more vertices picked".to_owned());
    }
    let kept = match at {
        MergeAt::Distance => {
            weld(soup, &input.vertices, distance.max(0.0));
            return Ok(Picked {
                vertices: input.vertices.clone(),
                ..Picked::default()
            });
        }
        MergeAt::Center => {
            input
                .vertices
                .iter()
                .map(|&v| soup.position(v))
                .sum::<Vec3>()
                / input.vertices.len() as f32
        }
        MergeAt::Cursor => cursor,
    };
    let into = *input.vertices.first().expect("checked above");
    soup.positions[into as usize] = kept;
    let to: HashMap<u32, u32> = input.vertices.iter().map(|&v| (v, into)).collect();
    remap(soup, &to);
    Ok(Picked {
        vertices: BTreeSet::from([into]),
        ..Picked::default()
    })
}

/// Joins the `vertices` closer together than `distance`, each group at its
/// middle, then takes out pairs of faces that end up back to back.
pub(super) fn weld(soup: &mut Soup, vertices: &BTreeSet<u32>, distance: f32) {
    // Sorted along X, each vertex only needs checking against those
    // within `distance` along it.
    let mut order: Vec<u32> = vertices.iter().copied().collect();
    order.sort_by(|&a, &b| soup.position(a).x.total_cmp(&soup.position(b).x));
    let mut parent: HashMap<u32, u32> = order.iter().map(|&v| (v, v)).collect();
    fn root(parent: &mut HashMap<u32, u32>, v: u32) -> u32 {
        let mut at = v;
        while parent[&at] != at {
            at = parent[&at];
        }
        parent.insert(v, at);
        at
    }
    for (i, &a) in order.iter().enumerate() {
        let pa = soup.position(a);
        for &b in &order[i + 1..] {
            let pb = soup.position(b);
            if pb.x - pa.x > distance {
                break;
            }
            if pa.distance(pb) <= distance {
                let (ra, rb) = (root(&mut parent, a), root(&mut parent, b));
                if ra != rb {
                    parent.insert(ra.max(rb), ra.min(rb));
                }
            }
        }
    }
    let mut groups: HashMap<u32, Vec<u32>> = HashMap::new();
    for &v in &order {
        let r = root(&mut parent, v);
        groups.entry(r).or_default().push(v);
    }
    let mut to = HashMap::new();
    for (into, group) in groups {
        if group.len() < 2 {
            continue;
        }
        let middle = group.iter().map(|&v| soup.position(v)).sum::<Vec3>() / group.len() as f32;
        soup.positions[into as usize] = middle;
        for v in group {
            to.insert(v, into);
        }
    }
    remap(soup, &to);
}

/// Each corner at a vertex in `to` moved to the vertex it maps to; then
/// faces that ended up back to back with another (the same corners the
/// other way round) are taken out, both of them.
fn remap(soup: &mut Soup, to: &HashMap<u32, u32>) {
    if to.is_empty() {
        return;
    }
    for face in &mut soup.faces {
        let mut changed = false;
        for corner in &mut face.corners {
            if let Some(&into) = to.get(corner) {
                changed |= into != *corner;
                *corner = into;
            }
        }
        if changed {
            face.normals.clear();
        }
    }
    let shape = |corners: &[u32]| {
        let mut cleaned: Vec<u32> = corners.to_vec();
        cleaned.dedup();
        while cleaned.len() > 1 && cleaned.first() == cleaned.last() {
            cleaned.pop();
        }
        // Started from the smallest corner, so the same cycle compares equal.
        let start = (0..cleaned.len()).min_by_key(|&k| cleaned[k]).unwrap_or(0);
        cleaned.rotate_left(start);
        cleaned
    };
    let mut by_shape: HashMap<Vec<u32>, Vec<usize>> = HashMap::new();
    for (index, face) in soup.faces.iter().enumerate() {
        by_shape
            .entry(shape(&face.corners))
            .or_default()
            .push(index);
    }
    let mut gone: HashSet<usize> = HashSet::new();
    for (index, face) in soup.faces.iter().enumerate() {
        if gone.contains(&index) {
            continue;
        }
        let mut reversed = face.corners.clone();
        reversed.reverse();
        let reversed = shape(&reversed);
        if reversed.len() < 3 {
            continue;
        }
        if let Some(other) = by_shape
            .get(&reversed)
            .and_then(|others| others.iter().find(|&&o| o != index && !gone.contains(&o)))
        {
            gone.insert(index);
            gone.insert(*other);
        }
    }
    drop_faces(soup, &gone);
}

/// Takes out the faces numbered in `gone`.
fn drop_faces(soup: &mut Soup, gone: &HashSet<usize>) {
    let mut index = 0;
    soup.faces.retain(|_| {
        index += 1;
        !gone.contains(&(index - 1))
    });
}

/// Delete: takes out the picked vertices, edges or faces.
pub(super) fn delete(soup: &mut Soup, input: &Input, kind: DeleteKind) -> Result<Picked, String> {
    let gone: HashSet<usize> = match kind {
        DeleteKind::Vertices => soup
            .faces
            .iter()
            .enumerate()
            .filter(|(_, face)| face.corners.iter().any(|c| input.vertices.contains(c)))
            .map(|(index, _)| index)
            .collect(),
        DeleteKind::Edges => soup
            .faces
            .iter()
            .enumerate()
            .filter(|(_, face)| {
                (0..face.corners.len()).any(|k| {
                    let (a, b) = face.edge(k);
                    input.edges.contains(&key(a, b))
                })
            })
            .map(|(index, _)| index)
            .collect(),
        DeleteKind::Faces | DeleteKind::OnlyFaces => input.faces.iter().copied().collect(),
    };
    if gone.is_empty() && (kind != DeleteKind::Vertices || input.vertices.is_empty()) {
        return Err("Nothing picked to delete".to_owned());
    }
    drop_faces(soup, &gone);
    Ok(Picked::default())
}

/// Dissolve: the picked faces joined into one per connected region; or
/// each picked edge taken out, joining the faces either side; or each
/// picked vertex taken out, joining the faces round it.
pub(super) fn dissolve(
    soup: &mut Soup,
    input: &Input,
    kind: DissolveKind,
) -> Result<Picked, String> {
    let edge_faces = soup.edge_faces();
    let groups: Vec<Vec<usize>> = match kind {
        DissolveKind::Faces => soup.components(&input.faces),
        DissolveKind::Edges | DissolveKind::Vertices => {
            // Faces joined across each picked edge (or each edge at a
            // picked vertex).
            let across: Vec<[u32; 2]> = match kind {
                DissolveKind::Edges => input.edges.iter().copied().collect(),
                _ => edge_faces
                    .keys()
                    .copied()
                    .filter(|edge| edge.iter().any(|v| input.vertices.contains(v)))
                    .collect(),
            };
            let mut parent: Vec<usize> = (0..soup.faces.len()).collect();
            fn root(parent: &mut [usize], f: usize) -> usize {
                let mut at = f;
                while parent[at] != at {
                    parent[at] = parent[parent[at]];
                    at = parent[at];
                }
                at
            }
            for edge in across {
                if let Some([a, b]) = edge_faces.get(&edge).map(Vec::as_slice) {
                    let (ra, rb) = (root(&mut parent, *a), root(&mut parent, *b));
                    parent[ra.max(rb)] = ra.min(rb);
                }
            }
            let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
            for face in 0..soup.faces.len() {
                let r = root(&mut parent, face);
                groups.entry(r).or_default().push(face);
            }
            let mut groups: Vec<Vec<usize>> =
                groups.into_values().filter(|g| g.len() > 1).collect();
            groups.sort();
            groups
        }
    };
    if groups.is_empty() {
        return Err("Dissolve needs something between faces picked".to_owned());
    }
    let mut picked = Picked::default();
    let mut gone = HashSet::new();
    for group in groups {
        if let Some(joined) = join_faces(soup, &group) {
            soup.faces[group[0]] = joined;
            picked.faces.insert(group[0]);
            gone.extend(group[1..].iter().copied());
        }
    }
    // Corners left between just two edges in a line of one face, where
    // vertices were dissolved or the ends of dissolved edges, go too.
    let loose: BTreeSet<u32> = match kind {
        DissolveKind::Faces => BTreeSet::new(),
        DissolveKind::Edges => input.edges.iter().flatten().copied().collect(),
        DissolveKind::Vertices => input.vertices.clone(),
    };
    let mut neighbours: HashMap<u32, BTreeSet<u32>> = HashMap::new();
    for (index, face) in soup.faces.iter().enumerate() {
        if gone.contains(&index) {
            continue;
        }
        for k in 0..face.corners.len() {
            let (a, b) = face.edge(k);
            neighbours.entry(a).or_default().insert(b);
            neighbours.entry(b).or_default().insert(a);
        }
    }
    for (index, face) in soup.faces.iter_mut().enumerate() {
        if gone.contains(&index) {
            continue;
        }
        let keep: Vec<usize> = (0..face.corners.len())
            .filter(|&k| {
                let c = face.corners[k];
                !(loose.contains(&c) && neighbours.get(&c).is_some_and(|n| n.len() == 2))
            })
            .collect();
        if keep.len() != face.corners.len() && keep.len() >= 3 {
            face.corners = keep.iter().map(|&k| face.corners[k]).collect();
            face.uvs = keep.iter().map(|&k| face.uvs[k]).collect();
            face.normals.clear();
        }
    }
    drop_faces(soup, &gone);
    // Picked faces moved down as earlier ones went.
    let shift = |face: usize| face - gone.iter().filter(|&&g| g < face).count();
    picked.faces = picked.faces.into_iter().map(shift).collect();
    Ok(picked)
}

/// The faces of `group` as one: the loop round its border. `None` if the
/// border isn't a single loop (a ring of faces, or faces only touching at
/// a corner).
fn join_faces(soup: &Soup, group: &[usize]) -> Option<SoupFace> {
    let members: HashSet<usize> = group.iter().copied().collect();
    let directed = soup.directed();
    let mut next: HashMap<u32, (u32, Vec2)> = HashMap::new();
    for &face in group {
        let polygon = &soup.faces[face];
        for k in 0..polygon.corners.len() {
            let (a, b) = polygon.edge(k);
            let inside = directed
                .get(&(b, a))
                .is_some_and(|(other, _)| members.contains(other));
            if !inside && next.insert(a, (b, polygon.uvs[k])).is_some() {
                return None;
            }
        }
    }
    let &start = next.keys().min()?;
    let (mut corners, mut uvs) = (Vec::new(), Vec::new());
    let mut at = start;
    loop {
        let &(to, uv) = next.get(&at)?;
        corners.push(at);
        uvs.push(uv);
        at = to;
        if at == start {
            break;
        }
        if corners.len() > next.len() {
            return None;
        }
    }
    (corners.len() == next.len()).then(|| SoupFace::like(&soup.faces[group[0]], corners, uvs))
}

/// F: a face across each picked loop of open-border edges.
pub(super) fn fill(soup: &mut Soup, input: &Input) -> Result<Picked, String> {
    let loops = soup.border_loops(&input.edges);
    if loops.is_empty() {
        return Err("Fill needs a closed loop of open-border edges picked".to_owned());
    }
    let uvs = soup.vertex_uvs();
    let like = like_border(soup, &loops[0]);
    let mut picked = Picked::default();
    for mut ring in loops {
        // The border's faces run one way; the new face runs the other.
        ring.reverse();
        let ring_uvs = ring
            .iter()
            .map(|v| uvs.get(v).copied().unwrap_or_default())
            .collect();
        picked.faces.insert(soup.faces.len());
        soup.faces.push(SoupFace::like(&like, ring, ring_uvs));
    }
    Ok(picked)
}

/// A face along the border `ring`, to draw new faces there like it.
fn like_border(soup: &Soup, ring: &[u32]) -> SoupFace {
    let directed = soup.directed();
    let face = directed
        .get(&(ring[0], ring[1]))
        .map_or(0, |&(face, _)| face);
    soup.faces[face].clone()
}

/// A grid of quads across one picked loop of open-border edges, which
/// needs an even number of edges: `span` quads along one side (0 picks
/// the squarest), starting `offset` edges round the loop.
pub(super) fn grid_fill(
    soup: &mut Soup,
    input: &Input,
    span: u32,
    offset: u32,
) -> Result<Picked, String> {
    let loops = soup.border_loops(&input.edges);
    let [ring] = &loops[..] else {
        return Err("Grid Fill needs one closed loop of open-border edges picked".to_owned());
    };
    let count = ring.len();
    if count % 2 != 0 || count < 4 {
        return Err(format!(
            "Grid Fill needs an even number of edges round the loop (this one has {count})"
        ));
    }
    let half = count / 2;
    let across = if span == 0 {
        (count / 4).max(1)
    } else {
        span as usize
    }
    .clamp(1, half - 1);
    let down = half - across;
    let like = like_border(soup, ring);
    let uvs = soup.vertex_uvs();
    // The new face runs the other way to the border's.
    let mut points: Vec<u32> = ring.iter().rev().copied().collect();
    points.rotate_left(offset as usize % count);
    // grid[i][j]: i across (0..=across), j down (0..=down), round the edge
    // counter-clockwise from (0, 0).
    let mut grid = vec![vec![u32::MAX; down + 1]; across + 1];
    for i in 0..=across {
        grid[i][0] = points[i];
        grid[across - i][down] = points[(across + down + i) % count];
    }
    for j in 0..=down {
        grid[across][j] = points[across + j];
        grid[0][down - j] = points[(2 * across + down + j) % count];
    }
    let at = |i: usize, j: usize| soup.position(grid[i][j]);
    let mut inside = Vec::new();
    for i in 1..across {
        for j in 1..down {
            let (u, v) = (i as f32 / across as f32, j as f32 / down as f32);
            // A Coons patch: the sides blended, less the corners counted
            // twice.
            let sides =
                at(i, 0) * (1.0 - v) + at(i, down) * v + at(0, j) * (1.0 - u) + at(across, j) * u;
            let corners = at(0, 0) * (1.0 - u) * (1.0 - v)
                + at(across, 0) * u * (1.0 - v)
                + at(0, down) * (1.0 - u) * v
                + at(across, down) * u * v;
            inside.push((i, j, sides - corners));
        }
    }
    for (i, j, position) in inside {
        grid[i][j] = soup.add(position);
    }
    let uv = |v: u32| uvs.get(&v).copied().unwrap_or_default();
    let mut picked = Picked::default();
    for i in 0..across {
        for j in 0..down {
            let corners = vec![
                grid[i][j],
                grid[i + 1][j],
                grid[i + 1][j + 1],
                grid[i][j + 1],
            ];
            let corner_uvs = corners.iter().map(|&v| uv(v)).collect();
            picked.faces.insert(soup.faces.len());
            soup.faces.push(SoupFace::like(&like, corners, corner_uvs));
        }
    }
    Ok(picked)
}

/// Faces joining two picked loops of open-border edges with the same
/// number of edges, each vertex to the one across from it that makes the
/// shortest joins.
pub(super) fn bridge(soup: &mut Soup, input: &Input) -> Result<Picked, String> {
    let loops = soup.border_loops(&input.edges);
    let [first, second] = &loops[..] else {
        return Err(format!(
            "Bridge needs two closed loops of open-border edges picked (found {})",
            loops.len()
        ));
    };
    let count = first.len();
    if second.len() != count {
        return Err(format!(
            "Bridge needs loops with as many edges ({count} and {})",
            second.len()
        ));
    }
    // The second loop run backward, from the turn that lines up best.
    let across = |shift: usize, i: usize| second[(shift + count - i % count) % count];
    let shift = (0..count)
        .min_by(|&x, &y| {
            let cost = |shift: usize| -> f32 {
                (0..count)
                    .map(|i| {
                        soup.position(first[i])
                            .distance(soup.position(across(shift, i)))
                    })
                    .sum()
            };
            cost(x).total_cmp(&cost(y))
        })
        .unwrap_or(0);
    let like = like_border(soup, first);
    let uvs = soup.vertex_uvs();
    let uv = |v: u32| uvs.get(&v).copied().unwrap_or_default();
    let mut picked = Picked::default();
    for i in 0..count {
        let corners = vec![
            first[(i + 1) % count],
            first[i],
            across(shift, i),
            across(shift, i + 1),
        ];
        let corner_uvs = corners.iter().map(|&v| uv(v)).collect();
        picked.faces.insert(soup.faces.len());
        soup.faces.push(SoupFace::like(&like, corners, corner_uvs));
    }
    Ok(picked)
}
