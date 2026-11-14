//! Biomes in the editor (procedural worlds PW2): adding one from a preset,
//! with its ground layer and scatter rules, as one undo step. Painting a
//! biome is the Paint tool on its ground layer, so it needs nothing here.

use super::layers::new_layer;
use super::*;
use crate::terrain_studio::BiomePreset;

impl EditorApp {
    /// Adds a Biome like `preset` to the end of `terrain`'s children, its
    /// ground layer (unless the terrain has a layer of that name, or no
    /// room for one) and its rules inside it, as one undo step, and
    /// selects it.
    pub(super) fn add_biome(&mut self, terrain: InstanceId, preset: &BiomePreset) {
        let Some(instance) = self.world.get(terrain) else {
            return;
        };
        let height = instance
            .property("Size")
            .and_then(Value::as_vector3)
            .map_or(128.0, |size| size.y);
        let mut moves = Vec::new();
        let mut put = |world: &mut World, id: InstanceId, parent: InstanceId| {
            let index = world.get(parent).map_or(0, |t| t.children().len());
            let place = Place { parent, index };
            if history::put(world, id, Some(place)).is_ok() {
                moves.push(Move {
                    id,
                    before: None,
                    after: Some(place),
                });
            }
        };
        let layers = materials::layers(&self.world, terrain);
        let ground = preset.ground.and_then(|ground| {
            let have = layers
                .iter()
                .any(|&l| self.world.get(l).is_some_and(|l| l.name() == ground.name));
            if have {
                return Some(ground.name.to_owned());
            }
            if layers.len() >= MAX_TERRAIN_LAYERS {
                self.output.warning(format!(
                    "A terrain draws up to {MAX_TERRAIN_LAYERS} layers, so {} has no ground layer; \
                     pick one of the terrain's in its settings.",
                    preset.name
                ));
                return None;
            }
            let layer = new_layer(&mut self.world, terrain, &ground, height)?;
            put(&mut self.world, layer, terrain);
            self.world.get(layer).map(|l| l.name().to_owned())
        });
        let Ok(biome) = self.world.create(ClassName::Biome) else {
            return;
        };
        for (property, value) in preset.properties(height) {
            self.world
                .set_property(biome, property, value)
                .expect("presets set Biome properties");
        }
        if let Some(ground) = ground {
            self.world
                .set_property(biome, "GroundLayer", Value::String(ground))
                .expect("a Biome has a GroundLayer");
        }
        let name = tree::unique_name(&self.world, terrain, preset.name);
        self.world.set_name(biome, name).expect("just made");
        put(&mut self.world, biome, terrain);
        self.make_scatter_rules(terrain, biome, preset.rules, &mut moves);
        if moves.is_empty() {
            return;
        }
        self.history
            .record_tree(&mut self.world, format!("Add {} Biome", preset.name), moves);
        self.select_from_viewport(Some(biome));
        self.generate_frame();
    }
}
