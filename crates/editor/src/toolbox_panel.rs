//! The Toolbox tab: find assets on ambientCG, Poly Haven and Openverse,
//! keep them in the central asset directory, and get them into the project
//! through the editor's own paths (a texture set, a sky, a sound, or a
//! model through the Import dialog).
//!
//! The tab sits beside Assets and Console (the Assets panel's Toolbox
//! button opens it). Kind tabs pick Textures, Skies, Models or Sounds; the
//! view menu shows search results or the library's own lists (favourites,
//! recently used, everything downloaded, what this project uses, a
//! collection). Clicking a card opens its details drawer: who made it, its
//! licence in plain words, its packages and sizes, whether it's in the
//! library or the project, and Get, Favourite, Add to collection, Copy
//! credit line and, for textures, Make material.
//!
//! Textures and skies search every source that has them at once ("All
//! sources", the default): each source is asked on its own thread, the
//! answers are interleaved so no source buries another, a result already
//! shown is not shown twice, and a source that fails or times out is named
//! above the results while the others' still show. Each card says where it
//! came from, and source chips narrow the results without searching again.
//!
//! The Sounds tab shows a licence notice above its results: Openverse
//! gathers sounds from many sites under many licences, so each card keeps
//! its own licence and the notice says to check it before use.
//!
//! Searches need a network connection. Results show as soon as the sources
//! answer; their previews arrive afterwards, fetched by a few threads at
//! once, and are kept for the panel's life. A download runs on its own
//! thread: a package is fetched once into the central asset directory
//! (verified, and recorded with its source and licence in the metadata
//! database) and copied into each project from there, so a second project
//! gets it without downloading. Cancelling leaves the project as it was.
//! Every install is recorded in the database and the project's
//! `.axcenity/asset-sources.json` (`install.rs`), which Build Game's
//! `CREDITS.txt` and the Assets panel's tooltips read.

use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axcenity_assets::{
    AUDIO_EXTENSIONS, PROJECT_AUDIO_FOLDER, PROJECT_SKY_FOLDER, PROJECT_TEXTURES_FOLDER,
    TEXTURE_EXTENSIONS, TEXTURE_SET_SUFFIX, texture_set_from_files, write_texture_set,
};
use axcenity_core::InstanceId;
use axcenity_toolbox::ambientcg::AMBIENTCG;
use axcenity_toolbox::db::{self, Database, Install};
use axcenity_toolbox::directory::{self, AssetDirectory};
use axcenity_toolbox::download::Fetcher;
use axcenity_toolbox::http::{self, Http};
use axcenity_toolbox::variants::{self, Choice};
use axcenity_toolbox::{
    AssetRecord, AssetSource, Kind, Package, Query, SearchPage, SourceError, Variant, files,
    license, provenance,
};
use eframe::egui;

use crate::theme::{self, icons};
use crate::{responsive, widgets};

mod drawer;
pub mod install;
mod packs;
#[cfg(test)]
mod search_tests;
mod views;

pub(crate) use drawer::size_text;
use install::{DownloadResult, Installed, download};
pub use views::View;

const PAGE: usize = 24;
const RESOLUTIONS: [&str; 4] = ["1K", "2K", "4K", "8K"];
const FORMATS: [&str; 2] = ["JPG", "PNG"];
const PREVIEW_LIMIT: u64 = 2 * 1024 * 1024;
/// How many previews are fetched at once.
const PREVIEW_FETCHERS: usize = 6;
/// A result card's width and height, and its picture's side.
const CARD: egui::Vec2 = egui::vec2(124.0, 178.0);
const CARD_PICTURE: f32 = 112.0;
/// The largest a preview is shown when its card is hovered.
const HOVER_PREVIEW: f32 = 256.0;
/// From this width the details drawer sits beside the results.
const DRAWER_BESIDE_FROM: f32 = 620.0;
/// The Sounds tab's licence notice (Paul's wording, 2026-11-11).
pub const SOUND_LICENCE_NOTICE: &str = "Sound assets obtained through this provider are not \
    licensed for commercial use unless their individual source licenses explicitly permit it. \
    Always review the original asset license before using an asset in a project.";

/// Which sources a search asks.
#[derive(Clone, Copy)]
enum Sources {
    /// Every source that has the tab's kind.
    All,
    One(&'static dyn AssetSource),
}

impl Sources {
    /// The sources to ask for `kind`.
    fn list(self, kind: Kind) -> Vec<&'static dyn AssetSource> {
        match self {
            Sources::All => axcenity_toolbox::sources_for(kind),
            Sources::One(source) => vec![source],
        }
    }

    /// The menu's text: "All sources" or the source's name.
    fn label(self) -> &'static str {
        match self {
            Sources::All => "All sources",
            Sources::One(source) => source.info().name,
        }
    }

    /// Whether this is `other` (two `All`s, or the same source).
    fn is(self, other: Sources) -> bool {
        match (self, other) {
            (Sources::All, Sources::All) => true,
            (Sources::One(a), Sources::One(b)) => a.info().key == b.info().key,
            _ => false,
        }
    }
}

/// Where a texture got for a purpose goes once it's in the project.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// Terrain Studio's "From Toolbox…" for this layer.
    TerrainLayer(InstanceId),
}

/// What the editor should do after the panel drew.
#[derive(Debug, PartialEq)]
pub enum ToolboxAction {
    /// A sentence for the Console; `error` for a failure.
    Notice { error: bool, text: String },
    /// A texture set is in the project at `path` (its `.texture.json`);
    /// with a target, fill it.
    TextureSet {
        path: String,
        name: String,
        target: Option<Target>,
    },
    /// A sky is in the project: ask whether to use it.
    AskSky(String),
    /// A sound is in the project; the Assets panel should show it.
    Sound(String),
    /// A model is in the asset directory: open the Import dialog on it, and
    /// record the install under `key` once it's imported.
    ImportModel {
        file: PathBuf,
        key: String,
        variant: String,
    },
    /// Write a material file from the texture set at `set` and open it.
    MakeMaterial { set: String, name: String },
    /// Show the Downloaded / Imported Assets tab.
    OpenInventory,
}

#[derive(Clone)]
pub(crate) struct Listing {
    name: String,
    category: String,
    maps: Vec<String>,
    preview: Option<String>,
    /// The packages Get can install for its kind, such as "2K JPG": one zip
    /// each from ambientCG, loose maps from Poly Haven, a glTF bundle.
    packages: Vec<Variant>,
    /// The source lists its packages only in the full record (Poly Haven),
    /// so Get looks them up first.
    lookup: bool,
    /// The source's record, kept in the metadata database on download.
    record: AssetRecord,
}

impl Listing {
    /// A search result from one of the Toolbox's sources, or a record from
    /// the library.
    pub(crate) fn from_record(record: AssetRecord) -> Self {
        let kept = record.clone();
        // Poly Haven and Openverse list files only in the full record.
        let lookup = record.variants.is_empty() && record.provider != AMBIENTCG.info().key;
        let kind = record.kind;
        Self {
            record: kept,
            lookup,
            category: record.categories.first().cloned().unwrap_or_default(),
            maps: record
                .maps
                .iter()
                .map(|map| map_label(map).to_owned())
                .collect(),
            preview: Some(record.thumbnail_url).filter(|url| url.starts_with("https://")),
            packages: record
                .variants
                .into_iter()
                .filter(|variant| usable(kind, variant))
                .collect(),
            name: record.name,
        }
    }

    fn key(&self) -> String {
        self.record.key()
    }
}

/// What the panel offers for one kind of asset.
struct KindRules {
    /// The tab's label and the plural in messages.
    label: &'static str,
    noun: &'static str,
    /// The format menu, in order of preference when one is missing.
    formats: &'static [&'static str],
    /// Whether the resolution menu applies.
    resolution: bool,
    /// Files taken out of a zip package.
    extract: &'static [&'static str],
    /// Licences searched for (empty: any). Sounds have their own licences;
    /// the default keeps the ones a game can use with at most a credit.
    licenses: &'static [&'static str],
}

const SKY_EXTENSIONS: [&str; 2] = ["hdr", "exr"];
/// What a model package may bring: the model, its buffers and textures.
const MODEL_EXTENSIONS: [&str; 7] = ["gltf", "glb", "bin", "fbx", "png", "jpg", "jpeg"];

fn rules(kind: Kind) -> KindRules {
    match kind {
        Kind::Hdri => KindRules {
            label: "Skies",
            noun: "skies",
            formats: &["HDR", "EXR"],
            resolution: true,
            extract: &SKY_EXTENSIONS,
            licenses: &[],
        },
        Kind::Sound => KindRules {
            label: "Sounds",
            noun: "sounds",
            formats: &["MP3", "OGG", "WAV", "FLAC"],
            resolution: false,
            extract: &AUDIO_EXTENSIONS,
            licenses: &[license::CC0, license::PD, license::CC_BY],
        },
        Kind::Model => KindRules {
            label: "Models",
            noun: "models",
            formats: &["GLTF", "FBX"],
            resolution: true,
            extract: &MODEL_EXTENSIONS,
            licenses: &[],
        },
        Kind::Texture | Kind::Sprites => KindRules {
            label: "Textures",
            noun: "materials",
            formats: &FORMATS,
            resolution: true,
            extract: &TEXTURE_EXTENSIONS,
            licenses: &[],
        },
    }
}

