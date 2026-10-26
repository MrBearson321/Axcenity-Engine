//! Tests for change events (`Changed`, `GetPropertyChangedSignal`,
//! `ChildAdded`, `ChildRemoved`) and `RunService.Heartbeat`.

use axcenity_core::{ClassName, Color3, InstanceId, Value, Vector3, World};

use super::*;

/// A world with one Part named `Block` in the Workspace.
fn world_with_block() -> (World, InstanceId) {
    let mut world = World::new();
    let block = world.create(ClassName::Part).unwrap();
    world.set_name(block, "Block").unwrap();
    world.set_parent(block, Some(world.workspace())).unwrap();
    (world, block)
}

/// Adds a Script (or a LocalScript, for a `.client.lua` path) and starts it.
fn start(runtime: &mut ScriptRuntime, world: &mut World, path: &str, source: &str) {
    let (class, service) = if path.ends_with(".client.lua") {
        (ClassName::LocalScript, world.starter_player_scripts())
    } else {
        (ClassName::Script, world.server_script_service())
    };
    let script = world.create(class).unwrap();
    world.set_parent(script, Some(service)).unwrap();
    runtime.start_script(world, script, path, source);
}

fn lines(runtime: &mut ScriptRuntime) -> Vec<String> {
    runtime
        .take_output()
        .into_iter()
        .map(|line| match line.kind {
            OutputKind::Print => line.text,
            OutputKind::Error => format!("error: {}", line.text),
        })
        .collect()
}

#[test]
fn changed_fires_with_the_property_name_when_a_script_changes_one() {
    let (mut world, _) = world_with_block();
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        "scripts/watch.server.lua",
        r#"
        local block = workspace.Block
        block.Changed:Connect(function(property)
            print("changed", property, tostring(block[property]))
        end)
        block.Color = Color3.new(1, 0, 0)
        block.Anchored = false -- already false: not a change
        block.Name = "Pad"
        print("set")
        "#,
    );
    // Handlers run after the script, not in the middle of it.
    assert_eq!(
        lines(&mut runtime),
        ["set", "changed\tColor\t1, 0, 0", "changed\tName\tPad"]
    );
}

#[test]
fn a_property_changed_signal_fires_only_for_its_property_with_no_arguments() {
    let (mut world, _) = world_with_block();
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        "scripts/watch.server.lua",
        r##"
        local block = workspace.Block
        block:GetPropertyChangedSignal("Position"):Connect(function(...)
            print("moved", select("#", ...), block.Position)
        end)
        print(block:GetPropertyChangedSignal("Position") == block:GetPropertyChangedSignal("Position"))
        print(tostring(block:GetPropertyChangedSignal("Position")))
        block.Size = Vector3.new(1, 1, 1)
        block.Position = Vector3.new(0, 5, 0)
        print(pcall(block.GetPropertyChangedSignal, block, "Speed"))
        "##,
    );
    let output = lines(&mut runtime);
    assert_eq!(output[0], "true");
    assert_eq!(output[1], "Signal Position changed");
    assert!(
        output[2].ends_with(
            "Speed is not a property of Part; choose from Name, Parent, Anchored, CanCollide, \
             CastShadow, Color, Emission, EmissiveColor, Material, MaterialVariant, Metalness, Orientation, \
             Position, Roughness, Shape, Size, TileSize, Transparency, WindAffected"
        ),
        "{}",
        output[2]
    );
    assert_eq!(output[3], "moved\t0\t0, 5, 0");
    assert_eq!(output.len(), 4, "{output:?}");
}

/// Physics and the editor change the World between script calls. Those
/// changes fire at the runtime's next call.
#[test]
fn changes_made_outside_scripts_fire_changed_at_the_next_call() {
    let (mut world, block) = world_with_block();
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        "scripts/watch.server.lua",
        r#"
        workspace.Block.Changed:Connect(function(property)
            print("changed", property)
        end)
        "#,
    );
    // As physics does in frame step 3.
    world
        .set_property(
            block,
            "Position",
            Value::Vector3(Vector3::new(0.0, -1.0, 0.0)),
        )
        .unwrap();
    assert!(lines(&mut runtime).is_empty());
    runtime.fire_touched(&mut world, &[]);
    assert_eq!(lines(&mut runtime), ["changed\tPosition"]);

    // As the editor does between frames, during Play.
    world
        .set_property(block, "Color", Value::Color3(Color3::new(0.0, 1.0, 0.0)))
        .unwrap();
    runtime.step(&mut world, 1.0 / 60.0);
    assert_eq!(lines(&mut runtime), ["changed\tColor"]);
}

