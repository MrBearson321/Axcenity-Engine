//! Axcenity assets: the files a project uses besides its world and scripts.
//!
//! - [`Assets`] loads each model a MeshPart names the first time
//!   something asks for it, and keeps it. The player and the editor make one
//!   per project and share it (it's a cheap handle) with `physics` and `scene`.
//!   Files are read and decoded with its lock released, and
//!   [`Assets::request_model`] and its siblings load them on background
//!   threads without waiting ([`LoadTicket`]).
//! - [`Assets::material`] reads a `*.material.json` file a part's
//!   `MaterialVariant` names, with its base files ([`MaterialFile`]).
//! - [`Assets::data_table`] reads a `*.data.json` table scripts read
//!   through `DataService` ([`DataTable`]).
//! - [`Assets::sky_image`] reads the sky image Lighting's `SkyImage` names
//!   ([`SkyImage`]).
//! - [`Assets::reflection_probe`] reads the bake a `ReflectionProbe`'s
//!   `Bake` names ([`ProbeImage`]).
//! - [`Assets::light_probes`] reads the bake a `LightProbeVolume`'s `Bake`
//!   names ([`LightProbes`]).
//! - [`Assets::color_lookup`] and [`Assets::palette`] read the lookup table
//!   and palette a `ColorCorrectionEffect` names ([`ColorLookup`],
//!   [`Palette`]).
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

mod asset_id;
mod color_grading;
mod cooked;
mod data_table;
mod fonts;
mod import;
mod light_probes;
mod loading;
mod material;
mod model;
mod orphaned;
mod probe;
mod references;
mod reload;
mod sky_image;
mod texture_set;

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use loading::{Kind, LoadKey};

