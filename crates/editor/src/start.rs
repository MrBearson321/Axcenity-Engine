//! The start screen shown while no project is open: a sidebar with the
//! wordmark, the Projects and Learn pages and Settings; on Projects, a
//! header with New and Open Project, templates to start from, and the
//! tracked projects as cards (or a list) with thumbnails, a search box and
//! sorting. Also the New Project, Project Settings, Version History and
//! Delete dialogs, and the About box.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use axcenity_project::Template;
use eframe::egui::{self, CornerRadius, FontId, Rect, RichText, Sense, Stroke, pos2, vec2};

use crate::theme::{self, Palette, icons, type_scale};
use crate::widgets::{self, Role};

/// How many projects the start screen tracks.
pub const MAX_TRACKED: usize = 100;
/// How wide the wordmark is drawn in the About box, in points. The images
/// are twice this, so they stay sharp on high-DPI screens.
const WORDMARK_WIDTH: f32 = 360.0;
const SIDEBAR_WIDTH: f32 = 220.0;
/// Project cards are at least this wide; a row fills the page with as many
/// as fit, all the same width.
const MIN_CARD_WIDTH: f32 = 230.0;
const CARD_SPACING: f32 = 16.0;
const CARD_RADIUS: u8 = 8;
/// The page's margin around its content, and the widest the content gets.
const PAGE_MARGIN: f32 = 40.0;
const MAX_PAGE_WIDTH: f32 = 1160.0;

