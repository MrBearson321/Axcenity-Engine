//! The Values view's rows: each of a material's values and maps beside its
//! base's, with a reset button for each one the file sets, and the Shading,
//! Toon, Outline and Water sections.

use super::*;

/// The values of `file` (the material at `path`), each beside its base's
/// (`chain`), adding a [`Change`] for each one edited.
pub(super) fn values(
    ui: &mut egui::Ui,
    scene: &MaterialScene,
    path: &str,
    file: &MaterialFile,
    chain: Option<&MaterialChain>,
    changes: &mut Vec<Change>,
) {
    let inherited_file = chain.map(|chain| chain.merged.clone()).unwrap_or_default();
    let base = inherited_file.resolve();
    let palette = theme::palette(ui.ctx());

    widgets::section_label(ui, "Base");
    responsive::form(ui, "material base", |ui| {
        ui.label("Builds on");
        let shown = file.base.as_deref().filter(|base| !base.is_empty());
        egui::ComboBox::from_id_salt("material base")
            .selected_text(shown.map_or("Nothing".to_owned(), short_name))
            .width(ui.available_width().min(220.0) - 24.0)
            .show_ui(ui, |ui| {
                if ui.selectable_label(shown.is_none(), "Nothing").clicked() {
                    changes.push(("base", Box::new(|file| file.base = None)));
                }
                for other in scene.files.iter().filter(|other| *other != path) {
                    if ui
                        .selectable_label(shown == Some(other.as_str()), other)
                        .clicked()
                    {
                        let other = other.clone();
                        changes.push(("base", Box::new(move |file| file.base = Some(other))));
                    }
                }
            });
        ui.end_row();
    });
    let mut names = vec![short_name(path)];
    match (chain, file.base.as_deref().filter(|base| !base.is_empty())) {
        (Some(chain), _) => names.extend(chain.files.iter().map(|file| short_name(file))),
        (None, Some(base)) => names.push(format!("{} (can't be read)", short_name(base))),
        (None, None) => {}
    }
    ui.label(
        egui::RichText::new(format!("{} → default", names.join(" → ")))
            .color(palette.text_secondary),
    )
    .on_hover_text("Each value this material leaves out comes from the next one along.");
    let unread = file.base.as_deref().is_some_and(|base| !base.is_empty()) && chain.is_none();
    if unread || chain.is_some_and(|chain| chain.broken) {
        ui.colored_label(
            palette.warning,
            "A base is missing or leads back around: the default material stands in for it.",
        );
    }

    ui.add_space(6.0);
    widgets::section_label(ui, "Presets");
    responsive::toolbar(ui, |ui| {
        for preset in PRESETS {
            if ui.button(preset.name).clicked() {
                changes.push(("preset", Box::new(move |file| preset.apply(file))));
            }
        }
    });

    ui.add_space(6.0);
    widgets::section_label(ui, "Surface");
    responsive::form(ui, "material surface", |ui| {
        color_row(
            ui,
            "Color",
            file.color,
            base.color,
            "color",
            changes,
            |file, value| {
                file.color = value;
            },
        );
        number_row(
            ui,
            ("Metalness", "metalness"),
            (file.metalness, base.metalness),
            0.0..=1.0,
            changes,
            |file, value| file.metalness = value,
        );
        number_row(
            ui,
            ("Roughness", "roughness"),
            (file.roughness, base.roughness),
            0.0..=1.0,
            changes,
            |file, value| file.roughness = value,
        );
        number_row(
            ui,
            ("Opacity", "opacity"),
            (file.opacity, base.opacity),
            0.0..=1.0,
            changes,
            |file, value| file.opacity = value,
        );
        number_row(
            ui,
            ("Cut out below", "alpha cutoff"),
            (file.alpha_cutoff, base.alpha_cutoff),
            0.0..=1.0,
            changes,
            |file, value| file.alpha_cutoff = value,
        );
        color_row(
            ui,
            "Glow color",
            file.emissive_color,
            base.emissive_color,
            "emissive color",
            changes,
            |file, value| file.emissive_color = value,
        );
        number_row(
            ui,
            ("Glow strength", "emissive strength"),
            (file.emissive_strength, base.emissive_strength),
            0.0..=20.0,
            changes,
            |file, value| file.emissive_strength = value,
        );
        let label = value_label(ui, "Double-sided", file.double_sided.is_some());
        label.on_hover_text("Both sides of each face are drawn and lit.");
        ui.horizontal(|ui| {
            let mut on = file.double_sided.unwrap_or(base.double_sided);
            if ui.checkbox(&mut on, "").changed() {
                changes.push((
                    "double sided",
                    Box::new(move |file| file.double_sided = Some(on)),
                ));
            }
            reset_button(
                ui,
                file.double_sided.is_some(),
                "double sided",
                changes,
                |file| {
                    file.double_sided = None;
                },
            );
        });
        ui.end_row();
        number_row(
            ui,
            ("Sway", "sway"),
            (file.sway, base.sway),
            0.0..=2.0,
            changes,
            |file, value| file.sway = value,
        );
        pair_row(
            ui,
            ("Tiling", "tiling"),
            (file.tiling, base.tiling),
            changes,
            |file, value| file.tiling = value,
        );
        pair_row(
            ui,
            ("Offset", "offset"),
            (file.offset, base.offset),
            changes,
            |file, value| file.offset = value,
        );
    });

    shading_rows(ui, scene, file, &base, changes);

    outline_rows(ui, file, &base, changes);

    water_rows(ui, file, base.water, changes);

    graph::parameter_rows(ui, scene, path, file, chain, changes);

    ui.add_space(6.0);
    widgets::section_label(ui, "Maps");
    // A graph's textures are its other maps.
    let graph = file.graph.is_some();
    if graph {
        ui.label(
            egui::RichText::new("Its other maps are the images its graph reads.")
                .color(palette.text_secondary),
        );
    }
    for slot in MapSlot::ALL {
        if !graph || matches!(slot, MapSlot::Occlusion | MapSlot::Specular) {
            map_rows(ui, scene, slot, file, &inherited_file, changes);
        }
    }
}

