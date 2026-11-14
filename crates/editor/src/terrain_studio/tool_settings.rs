use super::*;

impl TerrainStudio {
    /// Size, strength and falloff, then the rest folded away. `extra` adds
    /// rows to the main grid.
    pub(super) fn brush_settings(
        &mut self,
        ui: &mut egui::Ui,
        extra: Option<&mut dyn FnMut(&mut egui::Ui)>,
    ) {
        let brush = &mut self.brush;
        section(ui, "Brush", |ui| {
            responsive::form(ui, "brush", |ui| {
                field(ui, "Size").on_hover_text("The brush's radius. [ and ] change it.");
                ui.add(
                    egui::Slider::new(&mut brush.radius, 0.5..=256.0)
                        .logarithmic(true)
                        .suffix(" studs"),
                );
                ui.end_row();
                field(ui, "Strength");
                ui.add(egui::Slider::new(&mut brush.strength, 0.01..=1.0));
                ui.end_row();
                field(ui, "Falloff").on_hover_text("How much of the brush fades toward its rim.");
                ui.add(egui::Slider::new(&mut brush.falloff, 0.0..=1.0));
                ui.end_row();
                if let Some(extra) = extra {
                    extra(ui);
                }
            });
            egui::CollapsingHeader::new("More brush settings")
                .id_salt("brush more")
                .default_open(false)
                .show(ui, |ui| {
                    responsive::form(ui, "brush more grid", |ui| {
                        field(ui, "Fade");
                        egui::ComboBox::from_id_salt("brush curve")
                            .selected_text(brush.curve.label())
                            .show_ui(ui, |ui| {
                                for curve in Curve::ALL {
                                    ui.selectable_value(&mut brush.curve, curve, curve.label());
                                }
                            });
                        ui.end_row();
                        field(ui, "Shape");
                        widgets::segmented(
                            ui,
                            &mut brush.shape,
                            &[
                                (Shape::Circle, Some(icons::CIRCLE), "Circle"),
                                (Shape::Square, Some(icons::SQUARE), "Square"),
                            ],
                        );
                        ui.end_row();
                        field(ui, "Spacing").on_hover_text(
                            "How far apart stamps are along a stroke, as a part of the size.",
                        );
                        ui.add(egui::Slider::new(&mut brush.spacing, 0.05..=1.0));
                        ui.end_row();
                        field(ui, "Symmetry")
                            .on_hover_text("Mirror every stamp across the terrain's middle.");
                        egui::ComboBox::from_id_salt("brush symmetry")
                            .selected_text(brush.symmetry.label())
                            .show_ui(ui, |ui| {
                                for symmetry in Symmetry::ALL {
                                    ui.selectable_value(
                                        &mut brush.symmetry,
                                        symmetry,
                                        symmetry.label(),
                                    );
                                }
                            });
                        ui.end_row();
                    });
                    masks_summary(ui, brush);
                });
        });
    }

    pub(super) fn flatten_settings(&mut self, ui: &mut egui::Ui) {
        let mut mode = self.flatten_mode;
        let mut from_stroke = self.flatten_from_stroke;
        let mut height = self.flatten_height;
        self.brush_settings(
            ui,
            Some(&mut |ui: &mut egui::Ui| {
                field(ui, "Direction");
                egui::ComboBox::from_id_salt("flatten mode")
                    .selected_text(mode.label())
                    .show_ui(ui, |ui| {
                        for option in FlattenMode::ALL {
                            ui.selectable_value(&mut mode, option, option.label());
                        }
                    });
                ui.end_row();
                field(ui, "Height");
                ui.vertical(|ui| {
                    ui.checkbox(&mut from_stroke, "Where the stroke starts");
                    ui.add_enabled(
                        !from_stroke,
                        egui::DragValue::new(&mut height).speed(0.25).suffix(" Y"),
                    )
                    .on_hover_text(
                        "The world height to level to. Ctrl+click the ground to pick it.",
                    );
                });
                ui.end_row();
            }),
        );
        self.flatten_mode = mode;
        self.flatten_from_stroke = from_stroke;
        self.flatten_height = height;
    }

