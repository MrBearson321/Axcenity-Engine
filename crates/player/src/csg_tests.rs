//! Phase 50: hello-csg's Doorway, a wall the editor unioned with a negated
//! block, lets a character walk through with Precise fidelity; its script
//! scoops a crater out of the ground at runtime with `SubtractAsync`; and
//! the union saves, reloads and packs into a built game.

use axcenity_core::{ClassName, Value, Vector3};

use super::tests::TempCopy;
use super::*;

const FRAME: f32 = 1.0 / 60.0;

fn make(
    world: &mut World,
    class: ClassName,
    name: &str,
    parent: InstanceId,
    properties: Vec<(&str, Value)>,
) -> InstanceId {
    let id = world.create(class).unwrap();
    world.set_name(id, name).unwrap();
    for (property, value) in properties {
        world.set_property(id, property, value).unwrap();
    }
    world.set_parent(id, Some(parent)).unwrap();
    id
}

fn v(x: f32, y: f32, z: f32) -> Value {
    Value::Vector3(Vector3::new(x, y, z))
}

fn vector(world: &World, id: InstanceId, name: &str) -> Vector3 {
    world
        .get(id)
        .unwrap()
        .property(name)
        .and_then(Value::as_vector3)
        .unwrap()
}

/// A copy of hello-csg to open, in a folder of its own.
fn example_copy(tag: &str) -> TempCopy {
    let example = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-csg");
    TempCopy::of(
        &example,
        &format!(
            "axcenity-csg-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ),
    )
}

/// The player's frame, from the controllers through scripts and physics.
struct Game {
    world: World,
    scripts: ScriptRuntime,
    assets: Assets,
    characters: Controllers,
    physics: Physics,
    printed: Vec<String>,
}

impl Game {
    fn new(world: World, assets: Assets) -> Self {
        let mut scripts = axcenity_sim::script_runtime();
        scripts.set_assets(assets.clone());
        Self {
            world,
            scripts,
            physics: Physics::with_assets(assets.clone()),
            assets,
            characters: Controllers::default(),
            printed: Vec::new(),
        }
    }

    fn run(&mut self, source: &str) {
        let script = self.world.create(ClassName::Script).unwrap();
        let service = self.world.server_script_service();
        self.world.set_parent(script, Some(service)).unwrap();
        self.scripts
            .start_script(&mut self.world, script, "scripts/Test.server.lua", source);
        self.take_printed();
    }

    fn frame(&mut self) {
        let seconds = f64::from(FRAME);
        self.characters
            .step(&mut self.world, &self.assets, MoveInput::default(), FRAME);
        axcenity_gameplay::step_humanoids(
            &mut self.scripts,
            &mut self.world,
            &self.characters,
            seconds,
        );
        self.scripts.step(&mut self.world, seconds);
        self.characters.keep_kinematic(&mut self.world);
        self.physics.step(&mut self.world, seconds);
        self.take_printed();
    }

    fn frames(&mut self, count: usize) {
        for _ in 0..count {
            self.frame();
        }
    }

    fn take_printed(&mut self) {
        for line in self.scripts.take_output() {
            assert!(
                !matches!(line.kind, OutputKind::Error),
                "script error: {}",
                line.text
            );
            self.printed.push(line.text);
        }
    }
}

/// hello-csg's world, with the crater script left out, a character that
/// takes the (empty) player input far away, and one in front of the
/// Doorway with a Humanoid, facing it.
fn doorway_world(folder: &std::path::Path, fidelity: &str) -> (World, InstanceId) {
    let mut world = axcenity_project::load(folder).unwrap().world;
    let doorway = world
        .find_first_child(world.workspace(), "Doorway")
        .unwrap();
    world
        .set_property(
            doorway,
            "CollisionFidelity",
            Value::EnumItem(axcenity_core::EnumItem {
                enum_name: "CollisionFidelity".into(),
                item: fidelity.into(),
            }),
        )
        .unwrap();
    let workspace = world.workspace();
    for (name, x, z) in [("Player", -30.0, 30.0), ("Walker", 0.0, 6.0)] {
        let model = make(&mut world, ClassName::Model, name, workspace, Vec::new());
        make(
            &mut world,
            ClassName::Part,
            "Body",
            model,
            vec![("Size", v(2.0, 5.0, 1.0)), ("Position", v(x, 2.5, z))],
        );
        make(
            &mut world,
            ClassName::ThirdPersonController,
            "ThirdPersonController",
            model,
            Vec::new(),
        );
    }
    let walker = world.find_first_child(workspace, "Walker").unwrap();
    make(
        &mut world,
        ClassName::Humanoid,
        "Humanoid",
        walker,
        Vec::new(),
    );
    let body = world.find_first_child(walker, "Body").unwrap();
    (world, body)
}

/// Walks the Walker toward -Z, through the wall at z = 0, for 2.5 s, and
/// says where its body ends up.
fn walk_through(fidelity: &str) -> Vector3 {
    let folder = example_copy(&format!("walk-{fidelity}"));
    let (world, body) = doorway_world(&folder, fidelity);
    let mut game = Game::new(world, Assets::new(&folder));
    game.run("workspace.Walker.Humanoid:Move(Vector3.new(0, 0, -1))");
    game.frames(150);
    let at = vector(&game.world, body, "Position");
    let _ = fs::remove_dir_all(&folder);
    at
}

#[test]
fn a_character_walks_through_the_doorway_unioned_into_a_wall() {
    let at = walk_through("PreciseConvexDecomposition");
    assert!(at.z < -6.0, "it walked through the doorway: {at:?}");
    assert!(at.x.abs() < 1.0, "straight through: {at:?}");
    // With the hull, the doorway is filled in and the wall stops it.
    let blocked = walk_through("Hull");
    assert!(blocked.z > 0.0, "the hull blocks the way: {blocked:?}");
}

#[test]
fn subtract_async_scoops_a_crater_that_the_marble_rolls_into() {
    let folder = example_copy("crater");
    let project = axcenity_project::load(&folder).unwrap();
    let mut world = project.world;
    let assets = Assets::new(&folder);
    let mut game = {
        let mut scripts = axcenity_sim::script_runtime();
        scripts.set_assets(assets.clone());
        start_scripts(&mut scripts, &mut world, &project.scripts);
        Game {
            world,
            scripts,
            physics: Physics::with_assets(assets.clone()),
            assets,
            characters: Controllers::default(),
            printed: Vec::new(),
        }
    };
    // The boolean (and its precise collider) runs on its own thread while
    // frames go on; test frames take no time, so they wait a little each.
    let started = std::time::Instant::now();
    let mut frames = 0;
    while game.printed.is_empty() && started.elapsed().as_secs() < 60 {
        game.frame();
        frames += 1;
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(game.printed, ["made the crater"], "after {frames} frames");
    assert!(frames > 1, "the script went on waiting while frames ran");

    let workspace = game.world.workspace();
    let crater = game.world.find_first_child(workspace, "Crater").unwrap();
    let instance = game.world.get(crater).unwrap();
    assert_eq!(instance.class(), ClassName::UnionOperation);
    let size = vector(&game.world, crater, "Size");
    assert!(
        (size.x - 16.0).abs() < 1e-3 && (size.y - 4.0).abs() < 1e-3,
        "the ground's box: {size:?}"
    );
    assert!(game.world.find_first_child(workspace, "Ground").is_none());
    assert!(game.world.find_first_child(workspace, "Scoop").is_none());
    let mesh_id = instance.property("MeshId").and_then(Value::as_str).unwrap();
    let model = game
        .assets
        .model_blocking(mesh_id)
        .expect("its model is in the assets");
    // A dish: the middle of the top is gone.
    assert!(
        model
            .primitives
            .iter()
            .map(|p| p.indices.len())
            .sum::<usize>()
            > 36
    );

    // The marble falls in and settles at the bottom, below the ground's
    // top (y = 4) and above where the ball's bottom was (y = 0).
    let marble = game.world.find_first_child(workspace, "Marble").unwrap();
    game.frames(360);
    let at = vector(&game.world, marble, "Position");
    assert!(
        at.y < 3.0 && at.y > 0.4,
        "the marble rests in the crater: {at:?}"
    );
    assert!(
        (at.x - 24.0).hypot(at.z) < 3.5,
        "near the crater's middle: {at:?}"
    );
    let _ = fs::remove_dir_all(&folder);
}

#[test]
fn a_union_saves_reloads_and_packs_into_a_built_game() {
    let folder = example_copy("pack");
    let world = axcenity_project::load(&folder).unwrap().world;
    let names = |world: &World| {
        let doorway = world
            .find_first_child(world.workspace(), "Doorway")
            .unwrap();
        let instance = world.get(doorway).unwrap();
        let children: Vec<(String, ClassName)> = instance
            .children()
            .iter()
            .map(|&id| {
                let child = world.get(id).unwrap();
                (child.name().to_owned(), child.class())
            })
            .collect();
        (
            instance.class(),
            instance.property("MeshId").cloned(),
            instance.property("CollisionFidelity").cloned(),
            children,
        )
    };
    let loaded = names(&world);
    assert_eq!(loaded.0, ClassName::UnionOperation);
    assert_eq!(
        loaded.3,
        [
            ("Wall".to_owned(), ClassName::Part),
            ("Door".to_owned(), ClassName::NegateOperation)
        ]
    );

    // Saved and loaded again, it's the same.
    axcenity_project::save_world(&folder, &world).unwrap();
    let reloaded = axcenity_project::load(&folder).unwrap().world;
    assert_eq!(names(&reloaded), loaded);

    // Packed and unpacked, its model comes along and loads.
    let packed = axcenity_project::pack_game(&folder).unwrap();
    let built = folder.with_extension("built");
    let _ = fs::remove_dir_all(&built);
    axcenity_project::unpack_game(&packed, &built).unwrap();
    let game = axcenity_project::load(&built).unwrap().world;
    assert_eq!(names(&game), loaded);
    let Some(Value::String(mesh_id)) = &loaded.1 else {
        panic!("a MeshId");
    };
    let model = Assets::new(&built)
        .model_blocking(mesh_id)
        .expect("the union's model");
    let size = model.size;
    assert!(
        (size[0] - 12.0).abs() < 1e-3 && (size[1] - 8.0).abs() < 1e-3,
        "{size:?}"
    );
    let _ = fs::remove_dir_all(&folder);
    let _ = fs::remove_dir_all(&built);
}
