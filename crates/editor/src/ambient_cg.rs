//! The Assets panel's Get texture dialog: search ambientCG's public
//! catalog, download one material, and save it as a texture set.
//!
//! The catalog needs a network connection. Results show as soon as the
//! catalog answers; their previews arrive afterwards, fetched by a few
//! threads at once, and are kept for the rest of the dialog's life. A
//! download runs on its own thread and writes the zip to disk before
//! extracting it, so a large package is not held in memory. Cancelling, or
//! closing the dialog, leaves the project as it was.
//!
//! Searching and the packages on offer come from the Toolbox's ambientCG
//! source (its v3 API); when the chosen resolution and format aren't
//! there, Get takes the nearest package and says so. A package is fetched
//! once into the central asset directory (verified, and recorded with its
//! source and licence in the metadata database) and copied into each
//! project from there, so a second project gets it without downloading.

use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axcenity_assets::{
    AUDIO_EXTENSIONS, PROJECT_AUDIO_FOLDER, PROJECT_SKY_FOLDER, PROJECT_TEXTURES_FOLDER,
    TEXTURE_EXTENSIONS, TEXTURE_SET_SUFFIX, texture_set_from_files, write_texture_set,
};
use axcenity_toolbox::ambientcg::AMBIENTCG;
use axcenity_toolbox::db::{self, Database, Install};
use axcenity_toolbox::directory::{self, AssetDirectory};
use axcenity_toolbox::download::Fetcher;
use axcenity_toolbox::http::{self, Http};
use axcenity_toolbox::variants::{self, Choice};
use axcenity_toolbox::{
    AssetRecord, AssetSource, Kind, Licenses, Package, Query, SourceError, Variant, files, license,
};
use eframe::egui;

use crate::dialog;
use crate::theme::icons;

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

/// What finished while the dialog was open.
pub struct TextureEvent {
    pub close: bool,
    /// `error` is a failure for Output. A saved set is not.
    pub notice: Option<(bool, String)>,
}

struct Listing {
    id: String,
    name: String,
    category: String,
    maps: Vec<String>,
    preview: Option<String>,
    /// Its JPG and PNG packages, such as "2K JPG": one zip each from
    /// ambientCG, loose maps from Poly Haven.
    packages: Vec<Variant>,
    /// The source lists its packages only in the full record (Poly Haven),
    /// so Get looks them up first.
    lookup: bool,
    /// The source's record, kept in the metadata database on download.
    record: AssetRecord,
}

impl Listing {
    /// A search result from one of the Toolbox's texture sources.
    fn from_record(record: AssetRecord) -> Self {
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
            id: record.id,
            name: record.name,
        }
    }
}

/// What the dialog offers for one kind of asset.
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
        Kind::Texture | Kind::Model => KindRules {
            label: "Textures",
            noun: "materials",
            formats: &FORMATS,
            resolution: true,
            extract: &TEXTURE_EXTENSIONS,
            licenses: &[],
        },
    }
}

/// The kinds the dialog has tabs for, in order.
const KINDS: [Kind; 3] = [Kind::Texture, Kind::Hdri, Kind::Sound];

/// Whether Get can install a package of a `kind` asset: for textures one
/// zip or loose maps of JPG or PNG images (the formats the engine reads);
/// for skies one HDR or EXR file, or a zip of them; for sounds one file the
/// engine plays.
fn usable(kind: Kind, variant: &Variant) -> bool {
    let format_known = rules(kind)
        .formats
        .iter()
        .any(|format| variant.format.eq_ignore_ascii_case(format));
    let shape = match variant.package {
        Package::Archive => variant.files.len() == 1 && kind != Kind::Sound,
        Package::Maps => !variant.files.is_empty() && kind == Kind::Texture,
        Package::Single => variant.files.len() == 1 && kind != Kind::Texture,
        Package::Bundle => false,
    };
    // ambientCG labels its HDRI zips by resolution only ("4K"), so their
    // format is unknown until they're unpacked.
    let unlabelled_sky_zip =
        kind == Kind::Hdri && variant.package == Package::Archive && variant.format.is_empty();
    shape && (format_known || unlabelled_sky_zip)
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
    total: usize,
    listings: Vec<Listing>,
}

