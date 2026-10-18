//! The start screen shown while no project is open (the logo, Open Project,
//! recent projects and the examples), the About box, and the logo images.

use std::path::{Path, PathBuf};

use eframe::egui;

use crate::theme::{self, icons};

/// How many recent projects the start screen lists.
pub const MAX_RECENT: usize = 8;
/// How wide the wordmark is drawn, in points. The images are twice this,
/// so they stay sharp on high-DPI screens.
const WORDMARK_WIDTH: f32 = 420.0;

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
    pub folder: PathBuf,
}

impl ProjectLink {
    /// The project in `folder`, or `None` if it isn't a project folder.
    pub fn read(folder: &Path) -> Option<Self> {
        Some(Self {
            name: axcenity_project::project_name(folder)?,
            folder: folder.to_owned(),
        })
    }
}

/// The example projects in `examples/` under the current folder, by name.
/// Empty when the editor isn't run from the engine's folder.
pub fn find_examples(root: &Path) -> Vec<ProjectLink> {
    let Ok(entries) = std::fs::read_dir(root.join("examples")) else {
        return Vec::new();
    };
    let mut examples: Vec<ProjectLink> = entries
        .filter_map(|entry| ProjectLink::read(&entry.ok()?.path()))
        .collect();
    examples.sort_by(|a, b| a.name.cmp(&b.name));
    examples
}

/// Puts `folder` first in `recent`, removing it from further down, and keeps
/// the list at [`MAX_RECENT`].
pub fn remember(recent: &mut Vec<ProjectLink>, project: ProjectLink) {
    recent.retain(|link| link.folder != project.folder);
    recent.insert(0, project);
    recent.truncate(MAX_RECENT);
}

/// What was picked on the start screen.
pub enum StartAction {
    OpenDialog,
    Open(PathBuf),
}

/// Draws the start screen. Returns what was picked, if anything.
pub fn show(
    ui: &mut egui::Ui,
    brand: &Brand,
    recent: &[ProjectLink],
    examples: &[ProjectLink],
) -> Option<StartAction> {
    let mut action = None;
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.set_max_width(640.0);
                ui.add_space(36.0);
                wordmark(ui, brand);
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("Make 3D games with Lua and blocks.")
                        .size(15.0)
                        .color(ui.visuals().weak_text_color()),
                );
                ui.add_space(22.0);
                let open = egui::Button::new(
                    egui::RichText::new(theme::with_icon(icons::FOLDER_OPEN, "Open Project…"))
                        .size(15.0)
                        .color(egui::Color32::WHITE),
                )
                .fill(theme::BRAND_BLUE)
                .min_size(egui::vec2(220.0, 40.0));
                if ui.add(open).clicked() {
                    action = Some(StartAction::OpenDialog);
                }
                ui.add_space(28.0);

                if !recent.is_empty() {
                    heading(ui, icons::CLOCK_COUNTER_CLOCKWISE, "Recent");
                    for link in recent {
                        let folder = link.folder.display().to_string();
                        if project_row(ui, icons::FOLDER_SIMPLE, link, &folder) {
                            action = Some(StartAction::Open(link.folder.clone()));
                        }
                    }
                    ui.add_space(20.0);
                }
                if !examples.is_empty() {
                    heading(ui, icons::BOOK_OPEN, "Examples");
                    for link in examples {
                        // Every example is in the same folder, so its own name is enough.
                        let folder = link
                            .folder
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy();
                        if project_row(ui, icons::GAME_CONTROLLER, link, &folder) {
                            action = Some(StartAction::Open(link.folder.clone()));
                        }
                    }
                }
                ui.add_space(24.0);
            });
        });
    action
}

fn wordmark(ui: &mut egui::Ui, brand: &Brand) {
    let texture = brand.wordmark(ui.visuals());
    let size = texture.size_vec2();
    let height = WORDMARK_WIDTH * size.y / size.x;
    ui.add(egui::Image::new(texture).fit_to_exact_size(egui::vec2(WORDMARK_WIDTH, height)));
}

/// A section title on the start screen, on the left.
fn heading(ui: &mut egui::Ui, icon: &str, title: &str) {
    // `horizontal` is only one row tall, and starts at the left.
    ui.horizontal(|ui| theme::panel_title(ui, icon, title));
    ui.add_space(2.0);
}

/// One project as a wide button: its name, and `folder` in grey. Returns
/// true when clicked.
fn project_row(ui: &mut egui::Ui, icon: &str, link: &ProjectLink, folder: &str) -> bool {
    let weak = ui.visuals().weak_text_color();
    let accent = if ui.visuals().dark_mode {
        theme::BRAND_BLUE_BRIGHT
    } else {
        theme::BRAND_BLUE
    };
    let button = egui::Button::new((
        egui::RichText::new(icon).size(18.0).color(accent),
        egui::RichText::new(&link.name).size(14.0),
        egui::Atom::grow(),
        egui::RichText::new(folder).size(11.0).color(weak),
    ))
    .min_size(egui::vec2(ui.available_width(), 36.0));
    ui.add(button)
        .on_hover_text(format!("Open {}", link.folder.display()))
        .clicked()
}

/// The About box: the logo, the version, and the licences of what's built in.
/// Returns false once it's closed.
pub fn about(ctx: &egui::Context, brand: &Brand) -> bool {
    let mut close = false;
    let modal = egui::Modal::new(egui::Id::new("about")).show(ctx, |ui| {
        ui.set_width(WORDMARK_WIDTH);
        ui.vertical_centered(|ui| {
            wordmark(ui, brand);
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

        for n in 0..MAX_RECENT + 3 {
            remember(&mut recent, link(&n.to_string()));
        }
        assert_eq!(recent.len(), MAX_RECENT);
        assert_eq!(recent[0], link(&(MAX_RECENT + 2).to_string()));
    }

    #[test]
    fn the_examples_are_found_by_name() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let names: Vec<String> = find_examples(&root)
            .into_iter()
            .map(|link| link.name)
            .collect();
        assert!(names.contains(&"Hello Touched".to_string()), "{names:?}");
        assert!(names.is_sorted());
        assert!(find_examples(&root.join("does-not-exist")).is_empty());
    }
}
