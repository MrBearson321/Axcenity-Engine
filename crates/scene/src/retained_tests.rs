//! A frame drawn with kept items must be the same as one built from
//! scratch. The first test checks that on random worlds under random edits;
//! the rest are the ways a kept item could go stale, each on its own.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use axcenity_assets::{Assets, MaterialFile, Reload};
use axcenity_core::{
    ClassName, Color3, InstanceId, Material as PartMaterial, PartType, Value, Vector3, World,
};
use axcenity_render::glam::Vec3;
use axcenity_render::{Camera, DecalRole, DrawItem, DrawList, Shape};

use crate::{Poses, TerrainCache, build_draw_list_posed};

const PYRAMID: &str = "assets/models/pyramid.gltf";
const LIMIT: Duration = Duration::from_secs(30);

fn camera() -> Camera {
    Camera {
        position: Vec3::new(0.0, 20.0, 60.0),
        look_at: Vec3::ZERO,
        fov_y_degrees: 60.0,
    }
}

fn examples() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples")
}

fn vector(x: f32, y: f32, z: f32) -> Value {
    Value::Vector3(Vector3::new(x, y, z))
}

fn color(r: f32, g: f32, b: f32) -> Value {
    Value::Color3(Color3::new(r, g, b))
}

fn child(world: &mut World, parent: InstanceId, class: ClassName) -> InstanceId {
    let id = world.create(class).unwrap();
    world.set_parent(id, Some(parent)).unwrap();
    id
}

fn part(world: &mut World, parent: InstanceId) -> InstanceId {
    child(world, parent, ClassName::Part)
}

fn mesh_part(world: &mut World, parent: InstanceId, mesh_id: &str) -> InstanceId {
    let part = child(world, parent, ClassName::MeshPart);
    world
        .set_property(part, "MeshId", Value::String(mesh_id.to_owned()))
        .unwrap();
    world
        .set_property(part, "Size", vector(4.0, 4.0, 4.0))
        .unwrap();
    part
}

fn set(world: &mut World, id: InstanceId, name: &str, value: Value) {
    world.set_property(id, name, value).unwrap();
}

/// A cache kept between frames, as a host keeps one, with items retained
/// whatever the environment says.
fn retained() -> TerrainCache {
    let mut cache = TerrainCache::new();
    cache.set_retained_draw_list(true);
    cache
}

fn from_cache(world: &World, assets: &Assets, cache: &mut TerrainCache) -> DrawList {
    build_draw_list_posed(world, assets, camera(), &Poses::new(), cache)
}

fn from_scratch(world: &World, assets: &Assets) -> DrawList {
    build_draw_list_posed(
        world,
        assets,
        camera(),
        &Poses::new(),
        &mut TerrainCache::for_one_frame(),
    )
}

/// The two lists are the same, in the same order. Meshes and textures print
/// their keys, which don't depend on which copy of a mesh an item holds.
fn assert_same(kept: &DrawList, fresh: &DrawList, context: &str) {
    assert_eq!(
        kept.items.len(),
        fresh.items.len(),
        "{context}: item counts differ"
    );
    for (index, (a, b)) in kept.items.iter().zip(&fresh.items).enumerate() {
        assert_eq!(
            format!("{a:?}"),
            format!("{b:?}"),
            "{context}: item {index}"
        );
    }
    assert_eq!(kept.lights, fresh.lights, "{context}: lights");
    assert_eq!(
        format!("{:?}", kept.decals),
        format!("{:?}", fresh.decals),
        "{context}: decals"
    );
    assert_eq!(
        format!("{:?}", kept.reflection_probes),
        format!("{:?}", fresh.reflection_probes),
        "{context}: reflection probes"
    );
    assert_eq!(
        format!("{:?}", kept.light_probe_volumes),
        format!("{:?}", fresh.light_probe_volumes),
        "{context}: light probe volumes"
    );
}

/// Draws a frame with `cache`, checks it against one from scratch, and
/// returns its draw items.
fn draw_checked(
    world: &World,
    assets: &Assets,
    cache: &mut TerrainCache,
    context: &str,
) -> Vec<DrawItem> {
    let kept = from_cache(world, assets, cache);
    assert_same(&kept, &from_scratch(world, assets), context);
    kept.items
}

