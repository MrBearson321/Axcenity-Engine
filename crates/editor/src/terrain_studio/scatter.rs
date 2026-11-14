//! The Scatter section (procedural worlds PW1): the terrain's ScatterRules,
//! which place plants and props over the ground by themselves and follow
//! it as it's sculpted. Rules are added from presets, edited here or in
//! Properties, and baked into painted foliage (`docs/terrain.md`).

use super::*;
use axcenity_core::Vector2;

/// A ScatterRule to start from. Heights are fractions of the terrain's
/// height range, so they suit any size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RulePreset {
    pub name: &'static str,
    /// The rule's `Models`; empty for one you fill from Assets.
    pub models: &'static str,
    pub spacing: f64,
    pub density: f64,
    /// Degrees: only steeper ground than this (cliff pieces).
    pub min_slope: f64,
    pub max_slope: f64,
    /// Below this fraction of the height range (the tree line), if any.
    pub below: Option<f64>,
    pub avoid: f64,
    pub clumping: f64,
    pub clump_size: f64,
    pub scale: (f32, f32),
    pub collide: bool,
    pub seed: f64,
    /// 0 to 1: how far what it places leans with the slope.
    pub lean: f64,
    /// Studs it sinks into the ground.
    pub sink: f64,
}

impl RulePreset {
    /// The properties a new rule from this preset gets on a terrain
    /// `height` studs tall.
    pub fn properties(&self, height: f32) -> Vec<(&'static str, Value)> {
        let mut properties = vec![
            ("Models", Value::String(self.models.to_owned())),
            ("Spacing", Value::Number(self.spacing)),
            ("Density", Value::Number(self.density)),
            ("MinSlope", Value::Number(self.min_slope)),
            ("MaxSlope", Value::Number(self.max_slope)),
            ("AvoidRadius", Value::Number(self.avoid)),
            ("Clumping", Value::Number(self.clumping)),
            ("ClumpSize", Value::Number(self.clump_size)),
            (
                "ScaleRange",
                Value::Vector2(Vector2::new(self.scale.0, self.scale.1)),
            ),
            ("CanCollide", Value::Bool(self.collide)),
            ("Seed", Value::Number(self.seed)),
            ("AlignToSlope", Value::Number(self.lean)),
            ("SinkDepth", Value::Number(self.sink)),
        ];
        if let Some(below) = self.below {
            properties.push((
                "MaxHeight",
                Value::Number((below * f64::from(height)).round()),
            ));
        }
        properties
    }
}

