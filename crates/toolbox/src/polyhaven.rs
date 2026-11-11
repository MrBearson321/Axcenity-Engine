//! Poly Haven (<https://polyhaven.com>): CC0 textures, HDRIs and models,
//! through its public API (<https://api.polyhaven.com>).
//!
//! Search fetches the whole catalogue of a kind once (`/assets?t=textures`,
//! one request) and matches every word locally against names, tags and
//! categories, as the Blender pipeline does; the catalogue is kept for 12
//! hours, the API's own `Cache-Control`. That replaces the Django Toolbox's
//! `/search` plus one `/info` per result. A full record is `/info/<id>`
//! plus `/files/<id>`, turned into variants as the Django Toolbox does:
//! loose maps grouped by resolution and format ("2K JPG"), each bundle
//! (glTF, FBX, USD, MaterialX) its own variant with the files it includes.
//! `.blend` files are never offered (they can carry scripts).
//!
//! Its API terms ask for a User-Agent naming the software (sent by
//! [`http`]) and for its results to say where they come from (`credit`).

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Map, Value};

use crate::http::{self, Http};
use crate::json::{self, JSON_LIMIT};
use crate::license::{CC0, CC0_URL};
use crate::record::{AssetRecord, Kind, Package, RemoteFile, Variant};
use crate::source::{AssetSource, Licenses, Query, SearchPage, SourceInfo};
use crate::{SourceError, ambientcg, guard};

const API: &str = "https://api.polyhaven.com";
/// How long a fetched catalogue is reused.
const CATALOGUE_LIFETIME: Duration = Duration::from_secs(12 * 60 * 60);
/// The resolutions Poly Haven publishes, smallest first.
const RESOLUTIONS: [&str; 5] = ["1k", "2k", "4k", "8k", "16k"];

/// One kind's catalogue (`id → info`) and when it was fetched.
type Catalogue = (Kind, Instant, Arc<Map<String, Value>>);

/// The source. It keeps each kind's catalogue in memory for a while.
pub struct PolyHaven {
    catalogues: Mutex<Vec<Catalogue>>,
}

/// The one instance, listed in [`sources`](crate::sources).
pub static POLYHAVEN: PolyHaven = PolyHaven {
    catalogues: Mutex::new(Vec::new()),
};

static INFO: SourceInfo = SourceInfo {
    key: "polyhaven",
    name: "Poly Haven",
    home: "https://polyhaven.com",
    credit: "Powered by Poly Haven",
    kinds: &[Kind::Texture, Kind::Hdri, Kind::Model],
    licenses: Licenses::All(CC0),
    download_hosts: Some(&["dl.polyhaven.org"]),
};

/// Poly Haven's type name for each kind.
fn type_name(kind: Kind) -> Option<&'static str> {
    match kind {
        Kind::Texture => Some("textures"),
        Kind::Hdri => Some("hdris"),
        Kind::Model => Some("models"),
        Kind::Sound | Kind::Sprites => None,
    }
}

/// The catalogue address for one type.
pub fn catalogue_url(type_name: &str) -> String {
    format!("{API}/assets?t={type_name}")
}

/// The `/info` address for one id.
pub fn info_url(id: &str) -> String {
    format!("{API}/info/{id}")
}

/// The `/files` address for one id.
pub fn files_url(id: &str) -> String {
    format!("{API}/files/{id}")
}

impl PolyHaven {
    /// Every asset of `kind` (`id → info`), fetched at most every 12 hours.
    fn catalogue(
        &self,
        http: &dyn Http,
        kind: Kind,
        type_name: &str,
    ) -> Result<Arc<Map<String, Value>>, SourceError> {
        if let Ok(cached) = self.catalogues.lock()
            && let Some((_, _, catalogue)) = cached
                .iter()
                .find(|(cached, at, _)| *cached == kind && at.elapsed() < CATALOGUE_LIFETIME)
        {
            return Ok(Arc::clone(catalogue));
        }
        let answer = json::parse(&http::get_text(
            http,
            &catalogue_url(type_name),
            JSON_LIMIT,
        )?)?;
        let Value::Object(catalogue) = answer else {
            return Err(SourceError::BadResponse(
                "Poly Haven sent back results the Toolbox couldn't read",
            ));
        };
        let catalogue = Arc::new(catalogue);
        if let Ok(mut cached) = self.catalogues.lock() {
            cached.retain(|(cached, _, _)| *cached != kind);
            cached.push((kind, Instant::now(), Arc::clone(&catalogue)));
        }
        Ok(catalogue)
    }
}

