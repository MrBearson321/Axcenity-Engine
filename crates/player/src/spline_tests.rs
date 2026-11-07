//! Phase 73: the hello-splines example in the player's frame order. The
//! lift on its PingPong SplineFollower carries the character standing on
//! it, the guard patrols its closed spline with no script, and the Flyby
//! Sequence's camera rides its rail.

use axcenity_core::spline::SplineCurve;
use axcenity_core::{InstanceId, Value, Vector3};

use super::tests::start_example_in;
use super::*;

const FRAME: f32 = 1.0 / 60.0;

struct Game {
    world: World,
    scripts: ScriptRuntime,
    assets: Assets,
    characters: Controllers,
    agents: Agents,
    physics: Physics,
    camera: Option<axcenity_script::GameCamera>,
    printed: Vec<String>,
}

impl Game {
    fn start() -> Self {
        let (world, mut scripts, folder) = start_example_in("hello-splines");
        let assets = Assets::new(&folder);
        scripts.set_assets(assets.clone());
        let mut game = Self {
            world,
            scripts,
            assets,
            characters: Controllers::default(),
            agents: Agents::default(),
            physics: Physics::new(),
            camera: None,
            printed: Vec::new(),
        };
        let printed = game.take_printed();
        game.printed = printed;
        game
    }

    /// One frame: characters, agents, scripts (the followers and sequences
    /// after them), the kinematic hold, physics, then the camera.
    fn frame(&mut self) {
        let seconds = f64::from(FRAME);
        self.characters
            .step(&mut self.world, &self.assets, MoveInput::default(), FRAME);
        self.agents.step(&mut self.world, &self.assets, FRAME);
        self.scripts.step(&mut self.world, seconds);
        self.characters.keep_kinematic(&mut self.world);
        self.agents.keep_kinematic(&mut self.world);
        self.physics.step(&mut self.world, seconds);
        self.scripts.heartbeat(&mut self.world, seconds);
        self.camera = Some(self.scripts.step_camera(&self.world, seconds));
        let printed = self.take_printed();
        self.printed.extend(printed);
    }

    fn take_printed(&mut self) -> Vec<String> {
        self.scripts
            .take_output()
            .into_iter()
            .map(|line| line.text)
            .collect()
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

/// Where each of `spline`'s points is.
fn corners(world: &World, spline: InstanceId) -> Vec<Vector3> {
    world
        .get(spline)
        .unwrap()
        .children()
        .iter()
        .filter_map(|&id| world.get(id)?.property("Position")?.as_vector3())
        .collect()
}

fn sub(a: Vector3, b: Vector3) -> Vector3 {
    Vector3::new(a.x - b.x, a.y - b.y, a.z - b.z)
}

fn length(a: Vector3) -> f32 {
    (a.x * a.x + a.y * a.y + a.z * a.z).sqrt()
}

#[test]
fn the_lift_ping_pongs_along_its_spline_carrying_the_character_smoothly() {
    let mut game = Game::start();
    let path = SplineCurve::of(&game.world, game.find(&["LiftPath"])).unwrap();
    // Speed 8: there in Length / 8 seconds, and back as long again.
    let trip = path.length() / 8.0;
    let frames = ((2.0 * trip + 0.2) / FRAME).ceil() as usize;
    // Standing on the lift, its middle 3 studs above the lift's.
    let standing = sub(
        game.vector(&["Character", "Body"], "Position"),
        game.vector(&["Lift"], "Position"),
    );
    assert_eq!(standing, Vector3::new(0.0, 3.0, 0.0));
    let mut last = game.vector(&["Character", "Body"], "Position");
    let mut highest: f32 = 0.0;
    for frame in 0..frames {
        game.frame();
        let body = game.vector(&["Character", "Body"], "Position");
        let lift = game.vector(&["Lift"], "Position");
        // Standing still on the lift, it keeps its place on it, at most a
        // frame of the lift's travel behind (the controller follows the
        // lift's last move), and never moves further in a frame than the
        // lift does at 8 studs a second.
        let now = sub(body, lift);
        assert!(
            length(sub(now, standing)) < 8.0 * FRAME + 0.02,
            "{frame}: {now:?}"
        );
        assert!(
            length(sub(body, last)) < 8.0 * FRAME * 1.05 + 0.01,
            "{frame}: jumped"
        );
        last = body;
        highest = highest.max(lift.y);
    }
    // It went all the way up and came back down where it started.
    assert!(highest > 6.9, "{highest}");
    let lift = game.vector(&["Lift"], "Position");
    assert!(lift.y < 2.0, "back down: {lift:?}");
    let body = game.vector(&["Character", "Body"], "Position");
    assert!(
        (body.y - (lift.y + 3.0)).abs() < 0.2,
        "on the lift: {body:?} {lift:?}"
    );
    assert!(
        game.printed
            .iter()
            .any(|line| line == "The lift reached the end")
            && game
                .printed
                .iter()
                .any(|line| line == "The lift reached the start"),
        "{:?}",
        game.printed
    );
}

#[test]
fn the_guard_patrols_its_closed_route_and_the_flyby_camera_rides_its_rail() {
    let mut game = Game::start();
    let route = corners(&game.world, game.find(&["PatrolRoute"]));
    let rail = SplineCurve::of(&game.world, game.find(&["CameraRail"])).unwrap();
    let mut visited: Vec<usize> = Vec::new();
    for frame in 0..(14.0 / FRAME) as usize {
        game.frame();
        // The camera rides the rail for the sequence's 6 seconds: at 3,
        // halfway along it, facing along it.
        if frame == (3.0 / FRAME) as usize - 1 {
            let held = game
                .camera
                .and_then(|camera| camera.scripted)
                .expect("the Flyby holds the camera");
            let middle = rail.position_at_distance(rail.length() / 2.0);
            assert!(
                length(sub(held.position, middle)) < 0.5,
                "{held:?} {middle:?}"
            );
            let facing = rail.orientation_at_distance(rail.length() / 2.0);
            assert!(
                (held.orientation.y - facing.y).abs() < 2.0,
                "{held:?} {facing:?}"
            );
        }
        let feet = game.vector(&["Guard", "Body"], "Position");
        if let Some(i) = route
            .iter()
            .position(|p| (p.x - feet.x).hypot(p.z - feet.z) < 0.5)
            && visited.last() != Some(&i)
        {
            visited.push(i);
        }
    }
    // Then the camera is the game's again.
    assert!(game.camera.and_then(|camera| camera.scripted).is_none());
    // 16 + 24 + 16 + 24 studs at 10 a second: once round and back to the
    // first corner, in order.
    assert!(visited.len() >= 5, "{visited:?}");
    for pair in visited.windows(2) {
        assert_eq!(pair[1], (pair[0] + 1) % route.len(), "{visited:?}");
    }
}
