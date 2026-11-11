//! ambientCG (<https://ambientcg.com>): CC0 materials, HDRIs and a few
//! models, through its v3 API (<https://docs.ambientcg.com/api/v3/>).
//!
//! Ported from the Django Toolbox's `providers/ambientcg.py`. Search is
//! `GET /api/v3/assets` with `offset` paging, and asks for each result's
//! downloads too, so its records already carry their variants; a full
//! record asks for one asset by `id`. Each download is one zip (or file)
//! per resolution and format ("2K-JPG"). ambientCG answers an unknown id with
//! an empty list, not 404. It's run by one person, so callers should cache.

use serde_json::Value;

use crate::http::{self, Http};
use crate::json::{self, JSON_LIMIT};
use crate::license::{CC0, CC0_URL};
use crate::record::{AssetRecord, Kind, Package, RemoteFile, Variant};
use crate::source::{AssetSource, Licenses, Query, SearchPage, SourceInfo};
use crate::{SourceError, guard};

const API: &str = "https://ambientcg.com/api/v3/assets";
/// Search asks for the downloads too, so a result already says which
/// packages it has (a page of 24 is still small).
const FIELDS: &str = "title,url,tags,type,thumbnails,downloads,maps";
/// Preview sizes in order of preference: big enough for a card, small to load.
const THUMBNAILS: [&str; 6] = [
    "256-PNG", "256-JPG", "512-PNG", "512-JPG", "128-PNG", "128-JPG",
];

/// The source.
pub struct AmbientCg;

/// The one instance, listed in [`sources`](crate::sources).
pub static AMBIENTCG: AmbientCg = AmbientCg;

static INFO: SourceInfo = SourceInfo {
    key: "ambientcg",
    name: "ambientCG",
    home: "https://ambientcg.com",
    credit: "Assets from ambientCG · CC0",
    kinds: &[Kind::Texture, Kind::Hdri, Kind::Model],
    licenses: Licenses::All(CC0),
    download_hosts: Some(&["ambientcg.com"]),
};

/// ambientCG's `type` for each kind.
fn type_filter(kind: Kind) -> Option<&'static str> {
    match kind {
        Kind::Texture => Some("material"),
        Kind::Hdri => Some("hdri"),
        Kind::Model => Some("3d-model"),
        Kind::Sound => None,
    }
}

fn kind_of(type_name: &str) -> Option<Kind> {
    Kind::ALL
        .into_iter()
        .find(|kind| type_filter(*kind) == Some(type_name))
}

/// The search address for `query` (public for tests and the CLI's dry runs).
pub fn search_url(query: &Query, type_name: &str) -> String {
    let mut url = format!(
        "{API}?q={}&type={type_name}&sort=popular&limit={}&include={FIELDS}",
        http::encode_query(query.text.trim()),
        query.per_page.max(1),
    );
    if query.offset() > 0 {
        url.push_str(&format!("&offset={}", query.offset()));
    }
    url
}

/// The full-record address for one id.
pub fn record_url(id: &str) -> String {
    format!("{API}?id={id}&include={FIELDS}")
}

impl AssetSource for AmbientCg {
    fn info(&self) -> &SourceInfo {
        &INFO
    }

    fn search(&self, http: &dyn Http, query: &Query) -> Result<SearchPage, SourceError> {
        let Some(type_name) = type_filter(query.kind) else {
            return Ok(SearchPage::default());
        };
        if !INFO.may_match(query) {
            return Ok(SearchPage::default());
        }
        let answer = json::parse(&http::get_text(
            http,
            &search_url(query, type_name),
            JSON_LIMIT,
        )?)?;
        let records: Vec<AssetRecord> = assets(&answer)?
            .iter()
            .filter_map(|asset| full(asset, query.kind))
            .collect();
        let total = json::count(answer.get("totalResults"));
        let shown = query.offset() + records.len() as u64;
        Ok(SearchPage {
            more: total.map_or(records.len() as u32 == query.per_page, |total| {
                shown < total
            }),
            total,
            records,
        })
    }