impl AssetSource for PolyHaven {
    fn info(&self) -> &SourceInfo {
        &INFO
    }

    fn search(&self, http: &dyn Http, query: &Query) -> Result<SearchPage, SourceError> {
        let Some(type_name) = type_name(query.kind) else {
            return Ok(SearchPage::default());
        };
        if !INFO.may_match(query) {
            return Ok(SearchPage::default());
        }
        let catalogue = self.catalogue(http, query.kind, type_name)?;
        let wanted: Vec<String> = json::words(&query.text).collect();
        let mut hits: Vec<(usize, AssetRecord)> = catalogue
            .iter()
            .filter(|(id, _)| safe_id(id))
            .filter_map(|(id, info)| {
                let record = summary(id, info, query.kind);
                let labels: Vec<String> = record
                    .tags
                    .iter()
                    .chain(&record.categories)
                    .map(|label| label.to_lowercase())
                    .collect();
                let vocabulary: Vec<String> = json::words(id)
                    .chain(json::words(&record.name))
                    .chain(labels.iter().flat_map(|label| json::words(label)))
                    .collect();
                let matches = wanted.iter().all(|word| {
                    vocabulary
                        .iter()
                        .any(|known| known.starts_with(word.as_str()))
                });
                // Words that are a whole tag or category rank first.
                let strong = wanted.iter().filter(|word| labels.contains(word)).count();
                matches.then_some((strong, record))
            })
            .collect();
        hits.sort_by(|(a_strong, a), (b_strong, b)| {
            b_strong
                .cmp(a_strong)
                .then(b.popularity.total_cmp(&a.popularity))
                .then(a.id.cmp(&b.id))
        });
        let total = hits.len() as u64;
        let start = query.offset().min(total) as usize;
        let records: Vec<AssetRecord> = hits
            .into_iter()
            .skip(start)
            .take(query.per_page as usize)
            .map(|(_, record)| record)
            .collect();
        Ok(SearchPage {
            more: (start + records.len()) < total as usize,
            total: Some(total),
            records,
        })
    }

    fn record(&self, http: &dyn Http, id: &str) -> Result<AssetRecord, SourceError> {
        let id = id.trim();
        if !safe_id(id) {
            return Err(SourceError::NotFound);
        }
        let info = json::parse(&http::get_text(http, &info_url(id), JSON_LIMIT)?)?;
        let kind = match info.get("type").and_then(Value::as_u64) {
            Some(0) => Kind::Hdri,
            Some(1) => Kind::Texture,
            Some(2) => Kind::Model,
            _ => {
                return Err(SourceError::BadResponse(
                    "Poly Haven sent back an asset the Toolbox couldn't read",
                ));
            }
        };
        let files = json::parse(&http::get_text(http, &files_url(id), JSON_LIMIT)?)?;
        let Value::Object(files) = files else {
            return Err(SourceError::BadResponse(
                "Poly Haven sent back files the Toolbox couldn't read",
            ));
        };
        let mut record = summary(id, &info, kind);
        record.variants = variants(id, &files, kind);
        record.resolutions = ambientcg::resolutions(&record.variants);
        if kind == Kind::Texture {
            let mut maps: Vec<String> = files
                .keys()
                .filter_map(|name| map_name(name))
                .map(str::to_owned)
                .collect();
            maps.sort();
            maps.dedup();
            record.maps = maps;
        }
        Ok(record)
    }
}

