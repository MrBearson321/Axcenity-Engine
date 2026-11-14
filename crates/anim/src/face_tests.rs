//! Facial animation F3 and F4: face profiles, FaceRig and FaceControls,
//! then expressions, layers, blinks and gaze.

use std::path::Path;
use std::sync::Arc;

use axcenity_assets::{
    Assets, Bone, BoneTransform, FaceProfile, FaceProfiles, Model, ModelMaterial, MorphTarget,
    Primitive, Rig, ShapeWeight,
};
use axcenity_core::face::{FACE_CONTROLS, face_control};
use axcenity_core::{ClassName, InstanceId, Value, World};
use glam::{Mat4, Quat, Vec3};

use crate::retarget::Severity;
use crate::{
    Animators, CompiledVisemes, FaceProblem, FaceRig, built_in_face_profile, check_face,
    choose_face_profile,
};
use axcenity_assets::VisemeMap;

fn bone(name: &str, parent: Option<usize>, at: Vec3, rotation: Quat) -> Bone {
    Bone {
        name: name.into(),
        parent,
        rest: BoneTransform {
            translation: at,
            rotation,
            scale: Vec3::ONE,
        },
    }
}

/// A head (turned `head_turn` at rest) with a jaw and two eyes, and blend
/// shapes named as ARKit names them, plus one no profile knows.
fn head(head_turn: Quat) -> Model {
    let triangle = Primitive::new(
        vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        vec![[0.0, 0.0, 1.0]; 3],
        Vec::new(),
        vec![0, 1, 2],
        ModelMaterial::default(),
    );
    let mut model = Model::from_primitives(vec![triangle]).unwrap();
    model.morph_targets = [("eyeBlinkLeft", 0.1), ("jawOpen", 0.0), ("Freckles", 0.3)]
        .into_iter()
        .map(|(name, default_weight)| MorphTarget {
            name: name.into(),
            default_weight,
        })
        .collect();
    model.rig = Some(Arc::new(Rig {
        bones: vec![
            bone("Head", None, Vec3::new(0.0, 1.6, 0.0), head_turn),
            bone("Jaw", Some(0), Vec3::new(0.0, -0.05, 0.02), Quat::IDENTITY),
            bone(
                "eye.L",
                Some(0),
                Vec3::new(0.03, 0.05, 0.08),
                Quat::IDENTITY,
            ),
            bone(
                "eye.R",
                Some(0),
                Vec3::new(-0.03, 0.05, 0.08),
                Quat::IDENTITY,
            ),
        ],
        joints: Vec::new(),
        fit: Mat4::IDENTITY,
    }));
    model
}

fn controls(set: &[(&str, f32)]) -> Vec<f32> {
    let mut values = vec![0.0; FACE_CONTROLS.len()];
    for (name, value) in set {
        values[face_control(name).unwrap()] = *value;
    }
    values
}

/// Where `bone`'s +Z points in the file's space once `pose` is applied.
fn pointing(model: &Model, pose: &[BoneTransform], bone: usize) -> Vec3 {
    let rig = model.rig.as_ref().unwrap();
    rig.world(pose)[bone].transform_vector3(Vec3::Z).normalize()
}

#[test]
fn the_profile_naming_the_most_shapes_is_chosen_unless_one_is_named() {
    let model = head(Quat::IDENTITY);
    let none = FaceProfiles::default();
    let (name, _) = choose_face_profile(&model, &none, "").unwrap();
    assert_eq!(name, "arkit", "two ARKit shapes beat none");
    let mut own = FaceProfile::new();
    own.controls = vec![
        (
            "Blink_Left".into(),
            vec![ShapeWeight::new("eyeBlinkLeft", 1.0)],
        ),
        ("Cheek_Puff".into(), vec![ShapeWeight::new("Freckles", 1.0)]),
        ("Jaw_Open".into(), vec![ShapeWeight::new("jawOpen", 1.0)]),
    ];
    let project = FaceProfiles {
        profiles: vec![("assets/faces/mine.face.json".into(), own)],
        problems: Vec::new(),
    };
    let (name, _) = choose_face_profile(&model, &project, "").unwrap();
    assert_eq!(name, "assets/faces/mine.face.json", "three beat two");
    let (name, _) = choose_face_profile(&model, &project, "ARKit").unwrap();
    assert_eq!(name, "arkit", "a built-in by name, any case");
    let missing = choose_face_profile(&model, &project, "nope").unwrap_err();
    assert!(missing.contains("nope"), "{missing}");
    // A model with no shapes anyone names gets the standard profile.
    let mut bare = head(Quat::IDENTITY);
    bare.morph_targets.clear();
    assert_eq!(choose_face_profile(&bare, &none, "").unwrap().0, "axcenity");
}