#[test]
fn a_handler_does_not_hear_about_changes_from_before_it_was_connected() {
    let (mut world, _) = world_with_block();
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        "scripts/watch.server.lua",
        r#"
        local block = workspace.Block
        block.Name = "Early"
        block.Changed:Connect(function(property)
            print("changed", property, block.Name)
        end)
        block.Name = "Late"
        "#,
    );
    assert_eq!(lines(&mut runtime), ["changed\tName\tLate"]);
}

#[test]
fn child_added_and_child_removed_fire_on_the_parent_with_the_child() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        "scripts/children.server.lua",
        r#"
        workspace.ChildAdded:Connect(function(child)
            print("added", child.Name, child.Parent == workspace)
        end)
        workspace.ChildRemoved:Connect(function(child)
            print("removed", child.Name, child.Parent)
        end)
        local part = Instance.new("Part")
        part.Name = "Crate"
        part.Parent = workspace
        task.wait()
        part.Parent = nil
        "#,
    );
    assert_eq!(lines(&mut runtime), ["added\tCrate\ttrue"]);
    runtime.step(&mut world, 1.0 / 60.0);
    assert_eq!(lines(&mut runtime), ["removed\tCrate\tnil"]);
}

#[test]
fn players_child_added_fires_when_the_player_joins() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        "scripts/welcome.server.lua",
        r#"
        game.Players.ChildAdded:Connect(function(player)
            print("welcome", player.Name, player.ClassName)
        end)
        "#,
    );
    // A game with no LocalScripts: the player joins at the first frame.
    runtime.step(&mut world, 1.0 / 60.0);
    assert_eq!(lines(&mut runtime), ["welcome\tPlayer\tPlayer"]);
}

#[test]
fn change_events_fire_in_scripts_and_local_scripts() {
    let (mut world, _) = world_with_block();
    let mut runtime = ScriptRuntime::new();
    let watch = r#"
        workspace.Block.Changed:Connect(function(property)
            print(script.ClassName, "saw", property)
        end)
        "#;
    start(&mut runtime, &mut world, "scripts/watch.server.lua", watch);
    start(&mut runtime, &mut world, "scripts/watch.client.lua", watch);
    start(
        &mut runtime,
        &mut world,
        "scripts/change.client.lua",
        "workspace.Block.Anchored = true",
    );
    assert_eq!(
        lines(&mut runtime),
        ["Script\tsaw\tAnchored", "LocalScript\tsaw\tAnchored"]
    );
}

#[test]
fn a_disconnected_change_handler_stops_running() {
    let (mut world, _) = world_with_block();
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        "scripts/once.server.lua",
        r#"
        local connection
        connection = workspace.Block.Changed:Connect(function(property)
            print("changed", property)
            connection:Disconnect()
        end)
        workspace.Block.Anchored = true
        task.wait()
        workspace.Block.Anchored = false
        "#,
    );
    runtime.step(&mut world, 1.0 / 60.0);
    assert_eq!(lines(&mut runtime), ["changed\tAnchored"]);
}

#[test]
fn a_handler_that_keeps_changing_what_it_listens_to_is_reported_once_and_does_not_hang() {
    let (mut world, block) = world_with_block();
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        "scripts/loop.server.lua",
        r#"
        local block = workspace.Block
        block:GetPropertyChangedSignal("Position"):Connect(function()
            block.Position = block.Position + Vector3.new(1, 0, 0)
        end)
        block.Position = Vector3.new(1, 0, 0)
        "#,
    );
    let output = lines(&mut runtime);
    assert_eq!(output.len(), 1, "{output:?}");
    assert!(
        output[0].contains("kept changing things that fire more of them"),
        "{}",
        output[0]
    );
    let x = |world: &World| match world.get(block).unwrap().property("Position") {
        Some(Value::Vector3(position)) => position.x,
        other => panic!("{other:?}"),
    };
    // The first set, then one more per round.
    assert_eq!(x(&world), 1.0 + MAX_CHANGE_ROUNDS as f32);
    // It carries on next frame, without saying so again.
    runtime.step(&mut world, 1.0 / 60.0);
    assert!(x(&world) > 1.0 + MAX_CHANGE_ROUNDS as f32);
    assert!(lines(&mut runtime).is_empty());
}

