//! Toolbox packs: the Django Toolbox's `axcenity-pack` manifest, version 1
//! (`packs/manifest.py` there; its golden file is
//! `tests/fixtures/pack_manifest.json` here).
//!
//! A pack lists assets, each with its chosen variant's files, licence,
//! author, credit and where it goes in a game (`import_path`). Importing
//! one ([`plan`]) says what each item is, what its licence blocks, how big
//! it is and where it lands, before anything is downloaded; a collection
//! exported as a pack ([`export`]) imports back as the same collection
//! ([`Manifest::collection_items`]), so collections and the site's packs
//! are one thing.

use serde::{Deserialize, Serialize};

use crate::SourceError;
use crate::db::Database;
use crate::guard;
use crate::license;
use crate::record::{AssetRecord, Kind, Package, RemoteFile, Variant};

pub const FORMAT: &str = "axcenity-pack";
pub const VERSION: u32 = 1;
/// The end of a pack file's name: `medieval-village.axcenity-pack.json`.
pub const SUFFIX: &str = ".axcenity-pack.json";

/// A pack manifest.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub format: String,
    pub version: u32,
    #[serde(default)]
    pub exported_at: String,
    pub pack: PackInfo,
    #[serde(default)]
    pub items: Vec<Item>,
    #[serde(default)]
    pub credits: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PackInfo {
    pub title: String,
    pub slug: String,
    pub author: String,
    pub description: String,
    pub tags: Vec<String>,
    pub source_url: String,
}

/// One asset in a pack.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Item {
    pub name: String,
    /// `texture`, `hdri`, `model`, `sound`.
    pub kind: String,
    pub provider: String,
    pub external_id: String,
    /// The chosen variant's label, such as "2K JPG"; `null` for none.
    pub variant: Option<String>,
    pub files: Vec<File>,
    pub total_size: Option<u64>,
    /// As the provider wrote it: "CC0", "CC BY 4.0".
    pub license: String,
    pub license_url: String,
    pub author: String,
    pub attribution: String,
    pub source_url: String,
    /// Where it goes in a game, such as `textures/red_brick_03/`.
    pub import_path: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct File {
    pub url: String,
    pub filename: String,
    pub size: Option<u64>,
}

impl Item {
    /// `provider:external_id`.
    pub fn key(&self) -> String {
        format!("{}:{}", self.provider, self.external_id)
    }

    pub fn kind(&self) -> Option<Kind> {
        Kind::parse(&self.kind)
    }

    /// The item as an asset record, its listed files as its one variant.
    pub fn record(&self) -> AssetRecord {
        let code = license::normalize(&self.license);
        let version = self
            .license
            .split_whitespace()
            .find(|word| word.chars().next().is_some_and(|c| c.is_ascii_digit()))
            .unwrap_or("")
            .to_owned();
        let variants = if self.files.is_empty() {
            Vec::new()
        } else {
            vec![self.variant()]
        };
        AssetRecord {
            provider: self.provider.clone(),
            id: self.external_id.clone(),
            kind: self.kind().unwrap_or_default(),
            name: self.name.clone(),
            url: self.source_url.clone(),
            authors: Some(self.author.trim().to_owned())
                .filter(|author| !author.is_empty())
                .into_iter()
                .collect(),
            license: code.to_owned(),
            license_version: version,
            license_url: self.license_url.clone(),
            attribution: self.attribution.clone(),
            variants,
            ..AssetRecord::default()
        }
    }

    /// The listed files as a variant.
    pub fn variant(&self) -> Variant {
        let label = self.variant.clone().unwrap_or_else(|| "Pack".to_owned());
        let format = self
            .files
            .first()
            .and_then(|file| file.filename.rsplit_once('.'))
            .map(|(_, ext)| ext.to_ascii_lowercase())
            .unwrap_or_default();
        Variant {
            resolution: label
                .split_whitespace()
                .next()
                .filter(|word| word.ends_with(['k', 'K']) && word.len() <= 3)
                .map(str::to_ascii_lowercase),
            format,
            package: if self.files.len() == 1 {
                Package::Single
            } else {
                Package::Maps
            },
            files: self
                .files
                .iter()
                .map(|file| RemoteFile {
                    url: file.url.clone(),
                    path: file.filename.clone(),
                    size: file.size,
                    md5: None,
                    map: None,
                })
                .collect(),
            label,
        }
    }
}