pub use asset_id::{
    AssetId, IMPORT_SUFFIX, KNOWN_IDS_FILE, asset_ids, encode_import_file, give_asset_id,
    give_asset_ids, id_files, import_path, is_import_file, moved_assets, read_asset_id,
    takes_asset_id,
};
pub use color_grading::{
    ColorLookup, LOOKUP_EXTENSIONS, MAX_LOOKUP_SIZE, MAX_PALETTE_COLORS, PROJECT_GRADING_FOLDER,
    Palette, lookup_from_strip, palette_from_image, parse_cube, read_color_lookup, read_palette,
};
pub use cooked::{
    COOKED_EXTENSION, COOKED_FOLDER, CookedFormat, CookedTexture, SourceStamp, content_hash,
    cooked_format, cooked_path, cooked_relative_path, decode_cooked, embedded_cooked_done,
    embedded_cooked_files, embedded_cooked_format, embedded_cooked_path,
    embedded_cooked_relative_path, encode_cooked, has_alpha, is_cooked, mark_embedded_cooked,
    read_cooked, sampled_average, write_cooked, write_embedded_cooked,
};
pub use data_table::{
    AssetType, Cell, Column, ColumnType, DATA_FOLDER, DATA_TABLE_SUFFIX, DataTable, DataTableError,
    Row, data_table_name, data_table_path, is_data_table, read_data_table, write_data_table,
};
pub use fonts::{FontFile, is_font_file};
pub use import::{
    AUDIO_EXTENSIONS, ImportError, MESH_EDIT_FOLDER_SUFFIX, MESH_EDIT_SUFFIX, MODELS_FOLDER,
    PROJECT_AUDIO_FOLDER, PROJECT_SKY_FOLDER, PROJECT_TEXTURES_FOLDER, TEXTURE_EXTENSIONS,
    TEXTURES_FOLDER, import_animation_file, import_audio, import_grading_file, import_model,
    import_picks, import_sky_image, import_texture, list_model_objects, load_model_file,
    load_takes_file, safe_file_name, unique_file_name,
};
pub use light_probes::{
    LIGHT_PROBES_SUFFIX, LightProbes, MAX_LIGHT_PROBES_PER_SIDE, ProbeLight, decode_light_probes,
    encode_light_probes, is_light_probes_file, light_probes_path, read_light_probes,
    write_light_probes,
};
pub use loading::{LoadStatus, LoadTicket, Slot};
pub use material::{
    MATERIAL_SUFFIX, MATERIALS_FOLDER, MapFile, MapSettings, MaterialChain, MaterialFile,
    MaterialFileError, MaterialMaps, MaterialShader, OutlineFile, ParamValue, Shading,
    StandardMaps, StandardMaterial, StandardOutline, StandardToon, StandardWater, ToonFile,
    WaterFile, is_material_file, material_json, parse_material, read_material_file,
    write_material_file,
};
pub use model::{
    AlphaMode, Blend, Bone, BoneTransform, Channel, ChannelTexture, Image, Joint, Keys,
    MESH_CACHE_FOLDER, MODEL_CACHE_FOLDER, MapSlot, Model, ModelError, ModelFormat, ModelMaterial,
    ModelObject, Pick, Pose, Posed, PosedPrimitive, Primitive, Rig, Take, TextureRole, Track,
    cached_meshes, cooked_models, embedded_files, list_objects, load_model, load_models,
    load_takes, lod_level, tangents, texture_roles,
};
pub use orphaned::remove_orphaned_copies;
pub use probe::{
    PROBE_SUFFIX, PROBES_FOLDER, ProbeImage, decode_probe, encode_probe, is_probe_file, probe_path,
    read_probe, write_probe,
};
pub use references::{
    ASSET_INDEX_FILE, AssetPath, encode_asset_index, is_asset_reference, read_asset_index,
    split_reference,
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

/// The project's loaded assets. Cloning gives another handle to the same
/// assets, so a model is loaded once however many parts use it.
///
/// `Arc<Mutex<…>>` shares one value between owners (`Arc`) and lets any of
/// them change it, one at a time (`Mutex`). The lock is held only to look a
/// file up and to keep what was read, never while reading (`loading.rs`).
#[derive(Clone, Default)]
pub struct Assets {
    shared: Arc<Mutex<Shared>>,
}

/// Called from a worker thread each time a background file is ready.
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
    /// Every reflection probe bake asked for so far, by its path, the same
    /// way.
    probes: HashMap<String, Option<Arc<ProbeImage>>>,
    /// Every light probe bake asked for so far, by its path, the same way.
    light_probes: HashMap<String, Option<Arc<LightProbes>>>,
    /// Every color lookup table asked for so far, by its path, the same way.
    lookups: HashMap<String, Option<Arc<ColorLookup>>>,
    /// Every palette asked for so far, by its path, the same way.
    palettes: HashMap<String, Option<Arc<Palette>>>,
    /// The image parts with `BlueprintTexture` on are drawn in, while the
    /// editor is editing (see [`Assets::set_blueprint`]).
    blueprint: Option<Arc<Image>>,
    /// Every material file asked for so far, by its path, put together
    /// with its bases. `None` means it couldn't be read.
    materials: HashMap<String, Option<material::Resolved>>,
    /// Material files as the editor's Material tab has them, used in place
    /// of the files on disk (see [`Assets::put_material_file`]).
    edited_materials: HashMap<String, MaterialFile>,
    /// The takes of every model file asked for so far, by its path.
    takes: HashMap<String, Option<Arc<Vec<Take>>>>,
    /// Every data table asked for so far, by its path, or why it can't
    /// be read.
    tables: HashMap<String, Result<Arc<DataTable>, String>>,
    /// Loading errors not yet collected by [`Assets::take_errors`].
    errors: Vec<String>,
    /// Set by [`Assets::load_textures_in_background`].
    waker: Option<Waker>,
    /// Files waiting for a worker, oldest first.
    queue: VecDeque<LoadKey>,
    /// Every file queued or being read, each with its ticket.
    loads: HashMap<LoadKey, loading::Load>,
    /// Worker threads running.
    workers: usize,
    /// Everything queued since none were loading, for [`Assets::progress`].
    jobs: loading::Batch,
    /// Textures and sky images among those, for [`TextureProgress`].
    images: loading::Batch,
    /// Goes up when anything is loaded (see [`Assets::generation`]).
    generation: u64,
    /// Set by [`Assets::set_load_gate`].
    gate: Option<loading::LoadGate>,
    /// The project's font files, read the first time they're asked for.
    fonts: Option<Arc<Vec<FontFile>>>,
    /// Each model and texture file's stamp when it was read, so
    /// [`Assets::reload`] skips one that hasn't changed since (such as a
    /// model the editor just imported).
    stamps: HashMap<String, reload::Stamp>,
    /// Files whose last reload failed, so the next failure isn't reported
    /// again until one succeeds.
    reload_failed: HashSet<String>,
    /// Which file each asset ID names (see [`Assets::resolve`]).
    ids: references::IdCache,
}

