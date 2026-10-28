//! Tests for Phase 39: `seat:Sit`, `Occupant` and `Seated` from Lua.

use axcenity_core::{ClassName, InstanceId, Value, World};

use super::*;

fn child(world: &mut World, parent: InstanceId, class: ClassName, name: &str) -> InstanceId {
    let id = world.create(class).unwrap();
    world.set_name(id, name).unwrap();
    world.set_parent(id, Some(parent)).unwrap();
    id
}

fn output(runtime: &mut ScriptRuntime) -> Vec<String> {
    runtime
        .take_output()
        .into_iter()
        .map(|line| match line.kind {
            OutputKind::Error => format!("error: {}", line.text),
            _ => line.text.replace('\t', " "),
        })
        .collect()
}

/// A character with a controller, and two Seats, the bench Disabled.
fn chairs() -> (World, InstanceId, InstanceId, InstanceId) {
    let mut world = World::new();
    let workspace = world.workspace();
    let model = child(&mut world, workspace, ClassName::Model, "Character");
    child(&mut world, model, ClassName::Part, "Body");
    let controller = child(
        &mut world,
        model,
        ClassName::ThirdPersonController,
        "ThirdPersonController",
    );
    let chair = child(&mut world, workspace, ClassName::Seat, "Chair");
    let bench = child(&mut world, workspace, ClassName::Seat, "Bench");
    world
        .set_property(bench, "Disabled", Value::Bool(true))
        .unwrap();
    (world, controller, chair, bench)
}

fn start(world: &mut World, runtime: &mut ScriptRuntime, source: &str) {
    let script = world.create(ClassName::Script).unwrap();
    world
        .set_parent(script, Some(world.server_script_service()))
        .unwrap();
    runtime.start_script(world, script, "scripts/test.server.lua", source);
}

#[test]
fn sit_writes_the_occupant_on_a_disabled_seat_and_leaves_the_other_seat() {
    let (mut world, controller, chair, bench) = chairs();
    let mut runtime = ScriptRuntime::new();
    start(
        &mut world,
        &mut runtime,
        r#"
local controller = workspace.Character.ThirdPersonController
print(workspace.Chair:IsA("Part"), workspace.Chair:IsA("BasePart"), workspace.Chair.Disabled)
workspace.Chair:Sit(controller)
print(workspace.Chair.Occupant == controller, controller.Sit)
workspace.Bench:Sit(controller)
print(workspace.Chair.Occupant, workspace.Bench.Occupant == controller)
print(pcall(function() workspace.Bench.Occupant = nil end))
print(pcall(function() workspace.Bench:Sit(workspace.Chair) end))
controller.Sit = false
"#,
    );
    let lines = output(&mut runtime);
    assert_eq!(lines[0], "true true false");
    assert_eq!(lines[1], "true true");
    assert_eq!(lines[2], "nil true");
    assert!(
        lines[3].starts_with("false") && lines[3].contains("Occupant is read-only"),
        "{lines:?}"
    );
    assert!(
        lines[4].starts_with("false") && lines[4].contains("Sit(controller) needs"),
        "{lines:?}"
    );
    assert_eq!(
        world.get(chair).unwrap().property("Occupant"),
        Some(&Value::Instance(None))
    );
    assert_eq!(
        world.get(bench).unwrap().property("Occupant"),
        Some(&Value::Instance(Some(controller)))
    );
    assert_eq!(
        world.get(controller).unwrap().property("Sit"),
        Some(&Value::Bool(false))
    );
}

#[test]
fn seated_fires_on_the_controller_with_whether_it_sat_and_the_seat() {
    let (mut world, controller, chair, _) = chairs();
    let mut runtime = ScriptRuntime::new();
    start(
        &mut world,
        &mut runtime,
        r#"
workspace.Character.ThirdPersonController.Seated:Connect(function(active, seat)
    print("seated", active, seat.Name)
end)
"#,
    );
    runtime.fire_seated(
        &mut world,
        &[(controller, true, chair), (controller, false, chair)],
    );
    assert_eq!(
        output(&mut runtime),
        ["seated true Chair", "seated false Chair"]
    );
}
