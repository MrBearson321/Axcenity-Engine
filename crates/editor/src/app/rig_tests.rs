//! Phase 62 in the editor: a humanoid rigged in the Model Studio, saved as
//! a skinned `.glb`, playing hello-character's Mixamo walk through an
//! Animator without retargeting; and a shoulder's weights painted and
//! saved, deforming the game's MeshPart just as the Studio's Pose test.

use axcenity_anim::Animators;
use axcenity_assets::{Model, Pose};
use axcenity_mesh_edit::rig::HUMANOID_BONES;
use axcenity_render::glam::Vec3;
use eframe::egui;

use super::tests::copy_of_example;
use super::*;
use crate::model_studio::rig_tests::{HEIGHT, bone, corners, front_view, humanoid_doc, rigged};
use crate::model_studio::{Mode, StudioDoc};

const WALK: &str = "assets/models/Walking/Walking.fbx";
const MACHINE: &str = "assets/animations/Walking/Walking.statemachine.json";

/// The humanoid open in the Studio, to be saved as
/// `assets/models/Humanoid.glb`, with the Humanoid rig and automatic
/// weights.
fn rigged_humanoid(app: &mut EditorApp) {
    let doc = StudioDoc::new(
        "assets/models/Humanoid.glb".to_owned(),
        humanoid_doc(),
        false,
    );
    app.model_studio.open(doc);
    rigged(&mut app.model_studio);
}

/// A MeshPart in the Workspace drawing `mesh_id` at its size.
fn mesh_part(app: &mut EditorApp, mesh_id: &str) -> InstanceId {
    let size = app.assets.model(mesh_id).unwrap().size;
    let workspace = app.world.workspace();
    let part = app.world.create(ClassName::MeshPart).unwrap();
    for (name, value) in [
        ("MeshId", Value::String(mesh_id.to_owned())),
        (
            "Size",
            Value::Vector3(Vector3::new(size[0], size[1], size[2])),
        ),
        ("Anchored", Value::Bool(true)),
    ] {
        app.world.set_property(part, name, value).unwrap();
    }
    app.world.set_parent(part, Some(workspace)).unwrap();
    part
}

