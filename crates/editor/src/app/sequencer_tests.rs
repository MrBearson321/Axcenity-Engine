//! Phase 51: the Sequencer tab on copies of the hello-cutscene example:
//! its preview against Play, edits written as undo steps, and New and
//! Insert.

use axcenity_anim::sequence::{OVERLAY_NAME, SequenceFile};

use super::tests::{copy_of_example, play_for};
use super::*;
use crate::sequencer::Row;

const INTRO: &str = "assets/sequences/Intro.sequence.json";

fn find(world: &World, path: &[&str]) -> InstanceId {
    let mut at = world.workspace();
    for name in path {
        at = world.find_first_child(at, name).unwrap();
    }
    at
}

fn vector(world: &World, path: &[&str], name: &str) -> Vector3 {
    world
        .get(find(world, path))
        .unwrap()
        .property(name)
        .and_then(Value::as_vector3)
        .unwrap()
}

fn close(a: Vector3, b: Vector3, what: &str) {
    let gap = (a.x - b.x)
        .abs()
        .max((a.y - b.y).abs())
        .max((a.z - b.z).abs());
    assert!(gap < 0.01, "{what}: {a:?} vs {b:?}");
}

/// What a frame shows: the door, the hero's torso and left leg, the
/// camera, and the subtitle's key.
#[derive(Debug)]
struct Shown {
    door: Vector3,
    torso: Vector3,
    leg: Vector3,
    camera: (Vector3, Vector3, f32),
    subtitle: String,
}

fn shown_in(world: &World, camera: (Vector3, Vector3, f32)) -> Shown {
    let subtitle = world
        .find_first_child(world.workspace(), OVERLAY_NAME)
        .and_then(|gui| world.find_first_child(gui, "Subtitle"))
        .and_then(|label| {
            let label = world.get(label)?;
            (label.property("Visible") == Some(&Value::Bool(true)))
                .then(|| {
                    label
                        .property("TextKey")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                })
                .flatten()
        })
        .unwrap_or_default();
    Shown {
        door: vector(world, &["Door"], "Position"),
        torso: vector(world, &["Hero", "Torso"], "Position"),
        leg: vector(world, &["Hero", "LeftLeg"], "Orientation"),
        camera,
        subtitle,
    }
}

fn same(a: &Shown, b: &Shown, at: f64) {
    close(a.door, b.door, &format!("door at {at:.2}s"));
    close(a.torso, b.torso, &format!("torso at {at:.2}s"));
    close(a.leg, b.leg, &format!("leg at {at:.2}s"));
    close(a.camera.0, b.camera.0, &format!("camera at {at:.2}s"));
    close(a.camera.1, b.camera.1, &format!("camera turn at {at:.2}s"));
    assert!((a.camera.2 - b.camera.2).abs() < 0.01, "lens at {at:.2}s");
    assert_eq!(a.subtitle, b.subtitle, "subtitle at {at:.2}s");
}

