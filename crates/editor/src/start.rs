//! The start screen shown while no project is open: a sidebar with the
//! wordmark, the Projects, Global Assets, Learn and API Reference pages and
//! Settings (global assets are in `global`, the tutorials in `learn`, the
//! reference in `reference`; the last two also
//! show over an open project in Help > Learn); on Projects, a
//! header with New and Open Project, templates to start from, and the
//! tracked projects as cards (or a list) with thumbnails, a search box and
//! sorting. Also the New Project, Project Settings, Version History and
//! Delete dialogs, and the About box.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use axcenity_project::Template;
use eframe::egui::{self, CornerRadius, FontId, Rect, RichText, Sense, Stroke, pos2, vec2};

mod cards;
mod dialogs;
mod global;
mod learn;
mod pages;
mod reference;

use cards::*;
use dialogs::*;
use global::GlobalPage;
use learn::{Learn, LearnAction, SampleState};
use pages::*;
use reference::Reference;

use crate::theme::{self, Palette, icons, type_scale};
use crate::widgets::{self, Role};

/// How many projects the start screen tracks.
pub const MAX_TRACKED: usize = 100;
/// How wide the About box's content is, in points.
const ABOUT_WIDTH: f32 = 360.0;
/// How wide the wordmark is drawn in the About box, in points. The image is
/// over twice this, so it stays sharp on high-DPI screens.
const WORDMARK_WIDTH: f32 = 280.0;
const SIDEBAR_WIDTH: f32 = 220.0;
/// Project cards are at least this wide; a row fills the page with as many
/// as fit, all the same width.
const MIN_CARD_WIDTH: f32 = 230.0;
const CARD_SPACING: f32 = 16.0;
const CARD_RADIUS: u8 = 8;
/// The page's margin around its content, and the widest the content gets.
const PAGE_MARGIN: f32 = 40.0;
const MAX_PAGE_WIDTH: f32 = 1160.0;

