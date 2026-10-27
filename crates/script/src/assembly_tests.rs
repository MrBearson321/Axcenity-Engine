//! Tests for Phase 35: welds, assemblies and attachments from Lua.

use axcenity_core::{ClassName, InstanceId, Value, Vector3, World};
use axcenity_physics::Physics;

use super::*;

const STEP: f64 = 1.0 / 60.0;

/// Runs `source` as a new server script and returns what it printed, with
/// errors marked.
fn run(world: &mut World, source: &str) -> Vec<String> {
    let mut runtime = ScriptRuntime::new();
    let script = world.create(ClassName::Script).unwrap();
    world
        .set_parent(script, Some(world.server_script_service()))
        .unwrap();
    runtime.start_script(world, script, "scripts/test.server.lua", source);
    runtime
        .take_output()
        .into_iter()
        .map(|line| match line.kind {
            OutputKind::Error => format!("error: {}", line.text),
            _ => line.text.replace('\t', " "),
        })
        .collect()
}

fn part(world: &mut World, name: &str, position: Vector3, size: Vector3) -> InstanceId {
    let id = world.create(ClassName::Part).unwrap();
    world.set_name(id, name).unwrap();
    world
        .set_property(id, "Position", Value::Vector3(position))
        .unwrap();
    world
        .set_property(id, "Size", Value::Vector3(size))
        .unwrap();
    world.set_parent(id, Some(world.workspace())).unwrap();
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

fn steps(physics: &mut Physics, world: &mut World, count: u32) {
    for _ in 0..count {
        physics.step(world, STEP);
    }
}

#[test]
fn welds_made_in_lua_join_parts_into_one_assembly() {
    let mut world = World::new();
    part(
        &mut world,
        "Big",
        Vector3::new(0.0, 50.0, 0.0),
        Vector3::new(4.0, 1.0, 2.0),
    );
    part(
        &mut world,
        "Small",
        Vector3::new(0.0, 51.0, 0.0),
        Vector3::new(1.0, 1.0, 1.0),
    );
    part(
        &mut world,
        "Far",
        Vector3::new(9.0, 50.0, 0.0),
        Vector3::new(1.0, 1.0, 1.0),
    );
    let lines = run(
        &mut world,
        r#"
        local big, small, far = workspace.Big, workspace.Small, workspace.Far
        print(#small:GetConnectedParts(), small:GetRootPart() == small, small.AssemblyMass)
        local weld = Instance.new("WeldConstraint")
        weld.Part0 = big
        weld.Part1 = small
        weld.Parent = small
        local connected = small:GetConnectedParts()
        print(#connected, connected[1] == small, connected[2] == big)
        print(small:GetRootPart() == big, small.AssemblyMass, small:GetMass())
        local second = Instance.new("WeldConstraint")
        second.Part0 = small
        second.Part1 = far
        second.Parent = far
        print(#big:GetConnectedParts(), #big:GetConnectedParts(true))
        second.Enabled = false
        print(#big:GetConnectedParts(true), weld:IsA("WeldConstraint"))
        "#,
    );
    assert_eq!(
        lines,
        ["1 true 1", "2 true true", "true 9 1", "2 3", "2 true"]
    );
}

#[test]
fn an_impulse_on_a_welded_part_pushes_its_whole_assembly() {
    let mut world = World::new();
    let a = part(
        &mut world,
        "A",
        Vector3::new(0.0, 500.0, 0.0),
        Vector3::new(1.0, 1.0, 1.0),
    );
    let b = part(
        &mut world,
        "B",
        Vector3::new(1.0, 500.0, 0.0),
        Vector3::new(1.0, 1.0, 1.0),
    );
    let lines = run(
        &mut world,
        r#"
        local weld = Instance.new("WeldConstraint")
        weld.Part0 = workspace.A
        weld.Part1 = workspace.B
        weld.Parent = workspace.B
        workspace.A:ApplyImpulse(Vector3.new(4, 0, 0))
        print(workspace.A.AssemblyLinearVelocity.X)
        "#,
    );
    // Two 1-mass parts: half the change one part alone would get.
    assert_eq!(lines, ["2"]);
    let mut physics = Physics::new();
    steps(&mut physics, &mut world, 1);
    for id in [a, b] {
        let velocity = world
            .get(id)
            .unwrap()
            .property("AssemblyLinearVelocity")
            .and_then(Value::as_vector3)
            .unwrap();
        assert!((velocity.x - 2.0).abs() < 1e-3, "{velocity:?}");
    }
}

#[test]
fn disabling_a_weld_from_lua_drops_the_part_and_enabling_it_holds_it_where_it_is() {
    let mut world = World::new();
    let beam = part(
        &mut world,
        "Beam",
        Vector3::new(0.0, 20.0, 0.0),
        Vector3::new(8.0, 1.0, 2.0),
    );
    world
        .set_property(beam, "Anchored", Value::Bool(true))
        .unwrap();
    let block = part(
        &mut world,
        "Block",
        Vector3::new(0.0, 18.5, 0.0),
        Vector3::new(2.0, 2.0, 2.0),
    );
    let lines = run(
        &mut world,
        r#"
        local weld = Instance.new("WeldConstraint")
        weld.Name = "Hold"
        weld.Part0 = workspace.Beam
        weld.Part1 = workspace.Block
        weld.Parent = workspace.Block
        "#,
    );
    assert!(lines.is_empty(), "{lines:?}");
    let mut physics = Physics::new();
    steps(&mut physics, &mut world, 30);
    assert_eq!(position(&world, block), Vector3::new(0.0, 18.5, 0.0));

    run(&mut world, "workspace.Block.Hold.Enabled = false");
    steps(&mut physics, &mut world, 20);
    let dropped = position(&world, block);
    assert!(dropped.y < 16.0, "{dropped:?}");

    run(&mut world, "workspace.Block.Hold.Enabled = true");
    steps(&mut physics, &mut world, 60);
    let held = position(&world, block);
    assert!((held.y - dropped.y).abs() < 0.05, "{held:?} vs {dropped:?}");
    steps(&mut physics, &mut world, 30);
    assert_eq!(position(&world, block), held);
}

#[test]
fn an_attachment_is_placed_on_its_part_and_follows_it() {
    let mut world = World::new();
    part(
        &mut world,
        "Arm",
        Vector3::new(10.0, 5.0, 0.0),
        Vector3::new(4.0, 1.0, 1.0),
    );
    let lines = run(
        &mut world,
        r#"
        local arm = workspace.Arm
        arm.Anchored = true
        arm.Orientation = Vector3.new(0, 90, 0)
        local tip = Instance.new("Attachment")
        tip.Name = "Tip"
        tip.Position = Vector3.new(2, 0, 0)
        tip.Parent = arm
        local function show(v) return string.format("%.2f %.2f %.2f", v.X, v.Y, v.Z) end
        -- Turned 90 degrees left, the arm's +X points along -Z.
        print(show(tip.WorldPosition), show(tip.CFrame.Position), show(tip.WorldAxis))
        print(show(tip.Axis), show(tip.SecondaryAxis))
        arm.Position = Vector3.new(0, 5, 0)
        print(show(tip.WorldPosition), show(tip.Position))
        tip.WorldPosition = Vector3.new(0, 6, 0)
        print(show(tip.Position))
        tip.WorldCFrame = CFrame.new(1, 5, 0)
        print(show(tip.Position), show(tip.WorldOrientation), show(tip.Orientation))
        tip.Axis = Vector3.new(1, 0, 0)
        print(show(tip.Axis), show(tip.Orientation))
        local ok, message = pcall(function() tip.WorldAxis = Vector3.new(1, 0, 0) end)
        print(ok, string.find(message, "read%-only") ~= nil)
        "#,
    );
    assert_eq!(
        lines,
        [
            "10.00 5.00 -2.00 2.00 0.00 0.00 0.00 0.00 -1.00",
            "1.00 0.00 0.00 0.00 1.00 0.00",
            "0.00 5.00 -2.00 2.00 0.00 0.00",
            "0.00 1.00 0.00",
            "0.00 0.00 1.00 0.00 0.00 0.00 0.00 -90.00 0.00",
            "1.00 0.00 0.00 0.00 0.00 0.00",
            "false true",
        ]
    );
}

#[test]
fn an_attachment_keeps_its_offset_while_its_part_falls() {
    let mut world = World::new();
    let crate_ = part(
        &mut world,
        "Crate",
        Vector3::new(0.0, 500.0, 0.0),
        Vector3::new(2.0, 2.0, 2.0),
    );
    run(
        &mut world,
        r#"
        local handle = Instance.new("Attachment")
        handle.Name = "Handle"
        handle.Position = Vector3.new(0, 1, 0)
        handle.Parent = workspace.Crate
        "#,
    );
    let mut physics = Physics::new();
    steps(&mut physics, &mut world, 20);
    let below = position(&world, crate_);
    assert!(below.y < 499.0);
    let lines = run(
        &mut world,
        r#"
        local handle = workspace.Crate.Handle
        print(handle.Position, (handle.WorldPosition - workspace.Crate.Position).Magnitude)
        "#,
    );
    assert_eq!(lines.len(), 1);
    assert!(lines[0].starts_with("0, 1, 0 1"), "{lines:?}");
}

#[test]
fn pivoting_a_model_moves_its_parts_and_their_attachments_once() {
    let mut world = World::new();
    let lines = run(
        &mut world,
        r#"
        local model = Instance.new("Model")
        model.Parent = workspace
        local block = Instance.new("Part")
        block.Position = Vector3.new(0, 1, 0)
        block.Parent = model
        local spot = Instance.new("Attachment")
        spot.Position = Vector3.new(0, 1, 0)
        spot.Parent = block
        model:PivotTo(model:GetPivot() + Vector3.new(5, 0, 0))
        print(block.Position, spot.Position, spot.WorldPosition)
        "#,
    );
    assert_eq!(lines, ["5, 1, 0 0, 1, 0 5, 2, 0"]);
}