#[test]
fn a_humanoid_rigged_in_the_studio_plays_a_mixamo_walk_through_an_animator_without_retargeting() {
    let folder = copy_of_example("hello-character", "rigged-walk");
    let mut app = EditorApp::new(Some(&folder));
    rigged_humanoid(&mut app);
    let path = app.save_model_studio().unwrap();
    assert_eq!(path, "assets/models/Humanoid.glb");
    let model = app.assets.model(&path).unwrap();
    let rig = model.rig.clone().expect("the saved model is skinned");

    // Without retargeting: the walk's tracks find the bones by name, and
    // the bone offsets it keys are the rig's own.
    let takes = app.assets.takes(WALK).unwrap();
    let take = takes.iter().find(|take| take.name == "mixamo.com").unwrap();
    let binding = take.bind(&rig);
    for name in HUMANOID_BONES.iter().filter(|name| !name.ends_with("_End")) {
        let name = format!("mixamorig:{name}");
        let track = take.tracks.iter().position(|t| t.bone == name).unwrap();
        let bound = binding[track].expect("the walk moves every template bone");
        assert_eq!(rig.bones[bound].name, name);
        let keys = &take.tracks[track].translation.values;
        if keys.windows(2).all(|w| w[0].distance(w[1]) < 1e-4) {
            let gap = keys[0].distance(rig.bones[bound].rest.translation);
            assert!(
                gap < 0.01,
                "{name}: the walk keys it {gap} cm from where it rests"
            );
        }
    }

    // A MeshPart with an Animator playing the walk's state machine.
    let part = mesh_part(&mut app, &path);
    let animator = app.world.create(ClassName::Animator).unwrap();
    app.world
        .set_property(animator, "StateMachine", Value::String(MACHINE.to_owned()))
        .unwrap();
    app.world.set_parent(animator, Some(part)).unwrap();
    let mut animators = Animators::new();

    // The toe tips and the triangles, in the file's space (the fitted
    // space is squeezed differently along each axis).
    let unfit = rig.fit.inverse();
    let file = |p: [f32; 3]| unfit.transform_point3(Vec3::from(p));
    let primitive = &model.primitives[0];
    let rest: Vec<Vec3> = primitive.positions.iter().map(|&p| file(p)).collect();
    let toe = |name: &str| {
        let bone = rig.bone(&format!("mixamorig:{name}")).unwrap();
        let tip = rig.world(&rig.rest_pose())[bone].transform_point3(Vec3::ZERO);
        (0..rest.len())
            .min_by(|&a, &b| rest[a].distance(tip).total_cmp(&rest[b].distance(tip)))
            .unwrap()
    };
    let (left, right) = (toe("LeftToe_End"), toe("RightToe_End"));
    let triangles = primitive.indices.as_chunks::<3>().0.to_vec();
    let (mut ahead, mut behind, mut growth) = (f32::MIN, f32::MAX, 0.0f32);
    for _ in 0..34 {
        animators.update(&mut app.world, &app.assets, 1.0 / 30.0);
        let posed = animators
            .poses()
            .get(&part)
            .expect("the Animator poses it")
            .clone();
        let corners: Vec<Vec3> = posed
            .primitive(&model, 0, 0)
            .positions
            .iter()
            .map(|&p| file(p))
            .collect();
        // Left foot ahead of the right (+Z), then behind it.
        let stride = corners[left].z - corners[right].z;
        ahead = ahead.max(stride);
        behind = behind.min(stride);
        // No edge pulls much longer than it was: the skin doesn't tear.
        for t in &triangles {
            for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                let (a, b) = (a as usize, b as usize);
                let longer = corners[a].distance(corners[b]) - rest[a].distance(rest[b]);
                growth = growth.max(longer);
            }
        }
    }
    assert!(animators.take_errors().is_empty());
    // Each foot swings well ahead of the other.
    assert!(
        ahead > 0.1 * HEIGHT && behind < -0.1 * HEIGHT,
        "stride {behind} to {ahead}"
    );
    assert!(
        growth < 0.03 * HEIGHT,
        "an edge grew {growth} studs while walking"
    );
}

/// The game's MeshPart model for `path` posed by `pose` (in the Studio's
/// rig order, matched by bone name as a take would), in the file's space.
fn game_corners(app: &EditorApp, path: &str, pose: &Pose) -> Vec<Vec3> {
    let studio_rig = app
        .model_studio
        .doc
        .as_ref()
        .unwrap()
        .model
        .armature
        .as_ref()
        .unwrap()
        .to_rig()
        .0;
    let model: std::sync::Arc<Model> = app.assets.model(path).unwrap();
    let rig = model.rig.as_deref().unwrap();
    let game_pose: Pose = rig
        .bones
        .iter()
        .map(|b| pose[studio_rig.bone(&b.name).unwrap()])
        .collect();
    let posed = model.pose(&game_pose).unwrap();
    let size = Vec3::from(model.size);
    let center = Vec3::from(model.center);
    (0..model.primitives.len())
        .flat_map(|k| posed.primitive(&model, k, 0).positions.clone())
        .map(|p| Vec3::from(p) * size + center)
        .collect()
}

fn largest_gap(a: &[Vec3], b: &[Vec3]) -> f32 {
    assert_eq!(a.len(), b.len());
    a.iter()
        .zip(b)
        .map(|(a, b)| a.distance(*b))
        .fold(0.0, f32::max)
}

