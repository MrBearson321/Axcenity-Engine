//! The start screen shown while no project is open: a sidebar with the
//! logo and the Projects and Learn pages; on Projects, a welcome banner with
//! New and Open Project, templates to start from, and the tracked projects
//! as cards (or a list) with thumbnails, a search box and sorting. Also the
//! New Project, Project Settings and Delete dialogs, the About box, and the
//! logo images.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use axcenity_project::Template;
use eframe::egui::{self, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, pos2, vec2};

use crate::theme::{self, icons};

/// How many projects the start screen tracks.
pub const MAX_TRACKED: usize = 100;
/// How wide the wordmark is drawn in the About box, in points. The images
/// are twice this, so they stay sharp on high-DPI screens.
const WORDMARK_WIDTH: f32 = 420.0;
/// How wide the wordmark is drawn in the sidebar.
const SIDEBAR_WORDMARK_WIDTH: f32 = 168.0;
const SIDEBAR_WIDTH: f32 = 216.0;
/// The width of a project card, and the space between cards.
const CARD_WIDTH: f32 = 264.0;
const CARD_SPACING: f32 = 18.0;
const CARD_RADIUS: u8 = 12;
/// The page's margin left and right of its content.
const PAGE_MARGIN: f32 = 36.0;

/// The logo images, as textures egui can draw.
pub struct Brand {
    /// The wordmark for light backgrounds, and for dark ones.
    wordmark: egui::TextureHandle,
    wordmark_dark: egui::TextureHandle,
    /// The same, made smaller for the sidebar: egui has no mipmaps, so an
    /// image drawn at under half its size looks jagged.
    wordmark_small: egui::TextureHandle,
    wordmark_dark_small: egui::TextureHandle,
}

impl Brand {
    pub fn load(ctx: &egui::Context) -> Self {
        Self {
            wordmark: texture(
                ctx,
                "wordmark",
                include_bytes!("../../../assets/branding/wordmark.png"),
            ),
            wordmark_dark: texture(
                ctx,
                "wordmark dark",
                include_bytes!("../../../assets/branding/wordmark-dark.png"),
            ),
            wordmark_small: texture(
                ctx,
                "wordmark small",
                include_bytes!("../../../assets/branding/wordmark-small.png"),
            ),
            wordmark_dark_small: texture(
                ctx,
                "wordmark dark small",
                include_bytes!("../../../assets/branding/wordmark-dark-small.png"),
            ),
        }
    }

    /// The wordmark that reads well on the current theme's background, in
    /// the size nearest twice `width` (in points), so it stays sharp.
    pub fn wordmark(&self, visuals: &egui::Visuals, width: f32) -> &egui::TextureHandle {
        let small = width * 2.0 <= self.wordmark_small.size_vec2().x * 1.25;
        match (visuals.dark_mode, small) {
            (true, true) => &self.wordmark_dark_small,
            (true, false) => &self.wordmark_dark,
            (false, true) => &self.wordmark_small,
            (false, false) => &self.wordmark,
        }
    }
}

/// Decodes a built-in PNG into a texture. eframe's window icon decoder
/// does the PNG part, so the editor needs no image crate of its own.
fn texture(ctx: &egui::Context, name: &str, png: &[u8]) -> egui::TextureHandle {
    let image = match eframe::icon_data::from_png_bytes(png) {
        Ok(icon) => egui::ColorImage::from_rgba_unmultiplied(
            [icon.width as usize, icon.height as usize],
            &icon.rgba,
        ),
        Err(err) => {
            // Can't happen with the built-in files, but an empty image is
            // better than closing the editor.
            log::warn!("could not decode the {name} image: {err}");
            egui::ColorImage::new([1, 1], vec![egui::Color32::TRANSPARENT])
        }
    };
    ctx.load_texture(name, image, egui::TextureOptions::LINEAR)
}

/// A project to offer on the start screen.
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectLink {
    pub name: String,
    pub description: String,
    pub folder: PathBuf,
}

impl ProjectLink {
    /// The project in `folder`, or `None` if it isn't a project folder.
    pub fn read(folder: &Path) -> Option<Self> {
        let manifest = axcenity_project::read_manifest(folder)?;
        Some(Self {
            name: manifest.name,
            description: manifest.description,
            folder: folder.to_owned(),
        })
    }
}

/// Puts `folder` first in `recent`, removing it from further down, and keeps
/// the list at [`MAX_TRACKED`].
pub fn remember(recent: &mut Vec<ProjectLink>, project: ProjectLink) {
    recent.retain(|link| link.folder != project.folder);
    recent.insert(0, project);
    recent.truncate(MAX_TRACKED);
}

/// What was picked on the start screen.
pub enum StartAction {
    OpenDialog,
    Open(PathBuf),
}

/// The start screen's pages, picked in its sidebar.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Page {
    #[default]
    Projects,
    Learn,
}

/// How tracked projects are shown.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum View {
    #[default]
    Grid,
    List,
}

/// The order tracked projects are shown in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Sort {
    /// Most recently opened first, as they're tracked.
    #[default]
    Recent,
    Name,
}

/// A box over the start screen, with what's typed in it.
enum Dialog {
    New {
        name: String,
        parent: Option<PathBuf>,
        template: Template,
        error: Option<String>,
    },
    Settings {
        folder: PathBuf,
        name: String,
        description: String,
        show_splash: bool,
        error: Option<String>,
    },
    Delete {
        link: ProjectLink,
        error: Option<String>,
    },
    History {
        folder: PathBuf,
        error: Option<String>,
        notice: Option<String>,
    },
}

/// What a project card's buttons asked for.
enum CardAction {
    Open,
    Settings,
    Reveal,
    History,
    Forget,
    Delete,
}

/// The start screen's own state: the loaded thumbnails, the search box and
/// the open dialog.
#[derive(Default)]
pub struct StartScreen {
    /// `None` for a project without a thumbnail yet.
    thumbnails: HashMap<PathBuf, Option<egui::TextureHandle>>,
    /// When each project's world was last saved, read once.
    edited: HashMap<PathBuf, Option<SystemTime>>,
    filter: String,
    page: Page,
    view: View,
    sort: Sort,
    dialog: Option<Dialog>,
    /// The folder last picked for a new project, to offer again.
    last_parent: Option<PathBuf>,
    /// The last problem with something that has no dialog of its own.
    notice: Option<String>,
}

impl StartScreen {
    /// Forgets the loaded thumbnails, so changed ones are read again.
    pub fn reload_thumbnails(&mut self) {
        self.thumbnails.clear();
        self.edited.clear();
    }

    /// When the project in `folder` was last saved.
    fn edited(&mut self, folder: &Path) -> Option<SystemTime> {
        *self.edited.entry(folder.to_owned()).or_insert_with(|| {
            std::fs::metadata(folder.join(axcenity_project::WORLD_FILE))
                .and_then(|metadata| metadata.modified())
                .ok()
        })
    }

