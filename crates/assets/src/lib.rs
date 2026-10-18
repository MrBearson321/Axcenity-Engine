//! Axcenity assets: the files a project uses besides its world and scripts.
//!
//! - [`Assets`] loads each model a MeshPart names the first time
//!   something asks for it, and keeps it. The player and the editor make one
//!   per project and share it (it's a cheap handle) with `physics` and `scene`.
//! - [`load_model`] reads a `.gltf`, `.glb` or `.fbx` file into a [`Model`]: plain
//!   triangles, materials and textures, fitted into the 1×1×1 cube that every
//!   part's mesh fills.
//! - [`import_model`] copies a model file (and the files it uses) into a
//!   project's `assets/models` folder; [`list_model_objects`] and
//!   [`import_picks`] import some of the objects in it, each a MeshPart
//!   whose `MeshId` names them ([`Pick`]).
//!
//! Every file is read from inside the project folder ([`safe_path`]), so a
//! world file or script can't make the engine read anything else.

mod import;
mod model;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

pub use import::{ImportError, MODELS_FOLDER, import_model, import_picks, list_model_objects};
pub use model::{
    AlphaMode, Image, Model, ModelError, ModelFormat, ModelMaterial, ModelObject, Pick, Primitive,
    list_objects, load_model, load_models,
};

/// The project's loaded assets. Cloning gives another handle to the same
/// assets, so a model is loaded once however many parts use it.
///
/// `Arc<Mutex<…>>` shares one value between owners (`Arc`) and lets any of
/// them change it, one at a time (`Mutex`). Only one thread uses it today,
/// so the lock never waits.
#[derive(Clone, Default)]
pub struct Assets {
    shared: Arc<Mutex<Shared>>,
}

#[derive(Default)]
struct Shared {
    /// The project folder, or `None` when no project is open (then there
    /// are no models).
    folder: Option<PathBuf>,
    /// Every model asked for so far, by its `MeshId`. `None` means it
    /// couldn't be loaded, so it isn't tried again.
    models: HashMap<String, Option<Arc<Model>>>,
    /// Loading errors not yet collected by [`Assets::take_errors`].
    errors: Vec<String>,
}

impl Assets {
    /// The assets of the project in `folder`.
    pub fn new(folder: impl Into<PathBuf>) -> Self {
        let assets = Self::default();
        assets.lock().folder = Some(folder.into());
        assets
    }

    /// The model at `mesh_id` (a MeshPart's `MeshId`, such as
    /// `assets/models/chair.glb`), loading it the first time. `None` if
    /// `mesh_id` is empty or the model can't be loaded; the reason goes to
    /// [`take_errors`](Self::take_errors), once, as do the model's
    /// [warnings](Model::warnings).
    pub fn model(&self, mesh_id: &str) -> Option<Arc<Model>> {
        if mesh_id.is_empty() {
            return None;
        }
        let mut shared = self.lock();
        if let Some(model) = shared.models.get(mesh_id) {
            return model.clone();
        }
        let folder = shared.folder.clone()?;
        let model = match read_model(&folder, mesh_id) {
            Ok(model) => {
                for warning in &model.warnings {
                    shared
                        .errors
                        .push(format!("The model `{mesh_id}` {warning}."));
                }
                Some(Arc::new(model))
            }
            Err(message) => {
                shared
                    .errors
                    .push(format!("Could not load the model `{mesh_id}`: {message}"));
                None
            }
        };
        shared.models.insert(mesh_id.to_owned(), model.clone());
        model
    }

    /// Adds `model` as the one at `mesh_id`, for a model just imported, so
    /// it isn't read again.
    pub fn insert(&self, mesh_id: &str, model: Model) -> Arc<Model> {
        let model = Arc::new(model);
        self.lock()
            .models
            .insert(mesh_id.to_owned(), Some(model.clone()));
        model
    }