/// The kinds the panel has tabs for, in order.
const KINDS: [Kind; 4] = [Kind::Texture, Kind::Hdri, Kind::Model, Kind::Sound];

/// Whether Get can install a package of a `kind` asset: for textures one
/// zip or loose maps of JPG or PNG images (the formats the engine reads);
/// for skies one HDR or EXR file, or a zip of them; for models a glTF or
/// FBX bundle, or a zip; for sounds one file the engine plays.
fn usable(kind: Kind, variant: &Variant) -> bool {
    let format_known = rules(kind)
        .formats
        .iter()
        .any(|format| variant.format.eq_ignore_ascii_case(format));
    let shape = match variant.package {
        Package::Archive => variant.files.len() == 1 && kind != Kind::Sound,
        Package::Maps => !variant.files.is_empty() && kind == Kind::Texture,
        Package::Single => variant.files.len() == 1 && !matches!(kind, Kind::Texture | Kind::Model),
        Package::Bundle => !variant.files.is_empty() && kind == Kind::Model,
    };
    // ambientCG labels its HDRI and model zips by resolution ("4K",
    // "LQ-2K-JPG"), so what's in them is known only once they're unpacked.
    let unlabelled_zip = variant.package == Package::Archive
        && (kind == Kind::Model || (kind == Kind::Hdri && variant.format.is_empty()));
    shape && (format_known || unlabelled_zip)
}

/// Leaves out loose maps the texture set doesn't need: a DirectX normal
/// map when there's an OpenGL one, and a packed AO/roughness/metalness map
/// when the separate maps are there.
fn needed_maps(variant: &Variant) -> Variant {
    let has = |map: &str| {
        variant
            .files
            .iter()
            .any(|file| file.map.as_deref() == Some(map))
    };
    let mut variant = variant.clone();
    if variant.package == Package::Maps {
        let skip_dx = has("normal");
        let skip_arm = has("ao") && has("roughness");
        variant.files.retain(|file| match file.map.as_deref() {
            Some("normal_dx") => !skip_dx,
            Some("arm") => !skip_arm,
            _ => true,
        });
    }
    variant
}

/// A source's id as a folder name: letters, digits, `_` and `-`.
fn folder_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// How a map's name reads on a card.
fn map_label(map: &str) -> &str {
    match map {
        "base_color" => "color",
        "normal" | "normal_dx" => "normal",
        "ao" => "occlusion",
        "height" => "displacement",
        "metallic" => "metalness",
        other => other,
    }
}

struct Page {
    listings: Vec<Listing>,
    /// The search page last asked of each source (from 1).
    number: u32,
    /// The sources that may have more after it, by key.
    more: Vec<&'static str>,
}

struct SearchResult {
    generation: u64,
    /// The page asked for, from 1.
    number: u32,
    found: Found,
}

/// What one search of one or more sources found.
#[derive(Default)]
struct Found {
    /// Interleaved, a source at a time.
    listings: Vec<Listing>,
    /// The sources that may have another page, by key.
    more: Vec<&'static str>,
    /// "Poly Haven: the connection timed out", for each source that failed
    /// while another answered.
    failed: Vec<String>,
}

struct Job<T> {
    rx: Receiver<Result<T, String>>,
}

/// A fetched preview: the asset's key, and `None` if it couldn't be fetched.
type Preview = (String, Option<egui::ColorImage>);

/// What one result card shows.
struct Card {
    key: String,
    name: String,
    detail: String,
    /// Its licence keeps it out of a commercial game.
    blocked: Option<&'static str>,
    /// What Get would fetch ("Gets 2K JPG."), or `None` with no download.
    plan: Option<String>,
}

/// The Toolbox tab.
pub struct ToolboxPanel {
    /// The tab: textures, skies, models or sounds.
    kind: Kind,
    /// Where they come from: every source, or ambientCG, Poly Haven or
    /// Openverse.
    sources: Sources,
    /// The one source whose results are shown (the source chips), when
    /// searching all of them.
    only: Option<&'static str>,
    /// The sources that failed in the last search while others answered.
    partial: Option<String>,
    /// Search results or one of the library's lists.
    view: View,
    query: String,
    resolution: String,
    format: String,
    /// Shows sounds whose licence needs care (NC, ND, SA, unknown).
    restricted: bool,
    page: Option<Page>,
    error: Option<String>,
    status: Option<String>,
    search: Option<Job<SearchResult>>,
    download: Option<Job<DownloadResult>>,
    generation: u64,
    cancel: Arc<AtomicBool>,
    /// By asset key: `None` while fetching.
    previews: HashMap<String, Option<egui::TextureHandle>>,
    /// Assets whose preview couldn't be fetched, shown blank.
    no_preview: HashSet<String>,
    preview_tx: Sender<Preview>,
    preview_rx: Receiver<Preview>,
    /// The card whose details are open.
    drawer: Option<drawer::Drawer>,
    /// Where the next texture got goes (Terrain Studio's From Toolbox…).
    target: Option<Target>,
    /// The project the library views were last read for.
    shown_for: Option<PathBuf>,
    /// The collections' names, for the menus.
    collections: Vec<String>,
    /// Searches the popular assets when next drawn (set when it's opened,
    /// so a tab restored with the layout doesn't reach the network).
    search_on_show: bool,
    /// The pack being reviewed (Packs > Import pack…).
    pack: Option<packs::Review>,
    /// The Gets waiting their turn (a pack's Get all).
    queue: VecDeque<Listing>,
    /// An asset to show in its library view, with its details open, when
    /// next drawn (`show_record`).
    reveal: Option<AssetRecord>,
    /// The view menu's Downloaded / Imported Assets… was picked.
    open_inventory: bool,
}

impl Default for ToolboxPanel {
    fn default() -> Self {
        Self::idle()
    }
}

impl ToolboxPanel {
    /// The panel before its first search.
    fn idle() -> Self {
        let (preview_tx, preview_rx) = mpsc::channel();
        Self {
            kind: Kind::Texture,
            sources: Sources::All,
            only: None,
            partial: None,
            view: View::Search,
            query: String::new(),
            resolution: "2K".to_owned(),
            format: "JPG".to_owned(),
            restricted: false,
            page: None,
            error: None,
            status: None,
            search: None,
            download: None,
            generation: 0,
            cancel: Arc::new(AtomicBool::new(false)),
            previews: HashMap::new(),
            no_preview: HashSet::new(),
            preview_tx,
            preview_rx,
            drawer: None,
            target: None,
            shown_for: None,
            collections: Vec::new(),
            search_on_show: false,
            pack: None,
            queue: VecDeque::new(),
            reveal: None,
            open_inventory: false,
        }
    }

    /// Switches to textures for `target` (Terrain Studio's From Toolbox…):
    /// the next texture got fills it.
    pub fn aim(&mut self, target: Target) {
        self.target = Some(target);
        if self.kind != Kind::Texture {
            self.set_kind(Kind::Texture);
        }
    }

    /// The tab was opened: show the popular assets if nothing is shown.
    pub fn opened(&mut self) {
        self.search_on_show = true;
    }

    /// Stops a running download; the project is left as it was.
    pub fn cancel(&mut self) {
        self.queue.clear();
        self.cancel.store(true, Ordering::Relaxed);
    }

