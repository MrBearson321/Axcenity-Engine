//! The start screen shown while no project is open: a sidebar (or top bar,
//! `nav`) with the wordmark, the Projects, Global Assets, Learn,
//! Documentation and API Reference pages and Settings (global assets are in
//! `global`, the learning path and tutorials in `learn`, the documentation in
//! `docs`, the reference in `reference`; the last three also show over an
//! open project in Help > Learn); on Projects, a
//! header with New and Open Project, templates to start from, and the
//! tracked projects as cards (or a list) with thumbnails, a search box and
//! sorting. Also the New Project, Project Settings, Version History and
//! Delete dialogs, and the About box.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use axcenity_project::Template;
use eframe::egui::{self, CornerRadius, FontId, Rect, RichText, Sense, Stroke, pos2, vec2};

mod backdrop;
mod cards;
mod dialogs;
pub(crate) mod docs;
mod global;
mod learn;
mod nav;
mod pages;
mod reference;
mod team;
mod tilt;

use cards::*;
use dialogs::*;
use docs::{DocAction, Docs};
use global::GlobalPage;
use learn::{Learn, LearnAction, SampleState};
pub use nav::{NavLayout, StartNav};
pub(crate) use pages::ago;
use pages::*;
use reference::Reference;
use team::TeamPage;

use crate::dialog;
use crate::theme::{self, Palette, icons, type_scale};
use crate::widgets::{self, Role};

/// How many projects the start screen tracks.
pub const MAX_TRACKED: usize = 100;
/// How wide the About box's content is, in points.
const ABOUT_WIDTH: f32 = 360.0;
/// How wide the wordmark is drawn in the About box, in points. The image is
/// over twice this, so it stays sharp on high-DPI screens.
const WORDMARK_WIDTH: f32 = 280.0;
/// Project cards are at least this wide; a row fills the page with as many
/// as fit, all the same width.
const MIN_CARD_WIDTH: f32 = 230.0;
const CARD_SPACING: f32 = 16.0;
/// The narrowest a genre template's card gets before the row wraps.
const MIN_GAME_CARD_WIDTH: f32 = 180.0;
const CARD_RADIUS: u8 = 8;
/// The page's margin around its content, and the widest the content gets.
const PAGE_MARGIN: f32 = 40.0;
const MAX_PAGE_WIDTH: f32 = 1160.0;
/// How far the reading glass reaches round the content column.
const GLASS_MARGIN: f32 = 24.0;
/// How far a page rises as it arrives, how long that takes, and the gap
/// between one project card's fade and the next. Cards past
/// [`STAGGERED_CARDS`] fade with the last of them.
const PAGE_RISE: f32 = 12.0;
const PAGE_SECONDS: f32 = 0.34;
const CARD_STAGGER: f32 = 0.04;
const STAGGERED_CARDS: usize = 12;
/// How many of the last opened projects Recent shows, and how many All
/// projects shows before "Show more" (and how many more each click adds).
const RECENT_PROJECTS: usize = 4;
const ALL_PROJECTS_SHOWN: usize = 8;

/// The logo, for the header, start screen and About box. Both are
/// white on a transparent background, so a tint draws them in any colour
/// (egui multiplies an image by its tint).
pub struct Brand {
    /// The A and the name.
    wordmark: egui::TextureHandle,
    /// The A alone, small, for the header and the start screen.
    mark: egui::TextureHandle,
}

impl Brand {
    pub fn load(ctx: &egui::Context) -> Self {
        Self {
            wordmark: texture(
                ctx,
                "wordmark",
                include_bytes!("../../../assets/branding/wordmark-white.png"),
            ),
            mark: texture(
                ctx,
                "mark",
                include_bytes!("../../../assets/branding/mark-white-small.png"),
            ),
        }
    }

    /// The white wordmark. Tint it with the theme's text colour.
    pub fn wordmark(&self) -> &egui::TextureHandle {
        &self.wordmark
    }

