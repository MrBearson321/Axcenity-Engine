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
use crate::animation::NewKeys;
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

/// Writes the open clip and records the edit as one undo step, as the
/// editor does each frame.
pub(super) fn record_clip_edit(app: &mut EditorApp, folder: &std::path::Path) {
    for action in app.animation.write_all(folder, &mut app.output) {
        if let StudioAction::RecordFile {
            path,
            before,
            after,
            label,
        } = action
        {
            app.history.record_files(
                &mut app.world,
                label,
                vec![crate::history::TextChange {
                    path,
                    before: Some(before),
                    after: Some(after),
                }],
            );
        }
    }
}

/// Plays the noodle's first Animation on its own (no state machine) and
/// checks, twice, that the game draws it as the preview does at the
/// Animation's time then.
pub(super) fn plays_as_previewed(app: &mut EditorApp, noodle: InstanceId, what: &str) {
    let model = app.assets.model(NOODLE).unwrap();
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
    for _ in 0..2 {
        play_for(app, 0.3);
        let time = app
            .play
            .as_ref()
            .unwrap()
            .world()
            .property_animation(animation)
            .expect("it's playing")
            .time;
        assert!(time > 0.1 && time < 0.95, "{time}");
        app.animation.set_playhead(time);
        let previewed = app.animation.preview_bones(&app.assets, &app.world);
        let shown = model.pose(&previewed).unwrap();
        assert_same_shape(&played(app, noodle), &shown, &format!("{what} at {time} s"));
    }
    app.stop();
}

#[test]
fn smooth_linear_and_constant_keys_made_by_posing_play_the_same_in_the_preview_and_in_play() {
    let folder = copy_of_example("hello-animation", "eased-wave");
    let mut app = EditorApp::new(Some(&folder));
    let noodle = part(&app, "Noodle");
    let rig = app.assets.model(NOODLE).unwrap().rig.clone().unwrap();
    let rest = rig.rest_pose();
    app.animate(noodle);
    assert_eq!(
        app.animation.new_keys(),
        NewKeys::Smooth,
        "Smooth to start with"
    );

    // Top is keyed Smooth, Mid Linear and Hat Constant: each with K at
    // rest, then turned 60° at 1 s with Record on.
    let picks = [
        ("Top", "Root/Mid/Top", NewKeys::Smooth),
        ("Mid", "Root/Mid", NewKeys::Linear),
        ("Hat", "Root/Mid/Top/Hat", NewKeys::Constant),
    ];
    {
        let (studio, assets, world) = (&mut app.animation, &app.assets, &app.world);
        for (name, _, choice) in picks {
            studio.set_new_keys(choice);
            studio.pick_bone(assets, world, name);
            studio.set_playhead(0.0);
            studio.key_picked(assets, world).unwrap();
            studio.set_playhead(1.0);
            studio
                .turn_bone(assets, world, Quat::from_rotation_z(FRAC_PI_3))
                .unwrap();
        }
    }
    let tracks = app.animation.open_tracks();
    for (_, path, choice) in picks {
        let track = tracks.iter().find(|t| t.target == path).unwrap();
        assert_eq!(track.keys.len(), 2, "{path}");
        for key in &track.keys {
            assert_eq!((key.interp, key.easing), choice.easing(), "{path}");
        }
    }

    // A quarter of the way: Top eases in, Mid is a quarter turned, and Hat
    // hasn't moved yet.
    app.animation.set_playhead(0.25);
    let quarter = app.animation.preview_bones(&app.assets, &app.world);
    let [top, mid, hat] = ["Top", "Mid", "Hat"].map(|name| rig.bone(name).unwrap());
    let eased = (1.0 - (std::f32::consts::PI * 0.25).cos()) / 2.0;
    assert!((turn_of(&quarter, &rest, top) - FRAC_PI_3 * eased).abs() < 1e-3);
    assert!((turn_of(&quarter, &rest, mid) - FRAC_PI_3 * 0.25).abs() < 1e-3);
    assert!(turn_of(&quarter, &rest, hat) < 1e-4);
    app.animation.set_playhead(0.75);
    let late = app.animation.preview_bones(&app.assets, &app.world);
    assert!((turn_of(&late, &rest, top) - FRAC_PI_3 * (1.0 - eased)).abs() < 1e-3);
    assert!(turn_of(&late, &rest, hat) < 1e-4);
    let _ = app.animation.write_all(&folder, &mut app.output);
    plays_as_previewed(&mut app, noodle, "Smooth, Linear and Constant");

    // Easing ▸ Linear on Top's two keys makes its motion linear, there too.
    app.animation.ease_keys_of("Root/Mid/Top", NewKeys::Linear);
    app.animation.set_playhead(0.25);
    let quarter = app.animation.preview_bones(&app.assets, &app.world);
    assert!((turn_of(&quarter, &rest, top) - FRAC_PI_3 * 0.25).abs() < 1e-3);
    let _ = app.animation.write_all(&folder, &mut app.output);
    plays_as_previewed(&mut app, noodle, "Top made Linear");
    let _ = fs::remove_dir_all(&folder);
}

