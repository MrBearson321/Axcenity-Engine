//! The Properties panel: the selected Instance's name, class and properties,
//! in groups like Roblox Studio's, each with a control that fits its type.
//!
//! The panel never changes the world itself. [`show`] returns the [`Edit`]
//! the user made, and the app applies it, which keeps the panel easy to test
//! and gives one place to notice unsaved changes.

use axcenity_core::{
    ClassName, Color3, Instance, InstanceId, Material, PartType, ParticleType, TextureChannel,
    Value, World,
};
use eframe::egui;

use crate::theme::{self, icons};

/// A change made in the Properties panel.
#[derive(Clone, Debug, PartialEq)]
pub enum Edit {
    Name(String),
    Property(&'static str, Value),
    /// Asks for a texture file for this map property (such as a
    /// SurfaceAppearance's `ColorMap`) with the file picker. The app does
    /// that, and sets the property.
    PickTexture(&'static str),
    /// Asks for an audio file for the Sound's `SoundId`, like [`Edit::PickTexture`].
    PickAudio,
    /// Asks for an animator controller file for the Animator's `Controller`.
    PickController,
    /// Plays the selected Sound in the editor, to hear it without pressing
    /// Play, or (false) stops that.
    PreviewSound(bool),
}

/// One group of properties in the panel, such as "Appearance".
#[derive(Debug, PartialEq)]
pub struct Section {
    pub title: &'static str,
    pub properties: Vec<(&'static str, Value)>,
}

/// The groups, in panel order, and which properties go in each. Name and
/// ClassName come first, under "Data", and aren't in the property map.
const GROUPS: [(&str, &[&str]); 15] = [
    (
        "Appearance",
        &[
            "Color",
            "Material",
            "Metalness",
            "Roughness",
            "Transparency",
            "EmissiveColor",
            "Emission",
            "CastShadow",
        ],
    ),
    ("Behavior", &["Anchored", "CanCollide", "WindAffected"]),
    ("Wind", &["GlobalWind"]),
    (
        "Part",
        &["Shape", "MeshId", "Position", "Orientation", "Size"],
    ),
    (
        "Surface",
        &[
            "MaterialName",
            "ColorMap",
            "NormalMap",
            "MetalnessMap",
            "RoughnessMap",
            "EmissiveMap",
            "AmbientOcclusionMap",
            "OpacityMap",
            "SpecularMap",
        ],
    ),
    (
        "Map Settings",
        &[
            "NormalStrength",
            "NormalMapDirectX",
            "MetalnessChannel",
            "RoughnessChannel",
            "InvertRoughness",
            "AmbientOcclusionChannel",
            "AmbientOcclusionStrength",
            "OpacityChannel",
            "InvertOpacity",
            "EmissiveStrength",
        ],
    ),
    (
        "UI",
        &[
            "Class",
            "Id",
            "Enabled",
            "Visible",
            "Position",
            "Size",
            "BackgroundColor3",
            "BackgroundTransparency",
            "Text",
            "Placeholder",
            "TextColor3",
            "TextSize",
            "MultiLine",
            "Image",
            "ImageTransparency",
            "CurrentPage",
            "Columns",
            "StudsOffset",
            "MaxDistance",
            "ScaleMode",
            "DesignWidth",
            "DesignHeight",
        ],
    ),
    (
        "Audio",
        &["SoundId", "Playing", "Volume", "PlaybackSpeed", "Looped"],
    ),
    ("Animation", &["Controller", "Enabled", "Speed"]),
    (
        "Shadows",
        &["ShadowsEnabled", "ShadowDistance", "ShadowSoftness"],
    ),
    (
        "Sky",
        &[
            "SkyTopColor",
            "SkyHorizonColor",
            "GroundColor",
            "EnvironmentIntensity",
        ],
    ),
    (
        "Post-processing",
        &[
            "PostProcessingEnabled",
            "Exposure",
            "BloomEnabled",
            "BloomIntensity",
            "BloomThreshold",
        ],
    ),
    (
        "Light",
        &["Enabled", "Brightness", "Range", "Angle", "Direction"],
    ),
    (
        "Emitter",
        &[
            "Enabled",
            "ParticleType",
            "Texture",
            "Rate",
            "Lifetime",
            "Speed",
            "Direction",
            "SpreadAngle",
            "Acceleration",
            "Drag",
            "WindAffectsDrag",
        ],
    ),
    (
        "Particles",
        &[
            "Size",
            "SizeEnd",
            "Color",
            "ColorEnd",
            "Transparency",
            "TransparencyEnd",
            "Rotation",
            "RotSpeed",
            "LightEmission",
            "Brightness",
        ],
    ),
];

/// True for a property that names a texture file, such as `ColorMap`.
pub fn is_map(name: &str) -> bool {
    (name.ends_with("Map") && name != "NormalMapDirectX") || name == "Texture"
}
/// Where properties that no group lists go, so a new property always shows.
const OTHER: &str = "Other";

/// The smallest a part can be on any axis, the same as Roblox. Zero would
/// leave nothing to see or click. The Scale tool stops here too.
pub const MIN_SIZE: f32 = 0.001;
/// Numbers show at most this many decimals, so a typed 0.3 doesn't show up
/// as 0.30000001 (the closest a 32-bit float gets).
const MAX_DECIMALS: usize = 3;

/// `instance`'s properties grouped into sections, in panel order. Empty
/// sections are left out.
pub fn sections(instance: &Instance) -> Vec<Section> {
    let mut sections: Vec<Section> = GROUPS
        .iter()
        .filter(|(title, _)| match *title {
            "Part" => matches!(instance.class(), ClassName::Part | ClassName::MeshPart),
            "Surface" | "Map Settings" => instance.class() == ClassName::SurfaceAppearance,
            "UI" => crate::style_panel::is_styled(instance.class()),
            "Audio" => instance.class() == ClassName::Sound,
            "Shadows" | "Sky" | "Post-processing" => instance.class() == ClassName::Lighting,
            "Light" => matches!(
                instance.class(),
                ClassName::PointLight | ClassName::SpotLight
            ),
            "Emitter" | "Particles" => instance.class() == ClassName::ParticleEmitter,
            "Animation" => instance.class() == ClassName::Animator,
            "Appearance" | "Behavior" => instance.class() != ClassName::ParticleEmitter,
            _ => true,
        })
        .map(|&(title, names)| Section {
            title,
            properties: names
                .iter()
                .filter_map(|&name| Some((name, instance.property(name)?.clone())))
                .collect(),
        })
        .collect();
    let other = instance
        .properties()
        .filter(|(name, _)| !GROUPS.iter().any(|(_, names)| names.contains(name)))
        .map(|(name, value)| (name, value.clone()))
        .collect();
    sections.push(Section {
        title: OTHER,
        properties: other,
    });
    sections.retain(|section| !section.properties.is_empty());
    sections
}

/// Whether the Properties panel lets you rename this Instance. Services keep
/// their names, and a Script is named after its file, so renaming it here
/// would be lost on the next load.
pub fn can_rename(instance: &Instance) -> bool {
    !instance.class().is_service()
}

/// Whether the Name box in this panel can rename `instance`. Renaming a
/// script renames its file, which shouldn't happen at every key typed, so
/// Scripts are renamed in the Explorer instead, all at once.
pub fn can_rename_here(instance: &Instance) -> bool {
    can_rename(instance) && !instance.class().is_script()
}

/// Draws the panel for `selected` and returns the edit made this frame, if any.
/// While `playing`, `world` is the game's, and the panel says that changes
/// last only until Stop. `more` draws further sections (the Style section)
/// at the end, in the same scrolling area.
pub fn show(
    ui: &mut egui::Ui,
    world: &World,
    selected: Option<InstanceId>,
    playing: bool,
    selection_count: usize,
    more: impl FnOnce(&mut egui::Ui),
) -> Option<Edit> {
    if playing {
        ui.weak(theme::with_icon(
            icons::INFO,
            "Changes made during Play are undone at Stop.",
        ));
    }
    if selection_count > 1 {
        ui.weak(format!(
            "{selection_count} selected. Properties are for the last one clicked. Delete and Duplicate apply to all of them."
        ));
    }
    ui.separator();
    let mut edit = None;
    // The scroll area fills the panel even when nothing is selected. Without
    // it the panel would shrink to fit one line of text and stay that small.
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            let Some((id, instance)) = selected.and_then(|id| Some((id, world.get(id)?))) else {
                ui.weak("Select something in the Explorer or the viewport to see its properties.");
                return;
            };
            // Widgets remember things such as half-typed numbers by ID. Mixing in
            // the selection gives each Instance its own, so nothing typed for one
            // part turns up on the next one selected.
            ui.push_id(id, |ui| {
                section(ui, "Data", |ui| {
                    ui.label("Name");
                    let mut name = instance.name().to_string();
                    if can_rename_here(instance) {
                        if ui.text_edit_singleline(&mut name).changed() {
                            edit = Some(Edit::Name(name));
                        }
                    } else {
                        let hint = if instance.class().is_script() {
                            "Rename a script in the Explorer (F2). That renames its file too."
                        } else {
                            "Services can't be renamed."
                        };
                        ui.add_enabled(false, egui::TextEdit::singleline(&mut name))
                            .on_disabled_hover_text(hint);
                    }
                    ui.end_row();
                    ui.label("ClassName");
                    ui.label(instance.class().as_str());
                    ui.end_row();
                });

                for group in sections(instance) {
                    section(ui, group.title, |ui| {
                        for (name, value) in group.properties {
                            ui.label(name);
                            if let Some(made) = value_control(ui, name, value) {
                                edit = Some(made);
                            }
                            ui.end_row();
                        }
                    });
                }
                if instance.class() == ClassName::ScreenGui
                    && instance
                        .property("ScaleMode")
                        .and_then(Value::as_str)
                        .is_some_and(|mode| mode.eq_ignore_ascii_case("Fit"))
                {
                    ui.weak(
                        "Fit lays this ScreenGui out at DesignWidth × DesignHeight, then scales \
                         it uniformly so it stays proportional and inside the screen.",
                    );
                }
                more(ui);
            });
        });
    edit
}

/// A collapsible group with a two-column grid of names and controls.
fn section(ui: &mut egui::Ui, title: &str, rows: impl FnOnce(&mut egui::Ui)) {
    let icon = match title {
        "Data" => icons::IDENTIFICATION_CARD,
        "Appearance" => icons::PALETTE,
        "Behavior" => icons::GEAR,
        "Part" => icons::CUBE,
        "Surface" => icons::IMAGE,
        "Map Settings" => icons::SLIDERS,
        "UI" => icons::LAYOUT,
        "Audio" => icons::SPEAKER_HIGH,
        "Shadows" => icons::SUN,
        "Sky" => icons::CLOUD_SUN,
        "Post-processing" => icons::SPARKLE,
        "Light" => icons::LIGHTBULB,
        _ => icons::DOTS_THREE_OUTLINE,
    };
    let header = egui::RichText::new(theme::with_icon(icon, title)).family(theme::heading_family());
    egui::CollapsingHeader::new(header)
        .id_salt(title)
        .default_open(true)
        .show(ui, |ui| {
            egui::Grid::new(title)
                .num_columns(2)
                .striped(true)
                .spacing([12.0, 4.0])
                .show(ui, rows);
        });
}

/// Draws the control for one property and returns the edit if the user
/// changed it this frame.
fn value_control(ui: &mut egui::Ui, name: &'static str, value: Value) -> Option<Edit> {
    match value {
        Value::String(path) if is_map(name) => map_control(ui, name, &path),
        Value::String(path) if name == "SoundId" => sound_control(ui, &path),
        Value::String(path) if name == "Controller" => controller_control(ui, &path),
        value => value_edit(ui, name, value).map(|value| Edit::Property(name, value)),
    }
}

/// A texture map's file, with buttons to pick another and to clear it.
/// Typing isn't offered: each half-typed path would be loaded, and fail.
fn map_control(ui: &mut egui::Ui, name: &'static str, path: &str) -> Option<Edit> {
    let mut edit = None;
    ui.horizontal(|ui| {
        if ui
            .small_button(icons::FOLDER_OPEN)
            .on_hover_text("Choose a texture file (PNG, JPEG, TGA or BMP)")
            .clicked()
        {
            edit = Some(Edit::PickTexture(name));
        }
        if path.is_empty() {
            ui.weak("None");
            return;
        }
        if ui
            .small_button(icons::X)
            .on_hover_text("Use no map")
            .clicked()
        {
            edit = Some(Edit::Property(name, Value::String(String::new())));
        }
        let file = path.rsplit('/').next().unwrap_or(path);
        ui.label(file).on_hover_text(path);
    });
    edit
}

/// An Animator's controller file, with buttons to pick another and to clear it.
fn controller_control(ui: &mut egui::Ui, path: &str) -> Option<Edit> {
    let mut edit = None;
    ui.horizontal(|ui| {
        if ui
            .small_button(icons::FOLDER_OPEN)
            .on_hover_text("Choose an animator controller (.controller.json) in the project")
            .clicked()
        {
            edit = Some(Edit::PickController);
        }
        if path.is_empty() {
            ui.weak("None");
            return;
        }
        if ui
            .small_button(icons::X)
            .on_hover_text("Play no controller")
            .clicked()
        {
            edit = Some(Edit::Property("Controller", Value::String(String::new())));
        }
        let file = path.rsplit('/').next().unwrap_or(path);
        ui.label(file).on_hover_text(path);
    });
    edit
}

/// A Sound's file, with buttons to pick another, and to hear it.
fn sound_control(ui: &mut egui::Ui, path: &str) -> Option<Edit> {
    let mut edit = None;
    ui.horizontal(|ui| {
        if ui
            .small_button(icons::FOLDER_OPEN)
            .on_hover_text("Choose a sound file (WAV, OGG, MP3 or FLAC)")
            .clicked()
        {
            edit = Some(Edit::PickAudio);
        }
        if path.is_empty() {
            ui.weak("None");
            return;
        }
        if ui
            .small_button(icons::PLAY)
            .on_hover_text("Listen to it here, without pressing Play")
            .clicked()
        {
            edit = Some(Edit::PreviewSound(true));
        }
        if ui
            .small_button(icons::STOP)
            .on_hover_text("Stop listening")
            .clicked()
        {
            edit = Some(Edit::PreviewSound(false));
        }
        let file = path.rsplit('/').next().unwrap_or(path);
        ui.label(file).on_hover_text(path);
    });
    edit
}

/// Draws the control for a property's value and returns its new value if
/// the user changed it this frame.
fn value_edit(ui: &mut egui::Ui, name: &str, value: Value) -> Option<Value> {
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
        Value::Number(mut number) => {
            let mut drag = egui::DragValue::new(&mut number).max_decimals(MAX_DECIMALS);
            drag = match name {
                // These only make sense from 0 to 1.
                "Metalness"
                | "Roughness"
                | "AmbientOcclusionStrength"
                | "BackgroundTransparency"
                | "Volume" => drag.speed(0.01).range(0.0..=1.0),
                "PlaybackSpeed" => drag.speed(0.01).range(0.01..=20.0),
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
        Value::String(text) if name == "ScaleMode" => {
            let current = if text.eq_ignore_ascii_case("Fit") {
                "Fit"
            } else {
                "Fixed"
            };
            choice_control(ui, name, current, &["Fixed", "Fit"], |mode| mode)
                .map(|mode| Value::String(mode.to_owned()))
        }
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
            ui.horizontal(|ui| {
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
        Value::UDim2(mut value) => {
            let mut changed = false;
            ui.horizontal(|ui| {
                for (prefix, component) in [("X", &mut value.x), ("Y", &mut value.y)] {
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
    }
}

/// A color swatch that opens a color picker, with the 0–255 values beside it.
fn color_control(ui: &mut egui::Ui, color: Color3) -> Option<Color3> {
    // `Color3` is sRGB, but egui's float color picker works in linear light,
    // so convert there and back.
    use egui::ecolor::{gamma_from_linear, linear_from_gamma};
    let mut linear = [color.r, color.g, color.b].map(linear_from_gamma);
    let mut picked = None;
    ui.horizontal(|ui| {
        if ui.color_edit_button_rgb(&mut linear).changed() {
            let [r, g, b] = linear.map(gamma_from_linear);
            picked = Some(Color3::new(r, g, b));
        }
        let shown = picked.unwrap_or(color);
        let byte = |channel: f32| (channel * 255.0).round() as u8;
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
fn choice_control<T: Copy + PartialEq>(
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

#[cfg(test)]
mod tests {
    use super::*;
    use axcenity_core::ClassName;

    fn titles(sections: &[Section]) -> Vec<&'static str> {
        sections.iter().map(|section| section.title).collect()
    }

    #[test]
    fn part_properties_are_grouped_like_studio_and_none_are_left_out() {
        let mut world = World::new();
        let part = world.create(ClassName::Part).unwrap();
        let instance = world.get(part).unwrap();

        let sections = sections(instance);
        assert_eq!(titles(&sections), ["Appearance", "Behavior", "Part"]);
        let names: Vec<&str> = sections
            .iter()
            .flat_map(|section| section.properties.iter().map(|(name, _)| *name))
            .collect();
        assert_eq!(
            names,
            [
                "Color",
                "Material",
                "Metalness",
                "Roughness",
                "Transparency",
                "EmissiveColor",
                "Emission",
                "CastShadow",
                "Anchored",
                "CanCollide",
                "WindAffected",
                "Shape",
                "Position",
                "Orientation",
                "Size"
            ]
        );
        // Every property shows exactly once, with its current value.
        assert_eq!(names.len(), instance.properties().count());
        for section in &sections {
            for (name, value) in &section.properties {
                assert_eq!(instance.property(name), Some(value));
            }
        }
    }

    #[test]
    fn ui_properties_only_appear_in_the_ui_section_for_ui_instances() {
        let mut world = World::new();
        let frame = world.create(ClassName::Frame).unwrap();
        let sections = sections(world.get(frame).unwrap());

        assert_eq!(titles(&sections), ["UI"]);
        assert!(
            sections[0]
                .properties
                .iter()
                .any(|(name, _)| *name == "Position")
        );
    }

    #[test]
    fn surface_appearances_show_their_maps_then_how_they_are_read() {
        let mut world = World::new();
        let look = world.create(ClassName::SurfaceAppearance).unwrap();
        let sections = sections(world.get(look).unwrap());
        assert_eq!(titles(&sections), ["Appearance", "Surface", "Map Settings"]);
        let maps: Vec<&str> = sections[1]
            .properties
            .iter()
            .map(|(name, _)| *name)
            .filter(|name| is_map(name))
            .collect();
        assert_eq!(maps.len(), 8);
        assert!(!is_map("NormalMapDirectX") && !is_map("MaterialName"));
    }

    #[test]
    fn the_workspace_shows_only_its_wind_and_other_services_nothing() {
        let world = World::new();
        let workspace = world.get(world.workspace()).unwrap();
        let sections = sections(workspace);
        assert_eq!(titles(&sections), ["Wind"]);
        assert_eq!(sections[0].properties[0].0, "GlobalWind");
        let storage = world.get(world.replicated_storage()).unwrap();
        assert!(super::sections(storage).is_empty());
    }

    #[test]
    fn parts_can_be_renamed_here_scripts_only_in_the_explorer_and_services_never() {
        let mut world = World::new();
        let part = world.create(ClassName::Part).unwrap();
        let script = world.create(ClassName::Script).unwrap();
        assert!(can_rename_here(world.get(part).unwrap()));
        assert!(can_rename(world.get(script).unwrap()));
        assert!(!can_rename_here(world.get(script).unwrap()));
        assert!(!can_rename(world.get(world.workspace()).unwrap()));
        assert!(!can_rename(world.get(world.root()).unwrap()));
    }
}
