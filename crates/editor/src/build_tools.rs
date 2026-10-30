//! The building tools' options panel, modelled on F3X Building Tools: a
//! small panel floating at the top left of the viewport for the picked
//! tool, acting on every selected part (and the parts inside selected
//! Models).
//!
//! - **Move / Resize / Rotate**: the axes, directions or pivot (see
//!   `gizmo.rs`), and the step to snap to.
//! - **Transform**: the last selected part's Position, Rotation and Size to
//!   type in (a new Position moves the whole selection by as much), the
//!   axes and the pivot, and both steps.
//! - **Paint**: a palette of swatches and a color picker. Picking a color
//!   paints the selection; with this tool, clicking a part paints it.
//! - **Material**: the material, transparency, metalness, roughness and
//!   whether it casts a shadow.
//! - **Anchor**: switch `Anchored` for the selection (Enter toggles, as in
//!   F3X). Whether parts collide is in Properties, and the Home tab's
//!   Collision Groups.
//! - **Lighting**: add, change or remove a PointLight or SpotLight in each
//!   selected part.
//! - **Emission**: a glow of any color (`EmissiveColor` and `Emission`),
//!   and particles: add a ParticleEmitter set up as one of the built-in
//!   types, remove them, or see a burst.
//! - **Angular**: copies of the last selected part at an angle, like
//!   Archimedes (see `angular.rs`); Enter or Extend places them.
//!
//! The panel only reads the world and returns [`ToolAction`]s; the app
//! applies each as one undo step. Values shown are the last selected part's.

use axcenity_core::{ClassName, Color3, InstanceId, Material, ParticleType, Value, Vector3, World};
use eframe::egui;

use crate::angular::AngularSettings;
use crate::gizmo::{Axis, Modes, MoveAxes, ResizeDirections, RotatePivot, Side, Snap, Tool};
use crate::theme::{self, icons};

/// F3X-style swatches, as sRGB bytes, in rows of 7.
const PALETTE: [[u8; 3]; 28] = [
    [248, 248, 248],
    [205, 205, 205],
    [163, 162, 165],
    [99, 95, 98],
    [27, 42, 53],
    [17, 17, 17],
    [215, 197, 154],
    [196, 40, 28],
    [255, 0, 0],
    [218, 133, 65],
    [245, 205, 48],
    [255, 255, 0],
    [160, 95, 53],
    [124, 92, 70],
    [75, 151, 75],
    [0, 255, 0],
    [39, 70, 45],
    [161, 196, 140],
    [234, 184, 146],
    [105, 64, 40],
    [204, 142, 105],
    [13, 105, 172],
    [0, 0, 255],
    [110, 153, 202],
    [0, 255, 255],
    [4, 175, 236],
    [107, 50, 124],
    [255, 0, 191],
];
const SWATCH: f32 = 20.0;

/// The building tools' own settings, kept while the editor runs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BuildSettings {
    /// The Paint tool's color.
    pub paint_color: Color3,
    pub angular: AngularSettings,
}

impl Default for BuildSettings {
    fn default() -> Self {
        Self {
            paint_color: Color3::new(0.64, 0.64, 0.64),
            angular: AngularSettings::default(),
        }
    }
}

/// Something the panel asks the app to do to the selected parts.
#[derive(Clone, Debug, PartialEq)]
pub enum ToolAction {
    /// Set a property on every selected part that has it.
    Set {
        property: &'static str,
        value: Value,
    },
    /// Put a new light of this class in each selected part without one.
    AddLight(ClassName),
    /// Take out the lights of this class in the selected parts.
    RemoveLights(ClassName),
    /// Set a property of every light of this class in the selected parts.
    SetLight {
        class: ClassName,
        property: &'static str,
        value: Value,
    },
    /// Place the Angular tool's copies of the last selected part.
    Extend,
    /// Put a ParticleEmitter set up as `kind` in each selected part.
    AddEmitter(ParticleType),
    /// Give off a burst from every emitter in the selected parts, to see it.
    Burst,
    /// Move every selected part by this much (the Transform panel's Position).
    MoveBy(Vector3),
}

