//! Which properties go in which section of the panel, in panel order, and
//! which are rare settings shown under Advanced.

use super::*;

/// One group of properties in the panel, such as "Appearance".
#[derive(Debug, PartialEq)]
pub struct Section {
    pub title: &'static str,
    pub properties: Vec<(&'static str, Value)>,
}

/// The groups, in panel order, and which properties go in each. Name and
/// ClassName come first, under "Data", and aren't in the property map.
pub(super) const GROUPS: [(&str, &[&str]); 35] = [
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
            "LinearDrag",
            "AngularDrag",
            "RollingFriction",
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
pub(super) const OTHER: &str = "Other";

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
pub(super) const ADVANCED: &str = "Advanced";

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
            "LinearDrag",
            "AngularDrag",
            "RollingFriction",
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
