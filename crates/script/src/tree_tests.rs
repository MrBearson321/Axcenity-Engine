//! Tests for the Instance tree API (Phase 27): `Clone`, `Destroy`,
//! `WaitForChild`, `IsA`, finding and listing, and the tree events.

use axcenity_core::{ClassName, InstanceId, Value, World};

use super::*;

/// Adds a Script (or a LocalScript, for a `.client.lua` path) under
/// `parent` and starts it.
fn start_in(
    runtime: &mut ScriptRuntime,
    world: &mut World,
    parent: InstanceId,
    path: &str,
    source: &str,
) -> InstanceId {
    let class = if path.ends_with(".client.lua") {
        ClassName::LocalScript
    } else {
        ClassName::Script
    };
    let script = world.create(class).unwrap();
    world.set_parent(script, Some(parent)).unwrap();
    runtime.start_script(world, script, path, source);
    script
}

fn start(runtime: &mut ScriptRuntime, world: &mut World, source: &str) -> InstanceId {
    let service = world.server_script_service();
    start_in(runtime, world, service, "scripts/main.server.lua", source)
}

fn lines(runtime: &mut ScriptRuntime) -> Vec<String> {
    runtime
        .take_output()
        .into_iter()
        .map(|line| match line.kind {
            OutputKind::Print => line.text,
            OutputKind::Error => format!("error: {}", line.text),
            OutputKind::Warning => format!("warning: {}", line.text),
        })
        .collect()
}

/// A Model named `name` in the Workspace.
fn model(world: &mut World, name: &str) -> InstanceId {
    let model = world.create(ClassName::Model).unwrap();
    world.set_name(model, name).unwrap();
    world.set_parent(model, Some(world.workspace())).unwrap();
    model
}

/// Steps the runtime `seconds` at a time, `count` times.
fn step(runtime: &mut ScriptRuntime, world: &mut World, seconds: f64, count: usize) {
    for _ in 0..count {
        runtime.step(world, seconds);
    }
}

#[test]
fn a_cloned_model_runs_its_script_and_destroying_it_stops_the_script() {
    let mut world = World::new();
    let coin = model(&mut world, "Coin");
    let mut runtime = ScriptRuntime::new();
    start_in(
        &mut runtime,
        &mut world,
        coin,
        "scripts/Spin.server.lua",
        r#"
        while true do
            print("tick " .. script.Parent.Name)
            task.wait(1)
        end
        "#,
    );
    start(
        &mut runtime,
        &mut world,
        r#"
        local copy = workspace.Coin:Clone()
        print("parent", copy.Parent)
        copy.Name = "Copy"
        copy.Destroying:Connect(function()
            print("destroying " .. copy.Name)
        end)
        copy.Parent = workspace
        task.wait(2.5)
        copy:Destroy()
        copy:Destroy() -- twice is harmless
        "#,
    );
    assert_eq!(
        lines(&mut runtime),
        ["tick Coin", "parent\tnil", "tick Copy"]
    );
    step(&mut runtime, &mut world, 0.5, 5);
    assert_eq!(
        lines(&mut runtime),
        [
            "tick Coin",
            "tick Copy",
            "tick Coin",
            "tick Copy",
            "destroying Copy"
        ]
    );
    // The copy's script stopped with it; the original carries on.
    step(&mut runtime, &mut world, 0.5, 4);
    assert_eq!(lines(&mut runtime), ["tick Coin", "tick Coin"]);
    assert!(world.find_first_child(world.workspace(), "Copy").is_none());
}

#[test]
fn a_cloned_script_waits_until_it_is_somewhere_scripts_run() {
    let mut world = World::new();
    let template = world.create(ClassName::Model).unwrap();
    world.set_name(template, "Template").unwrap();
    world
        .set_parent(template, Some(world.replicated_storage()))
        .unwrap();
    let mut runtime = ScriptRuntime::new();
    start_in(
        &mut runtime,
        &mut world,
        template,
        "scripts/Hello.server.lua",
        r#"print("hello from " .. script.Parent:GetFullName())"#,
    );
    start(
        &mut runtime,
        &mut world,
        r#"
        local copy = game.ReplicatedStorage.Template:Clone()
        copy.Name = "Held"
        copy.Parent = game.ReplicatedStorage
        task.wait(1)
        copy.Parent = workspace
        "#,
    );
    assert_eq!(
        lines(&mut runtime),
        ["hello from ReplicatedStorage.Template"]
    );
    step(&mut runtime, &mut world, 1.0, 1);
    assert_eq!(lines(&mut runtime), ["hello from Workspace.Held"]);
}

