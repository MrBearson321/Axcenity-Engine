//! Built-in terrain layers (grass, rock, snow…) and the set each landscape
//! preset starts with, so a beginner's first terrain already looks like
//! ground: grass on the flats, rock on the slopes, snow on the peaks.
//!
//! A layer preset is just values for a TerrainLayer's properties; textures
//! can be added afterwards (Import texture set…).

use axcenity_core::{Color3, Value};
use axcenity_terrain::generate::Preset;

/// Where a preset layer lays itself down. Heights are fractions of the
/// terrain's height range, so they suit any size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Rule {
    /// Painted only (or underneath everything, as the first layer).
    None,
    /// On slopes steeper than this many degrees.
    SlopeAbove(f64),
    /// On flat-ish ground (below this slope) under this height.
    GentleBelow { slope: f64, height: f64 },
    /// Above this height, on slopes gentler than `max_slope`.
    HeightAbove { height: f64, max_slope: f64 },
    /// Below this height.
    HeightBelow(f64),
    /// Patches everywhere, broken up by noise.
    Patches(f64),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayerPreset {
    pub name: &'static str,
    pub color: [f32; 3],
    pub roughness: f64,
    /// Studs per copy of the textures.
    pub tile: f64,
    pub triplanar: bool,
    pub rule: Rule,
}

pub const GRASS: LayerPreset = LayerPreset {
    name: "Grass",
    color: [0.34, 0.52, 0.22],
    roughness: 0.95,
    tile: 6.0,
    triplanar: false,
    rule: Rule::None,
};
pub const DIRT: LayerPreset = LayerPreset {
    name: "Dirt",
    color: [0.45, 0.34, 0.24],
    roughness: 0.95,
    tile: 6.0,
    triplanar: false,
    rule: Rule::SlopeAbove(24.0),
};
pub const ROCK: LayerPreset = LayerPreset {
    name: "Rock",
    color: [0.47, 0.46, 0.44],
    roughness: 0.85,
    tile: 10.0,
    triplanar: true,
    rule: Rule::SlopeAbove(34.0),
};
pub const SAND: LayerPreset = LayerPreset {
    name: "Sand",
    color: [0.82, 0.72, 0.5],
    roughness: 0.9,
    tile: 5.0,
    triplanar: false,
    rule: Rule::HeightBelow(0.18),
};
pub const SNOW: LayerPreset = LayerPreset {
    name: "Snow",
    color: [0.93, 0.95, 0.98],
    roughness: 0.6,
    tile: 8.0,
    triplanar: false,
    rule: Rule::HeightAbove {
        height: 0.6,
        max_slope: 42.0,
    },
};
pub const MUD: LayerPreset = LayerPreset {
    name: "Mud",
    color: [0.3, 0.24, 0.18],
    roughness: 0.55,
    tile: 5.0,
    triplanar: false,
    rule: Rule::GentleBelow {
        slope: 10.0,
        height: 0.2,
    },
};
pub const GRAVEL: LayerPreset = LayerPreset {
    name: "Gravel",
    color: [0.55, 0.53, 0.5],
    roughness: 0.9,
    tile: 4.0,
    triplanar: false,
    rule: Rule::Patches(0.4),
};

/// Every built-in layer, for the Add layer menu.
pub const ALL: [LayerPreset; 7] = [GRASS, DIRT, ROCK, SAND, SNOW, MUD, GRAVEL];

