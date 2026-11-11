//! Imported clips you can see, tidier trees, and Open ▾'s **not used**: an
//! imported FBX clip shows its take as read-only keys (exactly what
//! Convert to keys makes), its bone tree keeps its folds while switching
//! clips, and clips and state machines nothing uses are marked.

use std::fs;

use axcenity_anim::{ClipFile, PropertyTrack, bone_paths, convert_to_keys, find_bone, take_bones};
use axcenity_assets::BoneTransform;
use axcenity_render::glam::Quat;

use super::tests::{copy_of_example, example, part};
use super::*;
use crate::animation::{StudioAction, Subject};

/// Imports the Mixamo walk from `hello-events` with the dialog's choices
/// as they start. Returns the new MeshPart and its Animations' clips.
fn import_walking(app: &mut EditorApp) -> (InstanceId, Vec<String>) {
    app.import_model(&example("hello-events").join("assets/models/Walking/Walking.fbx"));
    let dialog = app.importing.take().expect("an animated file asks first");
    app.import_chosen(&dialog);
    let part = app.selected.expect("the new MeshPart is selected");
    let clips = Subject::of(&app.world, part)
        .unwrap()
        .animations
        .iter()
        .map(|animation| animation.clip.clone())
        .collect();
    (part, clips)
}

/// What Convert to keys makes of the open linked clip, on `mesh_id`'s
/// skeleton, for the bones its take moves.
fn converted_take_bones(app: &EditorApp, mesh_id: &str) -> Vec<PropertyTrack> {
    let file = app.animation.open_clip_file();
    let link = file.link.clone().expect("a linked clip");
    let takes = app.assets.takes(&link.source).unwrap();
    let model = app.assets.model_blocking(mesh_id).unwrap();
    let rig = model.rig.as_deref().unwrap();
    let take = takes.iter().find(|take| take.name == link.take).unwrap();
    let moved = take_bones(take, rig);
    let paths = bone_paths(rig);
    convert_to_keys(&file, takes.clone(), rig)
        .unwrap()
        .tracks
        .into_iter()
        .filter(|track| find_bone(rig, &paths, &track.target).is_some_and(|bone| moved[bone]))
        .collect()
}

/// Draws the Animation tab for a few frames, saved as `name` when
/// `AXCENITY_SNAPSHOTS` names a folder.
fn draw_tab(app: &mut EditorApp, name: &str) {
    let mut draw = |ui: &mut egui::Ui| {
        egui::CentralPanel::default().show(ui, |ui| app.animation_pane(ui, None));
    };
    match std::env::var_os("AXCENITY_SNAPSHOTS") {
        Some(folder) => crate::snapshot::snapshot(
            &std::path::Path::new(&folder).join(name),
            egui::vec2(1200.0, 760.0),
            vec![Vec::new(); 3],
            &mut draw,
        ),
        None => {
            let ctx = theme::test_context();
            for _ in 0..3 {
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1200.0, 760.0),
                    )),
                    ..Default::default()
                };
                let _ = crate::theme::run(&ctx, input, |ui| draw(ui));
            }
        }
    }
}

fn assert_same_poses(a: &[BoneTransform], b: &[BoneTransform], what: &str) {
    for (bone, (a, b)) in a.iter().zip(b).enumerate() {
        assert!(
            a.translation.abs_diff_eq(b.translation, 1e-2)
                && a.rotation.angle_between(b.rotation) < 5e-3
                && a.scale.abs_diff_eq(b.scale, 1e-3),
            "{what}: bone {bone}: {a:?} vs {b:?}"
        );
    }
}

