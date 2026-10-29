//! Material files: one standard material saved on its own
//! (`assets/materials/brick.material.json`), which a Part or MeshPart uses
//! through its `MaterialVariant`.
//!
//! A file can name a `base` material file (by its project path). Every
//! value it leaves out comes from the base, and the base's from its own
//! base, so an instance overrides a few values and keeps the rest in step
//! with its base. What no file in the chain sets is the
//! [default material](StandardMaterial::default). [`Assets::material`](crate::Assets::material)
//! reads and resolves them.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::model::Channel;

/// The end of a material file's name (`brick.material.json`).
pub const MATERIAL_SUFFIX: &str = ".material.json";

/// Where the editor saves new material files.
pub const MATERIALS_FOLDER: &str = "assets/materials";

/// Whether the file at `path` is a material file, by its name.
pub fn is_material_file(path: &str) -> bool {
    path.to_lowercase().ends_with(MATERIAL_SUFFIX)
}

/// A material file as written: each value is `None` when the file leaves
/// it to its base. Colors are sRGB, 0 to 1, like a Part's `Color`.
///
/// `#[serde(default)]` fills a value the JSON leaves out with `None`, and
/// `skip_serializing_if` leaves `None` values out when writing, so a file
/// says only what it sets.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MaterialFile {
    /// The material file this one builds on, such as
    /// `assets/materials/brick.material.json`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<[f32; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metalness: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roughness: Option<f32>,
    /// The glow's color, times `emissive_strength`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emissive_color: Option<[f32; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emissive_strength: Option<f32>,
    /// Below 1 is see-through.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opacity: Option<f32>,
    /// Above 0, pixels whose opacity is below it are cut out.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alpha_cutoff: Option<f32>,
    #[serde(skip_serializing_if = "MaterialMaps::is_empty")]
    pub maps: MaterialMaps,
    /// How many times as often the maps repeat, across and down.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tiling: Option<[f32; 2]>,
    /// Then how far they're shifted, in copies of the texture.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<[f32; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub double_sided: Option<bool>,
    /// How much what's drawn with it sways in the wind, by height: 0 (the
    /// default) keeps it still, 1 gives like grass (`docs/rendering/wind.md`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sway: Option<f32>,
    /// WGSL code with a `surface` function that works out each pixel's
    /// surface in place of the standard one (`docs/rendering/models.md`).
    /// `""` is none, so an instance can go back to the standard surface.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shader: Option<String>,
    /// Values the shader reads with `parameter(i)`, by name. The file that
    /// sets the shader names them (in name order, `i` from 0); an instance
    /// changes their values, merged name by name.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub parameters: BTreeMap<String, ParamValue>,
    /// The editor's material graph that this file's shader, maps and
    /// parameters were made from. Editor data: it isn't taken from a base,
    /// and the player reads only what it made.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub graph: Option<serde_json::Value>,
}

/// A material parameter's value: a number, or a color or vector (2 to 4
/// numbers; colors are sRGB, 0 to 1).
///
/// `#[serde(untagged)]` writes each as the plain JSON value (`4` or
/// `[1, 0.5, 0]`), and reads whichever one the JSON is.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ParamValue {
    Number(f32),
    Vector(Vec<f32>),
}

impl ParamValue {
    /// Its numbers as the shader reads them: a number in x, a vector's in
    /// order, the rest 0.
    pub fn as_vec4(&self) -> [f32; 4] {
        let mut out = [0.0; 4];
        match self {
            ParamValue::Number(number) => out[0] = *number,
            ParamValue::Vector(numbers) => {
                for (slot, number) in out.iter_mut().zip(numbers) {
                    *slot = *number;
                }
            }
        }
        out
    }
}

/// A material file's texture maps, each `None` when the file leaves it
/// to its base.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MaterialMaps {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<MapFile>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub normal: Option<MapFile>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metalness: Option<MapFile>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roughness: Option<MapFile>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub occlusion: Option<MapFile>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emissive: Option<MapFile>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opacity: Option<MapFile>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub specular: Option<MapFile>,
}

