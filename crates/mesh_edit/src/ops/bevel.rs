//! Bevel (Ctrl+B for edges, Ctrl+Shift+B for vertices): the picked edges
//! or corners cut off, `segments` faces across, rounded by `profile`.
//!
//! An edge bevel moves each corner of the faces beside a picked edge in
//! from it by the width: to a point inside the face when both its edges
//! there are picked, or along its other edge when only one is. A strip of
//! faces runs along each picked edge between the two faces' new corners,
//! curved by the profile; where strips meet at a vertex, a patch face
//! fills the gap (or, when one face round the vertex has neither of its
//! edges there picked, that face takes the gap in instead, as Blender's
//! does at the end of a beveled edge). A vertex bevel cuts every edge at
//! the vertex and fills the corner with one patch.

use std::collections::{BTreeSet, HashMap};

use glam::{Vec2, Vec3};

use super::{Input, Picked, Soup, SoupFace, key};

/// A corner of a face round the vertex being beveled.
struct Corner {
    face: usize,
    /// The vertex before it in the face (the edge coming in) and after it
    /// (the edge going out).
    before: u32,
    after: u32,
}

pub(super) fn bevel(
    soup: &mut Soup,
    input: &Input,
    width: f32,
    segments: u32,
    profile: f32,
    vertices: bool,
) -> Result<Picked, String> {
    let segments = segments.clamp(1, 64) as usize;
    let directed = soup.directed();
    let beveled: BTreeSet<[u32; 2]> = if vertices {
        BTreeSet::new()
    } else {
        input
            .edges
            .iter()
            .copied()
            .filter(|&[a, b]| directed.contains_key(&(a, b)) && directed.contains_key(&(b, a)))
            .collect()
    };
    let corners_of: BTreeSet<u32> = if vertices {
        input.vertices.clone()
    } else {
        beveled.iter().flatten().copied().collect()
    };
    if corners_of.is_empty() {
        return Err(if vertices {
            "Bevel Vertices needs vertices picked".to_owned()
        } else {
            "Bevel needs edges between two faces picked".to_owned()
        });
    }
    let fans = fans(soup, &directed, &corners_of)?;
    let mut fan_order: Vec<u32> = fans.keys().copied().collect();
    fan_order.sort_unstable();

    // No wider than nearly half the shortest edge at a beveled corner, so
    // the cuts can't cross.
    let shortest = fans
        .iter()
        .flat_map(|(&v, fan)| fan.iter().map(move |c| (v, c.after)))
        .map(|(a, b)| soup.position(a).distance(soup.position(b)))
        .fold(f32::INFINITY, f32::min);
    let width = width.clamp(0.0, shortest * 0.45);
    let is_beveled = |a: u32, b: u32| vertices || beveled.contains(&key(a, b));
    // A corner with neither edge beveled (edge bevels only).
    let is_plain =
        |v: u32, c: &Corner| !vertices && !is_beveled(v, c.before) && !is_beveled(v, c.after);

    // How far along each unbeveled edge its new point slides.
    let mut slides: HashMap<(u32, u32), Vec<f32>> = HashMap::new();
    for &vertex in &fan_order {
        let here = soup.position(vertex);
        for corner in &fans[&vertex] {
            let d_in = (soup.position(corner.before) - here).normalize_or_zero();
            let d_out = (soup.position(corner.after) - here).normalize_or_zero();
            let sin = d_in.cross(d_out).length();
            let along = if sin > 0.05 { width / sin } else { width };
            let (in_bev, out_bev) = (
                is_beveled(vertex, corner.before),
                is_beveled(vertex, corner.after),
            );
            if vertices {
                slides
                    .entry((vertex, corner.before))
                    .or_default()
                    .push(width);
                slides
                    .entry((vertex, corner.after))
                    .or_default()
                    .push(width);
            } else if in_bev && !out_bev {
                slides
                    .entry((vertex, corner.after))
                    .or_default()
                    .push(along);
            } else if out_bev && !in_bev {
                slides
                    .entry((vertex, corner.before))
                    .or_default()
                    .push(along);
            }
        }
    }
    let mut slide_ids: HashMap<(u32, u32), (u32, f32)> = HashMap::new();
    let mut slid_edges: Vec<(u32, u32)> = slides.keys().copied().collect();
    slid_edges.sort_unstable();
    for (vertex, other) in slid_edges {
        let distances = &slides[&(vertex, other)];
        let (from, to) = (soup.position(vertex), soup.position(other));
        let length = from.distance(to);
        let room = if corners_of.contains(&other) {
            0.45
        } else {
            0.9
        };
        let along = (distances.iter().sum::<f32>() / distances.len() as f32).min(length * room);
        let fraction = along / length.max(1e-12);
        let id = soup.add(from.lerp(to, fraction));
        slide_ids.insert((vertex, other), (id, fraction));
    }

    // Each corner round a beveled vertex becomes these points, in face
    // order.
    let mut sequences: HashMap<(usize, u32), Vec<(u32, Vec2)>> = HashMap::new();
    for &vertex in &fan_order {
        let here = soup.position(vertex);
        for corner in &fans[&vertex] {
            let face = &soup.faces[corner.face];
            let uv = face.uv_at(vertex);
            let slid = |other: u32| {
                slide_ids
                    .get(&(vertex, other))
                    .map(|&(id, fraction)| (id, uv.lerp(face.uv_at(other), fraction)))
            };
            let (first, last) = (slid(corner.before), slid(corner.after));
            let normal = soup.normal(corner.face);
            let (in_bev, out_bev) = (
                is_beveled(vertex, corner.before),
                is_beveled(vertex, corner.after),
            );
            let sequence = if vertices {
                let (Some(first), Some(last)) = (first, last) else {
                    continue;
                };
                let (p0, p1) = (soup.position(first.0), soup.position(last.0));
                let mut sequence = vec![first];
                for point in profile_points(here, p0, p1, segments, profile) {
                    sequence.push((soup.add(point), uv));
                }
                sequence.push(last);
                sequence
            } else if in_bev && out_bev {
                // In from both edges by the width.
                let d_in = (soup.position(corner.before) - here).normalize_or_zero();
                let d_out = (soup.position(corner.after) - here).normalize_or_zero();
                let sin = d_in.cross(d_out).length();
                let inside = if sin > 0.05 {
                    (d_in + d_out) * width / sin
                } else {
                    normal.cross(d_out).normalize_or_zero() * width
                };
                vec![(soup.add(here + inside), uv)]
            } else if in_bev {
                vec![last.expect("a slide on each one-sided corner")]
            } else if out_bev {
                vec![first.expect("a slide on each one-sided corner")]
            } else {
                let mut sequence: Vec<(u32, Vec2)> = first.into_iter().collect();
                sequence.push((vertex, uv));
                sequence.extend(last);
                sequence
            };
            sequences.insert((corner.face, vertex), sequence);
        }
    }

    // The profile across each beveled edge at each end: from the corner of
    // the face the edge comes into to the corner of the face after it.
    let mut profiles: HashMap<(u32, u32), Vec<u32>> = HashMap::new();
    if !vertices {
        for &vertex in &fan_order {
            let fan = &fans[&vertex];
            for (i, corner) in fan.iter().enumerate() {
                if !is_beveled(vertex, corner.before) {
                    continue;
                }
                let next = &fan[(i + 1) % fan.len()];
                let p0 = sequences[&(corner.face, vertex)][0].0;
                let p1 = sequences[&(next.face, vertex)]
                    .last()
                    .expect("a corner has points")
                    .0;
                // Two beveled edges whose profiles join the same points
                // (where a beveled loop turns a corner) share one: their
                // strips meet along it, mitred.
                let shared = profiles
                    .iter()
                    .find(|((v, _), ids)| *v == vertex && ids[0] == p1 && ids[ids.len() - 1] == p0)
                    .map(|(_, ids)| ids.iter().rev().copied().collect::<Vec<u32>>());
                if let Some(ids) = shared {
                    profiles.insert((vertex, corner.before), ids);
                    continue;
                }
                let points = profile_points(
                    soup.position(vertex),
                    soup.position(p0),
                    soup.position(p1),
                    segments,
                    profile,
                );
                let mut ids = vec![p0];
                ids.extend(points.into_iter().map(|p| soup.add(p)));
                ids.push(p1);
                profiles.insert((vertex, corner.before), ids);
            }
        }
    }
    let mut uv_of: HashMap<u32, Vec2> = HashMap::new();
    for sequence in sequences.values() {
        for &(id, uv) in sequence {
            uv_of.entry(id).or_insert(uv);
        }
    }
    let uv = |uv_of: &HashMap<u32, Vec2>, id: u32| uv_of.get(&id).copied().unwrap_or_default();

    // The gap round each vertex: filled by a patch, or taken in by its one
    // plain face.
    let mut picked = Picked::default();
    let mut added = Vec::new();
    for &vertex in &fan_order {
        let fan = &fans[&vertex];
        let mut ring: Vec<u32> = Vec::new();
        for corner in fan {
            ring.extend(sequences[&(corner.face, vertex)].iter().rev().map(|p| p.0));
            if let Some(ids) = profiles.get(&(vertex, corner.before)) {
                ring.extend(&ids[1..ids.len() - 1]);
            }
        }
        tidy_ring(&mut ring);
        let plain: Vec<&Corner> = fan.iter().filter(|c| is_plain(vertex, c)).collect();
        if let [only] = plain[..] {
            if let Some(at) = ring.iter().position(|&id| id == vertex) {
                let path: Vec<(u32, Vec2)> = ring[at + 1..]
                    .iter()
                    .chain(&ring[..at])
                    .map(|&id| (id, uv(&uv_of, id)))
                    .collect();
                sequences.insert((only.face, vertex), path);
            }
            continue;
        }
        if ring.len() >= 3 {
            let uvs = ring.iter().map(|&id| uv(&uv_of, id)).collect();
            added.push(SoupFace::like(&soup.faces[fan[0].face], ring, uvs));
        }
    }

    // The faces round each beveled vertex take their new corners.
    let changed: BTreeSet<usize> = sequences.keys().map(|&(face, _)| face).collect();
    for face in changed {
        let polygon = &soup.faces[face];
        let mut corners = Vec::new();
        let mut uvs = Vec::new();
        for (k, &corner) in polygon.corners.iter().enumerate() {
            match sequences.get(&(face, corner)) {
                Some(sequence) => {
                    for &(id, uv) in sequence {
                        corners.push(id);
                        uvs.push(uv);
                    }
                }
                None => {
                    corners.push(corner);
                    uvs.push(polygon.uvs[k]);
                }
            }
        }
        let polygon = &mut soup.faces[face];
        polygon.corners = corners;
        polygon.uvs = uvs;
        polygon.normals.clear();
    }

    // A strip along each beveled edge.
    for &[a, b] in &beveled {
        let (Some(at_a), Some(at_b)) = (profiles.get(&(a, b)), profiles.get(&(b, a))) else {
            continue;
        };
        let like = directed[&(a, b)].0;
        let last = at_a.len() - 1;
        for k in 0..last {
            let ids = vec![at_a[k + 1], at_a[k], at_b[last - k], at_b[last - k - 1]];
            let uvs = ids.iter().map(|&id| uv(&uv_of, id)).collect();
            picked.faces.insert(soup.faces.len());
            soup.faces.push(SoupFace::like(&soup.faces[like], ids, uvs));
        }
    }
    for face in added {
        picked.faces.insert(soup.faces.len());
        soup.faces.push(face);
    }
    Ok(picked)
}

