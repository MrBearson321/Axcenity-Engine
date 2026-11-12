//! Settings > This project > Branding (cohesion pass CW8.1): the game's
//! icon and banner, picked from the project's images, with previews and
//! what's wrong with a picture that won't do. Saved in the manifest
//! (`project::branding`); the player and Build Game use them.

use std::collections::HashMap;
use std::path::Path;

use axcenity_project::branding::{self, Branding, Picture};

use super::*;

/// The previews shown, by picture, each with the path it was made for.
#[derive(Default)]
pub(crate) struct Previews {
    shown: HashMap<&'static str, Preview>,
}

struct Preview {
    path: String,
    /// The file's time when it was read, so a changed file is read again.
    modified: Option<std::time::SystemTime>,
    texture: Option<egui::TextureHandle>,
    /// What's wrong with the picture, or `None` when it's good.
    problem: Option<String>,
}

impl Previews {
    /// What's wrong with the picture last previewed, if it was.
    #[cfg(test)]
    pub(crate) fn problem(&self, picture: Picture) -> Option<&str> {
        self.shown.get(picture.name())?.problem.as_deref()
    }

    /// The preview of `picture` at `path` in `folder`, made the first time.
    fn get(
        &mut self,
        ctx: &egui::Context,
        folder: &Path,
        picture: Picture,
        path: &str,
    ) -> &Preview {
        let modified = std::fs::metadata(folder.join(path))
            .and_then(|metadata| metadata.modified())
            .ok();
        let stale = self
            .shown
            .get(picture.name())
            .is_none_or(|preview| preview.path != path || preview.modified != modified);
        if stale {
            let problem = branding::check_picture(folder, picture, path);
            let texture = image::open(folder.join(path)).ok().map(|image| {
                let image = image.thumbnail(512, 512).to_rgba8();
                let size = [image.width() as usize, image.height() as usize];
                ctx.load_texture(
                    format!("branding {}", picture.name()),
                    egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw()),
                    egui::TextureOptions::LINEAR,
                )
            });
            self.shown.insert(
                picture.name(),
                Preview {
                    path: path.to_owned(),
                    modified,
                    texture,
                    problem,
                },
            );
        }
        &self.shown[picture.name()]
    }
}

/// The project's PNG and JPEG files, by path.
fn image_files(folder: &Path) -> Vec<String> {
    crate::asset_browser::scan(folder)
        .into_iter()
        .filter(|entry| {
            let lower = entry.path.to_lowercase();
            [".png", ".jpg", ".jpeg"]
                .iter()
                .any(|extension| lower.ends_with(extension))
        })
        .map(|entry| entry.path)
        .collect()
}

impl EditorApp {
    /// The Branding section of Settings > This project, under its heading.
    pub(super) fn branding_settings(&mut self, ui: &mut egui::Ui) {
        let Some(project) = &self.project else {
            return;
        };
        let folder = project.folder.clone();
        let mut branding = project.branding.clone();
        let palette = theme::palette(ui.ctx());
        let editing = !self.playing();
        ui.label(
            egui::RichText::new(
                "The built game's icon (window, taskbar and file) and a banner for menus and \
                 loading screens. ImageLabels and scripts can show them as branding://icon \
                 and branding://banner.",
            )
            .size(theme::type_scale::SMALL)
            .color(palette.text_secondary),
        );
        ui.add_space(theme::space::S);
        let mut changed = false;
        for picture in [Picture::Icon, Picture::Banner] {
            let slot = match picture {
                Picture::Icon => &mut branding.icon,
                Picture::Banner => &mut branding.banner,
            };
            let label = match picture {
                Picture::Icon => "Icon",
                Picture::Banner => "Banner",
            };
            ui.label(egui::RichText::new(label).color(palette.text_strong));
            ui.horizontal(|ui| {
                ui.add_enabled_ui(editing, |ui| {
                    egui::ComboBox::from_id_salt(("branding", picture.name()))
                        .selected_text(slot.as_deref().unwrap_or("Axcenity's (none set)"))
                        .width(ui.available_width().min(260.0) - 60.0)
                        .show_ui(ui, |ui| {
                            // Only listed while the menu is open.
                            for path in image_files(&folder) {
                                let picked = slot.as_deref() == Some(path.as_str());
                                if ui.selectable_label(picked, &path).clicked() && !picked {
                                    *slot = Some(path);
                                    changed = true;
                                }
                            }
                        });
                    if slot.is_some()
                        && widgets::icon_button(ui, icons::X, "Clear", false).clicked()
                    {
                        *slot = None;
                        changed = true;
                    }
                });
            });
            ui.label(
                egui::RichText::new(format!("Use {}.", picture.wanted()))
                    .size(theme::type_scale::SMALL)
                    .color(palette.text_weak),
            );
            if let Some(path) = slot.clone() {
                let preview = self
                    .branding_previews
                    .get(ui.ctx(), &folder, picture, &path);
                if let Some(texture) = &preview.texture {
                    let size = texture.size_vec2();
                    let shown = match picture {
                        Picture::Icon => egui::vec2(64.0, 64.0),
                        Picture::Banner => {
                            let width = ui.available_width().min(240.0);
                            egui::vec2(width, width * size.y / size.x.max(1.0))
                        }
                    };
                    ui.add(egui::Image::new(texture).fit_to_exact_size(shown));
                }
                if let Some(problem) = &preview.problem {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(format!(
                                "{} {problem} Built games use Axcenity's until it's fixed.",
                                icons::WARNING
                            ))
                            .size(theme::type_scale::SMALL)
                            .color(palette.warning),
                        )
                        .wrap(),
                    );
                }
            }
            ui.add_space(theme::space::S);
        }
        if changed {
            self.set_branding(branding);
        }
    }

    /// Keeps `branding` in the manifest and in use.
    pub(crate) fn set_branding(&mut self, branding: Branding) {
        let Some(project) = &mut self.project else {
            return;
        };
        match branding::set_branding(&project.folder, &branding) {
            Ok(()) => {
                self.assets
                    .set_branding(branding.icon.as_deref(), branding.banner.as_deref());
                project.branding = branding;
            }
            Err(error) => self
                .output
                .error(format!("Couldn't save the project's branding: {error}")),
        }
    }
}
