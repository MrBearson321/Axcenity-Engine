//! The Import dialog's tabs: choosing the objects and how they arrive,
//! and each material's texture maps. (The Animation tab is
//! `AnimationImport::show`.)

use axcenity_assets::Channel;
use axcenity_core::ClassName;
use eframe::egui;

use super::preview::ImportPreview;
use super::{ImportDialog, Placement, plural, thousands};
use crate::surface::{MapFile, MaterialMaps, Slot};
use crate::theme::{self, icons};
use crate::widgets;

/// The side of a map's picture on the Materials tab.
const MAP_PICTURE: f32 = 36.0;
/// The side of the small pictures in a material's header.
const HEADER_PICTURE: f32 = 18.0;

/// The object tree, with its search box and the count chosen.
pub(super) fn objects(dialog: &mut ImportDialog, ui: &mut egui::Ui) {
    hint(
        ui,
        "Tick the objects to bring in. Ticking a group ticks everything inside it.",
    );
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        widgets::search_field(ui, &mut dialog.filter, "Search objects", 160.0);
        let shown = dialog.shown();
        // With a search, these tick only what's shown.
        if ui.button("Select all").clicked() {
            for &i in &shown {
                dialog.ticked[i] = true;
            }
        }
        if ui.button("Select none").clicked() {
            for &i in &shown {
                dialog.ticked[i] = false;
            }
        }
    });
    ui.add_space(4.0);

    let palette = theme::palette(ui.ctx());
    egui::ScrollArea::vertical()
        .id_salt("objects")
        .max_height(300.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            let searching = !dialog.filter.trim().is_empty();
            for i in dialog.shown() {
                let object = &dialog.objects[i];
                let (mut checked, partly) = dialog.state(i);
                let (class, details) = if object.has_mesh() {
                    let materials = match object.materials.len() {
                        0 => String::new(),
                        _ => format!(" · {}", object.materials.join(", ")),
                    };
                    (
                        ClassName::MeshPart,
                        format!("{} triangles{materials}", thousands(object.triangles)),
                    )
                } else {
                    (ClassName::Model, "group".to_owned())
                };
                let name = object.name.clone();
                // A search shows matches without their place in the tree.
                let indent = if searching { 0 } else { object.depth };
                let changed = ui
                    .horizontal(|ui| {
                        ui.add_space(indent as f32 * 18.0);
                        let changed = ui
                            .add(egui::Checkbox::new(&mut checked, "").indeterminate(partly))
                            .changed();
                        ui.label(
                            egui::RichText::new(theme::class_icon(class))
                                .color(theme::class_color(class, ui.visuals())),
                        );
                        ui.label(name);
                        ui.label(
                            egui::RichText::new(details)
                                .size(theme::type_scale::SMALL)
                                .color(palette.text_weak),
                        );
                        changed
                    })
                    .inner;
                if changed {
                    dialog.set(i, checked);
                }
            }
        });

    let chosen = dialog.chosen();
    let triangles: usize = chosen.iter().map(|&i| dialog.objects[i].triangles).sum();
    let meshes = dialog.objects.iter().filter(|o| o.has_mesh()).count();
    ui.add_space(4.0);
    hint(
        ui,
        &format!(
            "{} of {meshes} objects ticked, {} triangles.",
            chosen.len(),
            thousands(triangles)
        ),
    );
}

