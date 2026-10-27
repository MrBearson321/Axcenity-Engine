//! Axcenity assets: the files a project uses besides its world and scripts.
//!
//! - [`Assets`] loads each model a MeshPart names the first time
//!   something asks for it, and keeps it. The player and the editor make one
//!   per project and share it (it's a cheap handle) with `physics` and `scene`.
//! - [`Assets::material`] reads a `*.material.json` file a part's
//!   `MaterialVariant` names, with its base files ([`MaterialFile`]).
//! - [`Assets::data_table`] reads a `*.data.json` table scripts read
//!   through `DataService` ([`DataTable`]).
//! - [`Assets::sky_image`] reads the sky image Lighting's `SkyImage` names
//!   ([`SkyImage`]).
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

mod data_table;
mod fonts;
mod import;
mod material;
mod model;
mod reload;
mod sky_image;
mod texture_set;

use std::collections::{HashMap, HashSet, VecDeque};
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

pub use data_table::{
    AssetType, Cell, Column, ColumnType, DATA_FOLDER, DATA_TABLE_SUFFIX, DataTable, DataTableError,
    Row, data_table_name, data_table_path, is_data_table, read_data_table, write_data_table,
};
pub use fonts::{FontFile, is_font_file};
pub use import::{
    AUDIO_EXTENSIONS, ImportError, MODELS_FOLDER, PROJECT_AUDIO_FOLDER, PROJECT_SKY_FOLDER,
    PROJECT_TEXTURES_FOLDER, TEXTURE_EXTENSIONS, TEXTURES_FOLDER, import_animation_file,
    import_audio, import_model, import_picks, import_sky_image, import_texture, list_model_objects,
    load_model_file, load_takes_file, safe_file_name, unique_file_name,
};
pub use material::{
    MATERIAL_SUFFIX, MATERIALS_FOLDER, MapFile, MapSettings, MaterialFile, MaterialFileError,
    MaterialMaps, StandardMaps, StandardMaterial, is_material_file, material_json, parse_material,
    read_material_file, write_material_file,
};
pub use model::{
    AlphaMode, Blend, Bone, BoneTransform, Channel, ChannelTexture, Image, Joint, Keys, MapSlot,
    Model, ModelError, ModelFormat, ModelMaterial, ModelObject, Pick, Pose, Posed, PosedPrimitive,
    Primitive, Rig, Take, TextureRole, Track, list_objects, load_model, load_models, load_takes,
    texture_roles,
};
pub use reload::Reload;
pub use sky_image::{
    HDR_SKY_EXTENSIONS, MAX_SKY_WIDTH, SKY_IMAGE_EXTENSIONS, SkyImage, decode_sky_image,
    is_hdr_sky_file, read_sky_image,
};
pub use texture_set::{
    TEXTURE_SET_SUFFIX, TextureSet, TextureSetError, TextureSetMap, read_texture_set,
    texture_set_from_files, write_texture_set,
};

/// At most this many textures are read at once in the background, leaving
/// cores for drawing.
const TEXTURE_WORKERS: usize = 4;

/// The project's loaded assets. Cloning gives another handle to the same
/// assets, so a model is loaded once however many parts use it.
///
/// `Arc<Mutex<…>>` shares one value between owners (`Arc`) and lets any of
/// them change it, one at a time (`Mutex`). Background texture workers hold
/// the lock only to take a path and to store what they read, never while
/// reading.
#[derive(Clone, Default)]
pub struct Assets {
    shared: Arc<Mutex<Shared>>,
}

/// Called from a worker thread each time a background texture is ready.
type Waker = Arc<dyn Fn() + Send + Sync>;

/// Textures being read in the background, for a progress note.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextureProgress {
    /// How many of the textures asked for since none were loading are done.
    pub done: usize,
    /// How many were asked for since none were loading.
    pub total: usize,
    /// One being read now, such as `assets/textures/Ground108/…_Color.jpg`.
    pub current: String,
}

/// A file for a background worker to read: a texture, or a sky image.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Queued {
    path: String,
    sky: bool,
}

