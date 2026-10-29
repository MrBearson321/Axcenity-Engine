//! Which way faces face, and how they're shaded: flip, recalculate
//! normals (Shift+N), shade smooth, flat and auto smooth.

use std::collections::{BTreeSet, HashSet};

use glam::Vec3;

use super::{Input, Picked, Soup, key};

fn needs_faces(input: &Input, what: &str) -> Result<(), String> {
    if input.faces.is_empty() {
        Err(format!("{what} needs faces picked"))
    } else {
        Ok(())
    }
}

/// The picked faces turned to face the other way.
pub(super) fn flip(soup: &mut Soup, input: &Input) -> Result<Picked, String> {
    needs_faces(input, "Flip")?;
    for &face in &input.faces {
        soup.faces[face].flip();
        soup.faces[face].normals.clear();
    }
    Ok(Picked {
        faces: input.faces.clone(),
        ..Picked::default()
    })
}

/// Shift+N: each connected part of the picked faces turned to agree with
/// its neighbours, then all of it to face out (or in, `inside`), judged by
/// the space it encloses.
pub(super) fn recalculate(soup: &mut Soup, input: &Input, inside: bool) -> Result<Picked, String> {
    needs_faces(input, "Recalculate Normals")?;
    let edge_faces = soup.edge_faces();
    let mut done: HashSet<usize> = HashSet::new();
    for &start in &input.faces {
        if !done.insert(start) {
            continue;
        }
        let mut part = vec![start];
        let mut stack = vec![start];
        while let Some(face) = stack.pop() {
            let corners = soup.faces[face].corners.clone();
            for k in 0..corners.len() {
                let (a, b) = (corners[k], corners[(k + 1) % corners.len()]);
                for &other in &edge_faces[&key(a, b)] {
                    if !input.faces.contains(&other) || !done.insert(other) {
                        continue;
                    }
                    // A neighbour running the same way along the edge
                    // faces the other way.
                    let theirs = &soup.faces[other].corners;
                    let same = (0..theirs.len())
                        .any(|j| theirs[j] == a && theirs[(j + 1) % theirs.len()] == b);
                    if same {
                        soup.faces[other].flip();
                        soup.faces[other].normals.clear();
                    }
                    part.push(other);
                    stack.push(other);
                }
            }
        }
        // The volume it encloses, measured from its middle: positive when
        // it faces out.
        let vertices: BTreeSet<u32> = part
            .iter()
            .flat_map(|&f| soup.faces[f].corners.iter().copied())
            .collect();
        let middle =
            vertices.iter().map(|&v| soup.position(v)).sum::<Vec3>() / vertices.len() as f32;
        let volume: f32 = part
            .iter()
            .map(|&f| {
                let corners = &soup.faces[f].corners;
                let p = |k: usize| soup.position(corners[k]) - middle;
                (1..corners.len() - 1)
                    .map(|k| p(0).dot(p(k).cross(p(k + 1))))
                    .sum::<f32>()
            })
            .sum();
        if (volume < 0.0) != inside {
            for &face in &part {
                soup.faces[face].flip();
                soup.faces[face].normals.clear();
            }
        }
    }
    Ok(Picked {
        faces: input.faces.clone(),
        ..Picked::default()
    })
}

/// Shade smooth or flat, or auto smooth (`auto` sets its angle, in
/// radians, on the mesh; smooth and flat turn it off).
pub(super) fn shade(
    soup: &mut Soup,
    input: &Input,
    smooth: bool,
    auto: Option<f32>,
) -> Result<Picked, String> {
    needs_faces(input, "Shading")?;
    if soup.auto_smooth != auto {
        // Every smooth face shades differently now.
        soup.auto_smooth = auto;
        for face in soup.faces.iter_mut().filter(|face| face.smooth) {
            face.normals.clear();
        }
    }
    for &face in &input.faces {
        soup.faces[face].smooth = smooth;
        soup.faces[face].normals.clear();
    }
    Ok(Picked {
        faces: input.faces.clone(),
        ..Picked::default()
    })
}