/// How the objects arrive: a card for each way, saying what it makes and
/// what it's good for, with the one that suits the file marked.
pub(super) fn placement(dialog: &mut ImportDialog, ui: &mut egui::Ui) {
    hint(
        ui,
        "How the ticked objects arrive in the world. \"What you'll get\" shows exactly what \
         the choice makes.",
    );
    ui.add_space(6.0);
    let name = dialog.model_name();
    let parts = dialog.chosen().len();
    let animated = dialog.animator_clips().is_some();
    let recommended = if animated {
        Placement::Merged
    } else {
        Placement::Grouped
    };
    let cards = [
        (
            Placement::Grouped,
            ClassName::Model,
            "A Model with a part per object",
            format!(
                "Each object becomes its own MeshPart, laid out as in the file, inside one \
                 Model named {name}. Move the Model to move everything, or select a part to \
                 change it alone. Best for buildings, vehicles and props packs."
            ),
        ),
        (
            Placement::Separate,
            ClassName::MeshPart,
            "Separate MeshParts",
            format!(
                "{} loose in the Workspace, all at the same spot (on top of each other until \
                 you move them). Best for a set of props you'll place one by one.",
                plural(parts, "MeshPart")
            ),
        ),
        (
            Placement::Merged,
            ClassName::MeshPart,
            "One MeshPart",
            format!(
                "Everything becomes a single MeshPart named {name}. Simplest to move, and one \
                 Animator moves all of it, so it's best for a character. Its pieces can't be \
                 selected or changed on their own."
            ),
        ),
    ];
    for (placement, class, title, text) in cards {
        let selected = dialog.placement == placement;
        if card(ui, class, title, &text, selected, placement == recommended) {
            dialog.placement = placement;
        }
        ui.add_space(6.0);
    }
    if animated && dialog.placement != Placement::Merged {
        let palette = theme::palette(ui.ctx());
        ui.label(
            egui::RichText::new(format!(
                "{} Each part gets its own Animator. For a character, One MeshPart keeps it \
                 moving as one.",
                icons::WARNING
            ))
            .color(palette.warning),
        );
    }
}

/// A choice drawn as a card: a radio, an icon and a title, with what it
/// does under them. Returns true when it's clicked anywhere.
fn card(
    ui: &mut egui::Ui,
    class: ClassName,
    title: &str,
    text: &str,
    selected: bool,
    recommended: bool,
) -> bool {
    let palette = theme::palette(ui.ctx());
    let id = ui.id().with(("card", title));
    // Hovered on the last frame: the card's rect isn't known until it's drawn.
    let hovered = ui.ctx().read_response(id).is_some_and(|r| r.hovered());
    let (stroke, fill) = if selected {
        (
            egui::Stroke::new(1.5_f32, palette.accent),
            palette.accent_soft,
        )
    } else if hovered {
        (
            egui::Stroke::new(1.0_f32, palette.border_strong),
            palette.control_hover,
        )
    } else {
        (egui::Stroke::new(1.0_f32, palette.border), palette.inset)
    };
    let inner = egui::Frame::new()
        .fill(fill)
        .stroke(stroke)
        .corner_radius(8)
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let radio = ui
                .horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    let radio = ui.add(egui::RadioButton::new(selected, ""));
                    quiet(
                        ui,
                        egui::RichText::new(theme::class_icon(class))
                            .size(16.0)
                            .color(theme::class_color(class, ui.visuals())),
                    );
                    quiet(
                        ui,
                        egui::RichText::new(title)
                            .family(theme::medium_family())
                            .color(palette.text_strong),
                    );
                    if recommended {
                        quiet(
                            ui,
                            egui::RichText::new(" Recommended ")
                                .size(theme::type_scale::CAPTION)
                                .color(palette.accent_text)
                                .background_color(palette.accent_soft),
                        );
                    }
                    radio.clicked()
                })
                .inner;
            ui.add_space(2.0);
            quiet(
                ui,
                egui::RichText::new(text)
                    .size(theme::type_scale::SMALL)
                    .color(palette.text_secondary),
            );
            radio
        });
    // Over the whole card, after its contents, so a click anywhere on it
    // counts (its labels don't take clicks).
    let response = ui.interact(inner.response.rect, id, egui::Sense::click());
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    inner.inner || response.clicked()
}

/// Each material's maps, as found in the file, to change before importing.
pub(super) fn materials(dialog: &mut ImportDialog, ui: &mut egui::Ui) {
    hint(
        ui,
        "Each material becomes a SurfaceAppearance on the new MeshPart, with these maps. \
         They're the ones the file uses: replace or remove any, or add one it's missing. You \
         can change them later in Properties.",
    );
    ui.add_space(6.0);
    let open = dialog.materials.len() <= 2;
    for (index, maps) in dialog.materials.iter_mut().enumerate() {
        material(ui, index, maps, dialog.preview.as_mut(), open || index == 0);
        ui.add_space(4.0);
    }
}