/// A small repeatable random number generator (xorshift), so a failure can
/// be run again from its seed.
struct Random(u64);

impl Random {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn below(&mut self, limit: usize) -> usize {
        (self.next() % limit as u64) as usize
    }

    fn unit(&mut self) -> f32 {
        (self.next() % 1000) as f32 / 1000.0
    }

    fn pick(&mut self, ids: &[InstanceId]) -> Option<InstanceId> {
        (!ids.is_empty()).then(|| ids[self.below(ids.len())])
    }
}

/// What a random world is made of, by what can be done to each.
#[derive(Default)]
struct Scene {
    containers: Vec<InstanceId>,
    parts: Vec<InstanceId>,
    looks: Vec<InstanceId>,
    lights: Vec<InstanceId>,
    decals: Vec<InstanceId>,
    inventories: Vec<(InstanceId, InstanceId)>,
    anything: Vec<InstanceId>,
}

impl Scene {
    fn forget_destroyed(&mut self, world: &World) {
        let alive = |id: &InstanceId| world.contains(*id);
        self.containers.retain(alive);
        self.parts.retain(alive);
        self.looks.retain(alive);
        self.lights.retain(alive);
        self.decals.retain(alive);
        self.anything.retain(alive);
        self.inventories
            .retain(|(inventory, tool)| world.contains(*inventory) && world.contains(*tool));
    }
}

fn add_something(world: &mut World, scene: &mut Scene, random: &mut Random) {
    let parent = random.pick(&scene.containers).unwrap_or(world.workspace());
    let part_or_parent = random.pick(&scene.parts).unwrap_or(parent);
    let new = match random.below(16) {
        0..=2 => {
            let id = part(world, parent);
            let shape = [
                PartType::Block,
                PartType::Ball,
                PartType::Cylinder,
                PartType::Wedge,
                PartType::Capsule,
            ][random.below(5)];
            set(world, id, "Shape", Value::PartType(shape));
            set(
                world,
                id,
                "Position",
                vector(random.unit() * 40.0, 2.0, 0.0),
            );
            scene.parts.push(id);
            id
        }
        3 => {
            let id = mesh_part(world, parent, PYRAMID);
            scene.parts.push(id);
            id
        }
        4 => {
            let id = child(world, parent, ClassName::MeshPart);
            scene.parts.push(id);
            id
        }
        5 | 6 => {
            let class = [ClassName::Folder, ClassName::Model][random.below(2)];
            let id = child(world, parent, class);
            scene.containers.push(id);
            id
        }
        7 => {
            let id = child(world, part_or_parent, ClassName::SurfaceAppearance);
            set(world, id, "Color", color(random.unit(), 1.0, 1.0));
            scene.looks.push(id);
            id
        }
        8 => {
            let class = [
                ClassName::PointLight,
                ClassName::SpotLight,
                ClassName::RectLight,
            ][random.below(3)];
            let id = child(world, part_or_parent, class);
            scene.lights.push(id);
            id
        }
        9 => {
            let id = child(world, part_or_parent, ClassName::Decal);
            scene.decals.push(id);
            id
        }
        10 => {
            let id = child(world, parent, ClassName::ProjectedDecal);
            set(
                world,
                id,
                "Position",
                vector(random.unit() * 20.0, 0.0, 0.0),
            );
            set(world, id, "Size", vector(4.0, 4.0, 4.0));
            id
        }
        11 => {
            let model = child(world, parent, ClassName::Model);
            child(world, model, ClassName::Humanoid);
            part(world, model);
            scene.containers.push(model);
            model
        }
        12 => {
            let union = child(world, parent, ClassName::UnionOperation);
            part(world, union);
            scene.parts.push(union);
            union
        }
        13 => {
            let inventory = child(world, parent, ClassName::Inventory);
            let tool = child(world, inventory, ClassName::Tool);
            let handle = part(world, tool);
            set(
                world,
                handle,
                "Position",
                vector(0.0, 8.0, random.unit() * 10.0),
            );
            child(world, handle, ClassName::PointLight);
            scene.containers.push(tool);
            scene.parts.push(handle);
            scene.inventories.push((inventory, tool));
            inventory
        }
        14 => {
            let spring = child(world, parent, ClassName::SpringConstraint);
            set(world, spring, "Visible", Value::Bool(true));
            for name in ["Attachment0", "Attachment1"] {
                let on = random.pick(&scene.parts).unwrap_or(part_or_parent);
                let attachment = child(world, on, ClassName::Attachment);
                set(world, spring, name, Value::Instance(Some(attachment)));
            }
            spring
        }
        _ => child(world, parent, ClassName::Script),
    };
    scene.anything.push(new);
}