    /// The white A. Tint it with the theme's text colour.
    pub fn mark(&self) -> &egui::TextureHandle {
        &self.mark
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
#[derive(Debug)]
pub enum StartAction {
    OpenDialog,
    Open(PathBuf),
    /// A tutorial's sample, from Learn. The tutorial stays open in Help > Learn.
    OpenSample(PathBuf),
    /// The Settings window, from the sidebar.
    Settings,
    /// Join a Collaborate session with this invite (the Join dialog).
    Join(axcenity_collab::access::Invite),
    /// Stop joining: Cancel while the Join dialog waits.
    CancelJoin,
    /// Open this project and host a session for it (Team Projects).
    Host(PathBuf),
    /// Rejoin a team project's session from this computer's record of it.
    Rejoin(Box<axcenity_collab::project::ProjectRecord>),
    /// The Collaborate setup wizard.
    SetUpCollaborate,
    /// Settings, at Collaborate.
    CollaborateSettings,
}

/// How joining a Collaborate session from the start screen is going, for
/// the Join dialog. The app sets it each frame while it joins.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JoinProgress {
    /// Signing in to `host`.
    Connecting { host: String },
    /// `host` can't be reached yet, because of `why`; still trying.
    Retrying { host: String, why: String },
    /// Let in; the session's files are arriving.
    Fetching { host: String },
}

impl JoinProgress {
    /// What the Join dialog says while it waits.
    pub fn describe(&self) -> String {
        match self {
            JoinProgress::Connecting { host } => format!("Signing in to {host}…"),
            JoinProgress::Retrying { host, why } => {
                format!("Can't reach {host} yet ({why}). Still trying…")
            }
            JoinProgress::Fetching { host } => {
                format!("Joined {host}. Fetching the session's files…")
            }
        }
    }
}
/// The start screen's pages, picked in its sidebar.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Page {
    #[default]
    Projects,
    Team,
    Global,
    Learn,
    Docs,
    Reference,
}

impl Page {
    /// Whether the page is mostly prose (Learn, Documentation, the API
    /// Reference), read off the reading glass rather than the moving scene.
    fn reads(self) -> bool {
        matches!(self, Page::Learn | Page::Docs | Page::Reference)
    }

    /// Where the reading glass sits for a content column starting at `left`,
    /// `width` wide, on `page`; `None` on pages without one.
    fn reading_glass(self, left: f32, width: f32, page: Rect) -> Option<Rect> {
        self.reads().then(|| {
            Rect::from_min_max(
                pos2(left - GLASS_MARGIN, page.top() + GLASS_MARGIN),
                pos2(left + width + GLASS_MARGIN, page.bottom() + 8.0),
            )
            .intersect(page.shrink2(vec2(8.0, 0.0)))
        })
    }
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
        /// One of the user's own templates (its file), picked in place of
        /// `template`.
        user_template: Option<PathBuf>,
        error: Option<String>,
    },
    Settings {
        folder: PathBuf,
        name: String,
        description: String,
        show_splash: bool,
        /// The ScreenGui the project uses as its loading screen, in place of
        /// the splash; empty for none.
        loading_gui: String,
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
        /// The version Restore was clicked on, waiting to be confirmed.
        confirm: Option<String>,
    },
    /// Join a Collaborate session: the invite link, then how joining goes.
    Join {
        link: String,
        error: Option<String>,
        /// Join was pressed: the dialog follows [`StartScreen::join`].
        waiting: bool,
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
    /// The genre templates' pictures, decoded the first time they're shown.
    pictures: HashMap<Template, egui::TextureHandle>,
    /// Where the user's own templates are (Save as Template…), and those
    /// found there when New Project last opened.
    templates_folder: Option<PathBuf>,
    user_templates: Vec<axcenity_project::UserTemplate>,
    /// Their pictures, by file.
    user_pictures: HashMap<PathBuf, egui::TextureHandle>,
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
    global: GlobalPage,
    team: TeamPage,
    learn: Learn,
    docs: Docs,
    reference: Reference,
    /// The page last shown and when it arrived, for its entrance.
    arrived: Option<(Page, f64)>,
    /// When the start screen was first drawn, for the logo's entrance.
    intro: Option<f64>,
    /// Sidebar or top bar, and whether the sidebar is folded. Saved.
    nav: StartNav,
    /// How many projects past the first page All projects shows.
    more: usize,
    /// How joining a Collaborate session goes, while it does.
    join: Option<JoinProgress>,
    /// What the Join dialog asked for this frame.
    join_asked: Option<StartAction>,
}

impl StartScreen {
    /// Lists the global assets in `library` on the Global Assets page.
    pub fn set_library(&mut self, library: Option<PathBuf>) {
        self.global.set_library(library);
    }

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

