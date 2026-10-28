//! Phase 39: the hello-seat example's chair, disabled bench and cart.

use axcenity_core::{InstanceId, KeyCode as K, Value, Vector3};
use axcenity_script::KeyEvent;

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
    fn frame(&mut self, keys: &[KeyEvent], input: MoveInput) {
        let seconds = 1.0 / 60.0;
        self.characters
            .step(&mut self.world, &self.assets, input, seconds);
        fire_seated(&mut self.scripts, &mut self.world, &mut self.characters);
        self.scripts.input(&mut self.world, keys);
        self.scripts.step(&mut self.world, f64::from(seconds));
        self.characters.keep_kinematic(&mut self.world);
        self.physics.step(&mut self.world, f64::from(seconds));
        self.scripts.heartbeat(&mut self.world, f64::from(seconds));
        self.printed
            .extend(self.scripts.take_output().into_iter().map(|line| line.text));
    }

    fn frames(&mut self, count: usize, input: MoveInput) {
        for _ in 0..count {
            self.frame(&[], input);
        }
    }

    fn press(&mut self, key: K) {
        self.frame(&[KeyEvent { key, pressed: true }], MoveInput::default());
        self.frame(
            &[KeyEvent {
                key,
                pressed: false,
            }],
            MoveInput::default(),
        );
    }

    fn find(&self, path: &[&str]) -> InstanceId {
        path.iter().fold(self.world.workspace(), |at, name| {
            self.world.find_first_child(at, name).unwrap()
        })
    }

    fn position(&self, path: &[&str]) -> Vector3 {
        self.world
            .get(self.find(path))
            .unwrap()
            .property("Position")
            .and_then(Value::as_vector3)
            .unwrap()
    }

    fn occupant(&self, seat: &[&str]) -> Option<InstanceId> {
        self.world
            .get(self.find(seat))
            .unwrap()
            .property("Occupant")
            .and_then(Value::as_instance)
    }
}

/// Walks toward the part at `path`, until it sits or three seconds are up.
fn walk_toward(game: &mut Game, path: &[&str]) {
    let toward = game.position(path);
    let body = game.position(&["Character", "Body"]);
    let (dx, dz) = (toward.x - body.x, toward.z - body.z);
    let len = dx.hypot(dz);
    // The camera faces -Z at yaw 0; walking forward goes the way it faces.
    let stick = [dx / len, -dz / len];
    let walk = MoveInput {
        stick,
        ..MoveInput::default()
    };
    let controller = game.find(&["Character", "ThirdPersonController"]);
    for _ in 0..180 {
        game.frame(&[], walk);
        if game.characters.is_seated(controller) {
            return;
        }
    }
}

#[test]
fn hello_seat_sits_on_the_chair_bench_and_cart_and_stands_up_beside_them() {
    let (world, scripts) = start_example("hello-seat");
    let mut game = Game {
        world,
        scripts,
        assets: Assets::default(),
        characters: Controllers::default(),
        physics: Physics::new(),
        printed: Vec::new(),
    };
    let controller = game.find(&["Character", "ThirdPersonController"]);
    game.frames(2, MoveInput::default());

    // Walking into the chair sits on it, its feet on the seat's top.
    walk_toward(&mut game, &["Chair"]);
    assert_eq!(game.occupant(&["Chair"]), Some(controller));
    let body = game.position(&["Character", "Body"]);
    assert!(
        (body.y - 4.5).abs() < 1e-3 && (body.z + 10.0).abs() < 1e-3,
        "{body:?}"
    );
    assert!(
        game.printed.contains(&"Sat on Chair".to_owned()),
        "{:?}",
        game.printed
    );

    // Jump stands it up beside the chair, on its front side, on the floor.
    game.frame(
        &[],
        MoveInput {
            jump: true,
            ..MoveInput::default()
        },
    );
    game.frames(60, MoveInput::default());
    assert_eq!(game.occupant(&["Chair"]), None);
    let body = game.position(&["Character", "Body"]);
    assert!((body.y - 2.5).abs() < 1e-3, "{body:?}");
    assert!(body.z - 0.5 > -9.0 && body.z < -5.0, "{body:?}");
    assert!(game.printed.contains(&"Stood up from Chair".to_owned()));

    // The Disabled bench ignores a walk into it; B sits there from Lua.
    walk_toward(&mut game, &["Bench"]);
    assert_eq!(game.occupant(&["Bench"]), None);
    game.press(K::B);
    game.frames(1, MoveInput::default());
    assert_eq!(game.occupant(&["Bench"]), Some(controller));
    let body = game.position(&["Character", "Body"]);
    assert!(
        (body.x - 12.0).abs() < 1e-3 && (body.y - 4.0).abs() < 1e-3,
        "{body:?}"
    );
    game.press(K::L);
    game.frames(60, MoveInput::default());
    assert_eq!(game.occupant(&["Bench"]), None);

    // The cart's seat starts its drive (a block script): the character
    // rides along for three seconds, then the block stands it up.
    walk_toward(&mut game, &["Cart", "Seat"]);
    assert_eq!(game.occupant(&["Cart", "Seat"]), Some(controller));
    let cart_start = game.position(&["Cart", "Base"]);
    let body_start = game.position(&["Character", "Body"]);
    game.frames(120, MoveInput::default());
    let cart_moved = game.position(&["Cart", "Base"]).x - cart_start.x;
    assert!(cart_moved < -8.0, "the cart moved {cart_moved}");
    let body = game.position(&["Character", "Body"]);
    assert!(
        (body.x - body_start.x - cart_moved).abs() < 0.1 && (body.y - body_start.y).abs() < 0.1,
        "{body_start:?} to {body:?}, the cart by {cart_moved}"
    );
    game.frames(90, MoveInput::default());
    assert_eq!(game.occupant(&["Cart", "Seat"]), None);
    assert!(!game.characters.is_seated(controller));
}
