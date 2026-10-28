//! Phase 42: hello-destruction's glass pane shatters when the thrown crate
//! hits it, and its shelf holds the light crate and breaks under the heavy
//! one.

use axcenity_core::Value;

use super::tests::start_example_in;
use super::*;

const FRAME: f64 = 1.0 / 60.0;

/// hello-destruction running through the frame loop's physics steps:
/// scripts, their shatters, physics, then `Touched`, `Broken` and
/// `Shattered`, collecting what scripts print.
struct Game {
    world: World,
    scripts: ScriptRuntime,
    physics: Physics,
    clock: u32,
    printed: Vec<String>,
    folder: PathBuf,
}

impl Game {
    fn start() -> Self {
        let (world, mut scripts, folder) = start_example_in("hello-destruction");
        let assets = Assets::new(&folder);
        scripts.set_assets(assets.clone());
        Self {
            world,
            scripts,
            physics: Physics::with_assets(assets),
            clock: 0,
            printed: Vec::new(),
            folder,
        }
    }

    fn run_until(&mut self, seconds: f64) {
        while f64::from(self.clock) * FRAME < seconds {
            let world = &mut self.world;
            self.scripts.step(world, FRAME);
            self.scripts.apply_shatters(world, &mut self.physics);
            self.physics.step(world, FRAME);
            let touches = self.physics.take_touches();
            self.scripts.fire_touched(world, &touches);
            self.scripts.fire_breaks(world, &mut self.physics);
            self.scripts.heartbeat(world, FRAME);
            for line in self.scripts.take_output() {
                assert!(line.kind != OutputKind::Error, "{}", line.text);
                self.printed.push(line.text);
            }
            self.clock += 1;
        }
    }

    fn find(&self, name: &str) -> Option<InstanceId> {
        self.world.find_first_child(self.world.workspace(), name)
    }

    fn said(&self, text: &str) -> usize {
        self.printed
            .iter()
            .filter(|line| line.contains(text))
            .count()
    }
}

#[test]
fn the_thrown_crate_shatters_the_glass_pane_into_its_30_pieces() {
    let mut game = Game::start();
    game.run_until(0.9);
    assert!(game.find("GlassPane").is_some());
    assert_eq!(game.said("shattered"), 0);
    // The crate is thrown at 80 studs a second at 1 s, from 12 studs away.
    game.run_until(1.5);
    assert!(game.find("GlassPane").is_none(), "{:?}", game.printed);
    assert_eq!(
        game.said("the pane shattered into 30 pieces"),
        1,
        "{:?}",
        game.printed
    );
    let pieces = game.find("GlassPanePieces").unwrap();
    let pieces = game.world.get(pieces).unwrap().children().to_vec();
    assert_eq!(pieces.len(), 30);
    // The crate went on through.
    let crate_ = game.find("Crate").unwrap();
    let z = game.world.get(crate_).unwrap().property("Position");
    assert!(z.and_then(Value::as_vector3).unwrap().z < -12.0);
    // Every piece is a body flying or fallen, none stuck in mid-air.
    game.run_until(3.0);
    for piece in pieces {
        assert!(game.physics.has_body(piece));
        let at = game.world.get(piece).unwrap().property("Position");
        assert!(at.and_then(Value::as_vector3).unwrap().y < 4.0);
    }
    assert_eq!(game.said("shattered"), 1);
}

#[test]
fn the_shelf_holds_the_light_crate_and_breaks_under_the_heavy_one_once() {
    let mut game = Game::start();
    game.run_until(2.9);
    assert_eq!(game.said("the shelf broke"), 0);
    let shelf = game.find("Shelf").unwrap();
    let height = |game: &Game| {
        let at = game.world.get(shelf).unwrap().property("Position");
        at.and_then(Value::as_vector3).unwrap().y
    };
    assert!((height(&game) - 4.0).abs() < 0.1);
    // The heavy crate drops at 3 s.
    game.run_until(6.0);
    assert_eq!(game.said("the shelf broke"), 1);
    let bracket = game.world.find_first_child(shelf, "Bracket").unwrap();
    let enabled = game.world.get(bracket).unwrap().property("Enabled");
    assert_eq!(enabled, Some(&Value::Bool(false)));
    assert!(height(&game) < 2.0);
}

#[test]
fn the_pieces_fade_out_and_are_gone_after_the_debris_lifetime() {
    let mut game = Game::start();
    game.run_until(1.5);
    let pieces = game.find("GlassPanePieces").unwrap();
    let first = game.world.get(pieces).unwrap().children()[0];
    let transparency = |game: &Game| {
        game.world
            .get(first)
            .and_then(|piece| piece.property("Transparency"))
            .and_then(Value::as_number)
    };
    let start = transparency(&game).unwrap();
    // It shattered about 1.15 s in, and DebrisLifetime is 4: they fade
    // over the last second.
    game.run_until(4.0);
    assert_eq!(transparency(&game), Some(start));
    game.run_until(4.6);
    let fading = transparency(&game).unwrap();
    assert!(fading > start && fading < 1.0, "{start} → {fading}");
    game.run_until(5.4);
    assert!(!game.world.contains(pieces));
    assert!(!game.world.contains(first));
    assert!(game.physics.debris().is_empty());
}

#[test]
fn the_examples_pieces_are_what_generate_makes_from_the_same_seed() {
    let game = Game::start();
    let pane = game.find("GlassPane").unwrap();
    let fracture = game.world.find_first_child(pane, "Fracture").unwrap();
    let path = "assets/models/GlassPane.fracture.glb";
    let saved = std::fs::read(game.folder.join(path)).unwrap();
    let assets = Assets::new(&game.folder);
    let made = axcenity_mesh_edit::generate(&game.world, &assets, &game.folder, fracture);
    assert_eq!(made.as_deref(), Ok(path));
    assert_eq!(std::fs::read(game.folder.join(path)).unwrap(), saved);
}
