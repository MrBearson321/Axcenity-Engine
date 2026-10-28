//! Phase 40: the hello-vehicle example's cars.

use axcenity_core::{InstanceId, Value, Vector3};

use super::tests::start_example;
use super::*;

/// The example running in the player's frame order: the character (and
/// `Seated`), scripts, the kinematic hold, physics.
struct Game {
    world: World,
    scripts: ScriptRuntime,
    assets: Assets,
    characters: Controllers,
    physics: Physics,
    printed: Vec<String>,
}

impl Game {
    fn start() -> Self {
        let (world, scripts) = start_example("hello-vehicle");
        Self {
            world,
            scripts,
            assets: Assets::default(),
            characters: Controllers::default(),
            physics: Physics::new(),
            printed: Vec::new(),
        }
    }

    fn frame(&mut self, input: MoveInput) {
        let seconds = 1.0 / 60.0;
        self.characters
            .step(&mut self.world, &self.assets, input, seconds);
        fire_seated(&mut self.scripts, &mut self.world, &mut self.characters);
        self.scripts.step(&mut self.world, f64::from(seconds));
        self.characters.keep_kinematic(&mut self.world);
        self.physics.step(&mut self.world, f64::from(seconds));
        self.scripts.heartbeat(&mut self.world, f64::from(seconds));
        self.printed
            .extend(self.scripts.take_output().into_iter().map(|line| line.text));
    }

    fn frames(&mut self, count: usize, input: MoveInput) {
        for _ in 0..count {
            self.frame(input);
        }
    }

    fn find(&self, path: &[&str]) -> InstanceId {
        path.iter().fold(self.world.workspace(), |at, name| {
            self.world.find_first_child(at, name).unwrap()
        })
    }

    fn vector(&self, path: &[&str], name: &str) -> Vector3 {
        self.world
            .get(self.find(path))
            .unwrap()
            .property(name)
            .and_then(Value::as_vector3)
            .unwrap()
    }
}

fn distance(a: Vector3, b: Vector3) -> f32 {
    ((a.x - b.x).powi(2) + (a.y - b.y).powi(2) + (a.z - b.z).powi(2)).sqrt()
}

#[test]
fn hello_vehicle_drives_the_empty_auto_car_from_lua_and_the_red_car_waits() {
    let mut game = Game::start();
    let chassis = ["AutoCar", "Chassis"];
    let start = game.vector(&chassis, "Position");
    let parked = game.vector(&["Car", "Chassis"], "Position");
    game.frames(180, MoveInput::default());
    let now = game.vector(&chassis, "Position");
    assert!(
        (now.x - start.x).hypot(now.z - start.z) > 5.0,
        "{start:?} {now:?}"
    );
    // Steering right all the way, it has turned well round.
    let speed = game.vector(&chassis, "AssemblyLinearVelocity");
    assert!(speed.x.hypot(speed.z) > 10.0, "{speed:?}");
    let heading = game.vector(&chassis, "Orientation").y;
    assert!(heading.abs() > 45.0, "{heading}");
    // Nobody is in the red car, so it settled where it was and holds.
    let red = game.vector(&["Car", "Chassis"], "Position");
    assert!((red.x - parked.x).abs() < 0.1 && (red.z - parked.z).abs() < 0.1);
}

#[test]
fn walking_into_the_red_cars_seat_drives_it_up_the_ramp_and_jump_gets_out_where_it_stopped() {
    let mut game = Game::start();
    let controller = game.find(&["Character", "ThirdPersonController"]);
    game.frames(30, MoveInput::default());

    // From behind the car's left side, walk forward then right into the seat.
    let seat = game.vector(&["Car", "DriverSeat"], "Position");
    let walk = |stick| MoveInput {
        stick,
        ..MoveInput::default()
    };
    for _ in 0..240 {
        let body = game.vector(&["Character", "Body"], "Position");
        let stick = if body.z > seat.z + 0.2 {
            [0.0, 1.0]
        } else {
            [1.0, 0.0]
        };
        game.frame(walk(stick));
        if game.characters.is_seated(controller) {
            break;
        }
    }
    assert!(game.characters.is_seated(controller));
    let seat_id = game.find(&["Car", "DriverSeat"]);
    let occupant = game.world.get(seat_id).unwrap().property("Occupant");
    assert_eq!(occupant, Some(&Value::Instance(Some(controller))));

    // W drives it forward (-Z) and up the 10° ramp onto the platform; the
    // camera stays behind the car.
    let chassis = ["Car", "Chassis"];
    let forward = MoveInput {
        forward: true,
        ..MoveInput::default()
    };
    for _ in 0..240 {
        game.frame(forward);
        if game.vector(&chassis, "Position").z < -44.0 {
            break;
        }
    }
    let up = game.vector(&chassis, "Position");
    assert!(up.z < -44.0 && up.y > 4.0, "on the platform: {up:?}");
    let view = game.characters.view(&game.world).unwrap();
    assert!(view.position.z > up.z + 3.0, "behind it: {view:?} {up:?}");

    // S brakes it.
    let back = MoveInput {
        back: true,
        ..MoveInput::default()
    };
    for _ in 0..120 {
        game.frame(back);
        let v = game.vector(&chassis, "AssemblyLinearVelocity");
        if v.x.hypot(v.z) < 1.0 {
            break;
        }
    }
    let v = game.vector(&chassis, "AssemblyLinearVelocity");
    assert!(v.x.hypot(v.z) < 1.0, "{v:?}");

    // Jump gets out; the car stays where it stopped, and the camera is
    // the character's again.
    let jump = MoveInput {
        jump: true,
        ..MoveInput::default()
    };
    game.frame(jump);
    assert!(!game.characters.is_seated(controller));
    let stopped = game.vector(&chassis, "Position");
    game.frames(120, MoveInput::default());
    let after = game.vector(&chassis, "Position");
    assert!(distance(stopped, after) < 0.3, "{stopped:?} {after:?}");
    let body = game.vector(&["Character", "Body"], "Position");
    assert!(distance(body, after) > 3.0, "it got out: {body:?}");
    let view = game.characters.view(&game.world).unwrap();
    assert!(distance(view.position, body) < 20.0, "{view:?} {body:?}");
    // Walking now moves the character, not the car.
    game.frames(30, forward);
    let walked = game.vector(&["Character", "Body"], "Position");
    assert!(distance(walked, body) > 3.0, "{body:?} {walked:?}");
    assert!(distance(game.vector(&chassis, "Position"), after) < 0.3);
}