    fn new_project(&mut self, template: Template) {
        self.dialog = Some(Dialog::New {
            name: String::new(),
            parent: self.last_parent.clone(),
            template,
            error: None,
        });
    }

    /// Shows `message` under the Projects heading, in red.
    pub fn set_notice(&mut self, message: String) {
        self.notice = Some(message);
    }

    fn thumbnail(&mut self, ctx: &egui::Context, folder: &Path) -> Option<egui::TextureHandle> {
        self.thumbnails
            .entry(folder.to_owned())
            .or_insert_with(|| {
                let bytes = std::fs::read(folder.join(axcenity_project::THUMBNAIL_FILE)).ok()?;
                let icon = eframe::icon_data::from_png_bytes(&bytes).ok()?;
                let image = egui::ColorImage::from_rgba_unmultiplied(
                    [icon.width as usize, icon.height as usize],
                    &icon.rgba,
                );
                Some(ctx.load_texture(
                    format!("thumbnail {}", folder.display()),
                    image,
                    egui::TextureOptions::LINEAR,
                ))
            })
            .clone()
    }

    /// Draws the start screen. Returns what was picked, if anything.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        brand: &Brand,
        projects: &mut Vec<ProjectLink>,
    ) -> Option<StartAction> {
        let mut action = None;
        let ctx = ui.ctx().clone();
        let full = ui.available_rect_before_wrap();
        let sidebar = Rect::from_min_size(full.min, vec2(SIDEBAR_WIDTH, full.height()));
        let page = Rect::from_min_max(pos2(sidebar.right(), full.top()), full.max);

        self.sidebar(ui, sidebar, brand, projects.len());
        let mut content = ui.new_child(egui::UiBuilder::new().max_rect(page));
        content.set_clip_rect(page);
        egui::ScrollArea::vertical()
            .id_salt("start page")
            .auto_shrink(false)
            .show(&mut content, |ui| {
                ui.add_space(PAGE_MARGIN - 8.0);
                let width = ui.available_width() - 2.0 * PAGE_MARGIN;
                ui.horizontal(|ui| {
                    ui.add_space(PAGE_MARGIN);
                    ui.vertical(|ui| {
                        ui.set_width(width);
                        match self.page {
                            Page::Projects => {
                                if let Some(picked) = self.projects_page(ui, &ctx, projects) {
                                    action = Some(picked);
                                }
                            }
                            Page::Learn => learn_page(ui),
                        }
                    });
                });
                ui.add_space(PAGE_MARGIN);
            });
        ui.advance_cursor_after_rect(full);
        if let Some(opened) = self.dialogs(&ctx, projects) {
            action = Some(StartAction::Open(opened));
        }
        action
    }

    /// The logo, the pages, and the version.
    fn sidebar(&mut self, ui: &mut egui::Ui, rect: Rect, brand: &Brand, tracked: usize) {
        let visuals = ui.visuals().clone();
        let fill = if visuals.dark_mode {
            visuals.extreme_bg_color
        } else {
            visuals.faint_bg_color
        };
        ui.painter().rect_filled(rect, 0, fill);
        ui.painter().vline(
            rect.right() - 0.5,
            rect.y_range(),
            visuals.widgets.noninteractive.bg_stroke,
        );
        let mut bar = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(vec2(16.0, 22.0))));
        wordmark(&mut bar, brand, SIDEBAR_WORDMARK_WIDTH);
        bar.add_space(26.0);
        for (page, icon, title, count) in [
            (
                Page::Projects,
                icons::SQUARES_FOUR,
                "Projects",
                Some(tracked),
            ),
            (Page::Learn, icons::GRADUATION_CAP, "Learn", None),
        ] {
            if nav_item(&mut bar, icon, title, count, self.page == page).clicked() {
                self.page = page;
            }
            bar.add_space(4.0);
        }
        let bottom = Rect::from_min_max(pos2(rect.left() + 18.0, rect.bottom() - 44.0), rect.max);
        let mut foot = ui.new_child(egui::UiBuilder::new().max_rect(bottom));
        foot.label(
            egui::RichText::new(format!("Axcenity Engine {}", env!("CARGO_PKG_VERSION")))
                .small()
                .color(visuals.weak_text_color()),
        );
        foot.label(
            egui::RichText::new("Free and open source, Apache-2.0")
                .small()
                .color(visuals.weak_text_color()),
        );
    }

    /// The welcome banner, the templates and the tracked projects. Returns
    /// a project to open, if one was picked.
    fn projects_page(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        projects: &mut Vec<ProjectLink>,
    ) -> Option<StartAction> {
        let mut action = None;
        match hero(ui, projects.is_empty()) {
            Some(HeroAction::New) => self.new_project(Template::Baseplate),
            Some(HeroAction::Open) => action = Some(StartAction::OpenDialog),
            None => {}
        }
        if let Some(notice) = &self.notice {
            ui.add_space(10.0);
            egui::Frame::new()
                .fill(theme::STOP_RED.gamma_multiply(0.15))
                .stroke(Stroke::new(1.0_f32, theme::STOP_RED))
                .corner_radius(8)
                .inner_margin(egui::Margin::symmetric(12, 8))
                .show(ui, |ui| {
                    ui.colored_label(theme::STOP_RED, theme::with_icon(icons::WARNING, notice));
                });
        }

        ui.add_space(28.0);
        section_heading(ui, "Start something new", None);
        ui.add_space(10.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(CARD_SPACING, CARD_SPACING);
            for template in Template::ALL {
                if template_card(ui, template).clicked() {
                    self.new_project(template);
                }
            }
        });

        ui.add_space(30.0);
        let mut filter = std::mem::take(&mut self.filter);
        let (mut view, mut sort) = (self.view, self.sort);
        section_heading(
            ui,
            "Recent projects",
            Some(&mut |ui: &mut egui::Ui| {
                let list = ui
                    .selectable_label(view == View::List, icons::LIST_BULLETS)
                    .on_hover_text("Show as a list");
                if list.clicked() {
                    view = View::List;
                }
                if ui
                    .selectable_label(view == View::Grid, icons::SQUARES_FOUR)
                    .on_hover_text("Show as cards")
                    .clicked()
                {
                    view = View::Grid;
                }
                ui.add_space(6.0);
                egui::ComboBox::from_id_salt("project sort")
                    .selected_text(match sort {
                        Sort::Recent => "Last opened",
                        Sort::Name => "Name",
                    })
                    .width(118.0)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut sort, Sort::Recent, "Last opened");
                        ui.selectable_value(&mut sort, Sort::Name, "Name");
                    });
                ui.add_space(6.0);
                ui.add(
                    egui::TextEdit::singleline(&mut filter)
                        .hint_text(theme::with_icon(icons::MAGNIFYING_GLASS, "Search projects"))
                        .desired_width(220.0),
                );
            }),
        );
        self.filter = filter;
        (self.view, self.sort) = (view, sort);
        ui.add_space(12.0);

        let filter = self.filter.trim().to_lowercase();
        let mut shown: Vec<ProjectLink> = projects
            .iter()
            .filter(|link| {
                filter.is_empty()
                    || link.name.to_lowercase().contains(&filter)
                    || link.description.to_lowercase().contains(&filter)
            })
            .cloned()
            .collect();
        if self.sort == Sort::Name {
            shown.sort_by_key(|link| link.name.to_lowercase());
        }
        if projects.is_empty() {
            empty_state(
                ui,
                "No projects yet",
                "Create one from a template above, or open a project folder.",
            );
        } else if shown.is_empty() {
            empty_state(
                ui,
                "Nothing matches",
                "No project's name or description has that in it.",
            );
        }

        let mut chosen = None;
        let now = SystemTime::now();
        match self.view {
            View::Grid => {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(CARD_SPACING, CARD_SPACING);
                    for link in &shown {
                        let thumbnail = self.thumbnail(ctx, &link.folder);
                        let edited = self.edited(&link.folder).map(|time| ago(time, now));
                        if let Some(picked) = project_card(ui, link, thumbnail.as_ref(), edited) {
                            chosen = Some((link.clone(), picked));
                        }
                    }
                });
            }
            View::List => {
                for link in &shown {
                    let thumbnail = self.thumbnail(ctx, &link.folder);
                    let edited = self.edited(&link.folder).map(|time| ago(time, now));
                    if let Some(picked) = project_row(ui, link, thumbnail.as_ref(), edited) {
                        chosen = Some((link.clone(), picked));
                    }
                    ui.add_space(6.0);
                }
            }
        }
        if let Some((link, picked)) = chosen {
            self.notice = None;
            match picked {
                CardAction::Open => action = Some(StartAction::Open(link.folder)),
                CardAction::Settings => self.open_settings(link),
                CardAction::History => {
                    self.dialog = Some(Dialog::History {
                        folder: link.folder,
                        error: None,
                        notice: None,
                    });
                }
                CardAction::Reveal => {
                    if let Err(err) = reveal(&link.folder) {
                        self.notice = Some(format!(
                            "Could not open `{}` in the file manager: {err}",
                            link.folder.display()
                        ));
                    }
                }
                CardAction::Forget => projects.retain(|other| other.folder != link.folder),
                CardAction::Delete => {
                    self.dialog = Some(Dialog::Delete { link, error: None });
                }
            }
        }
        action
    }

    fn open_settings(&mut self, link: ProjectLink) {
        let show_splash = axcenity_project::read_manifest(&link.folder)
            .is_none_or(|manifest| manifest.show_splash);
        self.dialog = Some(Dialog::Settings {
            folder: link.folder,
            name: link.name,
            description: link.description,
            show_splash,
            error: None,
        });
    }

    /// Draws the open dialog, if any. Returns the folder of a project that
    /// was just created, to open it.
    fn dialogs(&mut self, ctx: &egui::Context, projects: &mut Vec<ProjectLink>) -> Option<PathBuf> {
        let dialog = self.dialog.as_mut()?;
        let mut close = false;
        let mut created = None;
        let mut deleted = None;
        let mut picked_parent = None;
        let mut restored = None;
        let modal = egui::Modal::new(egui::Id::new("start dialog")).show(ctx, |ui| {
            ui.set_width(380.0);
            match dialog {
                Dialog::New {
                    name,
                    parent,
                    template,
                    error,
                } => {
                    ui.set_width(460.0);
                    ui.heading("New Project");
                    ui.add_space(8.0);
                    ui.label("Start from");
                    ui.horizontal(|ui| {
                        for choice in Template::ALL {
                            let picked = *template == choice;
                            let response = egui::Frame::new()
                                .fill(if picked {
                                    theme::BRAND_BLUE.gamma_multiply(0.25)
                                } else {
                                    ui.visuals().faint_bg_color
                                })
                                .stroke(Stroke::new(
                                    if picked { 1.5_f32 } else { 1.0 },
                                    if picked {
                                        theme::BRAND_BLUE_BRIGHT
                                    } else {
                                        ui.visuals().widgets.noninteractive.bg_stroke.color
                                    },
                                ))
                                .corner_radius(8)
                                .inner_margin(egui::Margin::same(8))
                                .show(ui, |ui| {
                                    ui.set_width(126.0);
                                    ui.label(
                                        egui::RichText::new(theme::with_icon(
                                            template_icon(choice),
                                            choice.name(),
                                        ))
                                        .strong(),
                                    );
                                })
                                .response
                                .interact(Sense::click())
                                .on_hover_text(choice.description())
                                .on_hover_cursor(egui::CursorIcon::PointingHand);
                            if response.clicked() {
                                *template = choice;
                            }
                        }
                    });
                    ui.weak(template.description());
                    ui.add_space(8.0);
                    ui.label("Name");
                    ui.add(egui::TextEdit::singleline(name).desired_width(f32::INFINITY));
                    ui.add_space(6.0);
                    ui.label("Location");
                    ui.horizontal(|ui| {
                        if ui.button("Browse…").clicked()
                            && let Some(folder) = rfd::FileDialog::new()
                                .set_title("Choose where to make the project")
                                .pick_folder()
                        {
                            *parent = Some(folder);
                        }
                        match parent {
                            Some(parent) => {
                                ui.add(egui::Label::new(parent.display().to_string()).truncate())
                            }
                            None => ui.weak("No folder chosen"),
                        };
                    });
                    if let Some(parent) = parent.as_deref()
                        && !name.trim().is_empty()
                    {
                        ui.add_space(4.0);
                        ui.weak(format!("Makes a new folder in {}", parent.display()));
                    }
                    if let Some(error) = error {
                        ui.colored_label(theme::STOP_RED, &*error);
                    }
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        let ready = parent.is_some() && !name.trim().is_empty();
                        if ui.add_enabled(ready, egui::Button::new("Create")).clicked()
                            && let Some(parent) = parent.as_deref()
                        {
                            match axcenity_project::create_project_from(parent, name, *template) {
                                Ok(folder) => {
                                    picked_parent = Some(parent.to_owned());
                                    created = Some(folder);
                                    close = true;
                                }
                                Err(err) if err.already_exists() => {
                                    *error =
                                        Some("A folder with that name is already there.".into());
                                }
                                Err(err) => *error = Some(format!("{err}: {}", err.source)),
                            }
                        }
                        if ui.button("Cancel").clicked() {
                            close = true;
                        }
                    });
                }
                Dialog::Settings {
                    folder,
                    name,
                    description,
                    show_splash,
                    error,
                } => {
                    ui.heading("Project Settings");
                    ui.weak(folder.display().to_string());
                    ui.add_space(6.0);
                    ui.label("Name");
                    ui.add(egui::TextEdit::singleline(name).desired_width(f32::INFINITY));
                    ui.add_space(6.0);
                    ui.label("Description");
                    ui.add(
                        egui::TextEdit::multiline(description)
                            .desired_rows(3)
                            .desired_width(f32::INFINITY),
                    );
                    ui.add_space(6.0);
                    ui.checkbox(show_splash, "Show the Axcenity splash in exported games");
                    if let Some(error) = error {
                        ui.colored_label(theme::STOP_RED, &*error);
                    }
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(!name.trim().is_empty(), egui::Button::new("Save"))
                            .clicked()
                        {
                            let saved = axcenity_project::set_name(folder, name)
                                .and_then(|()| {
                                    axcenity_project::set_description(folder, description)
                                })
                                .and_then(|()| {
                                    axcenity_project::set_show_splash(folder, *show_splash)
                                });
                            match saved {
                                Ok(()) => {
                                    if let Some(link) =
                                        projects.iter_mut().find(|link| link.folder == *folder)
                                    {
                                        link.name = name.trim().to_owned();
                                        link.description = description.clone();
                                    }
                                    close = true;
                                }
                                Err(err) => *error = Some(format!("{err}: {}", err.source)),
                            }
                        }
                        if ui.button("Cancel").clicked() {
                            close = true;
                        }
                    });
                }
                Dialog::Delete { link, error } => {
                    ui.heading("Delete project?");
                    ui.add_space(6.0);
                    ui.label(format!(
                        "\"{}\" and everything in its folder will be permanently deleted. \
                         This can't be undone.",
                        link.name
                    ));
                    ui.weak(link.folder.display().to_string());
                    if let Some(error) = error {
                        ui.colored_label(theme::STOP_RED, &*error);
                    }
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        let delete = egui::Button::new(
                            egui::RichText::new("Delete").color(egui::Color32::WHITE),
                        )
                        .fill(theme::STOP_RED);
                        if ui.add(delete).clicked() {
                            match delete_project(&link.folder) {
                                Ok(()) => {
                                    deleted = Some(link.folder.clone());
                                    close = true;
                                }
                                Err(err) => *error = Some(err),
                            }
                        }
                        if ui.button("Cancel").clicked() {
                            close = true;
                        }
                    });
                }
                Dialog::History {
                    folder,
                    error,
                    notice,
                } => {
                    ui.set_width(440.0);
                    ui.heading("Version History");
                    ui.add_space(4.0);
                    ui.weak(
                        "Each save keeps the world, scripts and styles. Restoring puts that \
                         version back. Assets and the project name stay as they are.",
                    );
                    ui.weak(folder.display().to_string());
                    ui.add_space(6.0);
                    let versions = axcenity_project::list_versions(folder);
                    if versions.is_empty() {
                        ui.weak("No versions yet. Open the project and save it to keep one.");
                    }
                    egui::ScrollArea::vertical()
                        .max_height(280.0)
                        .show(ui, |ui| {
                            for version in &versions {
                                ui.horizontal(|ui| {
                                    ui.vertical(|ui| {
                                        ui.label(version.label.as_str());
                                        ui.weak(version.when());
                                    });
                                    if ui.button("Restore").clicked() {
                                        match axcenity_project::restore_version(folder, &version.id)
                                        {
                                            Ok(()) => {
                                                *notice = Some(format!(
                                                    "Restored the version from {}.",
                                                    version.when()
                                                ));
                                                *error = None;
                                                restored = Some(folder.clone());
                                            }
                                            Err(err) => *error = Some(err.to_string()),
                                        }
                                    }
                                });
                                ui.separator();
                            }
                        });
                    if let Some(notice) = notice {
                        ui.colored_label(theme::PLAY_GREEN, notice.as_str());
                    }
                    if let Some(error) = error {
                        ui.colored_label(theme::STOP_RED, error.as_str());
                    }
                    ui.add_space(8.0);
                    if ui.button("Close").clicked() {
                        close = true;
                    }
                }
            }
        });
        if let Some(parent) = picked_parent {
            self.last_parent = Some(parent);
        }
        if let Some(folder) = deleted {
            self.thumbnails.remove(&folder);
            projects.retain(|link| link.folder != folder);
        }
        if let Some(folder) = restored {
            self.edited.remove(&folder);
        }
        if close || modal.should_close() {
            self.dialog = None;
        }
        created
    }
}

