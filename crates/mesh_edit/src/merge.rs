//! Tidying flat faces after a cut.
//!
//! A cut splits every triangle it crosses, and a later cut across the first
//! one's cap meets each of that cap's diagonals, adding a corner where it
//! does. Those corners lie in a straight line along the new edge. Left
//! alone they multiply with every cut, into thousands of needle-thin
//! triangles. So after each cut, every flat face (touching triangles with
//! the same surface and one normal shared by all their corners) is filled
//! again from its outline, leaving out the corners inside it and the ones
//! that lie in a straight line between flat faces. Both faces beside such a
//! corner leave it out, so their edges still match.

use std::collections::HashMap;
use std::collections::hash_map::Entry;

use glam::{DVec2, DVec3, Vec3};

use crate::fill::{chain, fill, has_outline};
use crate::{Mesh, Surface, Triangle, position_key};

/// A corner this close to the line between its neighbours, as a fraction
/// of the mesh's size, counts as straight. Positions are f32, so corners
/// that should be in line are off it by about 1e-7 of the size.
const STRAIGHT: f64 = 1e-6;

/// A face filled again must cover the same area as before, to within this
/// fraction; otherwise it's kept as it was.
const SAME_AREA: f64 = 1e-4;

/// Touching triangles in one plane with one surface, which can be filled
/// again as a whole.
struct Face {
    /// Indices into the mesh's triangles, in order.
    triangles: Vec<usize>,
    surface: Surface,
    normal: DVec3,
    /// Its outline as loops of point ids, counter-clockwise seen from the
    /// side its normal points to.
    loops: Vec<Vec<u32>>,
    /// False if it mustn't be changed: its outline doesn't close, or a
    /// corner inside it is also used by other triangles.
    ok: bool,
    /// True if some of its corners are inside it, not on its outline.
    has_inside: bool,
    /// The triangles it's filled with now, as point ids.
    filled: Option<Vec<[u32; 3]>>,
}

