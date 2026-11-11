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
    folder: super::tests::TempCopy,
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
    let generated = std::fs::read(game.folder.join(path)).unwrap();
    assert_same_pieces(&generated, &saved);
}

/// How far a number in the pieces may be from the saved one. Another
/// computer's maths library rounds the cuts' sines and roots in the last
/// bit (about 1e-6); a different seed or cut moves corners by whole studs.
const PIECES_TOLERANCE: f64 = 1e-4;

/// Splits a `.glb` into its JSON and its binary chunk.
fn glb_chunks(bytes: &[u8]) -> (serde_json::Value, &[u8]) {
    let word = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
    let json_length = word(12);
    let json = serde_json::from_slice(&bytes[20..20 + json_length]).unwrap();
    let binary = 20 + json_length;
    (json, &bytes[binary + 8..binary + 8 + word(binary)])
}

/// The same JSON, with numbers within [`PIECES_TOLERANCE`].
fn assert_same_json(made: &serde_json::Value, saved: &serde_json::Value, at: &str) {
    use serde_json::Value as Json;
    match (made, saved) {
        (Json::Number(a), Json::Number(b)) => {
            let (a, b) = (a.as_f64().unwrap(), b.as_f64().unwrap());
            assert!((a - b).abs() <= PIECES_TOLERANCE, "{at}: {a} ≠ {b}");
        }
        (Json::Array(a), Json::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{at}");
            for (index, (a, b)) in a.iter().zip(b).enumerate() {
                assert_same_json(a, b, &format!("{at}[{index}]"));
            }
        }
        (Json::Object(a), Json::Object(b)) => {
            assert_eq!(
                a.keys().collect::<Vec<_>>(),
                b.keys().collect::<Vec<_>>(),
                "{at}"
            );
            for (key, a) in a {
                assert_same_json(a, &b[key], &format!("{at}.{key}"));
            }
        }
        _ => assert_eq!(made, saved, "{at}"),
    }
}

/// The same pieces: the same glTF structure and the same indices, with
/// every float (corners, normals, UVs) within [`PIECES_TOLERANCE`].
fn assert_same_pieces(made: &[u8], saved: &[u8]) {
    let (made_json, made_binary) = glb_chunks(made);
    let (saved_json, saved_binary) = glb_chunks(saved);
    assert_same_json(&made_json, &saved_json, "glTF");
    assert_eq!(made_binary.len(), saved_binary.len());
    for (index, accessor) in saved_json["accessors"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        let view = &saved_json["bufferViews"][accessor["bufferView"].as_u64().unwrap() as usize];
        let start = view["byteOffset"].as_u64().unwrap_or(0) as usize
            + accessor["byteOffset"].as_u64().unwrap_or(0) as usize;
        let parts = match accessor["type"].as_str().unwrap() {
            "SCALAR" => 1,
            "VEC2" => 2,
            "VEC3" => 3,
            "VEC4" => 4,
            other => panic!("accessor type {other}"),
        };
        let count = accessor["count"].as_u64().unwrap() as usize * parts;
        // 5126 is FLOAT; the rest (indices) are whole numbers.
        let size = match accessor["componentType"].as_u64().unwrap() {
            5126 | 5125 => 4,
            5123 => 2,
            _ => 1,
        };
        let (made, saved) = (
            &made_binary[start..start + count * size],
            &saved_binary[start..start + count * size],
        );
        if accessor["componentType"] != 5126 {
            assert_eq!(made, saved, "accessor {index}");
            continue;
        }
        for (a, b) in made.as_chunks::<4>().0.iter().zip(saved.as_chunks::<4>().0) {
            let (a, b) = (f32::from_le_bytes(*a), f32::from_le_bytes(*b));
            assert!(
                f64::from((a - b).abs()) <= PIECES_TOLERANCE,
                "accessor {index}: {a} ≠ {b}"
            );
        }
    }
}
