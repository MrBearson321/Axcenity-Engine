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
    TEXTURE_EXTENSIONS, TEXTURE_SET_SUFFIX, texture_set_from_files, write_texture_set,
};
use axcenity_toolbox::ambientcg::AMBIENTCG;
use axcenity_toolbox::db::{self, Database, Install};
use axcenity_toolbox::directory::{self, AssetDirectory};
use axcenity_toolbox::download::Fetcher;
use axcenity_toolbox::http::{self, Http};
use axcenity_toolbox::polyhaven::POLYHAVEN;
use axcenity_toolbox::variants::{self, Choice};
use axcenity_toolbox::{
    AssetRecord, AssetSource, Kind, Package, Query, SourceError, Variant, files,
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
        let lookup = record.variants.is_empty() && record.provider == POLYHAVEN.info().key;
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
            packages: record.variants.into_iter().filter(usable).collect(),
            id: record.id,
            name: record.name,
        }
    }
}

/// Whether Get can make a texture set from a package: one zip, or loose
/// maps, of JPG or PNG images (the formats the engine reads).
fn usable(variant: &Variant) -> bool {
    let images = FORMATS
        .iter()
        .any(|format| variant.format.eq_ignore_ascii_case(format));
    let shape = match variant.package {
        Package::Archive => variant.files.len() == 1,
        Package::Maps => !variant.files.is_empty(),
        Package::Bundle | Package::Single => false,
    };
    images && shape
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
    /// Where materials come from: ambientCG or Poly Haven.
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
        let subtitle = format!(
            "Materials from {}. Saved files are CC0 and work offline.",
            source.name
        );
        let ((), dismissed) = dialog::modal(ctx, "get texture", width, |ui| {
            dialog::header(
                ui,
                icons::DOWNLOAD_SIMPLE,
                dialog::Tone::Question,
                "Get texture",
                Some(&subtitle),
            );
            ui.horizontal_wrapped(|ui| {
                self.source_choice(ui);
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.query)
                        .hint_text("Search materials")
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
                self.choice(ui, "resolution", &RESOLUTIONS, true);
                self.choice(ui, "format", &FORMATS, false);
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

    /// The source menu. Picking another source searches it.
    fn source_choice(&mut self, ui: &mut egui::Ui) {
        let mut picked = None;
        egui::ComboBox::from_id_salt("texture source")
            .selected_text(self.source.info().name)
            .show_ui(ui, |ui| {
                for source in axcenity_toolbox::sources_for(Kind::Texture) {
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
            ui.label("No materials match that.");
            return;
        }
        let total = page.total;
        let shown = page.listings.len();
        let cards: Vec<Card> = page
            .listings
            .iter()
            .map(|listing| {
                let maps = listing.maps.join(", ");
                let detail = if maps.is_empty() {
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
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = search(source, &query, offset).map(|(total, listings)| SearchResult {
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
    variants::choose(&listing.packages, &resolution, &format, &["jpg", "png"])
}

/// One page of materials matching `query` from one of the Toolbox's
/// texture sources (most popular first).
fn search(
    source: &dyn AssetSource,
    query: &str,
    offset: usize,
) -> Result<(usize, Vec<Listing>), String> {
    let client = http::Client::new(Duration::from_secs(30));
    let mut wanted = Query::new(query, Kind::Texture);
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

/// Gets one package of `record` into `project/assets/textures/<id>/` with
/// its texture set file.
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
    let fetched =
        match fetcher.fetch_variant(source.info(), record, variant, &TEXTURE_EXTENSIONS, cancel) {
            Ok(fetched) => fetched,
            Err(SourceError::Cancelled) => return Ok(DownloadResult::Cancelled),
            Err(err) => return Err(err.to_string()),
        };
    if cancel.load(Ordering::Relaxed) {
        return Ok(DownloadResult::Cancelled);
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |time| time.as_millis());
    let root = std::env::temp_dir().join(format!("axcenity-texture-{id}-{stamp}"));
    let outcome = install(&root, &dir, &fetched, project, record, cancel);
    let _ = fs::remove_dir_all(&root);
    let outcome = outcome?;
    if let DownloadResult::Saved(_) = &outcome {
        let install = Install {
            project: project.display().to_string(),
            path: format!("assets/textures/{id}"),
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
        "{verb} {} (CC0) in assets/textures/{id}.",
        record.name
    )))
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::path::PathBuf;

    use axcenity_toolbox::RemoteFile;
    use axcenity_toolbox::http::Fixture;

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
        let (_, listings) = search(&AMBIENTCG, "ground", 0).unwrap();
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
        let (_, listings) = search(&POLYHAVEN, "brick", 0).unwrap();
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
}
