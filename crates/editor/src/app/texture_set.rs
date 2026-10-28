//! Applying a texture set to Parts, MeshParts, and terrain layers.
//!
//! One drop or double-click is one undo step. A Part grows a
//! SurfaceAppearance when it has none. A MeshPart with none grows one per
//! material in its model. Terrain uses its top layer, and a selected layer
//! is filled itself. Height is a terrain map only.

use std::path::Path;

use axcenity_assets::{Channel, MapSlot, TextureSet, read_texture_set};
use axcenity_core::{
    ClassName, Color3, Instance, InstanceId, Material, TextureChannel, Value, World,
};

use super::EditorApp;
use crate::asset_browser::AssetEntry;
use crate::history::{self, Move, Place, PropertyEdit, Snapshot};

const SURFACE_MAPS: &[(&str, MapSlot)] = &[
    ("ColorMap", MapSlot::BaseColor),
    ("NormalMap", MapSlot::Normal),
    ("MetalnessMap", MapSlot::Metalness),
    ("RoughnessMap", MapSlot::Roughness),
    ("EmissiveMap", MapSlot::Emissive),
    ("AmbientOcclusionMap", MapSlot::Occlusion),
    ("OpacityMap", MapSlot::Opacity),
    ("SpecularMap", MapSlot::Specular),
];

const LAYER_MAPS: &[(&str, MapSlot)] = &[
    ("ColorMap", MapSlot::BaseColor),
    ("NormalMap", MapSlot::Normal),
    ("RoughnessMap", MapSlot::Roughness),
    ("MetalnessMap", MapSlot::Metalness),
    ("AmbientOcclusionMap", MapSlot::Occlusion),
    ("HeightMap", MapSlot::Height),
];

impl EditorApp {
    /// Fills `hit` from the texture set `entry`. With no hit (a
    /// double-click), fills the selection instead.
    pub(super) fn apply_texture_set(&mut self, entry: &AssetEntry, hit: Option<InstanceId>) {
        let Some(project) = self.project.as_ref().map(|project| project.folder.clone()) else {
            return;
        };
        let Some(set) = self.read_set(&project, &entry.path) else {
            return;
        };
        let targets = match hit {
            Some(id) => vec![id],
            None => unique(self.selection.clone()),
        };
        if targets.is_empty() {
            self.output.info(
                "Drop a texture set onto a part, MeshPart, or terrain, or select one and \
                 double-click the set.",
            );
            return;
        }
        let mut moves = Vec::new();
        let mut properties = Vec::new();
        let mut applied = false;
        for id in targets {
            if self.apply_one(id, &set, &mut moves, &mut properties) {
                applied = true;
            }
        }
        if !applied {
            return;
        }
        // Material resets Metalness and Roughness, so it has to be put back
        // first on undo and redo.
        properties.sort_by_key(|edit| edit.before.as_material().is_none());
        self.history.record_tree_and_properties(
            &mut self.world,
            format!("Texture {}", entry.name),
            moves,
            properties,
        );
    }

    fn read_set(&mut self, project: &Path, path: &str) -> Option<TextureSet> {
        let Some(relative) = axcenity_assets::safe_path(path) else {
            self.output.error(format!(
                "Could not read `{path}`: it isn't inside the project."
            ));
            return None;
        };
        match read_texture_set(&project.join(relative)) {
            Ok(set) => Some(set),
            Err(err) => {
                self.output
                    .error(format!("Could not read `{path}`: {err}."));
                None
            }
        }
    }

    /// Applies `set` to `id`. Returns whether anything was changed.
    fn apply_one(
        &mut self,
        id: InstanceId,
        set: &TextureSet,
        moves: &mut Vec<Move>,
        properties: &mut Vec<PropertyEdit>,
    ) -> bool {
        let Some(class) = self.world.get(id).map(|instance| instance.class()) else {
            return false;
        };
        match class {
            ClassName::Part | ClassName::Seat | ClassName::VehicleSeat | ClassName::DamageZone => {
                self.apply_part(id, set, moves, properties)
            }
            ClassName::MeshPart => self.apply_mesh(id, set, moves, properties),
            ClassName::Terrain => self.apply_terrain(id, set, properties),
            ClassName::TerrainLayer => {
                self.fill_existing(id, set, true, properties);
                true
            }
            _ => {
                let name = self
                    .world
                    .get(id)
                    .map(|instance| instance.name().to_owned())
                    .unwrap_or_else(|| "That".to_owned());
                self.output.info(format!(
                    "{name} can't wear a texture set. Use a part, MeshPart, terrain, or terrain \
                     layer."
                ));
                false
            }
        }
    }

    fn apply_part(
        &mut self,
        id: InstanceId,
        set: &TextureSet,
        moves: &mut Vec<Move>,
        properties: &mut Vec<PropertyEdit>,
    ) -> bool {
        let before = history::snapshot(&self.world, id);
        let _ = self
            .world
            .set_property(id, "Material", Value::Material(Material::Plastic));
        if set.path(MapSlot::BaseColor).is_some() {
            let _ = self
                .world
                .set_property(id, "Color", Value::Color3(Color3::new(1.0, 1.0, 1.0)));
        }
        properties.extend(property_changes(
            id,
            before,
            history::snapshot(&self.world, id),
        ));
        self.fill_appearances(id, set, Some(&[]), moves, properties);
        true
    }

