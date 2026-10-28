//! Phase 46: Humanoids over their pieces, in the player's frame order with
//! physics and Animators.

use std::path::Path;

use axcenity_anim::ragdoll;
use axcenity_core::{ClassName, Value, Vector3};

use super::*;

const FRAME: f32 = 1.0 / 60.0;

/// An Instance made with these properties, in `parent`.
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

/// A 60-stud floor.
fn floor(world: &mut World) {
    let workspace = world.workspace();
    make(
        world,
        ClassName::Part,
        "Floor",
        workspace,
        vec![
            ("Size", v(60.0, 1.0, 60.0)),
            ("Position", v(0.0, -0.5, 0.0)),
            ("Anchored", Value::Bool(true)),
        ],
    );
}

/// A Model named `name` with a 2×5×1 Body standing at (x, z), and a
/// ThirdPersonController. Returns the Model and its controller.
fn character(world: &mut World, name: &str, x: f32, z: f32) -> (InstanceId, InstanceId) {
    let workspace = world.workspace();
    let model = make(world, ClassName::Model, name, workspace, Vec::new());
    make(
        world,
        ClassName::Part,
        "Body",
        model,
        vec![("Size", v(2.0, 5.0, 1.0)), ("Position", v(x, 2.5, z))],
    );
    let controller = make(
        world,
        ClassName::ThirdPersonController,
        "ThirdPersonController",
        model,
        Vec::new(),
    );
    (model, controller)
}

/// The player's frame, from the controllers to the Animators.
struct Game {
    world: World,
    scripts: ScriptRuntime,
    assets: Assets,
    characters: Controllers,
    physics: Physics,
    animators: Animators,
    printed: Vec<String>,
}

impl Game {
    fn new(world: World, assets: Assets) -> Self {
        let mut scripts = ScriptRuntime::new();
        scripts.set_assets(assets.clone());
        Self {
            world,
            scripts,
            physics: Physics::with_assets(assets.clone()),
            animators: Animators::new(),
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
        let landings = self.characters.take_landings();
        self.scripts.fire_landed(&mut self.world, &landings);
        self.scripts
            .step_humanoids(&mut self.world, &self.characters, seconds);
        self.scripts.step(&mut self.world, seconds);
        self.characters.keep_kinematic(&mut self.world);
        self.physics.step(&mut self.world, seconds);
        self.scripts.step_health(&mut self.world, seconds);
        self.animators
            .update(&mut self.world, &self.assets, seconds);
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
            self.printed.push(line.text.replace('\t', " "));
        }
    }

    fn count(&self, line: &str) -> usize {
        self.printed
            .iter()
            .filter(|printed| *printed == line)
            .count()
    }
}

#[test]
fn walk_speed_is_one_value_on_the_humanoid_and_the_controller() {
    let mut world = World::new();
    floor(&mut world);
    let (model, controller) = character(&mut world, "Hero", 0.0, 0.0);
    let humanoid = make(
        &mut world,
        ClassName::Humanoid,
        "Humanoid",
        model,
        Vec::new(),
    );
    let mut game = Game::new(world, Assets::default());
    game.run(
        r#"
        local hero = workspace.Hero
        local humanoid = hero.Humanoid
        humanoid.WalkSpeed = 20
        print("controller", hero.ThirdPersonController.WalkSpeed)
        hero.ThirdPersonController.WalkSpeed = 12
        print("humanoid", humanoid.WalkSpeed)
        hero.Health.HealthChanged:Connect(function(new, old) print("changed", new, old) end)
        humanoid.Health = 40
        print("health", hero.Health.Health, humanoid.MaxHealth)
        "#,
    );
    // HealthChanged fires once the script has run, as a Health's own does.
    assert_eq!(
        game.printed,
        [
            "controller 20",
            "humanoid 12",
            "health 40 100",
            "changed 40 100"
        ]
    );
    let world = &game.world;
    assert_eq!(
        world.get(controller).unwrap().property("WalkSpeed"),
        Some(&Value::Number(12.0))
    );
    // The Humanoid itself keeps no copy.
    let own = world.get(humanoid).unwrap();
    assert!(own.property("WalkSpeed").is_none() && own.property("Health").is_none());
    // It made the pieces the Model lacked, once.
    let classes: Vec<ClassName> = world
        .get(model)
        .unwrap()
        .children()
        .iter()
        .map(|&child| world.get(child).unwrap().class())
        .collect();
    for class in [
        ClassName::ThirdPersonController,
        ClassName::Health,
        ClassName::Inventory,
        ClassName::Animator,
    ] {
        assert_eq!(
            classes.iter().filter(|&&c| c == class).count(),
            1,
            "{class}"
        );
    }
}