/// Forgets every material made from the material file `file`, so each is
/// put together again the next time it's asked for.
fn forget_materials_using(shared: &mut Shared, file: &str) {
    shared.materials.retain(|_, material| {
        material
            .as_ref()
            .is_some_and(|resolved| !resolved.files.iter().any(|used| used == file))
    });
    // One that couldn't be read has no files to look at.
    shared.materials.remove(file);
}

impl Assets {
    /// The assets of the project in `folder`.
    pub fn new(folder: impl Into<PathBuf>) -> Self {
        let assets = Self::default();
        assets.lock().folder = Some(folder.into());
        assets
    }

    /// The project folder, or `None` when no project is open.
    pub fn folder(&self) -> Option<PathBuf> {
        self.lock().folder.clone()
    }

    /// Sets the image `BlueprintTexture` parts are drawn in, or `None` to
    /// draw them as they really are. Only the editor sets one, and only
    /// while editing, so the image is never part of a project or a game.
    pub fn set_blueprint(&self, image: Option<Arc<Image>>) {
        let mut shared = self.lock();
        // The editor calls this every frame, so only a real change counts.
        let same = match (&shared.blueprint, &image) {
            (Some(old), Some(new)) => Arc::ptr_eq(old, new),
            (None, None) => true,
            _ => false,
        };
        if !same {
            shared.blueprint = image;
            shared.generation += 1;
        }
    }

    /// Whether `other` is this same set of assets (a clone of it), rather
    /// than another's that happens to hold the same files.
    pub fn same_as(&self, other: &Assets) -> bool {
        Arc::ptr_eq(&self.shared, &other.shared)
    }

    /// The image set by [`set_blueprint`](Self::set_blueprint).
    pub fn blueprint(&self) -> Option<Arc<Image>> {
        self.lock().blueprint.clone()
    }

    /// The model at `mesh_id` (a MeshPart's `MeshId`, such as
    /// `assets/models/chair.glb`), loading it the first time and waiting
    /// for that. `None` if `mesh_id` is empty or the model can't be loaded;
    /// the reason goes to [`take_errors`](Self::take_errors), once, as do
    /// the model's [warnings](Model::warnings). To load without waiting,
    /// see [`request_model`](Self::request_model).
    pub fn model_blocking(&self, mesh_id: &str) -> Option<Arc<Model>> {
        let resolved = self.resolve(mesh_id);
        let mesh_id: &str = &resolved;
        if mesh_id.is_empty() {
            return None;
        }
        self.load(
            Kind::Model,
            mesh_id,
            None,
            |shared| shared.models.get(mesh_id).cloned(),
            |folder| read_model(folder, mesh_id),
            |shared, read| store_model(shared, mesh_id, read),
        )
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
        let resolved = self.resolve(path);
        let path: &str = &resolved;
        if path.is_empty() {
            return None;
        }
        {
            let mut shared = self.lock();
            if let Some(texture) = shared.textures.get(path) {
                return texture.clone();
            }
            shared.folder.as_ref()?;
            if shared.waker.is_some() {
                self.enqueue(&mut shared, LoadKey::new(Kind::Texture, path));
                return None;
            }
        }
        self.texture_blocking(path)
    }

