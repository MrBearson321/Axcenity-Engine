//! Posing skeletal rigs in the Animation tab: a wave keyed on the noodle's
//! bones plays in the preview, in a state machine state and in Play, and a
//! linked clip's take bones are locked until it's converted to keys.

use std::f32::consts::FRAC_PI_3;
use std::fs;

use axcenity_anim::{ClipFile, Motion, PropertyName, StateDef, StateMachineFile};
use axcenity_assets::{BoneTransform, Posed};
use axcenity_render::glam::Quat;

use super::tests::{copy_of_example, part, play_for};
use super::*;
use crate::animation::StudioAction;

const NOODLE: &str = "assets/models/noodle.fbx";
const MACHINE: &str = "assets/animations/noodle/noodle.statemachine.json";
const IDLE: &str = "assets/animations/noodle/Idle.anim.json";

/// How far `bone` is turned from where it is in `rest`, in radians.
fn turn_of(pose: &[BoneTransform], rest: &[BoneTransform], bone: usize) -> f32 {
    pose[bone].rotation.angle_between(rest[bone].rotation)
}

fn assert_same_poses(a: &[BoneTransform], b: &[BoneTransform], what: &str) {
    for (bone, (a, b)) in a.iter().zip(b).enumerate() {
        assert!(
            a.translation.abs_diff_eq(b.translation, 1e-3)
                && a.rotation.angle_between(b.rotation) < 2e-3
                && a.scale.abs_diff_eq(b.scale, 1e-3),
            "{what}: bone {bone}: {a:?} vs {b:?}"
        );
    }
}

/// Whether two posed models put every corner in the same place.
fn assert_same_shape(a: &Posed, b: &Posed, what: &str) {
    assert_eq!(a.primitives.len(), b.primitives.len());
    for (a, b) in a.primitives.iter().zip(&b.primitives) {
        for (a, b) in a.positions.iter().zip(&b.positions) {
            let off = (0..3).map(|i| (a[i] - b[i]).abs()).fold(0.0, f32::max);
            assert!(off < 1e-2, "{what}: {a:?} vs {b:?}");
        }
    }
}

/// The noodle as the game draws it now, in Play.
fn played(app: &EditorApp, noodle: InstanceId) -> std::sync::Arc<Posed> {
    app.play
        .as_ref()
        .expect("playing")
        .poses()
        .get(&noodle)
        .cloned()
        .expect("the noodle is posed")
}