#[test]
fn controls_set_the_shapes_they_drive_and_leave_the_others_as_the_file_has_them() {
    let model = head(Quat::IDENTITY);
    let (name, profile) = choose_face_profile(&model, &FaceProfiles::default(), "").unwrap();
    let (face, problems) = FaceRig::bind(&model, &name, &profile);
    assert!(
        problems.iter().all(|p| p.severity() != Severity::Error),
        "a built-in profile's shapes the model lacks aren't errors: {problems:?}"
    );
    let mut weights = Vec::new();
    face.weights(&model, &controls(&[]), &mut weights);
    assert_eq!(weights, [0.0, 0.0, 0.3], "driven shapes rest at 0");
    face.weights(
        &model,
        &controls(&[
            ("Blink_Left", 1.0),
            ("Jaw_Open", 0.5),
            ("Eye_Look_Left", 1.0),
        ]),
        &mut weights,
    );
    assert_eq!(weights, [1.0, 0.5, 0.3]);
    // Values are held to 0 to 1.
    face.weights(&model, &controls(&[("Blink_Left", 7.0)]), &mut weights);
    assert_eq!(weights[0], 1.0);
    // Blink_Left, Eye_Closed_Left and Jaw_Open move shapes; the look
    // controls and Jaw_Left/Right turn bones.
    assert_eq!(face.controls_used(), 3 + 4 + 2);
}

#[test]
fn eyes_and_jaw_turn_about_the_bodys_axes_within_their_limits() {
    for head_turn in [
        Quat::IDENTITY,
        Quat::from_rotation_y(1.2) * Quat::from_rotation_x(0.3),
    ] {
        let model = head(head_turn);
        let rig = model.rig.clone().unwrap();
        let (face, _) = FaceRig::bind(
            &model,
            "axcenity",
            &crate::built_in_face_profile("axcenity").unwrap(),
        );
        let rest = rig.rest_pose();
        // The body's left (eye.R to eye.L), up and forward at rest.
        let world = rig.world(&rest);
        let left = (world[2].w_axis - world[3].w_axis).truncate().normalize();
        let forward = left.cross(Vec3::Y).normalize();
        let up = forward.cross(left);

        let mut pose = rest.clone();
        face.turn(&mut pose, &controls(&[("Eye_Look_Left", 1.0)]));
        for eye in [2, 3] {
            let expected =
                Quat::from_axis_angle(up, 30f32.to_radians()) * pointing(&model, &rest, eye);
            let turned = pointing(&model, &pose, eye);
            assert!(turned.dot(expected) > 0.9999, "{turned} {expected}");
            let before = pointing(&model, &rest, eye);
            assert!(
                turned.dot(left) > before.dot(left) + 0.4,
                "looks to the character's left"
            );
        }
        assert_eq!(pose[1], rest[1], "the jaw stays");

        let mut pose = rest.clone();
        face.turn(
            &mut pose,
            &controls(&[("Jaw_Open", 1.0), ("Eye_Look_Up", 1.0)]),
        );
        let chin = pointing(&model, &pose, 1);
        let before = pointing(&model, &rest, 1);
        assert!((chin.angle_between(before).to_degrees() - 25.0).abs() < 0.01);
        assert!(chin.dot(up) < before.dot(up), "the jaw drops");
        assert!(
            pointing(&model, &pose, 2).dot(up) > before.dot(up),
            "the eyes look up"
        );
    }
}

