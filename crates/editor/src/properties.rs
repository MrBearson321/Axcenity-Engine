//! The Properties panel: the selected Instance's name, class and properties,
//! in groups like Roblox Studio's, each with a control that fits its type.
//!
//! The panel never changes the world itself. [`show`] returns the [`Edit`]
//! the user made, and the app applies it, which keeps the panel easy to test
//! and gives one place to notice unsaved changes.

use std::collections::HashMap;

use axcenity_core::{
    CFrame, ClassName, Color3, EnumItem, Instance, InstanceId, Material, PartType, ParticleType,
    PhysicalProperties, TextureChannel, ToneMapping, UDim2, Value, Vector2, Vector3, World,
    is_attribute_name,
};

use axcenity_assets::{Assets, is_asset_reference};
use eframe::egui;

use crate::history::Field;
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
    /// Asks for a lookup table or palette file for this ColorCorrectionEffect
    /// property (`LookupTable` or `Palette`), like [`Edit::PickTexture`].
    PickGradingFile(&'static str),
    /// Asks for an audio file for the Sound's `SoundId`, like [`Edit::PickTexture`].
    PickAudio,
    /// Asks for a state machine file for the Animator's `StateMachine`.
    PickController,
    /// Opens the Animator's state machine in the Animation tab.
    OpenController,
    /// Asks for a clip file for an Animation's `Clip`.
    PickClip,
    /// Opens the Animation's clip in the Animation tab.
    OpenClip,
    /// Asks for a behavior tree file for a NavigationAgent's `BehaviorTree`.
    PickBehaviorTree,
    /// Opens the agent's behavior tree in the Behavior Tree tab.
    OpenBehaviorTree,
    /// Asks for the code file of a script inside an object (its
    /// `SourceFile`, G2), with the ending its class needs.
    PickSourceFile,
    /// Opens the script's `SourceFile` in a tab.
    OpenSourceFile,
    /// Rename or move the file a script inside an object takes its code
    /// from, pointing every script that uses it at the new path.
    MoveSourceFile,
    /// Asks for a sky image file for Lighting's `SkyImage` with the file
    /// picker, then asks whether to use it.
    PickSkyImage,
    /// Asks whether to use the sky image at this project path (dragged onto
    /// the row from the Assets panel).
    AskSkyImage(String),
    /// Animates the selected Part, Model or UI element: an Animator, a new
    /// clip and an Animation playing it, opened in the Animation tab.
    Animate,
    /// Plays the selected Sound in the editor, to hear it without pressing
    /// Play, or (false) stops that.
    PreviewSound(bool),
    /// Makes the selected Ragdoll's bodies and joints from its MeshPart's
    /// skeleton, replacing what was in it.
    BuildRagdoll,
    /// Cuts the selected Fracture's part into its pieces and saves them.
    GenerateFracture,
    /// Turns what the selected SplineMesh or SplineScatter made into
    /// ordinary Instances (Phase 74).
    BakeSpline,
    /// Flattens the Terrain under the selected Spline (or the one the
    /// selection is in), road-wide (Phase 74).
    FlattenTerrainAlong,
    /// Opens the selected MeshPart's model in the Model Studio.
    OpenInModelStudio,
    /// During Play, pins (true) or unpins a value edited during Play, so
    /// Stop copies it into the edit world (see `play::kept`).
    Keep(Field, bool),
    /// Sets an attribute, or removes it (`None`).
    Attribute(String, Option<Value>),
    AddTag(String),
    RemoveTag(String),
    /// Sets this overridden property of a placed prefab copy back to the
    /// prefab's value (Phase 49).
    RevertOverride(String),
    /// Writes this overridden property into the copy's prefab.
    ApplyOverride(String),
    /// Sets this property of each selected Instance back to its class's
    /// default (↺ with several selected).
    Reset(&'static str),
}

/// One group of properties in the panel, such as "Appearance".
#[derive(Debug, PartialEq)]
pub struct Section {
    pub title: &'static str,
    pub properties: Vec<(&'static str, Value)>,
}

/// The groups, in panel order, and which properties go in each. Name and
/// ClassName come first, under "Data", and aren't in the property map.
const GROUPS: [(&str, &[&str]); 35] = [
    (
        "Appearance",
        &[
            "Color",
            "Material",
            "MaterialVariant",
            "BlueprintTexture",
            "Metalness",
            "Roughness",
            "Transparency",
            "EmissiveColor",
            "Emission",
            "CastShadow",
        ],
    ),
    (
        "Behavior",
        &[
            "Locked",
            "Anchored",
            "CanCollide",
            "CanTouch",
            "CanQuery",
            "CollisionGroup",
            "WindAffected",
        ],
    ),
    (
        "Physics",
        &[
            "CustomPhysicalProperties",
            "Massless",
            "AssemblyLinearVelocity",
            "AssemblyAngularVelocity",
            "Gravity",
            "PhysicsInterpolation",
        ],
    ),
    ("Wind", &["GlobalWind"]),
    // The Workspace's cap on `SpawnDecal`'s decals (Phase 75).
    ("Decals", &["MaxSpawnedDecals"]),
    // The Workspace's world streaming (Phase 80).
    (
        "Streaming",
        &[
            "StreamingEnabled",
            "StreamingMinRadius",
            "StreamingTargetRadius",
            "ModelStreamingMode",
        ],
    ),
    // A Decal's or ProjectedDecal's picture, and a ProjectedDecal's box.
    (
        "Decal",
        &[
            "Texture",
            "Face",
            "Color3",
            "StudsPerTileU",
            "StudsPerTileV",
            "FadeAngle",
            "IgnoreCharacters",
        ],
    ),
    ("Box", &["Position", "Orientation", "Size"]),
    (
        "Part",
        &["Shape", "MeshId", "Position", "Orientation", "Size"],
    ),
    ("Terrain", &["Position", "Size", "DataFolder"]),
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
            "HeightMap",
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
        "Tiling",
        &[
            "TileSize",
            "Tiling",
            "Offset",
            "TextureRotation",
            "Triplanar",
            "AntiTiling",
            "HeightBlend",
        ],
    ),
    (
        "Auto Material",
        &[
            "AutoEnabled",
            "MinHeight",
            "MaxHeight",
            "HeightFade",
            "MinSlope",
            "MaxSlope",
            "SlopeFade",
            "RuleStrength",
            "RuleNoise",
            "NoiseScale",
        ],
    ),
    (
        "UI",
        &[
            "Class",
            "Id",
            "Enabled",
            "Visible",
            "Interactable",
            "Tooltip",
            "Selectable",
            "NextSelectionUp",
            "NextSelectionDown",
            "NextSelectionLeft",
            "NextSelectionRight",
            "AccessibleName",
            "HoverSound",
            "ClickSound",
            "Position",
            "Size",
            "Rotation",
            "BackgroundColor3",
            "BackgroundTransparency",
            "Text",
            "TextKey",
            "Placeholder",
            "TextColor3",
            "TextTransparency",
            "TextSize",
            "TextScaled",
            "RichText",
            "MultiLine",
            "Image",
            "ImageTransparency",
            "CurrentPage",
            "Columns",
            "StudsOffset",
            "MaxDistance",
            "ScaleMode",
            "ScreenInsets",
            "DesignWidth",
            "DesignHeight",
            "ZIndex",
            "DisplayOrder",
            "CanvasSize",
            "CanvasPosition",
            "ScrollingDirection",
            "ScrollBarThickness",
            "AutomaticCanvasSize",
        ],
    ),
    (
        "Audio",
        &[
            "SoundId",
            "Playing",
            "Volume",
            "PlaybackSpeed",
            "Looped",
            "Spatial",
            "Position",
            "RollOffMinDistance",
            "RollOffMaxDistance",
        ],
    ),
    (
        "Animation",
        &[
            "StateMachine",
            "Clip",
            "AutoPlay",
            "Enabled",
            "Speed",
            "Looped",
        ],
    ),
    ("Time of Day", &["ClockTime", "GeographicLatitude"]),
    (
        "Shadows",
        &[
            "ShadowsEnabled",
            "ShadowDistance",
            "ShadowSoftness",
            "AmbientOcclusionIntensity",
        ],
    ),
    (
        "Sky",
        &[
            "SkyTopColor",
            "SkyHorizonColor",
            "GroundColor",
            "EnvironmentIntensity",
            "SkyImage",
        ],
    ),
    ("Fog", &["FogColor", "FogStart", "FogEnd"]),
    (
        "Atmosphere",
        &["Density", "Offset", "Decay", "Glare", "Haze"],
    ),
    ("Clouds", &["Cover", "Density", "Speed", "Direction"]),
    (
        "Weather",
        &[
            "Precipitation",
            "Intensity",
            "Wetness",
            "SnowCover",
            "TransitionTime",
        ],
    ),
    ("Outlines", &["Enabled", "Thickness"]),
    (
        "Color Correction",
        &[
            "Enabled",
            "Brightness",
            "Contrast",
            "Saturation",
            "TintColor",
            "LookupTable",
            "Posterize",
            "Palette",
        ],
    ),
    (
        "Lens",
        &[
            "Enabled",
            "FocusDistance",
            "InFocusRadius",
            "NearIntensity",
            "FarIntensity",
            "Intensity",
            "Size",
            "Color",
        ],
    ),
    ("Graphics Volume", &["Enabled", "BlendDistance", "Priority"]),
    (
        "Effect",
        &[
            "Enabled",
            "Exposure",
            "Start",
            "End",
            "Intensity",
            "Threshold",
        ],
    ),
    (
        "Post-processing",
        &[
            "PostProcessingEnabled",
            "Exposure",
            "ToneMapping",
            "BloomEnabled",
            "BloomIntensity",
            "BloomThreshold",
        ],
    ),
    (
        "Light",
        &[
            "Enabled",
            "Brightness",
            "Range",
            "Angle",
            "Width",
            "Height",
            "Direction",
            "Shadows",
        ],
    ),
    // Where a light or emitter stands when it isn't in a part.
    ("Placement", &["Position", "Orientation"]),
    (
        "Emitter",
        &[
            "Locked",
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
    (
        "Movement",
        &[
            "Movement",
            "WalkSpeed",
            "RunSpeed",
            "JumpHeight",
            "Jump",
            "MaxSlopeAngle",
            "MaxStepHeight",
            "Crouch",
            "CrouchHeight",
            "CrouchSpeedScale",
            "CameraDistance",
            "CameraMinZoomDistance",
            "CameraMaxZoomDistance",
            "CameraHeight",
            "EyeHeight",
        ],
    ),
];

/// True for a property that names a texture file, such as `ColorMap`.
pub fn is_map(name: &str) -> bool {
    (name.ends_with("Map") && name != "NormalMapDirectX") || name == "Texture"
}
/// Where properties that no group lists go, so a new property always shows.
const OTHER: &str = "Other";

pub use crate::gizmo::MIN_SIZE;
/// Numbers show at most this many decimals, so a typed 0.3 doesn't show up
/// as 0.30000001 (the closest a 32-bit float gets).
const MAX_DECIMALS: usize = 3;
/// The width a UDim2's two axes need side by side.
const UDIM2_ONE_ROW: f32 = 270.0;

/// `instance`'s properties grouped into sections, in panel order. Empty
/// sections are left out.
pub fn sections(instance: &Instance) -> Vec<Section> {
    let mut sections: Vec<Section> = GROUPS
        .iter()
        .filter(|(title, _)| match *title {
            "Part" => instance.class().is_base_part(),
            "Surface" | "Map Settings" => matches!(
                instance.class(),
                ClassName::SurfaceAppearance | ClassName::TerrainLayer
            ),
            "Terrain" => instance.class() == ClassName::Terrain,
            "Decal" => matches!(
                instance.class(),
                ClassName::Decal | ClassName::ProjectedDecal
            ),
            "Box" => instance.class() == ClassName::ProjectedDecal,
            "Tiling" => matches!(
                instance.class(),
                ClassName::Part | ClassName::TerrainLayer | ClassName::SurfaceAppearance
            ),
            "Auto Material" => instance.class() == ClassName::TerrainLayer,
            "UI" => crate::style_panel::is_styled(instance.class()),
            "Audio" => instance.class() == ClassName::Sound,
            "Time of Day" | "Shadows" | "Sky" | "Fog" | "Post-processing" => {
                instance.class() == ClassName::Lighting
            }
            "Atmosphere" => instance.class() == ClassName::Atmosphere,
            "Clouds" => instance.class() == ClassName::Clouds,
            "Weather" => instance.class() == ClassName::Weather,
            "Outlines" => instance.class() == ClassName::OutlineEffect,
            "Color Correction" => instance.class() == ClassName::ColorCorrectionEffect,
            "Lens" => instance.class().is_lens_effect(),
            "Graphics Volume" => instance.class() == ClassName::GraphicsVolume,
            "Effect" => matches!(
                instance.class(),
                ClassName::ExposureEffect | ClassName::FogEffect | ClassName::BloomEffect
            ),
            "Light" => instance.class().is_light(),
            "Placement" => instance.class().can_stand_free(),
            "Emitter" | "Particles" => instance.class() == ClassName::ParticleEmitter,
            "Animation" => matches!(instance.class(), ClassName::Animator | ClassName::Animation),
            "Movement" => instance.class().is_character_controller(),
            // A lens effect's Color is in its own section.
            "Appearance" | "Behavior" => {
                instance.class() != ClassName::ParticleEmitter && !instance.class().is_lens_effect()
            }
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

/// The title of the section that holds rare settings.
const ADVANCED: &str = "Advanced";

/// Whether `name` on a `class` is a rare setting, shown in the Advanced
/// section instead of its group (the list Paul approved, 2026-11-10).
pub fn is_advanced(class: ClassName, name: &str) -> bool {
    let names: &[&str] = if class.is_base_part() {
        &[
            "MaterialVariant",
            "BlueprintTexture",
            "EmissiveColor",
            "Emission",
            "CanQuery",
            "CollisionGroup",
            "WindAffected",
            "CustomPhysicalProperties",
            "Massless",
            "AssemblyLinearVelocity",
            "AssemblyAngularVelocity",
            "TileSize",
        ]
    } else if crate::style_panel::is_styled(class) {
        &[
            "Class",
            "Id",
            "Interactable",
            "Selectable",
            "NextSelectionUp",
            "NextSelectionDown",
            "NextSelectionLeft",
            "NextSelectionRight",
            "AccessibleName",
            "TextKey",
            "RichText",
        ]
    } else if class.is_character_controller() {
        &[
            "MaxSlopeAngle",
            "MaxStepHeight",
            "CrouchHeight",
            "CrouchSpeedScale",
            "CameraMinZoomDistance",
            "CameraMaxZoomDistance",
            "FootPlacement",
            "MoveDirection",
            "PlatformStand",
            "Sit",
        ]
    } else {
        match class {
            ClassName::ParticleEmitter => {
                &["Drag", "WindAffectsDrag", "LightEmission", "Brightness"]
            }
            ClassName::Sound => &["RollOffMinDistance", "RollOffMaxDistance", "SoundGroup"],
            ClassName::Model => &["ModelStreamingMode"],
            _ => &[],
        }
    };
    names.contains(&name)
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
    can_rename(instance) && !crate::tree::is_file_script(instance)
}

/// Draws the panel for `selected` and returns the edit made this frame, if any.
/// While `playing`, `world` is the game's, and the panel says that changes
/// last only until Stop unless kept: `kept` lists the fields edited during
/// Play and whether each is kept, and each gets a Keep pin by its name.
/// `more` draws further sections (the Style section) at the end, in the
/// same scrolling area.
pub fn show(
    ui: &mut egui::Ui,
    world: &World,
    assets: &Assets,
    selected: Option<InstanceId>,
    playing: bool,
    kept: &[(Field, bool)],
    more: impl FnOnce(&mut egui::Ui),
) -> Option<Edit> {
    let palette = theme::palette(ui.ctx());
    if playing {
        notice(
            ui,
            icons::INFO,
            "Changes made during Play are undone at Stop, unless you keep them: click the pin \
             beside a value you changed, or right-click the object in the Explorer and pick \
             Keep changes.",
        );
    }
    let pin = |field: Field| {
        kept.iter()
            .find(|(edited, _)| *edited == field)
            .map(|(_, kept)| *kept)
    };
    let mut edit = None;
    // The scroll area fills the panel even when nothing is selected. Without
    // it the panel would shrink to fit one line of text and stay that small.
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            let Some((id, instance)) = selected.and_then(|id| Some((id, world.get(id)?))) else {
                crate::widgets::empty_state(
                    ui,
                    icons::CURSOR_CLICK,
                    "Nothing selected",
                    "Select something in the Explorer or the viewport to see its properties.",
                    None,
                );
                return;
            };
            // What's selected, so it's clear what the fields below change.
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(theme::class_icon(instance.class()))
                        .size(16.0)
                        .color(theme::class_color(instance.class(), ui.visuals())),
                );
                // The class keeps its room; a long name ends in "…".
                let class = ui.painter().layout_no_wrap(
                    instance.class().as_str().to_owned(),
                    egui::FontId::proportional(theme::type_scale::SMALL),
                    palette.text_weak,
                );
                let name_room = ui.available_width() - class.size().x - ui.spacing().item_spacing.x;
                ui.scope(|ui| {
                    ui.set_max_width(name_room.max(24.0));
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(instance.name())
                                .family(theme::heading_family())
                                .color(palette.text_strong),
                        )
                        .truncate(),
                    );
                });
                ui.label(class);
            });
            if !playing && crate::animation::can_animate(instance.class()) {
                let icon = theme::AnimKind::Animator.rich_icon(ui.visuals());
                if ui
                    .button((icon, format!("  Animate {}", instance.name())))
                    .on_hover_text(
                        "Give it an Animator, a new clip and an Animation playing it, and open the clip with Record on",
                    )
                    .clicked()
                {
                    edit = Some(Edit::Animate);
                }
            }
            if !playing
                && instance.class() == ClassName::Ragdoll
                && ui
                    .button(theme::with_icon(icons::PERSON_SIMPLE, "Build"))
                    .on_hover_text(
                        "Make a capsule for each main bone of the MeshPart's skeleton and a joint between them, with a humanoid's limits. Replaces what's in the Ragdoll.",
                    )
                    .clicked()
            {
                edit = Some(Edit::BuildRagdoll);
            }
            if !playing
                && instance.class() == ClassName::Fracture
                && ui
                    .button(theme::with_icon(icons::HAMMER, "Generate"))
                    .on_hover_text(
                        "Cut the part into Pieces convex pieces by Seed and Pattern, and save them as <name>.fracture.glb. Generate again after changing the part or these settings.",
                    )
                    .clicked()
            {
                edit = Some(Edit::GenerateFracture);
            }
            let generator = axcenity_core::spline::is_generator(instance.class());
            if !playing
                && generator
                && ui
                    .button(theme::with_icon(icons::PACKAGE, "Bake"))
                    .on_hover_text(
                        "Keep what it made as ordinary Instances, in a Model beside the Spline, and remove it. A SplineMesh's bent pieces are saved as <name>.spline.glb.",
                    )
                    .clicked()
            {
                edit = Some(Edit::BakeSpline);
            }
            if !playing
                && (generator || instance.class() == ClassName::Spline)
                && ui
                    .button(theme::with_icon(icons::MOUNTAINS, "Flatten Terrain Along"))
                    .on_hover_text(
                        "Flatten a strip of the Terrain under the spline to the curve's height, as wide as its Spline Mesh's road and a stud more each side (8 studs without one). One undo step.",
                    )
                    .clicked()
            {
                edit = Some(Edit::FlattenTerrainAlong);
            }
            let has_mesh = instance
                .property("MeshId")
                .and_then(Value::as_str)
                .is_some_and(|id| !id.is_empty());
            if !playing
                && instance.class() == ClassName::MeshPart
                && has_mesh
                && ui
                    .button(theme::with_icon(icons::POLYGON, "Edit in Model Studio"))
                    .on_hover_text("Edit its model's meshes in the Model Studio tab. Saving writes a .glb.")
                    .clicked()
            {
                edit = Some(Edit::OpenInModelStudio);
            }
            ui.add_space(2.0);
            // Filter properties: shows only the rows whose names (or whose
            // section's title) match, with their sections open. Kept from
            // one selection to the next, like Roblox Studio's.
            let filter_id = egui::Id::new("properties filter");
            let mut filter: String = ui.data_mut(|d| d.get_temp(filter_id)).unwrap_or_default();
            let width = (ui.available_width() - crate::widgets::SEARCH_FIELD_MARGINS).max(60.0);
            crate::widgets::search_field(ui, &mut filter, "Filter properties", width)
                .on_hover_text("Show only the properties whose names have this in them");
            ui.data_mut(|d| d.insert_temp(filter_id, filter.clone()));
            let filter = filter.trim();
            let filtering = !filter.is_empty();
            let open = filtering.then_some(true);
            let shows = |title: &str, name: &str| {
                !filtering || matches_filter(name, filter) || matches_filter(title, filter)
            };
            let mut shown_any = false;
            // Only `Locked` itself can be changed on something locked.
            let locked = !playing && world.is_locked(id);
            if locked {
                ui.weak(theme::with_icon(
                    icons::LOCK,
                    "Locked. Turn Locked off to change it.",
                ));
            }
            // Widgets remember things such as half-typed numbers by ID. Mixing in
            // the selection gives each Instance its own, so nothing typed for one
            // part turns up on the next one selected.
            ui.push_id(id, |ui| {
                let class = instance.class();
                // The header already says the class, so its own row only
                // shows when the filter asks for it.
                let data = [
                    shows("Data", "Name"),
                    filtering && shows("Data", "ClassName"),
                ];
                shown_any |= data.contains(&true);
                section(ui, "Data", open, data.contains(&true), |ui| {
                  if data[0] {
                    if let Some(keep) = field_name_with_pin(ui, class, "Name", pin(Field::Name)) {
                        edit = Some(Edit::Keep(Field::Name, keep));
                    }
                    let mut name = instance.name().to_string();
                    if can_rename_here(instance) && !locked {
                        if ui.text_edit_singleline(&mut name).changed() {
                            edit = Some(Edit::Name(name));
                        }
                    } else {
                        let hint = if locked {
                            "It's locked. Turn Locked off to rename it."
                        } else if instance.class().is_script() {
                            "Rename a script in the Explorer (F2). That renames its file too."
                        } else {
                            "Services can't be renamed."
                        };
                        ui.add_enabled(false, egui::TextEdit::singleline(&mut name))
                            .on_disabled_hover_text(hint);
                    }
                    ui.end_row();
                  }
                  if data[1] {
                    described_name(ui, class, "ClassName");
                    ui.label(instance.class().as_str());
                    ui.end_row();
                  }
                });

                // In a placed prefab copy, what it sets differently (Phase 49).
                let overridden = match playing {
                    true => Default::default(),
                    false => axcenity_core::prefab::place(world, id)
                        .map(|place| place.here().properties)
                        .unwrap_or_default(),
                };
                // What a new one of its class starts with, to offer ↺ on
                // what differs (not during Play, whose changes go at Stop).
                let defaults: HashMap<&str, Value> = if playing {
                    HashMap::new()
                } else {
                    instance.class().default_properties().into_iter().collect()
                };
                // One property's row: its name (with ↺, the prefab's ↺ or
                // Play's pin) and its control.
                let mut row = |ui: &mut egui::Ui, name: &'static str, value: Value| {
                    let field = Field::Property(name);
                    let reset = defaults
                        .get(name)
                        .filter(|default| resettable(default) && **default != value)
                        .filter(|_| !locked || name == "Locked");
                    if overridden.contains(name) {
                        if let Some(made) = override_name(ui, name) {
                            edit = Some(made);
                        }
                    } else if let Some(default) = reset {
                        if field_name_with_reset(ui, class, name, default) {
                            edit = Some(Edit::Property(name, default.clone()));
                        }
                    } else if let Some(keep) =
                        field_name_with_pin(ui, class, name, pin(field))
                    {
                        edit = Some(Edit::Keep(field, keep));
                    }
                    let enabled = !locked || name == "Locked";
                    let made = ui.add_enabled_ui(enabled, |ui| match value {
                        // Who sits there is up to the game, not the editor.
                        Value::Instance(target)
                            if name == "Occupant"
                                && instance.class().is_seat() =>
                        {
                            let who = target
                                .and_then(|target| world.full_name(target))
                                .unwrap_or_else(|| "None".to_owned());
                            ui.label(who).on_hover_text(
                                "The controller sitting here during Play: set by \
                                 walking into the seat or by seat:Sit(controller).",
                            );
                            None
                        }
                        // An ObjectValue's Value picks from the world.
                        Value::Instance(target) => object_control(ui, world, id, target)
                            .map(|target| Edit::Property(name, Value::Instance(target))),
                        // Shows the material's values while it has none.
                        Value::PhysicalProperties(custom) => {
                            physical_control(ui, instance, custom).map(|custom| {
                                Edit::Property(name, Value::PhysicalProperties(custom))
                            })
                        }
                        value => value_control(ui, name, value, assets),
                    })
                    .inner;
                    if made.is_some() {
                        edit = made;
                    }
                    ui.end_row();
                };
                // Rare settings go in Advanced, at the end and closed until
                // opened, so the common ones come first.
                let mut advanced = Vec::new();
                for mut group in sections(instance) {
                    group.properties.retain(|(name, _)| shows(group.title, name));
                    let (rare, common): (Vec<_>, Vec<_>) = group
                        .properties
                        .into_iter()
                        .partition(|(name, _)| is_advanced(class, name));
                    advanced.extend(rare);
                    if common.is_empty() {
                        continue;
                    }
                    shown_any = true;
                    section(ui, group.title, open, true, |ui| {
                        for (name, value) in common {
                            row(ui, name, value);
                        }
                    });
                }
                if !advanced.is_empty() {
                    shown_any = true;
                    let changed = advanced
                        .iter()
                        .filter(|(name, value)| {
                            defaults.get(name).is_some_and(|default| default != value)
                        })
                        .count();
                    let title = match changed {
                        0 => ADVANCED.to_owned(),
                        n => format!("{ADVANCED} · {n} changed"),
                    };
                    section_titled(ui, ADVANCED, &title, false, open, |ui| {
                        for (name, value) in advanced {
                            row(ui, name, value);
                        }
                    });
                }
                // Services keep neither in the world file.
                // While filtering, they show if their title or one of their
                // attributes or tags matches.
                if !instance.class().is_service() {
                    let attributes = !filtering
                        || matches_filter("Attributes", filter)
                        || instance.attributes().keys().any(|a| matches_filter(a, filter));
                    let tags = !filtering
                        || matches_filter("Tags", filter)
                        || instance.tags().iter().any(|t| matches_filter(t, filter));
                    shown_any |= attributes || tags;
                    ui.add_enabled_ui(!locked, |ui| {
                        if attributes && let Some(made) = attributes_section(ui, instance, open) {
                            edit = Some(made);
                        }
                        if tags && let Some(made) = tags_section(ui, instance, open) {
                            edit = Some(made);
                        }
                    });
                }
                if !shown_any {
                    ui.add_space(8.0);
                    ui.weak(format!("No properties match \"{filter}\"."));
                }
                if instance.class() == ClassName::ScreenGui
                    && instance
                        .property("ScaleMode")
                        .and_then(Value::as_str)
                        .is_some_and(|mode| !mode.eq_ignore_ascii_case("Fixed"))
                {
                    ui.weak(
                        "Fit and Scale scale this ScreenGui evenly from DesignWidth × \
                         DesignHeight to the screen. Fit keeps the design's shape, with empty \
                         bars on other screens; Scale fills the whole screen, so things placed \
                         against an edge stay there.",
                    );
                }
                more(ui);
            });
        });
    edit
}