    fn record(&self, http: &dyn Http, id: &str) -> Result<AssetRecord, SourceError> {
        let id = id.trim();
        if !guard::safe_asset_id(id) {
            return Err(SourceError::NotFound);
        }
        let answer = json::parse(&http::get_text(http, &record_url(id), JSON_LIMIT)?)?;
        let asset = assets(&answer)?.first().ok_or(SourceError::NotFound)?;
        let kind = kind_of(&json::text(asset.get("type"))).ok_or(SourceError::BadResponse(
            "ambientCG sent an asset of a kind the Toolbox doesn't hold",
        ))?;
        full(asset, kind).ok_or(SourceError::NotFound)
    }
}

/// A record with its downloads, resolutions and maps; `None` for an entry
/// with no usable id.
fn full(asset: &Value, kind: Kind) -> Option<AssetRecord> {
    let mut record = summary(asset, kind)?;
    record.variants = variants(asset, &record.id);
    record.resolutions = resolutions(&record.variants);
    record.maps = maps(asset.get("maps"));
    Some(record)
}

fn assets(answer: &Value) -> Result<&Vec<Value>, SourceError> {
    answer
        .get("assets")
        .and_then(Value::as_array)
        .ok_or(SourceError::BadResponse(
            "ambientCG sent back results the Toolbox couldn't read",
        ))
}

/// A search result's record; `None` for an entry with no usable id.
fn summary(asset: &Value, kind: Kind) -> Option<AssetRecord> {
    let id = json::text(asset.get("id"));
    if !guard::safe_asset_id(&id) {
        return None;
    }
    let name = Some(json::text(asset.get("title")))
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| id.clone());
    let url = Some(json::text(asset.get("url")))
        .filter(|url| url.starts_with("https://"))
        .unwrap_or_else(|| format!("https://ambientcg.com/a/{id}"));
    Some(AssetRecord {
        provider: INFO.key.to_owned(),
        kind,
        attribution: format!("\"{name}\" from ambientCG, CC0 1.0"),
        // ambientCG publishes its own work and names no per-asset author.
        authors: vec![INFO.name.to_owned()],
        license: CC0.to_owned(),
        license_version: "1.0".to_owned(),
        license_url: CC0_URL.to_owned(),
        tags: json::names(asset.get("tags")),
        thumbnail_url: thumbnail(asset.get("thumbnails")),
        url,
        name,
        id,
        ..AssetRecord::default()
    })
}

fn thumbnail(thumbnails: Option<&Value>) -> String {
    let Some(thumbnails) = thumbnails.and_then(Value::as_object) else {
        return String::new();
    };
    THUMBNAILS
        .iter()
        .map(|size| json::text(thumbnails.get(*size)))
        .chain(thumbnails.values().map(|url| json::text(Some(url))))
        .find(|url| url.starts_with("https://"))
        .unwrap_or_default()
}