    /// Draws the tab. `project` is the open project's folder; `can_edit`
    /// is false during Play, when nothing is installed.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        project: Option<&Path>,
        can_edit: bool,
    ) -> Vec<ToolboxAction> {
        let ctx = ui.ctx().clone();
        if self.search.is_some() || self.download.is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        let mut actions = Vec::new();
        self.poll(&ctx, &mut actions);
        if can_edit {
            self.advance_queue(&ctx, project);
        }
        self.load_previews(&ctx);
        if self.search_on_show
            && self.view == View::Search
            && self.page.is_none()
            && self.search.is_none()
        {
            self.start_search(&ctx, 1);
        }
        self.search_on_show = false;
        if self.shown_for.as_deref() != project || self.reveal.is_some() {
            self.shown_for = project.map(Path::to_owned);
            self.refresh_collections();
            if self.view != View::Search {
                self.load_view(&ctx);
            }
        }
        if let Some(record) = self.reveal.take() {
            let key = record.key();
            if self.listing(&key).is_some() {
                self.open_drawer(&ctx, project, &key);
            } else {
                // Not in the library view (its download was just removed).
                self.status = Some(format!(
                    "{} isn't in the library any more. Search for it to get it again.",
                    record.name
                ));
            }
        }
        if std::mem::take(&mut self.open_inventory) {
            actions.push(ToolboxAction::OpenInventory);
        }
        let palette = theme::palette(&ctx);
        self.kind_tabs(ui);
        if self.kind == Kind::Sound {
            sound_licence_notice(ui);
        }
        responsive::toolbar(ui, |ui| {
            self.view_choice(ui);
            responsive::room_for(ui, 60.0);
            self.packs_menu(ui);
            if self.view == View::Search {
                responsive::room_for(ui, 120.0);
                self.source_choice(ui);
                let kind = rules(self.kind);
                responsive::room_for(ui, 120.0);
                let width = ui.available_size_before_wrap().x.clamp(80.0, 160.0);
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.query)
                        .hint_text(format!("Search {}", kind.noun))
                        .desired_width(width),
                );
                let enter =
                    response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
                if (ui
                    .add_enabled(
                        self.search.is_none(),
                        widgets::button(
                            widgets::Role::Secondary,
                            Some(icons::MAGNIFYING_GLASS),
                            "Search",
                            &palette,
                        ),
                    )
                    .clicked()
                    || enter)
                    && self.search.is_none()
                {
                    self.start_search(ui.ctx(), 1);
                }
            }
            let kind = rules(self.kind);
            if kind.resolution {
                responsive::room_for(ui, 70.0);
                self.choice(ui, "toolbox resolution", &RESOLUTIONS, true);
            }
            responsive::room_for(ui, 70.0);
            self.choice(ui, "toolbox format", kind.formats, false);
            if self.kind == Kind::Sound
                && self.view == View::Search
                && ui
                    .checkbox(&mut self.restricted, "Show restricted")
                    .on_hover_text(
                        "Also show sounds whose licence is non-commercial, no-derivatives, \
                         share-alike or unknown. Each is marked with why it needs care.",
                    )
                    .changed()
            {
                self.start_search(ui.ctx(), 1);
            }
        });
        if let Some(target) = self.target {
            ui.horizontal_wrapped(|ui| {
                let Target::TerrainLayer(_) = target;
                ui.label(
                    egui::RichText::new("The next texture you get fills the terrain layer.")
                        .color(palette.accent_text),
                );
                if ui.small_button("Stop").clicked() {
                    self.target = None;
                }
            });
        }
        if let Some(error) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        if let Some(partial) = &self.partial
            && self.view == View::Search
        {
            ui.add(
                egui::Label::new(
                    egui::RichText::new(format!("{} {partial}", icons::WARNING))
                        .color(ui.visuals().warn_fg_color),
                )
                .wrap(),
            );
        }
        if let Some(status) = self.status.clone() {
            ui.horizontal_wrapped(|ui| {
                ui.label(status);
                if self.download.is_some() && ui.small_button("Cancel").clicked() {
                    self.cancel();
                }
            });
        }
        self.pack_review(ui, project, can_edit);
        ui.add_space(4.0);
        let credit = self
            .sources
            .list(self.kind)
            .iter()
            .map(|source| source.info().credit)
            .collect::<Vec<_>>()
            .join(" · ");
        let search_view = self.view == View::Search;
        let open = self.drawer.is_some();
        let width = ui.available_width();
        if open && width >= DRAWER_BESIDE_FROM {
            responsive::split(ui, DRAWER_BESIDE_FROM, 0.62, |ui, pane| {
                if pane == 0 {
                    self.results(ui, project, can_edit);
                } else {
                    self.drawer_pane(ui, project, can_edit, &mut actions);
                }
            });
        } else {
            self.results(ui, project, can_edit);
            if open {
                ui.separator();
                self.drawer_pane(ui, project, can_edit, &mut actions);
            }
        }
        // Where the results come from (Poly Haven's API terms ask for it).
        if search_view {
            ui.label(egui::RichText::new(credit).weak().small());
        }
        actions
    }

    /// Textures, Skies, Models and Sounds.
    fn kind_tabs(&mut self, ui: &mut egui::Ui) {
        let mut picked = None;
        ui.horizontal_wrapped(|ui| {
            for kind in KINDS {
                if ui
                    .selectable_label(self.kind == kind, rules(kind).label)
                    .clicked()
                    && self.kind != kind
                {
                    picked = Some(kind);
                }
            }
        });
        if let Some(kind) = picked
            && self.search.is_none()
        {
            self.set_kind(kind);
            if self.view == View::Search {
                self.start_search(ui.ctx(), 1);
            } else {
                self.load_view(ui.ctx());
            }
        }
    }

    /// Picks another kind: all its sources (or its only one) and its first
    /// format. Searches when the panel is next drawn.
    fn set_kind(&mut self, kind: Kind) {
        let sources = axcenity_toolbox::sources_for(kind);
        let Some(first) = sources.first() else {
            return;
        };
        self.kind = kind;
        self.sources = if sources.len() > 1 {
            Sources::All
        } else {
            Sources::One(*first)
        };
        self.only = None;
        self.format = rules(kind).formats[0].to_owned();
        self.page = None;
        self.drawer = None;
        self.error = None;
    }

    /// The source menu: All sources (when the kind has more than one), then
    /// each source. Picking another searches it.
    fn source_choice(&mut self, ui: &mut egui::Ui) {
        let mut picked = None;
        let sources = axcenity_toolbox::sources_for(self.kind);
        let mut choices: Vec<Sources> = Vec::new();
        if sources.len() > 1 {
            choices.push(Sources::All);
        }
        choices.extend(sources.into_iter().map(Sources::One));
        egui::ComboBox::from_id_salt("toolbox source")
            .selected_text(self.sources.label())
            .show_ui(ui, |ui| {
                for choice in choices {
                    let current = choice.is(self.sources);
                    if ui.selectable_label(current, choice.label()).clicked() && !current {
                        picked = Some(choice);
                    }
                }
            })
            .response
            .on_hover_text("Search every source at once, or just one.");
        if let Some(sources) = picked
            && self.search.is_none()
        {
            self.sources = sources;
            self.start_search(ui.ctx(), 1);
        }
    }

    /// The source chips above results from every source: each source's
    /// count, and a click shows only its results (again to show all).
    fn source_chips(&mut self, ui: &mut egui::Ui) {
        if !matches!(self.sources, Sources::All) || self.view != View::Search {
            return;
        }
        let Some(page) = &self.page else {
            return;
        };
        let counts: Vec<(&'static str, &'static str, usize)> = self
            .sources
            .list(self.kind)
            .iter()
            .map(|source| {
                let info = source.info();
                let count = page
                    .listings
                    .iter()
                    .filter(|listing| listing.record.provider == info.key)
                    .count();
                (info.key, info.name, count)
            })
            .collect();
        let mut picked = None;
        responsive::toolbar(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            if widgets::chip(ui, "All", self.only.is_none()).clicked() {
                picked = Some(None);
            }
            for (key, name, count) in counts {
                let on = self.only == Some(key);
                if widgets::chip(ui, &format!("{name} {count}"), on)
                    .on_hover_text(format!("Show only the results from {name}."))
                    .clicked()
                {
                    picked = Some(if on { None } else { Some(key) });
                }
            }
        });
        if let Some(only) = picked {
            self.only = only;
        }
    }

    fn choice(&mut self, ui: &mut egui::Ui, id: &str, options: &[&str], resolution: bool) {
        let selected = if resolution {
            self.resolution.clone()
        } else {
            self.format.clone()
        };
        egui::ComboBox::from_id_salt(id)
            .selected_text(&selected)
            .width(56.0)
            .show_ui(ui, |ui| {
                for option in options {
                    let value = (*option).to_owned();
                    if resolution {
                        ui.selectable_value(&mut self.resolution, value, *option);
                    } else {
                        ui.selectable_value(&mut self.format, value, *option);
                    }
                }
            });
    }

    fn results(&mut self, ui: &mut egui::Ui, project: Option<&Path>, can_edit: bool) {
        self.source_chips(ui);
        let Some(page) = self.page.as_ref() else {
            if self.search.is_some() {
                ui.label("Searching…");
            } else if self.view == View::Search && self.error.is_none() {
                let whose = match self.sources {
                    Sources::All => "every source".to_owned(),
                    Sources::One(source) => source.info().name.to_owned(),
                };
                ui.weak(format!(
                    "Press Search to see the most popular {} from {whose}, or type what you're \
                     after.",
                    rules(self.kind).noun
                ));
            }
            return;
        };
        if page.listings.is_empty() {
            ui.label(self.view.empty_text(rules(self.kind).noun));
            return;
        }
        let more = !page.more.is_empty();
        let all_sources = matches!(self.sources, Sources::All) && self.view == View::Search;
        let only = self.only.filter(|_| all_sources);
        let cards: Vec<Card> = page
            .listings
            .iter()
            .filter(|listing| only.is_none_or(|key| listing.record.provider == key))
            .map(|listing| {
                let maps = listing.maps.join(", ");
                let record = &listing.record;
                let detail = if record.kind == Kind::Sound {
                    // Each sound has its own licence and author: show both.
                    let author = record
                        .authors
                        .first()
                        .map_or("unknown author", String::as_str);
                    format!(
                        "{} · {author}",
                        license::display(&record.license, &record.license_version)
                    )
                } else if maps.is_empty() {
                    listing.category.clone()
                } else if listing.category.is_empty() {
                    maps
                } else {
                    format!("{} · {maps}", listing.category)
                };
                // Results from several sources say whose each one is.
                let detail = if all_sources {
                    let source = provenance::source_name(&record.provider);
                    if detail.is_empty() {
                        source
                    } else {
                        format!("{source} · {detail}")
                    }
                } else {
                    detail
                };
                Card {
                    key: listing.key(),
                    name: listing.name.clone(),
                    detail,
                    blocked: license::commercial_block(&record.license),
                    plan: self
                        .package(listing)
                        .map(|choice| choice.describe())
                        .or_else(|| {
                            listing.lookup.then(|| {
                                format!("Gets the nearest to {} {}.", self.resolution, self.format)
                            })
                        }),
                }
            })
            .collect();
        let height = (ui.available_height() - 24.0).max(120.0);
        let can_get = can_edit && project.is_some();
        egui::ScrollArea::vertical()
            .id_salt("toolbox results")
            .max_height(height)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
                    for card in &cards {
                        ui.allocate_ui_with_layout(
                            CARD,
                            egui::Layout::top_down(egui::Align::Center),
                            |ui| {
                                ui.set_min_size(CARD);
                                self.card(ui, project, can_get, card);
                            },
                        );
                    }
                });
            });
        if self.view == View::Search
            && more
            && ui
                .add_enabled(self.search.is_none(), egui::Button::new("More"))
                .clicked()
        {
            let next = self.page.as_ref().map_or(1, |page| page.number + 1);
            self.start_search(ui.ctx(), next);
        }
    }

    /// One result: its preview (larger when hovered; click for details),
    /// name, kind and Get.
    fn card(&mut self, ui: &mut egui::Ui, project: Option<&Path>, can_get: bool, card: &Card) {
        ui.spacing_mut().item_spacing.y = 2.0;
        let (rect, response) =
            ui.allocate_exact_size(egui::Vec2::splat(CARD_PICTURE), egui::Sense::click());
        let preview = self.previews.get(&card.key);
        let selected = self
            .drawer
            .as_ref()
            .is_some_and(|drawer| drawer.key == card.key);
        ui.painter()
            .rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);
        match preview {
            Some(Some(texture)) => {
                egui::Image::new(texture)
                    .corner_radius(4.0)
                    .paint_at(ui, rect);
            }
            Some(None) => {
                egui::Spinner::new().paint_at(ui, rect.shrink(CARD_PICTURE / 3.0));
            }
            None => {}
        }
        if selected {
            ui.painter().rect_stroke(
                rect,
                4.0,
                egui::Stroke::new(2.0_f32, ui.visuals().selection.stroke.color),
                egui::StrokeKind::Inside,
            );
        }
        let mut tip = format!("{}\n{}", card.name, card.detail);
        if let Some(why) = card.blocked {
            tip.push_str(&format!("\nNot for a commercial game: {why}."));
        }
        tip.push_str("\nClick for details.");
        let clicked = response.clicked();
        match preview {
            Some(Some(texture)) => {
                let texture = texture.clone();
                response.on_hover_ui(|ui| {
                    ui.add(egui::Image::new(&texture).max_size(egui::Vec2::splat(HOVER_PREVIEW)));
                    ui.label(&tip);
                });
            }
            _ => {
                response.on_hover_text(&tip);
            }
        }
        if clicked {
            self.open_drawer(ui.ctx(), project, &card.key);
        }
        ui.add(egui::Label::new(&card.name).truncate());
        let detail = match card.blocked {
            Some(_) => egui::RichText::new(format!("{} {}", icons::WARNING, card.detail))
                .color(ui.visuals().warn_fg_color)
                .small(),
            None => egui::RichText::new(&card.detail).weak().small(),
        };
        ui.add(egui::Label::new(detail).truncate());
        let button = ui.add_enabled(
            can_get && card.plan.is_some() && self.download.is_none(),
            egui::Button::new("Get"),
        );
        let button = match &card.plan {
            Some(plan) if can_get => button.on_hover_text(plan),
            Some(_) => {
                button.on_disabled_hover_text("Open a project (and stop Play) to get assets.")
            }
            None => button.on_disabled_hover_text(format!("{} has no download.", card.name)),
        };
        if button.clicked()
            && let Some(project) = project
        {
            self.start_download(ui.ctx(), project, &card.key);
        }
    }

    /// The listing with this key, in the page shown.
    fn listing(&self, key: &str) -> Option<&Listing> {
        self.page
            .as_ref()?
            .listings
            .iter()
            .find(|listing| listing.key() == key)
    }

    /// The package Get fetches for `listing`: the chosen resolution and
    /// format, else the nearest there is (`variants::choose`).
    fn package<'a>(&self, listing: &'a Listing) -> Option<Choice<'a>> {
        pick(listing, &self.resolution, &self.format)
    }

    /// Searches page `number` (from 1): the first page anew, a later one
    /// (More) from the sources that said they have more.
    fn start_search(&mut self, ctx: &egui::Context, number: u32) {
        let mut sources = self.sources.list(self.kind);
        if number <= 1 {
            self.generation += 1;
            self.page = None;
            self.only = None;
        } else if let Some(page) = &self.page {
            sources.retain(|source| page.more.contains(&source.info().key));
        }
        self.view = View::Search;
        self.error = None;
        self.partial = None;
        self.status = Some("Searching…".to_owned());
        let generation = self.generation;
        let query = self.query.trim().to_owned();
        let kind = self.kind;
        let restricted = self.restricted;
        let number = number.max(1);
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let client = http::Client::new(Duration::from_secs(30));
            let result = search(&client, &sources, kind, &query, number, restricted).map(|found| {
                SearchResult {
                    generation,
                    number,
                    found,
                }
            });
            let _ = tx.send(result);
            ctx.request_repaint();
        });
        self.search = Some(Job { rx });
    }

    fn start_download(&mut self, ctx: &egui::Context, project: &Path, key: &str) {
        if let Some(listing) = self.listing(key).cloned() {
            self.start_listing_download(ctx, project, &listing);
        }
    }

    fn start_listing_download(&mut self, ctx: &egui::Context, project: &Path, listing: &Listing) {
        // Known now, or (Poly Haven) once the full record is looked up.
        let known = self
            .package(listing)
            .map(|choice| (choice.variant.clone(), choice.notes.join(" ")));
        if known.is_none() && !listing.lookup {
            self.error = Some(format!("{} has no download.", listing.name));
            return;
        }
        let name = listing.name.clone();
        let record = listing.record.clone();
        let project = project.to_owned();
        let Some(source) = axcenity_toolbox::source(&record.provider) else {
            self.error = Some(format!("{name} can't be downloaded from here."));
            return;
        };
        let (resolution, format) = (self.resolution.clone(), self.format.clone());
        self.error = None;
        self.status = Some(match &known {
            Some((variant, notes)) => format!("Downloading {name} ({})… {notes}", variant.label)
                .trim_end()
                .to_owned(),
            None => format!("Downloading {name}…"),
        });
        self.cancel = Arc::new(AtomicBool::new(false));
        let cancel = Arc::clone(&self.cancel);
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let client = http::Client::new(Duration::from_secs(15 * 60));
            let mut notes = String::new();
            let outcome = (|| {
                let (record, variant) = match known {
                    Some((variant, known_notes)) => {
                        notes = known_notes;
                        (record, variant)
                    }
                    None => {
                        let full = source
                            .record(&client, &record.id)
                            .map_err(|err| err.to_string())?;
                        let listing = Listing::from_record(full.clone());
                        let choice = pick(&listing, &resolution, &format).ok_or_else(|| {
                            format!("it has no {} the engine reads", rules(full.kind).noun)
                        })?;
                        notes = choice.notes.join(" ");
                        let variant = choice.variant.clone();
                        (full, variant)
                    }
                };
                let assets = directory::locate().ok_or_else(|| {
                    format!(
                        "there is no asset directory; set {} to a folder",
                        directory::ENV
                    )
                })?;
                let variant = needed_maps(&variant);
                download(
                    &client, source, &assets, &project, &record, &variant, &cancel,
                )
            })();
            let result = match outcome {
                Ok(DownloadResult::Saved { text, installed }) if !notes.is_empty() => {
                    Ok(DownloadResult::Saved {
                        text: format!("{text} {notes}"),
                        installed,
                    })
                }
                Ok(other) => Ok(other),
                Err(err) => Err(format!("Could not get {name}: {err}")),
            };
            let _ = tx.send(result);
            ctx.request_repaint();
        });
        self.download = Some(Job { rx });
    }

    /// Finished searches, look-ups and downloads since last frame.
    fn poll(&mut self, ctx: &egui::Context, actions: &mut Vec<ToolboxAction>) {
        let search = self.search.take();
        if let Some(job) = search {
            match job.rx.try_recv() {
                Ok(Ok(result)) => self.accept_search(ctx, result),
                Ok(Err(err)) => {
                    self.status = None;
                    self.error = Some(format!("Could not search: {err}"));
                }
                Err(TryRecvError::Empty) => self.search = Some(job),
                Err(TryRecvError::Disconnected) => {
                    self.status = None;
                    self.error = Some("Could not search.".to_owned());
                }
            }
        }
        self.poll_drawer();
        let Some(job) = self.download.take() else {
            return;
        };
        match job.rx.try_recv() {
            Ok(Ok(DownloadResult::Saved { text, installed })) => {
                self.status = None;
                actions.push(ToolboxAction::Notice { error: false, text });
                actions.push(match installed {
                    Installed::TextureSet { path, name } => ToolboxAction::TextureSet {
                        path,
                        name,
                        target: self.target.take(),
                    },
                    Installed::Sky(path) => ToolboxAction::AskSky(path),
                    Installed::Sound(path) => ToolboxAction::Sound(path),
                    Installed::Model { file, key, variant } => {
                        ToolboxAction::ImportModel { file, key, variant }
                    }
                });
                self.refresh_drawer();
                if self.view != View::Search {
                    self.load_view(ctx);
                }
            }
            Ok(Ok(DownloadResult::Cancelled)) => {
                self.status = None;
            }
            Ok(Err(err)) => {
                self.status = None;
                self.error = Some(err.clone());
                actions.push(ToolboxAction::Notice {
                    error: true,
                    text: err,
                });
            }
            Err(TryRecvError::Empty) => {
                self.download = Some(job);
            }
            Err(TryRecvError::Disconnected) => {
                self.status = None;
                let err = "Could not download that asset.".to_owned();
                self.error = Some(err.clone());
                actions.push(ToolboxAction::Notice {
                    error: true,
                    text: err,
                });
            }
        }
    }

    fn accept_search(&mut self, ctx: &egui::Context, result: SearchResult) {
        if result.generation != self.generation {
            return;
        }
        self.status = None;
        let Found {
            listings,
            more,
            failed,
        } = result.found;
        if !failed.is_empty() {
            self.partial = Some(format!(
                "Showing the other sources' results. Could not search {}.",
                failed.join("; ")
            ));
        }
        let page = self.page.get_or_insert_with(|| Page {
            listings: Vec::new(),
            number: 0,
            more: Vec::new(),
        });
        let fresh = merge(&mut page.listings, listings);
        page.number = result.number;
        page.more = more;
        self.fetch_previews_for(ctx, &fresh);
    }

    /// Starts fetching the previews of `listings` not seen before.
    fn fetch_previews_for(&mut self, ctx: &egui::Context, listings: &[Listing]) {
        let wanted: VecDeque<(String, String)> = listings
            .iter()
            .filter(|listing| {
                let key = listing.key();
                !self.previews.contains_key(&key) && !self.no_preview.contains(&key)
            })
            .filter_map(|listing| Some((listing.key(), listing.preview.clone()?)))
            .collect();
        for (key, _) in &wanted {
            self.previews.insert(key.clone(), None);
        }
        fetch_previews(ctx, wanted, &self.preview_tx);
    }

    fn load_previews(&mut self, ctx: &egui::Context) {
        for (key, image) in self.preview_rx.try_iter() {
            match image {
                Some(image) => {
                    let handle = ctx.load_texture(
                        format!("toolbox {key}"),
                        image,
                        egui::TextureOptions::LINEAR,
                    );
                    self.previews.insert(key, Some(handle));
                }
                None => {
                    self.previews.remove(&key);
                    self.no_preview.insert(key);
                }
            }
        }
    }
}

