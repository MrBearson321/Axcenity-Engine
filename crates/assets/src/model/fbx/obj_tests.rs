//! OBJ files (P7.1), read by `ufbx` with their `.mtl` material library.

use std::path::{Path, PathBuf};

use super::tests::checker_png;
use crate::model::{ModelFormat, Pick, list_objects, load_model, load_models};

/// Two boxes, "Crate" (2 × 2 × 2 at the origin, wooden, textured) and
/// "Lid" (2 × 1 × 2 standing on top of it, red), with `mtllib` naming
/// `library`.
fn crates_obj(library: &str) -> String {
    let mut obj = format!("# Two boxes\nmtllib {library}\n");
    let mut corner = 1;
    for (name, material, low, high) in [
        ("Crate", "Wood", [-1.0, 0.0, -1.0], [1.0, 2.0, 1.0]),
        ("Lid", "Paint", [-1.0, 2.0, -1.0], [1.0, 3.0, 1.0]),
    ] {
        obj.push_str(&format!("o {name}\n"));
        for x in [low[0], high[0]] {
            for y in [low[1], high[1]] {
                for z in [low[2], high[2]] {
                    obj.push_str(&format!("v {x} {y} {z}\n"));
                }
            }
        }
        obj.push_str("vt 0 0\nvt 1 0\nvt 1 1\nvt 0 1\n");
        obj.push_str(&format!("usemtl {material}\n"));
        // The six faces of the box, by its corners' numbers (x, y, z bits).
        for face in [
            [0, 1, 3, 2],
            [4, 6, 7, 5],
            [0, 4, 5, 1],
            [2, 3, 7, 6],
            [0, 2, 6, 4],
            [1, 5, 7, 3],
        ] {
            let uv = (corner - 1) / 8 * 4 + 1;
            let corners: Vec<String> = face
                .iter()
                .enumerate()
                .map(|(i, c)| format!("{}/{}", corner + c, uv + i))
                .collect();
            obj.push_str(&format!("f {}\n", corners.join(" ")));
        }
        corner += 8;
    }
    obj
}

const LIBRARY: &str =
    "newmtl Wood\nKd 0.8 0.6 0.4\nmap_Kd textures/wood.png\n\nnewmtl Paint\nKd 1 0 0\n";

