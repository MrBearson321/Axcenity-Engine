//! The Downloaded / Imported Assets tab: one list of every asset that came
//! from outside the editor, whether it was downloaded into this computer's
//! asset library, imported into the open project, or both.
//!
//! Each asset is one row however many times it was downloaded or imported:
//! a Toolbox asset by its `provider:id` (its variants in the library, its
//! installs and the project's files all on that row), anything else by its
//! project path, and an imported model's folder as one row with its
//! textures and clips. Rows are grouped by type (Models, Textures,
//! Materials, HDRIs, Sounds, Animations, Other) and say whether the asset is
//! **Downloaded** (in the library), **Imported** (brought into this
//! project) and **In project** (its files are there now; an install whose
//! files were deleted says **Removed from project**). The details show its
//! source, licence and credit, the library's variants with their sizes, and
//! the project's files, with **Show in Assets**, **Open in Toolbox**, **Open
//! library folder** and **Remove download** (the project keeps its copy).
//!
//! What it reads: the Toolbox's metadata database (downloads and this
//! project's installs), the project's `.axcenity/asset-sources.json`, and
//! the Assets panel's scan and origins (`asset_browser/origin.rs`). It reads
//! them again when the project's files change, every few seconds while
//! shown, and after a Toolbox download or a removal.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use axcenity_toolbox::db::{Database, Download, Install};
use axcenity_toolbox::directory::{self, AssetDirectory};
use axcenity_toolbox::library::Filter;
use axcenity_toolbox::provenance::{self, Snapshot};
use axcenity_toolbox::{AssetRecord, Kind, license};
use eframe::egui;

use crate::asset_browser::origin::{Origin, Origins};
use crate::asset_browser::packages::Package;
use crate::asset_browser::{AssetEntry, AssetKind};
use crate::theme::{self, icons};
use crate::{responsive, widgets};

/// How often the lists are read again while the tab is shown.
const REREAD: Duration = Duration::from_secs(3);
/// From this width the details sit beside the list.
const DETAILS_BESIDE_FROM: f32 = 640.0;
/// The most project files the details name before "and N more".
const FILES_SHOWN: usize = 8;

/// The tab's groups, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Category {
    Models,
    Textures,
    Materials,
    Hdris,
    Sounds,
    Animations,
    Other,
}

impl Category {
    pub const ALL: [Category; 7] = [
        Category::Models,
        Category::Textures,
        Category::Materials,
        Category::Hdris,
        Category::Sounds,
        Category::Animations,
        Category::Other,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Category::Models => "Models",
            Category::Textures => "Textures",
            Category::Materials => "Materials",
            Category::Hdris => "HDRIs",
            Category::Sounds => "Sounds",
            Category::Animations => "Animations",
            Category::Other => "Other",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Category::Models => icons::CUBE,
            Category::Textures => icons::IMAGE,
            Category::Materials => icons::SPHERE,
            Category::Hdris => icons::SUN_HORIZON,
            Category::Sounds => icons::WAVEFORM,
            Category::Animations => icons::FILM_STRIP,
            Category::Other => icons::FILE,
        }
    }

    /// A Toolbox record's group.
    fn of_kind(kind: Kind) -> Category {
        match kind {
            Kind::Texture => Category::Textures,
            Kind::Hdri => Category::Hdris,
            Kind::Model => Category::Models,
            Kind::Sound => Category::Sounds,
            Kind::Sprites => Category::Other,
        }
    }

    /// A project file's group.
    fn of_asset(kind: AssetKind) -> Category {
        match kind {
            AssetKind::Model | AssetKind::Prop | AssetKind::Prefab => Category::Models,
            AssetKind::Texture | AssetKind::TextureSet => Category::Textures,
            AssetKind::Material => Category::Materials,
            AssetKind::Sky => Category::Hdris,
            AssetKind::Audio => Category::Sounds,
            AssetKind::Animation | AssetKind::Controller | AssetKind::Sequence => {
                Category::Animations
            }
            AssetKind::Image
            | AssetKind::Particle
            | AssetKind::BehaviorTree
            | AssetKind::DataTable => Category::Other,
        }
    }
}

