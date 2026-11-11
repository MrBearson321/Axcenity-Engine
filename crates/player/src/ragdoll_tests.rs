//! Phase 41: ragdolls built from a Mixamo character, collapsing, knocked
//! over and blending back to the animation.

use std::path::Path;

use axcenity_anim::ragdoll;
use axcenity_core::{ClassName, PartType, Value, Vector3};
use axcenity_render::glam::{EulerRot, Quat};

use super::*;

const FRAME: f32 = 1.0 / 60.0;

fn example(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .join(name)
}

fn vector(world: &World, id: InstanceId, name: &str) -> Vec3 {
    let v = world
        .get(id)
        .unwrap()
        .property(name)
        .and_then(Value::as_vector3)
        .unwrap();
    Vec3::new(v.x, v.y, v.z)
}

fn rotation(world: &World, id: InstanceId) -> Quat {
    let o = vector(world, id, "Orientation");
    Quat::from_euler(
        EulerRot::YXZ,
        o.y.to_radians(),
        o.x.to_radians(),
        o.z.to_radians(),
    )
}

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

/// A floor, and hello-character's Mixamo character standing on it
/// (anchored) with a Ragdoll built in it. Returns the world, the
/// example's assets, the MeshPart and the Ragdoll.
fn dummy() -> (World, Assets, InstanceId, InstanceId) {
    let assets = Assets::new(example("hello-character"));
    let mut world = World::new();
    let workspace = world.workspace();
    make(
        &mut world,
        ClassName::Part,
        "Floor",
        workspace,
        vec![
            ("Size", Value::Vector3(Vector3::new(60.0, 1.0, 60.0))),
            ("Position", Value::Vector3(Vector3::new(0.0, -0.5, 0.0))),
            ("Anchored", Value::Bool(true)),
        ],
    );
    let dummy = make(
        &mut world,
        ClassName::MeshPart,
        "Dummy",
        workspace,
        vec![
            (
                "MeshId",
                Value::String("assets/models/Walking/Walking.fbx".into()),
            ),
            (
                "Size",
                Value::Vector3(Vector3::new(3.8939824, 3.784847, 0.7229666)),
            ),
            (
                "Position",
                Value::Vector3(Vector3::new(0.0, 1.8924235, 0.0)),
            ),
            ("Anchored", Value::Bool(true)),
        ],
    );
    let ragdoll = make(&mut world, ClassName::Ragdoll, "Ragdoll", dummy, Vec::new());
    let bodies = ragdoll::build(&mut world, &assets, ragdoll).unwrap();
    assert_eq!(bodies, 11);
    (world, assets, dummy, ragdoll)
}

/// Physics, then Animators, as the player's frame runs them.
struct Sim {
    world: World,
    assets: Assets,
    physics: Physics,
    animators: Animators,
}

impl Sim {
    fn new(world: World, assets: Assets) -> Self {
        Self {
            physics: Physics::with_assets(assets.clone()),
            world,
            assets,
            animators: Animators::new(),
        }
    }

    fn frames(&mut self, count: usize) {
        for _ in 0..count {
            self.physics.step(&mut self.world, f64::from(FRAME));
            self.animators
                .update(&mut self.world, &self.assets, f64::from(FRAME));
        }
    }

    fn body(&self, ragdoll: InstanceId, bone: &str) -> InstanceId {
        ragdoll::body_of(&self.world, ragdoll, bone).unwrap()
    }
}

/// Each joint's turn as `physics` limits it, in degrees: around
/// Attachment0's X (a ball socket's twist, a hinge's angle), measured from
/// the middle of a ball socket's twist range; then how far the attachments'
/// X axes are apart (a ball socket's cone; 0 on a hinge, whose other axes
/// are locked).
fn joint_angles(world: &World, joint: InstanceId) -> (f32, f32) {
    let instance = world.get(joint).unwrap();
    let number = |name| instance.property(name).and_then(Value::as_number).unwrap() as f32;
    let middle = match instance.class() {
        ClassName::BallSocketConstraint => {
            (number("TwistLowerAngle") + number("TwistUpperAngle")) / 2.0
        }
        _ => 0.0,
    };
    let frame = |name| {
        let attachment = instance
            .property(name)
            .and_then(Value::as_instance)
            .unwrap();
        let part = world.get(attachment).unwrap().parent().unwrap();
        rotation(world, part) * rotation(world, attachment)
    };
    let (frame0, frame1) = (frame("Attachment0"), frame("Attachment1"));
    let q = (frame0 * Quat::from_rotation_x(middle.to_radians())).inverse() * frame1;
    let q = if q.w < 0.0 { -q } else { q };
    let turn = (2.0 * q.x.atan2(q.w)).to_degrees() + middle;
    let apart = (frame0 * Vec3::X)
        .angle_between(frame1 * Vec3::X)
        .to_degrees();
    (turn, apart)
}