fn temp_folder(name: &str) -> PathBuf {
    let folder = std::env::temp_dir().join(format!("axcenity-obj-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).unwrap();
    folder
}

/// A folder with `crates.obj`, `crates.mtl` and `textures/wood.png`.
fn crates_folder(name: &str) -> PathBuf {
    let folder = temp_folder(name);
    std::fs::write(folder.join("crates.obj"), crates_obj("crates.mtl")).unwrap();
    std::fs::write(folder.join("crates.mtl"), LIBRARY).unwrap();
    std::fs::create_dir_all(folder.join("textures")).unwrap();
    std::fs::write(folder.join("textures/wood.png"), checker_png()).unwrap();
    folder
}

fn obj(folder: &Path) -> Vec<u8> {
    std::fs::read(folder.join("crates.obj")).unwrap()
}

#[test]
fn obj_files_are_found_by_their_extension() {
    assert_eq!(
        ModelFormat::of(Path::new("scan/Statue.OBJ")),
        Some(ModelFormat::Obj)
    );
    assert!(crate::takes_asset_id("assets/models/statue.obj"));
}

#[test]
fn an_obj_file_loads_with_its_materials_and_textures() {
    let folder = crates_folder("load");
    let model = load_model(&obj(&folder), &folder, ModelFormat::Obj).unwrap();
    assert!(model.warnings.is_empty(), "{:?}", model.warnings);
    for (size, expected) in model.size.iter().zip([2.0, 3.0, 2.0]) {
        assert!((size - expected).abs() < 1e-4, "{:?}", model.size);
    }
    assert_eq!(model.primitives.len(), 2);
    let material = |name: &str| {
        &model
            .primitives
            .iter()
            .find(|primitive| primitive.material.name == name)
            .unwrap_or_else(|| panic!("a {name} primitive"))
            .material
    };
    let wood = material("Wood");
    assert!(
        (wood.base_color[1] - 0.6).abs() < 0.02,
        "{:?}",
        wood.base_color
    );
    let texture = wood.base_color_texture.as_ref().expect("map_Kd is read");
    assert_eq!((texture.width, texture.height), (2, 2));
    let paint = material("Paint");
    assert!(paint.base_color_texture.is_none());
    assert!((paint.base_color[0] - 1.0).abs() < 0.02 && paint.base_color[1] < 0.02);
}

#[test]
fn an_obj_files_objects_are_listed_and_picked_like_fbx_ones() {
    let folder = crates_folder("objects");
    let bytes = obj(&folder);
    let objects = list_objects(&bytes, ModelFormat::Obj).unwrap();
    let names: Vec<(&str, usize)> = objects
        .iter()
        .map(|object| (object.path.as_str(), object.triangles))
        .collect();
    assert_eq!(names, [("Crate", 12), ("Lid", 12)]);
    assert_eq!(objects[0].materials, ["Wood"]);
    let models = load_models(
        &bytes,
        &folder,
        ModelFormat::Obj,
        &[Pick::Objects(vec!["Lid".into()])],
    )
    .unwrap();
    let lid = &models[0];
    for (size, expected) in lid.size.iter().zip([2.0, 1.0, 2.0]) {
        assert!((size - expected).abs() < 1e-4, "{:?}", lid.size);
    }
    assert!((lid.center[1] - 2.5).abs() < 1e-4, "{:?}", lid.center);
}

#[test]
fn an_obj_without_its_material_library_loads_and_says_so() {
    let folder = temp_folder("no-library");
    let bytes = crates_obj("gone.mtl");
    let model = load_model(bytes.as_bytes(), &folder, ModelFormat::Obj).unwrap();
    assert_eq!(model.primitives.len(), 2);
    assert_eq!(
        model.warnings,
        [
            "uses the material library `gone.mtl`, which isn't next to it, so it's drawn without its materials"
        ]
    );
    // A library outside the folder is never read.
    assert_eq!(super::obj_library(b"mtllib ../secret.mtl\n"), None);
    assert_eq!(
        super::obj_library(b"# made by hand\nmtllib  my materials.mtl \n"),
        Some(PathBuf::from("my materials.mtl"))
    );
    assert_eq!(super::obj_library(b"mtllibrary.mtl\n"), None);
}

#[test]
fn broken_obj_files_give_errors_not_crashes() {
    let folder = temp_folder("broken");
    let error = |bytes: &[u8]| {
        load_model(bytes, &folder, ModelFormat::Obj)
            .unwrap_err()
            .to_string()
    };
    assert_eq!(error(b""), "it has no triangles to draw");
    assert_eq!(error(b"v 0 0 0\nv 1 0 0\n"), "it has no triangles to draw");
    // Corners that aren't there.
    let bad = error(b"f 1 2 3\n");
    assert!(
        bad == "it has no triangles to draw" || bad.contains("isn't a valid OBJ file"),
        "{bad}"
    );
}

#[test]
fn importing_an_obj_copies_its_library_and_textures_beside_it() {
    let folder = crates_folder("import-source");
    let project = temp_folder("import-project");
    let (mesh_id, model) = crate::import_model(&project, &folder.join("crates.obj")).unwrap();
    assert_eq!(mesh_id, "assets/models/crates/crates.obj");
    assert!(model.warnings.is_empty(), "{:?}", model.warnings);
    for file in ["crates.obj", "crates.mtl", "textures/wood.png"] {
        assert!(
            project.join("assets/models/crates").join(file).is_file(),
            "{file} is copied"
        );
    }
    // And it loads from the project as any model does, textures and all.
    let copy = project.join("assets/models/crates");
    let again = load_model(&obj(&copy), &copy, ModelFormat::Obj).unwrap();
    assert!(
        again
            .primitives
            .iter()
            .any(|primitive| primitive.material.base_color_texture.is_some())
    );
}