/// An error and its causes on one line.
pub(super) fn with_causes(err: &dyn std::error::Error) -> String {
    let mut text = err.to_string();
    let mut source = err.source();
    while let Some(cause) = source {
        text.push_str(&format!(": {cause}"));
        source = cause.source();
    }
    text
}

/// The file name of `path` without its folder or `.material.json`.
pub(super) fn short_name(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.strip_suffix(MATERIAL_SUFFIX)
        .unwrap_or(name)
        .to_owned()
}

/// A value's name, marked as changed (`widgets::changed_bar`, as in
/// Properties) when this material sets it.
pub(super) fn value_label(ui: &mut egui::Ui, name: &str, set: bool) -> egui::Response {
    let palette = theme::palette(ui.ctx());
    let label = ui.label(egui::RichText::new(name).color(palette.text_secondary));
    if set {
        widgets::changed_bar(ui, label.rect);
        label.on_hover_text("Set by this material; ↺ leaves it to the base")
    } else {
        label.on_hover_text("From its base (or the default material)")
    }
}

/// A reset button, shown when this material sets the value, that leaves it
/// to the base again.
pub(super) fn reset_button(
    ui: &mut egui::Ui,
    set: bool,
    field: &'static str,
    changes: &mut Vec<Change>,
    reset: impl FnOnce(&mut MaterialFile) + 'static,
) {
    if set
        && widgets::icon_button(
            ui,
            icons::ARROW_COUNTER_CLOCKWISE,
            "Reset to the base",
            false,
        )
        .clicked()
    {
        changes.push((field, Box::new(reset)));
    }
}

