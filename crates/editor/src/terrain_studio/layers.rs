use super::*;

/// The largest a picker's thumbnail is drawn, in points.
const PICK_THUMBNAIL: f32 = 32.0;
/// A map's thumbnail beside its file name, and a layer's in the list.
const MAP_PICTURE: f32 = 16.0;
/// From this many choices, a picker has a search box.
const SEARCH_FROM: usize = 7;

/// One layer's material: a texture set from the project, then its color,
/// maps and how they tile. `textures` is the project's texture sets and
/// textures, for the pickers; `search` is what those are searching for;
/// `pictures` holds the maps' thumbnails by path.
pub(super) fn layer_editor(
    ui: &mut egui::Ui,
    layer: InstanceId,
    instance: &axcenity_core::Instance,
    textures: &[TexturePick],
    pictures: &HashMap<String, egui::TextureHandle>,
    search: &mut String,
    actions: &mut Vec<TerrainAction>,
) {
    let sets: Vec<&TexturePick> = textures
        .iter()
        .filter(|pick| pick.entry.kind == AssetKind::TextureSet)
        .collect();
    let images: Vec<&TexturePick> = textures
        .iter()
        .filter(|pick| pick.entry.kind == AssetKind::Texture)
        .collect();
    ui.horizontal_wrapped(|ui| {
        ui.menu_button(theme::with_icon(icons::SQUARES_FOUR, "Use texture set"), |ui| {
            if let Some(entry) = texture_menu(
                ui,
                &sets,
                search,
                "No texture sets in the project yet. Get texture in the Assets panel downloads one.",
            ) {
                actions.push(TerrainAction::UseTextureSet(layer, entry));
            }
        })
        .response
        .on_hover_text(
            "Fill every map from a texture set in the project. You can also drag a set here \
             from the Assets panel.",
        );
    });
    ui.add_space(2.0);
    let number = |name| {
        instance
            .property(name)
            .and_then(Value::as_number)
            .unwrap_or(0.0)
    };
    let flag = |name| instance.property(name).and_then(Value::as_bool) == Some(true);
    responsive::form(ui, ("layer", layer), |ui| {
        field(ui, "Tint");
        if let Some(color) = instance.property("Color").and_then(Value::as_color3)
            && let Some(color) = crate::properties::color_control(ui, color)
        {
            actions.push(TerrainAction::SetLayer(
                layer,
                "Color",
                Value::Color3(color),
            ));
        }
        ui.end_row();
        for (name, label) in LAYER_MAPS {
            field(ui, label);
            let row = ui.horizontal(|ui| {
                ui.menu_button(icons::FOLDER_OPEN, |ui| {
                    if let Some(entry) =
                        texture_menu(ui, &images, search, "No textures in the project yet.")
                    {
                        actions.push(TerrainAction::SetLayer(
                            layer,
                            name,
                            Value::String(entry.path),
                        ));
                    }
                    ui.separator();
                    if ui
                        .button(theme::with_icon(icons::FOLDER_OPEN, "Browse files…"))
                        .on_hover_text(
                            "Choose a file (PNG, JPEG, TGA or BMP), copied into the project",
                        )
                        .clicked()
                    {
                        actions.push(TerrainAction::PickLayerTexture(layer, name));
                        ui.close();
                    }
                })
                .response
                .on_hover_text("Use a texture in the project, or browse for a file");
                let path = instance
                    .property(name)
                    .and_then(Value::as_str)
                    .unwrap_or("");
                if path.is_empty() {
                    ui.weak("None");
                } else {
                    if ui
                        .small_button(icons::X)
                        .on_hover_text("Use no map")
                        .clicked()
                    {
                        actions.push(TerrainAction::SetLayer(
                            layer,
                            name,
                            Value::String(String::new()),
                        ));
                    }
                    if let Some(picture) = pictures.get(path) {
                        ui.add(
                            egui::Image::new(picture)
                                .fit_to_exact_size(egui::vec2(MAP_PICTURE, MAP_PICTURE))
                                .corner_radius(2),
                        );
                    }
                    ui.add(egui::Label::new(path.rsplit('/').next().unwrap_or(path)).truncate())
                        .on_hover_text(path);
                }
            });
            if let Some(entry) = dropped_asset(ui, &row.response, AssetKind::Texture) {
                actions.push(TerrainAction::SetLayer(
                    layer,
                    name,
                    Value::String(entry.path),
                ));
            }
            ui.end_row();
        }
        let has_map = |name| {
            instance
                .property(name)
                .and_then(Value::as_str)
                .is_some_and(|path| !path.is_empty())
        };
        let mut slider = |ui: &mut egui::Ui,
                          name: &'static str,
                          label: &str,
                          range: std::ops::RangeInclusive<f64>,
                          suffix: &str| {
            field(ui, label);
            let mut value = number(name);
            let response = ui.add(egui::Slider::new(&mut value, range).suffix(suffix));
            if response.changed() {
                actions.push(TerrainAction::SetLayer(layer, name, Value::Number(value)));
            }
            ui.end_row();
            response
        };
        // With a map these scale it, so 1 is the map as it is.
        for (name, map, label) in [
            ("Roughness", "RoughnessMap", "Roughness"),
            ("Metalness", "MetalnessMap", "Metalness"),
        ] {
            if has_map(map) {
                slider(ui, name, label, 0.0..=1.0, " × map").on_hover_text(format!(
                    "Scales the {} map: 1 shows it as it is, lower is less.",
                    label.to_lowercase()
                ));
            } else {
                slider(ui, name, label, 0.0..=1.0, "");
            }
        }
        slider(ui, "NormalStrength", "Normal strength", 0.0..=3.0, "");
        slider(ui, "TileSize", "Tile size", 0.25..=256.0, " studs");
        slider(ui, "TextureRotation", "Turn", -180.0..=180.0, "°");
        slider(ui, "AntiTiling", "Anti-tiling", 0.0..=1.0, "");
        slider(ui, "HeightBlend", "Height blend", 0.0..=1.0, "");
        field(ui, "Triplanar");
        let mut triplanar = flag("Triplanar");
        if ui
            .checkbox(&mut triplanar, "Project from the sides too")
            .on_hover_text("For cliffs and rock: textures aren't stretched on steep ground.")
            .changed()
        {
            actions.push(TerrainAction::SetLayer(
                layer,
                "Triplanar",
                Value::Bool(triplanar),
            ));
        }
        ui.end_row();
    });
    ui.horizontal_wrapped(|ui| {
        if ui
            .button(theme::with_icon(icons::DOWNLOAD, "Import texture set…"))
            .on_hover_text(
                "Pick a material's textures together (BaseColor, Normal, Roughness, ORM…): \
                 each goes to its map by its name.",
            )
            .clicked()
        {
            actions.push(TerrainAction::ImportTextureSet(layer));
        }
        if ui
            .button(theme::with_icon(icons::IMAGE, "From SurfaceAppearance"))
            .on_hover_text("Copy the maps of the SurfaceAppearance selected in the Explorer.")
            .clicked()
        {
            actions.push(TerrainAction::CopySurfaceAppearance(layer));
        }
        if ui
            .button(theme::with_icon(icons::MAGIC_WAND, "Auto settings"))
            .on_hover_text(
                "Set roughness, metalness and normal strength to suit this layer's maps, as a \
                 texture set does when it fills the layer.",
            )
            .clicked()
        {
            actions.push(TerrainAction::AutoLayerSettings(layer));
        }
    });
}