#[test]
fn an_imported_fbx_clip_shows_its_take_as_the_keys_convert_to_keys_makes_and_they_cant_change() {
    let folder = copy_of_example("hello-part", "imported-keys");
    let mut app = EditorApp::new(Some(&folder));
    let (walker, clips) = import_walking(&mut app);
    let mesh_id = app
        .world
        .get(walker)
        .and_then(|part| part.property("MeshId"))
        .and_then(Value::as_str)
        .unwrap()
        .to_owned();
    app.animation.open_for(
        &folder,
        &app.assets,
        walker,
        &clips[0],
        &mut app.world,
        &mut app.output,
    );
    assert!(app.animation.open_badge().contains("Linked"));

    // The whole take: one row per bone it moves, exactly Convert to keys'.
    let whole = converted_take_bones(&app, &mesh_id);
    assert!(whole.len() > 20, "a Mixamo walk moves many bones");
    let shown = app.animation.imported_view(&app.assets).file.tracks.clone();
    assert_eq!(shown, whole);

    // A clip starting and ending partway through the take, on whole
    // frames as the clip's Start and End set them.
    let length = app.animation.open_clip_file().link.unwrap().end;
    let fps = 30.0;
    let frame = |at: f64| (at * fps).round() / fps;
    app.animation
        .set_clip_range(frame(length * 0.3), frame(length * 0.7));
    let part_way = converted_take_bones(&app, &mesh_id);
    let shown = app.animation.imported_view(&app.assets).file.tracks.clone();
    assert_eq!(shown, part_way);
    assert_ne!(part_way, whole);
    let duration = app.animation.open_clip_file().duration;
    let last_key = shown
        .iter()
        .flat_map(|track| track.keys.iter().map(|key| key.time))
        .fold(0.0, f64::max);
    assert!((last_key - duration).abs() < 1e-6, "keys span the clip");

    // They show in the dope sheet and as curves. With `AXCENITY_SNAPSHOTS`
    // they're saved as `imported-keys-dope-sheet.png` and `…-curves.png`.
    draw_tab(&mut app, "imported-keys-dope-sheet.png");
    assert!(app.animation.imported_view(&app.assets).sheet.width() > 100.0);
    app.animation.show_curves(true);
    draw_tab(&mut app, "imported-keys-curves.png");
    let view = app.animation.imported_view(&app.assets);
    assert!(view.curve_view.rect.width() > 100.0);

    // Whatever happens to them, they're the take's again.
    view.file.tracks[0].keys.clear();
    view.file.tracks.pop();
    assert_eq!(
        app.animation.imported_view(&app.assets).file.tracks,
        part_way
    );

    // Convert to keys makes those keys, editable, and the motion stays.
    let times = [0.0, duration * 0.25, duration * 0.6, duration];
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
    let _ = app.animation.write_all(&folder, &mut app.output);
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
    assert!(app.animation.open_badge().contains("Keyed"));
    let converted =
        ClipFile::from_json(&fs::read_to_string(folder.join(&clips[0])).unwrap()).unwrap();
    assert!(converted.link.is_none());
    // The same keys, give or take the last digit of a time read back from
    // the file.
    let saved = app.animation.open_tracks();
    assert_eq!(saved.len(), part_way.len());
    for (saved, shown) in saved.iter().zip(&part_way) {
        assert_eq!(saved.target, shown.target);
        assert_eq!(saved.keys.len(), shown.keys.len(), "{}", saved.target);
        for (a, b) in saved.keys.iter().zip(&shown.keys) {
            assert!((a.time - b.time).abs() < 1e-9, "{}", saved.target);
            assert_eq!(a.value, b.value, "{}", saved.target);
        }
    }
    let keyed = poses_at(&mut app);
    for (at, (linked, keyed)) in times.iter().zip(linked.iter().zip(&keyed)) {
        assert_same_poses(linked, keyed, &format!("at {at} s"));
    }
    {
        let (studio, assets, world) = (&mut app.animation, &app.assets, &app.world);
        studio.set_record(true);
        studio.pick_bone(assets, world, "mixamorig:Spine");
        studio.set_playhead(duration * 0.5);
        let was = studio.preview_bones(assets, world);
        studio
            .turn_bone(assets, world, Quat::from_rotation_x(0.4))
            .expect("a converted bone can be keyed");
        assert_ne!(studio.preview_bones(assets, world), was);
    }
    let _ = fs::remove_dir_all(&folder);
}