/// A Poly Haven slug: letters, digits, `_` and `-`, so it's safe in a path.
fn safe_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// A record from a catalogue entry or `/info`, without variants.
fn summary(id: &str, info: &Value, kind: Kind) -> AssetRecord {
    let name = Some(json::text(info.get("name")))
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| id.to_owned());
    let authors: Vec<String> = info
        .get("authors")
        .and_then(Value::as_object)
        .map(|authors| {
            authors
                .keys()
                .map(|name| name.trim().to_owned())
                .filter(|name| !name.is_empty())
                .collect()
        })
        .unwrap_or_default();
    let credited = if authors.is_empty() {
        INFO.name.to_owned()
    } else {
        authors.join(", ")
    };
    let max_width = info
        .get("max_resolution")
        .and_then(Value::as_array)
        .and_then(|size| size.first())
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let physical_size = info
        .get("dimensions")
        .and_then(Value::as_array)
        .map(|dims| {
            dims.iter()
                .filter_map(Value::as_f64)
                .map(|mm| mm / 1000.0)
                .collect::<Vec<_>>()
        })
        .filter(|dims| !dims.is_empty() && dims.iter().all(|metres| *metres > 0.0));
    AssetRecord {
        provider: INFO.key.to_owned(),
        id: id.to_owned(),
        kind,
        url: format!("https://polyhaven.com/a/{id}"),
        description: json::text(info.get("description")),
        tags: json::names(info.get("tags")),
        categories: json::names(info.get("categories")),
        attribution: format!("\"{name}\" by {credited}, from Poly Haven, CC0 1.0"),
        authors,
        license: CC0.to_owned(),
        license_version: "1.0".to_owned(),
        license_url: CC0_URL.to_owned(),
        resolutions: RESOLUTIONS
            .iter()
            .filter(|res| {
                let k: u64 = res.trim_end_matches('k').parse().unwrap_or(0);
                max_width > 0 && k * 1024 <= max_width
            })
            .map(|res| (*res).to_owned())
            .collect(),
        physical_size,
        polycount: json::count(info.get("polycount")).unwrap_or(0),
        popularity: info
            .get("download_count")
            .and_then(Value::as_f64)
            .unwrap_or(0.0),
        // The CDN sends WebP by default; ask for PNG, which every reader decodes.
        thumbnail_url: json::text(info.get("thumbnail_url")).replace("format=webp", "format=png"),
        name,
        ..AssetRecord::default()
    }
}

/// The record's map name for one of Poly Haven's `/files` keys.
fn map_name(key: &str) -> Option<&'static str> {
    Some(match key.to_ascii_lowercase().as_str() {
        "diffuse" | "diff" | "col" | "color" | "albedo" => "base_color",
        "nor_gl" => "normal",
        "nor_dx" => "normal_dx",
        "rough" | "roughness" => "roughness",
        "metal" | "metallic" | "metalness" => "metallic",
        "ao" => "ao",
        "displacement" | "disp" | "height" => "height",
        "arm" => "arm",
        "alpha" | "opacity" => "opacity",
        "emission" | "emissive" => "emission",
        _ => return None,
    })
}

/// How formats are written in labels; anything else is upper-cased.
fn format_label(format: &str) -> String {
    match format {
        "gltf" => "glTF".to_owned(),
        "mtlx" => "MaterialX".to_owned(),
        other => other.to_ascii_uppercase(),
    }
}