#[test]
fn weight_painting_a_shoulder_and_saving_changes_the_deformation_in_the_game_as_in_the_studio() {
    let folder = copy_of_example("hello-character", "rigged-shoulder");
    let mut app = EditorApp::new(Some(&folder));
    rigged_humanoid(&mut app);
    // The left arm raised in Pose test.
    let studio = &mut app.model_studio;
    studio.doc.as_mut().unwrap().set_mode(Mode::Pose);
    let arm = bone(studio, "LeftArm");
    studio.set_pose(arm, Vec3::new(0.0, 0.0, -70.0));
    let pose = studio.test_pose().unwrap();
    let studio_before = corners(&studio.rig_model().unwrap());
    let path = app.save_model_studio().unwrap();
    let game_before = game_corners(&app, &path, &pose);
    assert!(largest_gap(&studio_before, &game_before) < 1e-3);

    // The shoulder's weights painted up from the front.
    let studio = &mut app.model_studio;
    studio.doc.as_mut().unwrap().set_mode(Mode::Weights);
    let shoulder = bone(studio, "LeftShoulder");
    studio.rig.bone = Some(shoulder);
    studio.rig.brush.radius = 35.0;
    studio.rig.brush.strength = 1.0;
    let screen = front_view();
    let armature = studio.doc.as_ref().unwrap().model.armature.clone().unwrap();
    let (from, to) = (armature.bones[shoulder].head, armature.bones[arm].head);
    let points: Vec<egui::Pos2> = (0..=10)
        .filter_map(|k| screen.point(from.lerp(to, k as f32 / 10.0) + Vec3::Z * 0.3))
        .collect();
    assert!(studio.weight_stroke(&screen, &points));
    studio.doc.as_mut().unwrap().set_mode(Mode::Pose);
    assert_eq!(studio.test_pose().unwrap(), pose, "the pose test stays");
    let studio_after = corners(&studio.rig_model().unwrap());
    let changed = largest_gap(&studio_before, &studio_after);
    assert!(
        changed > 0.01 * HEIGHT,
        "the shoulder deforms differently: {changed}"
    );

    // Saved, the game deforms the same way.
    app.save_model_studio().unwrap();
    let game_after = game_corners(&app, &path, &pose);
    let gap = largest_gap(&studio_after, &game_after);
    assert!(gap < 1e-3, "the game's arm is {gap} from the Studio's");
    assert!(largest_gap(&game_before, &game_after) > 0.01 * HEIGHT);
}

/// Snapshots of the saved humanoid as the game poses it, a quarter and
/// half way through the walk.
#[test]
fn snapshots_of_the_walk() {
    let Some(shots) = std::env::var_os("AXCENITY_SNAPSHOTS") else {
        return;
    };
    let folder = copy_of_example("hello-character", "rigged-walk-shot");
    let mut app = EditorApp::new(Some(&folder));
    rigged_humanoid(&mut app);
    let path = app.save_model_studio().unwrap();
    let model = app.assets.model(&path).unwrap();
    let part = mesh_part(&mut app, &path);
    let animator = app.world.create(ClassName::Animator).unwrap();
    app.world
        .set_property(animator, "StateMachine", Value::String(MACHINE.to_owned()))
        .unwrap();
    app.world.set_parent(animator, Some(part)).unwrap();
    let mut animators = Animators::new();
    for frame in 0..14 {
        animators.update(&mut app.world, &app.assets, 1.0 / 30.0);
        if frame % 7 != 6 {
            continue;
        }
        let posed = animators.poses().get(&part).unwrap().clone();
        let primitives = (0..model.primitives.len())
            .map(|k| {
                let p = &model.primitives[k];
                let moved = posed.primitive(&model, k, 0);
                let size = Vec3::from(model.size);
                let center = Vec3::from(model.center);
                axcenity_assets::Primitive::new(
                    moved
                        .positions
                        .iter()
                        .map(|&q| (Vec3::from(q) * size + center).to_array())
                        .collect(),
                    moved.normals.clone(),
                    Vec::new(),
                    p.indices.clone(),
                    p.material.clone(),
                )
            })
            .collect();
        let posed_model = Model::from_primitives(primitives).unwrap();
        let (mesh, materials) = axcenity_mesh_edit::from_model(&posed_model);
        let mut doc = humanoid_doc();
        doc.objects[0].mesh = Some(mesh);
        doc.objects[0].materials = materials;
        let mut studio = crate::model_studio::ModelStudio::default();
        studio.open(StudioDoc::new(String::new(), doc, false));
        let shots = std::path::PathBuf::from(&shots);
        crate::snapshot::snapshot_rendered(
            &shots.join(format!("walk-{frame}.png")),
            egui::vec2(900.0, 700.0),
            vec![Vec::new(); 3],
            |ctx, render| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let _ = studio.show(ui, Some(render), &[], crate::gizmo::Snap::default(), true);
                });
            },
        );
    }
}