/// Deletes a project folder, only if it really is one.
fn delete_project(folder: &Path) -> Result<(), String> {
    if !folder.join(axcenity_project::MANIFEST_FILE).is_file() {
        return Err("That folder isn't a project any more, so it was left alone.".into());
    }
    std::fs::remove_dir_all(folder).map_err(|err| format!("Could not delete it: {err}"))
}

/// Shows `folder` in the system's file manager.
fn reveal(folder: &Path) -> std::io::Result<()> {
    let program = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    std::process::Command::new(program).arg(folder).spawn()?;
    Ok(())
}

/// What the welcome banner's buttons asked for.
enum HeroAction {
    New,
    Open,
}

/// The welcome banner: a brand gradient with a greeting and the main buttons.
fn hero(ui: &mut egui::Ui, first_time: bool) -> Option<HeroAction> {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(width, 148.0), Sense::hover());
    let dark = ui.visuals().dark_mode;
    let (left, right) = if dark {
        (
            Color32::from_rgb(0, 58, 140),
            Color32::from_rgb(76, 41, 148),
        )
    } else {
        (
            Color32::from_rgb(40, 110, 220),
            Color32::from_rgb(120, 80, 220),
        )
    };
    gradient(ui.painter(), rect, left, right, 16);
    // Faint hexagons, the logo's shape, at the right.
    let center = pos2(rect.right() - 120.0, rect.center().y);
    for (size, alpha) in [(54.0, 60), (78.0, 34), (102.0, 18)] {
        let hexagon: Vec<Pos2> = (0..6)
            .map(|corner| {
                let angle =
                    std::f32::consts::FRAC_PI_3 * corner as f32 + std::f32::consts::FRAC_PI_6;
                center + vec2(angle.cos(), angle.sin()) * size
            })
            .collect();
        ui.painter()
            .with_clip_rect(rect)
            .add(egui::Shape::closed_line(
                hexagon,
                Stroke::new(2.0_f32, Color32::from_white_alpha(alpha)),
            ));
    }

    let mut action = None;
    let inner = rect.shrink2(vec2(30.0, 26.0));
    let mut text = ui.new_child(egui::UiBuilder::new().max_rect(inner));
    text.label(
        egui::RichText::new(if first_time {
            "Welcome to Axcenity"
        } else {
            "Welcome back"
        })
        .size(26.0)
        .family(theme::heading_family())
        .color(Color32::WHITE),
    );
    text.add_space(2.0);
    text.label(
        egui::RichText::new(
            "Build worlds from parts, script them in Lua or blocks, and press Play.",
        )
        .size(14.0)
        .color(Color32::from_white_alpha(215)),
    );
    text.add_space(16.0);
    text.horizontal(|ui| {
        let new = egui::Button::new(
            egui::RichText::new(theme::with_icon(icons::PLUS, "New Project"))
                .size(14.5)
                .strong()
                .color(theme::BRAND_BLUE),
        )
        .fill(Color32::WHITE)
        .corner_radius(8)
        .min_size(vec2(150.0, 36.0));
        if ui.add(new).clicked() {
            action = Some(HeroAction::New);
        }
        ui.add_space(6.0);
        let open = egui::Button::new(
            egui::RichText::new(theme::with_icon(icons::FOLDER_OPEN, "Open Project…"))
                .size(14.5)
                .color(Color32::WHITE),
        )
        .fill(Color32::from_white_alpha(28))
        .stroke(Stroke::new(1.0_f32, Color32::from_white_alpha(90)))
        .corner_radius(8)
        .min_size(vec2(150.0, 36.0));
        if ui.add(open).clicked() {
            action = Some(HeroAction::Open);
        }
    });
    action
}

