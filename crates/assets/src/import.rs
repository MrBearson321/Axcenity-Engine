//! Copying a model file into a project, so the project has everything it
//! needs in its own folder.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::model::{self, Model, ModelError, ModelFormat, ModelObject, Pick, Take};

/// Where imported models go, inside the project folder.
pub const MODELS_FOLDER: &str = "assets/models";
/// Where an imported model's embedded textures are copied out to, inside
/// its folder.
pub const TEXTURES_FOLDER: &str = "textures";

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("only glTF and FBX models (.gltf, .glb or .fbx files) can be imported")]
    NotAModel,
    #[error("only PNG, JPEG, TGA and BMP textures can be used")]
    NotATexture,
    #[error("only WAV, OGG, MP3 and FLAC sounds can be used")]
    NotAudio,
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
/// A `.glb` or `.fbx` that uses no other files is copied as one file,
/// `<name>.glb` or `<name>.fbx`. A `.gltf`, or a model that uses other
/// files beside it (`.bin` buffers, textures) or embeds textures, goes into a folder of its
/// own, `<name>/<name>.gltf`, with them at the same paths. A number goes
/// after the name if it's taken. The model is loaded first, so a file the
/// engine can't load is never copied. An FBX texture that isn't there is
/// left out (see [`Model::warnings`]).
pub fn import_model(project: &Path, file: &Path) -> Result<(String, Model), ImportError> {
    let mut models = import_picks(project, file, &[Pick::Whole])?;
    Ok(models.remove(0))
}

/// Like [`import_model`], but copies the file once and returns a `MeshId`
/// and a loaded model for each of `picks` (some of the objects in it, see
/// [`list_model_objects`]).
pub fn import_picks(
    project: &Path,
    file: &Path,
    picks: &[Pick],
) -> Result<Vec<(String, Model)>, ImportError> {
    let format = ModelFormat::of(file).ok_or(ImportError::NotAModel)?;
    let extension = match file.extension().and_then(|extension| extension.to_str()) {
        Some(extension) => extension.to_ascii_lowercase(),
        None => return Err(ImportError::NotAModel),
    };
    let bytes = fs::read(file).map_err(ImportError::Read)?;
    let folder = file.parent().unwrap_or(Path::new("."));
    let models = model::load_models(&bytes, folder, format, picks)?;
    let used = model::files_used(&bytes, folder, format)?;
    let embedded = model::embedded_files(&bytes, format)?;

    let name = file_name(file);
    let models_folder = project.join(MODELS_FOLDER);
    let mesh_id = if used.is_empty() && embedded.is_empty() && extension != "gltf" {
        let name = free_name(&models_folder, &name, &format!(".{extension}"));
        let mesh_id = format!("{MODELS_FOLDER}/{name}.{extension}");
        copy(file, &project.join(&mesh_id))?;
        mesh_id
    } else {
        let name = free_name(&models_folder, &name, "");
        let copy_folder = models_folder.join(&name);
        for path in used {
            copy(&folder.join(&path), &copy_folder.join(&path))?;
        }
        // Embedded textures, copied out as files too, so they can be seen
        // and used on their own. The model still draws with its own copies.
        let textures = copy_folder.join(TEXTURES_FOLDER);
        for (texture, content) in embedded {
            let texture = unique_file_name(&texture, |taken| textures.join(taken).exists());
            write(&textures.join(texture), &content)?;
        }
        // Renamed to match its folder. Nothing in a model refers to its own file.
        copy(file, &copy_folder.join(format!("{name}.{extension}")))?;
        format!("{MODELS_FOLDER}/{name}/{name}.{extension}")
    };
    Ok(picks
        .iter()
        .map(|pick| pick.mesh_id(&mesh_id))
        .zip(models)
        .collect())
}

/// Where texture files picked from outside the project are copied to.
pub const PROJECT_TEXTURES_FOLDER: &str = "assets/textures";

/// The extensions of the texture files the engine reads.
pub const TEXTURE_EXTENSIONS: [&str; 5] = ["png", "jpg", "jpeg", "tga", "bmp"];

/// The path in `project` (with `/` between folders) of the texture `file`,
/// for a SurfaceAppearance's maps: its own if it's inside the project,
/// otherwise a copy's, made in `into` (a folder in the project, such as
/// [`PROJECT_TEXTURES_FOLDER`]) under a name not yet taken there.
pub fn import_texture(project: &Path, file: &Path, into: &str) -> Result<String, ImportError> {
    import_file(project, file, into, &TEXTURE_EXTENSIONS, "texture")
        .ok_or(ImportError::NotATexture)?
}

/// Where sound files picked from outside the project are copied to.
pub const PROJECT_AUDIO_FOLDER: &str = "assets/audio";

/// The extensions of the sound files the engine plays.
pub const AUDIO_EXTENSIONS: [&str; 4] = ["wav", "ogg", "mp3", "flac"];

