//! Phase 40 in the editor: Insert > Vehicle, and driving the car it makes
//! in Play with the keyboard and a gamepad.

use axcenity_core::input::{InputEvent, InputKind};
use axcenity_core::{KeyCode, Value, Vector3};
use axcenity_render::glam::Vec3;

use super::tests::{copy_of_example, inserted, part};
use super::*;

const FRAME: f64 = 1.0 / 60.0;

fn key(key: KeyCode, pressed: bool) -> InputEvent {
    InputEvent::new(InputKind::Key { key, pressed })
}

fn stick(position: [f32; 2]) -> InputEvent {
    InputEvent::new(InputKind::PadStick {
        pad: 0,
        stick: KeyCode::Thumbstick1,
        position,
    })
}

/// Plays `frames` frames, the first with `events`.
fn play(app: &mut EditorApp, events: &[InputEvent], frames: usize) {
    app.play_frame(FRAME, events, None, None);
    for _ in 1..frames {
        app.play_frame(FRAME, &[], None, None);
    }
}

fn game_vector(app: &EditorApp, id: InstanceId, name: &str) -> Vector3 {
    app.play
        .as_ref()
        .unwrap()
        .world()
        .get(id)
        .unwrap()
        .property(name)
        .and_then(Value::as_vector3)
        .unwrap()
}

fn ground_speed(app: &EditorApp, chassis: InstanceId) -> f32 {
    let v = game_vector(app, chassis, "AssemblyLinearVelocity");
    v.x.hypot(v.z)
}

/// Holds S until the car has all but stopped, then lets go.
fn brake(app: &mut EditorApp, chassis: InstanceId) {
    app.play_frame(FRAME, &[key(KeyCode::S, true)], None, None);
    for _ in 0..180 {
        if ground_speed(app, chassis) < 1.0 {
            break;
        }
        app.play_frame(FRAME, &[], None, None);
    }
    app.play_frame(FRAME, &[key(KeyCode::S, false)], None, None);
    assert!(
        ground_speed(app, chassis) < 1.5,
        "{}",
        ground_speed(app, chassis)
    );
}

/// The character controller's camera, which Play draws from.
fn camera(app: &EditorApp) -> axcenity_character::CharacterCamera {
    app.play.as_ref().unwrap().character_camera().unwrap()
}

fn named(app: &EditorApp, parent: InstanceId, name: &str) -> InstanceId {
    app.world.find_first_child(parent, name).unwrap()
}