/// Which rows the tab lists.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Showing {
    #[default]
    All,
    /// Only what's in the asset library.
    Downloaded,
    /// Only what was brought into this project.
    Imported,
}

/// One asset: everything known about it, on one row.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    /// `provider:id` for a Toolbox asset, else its main file's project path.
    pub key: String,
    pub name: String,
    pub category: Category,
    pub origin: Origin,
    /// The Toolbox record: source, licence, authors.
    pub record: Option<AssetRecord>,
    /// Its variants in the asset library, newest first.
    pub downloads: Vec<Download>,
    /// Brought into this project: installed by the Toolbox, or imported.
    pub imported: bool,
    /// Its files in the project now, the main one first (what Show in
    /// Assets picks).
    pub files: Vec<String>,
    /// "1 model, 2 textures" for a model's folder; the texture set a
    /// material was made from.
    pub note: Option<String>,
}

impl Item {
    pub fn downloaded(&self) -> bool {
        !self.downloads.is_empty()
    }

    pub fn in_project(&self) -> bool {
        !self.files.is_empty()
    }

    /// The library's copies' total size.
    pub fn bytes(&self) -> u64 {
        self.downloads.iter().map(|download| download.bytes).sum()
    }

    fn shown_in(&self, showing: Showing) -> bool {
        match showing {
            Showing::All => true,
            Showing::Downloaded => self.downloaded(),
            Showing::Imported => self.imported,
        }
    }

    fn matches(&self, search: &str) -> bool {
        let mut haystack = format!("{} {} {}", self.name, self.key, self.origin.label());
        if let Some(record) = &self.record {
            haystack.push(' ');
            haystack.push_str(&record.authors.join(" "));
            haystack.push(' ');
            haystack.push_str(&record.tags.join(" "));
        }
        for file in &self.files {
            haystack.push(' ');
            haystack.push_str(file);
        }
        let haystack = haystack.to_lowercase();
        search
            .split_whitespace()
            .all(|word| haystack.contains(word))
    }

    /// The badges after its name.
    pub fn badges(&self) -> Vec<&'static str> {
        let mut badges = Vec::new();
        if self.downloaded() {
            badges.push("Downloaded");
        }
        if self.imported && !self.in_project() {
            badges.push("Removed from project");
        } else if self.imported {
            badges.push("Imported");
        }
        if self.in_project() {
            badges.push("In project");
        }
        badges
    }
}

/// What the tab is built from, read by [`AssetInventory::read`].
pub struct Inputs<'a> {
    /// Every asset with a download in the library, and its downloads.
    pub library: Vec<(AssetRecord, Vec<Download>)>,
    /// What the Toolbox installed into this project.
    pub installs: Vec<Install>,
    pub snapshot: &'a Snapshot,
    /// The project's files (not the global ones it hasn't got), as the
    /// Assets panel lists them.
    pub files: &'a [AssetEntry],
    pub packages: &'a [Package],
    pub origins: &'a Origins,
}