impl MaterialMaps {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// One map in a material file. Each value is `None` when the file leaves
/// it to its base.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MapFile {
    /// The image, relative to the project folder. `""` is no map, so an
    /// instance can take away a map its base has.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Which channel holds the value (metalness, roughness, occlusion and
    /// opacity maps).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel: Option<Channel>,
    /// Reads the value upside down: a glossiness map as roughness, or an
    /// opacity map that's white where see-through.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invert: Option<bool>,
    /// How strongly it shows (normal and occlusion maps).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strength: Option<f32>,
    /// A DirectX-style normal map, whose green points down.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direct_x: Option<bool>,
}

impl MapFile {
    /// A map of the image at `path`, with every other value left to the base.
    pub fn of(path: impl Into<String>) -> Self {
        Self {
            path: Some(path.into()),
            ..Self::default()
        }
    }

    /// `self`'s values, and `base`'s where `self` has none.
    fn over(&self, base: &MapFile) -> MapFile {
        MapFile {
            path: self.path.clone().or_else(|| base.path.clone()),
            channel: self.channel.or(base.channel),
            invert: self.invert.or(base.invert),
            strength: self.strength.or(base.strength),
            direct_x: self.direct_x.or(base.direct_x),
        }
    }
}

impl MaterialFile {
    /// `self`'s values, and `base`'s where `self` has none. The result
    /// names no base: it's the two put together.
    pub fn over(&self, base: &MaterialFile) -> MaterialFile {
        let map = |own: &Option<MapFile>, below: &Option<MapFile>| match (own, below) {
            (Some(own), Some(below)) => Some(own.over(below)),
            (own, below) => own.clone().or_else(|| below.clone()),
        };
        let (own, below) = (&self.maps, &base.maps);
        MaterialFile {
            base: None,
            color: self.color.or(base.color),
            metalness: self.metalness.or(base.metalness),
            roughness: self.roughness.or(base.roughness),
            emissive_color: self.emissive_color.or(base.emissive_color),
            emissive_strength: self.emissive_strength.or(base.emissive_strength),
            opacity: self.opacity.or(base.opacity),
            alpha_cutoff: self.alpha_cutoff.or(base.alpha_cutoff),
            maps: MaterialMaps {
                color: map(&own.color, &below.color),
                normal: map(&own.normal, &below.normal),
                metalness: map(&own.metalness, &below.metalness),
                roughness: map(&own.roughness, &below.roughness),
                occlusion: map(&own.occlusion, &below.occlusion),
                emissive: map(&own.emissive, &below.emissive),
                opacity: map(&own.opacity, &below.opacity),
                specular: map(&own.specular, &below.specular),
            },
            tiling: self.tiling.or(base.tiling),
            offset: self.offset.or(base.offset),
            double_sided: self.double_sided.or(base.double_sided),
            sway: self.sway.or(base.sway),
            shader: self.shader.clone().or_else(|| base.shader.clone()),
            parameters: base
                .parameters
                .iter()
                .chain(&self.parameters)
                .map(|(name, value)| (name.clone(), value.clone()))
                .collect(),
            graph: self.graph.clone(),
        }
    }