struct SearchResult {
    generation: u64,
    offset: usize,
    total: usize,
    listings: Vec<Listing>,
}

#[derive(Debug)]
enum DownloadResult {
    Saved(String),
    Cancelled,
}

struct Job<T> {
    rx: Receiver<Result<T, String>>,
}

/// A fetched preview: the asset id, and `None` if it couldn't be fetched.
type Preview = (String, Option<egui::ColorImage>);

/// What one result card shows.
struct Card {
    id: String,
    name: String,
    detail: String,
    /// What Get would fetch ("Gets 2K JPG."), or `None` with no download.
    plan: Option<String>,
}

/// The Get texture dialog.
pub struct GetTexture {
    /// The tab: textures, skies or sounds.
    kind: Kind,
    /// Where they come from: ambientCG, Poly Haven or Openverse.
    source: &'static dyn AssetSource,
    query: String,
    resolution: String,
    format: String,
    page: Option<Page>,
    error: Option<String>,
    status: Option<String>,
    search: Option<Job<SearchResult>>,
    download: Option<Job<DownloadResult>>,
    generation: u64,
    cancel: Arc<AtomicBool>,
    /// By asset id: `None` while fetching.
    previews: HashMap<String, Option<egui::TextureHandle>>,
    /// Assets whose preview couldn't be fetched, shown blank.
    no_preview: HashSet<String>,
    preview_tx: Sender<Preview>,
    preview_rx: Receiver<Preview>,
}

impl GetTexture {
    /// Opens the dialog and searches the popular materials.
    pub fn open(ctx: &egui::Context) -> Self {
        let mut dialog = Self::idle();
        dialog.start_search(ctx, 0);
        dialog
    }

    /// The dialog before its first search.
    fn idle() -> Self {
        let (preview_tx, preview_rx) = mpsc::channel();
        Self {
            kind: Kind::Texture,
            source: &AMBIENTCG,
            query: String::new(),
            resolution: "2K".to_owned(),
            format: "JPG".to_owned(),
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
        }
    }

