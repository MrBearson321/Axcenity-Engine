//! Tests for Phase 43: Health's methods, events and step, from Lua.

use axcenity_core::{ClassName, InstanceId, Value, Vector3, World};

use axcenity_script::{OutputKind, ScriptRuntime};

use crate::{script_runtime, step_health};

fn start(world: &mut World, runtime: &mut ScriptRuntime, source: &str) {
    let script = world.create(ClassName::Script).unwrap();
    world
        .set_parent(script, Some(world.server_script_service()))
        .unwrap();
    runtime.start_script(world, script, "scripts/test.server.lua", source);
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

/// A Model named Character with a Body part and a Health.
fn character(world: &mut World) -> (InstanceId, InstanceId) {
    let model = world.create(ClassName::Model).unwrap();
    world.set_name(model, "Character").unwrap();
    world.set_parent(model, Some(world.workspace())).unwrap();
    let body = world.create(ClassName::Part).unwrap();
    world.set_name(body, "Body").unwrap();
    world.set_parent(body, Some(model)).unwrap();
    world
        .set_property(
            body,
            "Position",
            Value::Vector3(Vector3::new(0.0, 2.5, 0.0)),
        )
        .unwrap();
    world
        .set_property(body, "Size", Value::Vector3(Vector3::new(2.0, 5.0, 1.0)))
        .unwrap();
    world
        .set_property(body, "Anchored", Value::Bool(true))
        .unwrap();
    let health = world.create(ClassName::Health).unwrap();
    world.set_name(health, "Health").unwrap();
    world.set_parent(health, Some(model)).unwrap();
    (model, health)
}

fn health_of(world: &World, health: InstanceId) -> f64 {
    world
        .get(health)
        .unwrap()
        .property("Health")
        .and_then(Value::as_number)
        .unwrap()
}

const LISTEN: &str = r#"
local health = workspace.Character.Health
health.HealthChanged:Connect(function(new, old) print("changed", new, old) end)
health.Damaged:Connect(function(amount, source)
    print("damaged", amount, source and source.Name or "nil")
end)
health.Died:Connect(function() print("died") end)
"#;

#[test]
fn died_fires_once_at_zero_even_when_several_hits_land_in_one_frame() {
    let mut world = World::new();
    character(&mut world);
    let mut runtime = script_runtime();
    let source = format!(
        "{LISTEN}
health:TakeDamage(60, workspace.Character.Body)
health:TakeDamage(60)
health:TakeDamage(60)
print('health', health.Health)"
    );
    start(&mut world, &mut runtime, &source);
    runtime.fire_changes(&mut world);
    let lines = output(&mut runtime);
    assert_eq!(
        lines,
        [
            "health 0",
            "changed 40 100",
            "damaged 60 Body",
            "changed 0 40",
            "damaged 40 nil",
            "died",
        ],
        "{lines:?}"
    );
    // Later frames add nothing: it's dead.
    step_health(&mut runtime, &mut world, 1.0 / 60.0);
    runtime.heartbeat(&mut world, 1.0 / 60.0);
    assert!(output(&mut runtime).is_empty());
}

#[test]
fn heal_stops_at_max_health_and_does_nothing_once_dead_until_health_is_set() {
    let mut world = World::new();
    let (_, health) = character(&mut world);
    let mut runtime = script_runtime();
    let source = format!(
        "{LISTEN}
health:TakeDamage(30)
health:Heal(50)
health.Health = 0
health:Heal(10)
health.Health = 250"
    );
    start(&mut world, &mut runtime, &source);
    runtime.fire_changes(&mut world);
    assert_eq!(
        output(&mut runtime),
        [
            "changed 70 100",
            "damaged 30 nil",
            "changed 100 70",
            "changed 0 100",
            "died",
            "changed 100 0",
        ]
    );
    assert_eq!(health_of(&world, health), 100.0);
}

#[test]
fn an_invulnerable_health_takes_no_damage_and_bad_amounts_are_errors() {
    let mut world = World::new();
    let (_, health) = character(&mut world);
    let mut runtime = script_runtime();
    let source = format!(
        "{LISTEN}
health.Invulnerable = true
health:TakeDamage(30)
health.Invulnerable = false
health:TakeDamage(-5)
print(pcall(function() health:TakeDamage('lots') end))
print(pcall(function() health:TakeDamage(5, 'me') end))"
    );
    start(&mut world, &mut runtime, &source);
    runtime.fire_changes(&mut world);
    let lines = output(&mut runtime);
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert!(lines[0].starts_with("false") && lines[0].contains("finite number"));
    assert!(lines[1].starts_with("false") && lines[1].contains("Instance or nil"));
    assert_eq!(health_of(&world, health), 100.0);
}

#[test]
fn regeneration_waits_regen_delay_after_damage_and_stops_at_max_health() {
    let mut world = World::new();
    let (_, health) = character(&mut world);
    for (name, value) in [("RegenRate", 10.0), ("RegenDelay", 1.0)] {
        world
            .set_property(health, name, Value::Number(value))
            .unwrap();
    }
    let mut runtime = script_runtime();
    start(
        &mut world,
        &mut runtime,
        "workspace.Character.Health:TakeDamage(25)",
    );
    let step = |world: &mut World, runtime: &mut ScriptRuntime, seconds: f64| {
        for _ in 0..(seconds * 60.0).round() as usize {
            step_health(runtime, world, 1.0 / 60.0);
        }
    };
    step(&mut world, &mut runtime, 0.9);
    assert_eq!(health_of(&world, health), 75.0);
    step(&mut world, &mut runtime, 1.1);
    let now = health_of(&world, health);
    assert!((now - 85.0).abs() < 1.0, "{now}");
    step(&mut world, &mut runtime, 3.0);
    assert_eq!(health_of(&world, health), 100.0);
}

#[test]
fn a_damage_zone_damages_each_health_inside_it_once_a_frame() {
    let mut world = World::new();
    let (_, health) = character(&mut world);
    let zone = world.create(ClassName::DamageZone).unwrap();
    world.set_parent(zone, Some(world.workspace())).unwrap();
    world
        .set_property(zone, "Size", Value::Vector3(Vector3::new(8.0, 4.0, 8.0)))
        .unwrap();
    world
        .set_property(
            zone,
            "Position",
            Value::Vector3(Vector3::new(0.0, 2.0, 0.0)),
        )
        .unwrap();
    assert!(world.get(zone).unwrap().class().is_a("Part"));
    let mut runtime = script_runtime();
    start(
        &mut world,
        &mut runtime,
        "workspace.Character.Health.Damaged:Connect(function(amount, source)
            print(string.format('%.3f', amount), source.ClassName)
        end)",
    );
    step_health(&mut runtime, &mut world, 0.5);
    assert_eq!(output(&mut runtime), ["5.000 DamageZone"]);
    assert_eq!(health_of(&world, health), 95.0);
}
