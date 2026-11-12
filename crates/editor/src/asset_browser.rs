//! The Assets panel: the project's asset files (models, prefabs, images,
//! textures, sounds and particle textures) and the built-in props, as tiles
//! to drag into the viewport.
//!
//! Files are found by walking the project's `assets` folder, again every
//! couple of seconds while the panel is shown, so files copied in by other
//! programs turn up without reopening the project. What a file is comes
//! from its extension and folder:
//!
//! - `.gltf`, `.glb` and `.fbx` are models, `*.prefab.json` prefabs, and
//!   `.wav`, `.ogg`, `.mp3` and `.flac` sounds.
//! - Images are particle textures in `assets/particles`, textures in
//!   `assets/textures` or beside a model in `assets/models`, and UI images
//!   anywhere else. A `*.texture.json` file is one texture set; the images
//!   in its folder are not listed on their own.
//!
//! By default the grid is in a section for each source the assets came
//! from (built-in, made in Axcenity, imported, each Toolbox source, global
//! assets; `origin.rs`), each holding its model folders and files, and a
//! material made from a texture set right after the set. The Source menu
//! shows one source, or turns the sections off. Sections fold.
//!
//! Dragging a tile sets an egui drag-and-drop payload ([`AssetPayload`]);
//! the app places it where it's let go over the viewport. Double-clicking a
//! tile places it at the camera's focus point.

use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use axcenity_core::FoliageSource;
use eframe::egui;

use crate::dialog::{self, Button, Enter, Kind, Tone};
use crate::global_assets;
use crate::props::{PROPS, Prop};
use crate::responsive;
use crate::theme::{self, icons};
use crate::widgets;

pub mod origin;
pub mod packages;
mod panel;
mod scan;
pub mod thumbnails;

use origin::{Origin, Origins};
use packages::{Shown, grouped, shown};
pub use scan::{display_name, kind_of, scan};
use thumbnails::Thumbnails;

/// Where prefabs are saved, in the project folder.
pub const PREFABS_FOLDER: &str = "assets/prefabs";
pub const PREFAB_SUFFIX: &str = ".prefab.json";
/// Where imported UI images and particle textures go.
pub const IMAGES_FOLDER: &str = "assets/images";
pub const PARTICLES_FOLDER: &str = "assets/particles";
/// How often the folder is walked again while the panel is shown.
const RESCAN: Duration = Duration::from_secs(2);
const TILE: egui::Vec2 = egui::vec2(84.0, 96.0);
const PICTURE: f32 = 60.0;
/// The largest a picture is shown when its tile is hovered.
const HOVER_PICTURE: f32 = 192.0;
/// Narrower than this, the panel lists its assets instead of tiling them.
const LIST_BELOW: f32 = 360.0;
const LIST_ROW: f32 = 30.0;
const LIST_PICTURE: f32 = 24.0;
/// From this width the chips, Source menu, search and buttons share one row.
const ONE_ROW_FROM: f32 = 1300.0;
/// From this width the buttons sit beside the search.
const BUTTONS_BESIDE_FROM: f32 = 520.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AssetKind {
    Prop,
    Model,
    Prefab,
    Image,
    Texture,
    /// A texture set (`*.texture.json`) and the maps beside it.
    TextureSet,
    /// A material file (`*.material.json`).
    Material,
    Audio,
    Particle,
    /// An animation clip (`*.anim.json`).
    Animation,
    /// A state machine (`*.statemachine.json`).
    Controller,
    /// A navigation agent's behavior tree (`*.behaviortree.json`).
    BehaviorTree,
    /// A data table scripts read (`assets/data/*.data.json`).
    DataTable,
    /// A cutscene sequence (`*.sequence.json`, Phase 51).
    Sequence,
    /// A sky image (`.exr` or `.hdr`) for Lighting's `SkyImage`.
    Sky,
}