#[test]
fn heartbeat_fires_once_a_frame_with_the_frame_time_on_both_sides() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    let source = r#"
        local RunService = game:GetService("RunService")
        print(RunService, game.RunService)
        RunService.Heartbeat:Connect(function(deltaTime)
            print(script.ClassName, deltaTime)
        end)
        "#;
    start(&mut runtime, &mut world, "scripts/tick.server.lua", source);
    start(&mut runtime, &mut world, "scripts/tick.client.lua", source);
    assert_eq!(
        lines(&mut runtime),
        ["RunService	RunService", "RunService	RunService"]
    );

    runtime.heartbeat(&mut world, 0.25);
    runtime.heartbeat(&mut world, 0.5);
    assert_eq!(
        lines(&mut runtime),
        [
            "Script\t0.25",
            "LocalScript\t0.25",
            "Script\t0.5",
            "LocalScript\t0.5"
        ]
    );
}

#[test]
fn assigning_to_a_change_event_explains_how_to_connect() {
    let (mut world, _) = world_with_block();
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        "scripts/oops.server.lua",
        "workspace.Block.Changed = print",
    );
    assert_eq!(
        lines(&mut runtime),
        [
            "error: scripts/oops.server.lua:1: Part.Changed is an event and can't be set; \
             use Changed:Connect(function) to react to it"
        ]
    );
}

/// The widgets' `ValueChanged` fires with the new value however `Value`
/// changes: from a script, or from the pointer (the host sets it between
/// script calls).
#[test]
fn value_changed_fires_with_the_new_value_from_scripts_and_the_pointer() {
    let mut world = World::new();
    let gui = world.create(ClassName::ScreenGui).unwrap();
    world.set_name(gui, "Settings").unwrap();
    world.set_parent(gui, Some(world.workspace())).unwrap();
    let mut widget = |class, name: &str| {
        let id = world.create(class).unwrap();
        world.set_name(id, name).unwrap();
        world.set_parent(id, Some(gui)).unwrap();
        id
    };
    let volume = widget(ClassName::Slider, "Volume");
    let fullscreen = widget(ClassName::Toggle, "Fullscreen");
    let quality = widget(ClassName::Dropdown, "Quality");
    widget(ClassName::ProgressBar, "Loading");
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        "scripts/settings.client.lua",
        r#"
        local settings = workspace.Settings
        for _, name in { "Volume", "Fullscreen", "Quality", "Loading" } do
            settings[name].ValueChanged:Connect(function(value)
                print(name, value, type(value))
            end)
        end
        settings.Volume.Value = 75
        settings.Volume.Value = 75 -- the same: not a change
        settings.Loading.Value = 10
        print(settings.Volume.Value, settings.Quality.Options)
        "#,
    );
    assert_eq!(
        lines(&mut runtime),
        [
            "75\tLow\nMedium\nHigh",
            "Volume\t75\tnumber",
            "Loading\t10\tnumber"
        ]
    );
    // As the player does when a Toggle is clicked or an option picked.
    world
        .set_property(fullscreen, "Value", Value::Bool(true))
        .unwrap();
    world
        .set_property(quality, "Value", Value::String("High".into()))
        .unwrap();
    world
        .set_property(volume, "Value", Value::Number(30.0))
        .unwrap();
    runtime.step(&mut world, 1.0 / 60.0);
    assert_eq!(
        lines(&mut runtime),
        [
            "Fullscreen\ttrue\tboolean",
            "Quality\tHigh\tstring",
            "Volume\t30\tnumber"
        ]
    );
}
