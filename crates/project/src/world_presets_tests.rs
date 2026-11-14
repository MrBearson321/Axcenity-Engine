use super::*;
use axcenity_core::Vector3;

/// A terrain `height` studs tall with a layer, a biome holding a rule, a
/// rule of its own and water a quarter of the way up.
fn setup(height: f32) -> (World, InstanceId) {
    let mut world = World::new();
    let terrain = world.create(ClassName::Terrain).unwrap();
    world.set_parent(terrain, Some(world.workspace())).unwrap();
    world
        .set_property(
            terrain,
            "Size",
            Value::Vector3(Vector3::new(256.0, height, 256.0)),
        )
        .unwrap();
    let add = |world: &mut World, class, name: &str, parent| {
        let id = world.create(class).unwrap();
        world.set_parent(id, Some(parent)).unwrap();
        world.set_name(id, name).unwrap();
        id
    };
    add(&mut world, ClassName::TerrainLayer, "Forest Floor", terrain);
    let biome = add(&mut world, ClassName::Biome, "Lowland Forest", terrain);
    world
        .set_property(biome, "MaxHeight", Value::Number(40.0))
        .unwrap();
    world
        .set_property(biome, "GroundLayer", Value::String("Forest Floor".into()))
        .unwrap();
    let trees = add(&mut world, ClassName::ScatterRule, "Trees", biome);
    world
        .set_property(trees, "Models", Value::String("plant:Tree".into()))
        .unwrap();
    add(&mut world, ClassName::ScatterRule, "Grass", terrain);
    let placement = Placement::of(world.get(terrain).unwrap()).unwrap();
    world
        .set_property(terrain, "Water", Value::Bool(true))
        .unwrap();
    world
        .set_property(
            terrain,
            "WaterLevel",
            Value::Number(f64::from(placement.world_y(0.25))),
        )
        .unwrap();
    (world, terrain)
}

fn names(world: &World, terrain: InstanceId) -> Vec<String> {
    let mut out = Vec::new();
    for id in preset_objects(world, terrain) {
        out.push(world.get(id).unwrap().name().to_owned());
        for child in world.descendants(id) {
            out.push(format!("  {}", world.get(child).unwrap().name()));
        }
    }
    out
}

#[test]
fn a_preset_round_trips_through_its_file_and_rebuilds_the_terrains_setup() {
    let (world, terrain) = setup(100.0);
    let preset = WorldPreset::capture(
        &world,
        terrain,
        " Woods ",
        "Trees low down",
        axcenity_terrain::generate::Preset::Island.settings(),
    )
    .unwrap();
    assert_eq!(preset.name, "Woods");
    assert_eq!(preset.water, Some(0.25));
    let read = WorldPreset::from_json(&preset.to_json()).unwrap();
    assert_eq!(read, preset);

    // Another terrain, with its own layer and no water.
    let mut other = World::new();
    let land = other.create(ClassName::Terrain).unwrap();
    other.set_parent(land, Some(other.workspace())).unwrap();
    other
        .set_property(
            land,
            "Size",
            Value::Vector3(Vector3::new(512.0, 100.0, 512.0)),
        )
        .unwrap();
    let old = other.create(ClassName::TerrainLayer).unwrap();
    other.set_parent(old, Some(land)).unwrap();
    read.apply_objects(&mut other, land).unwrap();
    assert_eq!(
        names(&other, land),
        ["Forest Floor", "Lowland Forest", "  Trees", "Grass"]
    );
    assert!(other.get(old).is_none(), "the old layer went");
    assert_eq!(
        other.get(land).unwrap().property("Water"),
        Some(&Value::Bool(true))
    );
    let placement = Placement::of(other.get(land).unwrap()).unwrap();
    let level = other.get(land).unwrap().property("WaterLevel").unwrap();
    assert_eq!(
        level.as_number().map(|l| l as f32),
        Some(placement.world_y(0.25))
    );
}

#[test]
fn heights_follow_the_terrain_a_preset_is_applied_to() {
    let (world, terrain) = setup(100.0);
    let preset = WorldPreset::capture(&world, terrain, "Woods", "", Default::default()).unwrap();
    let (mut tall, land) = setup(300.0);
    preset.apply_objects(&mut tall, land).unwrap();
    let biome = preset_objects(&tall, land)[1];
    let number = |name| tall.get(biome).unwrap().property(name).unwrap().as_number();
    assert_eq!(number("MaxHeight"), Some(120.0));
    assert_eq!(number("HeightFade"), Some(24.0));
    // "No top" stays no top.
    let rule = tall.get(biome).unwrap().children()[0];
    let top = tall
        .get(rule)
        .unwrap()
        .property("MaxHeight")
        .unwrap()
        .as_number();
    assert_eq!(top, Some(10000.0));
}

#[test]
fn a_damaged_preset_changes_nothing_and_a_newer_one_is_refused() {
    let (world, terrain) = setup(100.0);
    let mut preset =
        WorldPreset::capture(&world, terrain, "Woods", "", Default::default()).unwrap();
    preset.objects = serde_json::json!({ "format_version": 1, "instances": [{ "class": "NoSuchClass", "name": "x" }] });
    let (mut target, land) = setup(100.0);
    let before = names(&target, land);
    assert!(preset.apply_objects(&mut target, land).is_err());
    assert_eq!(names(&target, land), before);

    preset.format = WORLD_PRESET_FORMAT + 1;
    assert!(matches!(
        WorldPreset::from_json(&preset.to_json()),
        Err(WorldPresetError::TooNew { .. })
    ));
}

#[test]
fn presets_save_to_a_folder_and_list_by_name() {
    let folder =
        std::env::temp_dir().join(format!("axcenity-world-presets-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    let (world, terrain) = setup(100.0);
    for name in ["Tundra", "alpine: high"] {
        let preset = WorldPreset::capture(&world, terrain, name, "", Default::default()).unwrap();
        save_world_preset(&folder, &preset).unwrap();
    }
    std::fs::write(folder.join("broken.axworld"), "{").unwrap();
    std::fs::write(folder.join("notes.txt"), "not a preset").unwrap();
    let (found, problems) = list_world_presets(&folder);
    let listed: Vec<&str> = found.iter().map(|(_, p)| p.name.as_str()).collect();
    assert_eq!(listed, ["alpine: high", "Tundra"]);
    assert!(found[0].0.ends_with("alpine- high.axworld"));
    assert_eq!(problems.len(), 1);
    assert!(list_world_presets(&folder.join("missing")).0.is_empty());
    std::fs::remove_dir_all(folder).unwrap();
}
