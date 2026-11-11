//! Where a project's files came from, and the credits that follow.
//!
//! The metadata database is the source of truth, but it's per machine, so
//! each project also keeps a snapshot, `.axcenity/asset-sources.json`: for
//! each file the Toolbox installed, its path, its `ax://` ID when it has
//! one, and the asset's source and licence record. It's rewritten after
//! every install and on Build Game ([`refresh`]); opening the project on
//! another machine adds the records its database lacks ([`adopt`]). Build
//! Game's `CREDITS.txt` comes from it ([`credits`]).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::SourceError;
use crate::db::{Database, Install};
use crate::license;
use crate::record::AssetRecord;

/// The snapshot's format name and version.
pub const FORMAT: &str = "axcenity-asset-sources";
pub const VERSION: u32 = 1;

/// The snapshot's path inside a project.
pub fn snapshot_path(project: &Path) -> PathBuf {
    project.join(".axcenity").join("asset-sources.json")
}

/// One project file that came from the Toolbox.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourcedFile {
    /// Relative to the project, `/` separators: a file, or a folder for a
    /// texture set or a model.
    pub path: String,
    /// `provider:id`.
    pub key: String,
    #[serde(default)]
    pub variant: String,
    /// Its `ax://` ID, when it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uid: Option<String>,
}

/// A project's `.axcenity/asset-sources.json`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub format: String,
    pub version: u32,
    pub files: Vec<SourcedFile>,
    /// Each asset's record, without its variants, by key.
    pub assets: BTreeMap<String, AssetRecord>,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            format: FORMAT.to_owned(),
            version: VERSION,
            files: Vec::new(),
            assets: BTreeMap::new(),
        }
    }
}

impl Snapshot {
    /// The project's snapshot; empty when there is none. A damaged or newer
    /// one is an error, so it isn't overwritten by mistake.
    pub fn read(project: &Path) -> Result<Self, SourceError> {
        let path = snapshot_path(project);
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(err) => return Err(err.into()),
        };
        let snapshot: Self = serde_json::from_str(&text)
            .map_err(|_| SourceError::Refused("the project's asset-sources.json is damaged"))?;
        if snapshot.format != FORMAT || snapshot.version > VERSION {
            return Err(SourceError::Refused(
                "the project's asset-sources.json is from a newer Axcenity",
            ));
        }
        Ok(snapshot)
    }

    pub fn write(&self, project: &Path) -> Result<(), SourceError> {
        let path = snapshot_path(project);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(self)
            .map_err(|_| SourceError::Refused("the asset sources could not be written"))?;
        let part = path.with_extension("json.part");
        fs::write(&part, text + "\n")?;
        fs::rename(&part, &path)?;
        Ok(())
    }

    /// The record of the asset a project file came from: the file itself, or
    /// the folder it's in (a texture set's maps, a model's textures).
    pub fn source_of(&self, path: &str) -> Option<&AssetRecord> {
        let path = path.trim_start_matches("./");
        self.files
            .iter()
            .filter(|file| {
                path == file.path
                    || path
                        .strip_prefix(file.path.as_str())
                        .is_some_and(|rest| rest.starts_with('/'))
            })
            .max_by_key(|file| file.path.len())
            .and_then(|file| self.assets.get(&file.key))
    }

    /// The records of every asset still in the project, by name.
    pub fn records(&self) -> Vec<&AssetRecord> {
        let mut keys: Vec<&str> = self.files.iter().map(|file| file.key.as_str()).collect();
        keys.sort_unstable();
        keys.dedup();
        let mut records: Vec<&AssetRecord> = keys
            .iter()
            .filter_map(|key| self.assets.get(*key))
            .collect();
        records.sort_by_key(|record| record.name.to_lowercase());
        records
    }
}

/// The key the database files a project's installs under.
pub fn project_key(project: &Path) -> String {
    project.display().to_string()
}