    /// Draws the dialog. `Some` when it should close or Output should hear
    /// about a download.
    pub fn show(&mut self, ctx: &egui::Context, project: &Path) -> Option<TextureEvent> {
        if self.search.is_some() || self.download.is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        let notice = self.poll(ctx);
        self.load_previews(ctx);

        let screen = ctx.screen_rect().size();
        let width = (screen.x - 64.0).clamp(280.0, 800.0);
        let height = (screen.y - 260.0).clamp(200.0, 600.0);
        let mut close = false;
        let source = self.source.info();
        let kind = rules(self.kind);
        let licence = match source.licenses {
            Licenses::All(code) => format!("Saved files are {code} and work offline."),
            Licenses::PerAsset => {
                "Each has its own licence, shown on its card; saved files work offline.".to_owned()
            }
        };
        let subtitle = format!("{} from {}. {licence}", capitalize(kind.noun), source.name);
        let ((), dismissed) = dialog::modal(ctx, "get texture", width, |ui| {
            dialog::header(
                ui,
                icons::DOWNLOAD_SIMPLE,
                dialog::Tone::Question,
                "Toolbox",
                Some(&subtitle),
            );
            self.kind_tabs(ui);
            ui.horizontal_wrapped(|ui| {
                self.source_choice(ui);
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.query)
                        .hint_text(format!("Search {}", kind.noun))
                        .desired_width(180.0),
                );
                let enter =
                    response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
                if (ui
                    .add_enabled(self.search.is_none(), egui::Button::new("Search"))
                    .clicked()
                    || enter)
                    && self.search.is_none()
                {
                    self.start_search(ui.ctx(), 0);
                }
                if kind.resolution {
                    self.choice(ui, "resolution", &RESOLUTIONS, true);
                }
                self.choice(ui, "format", kind.formats, false);
            });
            if let Some(error) = &self.error {
                ui.colored_label(ui.visuals().error_fg_color, error);
            }
            if let Some(status) = &self.status {
                ui.label(status);
            }
            ui.add_space(4.0);
            self.results(ui, project, height);
            // Where the results come from (Poly Haven's API terms ask for it).
            ui.label(egui::RichText::new(source.credit).weak().small());
            close |= dialog::buttons(
                ui,
                &[dialog::Button::new(icons::X, "Close", dialog::Kind::Plain)],
                dialog::Enter::Main,
            )
            .is_some();
        });
        close |= dismissed;
        if close {
            self.cancel.store(true, Ordering::Relaxed);
        }
        (close || notice.is_some()).then_some(TextureEvent { close, notice })
    }

    /// Textures, Skies and Sounds. Picking another kind takes its first
    /// source and format, and searches.
    fn kind_tabs(&mut self, ui: &mut egui::Ui) {
        let mut picked = None;
        ui.horizontal(|ui| {
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
            && let Some(source) = axcenity_toolbox::sources_for(kind).into_iter().next()
        {
            self.kind = kind;
            self.source = source;
            self.format = rules(kind).formats[0].to_owned();
            self.start_search(ui.ctx(), 0);
        }
    }

    /// The source menu. Picking another source searches it.
    fn source_choice(&mut self, ui: &mut egui::Ui) {
        let mut picked = None;
        egui::ComboBox::from_id_salt("texture source")
            .selected_text(self.source.info().name)
            .show_ui(ui, |ui| {
                for source in axcenity_toolbox::sources_for(self.kind) {
                    let current = source.info().key == self.source.info().key;
                    if ui.selectable_label(current, source.info().name).clicked() && !current {
                        picked = Some(source);
                    }
                }
            });
        if let Some(source) = picked
            && self.search.is_none()
        {
            self.source = source;
            self.start_search(ui.ctx(), 0);
        }
    }

    fn choice(&mut self, ui: &mut egui::Ui, salt: &str, options: &[&str], resolution: bool) {
        let selected = if resolution {
            self.resolution.clone()
        } else {
            self.format.clone()
        };
        egui::ComboBox::from_id_salt(salt)
            .selected_text(&selected)
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

    fn results(&mut self, ui: &mut egui::Ui, project: &Path, height: f32) {
        let Some(page) = self.page.as_ref() else {
            if self.search.is_some() {
                ui.label("Searching…");
            }
            return;
        };
        if page.listings.is_empty() {
            ui.label(format!("No {} match that.", rules(self.kind).noun));
            return;
        }
        let total = page.total;
        let shown = page.listings.len();
        let cards: Vec<Card> = page
            .listings
            .iter()
            .map(|listing| {
                let maps = listing.maps.join(", ");
                let record = &listing.record;
                let detail = if record.kind == Kind::Sound {
                    // Each sound has its own licence and author: show both.
                    let author = record
                        .authors
                        .first()
                        .map_or("unknown author", String::as_str);
                    format!("{} · {author}", record.license)
                } else if maps.is_empty() {
                    listing.category.clone()
                } else if listing.category.is_empty() {
                    maps
                } else {
                    format!("{} · {maps}", listing.category)
                };
                Card {
                    id: listing.id.clone(),
                    name: listing.name.clone(),
                    detail,
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
        egui::ScrollArea::vertical()
            .id_salt("ambientcg results")
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
                                self.card(ui, project, card);
                            },
                        );
                    }
                });
            });
        if shown < total
            && ui
                .add_enabled(self.search.is_none(), egui::Button::new("More"))
                .clicked()
        {
            self.start_search(ui.ctx(), shown);
        }
    }

    /// One result: its preview (larger when hovered), name, kind and Get.
    fn card(&mut self, ui: &mut egui::Ui, project: &Path, card: &Card) {
        ui.spacing_mut().item_spacing.y = 2.0;
        let (rect, response) =
            ui.allocate_exact_size(egui::Vec2::splat(CARD_PICTURE), egui::Sense::hover());
        let preview = self.previews.get(&card.id);
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
        let tip = format!("{}\n{}", card.name, card.detail);
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
        ui.add(egui::Label::new(&card.name).truncate());
        ui.add(egui::Label::new(egui::RichText::new(&card.detail).weak().small()).truncate());
        let button = ui.add_enabled(
            card.plan.is_some() && self.download.is_none(),
            egui::Button::new("Get"),
        );
        let button = match &card.plan {
            Some(plan) => button.on_hover_text(plan),
            None => button.on_disabled_hover_text(format!("{} has no download.", card.name)),
        };
        if button.clicked() {
            self.start_download(ui.ctx(), project, &card.id);
        }
    }

    /// The package Get fetches for `listing`: the chosen resolution and
    /// format, else the nearest there is (`variants::choose`).
    fn package<'a>(&self, listing: &'a Listing) -> Option<Choice<'a>> {
        pick(listing, &self.resolution, &self.format)
    }

    fn start_search(&mut self, ctx: &egui::Context, offset: usize) {
        if offset == 0 {
            self.generation += 1;
            self.page = None;
        }
        self.error = None;
        self.status = Some("Searching…".to_owned());
        let generation = self.generation;
        let query = self.query.trim().to_owned();
        let source = self.source;
        let kind = self.kind;
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result =
                search(source, kind, &query, offset).map(|(total, listings)| SearchResult {
                    generation,
                    offset,
                    total,
                    listings,
                });
            let _ = tx.send(result);
            ctx.request_repaint();
        });
        self.search = Some(Job { rx });
    }

    fn start_download(&mut self, ctx: &egui::Context, project: &Path, id: &str) {
        let Some(listing) = self
            .page
            .as_ref()
            .and_then(|page| page.listings.iter().find(|listing| listing.id == id))
        else {
            return;
        };
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
        let source = self.source;
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
                        let choice = pick(&listing, &resolution, &format)
                            .ok_or_else(|| "it has no JPG or PNG maps".to_owned())?;
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
                Ok(DownloadResult::Cancelled) => Ok(DownloadResult::Cancelled),
                Ok(DownloadResult::Saved(text)) if !notes.is_empty() => {
                    Ok(DownloadResult::Saved(format!("{text} {notes}")))
                }
                Ok(saved) => Ok(saved),
                Err(err) => Err(format!("Could not get {name}: {err}")),
            };
            let _ = tx.send(result);
            ctx.request_repaint();
        });
        self.download = Some(Job { rx });
    }

    /// Finished searches and downloads since last frame.
    fn poll(&mut self, ctx: &egui::Context) -> Option<(bool, String)> {
        let search = self.search.take();
        if let Some(job) = search {
            match job.rx.try_recv() {
                Ok(Ok(result)) => self.accept_search(ctx, result),
                Ok(Err(err)) => {
                    self.status = None;
                    self.error = Some(format!("Could not search ambientCG: {err}"));
                }
                Err(TryRecvError::Empty) => self.search = Some(job),
                Err(TryRecvError::Disconnected) => {
                    self.status = None;
                    self.error = Some("Could not search ambientCG.".to_owned());
                }
            }
        }
        let job = self.download.take()?;
        match job.rx.try_recv() {
            Ok(Ok(DownloadResult::Saved(text))) => {
                self.status = None;
                Some((false, text))
            }
            Ok(Ok(DownloadResult::Cancelled)) => {
                self.status = None;
                None
            }
            Ok(Err(err)) => {
                self.status = None;
                self.error = Some(err.clone());
                Some((true, err))
            }
            Err(TryRecvError::Empty) => {
                self.download = Some(job);
                None
            }
            Err(TryRecvError::Disconnected) => {
                self.status = None;
                let err = "Could not download that texture.".to_owned();
                self.error = Some(err.clone());
                Some((true, err))
            }
        }
    }

    fn accept_search(&mut self, ctx: &egui::Context, result: SearchResult) {
        if result.generation != self.generation {
            return;
        }
        self.status = None;
        // Previews not seen before, fetched now; the rest are kept.
        let wanted: VecDeque<(String, String)> = result
            .listings
            .iter()
            .filter(|listing| {
                !self.previews.contains_key(&listing.id) && !self.no_preview.contains(&listing.id)
            })
            .filter_map(|listing| Some((listing.id.clone(), listing.preview.clone()?)))
            .collect();
        for (id, _) in &wanted {
            self.previews.insert(id.clone(), None);
        }
        fetch_previews(ctx, wanted, &self.preview_tx);
        if result.offset == 0 || self.page.is_none() {
            self.page = Some(Page {
                total: result.total,
                listings: result.listings,
            });
        } else if let Some(page) = &mut self.page {
            page.total = result.total;
            page.listings.extend(result.listings);
        }
    }

    fn load_previews(&mut self, ctx: &egui::Context) {
        for (id, image) in self.preview_rx.try_iter() {
            match image {
                Some(image) => {
                    let handle = ctx.load_texture(
                        format!("ambientcg {id}"),
                        image,
                        egui::TextureOptions::LINEAR,
                    );
                    self.previews.insert(id, Some(handle));
                }
                None => {
                    self.previews.remove(&id);
                    self.no_preview.insert(id);
                }
            }
        }
    }
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