/// Reads a manifest, refusing another format or a newer version.
pub fn parse(text: &str) -> Result<Manifest, SourceError> {
    let value: serde_json::Value = serde_json::from_str(text)
        .map_err(|_| SourceError::BadResponse("the pack isn't valid JSON"))?;
    if value.get("format").and_then(|f| f.as_str()) != Some(FORMAT) {
        return Err(SourceError::Refused(
            "the file isn't an Axcenity Toolbox pack",
        ));
    }
    let version = value.get("version").and_then(|v| v.as_u64()).unwrap_or(0);
    if version > u64::from(VERSION) {
        return Err(SourceError::Refused(
            "the pack is from a newer Toolbox; update the editor to import it",
        ));
    }
    serde_json::from_value(value).map_err(|_| SourceError::BadResponse("the pack is damaged"))
}

/// Where a pack's `import_path` lands in a project, under `assets/`: the
/// Toolbox's folders map to the engine's (`textures/` to
/// `assets/textures/`, `hdris/` to `assets/sky/`, `models/` to
/// `assets/models/`, `sounds/` and `audio/` to `assets/audio/`). `None`
/// for a path that would leave the folder. A blank path takes the kind's
/// folder and the item's id.
pub fn destination(item: &Item) -> Option<String> {
    let path = item.import_path.trim().trim_end_matches('/');
    let path = if path.is_empty() {
        let folder = match item.kind() {
            Some(Kind::Hdri) => "hdris",
            Some(Kind::Model) => "models",
            Some(Kind::Sound) => "sounds",
            _ => "textures",
        };
        format!("{folder}/{}", item.external_id)
    } else {
        path.to_owned()
    };
    let path = guard::safe_relative_path(&path)?;
    let (first, rest) = path.split_once('/').unwrap_or((path.as_str(), ""));
    let folder = match first {
        "textures" => "assets/textures",
        "hdris" | "hdri" | "skies" | "sky" => "assets/sky",
        "models" => "assets/models",
        "sounds" | "audio" => "assets/audio",
        _ => return Some(format!("assets/{path}")),
    };
    Some(if rest.is_empty() {
        folder.to_owned()
    } else {
        format!("{folder}/{rest}")
    })
}

/// How an item would come in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fetch {
    /// Through its Toolbox source by `provider:id` (verified), the pack's
    /// variant when it's still offered.
    Source,
    /// From the listed addresses, under the download rules.
    Files,
    /// It can't: why.
    No(String),
}

/// One item of an import plan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Planned {
    pub key: String,
    pub name: String,
    pub kind: Option<Kind>,
    /// The licence as a badge ("CC BY 4.0").
    pub license: String,
    /// Why it can't go in a commercial game, if it can't.
    pub blocked: Option<&'static str>,
    pub size: Option<u64>,
    /// Its folder in the project.
    pub destination: Option<String>,
    pub fetch: Fetch,
}

/// What importing a pack would do, item by item, before anything is
/// downloaded.
pub fn plan(manifest: &Manifest) -> Vec<Planned> {
    manifest
        .items
        .iter()
        .map(|item| {
            let record = item.record();
            let destination = destination(item);
            let source = crate::source(&item.provider);
            let fetch = if item.kind().is_none() {
                Fetch::No(format!(
                    "its kind \"{}\" isn't one the engine imports",
                    item.kind
                ))
            } else if destination.is_none() {
                Fetch::No("its import path leaves the project's assets folder".into())
            } else if !guard::safe_relative_path(&item.external_id)
                .is_some_and(|id| !id.contains('/'))
            {
                Fetch::No("its id isn't a plain name".into())
            } else if source.is_some_and(|source| source.info().key != "openverse") {
                Fetch::Source
            } else if item.files.is_empty() {
                Fetch::No("it lists no files".into())
            } else if let Some(bad) = item.files.iter().find(|file| refused(file).is_some()) {
                Fetch::No(refused(bad).unwrap_or_default())
            } else {
                Fetch::Files
            };
            Planned {
                key: item.key(),
                name: item.name.clone(),
                kind: item.kind(),
                license: license::display(&record.license, &record.license_version),
                blocked: license::commercial_block(&record.license),
                size: item
                    .total_size
                    .or_else(|| item.files.iter().map(|file| file.size).sum()),
                destination,
                fetch,
            }
        })
        .collect()
}

/// File types never downloaded from a pack: Blender scenes, scripts and
/// programs.
const REFUSED_TYPES: [&str; 12] = [
    "blend", "py", "pyc", "exe", "dll", "bat", "cmd", "ps1", "sh", "js", "lua", "so",
];

/// Why a listed file won't be downloaded, if it won't.
fn refused(file: &File) -> Option<String> {
    if !file.url.starts_with("https://") {
        return Some(format!("{} isn't on an https address", file.filename));
    }
    if guard::safe_relative_path(&file.filename).is_none_or(|name| name.contains('/')) {
        return Some(format!("\"{}\" isn't a plain file name", file.filename));
    }
    let ext = file
        .filename
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .unwrap_or_default();
    REFUSED_TYPES
        .contains(&ext.as_str())
        .then(|| format!("{} is a type packs may not bring", file.filename))
}

