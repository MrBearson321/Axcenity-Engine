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
//! Dragging a tile sets an egui drag-and-drop payload ([`AssetPayload`]);
//! the app places it where it's let go over the viewport. Double-clicking a
//! tile places it at the camera's focus point.

use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use axcenity_core::FoliageSource;
use eframe::egui;

use crate::ambient_cg::GetTexture;
use crate::global_assets;
use crate::props::{PROPS, Prop};
use crate::responsive;
use crate::theme::{self, icons};
use crate::widgets;

pub mod packages;
mod scan;
pub mod thumbnails;

use packages::{Shown, shown};
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
/// From this width the chips, search and buttons share one row.
const ONE_ROW_FROM: f32 = 980.0;
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
                "Drag onto a part or MeshPart to draw it in this material (its MaterialVariant),                  or double-click to use it on the selection."
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
    /// The global assets, of any kind.
    Global,
}

impl Category {
    const ALL: [Category; 10] = [
        Category::All,
        Category::Props,
        Category::Models,
        Category::Prefabs,
        Category::Images,
        Category::Textures,
        Category::Audio,
        Category::Particles,
        Category::Animations,
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
                matches!(kind, AssetKind::Animation | AssetKind::Controller)
            }
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
    /// Place this at the camera's focus point (a double-click).
    Place(AssetEntry),
    /// Use this as the Terrain Studio's foliage brush (a click while Foliage is on).
    Foliage(AssetEntry),
    /// Copy files into the project; images go in this folder.
    Import { image_folder: &'static str },
    /// Save the selection as a prefab.
    SavePrefab,
    /// Rename the prefab file at `path` to `name` (already checked).
    RenamePrefab { path: String, name: String },
    /// Copy the prefab file at `path`.
    DuplicatePrefab { path: String },
    /// Delete these project files (assets' own files, already confirmed),
    /// as one undo step named after `name`.
    DeleteAssets { name: String, files: Vec<String> },
    /// Export these files (and what they need) as an asset package named `name`.
    ExportPackage { name: String, paths: Vec<String> },
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
    get_texture: Option<GetTexture>,
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
}

impl AssetBrowser {
    /// Forgets the old project's files, for a newly opened (or closed) one.
    pub fn open(&mut self, folder: Option<&Path>) {
        self.folder = folder.map(Path::to_owned);
        self.files.clear();
        self.shared.clear();
        self.thumbnails.open(folder);
        self.scanned_at = None;
        self.dialog = None;
        self.get_texture = None;
        self.open_package = None;
    }

    /// Uses the global assets in `library` (`None` for none), in every
    /// project from now on.
    pub fn set_library(&mut self, library: Option<PathBuf>) {
        self.global_thumbnails.open(library.as_deref());
        self.library = library;
        self.scanned_at = None;
    }

    /// Walks the folder again on the next frame the panel is shown.
    pub fn refresh(&mut self) {
        self.scanned_at = None;
    }