#[test]
fn a_bone_tree_stays_folded_as_it_was_when_switching_clips() {
    let folder = copy_of_example("hello-part", "folds-between-clips");
    let mut app = EditorApp::new(Some(&folder));
    let (walker, clips) = import_walking(&mut app);
    let other = "assets/animations/Walking/Walking.anim.json";
    let open = |app: &mut EditorApp, clip: &str| {
        app.animation.open_for(
            &folder,
            &app.assets,
            walker,
            clip,
            &mut app.world,
            &mut app.output,
        );
    };
    open(&mut app, &clips[0]);
    let first = app.animation.shown_bones(&app.assets);
    assert!(first.len() > 3, "three levels show to begin with");
    app.animation.fold_all_bones(&app.assets, true);
    assert_eq!(app.animation.shown_bones(&app.assets), ["mixamorig:Hips"]);

    open(&mut app, other);
    open(&mut app, &clips[0]);
    assert_eq!(app.animation.shown_bones(&app.assets), ["mixamorig:Hips"]);
    app.animation.fold_all_bones(&app.assets, false);
    let all = app.animation.shown_bones(&app.assets);
    assert!(all.len() > first.len());
    open(&mut app, other);
    open(&mut app, &clips[0]);
    assert_eq!(app.animation.shown_bones(&app.assets), all);
    let _ = fs::remove_dir_all(&folder);
}

#[test]
fn open_marks_clips_and_state_machines_nothing_uses_and_follows_changes() {
    const MACHINE: &str = "assets/animations/noodle/noodle.statemachine.json";
    const SPARE: &str = "assets/animations/noodle/Spare.anim.json";
    let folder = copy_of_example("hello-animation", "not-used");
    let mut app = EditorApp::new(Some(&folder));
    let noodle = part(&app, "Noodle");
    let animator = crate::animation::animator_of(&app.world, noodle).unwrap();
    let machine = axcenity_anim::StateMachineFile::from_json(
        &fs::read_to_string(folder.join(MACHINE)).unwrap(),
    )
    .unwrap();
    let played: Vec<String> = machine
        .states
        .iter()
        .flat_map(|state| state.motion.clips())
        .map(|clip| clip.to_string())
        .collect();
    assert!(!played.is_empty());

    // The noodle's state machine and the clips it plays are used.
    let unused = app.animation.unused_files(&folder, &app.world);
    assert!(!unused.iter().any(|path| path == MACHINE));
    for clip in &played {
        assert!(!unused.contains(clip), "{clip} is played");
    }

    // A clip nothing uses is marked, until an Animation plays it.
    axcenity_anim::files::write_file(&folder.join(SPARE), &ClipFile::new(1.0).to_json()).unwrap();
    assert!(
        app.animation
            .unused_files(&folder, &app.world)
            .contains(&SPARE.to_owned())
    );
    let animation = app.world.create(ClassName::Animation).unwrap();
    app.world
        .set_property(animation, "Clip", Value::String(SPARE.into()))
        .unwrap();
    app.world.set_parent(animation, Some(animator)).unwrap();
    assert!(
        !app.animation
            .unused_files(&folder, &app.world)
            .contains(&SPARE.to_owned())
    );
    app.world
        .set_property(animation, "Clip", Value::String(String::new()))
        .unwrap();
    assert!(
        app.animation
            .unused_files(&folder, &app.world)
            .contains(&SPARE.to_owned())
    );

    // With no Animator playing it, the state machine is marked, and so are
    // its clips that no Animation plays.
    app.world
        .set_property(animator, "StateMachine", Value::String(String::new()))
        .unwrap();
    let unused = app.animation.unused_files(&folder, &app.world);
    assert!(unused.iter().any(|path| path == MACHINE));
    let animated: Vec<String> = Subject::of(&app.world, noodle)
        .unwrap()
        .animations
        .iter()
        .map(|animation| animation.clip.clone())
        .collect();
    for clip in &played {
        assert_eq!(unused.contains(clip), !animated.contains(clip), "{clip}");
    }
    let _ = fs::remove_dir_all(&folder);
}
