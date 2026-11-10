//! The files the first drawing of a world reads: MeshParts' models, the
//! maps of SurfaceAppearances and TerrainLayers, and parts' material files. A host can load them ahead
//! of time (such as on a loading screen) and say which one it's on.

use std::collections::HashSet;

use axcenity_assets::{Assets, LoadTicket};
use axcenity_core::{ClassName, FoliageSource, TEXTURE_MAP_PROPERTIES, Value, World};

/// A file drawing the Workspace reads.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum AssetFile {
    /// A MeshPart's `MeshId`.
    Model(String),
    /// A map's texture file.
    Texture(String),
    /// A TerrainLayer's map: the renderer reads its pixels to build the
    /// terrain's texture arrays, so they're decoded ahead too (a cooked
    /// texture otherwise decodes its file on the first frame that draws it).
    LayerTexture(String),
    /// A part's `MaterialVariant`: the file, its bases and their maps.
    Material(String),
}

impl AssetFile {
    pub fn path(&self) -> &str {
        match self {
            Self::Model(path)
            | Self::Texture(path)
            | Self::LayerTexture(path)
            | Self::Material(path) => path,
        }
    }

    /// Starts reading it into `assets` on the asset workers and returns at
    /// once: the ticket says when it's in. A material's ticket waits for
    /// its textures too.
    pub fn request(&self, assets: &Assets) -> LoadTicket {
        match self {
            Self::Model(mesh_id) => assets.request_model(mesh_id),
            Self::Texture(path) => assets.request_texture(path),
            Self::LayerTexture(path) => assets.request_texture_pixels(path),
            Self::Material(path) => assets.request_material(path),
        }
    }

    /// Reads it into `assets`, which keeps it (or why it couldn't be read)
    /// for drawing, so drawing doesn't read it again.
    pub fn load(&self, assets: &Assets) {
        match self {
            Self::Model(mesh_id) => {
                assets.model_blocking(mesh_id);
            }
            Self::Texture(path) => {
                assets.texture(path);
            }
            Self::LayerTexture(path) => {
                assets.texture_pixels_blocking(path);
            }
            Self::Material(path) => {
                if let Some(material) = assets.material(path) {
                    for map in material.image_paths() {
                        assets.texture(map);
                    }
                }
            }
        }
    }
}

/// Every file drawing `world`'s Workspace reads, each once, in tree order.
pub fn asset_files(world: &World) -> Vec<AssetFile> {
    let mut seen = HashSet::new();
    let mut files = Vec::new();
    let mut add = |file: AssetFile| {
        if !file.path().is_empty() && seen.insert(file.clone()) {
            files.push(file);
        }
    };
    for id in world.descendants_iter(world.workspace()) {
        let Some(instance) = world.get(id) else {
            continue;
        };
        let text = |name| {
            instance
                .property(name)
                .and_then(Value::as_str)
                .map(str::to_owned)
        };
        match instance.class() {
            class if class.has_model() => {
                if let Some(mesh_id) = text("MeshId") {
                    add(AssetFile::Model(mesh_id));
                }
                if let Some(path) = text("MaterialVariant") {
                    add(AssetFile::Material(path));
                }
            }
            ClassName::Part
            | ClassName::NegateOperation
            | ClassName::Seat
            | ClassName::VehicleSeat
            | ClassName::DamageZone
            | ClassName::ReverbZone => {
                if let Some(path) = text("MaterialVariant") {
                    add(AssetFile::Material(path));
                }
            }
            ClassName::SurfaceAppearance => {
                for map in TEXTURE_MAP_PROPERTIES {
                    if let Some(path) = text(map) {
                        add(AssetFile::Texture(path));
                    }
                }
            }
            ClassName::TerrainLayer => {
                for map in TEXTURE_MAP_PROPERTIES {
                    if let Some(path) = text(map) {
                        add(AssetFile::LayerTexture(path));
                    }
                }
            }
            ClassName::Terrain => {
                if let Some(data) = world.terrain(id) {
                    for instance in data.foliage() {
                        if let FoliageSource::Model(path) = &instance.source {
                            add(AssetFile::Model(path.clone()));
                        }
                    }
                }
            }
            _ => {}
        }
    }
    files
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_models_and_maps_once_each() {
        let mut world = World::new();
        let workspace = world.workspace();
        for _ in 0..2 {
            let part = world.create(ClassName::MeshPart).unwrap();
            world
                .set_property(part, "MeshId", Value::String("models/chair.glb".into()))
                .unwrap();
            world.set_parent(part, Some(workspace)).unwrap();
            let look = world.create(ClassName::SurfaceAppearance).unwrap();
            world
                .set_property(look, "ColorMap", Value::String("wood.png".into()))
                .unwrap();
            world.set_parent(look, Some(part)).unwrap();
        }
        let block = world.create(ClassName::Part).unwrap();
        world
            .set_property(
                block,
                "MaterialVariant",
                Value::String("materials/brick.material.json".into()),
            )
            .unwrap();
        world.set_parent(block, Some(workspace)).unwrap();
        let layer = world.create(ClassName::TerrainLayer).unwrap();
        world
            .set_property(layer, "ColorMap", Value::String("moss.jpg".into()))
            .unwrap();
        world.set_parent(layer, Some(workspace)).unwrap();
        // Not drawn, so not read.
        let stored = world.create(ClassName::MeshPart).unwrap();
        world
            .set_property(stored, "MeshId", Value::String("models/kept.glb".into()))
            .unwrap();
        world
            .set_parent(stored, Some(world.replicated_storage()))
            .unwrap();

        assert_eq!(
            asset_files(&world),
            [
                AssetFile::Model("models/chair.glb".into()),
                AssetFile::Texture("wood.png".into()),
                AssetFile::Material("materials/brick.material.json".into()),
                // Its pixels are read for the terrain's arrays.
                AssetFile::LayerTexture("moss.jpg".into()),
            ]
        );
    }
}
