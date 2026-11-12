//! AutoRecover in the editor (`app/autorecover.rs`, `docs/project/autorecover.md`).

use std::time::{Duration, SystemTime};

use axcenity_core::PartType;

use super::tests::{copy_of_example, inserted};
use super::*;

const SCRIPT: &str = "scripts/pad.server.lua";

/// Waits for the snapshot being written, if any.
fn finish_writing(app: &mut EditorApp) {
    if let Some(writing) = app.autorecover.writing.take() {
        writing.join().unwrap().unwrap();
    }
}

/// Makes the snapshot look like one no running editor touched lately.
fn age_snapshot(folder: &Path) {
    let file = std::fs::File::options()
        .append(true)
        .open(
            folder
                .join(axcenity_project::RECOVERY_FOLDER)
                .join("recovery.json"),
        )
        .unwrap();
    file.set_modified(SystemTime::now() - Duration::from_secs(300))
        .unwrap();
}

/// An editor on a copy of hello-touched with an unsaved part named
/// "Recovered" and unsaved text in the pad script's tab, and AutoRecover's
/// snapshot of it written.
fn unsaved_work(test: &str) -> (EditorApp, PathBuf) {
    let folder = copy_of_example("hello-touched", test);
    let mut app = EditorApp::new(Some(&folder));
    let part = inserted(&mut app, Insertable::Part(PartType::Block), None);
    app.world.set_name(part, "Recovered").unwrap();
    app.history.seal();
    let saved = std::fs::read_to_string(folder.join(SCRIPT)).unwrap();
    app.tabs.open(SCRIPT, &saved, None);
    app.tabs.get_mut(SCRIPT).unwrap().text = "print(\"recovered\")\n".to_owned();
    // The interval has passed.
    app.autorecover.last = None;
    app.autorecover_frame();
    finish_writing(&mut app);
    (app, folder)
}

fn has_recovered_part(app: &EditorApp) -> bool {
    let workspace = app.world.workspace();
    app.world.find_first_child(workspace, "Recovered").is_some()
}

#[test]
fn unsaved_work_is_kept_after_the_interval_and_save_removes_it() {
    let (mut app, folder) = unsaved_work("autorecover-save");
    let info = axcenity_project::recovery_info(&folder).expect("a snapshot");
    assert!(info.world);
    assert_eq!(info.files, vec![SCRIPT.to_owned()]);
    // Not offered to the editor that's still running.
    assert!(axcenity_project::left_behind_recovery(&folder).is_none());

    // Nothing changed since: the same work isn't written again.
    let written = app.autorecover.written;
    app.autorecover.last = None;
    app.autorecover_frame();
    assert!(app.autorecover.writing.is_none());
    assert_eq!(app.autorecover.written, written);

    assert!(app.save());
    assert!(axcenity_project::recovery_info(&folder).is_none());
    assert!(!folder.join(axcenity_project::RECOVERY_FOLDER).exists());
    drop(app);
    let _ = std::fs::remove_dir_all(folder);
}

