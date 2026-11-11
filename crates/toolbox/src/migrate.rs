//! The conditional import of the Blender pipeline's `asset_library` into
//! the central asset directory (decision 3).
//!
//! Every asset in its `metadata/registry.json` has its record put in the
//! metadata database. Each download is copied into the central directory,
//! at the same relative folder, **only if every file it lists is still
//! there and the files add up to the size it recorded**; otherwise it's
//! reported and skipped. Freesound sounds saved beside their JSON sidecar
//! (`audio/freesound/<id>.json`) come across the same way. Nothing in the
//! old folder is changed or deleted, and running it again copies only what
//! isn't in the directory yet.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::SourceError;
use crate::db::{Database, Download, StoredFile, json};
use crate::directory::AssetDirectory;
use crate::download::{is_present, md5_of};
use crate::guard;
use crate::license;
use crate::record::{AssetRecord, Kind};

/// What happened to one download.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Entry {
    pub key: String,
    /// The pipeline's name for it: a resolution such as `2k`, or `pack`.
    pub variant: String,
    pub reason: String,
}

/// What the migration took and what it left.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Report {
    /// Records put in the database.
    pub records: usize,
    /// Downloads copied into the directory.
    pub copied: Vec<Entry>,
    /// Downloads already in the directory.
    pub present: Vec<Entry>,
    /// Downloads left behind, and why.
    pub skipped: Vec<Entry>,
    pub bytes: u64,
}

impl Report {
    /// One paragraph for people: totals, then each skip.
    pub fn summary(&self) -> String {
        let mut text = format!(
            "{} records; {} downloads copied ({:.1} MB), {} already there, {} skipped.",
            self.records,
            self.copied.len(),
            self.bytes as f64 / 1_000_000.0,
            self.present.len(),
            self.skipped.len()
        );
        for entry in &self.skipped {
            text.push_str(&format!(
                "\n  skipped {} {}: {}",
                entry.key, entry.variant, entry.reason
            ));
        }
        text
    }
}

#[derive(Deserialize)]
struct Registry {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    assets: BTreeMap<String, RegistryEntry>,
}

#[derive(Deserialize)]
struct RegistryEntry {
    record: serde_json::Value,
    #[serde(default)]
    downloads: BTreeMap<String, OldDownload>,
    #[serde(default)]
    analysis: BTreeMap<String, serde_json::Value>,
}

#[derive(Deserialize)]
struct OldDownload {
    folder: String,
    #[serde(default)]
    files: Vec<OldFile>,
    #[serde(default)]
    bytes: u64,
    #[serde(default)]
    source_urls: Vec<String>,
}

/// A file as the registry lists it: a path, or (later pipelines) a path
/// with its size and MD5.
#[derive(Deserialize)]
#[serde(untagged)]
enum OldFile {
    Path(String),
    Checked {
        path: String,
        #[serde(default)]
        size: Option<u64>,
        #[serde(default)]
        md5: Option<String>,
    },
}

impl OldFile {
    fn parts(&self) -> (&str, Option<u64>, Option<&str>) {
        match self {
            OldFile::Path(path) => (path, None, None),
            OldFile::Checked { path, size, md5 } => (path, *size, md5.as_deref()),
        }
    }
}

/// A Freesound sidecar from the pipeline's audio script.
#[derive(Deserialize)]
struct FreesoundSidecar {
    id: serde_json::Value,
    #[serde(default)]
    user: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    page: String,
    #[serde(default)]
    file: String,
    #[serde(default)]
    licence: String,
    #[serde(default)]
    licence_url: String,
    #[serde(default)]
    bytes: u64,
}

/// One download to bring across: where its files are now and what they
/// must be.
struct Planned {
    key: String,
    variant: String,
    folder: String,
    files: Vec<(String, Option<u64>, Option<String>)>,
    bytes: u64,
    source_urls: Vec<String>,
}