/// `text` with its first letter upper-case.
fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// One page of `kind` assets matching `query` from one of the Toolbox's
/// sources (most popular first; sounds only with the licences `rules` keeps).
fn search(
    source: &dyn AssetSource,
    kind: Kind,
    query: &str,
    offset: usize,
) -> Result<(usize, Vec<Listing>), String> {
    let client = http::Client::new(Duration::from_secs(30));
    let mut wanted = Query::new(query, kind);
    wanted.licenses = rules(kind)
        .licenses
        .iter()
        .map(|code| (*code).to_owned())
        .collect();
    wanted.per_page = PAGE as u32;
    wanted.page = (offset / PAGE) as u32 + 1;
    let page = source
        .search(&client, &wanted)
        .map_err(|err| err.to_string())?;
    let shown = offset + page.records.len();
    let total = page
        .total
        .map_or(shown + usize::from(page.more), |total| total as usize);
    let listings = page.records.into_iter().map(Listing::from_record).collect();
    Ok((total, listings))
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

/// Gets one package of `record` into the project: a texture into
/// `assets/textures/<id>/` with its texture set file, a sky into
/// `assets/sky/`, a sound into `assets/audio/`.
///
/// The package goes into the central asset directory at `assets` first
/// (verified, unpacked, recorded in its metadata database with the asset's
/// source and licence), unless it's already there, in which case nothing
/// is downloaded. Then its images are copied into the project, and the
/// database records the install. Does nothing to the project when `cancel`
/// is set before the files are moved into place.
fn download(
    http: &dyn Http,
    source: &dyn AssetSource,
    assets: &Path,
    project: &Path,
    record: &AssetRecord,
    variant: &Variant,
    cancel: &AtomicBool,
) -> Result<DownloadResult, String> {
    let id = record.id.as_str();
    if !folder_id(id) {
        return Err("the asset id is not a file name".to_owned());
    }
    let dir = AssetDirectory::open(assets).map_err(|err| err.to_string())?;
    let db = Database::open(&dir.database_path()).map_err(|err| err.to_string())?;
    let fetcher = Fetcher {
        http,
        dir: &dir,
        db: &db,
    };
    let extract = rules(record.kind).extract;
    let fetched = match fetcher.fetch_variant(source.info(), record, variant, extract, cancel) {
        Ok(fetched) => fetched,
        Err(SourceError::Cancelled) => return Ok(DownloadResult::Cancelled),
        Err(err) => return Err(err.to_string()),
    };
    if cancel.load(Ordering::Relaxed) {
        return Ok(DownloadResult::Cancelled);
    }
    let (outcome, installed) = if matches!(record.kind, Kind::Hdri | Kind::Sound) {
        install_file(&dir, &fetched, project, record, variant)?
    } else {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |time| time.as_millis());
        let root = std::env::temp_dir().join(format!("axcenity-texture-{id}-{stamp}"));
        let outcome = install(&root, &dir, &fetched, project, record, cancel);
        let _ = fs::remove_dir_all(&root);
        (outcome?, format!("{PROJECT_TEXTURES_FOLDER}/{id}"))
    };
    if let DownloadResult::Saved(_) = &outcome {
        let install = Install {
            project: project.display().to_string(),
            path: installed,
            key: record.key(),
            variant: variant.label.clone(),
            uid: None,
            installed_at: db::now(),
        };
        if let Err(err) = db.record_install(&install) {
            log::warn!("Could not record installing {}: {err}", record.key());
        }
    }
    Ok(outcome)
}

