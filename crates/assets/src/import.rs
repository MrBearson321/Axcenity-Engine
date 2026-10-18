//! Copying a model file into a project, so the project has everything it
//! needs in its own folder.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::model::{self, Model, ModelError};

/// Where imported models go, inside the project folder.
pub const MODELS_FOLDER: &str = "assets/models";

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("only glTF models (.gltf or .glb files) can be imported")]
    NotGltf,
    #[error("could not read it")]
    Read(#[source] io::Error),
    #[error(transparent)]
    Model(#[from] ModelError),
    #[error("could not copy `{}` into the project", path.display())]
    Copy {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

/// Copies the model at `file` into `project`'s [`MODELS_FOLDER`], and
/// returns its `MeshId` (its path in the project, with `/` between folders)
/// and the model, loaded.
///
/// A `.glb` is a single file, copied as `<name>.glb`. A `.gltf` can use
/// other files beside it (`.bin` buffers, textures), so it goes into a
/// folder of its own, `<name>/<name>.gltf`, with them. A number goes after
/// the name if it's taken. The model is loaded first, so a file the engine
/// can't load is never copied.
pub fn import_model(project: &Path, file: &Path) -> Result<(String, Model), ImportError> {
    let extension = file
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase);
    let binary = match extension.as_deref() {
        Some("glb") => true,
        Some("gltf") => false,
        _ => return Err(ImportError::NotGltf),
    };
    let bytes = fs::read(file).map_err(ImportError::Read)?;
    let folder = file.parent().unwrap_or(Path::new("."));
    let model = model::load_model(&bytes, folder)?;

    let name = file_name(file);
    let models = project.join(MODELS_FOLDER);
    if binary {
        let name = free_name(&models, &name, ".glb");
        let mesh_id = format!("{MODELS_FOLDER}/{name}.glb");
        copy(file, &project.join(&mesh_id))?;
        return Ok((mesh_id, model));
    }

    let name = free_name(&models, &name, "");
    let copy_folder = models.join(&name);
    // It loaded, so it reads again; the error can't happen.
    let document = gltf::Gltf::from_slice(&bytes).map_err(ModelError::Gltf)?;
    for uri in model::external_files(&document) {
        let path = model::uri_path(&uri)?;
        copy(&folder.join(&path), &copy_folder.join(&path))?;
    }
    // Renamed to match its folder. Nothing in a model refers to its own file.
    copy(file, &copy_folder.join(format!("{name}.gltf")))?;
    Ok((format!("{MODELS_FOLDER}/{name}/{name}.gltf"), model))
}

/// The model's file name without its extension, keeping only letters,
/// digits, spaces, `-` and `_`, so the copy's path works on every OS and as
/// a `MeshId`.
fn file_name(file: &Path) -> String {
    let stem = file
        .file_stem()
        .map(|stem| stem.to_string_lossy())
        .unwrap_or_default();
    let name: String = stem
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let name = name.trim();
    if name.is_empty() {
        "Model".to_owned()
    } else {
        name.to_owned()
    }
}

/// `name`, or `name2`, `name3`… the first with nothing called that (plus
/// `suffix`) in `folder`.
fn free_name(folder: &Path, name: &str, suffix: &str) -> String {
    (1u32..)
        .map(|n| match n {
            1 => name.to_owned(),
            n => format!("{name}{n}"),
        })
        .find(|name| !folder.join(format!("{name}{suffix}")).exists())
        .expect("some number is always free")
}

fn copy(from: &Path, to: &Path) -> Result<(), ImportError> {
    let error = |source| ImportError::Copy {
        path: to.to_owned(),
        source,
    };
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent).map_err(error)?;
    }
    fs::copy(from, to).map_err(error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Assets;

    fn temp_folder(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!("axcenity-assets-{name}"));
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(&folder).unwrap();
        folder
    }

    fn example_model() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/hello-mesh/assets/models/pyramid.gltf")
    }

    #[test]
    fn a_gltf_goes_into_a_folder_of_its_own_under_a_free_name() {
        let project = temp_folder("import-gltf");
        let (first, model) = import_model(&project, &example_model()).unwrap();
        assert_eq!(first, "assets/models/pyramid/pyramid.gltf");
        assert_eq!(model.size, [4.0, 3.0, 4.0]);
        let (second, _) = import_model(&project, &example_model()).unwrap();
        assert_eq!(second, "assets/models/pyramid2/pyramid2.gltf");
        // The copies load from the project like any MeshId.
        let assets = Assets::new(&project);
        assert!(assets.model(&first).is_some());
        assert!(assets.model(&second).is_some());
        assert!(assets.take_errors().is_empty());
    }

    /// A one-triangle model whose buffer is a separate file in a subfolder,
    /// with a space in its name.
    fn write_triangle_with_a_bin_file(folder: &Path) -> PathBuf {
        let corners: [f32; 9] = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
        let bytes: Vec<u8> = corners.iter().flat_map(|c| c.to_le_bytes()).collect();
        fs::create_dir_all(folder.join("data")).unwrap();
        fs::write(folder.join("data/tri angle.bin"), bytes).unwrap();
        let gltf = r#"{
            "asset": { "version": "2.0" },
            "buffers": [ { "uri": "data/tri%20angle.bin", "byteLength": 36 } ],
            "bufferViews": [ { "buffer": 0, "byteLength": 36 } ],
            "accessors": [ { "bufferView": 0, "componentType": 5126, "count": 3,
                             "type": "VEC3", "min": [0, 0, 0], "max": [1, 1, 0] } ],
            "meshes": [ { "primitives": [ { "attributes": { "POSITION": 0 } } ] } ]
        }"#;
        let file = folder.join("Tri.angle!.gltf");
        fs::write(&file, gltf).unwrap();
        file
    }

    #[test]
    fn the_files_a_gltf_uses_are_copied_with_it() {
        let source = temp_folder("import-source");
        let file = write_triangle_with_a_bin_file(&source);
        let project = temp_folder("import-bin");
        let (mesh_id, model) = import_model(&project, &file).unwrap();
        // The name loses characters that don't work everywhere.
        assert_eq!(mesh_id, "assets/models/Tri_angle_/Tri_angle_.gltf");
        assert_eq!(model.size, [1.0, 1.0, 0.0]);
        assert!(
            project
                .join("assets/models/Tri_angle_/data/tri angle.bin")
                .is_file()
        );
        assert!(Assets::new(&project).model(&mesh_id).is_some());
    }

    #[test]
    fn a_glb_is_copied_as_one_file() {
        // A .glb is a header and the JSON, padded to 4 bytes (this model
        // embeds its buffer, so it needs no binary chunk).
        let mut json = fs::read(example_model()).unwrap();
        while json.len() % 4 != 0 {
            json.push(b' ');
        }
        let mut glb = Vec::new();
        glb.extend(b"glTF");
        glb.extend(2u32.to_le_bytes());
        glb.extend((12 + 8 + json.len() as u32).to_le_bytes());
        glb.extend((json.len() as u32).to_le_bytes());
        glb.extend(b"JSON");
        glb.extend(json);
        let source = temp_folder("import-glb-source");
        fs::write(source.join("Pyramid.GLB"), glb).unwrap();

        let project = temp_folder("import-glb");
        let (mesh_id, _) = import_model(&project, &source.join("Pyramid.GLB")).unwrap();
        assert_eq!(mesh_id, "assets/models/Pyramid.glb");
        assert!(Assets::new(&project).model(&mesh_id).is_some());
    }

    #[test]
    fn a_file_that_does_not_load_is_not_copied() {
        let source = temp_folder("import-broken-source");
        fs::write(source.join("broken.glb"), b"glTF and then nonsense").unwrap();
        fs::write(source.join("notes.txt"), b"hello").unwrap();
        let project = temp_folder("import-broken");
        let broken = import_model(&project, &source.join("broken.glb")).unwrap_err();
        assert!(
            broken.to_string().contains("isn't a valid glTF file"),
            "{broken}"
        );
        assert!(matches!(
            import_model(&project, &source.join("notes.txt")),
            Err(ImportError::NotGltf)
        ));
        assert!(!project.join(MODELS_FOLDER).exists());
    }
}