    /// Opens the Join dialog, for an invite link.
    pub fn join_session(&mut self) {
        self.dialog = Some(Dialog::Join {
            link: String::new(),
            error: None,
            waiting: false,
        });
    }

    /// Opens the Join dialog with `link` filled in, for a link the system
    /// handed over (clicked in a browser or chat): joining waits for the
    /// user to press Join, having seen whose session it is (C4).
    pub fn offer_join(&mut self, link: String) {
        self.dialog = Some(Dialog::Join {
            link,
            error: None,
            waiting: false,
        });
    }

    /// Shows the Join dialog following a join already asked for (from the
    /// dialog itself, or the command line).
    pub fn show_joining(&mut self) {
        match &mut self.dialog {
            Some(Dialog::Join { error, waiting, .. }) => {
                *error = None;
                *waiting = true;
            }
            _ => {
                self.dialog = Some(Dialog::Join {
                    link: String::new(),
                    error: None,
                    waiting: true,
                });
            }
        }
    }

    /// The Join dialog's state: whether it waits, and its problem.
    #[cfg(test)]
    pub fn join_dialog(&self) -> Option<(bool, Option<&str>, Option<&JoinProgress>)> {
        match &self.dialog {
            Some(Dialog::Join { waiting, error, .. }) => {
                Some((*waiting, error.as_deref(), self.join.as_ref()))
            }
            _ => None,
        }
    }

    /// How joining goes, each frame while it does (`None` when it isn't).
    pub fn set_join_progress(&mut self, progress: Option<JoinProgress>) {
        self.join = progress;
    }

    /// Joining didn't work, because of `why`: the Join dialog says so and
    /// asks for a link again.
    pub fn join_failed(&mut self, why: String) {
        self.join = None;
        match &mut self.dialog {
            Some(Dialog::Join { error, waiting, .. }) => {
                *error = Some(why);
                *waiting = false;
            }
            _ => {
                self.dialog = Some(Dialog::Join {
                    link: String::new(),
                    error: Some(why),
                    waiting: false,
                });
            }
        }
    }

    /// Opens the New Project dialog, starting from `template`.
    pub fn new_project(&mut self, template: Template) {
        self.reload_user_templates();
        self.dialog = Some(Dialog::New {
            name: String::new(),
            parent: self.last_parent.clone(),
            template,
            user_template: None,
            error: None,
        });
    }

    /// Where the user's own templates are saved and listed from.
    pub fn set_templates_folder(&mut self, folder: Option<PathBuf>) {
        self.templates_folder = folder;
        self.reload_user_templates();
    }

    /// Lists the user's templates again (one was saved or imported).
    pub fn reload_user_templates(&mut self) {
        self.user_templates = match &self.templates_folder {
            Some(folder) => {
                let (templates, problems) = axcenity_project::list_templates(folder);
                for problem in problems {
                    log::warn!("Skipped a template: {problem}");
                }
                templates
            }
            None => Vec::new(),
        };
        self.user_pictures.clear();
    }

    /// The user's templates, as last listed (for tests).
    #[cfg(test)]
    pub fn user_templates(&self) -> &[axcenity_project::UserTemplate] {
        &self.user_templates
    }

    /// The picture of the user's template at `index`, if it has one.
    fn user_picture(&mut self, ctx: &egui::Context, index: usize) -> Option<egui::TextureHandle> {
        let template = self.user_templates.get(index)?;
        if let Some(texture) = self.user_pictures.get(&template.file) {
            return Some(texture.clone());
        }
        let png = template.thumbnail.as_ref()?;
        let texture = texture(ctx, &template.info.name, png);
        self.user_pictures
            .insert(template.file.clone(), texture.clone());
        Some(texture)
    }

    /// Shows `message` under the Projects heading, in red.
    pub fn set_notice(&mut self, message: String) {
        self.notice = Some(message);
    }