/// The full-colour logo, for the About box.
pub struct Brand {
    /// The wordmark for light backgrounds, and for dark ones.
    wordmark: egui::TextureHandle,
    wordmark_dark: egui::TextureHandle,
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
        }
    }

    /// The wordmark that reads well on the current theme's background.
    pub fn wordmark(&self, visuals: &egui::Visuals) -> &egui::TextureHandle {
        if visuals.dark_mode {
            &self.wordmark_dark
        } else {
            &self.wordmark
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
    /// The Settings window, from the sidebar.
    Settings,
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
        projects: &mut Vec<ProjectLink>,
    ) -> Option<StartAction> {
        let mut action = None;
        let ctx = ui.ctx().clone();
        let full = ui.available_rect_before_wrap();
        let sidebar = Rect::from_min_size(full.min, vec2(SIDEBAR_WIDTH, full.height()));
        let page = Rect::from_min_max(pos2(sidebar.right(), full.top()), full.max);

        if self.sidebar(ui, sidebar, projects.len()) {
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

    /// The wordmark, the pages, Settings and the version. Returns true when
    /// Settings was clicked.
    fn sidebar(&mut self, ui: &mut egui::Ui, rect: Rect, tracked: usize) -> bool {
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
            widgets::wordmark(ui, 15.0);
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
            (Page::Learn, icons::BOOK_OPEN, "Learn", None),
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
        let palette = theme::palette(ctx);
        let mut close = false;
        let mut created = None;
        let mut deleted = None;
        let mut picked_parent = None;
        let mut restored = None;
        let frame = egui::Frame::popup(&ctx.style())
            .corner_radius(12)
            .inner_margin(egui::Margin::same(20));
        let modal = egui::Modal::new(egui::Id::new("start dialog"))
            .frame(frame)
            .show(ctx, |ui| {
                ui.set_width(400.0);
                match dialog {
                    Dialog::New {
                        name,
                        parent,
                        template,
                        error,
                    } => {
                        ui.set_width(480.0);
                        dialog_title(ui, &palette, "New Project", None);
                        field_label(ui, &palette, "Template");
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 8.0;
                            for choice in Template::ALL {
                                if template_choice(ui, &palette, choice, *template == choice)
                                    .clicked()
                                {
                                    *template = choice;
                                }
                            }
                        });
                        ui.label(
                            RichText::new(template.description())
                                .size(type_scale::SMALL)
                                .color(palette.text_weak),
                        );
                        ui.add_space(10.0);
                        field_label(ui, &palette, "Name");
                        let name_field = ui.add(
                            egui::TextEdit::singleline(name)
                                .hint_text("My Game")
                                .desired_width(f32::INFINITY),
                        );
                        if name.is_empty() && !name_field.has_focus() && error.is_none() {
                            name_field.request_focus();
                        }
                        ui.add_space(10.0);
                        field_label(ui, &palette, "Location");
                        ui.horizontal(|ui| {
                            if ui
                                .add(widgets::button(
                                    Role::Secondary,
                                    Some(icons::FOLDER_OPEN),
                                    "Browse…",
                                    &palette,
                                ))
                                .clicked()
                                && let Some(folder) = rfd::FileDialog::new()
                                    .set_title("Choose where to make the project")
                                    .pick_folder()
                            {
                                *parent = Some(folder);
                            }
                            match parent {
                                Some(parent) => ui.add(
                                    egui::Label::new(
                                        RichText::new(parent.display().to_string())
                                            .color(palette.text_secondary),
                                    )
                                    .truncate(),
                                ),
                                None => ui.label(
                                    RichText::new("No folder chosen").color(palette.text_weak),
                                ),
                            };
                        });
                        if let Some(parent) = parent.as_deref()
                            && !name.trim().is_empty()
                        {
                            ui.label(
                                RichText::new(format!(
                                    "Makes a new folder: {}",
                                    parent.join(name.trim()).display()
                                ))
                                .size(type_scale::SMALL)
                                .color(palette.text_weak),
                            );
                        }
                        error_line(ui, &palette, error.as_deref());
                        let ready = parent.is_some() && !name.trim().is_empty();
                        let (create, cancel) = dialog_buttons(
                            ui,
                            &palette,
                            ("Create Project", Role::Primary, ready),
                            "Cancel",
                        );
                        let enter = ui.input(|input| input.key_pressed(egui::Key::Enter));
                        if (create || (enter && ready))
                            && let Some(parent) = parent.as_deref()
                        {
                            match axcenity_project::create_project_from(parent, name, *template) {
                                Ok(folder) => {
                                    picked_parent = Some(parent.to_owned());
                                    created = Some(folder);
                                    close = true;
                                }
                                Err(err) if err.already_exists() => {
                                    *error = Some(
                                        "A folder with that name is already there. Pick \
                                         another name or location."
                                            .into(),
                                    );
                                }
                                Err(err) => *error = Some(format!("{err}: {}", err.source)),
                            }
                        }
                        close |= cancel;
                    }
                    Dialog::Settings {
                        folder,
                        name,
                        description,
                        show_splash,
                        error,
                    } => {
                        dialog_title(
                            ui,
                            &palette,
                            "Project Settings",
                            Some(&folder.display().to_string()),
                        );
                        field_label(ui, &palette, "Name");
                        ui.add(egui::TextEdit::singleline(name).desired_width(f32::INFINITY));
                        ui.add_space(10.0);
                        field_label(ui, &palette, "Description");
                        ui.add(
                            egui::TextEdit::multiline(description)
                                .desired_rows(3)
                                .desired_width(f32::INFINITY),
                        );
                        ui.add_space(10.0);
                        ui.checkbox(show_splash, "Show the Axcenity splash in exported games");
                        error_line(ui, &palette, error.as_deref());
                        let (save, cancel) = dialog_buttons(
                            ui,
                            &palette,
                            ("Save", Role::Primary, !name.trim().is_empty()),
                            "Cancel",
                        );
                        if save {
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
                        close |= cancel;
                    }
                    Dialog::Delete { link, error } => {
                        ui.horizontal_top(|ui| {
                            let (tile, _) =
                                ui.allocate_exact_size(vec2(36.0, 36.0), Sense::hover());
                            ui.painter()
                                .rect_filled(tile, 8, palette.error.gamma_multiply(0.14));
                            ui.painter().text(
                                tile.center(),
                                egui::Align2::CENTER_CENTER,
                                icons::TRASH,
                                FontId::proportional(19.0),
                                palette.error,
                            );
                            ui.vertical(|ui| {
                                dialog_title(ui, &palette, "Delete project?", None);
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(format!(
                                            "\"{}\" and everything in its folder will be \
                                             permanently deleted. This can't be undone.",
                                            link.name
                                        ))
                                        .color(palette.text_secondary),
                                    )
                                    .wrap(),
                                );
                                ui.label(
                                    RichText::new(link.folder.display().to_string())
                                        .size(type_scale::SMALL)
                                        .color(palette.text_weak),
                                );
                            });
                        });
                        error_line(ui, &palette, error.as_deref());
                        let (delete, cancel) = dialog_buttons(
                            ui,
                            &palette,
                            ("Delete Project", Role::Danger, true),
                            "Cancel",
                        );
                        if delete {
                            match delete_project(&link.folder) {
                                Ok(()) => {
                                    deleted = Some(link.folder.clone());
                                    close = true;
                                }
                                Err(err) => *error = Some(err),
                            }
                        }
                        close |= cancel;
                    }
                    Dialog::History {
                        folder,
                        error,
                        notice,
                    } => {
                        ui.set_width(460.0);
                        dialog_title(
                            ui,
                            &palette,
                            "Version History",
                            Some(
                                "Each save keeps the world, scripts and styles. Restoring puts \
                                 that version back. Assets and the project name stay as they are.",
                            ),
                        );
                        let versions = axcenity_project::list_versions(folder);
                        if versions.is_empty() {
                            ui.label(
                                RichText::new(
                                    "No versions yet. Open the project and save it to keep one.",
                                )
                                .color(palette.text_weak),
                            );
                        }
                        egui::Frame::new()
                            .stroke(Stroke::new(1.0_f32, palette.border))
                            .corner_radius(8)
                            .show(ui, |ui| {
                                egui::ScrollArea::vertical()
                                    .max_height(280.0)
                                    .auto_shrink([false, true])
                                    .show(ui, |ui| {
                                        for (index, version) in versions.iter().enumerate() {
                                            if index > 0 {
                                                widgets::divider(ui);
                                            }
                                            egui::Frame::new()
                                                .inner_margin(egui::Margin::symmetric(12, 8))
                                                .show(ui, |ui| {
                                                    ui.horizontal(|ui| {
                                                        ui.vertical(|ui| {
                                                            ui.label(
                                                                RichText::new(
                                                                    version.label.as_str(),
                                                                )
                                                                .family(theme::medium_family()),
                                                            );
                                                            ui.label(
                                                                RichText::new(version.when())
                                                                    .size(type_scale::SMALL)
                                                                    .color(palette.text_weak),
                                                            );
                                                        });
                                                        ui.with_layout(
                                                            egui::Layout::right_to_left(
                                                                egui::Align::Center,
                                                            ),
                                                            |ui| {
                                                                let restore = ui.add(
                                                                    widgets::button(
                                                                        Role::Secondary,
                                                                        Some(
                                                                            icons::CLOCK_COUNTER_CLOCKWISE,
                                                                        ),
                                                                        "Restore",
                                                                        &palette,
                                                                    ),
                                                                );
                                                                if restore.clicked() {
                                                                    match axcenity_project::restore_version(
                                                                        folder,
                                                                        &version.id,
                                                                    ) {
                                                                        Ok(()) => {
                                                                            *notice = Some(format!(
                                                                                "Restored the version from {}.",
                                                                                version.when()
                                                                            ));
                                                                            *error = None;
                                                                            restored =
                                                                                Some(folder.clone());
                                                                        }
                                                                        Err(err) => {
                                                                            *error =
                                                                                Some(err.to_string())
                                                                        }
                                                                    }
                                                                }
                                                            },
                                                        );
                                                    });
                                                });
                                        }
                                    });
                            });
                        if let Some(notice) = notice {
                            ui.add_space(6.0);
                            ui.label(
                                RichText::new(theme::with_icon(icons::CHECK_CIRCLE, notice))
                                    .color(palette.success),
                            );
                        }
                        error_line(ui, &palette, error.as_deref());
                        ui.add_space(16.0);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add(widgets::button(Role::Secondary, None, "Close", &palette))
                                .clicked()
                            {
                                close = true;
                            }
                        });
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

/// A dialog's title, with an optional line under it.
fn dialog_title(ui: &mut egui::Ui, palette: &Palette, title: &str, subtitle: Option<&str>) {
    ui.label(
        RichText::new(title)
            .family(theme::heading_family())
            .size(type_scale::HEADING + 1.0)
            .color(palette.text_strong),
    );
    if let Some(subtitle) = subtitle {
        ui.add(
            egui::Label::new(
                RichText::new(subtitle)
                    .size(type_scale::SMALL)
                    .color(palette.text_weak),
            )
            .wrap(),
        );
    }
    ui.add_space(12.0);
}

/// The name over a field in a dialog.
fn field_label(ui: &mut egui::Ui, palette: &Palette, text: &str) {
    ui.label(
        RichText::new(text)
            .family(theme::medium_family())
            .size(type_scale::SMALL)
            .color(palette.text_secondary),
    );
}

/// A problem with what was typed, in red, with what to do about it.
fn error_line(ui: &mut egui::Ui, palette: &Palette, error: Option<&str>) {
    if let Some(error) = error {
        ui.add_space(8.0);
        ui.add(
            egui::Label::new(
                RichText::new(theme::with_icon(icons::WARNING_CIRCLE, error)).color(palette.error),
            )
            .wrap(),
        );
    }
}

/// A dialog's buttons at its bottom right: the main one (with its role and
/// whether it can be pressed) right of Cancel. Returns which was clicked.
fn dialog_buttons(
    ui: &mut egui::Ui,
    palette: &Palette,
    (main, role, enabled): (&str, Role, bool),
    cancel: &str,
) -> (bool, bool) {
    ui.add_space(20.0);
    let mut clicked = (false, false);
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        clicked.0 = ui
            .add_enabled(
                enabled,
                widgets::button(role, None, main, palette).min_size(vec2(84.0, 30.0)),
            )
            .clicked();
        clicked.1 = ui
            .add(widgets::button(Role::Secondary, None, cancel, palette).min_size(vec2(84.0, 30.0)))
            .clicked();
    });
    clicked
}

/// A template to pick in the New Project dialog.
fn template_choice(
    ui: &mut egui::Ui,
    palette: &Palette,
    template: Template,
    picked: bool,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(150.0, 34.0), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::RadioButton, true, picked, template.name())
    });
    let hover = theme::ease(ui.ctx(), response.id.with("hover"), response.hovered());
    let fill = if picked {
        palette.accent_soft
    } else {
        palette.control.gamma_multiply(hover)
    };
    let stroke = if picked {
        palette.accent
    } else {
        theme::mix(palette.border, palette.border_strong, hover)
    };
    ui.painter().rect(
        rect,
        6,
        fill,
        Stroke::new(1.0_f32, stroke),
        egui::StrokeKind::Inside,
    );
    if response.has_focus() && !picked {
        ui.painter().rect_stroke(
            rect,
            6,
            Stroke::new(1.0_f32, palette.accent),
            egui::StrokeKind::Inside,
        );
    }
    let color = if picked {
        palette.accent_text
    } else {
        palette.text
    };
    ui.painter().text(
        rect.left_center() + vec2(12.0, 0.0),
        egui::Align2::LEFT_CENTER,
        theme::with_icon(template_icon(template), template.name()),
        FontId::new(type_scale::BODY, theme::medium_family()),
        color,
    );
    response
        .on_hover_text(template.description())
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// What the page header's buttons asked for.
enum HeaderAction {
    New,
    Open,
}