/// Fills `rect` with a gradient from `left` to `right`, with rounded
/// corners and smooth edges: a mesh in the rounded shape whose rim fades out
/// over one pixel, so it looks right on any background.
fn gradient(
    painter: &egui::Painter,
    rect: Rect,
    left: Color32,
    right: Color32,
    radius: impl Into<CornerRadius>,
) {
    use std::f32::consts::{FRAC_PI_2, PI};
    let radius: CornerRadius = radius.into();
    let feather = 1.0 / painter.ctx().pixels_per_point();
    let color_at = |x: f32| {
        let t = ((x - rect.left()) / rect.width().max(1.0)).clamp(0.0, 1.0);
        left.lerp_to_gamma(right, t)
    };
    // The outline, clockwise, with the way out from each point.
    let mut outline: Vec<(Pos2, egui::Vec2)> = Vec::new();
    let largest = rect.width().min(rect.height()) / 2.0;
    for (corner, inward, r, start) in [
        (rect.left_top(), vec2(1.0, 1.0), radius.nw, PI),
        (rect.right_top(), vec2(-1.0, 1.0), radius.ne, PI + FRAC_PI_2),
        (rect.right_bottom(), vec2(-1.0, -1.0), radius.se, 0.0),
        (rect.left_bottom(), vec2(1.0, -1.0), radius.sw, FRAC_PI_2),
    ] {
        let r = f32::from(r).min(largest);
        if r <= 0.0 {
            outline.push((corner, -inward.normalized()));
            continue;
        }
        let center = corner + inward * r;
        let steps = (r * 0.75).ceil().clamp(4.0, 24.0) as usize;
        for step in 0..=steps {
            let angle = start + step as f32 / steps as f32 * FRAC_PI_2;
            let out = vec2(angle.cos(), angle.sin());
            outline.push((center + out * r, out));
        }
    }
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.center(), color_at(rect.center().x));
    for &(point, out) in &outline {
        let inner = point - out * feather * 0.5;
        mesh.colored_vertex(inner, color_at(inner.x));
        mesh.colored_vertex(point + out * feather * 0.5, Color32::TRANSPARENT);
    }
    let count = outline.len() as u32;
    for i in 0..count {
        let j = (i + 1) % count;
        let (inner_i, outer_i) = (1 + 2 * i, 2 + 2 * i);
        let (inner_j, outer_j) = (1 + 2 * j, 2 + 2 * j);
        mesh.add_triangle(0, inner_i, inner_j);
        mesh.add_triangle(inner_i, outer_i, outer_j);
        mesh.add_triangle(inner_i, outer_j, inner_j);
    }
    painter.add(mesh);
}