/// Rewrites the project's snapshot: what the database says was installed
/// into this project, plus what the snapshot already knew (from another
/// machine), leaving out files that are no longer in the project.
pub fn refresh(db: &Database, project: &Path) -> Result<Snapshot, SourceError> {
    let mut snapshot = Snapshot::read(project)?;
    let installs: Vec<Install> = db.installs(&project_key(project))?;
    for install in installs {
        let file = SourcedFile {
            path: install.path.clone(),
            key: install.key.clone(),
            variant: install.variant.clone(),
            uid: install.uid.clone(),
        };
        match snapshot.files.iter_mut().find(|f| f.path == install.path) {
            Some(existing) => *existing = file,
            None => snapshot.files.push(file),
        }
        if let Some(mut record) = db.asset(&install.key)? {
            record.variants.clear();
            snapshot.assets.insert(install.key, record);
        }
    }
    snapshot.files.retain(|file| {
        file.path
            .split('/')
            .fold(project.to_owned(), |path, part| path.join(part))
            .exists()
    });
    snapshot.files.sort_by(|a, b| a.path.cmp(&b.path));
    let used: Vec<String> = snapshot.files.iter().map(|f| f.key.clone()).collect();
    snapshot.assets.retain(|key, _| used.contains(key));
    snapshot.write(project)?;
    Ok(snapshot)
}

/// Adds the snapshot's records and installs to this machine's database
/// where it lacks them. Returns how many records were new.
pub fn adopt(db: &Database, project: &Path, snapshot: &Snapshot) -> Result<usize, SourceError> {
    let mut added = 0;
    for (key, record) in &snapshot.assets {
        if db.asset(key)?.is_none() {
            db.put_asset(record)?;
            added += 1;
        }
    }
    let known: Vec<String> = db
        .installs(&project_key(project))?
        .into_iter()
        .map(|install| install.path)
        .collect();
    for file in &snapshot.files {
        if !known.contains(&file.path) && snapshot.assets.contains_key(&file.key) {
            db.record_install(&Install {
                project: project_key(project),
                path: file.path.clone(),
                key: file.key.clone(),
                variant: file.variant.clone(),
                uid: file.uid.clone(),
                installed_at: crate::db::now(),
            })?;
        }
    }
    Ok(added)
}

/// A source's name for credit lines: "Poly Haven" for `polyhaven`.
pub fn source_name(provider: &str) -> String {
    match crate::source(provider) {
        Some(source) => source.info().name.to_owned(),
        None => match provider {
            "kenney" => "Kenney".to_owned(),
            "sketchfab" => "Sketchfab".to_owned(),
            "freesound" => "Freesound".to_owned(),
            "local" | "catalog" | "" => String::new(),
            other => other.to_owned(),
        },
    }
}