/// The items to keep as a collection: each key and its chosen variant.
impl Manifest {
    pub fn collection_items(&self) -> Vec<(String, Option<String>)> {
        self.items
            .iter()
            .map(|item| (item.key(), item.variant.clone()))
            .collect()
    }

    /// Stores every item's record and keeps the pack as a collection named
    /// after it. Records already stored keep their fuller details.
    pub fn save_as_collection(&self, db: &Database) -> Result<String, SourceError> {
        let name = if self.pack.title.trim().is_empty() {
            "Imported pack".to_owned()
        } else {
            self.pack.title.trim().to_owned()
        };
        for item in &self.items {
            if db.asset(&item.key())?.is_none() {
                db.put_asset(&item.record())?;
            }
        }
        db.create_collection(&name)?;
        for (key, variant) in self.collection_items() {
            db.add_to_collection(&name, &key, variant.as_deref())?;
        }
        Ok(name)
    }
}

/// The Toolbox's folder for a kind, in a pack's `import_path`.
fn import_folder(kind: Kind) -> &'static str {
    match kind {
        Kind::Hdri => "hdris",
        Kind::Model => "models",
        Kind::Sound => "sounds",
        Kind::Texture | Kind::Sprites => "textures",
    }
}

/// `unix` seconds as the manifest's `exported_at` (`2026-11-05T12:00:00+00:00`).
pub fn timestamp(unix: i64) -> String {
    let days = unix.div_euclid(86_400);
    let secs = unix.rem_euclid(86_400);
    // Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}+00:00",
        secs / 3_600,
        secs % 3_600 / 60,
        secs % 60
    )
}