/// Imports the Blender `asset_library` at `old` into `dir` and `db`. With
/// `dry_run`, checks everything and reports what it would do, changing
/// nothing.
pub fn import_blender_library(
    old: &Path,
    dir: &AssetDirectory,
    db: &Database,
    dry_run: bool,
) -> Result<Report, SourceError> {
    let registry_path = old.join("metadata").join("registry.json");
    let mut report = Report::default();
    let mut planned = Vec::new();
    if registry_path.is_file() {
        let text = fs::read_to_string(&registry_path)?;
        let registry: Registry = serde_json::from_str(&text)
            .map_err(|_| SourceError::BadResponse("the library's registry.json is damaged"))?;
        if registry.version > 1 {
            return Err(SourceError::Refused(
                "the library's registry is newer than this Toolbox reads",
            ));
        }
        for (key, entry) in registry.assets {
            let record: AssetRecord = match serde_json::from_value(entry.record) {
                Ok(record) => record,
                Err(_) => {
                    report.skipped.push(Entry {
                        key,
                        variant: String::new(),
                        reason: "its record couldn't be read".into(),
                    });
                    continue;
                }
            };
            if record.key() != key {
                report.skipped.push(Entry {
                    key,
                    variant: String::new(),
                    reason: "its record names another asset".into(),
                });
                continue;
            }
            if !dry_run {
                db.put_asset(&record)?;
                for (name, value) in &entry.analysis {
                    db.conn.execute(
                        "INSERT OR REPLACE INTO analysis (key, name, value) VALUES (?1, ?2, ?3)",
                        rusqlite::params![key, name, json(value)],
                    )?;
                }
            }
            report.records += 1;
            for (variant, download) in entry.downloads {
                planned.push(Planned {
                    key: key.clone(),
                    variant,
                    folder: download.folder,
                    files: download
                        .files
                        .iter()
                        .map(|file| {
                            let (path, size, md5) = file.parts();
                            (path.to_owned(), size, md5.map(str::to_owned))
                        })
                        .collect(),
                    bytes: download.bytes,
                    source_urls: download.source_urls,
                });
            }
        }
    }
    for (record, plan) in freesound_sounds(old) {
        if !dry_run {
            db.put_asset(&record)?;
        }
        report.records += 1;
        planned.push(plan);
    }
    for plan in planned {
        bring_across(old, dir, db, plan, dry_run, &mut report)?;
    }
    Ok(report)
}

/// Sounds saved by the pipeline's Freesound script, with their records.
fn freesound_sounds(old: &Path) -> Vec<(AssetRecord, Planned)> {
    let folder = old.join("audio").join("freesound");
    let Ok(entries) = fs::read_dir(&folder) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".json"))
        .collect();
    names.sort();
    let mut sounds = Vec::new();
    for name in names {
        let Ok(text) = fs::read_to_string(folder.join(&name)) else {
            continue;
        };
        let Ok(sidecar) = serde_json::from_str::<FreesoundSidecar>(&text) else {
            continue;
        };
        let id = match &sidecar.id {
            serde_json::Value::String(id) => id.clone(),
            other => other.to_string(),
        };
        let stem = name.trim_end_matches(".json");
        if !guard::safe_asset_id(&id) || stem != id {
            continue;
        }
        let ext = sidecar
            .file
            .rsplit_once('.')
            .map_or("mp3", |(_, ext)| ext)
            .to_ascii_lowercase();
        let file = format!("{id}.{ext}");
        let licence = license::normalize(&sidecar.licence);
        let version = sidecar
            .licence
            .split_whitespace()
            .find(|word| word.chars().next().is_some_and(|c| c.is_ascii_digit()))
            .unwrap_or("")
            .to_owned();
        let record = AssetRecord {
            provider: "freesound".into(),
            id: id.clone(),
            kind: Kind::Sound,
            name: sidecar.title.clone(),
            url: sidecar.page.clone(),
            authors: vec![sidecar.user.clone()]
                .into_iter()
                .filter(|u| !u.is_empty())
                .collect(),
            license: licence.into(),
            license_version: version,
            license_url: sidecar.licence_url.clone(),
            ..Default::default()
        };
        sounds.push((
            record,
            Planned {
                key: format!("freesound:{id}"),
                variant: ext.to_ascii_uppercase(),
                folder: "audio/freesound".into(),
                files: vec![(
                    format!("audio/freesound/{file}"),
                    Some(sidecar.bytes).filter(|b| *b > 0),
                    None,
                )],
                bytes: sidecar.bytes,
                source_urls: vec![sidecar.file],
            },
        ));
    }
    sounds
}

