//! The default humanoid (`assets/characters/humanoid`, shown by
//! `examples/hello-humanoid`): its glTF animations play on its model, its
//! skeleton maps to every humanoid role with nothing to fix, and its head
//! carries the facial profile's shapes.

use axcenity_anim::{
    Digit, HumanBone, ROLE_COUNT, Segment, Severity, Side, check_pair, check_rig, rest_arm_angle,
};
use axcenity_core::Value;

use super::tests::start_example_in;
use super::*;

const MODEL: &str = "assets/models/Humanoid/Humanoid.glb";
const ANIMATIONS: &str = "assets/models/Humanoid/HumanoidAnimations.glb";

#[test]
fn hello_humanoid_plays_its_glb_animations_on_each_humanoid() {
    let (mut world, _scripts, folder) = start_example_in("hello-humanoid");
    let assets = Assets::new(&folder);
    let mut animators = Animators::new();
    let mut joints_at = |world: &mut World, seconds: u32| {
        for _ in 0..seconds * 30 {
            animators.update(world, &assets, 1.0 / 60.0);
        }
        assert_eq!(animators.take_errors(), Vec::<String>::new());
        let walker = world.find_first_child(world.workspace(), "Walk").unwrap();
        assert_eq!(animators.poses().len(), 3, "Idle, Walk and Wave are posed");
        animators.poses()[&walker].joints().to_vec()
    };
    let first = joints_at(&mut world, 1);
    let later = joints_at(&mut world, 1);
    assert_ne!(first, later, "the walk moves on");
    assert!(
        assets.take_errors().is_empty(),
        "{:?}",
        assets.take_errors()
    );
}

#[test]
fn the_default_humanoid_skeleton_maps_every_role_with_no_problems() {
    let (_world, _scripts, folder) = start_example_in("hello-humanoid");
    let assets = Assets::new(&folder);
    let model = assets.model_blocking(MODEL).unwrap();
    let bones = &model.rig.as_ref().unwrap().bones;
    let report = check_rig(bones, None, &assets.rig_definitions());
    let problems: Vec<_> = report
        .problems
        .iter()
        .filter(|problem| problem.severity != Severity::Note)
        .collect();
    assert!(problems.is_empty(), "{problems:?}");
    assert_eq!(report.map.roles().count(), ROLE_COUNT, "fingers included");
    assert!(
        report
            .map
            .get(HumanBone::Finger(Side::Left, Digit::Index, Segment::Distal))
            .is_some()
    );
    // A relaxed A-pose: the upper arms 45° below level.
    let angle = rest_arm_angle(bones, &report.map).unwrap();
    assert!((angle - 45.0).abs() < 1.0, "{angle}");
    // Every starter animation is a take for the same skeleton.
    let takes = assets.takes(ANIMATIONS).unwrap();
    assert_eq!(takes.len(), 32);
    for take in takes.iter() {
        let problems = check_pair(&take.skeleton, bones, &assets.rig_definitions());
        assert!(problems.is_empty(), "{}: {problems:?}", take.name);
    }
}

#[test]
fn the_default_humanoid_head_has_the_facial_profile_shapes_at_rest() {
    let (_world, _scripts, folder) = start_example_in("hello-humanoid");
    let assets = Assets::new(&folder);
    let model = assets.model_blocking(MODEL).unwrap();
    let names: Vec<&str> = model
        .morph_targets
        .iter()
        .map(|target| target.name.as_str())
        .collect();
    for name in [
        "Blink_Left",
        "Blink_Right",
        "Eye_Squint_Left",
        "Eye_Wide_Right",
        "Brow_Raise_Left",
        "Brow_Lower_Right",
        "Mouth_Smile_Left",
        "Mouth_Frown_Right",
        "Mouth_Open",
        "Jaw_Open",
        "Mouth_Pucker",
        "Mouth_Left",
        "Mouth_Right",
        "Cheek_Raise_Left",
    ] {
        assert!(names.contains(&name), "{name} in {names:?}");
    }
    assert_eq!(names.len(), 41);
    assert!(model.morph_targets.iter().all(|t| t.default_weight == 0.0));
}