/// What the panel shows: the world and the parts the tools act on.
pub struct PanelScene<'a> {
    pub world: &'a World,
    /// The parts, with the last selected one last.
    pub parts: &'a [InstanceId],
}

impl PanelScene<'_> {
    fn last(&self) -> Option<&axcenity_core::Instance> {
        self.parts.last().and_then(|&id| self.world.get(id))
    }

    fn number(&self, property: &str) -> Option<f64> {
        self.last()?.property(property).and_then(Value::as_number)
    }

    fn color(&self, property: &str) -> Option<Color3> {
        self.last()?.property(property).and_then(Value::as_color3)
    }

    fn flag(&self, property: &str) -> Option<bool> {
        self.last()?.property(property).and_then(Value::as_bool)
    }

    /// How many of the parts have `property` set to true, and how many have it at all.
    fn count(&self, property: &str) -> (usize, usize) {
        let values: Vec<bool> = self
            .parts
            .iter()
            .filter_map(|&id| self.world.get(id)?.property(property)?.as_bool())
            .collect();
        (values.iter().filter(|on| **on).count(), values.len())
    }

    /// The first light of `class` in any of the parts.
    fn light(&self, class: ClassName) -> Option<&axcenity_core::Instance> {
        self.parts.iter().find_map(|&part| {
            self.world.get(part)?.children().iter().find_map(|&child| {
                self.world
                    .get(child)
                    .filter(|instance| instance.class() == class)
            })
        })
    }
}

/// Draws `tool`'s panel inside `ui`. Returns what it asks to be done.
pub fn panel(
    ui: &mut egui::Ui,
    tool: Tool,
    scene: &PanelScene,
    settings: &mut BuildSettings,
    modes: &mut Modes,
    snap: &mut Snap,
) -> Vec<ToolAction> {
    let mut actions = Vec::new();
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(theme::with_icon(theme::tool_icon(tool), tool.label()))
                .family(theme::heading_family())
                .color(theme::palette(ui.ctx()).text_strong),
        );
        ui.weak(match scene.parts.len() {
            0 => "nothing selected".to_owned(),
            1 => "1 part".to_owned(),
            n => format!("{n} parts"),
        });
    });
    ui.separator();
    match tool {
        Tool::Select => {}
        Tool::Move => {
            axes_choice(ui, modes);
            studs_step(ui, snap);
        }
        Tool::Scale => {
            choice(
                ui,
                "Directions",
                &mut modes.directions,
                &[
                    (
                        ResizeDirections::Normal,
                        "Normal",
                        "Move the dragged face; the opposite one stays.",
                    ),
                    (
                        ResizeDirections::Both,
                        "Both",
                        "Move both opposite faces; the center stays.",
                    ),
                ],
            );
            studs_step(ui, snap);
        }
        Tool::Rotate => {
            pivot_choice(ui, modes);
            turn_step(ui, snap);
        }
        Tool::Transform => {
            transform(ui, scene, &mut actions);
            ui.separator();
            axes_choice(ui, modes);
            pivot_choice(ui, modes);
            studs_step(ui, snap);
            turn_step(ui, snap);
        }
        Tool::Paint => paint(ui, scene, settings, &mut actions),
        Tool::Material => material(ui, scene, &mut actions),
        Tool::Anchor => switch(
            ui,
            scene,
            "Anchored",
            ("Anchor", icons::ANCHOR, "anchored"),
            (
                "Unanchor",
                icons::ANCHOR_SIMPLE,
                "Unanchored parts fall and collide.",
            ),
            &mut actions,
        ),
        Tool::Lighting => {
            light(ui, scene, ClassName::PointLight, &mut actions);
            ui.separator();
            light(ui, scene, ClassName::SpotLight, &mut actions);
        }
        Tool::Emission => emission(ui, scene, &mut actions),
        Tool::Angular => angular(ui, scene, &mut settings.angular, &mut actions),
    }
    actions
}