/// The rows, by type then name. An asset downloaded in several variants,
/// installed more than once or with many files is still one row.
pub fn gather(inputs: &Inputs) -> Vec<Item> {
    let mut items: BTreeMap<String, Item> = BTreeMap::new();
    let toolbox_item = |record: &AssetRecord| Item {
        key: record.key(),
        name: record.name.clone(),
        category: Category::of_kind(record.kind),
        origin: Origin::Toolbox(record.provider.clone()),
        record: Some(record.clone()),
        downloads: Vec::new(),
        imported: false,
        files: Vec::new(),
        note: None,
    };
    for (record, downloads) in &inputs.library {
        let item = items
            .entry(record.key())
            .or_insert_with(|| toolbox_item(record));
        item.downloads.extend(downloads.iter().cloned());
    }
    // Installs, here and (from the snapshot) on other computers.
    let installed = inputs
        .installs
        .iter()
        .map(|install| install.key.clone())
        .chain(inputs.snapshot.files.iter().map(|file| file.key.clone()));
    for key in installed {
        if let Some(item) = items.get_mut(&key) {
            item.imported = true;
        } else if let Some(record) = inputs.snapshot.assets.get(&key) {
            let mut item = toolbox_item(record);
            item.imported = true;
            items.insert(key, item);
        }
    }
    // The project's files: a model's folder as one, then each file.
    let mut in_package = std::collections::HashSet::new();
    for package in inputs.packages {
        if !inputs.files.iter().any(|entry| package.holds(entry)) {
            continue;
        }
        let paths: Vec<String> = package.members.iter().map(|m| m.path.clone()).collect();
        in_package.extend(paths.iter().cloned());
        let origin = inputs.origins.of(&package.model);
        if !brought_in(&origin) {
            continue;
        }
        let key = toolbox_key(inputs.snapshot, &package.model.path)
            .unwrap_or_else(|| package.model.path.clone());
        let item = items.entry(key.clone()).or_insert_with(|| Item {
            key,
            name: package.name.clone(),
            category: Category::Models,
            origin: origin.clone(),
            record: None,
            downloads: Vec::new(),
            imported: true,
            files: Vec::new(),
            note: None,
        });
        item.imported = true;
        item.category = Category::Models;
        item.note = Some(package.summary());
        item.files.extend(paths);
    }
    for entry in inputs.files {
        if entry.kind == AssetKind::Prop || in_package.contains(&entry.path) {
            continue;
        }
        let origin = inputs.origins.of(entry);
        if !brought_in(&origin) {
            continue;
        }
        let parent = inputs.origins.parent(&entry.path);
        // A material made from a set is its own row, noting the set.
        let key = match entry.kind {
            AssetKind::Material => entry.path.clone(),
            _ => toolbox_key(inputs.snapshot, &entry.path).unwrap_or_else(|| entry.path.clone()),
        };
        let item = items.entry(key.clone()).or_insert_with(|| Item {
            key,
            name: entry.name.clone(),
            category: Category::of_asset(entry.kind),
            origin: origin.clone(),
            record: None,
            downloads: Vec::new(),
            imported: true,
            files: Vec::new(),
            note: parent.map(|set| {
                format!(
                    "Made from the texture set {}",
                    crate::asset_browser::display_name(set)
                )
            }),
        });
        item.imported = true;
        item.files.push(entry.path.clone());
    }
    let mut items: Vec<Item> = items.into_values().collect();
    items.sort_by(|a, b| {
        (a.category, a.name.to_lowercase(), &a.key).cmp(&(
            b.category,
            b.name.to_lowercase(),
            &b.key,
        ))
    });
    items
}

/// Where a row came from, as its second line says it.
fn source_text(origin: &Origin) -> String {
    match origin {
        Origin::Imported => "From this computer".to_owned(),
        other => other.label(),
    }
}

/// Whether assets of this origin came from outside the editor.
fn brought_in(origin: &Origin) -> bool {
    matches!(origin, Origin::Imported | Origin::Toolbox(_))
}

fn toolbox_key(snapshot: &Snapshot, path: &str) -> Option<String> {
    snapshot.source_of(path).map(AssetRecord::key)
}

/// What the editor should do after the tab drew.
#[derive(Clone, Debug, PartialEq)]
pub enum InventoryAction {
    /// Show this project file in the Assets panel.
    Reveal(String),
    /// Show this asset in the Toolbox: in its Downloaded view when it's in
    /// the library, else in This project.
    OpenInToolbox {
        record: Box<AssetRecord>,
        downloaded: bool,
    },
    /// A line for Output.
    Notice { error: bool, text: String },
}

