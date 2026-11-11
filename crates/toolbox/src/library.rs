//! The Toolbox's library views over the metadata database: favourites,
//! collections, recently used assets, download history, listing what's
//! known, and totals. The tables are in schema version 1 (`db.rs`); this
//! adds their queries. Everything is per user, shared by every project,
//! the CLI and the Blender integration.

use rusqlite::{OptionalExtension, params};
use serde::Serialize;

use crate::SourceError;
use crate::db::{Database, Download, from_json, now};
use crate::record::{AssetRecord, Kind};

/// How many recently used assets are kept.
pub const RECENT_LIMIT: usize = 100;

/// One named list of assets.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Collection {
    pub name: String,
    pub items: Vec<CollectionItem>,
}

/// One asset in a collection, with the variant chosen for it, if any.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CollectionItem {
    pub key: String,
    pub variant: Option<String>,
}

/// What [`Database::assets`] lists.
#[derive(Clone, Debug, Default)]
pub struct Filter {
    pub kind: Option<Kind>,
    /// Words that must all appear in the name, key, tags or categories.
    pub text: String,
    /// Only assets with at least one download in the directory.
    pub downloaded: bool,
}

/// The library's totals, for `axcenity-toolbox stats`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Stats {
    pub assets: u64,
    pub downloads: u64,
    pub bytes: u64,
    pub installs: u64,
    pub favourites: u64,
    pub collections: u64,
}

fn read_download(row: &rusqlite::Row) -> rusqlite::Result<Download> {
    Ok(Download {
        key: row.get(0)?,
        variant: row.get(1)?,
        folder: row.get(2)?,
        files: from_json(&row.get::<_, String>(3)?),
        bytes: row.get::<_, i64>(4)?.max(0) as u64,
        source_urls: from_json(&row.get::<_, String>(5)?),
        fetched: row.get(6)?,
    })
}

const DOWNLOAD_COLUMNS: &str = "key, variant, folder, files, bytes, source_urls, fetched";

impl Database {
    /// Stars or un-stars an asset. Its record must be stored first.
    pub fn set_favourite(&self, key: &str, favourite: bool) -> Result<(), SourceError> {
        if favourite {
            self.conn.execute(
                "INSERT OR IGNORE INTO favourites (key, added) VALUES (?1, ?2)",
                params![key, now()],
            )?;
        } else {
            self.conn
                .execute("DELETE FROM favourites WHERE key = ?1", [key])?;
        }
        Ok(())
    }

    pub fn is_favourite(&self, key: &str) -> Result<bool, SourceError> {
        Ok(self
            .conn
            .query_row("SELECT 1 FROM favourites WHERE key = ?1", [key], |_| Ok(()))
            .optional()?
            .is_some())
    }

    /// Favourite assets, the latest starred first.
    pub fn favourites(&self) -> Result<Vec<AssetRecord>, SourceError> {
        self.records(
            "SELECT a.record FROM favourites f JOIN assets a ON a.key = f.key
             ORDER BY f.added DESC, f.rowid DESC",
            [],
        )
    }

    /// Marks an asset as just used (viewed, got or applied), keeping the
    /// last [`RECENT_LIMIT`].
    pub fn touch_recent(&self, key: &str) -> Result<(), SourceError> {
        // Seconds are coarse, so the order among equal times is by rowid,
        // which a replace renews.
        self.conn.execute(
            "INSERT OR REPLACE INTO recents (key, used) VALUES (?1, ?2)",
            params![key, now()],
        )?;
        self.conn.execute(
            "DELETE FROM recents WHERE key NOT IN
               (SELECT key FROM recents ORDER BY used DESC, rowid DESC LIMIT ?1)",
            [RECENT_LIMIT as i64],
        )?;
        Ok(())
    }

    /// Recently used assets, newest first.
    pub fn recents(&self) -> Result<Vec<AssetRecord>, SourceError> {
        self.records(
            "SELECT a.record FROM recents r JOIN assets a ON a.key = r.key
             ORDER BY r.used DESC, r.rowid DESC",
            [],
        )
    }