fn angular(
    ui: &mut egui::Ui,
    scene: &PanelScene,
    settings: &mut AngularSettings,
    actions: &mut Vec<ToolAction>,
) {
    egui::Grid::new("angular tool")
        .num_columns(2)
        .spacing([8.0, 4.0])
        .show(ui, |ui| {
            ui.label("Extend")
                .on_hover_text("The face of the part each copy goes against, on its own axes.");
            ui.horizontal(|ui| {
                for axis in [Axis::X, Axis::Y, Axis::Z] {
                    for (side, sign) in [(Side::Plus, "+"), (Side::Minus, "-")] {
                        let text = format!("{sign}{axis:?}");
                        if ui
                            .selectable_label(settings.extend == (axis, side), text)
                            .clicked()
                        {
                            settings.extend = (axis, side);
                        }
                    }
                }
            });
            ui.end_row();
            ui.label("Bend around")
                .on_hover_text("The part's own axis the copies turn around.");
            ui.horizontal(|ui| {
                let bend = settings.bend_axis();
                for axis in [Axis::X, Axis::Y, Axis::Z] {
                    if axis == settings.extend.0 {
                        continue;
                    }
                    if ui
                        .selectable_label(bend == axis, format!("{axis:?}"))
                        .clicked()
                    {
                        settings.bend = axis;
                    }
                }
            });
            ui.end_row();
            ui.label("Angle");
            ui.horizontal(|ui| {
                ui.add(
                    egui::DragValue::new(&mut settings.degrees)
                        .range(-180.0..=180.0)
                        .speed(0.5)
                        .max_decimals(2)
                        .suffix("°"),
                );
                if ui
                    .small_button("Flip")
                    .on_hover_text("Bend the other way.")
                    .clicked()
                {
                    settings.degrees = -settings.degrees;
                }
            });
            ui.end_row();
            ui.label("Copies");
            ui.horizontal(|ui| {
                ui.add(
                    egui::DragValue::new(&mut settings.count)
                        .range(1..=AngularSettings::MAX_COUNT)
                        .speed(0.2),
                );
                let close = settings.copies_to_close();
                if ui
                    .add_enabled(close.is_some(), egui::Button::new("Close loop").small())
                    .on_hover_text("As many copies as close a full ring.")
                    .on_disabled_hover_text("Pick an angle that divides 360°, such as 15° or 30°.")
                    .clicked()
                    && let Some(close) = close
                {
                    settings.count = close;
                }
            });
            ui.end_row();
        });
    let can = scene
        .last()
        .is_some_and(|part| part.property("Size").is_some());
    if ui
        .add_enabled(can, egui::Button::new(theme::with_icon(icons::SPIRAL, "Extend")))
        .on_hover_text("Place the copies shown by the outline (Enter). The last copy is selected, so pressing again carries on.")
        .on_disabled_hover_text("Select a part to extend.")
        .clicked()
    {
        actions.push(ToolAction::Extend);
    }
}

