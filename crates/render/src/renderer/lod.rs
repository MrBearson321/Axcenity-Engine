//! Levels of detail: which of a mesh's simpler copies
//! ([`MeshSource::lod`]) an item is drawn with, by how big it is on
//! screen, with a short dithered cross-fade when it switches. Also the
//! cull distance and its fade ([`DrawItem::cull_distance`]).
//!
//! An item's level is kept between frames by its [`DrawItem::id`] and
//! mesh, so it switches only once its size is clearly past a threshold
//! (hysteresis), and doesn't flicker when it sits on one.

use std::collections::HashMap;
use std::sync::Arc;

use super::culling::{Bounds, Sphere};
use super::types::MeshKey;
use crate::{Camera, DrawItem, DrawList, MeshSource, Shape};

/// Below these sizes on screen (the fraction of the screen's height its
/// bounding sphere covers), an item is drawn with its mesh's first, then
/// second, simpler copy.
pub(super) const LOD_SCREEN_SIZES: [f32; 2] = [0.3, 0.12];

/// How far past a threshold, as a fraction of it, an item's size must go
/// before its level changes.
const HYSTERESIS: f32 = 0.15;

/// How many frames a switch between levels cross-fades over.
pub(super) const FADE_FRAMES: u64 = 12;

/// The share of an item's cull distance it fades out over, at the far end.
const CULL_FADE: f32 = 0.1;

/// An item not drawn for this many frames has its level forgotten.
const FORGET_FRAMES: u64 = 600;

/// The fade of a draw that is wholly drawn (see `InstanceRaw::emissive_fade`).
pub(super) const OPAQUE: f32 = 1.0;

/// One copy of an item the frame draws: the item, the mesh it's drawn
/// with (its own, or one of its levels of detail) and how much of it is
/// drawn. A switching item is drawn twice, fading one level in and the
/// other out.
#[derive(Clone)]
pub(super) struct Draw<'a> {
    pub(super) item: &'a DrawItem,
    /// The simpler copy it's drawn with, or `None` for the item's own shape.
    pub(super) lod: Option<Arc<dyn MeshSource>>,
    /// 1 draws it all, below 1 fades it in and above 1 fades it out
    /// (`dithered_out` in shader.wgsl).
    pub(super) fade: f32,
    /// False for the copy fading out of a switch: only the level coming in
    /// casts a shadow.
    pub(super) casts_shadow: bool,
}

impl<'a> Draw<'a> {
    fn of(item: &'a DrawItem) -> Self {
        Self {
            item,
            lod: None,
            fade: OPAQUE,
            casts_shadow: item.casts_shadow,
        }
    }

    pub(super) fn mesh(&self) -> MeshKey {
        match &self.lod {
            Some(lod) => MeshKey::Custom(lod.key()),
            None => MeshKey::of(&self.item.shape),
        }
    }

    /// The caller's mesh it's drawn with, to upload, or `None` for a
    /// built-in shape.
    pub(super) fn source(&self) -> Option<&Arc<dyn MeshSource>> {
        match (&self.lod, &self.item.shape) {
            (Some(lod), _) => Some(lod),
            (None, Shape::Mesh(source)) => Some(source),
            (None, _) => None,
        }
    }
}

/// How big `sphere` is on screen from `camera`: the fraction of the
/// screen's height it covers. Infinite with the camera inside it.
pub(super) fn screen_size((center, radius): Sphere, camera: &Camera) -> f32 {
    let distance = center.distance(camera.position);
    if distance <= radius {
        return f32::INFINITY;
    }
    let tan_half = (camera.fov_y_degrees.to_radians() * 0.5).tan().max(1e-4);
    radius / (distance * tan_half)
}

/// The level to draw an item `size` big on screen with, out of `levels`
/// simpler copies: 0 is its own mesh. `current` is the level it was drawn
/// with last frame, if any: it's kept until the size is past the next
/// threshold by [`HYSTERESIS`].
pub(super) fn lod_level(size: f32, current: Option<usize>, levels: usize) -> usize {
    let levels = levels.min(LOD_SCREEN_SIZES.len());
    let Some(current) = current else {
        return LOD_SCREEN_SIZES[..levels]
            .iter()
            .filter(|&&threshold| size < threshold)
            .count();
    };
    let mut level = current.min(levels);
    while level > 0 && size > LOD_SCREEN_SIZES[level - 1] * (1.0 + HYSTERESIS) {
        level -= 1;
    }
    while level < levels && size < LOD_SCREEN_SIZES[level] * (1.0 - HYSTERESIS) {
        level += 1;
    }
    level
}

/// An item's level, kept between frames.
struct LodState {
    level: usize,
    /// The level it's switching from and the frame the switch began, while
    /// it cross-fades.
    fading_from: Option<(usize, u64)>,
    last_drawn: u64,
}

/// Every item's level of detail, by its ID and mesh.
#[derive(Default)]
pub(super) struct LodStates {
    states: HashMap<(u64, u64), LodState>,
}

