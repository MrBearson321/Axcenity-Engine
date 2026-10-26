//! Moving joints with IK in the Animation tab: a moved joint's chain is
//! keyed by Record and K (and nothing else is), the pose plays the same in
//! Play, undo and redo put it back, and pins are kept per clip while the
//! editor runs but never saved.

use std::fs;

use axcenity_anim::ik::joint_positions;
use axcenity_anim::{ClipFile, PropertyName};
use axcenity_assets::{BoneTransform, Rig};
use axcenity_render::glam::Vec3;

use super::posing_tests::{plays_as_previewed, record_clip_edit};
use super::tests::{copy_of_example, part};
use super::*;

const NOODLE: &str = "assets/models/noodle.fbx";
const IDLE: &str = "assets/animations/noodle/Idle.anim.json";
const WALK: &str = "assets/animations/noodle/Walk.anim.json";

/// The noodle's rig.
fn rig(app: &EditorApp) -> std::sync::Arc<Rig> {
    app.assets.model(NOODLE).unwrap().rig.clone().unwrap()
}

/// How far a move of the noodle's Hat goes: down and to the side, a fifth
/// of the model's height each way.
fn hat_move(app: &EditorApp) -> Vec3 {
    let height = app.assets.model(NOODLE).unwrap().size[1];
    Vec3::new(0.2, -0.2, 0.0) * height
}

/// Each bone's distance from its parent, in the rig's file space.
fn lengths(rig: &Rig, pose: &[BoneTransform]) -> Vec<f32> {
    let joints = joint_positions(rig, pose);
    rig.bones
        .iter()
        .enumerate()
        .map(|(bone, info)| {
            info.parent
                .map_or(0.0, |p| joints[p].distance(joints[bone]))
        })
        .collect()
}

fn assert_lengths_kept(rig: &Rig, pose: &[BoneTransform]) {
    for (bone, (a, b)) in lengths(rig, &rig.rest_pose())
        .iter()
        .zip(lengths(rig, pose))
        .enumerate()
    {
        assert!(
            (a - b).abs() <= a * 1e-4 + 1e-5,
            "bone {bone}: {a} became {b}"
        );
    }
}

/// The targets of the open clip's bone tracks, sorted.
fn keyed_bones(app: &EditorApp) -> Vec<String> {
    let mut targets: Vec<String> = app
        .animation
        .open_tracks()
        .into_iter()
        .filter(|t| t.property == PropertyName::Bone)
        .map(|t| t.target)
        .collect();
    targets.sort();
    targets
}

/// The Animate clip of the noodle, opened with Record on.
fn animate_noodle(app: &mut EditorApp) -> (InstanceId, String) {
    let noodle = part(app, "Noodle");
    app.animate(noodle);
    assert!(app.animation.recording(), "Animate turns Record on");
    let clip = crate::animation::Subject::of(&app.world, noodle)
        .unwrap()
        .animations[0]
        .clip
        .clone();
    (noodle, clip)
}

#[test]
fn a_pose_made_with_ik_and_keyed_with_record_plays_the_same_in_the_preview_and_in_play() {
    let folder = copy_of_example("hello-animation", "ik-record");
    let mut app = EditorApp::new(Some(&folder));
    let (noodle, _) = animate_noodle(&mut app);
    let rig = rig(&app);
    let by = hat_move(&app);
    let rest = joint_positions(&rig, &rig.rest_pose());
    let hat = rig.bone("Hat").unwrap();
    {
        let (studio, assets, world) = (&mut app.animation, &app.assets, &app.world);
        // At rest at 0 s; then at 1 s drag the Hat, which turns Top and Mid.
        for name in ["Mid", "Top"] {
            studio.pick_bone(assets, world, name);
            studio.set_playhead(0.0);
            studio.key_picked(assets, world).unwrap();
        }
        studio.pick_bone(assets, world, "Hat");
        studio.set_playhead(1.0);
        let slipped = studio.move_joint(assets, world, by).unwrap();
        assert!(slipped.is_empty());
    }
    assert_eq!(
        keyed_bones(&app),
        ["Root/Mid", "Root/Mid/Top"],
        "only what turned"
    );
    for track in app.animation.open_tracks() {
        let times: Vec<f64> = track.keys.iter().map(|k| k.time).collect();
        assert_eq!(times, [0.0, 1.0], "{}", track.target);
    }
    app.animation.set_playhead(1.0);
    let posed = app.animation.preview_bones(&app.assets, &app.world);
    assert_lengths_kept(&rig, &posed);
    let moved = joint_positions(&rig, &posed);
    assert!(moved[hat].distance(rest[hat]) > 1e-3, "the Hat moved");

    let _ = app.animation.write_all(&folder, &mut app.output);
    plays_as_previewed(&mut app, noodle, "an IK pose");
    let _ = fs::remove_dir_all(&folder);
}