/// The Downloaded / Imported Assets tab.
#[derive(Default)]
pub struct AssetInventory {
    showing: Showing,
    category: Option<Category>,
    source: Option<Origin>,
    search: String,
    items: Vec<Item>,
    selected: Option<String>,
    /// When, for which project and Assets scan, the lists were read.
    read_at: Option<Instant>,
    read_for: Option<(Option<PathBuf>, u64)>,
    /// Why the library couldn't be read, shown above the list.
    library_problem: Option<String>,
    /// The row whose Remove download waits for a second click.
    confirm_remove: Option<String>,
}

impl AssetInventory {
    /// Whether the lists should be read again for `project` and the Assets
    /// panel's scan `generation`.
    pub fn wants_reading(&self, project: Option<&Path>, generation: u64) -> bool {
        let current = (project.map(Path::to_owned), generation);
        self.read_for.as_ref() != Some(&current)
            || self.read_at.is_none_or(|at| at.elapsed() >= REREAD)
    }

    /// Reads them again on the next frame it's shown.
    pub fn refresh(&mut self) {
        self.read_at = None;
    }

    /// Reads the library and the project into rows. `files`, `packages`,
    /// `origins` and `snapshot` are the Assets panel's.
    pub fn read(
        &mut self,
        project: Option<&Path>,
        generation: u64,
        files: &[AssetEntry],
        packages: &[Package],
        origins: &Origins,
        snapshot: &Snapshot,
    ) {
        self.read_at = Some(Instant::now());
        self.read_for = Some((project.map(Path::to_owned), generation));
        let (library, installs) = match open_library() {
            Ok((_, db)) => {
                self.library_problem = None;
                (
                    downloaded(&db).unwrap_or_default(),
                    project
                        .and_then(|project| db.installs(&provenance::project_key(project)).ok())
                        .unwrap_or_default(),
                )
            }
            Err(problem) => {
                self.library_problem = Some(problem);
                (Vec::new(), Vec::new())
            }
        };
        self.items = gather(&Inputs {
            library,
            installs,
            snapshot,
            files,
            packages,
            origins,
        });
        if let Some(key) = &self.selected
            && !self.items.iter().any(|item| &item.key == key)
        {
            self.selected = None;
        }
    }

    #[cfg(test)]
    pub fn set_items(&mut self, items: Vec<Item>) {
        self.items = items;
        self.read_at = Some(Instant::now());
    }

    #[cfg(test)]
    pub fn select(&mut self, key: &str) {
        self.selected = Some(key.to_owned());
    }

    /// Draws the tab. `has_project` is false with no project open.
    pub fn show(&mut self, ui: &mut egui::Ui, has_project: bool) -> Vec<InventoryAction> {
        let mut actions = Vec::new();
        self.toolbar(ui);
        if let Some(problem) = &self.library_problem {
            ui.add(
                egui::Label::new(
                    egui::RichText::new(format!(
                        "{} The asset library can't be read, so only this project's assets \
                         show: {problem}",
                        icons::WARNING
                    ))
                    .small()
                    .color(ui.visuals().warn_fg_color),
                )
                .wrap(),
            );
        }
        ui.add_space(4.0);
        widgets::divider(ui);
        let shown: Vec<&Item> = self.shown();
        if shown.is_empty() {
            let (title, sentence) = if self.items.is_empty() {
                (
                    "Nothing downloaded or imported yet",
                    if has_project {
                        "Get assets from the Toolbox, or Import files from this computer. \
                         They're listed here."
                    } else {
                        "Open a project to see what it imported. Downloads in the asset \
                         library show here too."
                    },
                )
            } else {
                ("No matches", "Nothing downloaded or imported matches that.")
            };
            widgets::empty_state(ui, icons::TRAY_ARROW_DOWN, title, sentence, None);
            return actions;
        }
        let shown: Vec<Item> = shown.into_iter().cloned().collect();
        let selected = self
            .selected
            .as_ref()
            .and_then(|key| shown.iter().find(|item| &item.key == key))
            .cloned();
        // The room below the toolbar, measured before a split takes it.
        let height = ui.available_height();
        let stacked = selected.is_some() && ui.available_width() < DETAILS_BESIDE_FROM;
        let list = |this: &mut Self, ui: &mut egui::Ui| {
            // Stacked above the details, the list takes half the height.
            egui::ScrollArea::vertical()
                .id_salt("inventory list")
                .auto_shrink([false, stacked])
                .max_height(if stacked {
                    (height * 0.5).max(120.0)
                } else {
                    height
                })
                .show(ui, |ui| this.rows(ui, &shown));
        };
        match &selected {
            Some(item) => {
                responsive::split(ui, DETAILS_BESIDE_FROM, 0.55, |ui, pane| {
                    if pane == 0 {
                        list(self, ui);
                    } else {
                        egui::ScrollArea::vertical()
                            .id_salt("inventory details")
                            .auto_shrink([false, true])
                            .max_height(if stacked { height * 0.5 } else { height })
                            .show(ui, |ui| self.details(ui, item, has_project, &mut actions));
                    }
                });
            }
            None => list(self, ui),
        }
        actions
    }