/// One random change to the world.
fn edit(world: &mut World, scene: &mut Scene, random: &mut Random) {
    match random.below(20) {
        0..=2 => add_something(world, scene, random),
        3 | 4 => {
            // Move a part.
            if let Some(id) = random.pick(&scene.parts) {
                let position = vector(random.unit() * 50.0, random.unit() * 9.0, 0.0);
                let _ = world.set_property(id, "Position", position);
            }
        }
        5 => {
            if let Some(id) = random.pick(&scene.parts) {
                let _ = world.set_property(id, "Color", color(random.unit(), 0.5, 0.5));
            }
        }
        6 => {
            if let Some(id) = random.pick(&scene.parts) {
                let transparency = [0.0, 0.5, 1.0][random.below(3)];
                let _ = world.set_property(id, "Transparency", Value::Number(transparency));
            }
        }
        7 => {
            if let Some(id) = random.pick(&scene.parts) {
                let material = [
                    PartMaterial::Neon,
                    PartMaterial::Metal,
                    PartMaterial::Plastic,
                ][random.below(3)];
                let _ = world.set_property(id, "Material", Value::Material(material));
            }
        }
        8 => {
            // A property of the SurfaceAppearance, not of the part it's in.
            if let Some(id) = random.pick(&scene.looks) {
                let _ = world.set_property(id, "Color", color(random.unit(), 1.0, 1.0));
            }
        }
        9 => {
            if let Some(id) = random.pick(&scene.lights) {
                let on = random.below(2) == 0;
                let _ = world.set_property(id, "Enabled", Value::Bool(on));
                let _ = world.set_property(id, "Color", color(random.unit(), 1.0, 1.0));
            }
        }
        10 => {
            if let Some(id) = random.pick(&scene.decals) {
                let transparency = Value::Number(f64::from(random.unit()));
                let _ = world.set_property(id, "Transparency", transparency);
            }
        }
        11 => {
            // An ancestor's own change, which nothing inside it reads.
            if let Some(id) = random.pick(&scene.containers) {
                let _ = world.set_name(id, format!("Container{}", random.below(100)));
                let _ = world.set_attribute(id, "Marked", Some(Value::Bool(true)));
            }
        }
        12 => {
            // In and out of hand: the Inventory's property, above the parts.
            if !scene.inventories.is_empty() {
                let (inventory, tool) = scene.inventories[random.below(scene.inventories.len())];
                let equipped = (random.below(2) == 0).then_some(tool);
                let _ = world.set_property(inventory, "Equipped", Value::Instance(equipped));
            }
        }
        13 | 14 => {
            let moved = random.pick(&scene.anything);
            let parent = random.pick(&scene.containers).unwrap_or(world.workspace());
            if let Some(moved) = moved {
                let _ = world.set_parent(moved, Some(parent));
            }
        }
        15 => {
            // Among its parent's children.
            if let Some(moved) = random.pick(&scene.anything)
                && let Some(parent) = world.get(moved).and_then(|i| i.parent())
            {
                let _ = world.set_parent_at(moved, parent, 0);
            }
        }
        16 => {
            if let Some(gone) = random.pick(&scene.anything) {
                let _ = world.destroy(gone);
                scene.forget_destroyed(world);
            }
        }
        17 => {
            let lighting = world.lighting();
            let on = random.below(2) == 0;
            let _ = world.set_property(lighting, "BloomEnabled", Value::Bool(on));
            let _ = world.set_property(lighting, "PostProcessingEnabled", Value::Bool(on));
        }
        18 => {
            let lighting = world.lighting();
            let clock = Value::Number(f64::from(random.unit() * 24.0));
            let _ = world.set_property(lighting, "ClockTime", clock);
        }
        _ => {
            if let Some(id) = random.pick(&scene.parts) {
                let turn = vector(0.0, random.unit() * 360.0, 0.0);
                let _ = world.set_property(id, "Orientation", turn);
            }
        }
    }
}

