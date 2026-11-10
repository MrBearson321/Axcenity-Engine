//! Phase 78: 2D blend spaces (strafing in eight directions), and layers
//! (an upper-body reload over a run, faded by its weight).

use std::path::Path;
use std::sync::Arc;

use axcenity_assets::{Assets, BoneTransform, Keys, Rig, Take, Track};
use axcenity_core::{ClassName, InstanceId, LayerWeight, Parameter, Value, Vector3, World};
use glam::{Mat4, Quat, Vec3};

use crate::files::{
    ANY_STATE, BlendSpace, BlendSpaceChild, Condition, LayerDef, Motion, ParameterDef, StateDef,
    StateMachineFile, Test, TransitionDef,
};
use crate::{Animators, Clip, ClipFile, Compiled, Parameters};

const STRAFE_MODEL: &str = "assets/models/strafer.fbx";

/// A small biped: Hips (a meter up) with two legs under it.
fn biped() -> Rig {
    let bone = |name: &str, parent: Option<usize>, at: Vec3| axcenity_assets::Bone {
        name: name.into(),
        parent,
        rest: BoneTransform {
            translation: at,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        },
    };
    Rig {
        bones: vec![
            bone("Hips", None, Vec3::new(0.0, 1.0, 0.0)),
            bone("LeftLeg", Some(0), Vec3::new(-0.2, 0.0, 0.0)),
            bone("RightLeg", Some(0), Vec3::new(0.2, 0.0, 0.0)),
        ],
        joints: Vec::new(),
        fit: Mat4::IDENTITY,
    }
}

/// The eight directions and standing still, as `[sideways, forward]`.
fn directions() -> Vec<[f64; 2]> {
    let mut all = vec![[0.0, 0.0]];
    for x in [-1.0, 0.0, 1.0] {
        for z in [-1.0, 0.0, 1.0] {
            if x != 0.0 || z != 0.0 {
                all.push([x, z]);
            }
        }
    }
    all
}

fn take_name([x, z]: [f64; 2]) -> String {
    format!("Move {x} {z}")
}

/// One take per direction (a second each): the hips go 2 m that way while
/// the legs swing forward and back (each the other way round) along it.
fn strafe_takes() -> Arc<Vec<Take>> {
    let times = vec![0.0, 0.25, 0.5, 0.75, 1.0];
    let takes = directions()
        .into_iter()
        .map(|[x, z]| {
            let way = Vec3::new(x as f32, 0.0, z as f32);
            let swing_axis = if way.length() > 0.0 {
                Vec3::Y.cross(way).normalize()
            } else {
                Vec3::X
            };
            let swing = |sign: f32| Keys {
                times: times.clone(),
                values: [0.0, 1.0, 0.0, -1.0, 0.0]
                    .iter()
                    .map(|s: &f32| Quat::from_axis_angle(swing_axis, sign * s * 0.5))
                    .collect(),
            };
            let leg = |name: &str, side: f32, sign: f32| Track {
                bone: name.into(),
                translation: Keys {
                    times: vec![0.0],
                    values: vec![Vec3::new(side, 0.0, 0.0)],
                },
                rotation: swing(sign),
                scale: Keys::default(),
            };
            Take {
                name: take_name([x, z]),
                duration: 1.0,
                frame_rate: 30.0,
                tracks: vec![
                    Track {
                        bone: "Hips".into(),
                        translation: Keys {
                            times: vec![0.0, 1.0],
                            values: vec![
                                Vec3::new(0.0, 1.0, 0.0),
                                Vec3::new(0.0, 1.0, 0.0) + way * 2.0,
                            ],
                        },
                        rotation: Keys::default(),
                        scale: Keys::default(),
                    },
                    leg("LeftLeg", -0.2, 1.0),
                    leg("RightLeg", 0.2, -1.0),
                ],
            }
        })
        .collect();
    Arc::new(takes)
}

fn clip_path(direction: [f64; 2]) -> String {
    format!(
        "assets/animations/strafer/{}.anim.json",
        take_name(direction)
    )
}

/// A direction's clip file: its whole take, with root motion on.
fn strafe_clip(direction: [f64; 2]) -> ClipFile {
    let mut clip = ClipFile::linked(STRAFE_MODEL, &take_name(direction), 0.0, 1.0);
    clip.link.as_mut().unwrap().root_motion = true;
    clip
}