    /// The texture file at `path`, as [`texture`](Self::texture) gives it,
    /// but waiting for it even when textures load in the background.
    pub fn texture_blocking(&self, path: &str) -> Option<Arc<Image>> {
        let resolved = self.resolve(path);
        let path: &str = &resolved;
        if path.is_empty() {
            return None;
        }
        self.load(
            Kind::Texture,
            path,
            None,
            |shared| shared.textures.get(path).cloned(),
            |folder| read_texture(folder, path),
            |shared, read| store_texture(shared, path, read),
        )
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
        let resolved = self.resolve(path);
        let path: &str = &resolved;
        if path.is_empty() {
            return None;
        }
        {
            let mut shared = self.lock();
            if let Some(image) = shared.sky_images.get(path) {
                return image.clone();
            }
            shared.folder.as_ref()?;
            if shared.waker.is_some() {
                self.enqueue(&mut shared, LoadKey::new(Kind::SkyImage, path));
                return None;
            }
        }
        self.sky_image_blocking(path)
    }

    /// The sky image at `path`, as [`sky_image`](Self::sky_image) gives it,
    /// but waiting for it even when textures load in the background.
    pub fn sky_image_blocking(&self, path: &str) -> Option<Arc<SkyImage>> {
        let resolved = self.resolve(path);
        let path: &str = &resolved;
        if path.is_empty() {
            return None;
        }
        self.load(
            Kind::SkyImage,
            path,
            None,
            |shared| shared.sky_images.get(path).cloned(),
            |folder| read_sky_image(folder, path),
            |shared, read| store_sky_image(shared, path, read),
        )
    }

    /// The reflection probe bake at `path` (a ReflectionProbe's `Bake`,
    /// such as `assets/probes/Hall.probe.hdr`), read the first time. `None`
    /// if `path` is empty or it can't be read; the reason goes to
    /// [`take_errors`](Self::take_errors), once. Bakes are small, so they're
    /// read at once even while textures load in the background.
    pub fn reflection_probe(&self, path: &str) -> Option<Arc<ProbeImage>> {
        let resolved = self.resolve(path);
        let path: &str = &resolved;
        if path.is_empty() {
            return None;
        }
        self.load(
            Kind::ReflectionProbe,
            path,
            None,
            |shared| shared.probes.get(path).cloned(),
            |folder| read_probe(folder, path),
            |shared, read| {
                let image = match read {
                    Ok(image) => Some(Arc::new(image)),
                    Err(message) => {
                        shared.errors.push(format!(
                            "Could not load the reflection probe bake `{path}`: {message}. \
                             The sky is reflected instead; bake the probe again."
                        ));
                        None
                    }
                };
                shared.probes.insert(path.to_owned(), image.clone());
                image
            },
        )
    }

    /// Uses `image` as the bake at `path` from now on, without reading the
    /// file: the editor hands over each bake it saves.
    pub fn put_reflection_probe(&self, path: &str, image: ProbeImage) {
        let mut shared = self.lock();
        if let Some(folder) = shared.folder.clone() {
            reload::remember_stamp(&mut shared, &folder, path);
        }
        loading::replace_load(&mut shared, &LoadKey::new(Kind::ReflectionProbe, path));
        shared.probes.insert(path.to_owned(), Some(Arc::new(image)));
    }

    /// The light probe bake at `path` (a LightProbeVolume's `Bake`, such as
    /// `assets/probes/Hall.lightprobes.json`), read the first time. `None`
    /// if `path` is empty or it can't be read; the reason goes to
    /// [`take_errors`](Self::take_errors), once.
    pub fn light_probes(&self, path: &str) -> Option<Arc<LightProbes>> {
        let resolved = self.resolve(path);
        let path: &str = &resolved;
        if path.is_empty() {
            return None;
        }
        self.load(
            Kind::LightProbes,
            path,
            None,
            |shared| shared.light_probes.get(path).cloned(),
            |folder| read_light_probes(folder, path),
            |shared, read| {
                let probes = match read {
                    Ok(probes) => Some(Arc::new(probes)),
                    Err(message) => {
                        shared.errors.push(format!(
                            "Could not load the light probe bake `{path}`: {message}. \
                             The sky lights its volume instead; bake the probes again."
                        ));
                        None
                    }
                };
                shared.light_probes.insert(path.to_owned(), probes.clone());
                probes
            },
        )
    }