/// Checks one download's files where they are and, when every one is
/// there and right, copies them into the directory and records it.
fn bring_across(
    old: &Path,
    dir: &AssetDirectory,
    db: &Database,
    plan: Planned,
    dry_run: bool,
    report: &mut Report,
) -> Result<(), SourceError> {
    let entry = |reason: &str| Entry {
        key: plan.key.clone(),
        variant: plan.variant.clone(),
        reason: reason.to_owned(),
    };
    let Some(folder) = guard::safe_relative_path(&plan.folder) else {
        report
            .skipped
            .push(entry("its folder isn't inside the library"));
        return Ok(());
    };
    if plan.files.is_empty() {
        report.skipped.push(entry("it lists no files"));
        return Ok(());
    }
    // A sound sits in a shared folder, so it's kept in one of its own.
    let dest_folder = if folder == "audio/freesound" {
        let id = plan.key.trim_start_matches("freesound:");
        format!("audio/freesound/{id}/{}", plan.variant)
    } else {
        folder.clone()
    };
    if let Some(existing) = db.download(&plan.key, &plan.variant)?
        && is_present(dir, &existing)
    {
        report.present.push(entry("already in the directory"));
        return Ok(());
    }
    let mut stored = Vec::new();
    let mut total = 0;
    for (path, size, md5) in &plan.files {
        let Some(path) = guard::safe_relative_path(path) else {
            report
                .skipped
                .push(entry("a file's path leaves the library"));
            return Ok(());
        };
        let Some(inside) = path.strip_prefix(&format!("{folder}/")) else {
            report
                .skipped
                .push(entry("a file is outside its download's folder"));
            return Ok(());
        };
        let source = path.split('/').fold(old.to_owned(), |p, part| p.join(part));
        let Ok(meta) = fs::metadata(&source) else {
            report.skipped.push(entry(&format!("{inside} is missing")));
            return Ok(());
        };
        if !meta.is_file() || size.is_some_and(|size| size != meta.len()) {
            report
                .skipped
                .push(entry(&format!("{inside} has changed size")));
            return Ok(());
        }
        let md5 = match md5 {
            Some(expected) => {
                let actual = md5_of(&source)?;
                if !actual.eq_ignore_ascii_case(expected) {
                    report
                        .skipped
                        .push(entry(&format!("{inside} has changed (MD5)")));
                    return Ok(());
                }
                Some(actual)
            }
            None => None,
        };
        total += meta.len();
        let inside = if folder == "audio/freesound" {
            inside.rsplit('/').next().unwrap_or(inside).to_owned()
        } else {
            inside.to_owned()
        };
        stored.push((
            source,
            StoredFile {
                path: inside,
                size: meta.len(),
                md5,
            },
        ));
    }
    if plan.bytes > 0 && plan.bytes != total {
        report.skipped.push(entry(&format!(
            "its files add up to {total} bytes, not the {} recorded",
            plan.bytes
        )));
        return Ok(());
    }
    if !dry_run {
        let final_dir = dir.path(&dest_folder);
        let partial = dir.path(&format!("{dest_folder}.partial"));
        let _ = fs::remove_dir_all(&partial);
        for (source, file) in &stored {
            let dest = file
                .path
                .split('/')
                .fold(partial.clone(), |p, part| p.join(part));
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(source, &dest)?;
        }
        let _ = fs::remove_dir_all(&final_dir);
        if let Some(parent) = final_dir.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(&partial, &final_dir)?;
        db.put_download(&Download {
            key: plan.key.clone(),
            variant: plan.variant.clone(),
            folder: dest_folder,
            files: stored.into_iter().map(|(_, file)| file).collect(),
            bytes: total,
            source_urls: plan.source_urls.clone(),
            fetched: crate::db::now(),
        })?;
    }
    report.bytes += total;
    report.copied.push(entry("copied"));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("axcenity-migrate-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(root: &Path, relative: &str, bytes: &[u8]) {
        let path = relative
            .split('/')
            .fold(root.to_owned(), |p, part| p.join(part));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    /// A small library as the Blender pipeline writes it: one good texture,
    /// one with a missing map, one whose map changed size, a model with
    /// nested files, a Freesound sound and an unreadable record.
    fn old_library(name: &str) -> PathBuf {
        let old = temp(name);
        write(
            &old,
            "textures/ambientcg/Rock030/2k/Rock030_2K-JPG_Color.jpg",
            b"colour",
        );
        write(
            &old,
            "textures/ambientcg/Rock030/2k/Rock030_2K-JPG_NormalGL.jpg",
            b"nrm",
        );
        write(
            &old,
            "textures/polyhaven/brick/1k/brick_diff_1k.jpg",
            b"diff",
        );
        write(
            &old,
            "textures/polyhaven/tiles/1k/tiles_diff_1k.jpg",
            b"longer than recorded",
        );
        write(
            &old,
            "models/kenney/car-kit/pack/Models/GLB format/box.glb",
            b"glTFbox",
        );
        write(&old, "audio/freesound/338824.mp3", b"mp3data");
        write(
            &old,
            "audio/freesound/338824.json",
            br#"{"id": 338824, "user": "Garuda1982", "title": "gray woodpecker",
                "page": "https://freesound.org/people/Garuda1982/sounds/338824/",
                "file": "https://cdn.freesound.org/previews/338/338824_2061858-hq.mp3",
                "licence": "CC0 1.0", "licence_url": "https://creativecommons.org/publicdomain/zero/1.0/",
                "bytes": 7}"#,
        );
        let registry = serde_json::json!({
            "version": 1,
            "assets": {
                "ambientcg:Rock030": {
                    "record": {"provider": "ambientcg", "id": "Rock030", "kind": "texture",
                               "name": "Rock 030", "license": "CC0", "extra": {"downloads": {}},
                               "attributes": {}, "method": "PBRPhotogrammetry"},
                    "analysis": {"preview_colour": [0.4, 0.4, 0.4]},
                    "downloads": {"2k": {
                        "folder": "textures/ambientcg/Rock030/2k",
                        "files": ["textures/ambientcg/Rock030/2k/Rock030_2K-JPG_Color.jpg",
                                  "textures/ambientcg/Rock030/2k/Rock030_2K-JPG_NormalGL.jpg"],
                        "bytes": 9,
                        "source_urls": ["https://ambientcg.com/get?file=Rock030_2K-JPG.zip"]}}
                },
                "polyhaven:brick": {
                    "record": {"provider": "polyhaven", "id": "brick", "kind": "texture",
                               "name": "Brick", "license": "CC0"},
                    "downloads": {"1k": {
                        "folder": "textures/polyhaven/brick/1k",
                        "files": ["textures/polyhaven/brick/1k/brick_diff_1k.jpg",
                                  "textures/polyhaven/brick/1k/brick_nor_gl_1k.jpg"],
                        "bytes": 10}}
                },
                "polyhaven:tiles": {
                    "record": {"provider": "polyhaven", "id": "tiles", "kind": "texture",
                               "name": "Tiles", "license": "CC0"},
                    "downloads": {"1k": {
                        "folder": "textures/polyhaven/tiles/1k",
                        "files": [{"path": "textures/polyhaven/tiles/1k/tiles_diff_1k.jpg", "size": 4}],
                        "bytes": 4}}
                },
                "kenney:car-kit": {
                    "record": {"provider": "kenney", "id": "car-kit", "kind": "sprites",
                               "name": "Car Kit", "license": "CC0"},
                    "downloads": {"pack": {
                        "folder": "models/kenney/car-kit/pack",
                        "files": ["models/kenney/car-kit/pack/Models/GLB format/box.glb"],
                        "bytes": 7}}
                },
                "polyhaven:odd": {"record": {"provider": "polyhaven", "id": "odd", "kind": "spaceship"}}
            }
        });
        write(
            &old,
            "metadata/registry.json",
            serde_json::to_string(&registry).unwrap().as_bytes(),
        );
        old
    }

    fn keys(entries: &[Entry]) -> Vec<String> {
        entries
            .iter()
            .map(|entry| format!("{} {}", entry.key, entry.variant))
            .collect()
    }

    #[test]
    fn only_downloads_whose_files_verify_are_copied() {
        let old = old_library("old");
        let root = temp("dir");
        let dir = AssetDirectory::open(&root).unwrap();
        let db = Database::in_memory().unwrap();
        let report = import_blender_library(&old, &dir, &db, false).unwrap();
        assert_eq!(report.records, 5);
        assert_eq!(
            keys(&report.copied),
            [
                "ambientcg:Rock030 2k",
                "kenney:car-kit pack",
                "freesound:338824 MP3"
            ]
        );
        assert_eq!(
            keys(&report.skipped),
            ["polyhaven:odd ", "polyhaven:brick 1k", "polyhaven:tiles 1k"]
        );
        assert!(
            report.skipped[1]
                .reason
                .contains("brick_nor_gl_1k.jpg is missing")
        );
        assert!(report.skipped[2].reason.contains("changed size"));
        assert!(report.summary().contains("3 downloads copied"));

        // Copied at the same folder, recorded, and every record kept.
        assert!(
            root.join("textures/ambientcg/Rock030/2k/Rock030_2K-JPG_Color.jpg")
                .is_file()
        );
        assert!(
            root.join("models/kenney/car-kit/pack/Models/GLB format/box.glb")
                .is_file()
        );
        assert!(root.join("audio/freesound/338824/MP3/338824.mp3").is_file());
        let rock = db.download("ambientcg:Rock030", "2k").unwrap().unwrap();
        assert_eq!(rock.bytes, 9);
        assert_eq!(rock.files[0].path, "Rock030_2K-JPG_Color.jpg");
        assert!(db.asset("polyhaven:brick").unwrap().is_some());
        let sound = db.asset("freesound:338824").unwrap().unwrap();
        assert_eq!(
            (sound.license.as_str(), sound.license_version.as_str()),
            ("CC0", "1.0")
        );
        assert_eq!(sound.authors, ["Garuda1982"]);
        assert_eq!(
            db.asset("kenney:car-kit").unwrap().unwrap().kind,
            Kind::Sprites
        );

        // The old folder is untouched, and a second run copies nothing.
        assert!(
            old.join("textures/ambientcg/Rock030/2k/Rock030_2K-JPG_Color.jpg")
                .is_file()
        );
        let again = import_blender_library(&old, &dir, &db, false).unwrap();
        assert!(again.copied.is_empty());
        assert_eq!(again.present.len(), 3);
        let _ = fs::remove_dir_all(&old);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_dry_run_changes_nothing() {
        let old = old_library("old-dry");
        let root = temp("dry");
        let dir = AssetDirectory::open(&root).unwrap();
        let db = Database::in_memory().unwrap();
        let report = import_blender_library(&old, &dir, &db, true).unwrap();
        assert_eq!(report.copied.len(), 3);
        assert!(db.asset("ambientcg:Rock030").unwrap().is_none());
        assert!(!root.join("textures").exists());
        let _ = fs::remove_dir_all(&old);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_newer_or_damaged_registry_is_refused() {
        let old = temp("newer");
        write(
            &old,
            "metadata/registry.json",
            br#"{"version": 2, "assets": {}}"#,
        );
        let root = temp("newer-dir");
        let dir = AssetDirectory::open(&root).unwrap();
        let db = Database::in_memory().unwrap();
        assert!(import_blender_library(&old, &dir, &db, false).is_err());
        write(&old, "metadata/registry.json", b"{");
        assert!(import_blender_library(&old, &dir, &db, false).is_err());
        let _ = fs::remove_dir_all(&old);
        let _ = fs::remove_dir_all(&root);
    }
}
