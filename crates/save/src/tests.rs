use std::path::PathBuf;

use axcenity_core::{Color3, Material, PartType, ParticleType, TextureChannel, UDim2, Vector2};

use super::*;

/// An empty folder of its own for one test's slots.
fn folder(test: &str) -> PathBuf {
    let folder = std::env::temp_dir().join(format!("axcenity-save-{test}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&folder);
    folder
}

fn part(world: &mut World, parent: InstanceId, name: &str, position: Vector3) -> InstanceId {
    let id = world.create(ClassName::Part).unwrap();
    world.set_name(id, name).unwrap();
    world
        .set_property(id, "Position", Value::Vector3(position))
        .unwrap();
    world.set_parent(id, Some(parent)).unwrap();
    id
}

fn position(world: &World, id: InstanceId) -> Vector3 {
    world
        .get(id)
        .unwrap()
        .property("Position")
        .and_then(Value::as_vector3)
        .unwrap()
}

/// A level: a floor, a crate, a coin, a checkpoint and a character.
struct Level {
    world: World,
    crate_part: InstanceId,
    coin: InstanceId,
    checkpoint: InstanceId,
    body: InstanceId,
}

fn level() -> Level {
    let mut world = World::new();
    let workspace = world.workspace();
    part(&mut world, workspace, "Floor", Vector3::new(0.0, -0.5, 0.0));
    let crate_part = part(&mut world, workspace, "Crate", Vector3::new(5.0, 1.0, 0.0));
    let coin = part(&mut world, workspace, "Coin", Vector3::new(-5.0, 1.0, 0.0));
    let checkpoint = world.create(ClassName::Checkpoint).unwrap();
    world.set_name(checkpoint, "Cave").unwrap();
    world
        .set_property(
            checkpoint,
            "Position",
            Value::Vector3(Vector3::new(20.0, 0.0, 10.0)),
        )
        .unwrap();
    world.set_parent(checkpoint, Some(workspace)).unwrap();
    let hero = world.create(ClassName::Model).unwrap();
    world.set_name(hero, "Hero").unwrap();
    world.set_parent(hero, Some(workspace)).unwrap();
    let body = part(&mut world, hero, "Body", Vector3::new(0.0, 2.5, 0.0));
    world
        .set_property(body, "Size", Value::Vector3(Vector3::new(2.0, 5.0, 1.0)))
        .unwrap();
    Level {
        world,
        crate_part,
        coin,
        checkpoint,
        body,
    }
}

#[test]
fn saving_a_slot_and_loading_it_restores_player_data_world_state_checkpoint_and_settings() {
    let folder = folder("round-trip");
    let Level {
        mut world,
        crate_part,
        coin,
        checkpoint,
        ..
    } = level();
    let mut saves = Saves::new(Some(folder.clone()));
    saves
        .player_data
        .insert("Coins".to_owned(), Value::Number(3.0));
    saves
        .player_data
        .insert("Name".to_owned(), Value::String("Ada".to_owned()));
    saves
        .settings
        .insert("Volume".to_owned(), Value::Number(0.5));
    saves.set_checkpoint(&world, checkpoint).unwrap();
    // The world moves on: the crate is pushed, the coin is taken, a flag is planted.
    world
        .set_property(
            crate_part,
            "Position",
            Value::Vector3(Vector3::new(9.0, 1.0, 3.0)),
        )
        .unwrap();
    world.destroy(coin).unwrap();
    let workspace = world.workspace();
    let flag = part(&mut world, workspace, "Flag", Vector3::new(1.0, 2.0, 3.0));
    world
        .set_property(flag, "Color", Value::Color3(Color3::new(1.0, 0.0, 0.0)))
        .unwrap();
    let lighting = world.lighting();
    world
        .set_property(lighting, "Exposure", Value::Number(2.0))
        .unwrap();
    let path = saves.save(&world, 1).unwrap();
    assert_eq!(path, folder.join("slot-1.json"));
    assert!(saves.has_slot(1) && !saves.has_slot(2));

    // A new run of the game: the level as it starts.
    let Level {
        world: mut fresh,
        crate_part: fresh_crate,
        checkpoint: fresh_checkpoint,
        ..
    } = level();
    let mut loaded = Saves::new(Some(folder.clone()));
    loaded.load(&mut fresh, 1).unwrap();
    assert_eq!(position(&fresh, fresh_crate), Vector3::new(9.0, 1.0, 3.0));
    let workspace = fresh.workspace();
    assert_eq!(fresh.find_first_child(workspace, "Coin"), None);
    let flag = fresh.find_first_child(workspace, "Flag").unwrap();
    assert_eq!(position(&fresh, flag), Vector3::new(1.0, 2.0, 3.0));
    assert_eq!(
        fresh.get(flag).unwrap().property("Color"),
        Some(&Value::Color3(Color3::new(1.0, 0.0, 0.0)))
    );
    assert_eq!(
        fresh.get(fresh.lighting()).unwrap().property("Exposure"),
        Some(&Value::Number(2.0))
    );
    assert_eq!(loaded.player_data, saves.player_data);
    assert_eq!(loaded.settings, saves.settings);
    assert_eq!(loaded.checkpoint(&fresh), Some(fresh_checkpoint));
    // And what was captured at the checkpoint came with it: the coin was still there.
    loaded.restore_checkpoint(&mut fresh).unwrap();
    assert!(fresh.find_first_child(workspace, "Coin").is_some());
    assert_eq!(fresh.find_first_child(workspace, "Flag"), None);
    assert_eq!(position(&fresh, fresh_crate), Vector3::new(5.0, 1.0, 0.0));
}

#[test]
fn loading_into_the_running_game_keeps_the_instances_scripts_hold() {
    let folder = folder("same-ids");
    let Level {
        mut world,
        crate_part,
        coin,
        ..
    } = level();
    let mut saves = Saves::new(Some(folder));
    saves.save(&world, 4).unwrap();
    world
        .set_property(
            crate_part,
            "Position",
            Value::Vector3(Vector3::new(0.0, 9.0, 0.0)),
        )
        .unwrap();
    world.destroy(coin).unwrap();
    saves.load(&mut world, 4).unwrap();
    // The same crate, back where it was; the coin is made again.
    assert_eq!(position(&world, crate_part), Vector3::new(5.0, 1.0, 0.0));
    let coin = world.find_first_child(world.workspace(), "Coin").unwrap();
    assert_eq!(position(&world, coin), Vector3::new(-5.0, 1.0, 0.0));
}

#[test]
fn restoring_a_checkpoint_puts_the_character_there_with_the_state_captured_for_it() {
    let Level {
        mut world,
        crate_part,
        checkpoint,
        body,
        ..
    } = level();
    let mut saves = Saves::new(None);
    saves
        .player_data
        .insert("Coins".to_owned(), Value::Number(1.0));
    saves.set_checkpoint(&world, checkpoint).unwrap();
    assert_eq!(saves.checkpoint(&world), Some(checkpoint));
    saves
        .player_data
        .insert("Coins".to_owned(), Value::Number(5.0));
    world
        .set_property(
            crate_part,
            "Position",
            Value::Vector3(Vector3::new(30.0, 1.0, 0.0)),
        )
        .unwrap();
    world
        .set_property(
            body,
            "Position",
            Value::Vector3(Vector3::new(-40.0, -20.0, 0.0)),
        )
        .unwrap();

    let at = saves.restore_checkpoint(&mut world).unwrap();
    assert_eq!(at, Vector3::new(20.0, 0.0, 10.0));
    place_character(&mut world, &[body], at);
    // Feet on the checkpoint: the 5-stud body's middle is 2.5 above it.
    assert_eq!(position(&world, body), Vector3::new(20.0, 2.5, 10.0));
    assert_eq!(position(&world, crate_part), Vector3::new(5.0, 1.0, 0.0));
    assert_eq!(saves.player_data["Coins"], Value::Number(1.0));
}

#[test]
fn restoring_needs_a_checkpoint_that_is_still_there() {
    let Level {
        mut world,
        checkpoint,
        crate_part,
        ..
    } = level();
    let mut saves = Saves::new(None);
    assert!(matches!(
        saves.restore_checkpoint(&mut world),
        Err(SaveError::NoCheckpoint)
    ));
    assert!(matches!(
        saves.set_checkpoint(&world, crate_part),
        Err(SaveError::NotACheckpoint(_))
    ));
    saves.set_checkpoint(&world, checkpoint).unwrap();
    world.destroy(checkpoint).unwrap();
    assert_eq!(saves.checkpoint(&world), None);
    assert!(matches!(
        saves.restore_checkpoint(&mut world),
        Err(SaveError::CheckpointGone)
    ));
}

#[test]
fn loading_a_missing_damaged_or_newer_slot_fails_without_touching_the_world() {
    let folder = folder("failures");
    let Level {
        mut world,
        crate_part,
        ..
    } = level();
    world
        .set_property(
            crate_part,
            "Position",
            Value::Vector3(Vector3::new(7.0, 7.0, 7.0)),
        )
        .unwrap();
    let mut saves = Saves::new(Some(folder.clone()));
    saves
        .player_data
        .insert("Coins".to_owned(), Value::Number(2.0));
    let before = snapshot(&world);

    let err = saves.load(&mut world, 3).unwrap_err();
    assert!(matches!(err, SaveError::MissingSlot(3)));
    assert_eq!(
        err.to_string(),
        "save slot 3 doesn't exist yet; save to it first, or check with SaveService:HasSlot(3)"
    );

    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("slot-5.json"), "{ not json").unwrap();
    assert!(matches!(
        saves.load(&mut world, 5),
        Err(SaveError::Damaged { slot: 5, .. })
    ));
    fs::write(
        folder.join("slot-6.json"),
        r#"{"version": 99, "world": "a shape this game doesn't know"}"#,
    )
    .unwrap();
    assert!(matches!(
        saves.load(&mut world, 6),
        Err(SaveError::Newer {
            slot: 6,
            version: 99
        })
    ));

    assert_eq!(snapshot(&world), before);
    assert_eq!(saves.player_data["Coins"], Value::Number(2.0));
}

#[test]
fn slots_are_whole_numbers_from_one_and_need_a_folder() {
    assert_eq!(slot_number(1.0).unwrap(), 1);
    assert_eq!(slot_number(999.0).unwrap(), 999);
    for bad in [0.0, -1.0, 1.5, 1000.0, f64::NAN] {
        assert!(
            matches!(slot_number(bad), Err(SaveError::BadSlot(_))),
            "{bad}"
        );
    }
    let saves = Saves::new(None);
    assert!(matches!(
        saves.save(&World::new(), 1),
        Err(SaveError::NoFolder)
    ));
    assert!(!saves.has_slot(1));
}

#[test]
fn scripts_and_screen_guis_are_not_world_state() {
    let folder = folder("not-state");
    let mut world = World::new();
    let workspace = world.workspace();
    let script = world.create(ClassName::Script).unwrap();
    world.set_parent(script, Some(workspace)).unwrap();
    let gui = world.create(ClassName::ScreenGui).unwrap();
    world.set_parent(gui, Some(workspace)).unwrap();
    let saves = Saves::new(Some(folder.clone()));
    saves.save(&world, 1).unwrap();
    let file = saves.read(1).unwrap();
    assert!(
        file.world.instances.is_empty(),
        "{:?}",
        file.world.instances
    );

    // Loading a save without them leaves them where they are.
    let mut saves = Saves::new(Some(folder));
    let made_later = world.create(ClassName::LocalScript).unwrap();
    world.set_parent(made_later, Some(workspace)).unwrap();
    saves.load(&mut world, 1).unwrap();
    for id in [script, gui, made_later] {
        assert_eq!(world.get(id).unwrap().parent(), Some(workspace));
    }
}

#[test]
fn every_kind_of_value_survives_a_slot_file() {
    let values = [
        Value::Bool(true),
        Value::Number(-2.25),
        Value::String("hello".to_owned()),
        Value::Vector3(Vector3::new(1.0, 2.5, -3.0)),
        Value::Vector2(Vector2::new(4.0, 5.0)),
        Value::UDim2(UDim2::new(0.5, 10.0, 0.25, -4.0)),
        Value::Color3(Color3::new(0.5, 0.25, 1.0)),
        Value::PartType(PartType::Ball),
        Value::Material(Material::Neon),
        Value::TextureChannel(TextureChannel::Green),
        Value::ParticleType(ParticleType::Spark),
        Value::CFrame(axcenity_core::CFrame {
            position: Vector3::new(1.0, 2.0, 3.0),
            rotation: [0.0, 0.6, 0.0, 0.8],
        }),
        Value::EnumItem(axcenity_core::EnumItem {
            enum_name: "KeyCode".into(),
            item: "E".into(),
        }),
    ];
    for value in values {
        let json = serde_json::to_string(&SavedValue(value.clone())).unwrap();
        let back: SavedValue = serde_json::from_str(&json).unwrap();
        assert_eq!(back.0, value, "{json}");
    }
    assert!(serde_json::from_str::<SavedValue>(r#"{"Vector3": [1, 2]}"#).is_err());
}

#[test]
fn a_restored_material_keeps_the_metalness_it_was_saved_with() {
    let folder = folder("material");
    let mut world = World::new();
    let workspace = world.workspace();
    let block = part(&mut world, workspace, "Block", Vector3::default());
    world
        .set_property(block, "Material", Value::Material(Material::Metal))
        .unwrap();
    world
        .set_property(block, "Metalness", Value::Number(0.3))
        .unwrap();
    let mut saves = Saves::new(Some(folder));
    saves.save(&world, 1).unwrap();
    world
        .set_property(block, "Material", Value::Material(Material::Plastic))
        .unwrap();
    saves.load(&mut world, 1).unwrap();
    let block = world.get(block).unwrap();
    assert_eq!(
        block.property("Material"),
        Some(&Value::Material(Material::Metal))
    );
    assert_eq!(block.property("Metalness"), Some(&Value::Number(0.3)));
}

#[test]
fn the_save_folder_is_named_after_the_game() {
    assert_eq!(folder_name("Hello Saves"), "Hello Saves");
    assert_eq!(folder_name("a/b:c*"), "a_b_c_");
    assert_eq!(folder_name("  .. "), "Untitled");
    if let Some(folder) = game_folder("Hello Saves") {
        assert!(folder.ends_with("Hello Saves"), "{}", folder.display());
    }
}

/// The level of `slot-v1.json`: its checkpoint is in a Checkpoints folder.
fn v1_level() -> Level {
    let level = level();
    let mut world = level.world;
    let folder = world.create(ClassName::Folder).unwrap();
    world.set_name(folder, "Checkpoints").unwrap();
    let workspace = world.workspace();
    world.set_parent(folder, Some(workspace)).unwrap();
    world.set_parent(level.checkpoint, Some(folder)).unwrap();
    Level { world, ..level }
}

/// What `slot-v1.json` was saved from, played out on `level`: player data
/// and settings, a checkpoint set, then the crate pushed and the coin taken.
fn play_the_v1_save(level: &Level, world: &mut World, saves: &mut Saves) {
    saves
        .player_data
        .insert("Coins".to_owned(), Value::Number(3.0));
    saves
        .settings
        .insert("Volume".to_owned(), Value::Number(0.5));
    saves.set_checkpoint(world, level.checkpoint).unwrap();
    saves
        .player_data
        .insert("Coins".to_owned(), Value::Number(7.0));
    world
        .set_property(
            level.crate_part,
            "Position",
            Value::Vector3(Vector3::new(9.0, 1.0, 3.0)),
        )
        .unwrap();
    world.destroy(level.coin).unwrap();
}

#[test]
fn a_version_1_slot_loads_migrates_and_restores_the_same_player_world_checkpoint_and_settings() {
    let folder = folder("migrate-v1");
    fs::create_dir_all(&folder).unwrap();
    let v1 = include_str!("fixtures/slot-v1.json");
    fs::write(folder.join("slot-1.json"), v1).unwrap();

    let level = v1_level();
    let mut world = level.world.clone();
    let mut saves = Saves::new(Some(folder.clone()));
    saves.load(&mut world, 1).unwrap();
    assert_eq!(
        position(&world, level.crate_part),
        Vector3::new(9.0, 1.0, 3.0)
    );
    assert!(!world.contains(level.coin));
    assert_eq!(saves.player_data["Coins"], Value::Number(7.0));
    assert_eq!(saves.settings["Volume"], Value::Number(0.5));
    assert_eq!(saves.checkpoint(&world), Some(level.checkpoint));
    let at = saves.restore_checkpoint(&mut world).unwrap();
    assert_eq!(at, Vector3::new(20.0, 0.0, 10.0));
    assert_eq!(
        position(&world, level.crate_part),
        Vector3::new(5.0, 1.0, 0.0)
    );
    let workspace = world.workspace();
    assert!(world.find_first_child(workspace, "Coin").is_some());
    assert_eq!(saves.player_data["Coins"], Value::Number(3.0));

    // Migrated, it is what this version saves for the same game.
    let migrated = saves.read(1).unwrap();
    assert_eq!(migrated.version, SAVE_VERSION);
    let level = v1_level();
    let mut world = level.world.clone();
    let mut current = Saves::new(Some(folder.clone()));
    play_the_v1_save(&level, &mut world, &mut current);
    current.save(&world, 2).unwrap();
    assert_eq!(current.read(2).unwrap(), migrated);

    // Loading never rewrites the old file.
    assert_eq!(fs::read_to_string(folder.join("slot-1.json")).unwrap(), v1);
}

#[test]
fn moving_a_version_1_checkpoint_finds_it_by_name_or_drops_it() {
    let file = |path: serde_json::Value| {
        serde_json::json!({
            "version": 1,
            "world": { "instances": [
                { "parent": "Workspace", "class": "Folder", "name": "Pads" },
                { "parent": 0, "class": "Checkpoint", "name": "Pad" },
                { "parent": 0, "class": "Checkpoint", "name": "Pad" },
                { "parent": "Workspace", "class": "Part", "name": "Pad" },
            ]},
            "checkpoint": { "path": path, "player_data": {} },
        })
    };
    // The first of the same name, as loading version 1 found it.
    let mut found = file(serde_json::json!(["Pads", "Pad"]));
    assert_eq!(migrate(&mut found, 1), Ok(1));
    assert_eq!(found["version"], 2);
    assert_eq!(found["checkpoint"]["instance"], 1);
    assert!(found["checkpoint"].get("path").is_none());
    // A path to something that isn't a Checkpoint, or to nothing.
    for path in [serde_json::json!(["Pad"]), serde_json::json!(["Gone"])] {
        let mut dropped = file(path);
        assert_eq!(migrate(&mut dropped, 1), Ok(1));
        assert!(dropped["checkpoint"].is_null());
    }
    // A file with no checkpoint only changes its version.
    let mut none = serde_json::json!({ "version": 1, "checkpoint": null });
    assert_eq!(migrate(&mut none, 1), Ok(1));
    assert_eq!(
        none,
        serde_json::json!({ "version": 2, "checkpoint": null })
    );
    assert!(migrate(&mut serde_json::json!({}), 0).is_err());
}

#[test]
fn two_checkpoints_with_the_same_name_are_told_apart() {
    let folder = folder("same-name");
    let mut world = World::new();
    let workspace = world.workspace();
    let mut checkpoints = Vec::new();
    for x in [0.0, 50.0] {
        let checkpoint = world.create(ClassName::Checkpoint).unwrap();
        world
            .set_property(
                checkpoint,
                "Position",
                Value::Vector3(Vector3::new(x, 0.0, 0.0)),
            )
            .unwrap();
        world.set_parent(checkpoint, Some(workspace)).unwrap();
        checkpoints.push(checkpoint);
    }
    let mut saves = Saves::new(Some(folder));
    saves.set_checkpoint(&world, checkpoints[1]).unwrap();
    saves.save(&world, 1).unwrap();
    saves.set_checkpoint(&world, checkpoints[0]).unwrap();
    saves.load(&mut world, 1).unwrap();
    assert_eq!(saves.checkpoint(&world), Some(checkpoints[1]));
    assert_eq!(
        saves.restore_checkpoint(&mut world).unwrap(),
        Vector3::new(50.0, 0.0, 0.0)
    );
}

#[test]
fn a_newer_slot_is_refused_and_its_file_is_unchanged() {
    let folder = folder("newer");
    fs::create_dir_all(&folder).unwrap();
    let newer = format!(
        r#"{{"version": {}, "world": "a shape this game doesn't know"}}"#,
        SAVE_VERSION + 1
    );
    let slot = folder.join("slot-4.json");
    fs::write(&slot, &newer).unwrap();
    let Level { mut world, .. } = level();
    let before = snapshot(&world);
    let mut saves = Saves::new(Some(folder));
    saves
        .player_data
        .insert("Coins".to_owned(), Value::Number(1.0));

    let err = saves.load(&mut world, 4).unwrap_err();
    assert!(matches!(err, SaveError::Newer { slot: 4, .. }));
    let err = saves.save(&world, 4).unwrap_err();
    assert!(matches!(err, SaveError::WontReplaceNewer { slot: 4, .. }));
    assert!(err.to_string().contains("won't write over it"));

    assert_eq!(fs::read_to_string(&slot).unwrap(), newer);
    assert_eq!(snapshot(&world), before);
    assert_eq!(saves.player_data["Coins"], Value::Number(1.0));
}

#[test]
fn a_current_slot_round_trips_without_a_migration_step() {
    let folder = folder("current");
    let level = v1_level();
    let mut world = level.world.clone();
    let mut saves = Saves::new(Some(folder));
    play_the_v1_save(&level, &mut world, &mut saves);
    let path = saves.save(&world, 1).unwrap();

    let mut json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(json["version"], SAVE_VERSION);
    let unchanged = json.clone();
    assert_eq!(migrate(&mut json, SAVE_VERSION), Ok(0));
    assert_eq!(json, unchanged);

    let mut fresh = v1_level().world;
    let mut loaded = Saves::new(saves.folder().map(Path::to_path_buf));
    loaded.load(&mut fresh, 1).unwrap();
    assert_eq!(snapshot(&fresh), snapshot(&world));
    assert_eq!(loaded.player_data, saves.player_data);
    assert_eq!(loaded.settings, saves.settings);
    assert_eq!(loaded.checkpoint(&fresh), Some(level.checkpoint));
}

#[test]
fn project_files_are_never_read_as_save_files() {
    let folder = folder("project-files");
    fs::create_dir_all(&folder).unwrap();
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-saves");
    let Level { mut world, .. } = level();
    let before = snapshot(&world);
    let mut saves = Saves::new(Some(folder.clone()));
    for (slot, file) in [(1, "world.json"), (2, "axcenity.json")] {
        let text = fs::read_to_string(project.join(file)).unwrap();
        let path = folder.join(format!("slot-{slot}.json"));
        fs::write(&path, &text).unwrap();
        let err = saves.load(&mut world, slot).unwrap_err();
        assert!(
            matches!(&err, SaveError::Damaged { message, .. } if message == "it has no save version"),
            "{file}: {err}"
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), text);
    }
    assert_eq!(snapshot(&world), before);
}

#[test]
fn attributes_tags_and_value_objects_survive_a_slot_round_trip() {
    let folder = folder("attributes-tags");
    let Level {
        mut world,
        crate_part,
        coin,
        ..
    } = level();
    let workspace = world.workspace();
    world
        .set_attribute(crate_part, "Health", Some(Value::Number(30.0)))
        .unwrap();
    world.add_tag(crate_part, "Breakable").unwrap();
    let target = world.create(ClassName::ObjectValue).unwrap();
    world.set_name(target, "Target").unwrap();
    world.set_parent(target, Some(workspace)).unwrap();
    world
        .set_property(target, "Value", Value::Instance(Some(coin)))
        .unwrap();
    let score = world.create(ClassName::IntValue).unwrap();
    world.set_parent(score, Some(workspace)).unwrap();
    world
        .set_property(score, "Value", Value::Number(7.0))
        .unwrap();
    let mut saves = Saves::new(Some(folder));
    saves.save(&world, 1).unwrap();

    // Changed after saving: all of it goes back on load.
    world
        .set_attribute(crate_part, "Health", Some(Value::Number(1.0)))
        .unwrap();
    world
        .set_attribute(crate_part, "Extra", Some(Value::Bool(true)))
        .unwrap();
    world.remove_tag(crate_part, "Breakable").unwrap();
    world.add_tag(crate_part, "Other").unwrap();
    world.destroy(coin).unwrap();
    world
        .set_property(score, "Value", Value::Number(0.0))
        .unwrap();
    saves.load(&mut world, 1).unwrap();

    let crate_now = world.get(crate_part).unwrap();
    assert_eq!(crate_now.attribute("Health"), Some(&Value::Number(30.0)));
    assert_eq!(crate_now.attribute("Extra"), None);
    assert_eq!(crate_now.tags(), ["Breakable"]);
    let coin = world.find_first_child(workspace, "Coin").unwrap();
    let property = |id| world.get(id).unwrap().property("Value").unwrap().clone();
    assert_eq!(property(target), Value::Instance(Some(coin)));
    assert_eq!(property(score), Value::Number(7.0));
}