#[test]
fn a_left_behind_snapshot_is_offered_and_recover_brings_the_work_back_unsaved() {
    let (app, folder) = unsaved_work("autorecover-recover");
    let saved_script = std::fs::read_to_string(folder.join(SCRIPT)).unwrap();
    let saved_world = std::fs::read(folder.join("world.json")).unwrap();
    // The editor stops without saving or cleaning up (a crash).
    std::mem::forget(app);
    age_snapshot(&folder);

    let mut app = EditorApp::new(Some(&folder));
    assert!(!has_recovered_part(&app), "opens as saved");
    let offer = app
        .autorecover
        .offer
        .clone()
        .expect("Recover unsaved work?");
    assert!(offer.world);
    assert_eq!(offer.files, vec![SCRIPT.to_owned()]);
    // Nothing is written over it while it waits for an answer.
    app.autorecover.last = None;
    app.autorecover_frame();
    assert!(app.autorecover.writing.is_none());

    app.recover_unsaved_work();
    assert!(app.autorecover.offer.is_none());
    assert!(has_recovered_part(&app));
    assert!(app.unsaved());
    assert_eq!(
        app.tabs.get(SCRIPT).map(|tab| tab.text.as_str()),
        Some("print(\"recovered\")\n")
    );
    assert!(app.tabs.unsaved_paths().contains(&SCRIPT.to_owned()));
    // Nothing on disk changed, and the copy stays until the work is saved.
    assert_eq!(
        std::fs::read_to_string(folder.join(SCRIPT)).unwrap(),
        saved_script
    );
    assert_eq!(
        std::fs::read(folder.join("world.json")).unwrap(),
        saved_world
    );
    assert!(axcenity_project::recovery_info(&folder).is_some());

    assert!(app.save());
    assert!(axcenity_project::recovery_info(&folder).is_none());
    assert!(
        std::fs::read_to_string(folder.join(SCRIPT))
            .unwrap()
            .contains("recovered")
    );
    drop(app);
    let _ = std::fs::remove_dir_all(folder);
}

#[test]
fn discard_deletes_a_left_behind_snapshot() {
    let (app, folder) = unsaved_work("autorecover-discard");
    std::mem::forget(app);
    age_snapshot(&folder);

    let mut app = EditorApp::new(Some(&folder));
    assert!(app.autorecover.offer.is_some());
    app.discard_unsaved_work();
    assert!(app.autorecover.offer.is_none());
    assert!(!has_recovered_part(&app));
    assert!(axcenity_project::recovery_info(&folder).is_none());
    drop(app);
    // Opening again offers nothing.
    let app = EditorApp::new(Some(&folder));
    assert!(app.autorecover.offer.is_none());
    drop(app);
    let _ = std::fs::remove_dir_all(folder);
}

#[test]
fn with_autorecover_off_nothing_is_written() {
    let folder = copy_of_example("hello-touched", "autorecover-off");
    let mut app = EditorApp::new(Some(&folder));
    app.autorecover.read("0");
    assert_eq!(app.autorecover.to_text(), "0");
    inserted(&mut app, Insertable::Part(PartType::Block), None);
    app.history.seal();
    app.autorecover.last = None;
    app.autorecover_frame();
    finish_writing(&mut app);
    assert!(axcenity_project::recovery_info(&folder).is_none());
    drop(app);
    let _ = std::fs::remove_dir_all(folder);
}

/// "Recover unsaved work?" draws, and Escape doesn't close it: the copy is
/// only let go by choosing Discard.
#[test]
fn the_recover_prompt_draws_and_escape_keeps_it() {
    let (app, folder) = unsaved_work("autorecover-prompt");
    std::mem::forget(app);
    age_snapshot(&folder);
    let mut app = EditorApp::new(Some(&folder));
    if let Some(snapshots) = std::env::var_os("AXCENITY_SNAPSHOTS") {
        let mut shown = EditorApp::new(Some(&folder));
        crate::snapshot::snapshot(
            &PathBuf::from(snapshots).join("autorecover-prompt.png"),
            egui::vec2(700.0, 460.0),
            vec![Vec::new(); 3],
            |ui| {
                let ctx = &ui.ctx().clone();
                egui::CentralPanel::default().show(ui, |_| {});
                shown.autorecover_prompt(ctx);
            },
        );
    }
    let ctx = crate::theme::test_context();
    let escape = egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Default::default(),
    };
    for events in [vec![], vec![escape]] {
        let input = egui::RawInput {
            events,
            ..Default::default()
        };
        let _ = crate::theme::run(&ctx, input, |ui| {
            let ctx = &ui.ctx().clone();
            app.autorecover_prompt(ctx);
        });
    }
    assert!(app.autorecover.offer.is_some());
    assert!(axcenity_project::recovery_info(&folder).is_some());
    drop(app);
    let _ = std::fs::remove_dir_all(folder);
}