/// `mesh` with each flat face filled again without the corners it doesn't
/// need. Unused vertices are dropped.
pub(crate) fn merge_flat_faces(mesh: &Mesh) -> Mesh {
    // Number the distinct positions.
    let mut ids: HashMap<[u32; 3], u32> = HashMap::new();
    let mut points: Vec<DVec3> = Vec::new();
    let point_of: Vec<u32> = mesh
        .positions
        .iter()
        .map(|&position| {
            *ids.entry(position_key(position)).or_insert_with(|| {
                points.push(position.as_dvec3());
                (points.len() - 1) as u32
            })
        })
        .collect();
    let corners = |triangle: usize| {
        mesh.triangles[triangle]
            .corners
            .map(|c| point_of[c as usize])
    };
    // Far from the origin, f32 positions are less precise too.
    let (min, max) = mesh.bounds();
    let size = (max - min)
        .max_element()
        .max(min.abs().max(max.abs()).max_element());
    let tolerance = STRAIGHT * f64::from(size);

    let face_of = group_faces(mesh);
    let mut faces: Vec<Face> = Vec::new();
    let mut face_index: HashMap<usize, usize> = HashMap::new();
    for (triangle, root) in face_of.iter().enumerate() {
        let Some(root) = *root else { continue };
        let index = *face_index.entry(root).or_insert_with(|| {
            let first = mesh.triangles[triangle];
            faces.push(Face {
                triangles: Vec::new(),
                surface: first.surface,
                normal: mesh.normals[first.corners[0] as usize].as_dvec3(),
                loops: Vec::new(),
                ok: true,
                has_inside: false,
                filled: None,
            });
            faces.len() - 1
        });
        faces[index].triangles.push(triangle);
    }

    // Who uses each point: a face, or `None` for triangles in no face. A
    // point used by more than one owner is on an edge between them.
    let mut owners: Vec<Vec<Option<usize>>> = vec![Vec::new(); points.len()];
    for (triangle, root) in face_of.iter().enumerate() {
        let owner = root.map(|root| face_index[&root]);
        for point in corners(triangle) {
            if !owners[point as usize].contains(&owner) {
                owners[point as usize].push(owner);
            }
        }
    }

    // Each face's outline, and which points could be left out.
    let mut removable = vec![true; points.len()];
    for (point, owners) in owners.iter().enumerate() {
        if owners.contains(&None) {
            removable[point] = false;
        }
    }
    for face in &mut faces {
        outline(face, &corners);
        let mut on_outline: HashMap<u32, usize> = HashMap::new();
        for &point in face.loops.iter().flatten() {
            *on_outline.entry(point).or_default() += 1;
        }
        for point in face.triangles.iter().flat_map(|&t| corners(t)) {
            if !on_outline.contains_key(&point) {
                face.has_inside = true;
                if owners[point as usize].len() > 1 {
                    face.ok = false;
                }
            }
        }
        for lap in &face.loops {
            for (k, &point) in lap.iter().enumerate() {
                let before = lap[(k + lap.len() - 1) % lap.len()];
                let after = lap[(k + 1) % lap.len()];
                let straight = is_straight(
                    points[before as usize],
                    points[point as usize],
                    points[after as usize],
                    tolerance,
                );
                if !face.ok || !straight || on_outline[&point] > 1 {
                    removable[point as usize] = false;
                }
            }
        }
        if !face.ok {
            for point in face.triangles.iter().flat_map(|&t| corners(t)) {
                removable[point as usize] = false;
            }
        }
    }

    // Fill the faces again. A face that comes out wrong keeps every corner
    // of its outline, which its neighbours must keep too, so they're all
    // filled again until nothing changes.
    loop {
        let mut changed = false;
        for face in faces.iter_mut().filter(|face| face.ok) {
            face.filled = None;
            let drops = face
                .loops
                .iter()
                .flatten()
                .any(|&point| removable[point as usize]);
            if !face.has_inside && !drops {
                continue;
            }
            match refill(face, &points, &removable, &corners) {
                Some(triangles) => face.filled = Some(triangles),
                None => {
                    for &point in face.loops.iter().flatten() {
                        changed |= removable[point as usize];
                        removable[point as usize] = false;
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }

    rebuild(mesh, &faces, &face_of, &face_index, &point_of)
}

/// For each triangle, the first triangle of its flat face (the face's
/// name), or `None` if it isn't flat. Triangles are in one face when they
/// share an edge (the same two vertices), a surface and a normal.
fn group_faces(mesh: &Mesh) -> Vec<Option<usize>> {
    let key = |triangle: &Triangle| {
        let [a, b, c] = triangle.corners.map(|c| mesh.normals[c as usize]);
        (a == b && b == c && a != Vec3::ZERO).then(|| (triangle.surface, position_key(a)))
    };
    // Union-find: each triangle points toward the first of its face.
    let mut leader: Vec<usize> = (0..mesh.triangles.len()).collect();
    fn find(leader: &mut [usize], mut at: usize) -> usize {
        while leader[at] != at {
            leader[at] = leader[leader[at]];
            at = leader[at];
        }
        at
    }
    let mut by_edge = HashMap::new();
    for (index, triangle) in mesh.triangles.iter().enumerate() {
        let Some(key) = key(triangle) else { continue };
        let [a, b, c] = triangle.corners;
        for (from, to) in [(a, b), (b, c), (c, a)] {
            match by_edge.entry((from.min(to), from.max(to), key)) {
                Entry::Occupied(other) => {
                    let (x, y) = (find(&mut leader, index), find(&mut leader, *other.get()));
                    // The smaller index leads, so the result never
                    // depends on the order of the map.
                    leader[x.max(y)] = x.min(y);
                }
                Entry::Vacant(slot) => {
                    slot.insert(index);
                }
            }
        }
    }
    (0..mesh.triangles.len())
        .map(|index| key(&mesh.triangles[index]).map(|_| find(&mut leader, index)))
        .collect()
}

/// Works out `face`'s outline: the edges only one of its triangles uses
/// (by position), joined into loops. A face whose outline doesn't close up
/// isn't changed.
fn outline(face: &mut Face, corners: &impl Fn(usize) -> [u32; 3]) {
    let edges = |&triangle: &usize| {
        let [a, b, c] = corners(triangle);
        [(a, b), (b, c), (c, a)]
    };
    let mut surplus: HashMap<(u32, u32), i32> = HashMap::new();
    for (a, b) in face.triangles.iter().flat_map(edges) {
        if a != b {
            *surplus.entry((a, b)).or_default() += 1;
            *surplus.entry((b, a)).or_default() -= 1;
        }
    }
    let mut open = Vec::new();
    for (a, b) in face.triangles.iter().flat_map(edges) {
        if let Some(count) = surplus.get_mut(&(a, b))
            && *count > 0
        {
            *count -= 1;
            open.push((a, b));
        }
    }
    face.loops = chain(&open);
    if face.loops.iter().map(Vec::len).sum::<usize>() != open.len() {
        face.ok = false;
    }
}

/// True if `point` is between `before` and `after`, within `tolerance` of
/// the straight line from one to the other.
fn is_straight(before: DVec3, point: DVec3, after: DVec3, tolerance: f64) -> bool {
    let (into, out, across) = (point - before, after - point, after - before);
    into.dot(out) > 0.0 && into.cross(across).length() <= tolerance * across.length()
}

/// `face` filled from its outline without the `removable` points, or
/// `None` if that doesn't cover the same area.
fn refill(
    face: &Face,
    points: &[DVec3],
    removable: &[bool],
    corners: &impl Fn(usize) -> [u32; 3],
) -> Option<Vec<[u32; 3]>> {
    let loops: Vec<Vec<u32>> = face
        .loops
        .iter()
        .map(|lap| {
            lap.iter()
                .copied()
                .filter(|&point| !removable[point as usize])
                .collect()
        })
        .collect();
    if loops.iter().any(|lap: &Vec<u32>| lap.len() < 3) {
        return None;
    }
    // Flatten the outline onto the face's plane.
    let normal = face.normal.normalize_or_zero();
    let u = normal.any_orthonormal_vector();
    let v = normal.cross(u);
    let origin = points[loops[0][0] as usize];
    let mut flat = vec![DVec2::ZERO; points.len()];
    for &point in loops.iter().flatten() {
        let p = points[point as usize] - origin;
        flat[point as usize] = DVec2::new(p.dot(u), p.dot(v));
    }
    let triangles = fill(&flat, loops.clone());
    if !has_outline(&triangles, &loops) {
        return None;
    }

    let area = |[a, b, c]: [u32; 3]| {
        let [a, b, c] = [a, b, c].map(|p| points[p as usize]);
        (b - a).cross(c - a).dot(normal) / 2.0
    };
    // Added up without signs, so triangles turned over (overlapping
    // others) count against it.
    let before: f64 = face.triangles.iter().map(|&t| area(corners(t))).sum();
    let after: f64 = triangles.iter().map(|&t| area(t).abs()).sum();
    ((after - before).abs() <= SAME_AREA * before.abs()).then_some(triangles)
}

/// The mesh again: each refilled face's new triangles where its first
/// triangle was, everything else as it was, and only the vertices used.
fn rebuild(
    mesh: &Mesh,
    faces: &[Face],
    face_of: &[Option<usize>],
    face_index: &HashMap<usize, usize>,
    point_of: &[u32],
) -> Mesh {
    let mut triangles = Vec::with_capacity(mesh.triangles.len());
    for (index, triangle) in mesh.triangles.iter().enumerate() {
        let face = face_of[index].map(|root| (root, &faces[face_index[&root]]));
        match face {
            Some((root, face)) if face.filled.is_some() => {
                if root != index {
                    continue;
                }
                // Each point's vertex: the first of the face's corners
                // there, which has the face's normal and uvs.
                let mut vertex_at: HashMap<u32, u32> = HashMap::new();
                for &t in &face.triangles {
                    for corner in mesh.triangles[t].corners {
                        vertex_at.entry(point_of[corner as usize]).or_insert(corner);
                    }
                }
                for points in face.filled.iter().flatten() {
                    triangles.push(Triangle {
                        corners: points.map(|point| vertex_at[&point]),
                        surface: face.surface,
                    });
                }
            }
            _ => triangles.push(*triangle),
        }
    }

    let mut compact = Mesh::default();
    let mut index = vec![u32::MAX; mesh.positions.len()];
    for triangle in &mut triangles {
        for corner in &mut triangle.corners {
            let old = *corner as usize;
            if index[old] == u32::MAX {
                index[old] =
                    compact.push_vertex(mesh.positions[old], mesh.normals[old], mesh.uvs[old]);
            }
            *corner = index[old];
        }
    }
    compact.triangles = triangles;
    compact
}
