//! The Biomes section (procedural worlds PW2): the terrain's Biomes, areas
//! with their own ground layer and scatter rules, placed by height, slope
//! and moisture or painted by hand. Added from presets (with their ground
//! layer and rules), edited here or in Properties, painted with the Paint
//! tool on their ground layer (`docs/terrain.md`).

use super::presets::{LayerPreset, Rule as LayerRule, SAND};
use super::scatter::{FOREST_SET, GRASS_TUFTS, MEADOW_FLOWERS, RulePreset};
use super::*;

/// A Biome to start from. Heights are fractions of the terrain's height
/// range, so they suit any size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BiomePreset {
    pub name: &'static str,
    /// Its ground layer, made if the terrain has no layer of that name.
    pub ground: Option<LayerPreset>,
    /// From and to this fraction of the height range (1 or more: no top).
    pub low: f64,
    pub high: f64,
    pub max_slope: f64,
    /// From and to this moisture (0 dry, 1 wet).
    pub moisture: (f64, f64),
    /// The ScatterRules made inside it.
    pub rules: &'static [RulePreset],
}

impl BiomePreset {
    /// The Biome properties for this preset on a terrain `height` studs
    /// tall (not its GroundLayer, which the app sets to the layer it uses).
    pub fn properties(&self, height: f32) -> Vec<(&'static str, Value)> {
        let height = f64::from(height);
        let mut values = vec![
            ("MinHeight", Value::Number((self.low * height).round())),
            (
                "HeightFade",
                Value::Number((height * 0.06).max(4.0).round()),
            ),
            ("MaxSlope", Value::Number(self.max_slope)),
            ("SlopeFade", Value::Number(8.0)),
            ("MinMoisture", Value::Number(self.moisture.0)),
            ("MaxMoisture", Value::Number(self.moisture.1)),
        ];
        if self.high < 1.0 {
            values.push(("MaxHeight", Value::Number((self.high * height).round())));
        }
        values
    }
}

const fn ground(name: &'static str, color: [f32; 3], roughness: f64) -> LayerPreset {
    LayerPreset {
        name,
        color,
        roughness,
        tile: 6.0,
        triplanar: false,
        rule: LayerRule::None,
    }
}

pub const LOWLAND_FOREST: BiomePreset = BiomePreset {
    name: "Lowland Forest",
    ground: Some(ground("Forest Floor", [0.27, 0.33, 0.17], 0.95)),
    low: 0.1,
    high: 0.55,
    max_slope: 40.0,
    moisture: (0.45, 1.0),
    rules: &FOREST_SET,
};
pub const MEADOW: BiomePreset = BiomePreset {
    name: "Meadow",
    ground: Some(ground("Meadow Grass", [0.45, 0.6, 0.26], 0.95)),
    low: 0.1,
    high: 0.55,
    max_slope: 30.0,
    moisture: (0.0, 0.55),
    rules: &[MEADOW_FLOWERS, GRASS_TUFTS],
};
pub const ALPINE: BiomePreset = BiomePreset {
    name: "Alpine",
    ground: Some(ground("Alpine Turf", [0.42, 0.47, 0.33], 0.9)),
    low: 0.55,
    high: 0.8,
    max_slope: 40.0,
    moisture: (0.0, 1.0),
    rules: &[GRASS_TUFTS],
};
pub const BEACH: BiomePreset = BiomePreset {
    name: "Beach",
    ground: Some(LayerPreset {
        rule: LayerRule::None,
        ..SAND
    }),
    low: 0.0,
    high: 0.1,
    max_slope: 25.0,
    moisture: (0.0, 1.0),
    rules: &[],
};
/// Covers nothing by itself until its settings are changed or its ground
/// is painted; no rules yet.
pub const EMPTY_BIOME: BiomePreset = BiomePreset {
    name: "Biome",
    ground: None,
    low: 0.0,
    high: 1.0,
    max_slope: 90.0,
    moisture: (0.0, 1.0),
    rules: &[],
};

/// The biomes the Add menu offers.
pub const BIOME_PRESETS: [BiomePreset; 5] = [LOWLAND_FOREST, MEADOW, ALPINE, BEACH, EMPTY_BIOME];

/// The Biome children of `terrain`, in order (switched off ones too).
pub fn biomes(world: &World, terrain: InstanceId) -> Vec<InstanceId> {
    world
        .get(terrain)
        .map(|t| t.children().to_vec())
        .unwrap_or_default()
        .into_iter()
        .filter(|&c| world.get(c).map(|i| i.class()) == Some(ClassName::Biome))
        .collect()
}

/// The Biome a selected Biome or rule is, or is in.
pub fn biome_of(world: &World, id: InstanceId) -> Option<InstanceId> {
    let instance = world.get(id)?;
    match instance.class() {
        ClassName::Biome => Some(id),
        ClassName::ScatterRule => instance
            .parent()
            .filter(|&p| world.get(p).map(|i| i.class()) == Some(ClassName::Biome)),
        _ => None,
    }
}