#[test]
fn an_npc_with_a_humanoid_walks_round_a_wall_to_a_move_to_point() {
    let mut world = World::new();
    floor(&mut world);
    let workspace = world.workspace();
    // The player's character, first, receives the (empty) input.
    character(&mut world, "Player", -20.0, 20.0);
    let (npc, _) = character(&mut world, "NPC", 0.0, 10.0);
    make(&mut world, ClassName::Humanoid, "Humanoid", npc, Vec::new());
    // A wall across the straight line: only a path gets round it.
    make(
        &mut world,
        ClassName::Part,
        "Wall",
        workspace,
        vec![
            ("Size", v(12.0, 8.0, 1.0)),
            ("Position", v(0.0, 4.0, 0.0)),
            ("Anchored", Value::Bool(true)),
        ],
    );
    let mut game = Game::new(world, Assets::default());
    game.run(
        r#"
        local humanoid = workspace.NPC.Humanoid
        humanoid.MoveToFinished:Connect(function(reached) print("finished", reached) end)
        humanoid:MoveTo(Vector3.new(0, 0, -10))
        "#,
    );
    let body = game.world.find_first_child(npc, "Body").unwrap();
    let mut frames = 0;
    while game.printed.is_empty() && frames < 600 {
        game.frame();
        frames += 1;
    }
    assert_eq!(game.printed, ["finished true"], "after {frames} frames");
    let at = vector(&game.world, body, "Position");
    assert!(
        (at.x).hypot(at.z + 10.0) <= 0.6,
        "it stands at the point: {at:?}"
    );
    // It went round: 20 studs straight would take 1.25 s at 16 a second.
    assert!(frames > 80, "{frames} frames");
    // Arrived, it stays.
    game.frames(30);
    assert_eq!(game.printed, ["finished true"]);
    let still = vector(&game.world, body, "Position");
    assert!((still.x - at.x).abs() < 1e-3 && (still.z - at.z).abs() < 1e-3);
}

#[test]
fn a_jump_goes_through_jumping_freefall_and_landed_to_running() {
    let mut world = World::new();
    floor(&mut world);
    character(&mut world, "Player", -20.0, 20.0);
    let (npc, _) = character(&mut world, "NPC", 0.0, 0.0);
    make(&mut world, ClassName::Humanoid, "Humanoid", npc, Vec::new());
    let mut game = Game::new(world, Assets::default());
    game.run(
        r#"
        local humanoid = workspace.NPC.Humanoid
        humanoid.StateChanged:Connect(function(old, new) print(old.Name, new.Name) end)
        "#,
    );
    // It settles first; that isn't a landing.
    game.frames(10);
    assert!(game.printed.is_empty(), "{:?}", game.printed);
    game.run("workspace.NPC.Humanoid:Jump()");
    game.frames(120);
    assert_eq!(
        game.printed,
        [
            "Running Jumping",
            "Jumping Freefall",
            "Freefall Landed",
            "Landed Running"
        ]
    );
}