/// How far past its limits each joint of the ragdoll is now, in degrees,
/// with its name, worst first.
fn past_limits(world: &World, ragdoll: InstanceId) -> Vec<(f32, String)> {
    let mut past = Vec::new();
    for id in world.descendants(ragdoll) {
        let joint = world.get(id).unwrap();
        let number = |name| joint.property(name).and_then(Value::as_number).unwrap() as f32;
        let ((lower, upper), cone) = match joint.class() {
            ClassName::BallSocketConstraint => (
                (number("TwistLowerAngle"), number("TwistUpperAngle")),
                number("UpperAngle"),
            ),
            ClassName::HingeConstraint => ((number("LowerAngle"), number("UpperAngle")), 0.0),
            _ => continue,
        };
        let (turn, apart) = joint_angles(world, id);
        let over = [lower - turn, turn - upper, apart - cone]
            .into_iter()
            .fold(0.0f32, f32::max);
        past.push((over, joint.name().to_owned()));
    }
    past.sort_by(|a, b| b.0.total_cmp(&a.0));
    past
}

#[test]
fn build_makes_a_capsule_per_main_bone_and_a_limited_ball_socket_per_joint() {
    let (world, _, _, ragdoll) = dummy();
    let parts: Vec<&str> = world
        .get(ragdoll)
        .unwrap()
        .children()
        .iter()
        .map(|&id| world.get(id).unwrap().name())
        .collect();
    assert_eq!(
        parts,
        [
            "mixamorig:Hips",
            "mixamorig:Spine2",
            "mixamorig:Head",
            "mixamorig:LeftArm",
            "mixamorig:LeftForeArm",
            "mixamorig:RightArm",
            "mixamorig:RightForeArm",
            "mixamorig:LeftUpLeg",
            "mixamorig:LeftLeg",
            "mixamorig:RightUpLeg",
            "mixamorig:RightLeg",
        ]
    );
    let mut joints = Vec::new();
    for id in world.descendants(ragdoll) {
        let instance = world.get(id).unwrap();
        match instance.class() {
            ClassName::Part => {
                assert_eq!(
                    instance.property("Shape"),
                    Some(&Value::PartType(PartType::Capsule))
                );
                assert!(
                    world
                        .find_first_child(id, ragdoll::BONE_ATTACHMENT)
                        .is_some()
                );
            }
            ClassName::BallSocketConstraint | ClassName::HingeConstraint => {
                assert_eq!(instance.property("LimitsEnabled"), Some(&Value::Bool(true)));
                let hinge = instance.class() == ClassName::HingeConstraint;
                let bends = ["Elbow", "Knee"]
                    .iter()
                    .any(|n| instance.name().ends_with(n));
                assert_eq!(hinge, bends, "{}", instance.name());
                joints.push(instance.name().to_owned());
            }
            _ => {}
        }
    }
    joints.sort();
    assert_eq!(
        joints,
        [
            "LeftElbow",
            "LeftHip",
            "LeftKnee",
            "LeftShoulder",
            "Neck",
            "RightElbow",
            "RightHip",
            "RightKnee",
            "RightShoulder",
            "Waist",
        ]
    );
    // Built where the bones rest, every joint is within its limits.
    assert!(past_limits(&world, ragdoll)[0].0 <= 0.01);
}

