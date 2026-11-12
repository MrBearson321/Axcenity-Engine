//! Tests for Phase 34: impulses, mass and physical properties from Lua.

use axcenity_core::{ClassName, InstanceId, Material, PhysicalProperties, Value, Vector3, World};
use axcenity_physics::Physics;

use super::*;

const STEP: f64 = 1.0 / 60.0;

/// A world with an unanchored Part called `name`, of `size`, high in the
/// air (so nothing touches it), and physics that has already seen it.
fn floating(name: &str, size: Vector3) -> (World, InstanceId, Physics) {
    let mut world = World::new();
    let part = world.create(ClassName::Part).unwrap();
    world.set_name(part, name).unwrap();
    world
        .set_property(part, "Size", Value::Vector3(size))
        .unwrap();
    world
        .set_property(
            part,
            "Position",
            Value::Vector3(Vector3::new(0.0, 500.0, 0.0)),
        )
        .unwrap();
    world.set_parent(part, Some(world.workspace())).unwrap();
    let mut physics = Physics::new();
    physics.step(&mut world, STEP);
    (world, part, physics)
}

fn run(world: &mut World, source: &str) -> (ScriptRuntime, Vec<String>) {
    let mut runtime = ScriptRuntime::new();
    let script = world.create(ClassName::Script).unwrap();
    world
        .set_parent(script, Some(world.server_script_service()))
        .unwrap();
    runtime.start_script(world, script, "scripts/test.server.lua", source);
    let lines = runtime
        .take_output()
        .into_iter()
        .map(|line| match line.kind {
            OutputKind::Error => format!("error: {}", line.text),
            _ => line.text.replace('\t', " "),
        })
        .collect();
    (runtime, lines)
}

fn vector(world: &World, id: InstanceId, name: &str) -> Vector3 {
    world
        .get(id)
        .unwrap()
        .property(name)
        .and_then(Value::as_vector3)
        .unwrap()
}

#[test]
fn apply_impulse_changes_velocity_by_impulse_over_mass() {
    // A 1×1×1 Plastic block weighs 1: the 1 kg-equivalent part.
    let (mut world, part, mut physics) = floating("Box", Vector3::new(1.0, 1.0, 1.0));
    let before = vector(&world, part, "AssemblyLinearVelocity");
    let (_, lines) = run(
        &mut world,
        "print(workspace.Box:GetMass(), workspace.Box.AssemblyMass)\n\
         workspace.Box:ApplyImpulse(Vector3.new(6, 0, -3))",
    );
    assert_eq!(lines, ["1 1"]);
    physics.step(&mut world, STEP);
    let after = vector(&world, part, "AssemblyLinearVelocity");
    // Gravity only pulls down, so across it changes by exactly J / m.
    assert!((after.x - before.x - 6.0).abs() < 1e-3, "{after:?}");
    assert!((after.z - before.z + 3.0).abs() < 1e-3, "{after:?}");
    let position = vector(&world, part, "Position");
    assert!(
        position.x > 0.05 && position.z < -0.02,
        "it moved: {position:?}"
    );

    // Six times the mass (twice the volume, three times the density):
    // a sixth of the change.
    let (mut world, part, mut physics) = floating("Crate", Vector3::new(2.0, 1.0, 1.0));
    let (_, lines) = run(
        &mut world,
        "workspace.Crate.CustomPhysicalProperties = PhysicalProperties.new(3, 0.5, 0.25)\n\
         print(workspace.Crate:GetMass())\n\
         workspace.Crate:ApplyImpulse(Vector3.new(6, 0, 0))\n\
         workspace.Crate:ApplyImpulse(Vector3.new(6, 0, 0))",
    );
    assert_eq!(lines, ["6"]);
    physics.step(&mut world, STEP);
    let after = vector(&world, part, "AssemblyLinearVelocity");
    assert!((after.x - 2.0).abs() < 1e-3, "two pushes add up: {after:?}");
}

#[test]
fn impulses_off_center_and_angular_ones_spin_the_part() {
    let (mut world, part, mut physics) = floating("Box", Vector3::new(1.0, 1.0, 1.0));
    let (_, lines) = run(
        &mut world,
        "local box = workspace.Box\n\
         print(box.AssemblyCenterOfMass == box.Position)\n\
         box:ApplyImpulseAtPosition(Vector3.new(1, 0, 0), box.Position + Vector3.new(0, 0.5, 0))",
    );
    assert_eq!(lines, ["true"]);
    physics.step(&mut world, STEP);
    let velocity = vector(&world, part, "AssemblyLinearVelocity");
    let spin = vector(&world, part, "AssemblyAngularVelocity");
    assert!((velocity.x - 1.0).abs() < 1e-3, "{velocity:?}");
    // Pushed along +X above the middle: it turns about -Z. A unit cube's
    // inertia is 1/6, so the arm's 0.5 gives 3 radians a second.
    assert!((spin.z + 3.0).abs() < 0.1, "{spin:?}");

    let (mut world, part, mut physics) = floating("Box", Vector3::new(1.0, 1.0, 1.0));
    run(
        &mut world,
        "workspace.Box:ApplyAngularImpulse(Vector3.new(0, 1, 0))",
    );
    physics.step(&mut world, STEP);
    let spin = vector(&world, part, "AssemblyAngularVelocity");
    assert!((spin.y - 6.0).abs() < 0.1, "{spin:?}");
}

#[test]
fn an_anchored_part_ignores_impulses() {
    let (mut world, part, mut physics) = floating("Box", Vector3::new(1.0, 1.0, 1.0));
    world
        .set_property(part, "Anchored", Value::Bool(true))
        .unwrap();
    run(
        &mut world,
        "workspace.Box:ApplyImpulse(Vector3.new(50, 0, 0))",
    );
    physics.step(&mut world, STEP);
    assert_eq!(
        vector(&world, part, "AssemblyLinearVelocity"),
        Vector3::default()
    );
}