mod many;
#[cfg(test)]
pub use many::shared_names;
pub use many::{count_name, show_many};

/// The types an attribute can be, by name, with the value a new one starts at.
/// Makes the value a new attribute of one type starts at.
type NewValue = fn() -> Value;

pub const ATTRIBUTE_TYPES: [(&str, NewValue); 9] = [
    ("Boolean", || Value::Bool(false)),
    ("Number", || Value::Number(0.0)),
    ("String", || Value::String(String::new())),
    ("Vector3", || Value::Vector3(Vector3::default())),
    ("Vector2", || Value::Vector2(Vector2::default())),
    ("Color3", || Value::Color3(Color3::new(1.0, 1.0, 1.0))),
    ("UDim2", || Value::UDim2(UDim2::default())),
    ("CFrame", || Value::CFrame(CFrame::IDENTITY)),
    ("EnumItem", || {
        Value::EnumItem(EnumItem {
            enum_name: "Material".to_owned(),
            item: "Plastic".to_owned(),
        })
    }),
];

/// The Attributes section: each attribute with its control and a button
/// that removes it, then a row that adds one by name and type.
fn attributes_section(ui: &mut egui::Ui, instance: &Instance, open: Option<bool>) -> Option<Edit> {
    let mut edit = None;
    section(ui, "Attributes", open, true, |ui| {
        for (name, value) in instance.attributes() {
            // The ✕ goes by the name, so the value keeps the whole width.
            ui.horizontal(|ui| {
                field_name(ui, name);
                if widgets_remove(ui, &format!("Remove the attribute {name}")) {
                    edit = Some(Edit::Attribute(name.clone(), None));
                }
            });
            ui.push_id(name, |ui| {
                if let Some(value) = value_edit(ui, name, value.clone()) {
                    edit = Some(Edit::Attribute(name.clone(), Some(value)));
                }
            });
            ui.end_row();
        }
        // The half-typed name and the picked type, kept by egui between frames.
        let id = ui.id().with("new attribute");
        let (mut name, mut kind): (String, usize) =
            ui.data_mut(|data| data.get_temp(id)).unwrap_or_default();
        field_name(ui, "Add");
        ui.vertical(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut name)
                    .hint_text("Name")
                    .desired_width(f32::INFINITY),
            );
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt(id.with("type"))
                    .selected_text(ATTRIBUTE_TYPES[kind].0)
                    .show_ui(ui, |ui| {
                        for (index, (label, _)) in ATTRIBUTE_TYPES.iter().enumerate() {
                            ui.selectable_value(&mut kind, index, *label);
                        }
                    });
                let taken = instance.attribute(&name).is_some();
                let valid = is_attribute_name(&name) && !taken;
                let hint = if taken {
                    "It already has an attribute with this name."
                } else {
                    "Names are 1 to 100 letters, digits and underscores."
                };
                let add = ui
                    .add_enabled(valid, egui::Button::new(format!("{} Add", icons::PLUS)))
                    .on_disabled_hover_text(hint);
                if add.clicked() {
                    edit = Some(Edit::Attribute(
                        name.clone(),
                        Some(ATTRIBUTE_TYPES[kind].1()),
                    ));
                    name.clear();
                }
            });
        });
        ui.data_mut(|data| data.insert_temp(id, (name, kind)));
        ui.end_row();
    });
    edit
}

