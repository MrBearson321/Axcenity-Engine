//! Proportional editing (O) and pivots for the Model Studio's moves
//! (Phase 56), as in Blender: with proportional editing on, vertices near
//! what's picked move part of the way, by a falloff curve over a radius;
//! the pivot is the point turns and scales go round.

use std::collections::{BTreeSet, HashMap};

use axcenity_mesh_edit::{EditMesh, SelectMode, Selection};
use axcenity_render::glam::{Mat3, Mat4, Quat, Vec3};

/// The shape of proportional editing's falloff, from the picked vertices
/// (all the move) out to the radius (none of it).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Falloff {
    #[default]
    Smooth,
    Sphere,
    Root,
    InverseSquare,
    Sharp,
    Linear,
    Constant,
}

impl Falloff {
    pub const ALL: [Falloff; 7] = [
        Falloff::Smooth,
        Falloff::Sphere,
        Falloff::Root,
        Falloff::InverseSquare,
        Falloff::Sharp,
        Falloff::Linear,
        Falloff::Constant,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Falloff::Smooth => "Smooth",
            Falloff::Sphere => "Sphere",
            Falloff::Root => "Root",
            Falloff::InverseSquare => "Inverse Square",
            Falloff::Sharp => "Sharp",
            Falloff::Linear => "Linear",
            Falloff::Constant => "Constant",
        }
    }

    /// How much of the move a vertex `distance` from the nearest picked
    /// one takes, out to `radius`.
    pub fn weight(self, distance: f32, radius: f32) -> f32 {
        if distance <= 0.0 {
            return 1.0;
        }
        if distance >= radius || radius <= 0.0 {
            return 0.0;
        }
        // 1 at the picked vertices, 0 at the radius.
        let x = 1.0 - distance / radius;
        match self {
            Falloff::Smooth => x * x * (3.0 - 2.0 * x),
            Falloff::Sphere => (2.0 * x - x * x).sqrt(),
            Falloff::Root => x.sqrt(),
            Falloff::InverseSquare => x * (2.0 - x),
            Falloff::Sharp => x * x,
            Falloff::Linear => x,
            Falloff::Constant => 1.0,
        }
    }
}

/// Proportional editing's settings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Proportional {
    pub on: bool,
    pub falloff: Falloff,
    /// In the model's units.
    pub radius: f32,
}

impl Default for Proportional {
    fn default() -> Self {
        Self {
            on: false,
            falloff: Falloff::Smooth,
            radius: 1.0,
        }
    }
}

/// The point turns and scales go round.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Pivot {
    /// The middle of the picked vertices.
    #[default]
    Median,
    /// The middle of the box round them.
    BoundingBox,
    /// The 3D cursor.
    Cursor,
    /// Each connected part round its own middle.
    Individual,
}

impl Pivot {
    pub const ALL: [Pivot; 4] = [
        Pivot::Median,
        Pivot::BoundingBox,
        Pivot::Cursor,
        Pivot::Individual,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Pivot::Median => "Median Point",
            Pivot::BoundingBox => "Bounding Box Center",
            Pivot::Cursor => "3D Cursor",
            Pivot::Individual => "Individual Origins",
        }
    }
}

/// How far each of `mesh`'s vertices is from the nearest of `picked`, for
/// those within `radius` (picked ones are 0).
pub fn distances(mesh: &EditMesh, picked: &[u32], radius: f32) -> HashMap<u32, f32> {
    let points: Vec<Vec3> = picked.iter().map(|&v| mesh.position(v)).collect();
    let picked: BTreeSet<u32> = picked.iter().copied().collect();
    (0..mesh.vertex_count() as u32)
        .filter_map(|v| {
            if picked.contains(&v) {
                return Some((v, 0.0));
            }
            let p = mesh.position(v);
            let nearest = points
                .iter()
                .map(|q| q.distance(p))
                .fold(f32::INFINITY, f32::min);
            (nearest < radius).then_some((v, nearest))
        })
        .collect()
}

