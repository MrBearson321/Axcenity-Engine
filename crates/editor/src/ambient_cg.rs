//! The Assets panel's Get texture dialog: search ambientCG's public
//! catalog, download one material, and save it as a texture set.
//!
//! The catalog needs a network connection. Results show as soon as the
//! catalog answers; their previews arrive afterwards, fetched by a few
//! threads at once, and are kept for the rest of the dialog's life. A
//! download runs on its own thread and writes the zip to disk before
//! extracting it, so a large package is not held in memory. Cancelling, or
//! closing the dialog, leaves the project as it was.

use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axcenity_assets::{
    TEXTURE_EXTENSIONS, TEXTURE_SET_SUFFIX, texture_set_from_files, write_texture_set,
};
use eframe::egui;

use crate::dialog;
use crate::theme::icons;

const PAGE: usize = 24;
const RESOLUTIONS: [&str; 4] = ["1K", "2K", "4K", "8K"];
const FORMATS: [&str; 2] = ["JPG", "PNG"];
const USER_AGENT: &str = "AxcenityEngine/0.1 (texture catalog)";
/// A zip larger than this is refused, so a bad response can't fill the disk.
const ZIP_LIMIT: u64 = 2 * 1024 * 1024 * 1024;
const PREVIEW_LIMIT: u64 = 2 * 1024 * 1024;
const JSON_LIMIT: u64 = 8 * 1024 * 1024;
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
    link: String,
    maps: Vec<String>,
    preview: Option<String>,
    /// `(attribute, https url)`, such as `("2K-JPG", "https://ambientcg.com/get?…")`.
    packages: Vec<(String, String)>,
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
    available: bool,
}

