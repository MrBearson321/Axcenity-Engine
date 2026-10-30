//! Turning a part into ground: the editor's Solid Modeling ▸ To Terrain.
//!
//! The heightfield has one height per sample, so a part becomes the ground
//! under its top: every sample under it rises to where its top is there
//! (never lowered), and is painted with the layer that matches its look.
//! Overhangs and the space under the part can't be kept.

use axcenity_core::{MAX_TERRAIN_LAYERS, SampleRect, TerrainData};
use glam::Vec2;

use crate::Placement;
use crate::brush::{EditRecorder, Operation, PaintMode, paint_sample};

/// Raises the ground in the world XZ box `min`..`max` up to `top` (the
/// part's top, world Y, or `None` where it doesn't cover the ground), never
/// lowering it, and paints `layer` fully where the part covers. Heights are
/// kept inside the terrain's range. Each changed chunk is kept in
/// `recorder` first, for undo. Returns how many samples the part covered:
/// 0 when it's off the terrain.
pub fn raise_to(
    data: &mut TerrainData,
    placement: &Placement,
    (min, max): (Vec2, Vec2),
    top: impl Fn(Vec2) -> Option<f32>,
    layer: Option<usize>,
    recorder: &mut EditRecorder,
) -> usize {
    let (size_x, size_z) = data.size();
    let low = placement.to_grid(data, min.x, min.y).floor();
    let high = placement.to_grid(data, max.x, max.y).ceil();
    let clip = |value: f32, size: usize| value.clamp(0.0, (size - 1) as f32) as usize;
    let (x0, z0) = (clip(low.x, size_x), clip(low.y, size_z));
    let (x1, z1) = (clip(high.x, size_x), clip(high.y, size_z));
    if low.x > (size_x - 1) as f32 || low.y > (size_z - 1) as f32 || high.x < 0.0 || high.y < 0.0 {
        return 0;
    }
    // What changes, worked out before anything is changed.
    let mut covered = Vec::new();
    for z in z0..=z1 {
        for x in x0..=x1 {
            let at = placement.to_world_xz(data, Vec2::new(x as f32, z as f32));
            if let Some(y) = top(at).filter(|y| y.is_finite()) {
                let height = placement.height_of(y).clamp(0.0, 1.0);
                covered.push((data.index(x, z), height));
            }
        }
    }
    if covered.is_empty() {
        return 0;
    }
    let rect = SampleRect::new(x0, z0, x1 + 1, z1 + 1);
    recorder.before_change(data, rect);
    let paint = layer.map(|layer| Operation::Paint {
        layer: layer.min(MAX_TERRAIN_LAYERS - 1),
        mode: PaintMode::Replace,
        locked: [false; MAX_TERRAIN_LAYERS],
    });
    let (heights, weights) = data.both_mut_in(rect);
    for &(index, height) in &covered {
        heights[index] = heights[index].max(height);
        if let Some(paint) = &paint {
            weights[index] = paint_sample(weights[index], paint, 1.0);
        }
    }
    covered.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    #[test]
    fn a_part_raises_the_ground_under_it_to_its_top_and_paints_it() {
        let mut data = TerrainData::flat(33, 33, 0.1).unwrap();
        // 64 studs across, heights from y = 0 to y = 100.
        let placement = Placement {
            position: Vec3::ZERO,
            size: Vec3::new(64.0, 100.0, 64.0),
        };
        let mut recorder = EditRecorder::new();
        // A block 8 studs wide whose top is at y = 30, and a part sunk
        // below the ground at y = 5, which leaves it alone.
        let top = |at: Vec2| (at.x.abs() <= 4.0 && at.y.abs() <= 4.0).then_some(30.0);
        let covered = raise_to(
            &mut data,
            &placement,
            (Vec2::splat(-4.0), Vec2::splat(4.0)),
            top,
            Some(2),
            &mut recorder,
        );
        // Samples every 2 studs: 5 by 5 under the block.
        assert_eq!(covered, 25);
        let height = |data: &TerrainData, x: f32, z: f32| placement.ground_y(data, x, z).unwrap();
        assert!((height(&data, 0.0, 0.0) - 30.0).abs() < 1e-3);
        assert!((height(&data, 4.0, -4.0) - 30.0).abs() < 1e-3);
        assert!(
            (height(&data, 8.0, 0.0) - 10.0).abs() < 1e-3,
            "outside it, as it was"
        );
        let middle = data.index(16, 16);
        assert_eq!(data.paint()[middle][2], 255);
        let edit = recorder.finish(&data).unwrap();

        let mut recorder = EditRecorder::new();
        let sunk = raise_to(
            &mut data,
            &placement,
            (Vec2::splat(10.0), Vec2::splat(14.0)),
            |_| Some(5.0),
            None,
            &mut recorder,
        );
        assert_eq!(sunk, 9);
        assert!(
            (height(&data, 12.0, 12.0) - 10.0).abs() < 1e-3,
            "never lowered"
        );
        // Off the terrain, nothing.
        let off = raise_to(
            &mut data,
            &placement,
            (Vec2::splat(100.0), Vec2::splat(110.0)),
            |_| Some(50.0),
            None,
            &mut EditRecorder::new(),
        );
        assert_eq!(off, 0);

        edit.undo(&mut data);
        assert!((height(&data, 0.0, 0.0) - 10.0).abs() < 1e-3);
    }
}
