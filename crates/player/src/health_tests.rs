//! Phase 43: the hello-health example's lava, regeneration, fall damage
//! and health bars, in the player's frame order.

use axcenity_core::{InstanceId, Value, Vector3};

use super::tests::start_example;
use super::*;

const FRAME: f32 = 1.0 / 60.0;

/// The example running in the player's frame order: the character (and
/// `Landed`), scripts, the kinematic hold, physics, then health.
struct Game {
    world: World,
    scripts: ScriptRuntime,
    assets: Assets,
    characters: Controllers,
    physics: Physics,
    printed: Vec<String>,
    /// Every landing so far, with its speed.
    landings: Vec<f32>,
}

impl Game {
    fn start() -> Self {
        let (world, scripts) = start_example("hello-health");
        Self {
            world,
            scripts,
            assets: Assets::default(),
            characters: Controllers::default(),
            physics: Physics::new(),
            printed: Vec::new(),
            landings: Vec::new(),
        }
    }

    fn frame(&mut self, input: MoveInput) {
        self.characters
            .step(&mut self.world, &self.assets, input, FRAME);
        let landings = self.characters.take_landings();
        self.landings
            .extend(landings.iter().map(|&(_, speed)| speed));
        self.scripts.fire_landed(&mut self.world, &landings);
        self.scripts.step(&mut self.world, f64::from(FRAME));
        self.characters.keep_kinematic(&mut self.world);
        self.physics.step(&mut self.world, f64::from(FRAME));
        self.scripts.step_health(&mut self.world, f64::from(FRAME));
        self.scripts.heartbeat(&mut self.world, f64::from(FRAME));
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

    fn number(&self, path: &[&str], name: &str) -> f64 {
        self.world
            .get(self.find(path))
            .unwrap()
            .property(name)
            .and_then(Value::as_number)
            .unwrap()
    }

    fn health(&self) -> f64 {
        self.number(&["Character", "Health"], "Health")
    }

    fn body_x(&self) -> f32 {
        self.world
            .get(self.find(&["Character", "Body"]))
            .unwrap()
            .property("Position")
            .and_then(Value::as_vector3)
            .unwrap()
            .x
    }

    /// Walks along X (the camera faces -Z) until `arrived` says so.
    fn walk_until(&mut self, right: bool, arrived: impl Fn(f32) -> bool) {
        let walk = MoveInput {
            stick: [if right { 1.0 } else { -1.0 }, 0.0],
            ..MoveInput::default()
        };
        for _ in 0..300 {
            if arrived(self.body_x()) {
                return;
            }
            self.frame(walk);
        }
        panic!("never got there: x = {}", self.body_x());
    }
}

#[test]
fn standing_in_the_lava_lowers_health_and_leaving_regenerates_after_regen_delay() {
    let mut game = Game::start();
    game.frames(2, MoveInput::default());
    assert_eq!(game.health(), 100.0);

    // Into the lava (x 10 to 18), and stand there for a second: 20 a second.
    game.walk_until(true, |x| x >= 14.0);
    let entered = game.health();
    assert!(entered < 100.0, "{entered}");
    game.frames(60, MoveInput::default());
    let burnt = game.health();
    assert!((entered - burnt - 20.0).abs() < 0.5, "{entered} → {burnt}");

    // Out of it, nothing comes back for RegenDelay (2 s) after the last
    // burn, then 10 a second.
    let back = MoveInput {
        stick: [-1.0, 0.0],
        ..MoveInput::default()
    };
    let mut waited = 0;
    let mut last = game.health();
    loop {
        let out = game.body_x() <= 6.0;
        game.frame(if out { MoveInput::default() } else { back });
        let now = game.health();
        if now < last {
            waited = 0;
        } else if now > last {
            break;
        } else {
            waited += 1;
        }
        last = now;
        assert!(waited < 300, "never regenerated");
    }
    assert!((118..=121).contains(&waited), "{waited}");
    let left = game.health();
    game.frames(60, MoveInput::default());
    let regenerated = game.health() - left;
    assert!((regenerated - 10.0).abs() < 0.2, "{regenerated}");

    // Both bars show it, with no script.
    for bar in [
        &["Character", "Body", "HealthBillboard", "HealthBar"][..],
        &["HealthHud", "HealthBar"][..],
    ] {
        assert_eq!(game.number(bar, "Value"), game.health());
        assert_eq!(game.number(bar, "MaxValue"), 100.0);
    }
}

#[test]
fn a_fall_above_the_threshold_damages_the_character_in_proportion_to_its_speed() {
    let mut game = Game::start();
    game.frames(2, MoveInput::default());
    let body = game.find(&["Character", "Body"]);
    let health = game.find(&["Character", "Health"]);
    let mut lost = Vec::new();
    for height in [6.0, 20.0, 40.0] {
        game.world
            .set_property(health, "Health", Value::Number(100.0))
            .unwrap();
        game.world
            .set_property(
                body,
                "Position",
                Value::Vector3(Vector3::new(-30.0, 2.5 + height, 0.0)),
            )
            .unwrap();
        game.landings.clear();
        for _ in 0..150 {
            game.frame(MoveInput::default());
            if !game.landings.is_empty() {
                break;
            }
        }
        let [speed] = game.landings[..] else {
            panic!("{:?}", game.landings);
        };
        lost.push((f64::from(speed), 100.0 - game.health()));
    }
    // A short drop lands under the threshold (60) and costs nothing.
    assert!(lost[0].0 < 60.0 && lost[0].1 == 0.0, "{lost:?}");
    // Higher falls land faster, and lose FallDamageScale (1) for each stud
    // a second over the threshold.
    assert!(lost[2].0 > lost[1].0 && lost[1].0 > 60.0, "{lost:?}");
    for &(speed, damage) in &lost[1..] {
        assert!((damage - (speed - 60.0)).abs() < 1e-6, "{lost:?}");
    }
    assert!(
        game.printed
            .contains(&format!("Landed at {} studs a second", lost[2].0 as i64)),
        "{:?}",
        game.printed
    );
}
