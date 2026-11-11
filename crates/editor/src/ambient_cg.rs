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
//! there, Get takes the nearest package and says so. Requests, the
//! download's size limit, unpacking and swapping the folder into the
//! project go through the Toolbox crate (`axcenity_toolbox`) too.

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
use axcenity_toolbox::http::{self, Http};
use axcenity_toolbox::variants::{self, Choice};
use axcenity_toolbox::{AssetRecord, AssetSource, Kind, Package, Query, Variant, files, guard};
use eframe::egui;

use crate::dialog;
use crate::theme::icons;

const PAGE: usize = 24;
const RESOLUTIONS: [&str; 4] = ["1K", "2K", "4K", "8K"];
const FORMATS: [&str; 2] = ["JPG", "PNG"];
/// A zip larger than this is refused, so a bad response can't fill the disk.
const ZIP_LIMIT: u64 = 2 * 1024 * 1024 * 1024;
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
    link: String,
    maps: Vec<String>,
    preview: Option<String>,
    /// Its zip downloads, such as "2K JPG", each one file from ambientCG.
    packages: Vec<Variant>,
}

impl Listing {
    /// A search result from the Toolbox's ambientCG source.
    fn from_record(record: AssetRecord) -> Self {
        Self {
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
                .filter(|variant| variant.package == Package::Archive && variant.files.len() == 1)
                .collect(),
            id: record.id,
            name: record.name,
            link: record.url,
        }
    }
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
                    plan: self.package(listing).map(|choice| choice.describe()),
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
        let resolution = self.resolution.to_ascii_lowercase();
        let format = self.format.to_ascii_lowercase();
        variants::choose(&listing.packages, &resolution, &format, &["jpg", "png"])
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
        let Some(choice) = self.package(listing) else {
            self.error = Some(format!("{} has no download.", listing.name));
            return;
        };
        let url = choice.variant.files[0].url.clone();
        let label = choice.variant.label.clone();
        let notes = choice.notes.join(" ");
        let name = listing.name.clone();
        let asset_id = listing.id.clone();
        let link = listing.link.clone();
        let project = project.to_owned();
        self.error = None;
        self.status = Some(
            format!("Downloading {name} ({label})… {notes}")
                .trim_end()
                .to_owned(),
        );
        self.cancel = Arc::new(AtomicBool::new(false));
        let cancel = Arc::clone(&self.cancel);
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = match download(&project, &asset_id, &name, &link, &url, &cancel) {
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

/// One page of popular materials matching `query`, through the Toolbox's
/// ambientCG source.
fn search(query: &str, offset: usize) -> Result<(usize, Vec<Listing>), String> {
    let client = http::Client::new(Duration::from_secs(30));
    let mut wanted = Query::new(query, Kind::Texture);
    wanted.per_page = PAGE as u32;
    wanted.page = (offset / PAGE) as u32 + 1;
    let page = AMBIENTCG
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
    if !guard::safe_asset_id(id) {
        return Err("the asset id is not a file name".to_owned());
    }
    if !AMBIENTCG.info().allows_download_from(url) {
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
    let client = http::Client::new(Duration::from_secs(15 * 60));
    client
        .fetch(url, ZIP_LIMIT, &mut zip)
        .map_err(|err| err.to_string())?;
    drop(zip);
    if cancel.load(Ordering::Relaxed) {
        return Ok(DownloadResult::Cancelled);
    }
    let names = guard::extract_files(&zip_path, &staging, &TEXTURE_EXTENSIONS)
        .map_err(|err| err.to_string())?;
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
    let replaced = files::replace_dir(&staging, &dest, &root.join("previous"))
        .map_err(|err| err.to_string())?;
    let verb = if replaced { "Replaced" } else { "Saved" };
    Ok(DownloadResult::Saved(format!(
        "{verb} {name} (CC0) in assets/textures/{id}."
    )))
}

#[cfg(test)]
mod tests {
    use axcenity_toolbox::RemoteFile;

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

    /// The whole Get path against the real ambientCG: search, choose the
    /// nearest to 1K JPG, download, unpack and save the texture set.
    /// Reaches the network, so run it by hand:
    /// `cargo nextest run -p axcenity-editor --run-ignored only -E 'test(ambient_cg)'`
    #[test]
    #[ignore = "reaches ambientCG"]
    fn get_downloads_a_real_material_into_a_project() {
        let (_, listings) = search("ground", 0).unwrap();
        let listing = listings
            .into_iter()
            .find(|l| !l.packages.is_empty())
            .unwrap();
        let choice = variants::choose(&listing.packages, "1k", "jpg", &["png"]).unwrap();
        let project =
            std::env::temp_dir().join(format!("axcenity-get-texture-{}", std::process::id()));
        let _ = fs::remove_dir_all(&project);
        let cancel = AtomicBool::new(false);
        let url = &choice.variant.files[0].url;
        let saved = download(
            &project,
            &listing.id,
            &listing.name,
            &listing.link,
            url,
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
        let _ = fs::remove_dir_all(&project);
    }

    #[test]
    fn a_listing_keeps_only_single_zip_downloads_and_names_its_maps() {
        let mut single = zip("8K EXR", "8k", "exr");
        single.package = Package::Single;
        let record = AssetRecord {
            provider: "ambientcg".into(),
            id: "Fabric086".into(),
            name: "Fabric 086".into(),
            url: "https://ambientcg.com/a/Fabric086".into(),
            thumbnail_url: "https://example.com/Fabric086.png".into(),
            maps: vec!["base_color".into(), "normal".into(), "ao".into()],
            variants: vec![zip("2K JPG", "2k", "jpg"), single],
            ..AssetRecord::default()
        };
        let listing = Listing::from_record(record);
        assert_eq!(listing.id, "Fabric086");
        assert_eq!(listing.link, "https://ambientcg.com/a/Fabric086");
        assert_eq!(listing.maps, ["color", "normal", "occlusion"]);
        assert_eq!(listing.packages.len(), 1);
        assert!(listing.preview.is_some());
    }

    #[test]
    fn get_offers_the_nearest_package_when_the_chosen_one_is_missing() {
        let ctx = egui::Context::default();
        let mut dialog = GetTexture::open(&ctx);
        dialog.search = None;
        let listing = Listing {
            id: "Fabric086".into(),
            name: "Fabric 086".into(),
            category: String::new(),
            link: String::new(),
            maps: Vec::new(),
            preview: None,
            packages: vec![zip("1K JPG", "1k", "jpg"), zip("4K PNG", "4k", "png")],
        };
        dialog.resolution = "2K".into();
        dialog.format = "JPG".into();
        let choice = dialog.package(&listing).unwrap();
        assert_eq!(choice.variant.label, "1K JPG");
        assert_eq!(choice.notes, ["No 2K here, so 1K, the nearest."]);
        dialog.format = "PNG".into();
        assert_eq!(dialog.package(&listing).unwrap().variant.label, "4K PNG");
        let empty = Listing {
            packages: Vec::new(),
            ..listing
        };
        assert!(dialog.package(&empty).is_none());
    }
}
