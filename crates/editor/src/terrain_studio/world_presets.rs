//! World presets (procedural worlds PW3): a whole landscape setup in one
//! pick, the generator's settings with the layers, biomes, rules and water
//! that suit them. Six are built in, made here from the studio's own layer,
//! biome and rule presets; the project's own are `.axworld` files in its
//! `presets/` folder (`project::world_presets`, `docs/terrain.md`).

use axcenity_project::WorldPreset;

use super::biomes::{ALPINE, BEACH, BiomePreset, LOWLAND_FOREST, MEADOW};
use super::presets::{GRASS, GRAVEL, ROCK, Rule as LayerRule, SNOW};
use super::scatter::{GRASS_TUFTS, RulePreset};
use super::*;

/// The height range built-in presets are made on (their heights are scaled
/// to the terrain they're applied to).
const BUILT_IN_HEIGHT: f32 = 128.0;

/// A built-in world: its landscape and what's made on it.
struct BuiltInWorld {
    name: &'static str,
    description: &'static str,
    landscape: Preset,
    /// Its layers, bottom first; `None` for the landscape's own.
    layers: Option<Vec<LayerPreset>>,
    biomes: &'static [BiomePreset],
    rules: &'static [RulePreset],
    /// Water, as a share of the height range.
    water: Option<f32>,
}

const DESERT_SCRUB: RulePreset = RulePreset {
    name: "Desert Scrub",
    models: "plant:Bush",
    spacing: 14.0,
    density: 0.25,
    max_slope: 20.0,
    below: None,
    avoid: 0.0,
    clumping: 0.6,
    clump_size: 60.0,
    scale: (0.5, 0.9),
    collide: false,
    seed: 53.0,
    lean: 0.3,
    sink: 0.1,
};
const TUNDRA_TUFTS: RulePreset = RulePreset {
    name: "Tundra Tufts",
    density: 2.0,
    clumping: 0.7,
    scale: (0.5, 0.9),
    ..GRASS_TUFTS
};
const JUNGLE: BiomePreset = BiomePreset {
    name: "Jungle",
    low: 0.12,
    high: 0.7,
    moisture: (0.0, 1.0),
    ..LOWLAND_FOREST
};

fn built_in_worlds() -> Vec<BuiltInWorld> {
    vec![
        BuiltInWorld {
            name: "Temperate Forest",
            description: "Rolling hills split between forest and meadow by moisture",
            landscape: Preset::RollingHills,
            layers: None,
            biomes: &[LOWLAND_FOREST, MEADOW],
            rules: &[],
            water: None,
        },
        BuiltInWorld {
            name: "Alpine",
            description: "A valley with forest low down, alpine turf above it and snowy peaks",
            landscape: Preset::AlpineValley,
            layers: None,
            biomes: &[LOWLAND_FOREST, ALPINE],
            rules: &[],
            water: None,
        },
        BuiltInWorld {
            name: "Desert",
            description: "Dunes with scrub here and there",
            landscape: Preset::DesertDunes,
            layers: None,
            biomes: &[],
            rules: &[DESERT_SCRUB],
            water: None,
        },
        BuiltInWorld {
            name: "Tropical Island",
            description: "An island in the sea: beaches round a jungle",
            landscape: Preset::Island,
            layers: None,
            biomes: &[JUNGLE, BEACH],
            rules: &[],
            water: Some(0.18),
        },
        BuiltInWorld {
            name: "Tundra",
            description: "Cold, flat ground: moss, gravel, snow and tufts of grass",
            landscape: Preset::Flatlands,
            layers: Some(vec![
                LayerPreset {
                    name: "Tundra Moss",
                    color: [0.44, 0.47, 0.36],
                    ..GRASS
                },
                LayerPreset {
                    rule: LayerRule::Patches(0.5),
                    ..GRAVEL
                },
                LayerPreset {
                    rule: LayerRule::HeightAbove {
                        height: 0.3,
                        max_slope: 40.0,
                    },
                    ..SNOW
                },
                ROCK,
            ]),
            biomes: &[],
            rules: &[TUNDRA_TUFTS],
            water: None,
        },
        BuiltInWorld {
            name: "Meadow",
            description: "Gentle hills of grass and flowers",
            landscape: Preset::RollingHills,
            layers: None,
            biomes: &[MEADOW],
            rules: &[],
            water: None,
        },
    ]
}