    /// The rows the filters let through, in order.
    fn shown(&self) -> Vec<&Item> {
        let search = self.search.trim().to_lowercase();
        self.items
            .iter()
            .filter(|item| item.shown_in(self.showing))
            .filter(|item| {
                self.category
                    .is_none_or(|category| item.category == category)
            })
            .filter(|item| {
                self.source
                    .as_ref()
                    .is_none_or(|source| &item.origin == source)
            })
            .filter(|item| search.is_empty() || item.matches(&search))
            .collect()
    }

    /// All, Downloaded or Imported; the search; the type chips with counts;
    /// the Source menu.
    fn toolbar(&mut self, ui: &mut egui::Ui) {
        responsive::toolbar(ui, |ui| {
            widgets::segmented_tips(
                ui,
                &mut self.showing,
                &[
                    (Showing::All, None, "All"),
                    (Showing::Downloaded, None, "Downloaded"),
                    (Showing::Imported, None, "Imported"),
                ],
                &[
                    "Everything downloaded or imported.",
                    "In the asset library on this computer, which every project can use \
                     without downloading again.",
                    "Brought into this project: got from the Toolbox, or imported from this \
                     computer.",
                ],
            );
            responsive::room_for(ui, 160.0);
            widgets::search_field(ui, &mut self.search, "Search assets", 150.0);
        });
        ui.add_space(2.0);
        let counts: Vec<(Category, usize)> = Category::ALL
            .iter()
            .map(|&category| {
                let count = self
                    .items
                    .iter()
                    .filter(|item| item.shown_in(self.showing) && item.category == category)
                    .count();
                (category, count)
            })
            .collect();
        let mut origins: Vec<Origin> = self.items.iter().map(|item| item.origin.clone()).collect();
        origins.sort();
        origins.dedup();
        responsive::toolbar(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            if widgets::chip(ui, "All", self.category.is_none()).clicked() {
                self.category = None;
            }
            for (category, count) in counts {
                if count == 0 && self.category != Some(category) {
                    continue;
                }
                let on = self.category == Some(category);
                if widgets::chip(ui, &format!("{} {count}", category.label()), on).clicked() {
                    self.category = if on { None } else { Some(category) };
                }
            }
            responsive::room_for(ui, 140.0);
            let selected = self
                .source
                .as_ref()
                .map_or("All sources".to_owned(), Origin::label);
            egui::ComboBox::from_id_salt("inventory source")
                .selected_text(theme::with_icon(icons::FUNNEL, &selected))
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(self.source.is_none(), "All sources")
                        .clicked()
                    {
                        self.source = None;
                    }
                    for origin in origins {
                        let on = self.source.as_ref() == Some(&origin);
                        if ui
                            .selectable_label(on, theme::with_icon(origin.icon(), &origin.label()))
                            .clicked()
                        {
                            self.source = Some(origin);
                        }
                    }
                })
                .response
                .on_hover_text("Show the assets from one source.");
        });
    }

    /// The rows under their types' headings.
    fn rows(&mut self, ui: &mut egui::Ui, shown: &[Item]) {
        let palette = theme::palette(ui.ctx());
        let mut current = None;
        for item in shown {
            if current != Some(item.category) {
                current = Some(item.category);
                let count = shown.iter().filter(|i| i.category == item.category).count();
                ui.add_space(6.0);
                widgets::section_label(
                    ui,
                    &format!(
                        "{} {}  {count}",
                        item.category.icon(),
                        item.category.label()
                    ),
                );
                ui.add_space(2.0);
            }
            let selected = self.selected.as_deref() == Some(item.key.as_str());
            let width = ui.available_width();
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(width, 40.0), egui::Sense::click());
            widgets::paint_hover(ui, rect, &response, selected, theme::radius::CARD);
            let painter = ui.painter_at(rect);
            let left = rect.min.x + 8.0;
            let badges = item.badges();
            let badge_text = badges.join(" · ");
            // The badges at the right when there's room, else under the source.
            let wide = width >= 360.0;
            let mut name_right = rect.max.x - 8.0;
            if wide {
                let galley = painter.layout_no_wrap(
                    badge_text.clone(),
                    egui::FontId::proportional(theme::type_scale::SMALL),
                    palette.accent_text,
                );
                name_right = rect.max.x - 8.0 - galley.size().x - 8.0;
                painter.galley(
                    egui::pos2(
                        rect.max.x - 8.0 - galley.size().x,
                        rect.center().y - galley.size().y / 2.0,
                    ),
                    galley,
                    palette.accent_text,
                );
            }
            let name = widgets::elided(
                ui,
                &item.name,
                egui::FontId::proportional(theme::type_scale::BODY),
                palette.text_strong,
                (name_right - left).max(20.0),
                1,
            );
            painter.galley(
                egui::pos2(left, rect.min.y + 4.0),
                name,
                palette.text_strong,
            );
            let mut detail = source_text(&item.origin);
            if let Some(record) = &item.record {
                detail.push_str(" · ");
                detail.push_str(&license::display(&record.license, &record.license_version));
            }
            if !wide {
                detail.push_str(" · ");
                detail.push_str(&badge_text);
            }
            let detail = widgets::elided(
                ui,
                &detail,
                egui::FontId::proportional(theme::type_scale::SMALL),
                palette.text_weak,
                (name_right - left).max(20.0),
                1,
            );
            painter.galley(
                egui::pos2(left, rect.min.y + 22.0),
                detail,
                palette.text_weak,
            );
            let tip = match &item.note {
                Some(note) => format!("{}\n{note}\n{}", item.name, badge_text),
                None => format!("{}\n{}", item.name, badge_text),
            };
            if response.on_hover_text(tip).clicked() {
                self.selected = if selected {
                    None
                } else {
                    Some(item.key.clone())
                };
                self.confirm_remove = None;
            }
        }
    }

    /// The selected asset's details and what can be done with it.
    fn details(
        &mut self,
        ui: &mut egui::Ui,
        item: &Item,
        has_project: bool,
        actions: &mut Vec<InventoryAction>,
    ) {
        let palette = theme::palette(ui.ctx());
        ui.horizontal(|ui| {
            ui.add(
                egui::Label::new(
                    egui::RichText::new(&item.name)
                        .strong()
                        .size(theme::type_scale::BODY + 2.0),
                )
                .wrap(),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                if ui
                    .small_button(icons::X)
                    .on_hover_text("Close the details")
                    .clicked()
                {
                    self.selected = None;
                }
            });
        });
        ui.label(
            egui::RichText::new(format!(
                "{} · {}",
                item.category.label(),
                item.badges().join(" · ")
            ))
            .color(palette.accent_text),
        );
        if let Some(note) = &item.note {
            ui.label(egui::RichText::new(note).weak());
        }
        ui.add_space(4.0);
        // Where it came from and its licence.
        match &item.record {
            Some(record) => {
                ui.horizontal_wrapped(|ui| {
                    ui.label(format!("From {}", item.origin.label()));
                    if record.url.starts_with("https://") {
                        ui.hyperlink_to("Source page", &record.url);
                    }
                });
                let authors: Vec<&str> = record
                    .authors
                    .iter()
                    .map(String::as_str)
                    .filter(|author| !author.trim().is_empty())
                    .collect();
                if !authors.is_empty() {
                    ui.label(format!("By {}", authors.join(", ")));
                }
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        egui::RichText::new(license::display(
                            &record.license,
                            &record.license_version,
                        ))
                        .strong(),
                    );
                    if record.license_url.starts_with("https://") {
                        ui.hyperlink_to("Licence", &record.license_url);
                    }
                });
                ui.add(egui::Label::new(license::plain_words(&record.license)).wrap());
                if let Some(why) = license::commercial_block(&record.license) {
                    ui.colored_label(
                        ui.visuals().warn_fg_color,
                        format!("{} Not for a commercial game: {why}.", icons::WARNING),
                    );
                }
                if ui
                    .small_button(theme::with_icon(icons::COPY, "Copy credit line"))
                    .clicked()
                {
                    ui.ctx().copy_text(license::credit_line(
                        record,
                        &provenance::source_name(&record.provider),
                    ));
                    actions.push(InventoryAction::Notice {
                        error: false,
                        text: format!("Copied the credit line for {}.", item.name),
                    });
                }
            }
            None => {
                ui.add(
                    egui::Label::new(match &item.origin {
                        Origin::Imported => {
                            "Imported from this computer. Axcenity doesn't know its licence: \
                             check where it came from before using it in a game."
                        }
                        _ => "Made in Axcenity from the assets above it.",
                    })
                    .wrap(),
                );
            }
        }
        // The library's copies.
        if item.downloaded() {
            ui.add_space(6.0);
            widgets::section_label(ui, "In the asset library");
            for download in &item.downloads {
                ui.label(format!(
                    "{} · {} · {}",
                    download.variant,
                    crate::toolbox_panel::size_text(download.bytes),
                    age_text(download.fetched)
                ));
            }
        }
        // The project's files.
        ui.add_space(6.0);
        widgets::section_label(ui, "In this project");
        if item.files.is_empty() {
            ui.label(
                egui::RichText::new(if item.imported {
                    "Its files were removed from the project."
                } else {
                    "Not in this project. Get it from the Toolbox to use it."
                })
                .weak(),
            );
        } else {
            for file in item.files.iter().take(FILES_SHOWN) {
                ui.add(egui::Label::new(egui::RichText::new(file).small()).truncate());
            }
            if item.files.len() > FILES_SHOWN {
                ui.label(
                    egui::RichText::new(format!("and {} more", item.files.len() - FILES_SHOWN))
                        .small()
                        .weak(),
                );
            }
        }
        ui.add_space(6.0);
        responsive::toolbar(ui, |ui| {
            if let Some(first) = item.files.first()
                && ui
                    .button(theme::with_icon(icons::SHAPES, "Show in Assets"))
                    .clicked()
            {
                actions.push(InventoryAction::Reveal(first.clone()));
            }
            if let Some(record) = &item.record
                && ui
                    .add_enabled(
                        has_project || item.downloaded(),
                        egui::Button::new(theme::with_icon(icons::TOOLBOX, "Open in Toolbox")),
                    )
                    .clicked()
            {
                actions.push(InventoryAction::OpenInToolbox {
                    record: Box::new(record.clone()),
                    downloaded: item.downloaded(),
                });
            }
            if let Some(download) = item.downloads.first()
                && ui
                    .button(theme::with_icon(icons::FOLDER_OPEN, "Open library folder"))
                    .on_hover_text("Show this download's files in the file manager.")
                    .clicked()
            {
                match library_folder(download) {
                    Some(folder) => {
                        if let Err(err) = open_folder(&folder) {
                            actions.push(InventoryAction::Notice {
                                error: true,
                                text: format!("Could not open {}: {err}", folder.display()),
                            });
                        }
                    }
                    None => actions.push(InventoryAction::Notice {
                        error: true,
                        text: "There is no asset library on this computer.".to_owned(),
                    }),
                }
            }
        });
        if item.downloaded() {
            let confirming = self.confirm_remove.as_deref() == Some(item.key.as_str());
            let label = if confirming {
                format!(
                    "Remove {} from the library? Click again",
                    crate::toolbox_panel::size_text(item.bytes())
                )
            } else {
                "Remove download".to_owned()
            };
            if ui
                .button(theme::with_icon(icons::TRASH, &label))
                .on_hover_text(
                    "Delete the library's copies to free space. Projects keep their own copies; \
                     the Toolbox can download it again.",
                )
                .clicked()
            {
                if confirming {
                    self.confirm_remove = None;
                    actions.push(remove_downloads(item));
                    self.refresh();
                } else {
                    self.confirm_remove = Some(item.key.clone());
                }
            }
        }
    }
}