    /// The file at `path` changed on disk: list it again, with a new
    /// thumbnail.
    pub fn file_changed(&mut self, path: &str) {
        self.thumbnails.forget(path);
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

    fn files_and_shared(&self) -> Vec<AssetEntry> {
        self.files.iter().chain(&self.shared).cloned().collect()
    }

    /// True if the library has the asset at `path`.
    pub fn is_global(&self, path: &str) -> bool {
        self.global_paths.contains(&path.to_lowercase())
    }

    /// True if the project has the asset at `path`.
    fn is_local(&self, path: &str) -> bool {
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
        self.files = match &self.folder {
            Some(folder) => scan(folder),
            None => Vec::new(),
        };
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
        self.thumbnails.forget_missing();
        self.global_thumbnails.forget_missing();
    }

    /// Draws the panel. `can_edit` is false during Play, and `can_save` is
    /// true when something that can be a prefab is selected. `foliage` is
    /// the Terrain Studio's current item while the Foliage tool is on: a
    /// click on a prop or model then picks it to paint.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        can_edit: bool,
        can_save: bool,
        foliage: Option<&FoliageSource>,
    ) -> Option<BrowserAction> {
        let mut action = None;
        self.thumbnails.poll(ui.ctx());
        self.global_thumbnails.poll(ui.ctx());
        let palette = theme::palette(ui.ctx());
        let width = ui.available_width();
        if width >= ONE_ROW_FROM {
            // Everything on one row: the chips, then the rest at the right.
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                self.chips(ui);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    if let Some(done) = self.buttons(ui, can_edit, can_save, true) {
                        action = Some(done);
                    }
                    self.refresh_button(ui);
                    widgets::search_field(ui, &mut self.search, "Search assets", 170.0);
                });
            });
        } else {
            // The search on its own row, with the buttons beside it when
            // there's room, and the chips wrapping under it.
            let buttons_beside = width >= BUTTONS_BESIDE_FROM;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let search_width = if buttons_beside {
                    200.0
                } else {
                    // The width is the text's: the field adds its margins
                    // (for the magnifying glass and the clear button).
                    ui.available_width()
                        - ui.spacing().interact_size.y
                        - 6.0
                        - widgets::SEARCH_FIELD_MARGINS
                };
                widgets::search_field(ui, &mut self.search, "Search assets", search_width);
                self.refresh_button(ui);
                if buttons_beside {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let Some(done) = self.buttons(ui, can_edit, can_save, true) {
                            action = Some(done);
                        }
                    });
                }
            });
            ui.add_space(2.0);
            responsive::toolbar(ui, |ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                self.chips(ui);
            });
            if !buttons_beside {
                ui.add_space(2.0);
                responsive::toolbar(ui, |ui| {
                    if let Some(done) = self.buttons(ui, can_edit, can_save, false) {
                        action = Some(done);
                    }
                });
            }
        }
        ui.add_space(4.0);
        widgets::divider(ui);
        ui.add_space(4.0);

        let search = self.search.trim().to_lowercase();
        let entries: Vec<AssetEntry> = self
            .entries()
            .into_iter()
            .filter(|entry| self.category.holds(entry.kind))
            .filter(|entry| self.category != Category::Global || self.is_global(&entry.path))
            .filter(|entry| search.is_empty() || entry.path.to_lowercase().contains(&search))
            .collect();
        let packages = packages::packages(&self.files_and_shared());
        // A folder that's gone (deleted, or another project) closes.
        let open = self.open_package.as_ref().and_then(|name| {
            packages
                .iter()
                .find(|package| package.name.eq_ignore_ascii_case(name))
        });
        if open.is_none() {
            self.open_package = None;
        }
        let in_folder = open.is_some() && search.is_empty();
        let shown = shown(entries, &packages, open, !search.is_empty());
        if let Some(package) = open
            && search.is_empty()
            && self.folder_header(ui, package)
        {
            self.open_package = None;
        }
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                if shown.is_empty() {
                    ui.add_space(16.0);
                    ui.vertical_centered(|ui| {
                        ui.label(
                            egui::RichText::new(if search.is_empty() {
                                icons::FOLDER_DASHED
                            } else {
                                icons::MAGNIFYING_GLASS
                            })
                            .size(20.0)
                            .color(palette.text_muted),
                        );
                        ui.label(
                            egui::RichText::new(if in_folder {
                                "Nothing of this kind in this folder."
                            } else if search.is_empty() && self.category == Category::Global {
                                "No global assets yet. Right-click an asset and tick Global \
                                 asset to use it in every project."
                            } else if search.is_empty() {
                                "Nothing here yet. Import files, or copy them into the project's \
                                 assets folder."
                            } else {
                                "No asset's path has that in it."
                            })
                            .color(palette.text_weak),
                        );
                    });
                    return;
                }
                // A readable list in a narrow column, tiles where there's room.
                let list = ui.available_width() < LIST_BELOW;
                for block in shown {
                    if let Some(heading) = block.heading {
                        ui.add_space(4.0);
                        widgets::section_label(ui, heading);
                        ui.add_space(2.0);
                    }
                    let items = |ui: &mut egui::Ui| {
                        for item in block.items {
                            let entry = match item {
                                Shown::Asset(entry) => entry,
                                Shown::Folder(package) => {
                                    let (opened, menu) =
                                        self.folder_item(ui, &package, can_edit, list);
                                    if opened {
                                        self.open_package = Some(package.name.clone());
                                    }
                                    if let Some(done) = self.follow_menu(menu) {
                                        action = Some(done);
                                    }
                                    continue;
                                }
                            };
                            let (place, pick, menu) =
                                self.item(ui, &entry, can_edit, list, foliage);
                            if pick {
                                action = Some(BrowserAction::Foliage(entry.clone()));
                            } else if place {
                                action = Some(BrowserAction::Place(entry.clone()));
                            }
                            if let Some(done) = self.follow_menu(menu) {
                                action = Some(done);
                            }
                        }
                    };
                    if list {
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 1.0;
                            items(ui);
                        });
                    } else {
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
                            items(ui);
                        });
                    }
                }
            });
        if let Some(done) = self.asset_dialog(ui.ctx()) {
            action = Some(done);
        }
        if let Some(done) = self.texture_dialog(ui.ctx()) {
            action = Some(done);
        }
        action
    }

    /// Does what a tile's right-click menu asked for, or opens its dialog.
    fn follow_menu(&mut self, menu: Option<ItemMenu>) -> Option<BrowserAction> {
        match menu? {
            ItemMenu::Global { path, global } => {
                let (error, text) = match self.set_global(&path, global) {
                    Ok(text) => (false, text),
                    Err(problem) => (true, format!("Global assets: {problem}")),
                };
                return Some(BrowserAction::Notice { error, text });
            }
            ItemMenu::Rename { path, name } => {
                self.dialog = Some(AssetDialog::Rename {
                    path,
                    name,
                    error: None,
                });
            }
            ItemMenu::Duplicate { path } => {
                return Some(BrowserAction::DuplicatePrefab { path });
            }
            ItemMenu::Delete { name, paths } => {
                let files = self.files_to_delete(&paths);
                if files.is_empty() {
                    return Some(BrowserAction::Notice {
                        error: true,
                        text: format!("Could not delete {name}: its files aren't there."),
                    });
                }
                self.dialog = Some(AssetDialog::Delete {
                    name,
                    global: paths.iter().any(|path| self.is_global(path)),
                    files,
                });
            }
            ItemMenu::ExportPackage { name, paths } => {
                return Some(BrowserAction::ExportPackage { name, paths });
            }
        }
        None
    }

    /// The project files deleting the assets at `paths` removes: each one's
    /// own files (see `global_assets::own_files`: a model's folder and its
    /// clips, a texture set's folder, or the one file), sorted.
    pub fn files_to_delete(&self, paths: &[String]) -> Vec<String> {
        let Some(folder) = &self.folder else {
            return Vec::new();
        };
        let files: BTreeSet<String> = paths
            .iter()
            .flat_map(|path| global_assets::own_files(folder, path))
            .collect();
        files.into_iter().collect()
    }

    /// A click on the asset at `path`: picks only it, or with `adding`
    /// (Ctrl or Cmd held) picks or unpicks it along with the others.
    fn pick_click(&mut self, path: &str, adding: bool) {
        match self.picked.iter().position(|picked| picked == path) {
            Some(at) if adding => {
                self.picked.remove(at);
            }
            None if adding => self.picked.push(path.to_owned()),
            _ => self.picked = vec![path.to_owned()],
        }
    }

    /// The "Global asset" checkbox of the asset at `path`'s right-click
    /// menu. Off while there's no folder for global assets.
    pub(super) fn global_checkbox(&self, ui: &mut egui::Ui, path: &str) -> Option<ItemMenu> {
        let mut global = self.is_global(path);
        let response = ui
            .add_enabled(
                self.library.is_some() && self.folder.is_some(),
                egui::Checkbox::new(&mut global, theme::with_icon(icons::GLOBE, "Global asset")),
            )
            .on_hover_text(if global {
                "Untick to take it out of global assets. Projects that used it keep their copy."
            } else {
                "Tick to use it in every project. It's copied into a project the first time \
                 it's used there."
            })
            .on_disabled_hover_text("There's no folder for global assets on this computer.");
        if response.changed() {
            ui.close();
            return Some(ItemMenu::Global {
                path: path.to_owned(),
                global,
            });
        }
        None
    }

    /// The category chips, which pick what's shown.
    fn chips(&mut self, ui: &mut egui::Ui) {
        for category in Category::ALL {
            if chip(ui, category.label(), self.category == category).clicked() {
                self.category = category;
            }
        }
    }

    fn refresh_button(&mut self, ui: &mut egui::Ui) {
        if widgets::icon_button(ui, icons::ARROW_CLOCKWISE, "Look for new files now", false)
            .clicked()
        {
            self.refresh();
        }
    }

    /// Import and Save prefab, added last-first when `right_to_left`, so
    /// they read the same way in either layout.
    fn buttons(
        &mut self,
        ui: &mut egui::Ui,
        can_edit: bool,
        can_save: bool,
        right_to_left: bool,
    ) -> Option<BrowserAction> {
        let palette = theme::palette(ui.ctx());
        let mut action = None;
        let mut get_texture = |ui: &mut egui::Ui| {
            if ui
                .add_enabled(
                    can_edit && self.folder.is_some(),
                    widgets::button(
                        widgets::Role::Secondary,
                        Some(icons::IMAGE_SQUARE),
                        "Get texture",
                        &palette,
                    )
                    .min_size(egui::vec2(0.0, 24.0)),
                )
                .on_hover_text("Search ambientCG and save a texture set into assets/textures.")
                .clicked()
            {
                self.get_texture = Some(GetTexture::open(ui.ctx()));
            }
        };
        let mut import = |ui: &mut egui::Ui| {
            if ui
                .add_enabled(
                    can_edit && self.folder.is_some(),
                    widgets::button(
                        widgets::Role::Secondary,
                        Some(icons::PLUS),
                        "Import",
                        &palette,
                    )
                    .min_size(egui::vec2(0.0, 24.0)),
                )
                .on_hover_text(
                    "Copy models, images, sounds or prefabs into the project's assets folder.",
                )
                .clicked()
            {
                action = Some(BrowserAction::Import {
                    image_folder: self.category.image_folder(),
                });
            }
        };
        let save = |ui: &mut egui::Ui| {
            ui.add_enabled(
                can_edit && can_save,
                widgets::button(
                    widgets::Role::Secondary,
                    Some(icons::FLOPPY_DISK),
                    "Save prefab",
                    &palette,
                )
                .min_size(egui::vec2(0.0, 24.0)),
            )
            .on_hover_text(
                "Save the selected parts and Models as a prefab in assets/prefabs, to                  place copies of from here.",
            )
            .on_disabled_hover_text("Select parts or Models to save as a prefab.")
            .clicked()
        };
        let saved = if right_to_left {
            let saved = save(ui);
            import(ui);
            get_texture(ui);
            saved
        } else {
            get_texture(ui);
            import(ui);
            save(ui)
        };
        if saved {
            action = Some(BrowserAction::SavePrefab);
        }
        action
    }

    /// The rename question for a prefab tile, and the delete question for
    /// any project asset.
    fn asset_dialog(&mut self, ctx: &egui::Context) -> Option<BrowserAction> {
        let folder = self.folder.clone();
        let dialog = self.dialog.as_mut()?;
        let mut action = None;
        let mut close = false;
        let modal = egui::Modal::new(egui::Id::new("asset dialog")).show(ctx, |ui| {
            ui.set_width(360.0);
            match dialog {
                AssetDialog::Rename { path, name, error } => {
                    ui.heading("Rename prefab");
                    let response = ui.text_edit_singleline(name);
                    if let Some(problem) = error {
                        ui.colored_label(ui.visuals().error_fg_color, problem.as_str());
                    }
                    let enter =
                        response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    ui.horizontal(|ui| {
                        if ui.button("Rename").clicked() || enter {
                            match check_prefab_rename(folder.as_deref(), path, name) {
                                Ok(name) => {
                                    action = Some(BrowserAction::RenamePrefab {
                                        path: path.clone(),
                                        name,
                                    });
                                    close = true;
                                }
                                Err(problem) => *error = Some(problem),
                            }
                        }
                        if ui.button("Cancel").clicked() {
                            close = true;
                        }
                    });
                }
                AssetDialog::Delete {
                    name,
                    files,
                    global,
                } => {
                    ui.heading(format!("Delete {name}?"));
                    match files.as_slice() {
                        [only] => {
                            ui.label(format!("This deletes `{only}` from the project."));
                        }
                        files => {
                            ui.label(format!(
                                "This deletes {} files from the project:",
                                files.len()
                            ));
                            egui::ScrollArea::vertical()
                                .max_height(120.0)
                                .show(ui, |ui| {
                                    for file in files {
                                        ui.label(
                                            egui::RichText::new(file)
                                                .size(theme::type_scale::SMALL)
                                                .color(ui.visuals().weak_text_color()),
                                        );
                                    }
                                });
                        }
                    }
                    ui.label(
                        "Anything that uses it shows it as missing (File > Check Assets lists \
                         those). Undo puts the files back.",
                    );
                    if *global {
                        ui.label(
                            "The global copy stays. Untick Global asset to take it out of \
                             every project.",
                        );
                    }
                    ui.horizontal(|ui| {
                        if ui.button(format!("{} Delete", icons::TRASH)).clicked() {
                            action = Some(BrowserAction::DeleteAssets {
                                name: name.clone(),
                                files: files.clone(),
                            });
                            close = true;
                        }
                        if ui.button("Cancel").clicked() {
                            close = true;
                        }
                    });
                }
            }
        });
        if modal.should_close() {
            close = true;
        }
        if close {
            self.dialog = None;
        }
        if matches!(action, Some(BrowserAction::DeleteAssets { .. })) {
            self.picked.clear();
        }
        action
    }

    /// The ambientCG search. A finished download refreshes the tiles and
    /// asks Output to say so.
    fn texture_dialog(&mut self, ctx: &egui::Context) -> Option<BrowserAction> {
        let Some(folder) = self.folder.clone() else {
            self.get_texture = None;
            return None;
        };
        let event = self
            .get_texture
            .as_mut()
            .and_then(|dialog| dialog.show(ctx, &folder))?;
        let action = event.notice.map(|(error, text)| {
            if !error {
                self.refresh();
            }
            BrowserAction::Notice { error, text }
        });
        if event.close {
            self.get_texture = None;
        }
        action
    }

    /// Draws one asset: a tile, or a row of the list when `list`. The first
    /// bool is a double-click (place); the second is a foliage pick.
    fn item(
        &mut self,
        ui: &mut egui::Ui,
        entry: &AssetEntry,
        can_edit: bool,
        list: bool,
        foliage: Option<&FoliageSource>,
    ) -> (bool, bool, Option<ItemMenu>) {
        let size = if list {
            egui::vec2(ui.available_width(), LIST_ROW)
        } else {
            TILE
        };
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
        let id = response.id;
        let foliage_kind = matches!(entry.kind, AssetKind::Prop | AssetKind::Model);
        let selected = foliage.is_some_and(|source| foliage_matches(source, entry));
        let global = self.is_global(&entry.path);
        let mut hover = if foliage.is_some() && foliage_kind {
            format!("{}\nClick to paint this as foliage.", entry.path)
        } else {
            match entry.prop() {
                Some(prop) => format!("{}\n{}", prop.hover, entry.kind.hover()),
                None => format!("{}\n{}", entry.path, entry.kind.hover()),
            }
        };
        if global && !self.is_local(&entry.path) {
            hover.push_str("\nGlobal asset: copied into this project the first time it's used.");
        } else if global {
            hover.push_str("\nGlobal asset: in every project.");
        }
        // Only tiles on screen ask for their thumbnail, so a long list
        // loads what's visible first.
        let thumbnail = (ui.is_rect_visible(rect)
            && matches!(
                entry.kind,
                AssetKind::Image | AssetKind::Texture | AssetKind::TextureSet | AssetKind::Particle
            ))
        .then(|| self.thumbnail_of(ui.ctx(), entry))
        .flatten();
        // A picture's tooltip shows it larger, to see the whole texture.
        let response = match &thumbnail {
            Some(texture) => response.on_hover_ui(|ui| {
                ui.add(egui::Image::new(texture).max_size(egui::Vec2::splat(HOVER_PICTURE)));
                ui.label(hover);
            }),
            None => response.on_hover_text(hover),
        };
        if can_edit && response.drag_started() {
            egui::DragAndDrop::set_payload(ui.ctx(), AssetPayload(entry.clone()));
        }
        let palette = theme::palette(ui.ctx());
        let hover = theme::ease(
            ui.ctx(),
            id.with("hover"),
            response.hovered() || response.dragged(),
        );
        let painter = ui.painter();
        painter.rect_filled(rect, 6.0, palette.control_hover.gamma_multiply(hover));
        let picked = self.picked.contains(&entry.path);
        if picked {
            painter.rect_filled(rect, 6.0, palette.control_active);
        }
        if response.has_focus() || selected || picked {
            painter.rect_stroke(
                rect,
                6.0,
                egui::Stroke::new(1.0_f32, palette.accent),
                egui::StrokeKind::Inside,
            );
        }
        let picture = if list {
            egui::Rect::from_center_size(
                egui::pos2(rect.min.x + 6.0 + LIST_PICTURE / 2.0, rect.center().y),
                egui::vec2(LIST_PICTURE, LIST_PICTURE),
            )
        } else {
            egui::Rect::from_center_size(
                egui::pos2(rect.center().x, rect.min.y + 6.0 + PICTURE / 2.0),
                egui::vec2(PICTURE, PICTURE),
            )
        };
        self.paint_picture(ui, picture, entry, thumbnail.as_ref());
        if global {
            global_badge(ui, picture);
        }
        let text_color = if response.hovered() {
            palette.text_strong
        } else {
            palette.text
        };
        if list {
            // The kind at the right when there's room, then the name in
            // what's left, ending in "…" if it's too long.
            let mut name_right = rect.max.x - 6.0;
            if rect.width() >= 200.0 {
                let kind = ui.painter().text(
                    rect.right_center() - egui::vec2(8.0, 0.0),
                    egui::Align2::RIGHT_CENTER,
                    entry.kind.label(),
                    egui::FontId::proportional(theme::type_scale::SMALL),
                    palette.text_weak,
                );
                name_right = kind.min.x - 8.0;
            }
            let left = picture.max.x + 8.0;
            let galley = widgets::elided(
                ui,
                entry.name.clone(),
                egui::FontId::proportional(theme::type_scale::BODY),
                text_color,
                (name_right - left).max(1.0),
                1,
            );
            ui.painter().galley(
                egui::pos2(left, rect.center().y - galley.size().y / 2.0),
                galley,
                text_color,
            );
        } else {
            // Long names show two lines, then "…".
            let galley = widgets::elided(
                ui,
                entry.name.clone(),
                egui::FontId::proportional(theme::type_scale::SMALL),
                text_color,
                TILE.x - 8.0,
                2,
            );
            ui.painter().galley(
                egui::pos2(rect.center().x - galley.size().x / 2.0, picture.max.y + 5.0),
                galley,
                text_color,
            );
        }

        // While dragged, a copy of the picture follows the mouse.
        if response.dragged()
            && let Some(pointer) = ui.ctx().pointer_latest_pos()
        {
            let layer = egui::LayerId::new(egui::Order::Tooltip, id.with("dragged"));
            let ghost = egui::Rect::from_center_size(
                pointer + egui::vec2(18.0, 18.0),
                egui::vec2(40.0, 40.0),
            );
            let painter = ui.ctx().layer_painter(layer);
            painter.rect_filled(ghost.expand(3.0), 6.0, ui.visuals().window_fill);
            match &thumbnail {
                Some(texture) => {
                    painter.image(texture.id(), ghost, uv(), egui::Color32::WHITE);
                }
                None => {
                    painter.text(
                        ghost.center(),
                        egui::Align2::CENTER_CENTER,
                        icon_of(entry),
                        egui::FontId::proportional(28.0),
                        entry.kind.color(ui.visuals()),
                    );
                }
            }
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        }
        let mut menu = None;
        // Built-in props are in every project already.
        if entry.kind != AssetKind::Prop {
            let path = entry.path.clone();
            let name = entry.name.clone();
            let local = self.is_local(&path);
            let prefab = entry.kind == AssetKind::Prefab;
            // The picked assets if this is one of them, or else just this.
            let export = if picked {
                self.picked.clone()
            } else {
                vec![path.clone()]
            };
            // Only the project's own files: built-in props and global
            // assets it hasn't got have none to delete.
            let deletable: Vec<String> = export
                .iter()
                .filter(|path| self.is_local(path))
                .cloned()
                .collect();
            response.context_menu(|ui| {
                if let Some(picked) = self.global_checkbox(ui, &path) {
                    menu = Some(picked);
                }
                if local && let Some(chosen) = export_package_button(ui, &name, &export) {
                    menu = Some(chosen);
                }
                if !can_edit || !local {
                    return;
                }
                ui.separator();
                if prefab {
                    if ui
                        .button(format!("{} Rename", icons::PENCIL_SIMPLE))
                        .clicked()
                    {
                        menu = Some(ItemMenu::Rename {
                            path: path.clone(),
                            name: name.clone(),
                        });
                        ui.close();
                    }
                    if ui.button(format!("{} Duplicate", icons::COPY)).clicked() {
                        menu = Some(ItemMenu::Duplicate { path: path.clone() });
                        ui.close();
                    }
                }
                if let Some(chosen) = delete_button(ui, &name, &deletable) {
                    menu = Some(chosen);
                }
            });
        }
        let pick = can_edit && foliage.is_some() && foliage_kind && response.clicked();
        let place = can_edit && !pick && response.double_clicked();
        if response.clicked() && !pick {
            let modifiers = ui.input(|input| input.modifiers);
            self.pick_click(&entry.path, modifiers.command || modifiers.ctrl);
        }
        (place, pick, menu)
    }

    fn paint_picture(
        &self,
        ui: &egui::Ui,
        picture: egui::Rect,
        entry: &AssetEntry,
        thumbnail: Option<&egui::TextureHandle>,
    ) {
        let painter = ui.painter();
        let palette = theme::palette(ui.ctx());
        painter.rect(
            picture,
            5.0,
            palette.inset,
            egui::Stroke::new(1.0_f32, palette.border),
            egui::StrokeKind::Inside,
        );
        let side = picture.width();
        match thumbnail {
            Some(texture) => {
                // Fitted inside the square, keeping its shape.
                let [w, h] = texture.size().map(|n| n as f32);
                let scale = (side / w).min(side / h);
                let fitted =
                    egui::Rect::from_center_size(picture.center(), egui::vec2(w, h) * scale);
                painter.image(texture.id(), fitted, uv(), egui::Color32::WHITE);
            }
            None => {
                painter.text(
                    picture.center(),
                    egui::Align2::CENTER_CENTER,
                    icon_of(entry),
                    egui::FontId::proportional(side * 0.47),
                    entry.kind.color(ui.visuals()),
                );
            }
        }
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

/// A category to filter by: quiet text, filled while it's picked.
fn chip(ui: &mut egui::Ui, label: &str, selected: bool) -> egui::Response {
    let palette = theme::palette(ui.ctx());
    let galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        egui::FontId::new(theme::type_scale::SMALL + 0.5, theme::medium_family()),
        egui::Color32::PLACEHOLDER,
    );
    let size = egui::vec2(galley.size().x + 16.0, 24.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::RadioButton, true, selected, label)
    });
    widgets::paint_hover(ui, rect, &response, false, 12.0);
    if selected {
        ui.painter().rect_filled(rect, 12.0, palette.control_active);
    }
    let color = if selected || response.hovered() {
        palette.text_strong
    } else {
        palette.text_weak
    };
    ui.painter()
        .galley(rect.center() - galley.size() / 2.0, galley, color);
    response
}

#[cfg(test)]
mod tests;
