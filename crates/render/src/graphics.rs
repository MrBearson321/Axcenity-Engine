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
}

/// A shadow map's size in texels per side, and its number of cascades
/// (one layer each).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ShadowMapSize {
    pub size: u32,
    pub cascades: u32,
}

/// The machine's graphics choices: a preset, and overrides that each
/// replace one of its values. Saved per machine (the editor's settings, the
/// player's `graphics.json`), never in a project.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GraphicsSettings {
    pub quality: Quality,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shadow_quality: Option<ShadowQuality>,
    /// The farthest shadows reach, in studs: a ceiling on the world's
    /// `ShadowDistance`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shadow_distance: Option<f32>,
    /// Off turns bloom and tone mapping off whatever the world asks for.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub post_processing: Option<bool>,
}

impl GraphicsSettings {
    /// `quality` with no overrides.
    pub fn preset(quality: Quality) -> Self {
        Self {
            quality,
            ..Self::default()
        }
    }

    /// The settings a machine starts with: High, or Low on a software
    /// adapter (one that draws on the CPU).
    pub fn default_for(capabilities: &Capabilities) -> Self {
        Self::preset(if capabilities.software {
            Quality::Low
        } else {
            Quality::High
        })
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
            .find(|quality| {
                quality.map().is_none_or(|map| {
                    map.size <= capabilities.max_texture_size
                        && map.cascades <= capabilities.max_texture_layers
                })
            })
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
        EffectiveGraphics {
            shadow_quality,
            shadow_distance: self.shadow_distance(),
            post_processing: self.post_processing(),
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
}

impl Capabilities {
    /// Reads `device`'s limits and features. `adapter` (when known) says
    /// whether it draws in software.
    pub fn read(device: &wgpu::Device, adapter: Option<&wgpu::AdapterInfo>) -> Self {
        let limits = device.limits();
        Self {
            max_texture_size: limits.max_texture_dimension_2d,
            max_texture_layers: limits.max_texture_array_layers,
            timestamp_queries: device.features().contains(wgpu::Features::TIMESTAMP_QUERY),
            software: adapter.is_some_and(|info| info.device_type == wgpu::DeviceType::Cpu),
        }
    }
}

impl Default for Capabilities {
    /// wgpu's default limits on a graphics card.
    fn default() -> Self {
        let limits = wgpu::Limits::default();
        Self {
            max_texture_size: limits.max_texture_dimension_2d,
            max_texture_layers: limits.max_texture_array_layers,
            timestamp_queries: false,
            software: false,
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
            )
        };
        assert_eq!(values(Quality::Low), (map(1024, 2), 75.0, false));
        assert_eq!(values(Quality::Medium), (map(2048, 2), 100.0, true));
        assert_eq!(values(Quality::High), (map(2048, 3), 150.0, true));
        assert_eq!(values(Quality::Ultra), (map(4096, 4), 300.0, true));
        assert_eq!(ShadowQuality::Off.map(), None);
        assert_eq!(GraphicsSettings::default().quality, Quality::High);
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
        }
        let broken = GraphicsSettings {
            shadow_distance: Some(f32::NAN),
            ..GraphicsSettings::default()
        };
        assert_eq!(broken.shadow_distance(), 150.0);
    }

    #[test]
    fn settings_round_trip_through_json() {
        let settings = GraphicsSettings {
            quality: Quality::Medium,
            shadow_quality: Some(ShadowQuality::Ultra),
            shadow_distance: Some(80.0),
            post_processing: Some(false),
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

        let two_layers = Capabilities {
            max_texture_layers: 2,
            ..Capabilities::default()
        };
        let medium = GraphicsSettings::preset(Quality::High).effective(&two_layers);
        assert_eq!(medium.shadow_quality, ShadowQuality::Medium);
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
            },
            Capabilities {
                max_texture_size: 1024,
                ..Capabilities::default()
            },
            Capabilities {
                max_texture_size: 1,
                max_texture_layers: 1,
                timestamp_queries: false,
                software: true,
            },
        ];
        for quality in Quality::ALL {
            for shadows in ShadowQuality::ALL.map(Some).into_iter().chain([None]) {
                for post in [None, Some(false), Some(true)] {
                    let settings = GraphicsSettings {
                        quality,
                        shadow_quality: shadows,
                        shadow_distance: Some(60.0),
                        post_processing: post,
                    };
                    for capabilities in &capabilities {
                        let effective = settings.effective(capabilities);
                        assert!(effective.shadow_quality <= settings.shadow_quality());
                        assert_eq!(effective.shadow_distance, settings.shadow_distance());
                        assert!(effective.post_processing <= settings.post_processing());
                        assert_eq!(
                            effective.notes.is_empty(),
                            effective.shadow_quality == settings.shadow_quality()
                        );
                    }
                }
            }
        }
    }
}