/// The Get texture dialog.
pub struct GetTexture {
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
        let (preview_tx, preview_rx) = mpsc::channel();
        let mut dialog = Self {
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
        };
        dialog.start_search(ctx, 0);
        dialog
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
        let ((), dismissed) = dialog::modal(ctx, "get texture", width, |ui| {
            dialog::header(
                ui,
                icons::DOWNLOAD_SIMPLE,
                dialog::Tone::Question,
                "Get texture",
                Some("Materials from ambientCG. Saved files are CC0 and work offline."),
            );
            ui.horizontal_wrapped(|ui| {
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
        let attribute = format!("{}-{}", self.resolution, self.format);
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
                    available: listing
                        .packages
                        .iter()
                        .any(|(known, _)| known == &attribute),
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
        let mut button = ui.add_enabled(
            card.available && self.download.is_none(),
            egui::Button::new("Get"),
        );
        if !card.available {
            button = button.on_disabled_hover_text(format!(
                "{} has no {} {} package.",
                card.name, self.resolution, self.format
            ));
        }
        if button.clicked() {
            self.start_download(ui.ctx(), project, &card.id);
        }
    }

    fn package<'a>(&self, listing: &'a Listing) -> Option<&'a str> {
        let attribute = format!("{}-{}", self.resolution, self.format);
        listing
            .packages
            .iter()
            .find(|(known, _)| known == &attribute)
            .map(|(_, url)| url.as_str())
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
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = search(&query, offset).map(|(total, listings)| SearchResult {
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
        let Some(url) = self.package(listing).map(str::to_owned) else {
            self.error = Some(format!(
                "{} has no {} {} package.",
                listing.name, self.resolution, self.format
            ));
            return;
        };
        let name = listing.name.clone();
        let asset_id = listing.id.clone();
        let link = listing.link.clone();
        let project = project.to_owned();
        self.error = None;
        self.status = Some(format!("Downloading {name}…"));
        self.cancel = Arc::new(AtomicBool::new(false));
        let cancel = Arc::clone(&self.cancel);
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = match download(&project, &asset_id, &name, &link, &url, &cancel) {
                Ok(DownloadResult::Cancelled) => Ok(DownloadResult::Cancelled),
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

fn search(query: &str, offset: usize) -> Result<(usize, Vec<Listing>), String> {
    let url = format!(
        "https://ambientcg.com/api/v2/full_json?type=Material&sort=Popular&limit={PAGE}&offset={offset}&include=downloadData,previewData,displayData&q={}",
        encode(query)
    );
    let text = get_text(&url, Duration::from_secs(30))?;
    parse_catalog(&text)
}

/// Fetches each `(id, url)` in `wanted` on a few threads that share one
/// connection pool, sending each preview to `tx` as soon as it's ready.
/// They stop early once the dialog (the receiver) is gone.
fn fetch_previews(ctx: &egui::Context, wanted: VecDeque<(String, String)>, tx: &Sender<Preview>) {
    if wanted.is_empty() {
        return;
    }
    let Ok(agent) = http_agent(Duration::from_secs(20)) else {
        return;
    };
    let fetchers = PREVIEW_FETCHERS.min(wanted.len());
    let queue = Arc::new(Mutex::new(wanted));
    for _ in 0..fetchers {
        let queue = Arc::clone(&queue);
        let agent = agent.clone();
        let tx = tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            loop {
                let next = queue.lock().ok().and_then(|mut queue| queue.pop_front());
                let Some((id, url)) = next else {
                    return;
                };
                let image = preview_image(&agent, &url);
                if tx.send((id, image)).is_err() {
                    return;
                }
                ctx.request_repaint();
            }
        });
    }
}

fn preview_image(agent: &ureq::Agent, url: &str) -> Option<egui::ColorImage> {
    let bytes = get_bytes(agent, url, PREVIEW_LIMIT).ok()?;
    let image = image::load_from_memory(&bytes).ok()?.to_rgba8();
    Some(egui::ColorImage::from_rgba_unmultiplied(
        [image.width() as usize, image.height() as usize],
        image.as_raw(),
    ))
}

/// Downloads `url` into `project/assets/textures/<id>/` and writes the set
/// file. Does nothing to the project when `cancel` is set before the files
/// are moved into place.
fn download(
    project: &Path,
    id: &str,
    name: &str,
    link: &str,
    url: &str,
    cancel: &AtomicBool,
) -> Result<DownloadResult, String> {
    if !safe_asset_id(id) {
        return Err("the asset id is not a file name".to_owned());
    }
    if !url.starts_with("https://ambientcg.com/") {
        return Err("the download is not from ambientCG".to_owned());
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |time| time.as_millis());
    let root = std::env::temp_dir().join(format!("axcenity-texture-{id}-{stamp}"));
    let outcome = download_into(&root, project, id, name, link, url, cancel);
    let _ = fs::remove_dir_all(&root);
    outcome
}

fn download_into(
    root: &Path,
    project: &Path,
    id: &str,
    name: &str,
    link: &str,
    url: &str,
    cancel: &AtomicBool,
) -> Result<DownloadResult, String> {
    let staging = root.join("files");
    fs::create_dir_all(&staging).map_err(|err| err.to_string())?;
    let zip_path = root.join("package.zip");
    let mut zip = fs::File::create(&zip_path).map_err(|err| err.to_string())?;
    write_download(url, &mut zip)?;
    if cancel.load(Ordering::Relaxed) {
        return Ok(DownloadResult::Cancelled);
    }
    let names = extract_images(&zip_path, &staging)?;
    let pairs: Vec<(String, String)> = names
        .iter()
        .map(|file| (file.clone(), format!("assets/textures/{id}/{file}")))
        .collect();
    let borrowed: Vec<(&str, &str)> = pairs
        .iter()
        .map(|(file, path)| (file.as_str(), path.as_str()))
        .collect();
    let set = texture_set_from_files("ambientcg", id, link, &borrowed);
    if set.maps.is_empty() {
        return Err("none of the images were recognized as maps".to_owned());
    }
    write_texture_set(&staging.join(format!("{id}{TEXTURE_SET_SUFFIX}")), &set)
        .map_err(|err| err.to_string())?;
    if cancel.load(Ordering::Relaxed) {
        return Ok(DownloadResult::Cancelled);
    }
    let dest = project.join("assets").join("textures").join(id);
    let replaced = replace_dir(&staging, &dest, &root.join("previous"))?;
    let verb = if replaced { "Replaced" } else { "Saved" };
    Ok(DownloadResult::Saved(format!(
        "{verb} {name} (CC0) in assets/textures/{id}."
    )))
}

fn write_download(url: &str, file: &mut fs::File) -> Result<(), String> {
    let agent = http_agent(Duration::from_secs(15 * 60))?;
    let mut response = agent
        .get(url)
        .header("User-Agent", USER_AGENT)
        .call()
        .map_err(|err| err.to_string())?;
    let mut reader = response.body_mut().as_reader();
    copy_limited(&mut reader, file, ZIP_LIMIT)
}

fn extract_images(zip_path: &Path, dest: &Path) -> Result<Vec<String>, String> {
    let file = fs::File::open(zip_path).map_err(|err| err.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|err| err.to_string())?;
    let mut names = Vec::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|err| err.to_string())?;
        if entry.is_dir() {
            continue;
        }
        let Some(enclosed) = entry.enclosed_name() else {
            continue;
        };
        let Some(file_name) = enclosed.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if file_name.is_empty()
            || file_name.starts_with('.')
            || file_name == ".."
            || file_name.contains("..")
        {
            continue;
        }
        let extension = file_name
            .rsplit_once('.')
            .map(|(_, extension)| extension.to_ascii_lowercase())
            .unwrap_or_default();
        if !TEXTURE_EXTENSIONS.contains(&extension.as_str()) {
            continue;
        }
        if names.iter().any(|known| known == file_name) {
            continue;
        }
        let mut output = fs::File::create(dest.join(file_name)).map_err(|err| err.to_string())?;
        io::copy(&mut entry, &mut output).map_err(|err| err.to_string())?;
        names.push(file_name.to_owned());
    }
    if names.is_empty() {
        return Err("the package has no images".to_owned());
    }
    Ok(names)
}

/// Moves `staging` to `dest`, replacing `dest` when it is already there.
/// `backup` holds the old folder until the new one is in place.
fn replace_dir(staging: &Path, dest: &Path, backup: &Path) -> Result<bool, String> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let replaced = dest.exists();
    if replaced {
        let _ = fs::remove_dir_all(backup);
        fs::rename(dest, backup).map_err(|err| err.to_string())?;
    }
    if fs::rename(staging, dest).is_err()
        && let Err(err) = copy_dir(staging, dest)
    {
        if replaced {
            let _ = fs::rename(backup, dest);
        }
        return Err(err);
    }
    if replaced {
        let _ = fs::remove_dir_all(backup);
    }
    Ok(replaced)
}

fn copy_dir(from: &Path, to: &Path) -> Result<(), String> {
    fs::create_dir_all(to).map_err(|err| err.to_string())?;
    for entry in fs::read_dir(from).map_err(|err| err.to_string())? {
        let entry = entry.map_err(|err| err.to_string())?;
        let dest = to.join(entry.file_name());
        if entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
            copy_dir(&entry.path(), &dest)?;
        } else {
            fs::copy(entry.path(), &dest).map_err(|err| err.to_string())?;
        }
    }
    Ok(())
}