/// A patch's corners with repeats in a row taken out, and with spurs
/// (out to a point and straight back, where two strips share a profile)
/// folded away.
fn tidy_ring(ring: &mut Vec<u32>) {
    loop {
        ring.dedup();
        while ring.len() > 1 && ring.first() == ring.last() {
            ring.pop();
        }
        let count = ring.len();
        if count < 3 {
            return;
        }
        // A corner whose neighbours are the same point is a spur's tip.
        let Some(tip) =
            (0..count).find(|&k| ring[(k + count - 1) % count] == ring[(k + 1) % count])
        else {
            return;
        };
        let after = (tip + 1) % count;
        // Take out the tip and the way back from it.
        let (first, second) = (tip.max(after), tip.min(after));
        ring.remove(first);
        ring.remove(second);
    }
}

/// The faces round each of `vertices`, in turn: each next one is across
/// the edge coming into the corner before. An open or pinched fan is an
/// error.
fn fans(
    soup: &Soup,
    directed: &HashMap<(u32, u32), (usize, usize)>,
    vertices: &BTreeSet<u32>,
) -> Result<HashMap<u32, Vec<Corner>>, String> {
    let mut uses: HashMap<u32, usize> = HashMap::new();
    let mut start: HashMap<u32, (usize, usize)> = HashMap::new();
    for (index, face) in soup.faces.iter().enumerate() {
        for (k, &corner) in face.corners.iter().enumerate() {
            *uses.entry(corner).or_default() += 1;
            start.entry(corner).or_insert((index, k));
        }
    }
    let mut fans = HashMap::new();
    for &vertex in vertices {
        let Some(&(mut face, mut k)) = start.get(&vertex) else {
            continue;
        };
        let mut fan: Vec<Corner> = Vec::new();
        loop {
            let corners = &soup.faces[face].corners;
            let count = corners.len();
            let before = corners[(k + count - 1) % count];
            fan.push(Corner {
                face,
                before,
                after: corners[(k + 1) % count],
            });
            let Some(&(next, at)) = directed.get(&(vertex, before)) else {
                return Err("Bevel needs a closed surface round each corner".to_owned());
            };
            (face, k) = (next, at);
            if fan[0].face == face || fan.len() > uses[&vertex] {
                break;
            }
        }
        if fan.len() != uses[&vertex] {
            return Err("Bevel needs one fan of faces round each corner".to_owned());
        }
        fans.insert(vertex, fan);
    }
    Ok(fans)
}

/// The points between `p0` and `p1` (not those two) on a bevel's profile
/// round the sharp corner `corner`: a quarter of a superellipse, round at
/// `profile` 0.5, square at 1, flat at 0.25 and hollow below.
fn profile_points(corner: Vec3, p0: Vec3, p1: Vec3, segments: usize, profile: f32) -> Vec<Vec3> {
    let exponent = 2.0_f32.powf(4.0 * profile.clamp(0.0, 1.0) - 1.0);
    let middle = p0 + p1 - corner;
    (1..segments)
        .map(|k| {
            let angle = k as f32 / segments as f32 * std::f32::consts::FRAC_PI_2;
            let (x, y) = (angle.cos(), angle.sin());
            let scale = (x.powf(exponent) + y.powf(exponent)).powf(-1.0 / exponent);
            middle + (p0 - middle) * (x * scale) + (p1 - middle) * (y * scale)
        })
        .collect()
}
