//! Where a ray (from the camera through the mouse) meets a terrain.

use axcenity_core::TerrainData;
use glam::Vec3;

use crate::Placement;

/// How far along the ray the terrain is first hit, in units of `direction`,
/// or `None` if it isn't. Steps along the ray half a cell at a time, then
/// narrows down the crossing point.
pub fn raycast(
    data: &TerrainData,
    placement: &Placement,
    origin: Vec3,
    direction: Vec3,
) -> Option<f32> {
    let length = direction.length();
    if length < 1e-9 || !origin.is_finite() || !direction.is_finite() {
        return None;
    }
    let low = placement.position - Vec3::new(placement.size.x / 2.0, 0.0, placement.size.z / 2.0);
    let high = low + placement.size;
    let (enter, exit) = box_span(origin, direction, low, high)?;
    let cell = placement.cell_size(data).min_element().max(1e-4);
    let step = (cell * 0.5) / length;
    // How far above the ground a point on the ray is (negative below).
    let above = |t: f32| {
        let point = origin + direction * t;
        let grid = placement.to_grid(data, point.x, point.z);
        point.y - placement.world_y(data.height_at(grid.x, grid.y))
    };
    let mut t = enter.max(0.0);
    if above(t) < 0.0 {
        // Starts under the ground: that's the hit.
        return Some(t);
    }
    while t < exit {
        let next = (t + step).min(exit);
        let now = above(next);
        if now <= 0.0 {
            // Somewhere between `t` and `next`: halve the gap a few times.
            let (mut a, mut b) = (t, next);
            for _ in 0..20 {
                let middle = (a + b) / 2.0;
                if above(middle) > 0.0 {
                    a = middle;
                } else {
                    b = middle;
                }
            }
            return Some(b);
        }
        t = next;
    }
    None
}

/// Where a ray enters and leaves a box (slab test), or `None` if it misses.
fn box_span(origin: Vec3, direction: Vec3, low: Vec3, high: Vec3) -> Option<(f32, f32)> {
    let mut enter = f32::NEG_INFINITY;
    let mut exit = f32::INFINITY;
    for axis in 0..3 {
        let (o, d) = (origin[axis], direction[axis]);
        if d.abs() < 1e-12 {
            if o < low[axis] || o > high[axis] {
                return None;
            }
            continue;
        }
        let (a, b) = ((low[axis] - o) / d, (high[axis] - o) / d);
        enter = enter.max(a.min(b));
        exit = exit.min(a.max(b));
    }
    (exit >= enter.max(0.0)).then_some((enter, exit))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ray_down_hits_the_ground_and_one_beside_it_misses() {
        let mut data = TerrainData::flat(33, 33, 0.25).unwrap();
        let placement = Placement {
            position: Vec3::new(0.0, -10.0, 0.0),
            size: Vec3::new(64.0, 40.0, 64.0),
        };
        let down = Vec3::new(0.0, -1.0, 0.0);
        let t = raycast(&data, &placement, Vec3::new(3.0, 50.0, -7.0), down).unwrap();
        assert!((t - 50.0).abs() < 1e-3, "{t}");
        assert!(raycast(&data, &placement, Vec3::new(100.0, 50.0, 0.0), down).is_none());
        assert!(raycast(&data, &placement, Vec3::new(0.0, 50.0, 0.0), -down).is_none());

        // A slanted ray finds a hill in its way.
        let all = data.bounds();
        data.heights_mut_in(all)[16 * 33 + 16] = 1.0;
        let from = Vec3::new(-30.0, 20.0, 0.0);
        let t = raycast(&data, &placement, from, Vec3::new(1.0, -0.2, 0.0)).unwrap();
        let hit = from + Vec3::new(1.0, -0.2, 0.0) * t;
        assert!(hit.x < 0.0 && hit.y > 0.0, "{hit}");
    }
}