/// `CREDITS.txt` for these assets: the credits a licence asks for first,
/// then thanks for the public-domain ones, then any asset whose licence
/// blocks a commercial game. `None` when there are no assets.
pub fn credits(game: &str, records: &[&AssetRecord]) -> Option<String> {
    if records.is_empty() {
        return None;
    }
    let line = |record: &&AssetRecord| {
        format!(
            "- {}",
            license::credit_line(record, &source_name(&record.provider))
        )
    };
    let required: Vec<String> = records
        .iter()
        .filter(|record| license::needs_credit(license::normalize(&record.license)))
        .map(line)
        .collect();
    let thanks: Vec<String> = records
        .iter()
        .filter(|record| !license::needs_credit(license::normalize(&record.license)))
        .map(line)
        .collect();
    let blocked: Vec<String> = records
        .iter()
        .filter_map(|record| {
            license::commercial_block(&record.license)
                .map(|why| format!("- {}: {why}", record.name))
        })
        .collect();
    let mut text = format!("{game}: credits\n\nThird-party assets in this game.\n");
    if !required.is_empty() {
        text.push_str("\nCredits\n\n");
        text.push_str(&required.join("\n"));
        text.push('\n');
    }
    if !thanks.is_empty() {
        text.push_str("\nWith thanks (public domain, no credit required)\n\n");
        text.push_str(&thanks.join("\n"));
        text.push('\n');
    }
    if !blocked.is_empty() {
        text.push_str("\nNot cleared for a commercial game\n\n");
        text.push_str(&blocked.join("\n"));
        text.push('\n');
    }
    Some(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::Kind;

    fn temp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("axcenity-provenance-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn record(provider: &str, id: &str, name: &str, license: &str) -> AssetRecord {
        AssetRecord {
            provider: provider.into(),
            id: id.into(),
            kind: Kind::Texture,
            name: name.into(),
            authors: vec!["Ann".into()],
            license: license.into(),
            url: format!("https://example.com/{id}"),
            ..Default::default()
        }
    }

    fn installed(db: &Database, project: &Path, path: &str, record: &AssetRecord) {
        db.put_asset(record).unwrap();
        let file = path
            .split('/')
            .fold(project.to_owned(), |dir, part| dir.join(part));
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, "x").unwrap();
        db.record_install(&Install {
            project: project_key(project),
            path: path.into(),
            key: record.key(),
            variant: "2K JPG".into(),
            uid: Some("ax://1".into()),
            installed_at: 1,
        })
        .unwrap();
    }

    #[test]
    fn the_snapshot_follows_installs_and_drops_removed_files() {
        let project = temp("refresh");
        let db = Database::in_memory().unwrap();
        let brick = record("polyhaven", "brick", "Brick", "CC0");
        let rain = record("openverse", "rain", "Rain", "CC-BY");
        installed(&db, &project, "assets/textures/brick", &brick);
        installed(&db, &project, "assets/audio/rain.mp3", &rain);
        let snapshot = refresh(&db, &project).unwrap();
        assert_eq!(snapshot.files.len(), 2);
        assert_eq!(Snapshot::read(&project).unwrap(), snapshot);
        assert_eq!(
            snapshot.source_of("assets/textures/brick/brick_diff_2k.jpg"),
            Some(&brick)
        );
        assert_eq!(snapshot.source_of("assets/textures/bricks"), None);

        fs::remove_file(project.join("assets/audio/rain.mp3")).unwrap();
        let snapshot = refresh(&db, &project).unwrap();
        assert_eq!(snapshot.files.len(), 1);
        assert!(!snapshot.assets.contains_key("openverse:rain"));
        let _ = fs::remove_dir_all(&project);
    }

    #[test]
    fn another_machine_adopts_the_snapshots_records() {
        let project = temp("adopt");
        let first = Database::in_memory().unwrap();
        installed(
            &first,
            &project,
            "assets/sky/sunset_2k.hdr",
            &record("polyhaven", "sunset", "Sunset", "CC0"),
        );
        let snapshot = refresh(&first, &project).unwrap();
        let second = Database::in_memory().unwrap();
        assert_eq!(adopt(&second, &project, &snapshot).unwrap(), 1);
        assert_eq!(adopt(&second, &project, &snapshot).unwrap(), 0);
        assert!(second.asset("polyhaven:sunset").unwrap().is_some());
        assert_eq!(second.installs(&project_key(&project)).unwrap().len(), 1);
        // And refreshing there keeps what the first machine installed.
        assert_eq!(refresh(&second, &project).unwrap().files.len(), 1);
        let _ = fs::remove_dir_all(&project);
    }

    #[test]
    fn a_damaged_snapshot_is_not_overwritten() {
        let project = temp("damaged");
        fs::create_dir_all(project.join(".axcenity")).unwrap();
        fs::write(snapshot_path(&project), "{oops").unwrap();
        let db = Database::in_memory().unwrap();
        assert!(refresh(&db, &project).is_err());
        assert_eq!(
            fs::read_to_string(snapshot_path(&project)).unwrap(),
            "{oops"
        );
        let _ = fs::remove_dir_all(&project);
    }

    #[test]
    fn credits_put_required_lines_first_and_flag_non_commercial_assets() {
        let by = record("openverse", "rain", "Rain", "CC-BY");
        let cc0 = record("polyhaven", "brick", "Brick", "CC0");
        let nc = record("openverse", "song", "Song", "CC-BY-NC");
        let text = credits("Farm", &[&cc0, &by, &nc]).unwrap();
        let required = text.find("\nCredits\n").unwrap();
        let thanks = text.find("With thanks").unwrap();
        let blocked = text.find("Not cleared").unwrap();
        assert!(required < thanks && thanks < blocked);
        assert!(
            text.contains("- \"Rain\" by Ann, from Openverse, CC BY (https://example.com/rain)")
        );
        assert!(text.contains("- \"Brick\" by Ann, from Poly Haven, CC0"));
        assert!(text.contains("- Song: its licence is non-commercial"));
        let only_cc0 = credits("Farm", &[&cc0]).unwrap();
        assert!(!only_cc0.contains("\nCredits\n") && only_cc0.contains("With thanks"));
        assert_eq!(credits("Farm", &[]), None);
    }
}