/// Copies a downloaded sky into `assets/sky/<id>_<resolution>.<hdr|exr>`
/// or a sound into `assets/audio/<name>-<id start>.<ext>`, and returns what
/// to say and the file's path in the project.
fn install_file(
    dir: &AssetDirectory,
    fetched: &db::Download,
    project: &Path,
    record: &AssetRecord,
    variant: &Variant,
) -> Result<(DownloadResult, String), String> {
    let wanted = rules(record.kind).extract;
    let file = fetched
        .files
        .iter()
        .find(|file| wanted.contains(&extension(&file.path).as_str()))
        .ok_or_else(|| "the download has no file the engine reads".to_owned())?;
    let ext = extension(&file.path);
    let relative = if record.kind == Kind::Hdri {
        let resolution = variant.resolution.as_deref().unwrap_or("sky");
        format!("{PROJECT_SKY_FOLDER}/{}_{resolution}.{ext}", record.id)
    } else {
        format!("{PROJECT_AUDIO_FOLDER}/{}.{ext}", sound_name(record))
    };
    let dest = relative
        .split('/')
        .fold(project.to_owned(), |path, part| path.join(part));
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let replaced = dest.exists();
    let source = file
        .path
        .split('/')
        .fold(dir.path(&fetched.folder), |path, part| path.join(part));
    fs::copy(&source, &dest).map_err(|err| err.to_string())?;
    let verb = if replaced { "Replaced" } else { "Saved" };
    let message = format!("{verb} {} ({}) in {relative}.", record.name, record.license);
    Ok((DownloadResult::Saved(message), relative))
}

