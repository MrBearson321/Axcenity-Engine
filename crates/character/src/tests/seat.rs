//! Phase 39: seats.

use super::*;

/// A Seat in the Workspace, 2 studs on each side, its middle at `position`
/// and turned `yaw` degrees.
fn seat(world: &mut World, position: Vector3, yaw: f32, anchored: bool) -> InstanceId {
    let seat = world.create(ClassName::Seat).unwrap();
    world.set_name(seat, "Seat").unwrap();
    world.set_parent(seat, Some(world.workspace())).unwrap();
    let set = |world: &mut World, name, value| world.set_property(seat, name, value).unwrap();
    set(world, "Anchored", Value::Bool(anchored));
    set(world, "Size", Value::Vector3(Vector3::new(2.0, 2.0, 2.0)));
    set(world, "Position", Value::Vector3(position));
    set(
        world,
        "Orientation",
        Value::Vector3(Vector3::new(0.0, yaw, 0.0)),
    );
    seat
}

fn text(world: &World, id: InstanceId, name: &str) -> String {
    world
        .get(id)
        .unwrap()
        .property(name)
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned()
}

fn flag(world: &World, id: InstanceId, name: &str) -> bool {
    world
        .get(id)
        .unwrap()
        .property(name)
        .unwrap()
        .as_bool()
        .unwrap()
}

fn occupant(world: &World, seat: InstanceId) -> Option<InstanceId> {
    world
        .get(seat)
        .unwrap()
        .property("Occupant")
        .unwrap()
        .as_instance()
}

fn welds(world: &World, seat: InstanceId) -> usize {
    world
        .get(seat)
        .unwrap()
        .children()
        .iter()
        .filter(|&&id| world.get(id).unwrap().class() == ClassName::WeldConstraint)
        .count()
}

/// What `seat:Sit(controller)` writes.
fn sit_from_script(world: &mut World, seat: InstanceId, controller: InstanceId) {
    world
        .set_property(seat, "Occupant", Value::Instance(Some(controller)))
        .unwrap();
    world
        .set_property(controller, "Sit", Value::Bool(true))
        .unwrap();
}

fn jump() -> MoveInput {
    MoveInput {
        jump: true,
        ..MoveInput::default()
    }
}

#[test]
fn walking_into_a_chair_seats_the_character_and_jump_stands_it_up_in_front() {
    let (mut world, controller, body) = standing();
    // Ahead of the character, its front facing back at it (+Z).
    let chair = seat(&mut world, Vector3::new(0.0, 1.0, -6.0), 180.0, true);
    let mut controllers = Controllers::default();
    run(&mut world, &mut controllers, MoveInput::default(), 2);
    for _ in 0..60 {
        run(&mut world, &mut controllers, forward(), 1);
        if controllers.is_seated(controller) {
            break;
        }
    }
    assert!(
        controllers.is_seated(controller),
        "never sat: {:?}",
        pos(&world, body)
    );
    assert_eq!(occupant(&world, chair), Some(controller));
    assert!(flag(&world, controller, "Sit"));
    assert_eq!(text(&world, controller, "Movement"), "Seated");
    assert_eq!(welds(&world, chair), 1);
    assert!(!flag(&world, body, "Anchored"));
    assert_eq!(
        controllers.take_seat_changes(),
        vec![SeatChange {
            controller,
            seated: true,
            seat: chair,
        }]
    );
    // Feet on the top of the seat, over its middle.
    let sitting = pos(&world, body);
    assert!(
        (sitting.x).abs() < 1e-3
            && (sitting.y - 3.0).abs() < 1e-3
            && (sitting.z + 6.0).abs() < 1e-3,
        "{sitting:?}"
    );

    // Move keys do nothing, and the parts stay unanchored before physics.
    run(&mut world, &mut controllers, forward(), 30);
    assert_eq!(pos(&world, body), sitting);
    controllers.keep_kinematic(&mut world);
    assert!(!flag(&world, body, "Anchored"));

    // Jump stands it up in front of the chair and it lands on the floor.
    run(&mut world, &mut controllers, jump(), 1);
    assert!(!controllers.is_seated(controller));
    assert_eq!(occupant(&world, chair), None);
    assert!(!flag(&world, controller, "Sit"));
    assert_eq!(text(&world, controller, "Movement"), "Walk");
    assert_eq!(welds(&world, chair), 0);
    assert_eq!(
        controllers.take_seat_changes(),
        vec![SeatChange {
            controller,
            seated: false,
            seat: chair,
        }]
    );
    run(&mut world, &mut controllers, MoveInput::default(), 60);
    let stood = pos(&world, body);
    assert!((stood.y - 1.0).abs() < 1e-3, "{stood:?}");
    // Clear of the chair (whose front face is at z = -5), on its front side.
    assert!(stood.z - 1.0 > -5.0 && stood.x.abs() < 1e-3, "{stood:?}");
    assert!(!controllers.is_seated(controller));
    controllers.keep_kinematic(&mut world);
    assert!(flag(&world, body, "Anchored"));
}