pub const FOREST_TREES: RulePreset = RulePreset {
    name: "Forest Trees",
    models: "plant:Tree",
    spacing: 9.0,
    density: 0.8,
    min_slope: 0.0,
    max_slope: 30.0,
    below: Some(0.6),
    avoid: 0.0,
    clumping: 0.5,
    clump_size: 80.0,
    scale: (0.8, 1.3),
    collide: true,
    seed: 11.0,
    lean: 0.0,
    sink: 0.4,
};
pub const UNDERGROWTH: RulePreset = RulePreset {
    name: "Undergrowth",
    models: "plant:Bush",
    spacing: 4.0,
    density: 1.5,
    min_slope: 0.0,
    max_slope: 35.0,
    below: Some(0.65),
    avoid: 4.0,
    clumping: 0.3,
    clump_size: 48.0,
    scale: (0.7, 1.2),
    collide: false,
    seed: 23.0,
    lean: 0.3,
    sink: 0.1,
};
pub const MEADOW_FLOWERS: RulePreset = RulePreset {
    name: "Meadow Flowers",
    models: "plant:Flower",
    spacing: 2.5,
    density: 4.0,
    min_slope: 0.0,
    max_slope: 25.0,
    below: Some(0.5),
    avoid: 2.0,
    clumping: 0.7,
    clump_size: 24.0,
    scale: (0.8, 1.2),
    collide: false,
    seed: 37.0,
    lean: 0.5,
    sink: 0.0,
};
pub const GRASS_TUFTS: RulePreset = RulePreset {
    name: "Grass Tufts",
    models: "plant:Grass",
    spacing: 2.0,
    density: 10.0,
    min_slope: 0.0,
    max_slope: 30.0,
    below: Some(0.6),
    avoid: 1.5,
    clumping: 0.2,
    clump_size: 32.0,
    scale: (0.8, 1.3),
    collide: false,
    seed: 41.0,
    lean: 0.7,
    sink: 0.0,
};
/// Rock faces on steep ground (PW7): upright slabs leaning a little with
/// the slope and sunk into it, so slopes too steep for plants read as
/// cliffs. Beside a hole cut with the Holes tool they frame a cave mouth.
pub const CLIFF_ROCKS: RulePreset = RulePreset {
    name: "Cliff Rocks",
    models: "prop:Cliff",
    spacing: 7.0,
    density: 1.2,
    min_slope: 38.0,
    max_slope: 90.0,
    below: None,
    avoid: 0.0,
    clumping: 0.0,
    clump_size: 48.0,
    scale: (0.8, 1.5),
    collide: true,
    seed: 61.0,
    lean: 0.35,
    sink: 2.5,
};
/// Boulders on gentle and middling slopes, in groups, leaning with them.
pub const BOULDERS: RulePreset = RulePreset {
    name: "Boulders",
    models: "prop:Rock",
    spacing: 10.0,
    density: 0.25,
    min_slope: 8.0,
    max_slope: 40.0,
    below: None,
    avoid: 3.0,
    clumping: 0.6,
    clump_size: 40.0,
    scale: (0.6, 1.6),
    collide: true,
    seed: 67.0,
    lean: 0.8,
    sink: 0.6,
};
/// No models yet: select it and click models or props in Assets.
pub const YOUR_MODELS: RulePreset = RulePreset {
    name: "Scatter Rule",
    models: "",
    spacing: 8.0,
    density: 1.0,
    min_slope: 0.0,
    max_slope: 35.0,
    below: None,
    avoid: 0.0,
    clumping: 0.0,
    clump_size: 48.0,
    scale: (0.85, 1.15),
    collide: false,
    seed: 1.0,
    lean: 0.0,
    sink: 0.0,
};

/// The single rules the Add menu offers.
pub const RULE_PRESETS: [RulePreset; 7] = [
    FOREST_TREES,
    UNDERGROWTH,
    MEADOW_FLOWERS,
    GRASS_TUFTS,
    CLIFF_ROCKS,
    BOULDERS,
    YOUR_MODELS,
];

/// A forest floor in one click: trees, then bushes and grass keeping clear
/// of them.
pub const FOREST_SET: [RulePreset; 3] = [FOREST_TREES, UNDERGROWTH, GRASS_TUFTS];

/// The ScatterRules of `terrain`, in order (switched off ones too): its
/// own, and those inside its Biomes where each Biome stands.
pub fn scatter_rules(world: &World, terrain: InstanceId) -> Vec<InstanceId> {
    let children = |id: InstanceId| {
        world
            .get(id)
            .map(|t| t.children().to_vec())
            .unwrap_or_default()
    };
    let class = |id: InstanceId| world.get(id).map(|i| i.class());
    let mut rules = Vec::new();
    for child in children(terrain) {
        match class(child) {
            Some(ClassName::ScatterRule) => rules.push(child),
            Some(ClassName::Biome) => rules.extend(
                children(child)
                    .into_iter()
                    .filter(|&c| class(c) == Some(ClassName::ScatterRule)),
            ),
            _ => {}
        }
    }
    rules
}

/// What a rule's `Models` names, short: "Tree, Bush, Pine.glb".
fn models_summary(models: &str) -> String {
    let names: Vec<String> = models
        .split(';')
        .filter_map(axcenity_procedural::terrain_scatter::source)
        .map(|source| source.name().to_owned())
        .collect();
    names.join(", ")
}