    fn apply_mesh(
        &mut self,
        id: InstanceId,
        set: &TextureSet,
        moves: &mut Vec<Move>,
        properties: &mut Vec<PropertyEdit>,
    ) -> bool {
        let names = if appearances(&self.world, id).is_empty() {
            let mesh_id = self
                .world
                .get(id)
                .and_then(|instance| instance.property("MeshId"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            self.material_names(&mesh_id)
        } else {
            Some(Vec::new())
        };
        self.fill_appearances(id, set, names.as_deref(), moves, properties)
    }

    /// `materials` is the model's material names, used only when the part
    /// has no SurfaceAppearance yet. `None` means the model could not be
    /// read. A Part passes an empty list, which creates one appearance.
    fn fill_appearances(
        &mut self,
        id: InstanceId,
        set: &TextureSet,
        materials: Option<&[String]>,
        moves: &mut Vec<Move>,
        properties: &mut Vec<PropertyEdit>,
    ) -> bool {
        let existing = appearances(&self.world, id);
        if !existing.is_empty() {
            for appearance in existing {
                self.fill_existing(appearance, set, false, properties);
            }
            return true;
        }
        let Some(materials) = materials else {
            let name = self
                .world
                .get(id)
                .map(|instance| instance.name().to_owned())
                .unwrap_or_else(|| "The MeshPart".to_owned());
            self.output.warning(format!(
                "{name}'s model could not be read, so no SurfaceAppearance was added."
            ));
            return false;
        };
        let materials = if materials.is_empty() {
            vec![String::new()]
        } else {
            materials.to_vec()
        };
        let mut created = false;
        for material in &materials {
            if let Some(step) = self.make_appearance(id, material, set) {
                moves.push(step);
                created = true;
            }
        }
        created
    }

    fn apply_terrain(
        &mut self,
        id: InstanceId,
        set: &TextureSet,
        properties: &mut Vec<PropertyEdit>,
    ) -> bool {
        let terrain_name = self
            .world
            .get(id)
            .map(|instance| instance.name().to_owned())
            .unwrap_or_else(|| "The terrain".to_owned());
        let Some(layer) = top_layer(&self.world, id) else {
            self.output
                .warning(format!("{terrain_name} has no terrain layer to fill."));
            return false;
        };
        let layer_name = self
            .world
            .get(layer)
            .map(|instance| instance.name().to_owned())
            .unwrap_or_else(|| "the top layer".to_owned());
        self.fill_existing(layer, set, true, properties);
        self.output
            .info(format!("Put {terrain_name}'s texture on {layer_name}."));
        true
    }

    fn fill_existing(
        &mut self,
        id: InstanceId,
        set: &TextureSet,
        terrain: bool,
        properties: &mut Vec<PropertyEdit>,
    ) {
        let before = history::snapshot(&self.world, id);
        self.write_maps(id, set, terrain);
        properties.extend(property_changes(
            id,
            before,
            history::snapshot(&self.world, id),
        ));
    }

    fn make_appearance(
        &mut self,
        parent: InstanceId,
        material_name: &str,
        set: &TextureSet,
    ) -> Option<Move> {
        let id = self.world.create(ClassName::SurfaceAppearance).ok()?;
        if !material_name.is_empty() {
            let _ = self.world.set_property(
                id,
                "MaterialName",
                Value::String(material_name.to_owned()),
            );
        }
        self.write_maps(id, set, false);
        let name = crate::tree::unique_name(&self.world, parent, "SurfaceAppearance");
        let _ = self.world.set_name(id, name);
        let index = self
            .world
            .get(parent)
            .map(|parent| parent.children().len())
            .unwrap_or(0);
        let place = Place { parent, index };
        history::put(&mut self.world, id, Some(place)).ok()?;
        Some(Move {
            id,
            before: None,
            after: Some(place),
        })
    }

    fn write_maps(&mut self, id: InstanceId, set: &TextureSet, terrain: bool) {
        let bindings = if terrain { LAYER_MAPS } else { SURFACE_MAPS };
        for &(property, slot) in bindings {
            let path = set.path(slot).unwrap_or("");
            let _ = self
                .world
                .set_property(id, property, Value::String(path.to_owned()));
            self.write_map_settings(id, set, slot, terrain);
        }
        if set.path(MapSlot::BaseColor).is_some() {
            let _ = self
                .world
                .set_property(id, "Color", Value::Color3(Color3::new(1.0, 1.0, 1.0)));
        }
        if terrain && let Some(layer) = self.world.get(id) {
            for (property, value) in auto_layer_settings(layer) {
                let _ = self.world.set_property(id, property, value);
            }
        }
    }

    fn write_map_settings(
        &mut self,
        id: InstanceId,
        set: &TextureSet,
        slot: MapSlot,
        terrain: bool,
    ) {
        let map = set.maps.iter().find(|map| map.slot == slot);
        let channel = map.map_or(default_channel(slot), |map| texture_channel(map.channel));
        match slot {
            MapSlot::Normal => {
                let _ = self.world.set_property(
                    id,
                    "NormalMapDirectX",
                    Value::Bool(map.is_some_and(|map| map.flip_green)),
                );
                if map.is_some() {
                    let _ = self
                        .world
                        .set_property(id, "NormalStrength", Value::Number(1.0));
                }
            }
            MapSlot::Roughness => {
                let _ =
                    self.world
                        .set_property(id, "RoughnessChannel", Value::TextureChannel(channel));
                let _ = self.world.set_property(
                    id,
                    "InvertRoughness",
                    Value::Bool(map.is_some_and(|map| map.invert)),
                );
            }
            MapSlot::Metalness => {
                let _ =
                    self.world
                        .set_property(id, "MetalnessChannel", Value::TextureChannel(channel));
            }
            MapSlot::Occlusion => {
                let _ = self.world.set_property(
                    id,
                    "AmbientOcclusionChannel",
                    Value::TextureChannel(channel),
                );
                if map.is_some() && !terrain {
                    let _ =
                        self.world
                            .set_property(id, "AmbientOcclusionStrength", Value::Number(1.0));
                }
            }
            MapSlot::Opacity if !terrain => {
                let _ =
                    self.world
                        .set_property(id, "OpacityChannel", Value::TextureChannel(channel));
                let _ = self.world.set_property(
                    id,
                    "InvertOpacity",
                    Value::Bool(map.is_some_and(|map| map.invert)),
                );
            }
            MapSlot::Emissive if !terrain && map.is_some() => {
                let _ = self
                    .world
                    .set_property(id, "EmissiveStrength", Value::Number(1.0));
            }
            _ => {}
        }
    }

    fn material_names(&mut self, mesh_id: &str) -> Option<Vec<String>> {
        if mesh_id.is_empty() {
            return None;
        }
        let model = self.assets.model(mesh_id)?;
        let mut names = Vec::new();
        for primitive in &model.primitives {
            if !names.contains(&primitive.material.name) {
                names.push(primitive.material.name.clone());
            }
        }
        Some(names)
    }
}

/// The settings that suit a TerrainLayer's maps, set when a texture set
/// fills it and by the Terrain Studio's Auto button. With a roughness or
/// metalness map the value scales the map, so 1 shows the map as it is;
/// without one it's the value itself (rough, not metal: ground). A normal
/// map shows at full strength.
pub(super) fn auto_layer_settings(layer: &Instance) -> [(&'static str, Value); 3] {
    let has = |name| {
        layer
            .property(name)
            .and_then(Value::as_str)
            .is_some_and(|path| !path.is_empty())
    };
    let pick =
        |name, with: f64, without: f64| Value::Number(if has(name) { with } else { without });
    [
        ("Roughness", pick("RoughnessMap", 1.0, 0.9)),
        ("Metalness", pick("MetalnessMap", 1.0, 0.0)),
        ("NormalStrength", Value::Number(1.0)),
    ]
}

fn appearances(world: &World, parent: InstanceId) -> Vec<InstanceId> {
    world
        .get(parent)
        .map(|instance| {
            instance
                .children()
                .iter()
                .copied()
                .filter(|&id| {
                    world
                        .get(id)
                        .is_some_and(|child| child.class() == ClassName::SurfaceAppearance)
                })
                .collect()
        })
        .unwrap_or_default()
}

fn top_layer(world: &World, terrain: InstanceId) -> Option<InstanceId> {
    world
        .get(terrain)?
        .children()
        .iter()
        .rev()
        .copied()
        .find(|&id| {
            world
                .get(id)
                .is_some_and(|child| child.class() == ClassName::TerrainLayer)
        })
}

pub(super) fn property_changes(
    id: InstanceId,
    before: Snapshot,
    after: Snapshot,
) -> Vec<PropertyEdit> {
    after
        .into_iter()
        .filter_map(|(field, after_value)| {
            let before_value = before
                .iter()
                .find(|(known, _)| *known == field)
                .map(|(_, value)| value.clone())?;
            (before_value != after_value).then_some(PropertyEdit {
                id,
                field,
                before: before_value,
                after: after_value,
            })
        })
        .collect()
}

fn unique(ids: Vec<InstanceId>) -> Vec<InstanceId> {
    let mut unique = Vec::new();
    for id in ids {
        if !unique.contains(&id) {
            unique.push(id);
        }
    }
    unique
}

fn texture_channel(channel: Channel) -> TextureChannel {
    match channel {
        Channel::Red => TextureChannel::Red,
        Channel::Green => TextureChannel::Green,
        Channel::Blue => TextureChannel::Blue,
        Channel::Alpha => TextureChannel::Alpha,
    }
}

fn default_channel(slot: MapSlot) -> TextureChannel {
    if slot == MapSlot::Opacity {
        TextureChannel::Alpha
    } else {
        TextureChannel::Red
    }
}
