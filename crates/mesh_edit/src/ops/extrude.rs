//! Extrude (E), inset (I) and spin: faces copied out with a wall of new
//! faces joining the copy to where it came from.

use std::collections::{BTreeSet, HashMap, HashSet};

use glam::{Mat4, Quat, Vec2, Vec3};

use super::{ExtrudeMode, Input, Picked, Soup, SoupFace, key};

/// E: the picked faces pushed out by `distance`; with no faces picked,
/// picked edges on an open border pulled out into new faces.
pub(super) fn extrude(
    soup: &mut Soup,
    input: &Input,
    mode: ExtrudeMode,
    distance: f32,
) -> Result<Picked, String> {
    if input.faces.is_empty() {
        return extrude_border(soup, input, distance);
    }
    let groups: Vec<Vec<usize>> = match mode {
        ExtrudeMode::Individual => input.faces.iter().map(|&f| vec![f]).collect(),
        _ => soup.components(&input.faces),
    };
    let mut picked = Picked::default();
    for group in groups {
        let normals: HashMap<usize, Vec3> = group.iter().map(|&f| (f, soup.normal(f))).collect();
        let average = normals.values().sum::<Vec3>().normalize_or_zero();
        let mut around: HashMap<u32, Vec<Vec3>> = HashMap::new();
        for &face in &group {
            for &corner in &soup.faces[face].corners {
                around.entry(corner).or_default().push(normals[&face]);
            }
        }
        let offsets: HashMap<u32, Vec3> = around
            .into_iter()
            .map(|(vertex, normals)| {
                let offset = match mode {
                    ExtrudeMode::AlongNormals => {
                        let direction = normals.iter().sum::<Vec3>().normalize_or_zero();
                        // Even thickness: farther where the faces meet at
                        // an angle.
                        let least = normals
                            .iter()
                            .map(|n| n.dot(direction))
                            .fold(1.0_f32, f32::min)
                            .max(0.25);
                        direction * distance / least
                    }
                    _ => average * distance,
                };
                (vertex, offset)
            })
            .collect();
        let positions = soup.positions.clone();
        extrude_faces(soup, &group, |v| positions[v as usize] + offsets[&v]);
        picked.faces.extend(group);
    }
    Ok(picked)
}

/// Copies the faces of `group` to where `place` puts each vertex, with a
/// wall face along each edge of the group's border. The group's faces are
/// the copies after. Returns each vertex's copy.
fn extrude_faces(
    soup: &mut Soup,
    group: &[usize],
    place: impl Fn(u32) -> Vec3,
) -> HashMap<u32, u32> {
    let members: HashSet<usize> = group.iter().copied().collect();
    let directed = soup.directed();
    let mut copies: HashMap<u32, u32> = HashMap::new();
    for &face in group {
        for corner in soup.faces[face].corners.clone() {
            copies
                .entry(corner)
                .or_insert_with(|| soup.add(place(corner)));
        }
    }
    let mut walls = Vec::new();
    for &face in group {
        let polygon = &soup.faces[face];
        for k in 0..polygon.corners.len() {
            let (a, b) = polygon.edge(k);
            let inside = directed
                .get(&(b, a))
                .is_some_and(|(other, _)| members.contains(other));
            if !inside {
                let (ua, ub) = (polygon.uvs[k], polygon.uvs[(k + 1) % polygon.corners.len()]);
                walls.push(SoupFace::like(
                    polygon,
                    vec![a, b, copies[&b], copies[&a]],
                    vec![ua, ub, ub, ua],
                ));
            }
        }
    }
    for &face in group {
        let polygon = &mut soup.faces[face];
        for corner in &mut polygon.corners {
            *corner = copies[corner];
        }
        polygon.normals.clear();
    }
    soup.faces.extend(walls);
    copies
}

/// One edge of an open border, as its face runs along it.
struct BorderEdge {
    a: u32,
    b: u32,
    uvs: [Vec2; 2],
    face: usize,
}

/// The picked edges on an open border, as their faces run along them.
fn border_edges(soup: &Soup, edges: &BTreeSet<[u32; 2]>) -> Vec<BorderEdge> {
    let directed = soup.directed();
    let mut border: Vec<BorderEdge> = directed
        .iter()
        .filter(|((a, b), _)| edges.contains(&key(*a, *b)) && !directed.contains_key(&(*b, *a)))
        .map(|(&(a, b), &(face, k))| {
            let polygon = &soup.faces[face];
            BorderEdge {
                a,
                b,
                uvs: [polygon.uvs[k], polygon.uvs[(k + 1) % polygon.corners.len()]],
                face,
            }
        })
        .collect();
    border.sort_by_key(|edge| (edge.a, edge.b));
    border
}

