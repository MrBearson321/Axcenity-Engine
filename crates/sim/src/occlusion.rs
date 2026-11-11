//! Which walls stand between each spatial Sound and the listener (roadmap
//! G8). A few Sounds are checked each frame, in turn, so the cost stays
//! the same however many Sounds play; `audio` muffles each by its count
//! ([`axcenity_core::sound::muffle`]).

use std::collections::HashMap;

use axcenity_assets::Assets;
use axcenity_core::sound::{MAX_WALLS, emitter_part, emitter_position};
use axcenity_core::{ClassName, InstanceId, Value, Vector3, World};
use axcenity_physics::QueryParams;

/// The most Sounds checked in one frame. With more playing, each is
/// checked every few frames; the audio glides between counts.
pub const SOUNDS_PER_FRAME: usize = 8;

/// The most rays one Sound's check casts: a ray for each wall, and a few
/// more to pass loose parts (characters, props) that don't count.
pub const RAYS_PER_SOUND: usize = MAX_WALLS as usize + 3;

/// The walls in front of each spatial Sound, kept between frames.
#[derive(Debug, Default)]
pub struct Occlusion {
    walls: HashMap<InstanceId, u32>,
    /// Where the next frame's checks start in the list of Sounds.
    next: usize,
    /// Rays cast in the last [`step`](Self::step), for tests and stats.
    rays: usize,
}

impl Occlusion {
    /// Checks the next few spatial Sounds playing within earshot of
    /// `listener` (at most [`SOUNDS_PER_FRAME`]) and returns every Sound's
    /// walls. Empty while `SoundService.Occlusion` is off.
    pub fn step(
        &mut self,
        world: &World,
        assets: &Assets,
        listener: Vector3,
    ) -> &HashMap<InstanceId, u32> {
        self.rays = 0;
        let on = world
            .get(world.sound_service())
            .and_then(|service| service.property("Occlusion"))
            .is_none_or(|value| *value != Value::Bool(false));
        if !on {
            self.walls.clear();
            return &self.walls;
        }
        let heard: Vec<(InstanceId, Vector3)> = world
            .all_of(ClassName::Sound)
            .filter_map(|id| in_earshot(world, id, listener))
            .collect();
        self.walls
            .retain(|id, _| heard.iter().any(|(sound, _)| sound == id));
        if heard.is_empty() {
            return &self.walls;
        }
        let start = self.next % heard.len();
        for &(sound, at) in heard
            .iter()
            .cycle()
            .skip(start)
            .take(SOUNDS_PER_FRAME.min(heard.len()))
        {
            let (walls, rays) = walls_between(world, assets, sound, listener, at);
            self.rays += rays;
            self.walls.insert(sound, walls);
        }
        self.next = start + SOUNDS_PER_FRAME;
        &self.walls
    }

    /// Rays cast in the last [`step`](Self::step).
    pub fn rays(&self) -> usize {
        self.rays
    }
}

/// `sound` and where it's heard from, if it's spatial, playing and close
/// enough to `listener` to be heard at all.
fn in_earshot(
    world: &World,
    sound: InstanceId,
    listener: Vector3,
) -> Option<(InstanceId, Vector3)> {
    let instance = world.get(sound)?;
    let flag = |name| instance.property(name) == Some(&Value::Bool(true));
    if !flag("Spatial") || !flag("Playing") || !world.is_descendant_of(sound, world.workspace()) {
        return None;
    }
    let max = match instance.property("RollOffMaxDistance") {
        Some(Value::Number(max)) => *max as f32,
        _ => 150.0,
    };
    let at = emitter_position(world, sound);
    let to = between(listener, at);
    (to.x * to.x + to.y * to.y + to.z * to.z < max * max).then_some((sound, at))
}

/// The anchored, solid parts a ray from `listener` to `at` passes through
/// (up to [`MAX_WALLS`]), and the rays it took. The Sound's own object is
/// passed by, and so are loose parts, which move: a character walking
/// past shouldn't muffle the music.
fn walls_between(
    world: &World,
    assets: &Assets,
    sound: InstanceId,
    listener: Vector3,
    at: Vector3,
) -> (u32, usize) {
    let mut params = QueryParams {
        respect_can_collide: true,
        ..QueryParams::default()
    };
    params.filter.extend(own_object(world, sound));
    let direction = between(listener, at);
    let mut walls = 0;
    let mut rays = 0;
    while walls < MAX_WALLS && rays < RAYS_PER_SOUND {
        rays += 1;
        let Some(hit) = axcenity_physics::raycast(world, assets, listener, direction, &params)
        else {
            break;
        };
        if anchored(world, hit.instance) {
            walls += 1;
        }
        params.filter.push(hit.instance);
    }
    (walls, rays)
}

/// What a Sound belongs to: its emitter's top-level object in the
/// Workspace (a whole Model or a lone part), never a wall in front of it.
fn own_object(world: &World, sound: InstanceId) -> Option<InstanceId> {
    let workspace = world.workspace();
    let mut id = emitter_part(world, sound)?;
    while let Some(parent) = world.get(id)?.parent() {
        if parent == workspace {
            return Some(id);
        }
        id = parent;
    }
    None
}

/// The step from `from` to `to`.
fn between(from: Vector3, to: Vector3) -> Vector3 {
    Vector3::new(to.x - from.x, to.y - from.y, to.z - from.z)
}

