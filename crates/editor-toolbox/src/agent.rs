//! The Toolbox without the tab, for the editor's AI tools (`toolbox_search`
//! and `toolbox_get`, `docs/editor/mcp.md`): the same search of every
//! source, with the same licence filter, and the same Get into the central
//! asset directory and the project. Every call blocks.

use super::*;

/// A kind by the name the AI tools use: texture, sky, model or sound.
pub fn kind_named(name: &str) -> Option<Kind> {
    match name {
        "texture" => Some(Kind::Texture),
        "sky" => Some(Kind::Hdri),
        "model" => Some(Kind::Model),
        "sound" => Some(Kind::Sound),
        _ => None,
    }
}

/// One search result, in short.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hit {
    /// What `get_asset` takes: "provider:id".
    pub key: String,
    pub name: String,
    pub category: String,
    /// The licence's code, such as "CC0" or "CC-BY".
    pub license: String,
    /// Whether the licence asks for a credit line.
    pub credit: bool,
    /// The resolutions and formats Get can install ("1K", "JPG"), in the
    /// source's order. Empty when the source lists them only in the full
    /// record (Poly Haven): Get looks them up.
    pub resolutions: Vec<String>,
    pub formats: Vec<String>,
}

/// What one search found.
#[derive(Debug, Default)]
pub struct Hits {
    pub hits: Vec<Hit>,
    /// Sources that failed while another answered, with why.
    pub failed: Vec<String>,
    /// Whether a later page has more.
    pub more: bool,
}

/// Page `page` (from 1) of `kind` assets matching `query`, from `source`
/// (a key such as "ambientcg") or every source that has `kind`, as the
/// tab's default search finds them: sounds only under licences a game can
/// use with at most a credit.
pub fn search_assets(
    http: &dyn Http,
    kind: Kind,
    query: &str,
    source: Option<&str>,
    page: u32,
) -> Result<Hits, String> {
    let sources = match source {
        Some(key) => vec![source_named(key, kind)?],
        None => axcenity_toolbox::sources_for(kind),
    };
    let found = search(http, &sources, kind, query, page, false)?;
    let hits = found
        .listings
        .into_iter()
        .map(|listing| {
            let mut resolutions = Vec::new();
            let mut formats = Vec::new();
            for package in &listing.packages {
                let resolution = package.resolution.clone().unwrap_or_default();
                if !resolution.is_empty() && !resolutions.contains(&resolution.to_uppercase()) {
                    resolutions.push(resolution.to_uppercase());
                }
                let format = package.format.to_uppercase();
                if !format.is_empty() && !formats.contains(&format) {
                    formats.push(format);
                }
            }
            let record = &listing.record;
            Hit {
                key: record.key(),
                name: listing.name.clone(),
                category: listing.category.clone(),
                license: license::normalize(&record.license).to_owned(),
                credit: license::needs_credit(license::normalize(&record.license)),
                resolutions,
                formats,
            }
        })
        .collect();
    Ok(Hits {
        hits,
        failed: found.failed,
        more: !found.more.is_empty(),
    })
}

/// What `get_asset` got.
#[derive(Debug)]
pub struct Got {
    /// What's in the project now, or (a model) waiting in the asset
    /// directory for the import.
    pub installed: Installed,
    pub name: String,
    /// The package's label, such as "2K JPG".
    pub package: String,
    /// The licence as people read it, such as "CC0".
    pub license: String,
    /// The credit line when the licence asks for one.
    pub credit: Option<String>,
    /// Why another package than the one asked for was taken, if it was.
    pub notes: String,
}