impl TerrainStudio {
    /// The terrain's scatter rules: each switched on or off, picked to
    /// edit, moved or deleted; the selected one's main settings; Add and
    /// Bake.
    pub(super) fn scatter_section(
        &mut self,
        ui: &mut egui::Ui,
        cx: &StudioContext,
        terrain: InstanceId,
        actions: &mut Vec<TerrainAction>,
    ) {
        let world = cx.world;
        let palette = theme::palette(ui.ctx());
        let rules = scatter_rules(world, terrain);
        let placed = world
            .terrain(terrain)
            .map_or(0, |data| data.generated_count());
        section(ui, "Scatter Rules", |ui| {
            if rules.is_empty() {
                hint(
                    ui,
                    "Rules place plants, props and models over the ground by themselves \
                     (trees on gentle slopes below the snow line, say), and follow it as you \
                     sculpt. Add one to start.",
                );
            }
            for (index, &rule) in rules.iter().enumerate() {
                let Some(instance) = world.get(rule) else {
                    continue;
                };
                let selected = cx.selected == Some(rule);
                ui.horizontal(|ui| {
                    let mut on =
                        instance.property("Enabled").and_then(Value::as_bool) == Some(true);
                    if ui
                        .checkbox(&mut on, "")
                        .on_hover_text("Switch this rule on or off")
                        .changed()
                    {
                        actions.push(TerrainAction::SetLayer(rule, "Enabled", Value::Bool(on)));
                    }
                    let biome = instance
                        .parent()
                        .filter(|&p| p != terrain)
                        .and_then(|p| world.get(p))
                        .map(|b| b.name().to_owned());
                    let name = match biome {
                        Some(biome) => format!("{}. {} (in {biome})", index + 1, instance.name()),
                        None => format!("{}. {}", index + 1, instance.name()),
                    };
                    let row = ui.selectable_label(selected, name).on_hover_text(
                        "Select it to change its settings, and to add models from Assets",
                    );
                    if row.clicked() {
                        actions.push(TerrainAction::Select(rule));
                    }
                    row.context_menu(|ui| {
                        if ui.button("Move up").clicked() {
                            actions.push(TerrainAction::MoveLayer(rule, true));
                            ui.close();
                        }
                        if ui.button("Move down").clicked() {
                            actions.push(TerrainAction::MoveLayer(rule, false));
                            ui.close();
                        }
                        if ui.button("Delete").clicked() {
                            actions.push(TerrainAction::RemoveLayer(rule));
                            ui.close();
                        }
                    });
                });
                let models = instance
                    .property("Models")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let summary = models_summary(models);
                ui.label(
                    egui::RichText::new(if summary.is_empty() {
                        "Nothing to place yet: select it and click models or props in Assets."
                            .to_owned()
                    } else {
                        summary
                    })
                    .size(theme::type_scale::SMALL)
                    .color(palette.text_weak),
                );
                if selected {
                    card(ui, |ui| {
                        rule_settings(ui, world, terrain, rule, instance, actions)
                    });
                }
            }
            if placed > 0 {
                hint(ui, &format!("{placed} placed by rules."));
            }
            ui.horizontal_wrapped(|ui| {
                let into = cx
                    .selected
                    .and_then(|id| super::biomes::biome_of(world, id))
                    .filter(|&b| world.get(b).and_then(|b| b.parent()) == Some(terrain))
                    .and_then(|b| world.get(b))
                    .map(|b| b.name().to_owned());
                let add = match &into {
                    Some(biome) => format!("Add rule to {biome}"),
                    None => "Add rule".to_owned(),
                };
                crate::widgets::menu_button(ui, Some(icons::PLUS), &add, |ui| {
                    if ui
                        .button(theme::with_icon(
                            icons::TREE,
                            "Forest (trees, undergrowth, grass)",
                        ))
                        .on_hover_text(
                            "Three rules: trees first, then bushes and grass keeping clear of them",
                        )
                        .clicked()
                    {
                        actions.push(TerrainAction::AddScatterRules(
                            FOREST_SET.to_vec(),
                            "Add Forest Rules".to_owned(),
                        ));
                        ui.close();
                    }
                    ui.separator();
                    for preset in RULE_PRESETS {
                        if ui.button(preset.name).clicked() {
                            actions.push(TerrainAction::AddScatterRules(
                                vec![preset],
                                format!("Add {}", preset.name),
                            ));
                            ui.close();
                        }
                    }
                });
                if ui
                    .add_enabled(
                        placed > 0,
                        egui::Button::new(theme::with_icon(icons::STAMP, "Bake")),
                    )
                    .on_hover_text(
                        "Turn what the rules placed into painted foliage you can brush and erase \
                         by hand. The rules are switched off, so they don't place it twice.",
                    )
                    .clicked()
                {
                    actions.push(TerrainAction::BakeScatter);
                }
            });
        });
    }
}