    /// Uses `probes` as the bake at `path` from now on, without reading the
    /// file: the editor hands over each bake it saves.
    pub fn put_light_probes(&self, path: &str, probes: LightProbes) {
        let mut shared = self.lock();
        if let Some(folder) = shared.folder.clone() {
            reload::remember_stamp(&mut shared, &folder, path);
        }
        loading::replace_load(&mut shared, &LoadKey::new(Kind::LightProbes, path));
        shared
            .light_probes
            .insert(path.to_owned(), Some(Arc::new(probes)));
    }

    /// The color lookup table at `path` (a ColorCorrectionEffect's
    /// `LookupTable`: a `.cube` file or a strip image, such as
    /// `assets/grading/warm.cube`), read the first time. `None` if `path`
    /// is empty or it can't be read; the reason goes to
    /// [`take_errors`](Self::take_errors), once. Tables are small, so
    /// they're read at once even while textures load in the background.
    pub fn color_lookup(&self, path: &str) -> Option<Arc<ColorLookup>> {
        let resolved = self.resolve(path);
        let path: &str = &resolved;
        if path.is_empty() {
            return None;
        }
        self.load(
            Kind::ColorLookup,
            path,
            None,
            |shared| shared.lookups.get(path).cloned(),
            |folder| read_color_lookup(folder, path),
            |shared, read| {
                let lookup = match read {
                    Ok(lookup) => Some(Arc::new(lookup)),
                    Err(message) => {
                        shared.errors.push(format!(
                            "Could not load the lookup table `{path}`: {message}. Colors are \
                             left as they are."
                        ));
                        None
                    }
                };
                shared.lookups.insert(path.to_owned(), lookup.clone());
                lookup
            },
        )
    }

    /// The palette image at `path` (a ColorCorrectionEffect's `Palette`,
    /// such as `assets/grading/retro.png`), read the first time, as
    /// [`color_lookup`](Self::color_lookup) reads a table. A palette with
    /// too many colors is reported once too.
    pub fn palette(&self, path: &str) -> Option<Arc<Palette>> {
        let resolved = self.resolve(path);
        let path: &str = &resolved;
        if path.is_empty() {
            return None;
        }
        self.load(
            Kind::Palette,
            path,
            None,
            |shared| shared.palettes.get(path).cloned(),
            |folder| read_palette(folder, path),
            |shared, read| {
                let palette = match read {
                    Ok((palette, warning)) => {
                        if let Some(warning) = warning {
                            shared
                                .errors
                                .push(format!("The palette `{path}` {warning}."));
                        }
                        Some(Arc::new(palette))
                    }
                    Err(message) => {
                        shared.errors.push(format!(
                            "Could not load the palette `{path}`: {message}. Colors are left \
                             as they are."
                        ));
                        None
                    }
                };
                shared.palettes.insert(path.to_owned(), palette.clone());
                palette
            },
        )
    }