/// Gets the asset `key` ("provider:id", from [`search_assets`]) into
/// `project` the way the tab's Get does, at `resolution` and `format`
/// (such as "2K" and "JPG"; an empty format is the kind's first, as the
/// tab's menu lists them) or the nearest the asset has: into the asset
/// directory `assets` first (once), then a texture set, sky or sound into
/// the project. A model only reaches the asset directory; the caller
/// imports it.
pub fn get_asset(
    http: &dyn Http,
    assets: &Path,
    project: &Path,
    key: &str,
    resolution: &str,
    format: &str,
) -> Result<Got, String> {
    let Some((provider, id)) = key.split_once(':') else {
        return Err(format!(
            "\"{key}\" isn't an asset key; keys look like \"ambientcg:Ground054\""
        ));
    };
    let source = axcenity_toolbox::source(provider).ok_or_else(|| {
        format!(
            "there's no source \"{provider}\"; the sources are {}",
            source_keys()
        )
    })?;
    let record = source.record(http, id).map_err(|err| match err {
        SourceError::NotFound => format!("{} has no asset \"{id}\"", source.info().name),
        err => err.to_string(),
    })?;
    let listing = Listing::from_record(record);
    let rules = rules(listing.record.kind);
    let format = if format.trim().is_empty() {
        rules.formats[0]
    } else {
        format.trim()
    };
    let choice = pick(&listing, resolution.trim(), format)
        .ok_or_else(|| format!("{} has no {} the engine reads", listing.name, rules.noun))?;
    let notes = choice.notes.join(" ");
    let variant = needed_maps(choice.variant);
    let cancel = AtomicBool::new(false);
    let record = &listing.record;
    let installed = match download(http, source, assets, project, record, &variant, &cancel)? {
        DownloadResult::Saved { installed, .. } => installed,
        DownloadResult::Cancelled => return Err("the download was cancelled".to_owned()),
    };
    let code = license::normalize(&record.license);
    Ok(Got {
        installed,
        name: record.name.clone(),
        package: variant.label.clone(),
        license: license::display(&record.license, &record.license_version),
        credit: license::needs_credit(code)
            .then(|| license::credit_line(record, source.info().name)),
        notes,
    })
}

/// The source with the key `key` that has `kind`.
fn source_named(key: &str, kind: Kind) -> Result<&'static dyn AssetSource, String> {
    let source = axcenity_toolbox::source(key).ok_or_else(|| {
        format!(
            "there's no source \"{key}\"; the sources are {}",
            source_keys()
        )
    })?;
    if !source.info().has(kind) {
        let have: Vec<&str> = axcenity_toolbox::sources_for(kind)
            .iter()
            .map(|source| source.info().key)
            .collect();
        return Err(format!(
            "{} has no {}; {} {}",
            source.info().name,
            rules(kind).noun,
            have.join(" and "),
            if have.len() == 1 { "has" } else { "have" }
        ));
    }
    Ok(source)
}

fn source_keys() -> String {
    axcenity_toolbox::sources()
        .iter()
        .map(|source| source.info().key)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Answers for tests of the AI tools, here and in the editor.
#[doc(hidden)]
pub mod testing {
    use std::io::Write;

    use axcenity_toolbox::ambientcg::{record_url, search_url};
    use axcenity_toolbox::http::Fixture;

    use super::*;

    /// ambientCG with one material, Fabric086 ("fabric" finds it), as
    /// 1K and 2K JPG zips of a colour and a normal map; without its
    /// `files`, only the search and the record answer.
    pub fn fabric_on_ambientcg(files: bool) -> Fixture {
        let asset = r#"{"id": "Fabric086", "title": "Fabric 086", "type": "material",
            "url": "https://ambientcg.com/a/Fabric086", "tags": ["fabric"],
            "maps": ["Color", "NormalGL"],
            "downloads": [
                {"url": "https://ambientcg.com/get?file=Fabric086_1K-JPG.zip", "attributes": "1K-JPG", "extension": "zip"},
                {"url": "https://ambientcg.com/get?file=Fabric086_2K-JPG.zip", "attributes": "2K-JPG", "extension": "zip"}
            ]}"#;
        let answer = format!(r#"{{"totalResults": 1, "assets": [{asset}]}}"#);
        let mut query = Query::new("fabric", Kind::Texture);
        query.per_page = PAGE as u32;
        let mut http = Fixture::default()
            .with(&search_url(&query, "material"), answer.clone())
            .with(&record_url("Fabric086"), answer);
        for size in ["1K", "2K"].into_iter().filter(|_| files) {
            http = http.with(
                &format!("https://ambientcg.com/get?file=Fabric086_{size}-JPG.zip"),
                maps_zip(size),
            );
        }
        http
    }

    fn maps_zip(size: &str) -> Vec<u8> {
        let mut cursor = std::io::Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut cursor);
            let options = zip::write::SimpleFileOptions::default();
            for map in ["Color", "NormalGL"] {
                let name = format!("Fabric086_{size}-JPG_{map}.jpg");
                writer.start_file(name.as_str(), options).unwrap();
                writer.write_all(name.as_bytes()).unwrap();
            }
            writer.finish().unwrap();
        }
        cursor.into_inner()
    }
}