pub(super) fn number_row(
    ui: &mut egui::Ui,
    (name, field): (&str, &'static str),
    (own, base): (Option<f32>, f32),
    range: std::ops::RangeInclusive<f32>,
    changes: &mut Vec<Change>,
    set: impl Fn(&mut MaterialFile, Option<f32>) + Copy + 'static,
) {
    value_label(ui, name, own.is_some());
    ui.horizontal(|ui| {
        let mut value = own.unwrap_or(base);
        if ui.add(egui::Slider::new(&mut value, range)).changed() {
            changes.push((field, Box::new(move |file| set(file, Some(value)))));
        }
        reset_button(ui, own.is_some(), field, changes, move |file| {
            set(file, None)
        });
    });
    ui.end_row();
}

pub(super) fn color_row(
    ui: &mut egui::Ui,
    name: &str,
    own: Option<[f32; 3]>,
    base: [f32; 3],
    field: &'static str,
    changes: &mut Vec<Change>,
    set: impl Fn(&mut MaterialFile, Option<[f32; 3]>) + Copy + 'static,
) {
    value_label(ui, name, own.is_some());
    ui.horizontal(|ui| {
        // Material colors are sRGB, like the picker's bytes.
        let mut bytes = own
            .unwrap_or(base)
            .map(|channel| (channel.clamp(0.0, 1.0) * 255.0).round() as u8);
        if crate::color_palette::edit_srgb(ui, &mut bytes).changed() {
            let color = bytes.map(|byte| f32::from(byte) / 255.0);
            changes.push((field, Box::new(move |file| set(file, Some(color)))));
        }
        reset_button(ui, own.is_some(), field, changes, move |file| {
            set(file, None)
        });
    });
    ui.end_row();
}

pub(super) fn pair_row(
    ui: &mut egui::Ui,
    (name, field): (&str, &'static str),
    (own, base): (Option<[f32; 2]>, [f32; 2]),
    changes: &mut Vec<Change>,
    set: impl Fn(&mut MaterialFile, Option<[f32; 2]>) + Copy + 'static,
) {
    value_label(ui, name, own.is_some());
    ui.horizontal(|ui| {
        let mut value = own.unwrap_or(base);
        let across = ui.add(egui::DragValue::new(&mut value[0]).speed(0.01).prefix("U "));
        let down = ui.add(egui::DragValue::new(&mut value[1]).speed(0.01).prefix("V "));
        if across.changed() || down.changed() {
            changes.push((field, Box::new(move |file| set(file, Some(value)))));
        }
        reset_button(ui, own.is_some(), field, changes, move |file| {
            set(file, None)
        });
    });
    ui.end_row();
}

/// The Shading section: how the material is lit, and for Toon its bands,
/// ramp, softness, rim and highlight (`docs/rendering/stylized.md`).
/// `base` is what its bases give.
pub(super) fn shading_rows(
    ui: &mut egui::Ui,
    scene: &MaterialScene,
    file: &MaterialFile,
    base: &StandardMaterial,
    changes: &mut Vec<Change>,
) {
    ui.add_space(6.0);
    widgets::section_label(ui, "Shading");
    let shading = file.shading.unwrap_or(base.shading);
    responsive::form(ui, "material shading", |ui| {
        value_label(ui, "Shading", file.shading.is_some()).on_hover_text(
            "Standard is physically based. Unlit shows its color whatever the light. Toon \
             lights it in flat bands, with a rim light and a sharp highlight.",
        );
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("material shading")
                .selected_text(shading_name(shading))
                .show_ui(ui, |ui| {
                    for choice in [Shading::Standard, Shading::Unlit, Shading::Toon] {
                        if ui
                            .selectable_label(shading == choice, shading_name(choice))
                            .clicked()
                        {
                            changes.push((
                                "shading",
                                Box::new(move |file| file.shading = Some(choice)),
                            ));
                        }
                    }
                });
            reset_button(ui, file.shading.is_some(), "shading", changes, |file| {
                file.shading = None
            });
        });
        ui.end_row();
        if shading != Shading::Toon {
            return;
        }
        // Each value this file leaves out shows its base's (or the default).
        let below = &base.toon;
        let own = file.toon.clone().unwrap_or_default();
        value_label(ui, "Bands", own.steps.is_some())
            .on_hover_text("How many bands of light from dark to fully lit.");
        ui.horizontal(|ui| {
            let mut steps = own.steps.unwrap_or(below.steps);
            if ui.add(egui::Slider::new(&mut steps, 1..=8)).changed() {
                changes.push((
                    "toon steps",
                    Box::new(move |file| toon_of(file).steps = Some(steps)),
                ));
            }
            reset_button(ui, own.steps.is_some(), "toon steps", changes, |file| {
                toon_of(file).steps = None
            });
        });
        ui.end_row();
        value_label(ui, "Ramp", own.ramp.is_some()).on_hover_text(
            "An image whose colors, left to right, are the light from none to full, in place \
             of the bands.",
        );
        ui.horizontal(|ui| {
            let image = own
                .ramp
                .clone()
                .or_else(|| below.ramp.clone())
                .unwrap_or_default();
            let text = if image.is_empty() {
                "None".to_owned()
            } else {
                image.rsplit('/').next().unwrap_or(&image).to_owned()
            };
            egui::ComboBox::from_id_salt("toon ramp")
                .selected_text(text)
                .width((ui.available_width() - 40.0).clamp(60.0, 180.0))
                .show_ui(ui, |ui| {
                    for candidate in
                        std::iter::once("").chain(scene.images.iter().map(String::as_str))
                    {
                        let name = if candidate.is_empty() {
                            "None"
                        } else {
                            candidate
                        };
                        if ui.selectable_label(candidate == image, name).clicked() {
                            let candidate = candidate.to_owned();
                            changes.push((
                                "toon ramp",
                                Box::new(move |file| toon_of(file).ramp = Some(candidate)),
                            ));
                        }
                    }
                });
            reset_button(ui, own.ramp.is_some(), "toon ramp", changes, |file| {
                toon_of(file).ramp = None
            });
        });
        ui.end_row();
        let mut row = |name, field, (own, below), range, set: ToonSetter| {
            number_row(
                ui,
                (name, field),
                (own, below),
                range,
                changes,
                move |file, value| set(toon_of(file), value),
            );
        };
        row(
            "Softness",
            "toon softness",
            (own.softness, below.softness),
            0.0..=1.0,
            |toon, value| toon.softness = value,
        );
        row(
            "Rim light",
            "toon rim",
            (own.rim, below.rim),
            0.0..=2.0,
            |toon, value| toon.rim = value,
        );
        row(
            "Highlight",
            "toon highlight",
            (own.highlight, below.highlight),
            0.0..=1.0,
            |toon, value| toon.highlight = value,
        );
    });
}

