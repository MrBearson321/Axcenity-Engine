use std::sync::Arc;

use axcenity_core::{
    ClassName, FoliageInstance, FoliageProp, FoliageSource, InstanceId, PartType, SampleRect,
    TerrainData,
};

use super::*;

fn part(world: &mut World, position: Vector3, size: Vector3) -> InstanceId {
    let id = world.create(ClassName::Part).unwrap();
    world.set_parent(id, Some(world.workspace())).unwrap();
    set(world, id, "Position", Value::Vector3(position));
    set(world, id, "Size", Value::Vector3(size));
    set(world, id, "Anchored", Value::Bool(true));
    id
}

fn set(world: &mut World, id: InstanceId, name: &str, value: Value) {
    world.set_property(id, name, value).unwrap();
}

fn at(x: f32, y: f32, z: f32) -> Vector3 {
    Vector3::new(x, y, z)
}

/// The height of the ground a character at `point` stands on.
fn ground(mesh: &NavMesh, point: Vector3) -> Option<f32> {
    mesh.cell_at(point).map(|index| mesh.cells()[index].height)
}

fn build(world: &World) -> NavMesh {
    NavMesh::build(world, &Assets::default(), NavSettings::default())
}

/// A 64×64-stud flat Terrain with its ground at Y = 0: 33 samples each way,
/// 2 studs apart, heights in 32-stud units.
fn flat_terrain(world: &mut World, foliage: Option<FoliageInstance>) -> InstanceId {
    let terrain = world.create(ClassName::Terrain).unwrap();
    world.set_parent(terrain, Some(world.workspace())).unwrap();
    set(
        world,
        terrain,
        "Position",
        Value::Vector3(at(0.0, 0.0, 0.0)),
    );
    set(world, terrain, "Size", Value::Vector3(at(64.0, 32.0, 64.0)));
    let mut data = TerrainData::flat(33, 33, 0.0).unwrap();
    if let Some(instance) = foliage {
        data.foliage_mut().push(instance);
    }
    world.set_terrain(terrain, Arc::new(data));
    terrain
}

/// Raises samples 20 to 28 each way (X and Z from 8 to 24) to 16 studs:
/// a plateau with cliffs two studs wide all round.
fn raise_plateau(world: &mut World, terrain: InstanceId) {
    let data = world.terrain_mut(terrain).unwrap();
    let heights = data.heights_mut_in(SampleRect::new(20, 20, 29, 29));
    for z in 20..29 {
        for x in 20..29 {
            heights[z * 33 + x] = 0.5;
        }
    }
}

fn crate_at_centre(collide: bool) -> FoliageInstance {
    FoliageInstance {
        source: FoliageSource::Prop(FoliageProp::Crate),
        x: 16.0,
        z: 16.0,
        yaw: 0.0,
        scale: 1.0,
        collide,
    }
}

#[test]
fn a_floor_of_parts_is_walkable_and_a_gap_is_not() {
    let mut world = World::new();
    // Two 20-stud floors with their tops at Y = 0 and a 4-stud gap between.
    part(&mut world, at(-12.0, -0.5, 0.0), at(20.0, 1.0, 20.0));
    part(&mut world, at(12.0, -0.5, 0.0), at(20.0, 1.0, 20.0));
    let mesh = build(&world);

    let left = at(-12.0, 0.0, 0.0);
    let right = at(12.0, 0.0, 0.0);
    assert!(mesh.is_walkable(left));
    assert!(mesh.is_walkable(right));
    assert!(ground(&mesh, left).unwrap().abs() < 1e-3);
    assert!(!mesh.is_walkable(at(0.0, 0.0, 0.0)), "the gap");
    assert!(!mesh.connected(left, right), "nothing crosses the gap");
    assert_eq!(mesh.region_count(), 2);

    // A bridge over the gap joins them.
    part(&mut world, at(0.0, -0.5, 0.0), at(6.0, 1.0, 4.0));
    let mesh = build(&world);
    assert!(mesh.is_walkable(at(0.0, 0.0, 0.0)));
    assert!(mesh.connected(left, right));
}

