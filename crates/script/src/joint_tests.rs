//! Tests for Phase 36: joints, motors and springs from Lua.

use axcenity_core::{ClassName, Value, World};
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

/// An anchored post and an arm hinged to it, made from Lua; the script
/// keeps the hinge in `_G.hinge`.
const ARM: &str = r#"
    local post = Instance.new("Part")
    post.Anchored = true
    post.Size = Vector3.new(1, 1, 1)
    post.Position = Vector3.new(0, 10, 0)
    post.Parent = workspace
    local arm = Instance.new("Part")
    arm.Size = Vector3.new(1, 1, 5)
    arm.Position = Vector3.new(1, 10, 3)
    arm.Parent = workspace
    local a0 = Instance.new("Attachment")
    a0.Position = Vector3.new(0.5, 0, 0)
    a0.Parent = post
    local a1 = Instance.new("Attachment")
    a1.Position = Vector3.new(-0.5, 0, -3)
    a1.Parent = arm
    local hinge = Instance.new("HingeConstraint")
    hinge.Attachment0 = a0
    hinge.Attachment1 = a1
    hinge.Parent = post
    _G.hinge = hinge
"#;

#[test]
fn a_servo_hinge_turns_to_a_target_angle_set_from_lua() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    let output = run(
        &mut runtime,
        &mut world,
        &format!(
            "{ARM}
            hinge.ActuatorType = Enum.ActuatorType.Servo
            hinge.TargetAngle = 45
            print(hinge.ActuatorType, hinge:IsA(\"Constraint\"), hinge.CurrentAngle)"
        ),
    );
    assert_eq!(output, ["Enum.ActuatorType.Servo true 0"]);
    let mut physics = Physics::new();
    for _ in 0..180 {
        physics.step(&mut world, 1.0 / 60.0);
    }
    let output = run(
        &mut runtime,
        &mut world,
        "print(math.round(workspace.Part.HingeConstraint.CurrentAngle))",
    );
    assert_eq!(output, ["45"]);
}

#[test]
fn actuator_type_takes_its_own_items_or_their_names() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    let output = run(
        &mut runtime,
        &mut world,
        &format!(
            "{ARM}
            hinge.ActuatorType = \"Motor\"
            print(hinge.ActuatorType == Enum.ActuatorType.Motor)
            print(pcall(function() hinge.ActuatorType = Enum.Material.Neon end))
            print(pcall(function() hinge.ActuatorType = \"Fast\" end))
            print(pcall(function() hinge.CurrentAngle = 5 end))"
        ),
    );
    assert_eq!(output[0], "true");
    assert!(
        output[1].contains("must be an Enum.ActuatorType item, not a Material"),
        "{output:?}"
    );
    assert!(
        output[2].contains("Fast is not a valid member of Enum.ActuatorType"),
        "{output:?}"
    );
    assert!(
        output[3].contains("CurrentAngle is read-only"),
        "{output:?}"
    );
    let hinge = world
        .descendants(world.workspace())
        .into_iter()
        .find(|&id| world.get(id).unwrap().class() == ClassName::HingeConstraint)
        .unwrap();
    assert_eq!(
        world.get(hinge).unwrap().property("ActuatorType"),
        Some(&Value::EnumItem(axcenity_core::EnumItem {
            enum_name: "ActuatorType".to_owned(),
            item: "Motor".to_owned(),
        }))
    );
}

#[test]
fn only_the_right_classes_have_current_values() {
    let mut world = World::new();
    let mut runtime = ScriptRuntime::new();
    let output = run(
        &mut runtime,
        &mut world,
        r#"
        local rope = Instance.new("RopeConstraint")
        print(rope.CurrentLength)
        print(pcall(function() return rope.CurrentAngle end))
        "#,
    );
    assert_eq!(output[0], "0");
    assert!(output[1].starts_with("false"), "{output:?}");
}