#[test]
fn an_enabled_ragdoll_collapses_without_bending_a_joint_past_its_limit() {
    let (mut world, assets, dummy, ragdoll) = dummy();
    world
        .set_property(ragdoll, "Enabled", Value::Bool(true))
        .unwrap();
    let mut sim = Sim::new(world, assets);
    let head = sim.body(ragdoll, "Head");
    let hips = sim.body(ragdoll, "Hips");
    let head_before = vector(&sim.world, head, "Position");
    let snapshots = std::env::var_os("AXCENITY_SNAPSHOTS").map(PathBuf::from);
    let look = |sim: &Sim, name: &str| {
        if let Some(folder) = &snapshots {
            snapshot(
                sim,
                head_before,
                &folder.join(format!("ragdoll-{name}.png")),
            );
        }
    };
    look(&sim, "standing");
    // Landing hard, a joint can be knocked past its limit for a few frames
    // (rapier pulls it back at up to 3 radians a second); it must never go
    // far, and once the ragdoll lies still every joint is within its limits.
    let (mut worst, mut frames_far_past, mut settled) = (0.0f32, 0, 0.0f32);
    for frame in 0..4 * 60 {
        sim.frames(1);
        let past = past_limits(&sim.world, ragdoll)[0].0;
        worst = worst.max(past);
        if past > 10.0 {
            frames_far_past += 1;
        }
        if frame >= 3 * 60 {
            settled = settled.max(past);
        }
        if frame == 20 {
            look(&sim, "falling");
        }
    }
    look(&sim, "lying");
    let head_after = vector(&sim.world, head, "Position");
    let hips_after = vector(&sim.world, hips, "Position");
    assert!(
        head_after.y < 0.8,
        "the head ends near the floor: {head_after}"
    );
    assert!(hips_after.y < 0.8, "so do the hips: {hips_after}");
    assert!(head_after.y > -0.1, "nothing falls through the floor");
    assert!(
        head_before.distance(head_after) < 6.0,
        "it collapses where it stood: {head_before} -> {head_after}"
    );
    assert!(
        settled < 1.0,
        "lying still: {:?}",
        past_limits(&sim.world, ragdoll)
    );
    assert!(worst < 45.0, "never far past a limit: {worst}");
    assert!(
        frames_far_past <= 15,
        "and only for moments: {frames_far_past} frames more than 10° past"
    );
    // It lies still, and the MeshPart is posed by the bodies.
    let still = vector(&sim.world, hips, "AssemblyLinearVelocity");
    assert!(still.length() < 0.5, "{still}");
    assert!(sim.animators.poses().contains_key(&dummy));
}

/// Draws the ragdoll's MeshPart as posed by its bodies, from the side and
/// a little above `at`, into `path`, when there's a GPU.
fn snapshot(sim: &Sim, at: Vec3, path: &Path) {
    use axcenity_render::{Camera, TextureRenderer};
    const VIEW: (u32, u32) = (480, 360);
    let instance = axcenity_render::adapters::instance();
    let Ok(adapter) = pollster::block_on(axcenity_render::adapters::adapter(&instance, None))
    else {
        return;
    };
    let Ok((device, queue)) = pollster::block_on(adapter.request_device(&Default::default()))
    else {
        return;
    };
    let target = Vec3::new(at.x, 1.0, at.z);
    let camera = Camera {
        position: target + Vec3::new(4.5, 2.5, 4.5),
        look_at: target,
        fov_y_degrees: 60.0,
    };
    let mut renderer = TextureRenderer::new(&device, &queue, VIEW.0, VIEW.1);
    let mut terrain = axcenity_scene::TerrainCache::new();
    for _ in 0..2 {
        let list = axcenity_scene::build_draw_list_posed(
            &sim.world,
            &sim.assets,
            camera,
            sim.animators.poses(),
            &mut terrain,
        );
        renderer.render(&list).unwrap();
    }
    let pixels = renderer.read_pixels();
    let file = std::fs::File::create(path).unwrap();
    let mut encoder = png::Encoder::new(file, VIEW.0, VIEW.1);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(&pixels)
        .unwrap();
}

/// hello-character running in the player's frame order, from its start:
/// scripts get the assets and Animators start before any script runs.
pub(super) struct Game {
    pub world: World,
    scripts: ScriptRuntime,
    assets: Assets,
    characters: Controllers,
    agents: Agents,
    physics: Physics,
    animators: Animators,
    /// The example's copy, which its assets load from.
    _copy: super::tests::TempCopy,
}

