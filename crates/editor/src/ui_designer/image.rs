//! The inspector's Image row, for an element with an `Image` (ImageLabel,
//! TextButton): the picture it shows, picked from the project's images
//! (a grid of thumbnails, the Assets panel's Images then Textures), dropped
//! on the row from the Assets panel, or uploaded from this computer (copied
//! into `assets/images` by the app first).

use axcenity_core::{InstanceId, Value};
use eframe::egui;

use super::{DesignerAction, DesignerScene};
use crate::asset_browser::{AssetKind, AssetPayload, TexturePick};
use crate::properties::Edit;
use crate::theme::{self, icons};

/// A thumbnail's side in the picker's grid, in points.
const TILE: f32 = 64.0;
/// The row's own thumbnail, in points.
const SMALL: f32 = 22.0;

/// What was done in an image picker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImagePick {
    /// Use this project path (empty for no picture).
    Use(String),
    /// Choose a file on this computer and copy it into the project.
    Upload,
}

/// The Image row of `id` (whose `Image` is `current`), inside the
/// inspector's form. Its pick becomes an action.
pub(super) fn image_row(
    ui: &mut egui::Ui,
    scene: &DesignerScene,
    id: InstanceId,
    current: &str,
    actions: &mut Vec<DesignerAction>,
) {
    ui.label("Image")
        .on_hover_text("The picture it shows: one of the project's images, or upload one");
    let pick = picker(ui, scene.images, current, ("ui designer image", id), true);
    match pick {
        Some(ImagePick::Use(path)) => {
            actions.push(DesignerAction::Image(id, path));
        }
        Some(ImagePick::Upload) => actions.push(DesignerAction::UploadImage(id)),
        None => {}
    }
    ui.end_row();
}

/// A picture's thumbnail and name, a menu of the project's `images`, an
/// Upload button (when `upload`) and × to clear. Accepts an image dropped
/// from the Assets panel.
pub fn picker(
    ui: &mut egui::Ui,
    images: &[TexturePick],
    current: &str,
    salt: impl std::hash::Hash + std::fmt::Debug,
    upload: bool,
) -> Option<ImagePick> {
    let mut pick = None;
    let palette = theme::palette(ui.ctx());
    let row = ui.horizontal(|ui| {
        let thumbnail = images
            .iter()
            .find(|image| image.entry.path == current)
            .and_then(|image| image.thumbnail.as_ref());
        match thumbnail {
            Some(texture) => {
                ui.add(egui::Image::new(texture).fit_to_exact_size(egui::Vec2::splat(SMALL)));
            }
            None => {
                ui.label(egui::RichText::new(icons::IMAGE).size(SMALL * 0.8).weak());
            }
        }
        let name = if current.is_empty() {
            "None".to_owned()
        } else {
            current.rsplit('/').next().unwrap_or(current).to_owned()
        };
        let menu = egui::containers::menu::MenuButton::new(egui::RichText::new(format!(
            "{name} {}",
            icons::CARET_DOWN
        )))
        .config(
            egui::containers::menu::MenuConfig::new()
                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside),
        );
        let (response, _) = menu.ui(ui, |ui| {
            ui.push_id(&salt, |ui| {
                if let Some(path) = grid(ui, images, current) {
                    pick = Some(ImagePick::Use(path));
                    ui.close();
                }
            });
        });
        response.on_hover_text(if current.is_empty() {
            "Choose one of the project's images".to_owned()
        } else {
            current.to_owned()
        });
        if upload
            && ui
                .small_button(icons::UPLOAD_SIMPLE)
                .on_hover_text("Upload an image from this computer (copied into assets/images)")
                .clicked()
        {
            pick = Some(ImagePick::Upload);
        }
        if !current.is_empty()
            && ui
                .small_button(icons::X)
                .on_hover_text("Show no picture")
                .clicked()
        {
            pick = Some(ImagePick::Use(String::new()));
        }
    });
    // An image dragged from the Assets panel onto the row.
    let response = &row.response;
    if let Some(held) = response.dnd_hover_payload::<AssetPayload>()
        && is_picture(held.0.kind)
    {
        ui.painter().rect_stroke(
            response.rect.expand(2.0),
            4,
            egui::Stroke::new(1.5_f32, palette.accent),
            egui::StrokeKind::Outside,
        );
    }
    if let Some(dropped) = response.dnd_release_payload::<AssetPayload>()
        && is_picture(dropped.0.kind)
    {
        pick = Some(ImagePick::Use(dropped.0.path.clone()));
    }
    pick
}

/// Whether an Assets panel tile of `kind` can be a UI element's picture.
fn is_picture(kind: AssetKind) -> bool {
    matches!(kind, AssetKind::Image | AssetKind::Texture)
}

/// The project's images as a grid of thumbnails with their names, the
/// current one marked. Returns the path clicked.
fn grid(ui: &mut egui::Ui, images: &[TexturePick], current: &str) -> Option<String> {
    let palette = theme::palette(ui.ctx());
    if images.is_empty() {
        ui.weak("The project has no images yet. Upload one, or import one in the Assets panel.");
        return None;
    }
    let mut picked = None;
    let columns = 4;
    egui::ScrollArea::vertical()
        .max_height(320.0)
        .show(ui, |ui| {
            egui::Grid::new("image picker grid")
                .spacing(egui::vec2(6.0, 6.0))
                .show(ui, |ui| {
                    for (index, image) in images.iter().enumerate() {
                        let chosen = image.entry.path == current;
                        let response = ui
                            .vertical(|ui| {
                                ui.set_width(TILE);
                                let (rect, response) = ui.allocate_exact_size(
                                    egui::Vec2::splat(TILE),
                                    egui::Sense::click(),
                                );
                                ui.painter().rect_filled(rect, 4, palette.inset);
                                match &image.thumbnail {
                                    Some(texture) => {
                                        egui::Image::new(texture)
                                            .fit_to_exact_size(rect.size() - egui::Vec2::splat(6.0))
                                            .paint_at(ui, rect.shrink(3.0));
                                    }
                                    None => {
                                        ui.painter().text(
                                            rect.center(),
                                            egui::Align2::CENTER_CENTER,
                                            icons::IMAGE,
                                            egui::FontId::proportional(22.0),
                                            palette.text_muted,
                                        );
                                    }
                                }
                                if chosen || response.hovered() {
                                    ui.painter().rect_stroke(
                                        rect,
                                        4,
                                        egui::Stroke::new(
                                            if chosen { 2.0_f32 } else { 1.0 },
                                            palette.accent,
                                        ),
                                        egui::StrokeKind::Inside,
                                    );
                                }
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(&image.entry.name).small(),
                                    )
                                    .truncate(),
                                );
                                response
                            })
                            .inner;
                        if response.on_hover_text(&image.entry.path).clicked() {
                            picked = Some(image.entry.path.clone());
                        }
                        if index % columns == columns - 1 {
                            ui.end_row();
                        }
                    }
                });
        });
    picked
}

/// The edit an [`ImagePick::Use`] makes.
pub fn image_edit(path: String) -> Edit {
    Edit::Property("Image", Value::String(path))
}