#[test]
fn a_profile_that_doesnt_fit_says_what_is_wrong() {
    let model = head(Quat::IDENTITY);
    let text = r#"{
        "format_version": 1,
        "controls": {
            "Blink_Left": "eyeBlinkLeft",
            "Blink": "eyeBlinkLeft",
            "Blink_Left": "jawOpen",
            "Jaw_Open": [ { "shape": "jawOpen" }, { "shape": "jawOpen", "weight": 0.5 } ],
            "Cheek_Puff": "puff",
            "Mouth_Wide": [ { "shape": "Freckles", "weight": 3 } ]
        },
        "bones": { "Jaw": "", "LeftEye": "eyeball_left", "Nose": "nose" },
        "limits": { "look_up": 120 }
    }"#;
    let profile = FaceProfile::from_json(text).unwrap();
    let problems = check_face(&model, "assets/faces/bad.face.json", &profile);
    let has = |wanted: &FaceProblem| problems.contains(wanted);
    assert!(has(&FaceProblem::UnknownControl("Blink".into())));
    assert!(has(&FaceProblem::RepeatedControl("Blink_Left".into())));
    assert!(has(&FaceProblem::RepeatedShape {
        control: "Jaw_Open".into(),
        shape: "jawOpen".into()
    }));
    assert!(has(&FaceProblem::MissingShape {
        control: "Cheek_Puff".into(),
        shape: "puff".into()
    }));
    assert!(has(&FaceProblem::BadWeight {
        control: "Mouth_Wide".into(),
        shape: "Freckles".into(),
        weight: 3.0
    }));
    assert!(has(&FaceProblem::MissingBone {
        role: "LeftEye".into(),
        bone: "eyeball_left".into()
    }));
    assert!(has(&FaceProblem::UnknownBoneRole("Nose".into())));
    assert!(has(&FaceProblem::BadLimit {
        name: "look_up",
        degrees: 120.0
    }));
    // eye.R is still found by name, and Jaw_Open has a shape.
    assert!(!has(&FaceProblem::NoEyes) && !has(&FaceProblem::NoJaw));
    for problem in &problems {
        assert!(!problem.to_string().is_empty());
    }

    // No eye or jaw bones and no shapes for them.
    let mut flat = head(Quat::IDENTITY);
    flat.rig = None;
    flat.morph_targets.truncate(1);
    let problems = check_face(
        &flat,
        "arkit",
        &crate::built_in_face_profile("arkit").unwrap(),
    );
    assert!(problems.contains(&FaceProblem::NoEyes));
    assert!(problems.contains(&FaceProblem::NoJaw));
    let Some(FaceProblem::MissingControls(missing)) = problems
        .iter()
        .find(|problem| matches!(problem, FaceProblem::MissingControls(_)))
    else {
        panic!("{problems:?}");
    };
    assert_eq!(
        missing.len(),
        FACE_CONTROLS.len() - 2,
        "only the two blinks"
    );
}

const FRAME: f64 = 1.0 / 60.0;

fn face_controls(world: &mut World, part: InstanceId, set: &[(&str, f64)]) -> InstanceId {
    let face = world.create(ClassName::FaceControls).unwrap();
    for (name, value) in set {
        world
            .set_property(face, name, Value::Number(*value))
            .unwrap();
    }
    world.set_parent(face, Some(part)).unwrap();
    face
}

fn mesh_part(world: &mut World, mesh: &str) -> InstanceId {
    let part = world.create(ClassName::MeshPart).unwrap();
    world
        .set_property(part, "MeshId", Value::String(mesh.into()))
        .unwrap();
    world.set_parent(part, Some(world.workspace())).unwrap();
    part
}

/// How `eye` is turned from rest in `posed`, in the file's space (the
/// fitted space is squashed).
fn eye_turn(rig: &Rig, eye: usize, posed: &axcenity_assets::Posed) -> Quat {
    let joint = rig
        .joints
        .iter()
        .position(|joint| joint.bone == eye)
        .unwrap();
    let before = rig.joint_matrices(&rig.rest_pose())[joint];
    let after = posed.joints()[joint];
    (rig.fit.inverse() * after * before.inverse() * rig.fit)
        .to_scale_rotation_translation()
        .1
}

