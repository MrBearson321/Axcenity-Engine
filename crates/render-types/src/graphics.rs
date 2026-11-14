//! The machine's graphics choices: a quality preset plus optional
//! overrides, what the GPU can do, and the settings the renderer actually
//! uses once those are combined.
//!
//! These are *cost* settings, kept per machine and never in a project. The
//! world's look (Lighting) is separate: where a Lighting value is also a cost
//! (`ShadowsEnabled`, `ShadowDistance`, `PostProcessingEnabled`), the world's
//! value is a request and these settings are a ceiling.

use serde::{Deserialize, Serialize};

/// A quality preset. Each sets every graphics setting that has no override.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub enum Quality {
    Low,
    Medium,
    /// Today's look: what worlds were made with.
    #[default]
    High,
    Ultra,
}

impl Quality {
    pub const ALL: [Quality; 4] = [Quality::Low, Quality::Medium, Quality::High, Quality::Ultra];

    pub fn label(self) -> &'static str {
        match self {
            Quality::Low => "Low",
            Quality::Medium => "Medium",
            Quality::High => "High",
            Quality::Ultra => "Ultra",
        }
    }
}

/// How many projected decals are drawn, nearest the camera first, and how
/// far from it: they follow the quality preset, and Low draws none.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DecalLimits {
    pub count: u32,
    /// Studs from the camera past which a decal isn't drawn, fading out
    /// over the last tenth.
    pub distance: f32,
}

impl Quality {
    /// The projected decals this preset draws: none at Low, then 128
    /// within 150 studs, 512 within 300, and 1024 within 600 at Ultra.
    pub fn decal_limits(self) -> DecalLimits {
        let (count, distance) = match self {
            Quality::Low => (0, 0.0),
            Quality::Medium => (128, 150.0),
            Quality::High => (512, 300.0),
            Quality::Ultra => (1024, 600.0),
        };
        DecalLimits { count, distance }
    }
}

/// How much detail the sun's shadow map has: its size and how many
/// cascades cover the view.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ShadowQuality {
    /// No shadows, and no shadow map.
    Off,
    /// 1024² × 2 cascades.
    Low,
    /// 2048² × 2 cascades.
    Medium,
    /// 2048² × 3 cascades.
    High,
    /// 4096² × 4 cascades.
    Ultra,
}

impl ShadowQuality {
    pub const ALL: [ShadowQuality; 5] = [
        ShadowQuality::Off,
        ShadowQuality::Low,
        ShadowQuality::Medium,
        ShadowQuality::High,
        ShadowQuality::Ultra,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ShadowQuality::Off => "Off",
            ShadowQuality::Low => "Low",
            ShadowQuality::Medium => "Medium",
            ShadowQuality::High => "High",
            ShadowQuality::Ultra => "Ultra",
        }
    }

    /// The shadow map this quality draws, or `None` when it's off.
    pub fn map(self) -> Option<ShadowMapSize> {
        let (size, cascades) = match self {
            ShadowQuality::Off => return None,
            ShadowQuality::Low => (1024, 2),
            ShadowQuality::Medium => (2048, 2),
            ShadowQuality::High => (2048, 3),
            ShadowQuality::Ultra => (4096, 4),
        };
        Some(ShadowMapSize { size, cascades })
    }

    /// The spot light shadow map this quality draws, or `None` when spot
    /// lights cast no shadows (Off and Low).
    pub fn spot_map(self) -> Option<SpotShadowMapSize> {
        let (size, lights) = match self {
            ShadowQuality::Off | ShadowQuality::Low => return None,
            ShadowQuality::Medium => (512, 2),
            ShadowQuality::High => (1024, 4),
            ShadowQuality::Ultra => (2048, 8),
        };
        Some(SpotShadowMapSize { size, lights })
    }

    /// The point light shadow maps this quality draws, or `None` when point
    /// lights cast no shadows (Off and Low).
    pub fn point_map(self) -> Option<PointShadowMapSize> {
        let (size, lights) = match self {
            ShadowQuality::Off | ShadowQuality::Low => return None,
            ShadowQuality::Medium => (512, 1),
            ShadowQuality::High => (512, 2),
            ShadowQuality::Ultra => (1024, 4),
        };
        Some(PointShadowMapSize { size, lights })
    }

    /// Whether a GPU with `capabilities` can hold this quality's maps.
    fn fits(self, capabilities: &Capabilities) -> bool {
        let fits = |size: u32, layers: u32| {
            size <= capabilities.max_texture_size && layers <= capabilities.max_texture_layers
        };
        self.map().is_none_or(|map| fits(map.size, map.cascades))
            && self.spot_map().is_none_or(|map| fits(map.size, map.lights))
            && self
                .point_map()
                .is_none_or(|map| fits(map.size, map.lights * POINT_SHADOW_FACES))
    }
}

/// How sharp textures on surfaces are: how many of their largest mip levels
/// are left out at upload. UI images always keep their full size.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum TextureQuality {
    /// A quarter of the width and height: two levels left out.
    Quarter,
    /// Half the width and height: one level left out.
    Half,
    /// As the texture was made.
    Full,
}

impl TextureQuality {
    pub const ALL: [TextureQuality; 3] = [
        TextureQuality::Quarter,
        TextureQuality::Half,
        TextureQuality::Full,
    ];

    pub fn label(self) -> &'static str {
        match self {
            TextureQuality::Quarter => "Quarter",
            TextureQuality::Half => "Half",
            TextureQuality::Full => "Full",
        }
    }

    /// How many of a texture's largest mip levels are left out.
    pub fn skipped_levels(self) -> u32 {
        match self {
            TextureQuality::Quarter => 2,
            TextureQuality::Half => 1,
            TextureQuality::Full => 0,
        }
    }
}

/// How the scene's edges are smoothed. Ordered from cheapest to dearest,
/// then TAA, which `effective` never lowers to (it needs no more than one
/// sample per pixel, so every GPU can draw it).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum AntiAliasing {
    /// Hard, stair-stepped edges.
    Off,
    /// Blurs high-contrast edges in the finished picture: cheap, a little soft.
    Fxaa,
    /// Draws the scene with 2 or 4 samples per pixel and averages them.
    Msaa2,
    Msaa4,
    /// Moves the scene by part of a pixel each frame and blends each frame
    /// with the last ones, followed by its motion: smooths edges and thin
    /// shapes, a little soft.
    Taa,
}

impl AntiAliasing {
    pub const ALL: [AntiAliasing; 5] = [
        AntiAliasing::Off,
        AntiAliasing::Fxaa,
        AntiAliasing::Msaa2,
        AntiAliasing::Msaa4,
        AntiAliasing::Taa,
    ];

    pub fn label(self) -> &'static str {
        match self {
            AntiAliasing::Off => "Off",
            AntiAliasing::Fxaa => "FXAA",
            AntiAliasing::Msaa2 => "MSAA 2×",
            AntiAliasing::Msaa4 => "MSAA 4×",
            AntiAliasing::Taa => "TAA",
        }
    }

    /// How many samples per pixel the scene is drawn with.
    pub fn samples(self) -> u32 {
        match self {
            AntiAliasing::Off | AntiAliasing::Fxaa | AntiAliasing::Taa => 1,
            AntiAliasing::Msaa2 => 2,
            AntiAliasing::Msaa4 => 4,
        }
    }
}

/// Screen-space ambient occlusion: how much the corners and creases between
/// solid items darken their ambient and sky light. Ordered from cheapest
/// to dearest.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum AmbientOcclusion {
    /// No darkening, and no prepass or ambient occlusion targets.
    Off,
    /// 8 samples per pixel.
    Low,
    /// 16 samples per pixel.
    High,
}

impl AmbientOcclusion {
    pub const ALL: [AmbientOcclusion; 3] = [
        AmbientOcclusion::Off,
        AmbientOcclusion::Low,
        AmbientOcclusion::High,
    ];

    pub fn label(self) -> &'static str {
        match self {
            AmbientOcclusion::Off => "Off",
            AmbientOcclusion::Low => "Low",
            AmbientOcclusion::High => "High",
        }
    }

    /// How many points around each pixel are tested for what hides it: 0
    /// when it's off.
    pub fn samples(self) -> u32 {
        match self {
            AmbientOcclusion::Off => 0,
            AmbientOcclusion::Low => 8,
            AmbientOcclusion::High => 16,
        }
    }
}

/// How surfaces reflect the sky and take its light. Ordered from cheapest
/// to dearest.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ReflectionQuality {
    /// From the sky's gradient worked out per pixel, with no environment
    /// maps. A sky image isn't shown.
    Low,
    /// The sky baked into a 64² cubemap, filtered with 32 samples.
    Medium,
    /// 128², 64 samples.
    High,
    /// 256², 128 samples.
    Ultra,
}

impl ReflectionQuality {
    pub const ALL: [ReflectionQuality; 4] = [
        ReflectionQuality::Low,
        ReflectionQuality::Medium,
        ReflectionQuality::High,
        ReflectionQuality::Ultra,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ReflectionQuality::Low => "Low",
            ReflectionQuality::Medium => "Medium",
            ReflectionQuality::High => "High",
            ReflectionQuality::Ultra => "Ultra",
        }
    }

    /// The environment maps this quality bakes, or `None` for the
    /// gradient alone (Low).
    pub fn map(self) -> Option<EnvironmentMapSize> {
        let (size, samples) = match self {
            ReflectionQuality::Low => return None,
            ReflectionQuality::Medium => (64, 32),
            ReflectionQuality::High => (128, 64),
            ReflectionQuality::Ultra => (256, 128),
        };
        Some(EnvironmentMapSize { size, samples })
    }
}

/// How a `Clouds` layer is drawn. Ordered from cheapest to dearest. Every
/// quality draws the same clouds and their shadows; the dearer ones march
/// through them for depth and lighting.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum CloudQuality {
    /// A flat sheet of cloud, one look-up per pixel of sky.
    Low,
    /// Marched through in 16 steps, lit by the cloud's thickness above.
    Medium,
    /// 32 steps with fine detail, lit by 3 steps toward the sun.
    High,
    /// 64 steps with fine detail, lit by 6 steps toward the sun.
    Ultra,
}

impl CloudQuality {
    pub const ALL: [CloudQuality; 4] = [
        CloudQuality::Low,
        CloudQuality::Medium,
        CloudQuality::High,
        CloudQuality::Ultra,
    ];

    pub fn label(self) -> &'static str {
        match self {
            CloudQuality::Low => "Low",
            CloudQuality::Medium => "Medium",
            CloudQuality::High => "High",
            CloudQuality::Ultra => "Ultra",
        }
    }

    /// Steps through the cloud layer per pixel (0 for the flat sheet), and
    /// steps toward the sun from each (0 to judge it by the cloud's
    /// thickness above).
    pub fn steps(self) -> (u32, u32) {
        match self {
            CloudQuality::Low => (0, 0),
            CloudQuality::Medium => (16, 0),
            CloudQuality::High => (32, 3),
            CloudQuality::Ultra => (64, 6),
        }
    }
}

/// How a `DepthOfFieldEffect` blurs what's out of focus. Ordered from
/// cheapest to dearest.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum DepthOfFieldQuality {
    /// No blur, and no prepass or blur target for it.
    Off,
    /// 16 samples per pixel.
    Low,
    /// 32 samples per pixel.
    High,
}