impl AssetKind {
    pub fn icon(self) -> &'static str {
        match self {
            AssetKind::Prop => icons::SHAPES,
            AssetKind::Model => icons::CUBE_TRANSPARENT,
            AssetKind::Prefab => icons::PACKAGE,
            AssetKind::Image => icons::IMAGE,
            AssetKind::Texture | AssetKind::TextureSet => icons::IMAGE_SQUARE,
            AssetKind::Material => icons::SPHERE,
            AssetKind::Audio => icons::WAVEFORM,
            AssetKind::Particle => icons::SPARKLE,
            AssetKind::Animation => theme::AnimKind::Clip.icon(),
            AssetKind::Controller => theme::AnimKind::StateMachine.icon(),
            AssetKind::BehaviorTree => icons::BRAIN,
            AssetKind::DataTable => icons::TABLE,
            AssetKind::Sequence => icons::FILM_SLATE,
            AssetKind::Sky => icons::SUN_HORIZON,
        }
    }

    pub fn color(self, visuals: &egui::Visuals) -> egui::Color32 {
        use axcenity_core::ClassName;
        match self {
            AssetKind::Prop | AssetKind::Model | AssetKind::Prefab => {
                theme::class_color(ClassName::Part, visuals)
            }
            AssetKind::Image | AssetKind::Texture | AssetKind::TextureSet | AssetKind::Material => {
                theme::class_color(ClassName::SurfaceAppearance, visuals)
            }
            AssetKind::Audio => theme::class_color(ClassName::Sound, visuals),
            AssetKind::Particle => theme::class_color(ClassName::PointLight, visuals),
            AssetKind::Animation => theme::AnimKind::Clip.color(visuals),
            AssetKind::Controller => theme::AnimKind::StateMachine.color(visuals),
            AssetKind::BehaviorTree => theme::class_color(ClassName::NavigationAgent, visuals),
            AssetKind::DataTable => theme::class_color(ClassName::Script, visuals),
            AssetKind::Sequence => theme::AnimKind::Clip.color(visuals),
            AssetKind::Sky => theme::class_color(ClassName::Lighting, visuals),
        }
    }

    /// What it is, in a list row.
    pub fn label(self) -> &'static str {
        match self {
            AssetKind::Prop => "Prop",
            AssetKind::Model => "Model",
            AssetKind::Prefab => "Prefab",
            AssetKind::Image => "Image",
            AssetKind::Texture => "Texture",
            AssetKind::TextureSet => "Texture set",
            AssetKind::Material => "Material",
            AssetKind::Audio => "Sound",
            AssetKind::Particle => "Particle",
            AssetKind::Animation => "Clip",
            AssetKind::Controller => "State machine",
            AssetKind::BehaviorTree => "Behavior tree",
            AssetKind::DataTable => "Data table",
            AssetKind::Sequence => "Sequence",
            AssetKind::Sky => "Sky",
        }
    }

    /// What dropping one in the viewport does, for its tile's tooltip.
    fn hover(self) -> &'static str {
        match self {
            AssetKind::Prop | AssetKind::Prefab => {
                "Drag into the viewport to place a copy on the surface under the mouse."
            }
            AssetKind::Model => "Drag into the viewport to place it as a MeshPart.",
            AssetKind::Image => {
                "Drag onto an ImageLabel to show it there, or anywhere else to add an ImageLabel."
            }
            AssetKind::Texture => {
                "Drag onto a MeshPart to use it as the color map of its SurfaceAppearances."
            }
            AssetKind::TextureSet => {
                "Drag onto a part, MeshPart, or terrain (or a layer in the Terrain Studio) to \
                 fill its maps, or double-click to fill the selection."
            }
            AssetKind::Material => {
                "Drag onto a part or MeshPart to draw it in this material (its MaterialVariant), \
                 or double-click to use it on the selection."
            }
            AssetKind::Audio => {
                "Drag onto a part to put a Sound in it, or anywhere else for one in the Workspace."
            }
            AssetKind::Particle => {
                "Drag onto a part to give it a ParticleEmitter with this texture."
            }
            AssetKind::Animation => "Double-click to open it in the Animation tab.",
            AssetKind::Controller => "Drag onto a part to give its Animator this state machine.",
            AssetKind::BehaviorTree => {
                "Drag onto an agent's body to have it run this tree in Play, or double-click to \
                 open it in the Behavior Tree tab."
            }
            AssetKind::DataTable => {
                "Double-click to open it in the Data Table tab. Scripts read it with \
                 DataService:GetTable."
            }
            AssetKind::Sequence => {
                "Double-click to open it in the Sequencer tab. A Sequence Instance plays it."
            }
            AssetKind::Sky => {
                "Double-click, or drag into the viewport or onto Lighting's SkyImage, to use it \
                 as the sky. You're asked first."
            }
        }
    }
}