/// A list of `picks` to choose from, with their thumbnails, searchable
/// when it's long. `empty` says why there's nothing. Returns the one
/// clicked, and closes the menu it's in.
pub(super) fn texture_menu(
    ui: &mut egui::Ui,
    picks: &[&TexturePick],
    search: &mut String,
    empty: &str,
) -> Option<AssetEntry> {
    let palette = theme::palette(ui.ctx());
    if picks.is_empty() {
        ui.set_max_width(260.0);
        ui.label(egui::RichText::new(empty).color(palette.text_weak));
        return None;
    }
    if picks.len() >= SEARCH_FROM {
        widgets::search_field(ui, search, "Search textures", 220.0);
    }
    let needle = search.to_lowercase();
    let mut chosen = None;
    egui::ScrollArea::vertical()
        .max_height(320.0)
        .show(ui, |ui| {
            let mut shown = 0;
            for pick in picks {
                if !needle.is_empty() && !pick.entry.path.to_lowercase().contains(&needle) {
                    continue;
                }
                shown += 1;
                let size = egui::vec2(PICK_THUMBNAIL, PICK_THUMBNAIL);
                let button = match &pick.thumbnail {
                    Some(thumbnail) => egui::Button::image_and_text(
                        egui::Image::new(thumbnail)
                            .fit_to_exact_size(size)
                            .corner_radius(3),
                        &pick.entry.name,
                    ),
                    None => egui::Button::new(theme::with_icon(icons::IMAGE, &pick.entry.name)),
                };
                if ui
                    .add(button.min_size(egui::vec2(220.0, PICK_THUMBNAIL + 4.0)))
                    .on_hover_text(&pick.entry.path)
                    .clicked()
                {
                    chosen = Some(pick.entry.clone());
                    ui.close();
                }
            }
            if shown == 0 {
                ui.label(egui::RichText::new("Nothing matches.").color(palette.text_weak));
            }
        });
    chosen
}

