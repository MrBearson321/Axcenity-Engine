use std::path::PathBuf;

use axcenity_core::{ClassName, InstanceId, Value, Vector3, World};

use super::*;

fn save_folder(test: &str) -> PathBuf {
    let folder = std::env::temp_dir().join(format!(
        "axcenity-script-saves-{test}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&folder);
    folder
}

fn add(world: &mut World, class: ClassName, parent: InstanceId, name: &str) -> InstanceId {
    let id = world.create(class).unwrap();
    world.set_name(id, name).unwrap();
    world.set_parent(id, Some(parent)).unwrap();
    id
}

/// A crate, a checkpoint "Cave" at (20, 0, 10), and a Hero with a 5-stud
/// Body and a ThirdPersonController.
fn level() -> (World, InstanceId, InstanceId) {
    let mut world = World::new();
    let workspace = world.workspace();
    let crate_part = add(&mut world, ClassName::Part, workspace, "Crate");
    let cave = add(&mut world, ClassName::Checkpoint, workspace, "Cave");
    world
        .set_property(
            cave,
            "Position",
            Value::Vector3(Vector3::new(20.0, 0.0, 10.0)),
        )
        .unwrap();
    let hero = add(&mut world, ClassName::Model, workspace, "Hero");
    let body = add(&mut world, ClassName::Part, hero, "Body");
    world
        .set_property(body, "Size", Value::Vector3(Vector3::new(2.0, 5.0, 1.0)))
        .unwrap();
    world
        .set_property(
            body,
            "Position",
            Value::Vector3(Vector3::new(0.0, 2.5, 0.0)),
        )
        .unwrap();
    add(
        &mut world,
        ClassName::ThirdPersonController,
        hero,
        "ThirdPersonController",
    );
    (world, crate_part, body)
}

fn run(runtime: &mut ScriptRuntime, world: &mut World, class: ClassName, path: &str, source: &str) {
    let parent = match class {
        ClassName::LocalScript => world.starter_player_scripts(),
        _ => world.server_script_service(),
    };
    let script = add(world, class, parent, path);
    runtime.start_script(world, script, path, source);
}

fn output(runtime: &mut ScriptRuntime) -> Vec<String> {
    runtime
        .take_output()
        .into_iter()
        .map(|line| match line.kind {
            OutputKind::Print => line.text,
            OutputKind::Error => format!("error: {}", line.text),
        })
        .collect()
}

fn position(world: &World, id: InstanceId) -> Vector3 {
    world
        .get(id)
        .unwrap()
        .property("Position")
        .and_then(Value::as_vector3)
        .unwrap()
}

#[test]
fn lua_saves_a_slot_loads_it_in_a_new_game_and_restores_its_checkpoint() {
    let folder = save_folder("round-trip");
    let (mut world, crate_part, _) = level();
    let mut runtime = ScriptRuntime::new();
    runtime.set_save_folder(Some(folder.clone()));
    run(
        &mut runtime,
        &mut world,
        ClassName::Script,
        "scripts/save.server.lua",
        r#"
local SaveService = game:GetService("SaveService")
SaveService:SetPlayerData("Coins", 3)
SaveService:SetPlayerData("Spawn", Vector3.new(1, 2, 3))
SaveService:SetSetting("Volume", 0.5)
SaveService:SetCheckpoint(workspace.Cave)
workspace.Crate.Position = Vector3.new(9, 1, 3)
SaveService:SetPlayerData("Coins", 7)
print(SaveService:HasSlot(1))
SaveService:SaveSlot(1)
print(SaveService:HasSlot(1), SaveService:GetCheckpoint())
"#,
    );
    // LocalScripts share the same data.
    run(
        &mut runtime,
        &mut world,
        ClassName::LocalScript,
        "scripts/read.client.lua",
        r#"print(game.SaveService:GetPlayerData("Coins"), game.SaveService:GetSetting("Missing"))"#,
    );
    assert_eq!(output(&mut runtime), ["false", "true\tCave", "7\tnil"]);
    assert!(folder.join("slot-1.json").is_file());
    assert_eq!(position(&world, crate_part), Vector3::new(9.0, 1.0, 3.0));

    // A new run of the game loads it.
    let (mut fresh, fresh_crate, body) = level();
    let mut runtime = ScriptRuntime::new();
    runtime.set_save_folder(Some(folder));
    run(
        &mut runtime,
        &mut fresh,
        ClassName::Script,
        "scripts/load.server.lua",
        r#"
local SaveService = game:GetService("SaveService")
SaveService:LoadSlot(1)
print(SaveService:GetPlayerData("Coins"), SaveService:GetPlayerData("Spawn"), SaveService:GetSetting("Volume"))
print(SaveService:GetCheckpoint(), workspace.Crate.Position)
workspace.Hero.Body.Position = Vector3.new(-50, -30, 0)
SaveService:RestoreCheckpoint()
print(SaveService:GetPlayerData("Coins"), workspace.Crate.Position, workspace.Hero.Body.Position)
"#,
    );
    assert_eq!(
        output(&mut runtime),
        [
            "7\t1, 2, 3\t0.5",
            "Cave\t9, 1, 3",
            // At the checkpoint, the crate hadn't moved and there were 3 coins.
            "3\t0, 0, 0\t20, 2.5, 10",
        ]
    );
    assert_eq!(position(&fresh, fresh_crate), Vector3::new(0.0, 0.0, 0.0));
    assert_eq!(position(&fresh, body), Vector3::new(20.0, 2.5, 10.0));
    assert_eq!(runtime.saves().settings["Volume"], Value::Number(0.5));
}

#[test]
fn loading_a_missing_slot_is_an_error_at_its_line_and_changes_nothing() {
    let folder = save_folder("missing");
    let (mut world, crate_part, _) = level();
    world
        .set_property(
            crate_part,
            "Position",
            Value::Vector3(Vector3::new(4.0, 4.0, 4.0)),
        )
        .unwrap();
    let mut runtime = ScriptRuntime::new();
    runtime.set_save_folder(Some(folder));
    run(
        &mut runtime,
        &mut world,
        ClassName::Script,
        "scripts/load.server.lua",
        r#"
local SaveService = game:GetService("SaveService")
SaveService:SetPlayerData("Coins", 2)
print(pcall(function() SaveService:LoadSlot(2) end))
print(SaveService:GetPlayerData("Coins"))
SaveService:LoadSlot(3)
"#,
    );
    let lines = output(&mut runtime);
    assert!(
        lines[0].starts_with("false\t") && lines[0].contains("save slot 2 doesn't exist yet"),
        "{lines:?}"
    );
    assert_eq!(lines[1], "2");
    assert!(
        lines[2].starts_with("error: scripts/load.server.lua:6:")
            && lines[2].contains("save slot 3 doesn't exist yet"),
        "{lines:?}"
    );
    assert_eq!(position(&world, crate_part), Vector3::new(4.0, 4.0, 4.0));
}

#[test]
fn save_service_explains_bad_arguments_and_a_game_without_a_save_folder() {
    let (mut world, _, _) = level();
    let mut runtime = ScriptRuntime::new();
    run(
        &mut runtime,
        &mut world,
        ClassName::Script,
        "scripts/bad.server.lua",
        r#"
local SaveService = game:GetService("SaveService")
print(pcall(function() SaveService:SaveSlot(1) end))
print(pcall(function() SaveService:SaveSlot(0) end))
print(pcall(function() SaveService:SaveSlot("one") end))
print(pcall(function() SaveService:SetPlayerData("Bag", {}) end))
print(pcall(function() SaveService:SetCheckpoint(workspace.Crate) end))
print(pcall(function() SaveService:RestoreCheckpoint() end))
SaveService:SetPlayerData("Coins", 1)
SaveService:SetPlayerData("Coins", nil)
print(SaveService:GetPlayerData("Coins"), tostring(SaveService))
"#,
    );
    let lines = output(&mut runtime);
    let expected = [
        "saving isn't set up in this game",
        "save slots are numbered 1 to 999, so 0 isn't one",
        "SaveSlot needs a slot number, such as SaveService:SaveSlot(1), not a string",
        "SetPlayerData keeps a boolean, number, string",
        "Part is not a Checkpoint",
        "no checkpoint is active",
    ];
    for (line, expected) in lines.iter().zip(expected) {
        assert!(
            line.starts_with("false\t") && line.contains(expected),
            "{line:?} should say {expected:?}"
        );
    }
    assert_eq!(lines[6], "nil\tSaveService");
}