/// A floor and hello-character's Mixamo character on it, with a Ragdoll
/// built, a Health and a Humanoid with RagdollOnDeath.
fn dummy() -> (Game, InstanceId, InstanceId, InstanceId) {
    let assets =
        Assets::new(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-character"));
    let mut world = World::new();
    floor(&mut world);
    let workspace = world.workspace();
    let model = make(&mut world, ClassName::Model, "Dummy", workspace, Vec::new());
    let mesh = make(
        &mut world,
        ClassName::MeshPart,
        "Body",
        model,
        vec![
            (
                "MeshId",
                Value::String("assets/models/Walking/Walking.fbx".into()),
            ),
            ("Size", v(3.8939824, 3.784847, 0.7229666)),
            ("Position", v(0.0, 1.8924235, 0.0)),
            ("Anchored", Value::Bool(true)),
        ],
    );
    let ragdoll = make(&mut world, ClassName::Ragdoll, "Ragdoll", mesh, Vec::new());
    ragdoll::build(&mut world, &assets, ragdoll).unwrap();
    let humanoid = make(
        &mut world,
        ClassName::Humanoid,
        "Humanoid",
        model,
        vec![("RagdollOnDeath", Value::Bool(true))],
    );
    (Game::new(world, assets), model, ragdoll, humanoid)
}

#[test]
fn killing_a_character_with_ragdoll_on_death_ragdolls_it_and_fires_died_once() {
    let (mut game, _model, ragdoll, _humanoid) = dummy();
    game.frames(5);
    let head = ragdoll::body_of(&game.world, ragdoll, "Head").unwrap();
    let standing = vector(&game.world, head, "Position").y;
    game.run(
        r#"
        local humanoid = workspace.Dummy.Humanoid
        humanoid.Died:Connect(function() print("died") end)
        humanoid.StateChanged:Connect(function(old, new) print("state", new.Name) end)
        -- Two killing blows in one frame, and more later.
        humanoid:TakeDamage(60)
        humanoid:TakeDamage(60)
        "#,
    );
    game.frames(2);
    game.run("workspace.Dummy.Humanoid:TakeDamage(10) workspace.Dummy.Health.Health = 0");
    game.frames(120);
    assert_eq!(game.count("died"), 1, "{:?}", game.printed);
    assert_eq!(game.count("state Dead"), 1, "{:?}", game.printed);
    assert_eq!(
        game.world.get(ragdoll).unwrap().property("Enabled"),
        Some(&Value::Bool(true))
    );
    let lying = vector(&game.world, head, "Position").y;
    assert!(
        lying < standing - 1.0,
        "the head fell from {standing} to {lying}"
    );
    game.run("print(workspace.Dummy.Humanoid:GetState() == Enum.HumanoidStateType.Dead)");
    assert_eq!(game.printed.last().map(String::as_str), Some("true"));
}

#[test]
fn a_model_with_only_the_pieces_works_the_same_as_one_with_a_humanoid() {
    let mut world = World::new();
    floor(&mut world);
    character(&mut world, "Player", -20.0, 20.0);
    let mut bodies = Vec::new();
    let mut healths = Vec::new();
    for (name, x) in [("WithHumanoid", 0.0), ("PiecesOnly", 10.0)] {
        let (model, _) = character(&mut world, name, x, 0.0);
        healths.push(make(
            &mut world,
            ClassName::Health,
            "Health",
            model,
            Vec::new(),
        ));
        bodies.push(world.find_first_child(model, "Body").unwrap());
    }
    let with = world
        .find_first_child(world.workspace(), "WithHumanoid")
        .unwrap();
    make(
        &mut world,
        ClassName::Humanoid,
        "Humanoid",
        with,
        Vec::new(),
    );
    let mut game = Game::new(world, Assets::default());
    game.run(
        r#"
        local humanoid = workspace.WithHumanoid.Humanoid
        local pieces = workspace.PiecesOnly
        for _, model in { workspace.WithHumanoid, pieces } do
            model.Health.HealthChanged:Connect(function(new, old)
                print(model.Name, "changed", new, old)
            end)
        end
        humanoid.WalkSpeed = 10
        pieces.ThirdPersonController.WalkSpeed = 10
        humanoid:Move(Vector3.new(1, 0, -1))
        pieces.ThirdPersonController.MoveDirection = Vector3.new(1, 0, -1)
        humanoid:TakeDamage(30)
        pieces.Health:TakeDamage(30)
        "#,
    );
    game.frames(60);
    assert_eq!(
        game.printed,
        ["WithHumanoid changed 70 100", "PiecesOnly changed 70 100"]
    );
    let a = vector(&game.world, bodies[0], "Position");
    let b = vector(&game.world, bodies[1], "Position");
    assert!(
        (b.x - a.x - 10.0).abs() < 1e-3 && (b.z - a.z).abs() < 1e-3 && (b.y - a.y).abs() < 1e-3,
        "{a:?} and {b:?}"
    );
    // They walked (diagonally, at 10 a second) and turned the same way.
    assert!(a.z < -6.0 && a.x > 6.0, "{a:?}");
    let (ta, tb) = (
        vector(&game.world, bodies[0], "Orientation"),
        vector(&game.world, bodies[1], "Orientation"),
    );
    assert!(
        (ta.y - tb.y).abs() < 1e-3 && (ta.y + 45.0).abs() < 1e-2,
        "{ta:?}"
    );
    for health in healths {
        assert_eq!(
            game.world.get(health).unwrap().property("Health"),
            Some(&Value::Number(70.0))
        );
    }
}
