//! World streaming on the host's own screen (Phase 80).
//!
//! With `Workspace.StreamingEnabled` on, what's further than
//! `StreamingTargetRadius` from the camera isn't drawn, so it goes off the
//! GPU (the renderer frees what stops being drawn) and terrain chunks past
//! it go too. A [`Streaming`], owned with the host's [`TerrainCache`], says
//! what's hidden. It uses the same units as the network
//! ([`Partition`]), so a Model loads whole and Persistent stays.
//!
//! [`TerrainCache`]: crate::TerrainCache

use rustc_hash::FxHashSet;

use axcenity_core::streaming::{Partition, Settings};
use axcenity_core::{InstanceId, Vector3, World};
use axcenity_render::glam::Vec3;

/// Seconds between going over what's in range.
const TICK_SECONDS: f64 = 0.2;
/// How long a region `RequestStreamAroundAsync` asked for stays loaded.
const REGION_SECONDS: f64 = 10.0;

/// What's in range of the host's camera. See the module docs.
#[derive(Default)]
pub struct Streaming {
    partition: Partition,
    settings: Option<Settings>,
    /// Everything in a unit out of range.
    hidden: FxHashSet<InstanceId>,
    /// The units in range now, to keep near the edge.
    shown: FxHashSet<InstanceId>,
    regions: Vec<(Vec3, f64)>,
    clock: f64,
    focus: Vec3,
    next_tick: f64,
    urgent: bool,
}

impl Streaming {
    /// The streaming settings, while they're on.
    pub fn settings(&self) -> Option<&Settings> {
        self.settings.as_ref()
    }

    /// Whether `id` is out of range, so not drawn.
    pub fn is_hidden(&self, id: InstanceId) -> bool {
        self.hidden.contains(&id)
    }

    /// Where everything streams around: the camera, and any regions asked for.
    pub fn focuses(&self) -> Vec<Vec3> {
        let mut focuses = vec![self.focus];
        focuses.extend(self.regions.iter().map(|(center, _)| *center));
        focuses
    }

    /// Keeps what's around `position` loaded for a while
    /// (`RequestStreamAroundAsync` on the host's own screen).
    pub fn hold_region(&mut self, position: Vec3) {
        self.regions.push((position, self.clock + REGION_SECONDS));
        self.urgent = true;
    }

    /// Catches up with `world` and the camera, once a frame.
    pub fn step(&mut self, world: &World, focus: Vec3, seconds: f64) {
        self.clock += seconds;
        let clock = self.clock;
        self.regions.retain(|(_, until)| *until > clock);
        self.focus = focus;
        self.settings = Settings::of(world);
        let Some(settings) = self.settings else {
            self.hidden.clear();
            self.shown.clear();
            return;
        };
        self.partition.refresh(world);
        self.next_tick -= seconds;
        if self.next_tick > 0.0 && !self.urgent {
            return;
        }
        self.next_tick = TICK_SECONDS;
        self.urgent = false;
        self.go_over(&settings);
    }

    /// Goes over every unit's distance.
    fn go_over(&mut self, settings: &Settings) {
        let focuses: Vec<Vector3> = self
            .focuses()
            .into_iter()
            .map(|p| Vector3::new(p.x, p.y, p.z))
            .collect();
        let mut hidden = FxHashSet::default();
        let mut shown = FxHashSet::default();
        for unit in self.partition.units() {
            let distance = focuses
                .iter()
                .map(|&focus| unit.distance(focus))
                .fold(f32::MAX, f32::min);
            let radius = if self.shown.contains(&unit.root) {
                settings.unload_radius()
            } else {
                settings.target_radius
            };
            if distance <= radius {
                shown.insert(unit.root);
            } else {
                hidden.extend(unit.members.iter().copied());
            }
        }
        self.hidden = hidden;
        self.shown = shown;
    }