/// The layers a landscape preset starts with, bottom first.
pub fn layers_for(preset: Preset) -> Vec<LayerPreset> {
    match preset {
        Preset::RollingHills | Preset::Custom | Preset::Flatlands => {
            vec![GRASS, DIRT, ROCK]
        }
        Preset::MountainRange | Preset::AlpineValley => vec![GRASS, ROCK, SNOW],
        Preset::SnowyMountains => vec![
            GRASS,
            ROCK,
            LayerPreset {
                rule: Rule::HeightAbove {
                    height: 0.42,
                    max_slope: 45.0,
                },
                ..SNOW
            },
        ],
        Preset::DesertDunes => vec![
            LayerPreset {
                rule: Rule::None,
                ..SAND
            },
            LayerPreset {
                name: "Dune Crest",
                color: [0.76, 0.62, 0.42],
                rule: Rule::SlopeAbove(22.0),
                ..SAND
            },
            ROCK,
        ],
        Preset::RockyHighlands => vec![
            LayerPreset {
                rule: Rule::None,
                ..GRAVEL
            },
            LayerPreset {
                rule: Rule::GentleBelow {
                    slope: 18.0,
                    height: 0.6,
                },
                ..GRASS
            },
            ROCK,
        ],
        Preset::Island => vec![
            LayerPreset {
                rule: Rule::None,
                ..SAND
            },
            LayerPreset {
                rule: Rule::HeightAbove {
                    height: 0.17,
                    max_slope: 40.0,
                },
                ..GRASS
            },
            ROCK,
        ],
        Preset::Crater => vec![
            LayerPreset {
                rule: Rule::None,
                ..DIRT
            },
            GRAVEL,
            ROCK,
        ],
    }
}

impl LayerPreset {
    /// The TerrainLayer properties for this layer on a terrain `height`
    /// studs tall.
    pub fn properties(&self, height: f32) -> Vec<(&'static str, Value)> {
        let height = f64::from(height);
        let [r, g, b] = self.color;
        let mut values = vec![
            ("Color", Value::Color3(Color3::new(r, g, b))),
            ("Roughness", Value::Number(self.roughness)),
            ("TileSize", Value::Number(self.tile)),
            ("Triplanar", Value::Bool(self.triplanar)),
        ];
        let mut rule = |pairs: &[(&'static str, f64)]| {
            values.push(("AutoEnabled", Value::Bool(true)));
            for &(name, value) in pairs {
                values.push((name, Value::Number(value)));
            }
        };
        match self.rule {
            Rule::None => {}
            Rule::SlopeAbove(slope) => rule(&[("MinSlope", slope), ("SlopeFade", 6.0)]),
            Rule::GentleBelow { slope, height: top } => rule(&[
                ("MaxSlope", slope),
                ("SlopeFade", 6.0),
                ("MaxHeight", (top * height).round()),
                ("HeightFade", (height * 0.04).max(2.0).round()),
            ]),
            Rule::HeightAbove {
                height: bottom,
                max_slope,
            } => rule(&[
                ("MinHeight", (bottom * height).round()),
                ("HeightFade", (height * 0.06).max(2.0).round()),
                ("MaxSlope", max_slope),
                ("SlopeFade", 8.0),
            ]),
            Rule::HeightBelow(top) => rule(&[
                ("MaxHeight", (top * height).round()),
                ("HeightFade", (height * 0.03).max(2.0).round()),
            ]),
            Rule::Patches(strength) => rule(&[
                ("RuleStrength", strength),
                ("RuleNoise", 1.0),
                ("MinSlope", 0.0),
                ("MaxSlope", 30.0),
                ("SlopeFade", 20.0),
            ]),
        }
        values
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axcenity_core::{ClassName, World};

    #[test]
    fn every_preset_layer_sets_real_properties() {
        let mut world = World::new();
        let layer = world.create(ClassName::TerrainLayer).unwrap();
        for preset in ALL.iter().chain(
            Preset::ALL
                .iter()
                .flat_map(|p| layers_for(*p).leak().iter()),
        ) {
            for (name, value) in preset.properties(128.0) {
                world
                    .set_property(layer, name, value)
                    .unwrap_or_else(|error| panic!("{}: {error}", preset.name));
            }
        }
    }

    #[test]
    fn snow_sits_high_on_a_tall_terrain() {
        let values = SNOW.properties(200.0);
        assert!(values.contains(&("MinHeight", Value::Number(120.0))));
        assert!(values.contains(&("AutoEnabled", Value::Bool(true))));
        // The first layer of every set covers everything: no rule.
        for preset in Preset::ALL {
            assert_eq!(layers_for(preset)[0].rule, Rule::None, "{preset:?}");
        }
    }
}