pub(super) fn shading_name(shading: Shading) -> &'static str {
    match shading {
        Shading::Standard => "Standard",
        Shading::Unlit => "Unlit",
        Shading::Toon => "Toon",
    }
}

/// Sets one of toon shading's numbers (`None` leaves it to the base).
pub(super) type ToonSetter = fn(&mut ToonFile, Option<f32>);

/// `file`'s toon values, made if it had none.
pub(super) fn toon_of(file: &mut MaterialFile) -> &mut ToonFile {
    file.toon.get_or_insert_with(ToonFile::default)
}

/// The Outline section: the line drawn around its parts' silhouettes, its
/// width (0 for none) and color (`docs/rendering/stylized.md`). `base` is
/// what its bases give.
pub(super) fn outline_rows(
    ui: &mut egui::Ui,
    file: &MaterialFile,
    base: &StandardMaterial,
    changes: &mut Vec<Change>,
) {
    ui.add_space(6.0);
    widgets::section_label(ui, "Outline");
    let below = base.outline.unwrap_or(StandardOutline {
        width: 0.0,
        ..StandardOutline::default()
    });
    let own = file.outline.clone().unwrap_or_default();
    // An outline block that sets no width anywhere draws the default one.
    let width = own
        .width
        .unwrap_or(if base.outline.is_none() && file.outline.is_some() {
            StandardOutline::default().width
        } else {
            below.width
        });
    // Its width is in pixels at 1080 pixels tall, the same near or far.
    responsive::form(ui, "material outline", |ui| {
        number_row(
            ui,
            ("Width", "outline width"),
            (own.width, width),
            0.0..=10.0,
            changes,
            |file, value| outline_of(file).width = value,
        );
        if width > 0.0 {
            color_row(
                ui,
                "Color",
                own.color,
                below.color,
                "outline color",
                changes,
                |file, value| outline_of(file).color = value,
            );
        }
    });
}

