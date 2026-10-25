//! A texture set: one material's maps, saved together so they can be
//! applied as a group (`assets/textures/Fabric086/Fabric086.texture.json`).
//!
//! The images sit beside the file. Which map each image is comes from its
//! name, the same way as a model's textures ([`texture_roles`](crate::texture_roles)).

use std::fs;
use std::io;
use std::path::Path;

use serde_json::json;

use crate::model::{Channel, MapSlot, TextureRole, texture_roles};

/// The end of a texture set's file name (`Fabric086.texture.json`).
pub const TEXTURE_SET_SUFFIX: &str = ".texture.json";

/// One map in a [`TextureSet`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextureSetMap {
    pub slot: MapSlot,
    /// The image, relative to the project folder (`assets/textures/…`).
    pub path: String,
    pub channel: Channel,
    pub invert: bool,
    /// A DirectX-style normal map, whose green points down.
    pub flip_green: bool,
}

/// The maps of one material, and where it was downloaded from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextureSet {
    /// Who published it, such as `ambientcg`. Empty when it was assembled
    /// from files already in the project.
    pub source: String,
    /// The publisher's id, such as `Fabric086`.
    pub asset_id: String,
    /// A page about the asset. Empty when there isn't one.
    pub link: String,
    pub maps: Vec<TextureSetMap>,
}