/// The asset directory and its database, opened for a moment.
fn library() -> Result<(AssetDirectory, Database), String> {
    let root = directory::locate().ok_or_else(|| {
        format!(
            "there is no asset directory; set {} to a folder",
            directory::ENV
        )
    })?;
    let dir = AssetDirectory::open(&root).map_err(|err| err.to_string())?;
    let db = Database::open(&dir.database_path()).map_err(|err| err.to_string())?;
    Ok((dir, db))
}

/// The package Get fetches for `listing` at a resolution and format such as
/// "2K" and "JPG": that one, else the nearest there is (`variants::choose`).
fn pick<'a>(listing: &'a Listing, resolution: &str, format: &str) -> Option<Choice<'a>> {
    let resolution = resolution.to_ascii_lowercase();
    let format = format.to_ascii_lowercase();
    let fallbacks: Vec<String> = rules(listing.record.kind)
        .formats
        .iter()
        .map(|format| format.to_ascii_lowercase())
        .collect();
    let fallbacks: Vec<&str> = fallbacks.iter().map(String::as_str).collect();
    variants::choose(&listing.packages, &resolution, &format, &fallbacks)
}

/// Page `number` of `kind` assets matching `query` from each of `sources`
/// at once (most popular first; sounds only with the licences `rules`
/// keeps unless `restricted`), interleaved a source at a time. A source
/// that fails is named in [`Found::failed`] while another answered; when
/// every one fails, the search fails with each one's reason.
fn search(
    http: &dyn Http,
    sources: &[&dyn AssetSource],
    kind: Kind,
    query: &str,
    number: u32,
    restricted: bool,
) -> Result<Found, String> {
    let mut wanted = Query::new(query, kind);
    if !restricted {
        wanted.licenses = rules(kind)
            .licenses
            .iter()
            .map(|code| (*code).to_owned())
            .collect();
    }
    wanted.per_page = PAGE as u32;
    wanted.page = number.max(1);
    // Each source on its own thread, so a slow one doesn't hold up the rest.
    let answers: Vec<(&dyn AssetSource, Result<SearchPage, SourceError>)> =
        std::thread::scope(|scope| {
            let asks: Vec<_> = sources
                .iter()
                .map(|&source| {
                    let wanted = &wanted;
                    (source, scope.spawn(move || source.search(http, wanted)))
                })
                .collect();
            asks.into_iter()
                .map(|(source, ask)| {
                    let answer = ask
                        .join()
                        .unwrap_or(Err(SourceError::Refused("the search stopped unexpectedly")));
                    (source, answer)
                })
                .collect()
        });
    let mut found = Found::default();
    let mut errors = Vec::new();
    let mut lists = Vec::new();
    for (source, answer) in answers {
        let info = source.info();
        match answer {
            Ok(page) => {
                let shown = u64::from(wanted.page - 1) * u64::from(wanted.per_page)
                    + page.records.len() as u64;
                if page.more || page.total.is_some_and(|total| total > shown) {
                    found.more.push(info.key);
                }
                lists.push(page.records);
            }
            Err(err) => errors.push(format!("{}: {err}", info.name)),
        }
    }
    if lists.is_empty() && !errors.is_empty() {
        return Err(errors.join("; "));
    }
    found.failed = errors;
    found.listings = interleave(lists)
        .into_iter()
        .map(Listing::from_record)
        .collect();
    Ok(found)
}