    /// The picture of `template`, if it has one.
    fn picture(&mut self, ctx: &egui::Context, template: Template) -> Option<egui::TextureHandle> {
        let png = template.picture()?;
        Some(
            self.pictures
                .entry(template)
                .or_insert_with(|| texture(ctx, template.name(), png))
                .clone(),
        )
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
        projects: &mut Vec<ProjectLink>,
        brand: &Brand,
    ) -> Option<StartAction> {
        let mut action = None;
        let ctx = ui.ctx().clone();
        let full = ui.available_rect_before_wrap();
        // Under the navigation too, so it stays put while the sidebar slides.
        backdrop::paint(ui, full);
        let (room, settings) = self.navigation(ui, full, projects.len(), brand);
        let page = room.page;
        if settings {
            action = Some(StartAction::Settings);
        }
        // A reading page's text reads off hazy glass, not the moving scene:
        // a panel behind the content column, the scene showing round it. The
        // page scrolls inside the glass's top edge, so scrolled text never
        // slides up over the scene. The other pages (cards, not prose) keep
        // the whole scene.
        let scrolled = match self.page.reading_glass(room.left, room.width, page) {
            Some(glass) => {
                backdrop::reading_glass(&ui.painter_at(page), glass, &theme::palette(&ctx));
                Rect::from_min_max(pos2(page.left(), glass.top()), page.max)
            }
            None => page,
        };
        let now = ctx.input(|input| input.time);
        if self.arrived.is_none_or(|(shown, _)| shown != self.page) {
            self.arrived = Some((self.page, now));
            // Team Projects reads its records again each time it's shown.
            if self.page != Page::Team {
                self.forget_team_projects();
            }
        }
        let arrived_at = self.arrived.map_or(now, |(_, at)| at);
        let arrival = theme::arrival(&ctx, arrived_at, 0.0, PAGE_SECONDS, theme::curve::SETTLE);
        let mut content = ui.new_child(egui::UiBuilder::new().max_rect(scrolled));
        content.set_clip_rect(scrolled);
        content.multiply_opacity(arrival);
        // The content starts where it did before the glass moved the top.
        let top = PAGE_MARGIN - 4.0 - (scrolled.top() - page.top());
        egui::ScrollArea::vertical()
            .id_salt("start page")
            .auto_shrink(false)
            .show(&mut content, |ui| {
                ui.add_space(top + (1.0 - arrival) * PAGE_RISE);
                let width = room.width;
                let margin = (room.left - ui.max_rect().left()).max(0.0);
                ui.horizontal(|ui| {
                    ui.add_space(margin);
                    ui.vertical(|ui| {
                        ui.set_width(width);
                        match self.page {
                            Page::Projects => {
                                if let Some(picked) = self.projects_page(ui, &ctx, projects) {
                                    action = Some(picked);
                                }
                            }
                            Page::Team => {
                                if let Some(picked) = self.team_page(ui, projects) {
                                    action = Some(picked);
                                }
                            }
                            Page::Global => self.global.show(ui),
                            Page::Learn => {
                                let picked = self.learn.show(ui, |_| SampleState::Ready);
                                if let Some(sample) = self.follow_learn(picked) {
                                    action = Some(StartAction::OpenSample(sample));
                                }
                            }
                            Page::Docs => {
                                let picked = self.docs.show(ui);
                                if let Some(sample) = self.follow_doc(picked) {
                                    action = Some(StartAction::OpenSample(sample));
                                }
                            }
                            Page::Reference => {
                                let picked = self.reference.show(ui);
                                if let Some(sample) = self.follow_doc(picked) {
                                    action = Some(StartAction::OpenSample(sample));
                                }
                            }
                        }
                    });
                });
                ui.add_space(PAGE_MARGIN);
            });
        ui.advance_cursor_after_rect(full);
        if let Some(opened) = self.dialogs(&ctx, projects) {
            action = Some(self.opened_new(opened));
        } else if self.dialog.is_none() {
            // New Team Project…'s dialog was closed without a project.
            self.team.host_new = false;
        }
        if let Some(asked) = self.join_asked.take() {
            action = Some(asked);
        }
        action
    }