/// The path in `project` of the sound `file`, for a Sound's `SoundId`: its
/// own if it's inside the project, otherwise a copy's in
/// [`PROJECT_AUDIO_FOLDER`], as [`import_texture`] does for textures.
pub fn import_audio(project: &Path, file: &Path) -> Result<String, ImportError> {
    import_file(
        project,
        file,
        PROJECT_AUDIO_FOLDER,
        &AUDIO_EXTENSIONS,
        "sound",
    )
    .ok_or(ImportError::NotAudio)?
}

/// `file`'s path in `project`, copying it into `into` first if it's
/// outside. `None` if its extension isn't one of `extensions`.
fn import_file(
    project: &Path,
    file: &Path,
    into: &str,
    extensions: &[&str],
    fallback_name: &str,
) -> Option<Result<String, ImportError>> {
    let extension = file
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    if !extensions.contains(&extension.as_str()) {
        return None;
    }
    if let (Ok(project), Ok(file)) = (project.canonicalize(), file.canonicalize())
        && let Ok(inside) = file.strip_prefix(&project)
    {
        let parts: Vec<String> = inside
            .components()
            .map(|part| part.as_os_str().to_string_lossy().into_owned())
            .collect();
        return Some(Ok(parts.join("/")));
    }
    let name = file
        .file_name()
        .and_then(|name| safe_file_name(&name.to_string_lossy()))
        .unwrap_or_else(|| format!("{fallback_name}.{extension}"));
    let folder = project.join(into);
    let name = unique_file_name(&name, |taken| folder.join(taken).exists());
    Some(copy(file, &folder.join(&name)).map(|()| format!("{}/{name}", into.trim_end_matches('/'))))
}

/// Every take (animation timeline) in the model file at `file` (outside
/// the project), for looking at before importing it.
pub fn load_takes_file(file: &Path) -> Result<Vec<Take>, ImportError> {
    let format = ModelFormat::of(file).ok_or(ImportError::NotAModel)?;
    let bytes = fs::read(file).map_err(ImportError::Read)?;
    Ok(model::load_takes(&bytes, format)?)
}

/// Copies an FBX file that holds only animation (takes, but nothing to
/// draw, such as an animation exported "without skin") into `project`'s
/// [`MODELS_FOLDER`], as `<name>.fbx` (a number added if taken), and
/// returns its path in the project for clips' `source`.
pub fn import_animation_file(project: &Path, file: &Path) -> Result<String, ImportError> {
    let format = ModelFormat::of(file).ok_or(ImportError::NotAModel)?;
    let extension = file
        .extension()
        .and_then(|extension| extension.to_str())
        .ok_or(ImportError::NotAModel)?
        .to_ascii_lowercase();
    let bytes = fs::read(file).map_err(ImportError::Read)?;
    // Checked first, so a file that isn't an animation is never copied.
    model::load_takes(&bytes, format)?;
    let name = free_name(
        &project.join(MODELS_FOLDER),
        &file_name(file),
        &format!(".{extension}"),
    );
    let path = format!("{MODELS_FOLDER}/{name}.{extension}");
    copy(file, &project.join(&path))?;
    Ok(path)
}

/// The model at `file` (outside the project), loaded from where it is,
/// for looking at before importing it.
pub fn load_model_file(file: &Path) -> Result<Model, ImportError> {
    let format = ModelFormat::of(file).ok_or(ImportError::NotAModel)?;
    let bytes = fs::read(file).map_err(ImportError::Read)?;
    let folder = file.parent().unwrap_or(Path::new("."));
    Ok(model::load_model(&bytes, folder, format)?)
}