impl TextureSet {
    /// The image for `slot`, if this set has one.
    pub fn path(&self, slot: MapSlot) -> Option<&str> {
        self.maps
            .iter()
            .find(|map| map.slot == slot)
            .map(|map| map.path.as_str())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TextureSetError {
    #[error("could not read it")]
    Read(#[source] io::Error),
    #[error("could not write it")]
    Write(#[source] io::Error),
    #[error("it isn't a texture set")]
    Format(#[source] serde_json::Error),
    #[error("it has no asset id")]
    NoAsset,
}

/// The maps named by `files` (`file name`, project path). The same slot
/// from two files keeps the first, except a DirectX normal map, which an
/// OpenGL one replaces. Files are taken in name order, so the result
/// doesn't depend on the order they were passed.
pub fn texture_set_from_files(
    source: &str,
    asset_id: &str,
    link: &str,
    files: &[(&str, &str)],
) -> TextureSet {
    let mut files: Vec<(&str, &str)> = files.to_vec();
    files.sort_by(|a, b| a.0.cmp(b.0));
    let mut maps = Vec::new();
    for (name, path) in files {
        if path.is_empty() {
            continue;
        }
        for role in texture_roles(name) {
            place(&mut maps, path, role);
        }
    }
    TextureSet {
        source: source.to_owned(),
        asset_id: asset_id.to_owned(),
        link: link.to_owned(),
        maps,
    }
}

/// Puts `role` of the image at `path` into `maps`.
fn place(maps: &mut Vec<TextureSetMap>, path: &str, role: TextureRole) {
    let map = TextureSetMap {
        slot: role.slot,
        path: path.to_owned(),
        channel: role.channel,
        invert: role.invert,
        flip_green: role.flip_green,
    };
    match maps.iter().position(|existing| existing.slot == role.slot) {
        Some(index) => {
            // AmbientCG ships both normal conventions. The engine's default
            // is OpenGL, so that file wins.
            let replace =
                role.slot == MapSlot::Normal && maps[index].flip_green && !role.flip_green;
            if replace {
                maps[index] = map;
            }
        }
        None => maps.push(map),
    }
}

/// Reads the texture set at `path`.
pub fn read_texture_set(path: &Path) -> Result<TextureSet, TextureSetError> {
    let text = fs::read_to_string(path).map_err(TextureSetError::Read)?;
    let value: serde_json::Value = serde_json::from_str(&text).map_err(TextureSetError::Format)?;
    let asset_id = value
        .get("assetId")
        .and_then(|id| id.as_str())
        .filter(|id| !id.is_empty())
        .ok_or(TextureSetError::NoAsset)?
        .to_owned();
    let source = value
        .get("source")
        .and_then(|source| source.as_str())
        .unwrap_or("")
        .to_owned();
    let link = value
        .get("link")
        .and_then(|link| link.as_str())
        .unwrap_or("")
        .to_owned();
    let mut maps = Vec::new();
    if let Some(list) = value.get("maps").and_then(|maps| maps.as_array()) {
        for entry in list {
            let Some(slot) = entry
                .get("slot")
                .and_then(|slot| slot.as_str())
                .and_then(slot_from_name)
            else {
                continue;
            };
            let Some(path) = entry
                .get("path")
                .and_then(|path| path.as_str())
                .filter(|path| !path.is_empty())
            else {
                continue;
            };
            maps.push(TextureSetMap {
                slot,
                path: path.to_owned(),
                channel: entry
                    .get("channel")
                    .and_then(|channel| channel.as_str())
                    .and_then(channel_from_name)
                    .unwrap_or(Channel::Red),
                invert: entry.get("invert").and_then(|flag| flag.as_bool()) == Some(true),
                flip_green: entry.get("flipGreen").and_then(|flag| flag.as_bool()) == Some(true),
            });
        }
    }
    Ok(TextureSet {
        source,
        asset_id,
        link,
        maps,
    })
}

/// Writes `set` to `path`, creating the file's folder if it isn't there.
pub fn write_texture_set(path: &Path, set: &TextureSet) -> Result<(), TextureSetError> {
    if let Some(folder) = path.parent()
        && !folder.as_os_str().is_empty()
    {
        fs::create_dir_all(folder).map_err(TextureSetError::Write)?;
    }
    let maps: Vec<serde_json::Value> = set
        .maps
        .iter()
        .map(|map| {
            json!({
                "slot": slot_name(map.slot),
                "path": map.path,
                "channel": channel_name(map.channel),
                "invert": map.invert,
                "flipGreen": map.flip_green,
            })
        })
        .collect();
    let text = serde_json::to_string_pretty(&json!({
        "source": set.source,
        "assetId": set.asset_id,
        "link": set.link,
        "maps": maps,
    }))
    .map_err(TextureSetError::Format)?;
    fs::write(path, format!("{text}\n")).map_err(TextureSetError::Write)
}

const SLOTS: [(MapSlot, &str); 9] = [
    (MapSlot::BaseColor, "Color"),
    (MapSlot::Normal, "Normal"),
    (MapSlot::Metalness, "Metalness"),
    (MapSlot::Roughness, "Roughness"),
    (MapSlot::Occlusion, "Occlusion"),
    (MapSlot::Height, "Height"),
    (MapSlot::Emissive, "Emissive"),
    (MapSlot::Opacity, "Opacity"),
    (MapSlot::Specular, "Specular"),
];

fn slot_name(slot: MapSlot) -> &'static str {
    SLOTS
        .iter()
        .find(|(known, _)| *known == slot)
        .map(|(_, name)| *name)
        .unwrap_or("Color")
}

fn slot_from_name(name: &str) -> Option<MapSlot> {
    SLOTS
        .iter()
        .find(|(_, known)| *known == name)
        .map(|(slot, _)| *slot)
}

fn channel_name(channel: Channel) -> &'static str {
    match channel {
        Channel::Red => "Red",
        Channel::Green => "Green",
        Channel::Blue => "Blue",
        Channel::Alpha => "Alpha",
    }
}

fn channel_from_name(name: &str) -> Option<Channel> {
    Some(match name {
        "Red" => Channel::Red,
        "Green" => Channel::Green,
        "Blue" => Channel::Blue,
        "Alpha" => Channel::Alpha,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ambientcg_names_fill_every_map_and_prefer_opengl_normals() {
        let set = texture_set_from_files(
            "ambientcg",
            "Fabric086",
            "https://ambientcg.com/a/Fabric086",
            &[
                (
                    "Fabric086_2K-JPG_NormalDX.jpg",
                    "assets/textures/Fabric086/Fabric086_2K-JPG_NormalDX.jpg",
                ),
                (
                    "Fabric086_2K-JPG_Color.jpg",
                    "assets/textures/Fabric086/Fabric086_2K-JPG_Color.jpg",
                ),
                (
                    "Fabric086_2K-JPG_NormalGL.jpg",
                    "assets/textures/Fabric086/Fabric086_2K-JPG_NormalGL.jpg",
                ),
                (
                    "Fabric086_2K-JPG_Roughness.jpg",
                    "assets/textures/Fabric086/Fabric086_2K-JPG_Roughness.jpg",
                ),
                (
                    "Fabric086_2K-JPG_AmbientOcclusion.jpg",
                    "assets/textures/Fabric086/Fabric086_2K-JPG_AmbientOcclusion.jpg",
                ),
                (
                    "Fabric086_2K-JPG_Displacement.jpg",
                    "assets/textures/Fabric086/Fabric086_2K-JPG_Displacement.jpg",
                ),
            ],
        );
        assert_eq!(set.asset_id, "Fabric086");
        assert!(
            set.path(MapSlot::BaseColor)
                .unwrap()
                .ends_with("_Color.jpg")
        );
        let normal = set
            .maps
            .iter()
            .find(|map| map.slot == MapSlot::Normal)
            .unwrap();
        assert!(normal.path.ends_with("_NormalGL.jpg"));
        assert!(!normal.flip_green);
        assert!(set.path(MapSlot::Roughness).unwrap().contains("Roughness"));
        assert!(
            set.path(MapSlot::Occlusion)
                .unwrap()
                .contains("AmbientOcclusion")
        );
        assert_eq!(
            set.maps
                .iter()
                .find(|map| map.slot == MapSlot::Height)
                .unwrap()
                .slot,
            MapSlot::Height
        );
    }

    #[test]
    fn a_texture_set_roundtrips_through_json() {
        let set = texture_set_from_files(
            "ambientcg",
            "Rock035",
            "https://ambientcg.com/a/Rock035",
            &[(
                "Rock035_2K-JPG_Color.jpg",
                "assets/textures/Rock035/Rock035_2K-JPG_Color.jpg",
            )],
        );
        let folder =
            std::env::temp_dir().join(format!("axcenity-texture-set-{}", std::process::id()));
        let _ = fs::remove_dir_all(&folder);
        let path = folder.join("Rock035.texture.json");
        write_texture_set(&path, &set).unwrap();
        let read = read_texture_set(&path).unwrap();
        assert_eq!(read, set);
        let _ = fs::remove_dir_all(&folder);
    }
}