#[test]
fn a_terrain_surface_is_walkable_and_a_steep_drop_is_not() {
    let mut world = World::new();
    let terrain = flat_terrain(&mut world, None);
    raise_plateau(&mut world, terrain);
    let mesh = build(&world);

    let low = at(-16.0, 0.0, -16.0);
    let high = at(16.0, 16.0, 16.0);
    assert!(mesh.is_walkable(low));
    assert!(mesh.is_walkable(high));
    assert!((ground(&mesh, high).unwrap() - 16.0).abs() < 1e-3);
    // The cliff rises 16 studs in 2: far steeper than 45°.
    assert!(!mesh.is_walkable(at(7.0, 8.0, 16.0)));
    assert!(!mesh.is_walkable(at(7.0, 0.0, 16.0)));
    assert!(
        !mesh.connected(low, high),
        "the drop splits the low ground from the plateau"
    );
}

#[test]
fn a_gentle_ramp_is_walkable_and_a_steep_one_is_not() {
    // A 4-stud-high platform from Z = 10 to 20 on a floor, with a wedge
    // rising to it from -Z. 8 studs long is 27°; 2 studs long is 63°.
    fn world_with_ramp(length: f32) -> World {
        let mut world = World::new();
        part(&mut world, at(0.0, -0.5, 0.0), at(40.0, 1.0, 40.0));
        part(&mut world, at(0.0, 2.0, 15.0), at(8.0, 4.0, 10.0));
        let wedge = part(
            &mut world,
            at(0.0, 2.0, 10.0 - length / 2.0),
            at(8.0, 4.0, length),
        );
        set(&mut world, wedge, "Shape", Value::PartType(PartType::Wedge));
        world
    }
    let floor = at(0.0, 0.0, -15.0);
    let platform = at(0.0, 4.0, 15.0);

    let mesh = build(&world_with_ramp(8.0));
    assert!(mesh.is_walkable(at(0.0, 2.0, 6.0)), "halfway up the ramp");
    assert!(mesh.connected(floor, platform));

    let mesh = build(&world_with_ramp(2.0));
    assert!(!mesh.is_walkable(at(0.0, 2.0, 9.0)), "the steep ramp");
    assert!(mesh.is_walkable(platform));
    assert!(!mesh.connected(floor, platform));
}

#[test]
fn ground_under_a_low_ceiling_is_not_walkable() {
    let mut world = World::new();
    part(&mut world, at(0.0, -0.5, 0.0), at(40.0, 1.0, 40.0));
    // A slab 3 studs up leaves too little room for a 5-stud character.
    part(&mut world, at(-10.0, 3.5, 0.0), at(6.0, 1.0, 6.0));
    // One 8 studs up leaves enough.
    part(&mut world, at(10.0, 8.5, 0.0), at(6.0, 1.0, 6.0));
    let mesh = build(&world);
    assert!(!mesh.is_walkable(at(-10.0, 0.0, 0.0)));
    assert!(mesh.is_walkable(at(10.0, 0.0, 0.0)));
    assert_eq!(
        ground(&mesh, at(-10.0, 4.0, 0.0)),
        Some(4.0),
        "the slab's top"
    );
}

#[test]
fn a_non_colliding_part_adds_no_walkable_surface() {
    let mut world = World::new();
    let floor = part(&mut world, at(0.0, -0.5, 0.0), at(20.0, 1.0, 20.0));
    // A low ghost slab over part of the floor.
    let ghost = part(&mut world, at(0.0, 2.5, 0.0), at(6.0, 1.0, 6.0));
    set(&mut world, ghost, "CanCollide", Value::Bool(false));
    let mesh = build(&world);
    assert!(
        mesh.is_walkable(at(0.0, 0.0, 0.0)),
        "it doesn't block the floor"
    );
    assert_eq!(
        ground(&mesh, at(0.0, 3.0, 0.0)),
        Some(0.0),
        "and has no top"
    );

    set(&mut world, floor, "CanCollide", Value::Bool(false));
    assert!(build(&world).cells().is_empty());
}

#[test]
fn visual_foliage_adds_nothing_and_colliding_foliage_does() {
    let mut bare = World::new();
    flat_terrain(&mut bare, None);
    let mut visual = World::new();
    flat_terrain(&mut visual, Some(crate_at_centre(false)));
    let mut solid = World::new();
    flat_terrain(&mut solid, Some(crate_at_centre(true)));

    let bare = build(&bare);
    let visual = build(&visual);
    assert_eq!(
        visual.cells(),
        bare.cells(),
        "visual foliage changes nothing"
    );
    assert!(visual.is_walkable(at(0.5, 0.0, 0.5)));

    // The 3-stud crate covers the ground at the centre; its top is a
    // separate island, too tall to step onto.
    let solid = build(&solid);
    assert!(!solid.is_walkable(at(0.5, 0.0, 0.5)));
    assert!((ground(&solid, at(0.5, 3.0, 0.5)).unwrap() - 3.0).abs() < 1e-3);
    assert!(!solid.connected(at(0.5, 3.0, 0.5), at(-16.0, 0.0, -16.0)));
}