/// The page's title and a line about the engine, with New Project and Open
/// Project at the right.
fn page_header(ui: &mut egui::Ui, palette: &Palette, first_time: bool) -> Option<HeaderAction> {
    let mut action = None;
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(
                RichText::new(if first_time {
                    "Welcome to Axcenity"
                } else {
                    "Projects"
                })
                .family(theme::heading_family())
                .size(type_scale::DISPLAY)
                .color(palette.text_strong),
            );
            ui.label(
                RichText::new(
                    "Build worlds from parts, script them in Lua or blocks, and press Play.",
                )
                .color(palette.text_secondary),
            );
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let new = widgets::button(Role::Primary, Some(icons::PLUS), "New Project", palette)
                .min_size(vec2(0.0, 32.0));
            if ui
                .add(new)
                .on_hover_text("Start a project from a template")
                .clicked()
            {
                action = Some(HeaderAction::New);
            }
            let open = widgets::button(
                Role::Secondary,
                Some(icons::FOLDER_OPEN),
                "Open Project…",
                palette,
            )
            .min_size(vec2(0.0, 32.0));
            if ui
                .add(open)
                .on_hover_text("Open a project folder")
                .clicked()
            {
                action = Some(HeaderAction::Open);
            }
        });
    });
    action
}

fn template_icon(template: Template) -> &'static str {
    match template {
        Template::Baseplate => icons::SQUARE,
        Template::Empty => icons::FILE_DASHED,
        Template::Starter => icons::SPARKLE,
    }
}

