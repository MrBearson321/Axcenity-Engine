use std::fs;

use super::super::tests::{copy_of_example, printed};
use super::*;
use crate::import_dialog::Tab;
use axcenity_project::Clashes;

const MACHINE: &str = "assets/animations/noodle/noodle.statemachine.json";
const WALK: &str = "assets/animations/noodle/Walk.anim.json";

/// The noodle's state machine exported from a copy of hello-animation, as
/// Export Package… in the Assets panel does, into `file`.
fn export_noodle(file: &Path) {
    let source = copy_of_example("hello-animation", "package-source");
    let mut app = EditorApp::new(Some(&source));
    assert!(app.export_package(file, "noodle", vec![MACHINE.to_owned()], Vec::new()));
    assert!(
        printed(&app)
            .iter()
            .any(|line| line.contains("with 7 files")),
        "{:?}",
        printed(&app)
    );
    let _ = fs::remove_dir_all(&source);
}

fn package_file(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "axcenity-editor-{name}-{}.axpkg",
        std::process::id()
    ))
}

/// Imports the package at `file` with Import…, as the dialog's Import does.
fn import(app: &mut EditorApp, file: &Path) -> ImportDialog {
    app.import_files(
        vec![file.to_owned()],
        axcenity_assets::PROJECT_TEXTURES_FOLDER,
    );
    app.importing
        .take()
        .expect("a package opens the Import dialog")
}

fn files_in(folder: &Path, under: &str) -> usize {
    fn count(dir: &Path) -> usize {
        fs::read_dir(dir).map_or(0, |entries| {
            entries
                .flatten()
                .map(|entry| {
                    if entry.path().is_dir() {
                        count(&entry.path())
                    } else {
                        1
                    }
                })
                .sum()
        })
    }
    count(&folder.join(under))
}

#[test]
fn a_package_imports_into_another_project_as_one_undo_step_and_twice_adds_nothing() {
    let file = package_file("one-step");
    export_noodle(&file);
    let folder = copy_of_example("hello-part", "package-import");
    let mut app = EditorApp::new(Some(&folder));

    let dialog = import(&mut app, &file);
    assert_eq!(dialog.tabs(), [Tab::Package]);
    let package = dialog.package.as_ref().unwrap();
    assert_eq!(
        package.sentence(),
        "Adds 7 files: 5 clips, 1 state machine, 1 model."
    );
    assert_eq!(dialog.problem(), None);
    app.import_package_chosen(&dialog);
    assert!(folder.join(MACHINE).is_file());
    assert!(folder.join("assets/models/noodle.fbx").is_file());
    assert!(project::check_assets(&folder, &app.world).is_ok());

    // One undo takes every file out again; redo puts them back.
    assert_eq!(app.history.undo_label(), Some("Import noodle"));
    app.undo();
    assert!(!folder.join(MACHINE).exists());
    assert_eq!(files_in(&folder, "assets/animations/noodle"), 0);
    assert!(!folder.join("assets/models/noodle.fbx").exists());
    app.redo();
    assert_eq!(files_in(&folder, "assets/animations/noodle"), 6);

    // Again: nothing to add, so Import is off and says why.
    let again = import(&mut app, &file);
    let package = again.package.as_ref().unwrap();
    assert!(package.plan.writes.is_empty());
    assert_eq!(
        package.sentence(),
        "Adds nothing. 7 files are already in the project, left as they are."
    );
    assert!(again.problem().is_some());
    let _ = fs::remove_file(&file);
    let _ = fs::remove_dir_all(&folder);
}