/// "Forest Floor · 12 to 70 studs · 3 rules"
fn biome_summary(world: &World, biome: InstanceId) -> String {
    let Some(instance) = world.get(biome) else {
        return String::new();
    };
    let number = |name: &str| instance.property(name).and_then(Value::as_number);
    let ground = instance
        .property("GroundLayer")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let mut parts = vec![if ground.is_empty() {
        "No ground layer".to_owned()
    } else {
        ground.to_owned()
    }];
    if instance.property("AutoEnabled").and_then(Value::as_bool) == Some(false) {
        parts.push("only where painted".to_owned());
    } else {
        let low = number("MinHeight").unwrap_or(0.0);
        let high = number("MaxHeight").unwrap_or(10000.0);
        parts.push(if high >= 10000.0 && low <= 0.0 {
            "any height".to_owned()
        } else if high >= 10000.0 {
            format!("above {low:.0} studs")
        } else {
            format!("{low:.0} to {high:.0} studs")
        });
    }
    let rules = instance
        .children()
        .iter()
        .filter(|&&c| world.get(c).map(|i| i.class()) == Some(ClassName::ScatterRule))
        .count();
    parts.push(match rules {
        0 => "no rules".to_owned(),
        1 => "1 rule".to_owned(),
        n => format!("{n} rules"),
    });
    parts.join(" · ")
}

impl TerrainStudio {
    /// The terrain's biomes: each switched on or off, picked to edit,
    /// moved or deleted; the selected one's main settings and Paint; Add.
    pub(super) fn biomes_section(
        &mut self,
        ui: &mut egui::Ui,
        cx: &StudioContext,
        terrain: InstanceId,
        actions: &mut Vec<TerrainAction>,
    ) {
        let world = cx.world;
        let palette = theme::palette(ui.ctx());
        let list = biomes(world, terrain);
        let open = cx.selected.and_then(|id| biome_of(world, id));
        section(ui, "Biomes", |ui| {
            if list.is_empty() {
                hint(
                    ui,
                    "Biomes split the terrain into areas, such as forest low down and alpine \
                     turf higher up, each with its own ground and scatter rules. They follow \
                     height, slope and moisture by themselves, and you can paint them.",
                );
            }
            for &biome in &list {
                let Some(instance) = world.get(biome) else {
                    continue;
                };
                ui.horizontal(|ui| {
                    let mut on =
                        instance.property("Enabled").and_then(Value::as_bool) == Some(true);
                    if ui
                        .checkbox(&mut on, "")
                        .on_hover_text("Switch this biome on or off")
                        .changed()
                    {
                        actions.push(TerrainAction::SetLayer(biome, "Enabled", Value::Bool(on)));
                    }
                    let row = ui
                        .selectable_label(open == Some(biome), instance.name())
                        .on_hover_text("Select it to change it, paint it, or add rules to it");
                    if row.clicked() {
                        actions.push(TerrainAction::Select(biome));
                    }
                    row.context_menu(|ui| {
                        if ui.button("Move up").clicked() {
                            actions.push(TerrainAction::MoveLayer(biome, true));
                            ui.close();
                        }
                        if ui.button("Move down").clicked() {
                            actions.push(TerrainAction::MoveLayer(biome, false));
                            ui.close();
                        }
                        if ui.button("Delete").clicked() {
                            actions.push(TerrainAction::RemoveLayer(biome));
                            ui.close();
                        }
                    });
                });
                ui.label(
                    egui::RichText::new(biome_summary(world, biome))
                        .size(theme::type_scale::SMALL)
                        .color(palette.text_weak),
                );
                if open == Some(biome) {
                    card(ui, |ui| {
                        self.biome_settings(ui, world, terrain, biome, instance, actions)
                    });
                }
            }
            if list.len() > 1 {
                hint(ui, "Where biomes overlap, the lower one in the list wins.");
            }
            crate::widgets::menu_button(ui, Some(icons::PLUS), "Add biome", |ui| {
                for preset in BIOME_PRESETS {
                    let tip = match preset.ground {
                        Some(ground) if !preset.rules.is_empty() => {
                            let rules: Vec<&str> = preset.rules.iter().map(|r| r.name).collect();
                            format!("{} ground, with {}", ground.name, rules.join(", "))
                        }
                        Some(ground) => format!("{} ground", ground.name),
                        None => "Set it up yourself".to_owned(),
                    };
                    if ui.button(preset.name).on_hover_text(tip).clicked() {
                        actions.push(TerrainAction::AddBiome(preset));
                        ui.close();
                    }
                }
            });
        });
    }