#[test]
fn physical_properties_come_from_the_material_or_the_script() {
    let (mut world, part, _) = floating("Box", Vector3::new(1.0, 1.0, 1.0));
    let (_, lines) = run(
        &mut world,
        r#"
local box = workspace.Box
print(box.CustomPhysicalProperties)
box.Material = Enum.Material.Ice
local ice = box.CurrentPhysicalProperties
print(typeof(ice), ice.Friction < 0.1, ice.FrictionWeight)
print(PhysicalProperties.new(Enum.Material.Ice) == ice)
local own = PhysicalProperties.new(500, -1, 0.5)
print(own.Density, own.Friction, own.Elasticity, own.FrictionWeight, own.ElasticityWeight)
box.CustomPhysicalProperties = own
print(box.CurrentPhysicalProperties == own, box:GetMass())
box.CustomPhysicalProperties = nil
print(box.CustomPhysicalProperties)
print(pcall(function() box.AssemblyMass = 3 end))
print(pcall(function() return PhysicalProperties.new("heavy") end))
"#,
    );
    assert_eq!(lines[0], "nil");
    assert_eq!(lines[1], "PhysicalProperties true 3");
    assert_eq!(lines[2], "true");
    // Out-of-range numbers are brought into range; weights default to 1.
    assert_eq!(lines[3], "100 0 0.5 1 1");
    assert_eq!(lines[4], "true 100");
    assert_eq!(lines[5], "nil");
    assert!(
        lines[6].starts_with("false") && lines[6].contains("read-only"),
        "{}",
        lines[6]
    );
    assert!(
        lines[7].starts_with("false") && lines[7].contains("Material"),
        "{}",
        lines[7]
    );
    assert_eq!(
        world
            .get(part)
            .unwrap()
            .property("CustomPhysicalProperties"),
        Some(&Value::PhysicalProperties(None))
    );
    assert_eq!(Material::Ice.physical().friction_weight, 3.0);
    assert_ne!(Material::Metal.physical(), PhysicalProperties::CLASSIC);
}

#[test]
fn setting_velocity_throws_a_part_and_gravity_reads_from_the_workspace() {
    let (mut world, part, mut physics) = floating("Ball", Vector3::new(1.0, 1.0, 1.0));
    let (_, lines) = run(
        &mut world,
        "print(workspace.Gravity)\n\
         workspace.Gravity = 0\n\
         workspace.Ball.AssemblyLinearVelocity = Vector3.new(0, 0, 12)",
    );
    assert_eq!(lines, ["196.2"]);
    for _ in 0..30 {
        physics.step(&mut world, STEP);
    }
    let velocity = vector(&world, part, "AssemblyLinearVelocity");
    assert!(
        (velocity.z - 12.0).abs() < 1e-3,
        "no gravity, no drag: {velocity:?}"
    );
    assert!(velocity.y.abs() < 1e-3, "{velocity:?}");
    assert!((vector(&world, part, "Position").z - 6.0).abs() < 0.3);
}

#[test]
fn apply_force_pushes_for_one_step_or_the_time_given() {
    // A 2×2×2 Plastic block weighs 8.
    let (mut world, part, mut physics) = floating("Box", Vector3::new(2.0, 2.0, 2.0));
    run(
        &mut world,
        "workspace.Box:ApplyForce(Vector3.new(480, 0, 0))",
    );
    physics.step(&mut world, STEP);
    // 480 for 1/60 s is an impulse of 8: 1 stud a second.
    let velocity = vector(&world, part, "AssemblyLinearVelocity");
    assert!((velocity.x - 1.0).abs() < 1e-3, "{velocity:?}");

    let (mut world, part, mut physics) = floating("Box", Vector3::new(2.0, 2.0, 2.0));
    run(
        &mut world,
        "workspace.Box:ApplyForce(Vector3.new(16, 0, 0), 2)",
    );
    physics.step(&mut world, STEP);
    let velocity = vector(&world, part, "AssemblyLinearVelocity");
    assert!((velocity.x - 4.0).abs() < 1e-3, "{velocity:?}");

    let (_, lines) = run(
        &mut world,
        "print(pcall(function() workspace.Box:ApplyForce(Vector3.new(1, 0, 0), -1) end))",
    );
    assert!(lines[0].contains("0 seconds or more"), "{lines:?}");
}

#[test]
fn apply_torque_spins_through_inertia() {
    let (mut world, part, mut physics) = floating("Box", Vector3::new(1.0, 1.0, 1.0));
    // A unit cube's inertia is 1/6: a torque of 60 for one step spins it
    // at 6 radians a second.
    run(
        &mut world,
        "workspace.Box:ApplyTorque(Vector3.new(0, 60, 0))",
    );
    physics.step(&mut world, STEP);
    let spin = vector(&world, part, "AssemblyAngularVelocity");
    assert!((spin.y - 6.0).abs() < 0.1, "{spin:?}");
}

#[test]
fn linear_momentum_is_mass_times_velocity() {
    let (mut world, part, _) = floating("Box", Vector3::new(2.0, 2.0, 2.0));
    world
        .set_property(
            part,
            "AssemblyLinearVelocity",
            Value::Vector3(Vector3::new(3.0, 0.0, 0.0)),
        )
        .unwrap();
    let (_, lines) = run(&mut world, "print(workspace.Box.AssemblyLinearMomentum)");
    assert_eq!(lines, ["24, 0, 0"]);
}
