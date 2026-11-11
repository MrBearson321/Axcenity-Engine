//! The metadata database, `metadata.db` in the asset directory: the
//! persistent per-asset source and licence records, every download, and
//! which project files came from which asset.
//!
//! SQLite in WAL mode, so the editor and the Blender integration (Python's
//! built-in `sqlite3`) can use it at once. A row in `assets` is never
//! deleted when its files are, so a project's credits can always be
//! answered. Times are whole seconds since 1970, UTC. The schema is
//! versioned in `schema_version`; [`Database::open`] brings an older one
//! forward and refuses a newer one.

use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::SourceError;
use crate::record::AssetRecord;

/// The schema this build reads and writes.
pub const SCHEMA_VERSION: i64 = 1;

const SCHEMA_V1: &str = "
CREATE TABLE assets (
    key TEXT PRIMARY KEY,
    provider TEXT NOT NULL,
    id TEXT NOT NULL,
    kind TEXT NOT NULL,
    name TEXT NOT NULL,
    url TEXT NOT NULL,
    authors TEXT NOT NULL,          -- JSON list
    license TEXT NOT NULL,
    license_version TEXT NOT NULL,
    license_url TEXT NOT NULL,
    attribution TEXT NOT NULL,
    record TEXT NOT NULL,           -- the full asset-source record, JSON
    first_seen INTEGER NOT NULL,
    updated INTEGER NOT NULL
);
CREATE TABLE downloads (
    key TEXT NOT NULL REFERENCES assets(key),
    variant TEXT NOT NULL,          -- its label, such as '2K JPG'
    folder TEXT NOT NULL,           -- relative to the asset directory
    files TEXT NOT NULL,            -- JSON list of {path, size, md5}
    bytes INTEGER NOT NULL,
    source_urls TEXT NOT NULL,      -- JSON list
    fetched INTEGER NOT NULL,
    PRIMARY KEY (key, variant)
);
CREATE TABLE installs (
    project TEXT NOT NULL,          -- the project folder
    path TEXT NOT NULL,             -- relative to the project
    key TEXT NOT NULL REFERENCES assets(key),
    variant TEXT NOT NULL,
    uid TEXT,                       -- the file's ax:// ID, when it has one
    installed_at INTEGER NOT NULL,
    PRIMARY KEY (project, path)
);
CREATE TABLE favourites (
    key TEXT PRIMARY KEY REFERENCES assets(key),
    added INTEGER NOT NULL
);
CREATE TABLE collections (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    created INTEGER NOT NULL
);
CREATE TABLE collection_items (
    collection INTEGER NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
    key TEXT NOT NULL REFERENCES assets(key),
    variant TEXT,
    position INTEGER NOT NULL,
    PRIMARY KEY (collection, key)
);
CREATE TABLE recents (
    key TEXT PRIMARY KEY REFERENCES assets(key),
    used INTEGER NOT NULL
);
CREATE TABLE analysis (
    key TEXT NOT NULL REFERENCES assets(key),
    name TEXT NOT NULL,
    value TEXT NOT NULL,            -- JSON
    PRIMARY KEY (key, name)
);
";

/// One downloaded file, as the database keeps it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredFile {
    /// Relative to the download's folder, `/` separators.
    pub path: String,
    pub size: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub md5: Option<String>,
}

/// One variant of one asset in the asset directory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Download {
    pub key: String,
    pub variant: String,
    /// Relative to the asset directory, `/` separators.
    pub folder: String,
    pub files: Vec<StoredFile>,
    pub bytes: u64,
    pub source_urls: Vec<String>,
    pub fetched: i64,
}

/// One project file that came from an asset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Install {
    pub project: String,
    pub path: String,
    pub key: String,
    pub variant: String,
    pub uid: Option<String>,
    pub installed_at: i64,
}

/// An open metadata database.
pub struct Database {
    conn: Connection,
}

/// Seconds since 1970, UTC.
pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |time| time.as_secs() as i64)
}

fn json<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".to_owned())
}

fn from_json<T: for<'a> Deserialize<'a> + Default>(text: &str) -> T {
    serde_json::from_str(text).unwrap_or_default()
}