/// Edges on an open border pulled out by `distance`, away from their
/// faces, into a new face each.
fn extrude_border(soup: &mut Soup, input: &Input, distance: f32) -> Result<Picked, String> {
    let border = border_edges(soup, &input.edges);
    if border.is_empty() {
        return Err("Extrude needs faces, or edges on an open border, picked".to_owned());
    }
    let mut outward: HashMap<u32, Vec3> = HashMap::new();
    for edge in &border {
        let along = soup.position(edge.b) - soup.position(edge.a);
        let away = along.cross(soup.normal(edge.face)).normalize_or_zero();
        *outward.entry(edge.a).or_default() += away;
        *outward.entry(edge.b).or_default() += away;
    }
    let positions = soup.positions.clone();
    let copies = pull_border(soup, &border, |v| {
        positions[v as usize] + outward[&v].normalize_or_zero() * distance
    });
    Ok(Picked {
        edges: border
            .iter()
            .map(|edge| [copies[&edge.a], copies[&edge.b]])
            .collect(),
        ..Picked::default()
    })
}

/// A new face along each `border` edge, out to copies of its ends where
/// `place` puts them. Returns each vertex's copy.
fn pull_border(
    soup: &mut Soup,
    border: &[BorderEdge],
    place: impl Fn(u32) -> Vec3,
) -> HashMap<u32, u32> {
    let mut copies: HashMap<u32, u32> = HashMap::new();
    for edge in border {
        for vertex in [edge.a, edge.b] {
            copies
                .entry(vertex)
                .or_insert_with(|| soup.add(place(vertex)));
        }
    }
    for edge in border {
        let [ua, ub] = edge.uvs;
        let face = SoupFace::like(
            &soup.faces[edge.face],
            vec![edge.b, edge.a, copies[&edge.a], copies[&edge.b]],
            vec![ub, ua, ua, ub],
        );
        soup.faces.push(face);
    }
    copies
}

/// I: each region of picked faces (or each face, `individual`) shrunk
/// inside its border by `thickness` and pushed out by `depth`, with a ring
/// of faces joining it to the border.
pub(super) fn inset(
    soup: &mut Soup,
    input: &Input,
    thickness: f32,
    depth: f32,
    individual: bool,
) -> Result<Picked, String> {
    if input.faces.is_empty() {
        return Err("Inset needs faces picked".to_owned());
    }
    let groups: Vec<Vec<usize>> = if individual {
        input.faces.iter().map(|&f| vec![f]).collect()
    } else {
        soup.components(&input.faces)
    };
    for group in &groups {
        let members: HashSet<usize> = group.iter().copied().collect();
        let directed = soup.directed();
        let normal = group
            .iter()
            .map(|&f| soup.normal(f))
            .sum::<Vec3>()
            .normalize_or_zero();
        // The border, each edge with the way into its face.
        let mut into: HashMap<u32, Vec3> = HashMap::new();
        let mut out_of: HashMap<u32, Vec3> = HashMap::new();
        let mut border = Vec::new();
        for &face in group {
            let polygon = &soup.faces[face];
            let face_normal = soup.normal(face);
            for k in 0..polygon.corners.len() {
                let (a, b) = polygon.edge(k);
                let inside = directed
                    .get(&(b, a))
                    .is_some_and(|(other, _)| members.contains(other));
                if inside {
                    continue;
                }
                let inward = face_normal
                    .cross(soup.position(b) - soup.position(a))
                    .normalize_or_zero();
                out_of.entry(a).or_insert(inward);
                into.entry(b).or_insert(inward);
                border.push((face, k));
            }
        }
        if border.is_empty() {
            continue;
        }
        let mut copies: HashMap<u32, u32> = HashMap::new();
        for &face in group {
            for corner in soup.faces[face].corners.clone() {
                if copies.contains_key(&corner) {
                    continue;
                }
                let mut position = soup.position(corner) + normal * depth;
                if let (Some(&d_in), Some(&d_out)) = (into.get(&corner), out_of.get(&corner)) {
                    position += miter(d_in, d_out) * thickness;
                }
                let copy = soup.add(position);
                copies.insert(corner, copy);
            }
        }
        let mut ring = Vec::new();
        for &(face, k) in &border {
            let polygon = &soup.faces[face];
            let (a, b) = polygon.edge(k);
            let (ua, ub) = (polygon.uvs[k], polygon.uvs[(k + 1) % polygon.corners.len()]);
            ring.push(SoupFace::like(
                polygon,
                vec![a, b, copies[&b], copies[&a]],
                vec![ua, ub, ub, ua],
            ));
        }
        for &face in group {
            let polygon = &mut soup.faces[face];
            for corner in &mut polygon.corners {
                *corner = copies[corner];
            }
            polygon.normals.clear();
        }
        soup.faces.extend(ring);
    }
    Ok(Picked {
        faces: input.faces.clone(),
        ..Picked::default()
    })
}