/// The project's Walk clip changed after the first import. Each choice
/// does what it says, the summary names the clash, and Asset Check finds
/// nothing broken after any of them. With `AXCENITY_SNAPSHOTS` set, the
/// dialog is saved as `import-dialog-package.png`.
#[test]
fn a_changed_clip_is_a_clash_settled_by_keep_both_replace_or_skip_existing() {
    let file = package_file("clash");
    export_noodle(&file);
    for clashes in [Clashes::KeepBoth, Clashes::Replace, Clashes::SkipExisting] {
        let folder = copy_of_example("hello-part", &format!("package-{clashes:?}"));
        let mut app = EditorApp::new(Some(&folder));
        let first = import(&mut app, &file);
        app.import_package_chosen(&first);
        let packaged = fs::read(folder.join(WALK)).unwrap();
        fs::write(folder.join(WALK), b"{\"kind\": \"changed\"}").unwrap();
        let changed = fs::read(folder.join(WALK)).unwrap();

        let mut dialog = import(&mut app, &file);
        let package = dialog.package.as_mut().unwrap();
        package.set_clashes(clashes);
        let sentence = package.sentence();
        assert!(
            sentence.contains("1 file already exists: Walk."),
            "{sentence}"
        );
        match clashes {
            Clashes::KeepBoth => assert!(
                sentence.ends_with("Keep both puts the package's in `assets/animations/noodle2/`."),
                "{sentence}"
            ),
            Clashes::Replace => assert!(sentence.starts_with("Adds 1 file: 1 clip."), "{sentence}"),
            Clashes::SkipExisting => assert!(sentence.starts_with("Adds nothing."), "{sentence}"),
        }
        if clashes == Clashes::KeepBoth
            && let Some(snapshots) = std::env::var_os("AXCENITY_SNAPSHOTS")
        {
            crate::snapshot::snapshot(
                &std::path::PathBuf::from(snapshots).join("import-dialog-package.png"),
                egui::vec2(800.0, 600.0),
                vec![Vec::new(); 6],
                |ctx| {
                    dialog.show(ctx, None);
                },
            );
        }
        if clashes != Clashes::SkipExisting {
            app.import_package_chosen(&dialog);
        }
        let walk = fs::read(folder.join(WALK)).unwrap();
        match clashes {
            Clashes::KeepBoth => {
                assert_eq!(walk, changed);
                let copy = folder.join("assets/animations/noodle2/Walk.anim.json");
                assert_eq!(fs::read(copy).unwrap(), packaged);
            }
            Clashes::Replace => assert_eq!(walk, packaged),
            Clashes::SkipExisting => assert_eq!(walk, changed),
        }
        // The project's own changed Walk is no clip any more; put a real
        // one back before checking, so only the import is judged.
        if clashes != Clashes::Replace {
            fs::write(folder.join(WALK), &packaged).unwrap();
        }
        let report = project::check_assets(&folder, &app.world);
        assert!(report.is_ok(), "{clashes:?}: {:?}", report.findings);
        let _ = fs::remove_dir_all(&folder);
    }
    let _ = fs::remove_file(&file);
}

#[test]
fn an_object_in_the_explorer_exports_as_a_prefab_with_every_asset_it_uses() {
    let source = copy_of_example("hello-animation", "package-object");
    let mut app = EditorApp::new(Some(&source));
    let noodle = app
        .world
        .find_first_child(app.world.workspace(), "Noodle")
        .expect("hello-animation has noodle");
    let (name, prefab) = app.object_prefab(noodle).unwrap();
    assert_eq!(name, "Noodle");
    assert_eq!(prefab.0, "assets/prefabs/Noodle.prefab.json");
    let file = package_file("object");
    assert!(app.export_package(&file, &name, Vec::new(), vec![prefab]));
    let package = project::read_package(&file).unwrap();
    let files: Vec<&str> = package.files.keys().map(String::as_str).collect();
    for wanted in [
        "assets/prefabs/Noodle.prefab.json",
        "assets/models/noodle.fbx",
        MACHINE,
        WALK,
    ] {
        assert!(files.contains(&wanted), "{wanted} in {files:?}");
    }
    // The project itself gains no prefab file.
    assert!(!source.join("assets/prefabs/Noodle.prefab.json").exists());

    // Imported into another project, the prefab and all it uses check out.
    let folder = copy_of_example("hello-part", "package-object-import");
    let mut other = EditorApp::new(Some(&folder));
    let dialog = import(&mut other, &file);
    other.import_package_chosen(&dialog);
    assert!(project::check_assets(&folder, &other.world).is_ok());
    // A script can't be a prefab.
    let script = app
        .world
        .descendants(app.world.root())
        .into_iter()
        .find(|&id| app.world.get(id).is_some_and(|i| i.class().is_script()));
    if let Some(script) = script {
        assert!(app.object_prefab(script).is_none());
    }
    let _ = fs::remove_file(&file);
    let _ = fs::remove_dir_all(&source);
    let _ = fs::remove_dir_all(&folder);
}
