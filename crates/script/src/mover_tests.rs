//! Tests for Phase 37: movers from Lua.

use axcenity_core::{ClassName, World};
use axcenity_physics::Physics;

use super::*;

/// Runs `source` as a new server script with `runtime` and returns what it
/// printed, with errors marked.
fn run(runtime: &mut ScriptRuntime, world: &mut World, source: &str) -> Vec<String> {
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

#[test]
fn a_vector_force_of_its_weight_set_from_lua_makes_a_part_hover() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    let output = run(
        &mut runtime,
        &mut world,
        r#"
        local block = Instance.new("Part")
        block.Size = Vector3.new(2, 2, 2)
        block.Position = Vector3.new(0, 10, 0)
        block.Parent = workspace
        local middle = Instance.new("Attachment")
        middle.Parent = block
        local lift = Instance.new("VectorForce")
        lift.Attachment0 = middle
        lift.RelativeTo = Enum.ActuatorRelativeTo.World
        lift.Force = Vector3.new(0, block:GetMass() * workspace.Gravity, 0)
        lift.Parent = block
        print(lift:IsA("Constraint"), lift.RelativeTo, lift.ApplyAtCenterOfMass)
        "#,
    );
    assert_eq!(output, ["true Enum.ActuatorRelativeTo.World false"]);
    let mut physics = Physics::new();
    for _ in 0..120 {
        physics.step(&mut world, 1.0 / 60.0);
    }
    let output = run(
        &mut runtime,
        &mut world,
        "print(string.format('%.2f', workspace.Part.Position.Y))",
    );
    assert_eq!(output, ["10.00"]);
}

#[test]
fn align_modes_take_their_own_items_and_a_cframe() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    let output = run(
        &mut runtime,
        &mut world,
        r#"
        local align = Instance.new("AlignOrientation")
        align.Mode = "OneAttachment"
        align.CFrame = CFrame.Angles(0, math.rad(90), 0)
        print(align.Mode == Enum.OrientationAlignmentMode.OneAttachment)
        print(math.round(select(2, align.CFrame:ToEulerAnglesYXZ()) * 180 / math.pi))
        print(pcall(function() align.Mode = Enum.PositionAlignmentMode.OneAttachment end))
        local position = Instance.new("AlignPosition")
        print(position.Mode, position.Responsiveness, position.RigidityEnabled)
        print(pcall(function() position.Mode = "Three" end))
        "#,
    );
    assert_eq!(output[0], "true");
    assert_eq!(output[1], "90");
    assert!(
        output[2].contains("must be an Enum.OrientationAlignmentMode item"),
        "{output:?}"
    );
    assert_eq!(
        output[3],
        "Enum.PositionAlignmentMode.TwoAttachment 10 false"
    );
    assert!(
        output[4].contains("Three is not a valid member of Enum.PositionAlignmentMode"),
        "{output:?}"
    );
}