/// A tile in the panel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetEntry {
    pub kind: AssetKind,
    /// The file's path in the project, with `/` between folders, or a
    /// prop's name.
    pub path: String,
    /// What the tile says: the file name without its extension.
    pub name: String,
}

impl AssetEntry {
    /// The built-in prop this tile is, if it is one.
    pub fn prop(&self) -> Option<&'static Prop> {
        (self.kind == AssetKind::Prop)
            .then(|| PROPS.iter().find(|prop| prop.name == self.path))
            .flatten()
    }

    /// The foliage this tile paints, if the Foliage tool can use it.
    pub fn foliage_source(&self) -> Option<FoliageSource> {
        match self.kind {
            AssetKind::Prop => {
                axcenity_core::FoliageProp::from_name(&self.path).map(FoliageSource::Prop)
            }
            AssetKind::Model => Some(FoliageSource::Model(self.path.clone())),
            _ => None,
        }
    }
}

/// What a click on a tile picks, instead of selecting it.
#[derive(Clone, Debug, PartialEq)]
pub enum Picking {
    /// The Terrain Studio's foliage brush, while the Foliage tool is on: a
    /// prop or a model.
    Foliage(FoliageSource),
    /// The models the selected SplineMesh or SplineScatter uses (Phase
    /// 74): a click on a model uses it (or, for a scatter, takes it out).
    SplineModels(Vec<String>),
}

impl Picking {
    /// Whether a click on `entry` picks it.
    fn takes(&self, entry: &AssetEntry) -> bool {
        match self {
            Picking::Foliage(_) => matches!(entry.kind, AssetKind::Prop | AssetKind::Model),
            Picking::SplineModels(_) => entry.kind == AssetKind::Model,
        }
    }

    /// Whether `entry` is what's picked now, to highlight it.
    fn has(&self, entry: &AssetEntry) -> bool {
        match self {
            Picking::Foliage(source) => foliage_matches(source, entry),
            Picking::SplineModels(models) => {
                entry.kind == AssetKind::Model && models.contains(&entry.path)
            }
        }
    }

    fn hover(&self) -> &'static str {
        match self {
            Picking::Foliage(_) => "Click to paint this as foliage.",
            Picking::SplineModels(_) => "Click to use this model along the selected spline.",
        }
    }
}

fn foliage_matches(source: &FoliageSource, entry: &AssetEntry) -> bool {
    match (source, entry.kind) {
        (FoliageSource::Prop(prop), AssetKind::Prop) => prop.label() == entry.path,
        (FoliageSource::Model(path), AssetKind::Model) => path == &entry.path,
        _ => false,
    }
}

/// What's dragged from the panel to the viewport (or to a terrain layer).
#[derive(Clone, Debug)]
pub struct AssetPayload(pub AssetEntry);

/// A texture set or texture in the project, for a picker outside the panel.
#[derive(Clone)]
pub struct TexturePick {
    pub entry: AssetEntry,
    /// `None` until it has been read, or if it can't be.
    pub thumbnail: Option<egui::TextureHandle>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Category {
    #[default]
    All,
    Props,
    Models,
    Prefabs,
    Images,
    Textures,
    Audio,
    Particles,
    Animations,
    Skies,
    /// The global assets, of any kind.
    Global,
}

impl Category {
    const ALL: [Category; 11] = [
        Category::All,
        Category::Props,
        Category::Models,
        Category::Prefabs,
        Category::Images,
        Category::Textures,
        Category::Audio,
        Category::Particles,
        Category::Animations,
        Category::Skies,
        Category::Global,
    ];