/// Adds a Script inside `parent` whose code is in `objects/Coin/Spin.server.lua`
/// (a script inside an object, G2) and starts it.
fn start_object_script(
    runtime: &mut ScriptRuntime,
    world: &mut World,
    parent: InstanceId,
    source: &str,
) -> InstanceId {
    let script = world.create(ClassName::Script).unwrap();
    world.set_name(script, "Spin").unwrap();
    world
        .set_property(
            script,
            "SourceFile",
            Value::String("objects/Coin/Spin.server.lua".into()),
        )
        .unwrap();
    world.set_parent(script, Some(parent)).unwrap();
    runtime.start_script(world, script, "objects/Coin/Spin.server.lua", source);
    script
}

#[test]
fn scripts_inside_objects_run_per_copy_and_templates_wait_until_placed() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    let code = r#"print("spin " .. script.Parent:GetFullName())"#;
    let mut coins = Vec::new();
    for (name, place) in [
        ("A", world.workspace()),
        ("B", world.workspace()),
        ("Template", world.server_storage()),
    ] {
        let coin = world.create(ClassName::Model).unwrap();
        world.set_name(coin, name).unwrap();
        world.set_parent(coin, Some(place)).unwrap();
        coins.push(coin);
    }
    for &coin in &coins {
        start_object_script(&mut runtime, &mut world, coin, code);
    }
    // The template in ServerStorage doesn't run.
    assert_eq!(
        lines(&mut runtime),
        ["spin Workspace.A", "spin Workspace.B"]
    );

    start(
        &mut runtime,
        &mut world,
        r#"
        local copy = game.ServerStorage.Template:Clone()
        copy.Name = "C"
        copy.Parent = workspace
        task.wait(1)
        game.ServerStorage.Template.Parent = workspace
        "#,
    );
    assert_eq!(lines(&mut runtime), ["spin Workspace.C"]);
    // Moved where scripts run, the template itself starts.
    step(&mut runtime, &mut world, 1.0, 1);
    assert_eq!(lines(&mut runtime), ["spin Workspace.Template"]);
}

#[test]
fn a_script_that_destroys_itself_stops_after_its_current_run() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        r#"
        workspace.ChildAdded:Connect(function() print("never: handler") end)
        script:Destroy()
        print("still running")
        task.wait(1)
        print("never: after wait")
        "#,
    );
    let part = world.create(ClassName::Part).unwrap();
    world.set_parent(part, Some(world.workspace())).unwrap();
    step(&mut runtime, &mut world, 1.0, 2);
    assert_eq!(lines(&mut runtime), ["still running"]);
}

#[test]
fn wait_for_child_returns_a_late_child_and_nil_after_its_timeout() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        r#"
        task.delay(2, function()
            local part = Instance.new("Part")
            part.Name = "Late"
            part.Parent = workspace
        end)
        local found = workspace:WaitForChild("Late")
        print("found " .. found:GetFullName())
        local missing = workspace:WaitForChild("Never", 1)
        print("missing", missing)
        "#,
    );
    step(&mut runtime, &mut world, 0.5, 3);
    assert!(lines(&mut runtime).is_empty());
    step(&mut runtime, &mut world, 0.5, 2);
    assert_eq!(lines(&mut runtime), ["found Workspace.Late"]);
    step(&mut runtime, &mut world, 0.5, 1);
    assert!(lines(&mut runtime).is_empty());
    step(&mut runtime, &mut world, 0.5, 1);
    assert_eq!(lines(&mut runtime), ["missing\tnil"]);
    // No warning: both had a timeout or were found within 5 seconds.
}

#[test]
fn wait_for_child_without_a_timeout_warns_once_after_five_seconds() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        r#"
        local ok, err = pcall(function() return workspace:WaitForChild(5) end)
        print(err)
        workspace:WaitForChild("Ghost")
        "#,
    );
    assert_eq!(
        lines(&mut runtime),
        [
            "scripts/main.server.lua:2: WaitForChild needs a child's name, such as WaitForChild(\"Door\"), not a number"
        ]
    );
    step(&mut runtime, &mut world, 1.0, 4);
    assert!(lines(&mut runtime).is_empty());
    step(&mut runtime, &mut world, 1.0, 10);
    assert_eq!(
        lines(&mut runtime),
        [
            "warning: WaitForChild has waited 5 seconds for \"Ghost\" in Workspace, and keeps \
             waiting. Give it a timeout to stop, such as WaitForChild(\"Ghost\", 10)."
        ]
    );
}

#[test]
fn is_a_follows_the_class_hierarchy() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        r#"
        local part, mesh, frame = Instance.new("Part"), Instance.new("MeshPart"), Instance.new("Frame")
        print(part:IsA("BasePart"), mesh:IsA("BasePart"), frame:IsA("BasePart"))
        print(part:IsA("Part"), mesh:IsA("Part"), frame:IsA("GuiObject"), frame:IsA("Instance"))
        print(script:IsA("LuaSourceContainer"), script:IsA("LocalScript"))
        "#,
    );
    assert_eq!(
        lines(&mut runtime),
        [
            "true\ttrue\tfalse",
            "true\tfalse\ttrue\ttrue",
            "true\tfalse"
        ]
    );
}

