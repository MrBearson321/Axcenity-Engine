//! The one list of modified documents and "Don't Save" (`documents.rs`).

use super::tests::copy_of_example;
use super::*;
use axcenity_anim::{EvaluatedProperty, PropertyName};

/// A Door model with a Hinge, animated (its clip open with Record on), and
/// the project saved: the clip's path and its text as saved.
fn animated_door(app: &mut EditorApp, folder: &Path) -> (InstanceId, String, String) {
    let world = &mut app.world;
    let door = world.create(ClassName::Model).unwrap();
    world.set_name(door, "Door").unwrap();
    world.set_parent(door, Some(world.workspace())).unwrap();
    let hinge = world.create(ClassName::Part).unwrap();
    world.set_name(hinge, "Hinge").unwrap();
    world.set_parent(hinge, Some(door)).unwrap();
    app.animate(door);
    assert!(app.animation.recording());
    assert!(app.save(), "the project saves");
    let clip = crate::animation::Subject::of(&app.world, door)
        .unwrap()
        .animations[0]
        .clip
        .clone();
    let saved = std::fs::read_to_string(folder.join(&clip)).unwrap();
    assert!(!app.unsaved(), "{:?}", app.modified_documents());
    (door, clip, saved)
}

/// Turns the Hinge at 1 s with Record on: a change to the open clip that
/// isn't written yet.
fn key_the_hinge(app: &mut EditorApp) {
    app.animation.set_playhead(1.0);
    app.animation.move_in_preview(vec![EvaluatedProperty {
        target: "Hinge".into(),
        property: PropertyName::Orientation,
        value: vec![0.0, 90.0, 0.0],
    }]);
}

#[test]
fn an_unwritten_clip_counts_as_unsaved_and_is_listed() {
    let folder = copy_of_example("hello-touched", "documents-unwritten-clip");
    let mut app = EditorApp::new(Some(&folder));
    let (_, clip, _) = animated_door(&mut app, &folder);
    key_the_hinge(&mut app);
    assert!(app.unsaved(), "a keyed clip is an unsaved change");
    assert!(app.title().contains('*'));
    assert_eq!(
        app.modified_documents(),
        vec![documents::Modified {
            kind: documents::DocKind::Animation,
            name: clip,
        }]
    );
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn dont_save_on_close_leaves_an_open_clip_as_it_was_saved() {
    let folder = copy_of_example("hello-touched", "documents-dont-save-clip");
    let mut app = EditorApp::new(Some(&folder));
    let (_, clip, saved) = animated_door(&mut app, &folder);
    key_the_hinge(&mut app);

    assert_eq!(app.request(Discard::CloseProject), None, "it asks first");
    let ahead = app.answer(Answer::DontSave).expect("Don't Save goes ahead");
    let ctx = theme::test_context();
    app.go_ahead(&ctx, ahead);
    app.finish_closing_project();
    assert!(app.project.is_none());
    assert_eq!(
        std::fs::read_to_string(folder.join(&clip)).unwrap(),
        saved,
        "Don't Save wrote nothing"
    );
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn dont_save_puts_back_a_clip_written_when_another_was_opened() {
    let folder = copy_of_example("hello-touched", "documents-written-clip");
    let mut app = EditorApp::new(Some(&folder));
    let (door, clip, saved) = animated_door(&mut app, &folder);
    key_the_hinge(&mut app);
    // A second clip: opening it writes the first one.
    app.animate(door);
    app.studio_actions(Vec::new());
    let written = std::fs::read_to_string(folder.join(&clip)).unwrap();
    assert_ne!(written, saved, "opening another clip wrote the first");
    assert!(app.unsaved());

    assert_eq!(app.request(Discard::CloseProject), None);
    app.answer(Answer::DontSave).expect("Don't Save goes ahead");
    assert_eq!(
        std::fs::read_to_string(folder.join(&clip)).unwrap(),
        saved,
        "the written clip is back as it was saved"
    );
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn save_in_the_prompt_writes_the_open_clip() {
    let folder = copy_of_example("hello-touched", "documents-save-clip");
    let mut app = EditorApp::new(Some(&folder));
    let (_, clip, saved) = animated_door(&mut app, &folder);
    key_the_hinge(&mut app);
    app.request(Discard::CloseProject);
    assert_eq!(app.answer(Answer::Save), Some(Discard::CloseProject));
    assert_ne!(std::fs::read_to_string(folder.join(&clip)).unwrap(), saved);
    assert!(!app.unsaved(), "{:?}", app.modified_documents());
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn a_changed_model_studio_model_counts_and_saves_with_the_project() {
    let folder = copy_of_example("hello-touched", "documents-model");
    let mut app = EditorApp::new(Some(&folder));
    let doc = crate::model_studio::StudioDoc::new(
        String::new(),
        axcenity_mesh_edit::ModelDoc::primitive(&axcenity_mesh_edit::PrimitiveSettings::default()),
        false,
    );
    app.model_studio.open(doc);
    assert!(app.unsaved(), "a new, unsaved model counts");
    assert_eq!(
        app.modified_documents()
            .iter()
            .map(|document| document.kind)
            .collect::<Vec<_>>(),
        [documents::DocKind::Model]
    );
    assert!(app.save());
    assert!(!app.unsaved(), "{:?}", app.modified_documents());
    let path = app.model_studio.doc.as_ref().unwrap().path.clone();
    assert!(folder.join(&path).is_file(), "{path} was written");
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn dont_save_drops_a_changed_model_without_writing_it() {
    let folder = copy_of_example("hello-touched", "documents-model-dont-save");
    let mut app = EditorApp::new(Some(&folder));
    let doc = crate::model_studio::StudioDoc::new(
        String::new(),
        axcenity_mesh_edit::ModelDoc::primitive(&axcenity_mesh_edit::PrimitiveSettings::default()),
        false,
    );
    app.model_studio.open(doc);
    app.request(Discard::CloseProject);
    app.answer(Answer::DontSave).expect("goes ahead");
    assert!(app.model_studio.doc.is_none());
    assert!(
        !folder.join(axcenity_assets::MODELS_FOLDER).exists()
            || std::fs::read_dir(folder.join(axcenity_assets::MODELS_FOLDER))
                .unwrap()
                .next()
                .is_none(),
        "no model was written"
    );
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn the_prompt_lists_every_kind_of_modified_document_once() {
    let folder = copy_of_example("hello-touched", "documents-list");
    let mut app = EditorApp::new(Some(&folder));
    // Six edits, one undo step each: still one document.
    super::tests::edit_the_pad(&mut app);
    let names: Vec<String> = app
        .modified_documents()
        .into_iter()
        .map(|document| document.name)
        .collect();
    assert_eq!(names, ["The world"]);
    let _ = std::fs::remove_dir_all(&folder);
}