    /// Whether the terrain chunk at `center`, reaching `half` on each side
    /// of it, should be kept (within the unload radius) or built (within
    /// the target radius). `None` while streaming is off.
    pub(crate) fn chunk_in_range(&self, center: Vec3, half: Vec3) -> Option<(bool, bool)> {
        let settings = self.settings.as_ref()?;
        let distance = self
            .focuses()
            .into_iter()
            .map(|focus| {
                let offset = (focus - center).abs() - half;
                offset.max(Vec3::ZERO).length()
            })
            .fold(f32::MAX, f32::min);
        Some((
            distance <= settings.unload_radius(),
            distance <= settings.target_radius,
        ))
    }
}

#[cfg(test)]
mod tests {
    use axcenity_assets::Assets;
    use axcenity_core::{ClassName, TerrainData, Value};
    use axcenity_render::Camera;

    use super::*;
    use crate::{Poses, TerrainCache, build_draw_list_posed};

    /// Streaming on, radii 64 and 200, a part at each x.
    fn world_with_parts(xs: &[f32]) -> World {
        let mut world = World::new();
        let workspace = world.workspace();
        for (name, value) in [
            ("StreamingEnabled", Value::Bool(true)),
            ("StreamingMinRadius", Value::Number(64.0)),
            ("StreamingTargetRadius", Value::Number(200.0)),
        ] {
            world.set_property(workspace, name, value).unwrap();
        }
        for x in xs {
            let part = world.create(ClassName::Part).unwrap();
            world
                .set_property(part, "Position", Value::Vector3(Vector3::new(*x, 0.0, 0.0)))
                .unwrap();
            world.set_parent(part, Some(workspace)).unwrap();
        }
        world
    }

    fn camera() -> Camera {
        Camera {
            position: Vec3::ZERO,
            look_at: Vec3::new(0.0, 0.0, -1.0),
            fov_y_degrees: 60.0,
        }
    }

    fn drawn(world: &World, cache: &mut TerrainCache) -> usize {
        build_draw_list_posed(world, &Assets::default(), camera(), &Poses::new(), cache)
            .items
            .len()
    }

    #[test]
    fn parts_outside_the_radius_arent_drawn_and_come_back_in_range() {
        let world = world_with_parts(&[0.0, 150.0, 900.0]);
        let mut cache = TerrainCache::new();
        cache.streaming.step(&world, Vec3::ZERO, 1.0);
        assert_eq!(drawn(&world, &mut cache), 2, "the far part isn't drawn");

        cache
            .streaming
            .step(&world, Vec3::new(900.0, 0.0, 0.0), 1.0);
        assert_eq!(drawn(&world, &mut cache), 1, "now only the far one is near");
    }

    #[test]
    fn with_streaming_off_everything_is_drawn() {
        let mut world = world_with_parts(&[0.0, 900.0, 5000.0]);
        let workspace = world.workspace();
        world
            .set_property(workspace, "StreamingEnabled", Value::Bool(false))
            .unwrap();
        let mut cache = TerrainCache::new();
        cache.streaming.step(&world, Vec3::ZERO, 1.0);
        assert_eq!(drawn(&world, &mut cache), 3);
    }

    #[test]
    fn a_persistent_model_stays_drawn_however_far() {
        let mut world = world_with_parts(&[]);
        let workspace = world.workspace();
        let model = world.create(ClassName::Model).unwrap();
        world
            .set_property(
                model,
                "ModelStreamingMode",
                axcenity_core::streaming::streaming_mode("Persistent"),
            )
            .unwrap();
        world.set_parent(model, Some(workspace)).unwrap();
        let part = world.create(ClassName::Part).unwrap();
        world
            .set_property(
                part,
                "Position",
                Value::Vector3(Vector3::new(5000.0, 0.0, 0.0)),
            )
            .unwrap();
        world.set_parent(part, Some(model)).unwrap();
        let mut cache = TerrainCache::new();
        cache.streaming.step(&world, Vec3::ZERO, 1.0);
        assert_eq!(drawn(&world, &mut cache), 1);
    }