/// A heading for a part of the page, with `controls` at the right.
fn section_heading(
    ui: &mut egui::Ui,
    title: &str,
    controls: Option<&mut dyn FnMut(&mut egui::Ui)>,
) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(title)
                .size(17.0)
                .family(theme::heading_family()),
        );
        if let Some(controls) = controls {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                controls(ui)
            });
        }
    });
}

/// A page in the sidebar: highlighted while it's shown.
fn nav_item(
    ui: &mut egui::Ui,
    icon: &str,
    title: &str,
    count: Option<usize>,
    selected: bool,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 38.0), Sense::click());
    let visuals = ui.visuals();
    let accent = if visuals.dark_mode {
        theme::BRAND_BLUE_BRIGHT
    } else {
        theme::BRAND_BLUE
    };
    if selected {
        ui.painter()
            .rect_filled(rect, 8, accent.gamma_multiply(0.18));
        ui.painter().rect_filled(
            Rect::from_min_size(rect.left_top() + vec2(0.0, 9.0), vec2(3.0, 20.0)),
            2,
            accent,
        );
    } else if response.hovered() {
        ui.painter()
            .rect_filled(rect, 8, visuals.widgets.hovered.weak_bg_fill);
    }
    let color = if selected {
        accent
    } else {
        visuals.text_color()
    };
    // Drawn from its middle, so it grows and shrinks in place.
    ui.painter().text(
        rect.left_center() + vec2(22.5, 0.0),
        egui::Align2::CENTER_CENTER,
        icon,
        FontId::proportional(17.0 * theme::icon_scale(ui, &response)),
        color,
    );
    ui.painter().text(
        rect.left_center() + vec2(42.0, 0.0),
        egui::Align2::LEFT_CENTER,
        title,
        FontId::new(14.0, theme::heading_family()),
        color,
    );
    if let Some(count) = count {
        let text = count.to_string();
        let badge = Rect::from_center_size(rect.right_center() - vec2(22.0, 0.0), vec2(28.0, 20.0));
        ui.painter()
            .rect_filled(badge, 10, visuals.widgets.inactive.weak_bg_fill);
        ui.painter().text(
            badge.center(),
            egui::Align2::CENTER_CENTER,
            text,
            FontId::proportional(11.0),
            visuals.weak_text_color(),
        );
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn template_icon(template: Template) -> &'static str {
    match template {
        Template::Baseplate => icons::SQUARE,
        Template::Empty => icons::FILE,
        Template::Starter => icons::SPARKLE,
    }
}