impl DepthOfFieldQuality {
    pub const ALL: [DepthOfFieldQuality; 3] = [
        DepthOfFieldQuality::Off,
        DepthOfFieldQuality::Low,
        DepthOfFieldQuality::High,
    ];

    pub fn label(self) -> &'static str {
        match self {
            DepthOfFieldQuality::Off => "Off",
            DepthOfFieldQuality::Low => "Low",
            DepthOfFieldQuality::High => "High",
        }
    }

    /// How many points around each pixel its blur averages: 0 when it's off.
    pub fn samples(self) -> u32 {
        match self {
            DepthOfFieldQuality::Off => 0,
            DepthOfFieldQuality::Low => 16,
            DepthOfFieldQuality::High => 32,
        }
    }
}

/// Hardware ray traced sun shadows (`docs/rendering/ray-tracing.md`):
/// rays toward the sun from each solid pixel, in place of the shadow maps
/// near the camera. Needs a GPU that can ray trace. Ordered from cheapest
/// to dearest. The world's `ShadowSoftness` still sets how soft they are;
/// more rays make soft edges smoother.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum RayTracedShadows {
    /// The shadow maps alone.
    Off,
    /// One ray per pixel, soft edges smoothed.
    Low,
    /// Two rays per pixel.
    Medium,
    /// Four rays per pixel.
    High,
}

impl RayTracedShadows {
    pub const ALL: [RayTracedShadows; 4] = [
        RayTracedShadows::Off,
        RayTracedShadows::Low,
        RayTracedShadows::Medium,
        RayTracedShadows::High,
    ];

    pub fn label(self) -> &'static str {
        match self {
            RayTracedShadows::Off => "Off",
            RayTracedShadows::Low => "Low",
            RayTracedShadows::Medium => "Medium",
            RayTracedShadows::High => "High",
        }
    }

    /// Rays per pixel: 0 when off. Hard shadows (a world's `ShadowSoftness`
    /// of 0) need only one, whatever this says.
    pub fn rays(self) -> u32 {
        match self {
            RayTracedShadows::Off => 0,
            RayTracedShadows::Low => 1,
            RayTracedShadows::Medium => 2,
            RayTracedShadows::High => 4,
        }
    }
}

/// Hardware ray traced ambient occlusion (`docs/rendering/ray-tracing.md`):
/// short rays over the hemisphere above each solid pixel find what hides
/// it, in place of the screen-space guess, so things off screen or behind
/// others count too. Needs a GPU that can ray trace. Ordered from cheapest
/// to dearest; it reaches as far as screen-space ambient occlusion does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum RayTracedAmbientOcclusion {
    /// Screen-space ambient occlusion alone (as its own setting says).
    Off,
    /// Two rays per pixel, at half the scene's size, smoothed.
    Low,
    /// Four rays per pixel.
    Medium,
    /// Eight rays per pixel.
    High,
}

impl RayTracedAmbientOcclusion {
    pub const ALL: [RayTracedAmbientOcclusion; 4] = [
        RayTracedAmbientOcclusion::Off,
        RayTracedAmbientOcclusion::Low,
        RayTracedAmbientOcclusion::Medium,
        RayTracedAmbientOcclusion::High,
    ];

    pub fn label(self) -> &'static str {
        match self {
            RayTracedAmbientOcclusion::Off => "Off",
            RayTracedAmbientOcclusion::Low => "Low",
            RayTracedAmbientOcclusion::Medium => "Medium",
            RayTracedAmbientOcclusion::High => "High",
        }
    }

    /// Rays per pixel: 0 when off.
    pub fn rays(self) -> u32 {
        match self {
            RayTracedAmbientOcclusion::Off => 0,
            RayTracedAmbientOcclusion::Low => 2,
            RayTracedAmbientOcclusion::Medium => 4,
            RayTracedAmbientOcclusion::High => 8,
        }
    }
}

/// Hardware ray traced reflections (RL4, `docs/rendering/ray-tracing.md`):
/// a ray along the mirrored view from each glossy solid pixel, through the
/// ray traced scene, so a shiny floor or a car shows what's really beside
/// it (things off screen too) instead of the nearest reflection probe.
/// Surfaces rougher than the setting's cutoff keep the probes. Needs a GPU
/// that can ray trace. Ordered from cheapest to dearest.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum RayTracedReflections {
    /// Reflection probes and the sky alone (as Reflection quality says).
    Off,
    /// Mirror-like surfaces (roughness below 0.15), at half the scene's
    /// size.
    Low,
    /// Roughness below 0.3, at the scene's size.
    Medium,
    /// Roughness below 0.5, at the scene's size.
    High,
}

impl RayTracedReflections {
    pub const ALL: [RayTracedReflections; 4] = [
        RayTracedReflections::Off,
        RayTracedReflections::Low,
        RayTracedReflections::Medium,
        RayTracedReflections::High,
    ];

    pub fn label(self) -> &'static str {
        match self {
            RayTracedReflections::Off => "Off",
            RayTracedReflections::Low => "Low",
            RayTracedReflections::Medium => "Medium",
            RayTracedReflections::High => "High",
        }
    }

    /// The roughness from which surfaces keep the reflection probes: the
    /// traced reflection fades out toward it. 0 when off.
    pub fn roughness_cutoff(self) -> f32 {
        match self {
            RayTracedReflections::Off => 0.0,
            RayTracedReflections::Low => 0.15,
            RayTracedReflections::Medium => 0.3,
            RayTracedReflections::High => 0.5,
        }
    }

    /// How many times smaller than the scene the rays are traced, each way.
    pub fn downscale(self) -> u32 {
        match self {
            RayTracedReflections::Low => 2,
            _ => 1,
        }
    }
}

/// Ray traced global illumination (`docs/rendering/ray-tracing.md`): light
/// probes traced with rays each frame, in place of real-time GI's drawn
/// faces, so bounced light follows the sun, lamps, sky and moving things
/// within a second. Traced with the GPU's ray queries, or in software on a
/// GPU without them (RL3; slower). Ordered from cheapest to dearest.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum RayTracedGlobalIllumination {
    /// Light probes keep their bakes, or real-time GI's drawn faces (as its
    /// own setting says).
    Off,
    /// 128 probes a frame, 64 rays each.
    Low,
    /// 256 probes a frame, 128 rays each.
    Medium,
    /// 512 probes a frame, 256 rays each.
    High,
}

impl RayTracedGlobalIllumination {
    pub const ALL: [RayTracedGlobalIllumination; 4] = [
        RayTracedGlobalIllumination::Off,
        RayTracedGlobalIllumination::Low,
        RayTracedGlobalIllumination::Medium,
        RayTracedGlobalIllumination::High,
    ];

    pub fn label(self) -> &'static str {
        match self {
            RayTracedGlobalIllumination::Off => "Off",
            RayTracedGlobalIllumination::Low => "Low",
            RayTracedGlobalIllumination::Medium => "Medium",
            RayTracedGlobalIllumination::High => "High",
        }
    }

    /// Probes traced each frame: 0 when off.
    pub fn probes_per_frame(self) -> u32 {
        match self {
            RayTracedGlobalIllumination::Off => 0,
            RayTracedGlobalIllumination::Low => 128,
            RayTracedGlobalIllumination::Medium => 256,
            RayTracedGlobalIllumination::High => 512,
        }
    }

    /// Rays traced from each probe: 0 when off.
    pub fn rays(self) -> u32 {
        match self {
            RayTracedGlobalIllumination::Off => 0,
            RayTracedGlobalIllumination::Low => 64,
            RayTracedGlobalIllumination::Medium => 128,
            RayTracedGlobalIllumination::High => 256,
        }
    }
}

/// How a `MotionBlurEffect` streaks what moves across the view. Ordered
/// from cheapest to dearest.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum MotionBlurQuality {
    /// No streaks, and no motion or blur targets for them.
    Off,
    /// 8 samples per pixel.
    Low,
    /// 16 samples per pixel.
    High,
}

impl MotionBlurQuality {
    pub const ALL: [MotionBlurQuality; 3] = [
        MotionBlurQuality::Off,
        MotionBlurQuality::Low,
        MotionBlurQuality::High,
    ];

    pub fn label(self) -> &'static str {
        match self {
            MotionBlurQuality::Off => "Off",
            MotionBlurQuality::Low => "Low",
            MotionBlurQuality::High => "High",
        }
    }

    /// How many points along its streak each pixel averages: 0 when it's off.
    pub fn samples(self) -> u32 {
        match self {
            MotionBlurQuality::Off => 0,
            MotionBlurQuality::Low => 8,
            MotionBlurQuality::High => 16,
        }
    }
}

/// The smallest and largest render scale: the scene is drawn at this
/// fraction of the output's width and height.
pub const RENDER_SCALE_RANGE: std::ops::RangeInclusive<f32> = 0.5..=2.0;

/// The smallest and largest texture memory budget an override can set, in
/// megabytes (MiB).
pub const TEXTURE_BUDGET_RANGE: std::ops::RangeInclusive<u32> = 16..=65_536;

/// The nearest and farthest ray tracing can reach from the camera, in
/// studs: what's past it is left out of the ray traced scene.
pub const RAY_TRACING_DISTANCE_RANGE: std::ops::RangeInclusive<f32> = 50.0..=5000.0;

/// A shadow map's size in texels per side, and its number of cascades
/// (one layer each).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ShadowMapSize {
    pub size: u32,
    pub cascades: u32,
}

/// Spot light shadow maps: each one's size in texels per side, and how many
/// spot lights near the camera get one (one layer each).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SpotShadowMapSize {
    pub size: u32,
    pub lights: u32,
}

/// Point light shadow maps: the size of each of a light's six faces (one
/// per direction along the axes) in texels per side, and how many point
/// lights near the camera get them (six layers each).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PointShadowMapSize {
    pub size: u32,
    pub lights: u32,
}

/// How many views of the world a point light's shadows are drawn from: one
/// along each way of each axis.
pub const POINT_SHADOW_FACES: u32 = 6;

/// How much real-time GI traces each frame (roadmap R18.4): how many light
/// probes, and the size of each of a probe's six faces in texels per side.
/// It follows the preset while real-time GI is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RealtimeGiTier {
    pub probes_per_frame: u32,
    pub face_size: u32,
}

impl RealtimeGiTier {
    /// The most probes any tier traces in a frame.
    pub const MAX_PROBES_PER_FRAME: u32 = 4;

    /// The tier of `quality`: Low traces one probe a frame at 8² a face,
    /// Medium two at 8², High two at 16² (a bake's size, what real-time GI
    /// always did before tiers) and Ultra four at 16².
    pub fn of(quality: Quality) -> Self {
        let (probes_per_frame, face_size) = match quality {
            Quality::Low => (1, 8),
            Quality::Medium => (2, 8),
            Quality::High => (2, 16),
            Quality::Ultra => (4, 16),
        };
        Self {
            probes_per_frame,
            face_size,
        }
    }
}

/// The sky's environment maps: each cubemap face's size in texels per
/// side, and how many directions each texel of a rough reflection averages.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EnvironmentMapSize {
    pub size: u32,
    pub samples: u32,
}