fn load(path: &str, takes: &Arc<Vec<Take>>) -> Result<crate::animators::LoadedClip, String> {
    let direction = directions()
        .into_iter()
        .find(|&d| clip_path(d) == path)
        .ok_or_else(|| format!("no clip `{path}`"))?;
    Ok((strafe_clip(direction), Some(takes.clone())))
}

fn move_parameters(value: [f64; 2]) -> Parameters {
    [
        ("MoveX".to_owned(), Parameter::Number(value[0])),
        ("MoveZ".to_owned(), Parameter::Number(value[1])),
    ]
    .into()
}

/// A state machine whose one state strafes by a blend space of every
/// direction's clip.
fn strafing(rig: &Rig, takes: &Arc<Vec<Take>>) -> Compiled {
    let number = |name: &str| ParameterDef::Number {
        name: name.into(),
        default: 0.0,
    };
    let file = StateMachineFile {
        parameters: vec![number("MoveX"), number("MoveZ")],
        states: vec![StateDef::new(
            "Strafe",
            Motion::BlendSpace(BlendSpace {
                parameter_x: "MoveX".into(),
                parameter_y: "MoveZ".into(),
                children: directions()
                    .into_iter()
                    .map(|direction| BlendSpaceChild {
                        clip: clip_path(direction),
                        position: direction,
                    })
                    .collect(),
            }),
            [0.0, 0.0],
        )],
        ..StateMachineFile::default()
    };
    let (compiled, problems) = Compiled::new(file, Some(rig), |path| load(path, takes));
    assert_eq!(problems, Vec::<String>::new());
    compiled
}

fn close(a: &BoneTransform, b: &BoneTransform) -> bool {
    a.translation.abs_diff_eq(b.translation, 1e-5)
        && (a.rotation.dot(b.rotation).abs() - 1.0).abs() < 1e-5
        && a.scale.abs_diff_eq(b.scale, 1e-5)
}

#[test]
fn a_character_strafes_in_eight_directions_from_one_blend_space_without_foot_sliding() {
    let rig = biped();
    let takes = strafe_takes();
    let compiled = strafing(&rig, &takes);
    for (index, direction) in directions().into_iter().enumerate() {
        let parameters = move_parameters(direction);
        // At its point, the direction's clip plays alone...
        let weights = compiled.tree_weights(0, &parameters).unwrap();
        for (other, weight) in weights.iter().enumerate() {
            assert_eq!(
                *weight,
                if other == index { 1.0 } else { 0.0 },
                "{direction:?}"
            );
        }
        let alone = Clip::bind(strafe_clip(direction), Some(takes.clone()), Some(&rig)).unwrap();
        assert_eq!(compiled.duration(0, &parameters), alone.duration());
        // ...posed exactly as the clip poses it all the way through, the
        // legs swinging and the hips kept over where they started...
        for step in 0..=8 {
            let phase = step as f64 / 8.0;
            let (mut blended, mut clip) = (rig.rest_pose(), rig.rest_pose());
            compiled.sample(0, phase, &rig, &parameters, &mut blended);
            alone.sample(phase, &rig, &mut clip);
            assert_eq!(blended, clip, "{direction:?} at {phase}");
            assert!(blended[0].translation.x.abs() < 1e-5 && blended[0].translation.z.abs() < 1e-5);
        }
        // ...and the model travels as far as the clip's own stride, so the
        // feet don't slide: 2 m a pass in that direction.
        let travel = compiled.root_motion(0, &rig, &parameters, 0.0, 1.0).travel;
        assert_eq!(travel, alone.root_motion(&rig, 0.0, 1.0).travel);
        let want = Vec3::new(direction[0] as f32, 0.0, direction[1] as f32) * 2.0;
        assert!(travel.abs_diff_eq(want, 1e-4), "{direction:?}: {travel}");
    }
    // Between the points the travel follows the parameters, as the clips'
    // strides do.
    for value in [[0.5, 1.0], [-0.3, -0.6], [0.25, 0.0], [0.9, -0.2]] {
        let travel = compiled
            .root_motion(0, &rig, &move_parameters(value), 0.0, 1.0)
            .travel;
        let want = Vec3::new(value[0] as f32, 0.0, value[1] as f32) * 2.0;
        assert!(travel.abs_diff_eq(want, 1e-4), "{value:?}: {travel}");
    }
}