/// The Transform panel's numbers: the last selected part's Position,
/// Rotation (its Orientation, in degrees) and Size, each X, Y and Z.
fn transform(ui: &mut egui::Ui, scene: &PanelScene, actions: &mut Vec<ToolAction>) {
    if !needs_parts(ui, scene) {
        return;
    }
    let vector = |name| scene.last()?.property(name).and_then(Value::as_vector3);
    let rows: [(&str, &'static str, &str, f64); 3] = [
        ("Position", "Position", "", 0.05),
        ("Rotation", "Orientation", "°", 0.5),
        ("Size", "Size", "", 0.05),
    ];
    egui::Grid::new("transform numbers")
        .num_columns(4)
        .spacing([4.0, 4.0])
        .show(ui, |ui| {
            for (label, property, suffix, speed) in rows {
                let Some(before) = vector(property) else {
                    continue;
                };
                ui.label(label);
                let mut after = before;
                for (axis, value) in [
                    ("X", &mut after.x),
                    ("Y", &mut after.y),
                    ("Z", &mut after.z),
                ] {
                    let mut drag = egui::DragValue::new(value)
                        .speed(speed)
                        .max_decimals(3)
                        .prefix(format!("{axis} "))
                        .suffix(suffix);
                    if property == "Size" {
                        drag = drag.range(crate::properties::MIN_SIZE..=f32::MAX);
                    }
                    ui.add_sized([62.0, 20.0], drag);
                }
                ui.end_row();
                if after != before {
                    actions.push(if property == "Position" {
                        ToolAction::MoveBy(Vector3::new(
                            after.x - before.x,
                            after.y - before.y,
                            after.z - before.z,
                        ))
                    } else {
                        ToolAction::Set {
                            property,
                            value: Value::Vector3(after),
                        }
                    });
                }
            }
        });
    if scene.parts.len() > 1 {
        ui.weak("The last selected part's numbers. A new Position moves them all.");
    }
}

/// Which way the Move handles point.
fn axes_choice(ui: &mut egui::Ui, modes: &mut Modes) {
    choice(
        ui,
        "Axes",
        &mut modes.axes,
        &[
            (
                MoveAxes::Global,
                "Global",
                "The world's axes, from the middle of the selection.",
            ),
            (
                MoveAxes::Local,
                "Local",
                "Each part moves along its own axes.",
            ),
            (
                MoveAxes::Last,
                "Last",
                "Every part moves along the last selected part's axes.",
            ),
        ],
    );
}

/// What the Rotate rings turn parts around.
fn pivot_choice(ui: &mut egui::Ui, modes: &mut Modes) {
    choice(
        ui,
        "Pivot",
        &mut modes.pivot,
        &[
            (
                RotatePivot::Center,
                "Center",
                "Around the middle of the selection, on the world's axes.",
            ),
            (
                RotatePivot::Local,
                "Local",
                "Each part around its own center and axes.",
            ),
            (
                RotatePivot::Last,
                "Last",
                "Around the last selected part's center and axes.",
            ),
        ],
    );
}

/// The rotate snapping step.
fn turn_step(ui: &mut egui::Ui, snap: &mut Snap) {
    ui.horizontal(|ui| {
        ui.checkbox(&mut snap.rotate_on, "Turn");
        ui.add_enabled(
            snap.rotate_on,
            egui::DragValue::new(&mut snap.rotate_step)
                .range(Snap::MIN_ROTATE_STEP..=180.0)
                .speed(0.5)
                .suffix("°"),
        );
    });
}

/// A row of buttons picking one of `options`.
fn choice<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut T,
    options: &[(T, &str, &str)],
) {
    ui.horizontal(|ui| {
        ui.label(label);
        for &(option, text, hover) in options {
            if ui
                .selectable_label(*value == option, text)
                .on_hover_text(hover)
                .clicked()
            {
                *value = option;
            }
        }
    });
}

fn studs_step(ui: &mut egui::Ui, snap: &mut Snap) {
    ui.horizontal(|ui| {
        ui.checkbox(&mut snap.move_on, "Increment");
        ui.add_enabled(
            snap.move_on,
            egui::DragValue::new(&mut snap.move_step)
                .range(Snap::MIN_MOVE_STEP..=1000.0)
                .speed(0.05)
                .max_decimals(3)
                .suffix(" studs"),
        );
    });
}

fn needs_parts(ui: &mut egui::Ui, scene: &PanelScene) -> bool {
    if scene.parts.is_empty() {
        ui.weak("Select parts to change them.");
        return false;
    }
    true
}