    /// The selected biome's main settings (the rest are in Properties), and
    /// Paint, which picks the Paint tool on its ground layer.
    fn biome_settings(
        &mut self,
        ui: &mut egui::Ui,
        world: &World,
        terrain: InstanceId,
        biome: InstanceId,
        instance: &axcenity_core::Instance,
        actions: &mut Vec<TerrainAction>,
    ) {
        let number = |name: &str, otherwise: f64| {
            instance
                .property(name)
                .and_then(Value::as_number)
                .unwrap_or(otherwise)
        };
        let mut set = |name: &'static str, value: Value| {
            actions.push(TerrainAction::SetLayer(biome, name, value));
        };
        let top = world
            .get(terrain)
            .and_then(|t| t.property("Size"))
            .and_then(Value::as_vector3)
            .map_or(128.0, |size| f64::from(size.y));
        let layers: Vec<String> = axcenity_terrain::materials::layers(world, terrain)
            .into_iter()
            .filter_map(|layer| world.get(layer).map(|i| i.name().to_owned()))
            .collect();
        let ground = instance
            .property("GroundLayer")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        responsive::form(ui, ("biome", biome), |ui| {
            field(ui, "Ground");
            egui::ComboBox::from_id_salt(("biome ground", biome))
                .selected_text(if ground.is_empty() {
                    "None"
                } else {
                    ground.as_str()
                })
                .show_ui(ui, |ui| {
                    if ui.selectable_label(ground.is_empty(), "None").clicked() {
                        set("GroundLayer", Value::String(String::new()));
                    }
                    for name in &layers {
                        if ui.selectable_label(*name == ground, name).clicked() {
                            set("GroundLayer", Value::String(name.clone()));
                        }
                    }
                })
                .response
                .on_hover_text(
                    "The material layer under this biome. Painting it paints the biome.",
                );
            ui.end_row();
            let auto = instance.property("AutoEnabled").and_then(Value::as_bool) != Some(false);
            field(ui, "Placed");
            let mut painted_only = !auto;
            if ui
                .checkbox(&mut painted_only, "Only where painted")
                .on_hover_text(
                    "Off: it covers ground by itself where the settings below hold, as well as \
                     where you paint it",
                )
                .changed()
            {
                set("AutoEnabled", Value::Bool(!painted_only));
            }
            ui.end_row();
            if auto {
                field(ui, "Heights");
                ui.horizontal(|ui| {
                    let mut low = number("MinHeight", 0.0);
                    let mut high = number("MaxHeight", 10000.0).min(top);
                    if ui
                        .add(
                            egui::DragValue::new(&mut low)
                                .range(0.0..=top)
                                .speed(0.5)
                                .max_decimals(1),
                        )
                        .on_hover_text("Studs above the terrain's bottom where it starts")
                        .changed()
                    {
                        set("MinHeight", Value::Number(low));
                    }
                    ui.label("to");
                    if ui
                        .add(
                            egui::DragValue::new(&mut high)
                                .range(0.0..=top)
                                .speed(0.5)
                                .max_decimals(1),
                        )
                        .on_hover_text("Where it ends (the top of the terrain: no end)")
                        .changed()
                    {
                        let high = if high >= top { 10000.0 } else { high };
                        set("MaxHeight", Value::Number(high));
                    }
                });
                ui.end_row();
                field(ui, "Max slope");
                let mut slope = number("MaxSlope", 90.0);
                if ui
                    .add(egui::Slider::new(&mut slope, 0.0..=90.0).suffix("°"))
                    .on_hover_text("It doesn't cover ground steeper than this")
                    .changed()
                {
                    set("MaxSlope", Value::Number(slope));
                }
                ui.end_row();
                for (name, label, otherwise, tip) in [
                    (
                        "MinMoisture",
                        "Driest",
                        0.0,
                        "0 is the driest ground: raise it to keep this biome to wetter places",
                    ),
                    (
                        "MaxMoisture",
                        "Wettest",
                        1.0,
                        "1 is the wettest ground: lower it to keep this biome to drier places",
                    ),
                    (
                        "EdgeNoise",
                        "Ragged edges",
                        0.3,
                        "0 gives smooth edges; 1 breaks them up the most",
                    ),
                ] {
                    field(ui, label);
                    let mut value = number(name, otherwise);
                    if ui
                        .add(egui::Slider::new(&mut value, 0.0..=1.0))
                        .on_hover_text(tip)
                        .changed()
                    {
                        set(name, Value::Number(value));
                    }
                    ui.end_row();
                }
            }
        });
        let paint_layer = layers.iter().position(|name| *name == ground);
        let painting =
            self.tool == Some(TerrainTool::Paint) && paint_layer == Some(self.paint_layer);
        if ui
            .add_enabled(
                paint_layer.is_some(),
                egui::Button::new(theme::with_icon(icons::PAINT_BRUSH, "Paint this biome"))
                    .selected(painting),
            )
            .on_hover_text(if paint_layer.is_some() {
                "Paint its ground layer to spread the biome there; Shift erases back to the \
                 automatic biomes"
            } else {
                "Pick a ground layer first"
            })
            .on_disabled_hover_text("Pick a ground layer first")
            .clicked()
            && let Some(layer) = paint_layer
        {
            self.tool = Some(TerrainTool::Paint);
            self.paint_layer = layer;
        }
    }
}
