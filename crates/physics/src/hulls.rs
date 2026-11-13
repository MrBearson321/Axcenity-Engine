//! Convex shapes for the Model Studio's Collision fold (roadmap R15.2b):
//! a hull of a mesh's corners and a convex decomposition into a given
//! number of pieces, with parry's own algorithms, so the pieces it saves
//! as `_COL` objects are what physics would make. Plain arrays in and out,
//! so callers needn't know rapier's types.

use rapier3d::glamx::Vec3;
use rapier3d::parry::transformation::try_convex_hull;
use rapier3d::parry::transformation::vhacd::{VHACD, VHACDParameters};
use rapier3d::parry::transformation::voxelization::FillMode;

/// A convex piece: its corners and its triangles.
pub type ConvexMesh = (Vec<[f32; 3]>, Vec<[u32; 3]>);

/// The most pieces a decomposition is asked for.
pub const MAX_PIECES: u32 = 64;

/// The convex hull of `corners`, as triangles; `None` when they're flat or
/// fewer than four.
pub fn convex_hull_mesh(corners: &[[f32; 3]]) -> Option<ConvexMesh> {
    let points: Vec<Vec3> = corners.iter().map(|&p| Vec3::from(p)).collect();
    let (points, triangles) = try_convex_hull(&points).ok()?;
    (triangles.len() >= 4).then(|| (points.iter().map(|p| p.to_array()).collect(), triangles))
}

/// At most `pieces` convex pieces (1 to [`MAX_PIECES`]) following the
/// hollows of the mesh with these corners and triangles, each the exact
/// hull of the mesh's own triangles inside it. Empty for a mesh with no
/// volume.
pub fn convex_pieces(corners: &[[f32; 3]], triangles: &[[u32; 3]], pieces: u32) -> Vec<ConvexMesh> {
    if corners.len() < 4 || triangles.is_empty() {
        return Vec::new();
    }
    let points: Vec<Vec3> = corners.iter().map(|&p| Vec3::from(p)).collect();
    let params = VHACDParameters {
        // Tight, as physics' own PreciseConvexDecomposition.
        resolution: 64,
        concavity: 0.001,
        plane_downsampling: 1,
        convex_hull_downsampling: 1,
        alpha: 0.05,
        beta: 0.05,
        convex_hull_approximation: false,
        // As many as it takes; merged down to the count after, since
        // stopping parry early leaves slabs across the hollows.
        max_convex_hulls: 1024,
        fill_mode: FillMode::FloodFill {
            detect_cavities: false,
        },
    };
    let wanted = pieces.clamp(1, MAX_PIECES) as usize;
    if wanted == 1 {
        return convex_hull_mesh(corners).into_iter().collect();
    }
    let decomposition = VHACD::decompose(&params, &points, triangles, true);
    let mut hulls: Vec<Vec<Vec3>> = decomposition
        .compute_exact_convex_hulls(&points, triangles)
        .into_iter()
        .filter(|(_, triangles)| triangles.len() >= 4)
        .map(|(points, _)| points)
        .collect();
    // parry can leave more pieces than asked: the two whose boxes together
    // are smallest become one, until there are few enough. (Merging by
    // nearest middles snowballs: a merged arc's middle moves toward the
    // hole it should leave open.)
    let bounds = |hull: &[Vec3]| {
        hull.iter().fold((Vec3::MAX, Vec3::MIN), |(lo, hi), &p| {
            (lo.min(p), hi.max(p))
        })
    };
    while hulls.len() > wanted {
        let boxes: Vec<(Vec3, Vec3)> = hulls.iter().map(|hull| bounds(hull)).collect();
        let mut smallest = (0, 1, f32::MAX);
        for a in 0..hulls.len() {
            for b in a + 1..hulls.len() {
                let size = boxes[a].1.max(boxes[b].1) - boxes[a].0.min(boxes[b].0);
                let volume = size.x * size.y * size.z;
                if volume < smallest.2 {
                    smallest = (a, b, volume);
                }
            }
        }
        let taken = hulls.swap_remove(smallest.1);
        hulls[smallest.0].extend(taken);
    }
    hulls
        .into_iter()
        .filter_map(|hull| {
            let corners: Vec<[f32; 3]> = hull.iter().map(|p| p.to_array()).collect();
            convex_hull_mesh(&corners)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A U: two posts and a bar across their tops, with a doorway under it.
    fn arch() -> (Vec<[f32; 3]>, Vec<[u32; 3]>) {
        let mut corners = Vec::new();
        let mut triangles = Vec::new();
        for (min, max) in [
            ([0.0, 0.0, 0.0], [1.0, 3.0, 1.0]),
            ([3.0, 0.0, 0.0], [4.0, 3.0, 1.0]),
            ([0.0, 3.0, 0.0], [4.0, 4.0, 1.0]),
        ] {
            let base = corners.len() as u32;
            for k in 0..8 {
                corners.push([
                    if k & 1 != 0 { max[0] } else { min[0] },
                    if k & 2 != 0 { max[1] } else { min[1] },
                    if k & 4 != 0 { max[2] } else { min[2] },
                ]);
            }
            for [a, b, c, d] in [
                [0, 2, 3, 1],
                [4, 5, 7, 6],
                [0, 1, 5, 4],
                [2, 6, 7, 3],
                [0, 4, 6, 2],
                [1, 3, 7, 5],
            ] {
                triangles.push([base + a, base + b, base + c]);
                triangles.push([base + a, base + c, base + d]);
            }
        }
        (corners, triangles)
    }

    #[test]
    fn a_hull_closes_the_doorway_and_pieces_keep_it_open() {
        let (corners, triangles) = arch();
        let (hull, faces) = convex_hull_mesh(&corners).unwrap();
        assert!(hull.len() >= 8 && faces.len() >= 12);

        let pieces = convex_pieces(&corners, &triangles, 8);
        assert!(
            pieces.len() >= 2 && pieces.len() <= 8,
            "{} pieces",
            pieces.len()
        );
        // The doorway's middle (2, 1, 0.5) is in no piece.
        let inside = |(points, faces): &ConvexMesh, p: Vec3| {
            faces.iter().all(|&[a, b, c]| {
                let [a, b, c] = [a, b, c].map(|k| Vec3::from(points[k as usize]));
                (b - a).cross(c - a).dot(p - a) <= 1e-4
            })
        };
        let door = Vec3::new(2.0, 1.0, 0.5);
        assert!(pieces.iter().all(|piece| !inside(piece, door)));
        // A post's middle is in one.
        assert!(
            pieces
                .iter()
                .any(|piece| inside(piece, Vec3::new(0.5, 1.0, 0.5)))
        );

        assert_eq!(convex_pieces(&corners, &triangles, 1).len(), 1);
        assert!(convex_pieces(&[], &[], 4).is_empty());
        assert!(convex_hull_mesh(&[[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]).is_none());
    }
}