    #[test]
    fn a_region_held_for_a_teleport_is_loaded_before_the_camera_goes_there() {
        let world = world_with_parts(&[0.0, 900.0]);
        let mut cache = TerrainCache::new();
        cache.streaming.step(&world, Vec3::ZERO, 1.0);
        assert_eq!(drawn(&world, &mut cache), 1);
        cache.streaming.hold_region(Vec3::new(900.0, 0.0, 0.0));
        cache.streaming.step(&world, Vec3::ZERO, 0.01);
        assert_eq!(drawn(&world, &mut cache), 2, "both are in range now");
    }

    /// A Terrain 1000 studs square centered on the origin, with one chunk
    /// every `CHUNK_CELLS` cells, under `world_with_parts`' settings.
    fn terrain_world() -> (World, InstanceId) {
        let mut world = world_with_parts(&[]);
        let workspace = world.workspace();
        let terrain = world.create(ClassName::Terrain).unwrap();
        world
            .set_property(
                terrain,
                "Position",
                Value::Vector3(Vector3::new(-500.0, 0.0, -500.0)),
            )
            .unwrap();
        world
            .set_property(
                terrain,
                "Size",
                Value::Vector3(Vector3::new(1000.0, 64.0, 1000.0)),
            )
            .unwrap();
        world.set_parent(terrain, Some(workspace)).unwrap();
        world.set_terrain(terrain, std::sync::Arc::new(TerrainData::default()));
        (world, terrain)
    }

    fn chunk_items(world: &World, cache: &mut TerrainCache) -> usize {
        build_draw_list_posed(world, &Assets::default(), camera(), &Poses::new(), cache)
            .items
            .iter()
            .filter(|item| item.material.terrain.is_some())
            .count()
    }

    #[test]
    fn chunks_out_of_range_are_left_out_and_built_on_a_thread_when_they_come_in_range() {
        let (world, terrain) = terrain_world();
        let data = world.terrain(terrain).unwrap();
        let (cx, cz) = data.chunk_counts();
        let all = cx * cz;
        assert!(all > 1, "more than one chunk: {all}");

        // Without streaming every chunk is drawn, in the same frame.
        let mut off = world.clone();
        let workspace = off.workspace();
        off.set_property(workspace, "StreamingEnabled", Value::Bool(false))
            .unwrap();
        let mut cache = TerrainCache::new();
        cache.streaming.step(&off, Vec3::ZERO, 1.0);
        assert_eq!(chunk_items(&off, &mut cache), all);

        // With it, none at first (they're being built), then what's near.
        let mut cache = TerrainCache::new();
        cache.streaming.step(&world, Vec3::ZERO, 1.0);
        assert_eq!(chunk_items(&world, &mut cache), 0);
        assert!(cache.pending_chunks() > 0, "jobs were started");
        let mut drew = 0;
        for _ in 0..2000 {
            drew = chunk_items(&world, &mut cache);
            if cache.pending_chunks() == 0 && drew > 0 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert!(drew > 0, "chunks in range were built on the thread");
        assert!(
            drew < all,
            "chunks past the radius weren't: {drew} of {all}"
        );

        // The camera moves away: the far chunks go, and with them their
        // memory; moving back, they're built again.
        cache
            .streaming
            .step(&world, Vec3::new(5000.0, 0.0, 5000.0), 1.0);
        assert_eq!(chunk_items(&world, &mut cache), 0, "nothing near any more");
        let (kept, _) = cache
            .streaming
            .chunk_in_range(Vec3::ZERO, Vec3::splat(0.0))
            .unwrap();
        assert!(!kept);
    }

    #[test]
    fn prepare_builds_chunks_right_away_for_the_loading_screen() {
        let (world, terrain) = terrain_world();
        let mut cache = TerrainCache::new();
        cache.streaming.step(&world, Vec3::ZERO, 1.0);
        cache.prepare(&world, terrain, &Assets::default(), &camera());
        assert_eq!(cache.pending_chunks(), 0, "built here, not on the thread");
        assert!(
            chunk_items(&world, &mut cache) > 0,
            "the next frame can already draw them"
        );
    }
}