/// A collection as a pack manifest. Each item lists the files of its chosen
/// variant (or its first downloaded one) when the stored record has them.
pub fn export(db: &Database, collection: &str, exported_at: &str) -> Result<Manifest, SourceError> {
    let found = db.collection(collection)?.ok_or(SourceError::NotFound)?;
    let mut items = Vec::new();
    let mut credits: Vec<String> = Vec::new();
    for entry in &found.items {
        let Some(record) = db.asset(&entry.key)? else {
            continue;
        };
        let label = entry.variant.clone().or_else(|| {
            db.downloads_of(&entry.key)
                .ok()
                .and_then(|downloads| downloads.into_iter().next())
                .map(|download| download.variant)
        });
        let variant = label
            .as_ref()
            .and_then(|label| record.variants.iter().find(|v| &v.label == label));
        let files: Vec<File> = variant
            .map(|variant| {
                variant
                    .files
                    .iter()
                    .map(|file| File {
                        url: file.url.clone(),
                        filename: file
                            .path
                            .rsplit('/')
                            .next()
                            .unwrap_or(&file.path)
                            .to_owned(),
                        size: file.size,
                    })
                    .collect()
            })
            .unwrap_or_default();
        let badge = license::display(&record.license, &record.license_version);
        let license_text = if record.license == license::UNKNOWN {
            String::new()
        } else {
            badge
        };
        let item = Item {
            name: record.name.clone(),
            kind: record.kind.as_str().to_owned(),
            provider: record.provider.clone(),
            external_id: record.id.clone(),
            variant: label,
            total_size: variant.and_then(Variant::total_size),
            files,
            license: license_text,
            license_url: record.license_url.clone(),
            author: record.authors.join(", "),
            attribution: record.attribution.clone(),
            source_url: record.url.clone(),
            import_path: format!("{}/{}/", import_folder(record.kind), record.id),
        };
        if license::needs_credit(license::normalize(&record.license)) {
            let line =
                license::credit_line(&record, &crate::provenance::source_name(&record.provider));
            if !credits.contains(&line) {
                credits.push(line);
            }
        }
        items.push(item);
    }
    let slug: String = collection
        .to_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    Ok(Manifest {
        format: FORMAT.to_owned(),
        version: VERSION,
        exported_at: exported_at.to_owned(),
        pack: PackInfo {
            title: collection.to_owned(),
            slug,
            author: String::new(),
            description: String::new(),
            tags: Vec::new(),
            source_url: String::new(),
        },
        items,
        credits,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOLDEN: &str = include_str!("../tests/fixtures/pack_manifest.json");

    #[test]
    fn the_django_toolboxs_golden_manifest_reads_and_plans() {
        let manifest = parse(GOLDEN).unwrap();
        assert_eq!(manifest.pack.title, "Medieval Village");
        assert_eq!(manifest.items.len(), 5);
        let planned = plan(&manifest);
        let brick = &planned[0];
        assert_eq!(brick.key, "polyhaven:red_brick_03");
        assert_eq!(brick.fetch, Fetch::Source);
        assert_eq!(
            brick.destination.as_deref(),
            Some("assets/textures/red_brick_03")
        );
        assert_eq!(brick.size, Some(4_000_000));
        assert_eq!(brick.license, "CC0");
        assert_eq!(brick.blocked, None);

        let birds = &planned[1];
        assert_eq!(birds.fetch, Fetch::Files);
        assert_eq!(birds.destination.as_deref(), Some("assets/audio/ambience"));
        assert_eq!(birds.license, "CC BY 4.0");
        assert_eq!(birds.size, None);
        assert_eq!(
            planned[2].destination.as_deref(),
            Some("assets/audio/birds-002")
        );
        assert_eq!(planned[3].license, "CC BY-SA 3.0");
        // A crate with no files still comes through its source.
        assert_eq!(planned[4].fetch, Fetch::Source);
        assert_eq!(
            planned[4].destination.as_deref(),
            Some("assets/models/Crate001")
        );

        let record = manifest.items[1].record();
        assert_eq!(record.key(), "openverse:birds-001");
        assert_eq!(
            (record.license.as_str(), record.license_version.as_str()),
            ("CC-BY", "4.0")
        );
        assert_eq!(record.authors, ["Jane Doe"]);
        assert_eq!(record.variants[0].files[0].path, "birds-001.mp3");
    }

    #[test]
    fn licence_warnings_show_before_import() {
        let mut manifest = parse(GOLDEN).unwrap();
        manifest.items[2].license = "CC BY-NC 4.0".into();
        manifest.items[3].license = "All rights reserved".into();
        let planned = plan(&manifest);
        assert_eq!(planned[2].blocked, Some("its licence is non-commercial"));
        assert_eq!(planned[3].blocked, Some("its licence is unknown"));
    }

    #[test]
    fn other_formats_newer_versions_and_unsafe_items_are_refused() {
        assert!(parse(r#"{"format": "something-else", "version": 1}"#).is_err());
        let newer = GOLDEN.replacen("\"version\": 1", "\"version\": 2", 1);
        assert!(matches!(parse(&newer), Err(SourceError::Refused(_))));
        assert!(parse("{").is_err());
        let mut manifest = parse(GOLDEN).unwrap();
        manifest.items[1].import_path = "../../outside/".into();
        manifest.items[2].files[0].url = "http://cdn.freesound.org/x.mp3".into();
        manifest.items[3].files[0].filename = "door.blend".into();
        manifest.items[4].kind = "spaceship".into();
        let planned = plan(&manifest);
        for item in &planned[1..] {
            assert!(matches!(item.fetch, Fetch::No(_)), "{item:?}");
        }
    }

    #[test]
    fn an_exported_collection_imports_back_unchanged() {
        let db = Database::in_memory().unwrap();
        let manifest = parse(GOLDEN).unwrap();
        let name = manifest.save_as_collection(&db).unwrap();
        assert_eq!(name, "Medieval Village");
        let exported = export(&db, &name, "2026-11-11T06:00:00+00:00").unwrap();
        let text = serde_json::to_string_pretty(&exported).unwrap();
        let again = parse(&text).unwrap();
        assert_eq!(again.collection_items(), manifest.collection_items());
        assert_eq!(again.pack.slug, "medieval-village");
        let brick = &again.items[0];
        assert_eq!(brick.files.len(), 2);
        assert_eq!(brick.files[0].filename, "red_brick_03_diff_2k.jpg");
        assert_eq!(brick.import_path, "textures/red_brick_03/");
        assert_eq!(destination(brick), destination(&manifest.items[0]));
        // Every item that needs credit is credited once.
        assert_eq!(again.credits.len(), 2, "{:?}", again.credits);
        assert_eq!(again.credits[0], manifest.credits[0]);
        // And importing the export into a second library gives the same collection.
        let second = Database::in_memory().unwrap();
        again.save_as_collection(&second).unwrap();
        assert_eq!(
            second.collection(&name).unwrap(),
            db.collection(&name).unwrap()
        );
    }

    #[test]
    fn timestamps_are_utc_dates_and_times() {
        assert_eq!(timestamp(0), "1970-01-01T00:00:00+00:00");
        assert_eq!(timestamp(1_793_880_000), "2026-11-05T12:00:00+00:00");
        assert_eq!(timestamp(951_782_400), "2000-02-29T00:00:00+00:00");
    }
}