fn anchored(world: &World, part: InstanceId) -> bool {
    world
        .get(part)
        .is_some_and(|instance| instance.property("Anchored") == Some(&Value::Bool(true)))
        // Terrain is always in place.
        || world.get(part).is_some_and(|instance| instance.class() == ClassName::Terrain)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Scene {
        world: World,
        assets: Assets,
    }

    impl Scene {
        fn new() -> Self {
            Self {
                world: World::new(),
                assets: Assets::default(),
            }
        }

        fn part(&mut self, at: Vector3, size: Vector3, anchored: bool) -> InstanceId {
            let world = &mut self.world;
            let part = world.create(ClassName::Part).unwrap();
            world.set_parent(part, Some(world.workspace())).unwrap();
            world
                .set_property(part, "Position", Value::Vector3(at))
                .unwrap();
            world
                .set_property(part, "Size", Value::Vector3(size))
                .unwrap();
            world
                .set_property(part, "Anchored", Value::Bool(anchored))
                .unwrap();
            part
        }

        /// A wall across the x axis at `x`.
        fn wall(&mut self, x: f32) -> InstanceId {
            self.part(
                Vector3::new(x, 0.0, 0.0),
                Vector3::new(1.0, 20.0, 20.0),
                true,
            )
        }

        /// A playing spatial Sound inside a small part at `x`.
        fn sound(&mut self, x: f32) -> InstanceId {
            let speaker = self.part(Vector3::new(x, 0.0, 0.0), Vector3::new(2.0, 2.0, 2.0), true);
            let world = &mut self.world;
            let sound = world.create(ClassName::Sound).unwrap();
            world.set_parent(sound, Some(speaker)).unwrap();
            for name in ["Spatial", "Playing"] {
                world.set_property(sound, name, Value::Bool(true)).unwrap();
            }
            sound
        }

        fn walls(&mut self, occlusion: &mut Occlusion, sound: InstanceId) -> Option<u32> {
            occlusion
                .step(&self.world, &self.assets, Vector3::new(0.0, 0.0, 0.0))
                .get(&sound)
                .copied()
        }
    }

    #[test]
    fn each_anchored_wall_between_counts_up_to_the_most() {
        let mut scene = Scene::new();
        let sound = scene.sound(40.0);
        let mut occlusion = Occlusion::default();
        assert_eq!(scene.walls(&mut occlusion, sound), Some(0), "in the open");

        scene.wall(10.0);
        assert_eq!(scene.walls(&mut occlusion, sound), Some(1));
        scene.wall(20.0);
        assert_eq!(scene.walls(&mut occlusion, sound), Some(2));
        // A wall behind the listener or behind the sound doesn't count.
        scene.wall(-10.0);
        scene.wall(60.0);
        assert_eq!(scene.walls(&mut occlusion, sound), Some(2));
        for x in [25.0, 30.0, 35.0] {
            scene.wall(x);
        }
        assert_eq!(scene.walls(&mut occlusion, sound), Some(MAX_WALLS));
    }

    #[test]
    fn loose_parts_and_walk_through_parts_do_not_muffle() {
        let mut scene = Scene::new();
        let sound = scene.sound(40.0);
        // A character-sized loose part, and a see-through zone.
        scene.part(
            Vector3::new(10.0, 0.0, 0.0),
            Vector3::new(2.0, 5.0, 2.0),
            false,
        );
        let zone = scene.wall(20.0);
        scene
            .world
            .set_property(zone, "CanCollide", Value::Bool(false))
            .unwrap();
        let mut occlusion = Occlusion::default();
        assert_eq!(scene.walls(&mut occlusion, sound), Some(0));
    }

    #[test]
    fn only_spatial_playing_sounds_in_earshot_are_checked() {
        let mut scene = Scene::new();
        scene.wall(10.0);
        let sound = scene.sound(40.0);
        let mut occlusion = Occlusion::default();
        assert_eq!(scene.walls(&mut occlusion, sound), Some(1));

        let world = &mut scene.world;
        world
            .set_property(sound, "RollOffMaxDistance", Value::Number(30.0))
            .unwrap();
        assert_eq!(scene.walls(&mut occlusion, sound), None, "out of earshot");
        let world = &mut scene.world;
        world
            .set_property(sound, "RollOffMaxDistance", Value::Number(150.0))
            .unwrap();
        world
            .set_property(sound, "Playing", Value::Bool(false))
            .unwrap();
        assert_eq!(scene.walls(&mut occlusion, sound), None, "stopped");
        let world = &mut scene.world;
        world
            .set_property(sound, "Playing", Value::Bool(true))
            .unwrap();
        let service = world.sound_service();
        world
            .set_property(service, "Occlusion", Value::Bool(false))
            .unwrap();
        assert_eq!(scene.walls(&mut occlusion, sound), None, "Occlusion off");
    }

    #[test]
    fn many_sounds_share_a_fixed_ray_budget_and_each_gets_its_turn() {
        let mut scene = Scene::new();
        scene.wall(10.0);
        let sounds: Vec<InstanceId> = (0..50)
            .map(|i| scene.sound(20.0 + i as f32 * 0.5))
            .collect();
        let mut occlusion = Occlusion::default();
        let frames = sounds.len().div_ceil(SOUNDS_PER_FRAME);
        for _ in 0..frames {
            occlusion.step(&scene.world, &scene.assets, Vector3::new(0.0, 0.0, 0.0));
            assert!(
                occlusion.rays() <= SOUNDS_PER_FRAME * RAYS_PER_SOUND,
                "{} rays",
                occlusion.rays()
            );
        }
        let walls = occlusion.step(&scene.world, &scene.assets, Vector3::new(0.0, 0.0, 0.0));
        for sound in &sounds {
            // Each Sound's own speaker is passed by; the others' speakers
            // in front of it are anchored, so count as walls too.
            assert!(
                walls.get(sound).is_some_and(|&walls| walls >= 1),
                "{sound:?}"
            );
        }
    }
}