    pub(super) fn sculpt_settings(&mut self, ui: &mut egui::Ui) {
        let mut mode = self.sculpt;
        let mut step = self.terrace_step;
        self.brush_settings(
            ui,
            Some(&mut |ui: &mut egui::Ui| {
                field(ui, "Shape");
                egui::ComboBox::from_id_salt("sculpt mode")
                    .selected_text(mode.label())
                    .show_ui(ui, |ui| {
                        for option in SculptMode::ALL {
                            ui.selectable_value(&mut mode, option, option.label());
                        }
                    });
                ui.end_row();
                if mode == SculptMode::Terrace {
                    field(ui, "Step");
                    ui.add(studs(&mut step, 0.5..=256.0));
                    ui.end_row();
                }
            }),
        );
        self.sculpt = mode;
        self.terrace_step = step;
    }

    pub(super) fn stamp_settings(&mut self, ui: &mut egui::Ui, actions: &mut Vec<TerrainAction>) {
        let mut stamp = self.stamp.clone();
        let mut image = self.stamp_image.clone();
        let mut pick = false;
        self.brush_settings(
            ui,
            Some(&mut |ui: &mut egui::Ui| {
                field(ui, "Shape");
                ui.vertical(|ui| {
                    egui::ComboBox::from_id_salt("stamp shape")
                        .selected_text(if image.is_some() {
                            "Image"
                        } else {
                            stamp.shape.label()
                        })
                        .show_ui(ui, |ui| {
                            for shape in StampShape::ALL {
                                if ui
                                    .selectable_label(
                                        image.is_none() && stamp.shape == shape,
                                        shape.label(),
                                    )
                                    .clicked()
                                {
                                    stamp.shape = shape;
                                    image = None;
                                }
                            }
                        });
                    ui.horizontal(|ui| {
                        if ui
                            .small_button(theme::with_icon(icons::IMAGE, "Heightmap…"))
                            .on_hover_text("Stamp a greyscale PNG: white is high.")
                            .clicked()
                        {
                            pick = true;
                        }
                        if let Some(path) = &image {
                            ui.weak(path.rsplit('/').next().unwrap_or(path));
                        }
                    });
                });
                ui.end_row();
                field(ui, "Height");
                ui.add(studs(&mut stamp.height, -512.0..=512.0));
                ui.end_row();
                field(ui, "Turn");
                ui.add(egui::Slider::new(&mut stamp.rotation, -180.0..=180.0).suffix("°"));
                ui.end_row();
                field(ui, "Combine");
                egui::ComboBox::from_id_salt("stamp mode")
                    .selected_text(stamp.mode.label())
                    .show_ui(ui, |ui| {
                        for mode in StampMode::ALL {
                            ui.selectable_value(&mut stamp.mode, mode, mode.label());
                        }
                    });
                ui.end_row();
            }),
        );
        if image.is_none() {
            stamp.image = None;
        }
        self.stamp = stamp;
        self.stamp_image = image;
        if pick {
            actions.push(TerrainAction::PickStampImage);
        }
    }