    fn label(self) -> &'static str {
        match self {
            Category::All => "All",
            Category::Props => "Props",
            Category::Models => "Models",
            Category::Prefabs => "Prefabs",
            Category::Images => "Images",
            Category::Textures => "Textures",
            Category::Audio => "Audio",
            Category::Particles => "Particles",
            Category::Animations => "Animations",
            Category::Skies => "Skies",
            Category::Global => "Global",
        }
    }

    fn holds(self, kind: AssetKind) -> bool {
        match self {
            Category::All => true,
            Category::Props => kind == AssetKind::Prop,
            Category::Models => kind == AssetKind::Model,
            Category::Prefabs => kind == AssetKind::Prefab,
            Category::Images => kind == AssetKind::Image,
            Category::Textures => matches!(
                kind,
                AssetKind::Texture | AssetKind::TextureSet | AssetKind::Material
            ),
            Category::Audio => kind == AssetKind::Audio,
            Category::Particles => kind == AssetKind::Particle,
            Category::Animations => {
                matches!(
                    kind,
                    AssetKind::Animation | AssetKind::Controller | AssetKind::Sequence
                )
            }
            Category::Skies => kind == AssetKind::Sky,
            // Which files are global is the path's, checked by the panel.
            Category::Global => kind != AssetKind::Prop,
        }
    }

    /// Where files imported while this category is shown go, for images.
    pub fn image_folder(self) -> &'static str {
        match self {
            Category::Textures => axcenity_assets::PROJECT_TEXTURES_FOLDER,
            Category::Particles => PARTICLES_FOLDER,
            _ => IMAGES_FOLDER,
        }
    }
}

/// What the panel asks the app to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BrowserAction {
    /// Show the Toolbox tab.
    OpenToolbox,
    /// Place this at the camera's focus point (a double-click).
    Place(AssetEntry),
    /// A click while [`Picking`]: use this as the Terrain Studio's foliage
    /// brush, or along the selected SplineMesh or SplineScatter.
    Pick(AssetEntry),
    /// Copy files into the project; images go in this folder.
    Import { image_folder: &'static str },
    /// Save the selection as a prefab.
    SavePrefab,
    /// Rename the prefab file at `path` to `name` (already checked).
    RenamePrefab { path: String, name: String },
    /// Copy the prefab file at `path`.
    DuplicatePrefab { path: String },
    /// Make a variant of the prefab at `path` (Phase 49).
    NewVariant { path: String },
    /// Delete these project files (assets' own files, already confirmed),
    /// as one undo step named after `name`.
    DeleteAssets { name: String, files: Vec<String> },
    /// Export these files (and what they need) as an asset package named `name`.
    ExportPackage { name: String, paths: Vec<String> },
    /// Open the material file at this path in the Material tab.
    EditMaterial(String),
    /// Open the model file at this path in the Model Studio.
    EditModel(String),
    /// List what uses the asset at this path (Phase 70).
    FindReferences(String),
    /// Rename or move the asset at `from` to `to`, with every reference.
    RenameAsset { from: String, to: String },
    /// Work out which of these project files nothing uses, for the Unused
    /// filter ([`AssetBrowser::set_unused`]).
    FindUnused(Vec<String>),
    /// A line for Output. `error` is a failure.
    Notice { error: bool, text: String },
}

/// A question the panel is asking about an asset.
enum AssetDialog {
    /// Rename the prefab at `path`.
    Rename {
        path: String,
        name: String,
        error: Option<String>,
    },
    /// Rename or move the asset at `from`; `to` is the path being typed.
    Move { from: String, to: String },
    /// Delete `files`, the own files of the assets called `name`. `global`
    /// when one of them is a global asset, whose library copy stays.
    Delete {
        name: String,
        files: Vec<String>,
        global: bool,
    },
}