/// A file name's extension, lower case.
fn extension(name: &str) -> String {
    name.rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .unwrap_or_default()
}

/// A sound's file name: its title as lower-case words joined by `-`, then
/// the start of its id so two sounds with one title don't collide.
fn sound_name(record: &AssetRecord) -> String {
    let words: Vec<String> = record
        .name
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_ascii_lowercase)
        .collect();
    let mut title = words.join("-");
    title.truncate(40);
    let title = title.trim_end_matches('-');
    let id: String = record
        .id
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(8)
        .collect();
    match (title.is_empty(), id.is_empty()) {
        (true, _) => format!("sound-{id}"),
        (false, true) => title.to_owned(),
        (false, false) => format!("{title}-{id}"),
    }
}

/// Copies a downloaded package's images into a staging folder, writes the
/// texture set beside them, and swaps the folder into the project.
fn install(
    root: &Path,
    dir: &AssetDirectory,
    fetched: &db::Download,
    project: &Path,
    record: &AssetRecord,
    cancel: &AtomicBool,
) -> Result<DownloadResult, String> {
    let id = record.id.as_str();
    let staging = root.join("files");
    fs::create_dir_all(&staging).map_err(|err| err.to_string())?;
    let source = dir.path(&fetched.folder);
    let mut names = Vec::new();
    for file in &fetched.files {
        // Unpacked archives are flat, so each path is a plain file name.
        if file.path.contains('/') {
            continue;
        }
        fs::copy(source.join(&file.path), staging.join(&file.path))
            .map_err(|err| err.to_string())?;
        names.push(file.path.clone());
    }
    let pairs: Vec<(String, String)> = names
        .iter()
        .map(|file| (file.clone(), format!("assets/textures/{id}/{file}")))
        .collect();
    let borrowed: Vec<(&str, &str)> = pairs
        .iter()
        .map(|(file, path)| (file.as_str(), path.as_str()))
        .collect();
    let set = texture_set_from_files(&record.provider, id, &record.url, &borrowed);
    if set.maps.is_empty() {
        return Err("none of the images were recognized as maps".to_owned());
    }
    write_texture_set(&staging.join(format!("{id}{TEXTURE_SET_SUFFIX}")), &set)
        .map_err(|err| err.to_string())?;
    if cancel.load(Ordering::Relaxed) {
        return Ok(DownloadResult::Cancelled);
    }
    let dest = project.join("assets").join("textures").join(id);
    let replaced = files::replace_dir(&staging, &dest, &root.join("previous"))
        .map_err(|err| err.to_string())?;
    let verb = if replaced { "Replaced" } else { "Saved" };
    Ok(DownloadResult::Saved(format!(
        "{verb} {} ({}) in assets/textures/{id}.",
        record.name, record.license
    )))
}

