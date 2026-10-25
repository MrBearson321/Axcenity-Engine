//! A model material's texture maps as files, the way a SurfaceAppearance
//! holds them: what Import Model finds in a model, shows in its dialog for
//! changing, and writes into a SurfaceAppearance per material.

use std::path::PathBuf;

use axcenity_assets::{Channel, ChannelTexture, Image, ModelMaterial};
use axcenity_core::{Color3, TextureChannel, Value};

/// Where a map's file is.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum MapFile {
    /// One the model uses, relative to the model's folder (`textures/wood.png`).
    Model(String),
    /// One picked in the import dialog, anywhere; it's copied into the
    /// project on import.
    Picked(PathBuf),
}

impl MapFile {
    /// Its file name, for showing.
    pub fn file_name(&self) -> String {
        match self {
            MapFile::Model(path) => path.rsplit('/').next().unwrap_or(path).to_owned(),
            MapFile::Picked(path) => path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
        }
    }
}

/// A SurfaceAppearance's maps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    Color,
    Normal,
    Metalness,
    Roughness,
    Emissive,
    AmbientOcclusion,
    Opacity,
    Specular,
}

impl Slot {
    pub const ALL: [Slot; 8] = [
        Slot::Color,
        Slot::Normal,
        Slot::Metalness,
        Slot::Roughness,
        Slot::Emissive,
        Slot::AmbientOcclusion,
        Slot::Opacity,
        Slot::Specular,
    ];

    /// Its SurfaceAppearance property.
    pub fn property(self) -> &'static str {
        match self {
            Slot::Color => "ColorMap",
            Slot::Normal => "NormalMap",
            Slot::Metalness => "MetalnessMap",
            Slot::Roughness => "RoughnessMap",
            Slot::Emissive => "EmissiveMap",
            Slot::AmbientOcclusion => "AmbientOcclusionMap",
            Slot::Opacity => "OpacityMap",
            Slot::Specular => "SpecularMap",
        }
    }

    /// What the dialog calls it.
    pub fn label(self) -> &'static str {
        match self {
            Slot::Color => "Color",
            Slot::Normal => "Normal",
            Slot::Metalness => "Metalness",
            Slot::Roughness => "Roughness",
            Slot::Emissive => "Emission",
            Slot::AmbientOcclusion => "Occlusion",
            Slot::Opacity => "Opacity",
            Slot::Specular => "Specular",
        }
    }
}

/// One material's maps and how they're read.
#[derive(Clone, Debug, PartialEq)]
pub struct MaterialMaps {
    /// The material's name in the model.
    pub material: String,
    /// One per [`Slot`], in [`Slot::ALL`] order.
    pub files: [Option<MapFile>; 8],
    pub normal_directx: bool,
    pub metalness_channel: Channel,
    pub roughness_channel: Channel,
    pub invert_roughness: bool,
    pub occlusion_channel: Channel,
    pub opacity_channel: Channel,
    pub invert_opacity: bool,
    pub normal_strength: f32,
    pub occlusion_strength: f32,
}

impl MaterialMaps {
    /// The maps `material` has, with the files they came from.
    pub fn of(material: &ModelMaterial) -> Self {
        let file = |image: &Option<std::sync::Arc<Image>>| {
            image
                .as_ref()
                .and_then(|image| image.file.clone())
                .map(MapFile::Model)
        };
        let map_file = |map: &Option<ChannelTexture>| {
            map.as_ref()
                .and_then(|map| map.image.file.clone())
                .map(MapFile::Model)
        };
        let channel =
            |map: &Option<ChannelTexture>, default| map.as_ref().map_or(default, |map| map.channel);
        let inverted = |map: &Option<ChannelTexture>| map.as_ref().is_some_and(|map| map.invert);
        Self {
            material: material.name.clone(),
            files: [
                file(&material.base_color_texture),
                file(&material.normal_texture),
                map_file(&material.metalness_texture),
                map_file(&material.roughness_texture),
                file(&material.emissive_texture),
                map_file(&material.occlusion_texture),
                map_file(&material.opacity_texture),
                file(&material.specular_texture),
            ],
            normal_directx: material.normal_flip_green,
            metalness_channel: channel(&material.metalness_texture, Channel::Red),
            roughness_channel: channel(&material.roughness_texture, Channel::Red),
            invert_roughness: inverted(&material.roughness_texture),
            occlusion_channel: channel(&material.occlusion_texture, Channel::Red),
            opacity_channel: channel(&material.opacity_texture, Channel::Alpha),
            invert_opacity: inverted(&material.opacity_texture),
            normal_strength: material.normal_scale,
            occlusion_strength: material.occlusion_strength,
        }
    }

    /// The file of `slot`'s map, if it has one.
    pub fn file(&self, slot: Slot) -> Option<&MapFile> {
        self.files[slot as usize].as_ref()
    }