/// One variant per download, labelled from its attributes ("2K-JPG" is
/// "2K JPG", resolution `2k`, format `jpg`).
fn variants(asset: &Value, id: &str) -> Vec<Variant> {
    let Some(downloads) = asset.get("downloads").and_then(Value::as_array) else {
        return Vec::new();
    };
    downloads
        .iter()
        .filter_map(|download| {
            let url = json::text(download.get("url"));
            if !INFO.allows_download_from(&url) {
                return None;
            }
            let attributes = json::text(download.get("attributes"));
            let extension = Some(json::text(download.get("extension")).to_ascii_lowercase())
                .filter(|extension| !extension.is_empty())
                .unwrap_or_else(|| "zip".to_owned());
            let (resolution, format) = match attributes.split_once('-') {
                Some((resolution, format)) => {
                    (resolution.to_ascii_lowercase(), format.to_ascii_lowercase())
                }
                None => (attributes.to_ascii_lowercase(), String::new()),
            };
            let resolution = Some(resolution).filter(|res| is_resolution(res));
            let fallback = format!(
                "{id}_{}.{extension}",
                if attributes.is_empty() {
                    "download"
                } else {
                    &attributes
                }
            );
            Some(Variant {
                label: Some(attributes.replace('-', " "))
                    .filter(|label| !label.is_empty())
                    .unwrap_or_else(|| "Download".to_owned()),
                resolution,
                // A zip labelled by resolution only ("4K", HDRIs) has an
                // unknown format until it's unpacked: left empty.
                format: if format.is_empty() && extension != "zip" {
                    extension.clone()
                } else {
                    format
                },
                package: if extension == "zip" {
                    Package::Archive
                } else {
                    Package::Single
                },
                files: vec![RemoteFile {
                    path: json::file_name(&url, &fallback),
                    size: json::count(download.get("size")),
                    url,
                    ..RemoteFile::default()
                }],
            })
        })
        .collect()
}

/// `1k`, `2k`…: digits then `k`.
fn is_resolution(text: &str) -> bool {
    text.strip_suffix('k')
        .is_some_and(|number| !number.is_empty() && number.chars().all(|c| c.is_ascii_digit()))
}

/// The variants' resolutions, once each, smallest first.
pub(crate) fn resolutions(variants: &[Variant]) -> Vec<String> {
    let mut found: Vec<String> = variants
        .iter()
        .filter_map(|v| v.resolution.clone())
        .collect();
    found.sort_by_key(|res| res.trim_end_matches('k').parse::<u32>().unwrap_or(0));
    found.dedup();
    found
}