#[test]
fn record_and_k_key_only_the_bones_a_moved_joint_turned() {
    let folder = copy_of_example("hello-animation", "ik-keys");
    let mut app = EditorApp::new(Some(&folder));
    animate_noodle(&mut app);
    app.animation.set_record(false);
    let by = hat_move(&app);
    {
        let (studio, assets, world) = (&mut app.animation, &app.assets, &app.world);
        studio.pick_bone(assets, world, "Hat");
        studio.set_playhead(0.5);
        studio.move_joint(assets, world, by).unwrap();
        assert!(
            studio.open_tracks().is_empty(),
            "nothing is keyed without Record"
        );
        // K keys Top and Mid, which the move turned, not the Hat (picked,
        // but not turned) or the Root.
        studio.key_picked(assets, world).unwrap();
    }
    assert_eq!(keyed_bones(&app), ["Root/Mid", "Root/Mid/Top"]);
    // Each bone's first key after 0 also keys it at rest at 0.
    for track in app.animation.open_tracks() {
        let times: Vec<f64> = track.keys.iter().map(|k| k.time).collect();
        assert_eq!(times, [0.0, 0.5], "{}", track.target);
    }
    // A chain of 1 turns only Top, so Record keys only Top.
    app.animation.set_record(true);
    app.animation.set_chain(1);
    {
        let (studio, assets, world) = (&mut app.animation, &app.assets, &app.world);
        studio.set_playhead(1.0);
        studio.move_joint(assets, world, by * 0.5).unwrap();
    }
    let tracks = app.animation.open_tracks();
    let keys_of = |target: &str| {
        tracks
            .iter()
            .find(|t| t.target == target)
            .map_or(0, |t| t.keys.len())
    };
    assert_eq!((keys_of("Root/Mid"), keys_of("Root/Mid/Top")), (2, 3));

    // In a linked clip, Mid is the take's, so the chain stops below it:
    // only Top turns and is keyed.
    app.animation.open_for(
        &folder,
        &app.assets,
        part(&app, "Noodle"),
        IDLE,
        &mut app.world,
        &mut app.output,
    );
    app.animation.set_record(false);
    app.animation.set_chain(2);
    {
        let (studio, assets, world) = (&mut app.animation, &app.assets, &app.world);
        studio.pick_bone(assets, world, "Hat");
        studio.set_playhead(0.5);
        studio.move_joint(assets, world, by).unwrap();
        studio.key_picked(assets, world).unwrap();
    }
    assert_eq!(keyed_bones(&app), ["Root/Mid/Top"]);
    let _ = fs::remove_dir_all(&folder);
}

#[test]
fn undoing_an_ik_pose_puts_back_the_keys_and_redo_brings_back_the_same_pose() {
    let folder = copy_of_example("hello-animation", "ik-undo");
    let mut app = EditorApp::new(Some(&folder));
    let (_, clip) = animate_noodle(&mut app);
    let by = hat_move(&app);
    let read = |folder: &std::path::Path| {
        ClipFile::from_json(&fs::read_to_string(folder.join(&clip)).unwrap()).unwrap()
    };
    {
        let (studio, assets, world) = (&mut app.animation, &app.assets, &app.world);
        for name in ["Mid", "Top"] {
            studio.pick_bone(assets, world, name);
            studio.set_playhead(0.0);
            studio.key_picked(assets, world).unwrap();
        }
    }
    record_clip_edit(&mut app, &folder);
    let at_rest = read(&folder);
    let pose_at_one = |app: &mut EditorApp| {
        app.animation.set_playhead(1.0);
        app.animation.preview_bones(&app.assets, &app.world)
    };
    let before = pose_at_one(&mut app);
    {
        let (studio, assets, world) = (&mut app.animation, &app.assets, &app.world);
        studio.pick_bone(assets, world, "Hat");
        studio.set_playhead(1.0);
        studio.move_joint(assets, world, by).unwrap();
    }
    record_clip_edit(&mut app, &folder);
    let posed = read(&folder);
    let after = pose_at_one(&mut app);
    assert_ne!(before, after, "the move changed the pose");

    app.undo();
    assert_eq!(read(&folder), at_rest);
    assert_eq!(app.animation.open_tracks(), at_rest.tracks);
    assert_eq!(pose_at_one(&mut app), before, "the pose before the move");
    app.redo();
    assert_eq!(read(&folder), posed);
    assert_eq!(app.animation.open_tracks(), posed.tracks);
    assert_eq!(pose_at_one(&mut app), after, "the same pose again");
    let _ = fs::remove_dir_all(&folder);
}