/// The objects in the model at `file`, for choosing which to import.
pub fn list_model_objects(file: &Path) -> Result<Vec<ModelObject>, ImportError> {
    let format = ModelFormat::of(file).ok_or(ImportError::NotAModel)?;
    let bytes = fs::read(file).map_err(ImportError::Read)?;
    Ok(model::list_objects(&bytes, format)?)
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

/// The last part of `path` (with `/` or `\` between folders) as a file
/// name that works on every OS: letters, digits, spaces, `-`, `_` and `.`
/// kept, anything else `_`. `None` if nothing is left but dots and spaces.
pub(crate) fn safe_file_name(path: &str) -> Option<String> {
    let name = path.rsplit(['/', '\\']).next().unwrap_or_default();
    let name: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let name = name.trim_matches([' ', '.']);
    (!name.is_empty()).then(|| name.to_owned())
}

/// `name`, or `name` with 2, 3… before its extension, the first that
/// `taken` says isn't.
pub(crate) fn unique_file_name(name: &str, taken: impl Fn(&str) -> bool) -> String {
    let (stem, extension) = match name.rsplit_once('.') {
        Some((stem, extension)) => (stem, format!(".{extension}")),
        None => (name, String::new()),
    };
    (1u32..)
        .map(|n| match n {
            1 => name.to_owned(),
            n => format!("{stem}{n}{extension}"),
        })
        .find(|name| !taken(name))
        .expect("some number is always free")
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

fn write(to: &Path, content: &[u8]) -> Result<(), ImportError> {
    let error = |source| ImportError::Copy {
        path: to.to_owned(),
        source,
    };
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent).map_err(error)?;
    }
    fs::write(to, content).map_err(error)
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
    fn a_glb_is_copied_with_its_embedded_textures_copied_out() {
        // A .glb is a header and the JSON, padded to 4 bytes (this model
        // embeds its buffer, so it needs no binary chunk).
        let mut json = fs::read(example_model()).unwrap();
        while !json.len().is_multiple_of(4) {
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
        // Its texture is embedded, so it gets a folder to copy that out to.
        assert_eq!(mesh_id, "assets/models/Pyramid/Pyramid.glb");
        let textures = project.join("assets/models/Pyramid/textures");
        assert_eq!(fs::read_dir(textures).unwrap().count(), 1);
        assert!(Assets::new(&project).model(&mesh_id).is_some());
    }

    #[test]
    fn an_fbx_that_uses_no_other_files_is_copied_as_one_file() {
        let source = temp_folder("import-fbx-source");
        fs::write(source.join("Pyramid.FBX"), model::pyramid_fbx(false, None)).unwrap();
        let project = temp_folder("import-fbx");
        let (mesh_id, model) = import_model(&project, &source.join("Pyramid.FBX")).unwrap();
        assert_eq!(mesh_id, "assets/models/Pyramid.fbx");
        assert!((model.size[1] - 3.0).abs() < 1e-4, "{:?}", model.size);
        let (second, _) = import_model(&project, &source.join("Pyramid.FBX")).unwrap();
        assert_eq!(second, "assets/models/Pyramid2.fbx");
        let assets = Assets::new(&project);
        assert!(assets.model(&mesh_id).is_some());
        assert!(assets.take_errors().is_empty());
    }

    #[test]
    fn an_fbx_goes_into_a_folder_with_its_textures() {
        let source = temp_folder("import-fbx-texture-source");
        fs::create_dir_all(source.join("textures")).unwrap();
        fs::write(source.join("textures/checker.png"), model::checker_png()).unwrap();
        // The file names the texture by where it was on another machine.
        let fbx = model::pyramid_fbx(false, Some("../../maps/checker.png"));
        fs::write(source.join("robot.fbx"), fbx).unwrap();
        let project = temp_folder("import-fbx-texture");
        let (mesh_id, _) = import_model(&project, &source.join("robot.fbx")).unwrap();
        assert_eq!(mesh_id, "assets/models/robot/robot.fbx");
        assert!(
            project
                .join("assets/models/robot/textures/checker.png")
                .is_file()
        );
        let assets = Assets::new(&project);
        let model = assets.model(&mesh_id).unwrap();
        assert!(model.primitives[0].material.base_color_texture.is_some());
        assert!(assets.take_errors().is_empty());
    }

    #[test]
    fn an_fbxs_embedded_textures_are_copied_out_next_to_it() {
        let source = temp_folder("import-fbx-embedded-source");
        let png = model::checker_png();
        let fbx = model::pyramid_fbx_with_textures(
            false,
            &[model::TestTexture {
                property: Some("DiffuseColor"),
                path: "C:/Users/artist/Desktop/wood.png",
                embedded: Some(&png),
            }],
        );
        fs::write(source.join("crate.fbx"), fbx).unwrap();
        let project = temp_folder("import-fbx-embedded");
        let (mesh_id, model) = import_model(&project, &source.join("crate.fbx")).unwrap();
        assert!(model.warnings.is_empty(), "{:?}", model.warnings);
        assert_eq!(mesh_id, "assets/models/crate/crate.fbx");
        let copied = project.join("assets/models/crate/textures/wood.png");
        assert_eq!(fs::read(copied).unwrap(), png);
    }

    #[test]
    fn an_fbx_texture_that_is_not_there_is_reported_once() {
        let source = temp_folder("import-fbx-missing-source");
        let fbx = model::pyramid_fbx(false, Some("wood.png"));
        fs::write(source.join("crate.fbx"), fbx).unwrap();
        let project = temp_folder("import-fbx-missing");
        let (mesh_id, model) = import_model(&project, &source.join("crate.fbx")).unwrap();
        assert_eq!(mesh_id, "assets/models/crate.fbx");
        assert_eq!(model.warnings.len(), 1);
        let assets = Assets::new(&project);
        assert!(assets.model(&mesh_id).is_some());
        assert!(assets.model(&mesh_id).is_some());
        let errors = assets.take_errors();
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(
            errors[0].starts_with(
                "The model `assets/models/crate.fbx` is drawn without its texture `wood.png`"
            ),
            "{errors:?}"
        );
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
            Err(ImportError::NotAModel)
        ));
        assert!(!project.join(MODELS_FOLDER).exists());
    }
}