/// `file`'s outline values, made if it had none.
pub(super) fn outline_of(file: &mut MaterialFile) -> &mut OutlineFile {
    file.outline.get_or_insert_with(OutlineFile::default)
}

/// The Water section: whether the material is water, and its waves, color
/// and foam (`docs/rendering/water.md`). `base` is what its bases give.
pub(super) fn water_rows(
    ui: &mut egui::Ui,
    file: &MaterialFile,
    base: Option<StandardWater>,
    changes: &mut Vec<Change>,
) {
    ui.add_space(6.0);
    widgets::section_label(ui, "Water");
    let own = file.water.clone();
    responsive::form(ui, "material water", |ui| {
        let label = value_label(ui, "Water", own.is_some());
        label.on_hover_text(
            "Draws a part with this material as water: its box's top, with waves, what's \
             under seen through it, reflections and foam.",
        );
        ui.horizontal(|ui| {
            let mut on = own.is_some() || base.is_some();
            if ui.checkbox(&mut on, "").changed() {
                changes.push((
                    "water",
                    Box::new(move |file| file.water = on.then(WaterFile::default)),
                ));
            }
            reset_button(ui, own.is_some(), "water", changes, |file| {
                file.water = None
            });
        });
        ui.end_row();
        if own.is_none() && base.is_none() {
            return;
        }
        // Each value this file leaves out shows its base's (or the default).
        let below = base.unwrap_or_default();
        let own = own.unwrap_or_default();
        color_row(
            ui,
            "Deep color",
            own.deep_color,
            below.deep_color,
            "water deep color",
            changes,
            |file, value| water_of(file).deep_color = value,
        );
        let mut row = |name, field, (own, below), range, set: WaterSetter| {
            number_row(
                ui,
                (name, field),
                (own, below),
                range,
                changes,
                move |file, value| set(water_of(file), value),
            );
        };
        row(
            "Clarity",
            "water clarity",
            (own.clarity, below.clarity),
            0.1..=50.0,
            |water, value| water.clarity = value,
        );
        row(
            "Wave height",
            "water wave height",
            (own.wave_height, below.wave_height),
            0.0..=5.0,
            |water, value| water.wave_height = value,
        );
        row(
            "Wave length",
            "water wave length",
            (own.wave_length, below.wave_length),
            1.0..=200.0,
            |water, value| water.wave_length = value,
        );
        row(
            "Wave speed",
            "water wave speed",
            (own.wave_speed, below.wave_speed),
            0.0..=4.0,
            |water, value| water.wave_speed = value,
        );
        row(
            "Wave direction",
            "water wave direction",
            (own.wave_direction, below.wave_direction),
            0.0..=360.0,
            |water, value| water.wave_direction = value,
        );
        row(
            "Foam",
            "water foam",
            (own.foam, below.foam),
            0.0..=4.0,
            |water, value| water.foam = value,
        );
        row(
            "Refraction",
            "water refraction",
            (own.refraction, below.refraction),
            0.0..=2.0,
            |water, value| water.refraction = value,
        );
    });
}

/// Sets one of a water material's values (`None` leaves it to the base).
pub(super) type WaterSetter = fn(&mut WaterFile, Option<f32>);

