//! Scatter rules in the editor (procedural worlds PW1): adding them from
//! presets, the Assets panel filling the selected rule's `Models`, and
//! Bake. What rules place is made by `procedural::Generators` each frame
//! (`generate_frame`), never as an undo step.

use axcenity_core::{FoliageSource, MAX_FOLIAGE_INSTANCES};
use axcenity_procedural::terrain_scatter;

use super::*;
use crate::terrain_studio::RulePreset;

impl EditorApp {
    /// Adds ScatterRules from `presets` to the end of `terrain`'s children,
    /// or of the selected Biome's (PW2), as one undo step.
    pub(super) fn add_scatter_rules(
        &mut self,
        terrain: InstanceId,
        presets: &[RulePreset],
        label: String,
    ) {
        let parent = self
            .selected
            .and_then(|id| crate::terrain_studio::biome_of(&self.world, id))
            .filter(|&biome| self.world.get(biome).and_then(|b| b.parent()) == Some(terrain))
            .unwrap_or(terrain);
        let mut moves = Vec::new();
        let last = self.make_scatter_rules(terrain, parent, presets, &mut moves);
        if !moves.is_empty() {
            self.history.record_tree(&mut self.world, label, moves);
            // Selected, so models clicked in Assets go into it.
            self.select_from_viewport(last);
            self.generate_frame();
        }
    }

    /// Makes ScatterRules from `presets` at the end of `parent` (`terrain`
    /// or one of its Biomes), adding their moves to `moves`, and returns the
    /// last. Inside a Biome they leave out their tree line: the biome
    /// says where they go.
    pub(super) fn make_scatter_rules(
        &mut self,
        terrain: InstanceId,
        parent: InstanceId,
        presets: &[RulePreset],
        moves: &mut Vec<Move>,
    ) -> Option<InstanceId> {
        let height = self
            .world
            .get(terrain)?
            .property("Size")
            .and_then(Value::as_vector3)
            .map_or(128.0, |size| size.y);
        let in_biome = parent != terrain;
        let mut last = None;
        for preset in presets {
            let Ok(rule) = self.world.create(ClassName::ScatterRule) else {
                continue;
            };
            for (property, value) in preset.properties(height) {
                if in_biome && property == "MaxHeight" {
                    continue;
                }
                self.world
                    .set_property(rule, property, value)
                    .expect("presets set ScatterRule properties");
            }
            let name = tree::unique_name(&self.world, parent, preset.name);
            self.world.set_name(rule, name).expect("just made");
            let index = self.world.get(parent).map_or(0, |t| t.children().len());
            let place = Place { parent, index };
            if history::put(&mut self.world, rule, Some(place)).is_ok() {
                moves.push(Move {
                    id: rule,
                    before: None,
                    after: Some(place),
                });
                last = Some(rule);
            }
        }
        last
    }

    /// The ScatterRule selected while editing, and what its `Models`
    /// names, for the Assets panel to pick.
    pub(in crate::app) fn scatter_rule_picking(&self) -> Option<(InstanceId, Vec<String>)> {
        if self.playing() {
            return None;
        }
        let id = self.selected?;
        let instance = self.world.get(id)?;
        if instance.class() != ClassName::ScatterRule {
            return None;
        }
        let models = instance.property("Models")?.as_str()?;
        Some((id, rule_entries(models)))
    }

    /// A prop or model clicked in the Assets panel while `rule` is
    /// selected: added to its `Models`, or taken out if it was there. One
    /// undo step.
    pub(in crate::app) fn pick_scatter_model(&mut self, rule: InstanceId, source: &FoliageSource) {
        let Some(instance) = self.world.get(rule) else {
            return;
        };
        let name = instance.name().to_owned();
        let mut entries = rule_entries(
            instance
                .property("Models")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        );
        let entry = terrain_scatter::entry(source);
        match entries.iter().position(|e| *e == entry) {
            Some(index) => {
                entries.remove(index);
            }
            None => entries.push(entry),
        }
        self.change(
            rule,
            Field::Property("Models"),
            format!("Change models of {name}"),
            vec![Edit::Property("Models", Value::String(entries.join("; ")))],
        );
        self.history.seal();
        self.generate_frame();
    }

    /// Bake: what `terrain`'s rules placed becomes painted foliage (up to
    /// the most a terrain holds) and its rules are switched off, as one
    /// undo step.
    pub(super) fn bake_scatter(&mut self, terrain: InstanceId) {
        let Some(data) = self.world.terrain(terrain) else {
            return;
        };
        let generated: Vec<_> = data.generated_foliage().cloned().collect();
        if generated.is_empty() {
            self.output
                .info("The scatter rules haven't placed anything to bake.");
            return;
        }
        let room = MAX_FOLIAGE_INSTANCES.saturating_sub(data.foliage().len());
        let baked = generated.len().min(room);
        let before = self.world.clone();
        if let Some(data) = self.world.terrain_mut(terrain) {
            data.foliage_mut()
                .extend(generated.iter().take(room).cloned());
            data.clear_generated();
        }
        for rule in crate::terrain_studio::scatter_rules(&self.world, terrain) {
            let _ = self.world.set_property(rule, "Enabled", Value::Bool(false));
        }
        let name = self
            .world
            .get(terrain)
            .map_or(String::new(), |t| t.name().to_owned());
        self.history.record_world(
            &mut self.world,
            format!("Bake Scatter Rules of {name}"),
            before,
        );
        if baked < generated.len() {
            self.output.warning(format!(
                "A terrain holds up to {MAX_FOLIAGE_INSTANCES} painted plants, so {baked} of the {} placed were baked.",
                generated.len()
            ));
        } else {
            self.output.info(format!(
                "Baked {baked} plants and props into painted foliage; the rules are off."
            ));
        }
    }
}

/// The entries of a rule's `Models`, trimmed, without empty ones.
fn rule_entries(models: &str) -> Vec<String> {
    models
        .split(';')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(str::to_owned)
        .collect()
}
