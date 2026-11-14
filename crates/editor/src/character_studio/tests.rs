//! Character Studio's pieces that don't need the editor: what a selection
//! shows, mapping shapes, editing a lip sync's visemes and blending
//! expressions. The tab itself is tested in `app/character_studio.rs`.

use axcenity_assets::{Assets, FaceExpression, FaceProfile, LipSync, VisemeKey};
use axcenity_core::face::face_control;
use axcenity_core::{ClassName, World};

use super::expressions::add_expression;
use super::lip_sync::{add_key, key_at, peaks, retime};
use super::rig::{map_shape, shape_control};
use super::*;

#[test]
fn a_selection_shows_its_meshpart_and_face() {
    let mut world = World::new();
    let workspace = world.workspace();
    let model = world.create(ClassName::Model).unwrap();
    world.set_parent(model, Some(workspace)).unwrap();
    let part = world.create(ClassName::MeshPart).unwrap();
    world.set_parent(part, Some(model)).unwrap();
    let bare = world.create(ClassName::MeshPart).unwrap();
    world.set_parent(bare, Some(workspace)).unwrap();
    let face = world.create(ClassName::FaceControls).unwrap();
    world.set_parent(face, Some(part)).unwrap();
    let shown = Some(Target {
        part,
        face: Some(face),
    });
    assert_eq!(face_target(&world, Some(face)), shown);
    assert_eq!(face_target(&world, Some(part)), shown);
    assert_eq!(face_target(&world, Some(model)), shown);
    assert_eq!(
        face_target(&world, Some(bare)),
        Some(Target {
            part: bare,
            face: None
        })
    );
    assert_eq!(face_target(&world, Some(workspace)), shown);
    assert_eq!(face_target(&world, None), None);
    // A FaceControls outside a MeshPart has no model to show.
    world.set_parent(face, Some(model)).unwrap();
    assert_eq!(face_target(&world, Some(face)), None);
}

#[test]
fn a_shape_is_driven_by_one_control_at_a_time() {
    let mut profile = FaceProfile::new();
    map_shape(&mut profile, "smileL", Some("Mouth_Smile_Left"), 1.0);
    map_shape(&mut profile, "grin", Some("Mouth_Smile_Left"), 0.5);
    assert_eq!(
        shape_control(&profile, "grin"),
        Some(("Mouth_Smile_Left".to_owned(), 0.5))
    );
    // Moved to another control, it leaves the first.
    map_shape(&mut profile, "grin", Some("Mouth_Smile_Right"), 0.7);
    assert_eq!(profile.controls[0].1.len(), 1);
    assert_eq!(
        shape_control(&profile, "grin"),
        Some(("Mouth_Smile_Right".to_owned(), 0.7))
    );
    // Taken off, a control left driving nothing goes.
    map_shape(&mut profile, "grin", None, 1.0);
    assert_eq!(shape_control(&profile, "grin"), None);
    assert_eq!(profile.controls.len(), 1);
    assert!(FaceProfile::from_json(&profile.to_json()).is_ok());
}

fn line() -> LipSync {
    let key = |time: f64, viseme: &str| VisemeKey {
        time,
        viseme: viseme.into(),
        weight: 1.0,
    };
    LipSync {
        duration: 1.0,
        visemes: vec![key(0.0, "Silence"), key(0.3, "AI"), key(0.6, "Silence")],
        ..LipSync::default()
    }
}

