//! File ▸ Version History (unified Phase 72): the change view, restores
//! that ask first and refuse unsaved changes, and checkpoints.

use std::fs;

use axcenity_core::Vector3;

use super::tests::example;
use super::version_history::Restore;
use super::*;

const SIZE: egui::Vec2 = egui::vec2(1400.0, 900.0);

/// A copy of `examples/hello-part` in the temp folder.
fn scratch_project(name: &str) -> PathBuf {
    fn copy(from: &Path, to: &Path) {
        fs::create_dir_all(to).unwrap();
        for entry in fs::read_dir(from).unwrap().flatten() {
            // Not the history this example has from being opened before.
            if entry.file_name() == ".axcenity" {
                continue;
            }
            let target = to.join(entry.file_name());
            if entry.path().is_dir() {
                copy(&entry.path(), &target);
            } else {
                fs::copy(entry.path(), target).unwrap();
            }
        }
    }
    let folder = std::env::temp_dir().join(format!(
        "axcenity-editor-history-{name}-test-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&folder);
    copy(&example("hello-part"), &folder);
    folder
}

/// Draws the Version History window with AccessKit on, so every label and
/// button comes back with its place.
fn frame(
    app: &mut EditorApp,
    ctx: &egui::Context,
    clock: &mut f64,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    *clock += 0.05;
    ctx.enable_accesskit();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, SIZE)),
        events,
        time: Some(*clock),
        ..Default::default()
    };
    ctx.run(input, |ctx| {
        egui::CentralPanel::default().show(ctx, |_| {});
        app.version_window(ctx);
    })
}

/// Every piece of text drawn, without Phosphor icons, and its middle.
fn texts(output: &egui::FullOutput) -> Vec<(String, egui::Pos2)> {
    let Some(update) = output.platform_output.accesskit_update.as_ref() else {
        return Vec::new();
    };
    update
        .nodes
        .iter()
        .filter_map(|(_, node)| {
            let text = node.label().or_else(|| node.value())?;
            let text: String = text
                .chars()
                .filter(|c| !('\u{e000}'..='\u{f8ff}').contains(c))
                .collect();
            let bounds = node.bounds()?;
            let middle = egui::pos2(
                ((bounds.x0 + bounds.x1) * 0.5) as f32,
                ((bounds.y0 + bounds.y1) * 0.5) as f32,
            );
            Some((text.trim().to_owned(), middle))
        })
        .collect()
}

/// Clicks the text `label` (or a timeline row whose first line it is).
fn click(app: &mut EditorApp, ctx: &egui::Context, clock: &mut f64, label: &str) {
    use super::ui_designer_tests::press;
    let output = frame(app, ctx, clock, Vec::new());
    let at = texts(&output)
        .into_iter()
        .find(|(text, _)| {
            text == label
                || text.starts_with(&format!(
                    "{label}
"
                ))
        })
        .unwrap_or_else(|| panic!("`{label}` is drawn"))
        .1;
    for events in [
        vec![egui::Event::PointerMoved(at)],
        vec![press(at, true)],
        vec![press(at, false)],
    ] {
        frame(app, ctx, clock, events);
    }
}

fn shown(app: &mut EditorApp, ctx: &egui::Context, clock: &mut f64) -> Vec<String> {
    // Twice: a window lays itself out on its first frame.
    frame(app, ctx, clock, Vec::new());
    texts(&frame(app, ctx, clock, Vec::new()))
        .into_iter()
        .map(|(text, _)| text)
        .collect()
}

