//! Levels of detail: which of a mesh's simpler copies
//! ([`MeshSource::lod`]) an item is drawn with, by how big it is on
//! screen, with a short dithered cross-fade when it switches. Also the
//! cull distance and its fade ([`DrawItem::cull_distance`]).
//!
//! An item's level is kept between frames by its [`DrawItem::id`] and
//! mesh, so it switches only once its size is clearly past a threshold
//! (hysteresis), and doesn't flicker when it sits on one.

use std::sync::{Arc, Mutex, PoisonError};

use rustc_hash::FxHashMap;

use super::culling::{Bounds, Reach, Sphere};
use super::parallel::{map_each, map_parts, worth_sharing};
use super::types::MeshKey;
use crate::{Camera, DrawItem, DrawList, ItemChunk, MeshSource, Shape};

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
    pub(super) lod: Option<&'a Arc<dyn MeshSource>>,
    /// Its level of detail (0 for its own mesh), or `None` for an item
    /// whose mesh has no simpler copies.
    pub(super) level: Option<usize>,
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
            level: None,
            fade: OPAQUE,
            casts_shadow: item.casts_shadow,
        }
    }

    pub(super) fn mesh(&self) -> MeshKey {
        match self.lod {
            Some(lod) => MeshKey::Custom(lod.key()),
            None => MeshKey::of(&self.item.shape),
        }
    }

    /// The caller's mesh it's drawn with, to upload, or `None` for a
    /// built-in shape.
    pub(super) fn source(&self) -> Option<&Arc<dyn MeshSource>> {
        match (self.lod, &self.item.shape) {
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

/// An item's ID and mesh, what its level of detail is kept by.
type LodKey = (u64, u64);

/// Every item's level of detail, by its ID and mesh. Each state has a lock
/// of its own so the threads resolving a frame can share the map: each lock
/// is held only for a moment, and no two threads want the same item.
#[derive(Default)]
pub(super) struct LodStates {
    states: FxHashMap<LodKey, Mutex<LodState>>,
}

/// What [`LodStates::resolve`] gives a frame.
pub(super) struct Resolved<'a> {
    pub(super) draws: Vec<Draw<'a>>,
    /// Items past their cull distance, which aren't drawn at all.
    pub(super) culled: u32,
    /// Every item the list holds, in its groups too.
    pub(super) submitted: u32,
}

/// What one thread resolved. Parts are put together in the list's order, so
/// the frame doesn't depend on how the work was shared out.
#[derive(Default)]
struct Part<'a> {
    draws: Vec<Draw<'a>>,
    culled: u32,
    submitted: u32,
    /// Items seen for the first time, whose states are added afterwards.
    born: Vec<(LodKey, LodState)>,
}

impl LodStates {
    /// The copies of `list`'s items that frame `frame` draws, each with its
    /// level and fade. A group of items outside `reach` (what the frame's
    /// views see, or `None` to keep everything) is passed over whole, and
    /// the items of a group that's kept are checked one by one. The work is
    /// shared among the CPU's threads.
    pub(super) fn resolve<'a>(
        &mut self,
        list: &'a DrawList,
        frame: u64,
        reach: Option<&Reach>,
    ) -> Resolved<'a> {
        let camera = &list.camera;
        let states = &self.states;
        let mut parts = map_parts(&list.items, |items| {
            let mut part = Part {
                draws: Vec::with_capacity(items.len()),
                submitted: items.len() as u32,
                ..Part::default()
            };
            for item in items {
                resolve_item(states, item, camera, frame, &mut part);
            }
            part
        });
        // Most groups of a big terrain are passed over whole, which is too
        // quick to share out: only the rest are.
        let (mut kept, mut in_kept) = (Vec::new(), 0);
        let (mut groups_submitted, mut groups_culled) = (0, 0);
        for chunk in &list.chunks {
            let count = chunk.items.len();
            groups_submitted += count as u32;
            let far = chunk.cull_distance > 0.0
                && chunk.center.distance(camera.position) - chunk.radius > chunk.cull_distance;
            if far || reach.is_some_and(|reach| !reach.touches((chunk.center, chunk.radius))) {
                groups_culled += count as u32;
            } else {
                kept.push(chunk);
                in_kept += count;
            }
        }
        parts.extend(map_each(&kept, worth_sharing(in_kept), |chunk| {
            let chunk: &'a ItemChunk = chunk;
            let mut part = Part::default();
            part.draws.reserve(chunk.items.len());
            for item in chunk.items.iter() {
                if let Some(reach) = reach {
                    let (center, radius) = item.resting_ball();
                    let swayed = radius + item.sway.map_or(0.0, |sway| sway.reach());
                    if item.skin.is_none() && !reach.touches((center, swayed)) {
                        part.culled += 1;
                        continue;
                    }
                }
                resolve_item(states, item, camera, frame, &mut part);
            }
            part
        }));
        let mut resolved = Resolved {
            draws: Vec::with_capacity(parts.iter().map(|part| part.draws.len()).sum()),
            culled: groups_culled,
            submitted: groups_submitted,
        };
        for part in parts {
            resolved.draws.extend(part.draws);
            resolved.culled += part.culled;
            resolved.submitted += part.submitted;
            self.states.extend(
                part.born
                    .into_iter()
                    .map(|(key, state)| (key, Mutex::new(state))),
            );
        }
        if frame.is_multiple_of(60) {
            self.states.retain(|_, state| {
                let state = state.get_mut().unwrap_or_else(PoisonError::into_inner);
                frame - state.last_drawn < FORGET_FRAMES
            });
        }
        resolved
    }
}

/// Adds the copies of `item` the frame draws to `part`.
fn resolve_item<'a>(
    states: &FxHashMap<LodKey, Mutex<LodState>>,
    item: &'a DrawItem,
    camera: &Camera,
    frame: u64,
    part: &mut Part<'a>,
) {
    let mut draw = Draw::of(item);
    if item.cull_distance > 0.0 {
        let distance = item.transform.w_axis.truncate().distance(camera.position);
        // A NaN distance culls rather than draws.
        if distance > item.cull_distance || distance.is_nan() {
            part.culled += 1;
            return;
        }
        let fade = (item.cull_distance - distance) / (item.cull_distance * CULL_FADE);
        draw.fade = fade.min(OPAQUE);
    }
    let Shape::Mesh(source) = &item.shape else {
        part.draws.push(draw);
        return;
    };
    let levels = source.lod_count();
    if levels == 0 {
        part.draws.push(draw);
        return;
    }
    // Every shape fits the unit cube, so all the parts of one model
    // are the same size on screen and switch together.
    let size = screen_size(Bounds::UNIT.sphere(item.transform), camera);
    let with_level = |level: usize| Draw {
        lod: (level > 0).then(|| source.lod(level)).flatten(),
        level: Some(level),
        ..draw.clone()
    };
    if item.id == 0 {
        part.draws.push(with_level(lod_level(size, None, levels)));
        return;
    }
    let key = (item.id, source.key());
    let Some(state) = states.get(&key) else {
        let level = lod_level(size, None, levels);
        part.born.push((
            key,
            LodState {
                level,
                fading_from: None,
                last_drawn: frame,
            },
        ));
        part.draws.push(with_level(level));
        return;
    };
    let mut state = state.lock().unwrap_or_else(PoisonError::into_inner);
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
            part.draws.push(Draw {
                fade: shown,
                ..with_level(state.level)
            });
            part.draws.push(Draw {
                fade: OPAQUE + shown,
                casts_shadow: false,
                ..with_level(from)
            });
        }
        _ => {
            state.fading_from = None;
            part.draws.push(with_level(state.level));
        }
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