fn remy() -> (Assets, &'static str) {
    let assets =
        Assets::new(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-character"));
    let mesh = "assets/models/Walking/Walking.fbx";
    assets.model_blocking(mesh).unwrap();
    (assets, mesh)
}

#[test]
fn face_controls_turn_a_skinned_characters_eyes() {
    // The hello-character example's Remy has mixamorig:LeftEye and
    // RightEye, and faces +Z.
    let assets =
        Assets::new(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-character"));
    let mesh = "assets/models/Walking/Walking.fbx";
    let model = assets.model_blocking(mesh).unwrap();
    let rig = model.rig.clone().unwrap();
    let eye = rig.bone("mixamorig:LeftEye").unwrap();
    let mut world = World::new();
    let part = mesh_part(&mut world, mesh);
    let face = face_controls(&mut world, part, &[("Eye_Look_Left", 1.0)]);
    let mut animators = Animators::new();
    animators.update(&mut world, &assets, FRAME);
    let errors = animators.take_errors();
    assert!(errors.is_empty(), "{errors:?}");
    let posed = animators
        .poses()
        .get(&part)
        .expect("a face poses its MeshPart");
    let looks = |posed: &axcenity_assets::Posed| eye_turn(&rig, eye, posed);
    let turn = looks(posed);
    let ahead = turn * Vec3::Z;
    assert!(
        (turn.angle_between(Quat::IDENTITY).to_degrees() - 30.0).abs() < 1.0,
        "{turn:?}"
    );
    assert!(ahead.x > 0.4, "toward the character's left (+X): {ahead}");

    // Off, it rests (nothing else poses the part, so it isn't posed).
    world
        .set_property(face, "Enabled", Value::Bool(false))
        .unwrap();
    animators.update(&mut world, &assets, FRAME);
    assert!(animators.poses().get(&part).is_none());
}

#[test]
fn face_controls_set_a_still_models_blend_shapes() {
    let folder =
        std::env::temp_dir().join(format!("axcenity-face-controls-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).unwrap();
    // A square with two shapes and no skeleton.
    #[rustfmt::skip]
    let floats: Vec<f32> = vec![
        -1.0, -1.0, 0.0,  1.0, -1.0, 0.0,  1.0, 1.0, 0.0,  -1.0, 1.0, 0.0,
        0.0, 0.0, 0.0,  0.0, 0.0, 0.0,  0.0, 0.0, 1.0,  0.0, 0.0, 0.0,
        0.0, 0.0, 0.0,  1.0, 0.0, 0.0,  0.0, 0.0, 0.0,  0.0, 0.0, 0.0,
    ];
    let indices: [u16; 6] = [0, 1, 2, 0, 2, 3];
    let mut bytes: Vec<u8> = floats.iter().flat_map(|f| f.to_le_bytes()).collect();
    bytes.extend(indices.iter().flat_map(|i| i.to_le_bytes()));
    std::fs::write(folder.join("face.bin"), &bytes).unwrap();
    std::fs::write(
        folder.join("face.gltf"),
        r#"{
        "asset": { "version": "2.0" },
        "buffers": [ { "uri": "face.bin", "byteLength": 156 } ],
        "bufferViews": [
            { "buffer": 0, "byteOffset": 0, "byteLength": 48 },
            { "buffer": 0, "byteOffset": 48, "byteLength": 48 },
            { "buffer": 0, "byteOffset": 96, "byteLength": 48 },
            { "buffer": 0, "byteOffset": 144, "byteLength": 12 }
        ],
        "accessors": [
            { "bufferView": 0, "componentType": 5126, "count": 4, "type": "VEC3",
              "min": [-1, -1, 0], "max": [1, 1, 0] },
            { "bufferView": 1, "componentType": 5126, "count": 4, "type": "VEC3",
              "min": [0, 0, 0], "max": [0, 0, 1] },
            { "bufferView": 2, "componentType": 5126, "count": 4, "type": "VEC3",
              "min": [0, 0, 0], "max": [1, 0, 0] },
            { "bufferView": 3, "componentType": 5123, "count": 6, "type": "SCALAR" }
        ],
        "meshes": [ { "name": "Face",
            "primitives": [ { "attributes": { "POSITION": 0 }, "indices": 3,
                "targets": [ { "POSITION": 1 }, { "POSITION": 2 } ] } ],
            "weights": [0, 0.25],
            "extras": { "targetNames": ["Blink_Left", "Mouth_Smile_Left"] } } ],
        "nodes": [ { "name": "Face", "mesh": 0 } ],
        "scenes": [ { "nodes": [0] } ],
        "scene": 0
    }"#,
    )
    .unwrap();
    let assets = Assets::new(&folder);
    assets.model_blocking("face.gltf").unwrap();
    let mut world = World::new();
    let part = mesh_part(&mut world, "face.gltf");
    let face = face_controls(&mut world, part, &[("Blink_Left", 0.75)]);
    let mut animators = Animators::new();
    animators.update(&mut world, &assets, FRAME);
    assert_eq!(animators.take_errors(), Vec::<String>::new());
    let posed = animators.poses().get(&part).expect("posed for its face");
    assert_eq!(
        posed.morph_weights(),
        [0.75, 0.0],
        "the standard profile fits"
    );

    // A profile of the project's own, by path.
    std::fs::create_dir_all(folder.join("assets/faces")).unwrap();
    std::fs::write(
        folder.join("assets/faces/smiley.face.json"),
        r#"{ "format_version": 1, "controls": { "Blink_Left": "Mouth_Smile_Left" } }"#,
    )
    .unwrap();
    // As the editor's file watcher does.
    assets.forget("assets/faces/smiley.face.json");
    world
        .set_property(
            face,
            "Profile",
            Value::String("assets/faces/smiley.face.json".into()),
        )
        .unwrap();
    animators.update(&mut world, &assets, FRAME);
    let posed = animators.poses().get(&part).unwrap();
    assert_eq!(
        posed.morph_weights(),
        [0.0, 0.75],
        "the file's shapes rest as it has them"
    );

    // A profile that isn't there is said once.
    world
        .set_property(face, "Profile", Value::String("nope".into()))
        .unwrap();
    animators.update(&mut world, &assets, FRAME);
    animators.update(&mut world, &assets, FRAME);
    let errors = animators.take_errors();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].contains("nope"));
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn gaze_turns_the_eyes_toward_a_position_and_ahead_when_it_goes_behind() {
    let (assets, mesh) = remy();
    let model = assets.model_blocking(mesh).unwrap();
    let rig = model.rig.clone().unwrap();
    let eye = rig.bone("mixamorig:LeftEye").unwrap();
    let mut world = World::new();
    let part = mesh_part(&mut world, mesh);
    let face = face_controls(&mut world, part, &[("IdleMotion", 0.0)]);
    world
        .set_property(face, "BlinkEnabled", Value::Bool(false))
        .unwrap();
    world
        .set_property(face, "GazeMode", Value::String("Position".into()))
        .unwrap();
    world
        .set_property(face, "GazeSmoothing", Value::Number(0.0))
        .unwrap();
    // Far off to Remy's left (+X) and ahead (+Z), at about eye height.
    let look_at = |world: &mut World, x: f32, z: f32| {
        world
            .set_property(
                face,
                "GazePosition",
                Value::Vector3(axcenity_core::Vector3::new(x, 0.0, z)),
            )
            .unwrap();
    };
    look_at(&mut world, 100.0, 100.0);
    let mut animators = Animators::new();
    for _ in 0..30 {
        animators.update(&mut world, &assets, FRAME);
    }
    assert_eq!(animators.take_errors(), Vec::<String>::new());
    let turn = eye_turn(&rig, eye, animators.poses().get(&part).unwrap());
    let ahead = turn * Vec3::Z;
    assert!(
        (turn.angle_between(Quat::IDENTITY).to_degrees() - 30.0).abs() < 2.0,
        "45° off, held to the 30° limit: {turn:?}"
    );
    assert!(ahead.x > 0.4, "{ahead}");

    // Behind, the eyes give up and look ahead.
    look_at(&mut world, 0.0, -100.0);
    animators.update(&mut world, &assets, FRAME);
    let turn = eye_turn(&rig, eye, animators.poses().get(&part).unwrap());
    assert!(
        turn.angle_between(Quat::IDENTITY).to_degrees() < 1.0,
        "{turn:?}"
    );
}

#[test]
fn expressions_overrides_and_blinks_reach_the_shapes() {
    let (assets, mesh) = remy();
    let mut world = World::new();
    let part = mesh_part(&mut world, mesh);
    let face = face_controls(&mut world, part, &[("Mouth_Smile_Left", 0.25)]);
    // Remy has no blend shapes; the profile turns no bones for a smile,
    // so this checks what the layers make through a FaceRig directly.
    let model = assets.model_blocking(mesh).unwrap();
    let profile = crate::built_in_face_profile("axcenity").unwrap();
    let (rig, _) = FaceRig::bind(&model, "axcenity", &profile);
    let compose_with = |world: &mut World, motion: &mut crate::face_play::FaceMotion, dt: f64| {
        let mut heads = std::collections::HashMap::new();
        let layers = crate::face_world::face_layers(
            world,
            &assets,
            face,
            part,
            &model,
            motion,
            crate::face_world::Lookers {
                viewer: None,
                heads: &mut heads,
            },
            crate::face_play::FaceDetail::at(crate::FaceLod::Full, true),
            dt,
            &mut Vec::new(),
        )
        .unwrap();
        layers.compose(&rig, None, None)
    };
    let compose = |world: &mut World, dt: f64| {
        compose_with(world, &mut crate::face_play::FaceMotion::new(3), dt)
    };
    let at = |controls: &[f32], name: &str| controls[face_control(name).unwrap()];
    assert_eq!(at(&compose(&mut world, 0.0), "Mouth_Smile_Left"), 0.25);

    let state = world.face_mut(face).unwrap();
    state.expressions.push(axcenity_core::face::ExpressionPlay {
        name: "happy".into(),
        intensity: 1.0,
        target_intensity: 1.0,
        presence: 1.0,
        target_presence: 1.0,
        transition: None,
        weight: 1.0,
        priority: 0,
        asymmetry: 0.0,
        order: 0,
    });
    let controls = compose(&mut world, 0.0);
    assert_eq!(
        at(&controls, "Mouth_Smile_Left"),
        0.8,
        "Happy over the base"
    );

    // An override goes over everything; a masked layer leaves its controls.
    let state = world.face_mut(face).unwrap();
    state
        .overrides
        .insert(face_control("Mouth_Smile_Left").unwrap(), 0.1);
    assert_eq!(at(&compose(&mut world, 0.0), "Mouth_Smile_Left"), 0.1);
    let state = world.face_mut(face).unwrap();
    state.overrides.clear();
    state.layers.insert(
        axcenity_core::face::FaceLayer::Expression,
        axcenity_core::face::FaceLayerSetting {
            mask: axcenity_core::face::face_mask_controls("Eyes"),
            ..Default::default()
        },
    );
    assert_eq!(at(&compose(&mut world, 0.0), "Mouth_Smile_Left"), 0.25);

    // The eyes blink by themselves within the longest wait between
    // blinks, and the expression stays on.
    let mut motion = crate::face_play::FaceMotion::new(3);
    let mut closed = 0.0f32;
    for _ in 0..(7.0 / FRAME) as usize {
        let controls = compose_with(&mut world, &mut motion, FRAME);
        closed = closed.max(at(&controls, "Blink_Left").min(at(&controls, "Blink_Right")));
    }
    assert!(closed > 0.9, "{closed}");
    assert_eq!(world.face(face).unwrap().expressions.len(), 1);
}

#[test]
fn a_lip_sync_keeps_time_with_its_sound_and_fades_out_when_it_stops() {
    let folder = std::env::temp_dir().join(format!("axcenity-lip-sync-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(folder.join("voice")).unwrap();
    std::fs::write(
        folder.join("voice/hi.lipsync.json"),
        r#"{ "format_version": 1, "duration": 2,
            "visemes": [ { "time": 0, "viseme": "AI" }, { "time": 1, "viseme": "MBP" } ] }"#,
    )
    .unwrap();
    let assets = Assets::new(&folder);
    let model = head(Quat::IDENTITY);
    let mut world = World::new();
    let part = mesh_part(&mut world, "none.glb");
    let face = face_controls(&mut world, part, &[("Mouth_Open", 0.1)]);
    let sound = world.create(ClassName::Sound).unwrap();
    world.set_parent(sound, Some(part)).unwrap();
    world.face_mut(face).unwrap().lip_sync = Some(axcenity_core::face::LipSyncPlay {
        path: "voice/hi.lipsync.json".into(),
        time: 0.0,
        speed: 1.0,
        sound: Some(sound),
        heard: false,
        weight: 1.0,
        target_weight: 1.0,
        fade: 0.1,
    });
    let profile = crate::built_in_face_profile("axcenity").unwrap();
    let (rig, _) = FaceRig::bind(&model, "axcenity", &profile);
    let mut motion = crate::face_play::FaceMotion::new(1);
    let mut step = |world: &mut World, dt: f64| {
        let mut heads = std::collections::HashMap::new();
        let mut problems = Vec::new();
        let layers = crate::face_world::face_layers(
            world,
            &assets,
            face,
            part,
            &model,
            &mut motion,
            crate::face_world::Lookers {
                viewer: None,
                heads: &mut heads,
            },
            crate::face_play::FaceDetail::at(crate::FaceLod::Full, true),
            dt,
            &mut problems,
        )
        .unwrap();
        assert!(problems.is_empty(), "{problems:?}");
        let controls = layers.compose(&rig, None, None);
        let at = |name| controls[face_control(name).unwrap()];
        (at("Mouth_Open"), at("Jaw_Open"))
    };
    let time = |world: &World| world.face(face).unwrap().lip_sync.as_ref().map(|p| p.time);
    // The Sound hasn't started: the line waits on AI's first frame.
    let (open, jaw) = step(&mut world, 0.5);
    assert_eq!(time(&world), Some(0.0));
    assert!(
        (open - 0.45).abs() < 1e-5 && (jaw - 0.3).abs() < 1e-5,
        "{open} {jaw}"
    );
    world
        .set_property(sound, "Playing", Value::Bool(true))
        .unwrap();
    world
        .set_property(sound, "PlaybackSpeed", Value::Number(2.0))
        .unwrap();
    step(&mut world, 0.6);
    assert_eq!(time(&world), Some(1.2), "twice as fast as the Sound");
    let (open, _) = step(&mut world, 0.0);
    assert_eq!(open, 0.0, "MBP: closed, over the base's 0.1");
    // The Sound stops: the line fades out over 0.1 s and the face is its
    // own again.
    world
        .set_property(sound, "Playing", Value::Bool(false))
        .unwrap();
    step(&mut world, 0.05);
    let (open, _) = step(&mut world, 0.05);
    assert!((open - 0.1).abs() < 1e-5, "{open}");
    assert!(world.face(face).unwrap().lip_sync.is_none());
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn a_preview_poses_the_face_and_frames_the_head() {
    let model = head(Quat::IDENTITY);
    let profile = built_in_face_profile("arkit").unwrap();
    let (rig, _) = FaceRig::bind(&model, "arkit", &profile);
    let posed = crate::pose_face(&model, &rig, &controls(&[("Jaw_Open", 0.5)]));
    // jawOpen at half; eyeBlinkLeft driven (so 0); Freckles keeps its own.
    assert_eq!(posed.morph_weights(), &[0.0, 0.5, 0.3]);
    let at = crate::face_point(&model, &rig).unwrap();
    assert!((at - Vec3::new(0.0, 1.6, 0.0)).length() < 1e-5, "{at}");
    let moving = face_control("Jaw_Open").unwrap();
    assert!(rig.moves(moving));
    assert!(!rig.moves(face_control("Cheek_Puff").unwrap()));
    assert!(!rig.moves(FACE_CONTROLS.len()));
    let mut lip = VisemeMap::default();
    lip.visemes
        .insert("AI".into(), [("Jaw_Open".into(), 0.8)].into());
    let (compiled, problems) = CompiledVisemes::new(&lip);
    assert!(problems.is_empty());
    let mut values = controls(&[("Jaw_Open", 0.2), ("Brow_Lower_Left", 0.4)]);
    let mut weights = [0.0; 12];
    weights[1] = 1.0;
    compiled.apply(&weights, &mut values);
    assert!(
        (values[moving] - 0.8).abs() < 1e-6,
        "the mouth covers the jaw"
    );
    assert_eq!(values[face_control("Brow_Lower_Left").unwrap()], 0.4);
}

#[test]
fn far_and_unseen_faces_cost_less_and_say_so() {
    let (assets, mesh) = remy();
    let mut world = World::new();
    let part = mesh_part(&mut world, mesh);
    let face = face_controls(&mut world, part, &[("Eye_Look_Left", 1.0)]);
    world
        .set_property(face, "LodDistance", Value::Number(10.0))
        .unwrap();
    let mut animators = Animators::new();
    let look = Some(axcenity_core::Vector3::new(0.0, 0.0, 1.0));
    let from = |z: f32| Some(axcenity_core::Vector3::new(0.0, 0.0, -z));

    // Near and in view: whole, every frame.
    animators.set_viewer(from(5.0), look);
    for _ in 0..3 {
        animators.update(&mut world, &assets, FRAME);
    }
    let stats = animators.face_stats();
    assert_eq!(stats.faces, [1, 0, 0, 0]);
    assert_eq!(stats.evaluated, 1);
    assert!(stats.controls >= 4, "the eyes turn: {stats:?}");
    assert!(animators.poses().get(&part).is_some());

    // Past LodDistance: worked out 15 times a second, the rest reused.
    animators.set_viewer(from(12.0), look);
    let mut evaluated = 0;
    for _ in 0..60 {
        animators.update(&mut world, &assets, FRAME);
        evaluated += animators.face_stats().evaluated;
        assert_eq!(animators.face_stats().faces, [0, 1, 0, 0]);
        assert!(animators.poses().get(&part).is_some(), "still shown");
    }
    assert!((14..=17).contains(&evaluated), "{evaluated}");

    // Out of view: the face rests, so nothing poses Remy.
    animators.set_viewer(from(5.0), Some(axcenity_core::Vector3::new(0.0, 0.0, -1.0)));
    animators.update(&mut world, &assets, FRAME);
    assert_eq!(animators.face_stats().faces, [0, 0, 0, 1]);
    assert_eq!(animators.face_stats().controls, 0);
    assert!(animators.poses().get(&part).is_none());

    // LodDistance 0 keeps it whole however far.
    world
        .set_property(face, "LodDistance", Value::Number(0.0))
        .unwrap();
    animators.set_viewer(from(500.0), look);
    animators.update(&mut world, &assets, FRAME);
    assert_eq!(animators.face_stats().faces, [1, 0, 0, 0]);
    assert!(animators.take_errors().is_empty());
}

#[test]
fn a_face_without_procedural_motion_doesnt_blink_or_look_about() {
    let (assets, mesh) = remy();
    let mut world = World::new();
    let part = mesh_part(&mut world, mesh);
    let face = face_controls(&mut world, part, &[]);
    world
        .set_property(face, "ProceduralEnabled", Value::Bool(false))
        .unwrap();
    world
        .set_property(face, "GazeMode", Value::String("Camera".into()))
        .unwrap();
    world
        .set_property(face, "BlinkIntervalMax", Value::Number(0.2))
        .unwrap();
    world
        .set_property(face, "BlinkIntervalMin", Value::Number(0.1))
        .unwrap();
    let model = assets.model_blocking(mesh).unwrap();
    let rig = model.rig.clone().unwrap();
    let eye = rig.bone("mixamorig:LeftEye").unwrap();
    let mut animators = Animators::new();
    animators.set_viewer(
        Some(axcenity_core::Vector3::new(3.0, 1.0, 3.0)),
        Some(axcenity_core::Vector3::new(-1.0, 0.0, -1.0)),
    );
    for _ in 0..90 {
        animators.update(&mut world, &assets, FRAME);
        let posed = animators.poses().get(&part).expect("posed");
        let turn = eye_turn(&rig, eye, posed);
        assert!(
            turn.angle_between(Quat::IDENTITY) < 1e-3,
            "the eyes stay ahead"
        );
    }
}