fn block_x(folder: &Path) -> f32 {
    let project = axcenity_project::load(folder).unwrap();
    let block = project
        .world
        .find_first_child(project.world.workspace(), "Block")
        .unwrap();
    match project.world.get(block).unwrap().property("Position") {
        Some(Value::Vector3(position)) => position.x,
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_window_names_a_moved_part_and_an_edited_script_and_restores_only_when_asked_and_saved() {
    let folder = scratch_project("restore");
    let mut app = EditorApp::new(Some(&folder));
    let ctx = theme::test_context();
    let mut clock = 0.0;

    // A checkpoint of the project as it came.
    app.version_history.message = "As it came".to_owned();
    app.save_checkpoint();
    let versions = axcenity_project::list_versions(&folder);
    assert_eq!(versions.len(), 1);
    assert!(versions[0].pinned);
    assert_eq!(versions[0].label, "As it came");
    let start = versions[0].id.clone();

    // Move the Block in the editor, edit the script on disk, and save.
    let block = app
        .world
        .find_first_child(app.world.workspace(), "Block")
        .unwrap();
    app.set_on_parts(
        &[block],
        "Position",
        Value::Vector3(Vector3::new(10.0, 0.0, 0.0)),
    );
    fs::write(
        folder.join("scripts/Script.server.lua"),
        "print(\"Goodbye world!\")\n",
    )
    .unwrap();
    assert!(app.save());

    app.version_history.open = true;
    let drawn = shown(&mut app, &ctx, &mut clock);
    if let Some(snapshots) = std::env::var_os("AXCENITY_SNAPSHOTS") {
        let path = PathBuf::from(snapshots).join("version-history.png");
        crate::snapshot::snapshot(&path, SIZE, vec![Vec::new(); 3], |ctx| {
            egui::CentralPanel::default().show(ctx, |_| {});
            app.version_window(ctx);
        });
    }
    for expected in [
        "• Moved Part \"Block\" in Workspace",
        "scripts/Script.server.lua (changed)",
        "- print(\"Hello world!\")",
        "+ print(\"Goodbye world!\")",
        "As it came",
    ] {
        assert!(
            drawn.iter().any(|text| text.starts_with(expected)),
            "`{expected}` isn't drawn: {drawn:#?}"
        );
    }

    // Unsaved changes: a restore is refused and changes nothing.
    app.set_on_parts(
        &[block],
        "Position",
        Value::Vector3(Vector3::new(20.0, 0.0, 0.0)),
    );
    app.restore(&Restore::Project { id: start.clone() });
    assert_eq!(block_x(&folder), 10.0);
    assert!(
        last_warning(&app).contains("Save or undo your changes before restoring"),
        "{}",
        last_warning(&app)
    );
    assert!(app.save());

    // Restore asks first: picking the checkpoint and Restore project to
    // here only opens the question.
    click(&mut app, &ctx, &mut clock, "As it came");
    click(&mut app, &ctx, &mut clock, "Restore project to here");
    assert_eq!(
        app.version_history.confirm,
        Some(Restore::Project { id: start })
    );
    assert_eq!(block_x(&folder), 20.0);
    let drawn = shown(&mut app, &ctx, &mut clock);
    assert!(
        drawn.iter().any(|text| text == "Restore the project?"),
        "{drawn:#?}"
    );
    click(&mut app, &ctx, &mut clock, "Restore");

    assert_eq!(app.version_history.confirm, None);
    assert_eq!(block_x(&folder), 0.0);
    let newest = &axcenity_project::list_versions(&folder)[0];
    assert_eq!(newest.label, "Before restore");
    assert!(newest.pinned);
    let _ = fs::remove_dir_all(&folder);
}

#[test]
fn a_checkpoint_backs_up_when_that_is_turned_on() {
    let folder = scratch_project("backup");
    let backups = std::env::temp_dir().join(format!(
        "axcenity-editor-history-backups-test-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&backups);
    axcenity_project::save_backup_settings(
        &folder,
        &axcenity_project::BackupSettings {
            folder: Some(backups.clone()),
            on_checkpoint: true,
        },
    )
    .unwrap();
    let mut app = EditorApp::new(Some(&folder));
    app.save_checkpoint();
    let zips: Vec<_> = fs::read_dir(&backups).unwrap().flatten().collect();
    assert_eq!(zips.len(), 1);
    assert_eq!(
        axcenity_project::list_versions(&folder)[0].label,
        "Checkpoint"
    );
    let _ = fs::remove_dir_all(&folder);
    let _ = fs::remove_dir_all(&backups);
}

fn last_warning(app: &EditorApp) -> String {
    app.output
        .lines()
        .iter()
        .rev()
        .find(|line| line.kind == crate::output::OutputKind::Warning)
        .map(|line| line.text.clone())
        .unwrap_or_default()
}