/// The step from a corner to the point one unit inside both edges that
/// meet there, given the way into the face from each.
pub(super) fn miter(d_in: Vec3, d_out: Vec3) -> Vec3 {
    let sum = d_in + d_out;
    let cos = d_in.dot(d_out);
    if cos < -0.95 {
        // The edges run nearly straight on.
        return d_in;
    }
    sum / (1.0 + cos)
}

/// The picked faces (or, with none, open-border edges) swept `degrees`
/// round `axis` through `center` in `steps` extrusions. A whole turn joins
/// the end back to the start.
pub(super) fn spin(
    soup: &mut Soup,
    input: &Input,
    degrees: f32,
    steps: u32,
    axis: Vec3,
    center: Vec3,
) -> Result<Picked, String> {
    let steps = steps.clamp(1, 1024);
    let axis = axis.normalize_or(Vec3::Y);
    let turn = Quat::from_axis_angle(axis, degrees.to_radians() / steps as f32);
    let step =
        Mat4::from_translation(center) * Mat4::from_quat(turn) * Mat4::from_translation(-center);
    let whole_turn = (degrees.abs() - 360.0).abs() < 0.01;
    let first = soup.positions.len() as u32;
    // Each vertex of the front back to where it started.
    let mut origin: HashMap<u32, u32> = HashMap::new();
    let mut picked = Picked::default();
    if !input.faces.is_empty() {
        let group: Vec<usize> = input.faces.iter().copied().collect();
        for &face in &group {
            for &corner in &soup.faces[face].corners {
                origin.insert(corner, corner);
            }
        }
        for _ in 0..steps {
            let positions = soup.positions.clone();
            let copies = extrude_faces(soup, &group, |v| {
                step.transform_point3(positions[v as usize])
            });
            origin = copies
                .iter()
                .map(|(from, to)| (*to, origin[from]))
                .collect();
        }
        if whole_turn {
            // The front ends where the faces were: the walls close up and
            // the faces go.
            join(soup, &origin);
            let gone: HashSet<usize> = group.iter().copied().collect();
            let mut index = 0;
            soup.faces.retain(|_| {
                index += 1;
                !gone.contains(&(index - 1))
            });
        } else {
            picked.faces.extend(group);
        }
    } else {
        let mut border = border_edges(soup, &input.edges);
        if border.is_empty() {
            return Err("Spin needs faces, or edges on an open border, picked".to_owned());
        }
        for edge in &border {
            origin.insert(edge.a, edge.a);
            origin.insert(edge.b, edge.b);
        }
        for _ in 0..steps {
            let positions = soup.positions.clone();
            let copies = pull_border(soup, &border, |v| {
                step.transform_point3(positions[v as usize])
            });
            origin = copies
                .iter()
                .map(|(from, to)| (*to, origin[from]))
                .collect();
            // The new border: the far side of each face just added, which
            // are the last ones, in the border's order.
            let added = soup.faces.len() - border.len();
            for (k, edge) in border.iter_mut().enumerate() {
                edge.a = copies[&edge.a];
                edge.b = copies[&edge.b];
                edge.face = added + k;
            }
        }
        if whole_turn {
            join(soup, &origin);
        } else {
            picked.edges = border.iter().map(|edge| [edge.a, edge.b]).collect();
        }
    }
    // Points on the axis meet themselves: weld what the spin made there.
    let made: BTreeSet<u32> = (first..soup.positions.len() as u32)
        .chain(origin.values().copied())
        .collect();
    super::cleanup::weld(soup, &made, 1e-5);
    Ok(picked)
}

/// Every vertex in `origin` merged into the one it maps to.
fn join(soup: &mut Soup, origin: &HashMap<u32, u32>) {
    for face in &mut soup.faces {
        for corner in &mut face.corners {
            if let Some(&to) = origin.get(corner)
                && to != *corner
            {
                *corner = to;
                face.normals.clear();
            }
        }
    }
}