    /// The header, the templates and the tracked projects. Returns a
    /// project to open, if one was picked.
    fn projects_page(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        projects: &mut Vec<ProjectLink>,
    ) -> Option<StartAction> {
        let palette = theme::palette(ctx);
        let mut action = None;
        match page_header(ui, &palette, projects.is_empty()) {
            Some(HeaderAction::New) => self.new_project(Template::Baseplate),
            Some(HeaderAction::Open) => action = Some(StartAction::OpenDialog),
            Some(HeaderAction::Join) => self.join_session(),
            None => {}
        }
        if let Some(notice) = &self.notice {
            ui.add_space(14.0);
            let mut dismissed = false;
            egui::Frame::new()
                .fill(palette.error.gamma_multiply(0.1))
                .stroke(Stroke::new(1.0_f32, palette.error.gamma_multiply(0.5)))
                .corner_radius(8)
                .inner_margin(egui::Margin::symmetric(12, 8))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(icons::WARNING_CIRCLE).color(palette.error));
                        ui.add(egui::Label::new(RichText::new(notice).color(palette.text)).wrap());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            dismissed =
                                widgets::icon_button(ui, icons::X, "Dismiss", false).clicked();
                        });
                    });
                });
            if dismissed {
                self.notice = None;
            }
        }

        ui.add_space(28.0);
        widgets::section_label(ui, "Start from a template");
        ui.add_space(6.0);
        let width = ui.available_width();
        let columns = Template::BLANK.len() as f32;
        // Rounded down, so rounding never pushes the last card onto a new row.
        let template_width = ((width - CARD_SPACING * (columns - 1.0)) / columns)
            .floor()
            .max(200.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(CARD_SPACING, CARD_SPACING);
            for template in Template::BLANK {
                if template_card(ui, template, template_width).clicked() {
                    self.new_project(template);
                }
            }
        });

        // The genre templates, as pictures: whole small games to change.
        ui.add_space(20.0);
        widgets::section_label(ui, "Or from a game");
        ui.add_space(6.0);
        let columns = ((width + CARD_SPACING) / (MIN_GAME_CARD_WIDTH + CARD_SPACING))
            .floor()
            .clamp(1.0, Template::GAMES.len() as f32);
        let game_width = ((width - CARD_SPACING * (columns - 1.0)) / columns).floor();
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(CARD_SPACING, CARD_SPACING);
            for template in Template::GAMES {
                let picture = self.picture(ui.ctx(), template);
                if game_template_card(ui, template, picture.as_ref(), game_width).clicked() {
                    self.new_project(template);
                }
            }
        });

        ui.add_space(32.0);
        let now = SystemTime::now();
        let mut chosen = None;
        // Recent: the last few opened, newest first, whatever the sort.
        let recent = projects.len().min(RECENT_PROJECTS);
        if recent > 0 {
            widgets::section_label(ui, "Recent");
            ui.add_space(12.0);
            let links = projects[..recent].to_vec();
            if let Some(picked) = self.project_grid(ui, ctx, &links, "recent", 0, now) {
                chosen = Some(picked);
            }
            ui.add_space(36.0);
        }

        // All projects: searchable, sorted, a page at a time.
        let mut filter = std::mem::take(&mut self.filter);
        let (mut view, mut sort) = (self.view, self.sort);
        ui.horizontal(|ui| {
            widgets::section_label(ui, "All projects");
            if !projects.is_empty() {
                ui.label(
                    RichText::new(projects.len().to_string())
                        .size(type_scale::SMALL)
                        .color(theme::palette(ui.ctx()).text_weak),
                );
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                widgets::segmented(
                    ui,
                    &mut view,
                    &[
                        (View::Grid, Some(icons::SQUARES_FOUR), ""),
                        (View::List, Some(icons::LIST), ""),
                    ],
                );
                egui::ComboBox::from_id_salt("project sort")
                    .selected_text(match sort {
                        Sort::Recent => "Last opened",
                        Sort::Name => "Name",
                    })
                    .width(120.0)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut sort, Sort::Recent, "Last opened");
                        ui.selectable_value(&mut sort, Sort::Name, "Name");
                    })
                    .response
                    .on_hover_text("Sort");
                widgets::search_field(ui, &mut filter, "Search projects", 220.0);
            });
        });
        if filter != self.filter {
            // A new search starts from its first page.
            self.more = 0;
        }
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
                icons::CUBE,
                "No projects yet",
                "Create one from a template above, or open a project folder.",
            );
        } else if shown.is_empty() {
            empty_state(
                ui,
                icons::MAGNIFYING_GLASS,
                "Nothing matches",
                "No project's name or description has that in it.",
            );
        }
        let matching = shown.len();
        let limit = ALL_PROJECTS_SHOWN + self.more;
        shown.truncate(limit);

        match self.view {
            View::Grid => {
                if let Some(picked) = self.project_grid(ui, ctx, &shown, "all", recent, now) {
                    chosen = Some(picked);
                }
            }
            View::List => {
                ui.spacing_mut().item_spacing.y = 2.0;
                for link in &shown {
                    let thumbnail = self.thumbnail(ctx, &link.folder);
                    let edited = self.edited(&link.folder).map(|time| ago(time, now));
                    if let Some(picked) = project_row(ui, link, thumbnail.as_ref(), edited) {
                        chosen = Some((link.clone(), picked));
                    }
                }
            }
        }
        if matching > limit || self.more > 0 {
            ui.add_space(16.0);
            let palette = theme::palette(ctx);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                let hidden = matching.saturating_sub(limit);
                if hidden > 0 {
                    let next = hidden.min(ALL_PROJECTS_SHOWN);
                    if ui
                        .add(widgets::button(
                            Role::Secondary,
                            Some(icons::CARET_DOWN),
                            &format!("Show {next} more"),
                            &palette,
                        ))
                        .clicked()
                    {
                        self.more += ALL_PROJECTS_SHOWN;
                    }
                    if hidden > next
                        && ui
                            .add(widgets::button(
                                Role::Ghost,
                                None,
                                &format!("Show all {matching}"),
                                &palette,
                            ))
                            .clicked()
                    {
                        self.more = matching;
                    }
                }
                if self.more > 0
                    && ui
                        .add(widgets::button(
                            Role::Ghost,
                            Some(icons::CARET_UP),
                            "Show fewer",
                            &palette,
                        ))
                        .clicked()
                {
                    self.more = 0;
                }
            });
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
                        confirm: None,
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

    /// `links` as rows of project cards that fill the page's width, fading
    /// in one after another from `first` (their place on the page). The
    /// `section` keeps the same project's cards apart when it shows twice.
    fn project_grid(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        links: &[ProjectLink],
        section: &str,
        first: usize,
        now: SystemTime,
    ) -> Option<(ProjectLink, CardAction)> {
        let mut chosen = None;
        let (columns, card_width) = grid_columns(ui.available_width());
        let arrived_at = self.arrived.map_or(0.0, |(_, at)| at);
        ui.spacing_mut().item_spacing.y = CARD_SPACING + 4.0;
        // Rows of `columns` cards each, laid out here rather than by
        // `horizontal_wrapped`: each card fades in inside its own scope, and
        // a wrapping row never breaks before a scope, so the cards ran off
        // the page's right edge.
        for (row, links) in links.chunks(columns).enumerate() {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = CARD_SPACING;
                for (column, link) in links.iter().enumerate() {
                    let index = first + row * columns + column;
                    let thumbnail = self.thumbnail(ctx, &link.folder);
                    let edited = self.edited(&link.folder).map(|time| ago(time, now));
                    let delay = index.min(STAGGERED_CARDS) as f32 * CARD_STAGGER;
                    let fade = theme::arrival(ctx, arrived_at, delay, 0.5, theme::curve::SETTLE);
                    let card = ui.scope(|ui| {
                        ui.multiply_opacity(fade);
                        project_card(ui, link, thumbnail.as_ref(), edited, card_width, section)
                    });
                    if let Some(picked) = card.inner {
                        chosen = Some((link.clone(), picked));
                    }
                }
            });
        }
        chosen
    }

    /// Does what the Learn page asked for. Returns a sample to open.
    fn follow_learn(&mut self, action: Option<LearnAction>) -> Option<PathBuf> {
        match action? {
            LearnAction::OpenSample(folder) => Some(folder),
            LearnAction::LookUp(name) => {
                self.reference.look_up(&name);
                self.page = Page::Reference;
                None
            }
            LearnAction::Doc(index, anchor) => {
                self.docs.open(index, anchor);
                self.page = Page::Docs;
                None
            }
        }
    }

    /// Does what the Documentation page asked for. Returns a sample to open.
    fn follow_doc(&mut self, action: Option<DocAction>) -> Option<PathBuf> {
        match action? {
            DocAction::OpenSample(name) => learn::find_sample(&name),
            DocAction::LookUp(name) => {
                self.reference.look_up(&name);
                self.page = Page::Reference;
                None
            }
            // A Learn chapter opens in Learn, any other page in Documentation.
            DocAction::Open(index, anchor) => {
                let chapter = learn::chapters(docs::library())
                    .iter()
                    .any(|&(_, page)| page == index);
                if chapter {
                    self.learn.open_chapter(index, anchor);
                    self.page = Page::Learn;
                } else {
                    self.docs.open(index, anchor);
                    self.page = Page::Docs;
                }
                None
            }
        }
    }

    /// Shows the API page (on the start screen, or in Help > Learn) with
    /// the entry called `name` open, as Go to Definition does for an API
    /// member.
    pub fn look_up_reference(&mut self, name: &str) {
        self.page = Page::Reference;
        self.reference.look_up(name);
    }

    /// Help > Learn: the Learn, Documentation and API Reference pages in a window over the
    /// open project, at the same place as on the start screen. `project` is
    /// the open project's folder. Returns a tutorial's sample to open.
    pub fn learn_window(
        &mut self,
        ctx: &egui::Context,
        open: &mut bool,
        project: Option<&Path>,
        playing: bool,
    ) -> Option<PathBuf> {
        let mut sample = None;
        if self.page == Page::Projects {
            self.page = Page::Learn;
        }
        widgets::tool_window(icons::BOOK_OPEN, "Learn")
            .id("learn window")
            .size(vec2(480.0, 640.0))
            .min_width(300.0)
            .show(ctx, open, |ui| {
                let mut page = self.page;
                widgets::segmented(
                    ui,
                    &mut page,
                    &[
                        (Page::Learn, Some(icons::BOOK_OPEN), "Learn"),
                        (Page::Docs, Some(icons::FILE_TEXT), "Docs"),
                        (Page::Reference, Some(icons::BOOKS), "API"),
                    ],
                );
                self.page = page;
                ui.add_space(8.0);
                egui::ScrollArea::vertical()
                    .id_salt("learn window page")
                    .auto_shrink(false)
                    .show(ui, |ui| {
                        ui.add_space(4.0);
                        match self.page {
                            Page::Reference => {
                                let picked = self.reference.show(ui);
                                self.follow_doc(picked);
                            }
                            Page::Docs => {
                                let picked = self.docs.show(ui);
                                if let Some(folder) = self.follow_doc(picked) {
                                    let blocked = playing
                                        || project.is_some_and(|open| same_folder(open, &folder));
                                    if !blocked {
                                        sample = Some(folder);
                                    }
                                }
                            }
                            _ => {
                                let picked = self.learn.show(ui, |folder| {
                                    if playing {
                                        SampleState::Playing
                                    } else if project.is_some_and(|open| same_folder(open, folder))
                                    {
                                        SampleState::AlreadyOpen
                                    } else {
                                        SampleState::Ready
                                    }
                                });
                                sample = self.follow_learn(picked);
                            }
                        }
                        ui.add_space(12.0);
                    });
            });
        sample
    }

    fn open_settings(&mut self, link: ProjectLink) {
        let manifest = axcenity_project::read_manifest(&link.folder);
        let show_splash = manifest
            .as_ref()
            .is_none_or(|manifest| manifest.show_splash);
        let loading_gui = manifest
            .map(|manifest| manifest.loading_screen.screen_gui)
            .unwrap_or_default();
        self.dialog = Some(Dialog::Settings {
            folder: link.folder,
            name: link.name,
            description: link.description,
            show_splash,
            loading_gui,
            error: None,
        });
    }
}