#[test]
fn a_blend_space_needs_number_parameters() {
    let rig = biped();
    let takes = strafe_takes();
    let mut file = strafing(&rig, &takes).file.clone();
    file.parameters = vec![ParameterDef::Bool {
        name: "MoveX".into(),
        default: false,
    }];
    let (_, problems) = Compiled::new(file, Some(&rig), |path| load(path, &takes));
    assert_eq!(
        problems,
        [
            "the blend space of `Strafe` needs Number parameters, and `MoveX` isn't one",
            "the blend space of `Strafe` needs Number parameters, and `MoveZ` isn't one",
        ]
    );
}

#[test]
fn an_additive_layer_adds_its_turn_from_the_rest_pose() {
    let rig = biped();
    let mut file = StateMachineFile::default();
    let mut aim = LayerDef::new("Aim");
    aim.mode = crate::LayerMode::Additive;
    aim.mask = vec!["LeftLeg".into()];
    file.layers.push(aim);
    let (compiled, problems) = Compiled::new(file, Some(&rig), |_| Err("none".into()));
    assert_eq!(problems, Vec::<String>::new());
    let layer = &compiled.layers()[0];
    assert_eq!(layer.mask, [false, true, false]);
    // The base turns the left leg 30° about X; the layer's pose is the rest
    // turned 20° about X, on both legs, but the mask keeps the right one.
    let mut under = rig.rest_pose();
    under[1].rotation = Quat::from_rotation_x(0.5);
    let mut over = rig.rest_pose();
    over[1].rotation = Quat::from_rotation_x(0.3);
    over[2].rotation = Quat::from_rotation_x(0.3);
    let base_right = under[2];
    layer.mix(&mut under, &over, &[1.0, 1.0, 1.0], 0.5, &rig);
    assert!(
        under[1]
            .rotation
            .abs_diff_eq(Quat::from_rotation_x(0.65), 1e-5)
    );
    assert_eq!(under[2], base_right);
}

#[test]
fn a_mask_naming_a_bone_the_skeleton_lacks_is_reported() {
    let rig = biped();
    let mut file = StateMachineFile::default();
    let mut layer = LayerDef::new("Upper");
    layer.mask = vec!["Hips/Spine".into(), "Hips".into()];
    file.layers.push(layer);
    let (compiled, problems) = Compiled::new(file, Some(&rig), |_| Err("none".into()));
    assert_eq!(
        problems,
        ["the layer `Upper`'s mask names the bone `Hips/Spine`, which the skeleton doesn't have"]
    );
    // "Hips" and everything under it.
    assert_eq!(compiled.layers()[0].mask, [true, true, true]);
}

fn example() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-animation")
}

const RUN: &str = "assets/animations/noodle/Run.anim.json";
const RELOAD: &str = "assets/animations/noodle/Reload.anim.json";
const MACHINE: &str = "assets/animations/noodle/layered.statemachine.json";

/// The noodle's whole `Moves` take (Top waves, then everything hops), two
/// seconds: a "reload" that would move the whole body if nothing masked it.
fn reload_clip() -> ClipFile {
    ClipFile::linked("assets/models/noodle.fbx", "Moves", 0.0, 2.0)
}