    /// Every value, with the default material's where this file has none
    /// (its `base` isn't followed: put the chain together with
    /// [`over`](Self::over) first). Its parameters are all the ones it has.
    pub fn resolve(&self) -> StandardMaterial {
        let default = StandardMaterial::default();
        let maps = &self.maps;
        let map = |map: &Option<MapFile>, channel: Channel| {
            let map = map.as_ref()?;
            let path = map.path.as_deref().filter(|path| !path.is_empty())?;
            Some(MapSettings {
                path: path.to_owned(),
                channel: map.channel.unwrap_or(channel),
                invert: map.invert.unwrap_or(false),
                strength: map.strength.unwrap_or(1.0),
                direct_x: map.direct_x.unwrap_or(false),
            })
        };
        StandardMaterial {
            color: self.color.unwrap_or(default.color),
            metalness: self.metalness.unwrap_or(default.metalness),
            roughness: self.roughness.unwrap_or(default.roughness),
            emissive_color: self.emissive_color.unwrap_or(default.emissive_color),
            emissive_strength: self.emissive_strength.unwrap_or(default.emissive_strength),
            opacity: self.opacity.unwrap_or(default.opacity),
            alpha_cutoff: self.alpha_cutoff.unwrap_or(default.alpha_cutoff),
            maps: StandardMaps {
                color: map(&maps.color, Channel::Red),
                normal: map(&maps.normal, Channel::Red),
                metalness: map(&maps.metalness, Channel::Red),
                roughness: map(&maps.roughness, Channel::Red),
                occlusion: map(&maps.occlusion, Channel::Red),
                emissive: map(&maps.emissive, Channel::Red),
                opacity: map(&maps.opacity, Channel::Alpha),
                specular: map(&maps.specular, Channel::Red),
            },
            tiling: self.tiling.unwrap_or(default.tiling),
            offset: self.offset.unwrap_or(default.offset),
            double_sided: self.double_sided.unwrap_or(default.double_sided),
            sway: self.sway.unwrap_or(default.sway),
            shader: self
                .shader
                .as_deref()
                .filter(|code| !code.is_empty())
                .map(|code| MaterialShader {
                    file: String::new(),
                    code: code.into(),
                }),
            parameters: self
                .parameters
                .iter()
                .map(|(name, value)| (name.clone(), value.as_vec4()))
                .collect(),
        }
    }
}

/// A material with every value filled in: a material file and its bases
/// put together, over the default material.
#[derive(Clone, Debug, PartialEq)]
pub struct StandardMaterial {
    /// sRGB, 0 to 1. A Part's `Color` multiplies it.
    pub color: [f32; 3],
    pub metalness: f32,
    pub roughness: f32,
    /// sRGB, 0 to 1, times `emissive_strength`.
    pub emissive_color: [f32; 3],
    pub emissive_strength: f32,
    pub opacity: f32,
    pub alpha_cutoff: f32,
    pub maps: StandardMaps,
    pub tiling: [f32; 2],
    pub offset: [f32; 2],
    pub double_sided: bool,
    /// How much it sways in the wind (0: not at all).
    pub sway: f32,
    /// Its surface shader, if it has one.
    pub shader: Option<MaterialShader>,
    /// The values its shader reads (`parameter(i)` is the `i`th), in name
    /// order: the parameters the file with the shader names, with the
    /// values the chain gives them.
    pub parameters: Vec<(String, [f32; 4])>,
}

/// A material's surface shader: WGSL code with a `surface` function.
#[derive(Clone, Debug, PartialEq)]
pub struct MaterialShader {
    /// The material file the code is in (the material's own, or the base
    /// it comes from), whose lines an error is reported on. Empty for a
    /// material not read through [`Assets::material`](crate::Assets::material).
    pub file: String,
    pub code: std::sync::Arc<str>,
}

impl Default for StandardMaterial {
    /// Plain white plastic, so a part with a broken material looks like a
    /// plastic part of its `Color`.
    fn default() -> Self {
        Self {
            color: [1.0; 3],
            metalness: 0.0,
            roughness: 0.5,
            emissive_color: [1.0; 3],
            emissive_strength: 0.0,
            opacity: 1.0,
            alpha_cutoff: 0.0,
            maps: StandardMaps::default(),
            tiling: [1.0, 1.0],
            offset: [0.0, 0.0],
            double_sided: false,
            sway: 0.0,
            shader: None,
            parameters: Vec::new(),
        }
    }
}

/// A [`StandardMaterial`]'s maps; `None` is no map.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StandardMaps {
    pub color: Option<MapSettings>,
    pub normal: Option<MapSettings>,
    pub metalness: Option<MapSettings>,
    pub roughness: Option<MapSettings>,
    pub occlusion: Option<MapSettings>,
    pub emissive: Option<MapSettings>,
    pub opacity: Option<MapSettings>,
    pub specular: Option<MapSettings>,
}

impl StandardMaps {
    /// The image of every map, in slot order.
    pub fn paths(&self) -> impl Iterator<Item = &str> {
        [
            &self.color,
            &self.normal,
            &self.metalness,
            &self.roughness,
            &self.occlusion,
            &self.emissive,
            &self.opacity,
            &self.specular,
        ]
        .into_iter()
        .flatten()
        .map(|map| map.path.as_str())
    }
}