#[derive(Default)]
pub struct AssetBrowser {
    category: Category,
    search: String,
    folder: Option<PathBuf>,
    files: Vec<AssetEntry>,
    scanned_at: Option<Instant>,
    thumbnails: Thumbnails,
    dialog: Option<AssetDialog>,
    /// The imported model's folder shown (by its name), if one is open.
    open_package: Option<String>,
    /// The paths of the assets picked with a click (Ctrl or Cmd adds),
    /// for Export Package….
    picked: Vec<String>,
    /// The global assets' folder (see `global_assets.rs`), if there is one.
    library: Option<PathBuf>,
    /// The global assets this project hasn't got, scanned with `files`.
    shared: Vec<AssetEntry>,
    /// The paths of `files` and of every global asset, in lower case.
    local_paths: HashSet<String>,
    global_paths: HashSet<String>,
    global_thumbnails: Thumbnails,
    /// The Unused chip: only assets nothing uses.
    unused_only: bool,
    /// The project files nothing uses, as the app last worked out; `None`
    /// until it has (or after a change).
    unused: Option<HashSet<String>>,
    /// Goes up each time the listed files change (a scan that found
    /// something new, renamed or gone, or another project), so universal
    /// search lists them again only then (`app/search.rs`).
    generation: u64,
    /// Where the project's Toolbox assets came from
    /// (`.axcenity/asset-sources.json`), read with each scan.
    sources: axcenity_toolbox::provenance::Snapshot,
    /// Where every listed asset came from, worked out again when the files
    /// or `sources` change.
    origins: Origins,
    /// The Source menu: only the assets from here.
    source_filter: Option<Origin>,
    /// Work out `origins` on the next scan even if no file changed (a
    /// material file was edited).
    origins_stale: bool,
    /// The Source menu's "Group by source" is off: one grid, as before.
    flat: bool,
    /// The sections folded shut.
    folded: HashSet<Origin>,
}

impl AssetBrowser {
    /// Forgets the old project's files, for a newly opened (or closed) one.
    pub fn open(&mut self, folder: Option<&Path>) {
        self.folder = folder.map(Path::to_owned);
        self.files.clear();
        self.shared.clear();
        self.generation += 1;
        self.thumbnails.open(folder);
        self.scanned_at = None;
        self.dialog = None;
        self.open_package = None;
    }

    /// Uses the global assets in `library` (`None` for none), in every
    /// project from now on.
    pub fn set_library(&mut self, library: Option<PathBuf>) {
        self.global_thumbnails.open(library.as_deref());
        self.library = library;
        self.scanned_at = None;
    }

    /// Where the asset at `path` came from and its licence, for its
    /// tooltip: "From Poly Haven · CC0 · by Rob Tuytel".
    pub fn source_line(&self, path: &str) -> Option<String> {
        use axcenity_toolbox::{license, provenance};
        let record = self.sources.source_of(path)?;
        let mut line = format!(
            "From {} · {}",
            provenance::source_name(&record.provider),
            license::display(&record.license, &record.license_version)
        );
        if !record.authors.is_empty() {
            line.push_str(&format!(" · by {}", record.authors.join(", ")));
        }
        if let Some(why) = license::commercial_block(&record.license) {
            line.push_str(&format!("\n{why}"));
        }
        Some(line)
    }

    /// Walks the folder again on the next frame the panel is shown.
    pub fn refresh(&mut self) {
        self.scanned_at = None;
        self.unused = None;
    }

    /// Shows the asset at `path` (Quick Open): every category, searched
    /// for its path, and picked.
    pub fn reveal(&mut self, path: &str) {
        self.category = Category::All;
        self.unused_only = false;
        self.open_package = None;
        self.search = path.to_owned();
        self.picked = vec![path.to_owned()];
    }

    /// The files nothing uses, for the Unused filter.
    pub fn set_unused(&mut self, unused: Vec<String>) {
        self.unused = Some(unused.into_iter().collect());
    }

    /// The files the Unused filter shows, in path order.
    #[cfg(test)]
    pub fn unused_paths(&self) -> Vec<String> {
        let mut paths: Vec<String> = self.unused.iter().flatten().cloned().collect();
        paths.sort();
        paths
    }

    /// The file at `path` changed on disk: list it again, with a new
    /// thumbnail.
    pub fn file_changed(&mut self, path: &str) {
        self.thumbnails.forget(path);
        self.origins_stale = true;
        self.refresh();
    }

    /// Every tile: the props, then the project's files in path order, then
    /// the global assets it hasn't got.
    pub fn entries(&mut self) -> Vec<AssetEntry> {
        self.rescan_if_due();
        let props = PROPS.iter().map(|prop| AssetEntry {
            kind: AssetKind::Prop,
            path: prop.name.to_owned(),
            name: prop.name.to_owned(),
        });
        props.chain(self.files_and_shared()).collect()
    }