/// A temporary project: the noodle, its Run clip, a Reload clip, and a state
/// machine that runs on its base layer and has an `UpperBody` layer
/// (masked to `Top`) that rests in an empty state until `Reload`.
fn layered_project(name: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!(
        "axcenity-anim-layers-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("assets/models")).unwrap();
    std::fs::create_dir_all(root.join("assets/animations/noodle")).unwrap();
    std::fs::copy(
        example().join("assets/models/noodle.fbx"),
        root.join("assets/models/noodle.fbx"),
    )
    .unwrap();
    std::fs::copy(example().join(RUN), root.join(RUN)).unwrap();
    crate::files::write_file(&root.join(RELOAD), &reload_clip().to_json()).unwrap();
    let mut upper = LayerDef::new("UpperBody");
    upper.mask = vec!["Top".into()];
    upper.default_state = "Empty".into();
    upper.states = vec![
        StateDef::new("Empty", Motion::Clip(String::new()), [0.0, 0.0]),
        StateDef::new("Reload", Motion::Clip(RELOAD.into()), [200.0, 0.0]),
    ];
    upper.transitions = vec![TransitionDef {
        conditions: vec![Condition {
            parameter: "Reload".into(),
            test: Test::Triggered,
            value: 0.0,
        }],
        blend: 0.0,
        ..TransitionDef::new(ANY_STATE, "Reload")
    }];
    let file = StateMachineFile {
        parameters: vec![ParameterDef::Trigger {
            name: "Reload".into(),
        }],
        default_state: "Run".into(),
        states: vec![StateDef::new("Run", Motion::Clip(RUN.into()), [0.0, 0.0])],
        layers: vec![upper],
        ..StateMachineFile::default()
    };
    crate::files::write_file(&root.join(MACHINE), &file.to_json()).unwrap();
    root
}

fn noodle(machine: &str) -> (World, InstanceId, InstanceId) {
    let mut world = World::new();
    let part = world.create(ClassName::MeshPart).unwrap();
    world
        .set_property(
            part,
            "MeshId",
            Value::String("assets/models/noodle.fbx".into()),
        )
        .unwrap();
    world
        .set_property(part, "Size", Value::Vector3(Vector3::new(1.0, 3.8, 1.0)))
        .unwrap();
    world.set_parent(part, Some(world.workspace())).unwrap();
    let animator = world.create(ClassName::Animator).unwrap();
    world
        .set_property(animator, "StateMachine", Value::String(machine.into()))
        .unwrap();
    world.set_parent(animator, Some(part)).unwrap();
    (world, part, animator)
}

/// The pose the noodle should have: the Run clip at `run_phase`, with Top
/// moved `top_share` of the way to the Reload clip's at `reload_phase`.
fn expected(assets: &Assets, run_phase: f64, reload: Option<(f64, f32)>) -> Vec<Mat4> {
    let model = assets.model_blocking("assets/models/noodle.fbx").unwrap();
    let rig = model.rig.as_deref().unwrap();
    let bind = |path: &str| {
        let (file, takes) = crate::load_clip(assets, path).unwrap();
        Clip::bind(file, takes, Some(rig)).unwrap()
    };
    let mut pose = rig.rest_pose();
    bind(RUN).sample(run_phase, rig, &mut pose);
    if let Some((phase, share)) = reload {
        let mut over = rig.rest_pose();
        bind(RELOAD).sample(phase, rig, &mut over);
        let top = rig.bone("Top").unwrap();
        pose[top] = pose[top].lerp(&over[top], share);
    }
    model.pose(&pose).unwrap().joints().to_vec()
}

fn joints(animators: &Animators, part: InstanceId) -> Vec<Mat4> {
    animators.poses()[&part].joints().to_vec()
}

fn same(a: &[Mat4], b: &[Mat4]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(a, b)| a.abs_diff_eq(*b, 1e-4))
}