/// The Tags row: a chip for each tag (click its ✕ to remove it), and a box
/// that adds one on Enter.
fn tags_section(ui: &mut egui::Ui, instance: &Instance, open: Option<bool>) -> Option<Edit> {
    let mut edit = None;
    section(ui, "Tags", open, true, |ui| {
        field_name(ui, "Tags");
        ui.vertical(|ui| {
            ui.horizontal_wrapped(|ui| {
                for tag in instance.tags() {
                    let chip = egui::Button::new(format!("{} {tag}  {}", icons::TAG, icons::X))
                        .corner_radius(10.0)
                        .small();
                    if ui
                        .add(chip)
                        .on_hover_text(format!("Remove the tag {tag}"))
                        .clicked()
                    {
                        edit = Some(Edit::RemoveTag(tag.clone()));
                    }
                }
            });
            let id = ui.id().with("new tag");
            let mut text: String = ui.data_mut(|data| data.get_temp(id)).unwrap_or_default();
            let response = ui.add(
                egui::TextEdit::singleline(&mut text)
                    .hint_text("Add a tag")
                    .desired_width(f32::INFINITY),
            );
            let entered = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let tag = text.trim();
            if entered && !tag.is_empty() {
                edit = Some(Edit::AddTag(tag.to_owned()));
                text.clear();
            }
            ui.data_mut(|data| data.insert_temp(id, text));
        });
        ui.end_row();
    });
    edit
}