/// The logo, for the header, start screen, About box and splash. Both are
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
pub enum StartAction {
    OpenDialog,
    Open(PathBuf),
    /// A tutorial's sample, from Learn. The tutorial stays open in Help > Learn.
    OpenSample(PathBuf),
    /// The Settings window, from the sidebar.
    Settings,
}
/// The start screen's pages, picked in its sidebar.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Page {
    #[default]
    Projects,
    Global,
    Learn,
    Reference,
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
    global: GlobalPage,
    learn: Learn,
    reference: Reference,
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

    /// Opens the New Project dialog, starting from `template`.
    pub fn new_project(&mut self, template: Template) {
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
        projects: &mut Vec<ProjectLink>,
        brand: &Brand,
    ) -> Option<StartAction> {
        let mut action = None;
        let ctx = ui.ctx().clone();
        let full = ui.available_rect_before_wrap();
        let sidebar = Rect::from_min_size(full.min, vec2(SIDEBAR_WIDTH, full.height()));
        let page = Rect::from_min_max(pos2(sidebar.right(), full.top()), full.max);

        if self.sidebar(ui, sidebar, projects.len(), brand) {
            action = Some(StartAction::Settings);
        }
        let mut content = ui.new_child(egui::UiBuilder::new().max_rect(page));
        content.set_clip_rect(page);
        egui::ScrollArea::vertical()
            .id_salt("start page")
            .auto_shrink(false)
            .show(&mut content, |ui| {
                ui.add_space(PAGE_MARGIN - 4.0);
                let width = (ui.available_width() - 2.0 * PAGE_MARGIN).min(MAX_PAGE_WIDTH);
                let margin = (ui.available_width() - width) / 2.0;
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
                            Page::Global => self.global.show(ui),
                            Page::Learn => {
                                let picked = self.learn.show(ui, |_| SampleState::Ready);
                                if let Some(sample) = self.follow_learn(picked) {
                                    action = Some(StartAction::OpenSample(sample));
                                }
                            }
                            Page::Reference => self.reference.show(ui),
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

    /// The wordmark, the pages, Settings and the version. Returns true when
    /// Settings was clicked.
    fn sidebar(&mut self, ui: &mut egui::Ui, rect: Rect, tracked: usize, brand: &Brand) -> bool {
        let palette = theme::palette(ui.ctx());
        ui.painter().rect_filled(rect, 0, palette.app_bg);
        ui.painter().vline(
            rect.right() - 0.5,
            rect.y_range(),
            Stroke::new(1.0_f32, palette.border),
        );
        let inner = rect.shrink2(vec2(12.0, 18.0));
        let mut bar = ui.new_child(egui::UiBuilder::new().max_rect(inner));
        bar.horizontal(|ui| {
            ui.add_space(6.0);
            widgets::wordmark(ui, brand.mark(), 15.0);
        });
        bar.add_space(22.0);
        bar.spacing_mut().item_spacing.y = 2.0;
        for (page, icon, title, count) in [
            (
                Page::Projects,
                icons::SQUARES_FOUR,
                "Projects",
                Some(tracked),
            ),
            (Page::Global, icons::GLOBE, "Global Assets", None),
            (Page::Learn, icons::BOOK_OPEN, "Learn", None),
            (Page::Reference, icons::BOOKS, "API Reference", None),
        ] {
            if widgets::nav_item(&mut bar, icon, title, count, self.page == page).clicked() {
                self.page = page;
            }
        }

        let bottom = Rect::from_min_max(pos2(inner.left(), inner.bottom() - 72.0), inner.max);
        let mut foot = ui.new_child(egui::UiBuilder::new().max_rect(bottom));
        let settings = widgets::nav_item(&mut foot, icons::GEAR_SIX, "Settings", None, false)
            .on_hover_text(format!(
                "Theme, accent, density and motion ({})",
                ui.ctx().format_shortcut(&egui::KeyboardShortcut::new(
                    egui::Modifiers::COMMAND,
                    egui::Key::Comma
                ))
            ));
        foot.add_space(8.0);
        foot.horizontal(|ui| {
            ui.add_space(10.0);
            ui.label(
                RichText::new(format!("Axcenity Engine {}", env!("CARGO_PKG_VERSION")))
                    .size(type_scale::CAPTION)
                    .color(palette.text_weak),
            );
            ui.label(
                RichText::new("Apache-2.0")
                    .size(type_scale::CAPTION)
                    .color(palette.text_muted),
            );
        });
        settings.clicked()
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
        let columns = Template::ALL.len() as f32;
        let template_width = ((width - CARD_SPACING * (columns - 1.0)) / columns).max(200.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(CARD_SPACING, CARD_SPACING);
            for template in Template::ALL {
                if template_card(ui, template, template_width).clicked() {
                    self.new_project(template);
                }
            }
        });

        ui.add_space(32.0);
        let mut filter = std::mem::take(&mut self.filter);
        let (mut view, mut sort) = (self.view, self.sort);
        ui.horizontal(|ui| {
            widgets::section_label(ui, "Recent projects");
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

        let mut chosen = None;
        let now = SystemTime::now();
        match self.view {
            View::Grid => {
                let width = ui.available_width();
                let columns = ((width + CARD_SPACING) / (MIN_CARD_WIDTH + CARD_SPACING))
                    .floor()
                    .max(1.0);
                let card_width = (width - CARD_SPACING * (columns - 1.0)) / columns;
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(CARD_SPACING, CARD_SPACING + 4.0);
                    for link in &shown {
                        let thumbnail = self.thumbnail(ctx, &link.folder);
                        let edited = self.edited(&link.folder).map(|time| ago(time, now));
                        if let Some(picked) =
                            project_card(ui, link, thumbnail.as_ref(), edited, card_width)
                        {
                            chosen = Some((link.clone(), picked));
                        }
                    }
                });
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

    /// Does what the Learn page asked for. Returns a sample to open.
    fn follow_learn(&mut self, action: Option<LearnAction>) -> Option<PathBuf> {
        match action? {
            LearnAction::OpenSample(folder) => Some(folder),
            LearnAction::LookUp(name) => {
                self.reference.look_up(name);
                self.page = Page::Reference;
                None
            }
        }
    }

    /// Help > Learn: the Learn and API Reference pages in a window over the
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
        egui::Window::new(theme::with_icon(icons::BOOK_OPEN, "Learn"))
            .id(egui::Id::new("learn window"))
            .open(open)
            .default_size(vec2(480.0, 640.0))
            .min_width(300.0)
            .collapsible(false)
            .resizable(true)
            .show(ctx, |ui| {
                let mut page = self.page;
                widgets::segmented(
                    ui,
                    &mut page,
                    &[
                        (Page::Learn, Some(icons::BOOK_OPEN), "Tutorials"),
                        (Page::Reference, Some(icons::BOOKS), "API Reference"),
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
                            Page::Reference => self.reference.show(ui),
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
}

/// True when `a` and `b` are the same folder, however they're written.
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

/// The About box: the logo, the version, and the licences of what's built in.
/// Returns false once it's closed.
pub fn about(ctx: &egui::Context, brand: &Brand) -> bool {
    let palette = theme::palette(ctx);
    let mut close = false;
    let frame = egui::Frame::popup(&ctx.style())
        .corner_radius(12)
        .inner_margin(egui::Margin::same(24));
    let modal = egui::Modal::new(egui::Id::new("about"))
        .frame(frame)
        .show(ctx, |ui| {
            ui.set_width(ABOUT_WIDTH);
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
                    RichText::new(format!("Axcenity Engine {}", env!("CARGO_PKG_VERSION")))
                        .family(theme::heading_family())
                        .size(type_scale::HEADING)
                        .color(palette.text_strong),
                );
                ui.add_space(4.0);
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
                ui.add_space(16.0);
                if ui
                    .add(
                        widgets::button(Role::Secondary, None, "Close", &palette)
                            .min_size(vec2(84.0, 30.0)),
                    )
                    .clicked()
                {
                    close = true;
                }
            });
        });
    !(close || modal.should_close())
}

#[cfg(test)]
mod tests;
