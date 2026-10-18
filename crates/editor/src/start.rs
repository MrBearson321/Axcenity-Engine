//! The start screen shown while no project is open (a grid of tracked
//! projects with thumbnails, New and Open Project, and each project's
//! settings), the About box, and the logo images.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use eframe::egui;

use crate::theme::{self, icons};

/// How many projects the start screen tracks.
pub const MAX_TRACKED: usize = 100;
/// How wide the wordmark is drawn in the About box, in points. The images
/// are twice this, so they stay sharp on high-DPI screens.
const WORDMARK_WIDTH: f32 = 420.0;
/// How wide the wordmark is drawn on the start screen.
const HEADER_WORDMARK_WIDTH: f32 = 260.0;
/// The width of a project card, and the space between cards.
const CARD_WIDTH: f32 = 250.0;
const CARD_SPACING: f32 = 16.0;

/// The logo images, as textures egui can draw.
pub struct Brand {
    /// The wordmark for light backgrounds, and for dark ones.
    wordmark: egui::TextureHandle,
    wordmark_dark: egui::TextureHandle,
    /// The hexagon in white, to tint for the menu bar.
    pub mark: egui::TextureHandle,
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
            mark: texture(
                ctx,
                "mark",
                include_bytes!("../../../assets/branding/mark-white-40.png"),
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
}

/// A box over the start screen, with what's typed in it.
enum Dialog {
    New {
        name: String,
        parent: Option<PathBuf>,
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
}

/// What a project card's buttons asked for.
enum CardAction {
    Open,
    Settings,
    Reveal,
    Forget,
    Delete,
}

/// The start screen's own state: the loaded thumbnails, the search box and
/// the open dialog.
#[derive(Default)]
pub struct StartScreen {
    /// `None` for a project without a thumbnail yet.
    thumbnails: HashMap<PathBuf, Option<egui::TextureHandle>>,
    filter: String,
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
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                ui.add_space(28.0);
                ui.horizontal(|ui| {
                    ui.add_space(24.0);
                    wordmark(ui, brand, HEADER_WORDMARK_WIDTH);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(24.0);
                        let new = egui::Button::new(
                            egui::RichText::new(theme::with_icon(icons::PLUS, "New Project"))
                                .size(15.0)
                                .color(egui::Color32::WHITE),
                        )
                        .fill(theme::BRAND_BLUE)
                        .min_size(egui::vec2(160.0, 36.0));
                        if ui.add(new).clicked() {
                            self.dialog = Some(Dialog::New {
                                name: String::new(),
                                parent: self.last_parent.clone(),
                                error: None,
                            });
                        }
                        let open = egui::Button::new(
                            egui::RichText::new(theme::with_icon(
                                icons::FOLDER_OPEN,
                                "Open Project…",
                            ))
                            .size(15.0),
                        )
                        .min_size(egui::vec2(160.0, 36.0));
                        if ui.add(open).clicked() {
                            action = Some(StartAction::OpenDialog);
                        }
                    });
                });
                ui.add_space(20.0);
                ui.horizontal(|ui| {
                    ui.add_space(24.0);
                    theme::panel_title(ui, icons::CLOCK_COUNTER_CLOCKWISE, "Projects");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(24.0);
                        ui.add(
                            egui::TextEdit::singleline(&mut self.filter)
                                .hint_text("Search projects")
                                .desired_width(200.0),
                        );
                    });
                });
                if let Some(notice) = &self.notice {
                    ui.horizontal(|ui| {
                        ui.add_space(24.0);
                        ui.colored_label(theme::STOP_RED, notice);
                    });
                }
                ui.add_space(8.0);

                let filter = self.filter.trim().to_lowercase();
                let shown: Vec<ProjectLink> = projects
                    .iter()
                    .filter(|link| {
                        filter.is_empty()
                            || link.name.to_lowercase().contains(&filter)
                            || link.description.to_lowercase().contains(&filter)
                    })
                    .cloned()
                    .collect();
                if projects.is_empty() {
                    ui.add_space(40.0);
                    ui.vertical_centered(|ui| {
                        ui.label(
                            egui::RichText::new(
                                "No projects yet. Create one, or open an existing project folder.",
                            )
                            .size(14.0)
                            .color(ui.visuals().weak_text_color()),
                        );
                    });
                } else if shown.is_empty() {
                    ui.add_space(40.0);
                    ui.vertical_centered(|ui| ui.weak("No project matches that search."));
                }
                let mut chosen = None;
                ui.horizontal_wrapped(|ui| {
                    ui.add_space(24.0 - ui.spacing().item_spacing.x);
                    ui.spacing_mut().item_spacing = egui::vec2(CARD_SPACING, CARD_SPACING);
                    for link in &shown {
                        let thumbnail = self.thumbnail(&ctx, &link.folder);
                        if let Some(picked) = project_card(ui, link, thumbnail.as_ref()) {
                            chosen = Some((link.clone(), picked));
                        }
                    }
                });
                if let Some((link, picked)) = chosen {
                    self.notice = None;
                    match picked {
                        CardAction::Open => action = Some(StartAction::Open(link.folder)),
                        CardAction::Settings => self.open_settings(link),
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
                ui.add_space(24.0);
            });
        if let Some(opened) = self.dialogs(&ctx, projects) {
            action = Some(StartAction::Open(opened));
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
        let modal = egui::Modal::new(egui::Id::new("start dialog")).show(ctx, |ui| {
            ui.set_width(380.0);
            match dialog {
                Dialog::New {
                    name,
                    parent,
                    error,
                } => {
                    ui.heading("New Project");
                    ui.add_space(6.0);
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
                            match axcenity_project::create_project(parent, name) {
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
            }
        });
        if let Some(parent) = picked_parent {
            self.last_parent = Some(parent);
        }
        if let Some(folder) = deleted {
            self.thumbnails.remove(&folder);
            projects.retain(|link| link.folder != folder);
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

/// One project: its thumbnail, name, folder and description, with Open and a
/// menu of the project's options.
fn project_card(
    ui: &mut egui::Ui,
    link: &ProjectLink,
    thumbnail: Option<&egui::TextureHandle>,
) -> Option<CardAction> {
    let mut action = None;
    let weak = ui.visuals().weak_text_color();
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.vertical(|ui| {
            ui.set_width(CARD_WIDTH);
            let size = egui::vec2(CARD_WIDTH, CARD_WIDTH * 9.0 / 16.0);
            let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
            match thumbnail {
                Some(texture) => {
                    ui.put(
                        rect,
                        egui::Image::new(texture)
                            .fit_to_exact_size(size)
                            .corner_radius(4.0),
                    );
                }
                None => {
                    ui.painter()
                        .rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        icons::CUBE,
                        egui::FontId::proportional(40.0),
                        weak,
                    );
                }
            }
            if response
                .on_hover_text(format!("Open {}", link.folder.display()))
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
            {
                action = Some(CardAction::Open);
            }
            ui.add_space(4.0);
            ui.add(
                egui::Label::new(egui::RichText::new(&link.name).size(15.0).strong()).truncate(),
            );
            ui.add(
                egui::Label::new(
                    egui::RichText::new(link.folder.display().to_string())
                        .size(11.0)
                        .color(weak),
                )
                .truncate(),
            );
            if !link.description.is_empty() {
                ui.add(
                    egui::Label::new(egui::RichText::new(&link.description).size(12.0)).truncate(),
                );
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let open =
                    egui::Button::new(egui::RichText::new("Open").color(egui::Color32::WHITE))
                        .fill(theme::BRAND_BLUE);
                if ui.add(open).clicked() {
                    action = Some(CardAction::Open);
                }
                ui.menu_button(icons::DOTS_THREE_VERTICAL, |ui| {
                    if ui
                        .button(theme::with_icon(icons::GEAR, "Settings…"))
                        .clicked()
                    {
                        action = Some(CardAction::Settings);
                        ui.close();
                    }
                    if ui
                        .button(theme::with_icon(icons::FOLDER_OPEN, "Show in File Manager"))
                        .clicked()
                    {
                        action = Some(CardAction::Reveal);
                        ui.close();
                    }
                    if ui
                        .button(theme::with_icon(icons::X_CIRCLE, "Remove from List"))
                        .on_hover_text("Stops tracking it here. The files are kept.")
                        .clicked()
                    {
                        action = Some(CardAction::Forget);
                        ui.close();
                    }
                    ui.separator();
                    let delete = egui::RichText::new(theme::with_icon(icons::TRASH, "Delete…"))
                        .color(theme::STOP_RED);
                    if ui.button(delete).clicked() {
                        action = Some(CardAction::Delete);
                        ui.close();
                    }
                });
            });
        });
    });
    action
}

fn wordmark(ui: &mut egui::Ui, brand: &Brand, width: f32) {
    let texture = brand.wordmark(ui.visuals());
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