#[test]
fn retained_frames_match_frames_from_scratch_through_random_edits() {
    let assets = Assets::new(examples().join("hello-mesh"));
    for seed in 1..=12_u64 {
        let mut random = Random(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let mut world = World::new();
        let mut scene = Scene::default();
        scene.containers.push(world.workspace());
        for _ in 0..60 {
            add_something(&mut world, &mut scene, &mut random);
        }
        let mut cache = retained();
        for frame in 0..200 {
            for _ in 0..random.below(5) {
                edit(&mut world, &mut scene, &mut random);
            }
            draw_checked(
                &world,
                &assets,
                &mut cache,
                &format!("seed {seed} frame {frame}"),
            );
        }
    }
}

#[test]
fn a_frame_with_nothing_changed_makes_nothing_again() {
    let mut world = World::new();
    let workspace = world.workspace();
    let parts: Vec<_> = (0..50).map(|_| part(&mut world, workspace)).collect();
    let assets = Assets::default();
    let mut cache = retained();
    let rebuilt = |world: &World, cache: &mut TerrainCache| {
        from_cache(world, &assets, cache);
        cache.draw_cache.rebuilt_last_frame()
    };
    assert_eq!(
        rebuilt(&world, &mut cache),
        50,
        "the first frame makes them all"
    );
    assert_eq!(rebuilt(&world, &mut cache), 0, "nothing changed");

    set(&mut world, parts[7], "Position", vector(9.0, 9.0, 9.0));
    assert_eq!(rebuilt(&world, &mut cache), 1, "only the part that moved");
    assert_eq!(rebuilt(&world, &mut cache), 0);

    // Something new, which goes over the tree but keeps what's there.
    part(&mut world, workspace);
    assert_eq!(rebuilt(&world, &mut cache), 1, "only the new part");
}

#[test]
fn a_surface_appearance_child_changing_updates_its_part_and_mesh_part() {
    let assets = Assets::new(examples().join("hello-mesh"));
    let mut world = World::new();
    let workspace = world.workspace();
    let part = part(&mut world, workspace);
    let mesh = mesh_part(&mut world, workspace, PYRAMID);
    let mut cache = retained();
    draw_checked(&world, &assets, &mut cache, "no looks");

    // New SurfaceAppearances: the children changed, not the parts' revisions.
    let on_part = child(&mut world, part, ClassName::SurfaceAppearance);
    let on_mesh = child(&mut world, mesh, ClassName::SurfaceAppearance);
    draw_checked(&world, &assets, &mut cache, "added");
    let tinted = draw_checked(&world, &assets, &mut cache, "still");

    // Changing a property of each child.
    set(&mut world, on_part, "Color", color(0.0, 1.0, 0.0));
    set(
        &mut world,
        on_mesh,
        "MaterialName",
        Value::String("anything".into()),
    );
    let changed = draw_checked(&world, &assets, &mut cache, "changed");
    assert_ne!(
        format!("{:?}", tinted[0].material.base_color),
        format!("{:?}", changed[0].material.base_color),
        "the part follows its SurfaceAppearance",
    );

    world.destroy(on_part).unwrap();
    world.destroy(on_mesh).unwrap();
    draw_checked(&world, &assets, &mut cache, "removed");
}

#[test]
fn a_surface_appearance_moved_to_first_place_takes_over() {
    let mut world = World::new();
    let workspace = world.workspace();
    let part = part(&mut world, workspace);
    let green = child(&mut world, part, ClassName::SurfaceAppearance);
    set(&mut world, green, "Color", color(0.0, 1.0, 0.0));
    let red = child(&mut world, part, ClassName::SurfaceAppearance);
    set(&mut world, red, "Color", color(1.0, 0.0, 0.0));
    let assets = Assets::default();
    let mut cache = retained();
    let before = draw_checked(&world, &assets, &mut cache, "green first");
    world.set_parent_at(red, part, 0).unwrap();
    let after = draw_checked(&world, &assets, &mut cache, "red first");
    assert_ne!(before[0].material.base_color, after[0].material.base_color);
}

#[test]
fn a_light_child_changing_updates_its_part_s_glow() {
    let mut world = World::new();
    let workspace = world.workspace();
    let part = part(&mut world, workspace);
    let assets = Assets::default();
    let mut cache = retained();
    let dark = draw_checked(&world, &assets, &mut cache, "no light");
    let light = child(&mut world, part, ClassName::PointLight);
    let lit = draw_checked(&world, &assets, &mut cache, "a light added");
    assert_ne!(dark[0].material.emissive, lit[0].material.emissive);
    set(&mut world, light, "Color", color(1.0, 0.0, 0.0));
    let red = draw_checked(&world, &assets, &mut cache, "its color");
    assert_ne!(lit[0].material.emissive, red[0].material.emissive);
    set(&mut world, light, "Enabled", Value::Bool(false));
    let off = draw_checked(&world, &assets, &mut cache, "switched off");
    assert_eq!(dark[0].material.emissive, off[0].material.emissive);
}

#[test]
fn a_decal_follows_its_part_and_moves_with_it() {
    let mut world = World::new();
    let workspace = world.workspace();
    let first = part(&mut world, workspace);
    let second = part(&mut world, workspace);
    set(&mut world, second, "Position", vector(20.0, 0.0, 0.0));
    let decal = child(&mut world, first, ClassName::Decal);
    let assets = Assets::default();
    let mut cache = retained();
    let of = |items: &[DrawItem]| {
        items
            .iter()
            .find(|item| item.decal_role == DecalRole::Decal)
            .map(|item| item.transform)
    };
    let at_start = of(&draw_checked(&world, &assets, &mut cache, "on its part")).unwrap();

    set(&mut world, first, "Position", vector(0.0, 5.0, 0.0));
    let followed = of(&draw_checked(&world, &assets, &mut cache, "part moved")).unwrap();
    assert_ne!(
        at_start, followed,
        "the Decal's revision didn't change, its part's did"
    );

    world.set_parent(decal, Some(second)).unwrap();
    let moved = of(&draw_checked(&world, &assets, &mut cache, "reparented")).unwrap();
    assert_ne!(followed, moved);
}

#[test]
fn what_streaming_hides_changes_with_the_camera() {
    let mut world = World::new();
    let workspace = world.workspace();
    for (name, value) in [
        ("StreamingEnabled", Value::Bool(true)),
        ("StreamingMinRadius", Value::Number(64.0)),
        ("StreamingTargetRadius", Value::Number(200.0)),
    ] {
        set(&mut world, workspace, name, value);
    }
    for x in [0.0, 150.0, 900.0] {
        let part = part(&mut world, workspace);
        set(&mut world, part, "Position", vector(x, 0.0, 0.0));
    }
    let assets = Assets::default();
    let mut cache = retained();
    let drawn =
        |world: &World, cache: &mut TerrainCache| from_cache(world, &assets, cache).items.len();
    cache.streaming.step(&world, Vec3::ZERO, 1.0);
    assert_eq!(drawn(&world, &mut cache), 2);
    cache
        .streaming
        .step(&world, Vec3::new(900.0, 0.0, 0.0), 1.0);
    assert_eq!(
        drawn(&world, &mut cache),
        1,
        "without any Instance changing"
    );
    cache.streaming.step(&world, Vec3::ZERO, 1.0);
    assert_eq!(drawn(&world, &mut cache), 2, "and back");
}

#[test]
fn the_time_of_day_moves_the_sun_and_bloom_changes_how_neon_glows() {
    let mut world = World::new();
    let workspace = world.workspace();
    let neon = part(&mut world, workspace);
    set(
        &mut world,
        neon,
        "Material",
        Value::Material(PartMaterial::Neon),
    );
    let assets = Assets::default();
    let mut cache = retained();
    let lighting = world.lighting();

    let noon = from_cache(&world, &assets, &mut cache);
    set(&mut world, lighting, "ClockTime", Value::Number(21.0));
    let night = from_cache(&world, &assets, &mut cache);
    assert_ne!(
        format!("{:?}", noon.sun),
        format!("{:?}", night.sun),
        "the sun is worked out each frame",
    );
    assert_same(&night, &from_scratch(&world, &assets), "after dark");

    // Bloom decides how brightly Neon is drawn, from outside the part.
    set(
        &mut world,
        lighting,
        "PostProcessingEnabled",
        Value::Bool(true),
    );
    set(&mut world, lighting, "BloomEnabled", Value::Bool(true));
    let bright = draw_checked(&world, &assets, &mut cache, "bloom on");
    set(&mut world, lighting, "BloomEnabled", Value::Bool(false));
    let plain = draw_checked(&world, &assets, &mut cache, "bloom off");
    assert_ne!(bright[0].material.emissive, plain[0].material.emissive);
}

#[test]
fn a_character_s_parts_are_marked_when_it_becomes_one_and_unmarked_after() {
    let mut world = World::new();
    let workspace = world.workspace();
    let model = child(&mut world, workspace, ClassName::Model);
    let body = part(&mut world, model);
    let assets = Assets::default();
    let mut cache = retained();
    let role = |items: Vec<DrawItem>| items[0].decal_role;
    assert_eq!(
        role(draw_checked(&world, &assets, &mut cache, "a Model")),
        DecalRole::default()
    );
    let humanoid = child(&mut world, model, ClassName::Humanoid);
    assert_eq!(
        role(draw_checked(&world, &assets, &mut cache, "a character")),
        DecalRole::Character,
        "the part's own revision didn't change",
    );
    world.destroy(humanoid).unwrap();
    assert_eq!(
        role(draw_checked(&world, &assets, &mut cache, "no more")),
        DecalRole::default()
    );
    let _ = body;
}

#[test]
fn putting_away_a_tool_takes_its_parts_out_with_only_the_inventory_changing() {
    let mut world = World::new();
    let workspace = world.workspace();
    let inventory = child(&mut world, workspace, ClassName::Inventory);
    let tool = child(&mut world, inventory, ClassName::Tool);
    let handle = part(&mut world, tool);
    let light = child(&mut world, handle, ClassName::PointLight);
    let assets = Assets::default();
    let mut cache = retained();
    assert_eq!(
        draw_checked(&world, &assets, &mut cache, "put away").len(),
        0
    );
    set(
        &mut world,
        inventory,
        "Equipped",
        Value::Instance(Some(tool)),
    );
    let held = from_cache(&world, &assets, &mut cache);
    assert_eq!(
        held.items.len(),
        1,
        "in hand: the handle and its light are drawn"
    );
    assert_eq!(held.lights.len(), 1);
    assert_same(&held, &from_scratch(&world, &assets), "in hand");
    set(&mut world, inventory, "Equipped", Value::Instance(None));
    let away = from_cache(&world, &assets, &mut cache);
    assert_eq!(away.items.len(), 0);
    assert_eq!(away.lights.len(), 0);
    let _ = light;
}

#[test]
fn a_part_moved_into_a_union_stops_being_drawn() {
    let mut world = World::new();
    let workspace = world.workspace();
    let loose = part(&mut world, workspace);
    let assets = Assets::default();
    let mut cache = retained();
    assert_eq!(draw_checked(&world, &assets, &mut cache, "loose").len(), 1);
    let union = child(&mut world, workspace, ClassName::UnionOperation);
    world.set_parent(loose, Some(union)).unwrap();
    draw_checked(&world, &assets, &mut cache, "in a union");
    world.set_parent(loose, Some(workspace)).unwrap();
    draw_checked(&world, &assets, &mut cache, "separated");
}

#[test]
fn an_animated_mesh_part_is_drawn_posed_then_still_again() {
    const NOODLE: &str = "assets/models/noodle.fbx";
    let mut world = World::new();
    let workspace = world.workspace();
    let part = mesh_part(&mut world, workspace, NOODLE);
    let assets = Assets::new(examples().join("hello-animation"));
    let model = assets.model_blocking(NOODLE).unwrap();
    let rig = model.rig.clone().unwrap();
    let rest = Arc::new(model.pose(&rig.rest_pose()).unwrap());
    let mut cache = retained();
    let posed = Poses::from([(part, rest.clone())]);
    let camera = camera();

    let still = from_cache(&world, &assets, &mut cache);
    assert!(still.items[0].skin.is_none());
    let moving = build_draw_list_posed(&world, &assets, camera, &posed, &mut cache);
    let skin = moving.items[0].skin.clone().unwrap();
    assert_eq!(skin.joints(), rest.joints());
    drop(moving);
    drop(skin);
    // Nothing kept the skin, so the next pose can reuse its buffers.
    assert_eq!(
        cache.draw_cache.rebuilt_last_frame(),
        0,
        "posing isn't keeping"
    );
    let still = from_cache(&world, &assets, &mut cache);
    assert!(
        still.items[0].skin.is_none(),
        "still again once it has no pose"
    );
    assert_same(&still, &from_scratch(&world, &assets), "still");
}

/// A project folder with `file` copied in from the examples, deleted by
/// the caller.
fn project_with(name: &str, source: &Path, file: &str) -> PathBuf {
    let folder =
        std::env::temp_dir().join(format!("axcenity-retained-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    let target = folder.join(file);
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::copy(source.join(file), target).unwrap();
    folder
}

fn mesh_key(item: &DrawItem) -> u64 {
    match &item.shape {
        Shape::Mesh(mesh) => mesh.key(),
        shape => panic!("drawn as {shape:?}"),
    }
}

#[test]
fn a_hot_reloaded_model_is_drawn_new() {
    let hello_mesh = examples().join("hello-mesh");
    let folder = project_with("reload", &hello_mesh, PYRAMID);
    let mut world = World::new();
    let workspace = world.workspace();
    mesh_part(&mut world, workspace, PYRAMID);
    let assets = Assets::new(&folder);
    let mut cache = retained();
    let before = mesh_key(&draw_checked(&world, &assets, &mut cache, "first")[0]);

    let file = std::fs::read(folder.join(PYRAMID)).unwrap();
    std::fs::write(folder.join(PYRAMID), [&file[..], b"\n"].concat()).unwrap();
    assert_eq!(assets.reload(PYRAMID), Reload::Reloaded);
    let after = mesh_key(&draw_checked(&world, &assets, &mut cache, "reloaded")[0]);
    assert_ne!(before, after, "its mesh is the new file's");
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn a_model_finishing_loading_on_a_worker_is_drawn() {
    let hello_mesh = examples().join("hello-mesh");
    let folder = project_with("async-model", &hello_mesh, PYRAMID);
    let mut world = World::new();
    let workspace = world.workspace();
    mesh_part(&mut world, workspace, PYRAMID);
    let assets = Assets::new(&folder);
    assets.load_textures_in_background(|| {});
    let mut cache = retained();
    // Whatever it shows while the model is on its way...
    from_cache(&world, &assets, &mut cache);
    assets.request_model(PYRAMID).wait();
    // ...the frames after the model is in have it.
    let items = draw_checked(&world, &assets, &mut cache, "the model is in");
    assert!(matches!(items[0].shape, Shape::Mesh(_)));
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn a_texture_finishing_loading_on_a_worker_appears_on_a_kept_item() {
    const IMAGE: &str = "assets/textures/picture.png";
    let hello_mesh = examples().join("hello-mesh");
    let folder = project_with("async-texture", &hello_mesh, PYRAMID);
    std::fs::create_dir_all(folder.join("assets/textures")).unwrap();
    image::RgbaImage::new(2, 2)
        .save(folder.join(IMAGE))
        .unwrap();

    let mut world = World::new();
    let workspace = world.workspace();
    let part = part(&mut world, workspace);
    let look = child(&mut world, part, ClassName::SurfaceAppearance);
    set(&mut world, look, "ColorMap", Value::String(IMAGE.into()));

    let assets = Assets::new(&folder);
    assets.load_textures_in_background(|| {});
    let (release, released) = mpsc::channel::<()>();
    let released = Mutex::new(released);
    assets.set_load_gate(move |path| {
        if path == IMAGE {
            let _ = released.lock().unwrap().recv_timeout(LIMIT);
        }
    });
    let mut cache = retained();
    let waiting = from_cache(&world, &assets, &mut cache);
    assert!(waiting.items[0].material.base_color_texture.is_none());
    release.send(()).unwrap();
    assets.request_texture(IMAGE).wait();
    let items = draw_checked(&world, &assets, &mut cache, "the texture is in");
    assert!(
        items[0].material.base_color_texture.is_some(),
        "the part's revision didn't change, the assets did"
    );
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn editing_a_material_file_or_forgetting_a_texture_updates_kept_items() {
    const PATH: &str = "assets/materials/test.material.json";
    let folder =
        std::env::temp_dir().join(format!("axcenity-retained-material-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(folder.join("assets/materials")).unwrap();
    let mut world = World::new();
    let workspace = world.workspace();
    let part = part(&mut world, workspace);
    set(
        &mut world,
        part,
        "MaterialVariant",
        Value::String(PATH.into()),
    );
    let assets = Assets::new(&folder);
    let red = MaterialFile {
        color: Some([1.0, 0.0, 0.0]),
        ..MaterialFile::default()
    };
    assets.put_material_file(PATH, Some(red));
    let mut cache = retained();
    let first = draw_checked(&world, &assets, &mut cache, "first");
    let blue = MaterialFile {
        color: Some([0.0, 0.0, 1.0]),
        ..MaterialFile::default()
    };
    assets.put_material_file(PATH, Some(blue));
    let edited = draw_checked(&world, &assets, &mut cache, "edited");
    assert_ne!(first[0].material.base_color, edited[0].material.base_color);
    assets.forget(PATH);
    draw_checked(&world, &assets, &mut cache, "forgotten");
    let _ = assets.take_errors();
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn turning_the_blueprint_image_on_and_off_updates_kept_items() {
    let mut world = World::new();
    let workspace = world.workspace();
    let part = part(&mut world, workspace);
    set(&mut world, part, "BlueprintTexture", Value::Bool(true));
    let assets = Assets::default();
    let mut cache = retained();
    draw_checked(&world, &assets, &mut cache, "no image");
    let image = Arc::new(axcenity_assets::Image::new(1, 1, vec![255; 4]));
    assets.set_blueprint(Some(image.clone()));
    let on = draw_checked(&world, &assets, &mut cache, "the image on");
    assert!(on[0].material.base_color_texture.is_some());
    // The same image again, as the editor does every frame: nothing new.
    assets.set_blueprint(Some(image));
    draw_checked(&world, &assets, &mut cache, "the same image");
    assert_eq!(cache.draw_cache.rebuilt_last_frame(), 0);
    assets.set_blueprint(None);
    draw_checked(&world, &assets, &mut cache, "the image off");
}

#[test]
fn a_copy_of_the_world_isn_t_taken_for_the_original() {
    let mut world = World::new();
    let workspace = world.workspace();
    let block = part(&mut world, workspace);
    let assets = Assets::default();
    let mut cache = retained();
    draw_checked(&world, &assets, &mut cache, "the original");

    // Editing the copy and the original once each leaves the same revision.
    let mut copy = world.clone();
    set(&mut copy, block, "Position", vector(1.0, 2.0, 3.0));
    draw_checked(&copy, &assets, &mut cache, "the copy");
    set(&mut world, block, "Position", vector(7.0, 7.0, 7.0));
    draw_checked(&world, &assets, &mut cache, "the original again");
}

#[test]
fn turning_retained_items_off_draws_from_scratch() {
    let mut world = World::new();
    let workspace = world.workspace();
    part(&mut world, workspace);
    let assets = Assets::default();
    let mut cache = TerrainCache::new();
    cache.set_retained_draw_list(false);
    assert!(!cache.retained_draw_list());
    draw_checked(&world, &assets, &mut cache, "off");
    assert_eq!(cache.draw_cache.rebuilt_last_frame(), 0, "it isn't used");
}