/// A card to start a new project from `template`.
fn template_card(ui: &mut egui::Ui, template: Template) -> egui::Response {
    let size = vec2(CARD_WIDTH, 92.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let visuals = ui.visuals();
    let hovered = response.hovered();
    let (left, right) = match template {
        Template::Baseplate => (
            Color32::from_rgb(71, 85, 105),
            Color32::from_rgb(100, 116, 139),
        ),
        Template::Empty => (Color32::from_rgb(55, 65, 81), Color32::from_rgb(31, 41, 55)),
        Template::Starter => (
            Color32::from_rgb(14, 116, 144),
            Color32::from_rgb(124, 58, 237),
        ),
    };
    card_background(ui, rect, hovered);
    let swatch = Rect::from_min_size(rect.min + vec2(14.0, 14.0), vec2(64.0, 64.0));
    gradient(ui.painter(), swatch, left, right, 10);
    ui.painter().text(
        swatch.center(),
        egui::Align2::CENTER_CENTER,
        template_icon(template),
        FontId::proportional(28.0 * theme::icon_scale(ui, &response)),
        Color32::WHITE,
    );
    let text_left = swatch.right() + 14.0;
    ui.painter().text(
        pos2(text_left, rect.top() + 26.0),
        egui::Align2::LEFT_CENTER,
        template.name(),
        FontId::new(15.0, theme::heading_family()),
        visuals.strong_text_color(),
    );
    let galley = ui.painter().layout(
        template.description().to_owned(),
        FontId::proportional(11.5),
        visuals.weak_text_color(),
        rect.right() - text_left - 12.0,
    );
    ui.painter().galley(
        pos2(text_left, rect.top() + 40.0),
        galley,
        visuals.weak_text_color(),
    );
    response
        .on_hover_text(format!("New project: {}", template.description()))
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A card's rounded background, lifted with a shadow and outlined when hovered.
fn card_background(ui: &egui::Ui, rect: Rect, hovered: bool) {
    let visuals = ui.visuals();
    let accent = if visuals.dark_mode {
        theme::BRAND_BLUE_BRIGHT
    } else {
        theme::BRAND_BLUE
    };
    let lift = if hovered { 3.0 } else { 0.0 };
    let rect = rect.translate(vec2(0.0, -lift));
    if hovered {
        ui.painter().add(
            egui::epaint::Shadow {
                offset: [0, 8],
                blur: 22,
                spread: 0,
                color: Color32::from_black_alpha(if visuals.dark_mode { 110 } else { 45 }),
            }
            .as_shape(rect, CARD_RADIUS),
        );
    }
    let fill = if visuals.dark_mode {
        visuals.faint_bg_color.gamma_multiply(1.6)
    } else {
        Color32::WHITE
    };
    ui.painter().rect(
        rect,
        CARD_RADIUS,
        fill,
        Stroke::new(
            if hovered { 1.5_f32 } else { 1.0 },
            if hovered {
                accent
            } else {
                visuals.widgets.noninteractive.bg_stroke.color
            },
        ),
        egui::StrokeKind::Inside,
    );
}

/// A colour that stays the same for a project's name, for its placeholder.
fn name_hue(name: &str) -> f32 {
    let hash = name.bytes().fold(2166136261_u32, |hash, byte| {
        (hash ^ u32::from(byte)).wrapping_mul(16777619)
    });
    (hash % 360) as f32 / 360.0
}

/// A project's picture: its thumbnail, or a gradient with its initials.
fn thumbnail_image(
    ui: &egui::Ui,
    rect: Rect,
    radius: CornerRadius,
    name: &str,
    thumbnail: Option<&egui::TextureHandle>,
) {
    match thumbnail {
        Some(texture) => {
            egui::Image::new(texture)
                .corner_radius(radius)
                .paint_at(ui, rect);
        }
        None => {
            let hue = name_hue(name);
            let left: Color32 = egui::ecolor::Hsva::new(hue, 0.55, 0.62, 1.0).into();
            let right: Color32 =
                egui::ecolor::Hsva::new((hue + 0.12) % 1.0, 0.65, 0.42, 1.0).into();
            let painter = ui.painter().with_clip_rect(rect);
            gradient(&painter, rect, left, right, radius);
            let initials: String = name
                .split_whitespace()
                .filter_map(|word| word.chars().next())
                .take(2)
                .flat_map(char::to_uppercase)
                .collect();
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                if initials.is_empty() {
                    "?".to_owned()
                } else {
                    initials
                },
                FontId::new(rect.height() * 0.32, theme::heading_family()),
                Color32::from_white_alpha(230),
            );
        }
    }
}

/// The ⋮ menu of a project's options.
fn card_menu(ui: &mut egui::Ui, rect: Rect, id: egui::Id) -> Option<CardAction> {
    let response = ui
        .interact(rect, id, Sense::click())
        .on_hover_text("More")
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    let fill = if response.hovered() {
        Color32::from_black_alpha(150)
    } else {
        Color32::from_black_alpha(95)
    };
    ui.painter().rect_filled(rect, 8, fill);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        icons::DOTS_THREE,
        FontId::proportional(16.0),
        Color32::WHITE,
    );
    let mut action = None;
    egui::Popup::menu(&response).show(|ui| {
        if ui
            .button(theme::with_icon(icons::FOLDER_OPEN, "Open"))
            .clicked()
        {
            action = Some(CardAction::Open);
        }
        if ui
            .button(theme::with_icon(icons::GEAR, "Settings…"))
            .clicked()
        {
            action = Some(CardAction::Settings);
        }
        if ui
            .button(theme::with_icon(
                icons::CLOCK_COUNTER_CLOCKWISE,
                "Version History…",
            ))
            .clicked()
        {
            action = Some(CardAction::History);
        }
        if ui
            .button(theme::with_icon(
                icons::ARROW_SQUARE_OUT,
                "Show in File Manager",
            ))
            .clicked()
        {
            action = Some(CardAction::Reveal);
        }
        if ui
            .button(theme::with_icon(icons::X_CIRCLE, "Remove from List"))
            .on_hover_text("Stops tracking it here. The files are kept.")
            .clicked()
        {
            action = Some(CardAction::Forget);
        }
        ui.separator();
        let delete =
            egui::RichText::new(theme::with_icon(icons::TRASH, "Delete…")).color(theme::STOP_RED);
        if ui.button(delete).clicked() {
            action = Some(CardAction::Delete);
        }
    });
    action
}

