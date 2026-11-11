//! Openverse audio (<https://openverse.org>): Creative Commons sounds and
//! music gathered from many sites (much of it Freesound's), through its
//! API (<https://api.openverse.org/v1/>).
//!
//! Ported from the Django Toolbox's `providers/openverse.py`. Used without
//! an account, which Openverse limits to 20 requests a minute and 200 a
//! day per client, 20 results a page and 240 results deep; a page past that
//! depth is empty without a request. Each sound has its own licence, which
//! a search can filter by. A full record lists the main file, then one
//! variant per other file type.

use serde_json::Value;

use crate::SourceError;
use crate::http::{self, Http};
use crate::json::{self, JSON_LIMIT};
use crate::license;
use crate::record::{AssetRecord, Kind, Package, RemoteFile, Variant};
use crate::source::{AssetSource, Licenses, Query, SearchPage, SourceInfo};

const API: &str = "https://api.openverse.org/v1/audio/";
/// The most results a page an anonymous client may ask for.
pub const MAX_PAGE_SIZE: u32 = 20;
/// How deep anonymous clients may page.
pub const MAX_DEPTH: u64 = 240;

/// The source.
pub struct Openverse;

/// The one instance, listed in [`sources`](crate::sources).
pub static OPENVERSE: Openverse = Openverse;

static INFO: SourceInfo = SourceInfo {
    key: "openverse",
    name: "Openverse",
    home: "https://openverse.org",
    credit: "Sounds from Openverse",
    kinds: &[Kind::Sound],
    licenses: Licenses::PerAsset,
    // Its files are on the sites it gathers from.
    download_hosts: None,
};

/// The search address, or `None` when the licence filter names nothing
/// Openverse knows (then nothing can match).
pub fn search_url(query: &Query) -> Option<String> {
    let page_size = query.per_page.clamp(1, MAX_PAGE_SIZE);
    let mut url = format!(
        "{API}?q={}&page_size={page_size}&page={}",
        http::encode_query(query.text.trim()),
        query.page.max(1)
    );
    if !query.licenses.is_empty() {
        let codes: Vec<&str> = query
            .licenses
            .iter()
            .filter_map(|code| license::to_openverse(code))
            .collect();
        if codes.is_empty() {
            return None;
        }
        url.push_str("&license=");
        url.push_str(&codes.join(","));
    }
    Some(url)
}

/// The full-record address for one id.
pub fn record_url(id: &str) -> String {
    format!("{API}{id}/")
}

impl AssetSource for Openverse {
    fn info(&self) -> &SourceInfo {
        &INFO
    }

    fn search(&self, http: &dyn Http, query: &Query) -> Result<SearchPage, SourceError> {
        if query.kind != Kind::Sound {
            return Ok(SearchPage::default());
        }
        let page_size = u64::from(query.per_page.clamp(1, MAX_PAGE_SIZE));
        let start = u64::from(query.page.max(1) - 1) * page_size;
        if start >= MAX_DEPTH {
            return Ok(SearchPage::default());
        }
        let Some(url) = search_url(query) else {
            return Ok(SearchPage::default());
        };
        let answer = json::parse(&http::get_text(http, &url, JSON_LIMIT)?)?;
        let results =
            answer
                .get("results")
                .and_then(Value::as_array)
                .ok_or(SourceError::BadResponse(
                    "Openverse sent back results the Toolbox couldn't read",
                ))?;
        let records: Vec<AssetRecord> = results.iter().filter_map(summary).collect();
        let total = json::count(answer.get("result_count"));
        let shown = start + results.len() as u64;
        let pages = json::count(answer.get("page_count")).unwrap_or(0);
        Ok(SearchPage {
            more: u64::from(query.page.max(1)) < pages && shown < MAX_DEPTH,
            total,
            records,
        })
    }

    fn record(&self, http: &dyn Http, id: &str) -> Result<AssetRecord, SourceError> {
        let id = id.trim();
        if !safe_id(id) {
            return Err(SourceError::NotFound);
        }
        let answer = json::parse(&http::get_text(http, &record_url(id), JSON_LIMIT)?)?;
        let mut record = summary(&answer).ok_or(SourceError::BadResponse(
            "Openverse sent back a sound the Toolbox couldn't read",
        ))?;
        record.variants = variants(&answer, &record.id);
        Ok(record)
    }
}

/// An Openverse id (a UUID): letters, digits and `-`.
fn safe_id(id: &str) -> bool {
    !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// A record from a search result or the full answer, without variants;
/// `None` without a usable id.
fn summary(item: &Value) -> Option<AssetRecord> {
    let id = json::text(item.get("id"));
    if !safe_id(&id) {
        return None;
    }
    let name = Some(json::text(item.get("title")))
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| id.clone());
    let creator = json::text(item.get("creator"));
    let preview = json::text(item.get("url"));
    Some(AssetRecord {
        provider: INFO.key.to_owned(),
        kind: Kind::Sound,
        license: license::from_openverse(&json::text(item.get("license"))).to_owned(),
        license_version: json::text(item.get("license_version")),
        license_url: json::text(item.get("license_url")),
        attribution: json::text(item.get("attribution")),
        authors: if creator.is_empty() {
            Vec::new()
        } else {
            vec![creator]
        },
        url: json::text(item.get("foreign_landing_url")),
        tags: json::names(item.get("tags")),
        categories: Some(json::text(item.get("category")))
            .filter(|category| !category.is_empty())
            .into_iter()
            .collect(),
        thumbnail_url: json::text(item.get("thumbnail")),
        preview_url: if preview.starts_with("https://") {
            preview
        } else {
            String::new()
        },
        name,
        id,
        ..AssetRecord::default()
    })
}