/// Puts `layers`, then each of `biomes` (its ground layer made if no layer
/// has its name, its rules inside it), then `rules` into `terrain`, as a
/// preset makes them. Nothing here is an undo step.
pub(crate) fn fill(
    world: &mut World,
    terrain: InstanceId,
    height: f32,
    layers: &[LayerPreset],
    biomes: &[BiomePreset],
    rules: &[RulePreset],
) {
    let add = |world: &mut World, class, name: &str, parent, properties: Vec<(&str, Value)>| {
        let id = world.create(class).expect("a preset class");
        for (property, value) in properties {
            let _ = world.set_property(id, property, value);
        }
        let name = crate::tree::unique_name(world, parent, name);
        let _ = world.set_name(id, name);
        let _ = world.set_parent(id, Some(parent));
        id
    };
    let rule_properties = |rule: &RulePreset, in_biome: bool| {
        rule.properties(height)
            .into_iter()
            .filter(|(name, _)| !(in_biome && *name == "MaxHeight"))
            .collect::<Vec<_>>()
    };
    for layer in layers.iter().take(MAX_TERRAIN_LAYERS) {
        add(
            world,
            ClassName::TerrainLayer,
            layer.name,
            terrain,
            layer.properties(height),
        );
    }
    for biome in biomes {
        let have = |world: &World, name: &str| {
            axcenity_terrain::materials::layers(world, terrain)
                .iter()
                .any(|&l| world.get(l).is_some_and(|l| l.name() == name))
        };
        let mut properties = biome.properties(height);
        if let Some(ground) = biome.ground {
            let count = axcenity_terrain::materials::layers(world, terrain).len();
            if have(world, ground.name) || count < MAX_TERRAIN_LAYERS {
                if !have(world, ground.name) {
                    add(
                        world,
                        ClassName::TerrainLayer,
                        ground.name,
                        terrain,
                        ground.properties(height),
                    );
                }
                properties.push(("GroundLayer", Value::String(ground.name.to_owned())));
            }
        }
        let id = add(world, ClassName::Biome, biome.name, terrain, properties);
        for rule in biome.rules {
            add(
                world,
                ClassName::ScatterRule,
                rule.name,
                id,
                rule_properties(rule, true),
            );
        }
    }
    for rule in rules {
        add(
            world,
            ClassName::ScatterRule,
            rule.name,
            terrain,
            rule_properties(rule, false),
        );
    }
}

/// The six built-in world presets.
pub fn built_in_world_presets() -> Vec<WorldPreset> {
    built_in_worlds()
        .into_iter()
        .map(|built| {
            let mut world = World::new();
            let terrain = world.create(ClassName::Terrain).expect("a Terrain");
            let _ = world.set_parent(terrain, Some(world.workspace()));
            let _ = world.set_property(
                terrain,
                "Size",
                Value::Vector3(axcenity_core::Vector3::new(512.0, BUILT_IN_HEIGHT, 512.0)),
            );
            let layers = built.layers.unwrap_or_else(|| layers_for(built.landscape));
            fill(
                &mut world,
                terrain,
                BUILT_IN_HEIGHT,
                &layers,
                built.biomes,
                built.rules,
            );
            let mut preset = WorldPreset::capture(
                &world,
                terrain,
                built.name,
                built.description,
                built.landscape.settings(),
            )
            .expect("a built-in world captures");
            preset.water = built.water;
            preset
        })
        .collect()
}

impl TerrainStudio {
    /// The World Presets section of the Generate tool: pick one, generate
    /// with it (Apply then sets up its layers, biomes, rules and water in
    /// the same undo step), or use only its layers, biomes and rules on the
    /// terrain as it is; and save this terrain's setup as a preset.
    pub(super) fn world_presets_section(
        &mut self,
        ui: &mut egui::Ui,
        cx: &StudioContext,
        actions: &mut Vec<TerrainAction>,
    ) {
        let presets = cx.world_presets;
        section(ui, "World Presets", |ui| {
            if presets.is_empty() {
                return;
            }
            self.world_preset = self.world_preset.min(presets.len() - 1);
            let chosen = &presets[self.world_preset];
            responsive::form(ui, "world presets", |ui| {
                field(ui, "World");
                egui::ComboBox::from_id_salt("world preset")
                    .selected_text(chosen.name.as_str())
                    .show_ui(ui, |ui| {
                        for (index, preset) in presets.iter().enumerate() {
                            ui.selectable_value(&mut self.world_preset, index, &preset.name)
                                .on_hover_text(&preset.description);
                        }
                    });
                ui.end_row();
            });
            let chosen = &presets[self.world_preset];
            if !chosen.description.is_empty() {
                hint(ui, &chosen.description);
            }
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add(widgets::button(
                        Role::Primary,
                        Some(icons::SPARKLE),
                        "Generate with it",
                        &theme::palette(ui.ctx()),
                    ))
                    .on_hover_text(
                        "Makes a preview of its landscape; Apply also puts in its layers, \
                         biomes, rules and water, as one undo step",
                    )
                    .clicked()
                {
                    actions.push(TerrainAction::GenerateWorldPreset(self.world_preset));
                }
                if ui
                    .button("Only its materials and rules")
                    .on_hover_text(
                        "Keeps this terrain's shape and replaces its layers, biomes and scatter \
                         rules (and water) with the preset's",
                    )
                    .clicked()
                {
                    actions.push(TerrainAction::ApplyWorldPreset(self.world_preset));
                }
            });
            if let Some(pending) = self.pending_world.as_ref().map(|p| p.name.clone()) {
                ui.horizontal_wrapped(|ui| {
                    hint(
                        ui,
                        &format!(
                            "Applying the preview also sets up {}'s layers, biomes and rules.",
                            pending
                        ),
                    );
                    if ui.small_button("Don't").clicked() {
                        self.pending_world = None;
                    }
                });
            }
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.preset_name)
                        .hint_text("Preset name")
                        .desired_width(140.0),
                );
                let name = self.preset_name.trim().to_owned();
                if ui
                    .add_enabled(
                        !name.is_empty(),
                        egui::Button::new(theme::with_icon(icons::FLOPPY_DISK, "Save as preset")),
                    )
                    .on_hover_text(
                        "Saves this terrain's layers, biomes, rules, water and landscape \
                         settings to the project's presets folder",
                    )
                    .clicked()
                {
                    actions.push(TerrainAction::SaveWorldPreset(name));
                }
            });
        });
    }
}