/// The selected rule's main settings; the rest are in Properties.
fn rule_settings(
    ui: &mut egui::Ui,
    world: &World,
    terrain: InstanceId,
    rule: InstanceId,
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
        actions.push(TerrainAction::SetLayer(rule, name, value));
    };
    let top = world
        .get(terrain)
        .and_then(|t| t.property("Size"))
        .and_then(Value::as_vector3)
        .map_or(128.0, |size| f64::from(size.y));
    responsive::form(ui, ("scatter rule", rule), |ui| {
        for (name, label, otherwise, range, suffix, tip) in [
            (
                "Spacing",
                "Spacing",
                8.0,
                1.0..=64.0,
                " studs",
                "About how far apart two of its spots are.",
            ),
            (
                "Density",
                "Density",
                1.0,
                0.0..=100.0,
                " /100 st²",
                "How many per 100 square studs, at most one per Spacing square.",
            ),
            (
                "MaxSlope",
                "Max slope",
                35.0,
                0.0..=90.0,
                "°",
                "Nothing on ground steeper than this.",
            ),
            (
                "AvoidRadius",
                "Keep clear",
                0.0,
                0.0..=32.0,
                " studs",
                "How far it keeps from what the rules above it placed.",
            ),
            (
                "Clumping",
                "Clumping",
                0.0,
                0.0..=1.0,
                "",
                "0 spreads it evenly; 1 gathers it into clumps.",
            ),
        ] {
            field(ui, label);
            let mut value = number(name, otherwise);
            if ui
                .add(egui::Slider::new(&mut value, range).suffix(suffix))
                .on_hover_text(tip)
                .changed()
            {
                set(name, Value::Number(value));
            }
            ui.end_row();
        }
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
                .on_hover_text("Nothing lower than this many studs above the terrain's bottom")
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
                .on_hover_text("Nothing higher than this (the tree line)")
                .changed()
            {
                set("MaxHeight", Value::Number(high));
            }
        });
        ui.end_row();
        field(ui, "On layer");
        let current = instance
            .property("Layer")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let names: Vec<String> = axcenity_terrain::materials::layers(world, terrain)
            .into_iter()
            .filter_map(|layer| world.get(layer).map(|i| i.name().to_owned()))
            .collect();
        egui::ComboBox::from_id_salt(("scatter layer", rule))
            .selected_text(if current.is_empty() {
                "Any ground"
            } else {
                current.as_str()
            })
            .show_ui(ui, |ui| {
                if ui
                    .selectable_label(current.is_empty(), "Any ground")
                    .clicked()
                {
                    set("Layer", Value::String(String::new()));
                }
                for name in &names {
                    if ui.selectable_label(*name == current, name).clicked() {
                        set("Layer", Value::String(name.clone()));
                    }
                }
            })
            .response
            .on_hover_text("Only where this material layer covers the ground");
        ui.end_row();
        field(ui, "Seed");
        ui.horizontal(|ui| {
            let mut seed = number("Seed", 1.0);
            if ui
                .add(egui::DragValue::new(&mut seed).speed(1.0).max_decimals(0))
                .on_hover_text("The same seed gives the same layout")
                .changed()
            {
                set("Seed", Value::Number(seed.round()));
            }
            if ui
                .small_button(icons::DICE_FIVE)
                .on_hover_text("A new random layout")
                .clicked()
            {
                set("Seed", Value::Number(random_seed() as f64));
            }
        });
        ui.end_row();
        field(ui, "Solid");
        let mut solid = instance.property("CanCollide").and_then(Value::as_bool) == Some(true);
        if ui
            .checkbox(&mut solid, "Parts and characters bump into them")
            .changed()
        {
            set("CanCollide", Value::Bool(solid));
        }
        ui.end_row();
    });
}