#[cfg(test)]
mod tests {
    use std::io::Write;
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
            matches!(&saved, DownloadResult::Saved(text) if text.starts_with("Saved Fabric 086"))
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
    /// `cargo nextest run -p axcenity-editor --run-ignored only -E 'test(ambient_cg)'`
    #[test]
    #[ignore = "reaches ambientCG"]
    fn get_downloads_a_real_material_into_a_project() {
        let temp = Temp::new("live");
        let (_, listings) = search(&AMBIENTCG, Kind::Texture, "ground", 0).unwrap();
        let listing = listings
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
            matches!(saved, Ok(DownloadResult::Saved(_))),
            "{:?}",
            saved.err()
        );
        let folder = project.join("assets").join("textures").join(&listing.id);
        assert!(
            folder
                .join(format!("{}{TEXTURE_SET_SUFFIX}", listing.id))
                .is_file()
        );
    }

    #[test]
    fn a_listing_keeps_only_single_zip_downloads_and_names_its_maps() {
        let mut single = zip("8K EXR", "8k", "exr");
        single.package = Package::Single;
        let listing = Listing::from_record(record(vec![zip("2K JPG", "2k", "jpg"), single]));
        assert_eq!(listing.id, "Fabric086");
        assert_eq!(listing.record.url, "https://ambientcg.com/a/Fabric086");
        assert_eq!(listing.maps, ["color", "normal", "occlusion"]);
        assert_eq!(listing.packages.len(), 1);
        assert!(listing.preview.is_some());
    }

    #[test]
    fn get_offers_the_nearest_package_when_the_chosen_one_is_missing() {
        let mut dialog = GetTexture::idle();
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
        let mut dialog = GetTexture::idle();
        dialog.page = Some(Page {
            total: 1,
            listings: vec![listing],
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
            matches!(saved, Ok(DownloadResult::Saved(_))),
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
        let (_, listings) = search(&POLYHAVEN, Kind::Texture, "brick", 0).unwrap();
        let listing = &listings[0];
        assert!(listing.lookup);
        let client = http::Client::new(Duration::from_secs(600));
        let full = POLYHAVEN.record(&client, &listing.id).unwrap();
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
            matches!(saved, Ok(DownloadResult::Saved(_))),
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
            matches!(&saved, Ok(DownloadResult::Saved(text)) if text.ends_with("in assets/sky/fish_eagle_hill_4k.hdr.")),
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
            matches!(&saved, Ok(DownloadResult::Saved(text))
                if text == "Saved Creaking Door! (CC-BY) in assets/audio/creaking-door-4bd0a3e8.mp3."),
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
        let (_, listings) = search(source, kind, words, 0).unwrap();
        let client = http::Client::new(Duration::from_secs(600));
        let listing = listings.into_iter().next().expect("a result");
        let listing = if listing.lookup {
            Listing::from_record(source.record(&client, &listing.id).unwrap())
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
            Ok(DownloadResult::Saved(text)) => text,
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
}