/// One list from several, taking the next of each in turn: the first of
/// each, then the second of each, and so on.
fn interleave<T>(lists: Vec<Vec<T>>) -> Vec<T> {
    let mut iters: Vec<_> = lists.into_iter().map(Vec::into_iter).collect();
    let mut out = Vec::new();
    loop {
        let before = out.len();
        for iter in &mut iters {
            out.extend(iter.next());
        }
        if out.len() == before {
            return out;
        }
    }
}

/// Adds `more` to `shown`, leaving out any asset already there (or twice
/// in `more`); returns the ones added. Assets are the same when their
/// source and id are: two sources' assets are never merged, since their
/// files and licences differ.
fn merge(shown: &mut Vec<Listing>, more: Vec<Listing>) -> Vec<Listing> {
    let mut seen: HashSet<String> = shown.iter().map(Listing::key).collect();
    let fresh: Vec<Listing> = more
        .into_iter()
        .filter(|listing| seen.insert(listing.key()))
        .collect();
    shown.extend(fresh.iter().cloned());
    fresh
}

/// The Sounds tab's licence notice: a warning box above the results,
/// which every sound card's own licence badge then refines. Its caret
/// folds it to one line (remembered across restarts) so it doesn't take
/// a narrow column's room; folded, it still says what matters.
fn sound_licence_notice(ui: &mut egui::Ui) {
    let palette = theme::palette(ui.ctx());
    let id = egui::Id::new("toolbox sound licence notice folded");
    let folded = ui
        .ctx()
        .data_mut(|data| data.get_persisted::<bool>(id))
        .unwrap_or(false);
    let mut toggle = false;
    egui::Frame::new()
        .fill(palette.surface)
        .stroke(egui::Stroke::new(1.0_f32, palette.warning))
        .corner_radius(6)
        .inner_margin(egui::Margin::symmetric(8, 5))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                ui.label(egui::RichText::new(icons::WARNING).color(palette.warning));
                let (caret, tip) = if folded {
                    (icons::CARET_DOWN, "Show the whole notice")
                } else {
                    (icons::CARET_UP, "Fold the notice to one line")
                };
                let text = if folded {
                    "Not for commercial use unless a sound's own licence allows it."
                } else {
                    SOUND_LICENCE_NOTICE
                };
                let room = ui.available_width() - ui.spacing().interact_size.y - 4.0;
                let label = egui::Label::new(
                    egui::RichText::new(text)
                        .size(theme::type_scale::SMALL)
                        .color(palette.text_secondary),
                );
                ui.allocate_ui_with_layout(
                    egui::vec2(room.max(0.0), 0.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.add(if folded {
                            label.truncate()
                        } else {
                            label.wrap()
                        });
                    },
                );
                if widgets::icon_button(ui, caret, tip, false).clicked() {
                    toggle = true;
                }
            });
        });
    if toggle {
        ui.ctx().data_mut(|data| data.insert_persisted(id, !folded));
    }
    ui.add_space(2.0);
}