#[test]
fn a_linked_clips_own_bone_keys_use_new_keys_which_stays_picked_from_clip_to_clip() {
    let folder = copy_of_example("hello-animation", "linked-new-keys");
    let mut app = EditorApp::new(Some(&folder));
    let noodle = part(&app, "Noodle");
    app.animation.open_for(
        &folder,
        &app.assets,
        noodle,
        IDLE,
        &mut app.world,
        &mut app.output,
    );
    app.animation.set_record(true);
    app.animation.set_new_keys(NewKeys::Constant);
    {
        let (studio, assets, world) = (&mut app.animation, &app.assets, &app.world);
        studio.pick_bone(assets, world, "Top");
        studio.set_playhead(0.5);
        studio
            .turn_bone(assets, world, Quat::from_rotation_x(0.4))
            .unwrap();
        studio.set_new_keys(NewKeys::Linear);
        studio.set_playhead(0.9);
        studio.key_picked(assets, world).unwrap();
    }
    let tracks = app.animation.open_tracks();
    let kinds: Vec<_> = tracks[0]
        .keys
        .iter()
        .map(|k| (k.interp, k.easing))
        .collect();
    assert_eq!(
        kinds,
        [NewKeys::Constant.easing(), NewKeys::Linear.easing()]
    );
    // Another clip opens with the same New keys.
    let _ = app.animation.write_all(&folder, &mut app.output);
    app.animation.open_for(
        &folder,
        &app.assets,
        noodle,
        "assets/animations/noodle/Walk.anim.json",
        &mut app.world,
        &mut app.output,
    );
    assert_eq!(app.animation.new_keys(), NewKeys::Linear);
    let _ = fs::remove_dir_all(&folder);
}

#[test]
fn making_keys_and_changing_their_easing_undo_and_redo_with_their_values_and_easing() {
    let folder = copy_of_example("hello-animation", "easing-undo");
    let mut app = EditorApp::new(Some(&folder));
    let noodle = part(&app, "Noodle");
    app.animate(noodle);
    let clip = crate::animation::Subject::of(&app.world, noodle)
        .unwrap()
        .animations[0]
        .clip
        .clone();
    let read = |folder: &std::path::Path| {
        ClipFile::from_json(&fs::read_to_string(folder.join(&clip)).unwrap()).unwrap()
    };
    let animated = read(&folder);
    assert!(animated.tracks.is_empty());

    // Two Smooth keys on Top, as one step; then Easing ▸ Constant, another.
    {
        let (studio, assets, world) = (&mut app.animation, &app.assets, &app.world);
        studio.pick_bone(assets, world, "Top");
        studio.set_playhead(0.0);
        studio.key_picked(assets, world).unwrap();
        studio.set_playhead(1.0);
        studio
            .turn_bone(assets, world, Quat::from_rotation_z(FRAC_PI_3))
            .unwrap();
    }
    record_clip_edit(&mut app, &folder);
    let keyed = read(&folder);
    assert_eq!(keyed.tracks[0].keys.len(), 2);
    assert!(
        keyed.tracks[0]
            .keys
            .iter()
            .all(|k| (k.interp, k.easing) == NewKeys::Smooth.easing())
    );
    app.animation
        .ease_keys_of("Root/Mid/Top", NewKeys::Constant);
    record_clip_edit(&mut app, &folder);
    let eased = read(&folder);
    assert!(
        eased.tracks[0]
            .keys
            .iter()
            .all(|k| (k.interp, k.easing) == NewKeys::Constant.easing())
    );
    let values = |file: &ClipFile| -> Vec<(f64, Vec<f32>)> {
        file.tracks[0]
            .keys
            .iter()
            .map(|k| (k.time, k.value.clone()))
            .collect()
    };
    assert_eq!(values(&eased), values(&keyed), "only the easing changed");

    // Undo puts the Smooth keys back, file and open clip; redo Constant.
    app.undo();
    assert_eq!(read(&folder), keyed);
    assert_eq!(app.animation.open_tracks(), keyed.tracks);
    app.redo();
    assert_eq!(read(&folder), eased);
    assert_eq!(app.animation.open_tracks(), eased.tracks);
    // Undo both: no keys. Redo both: the same keys, values and easings.
    app.undo();
    app.undo();
    assert_eq!(read(&folder), animated);
    assert!(app.animation.open_tracks().is_empty());
    app.redo();
    assert_eq!(read(&folder), keyed);
    app.redo();
    assert_eq!(read(&folder), eased);
    assert_eq!(app.animation.open_tracks(), eased.tracks);
    let _ = fs::remove_dir_all(&folder);
}