/// True when `a` and `b` are the same folder, however they're written.
/// How many project cards fit in a row `width` wide, and how wide each is
/// so the row fills it (rounded down, so the row never overflows).
fn grid_columns(width: f32) -> (usize, f32) {
    let columns = ((width + CARD_SPACING) / (MIN_CARD_WIDTH + CARD_SPACING))
        .floor()
        .max(1.0);
    let card_width = ((width - CARD_SPACING * (columns - 1.0)) / columns)
        .floor()
        .max(1.0);
    (columns as usize, card_width)
}

fn same_folder(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

/// The sample folder (in `examples/`) of every tutorial, for tests that
/// check each one still opens and plays.
#[cfg(test)]
pub fn tutorial_samples() -> Vec<&'static str> {
    learn::TUTORIALS
        .iter()
        .map(|tutorial| tutorial.sample)
        .collect()
}

/// "0.2.0-alpha.1 · Development": the version and the channel it's on.
pub fn version_label() -> String {
    let version = axcenity_release::Version::current();
    format!("{version} · {}", version.channel())
}

/// The About box: the logo, the version, channel and build, and the
/// licences of what's built in. Returns false once it's closed. `gpu` names
/// the graphics adapter for Copy diagnostics.
pub fn about(ctx: &egui::Context, brand: &Brand, gpu: Option<&str>) -> bool {
    let build = axcenity_release::BuildInfo::current();
    let (close, dismissed) = dialog::modal(ctx, "about", ABOUT_WIDTH, |ui| {
        let palette = theme::palette(ui.ctx());
        dialog::header(
            ui,
            icons::INFO,
            dialog::Tone::Question,
            "About Axcenity",
            Some(&format!("Version {}", version_label())),
        );
        ui.vertical_centered(|ui| {
            let texture = brand.wordmark();
            let size = texture.size_vec2();
            let height = WORDMARK_WIDTH * size.y / size.x;
            ui.add_space(8.0);
            ui.add(
                egui::Image::new(texture)
                    .tint(palette.text_strong)
                    .fit_to_exact_size(vec2(WORDMARK_WIDTH, height)),
            );
            ui.add_space(12.0);
            ui.label(
                RichText::new("A 3D game engine and editor, scripted with Luau and blocks.")
                    .color(palette.text_secondary),
            );
            ui.label(RichText::new("Licensed under Apache-2.0.").color(palette.text_secondary));
            ui.add_space(12.0);
            ui.label(
                RichText::new(
                    "Fonts: Inter, Montserrat, JetBrains Mono, Roboto, Poppins, Nunito, \
                     Oswald, Bebas Neue, Bangers, Lobster, Press Start 2P and Lora (SIL \
                     Open Font License 1.1). Icons: Phosphor (MIT). Built with egui, wgpu, \
                     Rapier and Lua.",
                )
                .size(type_scale::SMALL)
                .color(palette.text_weak),
            );
            ui.add_space(8.0);
            ui.label(
                RichText::new(build_line(&build))
                    .size(type_scale::SMALL)
                    .color(palette.text_weak),
            );
        });
        let picked = dialog::buttons(
            ui,
            &[
                dialog::Button::new(icons::COPY, "Copy diagnostics", dialog::Kind::Plain),
                dialog::Button::new(icons::X, "Close", dialog::Kind::Plain),
            ],
            dialog::Enter::Main,
        );
        if picked == Some(0) {
            ui.ctx().copy_text(diagnostics(&build, gpu));
        }
        picked == Some(1)
    });
    !(close || dismissed)
}