fn get_text(url: &str, timeout: Duration) -> Result<String, String> {
    let bytes = get_bytes(&http_agent(timeout)?, url, JSON_LIMIT)?;
    String::from_utf8(bytes).map_err(|_| "the catalog was not text".to_owned())
}

/// `agent` can be shared, so requests to the same host reuse a connection.
fn get_bytes(agent: &ureq::Agent, url: &str, limit: u64) -> Result<Vec<u8>, String> {
    if !url.starts_with("https://") {
        return Err("the address is not https".to_owned());
    }
    let mut response = agent
        .get(url)
        .header("User-Agent", USER_AGENT)
        .call()
        .map_err(|err| err.to_string())?;
    let mut reader = response.body_mut().as_reader();
    let mut bytes = Vec::new();
    copy_limited(&mut reader, &mut bytes, limit)?;
    Ok(bytes)
}

fn http_agent(timeout: Duration) -> Result<ureq::Agent, String> {
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .build();
    Ok(config.into())
}

fn copy_limited(reader: &mut impl Read, writer: &mut impl Write, limit: u64) -> Result<(), String> {
    let copied = io::copy(&mut reader.take(limit + 1), writer).map_err(|err| err.to_string())?;
    if copied > limit {
        Err("the download is too large".to_owned())
    } else {
        Ok(())
    }
}

fn encode(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn safe_asset_id(id: &str) -> bool {
    !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric())
}