#[test]
fn visemes_are_retimed_between_neighbours_added_in_order_and_found() {
    let mut lip = line();
    retime(&mut lip, 1, 0.9);
    assert!(
        (lip.visemes[1].time - 0.59).abs() < 1e-9,
        "stops before the next"
    );
    retime(&mut lip, 1, -1.0);
    assert!(
        (lip.visemes[1].time - 0.01).abs() < 1e-9,
        "stops after the last"
    );
    retime(&mut lip, 2, 5.0);
    assert_eq!(lip.visemes[2].time, 1.0, "inside the line");
    let at = add_key(&mut lip, 0.5, "O");
    assert_eq!(at, 2);
    assert_eq!(lip.visemes[2].viseme, "O");
    assert_eq!(
        add_key(&mut lip, 0.5, "U"),
        2,
        "a key already there changes"
    );
    assert_eq!(lip.visemes.len(), 4);
    assert_eq!(key_at(&lip, 0.55), Some(2));
    assert_eq!(key_at(&lip, 0.0), Some(0));
    assert_eq!(key_at(&LipSync::default(), 0.5), None);
    assert!(LipSync::from_json(&lip.to_json()).is_ok());
}

#[test]
fn a_waveform_is_the_loudest_sample_of_each_slice() {
    let samples = [0.1, -0.5, 0.2, 0.0, 0.9, -0.3, 2.0];
    assert_eq!(peaks(&samples, 3), vec![0.5, 0.9, 1.0]);
    assert!(peaks(&[], 10).is_empty());
}

#[test]
fn the_lip_sync_editor_plays_its_visemes_on_the_face() {
    let assets = Assets::new(test_folder("lip-play"));
    let mut editor = LipSyncEditor::default();
    editor.open("voice/line.lipsync.json", line());
    let open = face_control("Mouth_Open").unwrap();
    assert_eq!(
        editor.controls(None, &assets)[open],
        0.0,
        "at rest at the start"
    );
    editor.playing = true;
    assert!(editor.advance(0.45));
    assert!(
        editor.controls(None, &assets)[open] > 0.2,
        "AI opens the mouth"
    );
    assert!(!editor.advance(1.0), "it stops at the end");
    assert_eq!(editor.time, 1.0);
}

#[test]
fn expressions_preview_at_an_intensity_and_blend() {
    let assets = Assets::new(test_folder("expressions"));
    let mut editor = ExpressionEditor::default();
    editor.pick(&assets, "happy");
    assert_eq!(editor.picked.as_deref(), Some("happy"));
    assert!(!editor.draft.controls.is_empty());
    let smile = face_control("Mouth_Smile_Left").unwrap();
    let full = editor.controls(&assets)[smile];
    assert!(full > 0.3, "{full}");
    editor.intensity = 0.5;
    assert!((editor.controls(&assets)[smile] - full / 2.0).abs() < 1e-5);
    // Blended all the way to Neutral, the smile goes.
    editor.intensity = 1.0;
    editor.blend = Some("Neutral".into());
    editor.blend_amount = 1.0;
    assert!(editor.controls(&assets)[smile] < full);

    let mut controls = vec![0.0; axcenity_core::face::FACE_CONTROLS.len()];
    let mut own = FaceExpression {
        intensity: 0.5,
        ..FaceExpression::default()
    };
    own.controls.insert("Jaw_Open".into(), 0.8);
    add_expression(&own, 0.5, &mut controls);
    assert!((controls[face_control("Jaw_Open").unwrap()] - 0.2).abs() < 1e-6);
}

#[test]
fn file_names_keep_plain_characters_and_files_are_listed() {
    assert_eq!(file_name(" My face! "), Some("My face".to_owned()));
    assert_eq!(file_name("../.."), None);
    let folder = test_folder("files");
    for path in [
        "voice/a.lipsync.json",
        "voice/a.ogg",
        ".hidden/b.lipsync.json",
        "target/c.lipsync.json",
        "notes.md",
    ] {
        let file = folder.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, "").unwrap();
    }
    assert_eq!(
        project_files(&folder, &[".lipsync.json"]),
        vec!["voice/a.lipsync.json"]
    );
    assert_eq!(
        project_files(&folder, &[".OGG"]).len(),
        0,
        "endings are lower case"
    );
    assert_eq!(project_files(&folder, &[".ogg"]), vec!["voice/a.ogg"]);
    let _ = std::fs::remove_dir_all(&folder);
}