/// What Copy diagnostics copies, for a bug report: the build, the system,
/// the GPU, how it was installed and the known issues of this version.
/// Nothing personal: no paths, user or project names.
pub fn diagnostics(build: &axcenity_release::BuildInfo, gpu: Option<&str>) -> String {
    let install = axcenity_release::InstallKind::current();
    let history = crate::version_explorer::built_in_history();
    let issues: Vec<&str> = history
        .issues_affecting(&build.version)
        .into_iter()
        .map(|(issue, _)| issue.id.as_str())
        .collect();
    let issues = if issues.is_empty() {
        "none recorded".to_owned()
    } else {
        issues.join(", ")
    };
    format!(
        "Axcenity Editor {build}\nchannel: {}\nos: {} {}\ngpu: {}\ninstall: {}\nknown issues: {issues}\n",
        build.version.channel(),
        std::env::consts::OS,
        std::env::consts::ARCH,
        gpu.unwrap_or("not started yet"),
        install.label(),
    )
}

/// The About box's build line: how it was built, from which commit, with
/// which toolchain.
fn build_line(build: &axcenity_release::BuildInfo) -> String {
    let how = match build.kind {
        axcenity_release::BuildKind::Official => {
            format!(
                "Official build {}",
                build.tag.as_deref().unwrap_or_default()
            )
        }
        axcenity_release::BuildKind::Local => "Local build".to_owned(),
    };
    let rustc = build
        .rustc
        .split_whitespace()
        .nth(1)
        .map(|v| format!(" · rustc {v}"))
        .unwrap_or_default();
    format!(
        "{how} · commit {}{rustc} · {} ({})",
        build.short_commit(),
        build.target,
        build.profile
    )
}

#[cfg(test)]
mod tests;