/// The machine's graphics choices: a preset, and overrides that each
/// replace one of its values. Saved per machine (the editor's settings, the
/// player's `graphics.json`), never in a project.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GraphicsSettings {
    pub quality: Quality,
    /// True when `quality` follows the machine (see
    /// [`GraphicsSettings::automatic`]) instead of being picked, so it's
    /// picked again on each start.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub automatic: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shadow_quality: Option<ShadowQuality>,
    /// The farthest shadows reach, in studs: a ceiling on the world's
    /// `ShadowDistance`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shadow_distance: Option<f32>,
    /// Off turns bloom and tone mapping off whatever the world asks for.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub post_processing: Option<bool>,
    /// The scene's size as a fraction of the output's (0.5 to 2). The UI is
    /// always drawn at the output's size.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub render_scale: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub texture_quality: Option<TextureQuality>,
    /// The most GPU memory textures may take, in megabytes (MiB). Over it,
    /// the least recently drawn surface textures lose their largest mips.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub texture_budget: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anti_aliasing: Option<AntiAliasing>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ambient_occlusion: Option<AmbientOcclusion>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reflection_quality: Option<ReflectionQuality>,
    /// Short shadows found on screen, for the gaps the shadow maps miss.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contact_shadows: Option<bool>,
    /// Light probe volumes kept up to date as the world's light changes,
    /// a few probes each frame. Off at every preset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub realtime_gi: Option<bool>,
    /// The sun's light in fog shadowed (sun shafts), and lamps lighting it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volumetric_fog: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cloud_quality: Option<CloudQuality>,
    /// The lens effects a world can ask for (`DepthOfFieldEffect`,
    /// `VignetteEffect`, `FilmGrainEffect`, `ChromaticAberrationEffect`,
    /// `SharpenEffect`, `SunRaysEffect`), each drawn only if its setting
    /// allows it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depth_of_field: Option<DepthOfFieldQuality>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vignette: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub film_grain: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chromatic_aberration: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sharpen: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sun_rays: Option<bool>,
    /// How a world's `MotionBlurEffect` is drawn.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub motion_blur: Option<MotionBlurQuality>,
    /// How far from the camera ray traced effects see, in studs: items
    /// farther away are left out of the ray traced scene. Only used while
    /// a ray traced effect is on (`docs/rendering/ray-tracing.md`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ray_tracing_distance: Option<f32>,
    /// Hardware ray traced sun shadows, on a GPU that can ray trace. Off at
    /// every preset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ray_traced_shadows: Option<RayTracedShadows>,
    /// Hardware ray traced ambient occlusion, on a GPU that can ray trace.
    /// Off at every preset. On, it draws ambient occlusion even while the
    /// screen-space setting is off.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ray_traced_ambient_occlusion: Option<RayTracedAmbientOcclusion>,
    /// Hardware ray traced global illumination, on a GPU that can ray
    /// trace. Off at every preset. On, it traces light probe volumes even
    /// while real-time GI is off.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ray_traced_global_illumination: Option<RayTracedGlobalIllumination>,
    /// Hardware ray traced reflections (RL4), on a GPU that can ray trace.
    /// Off at every preset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ray_traced_reflections: Option<RayTracedReflections>,
    /// Whether the GPU is opened with hardware ray tracing at the next
    /// start: off unless turned on (at every preset), since opening it
    /// changes the device even while no ray traced effect is on. Read once,
    /// before the device exists (`render::adapters::want_ray_tracing`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ray_tracing: Option<bool>,
}

impl GraphicsSettings {
    /// `quality` with no overrides.
    pub fn preset(quality: Quality) -> Self {
        Self {
            quality,
            ..Self::default()
        }
    }

    /// The settings a machine starts with: [`GraphicsSettings::automatic`].
    pub fn default_for(capabilities: &Capabilities) -> Self {
        Self::automatic(capabilities)
    }

    /// The quality that suits the machine, with no overrides: High, or Low
    /// on a software adapter (one that draws on the CPU).
    pub fn automatic(capabilities: &Capabilities) -> Self {
        Self {
            automatic: true,
            ..Self::preset(if capabilities.software {
                Quality::Low
            } else {
                Quality::High
            })
        }
    }

    /// The shadow quality: the override, or the preset's.
    pub fn shadow_quality(&self) -> ShadowQuality {
        self.shadow_quality.unwrap_or(match self.quality {
            Quality::Low => ShadowQuality::Low,
            Quality::Medium => ShadowQuality::Medium,
            Quality::High => ShadowQuality::High,
            Quality::Ultra => ShadowQuality::Ultra,
        })
    }

    /// The shadow distance ceiling in studs: the override, or the preset's.
    /// An override that isn't a number of 0 or more is ignored.
    pub fn shadow_distance(&self) -> f32 {
        let preset = match self.quality {
            Quality::Low => 75.0,
            Quality::Medium => 100.0,
            Quality::High => 150.0,
            Quality::Ultra => 300.0,
        };
        self.shadow_distance
            .filter(|distance| distance.is_finite() && *distance >= 0.0)
            .unwrap_or(preset)
    }

    /// Whether post-processing may run: the override, or the preset's.
    pub fn post_processing(&self) -> bool {
        self.post_processing.unwrap_or(self.quality != Quality::Low)
    }

    /// The render scale: the override, kept within [`RENDER_SCALE_RANGE`], or
    /// the preset's. An override that isn't a number is ignored.
    pub fn render_scale(&self) -> f32 {
        let preset = if self.quality == Quality::Low {
            0.75
        } else {
            1.0
        };
        self.render_scale
            .filter(|scale| scale.is_finite())
            .map_or(preset, |scale| {
                scale.clamp(*RENDER_SCALE_RANGE.start(), *RENDER_SCALE_RANGE.end())
            })
    }

    /// The texture quality: the override, or the preset's.
    pub fn texture_quality(&self) -> TextureQuality {
        self.texture_quality.unwrap_or(match self.quality {
            Quality::Low => TextureQuality::Quarter,
            Quality::Medium => TextureQuality::Half,
            Quality::High | Quality::Ultra => TextureQuality::Full,
        })
    }

    /// The texture memory budget in megabytes (MiB): the override, kept
    /// within [`TEXTURE_BUDGET_RANGE`], or the preset's.
    pub fn texture_budget(&self) -> u32 {
        self.texture_budget.map_or(
            match self.quality {
                Quality::Low => 256,
                Quality::Medium => 512,
                Quality::High => 1024,
                Quality::Ultra => 2048,
            },
            |budget| budget.clamp(*TEXTURE_BUDGET_RANGE.start(), *TEXTURE_BUDGET_RANGE.end()),
        )
    }

    /// The anti-aliasing: the override, or the preset's.
    pub fn anti_aliasing(&self) -> AntiAliasing {
        self.anti_aliasing.unwrap_or(match self.quality {
            Quality::Low => AntiAliasing::Off,
            Quality::Medium => AntiAliasing::Fxaa,
            Quality::High | Quality::Ultra => AntiAliasing::Msaa4,
        })
    }

    /// The ambient occlusion: the override, or the preset's.
    pub fn ambient_occlusion(&self) -> AmbientOcclusion {
        self.ambient_occlusion.unwrap_or(match self.quality {
            Quality::Low => AmbientOcclusion::Off,
            Quality::Medium => AmbientOcclusion::Low,
            Quality::High | Quality::Ultra => AmbientOcclusion::High,
        })
    }

    /// The reflection quality: the override, or the preset's.
    pub fn reflection_quality(&self) -> ReflectionQuality {
        self.reflection_quality.unwrap_or(match self.quality {
            Quality::Low => ReflectionQuality::Low,
            Quality::Medium => ReflectionQuality::Medium,
            Quality::High => ReflectionQuality::High,
            Quality::Ultra => ReflectionQuality::Ultra,
        })
    }

    /// Whether contact shadows are drawn: the override, or the preset's (on
    /// at High and Ultra).
    pub fn contact_shadows(&self) -> bool {
        self.contact_shadows
            .unwrap_or(self.quality >= Quality::High)
    }

    /// Whether light probe volumes are traced again as the light changes:
    /// the override, or off (every preset's).
    pub fn realtime_gi(&self) -> bool {
        self.realtime_gi.unwrap_or(false)
    }

    /// How much real-time GI traces each frame while it's on: the preset's
    /// tier.
    pub fn realtime_gi_tier(&self) -> RealtimeGiTier {
        RealtimeGiTier::of(self.quality)
    }

    /// Whether fog is drawn volumetric: the override, or the preset's (on
    /// at Ultra).
    pub fn volumetric_fog(&self) -> bool {
        self.volumetric_fog
            .unwrap_or(self.quality == Quality::Ultra)
    }

    /// How clouds are drawn: the override, or the preset's (its own name).
    pub fn cloud_quality(&self) -> CloudQuality {
        self.cloud_quality.unwrap_or(match self.quality {
            Quality::Low => CloudQuality::Low,
            Quality::Medium => CloudQuality::Medium,
            Quality::High => CloudQuality::High,
            Quality::Ultra => CloudQuality::Ultra,
        })
    }

    /// How depth of field is drawn: the override, or the preset's (off at
    /// Low, Low at Medium, High above).
    pub fn depth_of_field(&self) -> DepthOfFieldQuality {
        self.depth_of_field.unwrap_or(match self.quality {
            Quality::Low => DepthOfFieldQuality::Off,
            Quality::Medium => DepthOfFieldQuality::Low,
            Quality::High | Quality::Ultra => DepthOfFieldQuality::High,
        })
    }

    /// Whether a vignette is drawn: the override, or the preset's (on above
    /// Low). The other lens effects' switches follow the same presets.
    pub fn vignette(&self) -> bool {
        self.vignette.unwrap_or(self.quality != Quality::Low)
    }

    pub fn film_grain(&self) -> bool {
        self.film_grain.unwrap_or(self.quality != Quality::Low)
    }

    pub fn chromatic_aberration(&self) -> bool {
        self.chromatic_aberration
            .unwrap_or(self.quality != Quality::Low)
    }

    pub fn sharpen(&self) -> bool {
        self.sharpen.unwrap_or(self.quality != Quality::Low)
    }

    pub fn sun_rays(&self) -> bool {
        self.sun_rays.unwrap_or(self.quality != Quality::Low)
    }

    /// How motion blur is drawn: the override, or the preset's (off at Low,
    /// Low at Medium, High above).
    pub fn motion_blur(&self) -> MotionBlurQuality {
        self.motion_blur.unwrap_or(match self.quality {
            Quality::Low => MotionBlurQuality::Off,
            Quality::Medium => MotionBlurQuality::Low,
            Quality::High | Quality::Ultra => MotionBlurQuality::High,
        })
    }

    /// Whether the GPU is opened with hardware ray tracing at the next
    /// start: the override, or off.
    pub fn ray_tracing(&self) -> bool {
        self.ray_tracing.unwrap_or(false)
    }