/// One project as a card: its picture, name, when it was edited and its
/// description. Clicking it opens it; ⋮ has the project's options.
fn project_card(
    ui: &mut egui::Ui,
    link: &ProjectLink,
    thumbnail: Option<&egui::TextureHandle>,
    edited: Option<String>,
) -> Option<CardAction> {
    let picture_height = CARD_WIDTH * 9.0 / 16.0;
    let size = vec2(CARD_WIDTH, picture_height + 78.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let hovered = response.hovered() || ui.rect_contains_pointer(rect);
    let lift = if hovered { 3.0 } else { 0.0 };
    card_background(ui, rect, hovered);
    let rect = rect.translate(vec2(0.0, -lift));
    let picture = Rect::from_min_size(
        rect.min + vec2(1.0, 1.0),
        vec2(CARD_WIDTH - 2.0, picture_height),
    );
    let top = CornerRadius {
        nw: CARD_RADIUS - 1,
        ne: CARD_RADIUS - 1,
        sw: 0,
        se: 0,
    };
    thumbnail_image(ui, picture, top, &link.name, thumbnail);

    let visuals = ui.visuals();
    let mut y = picture.bottom() + 12.0;
    let width = CARD_WIDTH - 28.0;
    let name = ui.painter().layout(
        link.name.clone(),
        FontId::new(15.0, theme::heading_family()),
        visuals.strong_text_color(),
        f32::INFINITY,
    );
    ui.painter()
        .with_clip_rect(Rect::from_min_size(
            pos2(rect.left() + 14.0, y),
            vec2(width, 22.0),
        ))
        .galley(
            pos2(rect.left() + 14.0, y),
            name,
            visuals.strong_text_color(),
        );
    y += 22.0;
    let detail = match &edited {
        Some(edited) => format!("Edited {edited}"),
        None => link.folder.display().to_string(),
    };
    ui.painter()
        .with_clip_rect(Rect::from_min_size(
            pos2(rect.left() + 14.0, y),
            vec2(width, 18.0),
        ))
        .text(
            pos2(rect.left() + 14.0, y),
            egui::Align2::LEFT_TOP,
            theme::with_icon(icons::CLOCK, &detail),
            FontId::proportional(11.5),
            visuals.weak_text_color(),
        );
    y += 18.0;
    if !link.description.is_empty() {
        ui.painter()
            .with_clip_rect(Rect::from_min_size(
                pos2(rect.left() + 14.0, y),
                vec2(width, 18.0),
            ))
            .text(
                pos2(rect.left() + 14.0, y),
                egui::Align2::LEFT_TOP,
                link.description.lines().next().unwrap_or_default(),
                FontId::proportional(12.0),
                visuals.text_color(),
            );
    }

    let menu = Rect::from_min_size(
        pos2(picture.right() - 38.0, picture.top() + 8.0),
        vec2(30.0, 26.0),
    );
    let mut action;
    action = card_menu(ui, menu, egui::Id::new(("card menu", &link.folder)));
    if action.is_none()
        && response
            .on_hover_text(format!("Open {}", link.folder.display()))
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
    {
        action = Some(CardAction::Open);
    }
    action
}

/// One project as a row of the list view.
fn project_row(
    ui: &mut egui::Ui,
    link: &ProjectLink,
    thumbnail: Option<&egui::TextureHandle>,
    edited: Option<String>,
) -> Option<CardAction> {
    let size = vec2(ui.available_width(), 64.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let hovered = response.hovered() || ui.rect_contains_pointer(rect);
    let visuals = ui.visuals().clone();
    let fill = if hovered {
        visuals.widgets.hovered.weak_bg_fill
    } else {
        visuals.faint_bg_color
    };
    ui.painter().rect_filled(rect, 10, fill);
    let picture = Rect::from_min_size(rect.min + vec2(8.0, 8.0), vec2(48.0 * 16.0 / 9.0, 48.0));
    thumbnail_image(ui, picture, CornerRadius::same(6), &link.name, thumbnail);
    let left = picture.right() + 14.0;
    ui.painter().text(
        pos2(left, rect.top() + 22.0),
        egui::Align2::LEFT_CENTER,
        &link.name,
        FontId::new(14.5, theme::heading_family()),
        visuals.strong_text_color(),
    );
    ui.painter().text(
        pos2(left, rect.top() + 43.0),
        egui::Align2::LEFT_CENTER,
        link.folder.display().to_string(),
        FontId::proportional(11.5),
        visuals.weak_text_color(),
    );
    if let Some(edited) = edited {
        ui.painter().text(
            rect.right_center() - vec2(56.0, 0.0),
            egui::Align2::RIGHT_CENTER,
            format!("Edited {edited}"),
            FontId::proportional(12.0),
            visuals.weak_text_color(),
        );
    }
    let menu = Rect::from_center_size(rect.right_center() - vec2(26.0, 0.0), vec2(30.0, 26.0));
    let mut action = card_menu(ui, menu, egui::Id::new(("row menu", &link.folder)));
    if action.is_none()
        && response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
    {
        action = Some(CardAction::Open);
    }
    action
}

/// A friendly note in place of the project cards.
fn empty_state(ui: &mut egui::Ui, title: &str, text: &str) {
    ui.add_space(26.0);
    ui.vertical_centered(|ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(72.0, 72.0), Sense::hover());
        ui.painter()
            .circle_filled(rect.center(), 36.0, ui.visuals().faint_bg_color);
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            icons::CUBE,
            FontId::proportional(32.0),
            ui.visuals().weak_text_color(),
        );
        ui.add_space(8.0);
        ui.label(egui::RichText::new(title).size(15.0).strong());
        ui.weak(text);
    });
    ui.add_space(20.0);
}

/// "3 days ago", for when a project was last saved.
fn ago(then: SystemTime, now: SystemTime) -> String {
    let seconds = now
        .duration_since(then)
        .map_or(0, |elapsed| elapsed.as_secs());
    let plural = |count: u64, unit: &str| {
        if count == 1 {
            format!("1 {unit} ago")
        } else {
            format!("{count} {unit}s ago")
        }
    };
    match seconds {
        0..60 => "just now".to_owned(),
        60..3600 => plural(seconds / 60, "minute"),
        3600..86_400 => plural(seconds / 3600, "hour"),
        86_400..172_800 => "yesterday".to_owned(),
        172_800..2_592_000 => plural(seconds / 86_400, "day"),
        2_592_000..31_536_000 => plural(seconds / 2_592_000, "month"),
        _ => plural(seconds / 31_536_000, "year"),
    }
}

/// The Learn page: where to start with each part of the engine.
fn learn_page(ui: &mut egui::Ui) {
    section_heading(ui, "Learn Axcenity", None);
    ui.weak("The basics of each part of the editor. Open or make a project to try them.");
    ui.add_space(14.0);
    let topics = [
        (
            icons::CUBE,
            "Build",
            "Insert parts from the toolbar, then move, scale and rotate them with the \
             handles (Ctrl+2, 3, 4). Drag a part by its body to drop it onto others.",
        ),
        (
            icons::PUZZLE_PIECE,
            "Blocks",
            "Toolbar > Server Blocks makes a block script. Drag blocks from the palette, \
             snap them under an event, and press View as Lua to see what they do.",
        ),
        (
            icons::CODE,
            "Lua scripts",
            "Scripts run on the server, LocalScripts on each player's side. File > Set Up \
             VS Code Autocomplete gives you the whole API as you type.",
        ),
        (
            icons::LAYOUT,
            "UI and CSS",
            "Insert a Frame, Text or Button from the UI group. Drag its handles to place \
             it, or style it with CSS in the Style section of Properties.",
        ),
        (
            icons::SUN,
            "Lighting",
            "Select Lighting in the Explorer for shadows (with softness), the sky and \
             bloom. Turn off CastShadow on a part to stop it casting a shadow.",
        ),
        (
            icons::SPEAKER_HIGH,
            "Sound",
            "Insert a Sound into a part, pick a file in Properties and press ▶ to hear it. \
             Turn on Playing to start it with the game.",
        ),
        (
            icons::PLAY,
            "Play and export",
            "F5 plays the game in the viewport and Shift+F5 stops it. File > Export Game \
             makes a game anyone can run without the editor.",
        ),
    ];
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(CARD_SPACING, CARD_SPACING);
        for (icon, title, text) in topics {
            let (rect, response) = ui.allocate_exact_size(vec2(CARD_WIDTH, 132.0), Sense::hover());
            card_background(ui, rect, response.hovered());
            let visuals = ui.visuals();
            let accent = if visuals.dark_mode {
                theme::BRAND_BLUE_BRIGHT
            } else {
                theme::BRAND_BLUE
            };
            ui.painter().text(
                rect.min + vec2(16.0, 18.0),
                egui::Align2::LEFT_TOP,
                theme::with_icon(icon, title),
                FontId::new(15.0, theme::heading_family()),
                accent,
            );
            let galley = ui.painter().layout(
                text.to_owned(),
                FontId::proportional(12.0),
                visuals.text_color(),
                rect.width() - 32.0,
            );
            ui.painter()
                .galley(rect.min + vec2(16.0, 46.0), galley, visuals.text_color());
        }
    });
}