/// `file`'s water values, made water if it wasn't.
pub(super) fn water_of(file: &mut MaterialFile) -> &mut WaterFile {
    file.water.get_or_insert_with(WaterFile::default)
}

/// One of a material's texture maps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum MapSlot {
    Color,
    Normal,
    Metalness,
    Roughness,
    Occlusion,
    Emissive,
    Opacity,
    Specular,
}

impl MapSlot {
    const ALL: [MapSlot; 8] = [
        MapSlot::Color,
        MapSlot::Normal,
        MapSlot::Metalness,
        MapSlot::Roughness,
        MapSlot::Occlusion,
        MapSlot::Emissive,
        MapSlot::Opacity,
        MapSlot::Specular,
    ];

    fn label(self) -> &'static str {
        match self {
            MapSlot::Color => "Color map",
            MapSlot::Normal => "Normal map",
            MapSlot::Metalness => "Metalness map",
            MapSlot::Roughness => "Roughness map",
            MapSlot::Occlusion => "Occlusion map",
            MapSlot::Emissive => "Glow map",
            MapSlot::Opacity => "Opacity map",
            MapSlot::Specular => "Specular map",
        }
    }

    /// The undo step's name for this map.
    fn field(self) -> &'static str {
        match self {
            MapSlot::Color => "color map",
            MapSlot::Normal => "normal map",
            MapSlot::Metalness => "metalness map",
            MapSlot::Roughness => "roughness map",
            MapSlot::Occlusion => "occlusion map",
            MapSlot::Emissive => "glow map",
            MapSlot::Opacity => "opacity map",
            MapSlot::Specular => "specular map",
        }
    }

    fn of(self, file: &MaterialFile) -> &Option<MapFile> {
        let maps = &file.maps;
        match self {
            MapSlot::Color => &maps.color,
            MapSlot::Normal => &maps.normal,
            MapSlot::Metalness => &maps.metalness,
            MapSlot::Roughness => &maps.roughness,
            MapSlot::Occlusion => &maps.occlusion,
            MapSlot::Emissive => &maps.emissive,
            MapSlot::Opacity => &maps.opacity,
            MapSlot::Specular => &maps.specular,
        }
    }

    fn of_mut(self, file: &mut MaterialFile) -> &mut Option<MapFile> {
        let maps = &mut file.maps;
        match self {
            MapSlot::Color => &mut maps.color,
            MapSlot::Normal => &mut maps.normal,
            MapSlot::Metalness => &mut maps.metalness,
            MapSlot::Roughness => &mut maps.roughness,
            MapSlot::Occlusion => &mut maps.occlusion,
            MapSlot::Emissive => &mut maps.emissive,
            MapSlot::Opacity => &mut maps.opacity,
            MapSlot::Specular => &mut maps.specular,
        }
    }

    /// The channel a map reads its value from, or `None` for a map read in
    /// color (color, normal, glow and specular maps).
    fn default_channel(self) -> Option<Channel> {
        match self {
            MapSlot::Metalness | MapSlot::Roughness | MapSlot::Occlusion => Some(Channel::Red),
            MapSlot::Opacity => Some(Channel::Alpha),
            _ => None,
        }
    }

    fn has_strength(self) -> bool {
        matches!(self, MapSlot::Normal | MapSlot::Occlusion)
    }
}

/// Sets the map in `slot` of the material to its own values with `change`
/// made, filling in a map the material didn't set (so it overrides only
/// what's changed).
pub(super) fn change_map(
    slot: MapSlot,
    change: impl FnOnce(&mut MapFile) + 'static,
) -> Box<dyn FnOnce(&mut MaterialFile)> {
    Box::new(move |file| {
        let map = slot.of_mut(file).get_or_insert_with(MapFile::default);
        change(map);
    })
}