/// A new Baseplate project in a temporary folder, its player spawned as
/// the default humanoid, stepped frame by frame as the player steps it.
struct Game {
    parent: std::path::PathBuf,
    folder: std::path::PathBuf,
    world: World,
    assets: Assets,
    scripts: axcenity_script::ScriptRuntime,
    characters: Controllers,
    physics: Physics,
    animators: Animators,
}

impl Game {
    fn new(name: &str) -> Self {
        let parent = std::env::temp_dir().join(format!(
            "axcenity-player-default-humanoid-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&parent);
        std::fs::create_dir_all(&parent).unwrap();
        let folder = axcenity_project::create_project_from(
            &parent,
            "Humanoid Test",
            axcenity_project::Template::Baseplate,
        )
        .unwrap();
        let mut world = axcenity_project::load(&folder).unwrap().world;
        axcenity_project::load_starter_character(&folder, &mut world)
            .unwrap()
            .expect("the template names the Character prefab");
        let assets = Assets::new(&folder);
        let mut scripts = axcenity_sim::script_runtime();
        scripts.set_assets(assets.clone());
        let physics = Physics::with_assets(assets.clone());
        Self {
            parent,
            folder,
            world,
            assets,
            scripts,
            characters: Controllers::default(),
            physics,
            animators: Animators::new(),
        }
    }

    fn frames(&mut self, count: usize, input: MoveInput) {
        let seconds = 1.0 / 60.0;
        for _ in 0..count {
            let world = &mut self.world;
            self.characters
                .step(world, &self.assets, input, seconds as f32);
            axcenity_gameplay::step_humanoids(&mut self.scripts, world, &self.characters, seconds);
            self.scripts.step_players(world, seconds);
            self.scripts.step(world, seconds);
            self.characters.keep_kinematic(world);
            self.physics.step(world, seconds);
            self.animators.update(world, &self.assets, seconds);
        }
    }

    /// The player's character Model.
    fn character(&self) -> InstanceId {
        let players = self.world.players();
        let player = self.world.get(players).unwrap().children()[0];
        self.world
            .get(player)
            .unwrap()
            .property("Character")
            .and_then(Value::as_instance)
            .unwrap()
    }

    fn vector(&self, id: InstanceId, name: &str) -> axcenity_core::Vector3 {
        self.world
            .get(id)
            .unwrap()
            .property(name)
            .unwrap()
            .as_vector3()
            .unwrap()
    }

    fn no_errors(&mut self) {
        assert_eq!(self.animators.take_errors(), Vec::<String>::new());
        let errors = self.assets.take_errors();
        assert!(errors.is_empty(), "{errors:?}");
    }
}

impl Drop for Game {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.parent);
    }
}

fn walk() -> MoveInput {
    MoveInput {
        forward: true,
        ..MoveInput::default()
    }
}

/// A new Baseplate project's player spawns as the default humanoid, which
/// walks on its root part and animates through the Humanoid's states.
#[test]
fn a_new_baseplate_projects_player_is_the_default_humanoid_and_walks_animated() {
    let mut game = Game::new("walks");
    game.frames(30, MoveInput::default());
    let character = game.character();
    let root = game
        .world
        .find_first_child(character, "HumanoidRootPart")
        .unwrap();
    let body = game.world.find_first_child(character, "Body").unwrap();
    let start = game.vector(root, "Position");
    assert!(
        start.y > 2.0 && start.y < 3.0,
        "standing on the baseplate: {start:?}"
    );
    let animator = game.world.find_first_child(body, "Animator").unwrap();
    let state = |game: &Game| game.world.animator(animator).unwrap().current_state.clone();
    assert_eq!(state(&game), "Idle");
    let idle = game.animators.poses()[&body].joints().to_vec();
    game.frames(30, walk());
    let moved = game.vector(root, "Position");
    assert!(
        moved.z < start.z - 4.0,
        "walked forward: {start:?} → {moved:?}"
    );
    assert!(
        (game.vector(body, "Position").z - moved.z).abs() < 0.5,
        "the mesh came along"
    );
    assert_eq!(state(&game), "Walk");
    assert_ne!(game.animators.poses()[&body].joints(), idle.as_slice());
    // Crouched, the root squashes (the mesh doesn't) and it plays
    // CrouchWalk, then Crouch standing still.
    let crouch = MoveInput {
        crouch: true,
        ..walk()
    };
    game.frames(10, crouch);
    assert_eq!(state(&game), "CrouchWalk");
    let root_size = game.vector(root, "Size");
    assert!(root_size.y < 4.0, "{root_size:?}");
    assert!((game.vector(body, "Size").y - 5.0).abs() < 0.1);
    game.frames(
        10,
        MoveInput {
            forward: false,
            ..crouch
        },
    );
    assert_eq!(state(&game), "Crouch");
    game.no_errors();
}