/// Fetches each `(id, url)` in `wanted` on a few threads that share one
/// connection pool, sending each preview to `tx` as soon as it's ready.
/// They stop early once the dialog (the receiver) is gone.
fn fetch_previews(ctx: &egui::Context, wanted: VecDeque<(String, String)>, tx: &Sender<Preview>) {
    if wanted.is_empty() {
        return;
    }
    let client = http::Client::new(Duration::from_secs(20));
    let fetchers = PREVIEW_FETCHERS.min(wanted.len());
    let queue = Arc::new(Mutex::new(wanted));
    for _ in 0..fetchers {
        let queue = Arc::clone(&queue);
        let client = client.clone();
        let tx = tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            loop {
                let next = queue.lock().ok().and_then(|mut queue| queue.pop_front());
                let Some((id, url)) = next else {
                    return;
                };
                let image = preview_image(&client, &url);
                if tx.send((id, image)).is_err() {
                    return;
                }
                ctx.request_repaint();
            }
        });
    }
}

fn preview_image(client: &http::Client, url: &str) -> Option<egui::ColorImage> {
    let bytes = http::get_bytes(client, url, PREVIEW_LIMIT).ok()?;
    let image = image::load_from_memory(&bytes).ok()?.to_rgba8();
    Some(egui::ColorImage::from_rgba_unmultiplied(
        [image.width() as usize, image.height() as usize],
        image.as_raw(),
    ))
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::install::sound_name;
    use std::path::PathBuf;

    use axcenity_toolbox::RemoteFile;
    use axcenity_toolbox::http::Fixture;
    use axcenity_toolbox::openverse::OPENVERSE;
    use axcenity_toolbox::polyhaven::POLYHAVEN;

    use super::*;

    fn zip(label: &str, resolution: &str, format: &str) -> Variant {
        Variant {
            label: label.to_owned(),
            resolution: Some(resolution.to_owned()),
            format: format.to_owned(),
            package: Package::Archive,
            files: vec![RemoteFile {
                url: format!("https://ambientcg.com/get?file=Fabric086_{resolution}-{format}.zip"),
                path: format!("Fabric086_{resolution}-{format}.zip"),
                ..RemoteFile::default()
            }],
        }
    }

    fn record(variants: Vec<Variant>) -> AssetRecord {
        AssetRecord {
            provider: "ambientcg".into(),
            id: "Fabric086".into(),
            name: "Fabric 086".into(),
            url: "https://ambientcg.com/a/Fabric086".into(),
            license: "CC0".into(),
            thumbnail_url: "https://example.com/Fabric086.png".into(),
            maps: vec!["base_color".into(), "normal".into(), "ao".into()],
            variants,
            ..AssetRecord::default()
        }
    }

    /// A folder under the temp directory, removed when dropped.
    struct Temp(PathBuf);

    impl Temp {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "axcenity-get-texture-{name}-{}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&path);
            Self(path)
        }
    }

    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn package_zip() -> Vec<u8> {
        let mut cursor = std::io::Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut cursor);
            let options = zip::write::SimpleFileOptions::default();
            for name in [
                "Fabric086_2K-JPG_Color.jpg",
                "Fabric086_2K-JPG_NormalGL.jpg",
                "Fabric086.usdc",
            ] {
                writer.start_file(name, options).unwrap();
                writer.write_all(name.as_bytes()).unwrap();
            }
            writer.finish().unwrap();
        }
        cursor.into_inner()
    }

    #[test]
    fn a_package_is_downloaded_once_and_installed_into_each_project() {
        let temp = Temp::new("twice");
        let assets = temp.0.join("Assets");
        let variant = zip("2K JPG", "2k", "jpg");
        let record = record(vec![variant.clone()]);
        let http = Fixture::default().with(&variant.files[0].url, package_zip());
        let cancel = AtomicBool::new(false);

        let first = temp.0.join("First");
        let saved = download(
            &http, &AMBIENTCG, &assets, &first, &record, &variant, &cancel,
        )
        .unwrap();
        assert!(
            matches!(&saved, DownloadResult::Saved { text, .. } if text.starts_with("Saved Fabric 086"))
        );
        let folder = first.join("assets").join("textures").join("Fabric086");
        assert!(folder.join("Fabric086.texture.json").is_file());
        assert!(folder.join("Fabric086_2K-JPG_Color.jpg").is_file());
        assert!(!folder.join("Fabric086.usdc").exists());

        // The second project is filled from the asset directory: nothing is fetched.
        let second = temp.0.join("Second");
        download(
            &Fixture::default(),
            &AMBIENTCG,
            &assets,
            &second,
            &record,
            &variant,
            &cancel,
        )
        .unwrap();
        assert!(
            second
                .join("assets/textures/Fabric086/Fabric086.texture.json")
                .is_file()
        );

        let dir = AssetDirectory::open(&assets).unwrap();
        let db = Database::open(&dir.database_path()).unwrap();
        let stored = db.asset("ambientcg:Fabric086").unwrap().unwrap();
        assert_eq!(stored.license, "CC0");
        assert!(
            db.download("ambientcg:Fabric086", "2K JPG")
                .unwrap()
                .is_some()
        );
        let installs = db.installs(&second.display().to_string()).unwrap();
        assert_eq!(installs.len(), 1);
        assert_eq!(installs[0].path, "assets/textures/Fabric086");
    }

    #[test]
    fn a_failed_download_leaves_the_project_alone() {
        let temp = Temp::new("failed");
        let variant = zip("2K JPG", "2k", "jpg");
        let record = record(vec![variant.clone()]);
        let project = temp.0.join("Project");
        let cancel = AtomicBool::new(false);
        let err = download(
            &Fixture::default(),
            &AMBIENTCG,
            &temp.0.join("Assets"),
            &project,
            &record,
            &variant,
            &cancel,
        )
        .err()
        .unwrap();
        assert!(err.contains("no asset with that id"), "{err}");
        assert!(!project.exists());
    }

    /// The whole Get path against the real ambientCG: search, choose the
    /// nearest to 1K JPG, download into a temporary asset directory, unpack
    /// and save the texture set. Reaches the network, so run it by hand:
    /// `cargo nextest run -p axcenity-editor --run-ignored only -E 'test(toolbox_panel)'`
    #[test]
    #[ignore = "reaches ambientCG"]
    fn get_downloads_a_real_material_into_a_project() {
        let temp = Temp::new("live");
        let client = http::Client::new(Duration::from_secs(30));
        let found = search(&client, &[&AMBIENTCG], Kind::Texture, "ground", 1, false).unwrap();
        let listing = found
            .listings
            .into_iter()
            .find(|l| !l.packages.is_empty())
            .unwrap();
        let choice = variants::choose(&listing.packages, "1k", "jpg", &["png"]).unwrap();
        let project = temp.0.join("Project");
        let cancel = AtomicBool::new(false);
        let client = http::Client::new(Duration::from_secs(600));
        let saved = download(
            &client,
            &AMBIENTCG,
            &temp.0.join("Assets"),
            &project,
            &listing.record,
            choice.variant,
            &cancel,
        );
        assert!(
            matches!(saved, Ok(DownloadResult::Saved { .. })),
            "{:?}",
            saved.err()
        );
        let folder = project
            .join("assets")
            .join("textures")
            .join(&listing.record.id);
        assert!(
            folder
                .join(format!("{}{TEXTURE_SET_SUFFIX}", listing.record.id))
                .is_file()
        );
    }

    #[test]
    fn a_listing_keeps_only_single_zip_downloads_and_names_its_maps() {
        let mut single = zip("8K EXR", "8k", "exr");
        single.package = Package::Single;
        let listing = Listing::from_record(record(vec![zip("2K JPG", "2k", "jpg"), single]));
        assert_eq!(listing.record.id, "Fabric086");
        assert_eq!(listing.record.url, "https://ambientcg.com/a/Fabric086");
        assert_eq!(listing.maps, ["color", "normal", "occlusion"]);
        assert_eq!(listing.packages.len(), 1);
        assert!(listing.preview.is_some());
    }

    #[test]
    fn get_offers_the_nearest_package_when_the_chosen_one_is_missing() {
        let mut dialog = ToolboxPanel::idle();
        let listing = Listing::from_record(record(vec![
            zip("1K JPG", "1k", "jpg"),
            zip("4K PNG", "4k", "png"),
        ]));
        dialog.resolution = "2K".into();
        dialog.format = "JPG".into();
        let choice = dialog.package(&listing).unwrap();
        assert_eq!(choice.variant.label, "1K JPG");
        assert_eq!(choice.notes, ["No 2K here, so 1K, the nearest."]);
        assert_eq!(
            choice.describe(),
            "Gets 1K JPG. No 2K here, so 1K, the nearest."
        );
        dialog.format = "PNG".into();
        assert_eq!(dialog.package(&listing).unwrap().variant.label, "4K PNG");
        let empty = Listing::from_record(record(Vec::new()));
        assert!(dialog.package(&empty).is_none());
    }

    fn poly_haven_maps() -> (AssetRecord, Variant) {
        let map = |name: &str, map: &str| RemoteFile {
            url: format!("https://dl.polyhaven.org/file/{name}"),
            path: name.to_owned(),
            size: Some(name.len() as u64),
            md5: None,
            map: Some(map.to_owned()),
        };
        let variant = Variant {
            label: "2K JPG".into(),
            resolution: Some("2k".into()),
            format: "jpg".into(),
            package: Package::Maps,
            files: vec![
                map("red_brick_03_diff_2k.jpg", "base_color"),
                map("red_brick_03_nor_dx_2k.jpg", "normal_dx"),
                map("red_brick_03_nor_gl_2k.jpg", "normal"),
                map("red_brick_03_rough_2k.jpg", "roughness"),
                map("red_brick_03_ao_2k.jpg", "ao"),
                map("red_brick_03_arm_2k.jpg", "arm"),
            ],
        };
        let record = AssetRecord {
            provider: "polyhaven".into(),
            id: "red_brick_03".into(),
            name: "Red Brick 03".into(),
            url: "https://polyhaven.com/a/red_brick_03".into(),
            license: "CC0".into(),
            ..AssetRecord::default()
        };
        (record, variant)
    }

    #[test]
    fn a_poly_haven_result_is_looked_up_before_get() {
        let (record, _) = poly_haven_maps();
        let listing = Listing::from_record(record);
        assert!(listing.lookup);
        assert!(listing.packages.is_empty());
        let mut dialog = ToolboxPanel::idle();
        dialog.page = Some(Page {
            listings: vec![listing],
            number: 1,
            more: Vec::new(),
        });
        assert!(
            dialog
                .package(&dialog.page.as_ref().unwrap().listings[0])
                .is_none()
        );
    }

    #[test]
    fn poly_haven_maps_install_as_a_texture_set_without_the_spare_maps() {
        let temp = Temp::new("polyhaven");
        let (record, variant) = poly_haven_maps();
        let variant = needed_maps(&variant);
        let kept: Vec<_> = variant
            .files
            .iter()
            .filter_map(|f| f.map.as_deref())
            .collect();
        assert_eq!(kept, ["base_color", "normal", "roughness", "ao"]);
        let http = variant.files.iter().fold(Fixture::default(), |http, file| {
            http.with(&file.url, file.path.as_bytes())
        });
        let project = temp.0.join("Project");
        let cancel = AtomicBool::new(false);
        let saved = download(
            &http,
            &POLYHAVEN,
            &temp.0.join("Assets"),
            &project,
            &record,
            &variant,
            &cancel,
        );
        assert!(
            matches!(saved, Ok(DownloadResult::Saved { .. })),
            "{:?}",
            saved.err()
        );
        let folder = project.join("assets").join("textures").join("red_brick_03");
        let set =
            axcenity_assets::read_texture_set(&folder.join("red_brick_03.texture.json")).unwrap();
        assert_eq!(set.source, "polyhaven");
        assert_eq!(set.maps.len(), 4);
        assert!(set.maps.iter().all(|map| !map.flip_green));
        assert!(!folder.join("red_brick_03_nor_dx_2k.jpg").exists());
    }

    /// Get against the real Poly Haven: search, look the record up, choose
    /// the nearest to 1K JPG, download and save the texture set. Reaches the
    /// network, so run it by hand (see the ambientCG one above).
    #[test]
    #[ignore = "reaches Poly Haven"]
    fn get_downloads_a_real_poly_haven_texture_into_a_project() {
        let temp = Temp::new("polyhaven-live");
        let client = http::Client::new(Duration::from_secs(600));
        let listings = search(&client, &[&POLYHAVEN], Kind::Texture, "brick", 1, false)
            .unwrap()
            .listings;
        let listing = &listings[0];
        assert!(listing.lookup);
        let full = POLYHAVEN.record(&client, &listing.record.id).unwrap();
        let listing = Listing::from_record(full.clone());
        let choice = pick(&listing, "1K", "JPG").unwrap();
        let variant = needed_maps(choice.variant);
        let project = temp.0.join("Project");
        let cancel = AtomicBool::new(false);
        let saved = download(
            &client,
            &POLYHAVEN,
            &temp.0.join("Assets"),
            &project,
            &full,
            &variant,
            &cancel,
        );
        assert!(
            matches!(saved, Ok(DownloadResult::Saved { .. })),
            "{:?}",
            saved.err()
        );
        let set_file = project
            .join("assets")
            .join("textures")
            .join(&full.id)
            .join(format!("{}{TEXTURE_SET_SUFFIX}", full.id));
        assert!(set_file.is_file());
    }

    fn single(
        kind: Kind,
        provider: &str,
        id: &str,
        name: &str,
        url: &str,
        format: &str,
    ) -> (AssetRecord, Variant) {
        let variant = Variant {
            label: format!("4K {}", format.to_uppercase()),
            resolution: (kind == Kind::Hdri).then(|| "4k".to_owned()),
            format: format.to_owned(),
            package: Package::Single,
            files: vec![RemoteFile {
                url: url.to_owned(),
                path: url.rsplit('/').next().unwrap().to_owned(),
                ..RemoteFile::default()
            }],
        };
        let record = AssetRecord {
            provider: provider.into(),
            id: id.into(),
            kind,
            name: name.into(),
            license: if kind == Kind::Sound {
                "CC-BY".into()
            } else {
                "CC0".into()
            },
            authors: vec!["Jane Doe".into()],
            ..AssetRecord::default()
        };
        (record, variant)
    }

    #[test]
    fn a_sky_lands_in_assets_sky_named_by_its_resolution() {
        let temp = Temp::new("sky");
        let url = "https://dl.polyhaven.org/fish_eagle_hill_4k.hdr";
        let (record, variant) = single(
            Kind::Hdri,
            "polyhaven",
            "fish_eagle_hill",
            "Fish Eagle Hill",
            url,
            "hdr",
        );
        let http = Fixture::default().with(url, "radiance");
        let project = temp.0.join("Project");
        let cancel = AtomicBool::new(false);
        let saved = download(
            &http,
            &POLYHAVEN,
            &temp.0.join("Assets"),
            &project,
            &record,
            &variant,
            &cancel,
        );
        assert!(
            matches!(&saved, Ok(DownloadResult::Saved { text, .. }) if text.ends_with("in assets/sky/fish_eagle_hill_4k.hdr.")),
            "{saved:?}"
        );
        assert_eq!(
            fs::read(project.join("assets/sky/fish_eagle_hill_4k.hdr")).unwrap(),
            b"radiance"
        );
    }

    #[test]
    fn a_sound_lands_in_assets_audio_with_its_licence_named() {
        let temp = Temp::new("sound");
        let url = "https://cdn.freesound.org/previews/1/door.mp3";
        let id = "4bd0a3e8-1111-2222-3333-444455556666";
        let (record, variant) = single(Kind::Sound, "openverse", id, "Creaking Door!", url, "mp3");
        let http = Fixture::default().with(url, "mp3 bytes");
        let project = temp.0.join("Project");
        let cancel = AtomicBool::new(false);
        let saved = download(
            &http,
            &OPENVERSE,
            &temp.0.join("Assets"),
            &project,
            &record,
            &variant,
            &cancel,
        );
        assert!(
            matches!(&saved, Ok(DownloadResult::Saved { text, .. })
                if text == "Saved Creaking Door! (CC BY) in assets/audio/creaking-door-4bd0a3e8.mp3."),
            "{saved:?}"
        );
        assert!(
            project
                .join("assets/audio/creaking-door-4bd0a3e8.mp3")
                .is_file()
        );
    }

    #[test]
    fn each_tab_offers_only_packages_it_can_install() {
        let (_, sky) = single(
            Kind::Hdri,
            "polyhaven",
            "a",
            "A",
            "https://dl.polyhaven.org/a.exr",
            "exr",
        );
        assert!(usable(Kind::Hdri, &sky));
        assert!(!usable(Kind::Texture, &sky));
        let mut zip_sky = zip("4K", "4k", "");
        zip_sky.format = String::new();
        assert!(usable(Kind::Hdri, &zip_sky));
        let (_, sound) = single(
            Kind::Sound,
            "openverse",
            "a",
            "A",
            "https://x.org/a.mp3",
            "mp3",
        );
        assert!(usable(Kind::Sound, &sound));
        let (_, odd) = single(
            Kind::Sound,
            "openverse",
            "a",
            "A",
            "https://x.org/a.aiff",
            "aiff",
        );
        assert!(!usable(Kind::Sound, &odd));
        assert!(!usable(Kind::Sound, &zip("2K JPG", "2k", "jpg")));
    }

    #[test]
    fn sound_names_are_plain_and_unique() {
        let record = |name: &str, id: &str| AssetRecord {
            name: name.into(),
            id: id.into(),
            ..AssetRecord::default()
        };
        assert_eq!(
            sound_name(&record("Rain on a Tin Roof", "ab-cd-ef")),
            "rain-on-a-tin-roof-abcdef"
        );
        assert_eq!(sound_name(&record("???", "1234567890")), "sound-12345678");
    }

    /// Gets the first result of a real search on `source` into a project at
    /// the smallest package, looking the record up first when it needs it.
    fn get_first(
        source: &dyn AssetSource,
        kind: Kind,
        words: &str,
        resolution: &str,
        format: &str,
    ) -> String {
        let temp = Temp::new(&format!("live-{}", source.info().key));
        let client = http::Client::new(Duration::from_secs(600));
        let found = search(&client, &[source], kind, words, 1, false).unwrap();
        let listing = found.listings.into_iter().next().expect("a result");
        let listing = if listing.lookup {
            Listing::from_record(source.record(&client, &listing.record.id).unwrap())
        } else {
            listing
        };
        let choice = pick(&listing, resolution, format).expect("a usable package");
        let variant = needed_maps(choice.variant);
        let cancel = AtomicBool::new(false);
        let project = temp.0.join("Project");
        let saved = download(
            &client,
            source,
            &temp.0.join("Assets"),
            &project,
            &listing.record,
            &variant,
            &cancel,
        );
        match saved {
            Ok(DownloadResult::Saved { text, .. }) => text,
            other => panic!("{other:?}"),
        }
    }

    /// Skies and sounds from the real sources. Reaches the network, so run
    /// it by hand (see the ambientCG texture one above).
    #[test]
    #[ignore = "reaches ambientCG, Poly Haven and Openverse"]
    fn get_downloads_real_skies_and_sounds() {
        for (source, words, format) in [
            (&AMBIENTCG as &dyn AssetSource, "sky", "EXR"),
            (&POLYHAVEN, "sunset", "HDR"),
        ] {
            let text = get_first(source, Kind::Hdri, words, "1K", format);
            assert!(text.contains("in assets/sky/"), "{text}");
        }
        let text = get_first(&OPENVERSE, Kind::Sound, "door", "", "MP3");
        assert!(text.contains("in assets/audio/"), "{text}");
    }

    fn bundle(gltf: &str) -> (AssetRecord, Variant, Fixture) {
        let file = |path: &str, body: &str| RemoteFile {
            url: format!("https://dl.polyhaven.org/file/ph-assets/Models/{path}"),
            path: path.to_owned(),
            size: Some(body.len() as u64),
            md5: None,
            map: None,
        };
        let files = [
            ("chair_01_1k.gltf", gltf),
            ("chair_01.bin", "buffer"),
            ("textures/chair_01_diff_1k.jpg", "jpeg"),
        ];
        let variant = Variant {
            label: "1K GLTF".into(),
            resolution: Some("1k".into()),
            format: "gltf".into(),
            package: Package::Bundle,
            files: files.iter().map(|(path, body)| file(path, body)).collect(),
        };
        let http = files.iter().fold(Fixture::default(), |http, (path, body)| {
            http.with(&file(path, body).url, *body)
        });
        let record = AssetRecord {
            provider: "polyhaven".into(),
            id: "chair_01".into(),
            kind: Kind::Model,
            name: "Chair 01".into(),
            license: "CC0".into(),
            variants: vec![variant.clone()],
            ..AssetRecord::default()
        };
        (record, variant, http)
    }

    #[test]
    fn a_poly_haven_model_is_fetched_whole_and_handed_to_the_import_dialog() {
        let temp = Temp::new("model");
        let gltf = r#"{"asset": {"version": "2.0"}, "buffers": [{"uri": "chair_01.bin"}],
                       "images": [{"uri": "textures/chair_01_diff_1k.jpg"}]}"#;
        let (record, variant, http) = bundle(gltf);
        assert!(usable(Kind::Model, &variant));
        assert!(!usable(Kind::Texture, &variant));
        let project = temp.0.join("Project");
        let saved = download(
            &http,
            &POLYHAVEN,
            &temp.0.join("Assets"),
            &project,
            &record,
            &variant,
            &AtomicBool::new(false),
        )
        .unwrap();
        let DownloadResult::Saved {
            installed: Installed::Model { file, key, variant },
            ..
        } = saved
        else {
            panic!("{saved:?}");
        };
        assert_eq!(file.file_name().unwrap(), "chair_01_1k.gltf");
        assert!(
            file.parent()
                .unwrap()
                .join("textures/chair_01_diff_1k.jpg")
                .is_file()
        );
        assert_eq!(
            (key.as_str(), variant.as_str()),
            ("polyhaven:chair_01", "1K GLTF")
        );
        // The Import dialog copies it in; nothing is in the project before.
        assert!(!project.exists());
    }

    #[test]
    fn a_model_that_reaches_outside_its_folder_is_refused() {
        let temp = Temp::new("escape");
        let gltf = r#"{"asset": {"version": "2.0"}, "buffers": [{"uri": "../../metadata.db"}]}"#;
        let (record, variant, http) = bundle(gltf);
        let err = download(
            &http,
            &POLYHAVEN,
            &temp.0.join("Assets"),
            &temp.0.join("Project"),
            &record,
            &variant,
            &AtomicBool::new(false),
        )
        .unwrap_err();
        assert!(err.contains("outside its folder"), "{err}");
    }

    #[test]
    fn a_texture_install_is_recorded_in_the_projects_asset_sources() {
        let temp = Temp::new("sources");
        let variant = zip("2K JPG", "2k", "jpg");
        let record = record(vec![variant.clone()]);
        let http = Fixture::default().with(&variant.files[0].url, package_zip());
        let project = temp.0.join("Project");
        download(
            &http,
            &AMBIENTCG,
            &temp.0.join("Assets"),
            &project,
            &record,
            &variant,
            &AtomicBool::new(false),
        )
        .unwrap();
        let snapshot = axcenity_toolbox::provenance::Snapshot::read(&project).unwrap();
        assert_eq!(snapshot.files.len(), 1);
        assert_eq!(snapshot.files[0].path, "assets/textures/Fabric086");
        assert!(snapshot.files[0].uid.is_some(), "the texture set has an ID");
        let source = snapshot
            .source_of("assets/textures/Fabric086/Fabric086_2K-JPG_Color.jpg")
            .unwrap();
        assert_eq!(source.name, "Fabric 086");
    }

    #[test]
    fn make_material_names_each_map_of_the_texture_set() {
        use axcenity_assets::{Channel, MapSlot, TextureSet, TextureSetMap};
        let map = |slot, path: &str, channel, flip_green| TextureSetMap {
            slot,
            path: path.into(),
            channel,
            invert: false,
            flip_green,
        };
        let set = TextureSet {
            source: "ambientcg".into(),
            asset_id: "Rock030".into(),
            link: String::new(),
            maps: vec![
                map(
                    MapSlot::BaseColor,
                    "assets/textures/Rock030/c.jpg",
                    Channel::Red,
                    false,
                ),
                map(
                    MapSlot::Normal,
                    "assets/textures/Rock030/n.jpg",
                    Channel::Red,
                    true,
                ),
                map(
                    MapSlot::Roughness,
                    "assets/textures/Rock030/arm.jpg",
                    Channel::Green,
                    false,
                ),
            ],
        };
        let material = install::material_from_set(&set);
        let maps = &material.maps;
        assert_eq!(
            maps.color.as_ref().unwrap().path.as_deref(),
            Some("assets/textures/Rock030/c.jpg")
        );
        assert_eq!(maps.normal.as_ref().unwrap().direct_x, Some(true));
        assert_eq!(
            maps.roughness.as_ref().unwrap().channel,
            Some(Channel::Green)
        );
        assert!(maps.metalness.is_none());
        assert_eq!(material.roughness, Some(1.0));
        assert_eq!(material.metalness, None);
    }
}