/// What [`LodStates::resolve`] gives a frame.
pub(super) struct Resolved<'a> {
    pub(super) draws: Vec<Draw<'a>>,
    /// Items past their cull distance, which aren't drawn at all.
    pub(super) culled: u32,
}

impl LodStates {
    /// The copies of `list`'s items that frame `frame` draws, each with its
    /// level and fade.
    pub(super) fn resolve<'a>(&mut self, list: &'a DrawList, frame: u64) -> Resolved<'a> {
        let camera = &list.camera;
        let mut resolved = Resolved {
            draws: Vec::with_capacity(list.items.len()),
            culled: 0,
        };
        for item in &list.items {
            let mut draw = Draw::of(item);
            if item.cull_distance > 0.0 {
                let distance = item.transform.w_axis.truncate().distance(camera.position);
                // A NaN distance culls rather than draws.
                if distance > item.cull_distance || distance.is_nan() {
                    resolved.culled += 1;
                    continue;
                }
                let fade = (item.cull_distance - distance) / (item.cull_distance * CULL_FADE);
                draw.fade = fade.min(OPAQUE);
            }
            let Shape::Mesh(source) = &item.shape else {
                resolved.draws.push(draw);
                continue;
            };
            let levels = source.lod_count();
            if levels == 0 {
                resolved.draws.push(draw);
                continue;
            }
            // Every shape fits the unit cube, so all the parts of one model
            // are the same size on screen and switch together.
            let size = screen_size(Bounds::UNIT.sphere(item.transform), camera);
            let with_level = |level: usize| Draw {
                lod: (level > 0).then(|| source.lod(level)).flatten(),
                ..draw.clone()
            };
            if item.id == 0 {
                resolved
                    .draws
                    .push(with_level(lod_level(size, None, levels)));
                continue;
            }
            let state = self
                .states
                .entry((item.id, source.key()))
                .or_insert_with(|| LodState {
                    level: lod_level(size, None, levels),
                    fading_from: None,
                    last_drawn: frame,
                });
            if state.last_drawn + 1 < frame {
                // Not drawn last frame (hidden, or just shown): no fade.
                state.level = lod_level(size, None, levels);
                state.fading_from = None;
            } else {
                let level = lod_level(size, Some(state.level), levels);
                if level != state.level {
                    state.fading_from = Some((state.level, frame));
                    state.level = level;
                }
            }
            state.last_drawn = frame;
            match state.fading_from {
                Some((from, start)) if frame - start < FADE_FRAMES => {
                    let shown = (frame - start + 1) as f32 / (FADE_FRAMES + 1) as f32;
                    resolved.draws.push(Draw {
                        fade: shown,
                        ..with_level(state.level)
                    });
                    resolved.draws.push(Draw {
                        fade: OPAQUE + shown,
                        casts_shadow: false,
                        ..with_level(from)
                    });
                }
                _ => {
                    state.fading_from = None;
                    resolved.draws.push(with_level(state.level));
                }
            }
        }
        if frame.is_multiple_of(60) {
            self.states
                .retain(|_, state| frame - state.last_drawn < FORGET_FRAMES);
        }
        resolved
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_follow_the_size_on_screen() {
        assert_eq!(lod_level(1.0, None, 2), 0);
        assert_eq!(lod_level(0.2, None, 2), 1);
        assert_eq!(lod_level(0.05, None, 2), 2);
        // A mesh with one simpler copy never goes past it.
        assert_eq!(lod_level(0.05, None, 1), 1);
        assert_eq!(lod_level(0.05, None, 0), 0);
    }

    #[test]
    fn a_size_on_the_threshold_keeps_the_level_it_had() {
        let [threshold, _] = LOD_SCREEN_SIZES;
        for size in [threshold * 0.95, threshold, threshold * 1.05] {
            assert_eq!(lod_level(size, Some(0), 2), 0, "{size}");
            assert_eq!(lod_level(size, Some(1), 2), 1, "{size}");
        }
        assert_eq!(lod_level(threshold * 0.8, Some(0), 2), 1);
        assert_eq!(lod_level(threshold * 1.2, Some(1), 2), 0);
        // A big jump goes straight to the level for its size.
        assert_eq!(lod_level(0.01, Some(0), 2), 2);
    }

    #[test]
    fn screen_size_halves_with_twice_the_distance() {
        let camera = Camera {
            position: glam::Vec3::ZERO,
            look_at: glam::Vec3::NEG_Z,
            fov_y_degrees: 90.0,
        };
        let near = screen_size((glam::Vec3::new(0.0, 0.0, -10.0), 1.0), &camera);
        let far = screen_size((glam::Vec3::new(0.0, 0.0, -20.0), 1.0), &camera);
        assert!((near - 0.1).abs() < 1e-5, "{near}");
        assert!((far - near * 0.5).abs() < 1e-5, "{far}");
        assert_eq!(screen_size((glam::Vec3::ZERO, 1.0), &camera), f32::INFINITY);
    }
}