fn paint(
    ui: &mut egui::Ui,
    scene: &PanelScene,
    settings: &mut BuildSettings,
    actions: &mut Vec<ToolAction>,
) {
    let mut picked = None;
    egui::Grid::new("paint swatches")
        .spacing([3.0, 3.0])
        .show(ui, |ui| {
            for (i, rgb) in PALETTE.iter().enumerate() {
                let color = egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
                let (rect, response) =
                    ui.allocate_exact_size(egui::vec2(SWATCH, SWATCH), egui::Sense::click());
                let current = to_bytes(settings.paint_color) == *rgb;
                let stroke = if current || response.hovered() {
                    egui::Stroke::new(2.0_f32, ui.visuals().strong_text_color())
                } else {
                    egui::Stroke::new(1.0_f32, ui.visuals().weak_text_color())
                };
                ui.painter().rect_filled(rect, 3.0, color);
                ui.painter()
                    .rect_stroke(rect, 3.0, stroke, egui::StrokeKind::Inside);
                if response.clicked() {
                    picked = Some(from_bytes(*rgb));
                }
                if i % 7 == 6 {
                    ui.end_row();
                }
            }
        });
    ui.horizontal(|ui| {
        let mut bytes = to_bytes(settings.paint_color);
        if ui.color_edit_button_srgb(&mut bytes).changed() {
            picked = Some(from_bytes(bytes));
        }
        ui.label("Custom");
        if ui
            .add_enabled(
                !scene.parts.is_empty(),
                egui::Button::new("Paint selection"),
            )
            .clicked()
        {
            actions.push(ToolAction::Set {
                property: "Color",
                value: Value::Color3(settings.paint_color),
            });
        }
    });
    if let Some(color) = picked {
        settings.paint_color = color;
        if !scene.parts.is_empty() {
            actions.push(ToolAction::Set {
                property: "Color",
                value: Value::Color3(color),
            });
        }
    }
    ui.weak("Click a part to paint it.");
}

fn material(ui: &mut egui::Ui, scene: &PanelScene, actions: &mut Vec<ToolAction>) {
    if !needs_parts(ui, scene) {
        return;
    }
    let current = scene
        .last()
        .and_then(|part| part.property("Material"))
        .and_then(Value::as_material);
    egui::Grid::new("material tool")
        .num_columns(2)
        .spacing([8.0, 4.0])
        .show(ui, |ui| {
            if current.is_some() {
                ui.label("Material");
                ui.horizontal(|ui| {
                    for material in Material::ALL {
                        if ui
                            .selectable_label(current == Some(material), material.as_str())
                            .clicked()
                        {
                            actions.push(ToolAction::Set {
                                property: "Material",
                                value: Value::Material(material),
                            });
                        }
                    }
                });
                ui.end_row();
            }
            for (property, label) in [
                ("Transparency", "Transparency"),
                ("Metalness", "Metalness"),
                ("Roughness", "Roughness"),
            ] {
                let Some(mut value) = scene.number(property) else {
                    continue;
                };
                ui.label(label);
                if ui
                    .add(egui::Slider::new(&mut value, 0.0..=1.0).max_decimals(2))
                    .changed()
                {
                    actions.push(ToolAction::Set {
                        property,
                        value: Value::Number(value),
                    });
                }
                ui.end_row();
            }
            if let Some(mut casts) = scene.flag("CastShadow") {
                ui.label("Shadow");
                if ui.checkbox(&mut casts, "Casts a shadow").changed() {
                    actions.push(ToolAction::Set {
                        property: "CastShadow",
                        value: Value::Bool(casts),
                    });
                }
                ui.end_row();
            }
        });
}

/// A yes/no property's panel: how many parts have it on, and buttons to
/// switch it on or off for them all.
fn switch(
    ui: &mut egui::Ui,
    scene: &PanelScene,
    property: &'static str,
    on: (&str, &str, &str),
    off: (&str, &str, &str),
    actions: &mut Vec<ToolAction>,
) {
    if !needs_parts(ui, scene) {
        return;
    }
    let (count, of) = scene.count(property);
    ui.label(format!("{count} of {of} {}", on.2));
    ui.horizontal(|ui| {
        if ui
            .selectable_label(of > 0 && count == of, theme::with_icon(on.1, on.0))
            .clicked()
        {
            actions.push(ToolAction::Set {
                property,
                value: Value::Bool(true),
            });
        }
        if ui
            .selectable_label(of > 0 && count == 0, theme::with_icon(off.1, off.0))
            .on_hover_text(off.2)
            .clicked()
        {
            actions.push(ToolAction::Set {
                property,
                value: Value::Bool(false),
            });
        }
    });
    ui.weak("Enter switches them all.");
}