#[test]
fn insert_vehicle_makes_a_car_in_one_step_that_drives_with_wasd_and_a_gamepad() {
    let folder = copy_of_example("hello-vehicle", "insert-vehicle");
    let mut app = EditorApp::new(Some(&folder));
    // Only the inserted car, beside the character (at x = 0, z = 8).
    for name in ["Car", "AutoCar"] {
        let car = part(&app, name);
        app.world.destroy(car).unwrap();
    }
    app.viewport
        .look_at(Vec3::new(5.0, 20.0, 30.0), Vec3::new(5.0, 0.0, 8.0));
    app.snap.move_on = false;
    let car = inserted(&mut app, Insertable::Vehicle, None);
    assert_eq!(app.world.get(car).unwrap().name(), "Car");
    assert_eq!(app.history.undo_label(), Some("Insert Car"));
    let chassis = named(&app, car, "Chassis");
    let seat = named(&app, car, "DriverSeat");
    assert_eq!(app.world.get(seat).unwrap().class(), ClassName::VehicleSeat);
    let wheels = app
        .world
        .get(chassis)
        .unwrap()
        .children()
        .iter()
        .filter(|&&id| app.world.get(id).unwrap().class() == ClassName::VehicleWheel)
        .count();
    assert_eq!(wheels, 4);
    assert_eq!(
        app.world
            .get(chassis)
            .unwrap()
            .property("Position")
            .and_then(Value::as_vector3),
        Some(Vector3::new(5.0, 2.0, 8.0))
    );

    app.play();
    assert!(app.playing());
    let model = part(&app, "Character");
    let controller = named(&app, model, "ThirdPersonController");
    let body = named(&app, model, "Body");
    let occupant = |app: &EditorApp| {
        app.play
            .as_ref()
            .unwrap()
            .world()
            .get(seat)
            .unwrap()
            .property("Occupant")
            .cloned()
    };
    play(&mut app, &[], 30);

    // D walks the character right, into the seat on the car's left.
    app.play_frame(FRAME, &[key(KeyCode::D, true)], None, None);
    for _ in 0..120 {
        if occupant(&app) == Some(Value::Instance(Some(controller))) {
            break;
        }
        app.play_frame(FRAME, &[], None, None);
    }
    app.play_frame(FRAME, &[key(KeyCode::D, false)], None, None);
    assert_eq!(occupant(&app), Some(Value::Instance(Some(controller))));
    play(&mut app, &[], 30);

    // W drives forward (-Z), with the camera behind the car.
    let start = game_vector(&app, chassis, "Position");
    play(&mut app, &[key(KeyCode::W, true)], 36);
    let ahead = game_vector(&app, chassis, "Position");
    assert!(start.z - ahead.z > 8.0, "{start:?} {ahead:?}");
    assert!((ahead.x - start.x).abs() < 1.0, "{start:?} {ahead:?}");
    let eye = camera(&app).position;
    assert!(eye.z > ahead.z + 3.0, "{eye:?} {ahead:?}");

    // W and D steer it right; the camera turns with it.
    play(&mut app, &[key(KeyCode::D, true)], 36);
    play(
        &mut app,
        &[key(KeyCode::D, false), key(KeyCode::W, false)],
        1,
    );
    let heading = game_vector(&app, chassis, "Orientation").y;
    assert!(heading < -20.0, "{heading}");
    let look = camera(&app).orientation;
    let turned = (look.y - heading + 180.0).rem_euclid(360.0) - 180.0;
    assert!(turned.abs() < 5.0, "camera {look:?}, car {heading}");

    // S brakes it.
    brake(&mut app, chassis);

    // A gamepad's left stick drives it too.
    let before = game_vector(&app, chassis, "Position");
    play(
        &mut app,
        &[
            InputEvent::new(InputKind::PadConnected(0)),
            stick([0.0, 1.0]),
        ],
        36,
    );
    play(&mut app, &[stick([0.0, 0.0])], 1);
    let after = game_vector(&app, chassis, "Position");
    let moved = (after.x - before.x).hypot(after.z - before.z);
    assert!(moved > 8.0, "{before:?} {after:?}");
    brake(&mut app, chassis);

    // Space gets out: the car stays where it stopped, and the camera is the
    // character's again.
    play(&mut app, &[key(KeyCode::Space, true)], 1);
    play(&mut app, &[key(KeyCode::Space, false)], 1);
    assert_eq!(occupant(&app), Some(Value::Instance(None)));
    let stopped = game_vector(&app, chassis, "Position");
    play(&mut app, &[], 90);
    let parked = game_vector(&app, chassis, "Position");
    let drift = (parked.x - stopped.x).hypot(parked.z - stopped.z);
    assert!(drift < 0.3, "{stopped:?} {parked:?}");
    let standing = game_vector(&app, body, "Position");
    let eye = camera(&app).position;
    let from_character = (eye.x - standing.x).hypot(eye.z - standing.z);
    assert!(from_character < 20.0, "{eye:?} {standing:?}");
    play(&mut app, &[key(KeyCode::W, true)], 30);
    let walked = game_vector(&app, body, "Position");
    assert!(
        (walked.x - standing.x).hypot(walked.z - standing.z) > 3.0,
        "W walks again: {standing:?} {walked:?}"
    );
    let still = game_vector(&app, chassis, "Position");
    assert!((still.x - parked.x).hypot(still.z - parked.z) < 0.3);
}