    /// Removes and returns the loading errors since the last call, one line
    /// each, for the Output panel or the terminal.
    pub fn take_errors(&self) -> Vec<String> {
        std::mem::take(&mut self.lock().errors)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Shared> {
        // A panic while the lock was held leaves the data as it was (loading
        // catches its own panics), so carry on with it.
        self.shared.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Reads the model at `mesh_id` inside `folder`, or explains why it can't.
fn read_model(folder: &Path, mesh_id: &str) -> Result<Model, String> {
    let (file, pick) = Pick::of_mesh_id(mesh_id);
    let relative = safe_path(file).ok_or(
        "MeshId must be a path inside the project folder, such as \"assets/models/chair.glb\"",
    )?;
    let format = ModelFormat::of(&relative)
        .ok_or("MeshId must name a model file: a .gltf, .glb or .fbx file")?;
    let path = folder.join(relative);
    let bytes = std::fs::read(&path).map_err(|err| match err.kind() {
        std::io::ErrorKind::NotFound => "the file isn't there".to_owned(),
        _ => err.to_string(),
    })?;
    let base = path.parent().unwrap_or(folder);
    load_models(&bytes, base, format, &[pick])
        .map(|mut models| models.remove(0))
        .map_err(|err| with_causes(&err))
}

/// An error and its causes on one line: "it isn't a valid glTF file:
/// invalid glTF: …".
fn with_causes(err: &dyn std::error::Error) -> String {
    let mut text = err.to_string();
    let mut source = err.source();
    while let Some(cause) = source {
        text.push_str(&format!(": {cause}"));
        source = cause.source();
    }
    text
}

/// `path` (with `/` or `\` between folders) as a path that stays inside the
/// folder it's relative to, or `None` if it could leave it: an absolute
/// path, a drive or URL scheme (`C:`, `file:`), or a `..` part.
pub fn safe_path(path: &str) -> Option<PathBuf> {
    if path.contains(':') || path.starts_with(['/', '\\']) {
        return None;
    }
    let mut safe = PathBuf::new();
    for part in path.split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." => return None,
            part => safe.push(part),
        }
    }
    (!safe.as_os_str().is_empty()).then_some(safe)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_paths_stay_inside_the_folder() {
        assert_eq!(
            safe_path("assets/models/chair.glb"),
            Some(PathBuf::from("assets").join("models").join("chair.glb"))
        );
        assert_eq!(
            safe_path("./a\\b.bin"),
            Some(PathBuf::from("a").join("b.bin"))
        );
        for unsafe_path in [
            "",
            "/etc/passwd",
            "\\server\\share",
            "C:/Windows/win.ini",
            "file:///x",
            "assets/../../secret",
            "..",
        ] {
            assert_eq!(safe_path(unsafe_path), None, "{unsafe_path}");
        }
    }

    #[test]
    fn a_missing_model_is_reported_once_and_empty_ids_never() {
        let assets = Assets::new(std::env::temp_dir().join("axcenity-no-such-project"));
        assert!(assets.model("").is_none());
        assert!(assets.model("assets/models/gone.glb").is_none());
        assert!(assets.model("assets/models/gone.glb").is_none());
        let errors = assets.take_errors();
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(errors[0].contains("`assets/models/gone.glb`"), "{errors:?}");
        assert!(errors[0].contains("isn't there"), "{errors:?}");
        assert!(assets.take_errors().is_empty());
    }

    #[test]
    fn paths_outside_the_project_are_refused() {
        let assets = Assets::new(std::env::temp_dir());
        assert!(assets.model("../secret.glb").is_none());
        assert!(assets.take_errors()[0].contains("inside the project folder"));
    }

    #[test]
    fn only_model_files_are_loaded() {
        let assets = Assets::new(std::env::temp_dir());
        assert!(assets.model("assets/notes.txt").is_none());
        assert!(assets.take_errors()[0].contains(".gltf, .glb or .fbx"));
    }

    #[test]
    fn without_a_project_there_are_no_models_and_no_errors() {
        let assets = Assets::default();
        assert!(assets.model("assets/models/chair.glb").is_none());
        assert!(assets.take_errors().is_empty());
    }
}
