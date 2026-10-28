//! The files the first drawing of a world reads: MeshParts' models, the
//! maps of SurfaceAppearances and TerrainLayers, and parts' material files. A host can load them ahead
//! of time (such as on a loading screen) and say which one it's on.

use std::collections::HashSet;

use axcenity_assets::Assets;
use axcenity_core::{ClassName, FoliageSource, Value, World};

/// The properties that name texture files, on a SurfaceAppearance or a
/// TerrainLayer (each class has only some of them).
const MAPS: [&str; 9] = [
    "ColorMap",
    "NormalMap",
    "MetalnessMap",
    "RoughnessMap",
    "AmbientOcclusionMap",
    "EmissiveMap",
    "OpacityMap",
    "SpecularMap",
    "HeightMap",
];

/// A file drawing the Workspace reads.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum AssetFile {
    /// A MeshPart's `MeshId`.
    Model(String),
    /// A map's texture file.
    Texture(String),
    /// A part's `MaterialVariant`: the file, its bases and their maps.
    Material(String),
}

impl AssetFile {
    pub fn path(&self) -> &str {
        match self {
            Self::Model(path) | Self::Texture(path) | Self::Material(path) => path,
        }
    }

    /// Reads it into `assets`, which keeps it (or why it couldn't be read)
    /// for drawing, so drawing doesn't read it again.
    pub fn load(&self, assets: &Assets) {
        match self {
            Self::Model(mesh_id) => {
                assets.model(mesh_id);
            }
            Self::Texture(path) => {
                assets.texture(path);
            }
            Self::Material(path) => {
                if let Some(material) = assets.material(path) {
                    for map in material.maps.paths() {
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
    for id in world.descendants(world.workspace()) {
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
            ClassName::MeshPart => {
                if let Some(mesh_id) = text("MeshId") {
                    add(AssetFile::Model(mesh_id));
                }
                if let Some(path) = text("MaterialVariant") {
                    add(AssetFile::Material(path));
                }
            }
            ClassName::Part | ClassName::Seat | ClassName::VehicleSeat | ClassName::DamageZone => {
                if let Some(path) = text("MaterialVariant") {
                    add(AssetFile::Material(path));
                }
            }
            ClassName::SurfaceAppearance | ClassName::TerrainLayer => {
                for map in MAPS {
                    if let Some(path) = text(map) {
                        add(AssetFile::Texture(path));
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
            ]
        );
    }
}