/// An asset of `kind` dragged from the Assets panel and let go over
/// `response`. While one is held over it, it's outlined.
fn dropped_asset(ui: &egui::Ui, response: &egui::Response, kind: AssetKind) -> Option<AssetEntry> {
    let held = response.dnd_hover_payload::<AssetPayload>()?;
    if held.0.kind != kind {
        return None;
    }
    let palette = theme::palette(ui.ctx());
    ui.painter().rect_stroke(
        response.rect.expand(2.0),
        4,
        egui::Stroke::new(1.5_f32, palette.accent),
        egui::StrokeKind::Outside,
    );
    response
        .dnd_release_payload::<AssetPayload>()
        .map(|payload| payload.0.clone())
}

/// "Covers the layers above where the height is … and the slope is …".
pub(super) fn rule_editor(
    ui: &mut egui::Ui,
    layer: InstanceId,
    instance: &axcenity_core::Instance,
    first: bool,
    actions: &mut Vec<TerrainAction>,
) {
    let palette = theme::palette(ui.ctx());
    let number = |name| {
        instance
            .property(name)
            .and_then(Value::as_number)
            .unwrap_or(0.0)
    };
    let mut enabled = instance.property("AutoEnabled").and_then(Value::as_bool) == Some(true);
    ui.horizontal_wrapped(|ui| {
        ui.label(
            egui::RichText::new(instance.name())
                .family(theme::medium_family())
                .color(palette.text_strong),
        );
        if first {
            ui.label(
                egui::RichText::new("covers everything underneath")
                    .size(theme::type_scale::SMALL)
                    .color(palette.text_weak),
            );
            return;
        }
        if ui.checkbox(&mut enabled, "automatic").changed() {
            actions.push(TerrainAction::SetLayer(
                layer,
                "AutoEnabled",
                Value::Bool(enabled),
            ));
        }
    });
    if first || !enabled {
        return;
    }
    let mut value = |ui: &mut egui::Ui,
                     name: &'static str,
                     speed: f64,
                     range: std::ops::RangeInclusive<f64>,
                     suffix: &str| {
        let mut current = number(name);
        if ui
            .add(
                egui::DragValue::new(&mut current)
                    .speed(speed)
                    .range(range)
                    .suffix(suffix),
            )
            .changed()
        {
            actions.push(TerrainAction::SetLayer(layer, name, Value::Number(current)));
        }
    };
    ui.horizontal_wrapped(|ui| {
        ui.label("where the height is");
        value(ui, "MinHeight", 0.5, -100000.0..=100000.0, "");
        ui.label("to");
        value(ui, "MaxHeight", 0.5, -100000.0..=100000.0, " studs,");
        ui.label("fading over");
        value(ui, "HeightFade", 0.25, 0.0..=10000.0, " studs");
    });
    ui.horizontal_wrapped(|ui| {
        ui.label("and the slope is");
        value(ui, "MinSlope", 0.5, 0.0..=90.0, "°");
        ui.label("to");
        value(ui, "MaxSlope", 0.5, 0.0..=90.0, "°,");
        ui.label("fading over");
        value(ui, "SlopeFade", 0.25, 0.0..=90.0, "°");
    });
    ui.horizontal_wrapped(|ui| {
        ui.label("covering");
        value(ui, "RuleStrength", 0.01, 0.0..=1.0, "");
        ui.label("with noise");
        value(ui, "RuleNoise", 0.01, 0.0..=1.0, "");
        ui.label("in patches");
        value(ui, "NoiseScale", 0.5, 0.1..=10000.0, " studs across");
    });
}

/// What the masks are set to, so a brush that does nothing explains why.
pub(super) fn masks_summary(ui: &mut egui::Ui, brush: &Brush) {
    let mut parts = Vec::new();
    if let Some((low, high)) = brush.height_limits {
        parts.push(format!("heights {low:.0}–{high:.0} studs"));
    }
    if let Some((low, high)) = brush.slope_limits {
        parts.push(format!("slopes {low:.0}–{high:.0}°"));
    }
    if !parts.is_empty() {
        let palette = theme::palette(ui.ctx());
        ui.label(
            egui::RichText::new(theme::with_icon(
                icons::FUNNEL,
                &format!("Masked: only {}.", parts.join(" and ")),
            ))
            .size(theme::type_scale::SMALL)
            .color(palette.warning),
        );
    }
}

