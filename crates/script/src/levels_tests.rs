//! Phase 48: `LevelService` on worlds made here (the hello-levels example
//! runs in the player's tests).

use std::collections::BTreeMap;

use axcenity_core::{ClassName, InstanceId, World};

use super::*;

fn add(world: &mut World, class: ClassName, parent: InstanceId, name: &str) -> InstanceId {
    let id = world.create(class).unwrap();
    world.set_name(id, name).unwrap();
    world.set_parent(id, Some(parent)).unwrap();
    id
}

/// A world whose Workspace holds a part of each of `names`.
fn level(names: &[&str]) -> World {
    let mut world = World::new();
    let workspace = world.workspace();
    for name in names {
        add(&mut world, ClassName::Part, workspace, name);
    }
    world
}

fn names(world: &World, parent: InstanceId) -> Vec<String> {
    world
        .get(parent)
        .unwrap()
        .children()
        .iter()
        .map(|&id| world.get(id).unwrap().name().to_owned())
        .collect()
}

/// The start level "Meadow" (a Grass part and a Hud ScreenGui) in the
/// world, with "Castle" and "Dungeon" beside it.
fn game() -> (World, ScriptRuntime) {
    let mut world = level(&["Grass"]);
    let workspace = world.workspace();
    add(&mut world, ClassName::ScreenGui, workspace, "Hud");
    let storage = world.replicated_storage();
    add(&mut world, ClassName::Folder, storage, "Shared");
    let mut castle = level(&["Tower"]);
    let castle_workspace = castle.workspace();
    add(&mut castle, ClassName::ScreenGui, castle_workspace, "Hud");
    add(
        &mut castle,
        ClassName::ScreenGui,
        castle_workspace,
        "Banner",
    );
    let worlds = BTreeMap::from([
        ("Meadow".to_owned(), world.clone()),
        ("Castle".to_owned(), castle),
        ("Dungeon".to_owned(), level(&["Cell"])),
    ]);
    let mut runtime = ScriptRuntime::new();
    runtime.set_levels("Meadow", "Meadow", worlds);
    (world, runtime)
}

fn printed(runtime: &mut ScriptRuntime) -> Vec<String> {
    runtime
        .take_output()
        .into_iter()
        .map(|line| line.text)
        .collect()
}

#[test]
fn load_level_replaces_the_workspace_but_keeps_its_screen_guis_and_the_services() {
    let (mut world, mut runtime) = game();
    runtime
        .run_command(
            &mut world,
            "local LevelService = game:GetService('LevelService')\n\
             LevelService.LevelLoaded:Connect(function(name) print('loaded', name, LevelService.CurrentLevel) end)\n\
             print(LevelService.CurrentLevel)\n\
             LevelService:LoadLevel('Castle')",
        )
        .unwrap();
    runtime.fire_changes(&mut world);
    assert_eq!(printed(&mut runtime), ["Meadow", "loaded\tCastle\tCastle"]);
    // The kept Hud, then the Castle's own (its Hud isn't brought in twice).
    assert_eq!(names(&world, world.workspace()), ["Hud", "Tower", "Banner"]);
    assert_eq!(names(&world, world.replicated_storage()), ["Shared"]);
    assert_eq!(runtime.take_level_loads(), ["Castle"]);
    assert_eq!(runtime.levels().current(), "Castle");
    assert_eq!(
        runtime
            .saves()
            .levels
            .as_ref()
            .map(|saved| saved.current.as_str()),
        Some("Castle")
    );
}