    /// The project's own files (not the global assets it hasn't got), its
    /// model folders, every asset's origin and its Toolbox sources, scanned
    /// again if it's time: what the Downloaded / Imported Assets tab lists.
    pub fn project_assets(
        &mut self,
    ) -> (
        Vec<AssetEntry>,
        Vec<packages::Package>,
        Origins,
        axcenity_toolbox::provenance::Snapshot,
    ) {
        self.rescan_if_due();
        (
            self.files.clone(),
            packages::packages(&self.files),
            self.origins.clone(),
            self.sources.clone(),
        )
    }

    fn files_and_shared(&self) -> Vec<AssetEntry> {
        self.files.iter().chain(&self.shared).cloned().collect()
    }

    /// Scans the folders again if it's time (every couple of seconds, or
    /// after [`refresh`](Self::refresh)), whether or not the panel shows.
    pub fn poll(&mut self) {
        self.rescan_if_due();
    }

    /// The project's files, then the global assets it hasn't got, as last
    /// scanned (no built-in props).
    pub fn listed(&self) -> impl Iterator<Item = &AssetEntry> {
        self.files.iter().chain(&self.shared)
    }

    /// Changes whenever [`listed`](Self::listed) does.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// True if the library has the asset at `path`.
    pub fn is_global(&self, path: &str) -> bool {
        self.global_paths.contains(&path.to_lowercase())
    }

    /// True if the project has the asset at `path`.
    pub fn is_local(&self, path: &str) -> bool {
        self.local_paths.contains(&path.to_lowercase())
    }

    /// Makes the asset at `path` global (copied into the library, with what
    /// it needs) or local again (its files deleted from the library). Says
    /// what was done, for Output.
    pub fn set_global(&mut self, path: &str, global: bool) -> Result<String, String> {
        let (Some(project), Some(library)) = (&self.folder, &self.library) else {
            return Err("global assets need a project open and a folder for them".to_owned());
        };
        let name = display_name(path);
        let text = if global {
            let copied = global_assets::make_global(project, library, path)?;
            let files = if copied == 1 { "file" } else { "files" };
            format!("{name} is a global asset now, in every project ({copied} {files}).")
        } else {
            global_assets::remove(library, path)?;
            format!(
                "{name} is no longer a global asset. Projects that used it keep their own copy."
            )
        };
        self.refresh();
        Ok(text)
    }

    /// Copies the global asset at `path`, and what it needs, into the
    /// project if it's missing there, before it's used. Returns how many
    /// files were copied (0 for an asset the project already has).
    pub fn use_here(&mut self, path: &str) -> Result<usize, String> {
        let (Some(project), Some(library)) = (&self.folder, &self.library) else {
            return Ok(0);
        };
        let copied = global_assets::use_in(library, project, path)?;
        if copied > 0 {
            self.refresh();
        }
        Ok(copied)
    }

    /// The thumbnail of `entry`'s picture: the project's file, or the
    /// library's for a global asset the project hasn't got.
    fn thumbnail_of(
        &mut self,
        ctx: &egui::Context,
        entry: &AssetEntry,
    ) -> Option<egui::TextureHandle> {
        let set = entry.kind == AssetKind::TextureSet;
        if self.is_local(&entry.path) {
            self.thumbnails.get(ctx, &entry.path, set)
        } else {
            self.global_thumbnails.get(ctx, &entry.path, set)
        }
    }

    /// The project's texture sets and textures (and the global ones), sets
    /// first, each with its thumbnail once it has been read (asked for here
    /// the first time). For pickers outside the panel, such as a terrain
    /// layer's.
    pub fn textures(&mut self, ctx: &egui::Context) -> Vec<TexturePick> {
        self.rescan_if_due();
        self.thumbnails.poll(ctx);
        self.global_thumbnails.poll(ctx);
        let mut picks: Vec<TexturePick> = self
            .files_and_shared()
            .into_iter()
            .filter(|entry| matches!(entry.kind, AssetKind::Texture | AssetKind::TextureSet))
            .map(|entry| TexturePick {
                thumbnail: self.thumbnail_of(ctx, &entry),
                entry,
            })
            .collect();
        // A stable sort keeps path order within sets and within textures.
        picks.sort_by_key(|pick| pick.entry.kind != AssetKind::TextureSet);
        picks
    }