/// A small ✕ button. True when clicked.
fn widgets_remove(ui: &mut egui::Ui, tooltip: &str) -> bool {
    ui.small_button(icons::X).on_hover_text(tooltip).clicked()
}

/// An ObjectValue's `Value`: a dropdown of every Instance in the Workspace
/// and ReplicatedStorage by its full name, and None. Returns the new
/// target, if one was picked.
fn object_control(
    ui: &mut egui::Ui,
    world: &World,
    owner: InstanceId,
    target: Option<InstanceId>,
) -> Option<Option<InstanceId>> {
    let target = target.filter(|&target| world.contains(target));
    let current = target
        .and_then(|target| world.full_name(target))
        .unwrap_or_else(|| "None".to_owned());
    let mut picked = None;
    egui::ComboBox::from_id_salt(("object value", owner))
        .selected_text(current)
        .truncate()
        .show_ui(ui, |ui| {
            if ui.selectable_label(target.is_none(), "None").clicked() {
                picked = Some(None);
            }
            let places = [
                world.workspace(),
                world.replicated_storage(),
                world.server_storage(),
            ];
            for id in places
                .into_iter()
                .flat_map(|place| world.descendants(place))
            {
                if id == owner {
                    continue;
                }
                let name = world.full_name(id).unwrap_or_default();
                if ui.selectable_label(target == Some(id), name).clicked() {
                    picked = Some(Some(id));
                }
            }
        });
    picked.filter(|&picked| picked != target)
}

/// A collapsible group with a two-column grid of names and controls.
/// `open` forces it open (while filtering) or leaves it as it was (`None`);
/// with `shown` false nothing is drawn.
fn section(
    ui: &mut egui::Ui,
    title: &str,
    open: Option<bool>,
    shown: bool,
    rows: impl FnOnce(&mut egui::Ui),
) {
    if !shown {
        return;
    }
    section_titled(ui, title, title, true, open, rows);
}

/// [`section`] with a header that can say more than its `title` (which
/// keeps its open state), and closed at first unless `default_open`.
fn section_titled(
    ui: &mut egui::Ui,
    title: &str,
    header: &str,
    default_open: bool,
    open: Option<bool>,
    rows: impl FnOnce(&mut egui::Ui),
) {
    let icon = match title {
        "Data" => icons::IDENTIFICATION_CARD,
        "Appearance" => icons::PALETTE,
        "Behavior" => icons::GEAR,
        "Physics" => icons::ATOM,
        "Part" => icons::CUBE,
        "Surface" => icons::IMAGE,
        "Map Settings" => icons::SLIDERS,
        "Terrain" => icons::MOUNTAINS,
        "Tiling" => icons::GRID_FOUR,
        "Auto Material" => icons::MAGIC_WAND,
        "UI" => icons::LAYOUT,
        "Audio" => icons::SPEAKER_HIGH,
        "Shadows" => icons::SUN,
        "Time of Day" => icons::CLOCK,
        "Sky" => icons::CLOUD_SUN,
        "Fog" => icons::CLOUD_FOG,
        "Atmosphere" => icons::WIND,
        "Clouds" => icons::CLOUD,
        "Weather" => icons::CLOUD_RAIN,
        "Outlines" => icons::PEN_NIB,
        "Color Correction" => icons::SWATCHES,
        "Lens" => icons::APERTURE,
        "Graphics Volume" => icons::BOUNDING_BOX,
        "Effect" => icons::APERTURE,
        "Post-processing" => icons::SPARKLE,
        "Light" => icons::LIGHTBULB,
        "Attributes" => icons::LIST_BULLETS,
        "Tags" => icons::TAG,
        ADVANCED => icons::SLIDERS_HORIZONTAL,
        _ => icons::DOTS_THREE_OUTLINE,
    };
    // Every section's names take the same width (from the panel's), so the
    // fields line up from one section to the next. In a narrow panel the
    // values go under their names instead.
    crate::widgets::section(icon, header)
        .id_salt(title)
        .default_open(default_open)
        .forced(open)
        .show(ui, |ui| crate::responsive::form(ui, title, rows));
}

/// A property's name in the left column, after its Keep pin when it was
/// edited during Play (`pin` says whether it's kept). Returns the pin's new
/// state when it was clicked.
fn field_name_with_pin(
    ui: &mut egui::Ui,
    class: ClassName,
    name: &str,
    pin: Option<bool>,
) -> Option<bool> {
    let Some(kept) = pin else {
        described_name(ui, class, name);
        return None;
    };
    let mut clicked = None;
    let tooltip = if kept {
        "Kept: Stop copies this value into the world you're editing, as one undo step. \
         Click to let it go at Stop."
    } else {
        "Changed during Play. Click to keep this value after Stop."
    };
    crate::widgets::row_name(
        ui,
        false,
        |ui| described_name(ui, class, name),
        |ui| {
            if crate::widgets::icon_button(ui, icons::PUSH_PIN, tooltip, kept).clicked() {
                clicked = Some(!kept);
            }
        },
    );
    clicked
}

/// Whether a property holding `default` gets ↺ when it's changed: not one
/// that points at another Instance, which the game or a picker sets.
fn resettable(default: &Value) -> bool {
    !matches!(default, Value::Instance(_))
}

/// A changed property's name, marked as changed, with a ↺ after it that
/// sets it back to its class's default (one undo step, like any edit).
/// Returns whether ↺ was clicked.
fn field_name_with_reset(ui: &mut egui::Ui, class: ClassName, name: &str, default: &Value) -> bool {
    let mut clicked = false;
    let tooltip = match default_text(default) {
        Some(text) => format!("Changed. Click to reset it to the default, {text}."),
        None => "Changed. Click to reset it to the default.".to_owned(),
    };
    crate::widgets::row_name(
        ui,
        true,
        |ui| described_name(ui, class, name),
        |ui| clicked = crate::widgets::reset_button(ui, &tooltip),
    );
    clicked
}