#[test]
fn an_upper_body_reload_layer_plays_while_running_and_the_legs_keep_running() {
    let root = layered_project("reload");
    let assets = Assets::new(&root);
    let (mut world, part, animator) = noodle(MACHINE);
    let mut animators = Animators::new();
    animators.update(&mut world, &assets, 0.0);
    assert_eq!(animators.take_errors(), Vec::<String>::new());
    assert_eq!(
        world.animator(animator).unwrap().layer_weights["UpperBody"],
        LayerWeight::new(1.0)
    );
    // Its empty state moves nothing: the noodle just runs.
    animators.update(&mut world, &assets, 0.2);
    assert!(same(
        &joints(&animators, part),
        &expected(&assets, 0.4, None)
    ));

    // Reload: Top follows the reload while Root and Mid keep the run's
    // pose. The Run clip lasts half a second, the Reload two.
    world
        .animator_mut(animator)
        .unwrap()
        .parameters
        .insert("Reload".into(), Parameter::Trigger(true));
    animators.update(&mut world, &assets, 0.0);
    // Half a second in, Top is waving, which the run alone doesn't do.
    animators.update(&mut world, &assets, 0.5);
    let waving = joints(&animators, part);
    assert!(same(&waving, &expected(&assets, 1.4, Some((0.25, 1.0)))));
    assert!(!same(&waving, &expected(&assets, 1.4, None)));
    // 1.4 s in, the reload alone would have lifted the whole noodle (its
    // hop), but the mask keeps Root and Mid running.
    animators.update(&mut world, &assets, 0.9);
    let reloading = joints(&animators, part);
    assert!(same(&reloading, &expected(&assets, 3.2, Some((0.7, 1.0)))));
    let model = assets.model_blocking("assets/models/noodle.fbx").unwrap();
    let rig = model.rig.as_deref().unwrap();
    let root_bone = rig.bone("Root").unwrap();
    let (file, takes) = crate::load_clip(&assets, RELOAD).unwrap();
    let mut hop = rig.rest_pose();
    Clip::bind(file, takes, Some(rig))
        .unwrap()
        .sample(0.7, rig, &mut hop);
    let mut run = rig.rest_pose();
    let (file, takes) = crate::load_clip(&assets, RUN).unwrap();
    Clip::bind(file, takes, Some(rig))
        .unwrap()
        .sample(3.2, rig, &mut run);
    assert!(!close(&hop[root_bone], &run[root_bone]));

    // A script fades the layer out over half a second: a quarter second in,
    // Top is halfway between the run's and the reload's.
    world
        .animator_mut(animator)
        .unwrap()
        .layer_weights
        .get_mut("UpperBody")
        .unwrap()
        .set(0.0, 0.5);
    animators.update(&mut world, &assets, 0.25);
    let weight = world.animator(animator).unwrap().layer_weights["UpperBody"];
    assert!((weight.weight - 0.5).abs() < 1e-9, "{weight:?}");
    assert!(same(
        &joints(&animators, part),
        &expected(&assets, 3.7, Some((0.825, 0.5)))
    ));
    animators.update(&mut world, &assets, 0.25);
    assert!(same(
        &joints(&animators, part),
        &expected(&assets, 4.2, None)
    ));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_weight_for_a_layer_the_state_machine_lacks_is_reported() {
    let root = layered_project("unknown");
    let assets = Assets::new(&root);
    let (mut world, _, animator) = noodle(MACHINE);
    world
        .animator_mut(animator)
        .unwrap()
        .layer_weights
        .insert("Legs".into(), LayerWeight::new(1.0));
    let mut animators = Animators::new();
    animators.update(&mut world, &assets, 0.0);
    let errors = animators.take_errors();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].contains("the layer `Legs`"), "{errors:?}");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_state_machine_with_only_a_1d_blend_tree_poses_as_its_machine_alone() {
    // The hello-animation noodle's state machine has no layers: what the
    // Animators pose is exactly what its machine poses by itself.
    let assets = Assets::new(example());
    let path = "assets/animations/noodle/noodle.statemachine.json";
    let file = crate::read_state_machine(&assets, path).unwrap();
    assert!(file.layers.is_empty());
    let model = assets.model_blocking("assets/models/noodle.fbx").unwrap();
    let rig = model.rig.as_deref().unwrap();
    let (compiled, problems) =
        Compiled::new(file, Some(rig), |clip| crate::load_clip(&assets, clip));
    assert!(problems.is_empty(), "{problems:?}");
    let mut machine = crate::Machine::new(&compiled);
    let (mut world, part, animator) = noodle(path);
    let mut animators = Animators::new();
    animators.update(&mut world, &assets, 0.0);
    for (speed, dt) in [(0.0, 0.1), (1.0, 0.2), (3.0, 0.15), (6.0, 0.3), (2.5, 0.05)] {
        let parameters: Parameters = [("Speed".to_owned(), Parameter::Number(speed))].into();
        world
            .animator_mut(animator)
            .unwrap()
            .parameters
            .insert("Speed".into(), Parameter::Number(speed));
        animators.update(&mut world, &assets, dt);
        machine.advance(&compiled, Some(rig), &parameters, dt);
        let want = model
            .pose(&machine.pose(&compiled, rig, &parameters))
            .unwrap();
        assert_eq!(joints(&animators, part), want.joints(), "Speed {speed}");
    }
}