#[test]
fn a_disabled_seat_ignores_touches_and_a_script_still_sits_on_it() {
    let (mut world, controller, body) = standing();
    let bench = seat(&mut world, Vector3::new(0.0, 1.0, -6.0), 0.0, true);
    world
        .set_property(bench, "Disabled", Value::Bool(true))
        .unwrap();
    let mut controllers = Controllers::default();
    run(&mut world, &mut controllers, forward(), 60);
    assert!(!controllers.is_seated(controller));
    assert_eq!(occupant(&world, bench), None);
    // It walked up to the bench and stopped there.
    assert!(
        pos(&world, body).z > -5.0 - 1.0 - 0.1,
        "{:?}",
        pos(&world, body)
    );

    sit_from_script(&mut world, bench, controller);
    run(&mut world, &mut controllers, MoveInput::default(), 1);
    assert!(controllers.is_seated(controller));
    assert_eq!(occupant(&world, bench), Some(controller));
    let sitting = pos(&world, body);
    assert!((sitting.y - 3.0).abs() < 1e-3 && (sitting.z + 6.0).abs() < 1e-3);

    // `Sit = false` stands it up, in front (-Z) of the unturned bench.
    world
        .set_property(controller, "Sit", Value::Bool(false))
        .unwrap();
    run(&mut world, &mut controllers, MoveInput::default(), 60);
    assert!(!controllers.is_seated(controller));
    assert_eq!(occupant(&world, bench), None);
    let stood = pos(&world, body);
    assert!(
        stood.z + 1.0 < -7.0 && (stood.y - 1.0).abs() < 1e-3,
        "{stood:?}"
    );
}

#[test]
fn a_new_occupant_or_sit_without_a_seat_changes_who_sits() {
    let (mut world, controller, _) = standing();
    let first = seat(&mut world, Vector3::new(10.0, 1.0, 0.0), 0.0, true);
    let second = seat(&mut world, Vector3::new(-10.0, 1.0, 0.0), 0.0, true);
    let mut controllers = Controllers::default();
    // `Sit = true` with no seat is put back.
    world
        .set_property(controller, "Sit", Value::Bool(true))
        .unwrap();
    run(&mut world, &mut controllers, MoveInput::default(), 1);
    assert!(!flag(&world, controller, "Sit"));

    sit_from_script(&mut world, first, controller);
    run(&mut world, &mut controllers, MoveInput::default(), 1);
    assert_eq!(occupant(&world, first), Some(controller));
    // What `seat:Sit` does for another seat: the first loses its occupant.
    world
        .set_property(first, "Occupant", Value::Instance(None))
        .unwrap();
    sit_from_script(&mut world, second, controller);
    run(&mut world, &mut controllers, MoveInput::default(), 1);
    assert!(controllers.is_seated(controller));
    assert_eq!(occupant(&world, second), Some(controller));
    assert_eq!((welds(&world, first), welds(&world, second)), (0, 1));
    let changes: Vec<(bool, InstanceId)> = controllers
        .take_seat_changes()
        .into_iter()
        .map(|change| (change.seated, change.seat))
        .collect();
    assert_eq!(changes, vec![(true, first), (false, first), (true, second)]);

    // A script that changes Movement stands it up and keeps the new mode.
    world
        .set_property(controller, "Movement", Value::String("Run".to_owned()))
        .unwrap();
    run(&mut world, &mut controllers, MoveInput::default(), 1);
    assert!(!controllers.is_seated(controller));
    assert_eq!(text(&world, controller, "Movement"), "Run");
}