    /// The material file at `path` (a part's `MaterialVariant`, such as
    /// `assets/materials/brick.material.json`), put together with its
    /// bases, the first time. `None` if `path` is empty or the file can't be
    /// read; the reason goes to [`take_errors`](Self::take_errors), once. A
    /// base that can't be read, or that leads around in a cycle, is
    /// reported once too, and the default material stands in for it.
    pub fn material(&self, path: &str) -> Option<Arc<StandardMaterial>> {
        let resolved = self.resolve(path);
        let path: &str = &resolved;
        if path.is_empty() {
            return None;
        }
        self.load(
            Kind::Material,
            path,
            None,
            |shared| {
                shared
                    .materials
                    .get(path)
                    .map(|material| material.as_ref().map(|resolved| resolved.material.clone()))
            },
            |folder| {
                let edited = self.lock().edited_materials.clone();
                let mut errors = Vec::new();
                let resolved = material::resolve(folder, path, &edited, &mut errors);
                Ok((resolved, errors))
            },
            |shared, read: Result<(Option<material::Resolved>, Vec<String>), String>| {
                let resolved = match read {
                    Ok((resolved, errors)) => {
                        shared.errors.extend(errors);
                        resolved
                    }
                    Err(message) => {
                        shared
                            .errors
                            .push(format!("Could not load the material `{path}`: {message}"));
                        None
                    }
                };
                let material = resolved.as_ref().map(|resolved| resolved.material.clone());
                shared.materials.insert(path.to_owned(), resolved);
                material
            },
        )
    }

    /// The material file at `path` with its chain of bases, put together
    /// as [`material`](Self::material) does, or `None` if it can't be read.
    pub fn material_chain(&self, path: &str) -> Option<MaterialChain> {
        let resolved = self.resolve(path);
        let path: &str = &resolved;
        self.material(path)?;
        let shared = self.lock();
        let resolved = shared.materials.get(path)?.as_ref()?;
        Some(MaterialChain {
            files: resolved.files.clone(),
            merged: resolved.merged.clone(),
            broken: resolved.broken,
        })
    }

    /// Uses `file` as the material file at `path` from now on, without
    /// reading it (`None` reads the file again): the editor's Material tab
    /// hands over each edit, so every part using it, or a material built
    /// on it, is drawn with the edit at once.
    pub fn put_material_file(&self, path: &str, file: Option<MaterialFile>) {
        let mut shared = self.lock();
        match file {
            Some(file) => shared.edited_materials.insert(path.to_owned(), file),
            None => shared.edited_materials.remove(path),
        };
        forget_materials_using(&mut shared, path);
        loading::reread_loads(&mut shared, |key| key.kind == Kind::Material);
        shared.generation += 1;
    }

    /// The data table named `name` (such as `weapons`, the file
    /// `assets/data/weapons.data.json`), read the first time, or why it
    /// can't be. Scripts show that reason in their error, so it isn't
    /// also put in [`take_errors`](Self::take_errors).
    pub fn data_table(&self, name: &str) -> Result<Arc<DataTable>, String> {
        self.table_at(data_table_path(name))
    }

    /// The data table in the file at `path`, as
    /// [`data_table`](Self::data_table) reads it.
    fn table_at(&self, path: String) -> Result<Arc<DataTable>, String> {
        self.load(
            Kind::DataTable,
            &path,
            Err("there is no project folder".to_owned()),
            |shared| shared.tables.get(&path).cloned(),
            |folder| read_table(folder, &path).map(Arc::new),
            |shared, read| {
                shared.tables.insert(path.clone(), read.clone());
                read
            },
        )
    }

    /// Uses `table` as the one at `path` from now on, without reading the
    /// file: the editor's Data Table tab hands over each edit, so a game
    /// in Play reads it at once.
    pub fn put_data_table(&self, path: &str, table: DataTable) {
        let mut shared = self.lock();
        loading::replace_load(&mut shared, &LoadKey::new(Kind::DataTable, path));
        shared.tables.insert(path.to_owned(), Ok(Arc::new(table)));
    }