impl Database {
    /// Opens (or creates) the database at `path` and brings its schema up
    /// to date. A schema newer than this build knows is refused, so an old
    /// editor can't damage a newer database.
    pub fn open(path: &Path) -> Result<Self, SourceError> {
        let conn = Connection::open(path)?;
        conn.busy_timeout(Duration::from_secs(10))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", true)?;
        let db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    /// An empty database in memory, for tests.
    pub fn in_memory() -> Result<Self, SourceError> {
        let conn = Connection::open_in_memory()?;
        conn.pragma_update(None, "foreign_keys", true)?;
        let db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> Result<(), SourceError> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL);",
        )?;
        let version: Option<i64> =
            self.conn
                .query_row("SELECT MAX(version) FROM schema_version", [], |row| {
                    row.get(0)
                })?;
        match version.unwrap_or(0) {
            0 => {
                let tx = self.conn.unchecked_transaction()?;
                tx.execute_batch(SCHEMA_V1)?;
                tx.execute(
                    "INSERT INTO schema_version (version) VALUES (?1)",
                    [SCHEMA_VERSION],
                )?;
                tx.commit()?;
                Ok(())
            }
            SCHEMA_VERSION => Ok(()),
            _ => Err(SourceError::Refused(
                "the asset database is from a newer Axcenity; update the editor to use it",
            )),
        }
    }

    /// Stores an asset's record, keeping when it was first seen. A record
    /// with variants replaces one without; one without (a search result)
    /// keeps the variants already stored.
    pub fn put_asset(&self, record: &AssetRecord) -> Result<(), SourceError> {
        let mut record = record.clone();
        if record.variants.is_empty()
            && let Some(stored) = self.asset(&record.key())?
        {
            record.variants = stored.variants;
        }
        let now = now();
        self.conn.execute(
            "INSERT INTO assets (key, provider, id, kind, name, url, authors, license, license_version,
                                 license_url, attribution, record, first_seen, updated)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?13)
             ON CONFLICT(key) DO UPDATE SET
                 kind = excluded.kind, name = excluded.name, url = excluded.url,
                 authors = excluded.authors, license = excluded.license,
                 license_version = excluded.license_version, license_url = excluded.license_url,
                 attribution = excluded.attribution, record = excluded.record,
                 updated = excluded.updated",
            params![
                record.key(),
                record.provider,
                record.id,
                record.kind.as_str(),
                record.name,
                record.url,
                json(&record.authors),
                record.license,
                record.license_version,
                record.license_url,
                record.attribution,
                json(&record),
                now,
            ],
        )?;
        Ok(())
    }

    /// The stored record for `provider:id`.
    pub fn asset(&self, key: &str) -> Result<Option<AssetRecord>, SourceError> {
        let text: Option<String> = self
            .conn
            .query_row("SELECT record FROM assets WHERE key = ?1", [key], |row| {
                row.get(0)
            })
            .optional()?;
        Ok(text.and_then(|text| serde_json::from_str(&text).ok()))
    }

    /// Records a download of one variant, replacing an older one.
    pub fn put_download(&self, download: &Download) -> Result<(), SourceError> {
        self.conn.execute(
            "INSERT OR REPLACE INTO downloads (key, variant, folder, files, bytes, source_urls, fetched)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                download.key,
                download.variant,
                download.folder,
                json(&download.files),
                download.bytes as i64,
                json(&download.source_urls),
                download.fetched,
            ],
        )?;
        Ok(())
    }

    /// The download of one variant, if there is one.
    pub fn download(&self, key: &str, variant: &str) -> Result<Option<Download>, SourceError> {
        Ok(self
            .conn
            .query_row(
                "SELECT key, variant, folder, files, bytes, source_urls, fetched
                 FROM downloads WHERE key = ?1 AND variant = ?2",
                [key, variant],
                |row| {
                    Ok(Download {
                        key: row.get(0)?,
                        variant: row.get(1)?,
                        folder: row.get(2)?,
                        files: from_json(&row.get::<_, String>(3)?),
                        bytes: row.get::<_, i64>(4)?.max(0) as u64,
                        source_urls: from_json(&row.get::<_, String>(5)?),
                        fetched: row.get(6)?,
                    })
                },
            )
            .optional()?)
    }

    /// Forgets a download (its files are gone). The asset's record stays.
    pub fn forget_download(&self, key: &str, variant: &str) -> Result<(), SourceError> {
        self.conn.execute(
            "DELETE FROM downloads WHERE key = ?1 AND variant = ?2",
            [key, variant],
        )?;
        Ok(())
    }

    /// Records that a project file came from an asset, replacing what that
    /// path came from before.
    pub fn record_install(&self, install: &Install) -> Result<(), SourceError> {
        self.conn.execute(
            "INSERT OR REPLACE INTO installs (project, path, key, variant, uid, installed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                install.project,
                install.path,
                install.key,
                install.variant,
                install.uid,
                install.installed_at,
            ],
        )?;
        Ok(())
    }

    /// Everything installed into one project, by path.
    pub fn installs(&self, project: &str) -> Result<Vec<Install>, SourceError> {
        let mut statement = self.conn.prepare(
            "SELECT project, path, key, variant, uid, installed_at FROM installs
             WHERE project = ?1 ORDER BY path",
        )?;
        let rows = statement.query_map([project], |row| {
            Ok(Install {
                project: row.get(0)?,
                path: row.get(1)?,
                key: row.get(2)?,
                variant: row.get(3)?,
                uid: row.get(4)?,
                installed_at: row.get(5)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::{Kind, Package, RemoteFile, Variant};

    fn record() -> AssetRecord {
        AssetRecord {
            provider: "polyhaven".into(),
            id: "red_brick_03".into(),
            kind: Kind::Texture,
            name: "Red Brick 03".into(),
            authors: vec!["Rob Tuytel".into()],
            license: "CC0".into(),
            variants: vec![Variant {
                label: "2K JPG".into(),
                resolution: Some("2k".into()),
                format: "jpg".into(),
                package: Package::Maps,
                files: vec![RemoteFile::default()],
            }],
            ..AssetRecord::default()
        }
    }

    fn download() -> Download {
        Download {
            key: "polyhaven:red_brick_03".into(),
            variant: "2K JPG".into(),
            folder: "textures/polyhaven/red_brick_03/2K-JPG".into(),
            files: vec![StoredFile {
                path: "red_brick_03_diff_2k.jpg".into(),
                size: 10,
                md5: Some("abc".into()),
            }],
            bytes: 10,
            source_urls: vec!["https://dl.polyhaven.org/x.jpg".into()],
            fetched: 7,
        }
    }

    #[test]
    fn a_record_round_trips_and_a_search_result_keeps_its_variants() {
        let db = Database::in_memory().unwrap();
        db.put_asset(&record()).unwrap();
        assert_eq!(db.asset("polyhaven:red_brick_03").unwrap(), Some(record()));
        let mut summary = record();
        summary.variants.clear();
        summary.name = "Red Brick 03 (renamed)".into();
        db.put_asset(&summary).unwrap();
        let stored = db.asset("polyhaven:red_brick_03").unwrap().unwrap();
        assert_eq!(stored.name, "Red Brick 03 (renamed)");
        assert_eq!(stored.variants.len(), 1);
        assert_eq!(db.asset("polyhaven:missing").unwrap(), None);
    }

    #[test]
    fn downloads_and_installs_are_kept_and_the_record_outlives_its_files() {
        let db = Database::in_memory().unwrap();
        db.put_asset(&record()).unwrap();
        db.put_download(&download()).unwrap();
        assert_eq!(
            db.download("polyhaven:red_brick_03", "2K JPG").unwrap(),
            Some(download())
        );
        let install = Install {
            project: "C:/Games/Farm".into(),
            path: "assets/textures/red_brick_03".into(),
            key: "polyhaven:red_brick_03".into(),
            variant: "2K JPG".into(),
            uid: None,
            installed_at: 9,
        };
        db.record_install(&install).unwrap();
        assert_eq!(db.installs("C:/Games/Farm").unwrap(), [install]);
        db.forget_download("polyhaven:red_brick_03", "2K JPG")
            .unwrap();
        assert_eq!(
            db.download("polyhaven:red_brick_03", "2K JPG").unwrap(),
            None
        );
        assert!(db.asset("polyhaven:red_brick_03").unwrap().is_some());
    }

    #[test]
    fn a_download_needs_its_asset() {
        let db = Database::in_memory().unwrap();
        assert!(db.put_download(&download()).is_err());
    }

    #[test]
    fn a_file_database_reopens_and_refuses_a_newer_schema() {
        let path =
            std::env::temp_dir().join(format!("axcenity-toolbox-db-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        {
            let db = Database::open(&path).unwrap();
            db.put_asset(&record()).unwrap();
        }
        let db = Database::open(&path).unwrap();
        assert!(db.asset("polyhaven:red_brick_03").unwrap().is_some());
        db.conn
            .execute("INSERT INTO schema_version (version) VALUES (99)", [])
            .unwrap();
        drop(db);
        assert!(matches!(
            Database::open(&path),
            Err(SourceError::Refused(_))
        ));
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
        }
    }
}