    /// How far ray traced effects see, in studs: the override, kept within
    /// [`RAY_TRACING_DISTANCE_RANGE`], or the preset's. An override that
    /// isn't a number is ignored.
    pub fn ray_tracing_distance(&self) -> f32 {
        let preset = match self.quality {
            Quality::Low => 150.0,
            Quality::Medium => 250.0,
            Quality::High => 400.0,
            Quality::Ultra => 800.0,
        };
        self.ray_tracing_distance
            .filter(|distance| distance.is_finite())
            .map_or(preset, |distance| {
                distance.clamp(
                    *RAY_TRACING_DISTANCE_RANGE.start(),
                    *RAY_TRACING_DISTANCE_RANGE.end(),
                )
            })
    }

    /// Hardware ray traced sun shadows: the override, or Off (every preset).
    pub fn ray_traced_shadows(&self) -> RayTracedShadows {
        self.ray_traced_shadows.unwrap_or(RayTracedShadows::Off)
    }

    /// These settings with the ray traced look turned on in one go
    /// (Settings > Graphics > Ray tracing's "Use the ray traced look"):
    /// Hardware ray tracing on, ray traced shadows and ambient occlusion at
    /// Medium, and what they look best with: shadows and reflections at
    /// least High (reflections show a sky picture) and TAA, which averages
    /// the rays that turn each frame. Everything else stays as it was.
    /// On a GPU that can't ray trace, `effective` turns the ray traced parts
    /// off again with a note, and the rest still applies.
    pub fn ray_traced(&self) -> GraphicsSettings {
        GraphicsSettings {
            ray_tracing: Some(true),
            ray_traced_shadows: Some(RayTracedShadows::Medium),
            ray_traced_ambient_occlusion: Some(RayTracedAmbientOcclusion::Medium),
            shadow_quality: Some(self.shadow_quality().max(ShadowQuality::High)),
            reflection_quality: Some(self.reflection_quality().max(ReflectionQuality::High)),
            anti_aliasing: Some(AntiAliasing::Taa),
            ..self.clone()
        }
    }

    /// These settings with every ray traced effect off (back to their
    /// presets, which are all Off). Hardware ray tracing itself stays as
    /// it was, since turning it off takes a restart.
    pub fn without_ray_traced_effects(&self) -> GraphicsSettings {
        GraphicsSettings {
            ray_traced_shadows: None,
            ray_traced_ambient_occlusion: None,
            ray_traced_global_illumination: None,
            ray_traced_reflections: None,
            ..self.clone()
        }
    }

    /// Hardware ray traced reflections: the override, or Off (every preset).
    pub fn ray_traced_reflections(&self) -> RayTracedReflections {
        self.ray_traced_reflections
            .unwrap_or(RayTracedReflections::Off)
    }

    /// Hardware ray traced ambient occlusion: the override, or Off (every
    /// preset).
    pub fn ray_traced_ambient_occlusion(&self) -> RayTracedAmbientOcclusion {
        self.ray_traced_ambient_occlusion
            .unwrap_or(RayTracedAmbientOcclusion::Off)
    }

    /// Hardware ray traced global illumination: the override, or Off
    /// (every preset).
    pub fn ray_traced_global_illumination(&self) -> RayTracedGlobalIllumination {
        self.ray_traced_global_illumination
            .unwrap_or(RayTracedGlobalIllumination::Off)
    }

    /// The values the renderer uses: these settings, lowered where the GPU
    /// can't meet them, with a note for each one lowered. Never raises one.
    pub fn effective(&self, capabilities: &Capabilities) -> EffectiveGraphics {
        let mut notes = Vec::new();
        let wanted = self.shadow_quality();
        // The best quality at or below the one asked for whose map fits.
        let shadow_quality = ShadowQuality::ALL
            .into_iter()
            .rev()
            .filter(|&quality| quality <= wanted)
            .find(|quality| quality.fits(capabilities))
            .unwrap_or(ShadowQuality::Off);
        if shadow_quality != wanted {
            notes.push(format!(
                "Shadow quality lowered from {} to {}: this GPU's textures are at most {}² \
                 with {} layers.",
                wanted.label(),
                shadow_quality.label(),
                capabilities.max_texture_size,
                capabilities.max_texture_layers,
            ));
        }
        let wanted = self.anti_aliasing();
        // The best mode at or below the one asked for with a sample count
        // the GPU has. FXAA and Off draw with one sample, which every GPU has.
        let anti_aliasing = AntiAliasing::ALL
            .into_iter()
            .rev()
            .filter(|&mode| mode <= wanted)
            .find(|mode| capabilities.supports_samples(mode.samples()))
            .unwrap_or(AntiAliasing::Off);
        if anti_aliasing != wanted {
            notes.push(format!(
                "Anti-aliasing lowered from {} to {}: this GPU can't draw the scene with {} \
                 samples per pixel.",
                wanted.label(),
                anti_aliasing.label(),
                wanted.samples(),
            ));
        }
        let wanted = self.reflection_quality();
        // The best quality at or below the one asked for whose cubemap fits.
        let reflection_quality = ReflectionQuality::ALL
            .into_iter()
            .rev()
            .filter(|&quality| quality <= wanted)
            .find(|quality| {
                quality
                    .map()
                    .is_none_or(|map| map.size <= capabilities.max_texture_size)
            })
            .unwrap_or(ReflectionQuality::Low);
        if reflection_quality != wanted {
            notes.push(format!(
                "Reflection quality lowered from {} to {}: this GPU's textures are at most {}².",
                wanted.label(),
                reflection_quality.label(),
                capabilities.max_texture_size,
            ));
        }
        // Real-time GI sums each probe's light in a compute shader.
        let realtime_gi = self.realtime_gi() && capabilities.compute_shaders;
        if self.realtime_gi() && !realtime_gi {
            notes.push(
                "Real-time GI is off: this GPU can't run compute shaders. Light probe volumes \
                 keep their baked light."
                    .to_owned(),
            );
        }
        // Its grid is worked out by compute shaders.
        let volumetric_fog = self.volumetric_fog() && capabilities.compute_shaders;
        if self.volumetric_fog() && !volumetric_fog {
            notes.push(
                "Volumetric fog is off: this GPU can't run compute shaders. Fog is drawn without \
                 sun shafts."
                    .to_owned(),
            );
        }
        // Ray traced effects need hardware ray queries.
        let ray_traced_shadows = if capabilities.ray_queries {
            self.ray_traced_shadows()
        } else {
            RayTracedShadows::Off
        };
        if ray_traced_shadows != self.ray_traced_shadows() {
            notes.push(
                "Ray traced shadows are off: Hardware ray tracing is off or this GPU can't ray trace. The shadow maps are used."
                    .to_owned(),
            );
        }
        let ray_traced_ambient_occlusion = if capabilities.ray_queries {
            self.ray_traced_ambient_occlusion()
        } else {
            RayTracedAmbientOcclusion::Off
        };
        if ray_traced_ambient_occlusion != self.ray_traced_ambient_occlusion() {
            notes.push(
                "Ray traced ambient occlusion is off: Hardware ray tracing is off or this GPU can't ray trace. Screen-space ambient occlusion is used as set."
                    .to_owned(),
            );
        }
        let ray_traced_reflections = if capabilities.ray_queries {
            self.ray_traced_reflections()
        } else {
            RayTracedReflections::Off
        };
        if ray_traced_reflections != self.ray_traced_reflections() {
            notes.push(
                "Ray traced reflections are off: Hardware ray tracing is off or this GPU can't ray \
                 trace. Reflection probes are used."
                    .to_owned(),
            );
        }
        // Without ray queries, GI's rays are traced in software (RL3), in a
        // compute shader.
        let wanted_gi = self.ray_traced_global_illumination();
        let ray_traced_gi_in_software = wanted_gi != RayTracedGlobalIllumination::Off
            && !capabilities.ray_queries
            && capabilities.compute_shaders;
        let ray_traced_global_illumination =
            if capabilities.ray_queries || ray_traced_gi_in_software {
                wanted_gi
            } else {
                RayTracedGlobalIllumination::Off
            };
        if ray_traced_global_illumination != wanted_gi {
            notes.push(
                "Ray traced GI is off: this GPU can't ray trace or run compute shaders. Light \
                 probes keep their bakes."
                    .to_owned(),
            );
        }
        if ray_traced_gi_in_software {
            notes.push(
                "Ray traced GI is traced in software: Hardware ray tracing is off or this GPU \
                 can't ray trace. It costs more GPU time, and cut-outs' holes are guessed."
                    .to_owned(),
            );
        }
        EffectiveGraphics {
            shadow_quality,
            shadow_distance: self.shadow_distance(),
            post_processing: self.post_processing(),
            render_scale: self.render_scale(),
            texture_quality: self.texture_quality(),
            texture_budget: u64::from(self.texture_budget()) * 1024 * 1024,
            anti_aliasing,
            // Every GPU can draw it: it needs nothing past what the scene does.
            ambient_occlusion: self.ambient_occlusion(),
            reflection_quality,
            // Like ambient occlusion, it reads only the prepass.
            contact_shadows: self.contact_shadows(),
            realtime_gi,
            realtime_gi_tier: self.realtime_gi_tier(),
            volumetric_fog,
            // Every GPU can draw it: it's worked out in the sky's pixels.
            cloud_quality: self.cloud_quality(),
            // Every GPU can draw these: full-screen passes over targets the
            // scene already has, and the prepass's depth.
            depth_of_field: self.depth_of_field(),
            vignette: self.vignette(),
            film_grain: self.film_grain(),
            chromatic_aberration: self.chromatic_aberration(),
            sharpen: self.sharpen(),
            sun_rays: self.sun_rays(),
            // Like depth of field: full-screen passes over the prepass's
            // depth and the motion it finds.
            motion_blur: self.motion_blur(),
            // Every GPU can draw them: boxes read against the prepass.
            decals: self.quality.decal_limits(),
            ray_queries: capabilities.ray_queries,
            ray_tracing_distance: self.ray_tracing_distance(),
            ray_traced_shadows,
            ray_traced_ambient_occlusion,
            ray_traced_global_illumination,
            ray_traced_gi_in_software,
            ray_traced_reflections,
            notes,
        }
    }
}

/// What the GPU device can do, read once when a renderer is made.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Capabilities {
    /// The widest and tallest a 2D texture can be, in texels.
    pub max_texture_size: u32,
    /// The most layers a texture array can have.
    pub max_texture_layers: u32,
    /// Whether the GPU's time per frame can be measured.
    pub timestamp_queries: bool,
    /// True when the adapter draws on the CPU, not a graphics card.
    pub software: bool,
    /// The samples per pixel the scene's color and depth can both be drawn
    /// with, as bits: `n` is set when `n` samples work (1 always is).
    pub msaa_samples: u32,
    /// Whether compute shaders can run (with a storage buffer to write).
    pub compute_shaders: bool,
    /// Whether the GPU traces rays in hardware through wgpu's ray queries
    /// (and the device was opened with them), so ray traced effects can
    /// be turned on.
    pub ray_queries: bool,
    /// Whether the GPU could trace rays in hardware, whether or not the
    /// device was opened with them: a game can offer Hardware ray tracing
    /// (taking effect at its next start) only where this is true.
    pub ray_tracing_offered: bool,
}

impl Capabilities {
    /// Whether the scene can be drawn with `samples` per pixel.
    pub fn supports_samples(&self, samples: u32) -> bool {
        samples == 1 || (samples.is_power_of_two() && self.msaa_samples & samples != 0)
    }
}

