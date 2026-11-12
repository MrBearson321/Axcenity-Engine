//! The control for each type of property value: numbers, vectors, colours,
//! CFrames, enums, physical properties and the rest.

use super::*;

/// Whether `name` is one of a UI element's transparencies, each from 0
/// (solid) to 1 (invisible).
pub(super) fn is_ui_transparency(name: &str) -> bool {
    matches!(
        name,
        "BackgroundTransparency" | "TextTransparency" | "ImageTransparency"
    )
}

/// Draws the control for a property's value and returns its new value if
/// the user changed it this frame.
pub(super) fn value_edit(ui: &mut egui::Ui, name: &str, value: Value) -> Option<Value> {
    match value {
        Value::Bool(mut on) => ui
            .checkbox(&mut on, "")
            .changed()
            .then_some(Value::Bool(on)),
        // A slider shows where between crisp and soft the shadows are.
        Value::Number(mut number) if name == "ShadowSoftness" => ui
            .add(egui::Slider::new(&mut number, 0.0..=1.0).clamping(egui::SliderClamping::Edits))
            .changed()
            .then_some(Value::Number(number)),
        Value::Number(mut number) if name == "AmbientOcclusionIntensity" => ui
            .add(egui::Slider::new(&mut number, 0.0..=2.0).clamping(egui::SliderClamping::Edits))
            .changed()
            .then_some(Value::Number(number)),
        // Lighting's time of day goes round the clock (the world wraps it).
        Value::Number(mut number) if name == "ClockTime" => ui
            .add(
                egui::Slider::new(&mut number, 0.0..=24.0)
                    .clamping(egui::SliderClamping::Never)
                    .suffix(" h")
                    .max_decimals(2),
            )
            .changed()
            .then_some(Value::Number(number)),
        // `Edits`, not `Always`: `Always` writes the value back every frame,
        // rounded to one decimal, so just showing it was an undo step.
        Value::Number(mut number) if name == "GeographicLatitude" => ui
            .add(
                egui::Slider::new(&mut number, -90.0..=90.0)
                    .clamping(egui::SliderClamping::Edits)
                    .suffix("°")
                    .max_decimals(1),
            )
            .changed()
            .then_some(Value::Number(number)),
        // Which way Clouds drift (a SpotLight's or emitter's Direction is a
        // Vector3).
        Value::Number(mut number) if name == "Direction" => ui
            .add(
                egui::Slider::new(&mut number, 0.0..=360.0)
                    .clamping(egui::SliderClamping::Never)
                    .suffix("°")
                    .max_decimals(1),
            )
            .changed()
            .then_some(Value::Number(number)),
        // An Atmosphere's air, Clouds' cover and thickness, and the
        // Weather's shares.
        Value::Number(mut number)
            if matches!(
                name,
                "Density" | "Offset" | "Cover" | "Intensity" | "Wetness" | "SnowCover"
            ) =>
        {
            ui.add(egui::Slider::new(&mut number, 0.0..=1.0).clamping(egui::SliderClamping::Always))
                .changed()
                .then_some(Value::Number(number))
        }
        Value::Number(mut number) if matches!(name, "Glare" | "Haze") => ui
            .add(egui::Slider::new(&mut number, 0.0..=10.0).clamping(egui::SliderClamping::Always))
            .changed()
            .then_some(Value::Number(number)),
        // A UI element's three see-through parts, as percentages.
        Value::Number(number) if is_ui_transparency(name) => {
            transparency_slider(ui, number).map(Value::Number)
        }
        // A UI element's or a particle's turn, in degrees clockwise.
        Value::Number(mut number) if name == "Rotation" => ui
            .add(
                egui::DragValue::new(&mut number)
                    .speed(1.0)
                    .suffix("°")
                    .max_decimals(1),
            )
            .changed()
            .then_some(Value::Number(number)),
        Value::Number(mut number) => {
            let mut drag = egui::DragValue::new(&mut number).max_decimals(MAX_DECIMALS);
            drag = match name {
                // These only make sense from 0 to 1.
                "Metalness" | "Roughness" | "AmbientOcclusionStrength" | "Volume" => {
                    drag.speed(0.01).range(0.0..=1.0)
                }
                "PlaybackSpeed" => drag.speed(0.01).range(0.01..=20.0),
                "RollOffMinDistance" | "RollOffMaxDistance" => {
                    drag.speed(1.0).range(0.0..=100_000.0)
                }
                "NormalStrength" | "EmissiveStrength" | "TextSize" => {
                    drag.speed(0.01).range(0.0..=f64::INFINITY)
                }
                "DesignWidth" | "DesignHeight" => drag.speed(1.0).range(1.0..=10_000.0),
                _ => drag.speed(0.1),
            };
            // `clamp_existing_to_range(false)`: only clamp what the user
            // types or drags, never a value that was already there, so just
            // selecting a part can't change it.
            let drag = drag.clamp_existing_to_range(false);
            ui.add(drag).changed().then_some(Value::Number(number))
        }
        // Shown but not typed into: each half-typed path would be loaded
        // (and fail). Importing a model sets it.
        Value::String(text) if name == "ScreenInsets" => {
            let current = if text.eq_ignore_ascii_case("DeviceSafeInsets") {
                "DeviceSafeInsets"
            } else {
                "None"
            };
            choice_control(ui, name, current, &["None", "DeviceSafeInsets"], |insets| {
                insets
            })
            .map(|insets| Value::String(insets.to_owned()))
        }
        Value::String(text) if name == "ScaleMode" => {
            let current = ["Fit", "Scale"]
                .into_iter()
                .find(|mode| text.eq_ignore_ascii_case(mode))
                .unwrap_or("Fixed");
            choice_control(ui, name, current, &["Fixed", "Fit", "Scale"], |mode| mode)
                .map(|mode| Value::String(mode.to_owned()))
        }
        // A ScrollingFrame's axes.
        Value::String(text) if name == "ScrollingDirection" || name == "AutomaticCanvasSize" => {
            // What the layout makes of anything else.
            let (choices, otherwise): (&[&'static str], _) = if name == "ScrollingDirection" {
                (&["X", "Y", "XY"], "XY")
            } else {
                (&["None", "X", "Y", "XY"], "None")
            };
            let current = choices
                .iter()
                .copied()
                .find(|choice| text.eq_ignore_ascii_case(choice))
                .unwrap_or(otherwise);
            choice_control(ui, name, current, choices, |choice| choice)
                .map(|choice| Value::String(choice.to_owned()))
        }
        // An IKControl's Type.
        Value::String(text) if name == "Type" => {
            let choices: &[&'static str] = &["Transform", "Position", "Rotation", "LookAt"];
            let current = choices
                .iter()
                .copied()
                .find(|choice| text.eq_ignore_ascii_case(choice))
                .unwrap_or("Position");
            choice_control(ui, name, current, choices, |choice| choice)
                .map(|choice| Value::String(choice.to_owned()))
        }
        // A Slider's or ProgressBar's way, and a Toggle's look.
        Value::String(text) if name == "Orientation" || name == "Style" => {
            let choices: &[&'static str] = if name == "Orientation" {
                &["Horizontal", "Vertical"]
            } else {
                &["Switch", "Checkbox"]
            };
            // What the layout makes of anything else: the first.
            let current = choices
                .iter()
                .copied()
                .find(|choice| text.eq_ignore_ascii_case(choice))
                .unwrap_or(choices[0]);
            choice_control(ui, name, current, choices, |choice| choice)
                .map(|choice| Value::String(choice.to_owned()))
        }
        // A Dropdown's choices, one per line.
        Value::String(mut text) if name == "Options" => ui
            .add(egui::TextEdit::multiline(&mut text).desired_rows(3))
            .changed()
            .then_some(Value::String(text)),
        Value::String(mut text) if name == "MeshId" => {
            ui.add_enabled(false, egui::TextEdit::singleline(&mut text))
                .on_disabled_hover_text(
                    "The model's file in the project. Import Model (in the toolbar or \
                     the File menu) sets it; to use another model, import that one.",
                );
            None
        }
        Value::String(mut text) => ui
            .text_edit_singleline(&mut text)
            .changed()
            .then_some(Value::String(text)),
        Value::Vector3(mut vector) => {
            let (speed, min) = match name {
                // Degrees, so a bigger step per point dragged.
                "Orientation" => (1.0, f32::NEG_INFINITY),
                "Size" => (0.1, MIN_SIZE),
                _ => (0.1, f32::NEG_INFINITY),
            };
            let mut changed = false;
            // Wraps when the value column is too narrow for all three.
            ui.horizontal_wrapped(|ui| {
                for (axis, number) in [
                    ("X ", &mut vector.x),
                    ("Y ", &mut vector.y),
                    ("Z ", &mut vector.z),
                ] {
                    let drag = egui::DragValue::new(number)
                        .prefix(axis)
                        .speed(speed)
                        .range(min..=f32::INFINITY)
                        .clamp_existing_to_range(false)
                        .max_decimals(MAX_DECIMALS);
                    // `|=` keeps `changed` true once any axis changes.
                    changed |= ui.add(drag).changed();
                }
            });
            changed.then_some(Value::Vector3(vector))
        }
        Value::Vector2(mut vector) => {
            let mut changed = false;
            // A SurfaceAppearance's are in copies of a texture, not pixels.
            let speed = if matches!(name, "Tiling" | "Offset") {
                0.01
            } else {
                1.0
            };
            ui.horizontal_wrapped(|ui| {
                for (axis, number) in [("X ", &mut vector.x), ("Y ", &mut vector.y)] {
                    let drag = egui::DragValue::new(number)
                        .prefix(axis)
                        .speed(speed)
                        .max_decimals(MAX_DECIMALS);
                    changed |= ui.add(drag).changed();
                }
            });
            changed.then_some(Value::Vector2(vector))
        }
        Value::UDim2(mut value) => {
            let mut changed = false;
            // Each axis stays together; Y goes under X when narrow.
            let layout = if ui.available_width() < UDIM2_ONE_ROW {
                egui::Layout::top_down(egui::Align::Min)
            } else {
                egui::Layout::left_to_right(egui::Align::Center)
            };
            ui.with_layout(layout, |ui| {
                for (prefix, component) in [("X", &mut value.x), ("Y", &mut value.y)] {
                    ui.horizontal(|ui| {
                        ui.label(prefix);
                        changed |= ui
                            .add(
                                egui::DragValue::new(&mut component.scale)
                                    .prefix("S ")
                                    .speed(0.01)
                                    .max_decimals(MAX_DECIMALS),
                            )
                            .changed();
                        changed |= ui
                            .add(
                                egui::DragValue::new(&mut component.offset)
                                    .prefix("O ")
                                    .speed(1.0)
                                    .max_decimals(MAX_DECIMALS),
                            )
                            .changed();
                    });
                }
            });
            changed.then_some(Value::UDim2(value))
        }
        Value::Color3(color) => color_control(ui, color).map(Value::Color3),
        Value::PartType(shape) => {
            choice_control(ui, name, shape, &PartType::ALL, |s| s.as_str()).map(Value::PartType)
        }
        Value::Material(material) => {
            choice_control(ui, name, material, &Material::ALL, |m| m.as_str()).map(Value::Material)
        }
        Value::ParticleType(kind) => {
            choice_control(ui, name, kind, &ParticleType::ALL, |k| k.as_str())
                .map(Value::ParticleType)
        }
        Value::TextureChannel(channel) => {
            choice_control(ui, name, channel, &TextureChannel::ALL, |c| c.as_str())
                .map(Value::TextureChannel)
        }
        Value::ToneMapping(curve) => {
            choice_control(ui, name, curve, &ToneMapping::ALL, |c| c.as_str())
                .map(Value::ToneMapping)
        }
        Value::CFrame(cf) => cframe_control(ui, cf).map(Value::CFrame),
        Value::EnumItem(item) => enum_item_control(ui, name, &item).map(Value::EnumItem),
        // Edited with the world at hand (`object_control`), and with the
        // part's material at hand (`physical_control`).
        Value::Instance(_) | Value::PhysicalProperties(_) => None,
    }
}

/// A part's `CustomPhysicalProperties`: a Custom box and its five numbers.
/// Off, the numbers are the material's, greyed out; turning it on starts
/// from them. Returns the new value (`None` for the material's).
pub(super) fn physical_control(
    ui: &mut egui::Ui,
    instance: &Instance,
    custom: Option<PhysicalProperties>,
) -> Option<Option<PhysicalProperties>> {
    let material = instance
        .property("Material")
        .and_then(Value::as_material)
        .unwrap_or(Material::Plastic);
    let mut on = custom.is_some();
    let mut numbers = custom.unwrap_or_else(|| material.physical()).to_array();
    let mut changed = None;
    let mut dragged = false;
    // In `PhysicalProperties.new`'s order: name, range, hover text.
    let rows: [(&str, f32, f32, &str); 5] = [
        (
            "Density ",
            PhysicalProperties::MIN_DENSITY,
            100.0,
            "Mass per cubic stud.",
        ),
        ("Friction ", 0.0, 2.0, "0 slides like ice; 2 grips."),
        (
            "Elasticity ",
            0.0,
            1.0,
            "How much speed a bounce keeps: 0 none, 1 all.",
        ),
        (
            "Friction weight ",
            0.0,
            100.0,
            "Where two parts touch, the one with the bigger weight decides more of the friction.",
        ),
        (
            "Elasticity weight ",
            0.0,
            100.0,
            "Where two parts touch, the one with the bigger weight decides more of the bounce.",
        ),
    ];
    ui.vertical(|ui| {
        let hint = format!(
            "Off, it weighs, grips and bounces like {}. On, these numbers are its own.",
            material.as_str()
        );
        if ui.checkbox(&mut on, "Custom").on_hover_text(hint).changed() {
            changed = Some(on.then(|| material.physical()));
        }
        ui.add_enabled_ui(on, |ui| {
            for (number, (label, min, max, hint)) in numbers.iter_mut().zip(rows) {
                let drag = egui::DragValue::new(number)
                    .prefix(label)
                    .speed(0.01)
                    .range(min..=max)
                    .max_decimals(MAX_DECIMALS);
                dragged |= ui.add(drag).on_hover_text(hint).changed();
            }
        });
    });
    if dragged {
        changed = Some(Some(PhysicalProperties::from_array(numbers)));
    }
    changed
}

/// A CFrame as its position and its rotation in degrees (as a part's
/// Orientation). The rotation is only rewritten when it's edited.
pub(super) fn cframe_control(ui: &mut egui::Ui, cf: CFrame) -> Option<CFrame> {
    use axcenity_render::glam::Quat;
    let [x, y, z, w] = cf.rotation;
    let rotation = Quat::from_xyzw(x, y, z, w);
    let orientation = axcenity_scene::orientation_from_rotation(rotation);
    let (position, turned) = ui
        .vertical(|ui| {
            (
                value_edit(ui, "Position", Value::Vector3(cf.position)),
                value_edit(ui, "Orientation", Value::Vector3(orientation)),
            )
        })
        .inner;
    let mut changed = None;
    if let Some(Value::Vector3(position)) = position {
        changed = Some(CFrame { position, ..cf });
    }
    if let Some(Value::Vector3(orientation)) = turned {
        let rotation = axcenity_scene::rotation_from_orientation(orientation);
        changed = Some(CFrame {
            rotation: rotation.to_array(),
            ..changed.unwrap_or(cf)
        });
    }
    changed
}

/// The items of `enum_name`, when it's one of the enums properties use.
pub(crate) fn enum_items(enum_name: &str) -> Option<Vec<&'static str>> {
    Some(match enum_name {
        "Material" => Material::ALL.iter().map(|m| m.as_str()).collect(),
        "PartType" => PartType::ALL.iter().map(|t| t.as_str()).collect(),
        "TextureChannel" => TextureChannel::ALL.iter().map(|c| c.as_str()).collect(),
        "ParticleType" => ParticleType::ALL.iter().map(|t| t.as_str()).collect(),
        "ToneMapping" => ToneMapping::ALL.iter().map(|t| t.as_str()).collect(),
        // What drives a joint (Phase 36).
        "ActuatorType" => vec!["None", "Motor", "Servo"],
        // A mover's axes and an align's target (Phase 37).
        "ActuatorRelativeTo" => vec!["Attachment0", "Attachment1", "World"],
        "PositionAlignmentMode" | "OrientationAlignmentMode" => {
            vec!["OneAttachment", "TwoAttachment"]
        }
        // How a Fracture's pieces are spread (Phase 42).
        "FracturePattern" => vec!["Uniform", "Impact"],
        // What a UnionOperation collides as (Phase 50).
        "CollisionFidelity" => vec!["Box", "Hull", "PreciseConvexDecomposition"],
        // How a Model streams (Phase 80).
        "ModelStreamingMode" => axcenity_core::streaming::STREAMING_MODES.to_vec(),
        // Which ProximityPrompts in reach show (Phase 44).
        "ProximityPromptExclusivity" => vec!["OnePerButton", "OneGlobally", "AlwaysShow"],
        // The room a ReverbZone sounds like (Phase 52).
        "ReverbType" => axcenity_audio::REVERB_TYPES.to_vec(),
        // How a spatial Sound gets quieter with distance (G8).
        "RollOffMode" => axcenity_audio::mix::ROLL_OFF_MODES.to_vec(),
        // What a Weather lets fall (graphics Phase 27).
        "Precipitation" => vec!["None", "Rain", "Snow"],
        // A SplinePoint's handles and a SplineFollower's ends (Phase 73).
        "SplineTangentMode" => axcenity_core::spline::TangentMode::NAMES.to_vec(),
        "SplineFollowMode" => vec!["Loop", "PingPong", "Once"],
        // How an Effect changes what it changes (Phase 76).
        "EffectOperation" => vec!["Add", "Multiply"],
        // A Decal's face (Phase 75).
        "NormalId" => vec!["Right", "Top", "Back", "Left", "Bottom", "Front"],
        _ => return None,
    })
}

/// An enum item: a dropdown of the items when it's one of the enums
/// properties use, else its name, shown.
pub(super) fn enum_item_control(
    ui: &mut egui::Ui,
    name: &str,
    item: &EnumItem,
) -> Option<EnumItem> {
    let Some(names) = enum_items(&item.enum_name) else {
        ui.label(format!("{}.{}", item.enum_name, item.item));
        return None;
    };
    let current = names
        .iter()
        .copied()
        .find(|n| *n == item.item)
        .unwrap_or(names[0]);
    choice_control(ui, name, current, &names, |n| n).map(|picked| EnumItem {
        enum_name: item.enum_name.clone(),
        item: picked.to_owned(),
    })
}

/// A color swatch that opens the preset swatches, the saved palettes and a
/// color picker, with the 0–255 values beside it.
pub(crate) fn color_control(ui: &mut egui::Ui, color: Color3) -> Option<Color3> {
    let byte = |channel: f32| (channel.clamp(0.0, 1.0) * 255.0).round() as u8;
    let mut bytes = [color.r, color.g, color.b].map(byte);
    let mut picked = None;
    ui.horizontal(|ui| {
        if crate::color_palette::edit_srgb(ui, &mut bytes).changed() {
            let [r, g, b] = bytes.map(|channel| f32::from(channel) / 255.0);
            picked = Some(Color3::new(r, g, b));
        }
        let shown = picked.unwrap_or(color);
        ui.weak(format!(
            "{}, {}, {}",
            byte(shown.r),
            byte(shown.g),
            byte(shown.b)
        ));
    });
    picked
}

/// A dropdown to pick one item of an enum such as `Material`.
pub(super) fn choice_control<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    name: &str,
    current: T,
    items: &[T],
    label: fn(T) -> &'static str,
) -> Option<T> {
    let mut chosen = current;
    egui::ComboBox::from_id_salt(name)
        .selected_text(label(current))
        .show_ui(ui, |ui| {
            for &item in items {
                ui.selectable_value(&mut chosen, item, label(item));
            }
        });
    (chosen != current).then_some(chosen)
}