fn light(ui: &mut egui::Ui, scene: &PanelScene, class: ClassName, actions: &mut Vec<ToolAction>) {
    let icon = theme::class_icon(class);
    let Some(light) = scene.light(class) else {
        ui.horizontal(|ui| {
            ui.label(theme::with_icon(icon, class.as_str()));
            if ui
                .add_enabled(!scene.parts.is_empty(), egui::Button::new("Add"))
                .on_hover_text(format!("Put a {class} in each selected part."))
                .on_disabled_hover_text("Select parts to light.")
                .clicked()
            {
                actions.push(ToolAction::AddLight(class));
            }
        });
        return;
    };
    ui.horizontal(|ui| {
        ui.label(theme::with_icon(icon, class.as_str()));
        if ui.button("Add to all").clicked() {
            actions.push(ToolAction::AddLight(class));
        }
        if ui
            .button(theme::with_icon(icons::TRASH, "Remove"))
            .clicked()
        {
            actions.push(ToolAction::RemoveLights(class));
        }
    });
    let number = |name| light.property(name).and_then(Value::as_number);
    let mut set = |property, value| {
        actions.push(ToolAction::SetLight {
            class,
            property,
            value,
        })
    };
    egui::Grid::new(("light tool", class.as_str()))
        .num_columns(2)
        .spacing([8.0, 4.0])
        .show(ui, |ui| {
            if let Some(color) = light.property("Color").and_then(Value::as_color3) {
                ui.label("Color");
                let mut bytes = to_bytes(color);
                if ui.color_edit_button_srgb(&mut bytes).changed() {
                    set("Color", Value::Color3(from_bytes(bytes)));
                }
                ui.end_row();
            }
            for (property, range) in [
                ("Brightness", 0.0..=10.0),
                ("Range", 0.0..=60.0),
                ("Angle", 1.0..=179.0),
            ] {
                let Some(mut value) = number(property) else {
                    continue;
                };
                ui.label(property);
                if ui.add(egui::Slider::new(&mut value, range)).changed() {
                    set(property, Value::Number(value));
                }
                ui.end_row();
            }
        });
}

fn emission(ui: &mut egui::Ui, scene: &PanelScene, actions: &mut Vec<ToolAction>) {
    if !needs_parts(ui, scene) {
        return;
    }
    glow(ui, scene, actions);
    ui.separator();
    ui.label(theme::with_icon(
        theme::class_icon(ClassName::ParticleEmitter),
        "Particles",
    ));
    ui.horizontal_wrapped(|ui| {
        for kind in ParticleType::ALL {
            if ui
                .button(kind.as_str())
                .on_hover_text(format!(
                    "Put a ParticleEmitter set up as {} in each selected part.",
                    kind.as_str().to_lowercase()
                ))
                .clicked()
            {
                actions.push(ToolAction::AddEmitter(kind));
            }
        }
    });
    let emitters = lights_in(scene.world, scene.parts, ClassName::ParticleEmitter).len();
    if emitters > 0 {
        ui.horizontal(|ui| {
            ui.weak(match emitters {
                1 => "1 emitter".to_owned(),
                n => format!("{n} emitters"),
            });
            if ui
                .button("Burst")
                .on_hover_text("Give off 20 particles from each, to see them.")
                .clicked()
            {
                actions.push(ToolAction::Burst);
            }
            if ui
                .button(theme::with_icon(icons::TRASH, "Remove"))
                .clicked()
            {
                actions.push(ToolAction::RemoveLights(ClassName::ParticleEmitter));
            }
        });
        ui.weak("Fine-tune them in Properties.");
    }
}