#[test]
fn the_preview_scrubs_without_play_and_shows_what_play_shows() {
    let folder = copy_of_example("hello-cutscene", "sequencer-preview");
    let mut app = EditorApp::new(Some(&folder));
    app.open_sequence(INTRO);
    assert!(app.layout.is_open(Pane::Sequencer));
    assert!(app.sequencer.preview_on);
    let door_before = vector(&app.world, &["Door"], "Position");

    let frames = [60_u32, 240, 600, 840, 1000];
    let seconds = |frame: u32| f64::from(frame) / 60.0;
    let mut previewed = Vec::new();
    for &frame in &frames {
        app.sequencer.doc.as_mut().unwrap().keys.playhead = seconds(frame);
        assert!(app.update_sequence_preview());
        let preview = app.sequencer.shown().unwrap();
        let shot = preview.camera.expect("a cut is in charge");
        previewed.push(shown_in(
            &preview.world,
            (shot.position, shot.orientation, shot.field_of_view),
        ));
    }
    // Scrubbing never touched the world being edited.
    assert_eq!(vector(&app.world, &["Door"], "Position"), door_before);
    assert!(
        app.world
            .find_first_child(app.world.workspace(), OVERLAY_NAME)
            .is_none()
    );
    assert_eq!(previewed[1].subtitle, "intro.line1");
    assert!(previewed[2].door.x > 3.0, "the door is open at 10 s");

    // Play shows the same at the same times (the Intro plays by itself).
    app.play();
    assert!(!app.update_sequence_preview(), "Play hides the preview");
    let mut played = 0;
    for (index, &frame) in frames.iter().enumerate() {
        play_for(&mut app, f64::from(frame - played) / 60.0);
        played = frame;
        let session = app.play.as_ref().unwrap();
        let view = session.shown_camera(axcenity_script::CameraState::default());
        let shown = shown_in(
            session.world(),
            (view.position, view.orientation, view.field_of_view),
        );
        same(&shown, &previewed[index], seconds(frame));
    }
    app.stop();
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn an_edit_is_written_once_the_mouse_is_up_as_one_undo_step() {
    let folder = copy_of_example("hello-cutscene", "sequencer-edit");
    let mut app = EditorApp::new(Some(&folder));
    app.open_sequence(INTRO);
    let read = |app: &EditorApp| {
        let text = std::fs::read_to_string(folder.join(INTRO)).unwrap();
        let _ = app;
        SequenceFile::from_json(&text).unwrap()
    };
    assert_eq!(read(&app).subtitles.len(), 2);
    let doc = app.sequencer.doc.as_mut().unwrap();
    doc.keys.playhead = 15.0;
    doc.add(Row::Subtitle, String::new(), "intro.line1");
    doc.set_duration(25.0);
    let ctx = theme::test_context();
    app.flush_sequence(&ctx);
    let written = read(&app);
    assert_eq!(written.subtitles.len(), 3);
    assert_eq!(written.subtitles[2].start, 15.0);
    assert_eq!(written.duration, 25.0);
    assert_eq!(app.history.undo_label(), Some("Edit Intro"));

    app.undo();
    assert_eq!(read(&app).subtitles.len(), 2);
    let doc = app.sequencer.doc.as_ref().unwrap();
    assert_eq!(
        doc.file.subtitles.len(),
        2,
        "the tab shows the file as it is"
    );
    assert_eq!(doc.duration(), 20.0);
    app.redo();
    assert_eq!(app.sequencer.doc.as_ref().unwrap().file.subtitles.len(), 3);
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn new_makes_a_sequence_and_insert_puts_a_sequence_that_plays_it_in_the_workspace() {
    let folder = copy_of_example("hello-cutscene", "sequencer-new");
    let mut app = EditorApp::new(Some(&folder));
    app.new_sequence("Outro");
    let path = "assets/sequences/Outro.sequence.json";
    assert!(folder.join(path).exists());
    assert!(app.sequencer.is_open(path));
    app.insert_sequence(path);
    let id = app
        .world
        .find_first_child(app.world.workspace(), "Outro")
        .unwrap();
    let sequence = app.world.get(id).unwrap();
    assert_eq!(sequence.class(), ClassName::Sequence);
    assert_eq!(
        sequence.property("SequenceId"),
        Some(&Value::String(path.to_owned()))
    );
    app.undo();
    assert!(
        app.world
            .find_first_child(app.world.workspace(), "Outro")
            .is_none()
    );
    app.undo();
    assert!(!folder.join(path).exists(), "undoing New removes the file");
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn the_tab_and_the_preview_draw() {
    let Some(snapshots) = std::env::var_os("AXCENITY_SNAPSHOTS") else {
        return;
    };
    let folder = copy_of_example("hello-cutscene", "sequencer-draw");
    let mut app = EditorApp::new(Some(&folder));
    app.open_sequence(INTRO);
    app.sequencer.doc.as_mut().unwrap().keys.playhead = 10.0;
    let folder_out = std::path::PathBuf::from(snapshots);
    crate::snapshot::snapshot(
        &folder_out.join("sequencer-tab.png"),
        egui::vec2(900.0, 700.0),
        vec![Vec::new(); 3],
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| app.sequencer_pane(ui));
        },
    );
    for (time, name) in [
        (4.0, "sequencer-preview-4s.png"),
        (10.0, "sequencer-preview-10s.png"),
        (15.0, "sequencer-preview-15s.png"),
    ] {
        // A new app each time: each snapshot has a GPU of its own, and
        // the viewport keeps its texture.
        let mut app = EditorApp::new(Some(&folder));
        app.open_sequence(INTRO);
        app.sequencer.doc.as_mut().unwrap().keys.playhead = time;
        crate::snapshot::snapshot_rendered(
            &folder_out.join(name),
            egui::vec2(800.0, 450.0),
            vec![Vec::new(); 3],
            |ctx, render| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let _ = app.scene_pane(ui, Some(render));
                });
            },
        );
    }
    let _ = std::fs::remove_dir_all(&folder);
}