impl TerrainStudio {
    /// The terrain's layers, and the open one's material.
    pub(super) fn layers_section(
        &mut self,
        ui: &mut egui::Ui,
        cx: &StudioContext,
        terrain: InstanceId,
        painting: bool,
        actions: &mut Vec<TerrainAction>,
    ) {
        let world = cx.world;
        let palette = theme::palette(ui.ctx());
        let layers = axcenity_terrain::materials::layers(world, terrain);
        let all_layers: Vec<InstanceId> = world
            .get(terrain)
            .map(|t| t.children().to_vec())
            .unwrap_or_default()
            .into_iter()
            .filter(|&c| world.get(c).map(|i| i.class()) == Some(ClassName::TerrainLayer))
            .collect();
        if self.paint_layer >= layers.len() {
            self.paint_layer = 0;
        }
        if self.open_layer.is_some_and(|open| !layers.contains(&open)) {
            self.open_layer = None;
        }
        section(ui, "Layers", |ui| {
            if layers.is_empty() {
                ui.label(
                    egui::RichText::new(
                        "No layers: the ground is plain grey. Add one to give it a material.",
                    )
                    .size(theme::type_scale::SMALL)
                    .color(palette.text_weak),
                );
            }
            for (index, &layer) in layers.iter().enumerate() {
                let Some(instance) = world.get(layer) else {
                    continue;
                };
                let open = self.open_layer == Some(layer);
                let chosen = painting && self.paint_layer == index;
                let row = ui.horizontal(|ui| {
                    let color = instance
                        .property("Color")
                        .and_then(Value::as_color3)
                        .unwrap_or(Color3::new(0.5, 0.5, 0.5));
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(MAP_PICTURE, MAP_PICTURE), egui::Sense::hover());
                    let tint = egui::Color32::from_rgb(
                        (color.r * 255.0) as u8,
                        (color.g * 255.0) as u8,
                        (color.b * 255.0) as u8,
                    );
                    let color_map = instance
                        .property("ColorMap")
                        .and_then(Value::as_str)
                        .filter(|path| !path.is_empty());
                    // The color map, tinted as the terrain tints it, once its
                    // thumbnail is read; the color with an image icon until then.
                    if let Some(picture) = color_map.and_then(|path| cx.pictures.get(path)) {
                        egui::Image::new(picture)
                            .tint(tint)
                            .corner_radius(4)
                            .paint_at(ui, rect);
                    } else {
                        ui.painter().rect_filled(rect, 4, tint);
                    }
                    if color_map.is_some_and(|path| !cx.pictures.contains_key(path)) {
                        ui.painter().text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            icons::IMAGE,
                            egui::FontId::proportional(10.0),
                            egui::Color32::from_white_alpha(200),
                        );
                    }
                    let name = format!("{}. {}", index + 1, instance.name());
                    let row = ui.selectable_label(chosen || open, name);
                    if row.clicked() {
                        if painting {
                            self.paint_layer = index;
                        }
                        self.open_layer = if open { None } else { Some(layer) };
                    }
                    row.context_menu(|ui| {
                        if ui.button("Select in Explorer").clicked() {
                            actions.push(TerrainAction::Select(layer));
                            ui.close();
                        }
                        if ui.button("Move up").clicked() {
                            actions.push(TerrainAction::MoveLayer(layer, true));
                            ui.close();
                        }
                        if ui.button("Move down").clicked() {
                            actions.push(TerrainAction::MoveLayer(layer, false));
                            ui.close();
                        }
                        if ui.button("Delete").clicked() {
                            actions.push(TerrainAction::RemoveLayer(layer));
                            ui.close();
                        }
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if painting {
                            let locked = &mut self.locked[index];
                            let (icon, tip) = if *locked {
                                (icons::LOCK, "Locked: painting doesn't take this layer away. Click to unlock.")
                            } else {
                                (icons::LOCK_OPEN, "Lock this layer, so painting doesn't take it away.")
                            };
                            if widgets::icon_button(ui, icon, tip, *locked).clicked() {
                                *locked = !*locked;
                            }
                        }
                        if instance.property("AutoEnabled").and_then(Value::as_bool) == Some(true) {
                            ui.label(egui::RichText::new(icons::MAGIC_WAND).color(palette.text_weak))
                                .on_hover_text("Laid down by its rule");
                        }
                    });
                });
                // A texture set dragged from the Assets panel fills the layer.
                if let Some(entry) = dropped_asset(ui, &row.response, AssetKind::TextureSet) {
                    actions.push(TerrainAction::UseTextureSet(layer, entry));
                }
                if open {
                    let search = &mut self.texture_search;
                    let card = ui.scope(|ui| {
                        card(ui, |ui| {
                            layer_editor(
                                ui,
                                layer,
                                instance,
                                cx.textures,
                                cx.pictures,
                                search,
                                actions,
                            )
                        })
                    });
                    if let Some(entry) = dropped_asset(ui, &card.response, AssetKind::TextureSet) {
                        actions.push(TerrainAction::UseTextureSet(layer, entry));
                    }
                }
            }
            if all_layers.len() > MAX_TERRAIN_LAYERS {
                ui.colored_label(
                    palette.warning,
                    format!("Only the first {MAX_TERRAIN_LAYERS} layers are drawn."),
                );
            }
            ui.horizontal(|ui| {
                ui.menu_button(theme::with_icon(icons::PLUS, "Add layer"), |ui| {
                    for preset in presets::ALL {
                        if ui.button(preset.name).clicked() {
                            actions.push(TerrainAction::AddLayer(preset));
                            ui.close();
                        }
                    }
                    ui.separator();
                    ui.label(egui::RichText::new("A set for a landscape").color(palette.text_weak));
                    for preset in Preset::ALL {
                        if preset == Preset::Custom {
                            continue;
                        }
                        if ui.button(preset.label()).clicked() {
                            actions.push(TerrainAction::AddLayerSet(preset));
                            ui.close();
                        }
                    }
                });
            });
        });
    }

    pub(super) fn view_section(&mut self, ui: &mut egui::Ui) {
        section(ui, "View", |ui| {
            widgets::segmented(
                ui,
                &mut self.view,
                &[
                    (TerrainView::Shaded, None, "Shaded"),
                    (TerrainView::Height, None, "Height"),
                    (TerrainView::Slope, None, "Slope"),
                ],
            );
            widgets::segmented(
                ui,
                &mut self.view,
                &[
                    (TerrainView::Weights, None, "Layers"),
                    (TerrainView::Grid, None, "Grid"),
                ],
            );
        });
    }

    /// Resolution, heightmaps and clearing paint, folded away.
    pub(super) fn data_section(
        &mut self,
        ui: &mut egui::Ui,
        world: &World,
        terrain: InstanceId,
        actions: &mut Vec<TerrainAction>,
    ) {
        egui::CollapsingHeader::new(theme::with_icon(icons::GEAR, "Terrain data"))
            .id_salt("terrain data")
            .default_open(false)
            .show(ui, |ui| {
                let current = world.terrain(terrain).map_or(0, |data| data.size().0);
                ui.horizontal_wrapped(|ui| {
                    field(ui, "Resolution");
                    let mut samples = current;
                    egui::ComboBox::from_id_salt("terrain resample")
                        .selected_text(format!("{samples} samples"))
                        .show_ui(ui, |ui| {
                            for option in RESOLUTIONS {
                                ui.selectable_value(&mut samples, option, format!("{option} samples"));
                            }
                        });
                    if samples != current {
                        actions.push(TerrainAction::Resample(samples));
                    }
                });
                responsive::toolbar(ui, |ui| {
                    if ui
                        .button(theme::with_icon(icons::DOWNLOAD, "Import heightmap…"))
                        .on_hover_text("A greyscale PNG (8 or 16-bit): black is the bottom of the height range, white the top.")
                        .clicked()
                    {
                        actions.push(TerrainAction::ImportHeightmap);
                    }
                    if ui
                        .button(theme::with_icon(icons::UPLOAD, "Export…"))
                        .on_hover_text("Save the heights as a 16-bit greyscale PNG.")
                        .clicked()
                    {
                        actions.push(TerrainAction::ExportHeightmap);
                    }
                });
                if ui
                    .button(theme::with_icon(icons::ERASER, "Clear painted materials"))
                    .on_hover_text("Takes away all painting, leaving the automatic rules. You can undo it.")
                    .clicked()
                {
                    actions.push(TerrainAction::ClearPaint);
                }
            });
    }
}