#[test]
fn a_three_key_wave_made_on_an_imported_rig_plays_in_the_preview_a_state_and_play() {
    let folder = copy_of_example("hello-animation", "wave");
    let mut app = EditorApp::new(Some(&folder));
    let noodle = part(&app, "Noodle");
    let model = app.assets.model(NOODLE).unwrap();
    let rig = model.rig.clone().unwrap();
    let rest = rig.rest_pose();
    let top = rig.bone("Top").unwrap();

    // Animate opens a keyed clip with Record on; pick Top's joint and key
    // it at rest, turned 60°, and back.
    app.animate(noodle);
    let clip = crate::animation::Subject::of(&app.world, noodle)
        .unwrap()
        .animations[0]
        .clip
        .clone();
    {
        let (studio, assets, world) = (&mut app.animation, &app.assets, &app.world);
        studio.pick_bone(assets, world, "Top");
        studio.set_playhead(0.0);
        studio.key_picked(assets, world).unwrap();
        studio.set_playhead(0.5);
        studio
            .turn_bone(assets, world, Quat::from_rotation_z(FRAC_PI_3))
            .unwrap();
        studio.set_playhead(1.0);
        studio
            .turn_bone(assets, world, Quat::from_rotation_z(-FRAC_PI_3))
            .unwrap();
    }
    let tracks = app.animation.open_tracks();
    let [track] = tracks.as_slice() else {
        panic!("one track: {tracks:?}");
    };
    assert_eq!(
        (track.property, track.target.as_str()),
        (PropertyName::Bone, "Root/Mid/Top")
    );
    let times: Vec<f64> = track.keys.iter().map(|key| key.time).collect();
    assert_eq!(times, [0.0, 0.5, 1.0]);

    // The preview plays it: Top waves up to 60° and back, and nothing else moves.
    let mut preview_at = |seconds: f64| {
        app.animation.set_playhead(seconds);
        app.animation.preview_bones(&app.assets, &app.world)
    };
    let (start, quarter, wave, end) = (
        preview_at(0.0),
        preview_at(0.25),
        preview_at(0.5),
        preview_at(1.0),
    );
    assert!(turn_of(&start, &rest, top) < 1e-3);
    assert!((turn_of(&quarter, &rest, top) - FRAC_PI_3 / 2.0).abs() < 1e-2);
    assert!((turn_of(&wave, &rest, top) - FRAC_PI_3).abs() < 1e-3);
    assert!(turn_of(&end, &rest, top) < 1e-3);
    for bone in (0..rest.len()).filter(|&bone| bone != top) {
        assert!(turn_of(&wave, &rest, bone) < 1e-6, "bone {bone} stays");
    }
    let shown = model.pose(&wave).unwrap();
    let _ = app.animation.write_all(&folder, &mut app.output);
    let saved = ClipFile::from_json(&fs::read_to_string(folder.join(&clip)).unwrap()).unwrap();
    assert_eq!(saved.tracks, tracks);

    // In a state machine state: the noodle's state machine starts in Wave.
    let mut machine =
        StateMachineFile::from_json(&fs::read_to_string(folder.join(MACHINE)).unwrap()).unwrap();
    machine.states.push(StateDef::new(
        "Wave",
        Motion::Clip(clip.clone()),
        [0.0, 240.0],
    ));
    machine.default_state = "Wave".into();
    fs::write(folder.join(MACHINE), machine.to_json()).unwrap();
    app.play();
    play_for(&mut app, 0.5);
    assert_same_shape(&played(&app, noodle), &shown, "the Wave state in Play");
    app.stop();

    // Played by its Animation in Play, with no state machine.
    let animator = crate::animation::animator_of(&app.world, noodle).unwrap();
    let animation = crate::animation::Subject::of(&app.world, noodle)
        .unwrap()
        .animations[0]
        .id;
    app.world
        .set_property(animator, "StateMachine", Value::String(String::new()))
        .unwrap();
    app.world
        .set_property(animation, "AutoPlay", Value::Bool(true))
        .unwrap();
    app.play();
    play_for(&mut app, 0.5);
    assert_same_shape(&played(&app, noodle), &shown, "the Animation in Play");
    app.stop();
    let _ = fs::remove_dir_all(&folder);
}

