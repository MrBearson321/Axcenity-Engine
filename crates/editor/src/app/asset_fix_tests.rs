//! Asset Check fixes made by the app: one undo step each, and the ignored
//! list kept in the project.

use std::fs;

use axcenity_core::Value;
use axcenity_project::{self as project, Finding};

use super::EditorApp;
use super::tests::copy_of_example;
use crate::asset_check::FixChoice;

const MODEL: &str = "assets/models/noodle.fbx";
const MOVED: &str = "assets/models/moved/noodle.fbx";

fn shown(app: &EditorApp) -> Vec<Finding> {
    app.asset_check.shown().into_iter().cloned().collect()
}

fn mesh_ids(app: &EditorApp) -> Vec<String> {
    app.world
        .descendants(app.world.root())
        .into_iter()
        .filter_map(|id| match app.world.get(id)?.property("MeshId") {
            Some(Value::String(path)) if !path.is_empty() => Some(path.clone()),
            _ => None,
        })
        .collect()
}

/// The text of every clip file in the noodle's folder.
fn clip_texts(folder: &std::path::Path) -> String {
    let mut texts: Vec<_> = fs::read_dir(folder.join("assets/animations/noodle"))
        .unwrap()
        .flatten()
        .map(|entry| fs::read_to_string(entry.path()).unwrap())
        .collect();
    texts.sort();
    texts.join("\n")
}

#[test]
fn a_moved_model_is_fixed_everywhere_in_one_undo_step() {
    let folder = copy_of_example("hello-animation", "asset-fix-moved");
    fs::create_dir_all(folder.join("assets/models/moved")).unwrap();
    fs::rename(folder.join(MODEL), folder.join(MOVED)).unwrap();
    let clips_before = clip_texts(&folder);
    assert!(clips_before.contains(MODEL));

    let mut app = EditorApp::new(Some(&folder));
    app.run_asset_check();
    let findings = shown(&app);
    let first = findings
        .iter()
        .find(|finding| finding.path == MODEL && finding.owner.starts_with("Workspace."))
        .expect("the MeshPart's missing model")
        .clone();
    let like: Vec<Finding> = findings
        .iter()
        .filter(|finding| finding.is_like(&first))
        .cloned()
        .collect();
    assert!(
        like.len() > 1,
        "the clips that link the model are like it: {findings:?}"
    );
    let files = project::list_project_files(&folder);
    let suggested = project::suggestion(&first, &files).expect("a suggestion");
    assert_eq!(suggested, MOVED);

    app.fix_findings(&like, FixChoice::Use(suggested));
    assert_eq!(mesh_ids(&app), vec![MOVED.to_owned()]);
    let clips_after = clip_texts(&folder);
    assert!(
        !clips_after.contains(&format!("\"{MODEL}\"")),
        "{clips_after}"
    );
    assert!(clips_after.contains(MOVED));
    assert!(
        shown(&app).iter().all(|finding| finding.path != MODEL),
        "{:?}",
        shown(&app)
    );

    app.undo();
    assert_eq!(mesh_ids(&app), vec![MODEL.to_owned()]);
    assert_eq!(clip_texts(&folder), clips_before);
    app.redo();
    assert_eq!(mesh_ids(&app), vec![MOVED.to_owned()]);
    let _ = fs::remove_dir_all(&folder);
}

#[test]
fn ignored_problems_stay_hidden_after_reopening_and_delete_file_undoes() {
    let folder = copy_of_example("hello-part", "asset-fix-ignore");
    let broken = folder.join("assets/models/broken.glb");
    fs::create_dir_all(broken.parent().unwrap()).unwrap();
    fs::write(&broken, b"not a glb").unwrap();

    let mut app = EditorApp::new(Some(&folder));
    app.run_asset_check();
    let findings = shown(&app);
    assert_eq!(findings.len(), 1, "{findings:?}");
    app.ignore_findings(&findings);
    assert!(shown(&app).is_empty());

    let mut reopened = EditorApp::new(Some(&folder));
    reopened.run_asset_check();
    assert!(shown(&reopened).is_empty(), "{:?}", shown(&reopened));
    reopened.unignore_finding(&findings[0].ignore_key());
    assert_eq!(shown(&reopened), findings);

    reopened.fix_findings(&findings, FixChoice::Delete);
    assert!(!broken.exists());
    assert!(shown(&reopened).is_empty());
    reopened.undo();
    assert_eq!(fs::read(&broken).unwrap(), b"not a glb");
    let _ = fs::remove_dir_all(&folder);
}

/// The panel with fixes on its rows keeps inside a 260-point column. With
/// `AXCENITY_SNAPSHOTS` set, it's saved as `asset-check-fixes.png`.
#[test]
fn the_fixes_fit_a_narrow_column() {
    use crate::theme;
    use eframe::egui;

    let folder = copy_of_example("hello-animation", "asset-fix-draw");
    fs::create_dir_all(folder.join("assets/models/moved")).unwrap();
    fs::rename(folder.join(MODEL), folder.join(MOVED)).unwrap();
    fs::write(folder.join("assets/models/broken.glb"), b"not a glb").unwrap();
    let mut app = EditorApp::new(Some(&folder));
    app.run_asset_check();
    let size = egui::vec2(260.0, 900.0);
    let mut fit = None;
    let mut draw = |ctx: &egui::Context| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let room = ui.max_rect();
            app.asset_check_pane(ui);
            fit = Some((ui.min_rect(), room));
        });
    };
    let ctx = theme::test_context();
    for _ in 0..3 {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            ..Default::default()
        };
        let _ = ctx.run(input, &mut draw);
    }
    if let Some(snapshots) = std::env::var_os("AXCENITY_SNAPSHOTS") {
        let path = std::path::PathBuf::from(snapshots).join("asset-check-fixes.png");
        crate::snapshot::snapshot(&path, size, vec![Vec::new(); 3], &mut draw);
    }
    let (used, room) = fit.expect("the panel was drawn");
    assert!(
        used.width() <= room.width() + 0.5,
        "drew {} points wide in {}",
        used.width(),
        room.width()
    );
    let _ = fs::remove_dir_all(&folder);
}