fn parse_catalog(text: &str) -> Result<(usize, Vec<Listing>), String> {
    let json: serde_json::Value =
        serde_json::from_str(text).map_err(|_| "the catalog was not understood".to_owned())?;
    let total = json
        .get("numberOfResults")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0) as usize;
    let Some(assets) = json
        .get("foundAssets")
        .and_then(serde_json::Value::as_array)
    else {
        return Ok((total, Vec::new()));
    };
    let mut listings = Vec::new();
    for asset in assets {
        let Some(id) = asset.get("assetId").and_then(serde_json::Value::as_str) else {
            continue;
        };
        if !safe_asset_id(id) {
            continue;
        }
        let mut packages = Vec::new();
        if let Some(downloads) = asset
            .pointer("/downloadFolders/default/downloadFiletypeCategories/zip/downloads")
            .and_then(serde_json::Value::as_array)
        {
            for item in downloads {
                let Some(attribute) = item.get("attribute").and_then(serde_json::Value::as_str)
                else {
                    continue;
                };
                let Some(link) = item
                    .get("downloadLink")
                    .and_then(serde_json::Value::as_str)
                    .filter(|link| link.starts_with("https://ambientcg.com/"))
                else {
                    continue;
                };
                packages.push((attribute.to_owned(), link.to_owned()));
            }
        }
        // The larger preview when there is one, for the hover view.
        let preview = [
            "256-JPG-242424",
            "256-PNG",
            "256-JPG-FFFFFF",
            "128-JPG-242424",
            "128-PNG",
            "128-JPG-FFFFFF",
        ]
        .into_iter()
        .find_map(|key| {
            asset
                .pointer(&format!("/previewImage/{key}"))
                .and_then(serde_json::Value::as_str)
                .filter(|url| url.starts_with("https://"))
        })
        .map(str::to_owned);
        let maps = asset
            .get("maps")
            .and_then(serde_json::Value::as_array)
            .map(|maps| {
                maps.iter()
                    .filter_map(serde_json::Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        listings.push(Listing {
            id: id.to_owned(),
            name: asset
                .get("displayName")
                .and_then(serde_json::Value::as_str)
                .filter(|name| !name.is_empty())
                .unwrap_or(id)
                .to_owned(),
            category: asset
                .get("displayCategory")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
                .to_owned(),
            link: asset
                .get("shortLink")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
                .to_owned(),
            maps,
            preview,
            packages,
        });
    }
    Ok((total, listings))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_json_reads_a_material_and_its_package() {
        let text = r#"{
            "numberOfResults": 3,
            "foundAssets": [{
                "assetId": "Fabric086",
                "displayName": "Fabric 086",
                "displayCategory": "Fabric",
                "shortLink": "https://ambientcg.com/a/Fabric086",
                "maps": ["color", "normal"],
                "previewImage": { "128-JPG-242424": "https://example.com/Fabric086.jpg" },
                "downloadFolders": { "default": { "downloadFiletypeCategories": { "zip": { "downloads": [
                    { "attribute": "2K-JPG", "downloadLink": "https://ambientcg.com/get?file=Fabric086_2K-JPG.zip" },
                    { "attribute": "1K-PNG", "downloadLink": "https://evil.example/get" }
                ]}}}}
            }, { "assetId": "../nope" }]
        }"#;
        let (total, listings) = parse_catalog(text).unwrap();
        assert_eq!(total, 3);
        assert_eq!(listings.len(), 1);
        assert_eq!(listings[0].id, "Fabric086");
        assert_eq!(listings[0].packages.len(), 1);
        assert_eq!(listings[0].packages[0].0, "2K-JPG");
        assert!(listings[0].preview.is_some());
    }

    #[test]
    fn extraction_keeps_images_and_drops_paths_that_escape() {
        let root = std::env::temp_dir().join(format!("axcenity-zip-slip-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let zip_path = root.join("package.zip");
        let mut cursor = std::io::Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut cursor);
            let options = zip::write::SimpleFileOptions::default();
            writer
                .start_file("maps/Fabric086_Color.jpg", options)
                .unwrap();
            writer.write_all(b"color").unwrap();
            writer.start_file("../evil.png", options).unwrap();
            writer.write_all(b"no").unwrap();
            writer.start_file("readme.txt", options).unwrap();
            writer.write_all(b"skip").unwrap();
            writer.finish().unwrap();
        }
        fs::write(&zip_path, cursor.into_inner()).unwrap();
        let dest = root.join("out");
        fs::create_dir_all(&dest).unwrap();
        let names = extract_images(&zip_path, &dest).unwrap();
        assert_eq!(names, vec!["Fabric086_Color.jpg".to_owned()]);
        assert!(dest.join("Fabric086_Color.jpg").is_file());
        assert!(!root.join("evil.png").exists());
        let _ = fs::remove_dir_all(&root);
    }
}