/// A map's rows: its image, and (as the map has them) its channel,
/// inversion, strength and DirectX switch.
pub(super) fn map_rows(
    ui: &mut egui::Ui,
    scene: &MaterialScene,
    slot: MapSlot,
    file: &MaterialFile,
    inherited: &MaterialFile,
    changes: &mut Vec<Change>,
) {
    let own = slot.of(file).clone();
    let below = slot.of(inherited).clone().unwrap_or_default();
    let shown = own.as_ref().map_or(below.clone(), |own| {
        let mut merged = MaterialFile::default();
        *slot.of_mut(&mut merged) = Some(own.clone());
        let mut base = MaterialFile::default();
        *slot.of_mut(&mut base) = Some(below.clone());
        slot.of(&merged.over(&base)).clone().unwrap_or_default()
    });
    let field = slot.field();
    responsive::form(ui, ("material map", field), |ui| {
        value_label(ui, slot.label(), own.is_some());
        ui.horizontal(|ui| {
            let image = shown.path.clone().unwrap_or_default();
            let text = if image.is_empty() {
                "None".to_owned()
            } else {
                image.rsplit('/').next().unwrap_or(&image).to_owned()
            };
            egui::ComboBox::from_id_salt(("map image", field))
                .selected_text(text)
                .width((ui.available_width() - 40.0).clamp(60.0, 180.0))
                .show_ui(ui, |ui| {
                    if ui.selectable_label(image.is_empty(), "None").clicked() {
                        changes.push((
                            field,
                            change_map(slot, |map| map.path = Some(String::new())),
                        ));
                    }
                    for candidate in scene.images {
                        if ui
                            .selectable_label(*candidate == image, candidate)
                            .clicked()
                        {
                            let candidate = candidate.clone();
                            changes.push((
                                field,
                                change_map(slot, move |map| map.path = Some(candidate)),
                            ));
                        }
                    }
                });
            reset_button(ui, own.is_some(), field, changes, move |file| {
                *slot.of_mut(file) = None;
            });
        });
        ui.end_row();
        if shown.path.as_deref().is_none_or(str::is_empty) {
            return;
        }
        if let Some(default) = slot.default_channel() {
            ui.label("    Channel");
            ui.horizontal(|ui| {
                let before = shown.channel.unwrap_or(default);
                let mut channel = before;
                egui::ComboBox::from_id_salt(("map channel", field))
                    .selected_text(channel_name(channel))
                    .show_ui(ui, |ui| {
                        for choice in [Channel::Red, Channel::Green, Channel::Blue, Channel::Alpha]
                        {
                            ui.selectable_value(&mut channel, choice, channel_name(choice));
                        }
                    });
                if channel != before {
                    changes.push((
                        field,
                        change_map(slot, move |map| map.channel = Some(channel)),
                    ));
                }
                let mut invert = shown.invert.unwrap_or(false);
                if ui.checkbox(&mut invert, "Inverted").changed() {
                    changes.push((
                        field,
                        change_map(slot, move |map| map.invert = Some(invert)),
                    ));
                }
            });
            ui.end_row();
        }
        if slot.has_strength() {
            ui.label("    Strength");
            let mut strength = shown.strength.unwrap_or(1.0);
            if ui
                .add(egui::Slider::new(&mut strength, 0.0..=2.0))
                .changed()
            {
                changes.push((
                    field,
                    change_map(slot, move |map| map.strength = Some(strength)),
                ));
            }
            ui.end_row();
        }
        if slot == MapSlot::Normal {
            ui.label("    DirectX");
            let mut direct_x = shown.direct_x.unwrap_or(false);
            if ui.checkbox(&mut direct_x, "Green points down").changed() {
                changes.push((
                    field,
                    change_map(slot, move |map| map.direct_x = Some(direct_x)),
                ));
            }
            ui.end_row();
        }
    });
}

pub(super) fn channel_name(channel: Channel) -> &'static str {
    match channel {
        Channel::Red => "Red",
        Channel::Green => "Green",
        Channel::Blue => "Blue",
        Channel::Alpha => "Alpha",
    }
}