/// For Individual Origins: each picked vertex's own part (picked vertices
/// joined by a face's edges; in Face mode, by picked faces), and that
/// part's middle.
pub fn origins(mesh: &EditMesh, selection: &Selection) -> HashMap<u32, Vec3> {
    let picked: BTreeSet<u32> = selection.vertices_of(mesh).into_iter().collect();
    let mut parent: HashMap<u32, u32> = picked.iter().map(|&v| (v, v)).collect();
    fn root(parent: &mut HashMap<u32, u32>, v: u32) -> u32 {
        let mut at = v;
        while parent[&at] != at {
            at = parent[&at];
        }
        parent.insert(v, at);
        at
    }
    let mut join = |a: u32, b: u32| {
        let (ra, rb) = (root(&mut parent, a), root(&mut parent, b));
        parent.insert(ra.max(rb), ra.min(rb));
    };
    match selection.mode {
        SelectMode::Face => {
            for &face in &selection.faces {
                let corners = mesh.face_vertices(face);
                for pair in corners.windows(2) {
                    join(pair[0], pair[1]);
                }
            }
        }
        _ => {
            for edge in mesh.edges() {
                let (a, b) = mesh.ends(edge);
                if picked.contains(&a) && picked.contains(&b) {
                    join(a, b);
                }
            }
        }
    }
    let mut parts: HashMap<u32, (Vec3, u32)> = HashMap::new();
    let roots: Vec<(u32, u32)> = picked.iter().map(|&v| (v, root(&mut parent, v))).collect();
    for &(v, r) in &roots {
        let entry = parts.entry(r).or_insert((Vec3::ZERO, 0));
        entry.0 += mesh.position(v);
        entry.1 += 1;
    }
    roots
        .into_iter()
        .map(|(v, r)| {
            let (sum, count) = parts[&r];
            (v, sum / count as f32)
        })
        .collect()
}

/// `weight` of the move `matrix` (0 none, 1 all of it): a turn round
/// `center` turns that much of the way; anything else moves each point
/// that much of the way to where `matrix` puts it.
pub fn partial(matrix: Mat4, weight: f32, center: Vec3) -> Mat4 {
    if weight >= 1.0 {
        return matrix;
    }
    let linear = Mat3::from_mat4(matrix);
    let turn = (linear * linear.transpose()).abs_diff_eq(Mat3::IDENTITY, 1e-4)
        && linear.determinant() > 0.0
        && !linear.abs_diff_eq(Mat3::IDENTITY, 1e-6);
    if turn {
        let rotation = Quat::IDENTITY.slerp(Quat::from_mat3(&linear), weight);
        let shift = (matrix.transform_point3(center) - center) * weight;
        return Mat4::from_translation(center + shift)
            * Mat4::from_quat(rotation)
            * Mat4::from_translation(-center);
    }
    Mat4::IDENTITY * (1.0 - weight) + matrix * weight
}

/// `matrix` (round the move's pivot `center`) as it moves a part round its
/// own `origin` instead.
pub fn around(matrix: Mat4, center: Vec3, origin: Vec3) -> Mat4 {
    let shift = origin - center;
    Mat4::from_translation(shift) * matrix * Mat4::from_translation(-shift)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn falloffs_go_from_all_at_the_picked_vertices_to_none_at_the_radius() {
        for falloff in Falloff::ALL {
            assert_eq!(falloff.weight(0.0, 2.0), 1.0);
            assert_eq!(falloff.weight(2.0, 2.0), 0.0);
            let half = falloff.weight(1.0, 2.0);
            assert!((0.0..=1.0).contains(&half), "{falloff:?}");
        }
        assert!((Falloff::Linear.weight(0.5, 2.0) - 0.75).abs() < 1e-6);
        assert!((Falloff::Smooth.weight(1.0, 2.0) - 0.5).abs() < 1e-6);
        assert_eq!(Falloff::Constant.weight(1.9, 2.0), 1.0);
    }

    #[test]
    fn a_partial_turn_turns_part_of_the_way() {
        let center = Vec3::new(1.0, 0.0, 0.0);
        let quarter = Mat4::from_translation(center)
            * Mat4::from_rotation_y(std::f32::consts::FRAC_PI_2)
            * Mat4::from_translation(-center);
        let half = partial(quarter, 0.5, center);
        let point = center + Vec3::X;
        let turned = half.transform_point3(point) - center;
        // An eighth of a turn, still one unit from the center.
        assert!((turned.length() - 1.0).abs() < 1e-5);
        assert!((turned.angle_between(Vec3::X) - std::f32::consts::FRAC_PI_4).abs() < 1e-4);

        let moved = partial(Mat4::from_translation(Vec3::Y * 2.0), 0.25, center);
        assert!(
            moved
                .transform_point3(Vec3::ZERO)
                .abs_diff_eq(Vec3::Y * 0.5, 1e-6)
        );
    }
}