    pub(super) fn noise_settings(&mut self, ui: &mut egui::Ui) {
        let (mut amplitude, mut scale, mut seed) =
            (self.noise_amplitude, self.noise_scale, self.noise_seed);
        self.brush_settings(
            ui,
            Some(&mut |ui: &mut egui::Ui| {
                field(ui, "Bumps");
                ui.add(studs(&mut amplitude, 0.1..=128.0));
                ui.end_row();
                field(ui, "Bump size");
                ui.add(studs(&mut scale, 0.5..=512.0));
                ui.end_row();
                field(ui, "Seed");
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut seed));
                    if ui
                        .small_button(icons::DICE_FIVE)
                        .on_hover_text("Another pattern")
                        .clicked()
                    {
                        seed = random_seed();
                    }
                });
                ui.end_row();
            }),
        );
        self.noise_amplitude = amplitude;
        self.noise_scale = scale;
        self.noise_seed = seed;
    }

    pub(super) fn paint_settings(&mut self, ui: &mut egui::Ui) {
        let mut mode = self.paint_mode;
        self.brush_settings(
            ui,
            Some(&mut |ui: &mut egui::Ui| {
                field(ui, "Mode");
                widgets::segmented(
                    ui,
                    &mut mode,
                    &[
                        (PaintMode::Blend, None, "Blend"),
                        (PaintMode::Replace, None, "Replace"),
                    ],
                );
                ui.end_row();
            }),
        );
        self.paint_mode = mode;
    }

    pub(super) fn foliage_settings(&mut self, ui: &mut egui::Ui) {
        let source = self.foliage_source.clone();
        let mut density = self.foliage_density;
        let mut collide = self.foliage_collide;
        self.brush_settings(
            ui,
            Some(&mut |ui: &mut egui::Ui| {
                field(ui, "Item").on_hover_text(
                    "Click a prop or a model in the Assets panel to paint it on the terrain.",
                );
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(format!("{} {}", foliage_icon(&source), source.name()));
                    });
                    ui.label(
                        egui::RichText::new("Click a prop or model in Assets.")
                            .size(theme::type_scale::SMALL)
                            .color(theme::palette(ui.ctx()).text_weak),
                    );
                });
                ui.end_row();
                field(ui, "Collide").on_hover_text(
                    "Parts rest on this foliage. Off, they fall through it onto the terrain.",
                );
                ui.checkbox(&mut collide, "Solid");
                ui.end_row();
                field(ui, "Density").on_hover_text(
                    "How many to scatter per square stud. Shift+drag removes this item.",
                );
                ui.add(
                    egui::Slider::new(&mut density, 0.01..=1.0)
                        .logarithmic(true)
                        .suffix(" / stud²"),
                );
                ui.end_row();
            }),
        );
        self.foliage_density = density;
        self.foliage_collide = collide;
    }

    pub(super) fn mask_settings(&mut self, ui: &mut egui::Ui) {
        let brush = &mut self.brush;
        section(ui, "Masks", |ui| {
            ui.label(
                egui::RichText::new("Every brush only changes ground that passes these.")
                    .size(theme::type_scale::SMALL)
                    .color(theme::palette(ui.ctx()).text_weak),
            );
            let mut heights = brush.height_limits.is_some();
            let (mut low, mut high) = brush.height_limits.unwrap_or((0.0, 64.0));
            ui.checkbox(&mut heights, "Only between heights")
                .on_hover_text("Studs above the terrain's bottom. Raising stops at the top.");
            if heights {
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut low).speed(0.5).suffix(" studs"));
                    ui.label("to");
                    ui.add(egui::DragValue::new(&mut high).speed(0.5).suffix(" studs"));
                });
            }
            brush.height_limits = heights.then_some((low.min(high), high.max(low)));
            let mut slopes = brush.slope_limits.is_some();
            let (mut low, mut high) = brush.slope_limits.unwrap_or((0.0, 30.0));
            ui.checkbox(&mut slopes, "Only between slopes")
                .on_hover_text("0° is flat, 90° a wall. Show the Slope view to see them.");
            if slopes {
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut low).range(0.0..=90.0).suffix("°"));
                    ui.label("to");
                    ui.add(
                        egui::DragValue::new(&mut high)
                            .range(0.0..=90.0)
                            .suffix("°"),
                    );
                });
            }
            brush.slope_limits = slopes.then_some((low.min(high), high.max(low)));
        });
        masks_summary(ui, brush);
    }

    pub(super) fn auto_settings(
        &mut self,
        ui: &mut egui::Ui,
        world: &World,
        terrain: InstanceId,
        actions: &mut Vec<TerrainAction>,
    ) {
        let palette = theme::palette(ui.ctx());
        section(ui, "Rules", |ui| {
            ui.label(
                egui::RichText::new(
                    "The first layer covers everything. Each later layer whose rule holds \
                     covers the layers above it in this list. Painting still wins, and \
                     erasing paint shows the rules again.",
                )
                .size(theme::type_scale::SMALL)
                .color(palette.text_weak),
            );
            let layers = axcenity_terrain::materials::layers(world, terrain);
            if layers.is_empty() {
                ui.weak("Add a layer below first.");
            }
            for (index, &layer) in layers.iter().enumerate() {
                let Some(instance) = world.get(layer) else {
                    continue;
                };
                ui.add_space(4.0);
                card(ui, |ui| {
                    rule_editor(ui, layer, instance, index == 0, actions);
                });
            }
        });
    }

    pub(super) fn generate_settings(
        &mut self,
        ui: &mut egui::Ui,
        cx: &StudioContext,
        terrain: InstanceId,
        actions: &mut Vec<TerrainAction>,
    ) {
        self.world_presets_section(ui, cx, actions);
        let palette = theme::palette(ui.ctx());
        let before = self.generator.clone();
        let settings = &mut self.generator;
        section(ui, "Landscape", |ui| {
            responsive::form(ui, "generate main", |ui| {
                field(ui, "Preset");
                let mut preset = settings.preset;
                egui::ComboBox::from_id_salt("generator preset")
                    .selected_text(preset.label())
                    .show_ui(ui, |ui| {
                        for option in Preset::ALL {
                            ui.selectable_value(&mut preset, option, option.label())
                                .on_hover_text(option.description());
                        }
                    });
                if preset != settings.preset {
                    let seed = settings.seed;
                    *settings = GeneratorSettings {
                        seed,
                        ..preset.settings()
                    };
                }
                ui.end_row();
                field(ui, "Seed");
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut settings.seed));
                    if ui
                        .small_button(icons::DICE_FIVE)
                        .on_hover_text("Randomize the seed: another landscape like it")
                        .clicked()
                    {
                        settings.seed = random_seed();
                    }
                });
                ui.end_row();
                field(ui, "Height");
                ui.add(egui::Slider::new(&mut settings.amplitude, 0.0..=1.0))
                    .on_hover_text(
                        "How much the land rises and falls, as part of the height range.",
                    );
                ui.end_row();
                field(ui, "Base elevation");
                ui.add(egui::Slider::new(&mut settings.base_height, 0.0..=1.0));
                ui.end_row();
                field(ui, "Feature size");
                ui.add(
                    egui::Slider::new(&mut settings.feature_size, 8.0..=4096.0)
                        .logarithmic(true)
                        .suffix(" studs"),
                );
                ui.end_row();
                field(ui, "Mountains");
                ui.add(egui::Slider::new(&mut settings.mountains, 0.0..=1.0));
                ui.end_row();
                field(ui, "Valleys");
                ui.add(egui::Slider::new(&mut settings.valleys, 0.0..=1.0));
                ui.end_row();
                field(ui, "Roughness");
                ui.add(egui::Slider::new(&mut settings.roughness, 0.0..=1.0));
                ui.end_row();
                field(ui, "Island");
                ui.add(egui::Slider::new(&mut settings.island, 0.0..=1.0));
                ui.end_row();
                field(ui, "Plains");
                ui.add(egui::Slider::new(&mut settings.plains, 0.0..=1.0))
                    .on_hover_text("Flattens the lowlands.");
                ui.end_row();
            });
            egui::CollapsingHeader::new("Advanced")
                .id_salt("generate advanced")
                .default_open(false)
                .show(ui, |ui| {
                    responsive::form(ui, "generate advanced grid", |ui| {
                        field(ui, "Octaves");
                        ui.add(egui::Slider::new(&mut settings.octaves, 1..=10));
                        ui.end_row();
                        field(ui, "Persistence");
                        ui.add(egui::Slider::new(&mut settings.persistence, 0.1..=0.9));
                        ui.end_row();
                        field(ui, "Lacunarity");
                        ui.add(egui::Slider::new(&mut settings.lacunarity, 1.5..=3.5));
                        ui.end_row();
                        field(ui, "Warp");
                        ui.add(egui::Slider::new(&mut settings.warp, 0.0..=1.0))
                            .on_hover_text("Bends shapes so they swirl and branch.");
                        ui.end_row();
                        field(ui, "Height curve");
                        ui.add(egui::Slider::new(&mut settings.curve, 0.3..=3.0))
                            .on_hover_text("Above 1: broad valleys and steep peaks.");
                        ui.end_row();
                        field(ui, "Terraces");
                        ui.add(egui::Slider::new(&mut settings.terraces, 0..=24));
                        ui.end_row();
                        field(ui, "Terrace sharpness");
                        ui.add(egui::Slider::new(&mut settings.terrace_strength, 0.0..=1.0));
                        ui.end_row();
                        field(ui, "Dunes");
                        ui.add(egui::Slider::new(&mut settings.dunes, 0.0..=1.0));
                        ui.end_row();
                        field(ui, "Dune direction");
                        ui.add(
                            egui::Slider::new(&mut settings.dune_direction, 0.0..=180.0)
                                .suffix("°"),
                        );
                        ui.end_row();
                        field(ui, "Crater");
                        ui.add(egui::Slider::new(&mut settings.crater, 0.0..=1.0));
                        ui.end_row();
                        field(ui, "Lowest");
                        ui.add(egui::Slider::new(&mut settings.min_height, 0.0..=1.0));
                        ui.end_row();
                        field(ui, "Highest");
                        ui.add(egui::Slider::new(&mut settings.max_height, 0.0..=1.0));
                        ui.end_row();
                    });
                });
        });
        // Changing anything but the seed makes the settings your own.
        let only_seed = GeneratorSettings {
            seed: self.generator.seed,
            ..before.clone()
        };
        if self.generator.preset == before.preset && self.generator != only_seed {
            self.generator.preset = Preset::Custom;
        }
        if self.generator != before {
            self.changed_at = Some(cx.time);
        }
        section(ui, "Size", |ui| {
            ui.label(
                egui::RichText::new("The terrain gets this size when the landscape is applied.")
                    .size(theme::type_scale::SMALL)
                    .color(palette.text_weak),
            );
            let size_before = self.new_terrain;
            self.size_controls(ui);
            if self.new_terrain != size_before {
                self.changed_at = Some(cx.time);
            }
            ui.horizontal(|ui| {
                if ui
                    .small_button("Use the terrain's size")
                    .on_hover_text("Keep the terrain's current size and resolution.")
                    .clicked()
                    && let (Some(instance), Some(data)) =
                        (cx.world.get(terrain), cx.world.terrain(terrain))
                    && let Some(size) = instance.property("Size").and_then(Value::as_vector3)
                {
                    self.new_terrain = NewTerrain {
                        width: size.x,
                        length: size.z,
                        height: size.y,
                        samples: data.size().0,
                    };
                    self.changed_at = Some(cx.time);
                }
            });
        });
        section(ui, "Apply", |ui| {
            // The choices go under "Result" when narrow.
            responsive::form(ui, "apply result", |ui| {
                field(ui, "Result");
                if widgets::segmented(
                    ui,
                    &mut self.combine,
                    &[
                        (Combine::Replace, None, "Replaces the ground"),
                        (Combine::Add, None, "Adds to it"),
                    ],
                ) {
                    self.changed_at = Some(cx.time);
                }
                ui.end_row();
            });
            ui.checkbox(
                &mut self.setup_materials,
                "Add matching layers if the terrain has none",
            )
            .on_hover_text(
                "Grass, rock, snow and so on, with rules that lay them down by themselves.",
            );
            ui.checkbox(
                &mut self.live_preview,
                "Update the preview as settings change",
            );
            responsive::toolbar(ui, |ui| {
                let label = if self.preview.is_some() {
                    "Regenerate"
                } else {
                    "Preview"
                };
                if ui
                    .add(widgets::button(
                        Role::Primary,
                        Some(icons::SPARKLE),
                        label,
                        &palette,
                    ))
                    .clicked()
                {
                    actions.push(TerrainAction::StartGenerate);
                }
                if ui
                    .add(widgets::button(
                        Role::Ghost,
                        Some(icons::ARROW_COUNTER_CLOCKWISE),
                        "Reset",
                        &palette,
                    ))
                    .on_hover_text("Back to the preset's own settings")
                    .clicked()
                {
                    let preset = match self.generator.preset {
                        Preset::Custom => Preset::RollingHills,
                        preset => preset,
                    };
                    self.generator = GeneratorSettings {
                        seed: self.generator.seed,
                        ..preset.settings()
                    };
                    self.changed_at = Some(cx.time);
                }
            });
            // The settings this terrain was last made with, to make it again.
            let saved = cx
                .world
                .terrain(terrain)
                .and_then(|data| data.generation.clone())
                .and_then(|text| GeneratorSettings::from_json(&text).ok());
            if let Some(saved) = saved {
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        egui::RichText::new(format!(
                            "Made with {} v{}, seed {}.",
                            saved.algorithm, saved.version, saved.seed
                        ))
                        .size(theme::type_scale::SMALL)
                        .color(palette.text_weak),
                    );
                    if ui
                        .small_button("Load its settings")
                        .on_hover_text("Make the same landscape again, or change it a little.")
                        .clicked()
                    {
                        self.generator = saved;
                        self.changed_at = Some(cx.time);
                    }
                });
            }
        });
    }

    pub(super) fn erosion_settings(&mut self, ui: &mut egui::Ui, actions: &mut Vec<TerrainAction>) {
        let palette = theme::palette(ui.ctx());
        section(ui, "Erosion", |ui| {
            widgets::segmented(
                ui,
                &mut self.erosion,
                &[
                    (ErosionKind::Hydraulic, Some(icons::DROP), "Rain"),
                    (ErosionKind::Thermal, Some(icons::MOUNTAINS), "Scree"),
                ],
            );
            ui.label(
                egui::RichText::new(match self.erosion {
                    ErosionKind::Hydraulic => {
                        "Raindrops run downhill, carving gullies and filling valley floors."
                    }
                    ErosionKind::Thermal => {
                        "Loose ground slides off slopes steeper than the talus angle."
                    }
                })
                .size(theme::type_scale::SMALL)
                .color(palette.text_weak),
            );
            responsive::form(ui, "erosion", |ui| match self.erosion {
                ErosionKind::Hydraulic => {
                    let settings = &mut self.hydraulic;
                    field(ui, "Quality");
                    widgets::segmented(
                        ui,
                        &mut self.quality,
                        &[
                            (Quality::Draft, None, "Draft"),
                            (Quality::Normal, None, "Normal"),
                            (Quality::High, None, "High"),
                        ],
                    );
                    ui.end_row();
                    field(ui, "Erosion");
                    ui.add(egui::Slider::new(&mut settings.erosion, 0.0..=1.0));
                    ui.end_row();
                    field(ui, "Deposition");
                    ui.add(egui::Slider::new(&mut settings.deposition, 0.0..=1.0));
                    ui.end_row();
                    field(ui, "Evaporation");
                    ui.add(egui::Slider::new(&mut settings.evaporation, 0.0..=0.2));
                    ui.end_row();
                    field(ui, "Capacity").on_hover_text("How much soil water carries.");
                    ui.add(egui::Slider::new(&mut settings.capacity, 0.5..=16.0));
                    ui.end_row();
                    field(ui, "Inertia");
                    ui.add(egui::Slider::new(&mut settings.inertia, 0.0..=0.5));
                    ui.end_row();
                    field(ui, "Radius");
                    ui.add(egui::Slider::new(&mut settings.radius, 1..=6).suffix(" samples"));
                    ui.end_row();
                    field(ui, "Drop lifetime");
                    ui.add(egui::Slider::new(&mut settings.lifetime, 8..=96).suffix(" steps"));
                    ui.end_row();
                    field(ui, "Seed");
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut settings.seed));
                        if ui.small_button(icons::DICE_FIVE).clicked() {
                            settings.seed = random_seed();
                        }
                    });
                    ui.end_row();
                }
                ErosionKind::Thermal => {
                    let settings = &mut self.thermal;
                    field(ui, "Talus angle");
                    ui.add(egui::Slider::new(&mut settings.talus_angle, 5.0..=80.0).suffix("°"));
                    ui.end_row();
                    field(ui, "Strength");
                    ui.add(egui::Slider::new(&mut settings.strength, 0.05..=1.0));
                    ui.end_row();
                    field(ui, "Iterations");
                    ui.add(egui::Slider::new(&mut settings.iterations, 1..=400));
                    ui.end_row();
                }
            });
            if ui
                .add(widgets::button(
                    Role::Primary,
                    Some(icons::CLOUD_RAIN),
                    "Preview erosion",
                    &palette,
                ))
                .on_hover_text("Runs on a copy: the terrain only changes when you apply it.")
                .clicked()
            {
                actions.push(TerrainAction::StartErode);
            }
        });
    }
}