/// The glow half of the Emission tool.
fn glow(ui: &mut egui::Ui, scene: &PanelScene, actions: &mut Vec<ToolAction>) {
    let (Some(color), Some(mut strength)) =
        (scene.color("EmissiveColor"), scene.number("Emission"))
    else {
        ui.weak("Only Parts can glow. MeshParts glow through their textures.");
        return;
    };
    egui::Grid::new("emission tool")
        .num_columns(2)
        .spacing([8.0, 4.0])
        .show(ui, |ui| {
            ui.label("Color");
            let mut bytes = to_bytes(color);
            if ui.color_edit_button_srgb(&mut bytes).changed() {
                actions.push(ToolAction::Set {
                    property: "EmissiveColor",
                    value: Value::Color3(from_bytes(bytes)),
                });
            }
            ui.end_row();
            ui.label("Strength");
            if ui
                .add(egui::Slider::new(&mut strength, 0.0..=10.0).max_decimals(2))
                .on_hover_text("0 is no glow. Above 1 it blooms (with bloom on in Lighting).")
                .changed()
            {
                actions.push(ToolAction::Set {
                    property: "Emission",
                    value: Value::Number(strength),
                });
            }
            ui.end_row();
        });
    if ui.button("No glow").clicked() {
        actions.push(ToolAction::Set {
            property: "Emission",
            value: Value::Number(0.0),
        });
    }
}

/// A ParticleEmitter's properties for a built-in look: `kind`'s picture,
/// with colors, sizes and motion to suit it.
pub fn particle_preset(kind: ParticleType) -> Vec<(&'static str, Value)> {
    let number = |value: f64| Value::Number(value);
    let color = |r: f32, g: f32, b: f32| Value::Color3(Color3::new(r, g, b));
    let vector = |x: f32, y: f32, z: f32| Value::Vector3(Vector3::new(x, y, z));
    let mut properties = vec![("ParticleType", Value::ParticleType(kind))];
    properties.extend(match kind {
        ParticleType::Circle => vec![
            ("Rate", number(15.0)),
            ("Lifetime", number(2.0)),
            ("Speed", number(3.0)),
            ("SpreadAngle", number(40.0)),
            ("Size", number(0.6)),
            ("SizeEnd", number(0.2)),
            ("Color", color(1.0, 1.0, 1.0)),
            ("ColorEnd", color(0.6, 0.8, 1.0)),
        ],
        ParticleType::Spark => vec![
            ("Rate", number(30.0)),
            ("Lifetime", number(0.8)),
            ("Speed", number(15.0)),
            ("SpreadAngle", number(70.0)),
            ("Acceleration", vector(0.0, -40.0, 0.0)),
            ("Size", number(0.4)),
            ("SizeEnd", number(0.1)),
            ("Color", color(1.0, 0.85, 0.4)),
            ("ColorEnd", color(1.0, 0.4, 0.1)),
            ("LightEmission", number(1.0)),
            ("Brightness", number(3.0)),
        ],
        ParticleType::Smoke => vec![
            ("Rate", number(8.0)),
            ("Lifetime", number(4.0)),
            ("Speed", number(2.0)),
            ("SpreadAngle", number(25.0)),
            ("Drag", number(0.5)),
            ("Size", number(1.5)),
            ("SizeEnd", number(5.0)),
            ("Color", color(0.55, 0.55, 0.55)),
            ("ColorEnd", color(0.35, 0.35, 0.35)),
            ("Transparency", number(0.4)),
            ("RotSpeed", number(20.0)),
        ],
        ParticleType::Fire => vec![
            ("Rate", number(40.0)),
            ("Lifetime", number(1.0)),
            ("Speed", number(3.0)),
            ("SpreadAngle", number(20.0)),
            ("Acceleration", vector(0.0, 6.0, 0.0)),
            ("Size", number(2.0)),
            ("SizeEnd", number(0.5)),
            ("Color", color(1.0, 0.75, 0.25)),
            ("ColorEnd", color(0.9, 0.15, 0.05)),
            ("LightEmission", number(1.0)),
            ("Brightness", number(2.0)),
        ],
        ParticleType::Star => vec![
            ("Rate", number(6.0)),
            ("Lifetime", number(3.0)),
            ("Speed", number(2.0)),
            ("SpreadAngle", number(90.0)),
            ("Size", number(0.8)),
            ("SizeEnd", number(0.8)),
            ("Color", color(1.0, 1.0, 0.8)),
            ("ColorEnd", color(1.0, 0.85, 0.3)),
            ("RotSpeed", number(90.0)),
            ("LightEmission", number(0.7)),
            ("Brightness", number(1.5)),
        ],
        ParticleType::Magic => vec![
            ("Rate", number(25.0)),
            ("Lifetime", number(1.5)),
            ("Speed", number(1.5)),
            ("SpreadAngle", number(360.0)),
            ("Size", number(0.7)),
            ("SizeEnd", number(0.0)),
            ("Color", color(0.75, 0.4, 1.0)),
            ("ColorEnd", color(0.3, 0.9, 1.0)),
            ("RotSpeed", number(45.0)),
            ("LightEmission", number(1.0)),
            ("Brightness", number(2.0)),
        ],
    });
    properties
}