#[test]
fn a_seat_on_an_unanchored_cart_carries_the_sitting_character() {
    let (mut world, controller, body) = standing();
    let workspace = world.workspace();
    // A 6×1×4 cart base on the floor, a seat welded on top, and a
    // LinearVelocity pushing it along +X at 8 studs a second.
    let base = world.create(ClassName::Part).unwrap();
    world.set_parent(base, Some(workspace)).unwrap();
    world
        .set_property(base, "Size", Value::Vector3(Vector3::new(6.0, 1.0, 4.0)))
        .unwrap();
    world
        .set_property(
            base,
            "Position",
            Value::Vector3(Vector3::new(0.0, 0.5, -10.0)),
        )
        .unwrap();
    let chair = seat(&mut world, Vector3::new(0.0, 2.0, -10.0), 0.0, false);
    let weld = world.create(ClassName::WeldConstraint).unwrap();
    world.set_parent(weld, Some(base)).unwrap();
    world
        .set_property(weld, "Part0", Value::Instance(Some(base)))
        .unwrap();
    world
        .set_property(weld, "Part1", Value::Instance(Some(chair)))
        .unwrap();
    let middle = world.create(ClassName::Attachment).unwrap();
    world.set_parent(middle, Some(base)).unwrap();
    let push = world.create(ClassName::LinearVelocity).unwrap();
    world.set_parent(push, Some(base)).unwrap();
    world
        .set_property(push, "Attachment0", Value::Instance(Some(middle)))
        .unwrap();
    world
        .set_property(
            push,
            "VectorVelocity",
            Value::Vector3(Vector3::new(8.0, 0.0, 0.0)),
        )
        .unwrap();

    let assets = Assets::default();
    let mut controllers = Controllers::default();
    let mut physics = Physics::new();
    let mut frame = |world: &mut World, controllers: &mut Controllers| {
        controllers.step(world, &assets, MoveInput::default(), FRAME);
        controllers.keep_kinematic(world);
        physics.step(world, f64::from(FRAME));
    };
    sit_from_script(&mut world, chair, controller);
    frame(&mut world, &mut controllers);
    assert!(controllers.is_seated(controller));
    let seat_start = pos(&world, chair);
    let body_start = pos(&world, body);
    for _ in 0..120 {
        frame(&mut world, &mut controllers);
    }
    let seat_moved = pos(&world, chair).x - seat_start.x;
    assert!(seat_moved > 10.0, "the cart moved {seat_moved}");
    let body_now = pos(&world, body);
    // It rode along, still sitting on top of the seat.
    assert!(
        (body_now.x - body_start.x - seat_moved).abs() < 0.1,
        "{body_start:?} to {body_now:?}, seat by {seat_moved}"
    );
    assert!((body_now.y - body_start.y).abs() < 0.1, "{body_now:?}");
    assert!((body_now.z - body_start.z).abs() < 0.1, "{body_now:?}");
    assert!(controllers.is_seated(controller));

    // Standing up leaves it behind the moving cart.
    run(&mut world, &mut controllers, jump(), 1);
    assert!(!controllers.is_seated(controller));
    controllers.keep_kinematic(&mut world);
    assert!(flag(&world, body, "Anchored"));
}

#[test]
fn a_sitting_character_plays_its_animators_sit_state_and_goes_back_after() {
    let folder = std::env::temp_dir().join(format!("axcenity-seat-sit-{}", std::process::id()));
    std::fs::create_dir_all(&folder).unwrap();
    let state = |name: &str| {
        format!(
            r#"{{ "name": "{name}", "motion": {{ "clip": "" }}, "speed": 1.0, "position": [0.0, 0.0] }}"#
        )
    };
    let machine = format!(
        r#"{{ "format_version": 2, "default_state": "Idle", "states": [{}, {}] }}"#,
        state("Idle"),
        state("Sit")
    );
    std::fs::write(folder.join("hero.statemachine.json"), machine).unwrap();
    let assets = Assets::new(&folder);

    let (mut world, controller, body) = standing();
    let animator = world.create(ClassName::Animator).unwrap();
    world.set_parent(animator, Some(body)).unwrap();
    world
        .set_property(
            animator,
            "StateMachine",
            Value::String("hero.statemachine.json".to_owned()),
        )
        .unwrap();
    let chair = seat(&mut world, Vector3::new(10.0, 1.0, 0.0), 0.0, true);
    let mut controllers = Controllers::default();
    let requested = |world: &mut World| {
        world
            .animator_mut(animator)
            .unwrap()
            .play
            .take()
            .map(|request| request.state)
    };

    sit_from_script(&mut world, chair, controller);
    controllers.step(&mut world, &assets, MoveInput::default(), FRAME);
    assert!(controllers.is_seated(controller));
    assert_eq!(requested(&mut world).as_deref(), Some("Sit"));
    controllers.step(&mut world, &assets, jump(), FRAME);
    assert!(!controllers.is_seated(controller));
    // It hadn't run yet, so it goes back to the state machine's first state.
    assert_eq!(requested(&mut world).as_deref(), Some("Idle"));

    // A state machine without a Sit state is left alone.
    std::fs::write(
        folder.join("hero.statemachine.json"),
        format!(
            r#"{{ "format_version": 2, "default_state": "Idle", "states": [{}] }}"#,
            state("Idle")
        ),
    )
    .unwrap();
    let assets = Assets::new(&folder);
    sit_from_script(&mut world, chair, controller);
    controllers.step(&mut world, &assets, MoveInput::default(), FRAME);
    assert!(controllers.is_seated(controller));
    assert_eq!(requested(&mut world), None);
    let _ = std::fs::remove_dir_all(&folder);
}

