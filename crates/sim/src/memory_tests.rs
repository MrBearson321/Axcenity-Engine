//! `Sim::memory` (roadmap G7): the profilers' breakdown of a running game.

use axcenity_assets::Assets;
use axcenity_core::{ClassName, Value, Vector3, World};

use crate::Sim;

#[test]
fn the_breakdown_counts_the_parts_their_bodies_and_the_lua_heaps() {
    let mut world = World::new();
    let workspace = world.workspace();
    for x in 0..20 {
        let part = world.create(ClassName::Part).unwrap();
        let at = Vector3::new(x as f32 * 3.0, 4.0, 0.0);
        world
            .set_property(part, "Position", Value::Vector3(at))
            .unwrap();
        world.set_parent(part, Some(workspace)).unwrap();
    }
    let mut sim = Sim::new(world, crate::script_runtime(), Assets::default());
    sim.physics.step(&mut sim.world, 1.0 / 60.0);
    let memory = sim.memory();
    assert_eq!(memory.classes_of(ClassName::Part), 20);
    assert_eq!(memory.physics.count, 20);
    assert!(memory.lua > 0, "the runtimes' heaps");
    assert_eq!(memory.models.count, 0);
    assert_eq!(memory.sounds.count, 0, "the host's to fill in");
    assert!(memory.total() >= memory.lua + memory.instances.total.bytes);
}