/// One map of a [`StandardMaterial`].
#[derive(Clone, Debug, PartialEq)]
pub struct MapSettings {
    /// The image, relative to the project folder.
    pub path: String,
    pub channel: Channel,
    pub invert: bool,
    pub strength: f32,
    pub direct_x: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum MaterialFileError {
    #[error("could not read it")]
    Read(#[source] io::Error),
    #[error("could not write it")]
    Write(#[source] io::Error),
    #[error("it isn't a material file")]
    Format(#[source] serde_json::Error),
}

/// The material file in `text`.
pub fn parse_material(text: &str) -> Result<MaterialFile, MaterialFileError> {
    serde_json::from_str(text).map_err(MaterialFileError::Format)
}

/// `material` as a material file's text.
pub fn material_json(material: &MaterialFile) -> String {
    let text = serde_json::to_string_pretty(material)
        .expect("a material file is plain values, which always serialize");
    format!("{text}\n")
}

/// Reads the material file at `path`.
pub fn read_material_file(path: &Path) -> Result<MaterialFile, MaterialFileError> {
    let text = fs::read_to_string(path).map_err(MaterialFileError::Read)?;
    parse_material(&text)
}

/// Writes `material` to `path`, creating the file's folder if it isn't there.
pub fn write_material_file(path: &Path, material: &MaterialFile) -> Result<(), MaterialFileError> {
    if let Some(folder) = path.parent()
        && !folder.as_os_str().is_empty()
    {
        fs::create_dir_all(folder).map_err(MaterialFileError::Write)?;
    }
    fs::write(path, material_json(material)).map_err(MaterialFileError::Write)
}

/// A material file and its bases put together, as [`Assets`](crate::Assets)
/// keeps it.
#[derive(Clone)]
pub(crate) struct Resolved {
    pub material: std::sync::Arc<StandardMaterial>,
    /// The files it was made from, itself first, including a base that
    /// couldn't be read: changing any of them changes it.
    pub files: Vec<String>,
    /// A base was missing or made a cycle, so the default material stood in.
    pub broken: bool,
    /// The files' values put together, before the default material fills
    /// what none of them sets.
    pub merged: MaterialFile,
}

/// A material file's chain of bases, as the editor's Material tab shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct MaterialChain {
    /// The file, then its base, then that one's base, and so on, including
    /// a base that couldn't be read.
    pub files: Vec<String>,
    /// Their values put together: what the file and its bases set.
    pub merged: MaterialFile,
    /// A base was missing or made a cycle.
    pub broken: bool,
}

/// Reads the material file at `path` inside `folder` and its bases, or says
/// why it can't be read. A file in `edited` is taken from there rather than
/// read (the editor's Material tab hands its edits over). A base that can't
/// be read, or that leads back around, is reported in `errors` and the
/// default material stands in for it.
pub(crate) fn resolve(
    folder: &Path,
    path: &str,
    edited: &HashMap<String, MaterialFile>,
    errors: &mut Vec<String>,
) -> Option<Resolved> {
    let read = |path: &str| match edited.get(path) {
        Some(file) => Ok(file.clone()),
        None => read_project_material(folder, path),
    };
    let top = match read(path) {
        Ok(file) => file,
        Err(message) => {
            errors.push(format!("Could not load the material `{path}`: {message}"));
            return None;
        }
    };
    let mut files = vec![path.to_owned()];
    let mut chain = vec![top];
    let mut broken = false;
    while let Some(base) = chain
        .last()
        .and_then(|file| file.base.clone())
        .filter(|base| !base.is_empty())
    {
        if files.contains(&base) {
            errors.push(format!(
                "The material `{path}` has bases that go around in a cycle ({} → {base}), so \
                 the default material stands in for its base.",
                files.join(" → ")
            ));
            broken = true;
            break;
        }
        match read(&base) {
            Ok(file) => {
                files.push(base);
                chain.push(file);
            }
            Err(message) => {
                errors.push(format!(
                    "Could not load `{base}`, the base of the material `{path}`: {message}. The \
                     default material stands in for it."
                ));
                files.push(base);
                broken = true;
                break;
            }
        }
    }
    let merged = chain
        .iter()
        .rev()
        .fold(MaterialFile::default(), |below, file| file.over(&below));
    let mut material = merged.resolve();
    if let Some(shader) = &mut material.shader
        && let Some((file, declaring)) = files
            .iter()
            .zip(&chain)
            .find(|(_, file)| file.shader.is_some())
    {
        shader.file = file.clone();
        // Only the parameters its shader's file names: an instance's value
        // for one it doesn't is left out, so the others keep their places.
        material
            .parameters
            .retain(|(name, _)| declaring.parameters.contains_key(name));
    } else {
        material.parameters.clear();
    }
    Some(Resolved {
        material: std::sync::Arc::new(material),
        files,
        broken,
        merged,
    })
}

/// Reads the material file at `path` inside `folder`, or explains why it
/// can't.
fn read_project_material(folder: &Path, path: &str) -> Result<MaterialFile, String> {
    let relative = crate::safe_path(path).ok_or(
        "a material must be a file inside the project folder, such as \
         \"assets/materials/brick.material.json\"",
    )?;
    let text = fs::read_to_string(folder.join(relative)).map_err(|err| match err.kind() {
        io::ErrorKind::NotFound => "the file isn't there".to_owned(),
        _ => err.to_string(),
    })?;
    parse_material(&text).map_err(|err| crate::with_causes(&err))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Assets;

    fn project(name: &str) -> std::path::PathBuf {
        let folder =
            std::env::temp_dir().join(format!("axcenity-material-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(folder.join(MATERIALS_FOLDER)).unwrap();
        folder
    }

    fn write(folder: &Path, path: &str, material: &MaterialFile) {
        write_material_file(&folder.join(path), material).unwrap();
    }

    fn every_value() -> MaterialFile {
        MaterialFile {
            base: Some("assets/materials/stone.material.json".into()),
            color: Some([0.8, 0.3, 0.2]),
            metalness: Some(0.25),
            roughness: Some(0.75),
            emissive_color: Some([1.0, 0.5, 0.0]),
            emissive_strength: Some(2.0),
            opacity: Some(0.5),
            alpha_cutoff: Some(0.3),
            maps: MaterialMaps {
                color: Some(MapFile::of("assets/textures/brick.png")),
                normal: Some(MapFile {
                    path: Some("assets/textures/brick_n.png".into()),
                    strength: Some(0.5),
                    direct_x: Some(true),
                    ..MapFile::default()
                }),
                metalness: Some(MapFile {
                    path: Some("assets/textures/orm.png".into()),
                    channel: Some(Channel::Blue),
                    ..MapFile::default()
                }),
                roughness: Some(MapFile {
                    path: Some("assets/textures/orm.png".into()),
                    channel: Some(Channel::Green),
                    invert: Some(true),
                    ..MapFile::default()
                }),
                occlusion: Some(MapFile {
                    path: Some("assets/textures/orm.png".into()),
                    strength: Some(0.8),
                    ..MapFile::default()
                }),
                emissive: Some(MapFile::of("assets/textures/glow.png")),
                opacity: Some(MapFile {
                    path: Some("assets/textures/mask.png".into()),
                    channel: Some(Channel::Red),
                    invert: Some(false),
                    ..MapFile::default()
                }),
                specular: Some(MapFile::of("")),
            },
            tiling: Some([2.0, 3.0]),
            offset: Some([0.5, 0.25]),
            double_sided: Some(true),
            sway: Some(0.6),
            shader: Some(
                "fn surface(input: SurfaceInput) -> Surface {
    return standard_surface(input);
}
"
                .into(),
            ),
            parameters: BTreeMap::from([
                ("Scale".to_owned(), ParamValue::Number(4.0)),
                ("Tint".to_owned(), ParamValue::Vector(vec![1.0, 0.5, 0.0])),
            ]),
            graph: Some(serde_json::json!({ "nodes": [] })),
        }
    }

    #[test]
    fn a_material_file_round_trips_and_says_only_what_it_sets() {
        let folder = project("round-trip");
        let path = folder.join("assets/materials/brick.material.json");
        let material = every_value();
        write_material_file(&path, &material).unwrap();
        assert_eq!(read_material_file(&path).unwrap(), material);

        let only_color = MaterialFile {
            color: Some([0.1, 0.2, 0.3]),
            ..MaterialFile::default()
        };
        let text = material_json(&only_color);
        assert_eq!(
            text.replace("\r\n", "\n"),
            "{\n  \"color\": [\n    0.1,\n    0.2,\n    0.3\n  ]\n}\n"
        );
        assert_eq!(parse_material(&text).unwrap(), only_color);
        assert!(matches!(
            parse_material("{ \"metalness\": \"shiny\" }"),
            Err(MaterialFileError::Format(_))
        ));
        let _ = fs::remove_dir_all(folder);
    }

    #[test]
    fn an_instance_takes_its_bases_values_except_the_ones_it_overrides() {
        let folder = project("instance");
        let base = MaterialFile {
            color: Some([0.5, 0.5, 0.5]),
            metalness: Some(1.0),
            roughness: Some(0.3),
            maps: MaterialMaps {
                color: Some(MapFile::of("assets/textures/brick.png")),
                normal: Some(MapFile {
                    path: Some("assets/textures/brick_n.png".into()),
                    strength: Some(0.5),
                    ..MapFile::default()
                }),
                ..MaterialMaps::default()
            },
            tiling: Some([2.0, 2.0]),
            sway: Some(0.5),
            ..MaterialFile::default()
        };
        write(&folder, "assets/materials/brick.material.json", &base);
        // Overrides the roughness and the normal map's strength (keeping
        // its image), and takes the color map away.
        let instance = MaterialFile {
            base: Some("assets/materials/brick.material.json".into()),
            roughness: Some(0.9),
            maps: MaterialMaps {
                color: Some(MapFile::of("")),
                normal: Some(MapFile {
                    strength: Some(2.0),
                    ..MapFile::default()
                }),
                ..MaterialMaps::default()
            },
            ..MaterialFile::default()
        };
        write(&folder, "assets/materials/worn.material.json", &instance);

        let assets = Assets::new(&folder);
        let worn = assets
            .material("assets/materials/worn.material.json")
            .unwrap();
        assert_eq!(worn.color, [0.5, 0.5, 0.5]);
        assert_eq!(worn.metalness, 1.0);
        assert_eq!(worn.roughness, 0.9);
        assert_eq!(worn.tiling, [2.0, 2.0]);
        assert_eq!(worn.sway, 0.5);
        assert_eq!(worn.maps.color, None);
        let normal = worn.maps.normal.as_ref().unwrap();
        assert_eq!(normal.path, "assets/textures/brick_n.png");
        assert_eq!(normal.strength, 2.0);
        // What neither sets is the default material's.
        let default = StandardMaterial::default();
        assert_eq!(worn.opacity, default.opacity);
        assert_eq!(worn.emissive_strength, default.emissive_strength);
        assert!(!worn.double_sided);

        let brick = assets
            .material("assets/materials/brick.material.json")
            .unwrap();
        assert_eq!(brick.roughness, 0.3);
        assert_eq!(
            brick.maps.color.as_ref().unwrap().path,
            "assets/textures/brick.png"
        );
        assert!(assets.take_errors().is_empty());

        // The base changes on disk: forgetting it changes the instance too.
        write(
            &folder,
            "assets/materials/brick.material.json",
            &MaterialFile {
                metalness: Some(0.0),
                ..base
            },
        );
        assets.forget("assets/materials/brick.material.json");
        let worn = assets
            .material("assets/materials/worn.material.json")
            .unwrap();
        assert_eq!(worn.metalness, 0.0);
        assert_eq!(worn.roughness, 0.9);
        let _ = fs::remove_dir_all(folder);
    }

    #[test]
    fn a_cycle_or_missing_base_is_reported_once_and_the_default_stands_in() {
        let folder = project("broken");
        let based_on = |base: &str| MaterialFile {
            base: Some(format!("assets/materials/{base}.material.json")),
            ..MaterialFile::default()
        };
        write(&folder, "assets/materials/a.material.json", &based_on("b"));
        write(&folder, "assets/materials/b.material.json", &based_on("a"));
        write(
            &folder,
            "assets/materials/orphan.material.json",
            &based_on("gone"),
        );
        write(
            &folder,
            "assets/materials/own.material.json",
            &based_on("own"),
        );
        let assets = Assets::new(&folder);
        for _ in 0..2 {
            for name in ["a", "orphan", "own"] {
                let material = assets
                    .material(&format!("assets/materials/{name}.material.json"))
                    .unwrap();
                assert_eq!(*material, StandardMaterial::default(), "{name}");
            }
            assert!(
                assets
                    .material("assets/materials/missing.material.json")
                    .is_none()
            );
            assert!(assets.material("../outside.material.json").is_none());
        }
        let errors = assets.take_errors();
        assert_eq!(errors.len(), 5, "{errors:#?}");
        assert!(errors[0].contains("cycle (assets/materials/a.material.json → assets/materials/b.material.json → assets/materials/a.material.json)"), "{errors:#?}");
        assert!(errors[1].contains("`assets/materials/gone.material.json`, the base of the material `assets/materials/orphan.material.json`: the file isn't there"), "{errors:#?}");
        assert!(errors[2].contains("cycle"), "{errors:#?}");
        assert!(
            errors[3].contains("`assets/materials/missing.material.json`: the file isn't there"),
            "{errors:#?}"
        );
        assert!(
            errors[4].contains("inside the project folder"),
            "{errors:#?}"
        );

        // An instance keeps its own values over the stand-in.
        write(
            &folder,
            "assets/materials/orphan.material.json",
            &MaterialFile {
                roughness: Some(0.9),
                ..based_on("gone")
            },
        );
        assets.forget("assets/materials/orphan.material.json");
        let orphan = assets
            .material("assets/materials/orphan.material.json")
            .unwrap();
        assert_eq!(orphan.roughness, 0.9);
        assert_eq!(orphan.metalness, StandardMaterial::default().metalness);

        // Adding the missing base and retrying failures fixes it.
        write(
            &folder,
            "assets/materials/gone.material.json",
            &MaterialFile {
                metalness: Some(1.0),
                ..MaterialFile::default()
            },
        );
        assets.forget_failures();
        let orphan = assets
            .material("assets/materials/orphan.material.json")
            .unwrap();
        assert_eq!(orphan.metalness, 1.0);
        let _ = fs::remove_dir_all(folder);
    }

    #[test]
    fn a_shader_comes_from_the_file_that_sets_it_and_an_empty_one_takes_it_away() {
        let folder = project("shader");
        let (base, instance, plain) = (
            "assets/materials/water.material.json",
            "assets/materials/murky.material.json",
            "assets/materials/still.material.json",
        );
        let code = "fn surface(input: SurfaceInput) -> Surface {
    return standard_surface(input);
}
";
        write(
            &folder,
            base,
            &MaterialFile {
                shader: Some(code.to_owned()),
                ..MaterialFile::default()
            },
        );
        let based = |shader: Option<&str>| MaterialFile {
            base: Some(base.to_owned()),
            roughness: Some(0.9),
            shader: shader.map(str::to_owned),
            ..MaterialFile::default()
        };
        write(&folder, instance, &based(None));
        write(&folder, plain, &based(Some("")));
        let assets = Assets::new(&folder);
        let shader = |path| assets.material(path).unwrap().shader.clone();
        let from_base = shader(instance).unwrap();
        assert_eq!(&*from_base.code, code);
        assert_eq!(from_base.file, base, "its lines are the base's");
        assert_eq!(shader(base).unwrap().file, base);
        assert_eq!(shader(plain), None);
        assert_eq!(StandardMaterial::default().shader, None);
        let _ = fs::remove_dir_all(&folder);
    }

    #[test]
    fn an_edit_handed_over_is_used_in_place_of_the_file_by_it_and_every_instance() {
        let folder = project("edited");
        let (base, instance) = (
            "assets/materials/base.material.json",
            "assets/materials/instance.material.json",
        );
        write(
            &folder,
            base,
            &MaterialFile {
                roughness: Some(0.9),
                ..MaterialFile::default()
            },
        );
        write(
            &folder,
            instance,
            &MaterialFile {
                base: Some(base.to_owned()),
                metalness: Some(1.0),
                ..MaterialFile::default()
            },
        );
        let assets = Assets::new(&folder);
        assert_eq!(assets.material(instance).unwrap().roughness, 0.9);
        let chain = assets.material_chain(instance).unwrap();
        assert_eq!(chain.files, [instance, base]);
        assert_eq!(chain.merged.roughness, Some(0.9));
        assert_eq!(chain.merged.metalness, Some(1.0));
        assert!(!chain.broken);

        // An edit to the base, not yet saved: both draw with it.
        let edited = MaterialFile {
            roughness: Some(0.2),
            ..MaterialFile::default()
        };
        assets.put_material_file(base, Some(edited));
        assert_eq!(assets.material(base).unwrap().roughness, 0.2);
        assert_eq!(assets.material(instance).unwrap().roughness, 0.2);
        assert_eq!(assets.material(instance).unwrap().metalness, 1.0);
        // A material that has no file yet is used too.
        let new = "assets/materials/new.material.json";
        assets.put_material_file(
            new,
            Some(MaterialFile {
                base: Some(base.to_owned()),
                ..MaterialFile::default()
            }),
        );
        assert_eq!(assets.material(new).unwrap().roughness, 0.2);

        // Taken back, the file is read again.
        assets.put_material_file(base, None);
        assert_eq!(assets.material(instance).unwrap().roughness, 0.9);
        assert!(assets.take_errors().is_empty());
        let _ = fs::remove_dir_all(&folder);
    }

    #[test]
    fn an_instance_overrides_the_parameters_its_shaders_file_names_and_no_others() {
        let folder = project("parameters");
        let (base, instance) = (
            "assets/materials/moss.material.json",
            "assets/materials/autumn.material.json",
        );
        write(
            &folder,
            base,
            &MaterialFile {
                shader: Some(
                    "fn surface(input: SurfaceInput) -> Surface {
    return standard_surface(input);
}
"
                    .into(),
                ),
                parameters: BTreeMap::from([
                    ("Amount".to_owned(), ParamValue::Number(0.5)),
                    ("Tint".to_owned(), ParamValue::Vector(vec![0.2, 0.6, 0.2])),
                ]),
                graph: Some(serde_json::json!({ "nodes": [] })),
                ..MaterialFile::default()
            },
        );
        write(
            &folder,
            instance,
            &MaterialFile {
                base: Some(base.to_owned()),
                parameters: BTreeMap::from([
                    ("Tint".to_owned(), ParamValue::Vector(vec![0.8, 0.4, 0.1])),
                    ("Unknown".to_owned(), ParamValue::Number(9.0)),
                ]),
                ..MaterialFile::default()
            },
        );
        let text = fs::read_to_string(folder.join(instance)).unwrap();
        assert!(
            text.contains("\"Unknown\": 9.0"),
            "a number is written plain: {text}"
        );

        let assets = Assets::new(&folder);
        let parameters = |path| assets.material(path).unwrap().parameters.clone();
        assert_eq!(
            parameters(base),
            [
                ("Amount".to_owned(), [0.5, 0.0, 0.0, 0.0]),
                ("Tint".to_owned(), [0.2, 0.6, 0.2, 0.0]),
            ]
        );
        assert_eq!(
            parameters(instance),
            [
                ("Amount".to_owned(), [0.5, 0.0, 0.0, 0.0]),
                ("Tint".to_owned(), [0.8, 0.4, 0.1, 0.0]),
            ],
            "its own tint, its base's amount, and not a parameter the shader doesn't name"
        );
        // The graph is the editor's, so the instance doesn't take it.
        let chain = assets.material_chain(instance).unwrap();
        assert_eq!(chain.merged.graph, None);
        assert!(chain.merged.shader.is_some());
        let _ = fs::remove_dir_all(&folder);
    }
}