#[test]
fn add_level_puts_a_level_in_a_model_and_remove_level_takes_it_out() {
    let (mut world, mut runtime) = game();
    runtime
        .run_command(
            &mut world,
            "local LevelService = game.LevelService\n\
             local model = LevelService:AddLevel('Dungeon')\n\
             print(model.ClassName, model.Name, model.Cell.ClassName)\n\
             print(pcall(function() LevelService:AddLevel('Dungeon') end))",
        )
        .unwrap();
    assert_eq!(
        printed(&mut runtime),
        [
            "Model\tDungeon\tPart",
            "false\tconsole:4: the level \"Dungeon\" is added already; RemoveLevel(\"Dungeon\") takes it out"
        ]
    );
    assert_eq!(
        runtime.saves().levels.as_ref().unwrap().added,
        ["Dungeon".to_owned()]
    );
    runtime
        .run_command(
            &mut world,
            "game.LevelService:RemoveLevel('Dungeon')\n\
             print(workspace:FindFirstChild('Dungeon'))\n\
             print(pcall(function() game.LevelService:RemoveLevel('Dungeon') end))",
        )
        .unwrap();
    assert_eq!(
        printed(&mut runtime),
        [
            "nil",
            "false\tconsole:3: the level \"Dungeon\" isn't added; AddLevel(\"Dungeon\") adds it"
        ]
    );
    assert!(runtime.saves().levels.as_ref().unwrap().added.is_empty());
}

#[test]
fn an_unknown_level_is_an_error_naming_the_levels_and_changes_nothing() {
    let (mut world, mut runtime) = game();
    runtime
        .run_command(
            &mut world,
            "print(pcall(function() game.LevelService:LoadLevel('Moon') end))\n\
             print(pcall(function() game.LevelService:LoadLevel(5) end))",
        )
        .unwrap();
    assert_eq!(
        printed(&mut runtime),
        [
            "false\tconsole:1: there is no level called \"Moon\"; the levels are Castle, Dungeon, Meadow",
            "false\tconsole:2: LoadLevel needs a level's name, such as LevelService:LoadLevel(\"Dungeon\"), not a number"
        ]
    );
    assert_eq!(names(&world, world.workspace()), ["Grass", "Hud"]);
    // Without levels from the host, there are none.
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    runtime
        .run_command(
            &mut world,
            "print(game.LevelService.CurrentLevel == '')\n\
             print(pcall(function() game.LevelService:AddLevel('Moon') end))",
        )
        .unwrap();
    assert_eq!(
        printed(&mut runtime),
        [
            "true",
            "false\tconsole:2: there is no level called \"Moon\"; this game has no levels"
        ]
    );
    assert_eq!(runtime.saves().levels, None);
}

#[test]
fn a_slot_saved_in_another_level_with_one_added_goes_back_to_them() {
    let (mut world, mut runtime) = game();
    let folder = std::env::temp_dir().join(format!(
        "axcenity-script-levels-slot-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&folder);
    runtime.set_save_folder(Some(folder.clone()));
    runtime
        .run_command(
            &mut world,
            "local LevelService = game.LevelService\n\
             LevelService:LoadLevel('Castle')\n\
             LevelService:AddLevel('Dungeon')\n\
             workspace.Tower.Transparency = 0.5\n\
             game.SaveService:SaveSlot(1)",
        )
        .unwrap();
    runtime.fire_changes(&mut world);
    printed(&mut runtime);

    // A new game, in the start level.
    let (mut world, mut runtime) = game();
    runtime.set_save_folder(Some(folder));
    runtime
        .run_command(
            &mut world,
            "game.LevelService.LevelLoaded:Connect(function(name) print('loaded', name) end)\n\
             game.SaveService:LoadSlot(1)\n\
             print(game.LevelService.CurrentLevel, workspace.Tower.Transparency, workspace.Dungeon.Cell.ClassName)\n\
             print(workspace:FindFirstChild('Grass'))",
        )
        .unwrap();
    runtime.fire_changes(&mut world);
    assert_eq!(
        printed(&mut runtime),
        ["Castle\t0.5\tPart", "nil", "loaded\tCastle"]
    );
    assert_eq!(runtime.levels().added().collect::<Vec<_>>(), ["Dungeon"]);
}
