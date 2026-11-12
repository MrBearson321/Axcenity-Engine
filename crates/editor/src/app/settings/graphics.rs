//! The Graphics page: this computer's preset, its overrides, and what's in
//! use once the GPU's limits are applied.

use super::*;

/// This computer's graphics settings: a preset, its overrides, and what's
/// in use once the GPU's limits are applied, with why anything was lowered.
pub(super) fn graphics_page(
    ui: &mut egui::Ui,
    settings: &mut GraphicsSettings,
    capabilities: &Capabilities,
) {
    page_title(
        ui,
        "Graphics",
        "This computer's quality, for the viewport and Play. Not saved in projects.",
    );
    let palette = theme::palette(ui.ctx());
    section(ui, "Quality");
    widgets::segmented(
        ui,
        &mut settings.quality,
        &Quality::ALL.map(|quality| (quality, None, quality.label())),
    );
    let preset = GraphicsSettings::preset(settings.quality);
    ui.add_space(space::L);
    section(ui, "Overrides");
    egui::Grid::new("graphics overrides")
        .num_columns(2)
        .spacing([12.0, 8.0])
        .show(ui, |ui| {
            found_label(ui, "Shadow quality");
            let label = |quality: Option<ShadowQuality>| match quality {
                Some(quality) => quality.label().to_owned(),
                None => format!("Preset ({})", preset.shadow_quality().label()),
            };
            egui::ComboBox::from_id_salt("shadow quality")
                .selected_text(label(settings.shadow_quality))
                .show_ui(ui, |ui| {
                    for choice in std::iter::once(None).chain(ShadowQuality::ALL.map(Some)) {
                        ui.selectable_value(&mut settings.shadow_quality, choice, label(choice));
                    }
                });
            ui.end_row();

            found_label(ui, "Shadow distance");
            ui.horizontal(|ui| {
                let mut limited = settings.shadow_distance.is_some();
                if ui
                    .checkbox(&mut limited, "")
                    .on_hover_text("Off, the preset's distance is used.")
                    .changed()
                {
                    settings.shadow_distance = limited.then(|| preset.shadow_distance());
                }
                match &mut settings.shadow_distance {
                    Some(distance) => {
                        ui.add(
                            egui::DragValue::new(distance)
                                .range(0.0..=5000.0)
                                .speed(1.0)
                                .suffix(" studs"),
                        );
                    }
                    None => {
                        ui.label(
                            egui::RichText::new(format!(
                                "Preset ({} studs)",
                                preset.shadow_distance()
                            ))
                            .color(palette.text_secondary),
                        );
                    }
                }
            });
            ui.end_row();

            found_label(ui, "Post-processing");
            let label = |on: Option<bool>| match on {
                Some(true) => "On".to_owned(),
                Some(false) => "Off".to_owned(),
                None => format!(
                    "Preset ({})",
                    if preset.post_processing() {
                        "On"
                    } else {
                        "Off"
                    }
                ),
            };
            egui::ComboBox::from_id_salt("post-processing")
                .selected_text(label(settings.post_processing))
                .show_ui(ui, |ui| {
                    for choice in [None, Some(true), Some(false)] {
                        ui.selectable_value(&mut settings.post_processing, choice, label(choice));
                    }
                });
            ui.end_row();

            found_label(ui, "Render scale");
            ui.horizontal(|ui| {
                let mut custom = settings.render_scale.is_some();
                if ui
                    .checkbox(&mut custom, "")
                    .on_hover_text("Off, the preset's scale is used.")
                    .changed()
                {
                    settings.render_scale = custom.then(|| preset.render_scale());
                }
                match &mut settings.render_scale {
                    Some(scale) => {
                        let mut percent = (*scale * 100.0).round();
                        let range = RENDER_SCALE_RANGE;
                        if ui
                            .add(
                                egui::DragValue::new(&mut percent)
                                    .range(range.start() * 100.0..=range.end() * 100.0)
                                    .speed(1.0)
                                    .suffix("%"),
                            )
                            .changed()
                        {
                            *scale = percent / 100.0;
                        }
                    }
                    None => {
                        ui.label(
                            egui::RichText::new(format!(
                                "Preset ({}%)",
                                (preset.render_scale() * 100.0).round()
                            ))
                            .color(palette.text_secondary),
                        );
                    }
                }
            });
            ui.end_row();

            found_label(ui, "Texture quality");
            let label = |quality: Option<TextureQuality>| match quality {
                Some(quality) => quality.label().to_owned(),
                None => format!("Preset ({})", preset.texture_quality().label()),
            };
            egui::ComboBox::from_id_salt("texture quality")
                .selected_text(label(settings.texture_quality))
                .show_ui(ui, |ui| {
                    for choice in std::iter::once(None).chain(TextureQuality::ALL.map(Some)) {
                        ui.selectable_value(&mut settings.texture_quality, choice, label(choice));
                    }
                });
            ui.end_row();

            found_label(ui, "Texture memory");
            ui.horizontal(|ui| {
                let mut custom = settings.texture_budget.is_some();
                if ui
                    .checkbox(&mut custom, "")
                    .on_hover_text("Off, the preset's budget is used.")
                    .changed()
                {
                    settings.texture_budget = custom.then(|| preset.texture_budget());
                }
                match &mut settings.texture_budget {
                    Some(budget) => {
                        ui.add(
                            egui::DragValue::new(budget)
                                .range(TEXTURE_BUDGET_RANGE)
                                .speed(4.0)
                                .suffix(" MB"),
                        );
                    }
                    None => {
                        ui.label(
                            egui::RichText::new(format!("Preset ({} MB)", preset.texture_budget()))
                                .color(palette.text_secondary),
                        );
                    }
                }
            });
            ui.end_row();

            found_label(ui, "Anti-aliasing");
            let label = |mode: Option<AntiAliasing>| match mode {
                Some(mode) => mode.label().to_owned(),
                None => format!("Preset ({})", preset.anti_aliasing().label()),
            };
            egui::ComboBox::from_id_salt("anti-aliasing")
                .selected_text(label(settings.anti_aliasing))
                .show_ui(ui, |ui| {
                    for choice in std::iter::once(None).chain(AntiAliasing::ALL.map(Some)) {
                        ui.selectable_value(&mut settings.anti_aliasing, choice, label(choice));
                    }
                });
            ui.end_row();

            found_label(ui, "Ambient occlusion");
            let label = |mode: Option<AmbientOcclusion>| match mode {
                Some(mode) => mode.label().to_owned(),
                None => format!("Preset ({})", preset.ambient_occlusion().label()),
            };
            egui::ComboBox::from_id_salt("ambient occlusion")
                .selected_text(label(settings.ambient_occlusion))
                .show_ui(ui, |ui| {
                    for choice in std::iter::once(None).chain(AmbientOcclusion::ALL.map(Some)) {
                        ui.selectable_value(&mut settings.ambient_occlusion, choice, label(choice));
                    }
                });
            ui.end_row();

            found_label(ui, "Reflection quality");
            let label = |quality: Option<ReflectionQuality>| match quality {
                Some(quality) => quality.label().to_owned(),
                None => format!("Preset ({})", preset.reflection_quality().label()),
            };
            egui::ComboBox::from_id_salt("reflection quality")
                .selected_text(label(settings.reflection_quality))
                .show_ui(ui, |ui| {
                    for choice in std::iter::once(None).chain(ReflectionQuality::ALL.map(Some)) {
                        ui.selectable_value(
                            &mut settings.reflection_quality,
                            choice,
                            label(choice),
                        );
                    }
                });
            ui.end_row();

            found_label(ui, "Cloud quality");
            let label = |quality: Option<CloudQuality>| match quality {
                Some(quality) => quality.label().to_owned(),
                None => format!("Preset ({})", preset.cloud_quality().label()),
            };
            egui::ComboBox::from_id_salt("cloud quality")
                .selected_text(label(settings.cloud_quality))
                .show_ui(ui, |ui| {
                    for choice in std::iter::once(None).chain(CloudQuality::ALL.map(Some)) {
                        ui.selectable_value(&mut settings.cloud_quality, choice, label(choice));
                    }
                });
            ui.end_row();

            found_label(ui, "Contact shadows");
            let on_off = |on: bool| if on { "On" } else { "Off" };
            let label = |on: Option<bool>| match on {
                Some(on) => on_off(on).to_owned(),
                None => format!("Preset ({})", on_off(preset.contact_shadows())),
            };
            egui::ComboBox::from_id_salt("contact shadows")
                .selected_text(label(settings.contact_shadows))
                .show_ui(ui, |ui| {
                    for choice in [None, Some(true), Some(false)] {
                        ui.selectable_value(&mut settings.contact_shadows, choice, label(choice));
                    }
                });
            ui.end_row();

            found_label(ui, "Real-time GI");
            let label = |on: Option<bool>| match on {
                Some(on) => on_off(on).to_owned(),
                None => format!("Preset ({})", on_off(preset.realtime_gi())),
            };
            egui::ComboBox::from_id_salt("realtime gi")
                .selected_text(label(settings.realtime_gi))
                .show_ui(ui, |ui| {
                    for choice in [None, Some(true), Some(false)] {
                        ui.selectable_value(&mut settings.realtime_gi, choice, label(choice));
                    }
                });
            ui.end_row();

            found_label(ui, "Volumetric fog");
            let label = |on: Option<bool>| match on {
                Some(on) => on_off(on).to_owned(),
                None => format!("Preset ({})", on_off(preset.volumetric_fog())),
            };
            egui::ComboBox::from_id_salt("volumetric fog")
                .selected_text(label(settings.volumetric_fog))
                .show_ui(ui, |ui| {
                    for choice in [None, Some(true), Some(false)] {
                        ui.selectable_value(&mut settings.volumetric_fog, choice, label(choice));
                    }
                });
            ui.end_row();

            found_label(ui, "Depth of field");
            let label = |quality: Option<DepthOfFieldQuality>| match quality {
                Some(quality) => quality.label().to_owned(),
                None => format!("Preset ({})", preset.depth_of_field().label()),
            };
            egui::ComboBox::from_id_salt("depth of field")
                .selected_text(label(settings.depth_of_field))
                .show_ui(ui, |ui| {
                    for choice in std::iter::once(None).chain(DepthOfFieldQuality::ALL.map(Some)) {
                        ui.selectable_value(&mut settings.depth_of_field, choice, label(choice));
                    }
                });
            ui.end_row();

            found_label(ui, "Motion blur");
            let label = |quality: Option<MotionBlurQuality>| match quality {
                Some(quality) => quality.label().to_owned(),
                None => format!("Preset ({})", preset.motion_blur().label()),
            };
            egui::ComboBox::from_id_salt("motion blur")
                .selected_text(label(settings.motion_blur))
                .show_ui(ui, |ui| {
                    for choice in std::iter::once(None).chain(MotionBlurQuality::ALL.map(Some)) {
                        ui.selectable_value(&mut settings.motion_blur, choice, label(choice));
                    }
                });
            ui.end_row();

            let switches = [
                ("Vignette", preset.vignette(), &mut settings.vignette),
                ("Film grain", preset.film_grain(), &mut settings.film_grain),
                (
                    "Chromatic aberration",
                    preset.chromatic_aberration(),
                    &mut settings.chromatic_aberration,
                ),
                ("Sharpen", preset.sharpen(), &mut settings.sharpen),
            ];
            for (name, preset_on, value) in switches {
                found_label(ui, name);
                let label = |on: Option<bool>| match on {
                    Some(on) => on_off(on).to_owned(),
                    None => format!("Preset ({})", on_off(preset_on)),
                };
                egui::ComboBox::from_id_salt(name)
                    .selected_text(label(*value))
                    .show_ui(ui, |ui| {
                        for choice in [None, Some(true), Some(false)] {
                            ui.selectable_value(value, choice, label(choice));
                        }
                    });
                ui.end_row();
            }
        });
    ui.add_space(space::L);
    section(ui, "In use");
    let effective = settings.effective(capabilities);
    let shadows = match effective.shadow_quality.map() {
        Some(map) => format!(
            "Shadows: {}, a {}² map with {} cascades, up to {} studs",
            effective.shadow_quality.label(),
            map.size,
            map.cascades,
            effective.shadow_distance
        ),
        None => "Shadows: off".to_owned(),
    };
    ui.label(shadows);
    ui.label(if effective.post_processing {
        "Post-processing: on (bloom and tone mapping)"
    } else {
        "Post-processing: off"
    });
    ui.label(format!(
        "Scene drawn at {}% of the view's size; the UI always at 100%",
        (effective.render_scale * 100.0).round()
    ));
    ui.label(format!(
        "Textures: {} size, within {} MB (the least recently drawn lose detail first)",
        effective.texture_quality.label().to_lowercase(),
        effective.texture_budget >> 20
    ));
    ui.label(format!(
        "Anti-aliasing: {}",
        match effective.anti_aliasing {
            AntiAliasing::Off => "off".to_owned(),
            mode => mode.label().to_owned(),
        }
    ));
    ui.label(match effective.ambient_occlusion {
        AmbientOcclusion::Off => "Ambient occlusion: off".to_owned(),
        mode => format!(
            "Ambient occlusion: {}, {} samples per pixel",
            mode.label(),
            mode.samples()
        ),
    });
    ui.label(match effective.reflection_quality.map() {
        Some(map) => format!(
            "Reflections: {}, the sky baked into a {}² cubemap ({} samples)",
            effective.reflection_quality.label(),
            map.size,
            map.samples
        ),
        None => "Reflections: Low, from the sky's gradient (no sky image)".to_owned(),
    });
    ui.label(if effective.contact_shadows {
        "Contact shadows: on (small gaps the shadow maps miss)"
    } else {
        "Contact shadows: off"
    });
    ui.label(if effective.realtime_gi {
        "Real-time GI: on (light probe volumes traced again, two probes a frame)"
    } else {
        "Real-time GI: off (light probe volumes keep their bakes)"
    });
    ui.label(match effective.cloud_quality.steps() {
        (0, _) => "Clouds: Low, a flat sheet".to_owned(),
        (steps, _) => format!(
            "Clouds: {}, looked through in {steps} steps",
            effective.cloud_quality.label()
        ),
    });
    ui.label(if effective.volumetric_fog {
        "Volumetric fog: on (sun shafts and lamp light in an Atmosphere's fog)"
    } else {
        "Volumetric fog: off (fog without shadows in it)"
    });
    ui.label(match effective.depth_of_field {
        DepthOfFieldQuality::Off => "Depth of field: off".to_owned(),
        quality => format!(
            "Depth of field: {}, {} samples per pixel",
            quality.label(),
            quality.samples()
        ),
    });
    ui.label(match effective.motion_blur {
        MotionBlurQuality::Off => "Motion blur: off".to_owned(),
        quality => format!(
            "Motion blur: {}, {} samples per pixel",
            quality.label(),
            quality.samples()
        ),
    });
    let lens: Vec<&str> = [
        (effective.vignette, "vignette"),
        (effective.film_grain, "film grain"),
        (effective.chromatic_aberration, "chromatic aberration"),
        (effective.sharpen, "sharpening"),
    ]
    .into_iter()
    .filter_map(|(on, name)| on.then_some(name))
    .collect();
    ui.label(if lens.is_empty() {
        "Lens effects: none".to_owned()
    } else {
        format!("Lens effects: {}", lens.join(", "))
    });
    for note in &effective.notes {
        ui.label(egui::RichText::new(note).color(palette.warning));
    }
    ui.label(
        egui::RichText::new(
            "A world's Lighting can ask for less (ShadowsEnabled, ShadowDistance, \
             PostProcessingEnabled), never more.",
        )
        .color(palette.text_secondary),
    );
}