    pub fn set_file(&mut self, slot: Slot, file: Option<MapFile>) {
        self.files[slot as usize] = file;
    }

    /// The channel `slot`'s map is read from, for the grey maps.
    pub fn channel_mut(&mut self, slot: Slot) -> Option<&mut Channel> {
        match slot {
            Slot::Metalness => Some(&mut self.metalness_channel),
            Slot::Roughness => Some(&mut self.roughness_channel),
            Slot::AmbientOcclusion => Some(&mut self.occlusion_channel),
            Slot::Opacity => Some(&mut self.opacity_channel),
            _ => None,
        }
    }

    /// The properties of a SurfaceAppearance for these maps, with each
    /// map's file as `project_path` turns it into a path in the project
    /// (`None` leaves the map empty).
    pub fn properties(
        &self,
        mut project_path: impl FnMut(&MapFile) -> Option<String>,
    ) -> Vec<(&'static str, Value)> {
        let channel = |channel: Channel| {
            Value::TextureChannel(match channel {
                Channel::Red => TextureChannel::Red,
                Channel::Green => TextureChannel::Green,
                Channel::Blue => TextureChannel::Blue,
                Channel::Alpha => TextureChannel::Alpha,
            })
        };
        let mut properties = vec![
            ("MaterialName", Value::String(self.material.clone())),
            ("Color", Value::Color3(Color3::new(1.0, 1.0, 1.0))),
        ];
        for slot in Slot::ALL {
            let path = self.file(slot).and_then(&mut project_path);
            properties.push((slot.property(), Value::String(path.unwrap_or_default())));
        }
        properties.extend([
            (
                "NormalStrength",
                Value::Number(f64::from(self.normal_strength)),
            ),
            ("NormalMapDirectX", Value::Bool(self.normal_directx)),
            ("MetalnessChannel", channel(self.metalness_channel)),
            ("RoughnessChannel", channel(self.roughness_channel)),
            ("InvertRoughness", Value::Bool(self.invert_roughness)),
            ("AmbientOcclusionChannel", channel(self.occlusion_channel)),
            (
                "AmbientOcclusionStrength",
                Value::Number(f64::from(self.occlusion_strength)),
            ),
            ("OpacityChannel", channel(self.opacity_channel)),
            ("InvertOpacity", Value::Bool(self.invert_opacity)),
        ]);
        properties
    }
}

/// Each distinct material of `materials` (by name, first one kept), as maps.
pub fn materials_maps<'a>(
    materials: impl IntoIterator<Item = &'a ModelMaterial>,
) -> Vec<MaterialMaps> {
    let mut maps: Vec<MaterialMaps> = Vec::new();
    for material in materials {
        if !maps.iter().any(|known| known.material == material.name) {
            maps.push(MaterialMaps::of(material));
        }
    }
    maps
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_materials_maps_become_surface_appearance_properties() {
        let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/hello-mesh/assets/models");
        let bytes = std::fs::read(folder.join("pyramid.gltf")).unwrap();
        let model =
            axcenity_assets::load_model(&bytes, &folder, axcenity_assets::ModelFormat::Gltf)
                .unwrap();
        let mut maps = MaterialMaps::of(&model.primitives[0].material);
        assert_eq!(maps.material, "Sandstone");
        let Some(MapFile::Model(color)) = maps.file(Slot::Color).cloned() else {
            panic!("the pyramid has a color texture");
        };
        assert!(color.starts_with("textures/"), "{color}");
        maps.set_file(
            Slot::Roughness,
            Some(MapFile::Picked("C:/maps/rough.png".into())),
        );
        *maps.channel_mut(Slot::Roughness).unwrap() = Channel::Green;

        let properties = maps.properties(|file| match file {
            MapFile::Model(path) => Some(format!("assets/models/pyramid/{path}")),
            MapFile::Picked(_) => Some("assets/textures/rough.png".to_owned()),
        });
        let get = |name| {
            properties
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(_, v)| v.clone())
        };
        assert_eq!(
            get("ColorMap"),
            Some(Value::String(format!("assets/models/pyramid/{color}")))
        );
        assert_eq!(
            get("RoughnessMap"),
            Some(Value::String("assets/textures/rough.png".into()))
        );
        assert_eq!(
            get("RoughnessChannel"),
            Some(Value::TextureChannel(TextureChannel::Green))
        );
        assert_eq!(get("NormalMap"), Some(Value::String(String::new())));
        // Every one is a real SurfaceAppearance property, of the right type.
        let defaults = axcenity_core::ClassName::SurfaceAppearance.default_properties();
        for (name, value) in &properties {
            let default = defaults
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, v)| v.kind());
            assert_eq!(default, Some(value.kind()), "{name}");
        }
    }
}