/// Every asset with a download, and its downloads.
fn downloaded(db: &Database) -> Result<Vec<(AssetRecord, Vec<Download>)>, String> {
    let records = db
        .assets(&Filter {
            downloaded: true,
            ..Filter::default()
        })
        .map_err(|err| err.to_string())?;
    records
        .into_iter()
        .map(|record| {
            let downloads = db
                .downloads_of(&record.key())
                .map_err(|err| err.to_string())?;
            Ok((record, downloads))
        })
        .collect()
}

/// The asset directory and its database, opened for a moment.
fn open_library() -> Result<(AssetDirectory, Database), String> {
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

fn library_folder(download: &Download) -> Option<PathBuf> {
    let root = directory::locate()?;
    let dir = AssetDirectory::open(&root).ok()?;
    Some(dir.path(&download.folder))
}

/// Deletes `item`'s downloads from the asset library and forgets them. The
/// asset's record, favourites and installs stay.
fn remove_downloads(item: &Item) -> InventoryAction {
    match open_library().and_then(|(dir, db)| remove_from(&dir, &db, item)) {
        Ok(()) => InventoryAction::Notice {
            error: false,
            text: format!(
                "Removed {} from the asset library. Projects that use it keep their copy.",
                item.name
            ),
        },
        Err(err) => InventoryAction::Notice {
            error: true,
            text: format!("Could not remove {}: {err}", item.name),
        },
    }
}

fn remove_from(dir: &AssetDirectory, db: &Database, item: &Item) -> Result<(), String> {
    for download in &item.downloads {
        // Only folders inside the directory, as the database names them.
        if download.folder.is_empty()
            || axcenity_toolbox::guard::safe_relative_path(&download.folder).is_none()
        {
            return Err("its folder isn't inside the asset library".to_owned());
        }
        let folder = dir.path(&download.folder);
        match std::fs::remove_dir_all(&folder) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(err.to_string()),
        }
        db.forget_download(&download.key, &download.variant)
            .map_err(|err| err.to_string())?;
    }
    Ok(())
}

/// Opens `folder` in the system's file manager.
fn open_folder(folder: &Path) -> std::io::Result<()> {
    let opener = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    std::process::Command::new(opener).arg(folder).spawn()?;
    Ok(())
}

/// How long ago a download was fetched: "today", "yesterday", "12 days
/// ago".
fn age_text(fetched: i64) -> String {
    let days = (axcenity_toolbox::db::now() - fetched).max(0) / 86_400;
    match days {
        0 => "today".to_owned(),
        1 => "yesterday".to_owned(),
        days => format!("{days} days ago"),
    }
}

#[cfg(test)]
mod tests;