    /// Makes a collection called `name` if there isn't one. Names are
    /// trimmed and can't be empty.
    pub fn create_collection(&self, name: &str) -> Result<(), SourceError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(SourceError::Refused("a collection needs a name"));
        }
        self.conn.execute(
            "INSERT OR IGNORE INTO collections (name, created) VALUES (?1, ?2)",
            params![name, now()],
        )?;
        Ok(())
    }

    /// Deletes a collection (not its assets).
    pub fn delete_collection(&self, name: &str) -> Result<(), SourceError> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "DELETE FROM collection_items WHERE collection =
               (SELECT id FROM collections WHERE name = ?1)",
            [name.trim()],
        )?;
        tx.execute("DELETE FROM collections WHERE name = ?1", [name.trim()])?;
        tx.commit()?;
        Ok(())
    }

    /// Adds an asset to the end of a collection, making the collection if
    /// needed; adding it again changes only its variant.
    pub fn add_to_collection(
        &self,
        name: &str,
        key: &str,
        variant: Option<&str>,
    ) -> Result<(), SourceError> {
        self.create_collection(name)?;
        self.conn.execute(
            "INSERT INTO collection_items (collection, key, variant, position)
             SELECT c.id, ?2, ?3,
                    COALESCE((SELECT MAX(position) + 1 FROM collection_items WHERE collection = c.id), 0)
             FROM collections c WHERE c.name = ?1
             ON CONFLICT(collection, key) DO UPDATE SET variant = excluded.variant",
            params![name.trim(), key, variant],
        )?;
        Ok(())
    }

    pub fn remove_from_collection(&self, name: &str, key: &str) -> Result<(), SourceError> {
        self.conn.execute(
            "DELETE FROM collection_items WHERE key = ?2 AND collection =
               (SELECT id FROM collections WHERE name = ?1)",
            params![name.trim(), key],
        )?;
        Ok(())
    }

    /// One collection with its items in order.
    pub fn collection(&self, name: &str) -> Result<Option<Collection>, SourceError> {
        let id: Option<i64> = self
            .conn
            .query_row(
                "SELECT id FROM collections WHERE name = ?1",
                [name.trim()],
                |row| row.get(0),
            )
            .optional()?;
        let Some(id) = id else {
            return Ok(None);
        };
        let mut statement = self.conn.prepare(
            "SELECT key, variant FROM collection_items WHERE collection = ?1 ORDER BY position",
        )?;
        let items = statement
            .query_map([id], |row| {
                Ok(CollectionItem {
                    key: row.get(0)?,
                    variant: row.get(1)?,
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(Some(Collection {
            name: name.trim().to_owned(),
            items,
        }))
    }

    /// Every collection, by name.
    pub fn collections(&self) -> Result<Vec<Collection>, SourceError> {
        let mut statement = self
            .conn
            .prepare("SELECT name FROM collections ORDER BY name COLLATE NOCASE")?;
        let names: Vec<String> = statement
            .query_map([], |row| row.get(0))?
            .collect::<Result<_, _>>()?;
        let mut collections = Vec::new();
        for name in names {
            if let Some(collection) = self.collection(&name)? {
                collections.push(collection);
            }
        }
        Ok(collections)
    }

    /// Every download of one asset, newest first.
    pub fn downloads_of(&self, key: &str) -> Result<Vec<Download>, SourceError> {
        let mut statement = self.conn.prepare(&format!(
            "SELECT {DOWNLOAD_COLUMNS} FROM downloads WHERE key = ?1 ORDER BY fetched DESC, variant"
        ))?;
        let rows = statement.query_map([key], read_download)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// The download history: every download in the directory, newest first.
    pub fn history(&self) -> Result<Vec<Download>, SourceError> {
        let mut statement = self.conn.prepare(&format!(
            "SELECT {DOWNLOAD_COLUMNS} FROM downloads ORDER BY fetched DESC, key, variant"
        ))?;
        let rows = statement.query_map([], read_download)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Stored records matching `filter`, by name.
    pub fn assets(&self, filter: &Filter) -> Result<Vec<AssetRecord>, SourceError> {
        let downloaded = if filter.downloaded {
            "AND EXISTS (SELECT 1 FROM downloads d WHERE d.key = assets.key)"
        } else {
            ""
        };
        let kind = filter.kind.map(Kind::as_str).unwrap_or("");
        let records = self.records(
            &format!(
                "SELECT record FROM assets WHERE (?1 = '' OR kind = ?1) {downloaded}
                 ORDER BY name COLLATE NOCASE, key"
            ),
            [kind],
        )?;
        let words: Vec<String> = filter
            .text
            .split_whitespace()
            .map(str::to_lowercase)
            .collect();
        Ok(records
            .into_iter()
            .filter(|record| {
                let haystack = format!(
                    "{} {} {} {}",
                    record.key(),
                    record.name,
                    record.tags.join(" "),
                    record.categories.join(" ")
                )
                .to_lowercase();
                words.iter().all(|word| haystack.contains(word.as_str()))
            })
            .collect())
    }

    /// The records for some keys, in that order, leaving out unknown ones.
    pub fn assets_by_key(&self, keys: &[String]) -> Result<Vec<AssetRecord>, SourceError> {
        let mut records = Vec::new();
        for key in keys {
            if let Some(record) = self.asset(key)? {
                records.push(record);
            }
        }
        Ok(records)
    }

    /// The library's totals.
    pub fn stats(&self) -> Result<Stats, SourceError> {
        let count = |sql: &str| -> Result<u64, SourceError> {
            Ok(self
                .conn
                .query_row(sql, [], |row| row.get::<_, i64>(0))?
                .max(0) as u64)
        };
        Ok(Stats {
            assets: count("SELECT COUNT(*) FROM assets")?,
            downloads: count("SELECT COUNT(*) FROM downloads")?,
            bytes: count("SELECT COALESCE(SUM(bytes), 0) FROM downloads")?,
            installs: count("SELECT COUNT(*) FROM installs")?,
            favourites: count("SELECT COUNT(*) FROM favourites")?,
            collections: count("SELECT COUNT(*) FROM collections")?,
        })
    }

    fn records<P: rusqlite::Params>(
        &self,
        sql: &str,
        params: P,
    ) -> Result<Vec<AssetRecord>, SourceError> {
        let mut statement = self.conn.prepare(sql)?;
        let texts: Vec<String> = statement
            .query_map(params, |row| row.get(0))?
            .collect::<Result<_, _>>()?;
        Ok(texts
            .iter()
            .filter_map(|text| serde_json::from_str(text).ok())
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(provider: &str, id: &str, kind: Kind, name: &str) -> AssetRecord {
        AssetRecord {
            provider: provider.into(),
            id: id.into(),
            kind,
            name: name.into(),
            tags: vec!["wall".into()],
            license: "CC0".into(),
            ..Default::default()
        }
    }

    fn library() -> Database {
        let db = Database::in_memory().unwrap();
        db.put_asset(&asset(
            "polyhaven",
            "red_brick_03",
            Kind::Texture,
            "Red Brick 03",
        ))
        .unwrap();
        db.put_asset(&asset("ambientcg", "Rock030", Kind::Texture, "Rock 030"))
            .unwrap();
        db.put_asset(&asset("openverse", "abc", Kind::Sound, "Rain"))
            .unwrap();
        db
    }

    #[test]
    fn favourites_are_starred_listed_and_unstarred() {
        let db = library();
        db.set_favourite("polyhaven:red_brick_03", true).unwrap();
        db.set_favourite("openverse:abc", true).unwrap();
        db.set_favourite("openverse:abc", true).unwrap();
        let keys: Vec<String> = db
            .favourites()
            .unwrap()
            .iter()
            .map(AssetRecord::key)
            .collect();
        assert_eq!(keys, ["openverse:abc", "polyhaven:red_brick_03"]);
        assert!(db.is_favourite("openverse:abc").unwrap());
        db.set_favourite("openverse:abc", false).unwrap();
        assert!(!db.is_favourite("openverse:abc").unwrap());
        assert!(db.set_favourite("nobody:knows", true).is_err());
    }

    #[test]
    fn recents_put_the_last_used_first_and_keep_a_hundred() {
        let db = library();
        db.touch_recent("polyhaven:red_brick_03").unwrap();
        db.touch_recent("ambientcg:Rock030").unwrap();
        db.touch_recent("polyhaven:red_brick_03").unwrap();
        let keys: Vec<String> = db.recents().unwrap().iter().map(AssetRecord::key).collect();
        assert_eq!(keys, ["polyhaven:red_brick_03", "ambientcg:Rock030"]);
        for n in 0..RECENT_LIMIT + 5 {
            let id = format!("a{n}");
            db.put_asset(&asset("local", &id, Kind::Model, &id))
                .unwrap();
            db.touch_recent(&format!("local:{id}")).unwrap();
        }
        let recents = db.recents().unwrap();
        assert_eq!(recents.len(), RECENT_LIMIT);
        assert_eq!(recents[0].key(), format!("local:a{}", RECENT_LIMIT + 4));
    }

    #[test]
    fn collections_keep_their_order_and_chosen_variants() {
        let db = library();
        db.add_to_collection("Castle", "polyhaven:red_brick_03", Some("2K JPG"))
            .unwrap();
        db.add_to_collection("Castle", "openverse:abc", None)
            .unwrap();
        db.add_to_collection(" Castle ", "polyhaven:red_brick_03", Some("4K PNG"))
            .unwrap();
        db.create_collection("Forest").unwrap();
        let castle = db.collection("Castle").unwrap().unwrap();
        assert_eq!(
            castle.items,
            [
                CollectionItem {
                    key: "polyhaven:red_brick_03".into(),
                    variant: Some("4K PNG".into())
                },
                CollectionItem {
                    key: "openverse:abc".into(),
                    variant: None
                },
            ]
        );
        let names: Vec<String> = db
            .collections()
            .unwrap()
            .into_iter()
            .map(|c| c.name)
            .collect();
        assert_eq!(names, ["Castle", "Forest"]);
        db.remove_from_collection("Castle", "openverse:abc")
            .unwrap();
        assert_eq!(db.collection("Castle").unwrap().unwrap().items.len(), 1);
        db.delete_collection("Castle").unwrap();
        assert_eq!(db.collection("Castle").unwrap(), None);
        assert!(db.create_collection("  ").is_err());
        assert!(db.asset("polyhaven:red_brick_03").unwrap().is_some());
    }

    #[test]
    fn assets_are_listed_by_kind_words_and_downloads() {
        let db = library();
        let all = db.assets(&Filter::default()).unwrap();
        assert_eq!(all.len(), 3);
        let textures = db
            .assets(&Filter {
                kind: Some(Kind::Texture),
                text: "BRICK wall".into(),
                ..Filter::default()
            })
            .unwrap();
        assert_eq!(textures.len(), 1);
        assert_eq!(textures[0].id, "red_brick_03");
        db.put_download(&Download {
            key: "openverse:abc".into(),
            variant: "MP3".into(),
            folder: "audio/openverse/abc/MP3".into(),
            files: vec![],
            bytes: 40,
            source_urls: vec![],
            fetched: 5,
        })
        .unwrap();
        let downloaded = db
            .assets(&Filter {
                downloaded: true,
                ..Filter::default()
            })
            .unwrap();
        assert_eq!(downloaded.len(), 1);
        assert_eq!(db.history().unwrap().len(), 1);
        assert_eq!(db.downloads_of("openverse:abc").unwrap()[0].bytes, 40);
        let stats = db.stats().unwrap();
        assert_eq!(
            stats,
            Stats {
                assets: 3,
                downloads: 1,
                bytes: 40,
                ..Stats::default()
            }
        );
        let keys = ["openverse:abc".to_owned(), "x:y".to_owned()];
        assert_eq!(db.assets_by_key(&keys).unwrap().len(), 1);
    }
}