/// HD4: a part from `HumanoidParts.glb` put inside the default character's
/// Body follows its pose (each bone where the Body's is) and is placed
/// where the Body's skeleton puts it, moving and turning with it.
#[test]
fn a_part_inside_the_body_follows_its_pose_and_place() {
    let mut game = Game::new("parts");
    axcenity_project::add_humanoid_parts(&game.folder).unwrap();
    game.frames(10, MoveInput::default());
    let character = game.character();
    let body = game.world.find_first_child(character, "Body").unwrap();
    let hair = axcenity_project::humanoid_part(&mut game.world, body, "Hair_Long");
    let hair_mesh = axcenity_project::humanoid_part_mesh_id("Hair_Long");
    let _ = game.assets.model_blocking(&hair_mesh);
    game.frames(40, walk());
    // Turn it too, so the place is checked off the axes.
    game.frames(
        20,
        MoveInput {
            right: true,
            ..walk()
        },
    );
    assert!(
        game.animators.poses().contains_key(&hair),
        "the hair is posed"
    );
    assert_eq!(
        game.vector(hair, "Orientation"),
        game.vector(body, "Orientation")
    );
    let hair_size = game.vector(hair, "Size");
    assert!(
        hair_size.y > 0.5 && hair_size.y < 2.5,
        "hair is a small part of a 5-stud body: {hair_size:?}"
    );
    // The head bone is in the same place in the world in both, posed.
    let bone_at = |game: &Game, part: InstanceId, mesh: &str| {
        let rig = game
            .assets
            .model_blocking(mesh)
            .unwrap()
            .rig
            .clone()
            .unwrap();
        let bone = rig.bone("Head").unwrap();
        let posed = game.animators.poses().get(&part).cloned();
        axcenity_anim::ragdoll::bone_in_world(&game.world, part, &rig, posed.as_deref(), bone)
            .unwrap()
            .w_axis
    };
    let body_head = bone_at(&game, body, MODEL);
    let hair_head = bone_at(&game, hair, &hair_mesh);
    assert!(
        (body_head - hair_head).length() < 1e-3,
        "{body_head:?} vs {hair_head:?}"
    );
    // And the hair's mesh sits around that head: its box's top is near
    // the top of the body.
    let top = |game: &Game, id| game.vector(id, "Position").y + game.vector(id, "Size").y * 0.5;
    assert!(
        (top(&game, hair) - top(&game, body)).abs() < 0.3,
        "{} vs {}",
        top(&game, hair),
        top(&game, body)
    );
    // Swapping is setting the MeshId: the short hair is placed anew.
    let short = axcenity_project::humanoid_part_mesh_id("Hair_Short");
    game.world
        .set_property(hair, "MeshId", Value::String(short.clone()))
        .unwrap();
    let _ = game.assets.model_blocking(&short);
    game.frames(2, MoveInput::default());
    assert_ne!(game.vector(hair, "Size"), hair_size);
    assert!(game.animators.poses().contains_key(&hair));
    assert!(
        (bone_at(&game, hair, &short) - bone_at(&game, body, MODEL)).length() < 1e-3,
        "the short hair's head is the body's"
    );
    game.no_errors();
}
