//! USD files converted to `.glb` on import (ADR-0027).

use std::fs;
use std::path::PathBuf;

use super::super::tests::{copy_of_example, printed};
use super::*;

/// A Z-up USD layer in centimetres with one 200 cm box under `World/Crate`,
/// and a sphere glTF import doesn't bring in.
const CRATE_USDA: &str = r#"#usda 1.0
(
    defaultPrim = "World"
    metersPerUnit = 0.01
    upAxis = "Z"
)

def Xform "World"
{
    def Mesh "Crate"
    {
        int[] faceVertexCounts = [4, 4, 4, 4, 4, 4]
        int[] faceVertexIndices = [0, 1, 3, 2, 4, 6, 7, 5, 0, 4, 5, 1, 2, 3, 7, 6, 0, 2, 6, 4, 1, 5, 7, 3]
        point3f[] points = [(-100, -100, -100), (100, -100, -100), (-100, 100, -100), (100, 100, -100), (-100, -100, 100), (100, -100, 100), (-100, 100, 100), (100, 100, 100)]
    }

    def Sphere "Ball"
    {
    }
}
"#;

#[test]
fn a_usd_file_is_converted_and_imported_as_a_glb_that_remembers_it() {
    let folder = copy_of_example("hello-part", "import-usd");
    let mut app = EditorApp::new(Some(&folder));
    let usd = folder.join("..").join("import-usd-Crate.usda");
    fs::write(&usd, CRATE_USDA).unwrap();

    // Import… and dropping both come here.
    app.import_files(vec![usd.clone()], axcenity_assets::PROJECT_TEXTURES_FOLDER);
    let lines = printed(&app);
    assert!(
        lines
            .iter()
            .any(|line| line.contains("Converting `import-usd-Crate.usda`")
                && line.contains("Sphere")),
        "{lines:?}"
    );
    // One object, no maps: it goes straight in, named after the USD file.
    assert!(app.importing.is_none());
    let copy = "assets/models/import-usd-Crate.glb";
    assert!(folder.join(copy).is_file(), "{:?}", printed(&app));
    assert!(
        printed(&app)
            .iter()
            .any(|line| line.contains("import-usd-Crate.usda") && line.contains(copy))
    );
    assert_eq!(
        axcenity_assets::read_converted_from(&folder, copy).as_deref(),
        Some("import-usd-Crate.usda")
    );
    // Centimetres made metres: a 2 stud box.
    let id = app.selected.expect("the new MeshPart is selected");
    assert_eq!(
        app.world.get(id).unwrap().property("Size"),
        Some(&Value::Vector3(Vector3::new(2.0, 2.0, 2.0)))
    );

    // The note is part of the import's undo step.
    app.undo();
    assert!(!folder.join(copy).exists());
    assert!(!folder.join(axcenity_assets::import_path(copy)).exists());
    let _ = fs::remove_file(&usd);
    let _ = fs::remove_dir_all(&folder);
}

#[test]
fn a_usd_file_that_cant_be_read_is_reported_not_imported() {
    let folder = copy_of_example("hello-part", "import-usd-bad");
    let mut app = EditorApp::new(Some(&folder));
    let usd = folder.join("..").join("import-usd-bad.usda");
    fs::write(&usd, "#usda 1.0\ndef Xform \"World\" {\n").unwrap();
    app.import_files(vec![usd.clone()], axcenity_assets::PROJECT_TEXTURES_FOLDER);
    assert!(app.importing.is_none());
    assert!(
        printed(&app)
            .iter()
            .any(|line| line.contains("Could not import") && line.contains("import-usd-bad.usda"))
    );
    assert!(!folder.join("assets/models/import-usd-bad.glb").exists());
    let _ = fs::remove_file(&usd);
    let _ = fs::remove_dir_all(&folder);
}

#[test]
fn the_import_dialog_names_the_usd_file() {
    let mut dialog = ImportDialog::new(PathBuf::from("/tmp/x/Kitchen.glb"), Vec::new(), Vec::new());
    assert_eq!(dialog.shown_file(), Path::new("/tmp/x/Kitchen.glb"));
    dialog.source = Some(PathBuf::from("/home/me/Kitchen.usdz"));
    assert_eq!(dialog.shown_file(), Path::new("/home/me/Kitchen.usdz"));
}