    /// From now on, [`texture`](Self::texture) doesn't wait for a file it
    /// hasn't read: worker threads read them, and
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
        let (key, _) = shared
            .loads
            .iter()
            .filter(|(key, load)| load.background && key.kind.is_image())
            // One being read now, if any is; otherwise one waiting.
            .min_by_key(|(_, load)| load.queued)?;
        Some(TextureProgress {
            done: shared.images.done(),
            total: shared.images.total(),
            current: key.file().to_owned(),
        })
    }

    /// The takes (animation timelines) in the model file at `path`, such
    /// as `assets/models/hero.fbx` (anything after a `#` is ignored), read
    /// the first time. `None` if it can't be read; the reason goes to
    /// [`take_errors`](Self::take_errors), once. A file with no animation
    /// has no takes.
    pub fn takes(&self, path: &str) -> Option<Arc<Vec<Take>>> {
        let resolved = self.resolve(path);
        let path: &str = &resolved;
        let path = path.split('#').next().unwrap_or(path);
        if path.is_empty() {
            return None;
        }
        self.load(
            Kind::Takes,
            path,
            None,
            |shared| shared.takes.get(path).cloned(),
            |folder| read_takes(folder, path),
            |shared, read| {
                let takes = match read {
                    Ok(takes) => Some(Arc::new(takes)),
                    Err(message) => {
                        shared.errors.push(format!(
                            "Could not load the animation in `{path}`: {message}"
                        ));
                        None
                    }
                };
                shared.takes.insert(path.to_owned(), takes.clone());
                takes
            },
        )
    }

    /// Reads a project asset by a relative path such as
    /// `assets/audio/chime.wav`. Paths that leave the project are refused.
    pub fn read_file(&self, path: &str) -> Result<Vec<u8>, String> {
        let resolved = self.resolve(path);
        let path: &str = &resolved;
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
        loading::replace_load(&mut shared, &LoadKey::new(Kind::Model, mesh_id));
        shared
            .models
            .insert(mesh_id.to_owned(), Some(model.clone()));
        model
    }

    /// Forgets `path` (a model file, with or without a `#` pick, a
    /// texture, a probe bake, or a material file, and every material built on it), so
    /// it's read again the next time it's asked for, after the file was
    /// changed. The errors it gave are reported again too.
    pub fn forget(&self, path: &str) {
        let file = path.split('#').next().unwrap_or(path);
        let mut shared = self.lock();
        shared
            .models
            .retain(|mesh_id, _| mesh_id.split('#').next() != Some(file));
        shared.textures.remove(file);
        shared.probes.remove(file);
        shared.light_probes.remove(file);
        shared.lookups.remove(file);
        shared.palettes.remove(file);
        shared.takes.remove(file);
        shared.tables.remove(file);
        forget_materials_using(&mut shared, file);
        loading::reread_loads(&mut shared, |key| {
            key.file() == file || key.kind == Kind::Material
        });
        shared.generation += 1;
    }

    /// Forgets every model, texture, probe bake, take and material that couldn't be
    /// loaded (or whose base couldn't), so each is tried again, after files
    /// were added or fixed.
    pub fn forget_failures(&self) {
        let mut shared = self.lock();
        shared.models.retain(|_, model| model.is_some());
        shared.textures.retain(|_, texture| texture.is_some());
        shared.probes.retain(|_, probe| probe.is_some());
        shared.light_probes.retain(|_, probes| probes.is_some());
        shared.lookups.retain(|_, lookup| lookup.is_some());
        shared.palettes.retain(|_, palette| palette.is_some());
        shared.takes.retain(|_, takes| takes.is_some());
        shared.tables.retain(|_, table| table.is_ok());
        shared
            .materials
            .retain(|_, material| material.as_ref().is_some_and(|resolved| !resolved.broken));
        loading::reread_loads(&mut shared, |_| true);
        shared.generation += 1;
    }

    /// Removes and returns the loading errors since the last call, one line
    /// each, for the Output panel or the terminal.
    pub fn take_errors(&self) -> Vec<String> {
        std::mem::take(&mut self.lock().errors)
    }

    /// The project's font files (`.ttf` and `.otf` anywhere in it), read
    /// the first time they're asked for. Empty with no project.
    pub fn fonts(&self) -> Arc<Vec<FontFile>> {
        self.load(
            Kind::Fonts,
            "",
            None,
            |shared| shared.fonts.clone().map(Some),
            |folder| Ok(Arc::new(fonts::read_fonts(folder))),
            |shared, read| {
                let fonts = read.unwrap_or_default();
                shared.fonts = Some(fonts.clone());
                Some(fonts)
            },
        )
        .unwrap_or_default()
    }

    /// Reads the project's fonts again next time, after one was added,
    /// changed or removed.
    pub fn forget_fonts(&self) {
        let mut shared = self.lock();
        shared.fonts = None;
        loading::reread_loads(&mut shared, |key| key.kind == Kind::Fonts);
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
    let relative_text = relative.to_string_lossy().replace('\\', "/");
    let objects = mesh_id.split_once('#').map_or("", |(_, objects)| objects);
    if let Some(model) = model::cooked_model::read(folder, &relative_text, objects) {
        return Ok(model);
    }
    let path = folder.join(&relative);
    let bytes = std::fs::read(&path).map_err(|err| match err.kind() {
        std::io::ErrorKind::NotFound => "the file isn't there".to_owned(),
        _ => err.to_string(),
    })?;
    let base = path.parent().unwrap_or(folder);
    let in_project = model::InProject {
        project: folder,
        relative: &relative_text,
        bytes: &bytes,
    };
    let model = model::load_models_in(&bytes, base, Some(in_project), format, &[pick])
        .map(|mut models| models.remove(0))
        .map_err(|err| with_causes(&err))?;
    model::cooked_model::write(folder, &relative_text, objects, &bytes, &model);
    Ok(model)
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

/// Keeps what reading the model at `mesh_id` gave, reporting its warnings
/// or, once for its file, a failure.
fn store_model(
    shared: &mut Shared,
    mesh_id: &str,
    read: Result<Model, String>,
) -> Option<Arc<Model>> {
    let model = match read {
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
    shared.models.insert(mesh_id.to_owned(), model.clone());
    model
}

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
    shared.sky_images.insert(path.to_owned(), image.clone());
    image
}

/// Keeps what reading the texture at `path` gave, reporting a failure.
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
    shared.textures.insert(path.to_owned(), texture.clone());
    texture
}