/// A default short enough to say in a tooltip.
fn default_text(value: &Value) -> Option<String> {
    let number = |n: f64| {
        let text = format!("{n:.3}");
        text.trim_end_matches('0').trim_end_matches('.').to_owned()
    };
    match value {
        Value::Bool(on) => Some(if *on { "on" } else { "off" }.to_owned()),
        Value::Number(n) => Some(number(*n)),
        Value::String(text) if text.is_empty() => Some("empty".to_owned()),
        Value::String(text) if text.chars().count() <= 24 => Some(format!("\"{text}\"")),
        Value::Vector3(v) => Some(format!(
            "{}, {}, {}",
            number(f64::from(v.x)),
            number(f64::from(v.y)),
            number(f64::from(v.z))
        )),
        Value::EnumItem(item) => Some(item.item.clone()),
        _ => None,
    }
}

/// An overridden property's name in a placed prefab copy: bold, with a ↺
/// after it that sets it back to the prefab's value, and Apply to prefab on
/// a right-click.
fn override_name(ui: &mut egui::Ui, name: &str) -> Option<Edit> {
    let mut edit = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        let palette = theme::palette(ui.ctx());
        let label = ui
            .add(
                egui::Label::new(
                    egui::RichText::new(name)
                        .strong()
                        .color(palette.text_strong),
                )
                .truncate()
                .sense(egui::Sense::click()),
            )
            .on_hover_text(
                "Overridden: this copy sets it differently from its prefab. Right-click to \
                 apply it to the prefab.",
            );
        label.context_menu(|ui| {
            if ui
                .button(theme::with_icon(icons::UPLOAD_SIMPLE, "Apply to prefab"))
                .clicked()
            {
                edit = Some(Edit::ApplyOverride(name.to_owned()));
            }
        });
        if crate::widgets::reset_button(ui, "Revert to the prefab's value") {
            edit = Some(Edit::RevertOverride(name.to_owned()));
        }
    });
    edit
}

/// A property's name, in the left column.
fn field_name(ui: &mut egui::Ui, name: &str) {
    let palette = theme::palette(ui.ctx());
    ui.add(egui::Label::new(egui::RichText::new(name).color(palette.text_secondary)).truncate());
}

/// A property's name, as [`field_name`], that says on hover what the
/// property does: the same description scripts' autocomplete and the API
/// reference give it (`axcenity_script::property_description`), looked up
/// only while hovered.
fn described_name(ui: &mut egui::Ui, class: ClassName, name: &str) {
    let palette = theme::palette(ui.ctx());
    let label = ui.add(
        egui::Label::new(egui::RichText::new(name).color(palette.text_secondary))
            .truncate()
            .show_tooltip_when_elided(false),
    );
    if label.hovered()
        && let Some(about) = description(class, name)
    {
        label.on_hover_ui(|ui| {
            ui.set_max_width(320.0);
            ui.strong(name);
            ui.label(about);
        });
    }
}

/// What `class`'s `name` does, for its hover: the Data rows' own words, or
/// the script API's description of the property.
pub fn description(class: ClassName, name: &str) -> Option<&'static str> {
    match name {
        "Name" => Some(
            "What it's called, in the Explorer and in scripts (workspace.Door). Names don't \
             have to be unique.",
        ),
        "ClassName" => Some("What kind of object it is. It can't be changed."),
        _ => axcenity_script::property_description(class, name),
    }
}

/// Types `text` into the Properties filter, as universal search does to go
/// straight to a property (`app/search.rs`). Every selection shares it.
pub fn set_filter(ctx: &egui::Context, text: &str) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("properties filter"), text.to_owned()));
}

/// Whether a property `name` (or a section titled `name`) matches what's
/// typed in the filter: letters in order, ignoring case and spaces, so
/// "walk speed" finds WalkSpeed.
pub fn matches_filter(name: &str, filter: &str) -> bool {
    let squash = |text: &str| -> String {
        text.chars()
            .filter(|c| !c.is_whitespace())
            .flat_map(char::to_lowercase)
            .collect()
    };
    squash(name).contains(&squash(filter))
}

/// A note above the properties, such as during Play.
fn notice(ui: &mut egui::Ui, icon: &str, text: &str) {
    let palette = theme::palette(ui.ctx());
    egui::Frame::new()
        .fill(palette.surface)
        .stroke(egui::Stroke::new(1.0_f32, palette.border))
        .corner_radius(6)
        .inner_margin(egui::Margin::symmetric(8, 5))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                ui.label(egui::RichText::new(icon).color(palette.text_weak));
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(text)
                            .size(theme::type_scale::SMALL)
                            .color(palette.text_secondary),
                    )
                    .wrap(),
                );
            });
        });
    ui.add_space(4.0);
}

/// Draws the control for one property and returns the edit if the user
/// changed it this frame.
fn value_control(
    ui: &mut egui::Ui,
    name: &'static str,
    value: Value,
    assets: &Assets,
) -> Option<Edit> {
    match value {
        Value::String(path) if is_map(name) => map_control(ui, name, &path, assets),
        Value::String(path) if name == "SoundId" => sound_control(ui, &path, assets),
        Value::String(path) if name == "Image" => image_control(ui, &path),
        Value::String(path) if name == "StateMachine" => controller_control(ui, &path, assets),
        Value::String(path) if name == "Clip" => clip_control(ui, &path, assets),
        Value::String(path) if name == "BehaviorTree" => behavior_tree_control(ui, &path),
        Value::String(path) if name == "SourceFile" => source_file_control(ui, &path),
        Value::String(path) if name == "SkyImage" => sky_control(ui, &path, assets),
        Value::String(path) if matches!(name, "LookupTable" | "Palette") => {
            grading_control(ui, name, &path, assets)
        }
        // A model named by its asset ID shows the file it names.
        Value::String(mesh_id) if name == "MeshId" && is_asset_reference(&mesh_id) => {
            let shown = assets
                .asset_path(&mesh_id)
                .unwrap_or_else(|| format!("(no file has the ID {mesh_id})"));
            let hover =
                format!("{mesh_id}\n(the model's asset ID: it follows the file when it moves)");
            ui.add_enabled(false, egui::TextEdit::singleline(&mut shown.as_str()))
                .on_disabled_hover_text(hover);
            None
        }
        // Picked by the editor; the Terrain Studio says where it is.
        Value::String(path) if name == "DataFolder" => {
            ui.label(if path.is_empty() {
                "(chosen on save)"
            } else {
                path.as_str()
            })
            .on_hover_text("Where this terrain's heights and paint are saved, in the project.");
            None
        }
        value => value_edit(ui, name, value).map(|value| Edit::Property(name, value)),
    }
}

/// The name of the file `reference` names, with its project path on hover.
/// A reference by asset ID shows the file with that ID, or says no file has
/// it (its sidecar may be gone; Asset Check lists it).
fn file_label(ui: &mut egui::Ui, assets: &Assets, reference: &str) {
    let Some(path) = assets.asset_path(reference) else {
        ui.weak("No file has this ID").on_hover_text(format!(
            "No file in the project has the asset ID {reference}: its .import.json \
             sidecar may be gone. Asset Check lists it."
        ));
        return;
    };
    let hover = if path == reference {
        path.clone()
    } else {
        format!("{path}\n(by its asset ID {reference}, so it follows the file when it moves)")
    };
    let file = path.rsplit('/').next().unwrap_or(&path);
    ui.add(egui::Label::new(file).truncate())
        .on_hover_text(hover);
}

/// A texture map's file, with buttons to pick another and to clear it.
/// Typing isn't offered: each half-typed path would be loaded, and fail.
fn map_control(ui: &mut egui::Ui, name: &'static str, path: &str, assets: &Assets) -> Option<Edit> {
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
        file_label(ui, assets, path);
    });
    edit
}

/// A UI element's picture: typed as a project path, uploaded from this
/// computer (copied into `assets/images`), or dropped from the Assets
/// panel. The UI Designer's inspector also picks from the project's images.
fn image_control(ui: &mut egui::Ui, path: &str) -> Option<Edit> {
    let mut edit = None;
    let row = ui.horizontal(|ui| {
        if ui
            .small_button(icons::UPLOAD_SIMPLE)
            .on_hover_text("Upload an image from this computer (copied into assets/images)")
            .clicked()
        {
            edit = Some(Edit::PickTexture("Image"));
        }
        if let Some(value) = value_edit(ui, "Image", Value::String(path.to_owned())) {
            edit = Some(Edit::Property("Image", value));
        }
    });
    if let Some(held) = row
        .response
        .dnd_hover_payload::<crate::asset_browser::AssetPayload>()
        && matches!(
            held.0.kind,
            crate::asset_browser::AssetKind::Image | crate::asset_browser::AssetKind::Texture
        )
    {
        let palette = theme::palette(ui.ctx());
        ui.painter().rect_stroke(
            row.response.rect.expand(2.0),
            4,
            egui::Stroke::new(1.5_f32, palette.accent),
            egui::StrokeKind::Outside,
        );
        if let Some(payload) = row
            .response
            .dnd_release_payload::<crate::asset_browser::AssetPayload>()
        {
            edit = Some(Edit::Property(
                "Image",
                Value::String(payload.0.path.clone()),
            ));
        }
    }
    edit
}