/// One material, as a card that opens: its name, how many maps, and
/// small pictures of them; open, a row per map it has and Add map.
fn material(
    ui: &mut egui::Ui,
    index: usize,
    maps: &mut MaterialMaps,
    mut preview: Option<&mut ImportPreview>,
    open: bool,
) {
    let palette = theme::palette(ui.ctx());
    let title = match maps.material.as_str() {
        "" => "(unnamed material)".to_owned(),
        name => name.to_owned(),
    };
    let found = maps.files.iter().flatten().count();
    egui::Frame::new()
        .fill(palette.inset)
        .stroke(egui::Stroke::new(1.0_f32, palette.border))
        .corner_radius(8)
        .inner_margin(egui::Margin::symmetric(8, 6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let id = ui.id().with(("material", index));
            egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, open)
                .show_header(ui, |ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    ui.label(
                        egui::RichText::new(theme::class_icon(ClassName::SurfaceAppearance)).color(
                            theme::class_color(ClassName::SurfaceAppearance, ui.visuals()),
                        ),
                    );
                    ui.label(egui::RichText::new(&title).strong());
                    ui.label(
                        egui::RichText::new(match found {
                            0 => "no maps".to_owned(),
                            n => plural(n, "map"),
                        })
                        .size(theme::type_scale::SMALL)
                        .color(palette.text_weak),
                    );
                    for slot in Slot::ALL {
                        if let (Some(file), Some(preview)) =
                            (maps.file(slot), preview.as_deref_mut())
                        {
                            let picture = preview.thumbnail(ui.ctx(), file);
                            map_picture(ui, picture.as_ref(), HEADER_PICTURE)
                                .on_hover_text(slot.label());
                        }
                    }
                })
                .body(|ui| map_rows(ui, index, maps, preview));
        });
}

/// A row for each map the material has (its picture, what it is, its
/// file, its channel for grey maps, Replace and Remove), the settings
/// that matter for those maps, and Add map for the rest.
fn map_rows(
    ui: &mut egui::Ui,
    index: usize,
    maps: &mut MaterialMaps,
    mut preview: Option<&mut ImportPreview>,
) {
    let palette = theme::palette(ui.ctx());
    let has = |maps: &MaterialMaps, slot| maps.file(slot).is_some();
    if maps.files.iter().all(Option::is_none) {
        hint(ui, "No texture maps: it's drawn with its colors only.");
    }
    for slot in Slot::ALL {
        let Some(file) = maps.file(slot).cloned() else {
            continue;
        };
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let picture = preview
                .as_deref_mut()
                .and_then(|preview| preview.thumbnail(ui.ctx(), &file));
            map_picture(ui, picture.as_ref(), MAP_PICTURE);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.label(egui::RichText::new(slot.label()).strong())
                    .on_hover_text(slot_help(slot));
                let path = match &file {
                    MapFile::Model(path) => path.clone(),
                    MapFile::Picked(path) => path.display().to_string(),
                };
                ui.label(
                    egui::RichText::new(file.file_name())
                        .size(theme::type_scale::SMALL)
                        .color(palette.text_weak),
                )
                .on_hover_text(path);
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .small_button(icons::X)
                    .on_hover_text("Use no map here")
                    .clicked()
                {
                    maps.set_file(slot, None);
                }
                if ui
                    .small_button(icons::FOLDER_OPEN)
                    .on_hover_text("Use another texture file")
                    .clicked()
                    && let Some(picked) = pick_texture(slot)
                {
                    maps.set_file(slot, Some(MapFile::Picked(picked)));
                }
                if let Some(channel) = maps.channel_mut(slot) {
                    egui::ComboBox::from_id_salt(("channel", index, slot as usize))
                        .width(70.0)
                        .selected_text(channel_name(*channel))
                        .show_ui(ui, |ui| {
                            for item in
                                [Channel::Red, Channel::Green, Channel::Blue, Channel::Alpha]
                            {
                                ui.selectable_value(channel, item, channel_name(item));
                            }
                        })
                        .response
                        .on_hover_text("The channel of the texture this map is in");
                }
            });
        });
    }
    // Only the settings for maps it has.
    ui.horizontal_wrapped(|ui| {
        if has(maps, Slot::Normal) {
            ui.checkbox(&mut maps.normal_directx, "Normal map is DirectX-style")
                .on_hover_text(
                    "Tick if bumps look dented in (green points down, as in DirectX and Unreal).",
                );
        }
        if has(maps, Slot::Roughness) {
            ui.checkbox(&mut maps.invert_roughness, "Roughness map is glossiness")
                .on_hover_text(
                    "Tick if the map is white where it's shiny (a gloss or smoothness map).",
                );
        }
        if has(maps, Slot::Opacity) {
            ui.checkbox(&mut maps.invert_opacity, "Opacity map is inverted")
                .on_hover_text("Tick if the map is white where it's see-through.");
        }
    });
    let missing: Vec<Slot> = Slot::ALL
        .into_iter()
        .filter(|&slot| !has(maps, slot))
        .collect();
    if !missing.is_empty() {
        ui.menu_button(format!("{} Add map", icons::PLUS), |ui| {
            for slot in missing {
                if ui
                    .button(slot.label())
                    .on_hover_text(slot_help(slot))
                    .clicked()
                {
                    ui.close();
                    if let Some(picked) = pick_texture(slot) {
                        maps.set_file(slot, Some(MapFile::Picked(picked)));
                    }
                }
            }
        });
    }
}