/// A card to start a new project from `template`: an icon tile, the name
/// and what it starts with.
fn template_card(ui: &mut egui::Ui, template: Template, width: f32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(width, 68.0), Sense::click());
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, template.name()));
    let palette = theme::palette(ui.ctx());
    let hover = theme::ease(ui.ctx(), response.id.with("hover"), response.hovered());
    card_frame(ui, rect, &response, hover, &palette);
    let tile = Rect::from_min_size(rect.min + vec2(14.0, 14.0), vec2(40.0, 40.0));
    ui.painter().rect(
        tile,
        8,
        theme::mix(palette.surface, palette.control_hover, hover),
        Stroke::new(1.0_f32, palette.border),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        tile.center(),
        egui::Align2::CENTER_CENTER,
        template_icon(template),
        FontId::proportional(18.0),
        theme::mix(palette.text_secondary, palette.text_strong, hover),
    );
    let left = tile.right() + 12.0;
    ui.painter().text(
        pos2(left, rect.top() + 23.0),
        egui::Align2::LEFT_CENTER,
        template.name(),
        FontId::new(type_scale::BODY, theme::medium_family()),
        palette.text_strong,
    );
    let description = widgets::elided(
        ui,
        template.description(),
        FontId::proportional(type_scale::SMALL),
        palette.text_weak,
        rect.right() - left - 12.0,
        2,
    );
    ui.painter().galley(
        pos2(left, rect.top() + 33.0),
        description,
        palette.text_weak,
    );
    response
        .on_hover_text(format!("New project: {}", template.description()))
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A card's surface: a thin outline that strengthens on hover (`hover`
/// from 0 to 1), a faint fill, and the focus ring.
fn card_frame(ui: &egui::Ui, rect: Rect, response: &egui::Response, hover: f32, palette: &Palette) {
    ui.painter().rect(
        rect,
        CARD_RADIUS,
        theme::mix(palette.panel, palette.surface, 0.6 + 0.4 * hover),
        Stroke::new(
            1.0_f32,
            theme::mix(palette.border, palette.border_strong, hover),
        ),
        egui::StrokeKind::Inside,
    );
    if response.has_focus() {
        ui.painter().rect_stroke(
            rect.expand(2.0),
            CARD_RADIUS + 2,
            Stroke::new(1.5_f32, palette.accent),
            egui::StrokeKind::Inside,
        );
    }
}