#[test]
fn a_linked_clips_take_bones_are_locked_until_convert_to_keys_which_looks_the_same() {
    let folder = copy_of_example("hello-animation", "convert-to-keys");
    let mut app = EditorApp::new(Some(&folder));
    let noodle = part(&app, "Noodle");
    let rest = app
        .assets
        .model(NOODLE)
        .unwrap()
        .rig
        .clone()
        .unwrap()
        .rest_pose();
    app.animation.open_for(
        &folder,
        &app.assets,
        noodle,
        IDLE,
        &mut app.world,
        &mut app.output,
    );
    assert!(app.animation.open_badge().contains("Linked to noodle.fbx"));
    app.animation.set_record(true);

    // Idle moves Mid: it's locked. Top isn't, and keys alongside the take.
    {
        let (studio, assets, world) = (&mut app.animation, &app.assets, &app.world);
        studio.pick_bone(assets, world, "Mid");
        studio.set_playhead(0.5);
        let refused = studio.turn_bone(assets, world, Quat::from_rotation_x(0.4));
        assert!(refused.unwrap_err().contains("moved by the take"));
        assert!(studio.key_picked(assets, world).is_err());
        assert!(studio.open_tracks().is_empty(), "nothing was keyed");
        studio.pick_bone(assets, world, "Top");
        studio
            .turn_bone(assets, world, Quat::from_rotation_x(0.4))
            .unwrap();
    }
    let tracks = app.animation.open_tracks();
    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0].target, "Root/Mid/Top");
    // The drawer shows Mid locked and Top's key; with `AXCENITY_SNAPSHOTS`
    // it's saved as `animation-linked-bones.png`.
    let mut draw = |ctx: &egui::Context| {
        egui::CentralPanel::default().show(ctx, |ui| app.animation_pane(ui, None));
    };
    match std::env::var_os("AXCENITY_SNAPSHOTS") {
        Some(folder) => crate::snapshot::snapshot(
            &std::path::Path::new(&folder).join("animation-linked-bones.png"),
            egui::vec2(1100.0, 600.0),
            vec![Vec::new(); 3],
            &mut draw,
        ),
        None => {
            let ctx = theme::test_context();
            for _ in 0..3 {
                let _ = ctx.run(Default::default(), &mut draw);
            }
        }
    }
    let paths = app.animation.preview_bone_paths(&app.assets);
    let mid = paths.iter().position(|path| path == "Root/Mid").unwrap();
    let top = paths
        .iter()
        .position(|path| path == "Root/Mid/Top")
        .unwrap();

    let times = [0.0, 0.4, 0.5, 0.9, 1.3, 1.9];
    let poses_at = |app: &mut EditorApp| -> Vec<Vec<BoneTransform>> {
        times
            .iter()
            .map(|&seconds| {
                app.animation.set_playhead(seconds);
                app.animation.preview_bones(&app.assets, &app.world)
            })
            .collect()
    };
    let linked = poses_at(&mut app);
    assert!(turn_of(&linked[2], &rest, mid) > 0.2, "Idle sways Mid");
    assert!(turn_of(&linked[2], &rest, top) > 0.3, "Top's own key plays");

    // Convert to keys: one undo step; the clip is keyed and looks the same.
    let _ = app.animation.write_all(&folder, &mut app.output);
    let before = fs::read_to_string(folder.join(IDLE)).unwrap();
    let actions =
        app.animation
            .convert_open_clip(&folder, &app.assets, &mut app.world, &mut app.output);
    for action in actions {
        if let StudioAction::RecordFiles {
            label,
            files,
            rebind_controller,
        } = action
        {
            app.record_asset_files(label, files, rebind_controller);
        }
    }
    assert_eq!(app.history.undo_label(), Some("Convert Idle to keys"));
    assert!(app.animation.open_badge().contains("Keyed"));
    let converted = ClipFile::from_json(&fs::read_to_string(folder.join(IDLE)).unwrap()).unwrap();
    assert!(converted.link.is_none());
    let keyed = poses_at(&mut app);
    for (at, (linked, keyed)) in times.iter().zip(linked.iter().zip(&keyed)) {
        assert_same_poses(linked, keyed, &format!("at {at} s"));
    }

    // Undo puts the link back; redo the keys.
    app.undo();
    assert_eq!(fs::read_to_string(folder.join(IDLE)).unwrap(), before);
    assert!(app.animation.open_badge().contains("Linked"));
    app.redo();
    assert!(app.animation.open_badge().contains("Keyed"));

    // Now Mid can be keyed: turning it changes the clip.
    {
        let (studio, assets, world) = (&mut app.animation, &app.assets, &app.world);
        studio.set_record(true);
        studio.pick_bone(assets, world, "Mid");
        studio.set_playhead(0.5);
        let was = studio.preview_bones(assets, world)[mid];
        studio
            .turn_bone(assets, world, Quat::from_rotation_x(0.4))
            .unwrap();
        let now = studio.preview_bones(assets, world)[mid];
        assert!(now.rotation.angle_between(was.rotation) > 0.39);
    }
    let _ = fs::remove_dir_all(&folder);
}