/// A ColorCorrectionEffect's lookup table or palette file, with buttons to
/// pick one and to use none.
fn grading_control(
    ui: &mut egui::Ui,
    name: &'static str,
    path: &str,
    assets: &Assets,
) -> Option<Edit> {
    let mut edit = None;
    ui.horizontal(|ui| {
        let hover = if name == "LookupTable" {
            "Choose a lookup table: a .cube file, or a strip image of N squares of N × N \
             pixels side by side (such as 256 × 16)"
        } else {
            "Choose a palette: an image whose colors (up to 64) every pixel is snapped to"
        };
        if ui
            .small_button(icons::FOLDER_OPEN)
            .on_hover_text(hover)
            .clicked()
        {
            edit = Some(Edit::PickGradingFile(name));
        }
        if path.is_empty() {
            ui.weak("None");
            return;
        }
        if ui
            .small_button(icons::X)
            .on_hover_text("Use none")
            .clicked()
        {
            edit = Some(Edit::Property(name, Value::String(String::new())));
        }
        file_label(ui, assets, path);
    });
    edit
}

/// Lighting's sky image, with buttons to pick a file and to go back to the
/// sky colors. A sky dragged from the Assets panel can be let go on it.
/// Both ask before the sky changes.
fn sky_control(ui: &mut egui::Ui, path: &str, assets: &Assets) -> Option<Edit> {
    let mut edit = None;
    let row = ui.horizontal(|ui| {
        if ui
            .small_button(icons::FOLDER_OPEN)
            .on_hover_text(
                "Choose a sky image from this computer: a 2:1 panorama, best as an HDR                  (.exr or .hdr), or a PNG or JPEG",
            )
            .clicked()
        {
            edit = Some(Edit::PickSkyImage);
        }
        if path.is_empty() {
            ui.weak("Sky colors");
            return;
        }
        if ui
            .small_button(icons::X)
            .on_hover_text("Go back to the sky colors")
            .clicked()
        {
            edit = Some(Edit::Property("SkyImage", Value::String(String::new())));
        }
        file_label(ui, assets, path);
    });
    if let Some(held) = row
        .response
        .dnd_hover_payload::<crate::asset_browser::AssetPayload>()
        && held.0.kind == crate::asset_browser::AssetKind::Sky
    {
        let palette = theme::palette(ui.ctx());
        ui.painter().rect_stroke(
            row.response.rect.expand(2.0),
            4,
            egui::Stroke::new(1.5_f32, palette.accent),
            egui::StrokeKind::Outside,
        );
        if let Some(payload) = row
            .response
            .dnd_release_payload::<crate::asset_browser::AssetPayload>()
        {
            edit = Some(Edit::AskSkyImage(payload.0.path.clone()));
        }
    }
    edit
}

/// A property Animation's clip file.
fn clip_control(ui: &mut egui::Ui, path: &str, assets: &Assets) -> Option<Edit> {
    let mut edit = None;
    ui.horizontal(|ui| {
        if ui
            .small_button(icons::FOLDER_OPEN)
            .on_hover_text("Choose a property clip (.anim.json with kind property) in the project")
            .clicked()
        {
            edit = Some(Edit::PickClip);
        }
        if path.is_empty() {
            ui.weak("None");
            return;
        }
        if ui
            .small_button(icons::FILM_SLATE)
            .on_hover_text("Open it in the Animation tab")
            .clicked()
        {
            edit = Some(Edit::OpenClip);
        }
        if ui
            .small_button(icons::X)
            .on_hover_text("Play no clip")
            .clicked()
        {
            edit = Some(Edit::Property("Clip", Value::String(String::new())));
        }
        file_label(ui, assets, path);
    });
    edit
}

/// An Animator's controller file, with buttons to pick another and to clear it.
fn controller_control(ui: &mut egui::Ui, path: &str, assets: &Assets) -> Option<Edit> {
    let mut edit = None;
    ui.horizontal(|ui| {
        if ui
            .small_button(icons::FOLDER_OPEN)
            .on_hover_text("Choose a state machine (.statemachine.json) in the project")
            .clicked()
        {
            edit = Some(Edit::PickController);
        }
        if path.is_empty() {
            ui.weak("None");
            return;
        }
        if ui
            .small_button(icons::FLOW_ARROW)
            .on_hover_text("Open it in the Animation tab: its states, transitions and parameters")
            .clicked()
        {
            edit = Some(Edit::OpenController);
        }
        if ui
            .small_button(icons::X)
            .on_hover_text("Play no state machine")
            .clicked()
        {
            edit = Some(Edit::Property("StateMachine", Value::String(String::new())));
        }
        file_label(ui, assets, path);
    });
    edit
}

/// A NavigationAgent's behavior tree file, with buttons to pick another,
/// open it and clear it.
fn behavior_tree_control(ui: &mut egui::Ui, path: &str) -> Option<Edit> {
    let mut edit = None;
    ui.horizontal(|ui| {
        if ui
            .small_button(icons::FOLDER_OPEN)
            .on_hover_text("Choose a behavior tree (.behaviortree.json) in the project")
            .clicked()
        {
            edit = Some(Edit::PickBehaviorTree);
        }
        if path.is_empty() {
            ui.weak("None");
            return;
        }
        if ui
            .small_button(icons::BRAIN)
            .on_hover_text("Open it in the Behavior Tree tab")
            .clicked()
        {
            edit = Some(Edit::OpenBehaviorTree);
        }
        if ui
            .small_button(icons::X)
            .on_hover_text("Run no tree: scripts set where it walks")
            .clicked()
        {
            edit = Some(Edit::Property("BehaviorTree", Value::String(String::new())));
        }
        let name = axcenity_blocks::behavior_tree::behavior_tree_name(path);
        ui.add(egui::Label::new(name).truncate())
            .on_hover_text(path);
    });
    edit
}

/// A script's `SourceFile` (G2), with buttons to pick another and to open
/// it. Empty for a script from the `scripts` folder, which is its file.
fn source_file_control(ui: &mut egui::Ui, path: &str) -> Option<Edit> {
    if path.is_empty() {
        ui.weak("Its own file in scripts/");
        return None;
    }
    let mut edit = None;
    ui.horizontal(|ui| {
        if ui
            .small_button(icons::FOLDER_OPEN)
            .on_hover_text(
                "Choose the file this script runs. Copies of its object keep using theirs.",
            )
            .clicked()
        {
            edit = Some(Edit::PickSourceFile);
        }
        if ui
            .small_button(icons::ARROW_SQUARE_OUT)
            .on_hover_text("Open it in a tab")
            .clicked()
        {
            edit = Some(Edit::OpenSourceFile);
        }
        if ui
            .small_button(icons::PENCIL_SIMPLE)
            .on_hover_text(
                "Rename or move the file. Every script that uses it is changed to match.",
            )
            .clicked()
        {
            edit = Some(Edit::MoveSourceFile);
        }
        ui.add(egui::Label::new(path).truncate())
            .on_hover_text(path);
    });
    edit
}

/// A Sound's file, with buttons to pick another, and to hear it.
fn sound_control(ui: &mut egui::Ui, path: &str, assets: &Assets) -> Option<Edit> {
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
        file_label(ui, assets, path);
    });
    edit
}

/// Whether `name` is one of a UI element's transparencies, each from 0
/// (solid) to 1 (invisible).
fn is_ui_transparency(name: &str) -> bool {
    matches!(
        name,
        "BackgroundTransparency" | "TextTransparency" | "ImageTransparency"
    )
}