    /// The project's own pictures, images then textures, each with its
    /// thumbnail once it has been read: what the UI Designer's Image picker
    /// offers. Global ones aren't listed, since they'd need copying in first.
    pub fn pictures(&mut self, ctx: &egui::Context) -> Vec<TexturePick> {
        self.rescan_if_due();
        self.thumbnails.poll(ctx);
        let entries: Vec<AssetEntry> = self
            .files
            .iter()
            .filter(|entry| matches!(entry.kind, AssetKind::Image | AssetKind::Texture))
            .cloned()
            .collect();
        let mut picks: Vec<TexturePick> = entries
            .into_iter()
            .map(|entry| TexturePick {
                thumbnail: self.thumbnails.get(ctx, &entry.path, false),
                entry,
            })
            .collect();
        picks.sort_by_key(|pick| pick.entry.kind != AssetKind::Image);
        picks
    }

    /// The thumbnail of the image at `path` (a project path), asked for
    /// the first time; `None` until it has been read.
    pub fn thumbnail(&mut self, ctx: &egui::Context, path: &str) -> Option<egui::TextureHandle> {
        self.thumbnails.poll(ctx);
        self.thumbnails.get(ctx, path, false)
    }

    fn rescan_if_due(&mut self) {
        if self.scanned_at.is_some_and(|at| at.elapsed() < RESCAN) {
            return;
        }
        self.scanned_at = Some(Instant::now());
        let files = match &self.folder {
            Some(folder) => scan(folder),
            None => Vec::new(),
        };
        let old_shared = std::mem::take(&mut self.shared);
        let files_changed = files != self.files;
        self.files = files;
        let sources = self
            .folder
            .as_deref()
            .and_then(|folder| axcenity_toolbox::provenance::Snapshot::read(folder).ok())
            .unwrap_or_default();
        let sources_changed = sources != self.sources;
        self.sources = sources;
        // Global assets show only with a project open, to use in it.
        let global = match (&self.folder, &self.library) {
            (Some(_), Some(library)) => scan(library),
            _ => Vec::new(),
        };
        let lower = |entries: &[AssetEntry]| -> HashSet<String> {
            entries
                .iter()
                .map(|entry| entry.path.to_lowercase())
                .collect()
        };
        self.local_paths = lower(&self.files);
        self.global_paths = lower(&global);
        self.shared = global
            .into_iter()
            .filter(|entry| !self.local_paths.contains(&entry.path.to_lowercase()))
            .collect();
        if files_changed || old_shared != self.shared {
            self.generation += 1;
        }
        if files_changed || sources_changed || old_shared != self.shared || self.origins_stale {
            self.origins_stale = false;
            let packages = packages::packages(&self.files_and_shared());
            self.origins = Origins::work_out(
                self.folder.as_deref(),
                &self.files,
                &self.shared,
                &packages,
                &self.sources,
            );
        }
        self.thumbnails.forget_missing();
        self.global_thumbnails.forget_missing();
    }
}

fn uv() -> egui::Rect {
    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0))
}

pub fn icon_of(entry: &AssetEntry) -> &'static str {
    entry.prop().map_or(entry.kind.icon(), |prop| prop.icon)
}

/// What a tile's right-click menu asked for.
pub(super) enum ItemMenu {
    FindReferences {
        path: String,
    },
    /// Rename or move a project asset (not a prefab, which has its own).
    Move {
        path: String,
    },
    /// Make the asset at `path` global, or local again.
    Global {
        path: String,
        global: bool,
    },
    Rename {
        path: String,
        name: String,
    },
    Duplicate {
        path: String,
    },
    NewVariant {
        path: String,
    },
    /// Delete the assets at `paths` (called `name`), once confirmed.
    Delete {
        name: String,
        paths: Vec<String>,
    },
    /// Export these files as an asset package named `name`.
    ExportPackage {
        name: String,
        paths: Vec<String>,
    },
    EditMaterial {
        path: String,
    },
    EditModel {
        path: String,
    },
}