/// Reads the texture at `path` inside `folder`, or explains why it can't.
/// Not cached: [`Assets::texture`] is the cached way.
pub fn read_texture(folder: &Path, path: &str) -> Result<Image, String> {
    let relative = safe_path(path).ok_or(
        "a texture must be a file inside the project folder, such as \"assets/textures/wood.png\"",
    )?;
    if let Some(mut image) = cooked::cooked_image(folder, path) {
        image.file = Some(path.to_owned());
        return Ok(image);
    }
    let bytes = std::fs::read(folder.join(&relative)).map_err(|err| match err.kind() {
        std::io::ErrorKind::NotFound => "the file isn't there".to_owned(),
        _ => err.to_string(),
    })?;
    let mut image = model::decode_image(&bytes, path)?.with_origin(model::ImageOrigin::File(
        relative.to_string_lossy().replace('\\', "/"),
    ));
    image.file = Some(path.to_owned());
    Ok(image)
}

/// Decodes a PNG, JPEG, TGA or BMP texture file's `bytes` (`name` is for
/// guessing the format and for messages), never from a cooked copy. For
/// cooking (`axcenity-cook`).
pub fn decode_texture(bytes: &[u8], name: &str) -> Result<Image, String> {
    model::decode_image(bytes, name)
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
        assert!(assets.model_blocking("").is_none());
        assert!(assets.model_blocking("assets/models/gone.glb").is_none());
        assert!(assets.model_blocking("assets/models/gone.glb").is_none());
        let errors = assets.take_errors();
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(errors[0].contains("`assets/models/gone.glb`"), "{errors:?}");
        assert!(errors[0].contains("isn't there"), "{errors:?}");
        assert!(assets.take_errors().is_empty());
    }

    #[test]
    fn paths_outside_the_project_are_refused() {
        let assets = Assets::new(std::env::temp_dir());
        assert!(assets.model_blocking("../secret.glb").is_none());
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
        assert!(assets.model_blocking("assets/notes.txt").is_none());
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
        assert!(assets.model_blocking("assets/models/chair.glb").is_none());
        assert!(assets.take_errors().is_empty());
    }
}