#[test]
fn a_destroyed_instance_has_its_parent_locked_and_services_cant_be_destroyed() {
    let mut world = World::new();
    let block = world.create(ClassName::Part).unwrap();
    world.set_name(block, "Block").unwrap();
    world.set_parent(block, Some(world.workspace())).unwrap();
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        r#"
        local part = Instance.new("Part")
        part.Parent = workspace
        part:Destroy()
        print(pcall(function() part.Parent = workspace end))
        print(pcall(function() workspace:Destroy() end))
        print(pcall(function() workspace:Clone() end))
        print(pcall(function() workspace.Block.Parent = part end))
        "#,
    );
    let prefix = "false\tscripts/main.server.lua";
    assert_eq!(
        lines(&mut runtime),
        [
            format!(
                "{prefix}:5: can't set Parent: this Instance has been destroyed, so its Parent is locked"
            ),
            format!("{prefix}:6: Workspace is a service, so it can't be destroyed"),
            format!("{prefix}:7: Workspace is a service, so it can't be cloned"),
            format!("{prefix}:8: can't set Parent: the new Parent has been destroyed"),
        ]
    );
    // The Workspace is untouched, Block and all.
    assert_eq!(world.get(world.workspace()).unwrap().children(), [block]);
}

#[test]
fn finding_and_listing_instances() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        r#"
        local house = Instance.new("Model")
        house.Name = "House"
        local door = Instance.new("MeshPart")
        door.Name = "Door"
        door.Parent = house
        local knob = Instance.new("Part")
        knob.Name = "Knob"
        knob.Parent = door
        house.Parent = workspace

        print(#house:GetChildren(), #house:GetDescendants(), house:GetDescendants()[2] == knob)
        print(house:FindFirstChildOfClass("MeshPart"), house:FindFirstChildOfClass("Part"))
        print(house:FindFirstChildWhichIsA("BasePart"), house:FindFirstChildWhichIsA("Part", true))
        print(knob:FindFirstAncestor("House"), knob:FindFirstAncestorOfClass("Model"))
        print(knob:FindFirstAncestorWhichIsA("BasePart"), knob:FindFirstAncestorOfClass("Workspace"))
        print(knob:IsDescendantOf(workspace), workspace:IsAncestorOf(knob), knob:IsAncestorOf(house))
        print(knob:GetFullName())
        house:ClearAllChildren()
        print(#house:GetChildren(), #workspace:GetChildren())
        "#,
    );
    assert_eq!(
        lines(&mut runtime),
        [
            "1\t2\ttrue",
            "Door\tnil",
            "Door\tKnob",
            "House\tHouse",
            "Door\tWorkspace",
            "true\ttrue\tfalse",
            "Workspace.House.Door.Knob",
            "0\t1",
        ]
    );
}

#[test]
fn tree_events_fire_for_descendants_and_ancestry() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    start(
        &mut runtime,
        &mut world,
        r#"
        local folder = Instance.new("Folder")
        folder.Parent = workspace
        workspace.DescendantAdded:Connect(function(d) print("added", d.Name) end)
        workspace.DescendantRemoving:Connect(function(d) print("removing", d.Name) end)

        local box = Instance.new("Model")
        box.Name = "Box"
        local lid = Instance.new("Part")
        lid.Name = "Lid"
        lid.Parent = box
        lid.AncestryChanged:Connect(function(moved, parent)
            print("ancestry", moved.Name, parent)
        end)
        lid.Destroying:Connect(function() print("destroying", lid.Name) end)

        box.Parent = folder
        task.wait()
        box.Parent = nil
        task.wait()
        box.Parent = workspace
        task.wait()
        box:Destroy()
        "#,
    );
    step(&mut runtime, &mut world, 0.0, 3);
    assert_eq!(
        lines(&mut runtime),
        [
            "added\tBox",
            "added\tLid",
            "ancestry\tBox\tFolder",
            "removing\tBox",
            "removing\tLid",
            "ancestry\tBox\tnil",
            "added\tBox",
            "added\tLid",
            "ancestry\tBox\tWorkspace",
            "destroying\tLid",
            "removing\tBox",
            "removing\tLid",
            "ancestry\tBox\tnil",
        ]
    );
}

#[test]
fn a_cloned_module_can_be_required_at_once() {
    let mut world = World::new();
    let module = world.create(ClassName::ModuleScript).unwrap();
    world.set_name(module, "Config").unwrap();
    world
        .set_parent(module, Some(world.replicated_storage()))
        .unwrap();
    let mut runtime = ScriptRuntime::new();
    runtime.start_script(
        &mut world,
        module,
        "scripts/Config.module.lua",
        "return { speed = 4 }",
    );
    start(
        &mut runtime,
        &mut world,
        r#"
        local copy = game.ReplicatedStorage.Config:Clone()
        print(require(copy).speed, require(copy) ~= require(game.ReplicatedStorage.Config))
        "#,
    );
    assert_eq!(lines(&mut runtime), ["4\ttrue"]);
}
