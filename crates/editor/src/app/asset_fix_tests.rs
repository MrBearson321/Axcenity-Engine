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
    let mut draw = |ui: &mut egui::Ui| {
        egui::CentralPanel::default().show(ui, |ui| {
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
        let _ = crate::theme::run(&ctx, input, |ui| draw(ui));
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

#[test]
fn locate_copies_an_outside_file_in_and_a_models_texture_next_to_the_model() {
    // An ImageLabel whose image is gone, and a file picked outside the
    // project for it.
    let folder = copy_of_example("hello-part", "asset-fix-locate");
    let outside = folder.with_extension("outside");
    let _ = fs::remove_dir_all(&outside);
    fs::create_dir_all(&outside).unwrap();
    fs::write(outside.join("logo.png"), b"\x89PNG logo").unwrap();
    let mut app = EditorApp::new(Some(&folder));
    let label = app
        .world
        .create(axcenity_core::ClassName::ImageLabel)
        .unwrap();
    app.world
        .set_property(
            label,
            "Image",
            Value::String("assets/images/gone.png".into()),
        )
        .unwrap();
    let workspace = app.world.workspace();
    app.world.set_parent(label, Some(workspace)).unwrap();
    app.run_asset_check();
    let missing: Vec<Finding> = shown(&app)
        .into_iter()
        .filter(|finding| finding.path == "assets/images/gone.png")
        .collect();
    assert_eq!(missing.len(), 1, "{:?}", shown(&app));
    app.fix_findings(&missing, FixChoice::CopyIn(outside.join("logo.png")));
    assert_eq!(
        fs::read(folder.join("assets/images/logo.png")).unwrap(),
        b"\x89PNG logo",
        "copied into the folder Import uses for images"
    );
    assert_eq!(
        app.world.get(label).unwrap().property("Image"),
        Some(&Value::String("assets/images/logo.png".into()))
    );
    assert!(
        shown(&app)
            .iter()
            .all(|finding| finding.path != "assets/images/gone.png"),
        "the finding is gone: {:?}",
        shown(&app)
    );
    let _ = fs::remove_dir_all(&folder);

    // A model whose texture is gone: the picked image goes next to it.
    let folder = copy_of_example("hello-pbr", "asset-fix-locate-texture");
    let color = fs::read(folder.join("assets/models/maps/color.png")).unwrap();
    fs::write(outside.join("color.png"), &color).unwrap();
    fs::remove_file(folder.join("assets/models/maps/color.png")).unwrap();
    let mut app = EditorApp::new(Some(&folder));
    app.run_asset_check();
    let texture: Vec<Finding> = shown(&app)
        .into_iter()
        .filter(|finding| matches!(finding.site, project::Site::ModelTexture { .. }))
        .collect();
    assert!(!texture.is_empty(), "{:?}", shown(&app));
    let model = match &texture[0].site {
        project::Site::ModelTexture { model, .. } => model.clone(),
        _ => unreachable!(),
    };
    app.fix_findings(&texture[..1], FixChoice::CopyIn(outside.join("color.png")));
    let next_to = folder
        .join(&model)
        .parent()
        .unwrap()
        .join("textures/color.png");
    assert_eq!(fs::read(&next_to).unwrap(), color, "{}", next_to.display());
    assert!(
        !shown(&app)
            .iter()
            .any(|finding| finding.site == texture[0].site),
        "the finding is gone: {:?}",
        shown(&app)
    );
    let _ = fs::remove_dir_all(&folder);
    let _ = fs::remove_dir_all(&outside);
}