/// A project's picture: its thumbnail, or a quiet placeholder with its
/// initials on a faint grid.
fn thumbnail_image(
    ui: &egui::Ui,
    rect: Rect,
    radius: CornerRadius,
    name: &str,
    thumbnail: Option<&egui::TextureHandle>,
) {
    let palette = theme::palette(ui.ctx());
    match thumbnail {
        Some(texture) => {
            egui::Image::new(texture)
                .corner_radius(radius)
                .paint_at(ui, rect);
        }
        None => {
            let painter = ui.painter().with_clip_rect(rect);
            painter.rect_filled(rect, radius, palette.surface);
            let step = (rect.height() / 6.0).max(8.0);
            let line = Stroke::new(1.0_f32, palette.border.gamma_multiply(0.7));
            let mut x = rect.left() + step;
            while x < rect.right() {
                painter.vline(x, rect.y_range(), line);
                x += step;
            }
            let mut y = rect.top() + step;
            while y < rect.bottom() {
                painter.hline(rect.x_range(), y, line);
                y += step;
            }
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
                FontId::new(rect.height() * 0.26, theme::heading_family()),
                palette.text_weak,
            );
        }
    }
}

/// A project's options, for its ⋯ button and its right-click menu.
fn project_menu(ui: &mut egui::Ui, action: &mut Option<CardAction>) {
    let palette = theme::palette(ui.ctx());
    let mut item = |ui: &mut egui::Ui, icon: &str, label: &str, picked: CardAction| {
        if ui.button(theme::with_icon(icon, label)).clicked() {
            *action = Some(picked);
        }
    };
    item(ui, icons::FOLDER_OPEN, "Open", CardAction::Open);
    item(ui, icons::GEAR, "Settings…", CardAction::Settings);
    item(
        ui,
        icons::CLOCK_COUNTER_CLOCKWISE,
        "Version History…",
        CardAction::History,
    );
    item(
        ui,
        icons::ARROW_SQUARE_OUT,
        "Show in File Manager",
        CardAction::Reveal,
    );
    if ui
        .button(theme::with_icon(icons::X_CIRCLE, "Remove from List"))
        .on_hover_text("Stops tracking it here. The files are kept.")
        .clicked()
    {
        *action = Some(CardAction::Forget);
    }
    ui.separator();
    let delete = RichText::new(theme::with_icon(icons::TRASH, "Delete…")).color(palette.error);
    if ui.button(delete).clicked() {
        *action = Some(CardAction::Delete);
    }
}