/// The Export Package… item of a right-click menu, for the asset `name`
/// or, with several `paths`, the picked assets.
pub(super) fn export_package_button(
    ui: &mut egui::Ui,
    name: &str,
    paths: &[String],
) -> Option<ItemMenu> {
    let label = match paths.len() {
        1 => "Export Package…".to_owned(),
        n => format!("Export Package… ({n} assets)"),
    };
    let clicked = ui
        .button(theme::with_icon(icons::PACKAGE, &label))
        .on_hover_text(
            "Save it, and every file it needs, as one .axpkg file to import into another project.",
        )
        .clicked();
    clicked.then(|| {
        ui.close();
        ItemMenu::ExportPackage {
            name: if paths.len() == 1 {
                name.to_owned()
            } else {
                "Package".to_owned()
            },
            paths: paths.to_vec(),
        }
    })
}

/// The Delete… item of a right-click menu, for the asset `name` or, with
/// several `paths`, the picked assets. It asks before deleting.
pub(super) fn delete_button(ui: &mut egui::Ui, name: &str, paths: &[String]) -> Option<ItemMenu> {
    let label = match paths.len() {
        0 => return None,
        1 => "Delete…".to_owned(),
        n => format!("Delete… ({n} assets)"),
    };
    let clicked = ui
        .button(theme::with_icon(icons::TRASH, &label))
        .on_hover_text("Delete its files from the project. Undo puts them back.")
        .clicked();
    clicked.then(|| {
        ui.close();
        ItemMenu::Delete {
            name: if paths.len() == 1 {
                name.to_owned()
            } else {
                format!("{} assets", paths.len())
            },
            paths: paths.to_vec(),
        }
    })
}

/// A globe at the top right of a global asset's picture.
pub(super) fn global_badge(ui: &egui::Ui, picture: egui::Rect) {
    let palette = theme::palette(ui.ctx());
    let radius = (picture.width() * 0.17).max(6.0);
    let center = picture.right_top() + egui::vec2(-radius - 1.0, radius + 1.0);
    let painter = ui.painter();
    painter.circle_filled(center, radius, palette.accent);
    painter.text(
        center,
        egui::Align2::CENTER_CENTER,
        icons::GLOBE,
        egui::FontId::proportional(radius * 1.5),
        palette.on_accent,
    );
}

/// Why `name` can't replace the prefab at `path`, if it can't. The `Ok`
/// name is trimmed.
fn check_prefab_rename(folder: Option<&Path>, path: &str, name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("a prefab needs a name".to_owned());
    }
    if prefab_file_stem(name) != name {
        return Err("use only letters, digits, spaces, - and _".to_owned());
    }
    let parent = path
        .rsplit_once('/')
        .map_or(PREFABS_FOLDER, |(parent, _)| parent);
    let new_path = format!("{parent}/{name}{PREFAB_SUFFIX}");
    if new_path == path {
        return Ok(name.to_owned());
    }
    let differs_in_case_only = new_path.eq_ignore_ascii_case(path);
    if !differs_in_case_only && folder.is_some_and(|folder| folder.join(&new_path).exists()) {
        return Err(format!("there's already a prefab called {name}"));
    }
    Ok(name.to_owned())
}

/// A file name for a prefab called `name`: letters, digits, spaces, `-`
/// and `_` are kept, anything else becomes `_`.
pub fn prefab_file_stem(name: &str) -> String {
    let cleaned: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    match cleaned.trim() {
        "" => "Prefab".to_owned(),
        stem => stem.to_owned(),
    }
}

/// Whether an asset matches the panel's search (already lower case): every
/// word of it is in the asset's path or kind, so "sound door" finds
/// `assets/sounds/door.ogg` and "texture" every texture.
fn search_matches(entry: &AssetEntry, search: &str) -> bool {
    let text = format!("{} {}", entry.kind.label(), entry.path).to_lowercase();
    search.split_whitespace().all(|word| text.contains(word))
}

#[cfg(test)]
mod tests;