impl Default for Capabilities {
    /// wgpu's default limits on a graphics card. `render` checks these
    /// numbers against wgpu's own (`read_capabilities`'s tests).
    fn default() -> Self {
        Self {
            max_texture_size: 8192,
            max_texture_layers: 256,
            timestamp_queries: false,
            software: false,
            // 1× and the 4× wgpu guarantees for the scene's formats.
            msaa_samples: 1 | 4,
            compute_shaders: true,
            // An optional feature: wgpu's defaults have none.
            ray_queries: false,
            ray_tracing_offered: false,
        }
    }
}

/// The graphics values the renderer uses, from
/// [`GraphicsSettings::effective`].
#[derive(Clone, Debug, PartialEq)]
pub struct EffectiveGraphics {
    pub shadow_quality: ShadowQuality,
    /// The farthest shadows reach, in studs (a ceiling on the world's).
    pub shadow_distance: f32,
    pub post_processing: bool,
    /// The scene's size as a fraction of the output's. A scene too big for
    /// the GPU's textures is drawn at the largest size it can hold.
    pub render_scale: f32,
    pub texture_quality: TextureQuality,
    /// The most bytes textures may take on the GPU.
    pub texture_budget: u64,
    pub anti_aliasing: AntiAliasing,
    pub ambient_occlusion: AmbientOcclusion,
    pub reflection_quality: ReflectionQuality,
    pub contact_shadows: bool,
    /// Whether light probe volumes are traced again as the light changes.
    pub realtime_gi: bool,
    /// How much real-time GI traces each frame while it's on.
    pub realtime_gi_tier: RealtimeGiTier,
    /// Whether fog shows the sun's shadows and lamps' light (needs the
    /// prepass's depth).
    pub volumetric_fog: bool,
    pub cloud_quality: CloudQuality,
    /// Depth of field's samples (needs the prepass's depth).
    pub depth_of_field: DepthOfFieldQuality,
    pub vignette: bool,
    pub film_grain: bool,
    pub chromatic_aberration: bool,
    pub sharpen: bool,
    pub sun_rays: bool,
    /// Motion blur's samples (needs the prepass's depth and motion vectors).
    pub motion_blur: MotionBlurQuality,
    /// The projected decals drawn (they need the prepass's depth and
    /// normals while any are drawn).
    pub decals: DecalLimits,
    /// Whether ray traced effects can be drawn: the GPU has hardware ray
    /// queries. Each effect's own setting says whether it is.
    pub ray_queries: bool,
    /// How far from the camera ray traced effects see, in studs.
    pub ray_tracing_distance: f32,
    /// Ray traced sun shadows (Off on a GPU without ray queries).
    pub ray_traced_shadows: RayTracedShadows,
    /// Ray traced ambient occlusion (Off on a GPU without ray queries).
    pub ray_traced_ambient_occlusion: RayTracedAmbientOcclusion,
    /// Ray traced global illumination (Off on a GPU that can neither ray
    /// trace nor run compute shaders).
    pub ray_traced_global_illumination: RayTracedGlobalIllumination,
    /// Whether ray traced GI's rays are traced in software (RL3): it's on
    /// and the GPU has no ray queries (or Hardware ray tracing is off).
    pub ray_traced_gi_in_software: bool,
    /// Ray traced reflections (Off on a GPU without ray queries).
    pub ray_traced_reflections: RayTracedReflections,
    /// Why each lowered value was lowered, for showing in the settings.
    pub notes: Vec<String>,
}

