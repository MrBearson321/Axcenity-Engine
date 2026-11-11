//! The one asset record (`asset-source`, version 1): how every source
//! describes an asset, so nothing above the sources needs to know a
//! provider's answer. It is the Blender pipeline's `AssetRecord` with the
//! Django Toolbox's variants and attribution added, and reads and writes
//! the same JSON (unknown fields are ignored, missing ones take defaults).

use serde::{Deserialize, Serialize};

/// What an asset is for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// A material's maps (colour, normal, roughness…).
    #[default]
    Texture,
    /// An environment image, used as a sky.
    Hdri,
    /// A 3D model.
    Model,
    /// A sound effect or music.
    Sound,
    /// 2D sprite sheets (Kenney packs in the Blender pipeline's library).
    Sprites,
}

impl Kind {
    /// Every kind, in the order the Toolbox shows them.
    pub const ALL: [Kind; 4] = [Kind::Texture, Kind::Hdri, Kind::Model, Kind::Sound];

    /// The name in records and URLs: `texture`, `hdri`, `model`, `sound`.
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Texture => "texture",
            Kind::Hdri => "hdri",
            Kind::Model => "model",
            Kind::Sound => "sound",
            Kind::Sprites => "sprites",
        }
    }

    /// A kind from its name or a common plural ("textures", "skies").
    pub fn parse(text: &str) -> Option<Kind> {
        match text.trim().to_ascii_lowercase().as_str() {
            "texture" | "textures" | "material" | "materials" => Some(Kind::Texture),
            "hdri" | "hdris" | "sky" | "skies" => Some(Kind::Hdri),
            "model" | "models" => Some(Kind::Model),
            "sound" | "sounds" | "audio" => Some(Kind::Sound),
            "sprites" | "sprite" => Some(Kind::Sprites),
            _ => None,
        }
    }
}

/// How a variant's files go together.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Package {
    /// Loose map images, one file per map.
    Maps,
    /// One zip to unpack.
    Archive,
    /// A scene file and the files it includes, kept at their relative paths.
    Bundle,
    /// One file on its own (an HDRI, a sound).
    Single,
}

/// One file to download: the provider's address, never copied by us until
/// someone fetches it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RemoteFile {
    pub url: String,
    /// Where it goes inside the asset's folder (a file name, or a relative
    /// path inside a bundle). Already checked by `guard`.
    pub path: String,
    /// Bytes, when the provider says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// The provider's MD5, when it publishes one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub md5: Option<String>,
    /// The map it holds (`base_color`, `normal`…), for loose maps.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub map: Option<String>,
}

/// One way to download an asset, such as "2K JPG".
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Variant {
    pub label: String,
    /// `1k`, `2k`…, when the variant has one.
    #[serde(default)]
    pub resolution: Option<String>,
    /// Lowercase: `jpg`, `png`, `exr`, `hdr`, `gltf`, `mp3`…
    pub format: String,
    pub package: Package,
    pub files: Vec<RemoteFile>,
}

impl Variant {
    /// The files' combined size, or `None` when any size is unknown.
    pub fn total_size(&self) -> Option<u64> {
        self.files.iter().map(|file| file.size).sum()
    }
}

/// One asset as a source describes it. Search results leave `variants`
/// empty; a full lookup ([`AssetSource::record`](crate::AssetSource::record))
/// fills them in.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AssetRecord {
    /// The source's key, such as `polyhaven`.
    pub provider: String,
    /// The source's own id or slug.
    pub id: String,
    pub kind: Kind,
    pub name: String,
    /// The asset's page on the source's site.
    pub url: String,
    pub description: String,
    pub tags: Vec<String>,
    pub categories: Vec<String>,
    pub authors: Vec<String>,
    /// A normalised code from [`license`](crate::license): `CC0`, `CC-BY`…
    pub license: String,
    pub license_version: String,
    pub license_url: String,
    /// The source's own credit text, when it gives one.
    pub attribution: String,
    /// `1k`, `2k`…, smallest first.
    pub resolutions: Vec<String>,
    /// Normalised map names, for textures.
    pub maps: Vec<String>,
    /// Metres: width, height and, for models, depth.
    pub physical_size: Option<Vec<f64>>,
    pub polycount: u64,
    /// The source's download count, for sorting.
    pub popularity: f64,
    pub thumbnail_url: String,
    /// A sound's playable preview.
    pub preview_url: String,
    pub variants: Vec<Variant>,
}

impl AssetRecord {
    /// `provider:id`, the key every part of the Toolbox files it under.
    pub fn key(&self) -> String {
        format!("{}:{}", self.provider, self.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_blender_pipeline_record_reads_and_unknown_fields_are_ignored() {
        let json = r#"{
            "provider": "polyhaven", "id": "red_brick_03", "kind": "texture",
            "name": "Red Brick 03", "url": "https://polyhaven.com/a/red_brick_03",
            "authors": ["Rob Tuytel"], "license": "CC0", "resolutions": ["1k", "2k"],
            "maps": ["base_color", "normal"], "physical_size": null, "attributes": {},
            "method": "scan", "popularity": 12.0, "polycount": 0, "extra": {"category_path": ""}
        }"#;
        let record: AssetRecord = serde_json::from_str(json).unwrap();
        assert_eq!(record.key(), "polyhaven:red_brick_03");
        assert_eq!(record.kind, Kind::Texture);
        assert_eq!(record.authors, ["Rob Tuytel"]);
        assert!(record.variants.is_empty());
    }

    #[test]
    fn a_record_round_trips_through_json() {
        let record = AssetRecord {
            provider: "ambientcg".into(),
            id: "Fabric086".into(),
            kind: Kind::Texture,
            variants: vec![Variant {
                label: "2K JPG".into(),
                resolution: Some("2k".into()),
                format: "jpg".into(),
                package: Package::Archive,
                files: vec![RemoteFile {
                    url: "https://ambientcg.com/get?file=Fabric086_2K-JPG.zip".into(),
                    path: "Fabric086_2K-JPG.zip".into(),
                    size: Some(10),
                    ..RemoteFile::default()
                }],
            }],
            ..AssetRecord::default()
        };
        let json = serde_json::to_string(&record).unwrap();
        assert!(json.contains(r#""package":"archive""#));
        assert_eq!(serde_json::from_str::<AssetRecord>(&json).unwrap(), record);
    }

    #[test]
    fn a_total_size_needs_every_size() {
        let file = |size| RemoteFile {
            size,
            ..RemoteFile::default()
        };
        let variant = |files| Variant {
            label: String::new(),
            resolution: None,
            format: String::new(),
            package: Package::Maps,
            files,
        };
        assert_eq!(
            variant(vec![file(Some(1)), file(Some(2))]).total_size(),
            Some(3)
        );
        assert_eq!(variant(vec![file(Some(1)), file(None)]).total_size(), None);
    }
}