/// The ⋯ button of a card or row, shown while `visible` (the card is
/// hovered) or while its menu is open.
fn card_menu(ui: &mut egui::Ui, rect: Rect, id: egui::Id, visible: bool) -> Option<CardAction> {
    let response = ui.interact(rect, id, Sense::click());
    let open = egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&response));
    let shown = theme::ease(
        ui.ctx(),
        id.with("shown"),
        visible || open || response.has_focus(),
    );
    if shown > 0.0 {
        let palette = theme::palette(ui.ctx());
        let fill = if response.hovered() || open {
            palette.control_active
        } else {
            palette.elevated
        };
        ui.painter().rect(
            rect,
            6,
            fill.gamma_multiply(shown),
            Stroke::new(1.0_f32, palette.border.gamma_multiply(shown)),
            egui::StrokeKind::Inside,
        );
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            icons::DOTS_THREE,
            FontId::proportional(16.0),
            palette.text_strong.gamma_multiply(shown),
        );
    }
    let response = response.on_hover_text("More");
    let mut action = None;
    egui::Popup::menu(&response).show(|ui| project_menu(ui, &mut action));
    action
}

/// One project as a card: its picture, name and when it was edited. Clicking
/// it opens it; ⋯ (shown on hover) and right-click have its options.
fn project_card(
    ui: &mut egui::Ui,
    link: &ProjectLink,
    thumbnail: Option<&egui::TextureHandle>,
    edited: Option<String>,
    width: f32,
) -> Option<CardAction> {
    let picture_height = (width * 9.0 / 16.0).round();
    let size = vec2(width, picture_height + 54.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &link.name));
    let palette = theme::palette(ui.ctx());
    let hovered = ui.rect_contains_pointer(rect);
    let hover = theme::ease(ui.ctx(), response.id.with("hover"), hovered);
    let picture = Rect::from_min_size(rect.min, vec2(width, picture_height));
    thumbnail_image(
        ui,
        picture,
        CornerRadius::same(CARD_RADIUS),
        &link.name,
        thumbnail,
    );
    ui.painter().rect_stroke(
        picture,
        CARD_RADIUS,
        Stroke::new(
            1.0_f32,
            theme::mix(palette.border, palette.border_strong, hover),
        ),
        egui::StrokeKind::Inside,
    );
    // A faint wash over the picture on hover, so it reads as clickable.
    if hover > 0.0 {
        ui.painter().rect_filled(
            picture,
            CARD_RADIUS,
            palette.text_strong.gamma_multiply(0.04 * hover),
        );
    }
    if response.has_focus() {
        ui.painter().rect_stroke(
            picture.expand(2.0),
            CARD_RADIUS + 2,
            Stroke::new(1.5_f32, palette.accent),
            egui::StrokeKind::Inside,
        );
    }

    let text_left = rect.left() + 2.0;
    let text_width = width - 4.0;
    let name = widgets::elided(
        ui,
        link.name.clone(),
        FontId::new(type_scale::BODY + 0.5, theme::medium_family()),
        palette.text_strong,
        text_width,
        1,
    );
    let name_top = picture.bottom() + 10.0;
    ui.painter()
        .galley(pos2(text_left, name_top), name, palette.text_strong);
    let detail = match &edited {
        Some(edited) => format!("Edited {edited}"),
        None => link.folder.display().to_string(),
    };
    let detail = match link
        .description
        .lines()
        .next()
        .filter(|line| !line.is_empty())
    {
        Some(description) => format!("{detail}  ·  {description}"),
        None => detail,
    };
    let detail = widgets::elided(
        ui,
        detail,
        FontId::proportional(type_scale::SMALL),
        palette.text_weak,
        text_width,
        1,
    );
    ui.painter()
        .galley(pos2(text_left, name_top + 20.0), detail, palette.text_weak);

    let menu = Rect::from_min_size(
        pos2(picture.right() - 36.0, picture.top() + 8.0),
        vec2(28.0, 24.0),
    );
    let mut action = card_menu(
        ui,
        menu,
        egui::Id::new(("card menu", &link.folder)),
        hovered,
    );
    let response = response
        .on_hover_text(format!("Open {}", link.folder.display()))
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    response.context_menu(|ui| project_menu(ui, &mut action));
    if action.is_none() && response.clicked() {
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
    let size = vec2(ui.available_width(), 52.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &link.name));
    let palette = theme::palette(ui.ctx());
    let hovered = ui.rect_contains_pointer(rect);
    widgets::paint_hover(ui, rect, &response, false, 8.0);
    if hovered && !response.hovered() {
        // Over the ⋯ button: keep the row lit.
        ui.painter().rect_filled(rect, 8, palette.control_hover);
    }
    let picture = Rect::from_min_size(rect.min + vec2(8.0, 8.0), vec2(36.0 * 16.0 / 9.0, 36.0));
    thumbnail_image(ui, picture, CornerRadius::same(5), &link.name, thumbnail);
    ui.painter().rect_stroke(
        picture,
        5,
        Stroke::new(1.0_f32, palette.border),
        egui::StrokeKind::Inside,
    );
    let left = picture.right() + 14.0;
    let edited_width = 150.0;
    let text_width = (rect.right() - edited_width - 50.0 - left).max(40.0);
    let name = widgets::elided(
        ui,
        link.name.clone(),
        FontId::new(type_scale::BODY, theme::medium_family()),
        palette.text_strong,
        text_width,
        1,
    );
    ui.painter()
        .galley(pos2(left, rect.top() + 9.0), name, palette.text_strong);
    let path = widgets::elided(
        ui,
        link.folder.display().to_string(),
        FontId::proportional(type_scale::SMALL),
        palette.text_weak,
        text_width,
        1,
    );
    ui.painter()
        .galley(pos2(left, rect.top() + 28.0), path, palette.text_weak);
    if let Some(edited) = edited {
        ui.painter().text(
            rect.right_center() - vec2(50.0, 0.0),
            egui::Align2::RIGHT_CENTER,
            format!("Edited {edited}"),
            FontId::proportional(type_scale::SMALL),
            palette.text_weak,
        );
    }
    let menu = Rect::from_center_size(rect.right_center() - vec2(22.0, 0.0), vec2(28.0, 24.0));
    let mut action = card_menu(ui, menu, egui::Id::new(("row menu", &link.folder)), hovered);
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    response.context_menu(|ui| project_menu(ui, &mut action));
    if action.is_none() && response.clicked() {
        action = Some(CardAction::Open);
    }
    action
}