fn number(world: &World, id: InstanceId, name: &str) -> f64 {
    world
        .get(id)
        .unwrap()
        .property(name)
        .unwrap()
        .as_number()
        .unwrap()
}

#[test]
fn on_a_vehicle_seat_move_sets_throttle_and_steer_and_the_camera_turns_with_the_car() {
    let (mut world, controller, _) = standing();
    let driver = world.create(ClassName::VehicleSeat).unwrap();
    world.set_parent(driver, Some(world.workspace())).unwrap();
    for (name, value) in [
        ("Anchored", Value::Bool(true)),
        ("Size", Value::Vector3(Vector3::new(2.0, 1.0, 2.0))),
        ("Position", Value::Vector3(Vector3::new(0.0, 0.5, -10.0))),
        ("Orientation", Value::Vector3(Vector3::new(0.0, 90.0, 0.0))),
    ] {
        world.set_property(driver, name, value).unwrap();
    }
    let mut controllers = Controllers::default();
    sit_from_script(&mut world, driver, controller);
    run(&mut world, &mut controllers, MoveInput::default(), 1);
    assert!(controllers.is_seated(controller));
    // The camera is behind the car, looking the way it faces (-X).
    let view = controllers.view(&world).unwrap();
    assert!((view.orientation.y - 90.0).abs() < 1e-3, "{view:?}");
    assert!(view.position.x > 1.0, "{view:?}");

    // W and D: full throttle, steering right. A stick part way: part way.
    let keys = MoveInput {
        forward: true,
        right: true,
        ..MoveInput::default()
    };
    run(&mut world, &mut controllers, keys, 1);
    assert_eq!(number(&world, driver, "Throttle"), 1.0);
    assert_eq!(number(&world, driver, "Steer"), 1.0);
    let stick = MoveInput {
        stick: [-0.5, -0.25],
        ..MoveInput::default()
    };
    run(&mut world, &mut controllers, stick, 1);
    assert_eq!(number(&world, driver, "Throttle"), -0.25);
    assert_eq!(number(&world, driver, "Steer"), -0.5);

    // The car turns 30° right; the camera turns with it.
    world
        .set_property(
            driver,
            "Orientation",
            Value::Vector3(Vector3::new(0.0, 60.0, 0.0)),
        )
        .unwrap();
    run(&mut world, &mut controllers, MoveInput::default(), 1);
    let view = controllers.view(&world).unwrap();
    assert!((view.orientation.y - 60.0).abs() < 1e-3, "{view:?}");

    // Getting out lets go of the car, and the camera stops turning with it.
    run(&mut world, &mut controllers, keys, 1);
    run(&mut world, &mut controllers, jump(), 1);
    assert!(!controllers.is_seated(controller));
    assert_eq!(occupant(&world, driver), None);
    assert_eq!(number(&world, driver, "Throttle"), 0.0);
    assert_eq!(number(&world, driver, "Steer"), 0.0);
    world
        .set_property(
            driver,
            "Orientation",
            Value::Vector3(Vector3::new(0.0, 0.0, 0.0)),
        )
        .unwrap();
    run(&mut world, &mut controllers, MoveInput::default(), 1);
    let view = controllers.view(&world).unwrap();
    assert!((view.orientation.y - 60.0).abs() < 1e-3, "{view:?}");
}