/// The main audio file, then each alternative file, one variant per file
/// type. A URL with no file name (Freesound's download links) gets
/// `<id>.<type>`.
fn variants(item: &Value, id: &str) -> Vec<Variant> {
    let alternatives = item.get("alt_files").and_then(Value::as_array);
    let entries = std::iter::once(item).chain(alternatives.into_iter().flatten());
    let mut variants: Vec<Variant> = Vec::new();
    for entry in entries {
        let url = json::text(entry.get("url"));
        if !INFO.allows_download_from(&url) {
            continue;
        }
        let format = json::text(entry.get("filetype")).to_ascii_lowercase();
        let label = if format.is_empty() {
            "Audio".to_owned()
        } else {
            format.to_ascii_uppercase()
        };
        if variants.iter().any(|known| known.label == label) {
            continue;
        }
        let fallback = format!("{id}.{}", if format.is_empty() { "audio" } else { &format });
        variants.push(Variant {
            label,
            resolution: None,
            package: Package::Single,
            files: vec![RemoteFile {
                path: json::file_name(&url, &fallback),
                size: json::count(entry.get("filesize")),
                url,
                ..RemoteFile::default()
            }],
            format,
        });
    }
    variants
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::Fixture;

    const SEARCH: &str = r#"{
        "result_count": 45, "page_count": 3,
        "results": [
            {"id": "4bd0a3e8-1111-2222-3333-444455556666", "title": "Forest Birds", "creator": "Jane Doe",
             "license": "by", "license_version": "4.0", "license_url": "https://creativecommons.org/licenses/by/4.0/",
             "attribution": "\"Forest Birds\" by Jane Doe is licensed under CC BY 4.0.",
             "foreign_landing_url": "https://freesound.org/s/1/", "url": "https://cdn.freesound.org/previews/1/birds.mp3",
             "category": "sound_effect", "tags": [{"name": "birds"}, {"name": "forest"}]},
            {"id": "bad id!", "title": "Bad"}
        ]
    }"#;

    fn query(text: &str) -> Query {
        let mut query = Query::new(text, Kind::Sound);
        query.per_page = 20;
        query
    }

    #[test]
    fn search_reads_each_sounds_own_licence() {
        let query = query("birds");
        let http = Fixture::default().with(&search_url(&query).unwrap(), SEARCH);
        let page = OPENVERSE.search(&http, &query).unwrap();
        assert_eq!(page.total, Some(45));
        assert!(page.more);
        assert_eq!(page.records.len(), 1);
        let record = &page.records[0];
        assert_eq!(
            record.key(),
            "openverse:4bd0a3e8-1111-2222-3333-444455556666"
        );
        assert_eq!(record.license, "CC-BY");
        assert_eq!(record.license_version, "4.0");
        assert_eq!(record.authors, ["Jane Doe"]);
        assert_eq!(record.tags, ["birds", "forest"]);
        assert_eq!(record.categories, ["sound_effect"]);
        assert_eq!(
            record.preview_url,
            "https://cdn.freesound.org/previews/1/birds.mp3"
        );
    }

    #[test]
    fn the_licence_filter_uses_openverses_codes() {
        let mut query = query("door");
        query.licenses = vec!["CC0".into(), "CC-BY".into(), "unknown".into()];
        assert_eq!(
            search_url(&query).unwrap(),
            "https://api.openverse.org/v1/audio/?q=door&page_size=20&page=1&license=cc0,by"
        );
        query.licenses = vec!["unknown".into()];
        assert_eq!(search_url(&query), None);
        assert!(
            OPENVERSE
                .search(&Fixture::default(), &query)
                .unwrap()
                .records
                .is_empty()
        );
    }

    #[test]
    fn pages_are_capped_at_twenty_and_nothing_is_asked_past_the_anonymous_depth() {
        let mut query = query("rain");
        query.per_page = 50;
        assert!(search_url(&query).unwrap().contains("page_size=20"));
        query.page = 13; // results 240 onward
        assert!(
            OPENVERSE
                .search(&Fixture::default(), &query)
                .unwrap()
                .records
                .is_empty()
        );
        assert!(
            OPENVERSE
                .search(&Fixture::default(), &Query::new("rain", Kind::Texture))
                .unwrap()
                .records
                .is_empty()
        );
    }

    #[test]
    fn a_record_lists_the_main_file_then_each_other_type() {
        let id = "4bd0a3e8-1111-2222-3333-444455556666";
        let answer = r#"{"id": "4bd0a3e8-1111-2222-3333-444455556666", "title": "Door", "license": "cc0",
            "url": "https://freesound.org/apiv2/sounds/1/download", "filetype": "wav", "filesize": 5000,
            "alt_files": [
                {"url": "https://cdn.freesound.org/previews/1/door.mp3", "filetype": "mp3", "filesize": 900},
                {"url": "https://cdn.freesound.org/other/door.wav", "filetype": "wav"},
                {"url": "http://insecure.example/door.ogg", "filetype": "ogg"}
            ]}"#;
        let http = Fixture::default().with(&record_url(id), answer);
        let record = OPENVERSE.record(&http, id).unwrap();
        assert_eq!(record.license, "CC0");
        let labels: Vec<_> = record.variants.iter().map(|v| v.label.as_str()).collect();
        assert_eq!(labels, ["WAV", "MP3"]);
        assert_eq!(record.variants[0].files[0].path, format!("{id}.wav"));
        assert_eq!(record.variants[1].files[0].path, "door.mp3");
        assert_eq!(record.variants[0].package, Package::Single);
    }

    #[test]
    fn unsafe_ids_are_refused_without_a_request() {
        assert!(matches!(
            OPENVERSE.record(&Fixture::default(), "../x"),
            Err(SourceError::NotFound)
        ));
    }
}