/// A transparency from 0 to 1 as a 0–100% slider. Returns the new value.
pub fn transparency_slider(ui: &mut egui::Ui, value: f64) -> Option<f64> {
    let mut percent = (value * 100.0).clamp(0.0, 100.0);
    ui.add(
        egui::Slider::new(&mut percent, 0.0..=100.0)
            .suffix("%")
            .max_decimals(0)
            .clamping(egui::SliderClamping::Always),
    )
    .changed()
    .then_some((percent / 100.0).clamp(0.0, 1.0))
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
fn physical_control(
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
fn cframe_control(ui: &mut egui::Ui, cf: CFrame) -> Option<CFrame> {
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
fn enum_item_control(ui: &mut egui::Ui, name: &str, item: &EnumItem) -> Option<EnumItem> {
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
        assert_eq!(
            titles(&sections),
            ["Appearance", "Behavior", "Physics", "Part", "Tiling"]
        );
        let names: Vec<&str> = sections
            .iter()
            .flat_map(|section| section.properties.iter().map(|(name, _)| *name))
            .collect();
        assert_eq!(
            names,
            [
                "Color",
                "Material",
                "MaterialVariant",
                "BlueprintTexture",
                "Metalness",
                "Roughness",
                "Transparency",
                "EmissiveColor",
                "Emission",
                "CastShadow",
                "Locked",
                "Anchored",
                "CanCollide",
                "CanTouch",
                "CanQuery",
                "CollisionGroup",
                "WindAffected",
                "CustomPhysicalProperties",
                "Massless",
                "AssemblyLinearVelocity",
                "AssemblyAngularVelocity",
                "Shape",
                "Position",
                "Orientation",
                "Size",
                "TileSize"
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
        assert_eq!(
            titles(&sections),
            ["Appearance", "Surface", "Map Settings", "Tiling"]
        );
        let placement: Vec<&str> = sections[3]
            .properties
            .iter()
            .map(|(name, _)| *name)
            .collect();
        assert_eq!(placement, ["Tiling", "Offset"]);
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
    fn the_workspace_shows_only_its_gravity_wind_and_decal_cap_and_other_services_nothing() {
        let world = World::new();
        let workspace = world.get(world.workspace()).unwrap();
        let sections = sections(workspace);
        assert_eq!(
            titles(&sections),
            ["Physics", "Wind", "Decals", "Streaming"]
        );
        assert_eq!(sections[0].properties[0].0, "Gravity");
        assert_eq!(sections[1].properties[0].0, "GlobalWind");
        assert_eq!(sections[2].properties[0].0, "MaxSpawnedDecals");
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

#[cfg(test)]
mod attribute_section_tests {
    use super::*;

    /// A Folder with an attribute of every kind and some tags draws its
    /// Attributes section and Tags row inside a 260-point column, and
    /// clicking nothing makes no edit. With `AXCENITY_SNAPSHOTS` set it's
    /// saved as `properties-attributes.png`.
    #[test]
    fn attributes_and_tags_draw_in_a_narrow_column() {
        let mut world = World::new();
        let folder = world.create(ClassName::Folder).unwrap();
        world.set_parent(folder, Some(world.workspace())).unwrap();
        for (name, make) in ATTRIBUTE_TYPES {
            world.set_attribute(folder, name, Some(make())).unwrap();
        }
        world.add_tag(folder, "Lava").unwrap();
        world.add_tag(folder, "Hazard").unwrap();
        let size = egui::vec2(260.0, 1100.0);
        let mut fit = None;
        let mut edit = None;
        let mut draw = |ui: &mut egui::Ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let room = ui.max_rect();
                edit = show(
                    ui,
                    &world,
                    &Assets::default(),
                    Some(folder),
                    false,
                    &[],
                    |_| {},
                );
                fit = Some((ui.min_rect(), room));
            });
        };
        let ctx = theme::test_context();
        for _ in 0..3 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                ..Default::default()
            };
            let _ = crate::theme::run(&ctx, input, |ui| draw(ui));
        }
        if let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS") {
            crate::snapshot::snapshot(
                &std::path::PathBuf::from(folder).join("properties-attributes.png"),
                size,
                vec![Vec::new(); 3],
                &mut draw,
            );
        }
        let (used, room) = fit.unwrap();
        assert!(
            used.width() <= room.width() + 0.5,
            "{} in {}",
            used.width(),
            room.width()
        );
        assert_eq!(edit, None);
    }
}

#[cfg(test)]
mod physics_section_tests {
    use super::*;

    /// A Rubber Part with its own physical properties draws the Physics
    /// section (Custom on, five numbers, Massless and both velocities)
    /// inside a 260-point column. With `AXCENITY_SNAPSHOTS` set it's saved
    /// as `properties-physics.png`.
    #[test]
    fn a_parts_physics_section_draws_in_a_narrow_column() {
        let mut world = World::new();
        let part = world.create(ClassName::Part).unwrap();
        world.set_parent(part, Some(world.workspace())).unwrap();
        world
            .set_property(part, "Material", Value::Material(Material::Rubber))
            .unwrap();
        let custom = PhysicalProperties {
            friction: 0.2,
            ..Material::Rubber.physical()
        };
        world
            .set_property(
                part,
                "CustomPhysicalProperties",
                Value::PhysicalProperties(Some(custom)),
            )
            .unwrap();
        let titles: Vec<&str> = sections(world.get(part).unwrap())
            .iter()
            .map(|section| section.title)
            .collect();
        assert!(titles.contains(&"Physics"), "{titles:?}");
        let size = egui::vec2(260.0, 1300.0);
        let mut fit = None;
        let mut edit = None;
        let mut draw = |ui: &mut egui::Ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let room = ui.max_rect();
                edit = show(
                    ui,
                    &world,
                    &Assets::default(),
                    Some(part),
                    false,
                    &[],
                    |_| {},
                );
                fit = Some((ui.min_rect(), room));
            });
        };
        let ctx = theme::test_context();
        for _ in 0..3 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                ..Default::default()
            };
            let _ = crate::theme::run(&ctx, input, |ui| draw(ui));
        }
        if let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS") {
            crate::snapshot::snapshot(
                &std::path::PathBuf::from(folder).join("properties-physics.png"),
                size,
                vec![Vec::new(); 3],
                &mut draw,
            );
        }
        let (used, room) = fit.unwrap();
        assert!(
            used.width() <= room.width() + 0.5,
            "{} in {}",
            used.width(),
            room.width()
        );
        assert_eq!(edit, None);
    }
}

#[cfg(test)]
mod prefab_override_tests {
    use std::sync::Arc;

    use axcenity_core::prefab::{Overrides, PrefabLink, PrefabSource, sync};

    use super::*;

    /// A placed copy of a one-Part prefab whose Color it overrides draws
    /// Color bold with a ↺ in a 260-point column, and other properties
    /// plainly. With `AXCENITY_SNAPSHOTS` set it's saved as
    /// `properties-prefab-override.png`.
    #[test]
    fn an_overridden_property_is_bold_with_a_revert_button() {
        let mut source = World::new();
        let original = source.create(ClassName::Part).unwrap();
        source
            .set_parent(original, Some(source.workspace()))
            .unwrap();
        let source = Arc::new(PrefabSource {
            world: source,
            roots: vec![original],
        });
        let mut world = World::new();
        let part = world.copy_from(&source.world, original).unwrap();
        world.set_parent(part, Some(world.workspace())).unwrap();
        world
            .set_prefab(
                part,
                Some(PrefabLink::new("assets/prefabs/Crate.prefab.json", 0)),
            )
            .unwrap();
        sync(&mut world, part, &source, 0, &Overrides::default());
        world
            .set_property(part, "Color", Value::Color3(Color3::new(1.0, 0.0, 0.0)))
            .unwrap();

        let size = egui::vec2(260.0, 700.0);
        let mut fit = None;
        let mut edit = None;
        let mut draw = |ui: &mut egui::Ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let room = ui.max_rect();
                edit = show(
                    ui,
                    &world,
                    &Assets::default(),
                    Some(part),
                    false,
                    &[],
                    |_| {},
                );
                fit = Some((ui.min_rect(), room));
            });
        };
        let ctx = theme::test_context();
        for _ in 0..3 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                ..Default::default()
            };
            let _ = crate::theme::run(&ctx, input, |ui| draw(ui));
        }
        if let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS") {
            crate::snapshot::snapshot(
                &std::path::PathBuf::from(folder).join("properties-prefab-override.png"),
                size,
                vec![Vec::new(); 3],
                &mut draw,
            );
        }
        let (used, room) = fit.unwrap();
        assert!(used.width() <= room.width() + 0.5);
        assert_eq!(edit, None);
    }
}

#[cfg(test)]
mod decal_section_tests {
    use super::*;