fn wordmark(ui: &mut egui::Ui, brand: &Brand, width: f32) {
    let texture = brand.wordmark(ui.visuals(), width);
    let size = texture.size_vec2();
    let height = width * size.y / size.x;
    ui.add(egui::Image::new(texture).fit_to_exact_size(egui::vec2(width, height)));
}

/// The About box: the logo, the version, and the licences of what's built in.
/// Returns false once it's closed.
pub fn about(ctx: &egui::Context, brand: &Brand) -> bool {
    let mut close = false;
    let modal = egui::Modal::new(egui::Id::new("about")).show(ctx, |ui| {
        ui.set_width(WORDMARK_WIDTH);
        ui.vertical_centered(|ui| {
            wordmark(ui, brand, WORDMARK_WIDTH);
            ui.heading("Axcenity Engine");
            ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
            ui.add_space(6.0);
            ui.label("A 3D game engine and editor, scripted with Lua 5.4 and blocks.");
            ui.label("Licensed under Apache-2.0.");
            ui.add_space(10.0);
            ui.label(
                egui::RichText::new(
                    "Fonts: Montserrat and JetBrains Mono (SIL Open Font License 1.1). \
                     Icons: Phosphor (MIT). Built with egui, wgpu, Rapier and Lua.",
                )
                .small()
                .color(ui.visuals().weak_text_color()),
            );
            ui.add_space(10.0);
            if ui.button("Close").clicked() {
                close = true;
            }
        });
    });
    !(close || modal.should_close())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(name: &str) -> ProjectLink {
        ProjectLink {
            name: name.to_string(),
            description: String::new(),
            folder: PathBuf::from(name),
        }
    }

    #[test]
    fn recent_projects_put_the_newest_first_without_repeats() {
        let mut recent = Vec::new();
        for name in ["a", "b", "a"] {
            remember(&mut recent, link(name));
        }
        assert_eq!(recent, [link("a"), link("b")]);

        for n in 0..MAX_TRACKED + 3 {
            remember(&mut recent, link(&n.to_string()));
        }
        assert_eq!(recent.len(), MAX_TRACKED);
        assert_eq!(recent[0], link(&(MAX_TRACKED + 2).to_string()));
    }

    #[test]
    fn the_start_screen_draws_each_page_and_the_new_project_dialog() {
        let mut projects: Vec<ProjectLink> = ["Space Race", "Obby", "Castle Builder"]
            .into_iter()
            .map(|name| ProjectLink {
                name: name.to_owned(),
                description: format!("{name}, a test project"),
                folder: PathBuf::from(format!("not-a-folder/{name}")),
            })
            .collect();
        // One has a thumbnail: a bright picture, so its corners show.
        let pictured = std::env::temp_dir().join(format!("axcenity-thumb-{}", std::process::id()));
        std::fs::create_dir_all(&pictured).unwrap();
        let mut rgb = Vec::new();
        for y in 0..180u32 {
            for x in 0..320u32 {
                rgb.extend([(x * 255 / 320) as u8, (y * 255 / 180) as u8, 230]);
            }
        }
        let mut png_bytes = Vec::new();
        let mut encoder = png::Encoder::new(&mut png_bytes, 320, 180);
        encoder.set_color(png::ColorType::Rgb);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&rgb)
            .unwrap();
        std::fs::write(pictured.join(axcenity_project::THUMBNAIL_FILE), png_bytes).unwrap();
        projects[0].folder = pictured.clone();
        let snapshots = std::env::var_os("AXCENITY_SNAPSHOTS").map(PathBuf::from);
        for (name, page, dialog) in [
            ("start-projects", Page::Projects, false),
            ("start-learn", Page::Learn, false),
            ("start-new-project", Page::Projects, true),
        ] {
            let mut start = StartScreen {
                page,
                ..Default::default()
            };
            if dialog {
                start.new_project(Template::Starter);
            }
            let mut brand = None;
            let mut draw = |ctx: &egui::Context| {
                let brand = brand.get_or_insert_with(|| Brand::load(ctx));
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE.fill(ctx.style().visuals.panel_fill))
                    .show(ctx, |ui| {
                        assert!(start.show(ui, brand, &mut projects).is_none());
                    });
            };
            match &snapshots {
                Some(folder) => crate::snapshot::snapshot(
                    &folder.join(format!("{name}.png")),
                    vec2(1400.0, 900.0),
                    vec![Vec::new(); 3],
                    &mut draw,
                ),
                None => {
                    let ctx = theme::test_context();
                    for _ in 0..3 {
                        let _ = ctx.run(Default::default(), &mut draw);
                    }
                }
            }
        }
        assert_eq!(projects.len(), 3, "drawing alone changes nothing");
        std::fs::remove_dir_all(pictured).unwrap();
    }

    #[test]
    fn edit_times_read_like_people_say_them() {
        let now = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(10_000_000);
        let before = |seconds| now - std::time::Duration::from_secs(seconds);
        assert_eq!(ago(before(5), now), "just now");
        assert_eq!(ago(before(60), now), "1 minute ago");
        assert_eq!(ago(before(7_200), now), "2 hours ago");
        assert_eq!(ago(before(90_000), now), "yesterday");
        assert_eq!(ago(before(5 * 86_400), now), "5 days ago");
        // A clock set back doesn't panic.
        assert_eq!(
            ago(now + std::time::Duration::from_secs(9), now),
            "just now"
        );
    }

    #[test]
    fn only_project_folders_can_be_deleted() {
        let folder =
            std::env::temp_dir().join(format!("axcenity-delete-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        assert!(delete_project(&folder).is_err());
        assert!(folder.exists());
        std::fs::write(folder.join(axcenity_project::MANIFEST_FILE), "{}").unwrap();
        delete_project(&folder).unwrap();
        assert!(!folder.exists());
    }
}