#[test]
fn moving_a_floor_part_and_rebuilding_moves_the_mesh() {
    let mut world = World::new();
    part(&mut world, at(0.0, -0.5, 0.0), at(40.0, 1.0, 40.0));
    let platform = part(&mut world, at(0.0, 2.0, 0.0), at(8.0, 4.0, 8.0));
    let before = fingerprint(&world);
    let mesh = build(&world);
    assert_eq!(ground(&mesh, at(0.0, 4.0, 0.0)), Some(4.0));
    assert_eq!(ground(&mesh, at(12.0, 4.0, 0.0)), Some(0.0));

    set(
        &mut world,
        platform,
        "Position",
        Value::Vector3(at(12.0, 2.0, 0.0)),
    );
    assert_ne!(fingerprint(&world), before, "the move is noticed");
    let mesh = build(&world);
    assert_eq!(
        ground(&mesh, at(0.0, 4.0, 0.0)),
        Some(0.0),
        "the floor is clear again"
    );
    assert_eq!(ground(&mesh, at(12.0, 4.0, 0.0)), Some(4.0));
}

#[test]
fn sculpting_terrain_and_rebuilding_changes_the_mesh() {
    let mut world = World::new();
    let terrain = flat_terrain(&mut world, None);
    let before = fingerprint(&world);
    let mesh = build(&world);
    assert!(mesh.is_walkable(at(16.0, 0.0, 16.0)));
    assert!(mesh.connected(at(16.0, 0.0, 16.0), at(-16.0, 0.0, -16.0)));

    raise_plateau(&mut world, terrain);
    assert_ne!(fingerprint(&world), before, "the edit is noticed");
    let mesh = build(&world);
    assert!(!mesh.is_walkable(at(16.0, 0.0, 16.0)));
    assert!(mesh.is_walkable(at(16.0, 16.0, 16.0)));
    assert!(!mesh.connected(at(16.0, 16.0, 16.0), at(-16.0, 0.0, -16.0)));
}

#[test]
fn a_character_is_not_ground() {
    let mut world = World::new();
    part(&mut world, at(0.0, -0.5, 0.0), at(20.0, 1.0, 20.0));
    let model = world.create(ClassName::Model).unwrap();
    world.set_parent(model, Some(world.workspace())).unwrap();
    let body = part(&mut world, at(0.0, 2.5, 0.0), at(2.0, 5.0, 2.0));
    world.set_parent(body, Some(model)).unwrap();
    let controller = world.create(ClassName::ThirdPersonController).unwrap();
    world.set_parent(controller, Some(model)).unwrap();

    let before = fingerprint(&world);
    let mesh = build(&world);
    assert!(mesh.is_walkable(at(0.0, 0.0, 0.0)));
    assert_eq!(
        ground(&mesh, at(0.0, 5.0, 0.0)),
        Some(0.0),
        "no island on its head"
    );
    set(
        &mut world,
        body,
        "Position",
        Value::Vector3(at(4.0, 2.5, 0.0)),
    );
    assert_eq!(
        fingerprint(&world),
        before,
        "walking doesn't need a rebuild"
    );
}

#[test]
fn every_link_goes_both_ways_and_regions_agree() {
    let mut world = World::new();
    let terrain = flat_terrain(&mut world, Some(crate_at_centre(true)));
    raise_plateau(&mut world, terrain);
    part(&mut world, at(-20.0, 1.0, -20.0), at(6.0, 2.0, 6.0));
    let mesh = build(&world);
    assert!(!mesh.cells().is_empty());
    for index in 0..mesh.cells().len() {
        for other in mesh.links(index) {
            assert!(mesh.links(other).any(|back| back == index));
            assert_eq!(mesh.cells()[index].region, mesh.cells()[other].region);
        }
    }
    assert!(mesh.outline().len() > 4);
}

#[test]
fn an_empty_world_has_no_mesh() {
    let mesh = build(&World::new());
    assert!(mesh.cells().is_empty());
    assert!(!mesh.is_walkable(at(0.0, 0.0, 0.0)));
    assert_eq!(mesh.region_count(), 0);
}