/// ambientCG's map names as the record's (`AmbientOcclusion` is `ao`).
fn maps(value: Option<&Value>) -> Vec<String> {
    let mut maps: Vec<String> = json::names(value)
        .iter()
        .filter_map(|name| {
            let name = name.to_ascii_lowercase().replace(['-', '_', ' '], "");
            Some(match name.as_str() {
                "color" | "basecolor" | "diffuse" => "base_color",
                "normal" | "normalgl" => "normal",
                "normaldx" => "normal_dx",
                "roughness" => "roughness",
                "metalness" | "metallic" => "metallic",
                "ambientocclusion" | "ao" => "ao",
                "displacement" | "height" => "height",
                "opacity" => "opacity",
                "emission" => "emission",
                _ => return None,
            })
        })
        .map(str::to_owned)
        .collect();
    maps.dedup();
    maps
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::Fixture;

    const SEARCH: &str = r#"{
        "totalResults": 31,
        "assets": [
            {"id": "Bricks090", "title": "Bricks 090", "url": "https://ambientcg.com/a/Bricks090",
             "type": "material", "tags": ["brick", "wall"],
             "thumbnails": {"128-PNG": "https://acg-media.example/Bricks090_128.png",
                            "256-PNG": "https://acg-media.example/Bricks090_256.png"}},
            {"id": "../nope", "title": "Bad"},
            "not an object"
        ]
    }"#;

    const RECORD: &str = r#"{
        "assets": [{
            "id": "Bricks090", "title": "Bricks 090", "type": "material",
            "maps": ["Color", "NormalGL", "NormalDX", "Roughness", "AmbientOcclusion", "Displacement"],
            "downloads": [
                {"url": "https://ambientcg.com/get?file=Bricks090_2K-JPG.zip", "attributes": "2K-JPG", "extension": "zip", "size": 2000},
                {"url": "https://ambientcg.com/get?file=Bricks090_1K-PNG.zip", "attributes": "1K-PNG", "extension": "zip", "size": 1000},
                {"url": "https://evil.example/get?file=Bricks090_4K-JPG.zip", "attributes": "4K-JPG", "extension": "zip"}
            ]
        }]
    }"#;

    #[test]
    fn search_reads_records_and_skips_bad_entries() {
        let query = Query::new("red brick", Kind::Texture);
        let http = Fixture::default().with(&search_url(&query, "material"), SEARCH);
        let page = AMBIENTCG.search(&http, &query).unwrap();
        assert_eq!(page.total, Some(31));
        assert!(page.more);
        assert_eq!(page.records.len(), 1);
        let record = &page.records[0];
        assert_eq!(record.key(), "ambientcg:Bricks090");
        assert_eq!(record.name, "Bricks 090");
        assert_eq!(record.license, "CC0");
        assert_eq!(record.tags, ["brick", "wall"]);
        assert_eq!(
            record.thumbnail_url,
            "https://acg-media.example/Bricks090_256.png"
        );
        assert!(record.variants.is_empty());
    }

    #[test]
    fn the_search_address_pages_by_offset_and_encodes_the_words() {
        let mut query = Query::new("red brick", Kind::Hdri);
        assert_eq!(
            search_url(&query, "hdri"),
            "https://ambientcg.com/api/v3/assets?q=red%20brick&type=hdri&sort=popular&limit=24&include=title,url,tags,type,thumbnails,downloads,maps"
        );
        query.page = 3;
        assert!(search_url(&query, "hdri").ends_with("&offset=48"));
    }

    #[test]
    fn sounds_and_other_licences_are_empty_without_a_request() {
        let http = Fixture::default();
        assert!(
            AMBIENTCG
                .search(&http, &Query::new("door", Kind::Sound))
                .unwrap()
                .records
                .is_empty()
        );
        let mut query = Query::new("brick", Kind::Texture);
        query.licenses = vec!["CC-BY".into()];
        assert!(AMBIENTCG.search(&http, &query).unwrap().records.is_empty());
    }

    #[test]
    fn a_record_has_one_variant_per_download_from_ambientcg_only() {
        let http = Fixture::default().with(&record_url("Bricks090"), RECORD);
        let record = AMBIENTCG.record(&http, "Bricks090").unwrap();
        assert_eq!(record.kind, Kind::Texture);
        let labels: Vec<_> = record.variants.iter().map(|v| v.label.as_str()).collect();
        assert_eq!(labels, ["2K JPG", "1K PNG"]);
        let first = &record.variants[0];
        assert_eq!(first.resolution.as_deref(), Some("2k"));
        assert_eq!(first.format, "jpg");
        assert_eq!(first.package, Package::Archive);
        assert_eq!(first.files[0].path, "Bricks090_2K-JPG.zip");
        assert_eq!(first.total_size(), Some(2000));
        assert_eq!(record.resolutions, ["1k", "2k"]);
        assert_eq!(
            record.maps,
            [
                "base_color",
                "normal",
                "normal_dx",
                "roughness",
                "ao",
                "height"
            ]
        );
    }

    #[test]
    fn an_unknown_or_unsafe_id_is_not_found() {
        let http = Fixture::default().with(&record_url("Nothing001"), r#"{"assets": []}"#);
        assert!(matches!(
            AMBIENTCG.record(&http, "Nothing001"),
            Err(SourceError::NotFound)
        ));
        assert!(matches!(
            AMBIENTCG.record(&http, "../etc"),
            Err(SourceError::NotFound)
        ));
    }

    #[test]
    fn a_malformed_answer_is_a_bad_response() {
        let query = Query::new("", Kind::Texture);
        let http = Fixture::default().with(&search_url(&query, "material"), r#"{"assets": 5}"#);
        assert!(matches!(
            AMBIENTCG.search(&http, &query),
            Err(SourceError::BadResponse(_))
        ));
        let http = Fixture::default().with(&search_url(&query, "material"), "<html>");
        assert!(matches!(
            AMBIENTCG.search(&http, &query),
            Err(SourceError::BadResponse(_))
        ));
    }
}