#[cfg(test)]
mod tests {
    use axcenity_toolbox::http::Fixture;

    use super::*;

    #[test]
    fn kinds_have_the_tools_names() {
        assert_eq!(kind_named("sky"), Some(Kind::Hdri));
        assert_eq!(kind_named("texture"), Some(Kind::Texture));
        assert_eq!(kind_named("hdri"), None);
    }

    #[test]
    fn a_wrong_key_or_source_says_what_there_is() {
        let http = Fixture::default();
        let (a, b) = (Path::new("a"), Path::new("b"));
        let err = get_asset(&http, a, b, "Fabric086", "1K", "JPG").unwrap_err();
        assert!(err.contains("ambientcg:Ground054"), "{err}");
        let err = get_asset(&http, a, b, "nope:1", "1K", "JPG").unwrap_err();
        assert!(err.contains("ambientcg, polyhaven, openverse"), "{err}");
        let err = get_asset(&http, a, b, "ambientcg:Gone1", "1K", "JPG").unwrap_err();
        assert!(err.contains("ambientCG has no asset \"Gone1\""), "{err}");
        let err = search_assets(&http, Kind::Sound, "rain", Some("ambientcg"), 1).unwrap_err();
        assert!(err.contains("openverse has"), "{err}");
    }

    #[test]
    fn search_and_get_go_through_the_tabs_paths() {
        let temp = std::env::temp_dir().join(format!("axcenity-agent-get-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp);
        let http = testing::fabric_on_ambientcg(true);

        // Every source is asked; Poly Haven's failure is named, ambientCG's answer kept.
        let found = search_assets(&http, Kind::Texture, "fabric", None, 1).unwrap();
        assert_eq!(found.hits.len(), 1);
        let hit = &found.hits[0];
        assert_eq!(hit.key, "ambientcg:Fabric086");
        assert_eq!(hit.license, "CC0");
        assert!(!hit.credit);
        assert_eq!(hit.resolutions, ["1K", "2K"]);
        assert_eq!(hit.formats, ["JPG"]);
        assert!(
            found
                .failed
                .iter()
                .any(|failed| failed.starts_with("Poly Haven"))
        );

        // 4K isn't there: the nearest is taken, and said why.
        let project = temp.join("Project");
        let got = get_asset(
            &http,
            &temp.join("Assets"),
            &project,
            "ambientcg:Fabric086",
            "4K",
            "JPG",
        )
        .unwrap();
        assert_eq!(got.package, "2K JPG");
        assert!(!got.notes.is_empty());
        assert_eq!(got.license, "CC0 1.0");
        assert_eq!(got.credit, None);
        let Installed::TextureSet { path, .. } = &got.installed else {
            panic!("{:?}", got.installed);
        };
        assert!(project.join(path).is_file(), "{path}");
        let _ = fs::remove_dir_all(&temp);
    }
}