/// Asks for a texture file for `slot` with the system's file picker.
fn pick_texture(slot: Slot) -> Option<std::path::PathBuf> {
    rfd::FileDialog::new()
        .set_title(format!("Choose a {} map", slot.label().to_lowercase()))
        .add_filter("Texture", &axcenity_assets::TEXTURE_EXTENSIONS)
        .pick_file()
}

/// A map's picture in a square `side` points across, or an image icon
/// until there is one.
fn map_picture(
    ui: &mut egui::Ui,
    picture: Option<&egui::TextureHandle>,
    side: f32,
) -> egui::Response {
    let palette = theme::palette(ui.ctx());
    let (rect, response) = ui.allocate_exact_size(egui::Vec2::splat(side), egui::Sense::hover());
    ui.painter().rect(
        rect,
        3.0,
        palette.surface,
        egui::Stroke::new(1.0_f32, palette.border),
        egui::StrokeKind::Inside,
    );
    match picture {
        Some(texture) => {
            let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
            ui.painter()
                .image(texture.id(), rect.shrink(1.0), uv, egui::Color32::WHITE);
        }
        None => {
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                icons::IMAGE_SQUARE,
                egui::FontId::proportional(side * 0.5),
                palette.text_muted,
            );
        }
    }
    response
}

/// What a map does, for its tooltip.
fn slot_help(slot: Slot) -> &'static str {
    match slot {
        Slot::Color => "The surface's color (also called albedo, diffuse or base color).",
        Slot::Normal => "Fakes small bumps and dents in how light falls on it.",
        Slot::Metalness => "Which parts are metal (white) and which aren't (black).",
        Slot::Roughness => "How rough (white, matte) or smooth (black, shiny) each part is.",
        Slot::Emissive => "Parts that glow.",
        Slot::AmbientOcclusion => "Darkens creases and corners that little light reaches.",
        Slot::Opacity => "Which parts are solid (white) and which see-through (black).",
        Slot::Specular => "How strongly the non-metal parts reflect, for older materials.",
    }
}

fn channel_name(channel: Channel) -> &'static str {
    match channel {
        Channel::Red => "Red",
        Channel::Green => "Green",
        Channel::Blue => "Blue",
        Channel::Alpha => "Alpha",
    }
}

/// A line explaining a part of a tab, in the quiet text colour.
fn hint(ui: &mut egui::Ui, text: &str) {
    let palette = theme::palette(ui.ctx());
    ui.label(egui::RichText::new(text).color(palette.text_secondary));
}

/// Text that doesn't take clicks or selection, for inside a card.
fn quiet(ui: &mut egui::Ui, text: egui::RichText) {
    ui.add(
        egui::Label::new(text)
            .selectable(false)
            .sense(egui::Sense::hover()),
    );
}