    fn titles(instance: &Instance) -> Vec<&'static str> {
        sections(instance)
            .iter()
            .map(|section| section.title)
            .collect()
    }

    /// Phase 75: a Decal's and a ProjectedDecal's picture settings are one
    /// section, and a ProjectedDecal's box another; nothing is left over.
    #[test]
    fn decals_show_their_picture_and_box_sections() {
        let mut world = World::new();
        let decal = world.create(ClassName::Decal).unwrap();
        let projected = world.create(ClassName::ProjectedDecal).unwrap();
        assert_eq!(titles(world.get(decal).unwrap()), ["Appearance", "Decal"]);
        assert_eq!(
            titles(world.get(projected).unwrap()),
            ["Appearance", "Decal", "Box"]
        );
    }

    /// Every property the panel shows for the classes beginners meet first
    /// says on hover what it does, from the script API's descriptions.
    #[test]
    fn common_properties_say_what_they_do() {
        let mut world = World::new();
        let mut missing = Vec::new();
        for class in [
            ClassName::Part,
            ClassName::MeshPart,
            ClassName::Model,
            ClassName::PointLight,
            ClassName::SpotLight,
            ClassName::Sound,
            ClassName::ParticleEmitter,
            ClassName::Decal,
            ClassName::Animation,
            ClassName::Animator,
        ] {
            let id = world.create(class).unwrap();
            let instance = world.get(id).unwrap();
            for section in sections(instance) {
                for (name, _) in section.properties {
                    if description(class, name).is_none_or(str::is_empty) {
                        missing.push(format!("{class:?}.{name}"));
                    }
                }
            }
        }
        assert!(missing.is_empty(), "no description: {missing:?}");
        assert!(description(ClassName::Part, "Name").is_some());
        assert!(
            description(ClassName::Part, "Anchored")
                .unwrap()
                .contains("stays where it is")
        );
    }

    #[test]
    fn the_filter_ignores_case_and_spaces() {
        assert!(matches_filter("WalkSpeed", "walk speed"));
        assert!(matches_filter("CanCollide", "COLL"));
        assert!(matches_filter("Anchored", ""));
        assert!(!matches_filter("Anchored", "color"));
    }

    /// Text drawn by the Properties panel for `id` with `filter` typed in
    /// its filter box.
    fn shown_with_filter(world: &World, id: InstanceId, filter: &str) -> Vec<String> {
        fn walk(shape: &egui::Shape, found: &mut Vec<String>) {
            match shape {
                egui::Shape::Text(text) => found.push(text.galley.text().to_owned()),
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| walk(s, found)),
                _ => {}
            }
        }
        let ctx = crate::theme::test_context();
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("properties filter"), filter.to_owned()));
        let mut found = Vec::new();
        for _ in 0..3 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(300.0, 2000.0),
                )),
                ..Default::default()
            };
            let output = crate::theme::run(&ctx, input, |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    let _ = show(ui, world, &Assets::default(), Some(id), false, &[], |_| {});
                });
            });
            found.clear();
            for clipped in &output.shapes {
                walk(&clipped.shape, &mut found);
            }
        }
        found
    }

    /// Draws the Properties panel for `id` with `filter` typed, clicks the
    /// first ↺ it shows, and returns the edit made, or `None` when there's
    /// no ↺.
    fn click_reset(world: &World, id: InstanceId, filter: &str, playing: bool) -> Option<Edit> {
        fn find(shape: &egui::Shape, at: &mut Option<egui::Pos2>) {
            match shape {
                egui::Shape::Text(text)
                    if at.is_none() && text.galley.text() == icons::ARROW_COUNTER_CLOCKWISE =>
                {
                    *at = Some(text.visual_bounding_rect().center());
                }
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| find(s, at)),
                _ => {}
            }
        }
        let ctx = crate::theme::test_context();
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("properties filter"), filter.to_owned()));
        let frame = |events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(300.0, 2000.0),
                )),
                events,
                ..Default::default()
            };
            let mut edit = None;
            let output = crate::theme::run(&ctx, input, |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    edit = show(
                        ui,
                        world,
                        &Assets::default(),
                        Some(id),
                        playing,
                        &[],
                        |_| {},
                    );
                });
            });
            let mut at = None;
            for clipped in &output.shapes {
                find(&clipped.shape, &mut at);
            }
            (at, edit)
        };
        frame(Vec::new());
        let (at, _) = frame(Vec::new());
        let at = at?;
        let press = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(vec![egui::Event::PointerMoved(at)]);
        frame(vec![press(true)]);
        frame(vec![press(false)]).1
    }

    /// A property that differs from its class's default gets ↺, which sets
    /// the default back; one that doesn't, a locked one and one during Play
    /// get none.
    #[test]
    fn a_changed_property_resets_to_its_default_with_one_click() {
        let mut world = World::new();
        let part = world.create(ClassName::Part).unwrap();
        let default = ClassName::Part
            .default_properties()
            .into_iter()
            .find(|(name, _)| *name == "Transparency")
            .unwrap()
            .1;
        assert_eq!(click_reset(&world, part, "transparency", false), None);

        world
            .set_property(part, "Transparency", Value::Number(0.5))
            .unwrap();
        assert_eq!(
            click_reset(&world, part, "transparency", false),
            Some(Edit::Property("Transparency", default))
        );
        // Play's changes are kept or dropped at Stop, not reset.
        assert_eq!(click_reset(&world, part, "transparency", true), None);
        // Only Locked can be changed on something locked.
        world
            .set_property(part, "Locked", Value::Bool(true))
            .unwrap();
        assert_eq!(click_reset(&world, part, "transparency", false), None);
        assert_eq!(
            click_reset(&world, part, "locked", false),
            Some(Edit::Property("Locked", Value::Bool(false)))
        );

        // With AXCENITY_SNAPSHOTS: a moved, half-clear part with the mouse
        // over Transparency's ↺.
        if let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS") {
            world
                .set_property(part, "Locked", Value::Bool(false))
                .unwrap();
            world
                .set_property(
                    part,
                    "Position",
                    Value::Vector3(Vector3::new(4.0, 2.0, 0.0)),
                )
                .unwrap();
            let mut frames = vec![Vec::new(); 6];
            frames[0].push(egui::Event::PointerMoved(egui::pos2(21.0, 427.0)));
            crate::snapshot::snapshot(
                &std::path::PathBuf::from(folder).join("properties-reset.png"),
                egui::vec2(300.0, 560.0),
                frames,
                |ui| {
                    ui.data_mut(|d| {
                        d.insert_temp(egui::Id::new("properties filter"), String::new())
                    });
                    egui::CentralPanel::default().show(ui, |ui| {
                        let _ = show(
                            ui,
                            &world,
                            &Assets::default(),
                            Some(part),
                            false,
                            &[],
                            |_| {},
                        );
                    });
                },
            );
        }
    }

    #[test]
    fn defaults_are_said_briefly() {
        assert_eq!(default_text(&Value::Number(0.25)).as_deref(), Some("0.25"));
        assert_eq!(default_text(&Value::Number(16.0)).as_deref(), Some("16"));
        assert_eq!(default_text(&Value::Bool(true)).as_deref(), Some("on"));
        assert_eq!(
            default_text(&Value::Vector3(Vector3::new(4.0, 1.0, 2.0))).as_deref(),
            Some("4, 1, 2")
        );
        assert_eq!(
            default_text(&Value::String(String::new())).as_deref(),
            Some("empty")
        );
    }

    /// Rare settings start folded under Advanced, whose header counts the
    /// ones changed; the filter finds them and opens it.
    #[test]
    fn rare_settings_fold_under_advanced_and_say_how_many_changed() {
        let mut world = World::new();
        let part = world.create(ClassName::Part).unwrap();
        let has = |shown: &[String], text: &str| shown.iter().any(|t| t == text);
        // A section's header has its icon in front.
        let header =
            |shown: &[String], text: &str| shown.iter().any(|t| t.ends_with(&format!("  {text}")));

        let shown = shown_with_filter(&world, part, "");
        assert!(header(&shown, "Advanced"), "{shown:?}");
        for rare in ["Massless", "CollisionGroup", "MaterialVariant"] {
            assert!(!has(&shown, rare), "{rare} is folded: {shown:?}");
        }
        for common in ["Color", "Anchored", "CanCollide", "Size"] {
            assert!(has(&shown, common), "{common} stays: {shown:?}");
        }

        world
            .set_property(part, "Massless", Value::Bool(true))
            .unwrap();
        let shown = shown_with_filter(&world, part, "");
        assert!(header(&shown, "Advanced · 1 changed"), "{shown:?}");

        let shown = shown_with_filter(&world, part, "mass");
        assert!(has(&shown, "Massless"), "{shown:?}");

        // With AXCENITY_SNAPSHOTS: the part's panel, Advanced closed.
        if let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS") {
            crate::snapshot::snapshot(
                &std::path::PathBuf::from(folder).join("properties-advanced.png"),
                egui::vec2(300.0, 1100.0),
                vec![Vec::new(); 3],
                |ui| {
                    ui.data_mut(|d| {
                        d.insert_temp(egui::Id::new("properties filter"), String::new())
                    });
                    egui::CentralPanel::default().show(ui, |ui| {
                        let _ = show(
                            ui,
                            &world,
                            &Assets::default(),
                            Some(part),
                            false,
                            &[],
                            |_| {},
                        );
                    });
                },
            );
        }

        // Lights keep Brightness in view; an emitter's is rare.
        assert!(!is_advanced(ClassName::PointLight, "Brightness"));
        assert!(is_advanced(ClassName::ParticleEmitter, "Brightness"));
        assert!(is_advanced(
            ClassName::ThirdPersonController,
            "MaxStepHeight"
        ));
        assert!(!is_advanced(ClassName::ThirdPersonController, "WalkSpeed"));
        assert!(is_advanced(ClassName::TextLabel, "RichText"));
        assert!(!is_advanced(ClassName::TextLabel, "Text"));
    }

    /// Typing in Filter properties leaves only the rows whose names match
    /// (and whole sections whose titles do), and says so when none do.
    #[test]
    fn filter_properties_shows_only_matching_rows() {
        let mut world = World::new();
        let part = world.create(ClassName::Part).unwrap();
        world
            .set_attribute(part, "Health", Some(Value::Number(100.0)))
            .unwrap();
        let has = |shown: &[String], text: &str| shown.iter().any(|t| t == text);

        let all = shown_with_filter(&world, part, "");
        for name in ["Name", "Color", "Anchored", "CanCollide", "Size", "Health"] {
            assert!(has(&all, name), "{name} shows unfiltered: {all:?}");
        }

        // The header says the class; its own row is only for the filter.
        assert!(!has(&all, "ClassName"), "{all:?}");
        assert!(has(&shown_with_filter(&world, part, "class"), "ClassName"));

        let shown = shown_with_filter(&world, part, "can coll");
        assert!(has(&shown, "CanCollide"), "{shown:?}");
        for hidden in ["Name", "Color", "Anchored", "Size", "Health"] {
            assert!(!has(&shown, hidden), "{hidden} is filtered out: {shown:?}");
        }

        // A section's title shows all of it.
        let shown = shown_with_filter(&world, part, "appearance");
        assert!(
            has(&shown, "Color") && has(&shown, "Roughness"),
            "{shown:?}"
        );
        assert!(!has(&shown, "Anchored"), "{shown:?}");

        // Attributes match by name too.
        let shown = shown_with_filter(&world, part, "health");
        assert!(has(&shown, "Health"), "{shown:?}");

        let shown = shown_with_filter(&world, part, "zzz");
        assert!(
            shown.iter().any(|t| t.contains("No properties match")),
            "{shown:?}"
        );

        // With AXCENITY_SNAPSHOTS: the panel filtered to "can", and
        // unfiltered with the mouse over Anchored, showing what it does.
        if let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS") {
            let folder = std::path::PathBuf::from(folder);
            for (file, filter, hover) in [
                ("properties-filter.png", "can", None),
                (
                    "properties-description.png",
                    "",
                    Some(egui::pos2(40.0, 427.0)),
                ),
            ] {
                let mut draw = |ui: &mut egui::Ui| {
                    ui.data_mut(|d| {
                        d.insert_temp(egui::Id::new("properties filter"), filter.to_owned())
                    });
                    egui::CentralPanel::default().show(ui, |ui| {
                        let _ = show(
                            ui,
                            &world,
                            &Assets::default(),
                            Some(part),
                            false,
                            &[],
                            |_| {},
                        );
                    });
                };
                let frames = match hover {
                    Some(at) => {
                        let mut frames = vec![Vec::new(); 6];
                        frames[0].push(egui::Event::PointerMoved(at));
                        frames
                    }
                    None => vec![Vec::new(); 3],
                };
                crate::snapshot::snapshot(
                    &folder.join(file),
                    egui::vec2(300.0, 620.0),
                    frames,
                    &mut draw,
                );
            }
        }
    }
}