#[derive(Default)]
struct Shared {
    /// The project folder, or `None` when no project is open (then there
    /// are no models).
    folder: Option<PathBuf>,
    /// Every model asked for so far, by its `MeshId`. `None` means it
    /// couldn't be loaded, so it isn't tried again.
    models: HashMap<String, Option<Arc<Model>>>,
    /// Every texture file asked for so far, by its path, the same way.
    textures: HashMap<String, Option<Arc<Image>>>,
    /// Every sky image asked for so far, by its path, the same way.
    sky_images: HashMap<String, Option<Arc<SkyImage>>>,
    /// Every material file asked for so far, by its path, put together
    /// with its bases. `None` means it couldn't be read.
    materials: HashMap<String, Option<material::Resolved>>,
    /// The takes of every model file asked for so far, by its path.
    takes: HashMap<String, Option<Arc<Vec<Take>>>>,
    /// Every data table asked for so far, by its path, or why it can't
    /// be read.
    tables: HashMap<String, Result<Arc<DataTable>, String>>,
    /// Loading errors not yet collected by [`Assets::take_errors`].
    errors: Vec<String>,
    /// Set by [`Assets::load_textures_in_background`].
    waker: Option<Waker>,
    /// Texture and sky image files waiting for a worker, oldest first.
    queue: VecDeque<Queued>,
    /// Texture and sky image files queued or being read.
    loading: HashSet<Queued>,
    /// Worker threads running.
    workers: usize,
    /// The project's font files, read the first time they're asked for.
    fonts: Option<Arc<Vec<FontFile>>>,
    /// Textures asked for since none were loading, for [`TextureProgress`].
    batch: usize,
    /// Each model and texture file's stamp when it was read, so
    /// [`Assets::reload`] skips one that hasn't changed since (such as a
    /// model the editor just imported).
    stamps: HashMap<String, reload::Stamp>,
    /// Files whose last reload failed, so the next failure isn't reported
    /// again until one succeeds.
    reload_failed: HashSet<String>,
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
                let source_id = mesh_id.split('#').next().unwrap_or(mesh_id);
                let prefix = format!("Could not load the model `{source_id}`:");
                if !shared.errors.iter().any(|error| error.starts_with(&prefix)) {
                    shared
                        .errors
                        .push(format!("Could not load the model `{source_id}`: {message}"));
                }
                None
            }
        };
        let file = mesh_id.split('#').next().unwrap_or(mesh_id);
        reload::remember_stamp(&mut shared, &folder, file);
        shared.models.insert(mesh_id.to_owned(), model.clone());
        model
    }

    /// The texture file at `path` (a SurfaceAppearance's map, such as
    /// `assets/textures/wood.png`), decoded the first time. `None` if
    /// `path` is empty or it can't be read; the reason goes to
    /// [`take_errors`](Self::take_errors), once.
    ///
    /// After [`load_textures_in_background`](Self::load_textures_in_background),
    /// a texture not read yet is `None` until a worker thread has read it,
    /// instead of being read before this returns.
    pub fn texture(&self, path: &str) -> Option<Arc<Image>> {
        if path.is_empty() {
            return None;
        }
        let mut shared = self.lock();
        if let Some(texture) = shared.textures.get(path) {
            return texture.clone();
        }
        let folder = shared.folder.clone()?;
        if shared.waker.is_some() {
            self.queue_file(&mut shared, path, false);
            return None;
        }
        store_texture(&mut shared, path, read_texture(&folder, path))
    }

    /// The sky image at `path` (Lighting's `SkyImage`, such as
    /// `assets/sky/noon.exr`), decoded the first time. `None` if `path` is
    /// empty or it can't be read; the reason goes to
    /// [`take_errors`](Self::take_errors), once, as does a warning that it
    /// isn't a 2:1 panorama.
    ///
    /// After [`load_textures_in_background`](Self::load_textures_in_background),
    /// it's `None` until a worker thread has read it, like a texture, and
    /// [`texture_progress`](Self::texture_progress) counts it.
    pub fn sky_image(&self, path: &str) -> Option<Arc<SkyImage>> {
        if path.is_empty() {
            return None;
        }
        let mut shared = self.lock();
        if let Some(image) = shared.sky_images.get(path) {
            return image.clone();
        }
        let folder = shared.folder.clone()?;
        if shared.waker.is_some() {
            self.queue_file(&mut shared, path, true);
            return None;
        }
        store_sky_image(&mut shared, path, read_sky_image(&folder, path))
    }

    /// The material file at `path` (a part's `MaterialVariant`, such as
    /// `assets/materials/brick.material.json`), put together with its
    /// bases, the first time. `None` if `path` is empty or the file can't be
    /// read; the reason goes to [`take_errors`](Self::take_errors), once. A
    /// base that can't be read, or that leads around in a cycle, is
    /// reported once too, and the default material stands in for it.
    pub fn material(&self, path: &str) -> Option<Arc<StandardMaterial>> {
        if path.is_empty() {
            return None;
        }
        let mut shared = self.lock();
        if let Some(material) = shared.materials.get(path) {
            return material.as_ref().map(|resolved| resolved.material.clone());
        }
        let folder = shared.folder.clone()?;
        let resolved = material::resolve(&folder, path, &mut shared.errors);
        let material = resolved.as_ref().map(|resolved| resolved.material.clone());
        shared.materials.insert(path.to_owned(), resolved);
        material
    }

    /// The data table named `name` (such as `weapons`, the file
    /// `assets/data/weapons.data.json`), read the first time, or why it
    /// can't be. Scripts show that reason in their error, so it isn't
    /// also put in [`take_errors`](Self::take_errors).
    pub fn data_table(&self, name: &str) -> Result<Arc<DataTable>, String> {
        let path = data_table_path(name);
        let mut shared = self.lock();
        if let Some(table) = shared.tables.get(&path) {
            return table.clone();
        }
        let Some(folder) = shared.folder.clone() else {
            return Err("there is no project folder".to_owned());
        };
        let table = read_table(&folder, &path).map(Arc::new);
        reload::remember_stamp(&mut shared, &folder, &path);
        shared.tables.insert(path, table.clone());
        table
    }

    /// Uses `table` as the one at `path` from now on, without reading the
    /// file: the editor's Data Table tab hands over each edit, so a game
    /// in Play reads it at once.
    pub fn put_data_table(&self, path: &str, table: DataTable) {
        self.lock()
            .tables
            .insert(path.to_owned(), Ok(Arc::new(table)));
    }

    /// From now on, [`texture`](Self::texture) doesn't wait for a file it
    /// hasn't read: up to [`TEXTURE_WORKERS`] threads read them, and
    /// `ready` is called (on one of those threads) after each, so the host
    /// can draw again. For the editor, where a 4K texture set takes seconds
    /// to decode. The player reads its textures on its loading screen.
    pub fn load_textures_in_background(&self, ready: impl Fn() + Send + Sync + 'static) {
        self.lock().waker = Some(Arc::new(ready));
    }

    /// Whether [`load_textures_in_background`](Self::load_textures_in_background)
    /// was called.
    pub fn loads_textures_in_background(&self) -> bool {
        self.lock().waker.is_some()
    }

    /// The textures being read in the background, or `None` when none are.
    pub fn texture_progress(&self) -> Option<TextureProgress> {
        let shared = self.lock();
        let current = shared
            .loading
            .iter()
            .find(|queued| !shared.queue.contains(queued))
            .or_else(|| shared.queue.front())?
            .path
            .clone();
        Some(TextureProgress {
            done: shared.batch - shared.loading.len(),
            total: shared.batch,
            current,
        })
    }

    /// Queues `path` (a sky image if `sky`, otherwise a texture) for a
    /// background worker, starting one if fewer than [`TEXTURE_WORKERS`]
    /// are running.
    fn queue_file(&self, shared: &mut Shared, path: &str, sky: bool) {
        let queued = Queued {
            path: path.to_owned(),
            sky,
        };
        if !shared.loading.insert(queued.clone()) {
            return;
        }
        shared.queue.push_back(queued);
        shared.batch += 1;
        if shared.workers < TEXTURE_WORKERS {
            shared.workers += 1;
            let assets = self.clone();
            std::thread::spawn(move || assets.read_queued_textures());
        }
    }

    /// A worker: reads queued textures and sky images until there are
    /// none left.
    fn read_queued_textures(&self) {
        loop {
            let (queued, folder) = {
                let mut shared = self.lock();
                let Some(queued) = shared.queue.pop_front() else {
                    shared.workers -= 1;
                    return;
                };
                (queued, shared.folder.clone().unwrap_or_default())
            };
            let path = &queued.path;
            // A decoder that panics fails this file, not the worker, which
            // would leave the file "loading" forever.
            let failed = || "the decoder failed on it".to_owned();
            let waker = if queued.sky {
                let read = panic::catch_unwind(AssertUnwindSafe(|| read_sky_image(&folder, path)))
                    .unwrap_or_else(|_| Err(failed()));
                let mut shared = self.lock();
                store_sky_image(&mut shared, path, read);
                shared.loading.remove(&queued);
                if shared.loading.is_empty() {
                    shared.batch = 0;
                }
                shared.waker.clone()
            } else {
                let read = panic::catch_unwind(AssertUnwindSafe(|| read_texture(&folder, path)))
                    .unwrap_or_else(|_| Err(failed()));
                let mut shared = self.lock();
                store_texture(&mut shared, path, read);
                shared.loading.remove(&queued);
                if shared.loading.is_empty() {
                    shared.batch = 0;
                }
                shared.waker.clone()
            };
            if let Some(waker) = waker {
                waker();
            }
        }
    }

    /// The takes (animation timelines) in the model file at `path`, such
    /// as `assets/models/hero.fbx` (anything after a `#` is ignored), read
    /// the first time. `None` if it can't be read; the reason goes to
    /// [`take_errors`](Self::take_errors), once. A file with no animation
    /// has no takes.
    pub fn takes(&self, path: &str) -> Option<Arc<Vec<Take>>> {
        let path = path.split('#').next().unwrap_or(path);
        if path.is_empty() {
            return None;
        }
        let mut shared = self.lock();
        if let Some(takes) = shared.takes.get(path) {
            return takes.clone();
        }
        let folder = shared.folder.clone()?;
        let takes = match read_takes(&folder, path) {
            Ok(takes) => Some(Arc::new(takes)),
            Err(message) => {
                shared.errors.push(format!(
                    "Could not load the animation in `{path}`: {message}"
                ));
                None
            }
        };
        reload::remember_stamp(&mut shared, &folder, path);
        shared.takes.insert(path.to_owned(), takes.clone());
        takes
    }

    /// Reads a project asset by a relative path such as
    /// `assets/audio/chime.wav`. Paths that leave the project are refused.
    pub fn read_file(&self, path: &str) -> Result<Vec<u8>, String> {
        let relative = safe_path(path)
            .ok_or_else(|| "asset paths must name a file inside the project folder".to_owned())?;
        let folder = self
            .lock()
            .folder
            .clone()
            .ok_or_else(|| "there is no project folder".to_owned())?;
        std::fs::read(folder.join(relative)).map_err(|err| match err.kind() {
            std::io::ErrorKind::NotFound => format!("the file `{path}` isn't there"),
            _ => err.to_string(),
        })
    }

    /// Adds `model` as the one at `mesh_id`, for a model just imported, so
    /// it isn't read again.
    pub fn insert(&self, mesh_id: &str, model: Model) -> Arc<Model> {
        let model = Arc::new(model);
        let mut shared = self.lock();
        if let Some(folder) = shared.folder.clone() {
            let file = mesh_id.split('#').next().unwrap_or(mesh_id);
            reload::remember_stamp(&mut shared, &folder, file);
        }
        shared
            .models
            .insert(mesh_id.to_owned(), Some(model.clone()));
        model
    }

    /// Forgets `path` (a model file, with or without a `#` pick, a
    /// texture, or a material file, and every material built on it), so
    /// it's read again the next time it's asked for, after the file was
    /// changed. The errors it gave are reported again too.
    pub fn forget(&self, path: &str) {
        let file = path.split('#').next().unwrap_or(path);
        let mut shared = self.lock();
        shared
            .models
            .retain(|mesh_id, _| mesh_id.split('#').next() != Some(file));
        shared.textures.remove(file);
        shared.takes.remove(file);
        shared.tables.remove(file);
        shared.materials.retain(|_, material| {
            material
                .as_ref()
                .is_some_and(|resolved| !resolved.files.iter().any(|used| used == file))
        });
        // One that couldn't be read has no files to look at.
        shared.materials.remove(file);
    }

    /// Forgets every model, texture, take and material that couldn't be
    /// loaded (or whose base couldn't), so each is tried again, after files
    /// were added or fixed.
    pub fn forget_failures(&self) {
        let mut shared = self.lock();
        shared.models.retain(|_, model| model.is_some());
        shared.textures.retain(|_, texture| texture.is_some());
        shared.takes.retain(|_, takes| takes.is_some());
        shared.tables.retain(|_, table| table.is_ok());
        shared
            .materials
            .retain(|_, material| material.as_ref().is_some_and(|resolved| !resolved.broken));
    }

    /// Removes and returns the loading errors since the last call, one line
    /// each, for the Output panel or the terminal.
    pub fn take_errors(&self) -> Vec<String> {
        std::mem::take(&mut self.lock().errors)
    }

    /// The project's font files (`.ttf` and `.otf` anywhere in it), read
    /// the first time they're asked for. Empty with no project.
    pub fn fonts(&self) -> Arc<Vec<FontFile>> {
        let mut shared = self.lock();
        if let Some(fonts) = &shared.fonts {
            return fonts.clone();
        }
        let fonts = Arc::new(
            shared
                .folder
                .as_deref()
                .map(fonts::read_fonts)
                .unwrap_or_default(),
        );
        shared.fonts = Some(fonts.clone());
        fonts
    }

    /// Reads the project's fonts again next time, after one was added,
    /// changed or removed.
    pub fn forget_fonts(&self) {
        self.lock().fonts = None;
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

/// Reads the takes of the model file at `path` inside `folder`, or
/// explains why it can't.
fn read_takes(folder: &Path, path: &str) -> Result<Vec<Take>, String> {
    let relative = safe_path(path)
        .ok_or("an animation must come from a model file inside the project folder")?;
    let format = ModelFormat::of(&relative).ok_or("animations come from .fbx files")?;
    let bytes = std::fs::read(folder.join(relative)).map_err(|err| match err.kind() {
        std::io::ErrorKind::NotFound => "the file isn't there".to_owned(),
        _ => err.to_string(),
    })?;
    load_takes(&bytes, format).map_err(|err| with_causes(&err))
}

/// Reads the data table at `path` inside `folder`, or explains why it
/// can't.
fn read_table(folder: &Path, path: &str) -> Result<DataTable, String> {
    let relative = safe_path(path).ok_or("a data table's name must stay inside `assets/data`")?;
    let text = std::fs::read_to_string(folder.join(relative)).map_err(|err| match err.kind() {
        std::io::ErrorKind::NotFound => format!("there's no data table file `{path}`"),
        _ => format!("could not read `{path}`: {err}"),
    })?;
    DataTable::from_json(&text)
        .map_err(|err| format!("`{path}` can't be read: {}", with_causes(&err)))
}

/// Keeps what reading the texture at `path` gave, reporting a failure.
/// Keeps what reading the sky image at `path` gave, reporting a failure or
/// a picture that isn't a panorama once.
fn store_sky_image(
    shared: &mut Shared,
    path: &str,
    read: Result<SkyImage, String>,
) -> Option<Arc<SkyImage>> {
    let image = match read {
        Ok(image) => {
            if let Some(warning) = image.shape_warning() {
                shared
                    .errors
                    .push(format!("The sky image `{path}` {warning}."));
            }
            Some(Arc::new(image))
        }
        Err(message) => {
            shared
                .errors
                .push(format!("Could not load the sky image `{path}`: {message}"));
            None
        }
    };
    if let Some(folder) = shared.folder.clone() {
        reload::remember_stamp(shared, &folder, path);
    }
    shared.sky_images.insert(path.to_owned(), image.clone());
    image
}

fn store_texture(
    shared: &mut Shared,
    path: &str,
    read: Result<Image, String>,
) -> Option<Arc<Image>> {
    let texture = match read {
        Ok(image) => Some(Arc::new(image)),
        Err(message) => {
            shared
                .errors
                .push(format!("Could not load the texture `{path}`: {message}"));
            None
        }
    };
    if let Some(folder) = shared.folder.clone() {
        reload::remember_stamp(shared, &folder, path);
    }
    shared.textures.insert(path.to_owned(), texture.clone());
    texture
}

/// Reads the texture at `path` inside `folder`, or explains why it can't.
/// Not cached: [`Assets::texture`] is the cached way.
pub fn read_texture(folder: &Path, path: &str) -> Result<Image, String> {
    let relative = safe_path(path).ok_or(
        "a texture must be a file inside the project folder, such as \"assets/textures/wood.png\"",
    )?;
    let bytes = std::fs::read(folder.join(&relative)).map_err(|err| match err.kind() {
        std::io::ErrorKind::NotFound => "the file isn't there".to_owned(),
        _ => err.to_string(),
    })?;
    let mut image = model::decode_image(&bytes, path)?;
    image.file = Some(path.to_owned());
    Ok(image)
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
    fn project_files_can_be_read_without_leaving_the_asset_folder() {
        let folder =
            std::env::temp_dir().join(format!("axcenity-audio-asset-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(folder.join("assets/audio")).unwrap();
        std::fs::write(folder.join("assets/audio/tone.wav"), b"audio data").unwrap();

        let assets = Assets::new(&folder);
        assert_eq!(
            assets.read_file("assets/audio/tone.wav").unwrap(),
            b"audio data"
        );
        assert!(
            assets
                .read_file("../outside.wav")
                .unwrap_err()
                .contains("inside the project folder")
        );
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn only_model_files_are_loaded() {
        let assets = Assets::new(std::env::temp_dir());
        assert!(assets.model("assets/notes.txt").is_none());
        assert!(assets.take_errors()[0].contains(".gltf, .glb or .fbx"));
    }

    #[test]
    fn textures_load_once_and_failures_are_reported_once() {
        let folder = std::env::temp_dir().join("axcenity-assets-textures");
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(folder.join("assets/textures")).unwrap();
        std::fs::write(
            folder.join("assets/textures/checker.png"),
            model::checker_png(),
        )
        .unwrap();
        let assets = Assets::new(&folder);
        let first = assets.texture("assets/textures/checker.png").unwrap();
        let again = assets.texture("assets/textures/checker.png").unwrap();
        assert!(Arc::ptr_eq(&first, &again));
        assert!(assets.texture("").is_none());
        assert!(assets.texture("assets/textures/gone.png").is_none());
        assert!(assets.texture("assets/textures/gone.png").is_none());
        assert!(assets.texture("../outside.png").is_none());
        let errors = assets.take_errors();
        assert_eq!(errors.len(), 2, "{errors:?}");
        assert!(errors[0].contains("`assets/textures/gone.png`: the file isn't there"));
        assert!(errors[1].contains("inside the project folder"));
    }

    #[test]
    fn background_textures_arrive_later_with_progress_and_a_wake_up() {
        let folder = std::env::temp_dir().join("axcenity-assets-background");
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(folder.join("assets/textures")).unwrap();
        std::fs::write(
            folder.join("assets/textures/checker.png"),
            model::checker_png(),
        )
        .unwrap();
        let assets = Assets::new(&folder);
        let (woken, wakes) = std::sync::mpsc::channel();
        assets.load_textures_in_background(move || {
            let _ = woken.send(());
        });
        assert!(assets.loads_textures_in_background());
        assert!(assets.texture_progress().is_none());

        // Asked for, not read yet: nothing to draw, but it's loading.
        assert!(assets.texture("assets/textures/checker.png").is_none());
        assert!(assets.texture("assets/textures/gone.png").is_none());
        let progress = assets.texture_progress();
        assert!(
            progress.as_ref().is_none_or(|progress| progress.total == 2),
            "{progress:?}"
        );
        for _ in 0..2 {
            wakes
                .recv_timeout(std::time::Duration::from_secs(10))
                .expect("each texture wakes the host");
        }
        assert!(assets.texture_progress().is_none());
        assert!(assets.texture("assets/textures/checker.png").is_some());
        assert!(assets.texture("assets/textures/gone.png").is_none());
        let errors = assets.take_errors();
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(errors[0].contains("gone.png"), "{errors:?}");
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn without_a_project_there_are_no_models_and_no_errors() {
        let assets = Assets::default();
        assert!(assets.model("assets/models/chair.glb").is_none());
        assert!(assets.take_errors().is_empty());
    }
}