/// A quiet note in place of the project cards.
fn empty_state(ui: &mut egui::Ui, icon: &str, title: &str, text: &str) {
    let palette = theme::palette(ui.ctx());
    ui.add_space(8.0);
    egui::Frame::new()
        .stroke(Stroke::new(1.0_f32, palette.border))
        .corner_radius(CARD_RADIUS)
        .inner_margin(egui::Margin::symmetric(16, 32))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(icon).size(24.0).color(palette.text_weak));
                ui.add_space(4.0);
                ui.label(
                    RichText::new(title)
                        .family(theme::medium_family())
                        .color(palette.text_strong),
                );
                ui.label(RichText::new(text).color(palette.text_weak));
            });
        });
    ui.add_space(8.0);
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
    let palette = theme::palette(ui.ctx());
    ui.label(
        RichText::new("Learn")
            .family(theme::heading_family())
            .size(type_scale::DISPLAY)
            .color(palette.text_strong),
    );
    ui.label(
        RichText::new("The basics of each part of the editor. Open or make a project to try them.")
            .color(palette.text_secondary),
    );
    ui.add_space(24.0);
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
    let width = ui.available_width();
    let columns = ((width + CARD_SPACING) / (260.0 + CARD_SPACING))
        .floor()
        .max(1.0);
    let card_width = (width - CARD_SPACING * (columns - 1.0)) / columns;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(CARD_SPACING, CARD_SPACING);
        for (icon, title, text) in topics {
            let (rect, response) = ui.allocate_exact_size(vec2(card_width, 128.0), Sense::hover());
            let hover = theme::ease(ui.ctx(), response.id.with("hover"), response.hovered());
            card_frame(ui, rect, &response, hover, &palette);
            ui.painter().text(
                rect.min + vec2(16.0, 24.0),
                egui::Align2::LEFT_CENTER,
                icon,
                FontId::proportional(16.0),
                palette.accent,
            );
            ui.painter().text(
                rect.min + vec2(40.0, 24.0),
                egui::Align2::LEFT_CENTER,
                title,
                FontId::new(type_scale::BODY, theme::medium_family()),
                palette.text_strong,
            );
            let galley = ui.painter().layout(
                text.to_owned(),
                FontId::proportional(type_scale::SMALL + 0.5),
                palette.text_secondary,
                rect.width() - 32.0,
            );
            ui.painter().with_clip_rect(rect.shrink(1.0)).galley(
                rect.min + vec2(16.0, 44.0),
                galley,
                palette.text_secondary,
            );
        }
    });
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
            ui.set_width(WORDMARK_WIDTH);
            ui.vertical_centered(|ui| {
                let texture = brand.wordmark(ui.visuals());
                let size = texture.size_vec2();
                let height = WORDMARK_WIDTH * size.y / size.x;
                ui.add(egui::Image::new(texture).fit_to_exact_size(vec2(WORDMARK_WIDTH, height)));
                ui.label(
                    RichText::new(format!("Axcenity Engine {}", env!("CARGO_PKG_VERSION")))
                        .family(theme::heading_family())
                        .size(type_scale::HEADING)
                        .color(palette.text_strong),
                );
                ui.add_space(4.0);
                ui.label(
                    RichText::new("A 3D game engine and editor, scripted with Lua 5.4 and blocks.")
                        .color(palette.text_secondary),
                );
                ui.label(RichText::new("Licensed under Apache-2.0.").color(palette.text_secondary));
                ui.add_space(12.0);
                ui.label(
                    RichText::new(
                        "Fonts: Inter, Montserrat and JetBrains Mono (SIL Open Font License \
                         1.1). Icons: Phosphor (MIT). Built with egui, wgpu, Rapier and Lua.",
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
        for (name, page, dialog, theme) in [
            (
                "start-projects",
                Page::Projects,
                false,
                theme::ThemeChoice::Dark,
            ),
            (
                "start-projects-light",
                Page::Projects,
                false,
                theme::ThemeChoice::Light,
            ),
            ("start-learn", Page::Learn, false, theme::ThemeChoice::Dark),
            (
                "start-new-project",
                Page::Projects,
                true,
                theme::ThemeChoice::Dark,
            ),
        ] {
            let mut start = StartScreen {
                page,
                ..Default::default()
            };
            if dialog {
                start.new_project(Template::Starter);
            }
            let appearance = theme::Appearance {
                theme,
                ..Default::default()
            };
            let mut draw = |ctx: &egui::Context| {
                theme::apply(ctx, &appearance);
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE.fill(theme::palette(ctx).panel))
                    .show(ctx, |ui| {
                        assert!(start.show(ui, &mut projects).is_none());
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
