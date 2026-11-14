//! Flatten Terrain Along: a strip of terrain under a spline brought to the
//! curve's height, for a road to lie on.

use axcenity_core::TerrainData;
use axcenity_core::spline::SplineCurve;
use axcenity_terrain::Placement;
use glam::{Vec2, Vec3};

use crate::v3;

/// Puts every sample of `data` within `width / 2` studs of `curve` (seen
/// from above) at the height of the curve's nearest point. Returns how many
/// samples it set.
pub fn flatten_along(
    data: &mut TerrainData,
    placement: &Placement,
    curve: &SplineCurve,
    width: f32,
) -> usize {
    let cell = placement.cell_size(data).min_element().max(1e-3);
    let line: Vec<Vec3> = curve
        .polyline((cell / 2.0).max(0.25))
        .into_iter()
        .map(v3)
        .collect();
    let reach = (width / 2.0).max(0.0);
    let (size_x, _) = data.size();
    let Some(rect) = crate::paths::rect_around(data, placement, &line, reach) else {
        return 0;
    };
    // Worked out first, so the heights aren't borrowed while reading.
    let mut set = Vec::new();
    for z in rect.z0..rect.z1 {
        for x in rect.x0..rect.x1 {
            let at = placement.to_world_xz(data, Vec2::new(x as f32, z as f32));
            if let Some((away, y)) = nearest(&line, at)
                && away <= reach
            {
                set.push((z * size_x + x, placement.height_of(y)));
            }
        }
    }
    let heights = data.heights_mut_in(rect);
    for &(index, height) in &set {
        heights[index] = height;
    }
    data.clamp_heights_in(rect);
    set.len()
}

/// How far `at` is from the line seen from above, and the line's height at
/// its nearest point.
pub(crate) fn nearest(line: &[Vec3], at: Vec2) -> Option<(f32, f32)> {
    if line.len() == 1 {
        let p = line[0];
        return Some((Vec2::new(p.x, p.z).distance(at), p.y));
    }
    line.windows(2)
        .map(|pair| {
            let (a, b) = (pair[0], pair[1]);
            let (fa, fb) = (Vec2::new(a.x, a.z), Vec2::new(b.x, b.z));
            let along = fb - fa;
            let t = if along.length_squared() > 1e-12 {
                ((at - fa).dot(along) / along.length_squared()).clamp(0.0, 1.0)
            } else {
                0.0
            };
            ((fa + along * t).distance(at), a.y + (b.y - a.y) * t)
        })
        .min_by(|x, y| x.0.total_cmp(&y.0))
}