fn to_bytes(color: Color3) -> [u8; 3] {
    [color.r, color.g, color.b].map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8)
}

fn from_bytes(bytes: [u8; 3]) -> Color3 {
    let [r, g, b] = bytes.map(|b| b as f32 / 255.0);
    Color3::new(r, g, b)
}

/// The light Instances of `class` directly inside `parts`.
pub fn lights_in(world: &World, parts: &[InstanceId], class: ClassName) -> Vec<InstanceId> {
    parts
        .iter()
        .filter_map(|&part| world.get(part))
        .flat_map(|part| part.children().iter().copied())
        .filter(|&child| world.get(child).is_some_and(|c| c.class() == class))
        .collect()
}

/// The undo step's name for setting `property` on `count` parts, the last
/// called `name`, such as "Paint Pad" or "Anchor 3 parts".
pub fn step_label(property: &str, value: &Value, count: usize, name: &str) -> String {
    let verb = match (property, value) {
        ("Color", _) => "Paint",
        ("Anchored", Value::Bool(true)) => "Anchor",
        ("Anchored", _) => "Unanchor",
        ("CanCollide", Value::Bool(true)) => "Make solid",
        ("CanCollide", _) => "Make passable",
        ("Emission" | "EmissiveColor", _) => "Change glow of",
        _ => return what(&format!("Change {property} of"), count, name),
    };
    what(verb, count, name)
}

fn what(verb: &str, count: usize, name: &str) -> String {
    match count {
        1 => format!("{verb} {name}"),
        n => format!("{verb} {n} parts"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_round_trip_through_bytes() {
        for rgb in PALETTE {
            assert_eq!(to_bytes(from_bytes(rgb)), rgb);
        }
    }

    #[test]
    fn steps_are_named_like_f3x_actions() {
        let yes = Value::Bool(true);
        assert_eq!(step_label("Color", &yes, 1, "Pad"), "Paint Pad");
        assert_eq!(step_label("Anchored", &yes, 3, "Pad"), "Anchor 3 parts");
        assert_eq!(
            step_label("Anchored", &Value::Bool(false), 1, "Pad"),
            "Unanchor Pad"
        );
        assert_eq!(
            step_label("Transparency", &Value::Number(0.5), 2, "Pad"),
            "Change Transparency of 2 parts"
        );
    }

    #[test]
    fn every_tool_panel_draws() {
        let mut world = World::new();
        let part = world.create(ClassName::Part).unwrap();
        world.set_parent(part, Some(world.workspace())).unwrap();
        let light = world.create(ClassName::SpotLight).unwrap();
        world.set_parent(light, Some(part)).unwrap();
        let ctx = theme::test_context();
        let mut settings = BuildSettings::default();
        let mut modes = Modes::default();
        let mut snap = Snap::default();
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                for tool in Tool::ALL {
                    let scene = PanelScene {
                        world: &world,
                        parts: &[part],
                    };
                    let actions = panel(ui, tool, &scene, &mut settings, &mut modes, &mut snap);
                    assert!(actions.is_empty());
                }
            });
        });
        assert_eq!(lights_in(&world, &[part], ClassName::SpotLight), [light]);
        // Every preset only sets properties an emitter has.
        let emitter = world.create(ClassName::ParticleEmitter).unwrap();
        for kind in ParticleType::ALL {
            for (property, value) in particle_preset(kind) {
                world.set_property(emitter, property, value).unwrap();
            }
        }
        assert!(lights_in(&world, &[part], ClassName::PointLight).is_empty());
    }
}