impl Game {
    pub fn start() -> Self {
        let folder = super::tests::TempCopy::of(
            &example("hello-character"),
            &format!(
                "axcenity-player-ragdoll-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ),
        );
        let project = axcenity_project::load(&folder).unwrap();
        let mut world = project.world;
        let assets = Assets::new(&folder);
        let mut scripts = axcenity_sim::script_runtime();
        scripts.set_assets(assets.clone());
        let mut animators = Animators::new();
        animators.update(&mut world, &assets, 0.0);
        start_scripts(&mut scripts, &mut world, &project.scripts);
        Self {
            world,
            scripts,
            physics: Physics::with_assets(assets.clone()),
            assets,
            characters: Controllers::default(),
            agents: Agents::default(),
            animators,
            _copy: folder,
        }
    }

    pub fn frames(&mut self, count: usize) {
        for _ in 0..count {
            let seconds = FRAME;
            self.characters
                .step(&mut self.world, &self.assets, MoveInput::default(), seconds);
            fire_seated(&mut self.scripts, &mut self.world, &mut self.characters);
            self.agents.step(&mut self.world, &self.assets, seconds);
            self.scripts.step(&mut self.world, f64::from(seconds));
            self.scripts.stepped(&mut self.world, f64::from(seconds));
            self.characters.keep_kinematic(&mut self.world);
            self.agents.keep_kinematic(&mut self.world);
            self.physics.step(&mut self.world, f64::from(seconds));
            let touches = self.physics.take_touches();
            self.scripts.fire_touched(&mut self.world, &touches);
            let ends = self.physics.take_touch_ends();
            self.scripts.fire_touch_ended(&mut self.world, &ends);
            self.scripts.heartbeat(&mut self.world, f64::from(seconds));
            self.animators
                .update(&mut self.world, &self.assets, f64::from(seconds));
            for line in self.scripts.take_output() {
                assert!(line.kind != OutputKind::Error, "{}", line.text);
            }
        }
    }
}

/// The Dummy's MeshPart and Ragdoll in `world`.
pub(super) fn the_dummy(world: &World) -> (InstanceId, InstanceId) {
    let dummy = world.find_first_child(world.workspace(), "Dummy").unwrap();
    (dummy, world.find_first_child(dummy, "Ragdoll").unwrap())
}

/// Each body of the Dummy's ragdoll by name, where it is relative to the
/// Dummy's MeshPart.
fn body_offsets(world: &World) -> Vec<(String, Vec3)> {
    let (dummy, ragdoll) = the_dummy(world);
    let at = vector(world, dummy, "Position");
    world
        .get(ragdoll)
        .unwrap()
        .children()
        .iter()
        .map(|&id| {
            (
                world.get(id).unwrap().name().to_owned(),
                vector(world, id, "Position") - at,
            )
        })
        .collect()
}

#[test]
fn an_impulse_on_the_head_knocks_the_dummy_backward_and_it_blends_back_to_idle() {
    let mut game = Game::start();
    let (dummy, ragdoll) = the_dummy(&game.world);
    let head = ragdoll::body_of(&game.world, ragdoll, "Head").unwrap();
    // It faces its LookVector: -Z turned by its Orientation.
    let ahead = rotation(&game.world, dummy) * Vec3::NEG_Z;

    // Standing idle, the bodies follow the animation.
    game.frames(110);
    assert_eq!(
        game.world.get(ragdoll).unwrap().property("Enabled"),
        Some(&Value::Bool(false))
    );
    let idle = body_offsets(&game.world);
    let standing = vector(&game.world, head, "Position");
    assert!(standing.y > 3.0, "{standing}");

    // At 2 seconds the script turns it on and pushes the head back.
    game.frames(10 + 90);
    assert_eq!(
        game.world.get(ragdoll).unwrap().property("Enabled"),
        Some(&Value::Bool(true))
    );
    let fallen = vector(&game.world, head, "Position");
    let back = (standing - fallen).dot(ahead);
    assert!(back > 1.5, "the head went back {back} studs");
    assert!(fallen.y < 0.8, "and lies near the floor: {fallen}");
    assert!(
        past_limits(&game.world, ragdoll)[0].0 < 2.0,
        "{:?}",
        past_limits(&game.world, ragdoll)
    );

    // At 5 seconds it's turned off: from where it lies, it blends back to
    // the idle pose over BlendTime (half a second, 30 frames), with no jump.
    let enabled = |game: &Game| {
        game.world.get(ragdoll).unwrap().property("Enabled") == Some(&Value::Bool(true))
    };
    let mut before = body_offsets(&game.world);
    let mut lying = before.clone();
    for _ in 0..2 * 60 {
        lying = before;
        game.frames(1);
        before = body_offsets(&game.world);
        if !enabled(&game) {
            break;
        }
    }
    assert!(!enabled(&game), "the script turns it off");
    let far = |a: &[(String, Vec3)], b: &[(String, Vec3)]| {
        a.iter()
            .zip(b)
            .map(|((_, a), (_, b))| a.distance(*b))
            .fold(0.0f32, f32::max)
    };
    let mut biggest_step = far(&lying, &before);
    let mut back_at = None;
    for frame in 1..60 {
        game.frames(1);
        let now = body_offsets(&game.world);
        biggest_step = biggest_step.max(far(&before, &now));
        if back_at.is_none() && far(&idle, &now) < 0.01 {
            back_at = Some(frame);
        }
        before = now;
    }
    assert!(far(&lying, &idle) > 2.0, "it was lying down");
    let back_at = back_at.expect("it gets back to idle");
    assert!((29..=31).contains(&back_at), "back at frame {back_at}");
    assert!(biggest_step < 0.4, "no snap: {biggest_step}");
    // It stands up where it fell, not where it started.
    let moved = vector(&game.world, head, "Position") - standing;
    assert!(moved.dot(-ahead) > 0.5, "{moved}");
    assert!(moved.y.abs() < 0.01, "standing again: {moved}");
}