#[test]
fn pins_stay_with_their_clip_while_the_editor_runs_and_are_never_saved() {
    let folder = copy_of_example("hello-animation", "ik-pins");
    let mut app = EditorApp::new(Some(&folder));
    let (noodle, clip) = animate_noodle(&mut app);
    {
        let (studio, assets, world) = (&mut app.animation, &app.assets, &app.world);
        studio.pick_bone(assets, world, "Hat");
        studio.pin_picked(assets, world, true);
        assert_eq!(studio.pinned_bones(assets, world), ["Hat"]);
    }
    let _ = app.animation.write_all(&folder, &mut app.output);
    let open = |app: &mut EditorApp, path: &str| {
        let _ = app.animation.write_all(&folder, &mut app.output);
        app.animation.open_for(
            &folder,
            &app.assets,
            noodle,
            path,
            &mut app.world,
            &mut app.output,
        );
    };

    // Another clip has its own pins.
    open(&mut app, WALK);
    {
        let (studio, assets, world) = (&mut app.animation, &app.assets, &app.world);
        assert!(studio.pinned_bones(assets, world).is_empty());
        studio.pick_bone(assets, world, "Top");
        studio.pin_picked(assets, world, true);
    }
    open(&mut app, &clip);
    assert_eq!(app.animation.pinned_bones(&app.assets, &app.world), ["Hat"]);
    open(&mut app, WALK);
    assert_eq!(app.animation.pinned_bones(&app.assets, &app.world), ["Top"]);

    // Nothing about pins is in the files, and a new editor has none.
    open(&mut app, &clip);
    for path in [&clip[..], WALK] {
        let text = fs::read_to_string(folder.join(path)).unwrap();
        assert!(!text.to_lowercase().contains("pin"), "{path}: {text}");
    }
    drop(app);
    let mut app = EditorApp::new(Some(&folder));
    let noodle = part(&app, "Noodle");
    app.animation.open_for(
        &folder,
        &app.assets,
        noodle,
        &clip,
        &mut app.world,
        &mut app.output,
    );
    assert!(
        app.animation
            .pinned_bones(&app.assets, &app.world)
            .is_empty()
    );
    let _ = fs::remove_dir_all(&folder);
}

#[test]
fn a_pinned_joint_holds_while_another_bone_turns() {
    let folder = copy_of_example("hello-animation", "ik-pin-holds");
    let mut app = EditorApp::new(Some(&folder));
    animate_noodle(&mut app);
    let rig = rig(&app);
    let hat = rig.bone("Hat").unwrap();
    // Straight up, toward the Hat: the noodle stands straight, so any other
    // way would take the Hat out of reach of Top and Mid.
    let by = Vec3::new(0.0, hat_move(&app).length() * 0.3, 0.0);
    {
        let (studio, assets, world) = (&mut app.animation, &app.assets, &app.world);
        studio.set_playhead(0.0);
        studio.pick_bone(assets, world, "Hat");
        studio.pin_picked(assets, world, true);
        // Moving the Root drags everything with it, but Top and Mid turn
        // to keep the Hat where it was pinned.
        studio.pick_bone(assets, world, "Root");
        let slipped = studio.move_joint(assets, world, by).unwrap();
        assert!(slipped.is_empty(), "{slipped:?}");
    }
    let posed = app.animation.preview_bones(&app.assets, &app.world);
    let before = joint_positions(&rig, &rig.rest_pose());
    let now = joint_positions(&rig, &posed);
    let root = rig.bone("Root").unwrap();
    assert!(now[root].distance(before[root]) > 1e-3, "the Root moved");
    assert!(now[hat].distance(before[hat]) < 1e-3, "the Hat held");
    assert_eq!(
        keyed_bones(&app),
        ["Root", "Root/Mid", "Root/Mid/Top"],
        "the Root and the bones that held the pin are keyed, not the Hat"
    );
    let _ = fs::remove_dir_all(&folder);
}