/// `/files` as variants, every download URL kept.
///
/// The tree is `name → resolution → format → {url, size, md5, include?}`.
/// Loose files are grouped by resolution and format, so "2K JPG" holds every
/// map's 2K JPG; an entry with `include` is a bundle of its own, with each
/// included file at its relative path (paths that would leave the folder
/// are dropped). Smallest resolution first, loose maps before bundles.
fn variants(id: &str, files: &Map<String, Value>, kind: Kind) -> Vec<Variant> {
    // (resolution in K, is a bundle, format) → files
    let mut groups: BTreeMap<(u32, bool, String), Vec<RemoteFile>> = BTreeMap::new();
    for (name, by_resolution) in files {
        let Some(by_resolution) = by_resolution.as_object() else {
            continue;
        };
        for (resolution, by_format) in by_resolution {
            let Some(k) = resolution
                .strip_suffix('k')
                .and_then(|k| k.parse::<u32>().ok())
            else {
                continue;
            };
            let Some(by_format) = by_format.as_object() else {
                continue;
            };
            for (format, entry) in by_format {
                let format = format.to_ascii_lowercase();
                let url = json::text(entry.get("url"));
                if format == "blend"
                    || url.to_ascii_lowercase().ends_with(".blend")
                    || !INFO.allows_download_from(&url)
                {
                    continue;
                }
                let included = entry.get("include").and_then(Value::as_object);
                let fallback = format!("{id}_{name}_{resolution}.{format}");
                let group = groups
                    .entry((k, included.is_some(), format.clone()))
                    .or_default();
                group.push(RemoteFile {
                    path: json::file_name(&url, &fallback),
                    size: json::count(entry.get("size")),
                    md5: Some(json::text(entry.get("md5"))).filter(|md5| !md5.is_empty()),
                    // A model's loose texture maps are named like a texture's.
                    map: if included.is_none() {
                        map_name(name).map(str::to_owned)
                    } else {
                        None
                    },
                    url,
                });
                for (path, part) in included.into_iter().flatten() {
                    let part_url = json::text(part.get("url"));
                    let Some(path) = guard::safe_relative_path(path) else {
                        continue;
                    };
                    if !INFO.allows_download_from(&part_url) {
                        continue;
                    }
                    group.push(RemoteFile {
                        url: part_url,
                        path,
                        size: json::count(part.get("size")),
                        md5: Some(json::text(part.get("md5"))).filter(|md5| !md5.is_empty()),
                        map: None,
                    });
                }
            }
        }
    }
    groups
        .into_iter()
        .map(|((k, bundle, format), files)| Variant {
            label: format!("{k}K {}", format_label(&format)),
            resolution: Some(format!("{k}k")),
            package: if bundle {
                Package::Bundle
            } else if kind == Kind::Hdri {
                Package::Single
            } else {
                // A texture's maps, or a model's maps without its scene file.
                Package::Maps
            },
            format,
            files,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::Fixture;

    const TEXTURES: &str = r#"{
        "red_brick_03": {"name": "Red Brick 03", "type": 1, "tags": ["brick", "wall", "red"],
            "categories": ["brick", "outdoor"], "authors": {"Rob Tuytel": "All"},
            "download_count": 900, "max_resolution": [8192, 8192], "dimensions": [2000, 2000],
            "thumbnail_url": "https://cdn.polyhaven.com/asset_img/thumbs/red_brick_03.png?width=256&format=webp"},
        "brick_wall_001": {"name": "Brick Wall 001", "type": 1, "tags": ["wall"],
            "categories": ["brick"], "download_count": 5000, "max_resolution": [4096, 4096]},
        "rusty_metal": {"name": "Rusty Metal", "type": 1, "tags": ["metal"], "download_count": 99999},
        "../bad": {"name": "Bad", "tags": ["brick"]}
    }"#;

    const FILES: &str = r#"{
        "Diffuse": {"2k": {"jpg": {"url": "https://dl.polyhaven.org/file/ph-assets/Textures/jpg/2k/red_brick_03/red_brick_03_diff_2k.jpg", "size": 1500000, "md5": "abc"},
                           "png": {"url": "https://dl.polyhaven.org/file/ph-assets/Textures/png/2k/red_brick_03/red_brick_03_diff_2k.png", "size": 4000000}},
                    "1k": {"jpg": {"url": "https://dl.polyhaven.org/file/ph-assets/Textures/jpg/1k/red_brick_03/red_brick_03_diff_1k.jpg", "size": 400000}}},
        "nor_gl": {"2k": {"jpg": {"url": "https://dl.polyhaven.org/file/ph-assets/Textures/jpg/2k/red_brick_03/red_brick_03_nor_gl_2k.jpg", "size": 2500000}}},
        "nor_dx": {"2k": {"jpg": {"url": "https://dl.polyhaven.org/file/ph-assets/Textures/jpg/2k/red_brick_03/red_brick_03_nor_dx_2k.jpg", "size": 2500000}}},
        "blend": {"2k": {"blend": {"url": "https://dl.polyhaven.org/file/ph-assets/Textures/blend/2k/red_brick_03_2k.blend", "size": 9}}},
        "gltf": {"2k": {"gltf": {"url": "https://dl.polyhaven.org/file/ph-assets/Textures/gltf/2k/red_brick_03/red_brick_03_2k.gltf", "size": 3000,
            "include": {"textures/red_brick_03_diff_2k.jpg": {"url": "https://dl.polyhaven.org/file/ph-assets/Textures/jpg/2k/red_brick_03/red_brick_03_diff_2k.jpg", "size": 1500000},
                        "../escape.jpg": {"url": "https://dl.polyhaven.org/x.jpg", "size": 1}}}}},
        "evil": {"2k": {"jpg": {"url": "https://evil.example/x.jpg"}}},
        "tonemapped": {"url": "https://dl.polyhaven.org/file/x.jpg"}
    }"#;

    fn query(text: &str) -> Query {
        Query::new(text, Kind::Texture)
    }

    #[test]
    fn search_matches_every_word_and_ranks_tags_then_downloads() {
        let poly = PolyHaven {
            catalogues: Mutex::new(Vec::new()),
        };
        let http = Fixture::default().with(&catalogue_url("textures"), TEXTURES);
        let page = poly.search(&http, &query("brick")).unwrap();
        let ids: Vec<_> = page.records.iter().map(|r| r.id.as_str()).collect();
        // Both have the category "brick"; the more downloaded comes first.
        assert_eq!(ids, ["brick_wall_001", "red_brick_03"]);
        assert_eq!(page.total, Some(2));
        assert!(!page.more);

        let page = poly.search(&http, &query("red bri")).unwrap();
        let ids: Vec<_> = page.records.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, ["red_brick_03"]);
        let record = &page.records[0];
        assert_eq!(record.authors, ["Rob Tuytel"]);
        assert_eq!(record.physical_size, Some(vec![2.0, 2.0]));
        assert_eq!(
            record.thumbnail_url,
            "https://cdn.polyhaven.com/asset_img/thumbs/red_brick_03.png?width=256&format=png"
        );
        assert_eq!(record.resolutions, ["1k", "2k", "4k", "8k"]);
        assert_eq!(
            record.attribution,
            "\"Red Brick 03\" by Rob Tuytel, from Poly Haven, CC0 1.0"
        );

        // No words lists everything by downloads.
        let ids: Vec<_> = poly
            .search(&http, &query(""))
            .unwrap()
            .records
            .into_iter()
            .map(|r| r.id)
            .collect();
        assert_eq!(ids, ["rusty_metal", "brick_wall_001", "red_brick_03"]);
    }

    #[test]
    fn the_catalogue_is_fetched_once_and_pages_are_cut_from_it() {
        let poly = PolyHaven {
            catalogues: Mutex::new(Vec::new()),
        };
        let http = Fixture::default().with(&catalogue_url("textures"), TEXTURES);
        let mut first = query("");
        first.per_page = 2;
        let page = poly.search(&http, &first).unwrap();
        assert_eq!(page.records.len(), 2);
        assert!(page.more);
        // A second search is answered from memory: an empty fixture would 404.
        let mut second = first.clone();
        second.page = 2;
        let page = poly.search(&Fixture::default(), &second).unwrap();
        assert_eq!(page.records.len(), 1);
        assert!(!page.more);
    }

    #[test]
    fn a_record_groups_maps_and_keeps_bundles_but_never_blend_files() {
        let http = Fixture::default()
            .with(
                &info_url("red_brick_03"),
                r#"{"name": "Red Brick 03", "type": 1, "authors": {"Rob Tuytel": "All"}}"#,
            )
            .with(&files_url("red_brick_03"), FILES);
        let record = POLYHAVEN.record(&http, "red_brick_03").unwrap();
        let labels: Vec<_> = record.variants.iter().map(|v| v.label.as_str()).collect();
        assert_eq!(labels, ["1K JPG", "2K JPG", "2K PNG", "2K glTF"]);

        let maps_2k = &record.variants[1];
        assert_eq!(maps_2k.package, Package::Maps);
        let maps: Vec<_> = maps_2k
            .files
            .iter()
            .map(|f| f.map.as_deref().unwrap())
            .collect();
        assert_eq!(maps, ["base_color", "normal_dx", "normal"]);
        assert_eq!(maps_2k.files[0].md5.as_deref(), Some("abc"));
        assert_eq!(maps_2k.files[0].path, "red_brick_03_diff_2k.jpg");

        let bundle = &record.variants[3];
        assert_eq!(bundle.package, Package::Bundle);
        let paths: Vec<_> = bundle.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(
            paths,
            ["red_brick_03_2k.gltf", "textures/red_brick_03_diff_2k.jpg"]
        );

        assert_eq!(record.resolutions, ["1k", "2k"]);
        assert_eq!(record.maps, ["base_color", "normal", "normal_dx"]);
        assert!(
            record
                .variants
                .iter()
                .all(|v| v.files.iter().all(|f| !f.url.ends_with(".blend")))
        );
    }

    #[test]
    fn an_hdri_is_one_single_file_per_resolution_and_format() {
        let http = Fixture::default()
            .with(&info_url("sky_01"), r#"{"name": "Sky 01", "type": 0}"#)
            .with(&files_url("sky_01"), r#"{"hdri": {"4k": {"hdr": {"url": "https://dl.polyhaven.org/sky_01_4k.hdr", "size": 10},
                                                              "exr": {"url": "https://dl.polyhaven.org/sky_01_4k.exr", "size": 20}}}}"#);
        let record = POLYHAVEN.record(&http, "sky_01").unwrap();
        assert_eq!(record.kind, Kind::Hdri);
        let labels: Vec<_> = record
            .variants
            .iter()
            .map(|v| (v.label.as_str(), v.package))
            .collect();
        assert_eq!(
            labels,
            [("4K EXR", Package::Single), ("4K HDR", Package::Single)]
        );
    }

    #[test]
    fn a_models_loose_maps_and_its_gltf_are_separate_variants() {
        let http = Fixture::default()
            .with(&info_url("sofa_02"), r#"{"name": "Sofa 02", "type": 2, "polycount": 12000}"#)
            .with(&files_url("sofa_02"), r#"{
                "Diffuse": {"1k": {"jpg": {"url": "https://dl.polyhaven.org/sofa_02_diff_1k.jpg", "size": 1}}},
                "gltf": {"1k": {"gltf": {"url": "https://dl.polyhaven.org/sofa_02_1k.gltf", "size": 2,
                    "include": {"sofa_02.bin": {"url": "https://dl.polyhaven.org/sofa_02.bin", "size": 3}}}}}}"#);
        let record = POLYHAVEN.record(&http, "sofa_02").unwrap();
        assert_eq!(record.kind, Kind::Model);
        assert_eq!(record.polycount, 12000);
        let variants: Vec<_> = record
            .variants
            .iter()
            .map(|v| (v.label.as_str(), v.package, v.files.len()))
            .collect();
        assert_eq!(
            variants,
            [
                ("1K JPG", Package::Maps, 1),
                ("1K glTF", Package::Bundle, 2)
            ]
        );
        assert_eq!(
            record.variants[0].files[0].map.as_deref(),
            Some("base_color")
        );
    }

    #[test]
    fn unsafe_ids_and_unknown_types_are_refused() {
        assert!(matches!(
            POLYHAVEN.record(&Fixture::default(), "../x"),
            Err(SourceError::NotFound)
        ));
        let http = Fixture::default().with(&info_url("odd"), r#"{"name": "Odd", "type": 7}"#);
        assert!(matches!(
            POLYHAVEN.record(&http, "odd"),
            Err(SourceError::BadResponse(_))
        ));
    }
}
