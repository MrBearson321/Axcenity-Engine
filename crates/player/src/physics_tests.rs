//! Phase 34: worlds saved before physical properties simulate the same.
//! Phase 35: the hello-welds example's welded table and crate.
//! Phase 36: the hello-joints example's hinges, spring, rope and the rest.

use std::path::Path;

use axcenity_core::{Value, Vector3, World};
use axcenity_physics::Physics;

use super::tests::{copy_folder, position_of, run_until, start_example};

/// Opens the format 3 fixture (a copy, since opening upgrades it) and runs
/// three seconds of physics. Each unanchored part's name and Position.
fn simulate_format_3_world() -> (World, Vec<(String, Vector3)>) {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/format-3-physics");
    let folder = std::env::temp_dir().join(format!(
        "axcenity-player-format-3-physics-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&folder);
    copy_folder(&fixture, &folder);
    let mut world = axcenity_project::load(&folder).unwrap().world;
    let _ = std::fs::remove_dir_all(&folder);
    let mut physics = Physics::new();
    for _ in 0..180 {
        physics.step(&mut world, 1.0 / 60.0);
    }
    let moved = world
        .descendants(world.workspace())
        .iter()
        .filter_map(|&id| {
            let part = world.get(id)?;
            if part.property("Anchored") != Some(&Value::Bool(false)) {
                return None;
            }
            let position = part.property("Position")?.as_vector3()?;
            Some((part.name().to_owned(), position))
        })
        .collect();
    (world, moved)
}

/// Where each part was after three seconds before Phase 34 (recorded with
/// that engine). The Metal parts are dense now, so without the upgrade
/// keeping their old properties the ball would knock the target less far,
/// the dropped box would land differently and the wind would blow less.
const BEFORE_PHASE_34: [(&str, Vector3); 7] = [
    ("RampBall", Vector3::new(5.0084047, 0.9986193, -4.3357863)),
    ("Target", Vector3::new(5.0128736, 0.9986193, -6.70073)),
    ("HeavyDrop", Vector3::new(-5.6007466, 2.9866707, 0.34132874)),
    ("Stack", Vector3::new(-5.0042067, 0.9972394, 0.007871534)),
    ("Sheet", Vector3::new(13.841695, 0.09760466, 8.020891)),
    ("Slider", Vector3::new(20.001371, 1.1986202, 0.00031552312)),
    (
        "MetalSlider",
        Vector3::new(20.328312, 0.39766368, 2.9989216),
    ),
];

#[test]
fn a_world_saved_before_physical_properties_simulates_the_same() {
    let (_, moved) = simulate_format_3_world();
    assert_eq!(moved.len(), BEFORE_PHASE_34.len());
    for ((name, now), (old_name, before)) in moved.iter().zip(BEFORE_PHASE_34) {
        assert_eq!(name, old_name);
        let apart = [now.x - before.x, now.y - before.y, now.z - before.z]
            .map(f32::abs)
            .into_iter()
            .fold(0.0, f32::max);
        assert!(apart < 1e-3, "{name} is at {now:?}, not {before:?}");
    }
}

#[test]
fn hello_welds_table_lands_in_one_piece_and_its_script_drops_and_holds_the_crate() {
    let (mut world, mut scripts) = start_example("hello-welds");
    let mut physics = Physics::new();
    let mut clock = 0;
    let mut at = |until| {
        run_until(&mut world, &mut scripts, &mut physics, &mut clock, until);
        (position_of(&world, "Crate").y, world.clone())
    };

    // The crate hangs from the anchored beam it's welded to.
    let (crate_y, world_then) = at(1.9);
    assert_eq!(crate_y, 10.5);
    // By then the table has landed standing, every leg on the floor and
    // the top on them.
    for leg in ["Leg1", "Leg2", "Leg3", "Leg4"] {
        let y = position_of(&world_then, leg).y;
        assert!((y - 1.5).abs() < 0.05, "{leg} is at {y}");
    }
    assert!((position_of(&world_then, "TableTop").y - 3.25).abs() < 0.05);
    let top = world_then
        .find_first_child(world_then.workspace(), "TableTop")
        .unwrap();
    let handle = world_then.find_first_child(top, "Handle").unwrap();
    assert_eq!(
        world_then.get(handle).unwrap().property("Position"),
        Some(&Value::Vector3(Vector3::new(2.5, 0.25, 0.0))),
        "the attachment keeps its offset on the top"
    );

    // Unwelded at 2 s, welded again in mid-air, then dropped for good.
    let (held, _) = at(2.6);
    assert!(held < 10.0 && held > 3.0, "it fell a little: {held}");
    let (still, _) = at(3.5);
    assert_eq!(still, held, "welded again where it was");
    let (landed, _) = at(5.0);
    assert!(
        (landed - 1.0).abs() < 0.05,
        "it fell to the floor: {landed}"
    );
    let output: Vec<String> = scripts
        .take_output()
        .into_iter()
        .map(|line| line.text.replace('\t', " "))
        .collect();
    assert!(
        output.contains(&"5 parts in the table, rooted at TableTop".to_owned()),
        "{output:?}"
    );
}

/// The angular velocity of the part `name`.
fn spin_of(world: &World, name: &str) -> Vector3 {
    let id = world.find_first_child(world.workspace(), name).unwrap();
    world
        .get(id)
        .unwrap()
        .property("AssemblyAngularVelocity")
        .and_then(Value::as_vector3)
        .unwrap()
}

#[test]
fn hello_joints_door_servo_motor_spring_and_winch_work() {
    let (mut world, mut scripts) = start_example("hello-joints");
    let mut physics = Physics::new();
    let mut clock = 0;
    run_until(&mut world, &mut scripts, &mut physics, &mut clock, 5.5);

    let output: Vec<String> = scripts
        .take_output()
        .into_iter()
        .map(|line| line.text.replace('\t', " "))
        .collect();
    // The door stopped at its limit, the servo holds the arm up, the
    // spring settled at its free length and the winch lifted the crate.
    assert!(
        output.contains(&"door 110, arm -45, spring 5.0, rope 4.0".to_owned()),
        "{output:?}"
    );
    let y = |name| position_of(&world, name).y;
    assert!((y("Crate") - 14.5).abs() < 0.2, "{}", y("Crate"));
    assert!((y("Bob") - 8.0).abs() < 0.05, "{}", y("Bob"));
    // The motor spins the wheel around its axle at 4 radians a second.
    assert!((spin_of(&world, "Wheel").x - 4.0).abs() < 0.1);
    // The lift rose to its target, and the drill slid to its limit, spinning.
    assert!((y("Lift") - 7.75).abs() < 0.05, "{}", y("Lift"));
    assert!((position_of(&world, "Bit").x - 60.0).abs() < 0.05);
    assert!((spin_of(&world, "Bit").x - 3.0).abs() < 0.1);
    // The pendulums hang from their pins, 6 and 2.5 studs out.
    let weight = position_of(&world, "Weight");
    let from_pin =
        ((weight.x - 20.0).powi(2) + (weight.y - 14.0).powi(2) + weight.z.powi(2)).sqrt();
    assert!((from_pin - 6.0).abs() < 0.1, "{weight:?}");
    let pendulum = position_of(&world, "Pendulum");
    assert!(pendulum.y < 13.0, "it swung down: {pendulum:?}");
}