impl Default for EffectiveGraphics {
    /// The High preset on a machine that can do it.
    fn default() -> Self {
        GraphicsSettings::default().effective(&Capabilities::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(size: u32, cascades: u32) -> Option<ShadowMapSize> {
        Some(ShadowMapSize { size, cascades })
    }

    #[test]
    fn each_preset_gives_its_documented_values() {
        let values = |quality| {
            let settings = GraphicsSettings::preset(quality);
            (
                settings.shadow_quality().map(),
                settings.shadow_distance(),
                settings.post_processing(),
                settings.render_scale(),
                settings.texture_quality(),
                settings.anti_aliasing(),
            )
        };
        use AntiAliasing::{Fxaa, Msaa4, Off};
        use TextureQuality::{Full, Half, Quarter};
        assert_eq!(
            values(Quality::Low),
            (map(1024, 2), 75.0, false, 0.75, Quarter, Off)
        );
        assert_eq!(
            values(Quality::Medium),
            (map(2048, 2), 100.0, true, 1.0, Half, Fxaa)
        );
        assert_eq!(
            values(Quality::High),
            (map(2048, 3), 150.0, true, 1.0, Full, Msaa4)
        );
        assert_eq!(
            values(Quality::Ultra),
            (map(4096, 4), 300.0, true, 1.0, Full, Msaa4)
        );
        assert_eq!(
            AntiAliasing::ALL.map(AntiAliasing::samples),
            [1, 1, 2, 4, 1]
        );
        assert_eq!(
            [Quarter, Half, Full].map(TextureQuality::skipped_levels),
            [2, 1, 0]
        );
        assert_eq!(ShadowQuality::Off.map(), None);
        assert_eq!(GraphicsSettings::default().quality, Quality::High);
        let occlusion =
            Quality::ALL.map(|quality| GraphicsSettings::preset(quality).ambient_occlusion());
        use AmbientOcclusion as Ao;
        assert_eq!(occlusion, [Ao::Off, Ao::Low, Ao::High, Ao::High]);
        assert_eq!(Ao::ALL.map(Ao::samples), [0, 8, 16]);
        let budgets =
            Quality::ALL.map(|quality| GraphicsSettings::preset(quality).texture_budget());
        assert_eq!(budgets, [256, 512, 1024, 2048]);
        let reflections =
            Quality::ALL.map(|quality| GraphicsSettings::preset(quality).reflection_quality());
        assert_eq!(reflections, ReflectionQuality::ALL);
        let clouds = Quality::ALL.map(|quality| GraphicsSettings::preset(quality).cloud_quality());
        assert_eq!(clouds, CloudQuality::ALL);
        assert_eq!(
            CloudQuality::ALL.map(CloudQuality::steps),
            [(0, 0), (16, 0), (32, 3), (64, 6)]
        );
        let environment = |size, samples| Some(EnvironmentMapSize { size, samples });
        assert_eq!(
            ReflectionQuality::ALL.map(ReflectionQuality::map),
            [
                None,
                environment(64, 32),
                environment(128, 64),
                environment(256, 128)
            ]
        );
    }

    #[test]
    fn each_preset_gives_its_spot_shadow_maps() {
        let spot = |quality| {
            GraphicsSettings::preset(quality)
                .shadow_quality()
                .spot_map()
        };
        let map = |size, lights| Some(SpotShadowMapSize { size, lights });
        assert_eq!(spot(Quality::Low), None);
        assert_eq!(spot(Quality::Medium), map(512, 2));
        assert_eq!(spot(Quality::High), map(1024, 4));
        assert_eq!(spot(Quality::Ultra), map(2048, 8));
        assert_eq!(ShadowQuality::Off.spot_map(), None);
        // A GPU whose arrays can't hold Ultra's layers (8 spot lights, 24
        // point light faces) drops its spot and point maps too.
        let twelve_layers = Capabilities {
            max_texture_layers: 12,
            ..Capabilities::default()
        };
        let lowered = GraphicsSettings::preset(Quality::Ultra).effective(&twelve_layers);
        assert_eq!(lowered.shadow_quality, ShadowQuality::High);
        assert_eq!(lowered.notes.len(), 1, "{:?}", lowered.notes);
    }

    #[test]
    fn each_preset_gives_its_point_shadow_maps_and_contact_shadows() {
        let point = |quality| {
            GraphicsSettings::preset(quality)
                .shadow_quality()
                .point_map()
        };
        let map = |size, lights| Some(PointShadowMapSize { size, lights });
        assert_eq!(point(Quality::Low), None);
        assert_eq!(point(Quality::Medium), map(512, 1));
        assert_eq!(point(Quality::High), map(512, 2));
        assert_eq!(point(Quality::Ultra), map(1024, 4));
        assert_eq!(ShadowQuality::Off.point_map(), None);
        let contact =
            Quality::ALL.map(|quality| GraphicsSettings::preset(quality).contact_shadows());
        assert_eq!(contact, [false, false, true, true]);
        // Six faces per light: a GPU whose arrays hold 11 layers can't
        // give High's two point lights theirs.
        let eleven_layers = Capabilities {
            max_texture_layers: 11,
            ..Capabilities::default()
        };
        let lowered = GraphicsSettings::preset(Quality::High).effective(&eleven_layers);
        assert_eq!(lowered.shadow_quality, ShadowQuality::Medium);
        // Every GPU can draw contact shadows.
        let tiny = Capabilities {
            max_texture_size: 1,
            max_texture_layers: 1,
            ..Capabilities::default()
        };
        assert!(
            GraphicsSettings::preset(Quality::Ultra)
                .effective(&tiny)
                .contact_shadows
        );
    }

    #[test]
    fn an_override_replaces_only_its_own_value() {
        for quality in Quality::ALL {
            let preset = GraphicsSettings::preset(quality);
            let shadows = GraphicsSettings {
                shadow_quality: Some(ShadowQuality::Off),
                ..preset.clone()
            };
            assert_eq!(shadows.shadow_quality(), ShadowQuality::Off);
            assert_eq!(shadows.shadow_distance(), preset.shadow_distance());
            assert_eq!(shadows.post_processing(), preset.post_processing());

            let distance = GraphicsSettings {
                shadow_distance: Some(42.0),
                ..preset.clone()
            };
            assert_eq!(distance.shadow_distance(), 42.0);
            assert_eq!(distance.shadow_quality(), preset.shadow_quality());
            assert_eq!(distance.post_processing(), preset.post_processing());

            let post = GraphicsSettings {
                post_processing: Some(!preset.post_processing()),
                ..preset.clone()
            };
            assert_eq!(post.post_processing(), !preset.post_processing());
            assert_eq!(post.shadow_quality(), preset.shadow_quality());
            assert_eq!(post.shadow_distance(), preset.shadow_distance());
            assert_eq!(post.render_scale(), preset.render_scale());

            let scale = GraphicsSettings {
                render_scale: Some(1.5),
                ..preset.clone()
            };
            assert_eq!(scale.render_scale(), 1.5);
            assert_eq!(scale.texture_quality(), preset.texture_quality());
            assert_eq!(scale.shadow_quality(), preset.shadow_quality());

            let textures = GraphicsSettings {
                texture_quality: Some(TextureQuality::Half),
                ..preset.clone()
            };
            assert_eq!(textures.texture_quality(), TextureQuality::Half);
            assert_eq!(textures.render_scale(), preset.render_scale());
            assert_eq!(textures.post_processing(), preset.post_processing());
            assert_eq!(textures.anti_aliasing(), preset.anti_aliasing());

            let smoothing = GraphicsSettings {
                anti_aliasing: Some(AntiAliasing::Msaa2),
                ..preset.clone()
            };
            assert_eq!(smoothing.anti_aliasing(), AntiAliasing::Msaa2);
            assert_eq!(smoothing.texture_quality(), preset.texture_quality());
            assert_eq!(smoothing.shadow_quality(), preset.shadow_quality());
            assert_eq!(smoothing.post_processing(), preset.post_processing());

            let occlusion = GraphicsSettings {
                ambient_occlusion: Some(AmbientOcclusion::Low),
                ..preset.clone()
            };
            assert_eq!(occlusion.ambient_occlusion(), AmbientOcclusion::Low);
            assert_eq!(occlusion.anti_aliasing(), preset.anti_aliasing());
            assert_eq!(occlusion.shadow_quality(), preset.shadow_quality());
            assert_eq!(occlusion.render_scale(), preset.render_scale());

            let reflections = GraphicsSettings {
                reflection_quality: Some(ReflectionQuality::Low),
                ..preset.clone()
            };
            assert_eq!(reflections.reflection_quality(), ReflectionQuality::Low);
            assert_eq!(reflections.shadow_quality(), preset.shadow_quality());
            assert_eq!(reflections.ambient_occlusion(), preset.ambient_occlusion());
            assert_eq!(occlusion.reflection_quality(), preset.reflection_quality());

            let contact = GraphicsSettings {
                contact_shadows: Some(!preset.contact_shadows()),
                ..preset.clone()
            };
            assert_eq!(contact.contact_shadows(), !preset.contact_shadows());
            assert_eq!(contact.shadow_quality(), preset.shadow_quality());
            assert_eq!(contact.ambient_occlusion(), preset.ambient_occlusion());
            assert_eq!(occlusion.contact_shadows(), preset.contact_shadows());

            let budget = GraphicsSettings {
                texture_budget: Some(64),
                ..preset.clone()
            };
            assert_eq!(budget.texture_budget(), 64);
            assert_eq!(budget.texture_quality(), preset.texture_quality());
            assert_eq!(budget.anti_aliasing(), preset.anti_aliasing());
            assert_eq!(
                budget.effective(&Capabilities::default()).texture_budget,
                64 << 20
            );
        }
        let broken = GraphicsSettings {
            shadow_distance: Some(f32::NAN),
            render_scale: Some(f32::NAN),
            ..GraphicsSettings::default()
        };
        assert_eq!(broken.shadow_distance(), 150.0);
        assert_eq!(broken.render_scale(), 1.0);
        let scale = |scale| {
            GraphicsSettings {
                render_scale: Some(scale),
                ..GraphicsSettings::default()
            }
            .render_scale()
        };
        assert_eq!((scale(0.1), scale(9.0)), (0.5, 2.0), "kept within 50–200%");
        let budget = |budget| {
            GraphicsSettings {
                texture_budget: Some(budget),
                ..GraphicsSettings::default()
            }
            .texture_budget()
        };
        assert_eq!((budget(0), budget(u32::MAX)), (16, 65_536));
    }

    #[test]
    fn each_lens_effect_has_its_own_setting_by_preset_and_override() {
        use DepthOfFieldQuality as Dof;
        let presets = Quality::ALL.map(|quality| {
            let settings = GraphicsSettings::preset(quality);
            (
                settings.depth_of_field(),
                [
                    settings.vignette(),
                    settings.film_grain(),
                    settings.chromatic_aberration(),
                    settings.sharpen(),
                    settings.sun_rays(),
                ],
            )
        });
        assert_eq!(
            presets,
            [
                (Dof::Off, [false; 5]),
                (Dof::Low, [true; 5]),
                (Dof::High, [true; 5]),
                (Dof::High, [true; 5]),
            ]
        );
        assert_eq!(Dof::ALL.map(Dof::samples), [0, 16, 32]);
        for quality in Quality::ALL {
            let preset = GraphicsSettings::preset(quality);
            let switches = |settings: &GraphicsSettings| {
                [
                    settings.vignette(),
                    settings.film_grain(),
                    settings.chromatic_aberration(),
                    settings.sharpen(),
                    settings.sun_rays(),
                ]
            };
            let blur = GraphicsSettings {
                depth_of_field: Some(Dof::Low),
                ..preset.clone()
            };
            assert_eq!(blur.depth_of_field(), Dof::Low);
            assert_eq!(switches(&blur), switches(&preset));
            assert_eq!(blur.post_processing(), preset.post_processing());
            // Each switch turns only itself over.
            for which in 0..5 {
                let mut one = preset.clone();
                let flipped = Some(!switches(&preset)[which]);
                match which {
                    0 => one.vignette = flipped,
                    1 => one.film_grain = flipped,
                    2 => one.chromatic_aberration = flipped,
                    3 => one.sharpen = flipped,
                    _ => one.sun_rays = flipped,
                }
                let mut expected = switches(&preset);
                expected[which] = !expected[which];
                assert_eq!(switches(&one), expected);
                assert_eq!(one.depth_of_field(), preset.depth_of_field());
                let effective = one.effective(&Capabilities::default());
                assert_eq!(
                    [
                        effective.vignette,
                        effective.film_grain,
                        effective.chromatic_aberration,
                        effective.sharpen,
                        effective.sun_rays,
                    ],
                    expected
                );
            }
        }
        // Every GPU can draw them, so none is lowered.
        let tiny = Capabilities {
            max_texture_size: 64,
            max_texture_layers: 1,
            msaa_samples: 1,
            compute_shaders: false,
            ..Capabilities::default()
        };
        let ultra = GraphicsSettings::preset(Quality::Ultra).effective(&tiny);
        assert_eq!(ultra.depth_of_field, Dof::High);
        assert!(ultra.vignette && ultra.film_grain && ultra.chromatic_aberration && ultra.sharpen);
        assert!(ultra.sun_rays);
    }

    #[test]
    fn motion_blur_has_its_own_setting_and_taa_is_never_lowered() {
        use MotionBlurQuality as Blur;
        let presets = Quality::ALL.map(|quality| GraphicsSettings::preset(quality).motion_blur());
        assert_eq!(presets, [Blur::Off, Blur::Low, Blur::High, Blur::High]);
        assert_eq!(Blur::ALL.map(Blur::samples), [0, 8, 16]);
        for quality in Quality::ALL {
            let preset = GraphicsSettings::preset(quality);
            let blur = GraphicsSettings {
                motion_blur: Some(Blur::Low),
                ..preset.clone()
            };
            assert_eq!(blur.motion_blur(), Blur::Low);
            assert_eq!(blur.depth_of_field(), preset.depth_of_field());
            assert_eq!(blur.anti_aliasing(), preset.anti_aliasing());
            assert_eq!(
                blur.effective(&Capabilities::default()).motion_blur,
                Blur::Low
            );
        }
        // No preset picks TAA, and every GPU can draw it and motion blur.
        assert!(
            Quality::ALL
                .iter()
                .all(|&quality| GraphicsSettings::preset(quality).anti_aliasing()
                    != AntiAliasing::Taa)
        );
        let tiny = Capabilities {
            max_texture_size: 64,
            max_texture_layers: 1,
            msaa_samples: 1,
            compute_shaders: false,
            ..Capabilities::default()
        };
        let taa = GraphicsSettings {
            anti_aliasing: Some(AntiAliasing::Taa),
            ..GraphicsSettings::preset(Quality::Ultra)
        }
        .effective(&tiny);
        assert_eq!(taa.anti_aliasing, AntiAliasing::Taa);
        assert_eq!(taa.motion_blur, Blur::High);
        assert!(taa.notes.iter().all(|note| !note.contains("Anti-aliasing")));
        // A GPU without MSAA 4× still never gets TAA in its place.
        let msaa = GraphicsSettings::preset(Quality::High).effective(&tiny);
        assert_ne!(msaa.anti_aliasing, AntiAliasing::Taa);
        assert_eq!(AntiAliasing::Taa.samples(), 1);
    }

    #[test]
    fn settings_round_trip_through_json() {
        let settings = GraphicsSettings {
            quality: Quality::Medium,
            automatic: true,
            shadow_quality: Some(ShadowQuality::Ultra),
            shadow_distance: Some(80.0),
            post_processing: Some(false),
            render_scale: Some(0.5),
            texture_quality: Some(TextureQuality::Quarter),
            texture_budget: Some(300),
            anti_aliasing: Some(AntiAliasing::Fxaa),
            ambient_occlusion: Some(AmbientOcclusion::Off),
            reflection_quality: Some(ReflectionQuality::Medium),
            contact_shadows: Some(true),
            realtime_gi: Some(true),
            volumetric_fog: Some(false),
            cloud_quality: Some(CloudQuality::Low),
            depth_of_field: Some(DepthOfFieldQuality::Low),
            vignette: Some(false),
            film_grain: Some(true),
            chromatic_aberration: Some(false),
            sharpen: Some(true),
            sun_rays: Some(false),
            motion_blur: Some(MotionBlurQuality::Low),
            ray_tracing_distance: Some(600.0),
            ray_traced_shadows: Some(RayTracedShadows::Medium),
            ray_traced_ambient_occlusion: Some(RayTracedAmbientOcclusion::Low),
            ray_traced_global_illumination: Some(RayTracedGlobalIllumination::High),
            ray_traced_reflections: Some(RayTracedReflections::High),
            ray_tracing: Some(true),
        };
        let text = serde_json::to_string(&settings).unwrap();
        assert_eq!(
            serde_json::from_str::<GraphicsSettings>(&text).unwrap(),
            settings
        );
        // A preset alone saves no overrides, and missing keys read as defaults.
        let preset = serde_json::to_string(&GraphicsSettings::preset(Quality::Low)).unwrap();
        assert_eq!(preset, r#"{"quality":"Low"}"#);
        assert_eq!(
            serde_json::from_str::<GraphicsSettings>("{}").unwrap(),
            GraphicsSettings::default()
        );
    }

    #[test]
    fn a_software_adapter_starts_on_low() {
        let software = Capabilities {
            software: true,
            ..Capabilities::default()
        };
        assert_eq!(
            GraphicsSettings::default_for(&software).quality,
            Quality::Low
        );
        let card = Capabilities::default();
        assert_eq!(GraphicsSettings::default_for(&card).quality, Quality::High);
        // The machine's default is Automatic, saved as such.
        let automatic = GraphicsSettings::default_for(&card);
        assert!(automatic.automatic);
        assert_eq!(
            serde_json::to_string(&automatic).unwrap(),
            r#"{"quality":"High","automatic":true}"#
        );
    }

    #[test]
    fn effective_lowers_what_the_gpu_cant_do_and_says_why() {
        let small = Capabilities {
            max_texture_size: 2048,
            ..Capabilities::default()
        };
        let ultra = GraphicsSettings::preset(Quality::Ultra).effective(&small);
        assert_eq!(ultra.shadow_quality, ShadowQuality::High);
        assert_eq!(ultra.notes.len(), 1, "{:?}", ultra.notes);
        assert!(
            ultra.notes[0].contains("Ultra to High"),
            "{:?}",
            ultra.notes
        );
        // Values the GPU can meet aren't touched or noted.
        let high = GraphicsSettings::preset(Quality::High).effective(&small);
        assert_eq!(high.shadow_quality, ShadowQuality::High);
        assert!(high.notes.is_empty());

        let tiny = Capabilities {
            max_texture_size: 512,
            ..Capabilities::default()
        };
        let off = GraphicsSettings::preset(Quality::Low).effective(&tiny);
        assert_eq!(off.shadow_quality, ShadowQuality::Off);
        assert_eq!(off.notes.len(), 1);

        // High's maps need 12 layers (two point lights' six faces each),
        // Medium's 6.
        let six_layers = Capabilities {
            max_texture_layers: 6,
            ..Capabilities::default()
        };
        let medium = GraphicsSettings::preset(Quality::High).effective(&six_layers);
        assert_eq!(medium.shadow_quality, ShadowQuality::Medium);
    }

    #[test]
    fn reflection_quality_drops_to_a_cubemap_the_gpu_can_hold_with_a_note() {
        let small = Capabilities {
            max_texture_size: 128,
            ..Capabilities::default()
        };
        let ultra = GraphicsSettings {
            shadow_quality: Some(ShadowQuality::Off),
            ..GraphicsSettings::preset(Quality::Ultra)
        }
        .effective(&small);
        assert_eq!(ultra.reflection_quality, ReflectionQuality::High);
        assert_eq!(ultra.notes.len(), 1, "{:?}", ultra.notes);
        assert!(
            ultra.notes[0].contains("Reflection quality lowered from Ultra to High"),
            "{:?}",
            ultra.notes
        );
        let tiny = Capabilities {
            max_texture_size: 32,
            ..small
        };
        let low = GraphicsSettings::preset(Quality::Medium).effective(&tiny);
        assert_eq!(low.reflection_quality, ReflectionQuality::Low);
        // The gradient needs no texture, so Low is never lowered or noted.
        let gradient = GraphicsSettings::preset(Quality::Low).effective(&tiny);
        assert_eq!(gradient.reflection_quality, ReflectionQuality::Low);
        assert!(
            !gradient
                .notes
                .iter()
                .any(|note| note.contains("Reflection"))
        );
    }

    #[test]
    fn a_gpu_without_msaa_4x_gets_2x_or_fxaa_with_a_note() {
        // wgpu guarantees 4× for the scene's formats; 2× needs the adapter.
        assert_eq!(Capabilities::default().msaa_samples, 1 | 4);
        let high = GraphicsSettings::preset(Quality::High);
        assert_eq!(
            high.effective(&Capabilities::default()).anti_aliasing,
            AntiAliasing::Msaa4
        );

        let two_only = Capabilities {
            msaa_samples: 1 | 2,
            ..Capabilities::default()
        };
        let lowered = high.effective(&two_only);
        assert_eq!(lowered.anti_aliasing, AntiAliasing::Msaa2);
        assert_eq!(lowered.notes.len(), 1, "{:?}", lowered.notes);
        assert!(
            lowered.notes[0].contains("MSAA 4× to MSAA 2×"),
            "{:?}",
            lowered.notes
        );

        let none = Capabilities {
            msaa_samples: 1,
            ..Capabilities::default()
        };
        let fxaa = high.effective(&none);
        assert_eq!(fxaa.anti_aliasing, AntiAliasing::Fxaa);
        assert!(
            fxaa.notes[0].contains("MSAA 4× to FXAA"),
            "{:?}",
            fxaa.notes
        );
        // FXAA and Off need no samples, so they're never lowered.
        for mode in [AntiAliasing::Off, AntiAliasing::Fxaa] {
            let settings = GraphicsSettings {
                anti_aliasing: Some(mode),
                ..high.clone()
            };
            let effective = settings.effective(&none);
            assert_eq!(effective.anti_aliasing, mode);
            assert!(effective.notes.is_empty());
        }
        // A broken reading (no bits at all) still can't crash or raise.
        let broken = Capabilities {
            msaa_samples: 0,
            ..Capabilities::default()
        };
        assert_eq!(high.effective(&broken).anti_aliasing, AntiAliasing::Fxaa);
    }

    #[test]
    fn realtime_gi_is_off_at_every_preset_and_off_with_a_note_without_compute_shaders() {
        for quality in Quality::ALL {
            let preset = GraphicsSettings::preset(quality);
            assert!(!preset.realtime_gi());
            assert!(!preset.effective(&Capabilities::default()).realtime_gi);
        }
        let on = GraphicsSettings {
            realtime_gi: Some(true),
            ..GraphicsSettings::preset(Quality::High)
        };
        let used = on.effective(&Capabilities::default());
        assert!(used.realtime_gi);
        assert!(used.notes.is_empty(), "{:?}", used.notes);

        let no_compute = Capabilities {
            compute_shaders: false,
            ..Capabilities::default()
        };
        let lowered = on.effective(&no_compute);
        assert!(!lowered.realtime_gi);
        assert_eq!(lowered.notes.len(), 1, "{:?}", lowered.notes);
        assert!(
            lowered.notes[0].contains("Real-time GI is off"),
            "{:?}",
            lowered.notes
        );
        // Asking for none needs nothing, so nothing is noted.
        let off = GraphicsSettings::preset(Quality::High).effective(&no_compute);
        assert!(!off.realtime_gi && off.notes.is_empty());
    }

    /// R18.4: real-time GI's tier follows the preset, and High's is what it
    /// traced before tiers (two probes a frame, faces a bake's size).
    #[test]
    fn realtime_gi_tiers_follow_the_preset() {
        let tier = |quality| {
            let settings = GraphicsSettings {
                realtime_gi: Some(true),
                ..GraphicsSettings::preset(quality)
            };
            let tier = settings
                .effective(&Capabilities::default())
                .realtime_gi_tier;
            (tier.probes_per_frame, tier.face_size)
        };
        assert_eq!(tier(Quality::Low), (1, 8));
        assert_eq!(tier(Quality::Medium), (2, 8));
        assert_eq!(tier(Quality::High), (2, 16));
        assert_eq!(tier(Quality::Ultra), (4, 16));
        let mut last = 0;
        for quality in Quality::ALL {
            let tier = RealtimeGiTier::of(quality);
            let work = tier.probes_per_frame * tier.face_size * tier.face_size;
            assert!(work > last, "each tier traces more than the one below");
            assert!(tier.probes_per_frame <= RealtimeGiTier::MAX_PROBES_PER_FRAME);
            last = work;
        }
    }

    #[test]
    fn volumetric_fog_is_on_only_at_ultra_and_off_with_a_note_without_compute_shaders() {
        for quality in Quality::ALL {
            let preset = GraphicsSettings::preset(quality);
            let ultra = quality == Quality::Ultra;
            assert_eq!(preset.volumetric_fog(), ultra, "{quality:?}");
            assert_eq!(
                preset.effective(&Capabilities::default()).volumetric_fog,
                ultra
            );
        }
        let on = GraphicsSettings {
            volumetric_fog: Some(true),
            ..GraphicsSettings::preset(Quality::Low)
        };
        assert!(on.effective(&Capabilities::default()).volumetric_fog);
        let no_compute = Capabilities {
            compute_shaders: false,
            ..Capabilities::default()
        };
        let lowered = GraphicsSettings::preset(Quality::Ultra).effective(&no_compute);
        assert!(!lowered.volumetric_fog);
        assert!(
            lowered
                .notes
                .iter()
                .any(|note| note.contains("Volumetric fog is off")),
            "{:?}",
            lowered.notes
        );
    }

    #[test]
    fn hardware_ray_tracing_is_off_unless_turned_on() {
        for quality in Quality::ALL {
            assert!(!GraphicsSettings::preset(quality).ray_tracing());
        }
        let on = GraphicsSettings {
            ray_tracing: Some(true),
            ..GraphicsSettings::default()
        };
        assert!(on.ray_tracing());
        assert!(!GraphicsSettings::default().ray_tracing());
    }

    #[test]
    fn ray_tracing_distance_follows_the_preset_and_stays_in_range() {
        let distances = Quality::ALL.map(|q| GraphicsSettings::preset(q).ray_tracing_distance());
        assert_eq!(distances, [150.0, 250.0, 400.0, 800.0]);
        let with = |distance| GraphicsSettings {
            ray_tracing_distance: Some(distance),
            ..GraphicsSettings::default()
        };
        assert_eq!(with(1200.0).ray_tracing_distance(), 1200.0);
        assert_eq!(with(1.0).ray_tracing_distance(), 50.0);
        assert_eq!(with(1e9).ray_tracing_distance(), 5000.0);
        assert_eq!(with(f32::NAN).ray_tracing_distance(), 400.0);
        // The capability passes through; the GPU decides it, not a setting.
        let gpu = Capabilities {
            ray_queries: true,
            ..Capabilities::default()
        };
        assert!(GraphicsSettings::default().effective(&gpu).ray_queries);
        assert!(!EffectiveGraphics::default().ray_queries);
    }

    #[test]
    fn ray_traced_shadows_are_off_at_every_preset_and_without_ray_queries() {
        for quality in Quality::ALL {
            let preset = GraphicsSettings::preset(quality);
            assert_eq!(preset.ray_traced_shadows(), RayTracedShadows::Off);
        }
        let asked = GraphicsSettings {
            ray_traced_shadows: Some(RayTracedShadows::Medium),
            ..GraphicsSettings::default()
        };
        let gpu = Capabilities {
            ray_queries: true,
            ..Capabilities::default()
        };
        let effective = asked.effective(&gpu);
        assert_eq!(effective.ray_traced_shadows, RayTracedShadows::Medium);
        assert!(effective.notes.is_empty(), "{:?}", effective.notes);
        let without = asked.effective(&Capabilities::default());
        assert_eq!(without.ray_traced_shadows, RayTracedShadows::Off);
        assert!(
            without
                .notes
                .iter()
                .any(|note| note.contains("can't ray trace"))
        );
        assert_eq!(
            RayTracedShadows::ALL.map(RayTracedShadows::rays),
            [0, 1, 2, 4]
        );
    }

    #[test]
    fn ray_traced_ambient_occlusion_is_off_at_every_preset_and_without_ray_queries() {
        for quality in Quality::ALL {
            let preset = GraphicsSettings::preset(quality);
            assert_eq!(
                preset.ray_traced_ambient_occlusion(),
                RayTracedAmbientOcclusion::Off
            );
        }
        let asked = GraphicsSettings {
            ray_traced_ambient_occlusion: Some(RayTracedAmbientOcclusion::High),
            ..GraphicsSettings::default()
        };
        let gpu = Capabilities {
            ray_queries: true,
            ..Capabilities::default()
        };
        let effective = asked.effective(&gpu);
        assert_eq!(
            effective.ray_traced_ambient_occlusion,
            RayTracedAmbientOcclusion::High
        );
        assert!(effective.notes.is_empty(), "{:?}", effective.notes);
        let without = asked.effective(&Capabilities::default());
        assert_eq!(
            without.ray_traced_ambient_occlusion,
            RayTracedAmbientOcclusion::Off
        );
        assert!(
            without
                .notes
                .iter()
                .any(|note| note.contains("ambient occlusion is off"))
        );
        assert_eq!(
            RayTracedAmbientOcclusion::ALL.map(RayTracedAmbientOcclusion::rays),
            [0, 2, 4, 8]
        );
    }

    #[test]
    fn ray_traced_reflections_are_off_at_every_preset_and_without_ray_queries() {
        for quality in Quality::ALL {
            let preset = GraphicsSettings::preset(quality);
            assert_eq!(preset.ray_traced_reflections(), RayTracedReflections::Off);
        }
        let asked = GraphicsSettings {
            ray_traced_reflections: Some(RayTracedReflections::Medium),
            ..GraphicsSettings::default()
        };
        let gpu = Capabilities {
            ray_queries: true,
            ..Capabilities::default()
        };
        let effective = asked.effective(&gpu);
        assert_eq!(
            effective.ray_traced_reflections,
            RayTracedReflections::Medium
        );
        assert!(effective.notes.is_empty(), "{:?}", effective.notes);
        let without = asked.effective(&Capabilities::default());
        assert_eq!(without.ray_traced_reflections, RayTracedReflections::Off);
        assert!(
            without
                .notes
                .iter()
                .any(|note| note.contains("reflections are off"))
        );
        assert_eq!(
            RayTracedReflections::ALL.map(RayTracedReflections::roughness_cutoff),
            [0.0, 0.15, 0.3, 0.5]
        );
        assert_eq!(
            RayTracedReflections::ALL.map(RayTracedReflections::downscale),
            [1, 2, 1, 1]
        );
        // Turned off with the other ray traced effects, and kept in
        // graphics.json only when set.
        assert_eq!(
            asked.without_ray_traced_effects().ray_traced_reflections,
            None
        );
        let text = serde_json::to_string(&GraphicsSettings::default()).unwrap();
        assert!(!text.contains("ray_traced_reflections"), "{text}");
        let read: GraphicsSettings =
            serde_json::from_str(&serde_json::to_string(&asked).unwrap()).unwrap();
        assert_eq!(
            read.ray_traced_reflections,
            Some(RayTracedReflections::Medium)
        );
    }

    #[test]
    fn ray_traced_global_illumination_is_off_at_every_preset_and_in_software_without_ray_queries() {
        for quality in Quality::ALL {
            let preset = GraphicsSettings::preset(quality);
            assert_eq!(
                preset.ray_traced_global_illumination(),
                RayTracedGlobalIllumination::Off
            );
        }
        let asked = GraphicsSettings {
            ray_traced_global_illumination: Some(RayTracedGlobalIllumination::Medium),
            ..GraphicsSettings::default()
        };
        let gpu = Capabilities {
            ray_queries: true,
            ..Capabilities::default()
        };
        let effective = asked.effective(&gpu);
        assert_eq!(
            effective.ray_traced_global_illumination,
            RayTracedGlobalIllumination::Medium
        );
        assert!(effective.notes.is_empty(), "{:?}", effective.notes);
        assert!(!effective.ray_traced_gi_in_software);
        // Without ray queries its rays are traced in software (RL3).
        let software = asked.effective(&Capabilities::default());
        assert_eq!(
            software.ray_traced_global_illumination,
            RayTracedGlobalIllumination::Medium
        );
        assert!(software.ray_traced_gi_in_software);
        assert!(
            software
                .notes
                .iter()
                .any(|note| note.contains("in software"))
        );
        // Without compute shaders either, it's off.
        let without = asked.effective(&Capabilities {
            compute_shaders: false,
            ..Capabilities::default()
        });
        assert_eq!(
            without.ray_traced_global_illumination,
            RayTracedGlobalIllumination::Off
        );
        assert!(!without.ray_traced_gi_in_software);
        assert!(without.notes.iter().any(|note| note.contains("GI is off")));
        assert!(
            !GraphicsSettings::default()
                .effective(&Capabilities::default())
                .ray_traced_gi_in_software
        );
        assert_eq!(
            RayTracedGlobalIllumination::ALL.map(RayTracedGlobalIllumination::rays),
            [0, 64, 128, 256]
        );
        assert_eq!(
            RayTracedGlobalIllumination::ALL.map(RayTracedGlobalIllumination::probes_per_frame),
            [0, 128, 256, 512]
        );
    }

    #[test]
    fn the_ray_traced_look_turns_on_what_it_needs_and_keeps_the_rest() {
        let mine = GraphicsSettings {
            shadow_quality: Some(ShadowQuality::Ultra),
            reflection_quality: Some(ReflectionQuality::Low),
            render_scale: Some(0.75),
            ..GraphicsSettings::default()
        };
        let look = mine.ray_traced();
        assert!(look.ray_tracing());
        assert_eq!(look.ray_traced_shadows(), RayTracedShadows::Medium);
        assert_eq!(
            look.ray_traced_ambient_occlusion(),
            RayTracedAmbientOcclusion::Medium
        );
        assert_eq!(look.shadow_quality(), ShadowQuality::Ultra, "never lowered");
        assert_eq!(look.reflection_quality(), ReflectionQuality::High, "raised");
        assert_eq!(look.anti_aliasing(), AntiAliasing::Taa);
        assert_eq!(look.render_scale(), 0.75, "the rest is kept");
        // On a GPU that ray traces it's all drawn; on one that can't, the
        // rays are off with notes.
        let rays = Capabilities {
            ray_queries: true,
            ..Capabilities::default()
        };
        assert_eq!(
            look.effective(&rays).ray_traced_shadows,
            RayTracedShadows::Medium
        );
        let none = look.effective(&Capabilities::default());
        assert_eq!(none.ray_traced_shadows, RayTracedShadows::Off);
        assert!(!none.notes.is_empty());

        let off = look.without_ray_traced_effects();
        assert_eq!(off.ray_traced_shadows(), RayTracedShadows::Off);
        assert_eq!(
            off.ray_traced_ambient_occlusion(),
            RayTracedAmbientOcclusion::Off
        );
        let with_gi = GraphicsSettings {
            ray_traced_global_illumination: Some(RayTracedGlobalIllumination::High),
            ..look.clone()
        };
        assert_eq!(
            with_gi
                .without_ray_traced_effects()
                .ray_traced_global_illumination(),
            RayTracedGlobalIllumination::Off
        );
        assert!(off.ray_tracing(), "turning the device off takes a restart");
        assert_eq!(off.shadow_quality(), ShadowQuality::Ultra);
    }

    #[test]
    fn effective_never_raises_a_setting() {
        let capabilities = [
            Capabilities::default(),
            Capabilities {
                max_texture_size: 16384,
                max_texture_layers: 2048,
                timestamp_queries: true,
                software: false,
                msaa_samples: 1 | 2 | 4 | 8,
                compute_shaders: true,
                ray_queries: true,
                ray_tracing_offered: true,
            },
            Capabilities {
                max_texture_size: 1024,
                msaa_samples: 1 | 2,
                ..Capabilities::default()
            },
            Capabilities {
                max_texture_size: 1,
                max_texture_layers: 1,
                timestamp_queries: false,
                software: true,
                msaa_samples: 1,
                compute_shaders: false,
                ray_queries: false,
                ray_tracing_offered: false,
            },
        ];
        let modes = AntiAliasing::ALL.map(Some).into_iter().chain([None]);
        let combinations = Quality::ALL
            .into_iter()
            .flat_map(|quality| modes.clone().map(move |mode| (quality, mode)));
        for (quality, anti_aliasing) in combinations {
            for shadows in ShadowQuality::ALL.map(Some).into_iter().chain([None]) {
                for post in [None, Some(false), Some(true)] {
                    let settings = GraphicsSettings {
                        quality,
                        automatic: false,
                        shadow_quality: shadows,
                        shadow_distance: Some(60.0),
                        post_processing: post,
                        render_scale: Some(0.5),
                        texture_quality: Some(TextureQuality::Half),
                        texture_budget: Some(200),
                        anti_aliasing,
                        ambient_occlusion: Some(AmbientOcclusion::Low),
                        reflection_quality: None,
                        contact_shadows: post,
                        realtime_gi: post,
                        volumetric_fog: post,
                        cloud_quality: Some(CloudQuality::Medium),
                        depth_of_field: Some(DepthOfFieldQuality::Low),
                        vignette: post,
                        film_grain: post,
                        chromatic_aberration: post,
                        sharpen: post,
                        sun_rays: post,
                        motion_blur: Some(MotionBlurQuality::Low),
                        ray_tracing_distance: None,
                        ray_traced_shadows: Some(RayTracedShadows::High),
                        ray_traced_ambient_occlusion: Some(RayTracedAmbientOcclusion::Medium),
                        ray_traced_global_illumination: Some(RayTracedGlobalIllumination::Low),
                        ray_traced_reflections: Some(RayTracedReflections::Medium),
                        ray_tracing: post,
                    };
                    for capabilities in &capabilities {
                        let effective = settings.effective(capabilities);
                        assert!(effective.shadow_quality <= settings.shadow_quality());
                        assert_eq!(effective.shadow_distance, settings.shadow_distance());
                        assert!(effective.post_processing <= settings.post_processing());
                        assert_eq!(effective.render_scale, settings.render_scale());
                        assert_eq!(effective.texture_quality, settings.texture_quality());
                        assert_eq!(effective.texture_budget, 200 << 20);
                        assert!(effective.anti_aliasing <= settings.anti_aliasing());
                        assert!(effective.ray_traced_shadows <= settings.ray_traced_shadows());
                        assert!(
                            effective.ray_traced_ambient_occlusion
                                <= settings.ray_traced_ambient_occlusion()
                        );
                        assert!(
                            effective.ray_traced_global_illumination
                                <= settings.ray_traced_global_illumination()
                        );
                        assert!(
                            effective.ray_traced_reflections <= settings.ray_traced_reflections()
                        );
                        assert_eq!(effective.ambient_occlusion, settings.ambient_occlusion());
                        assert!(effective.reflection_quality <= settings.reflection_quality());
                        assert_eq!(effective.contact_shadows, settings.contact_shadows());
                        assert!(effective.realtime_gi <= settings.realtime_gi());
                        assert!(effective.volumetric_fog <= settings.volumetric_fog());
                        assert_eq!(effective.cloud_quality, settings.cloud_quality());
                        assert_eq!(effective.depth_of_field, settings.depth_of_field());
                        assert_eq!(effective.vignette, settings.vignette());
                        assert_eq!(effective.film_grain, settings.film_grain());
                        assert_eq!(
                            effective.chromatic_aberration,
                            settings.chromatic_aberration()
                        );
                        assert_eq!(effective.sharpen, settings.sharpen());
                        assert_eq!(effective.sun_rays, settings.sun_rays());
                        let lowered =
                            usize::from(effective.shadow_quality != settings.shadow_quality())
                                + usize::from(effective.anti_aliasing != settings.anti_aliasing())
                                + usize::from(
                                    effective.reflection_quality != settings.reflection_quality(),
                                )
                                + usize::from(effective.realtime_gi != settings.realtime_gi())
                                + usize::from(
                                    effective.volumetric_fog != settings.volumetric_fog(),
                                )
                                + usize::from(
                                    effective.ray_traced_shadows != settings.ray_traced_shadows(),
                                )
                                + usize::from(
                                    effective.ray_traced_ambient_occlusion
                                        != settings.ray_traced_ambient_occlusion(),
                                )
                                + usize::from(
                                    effective.ray_traced_global_illumination
                                        != settings.ray_traced_global_illumination(),
                                )
                                + usize::from(effective.ray_traced_gi_in_software)
                                + usize::from(
                                    effective.ray_traced_reflections
                                        != settings.ray_traced_reflections(),
                                );
                        assert_eq!(effective.notes.len(), lowered);
                    }
                }
            }
        }
    }
}
