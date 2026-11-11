//! Checks against the real sources. They reach the network, so they're
//! ignored by default; run them by hand after changing a source, or when
//! a source may have changed its API:
//!
//! `cargo nextest run -p axcenity-toolbox --run-ignored only`

use std::time::Duration;

use axcenity_toolbox::http::Client;
use axcenity_toolbox::{Kind, Package, Query, source};

fn search_then_record(key: &str, kind: Kind, words: &str) {
    let source = source(key).unwrap();
    let http = Client::new(Duration::from_secs(60));
    let mut query = Query::new(words, kind);
    query.per_page = 5;
    let page = source.search(&http, &query).unwrap();
    assert!(
        !page.records.is_empty(),
        "{key} found no {kind:?} for {words:?}"
    );
    let first = &page.records[0];
    assert_eq!(first.provider, key);
    assert_eq!(first.kind, kind);
    assert!(!first.name.is_empty());
    assert!(!first.license.is_empty());

    let record = source.record(&http, &first.id).unwrap();
    assert_eq!(record.id, first.id);
    assert!(
        !record.variants.is_empty(),
        "{key}:{} has no downloads",
        record.id
    );
    for variant in &record.variants {
        assert!(!variant.files.is_empty());
        for file in &variant.files {
            assert!(
                source.info().allows_download_from(&file.url),
                "{}",
                file.url
            );
            assert!(!file.path.is_empty());
        }
    }
    println!(
        "{}: {} variants, first {:?} ({:?}, {} files)",
        record.key(),
        record.variants.len(),
        record.variants[0].label,
        record.variants[0].package,
        record.variants[0].files.len()
    );
}

#[test]
#[ignore = "reaches ambientCG"]
fn ambientcg_textures_and_hdris() {
    search_then_record("ambientcg", Kind::Texture, "brick");
    search_then_record("ambientcg", Kind::Hdri, "sky");
}

#[test]
#[ignore = "reaches Poly Haven"]
fn polyhaven_textures_hdris_and_models() {
    search_then_record("polyhaven", Kind::Texture, "brick");
    search_then_record("polyhaven", Kind::Hdri, "sunset");
    search_then_record("polyhaven", Kind::Model, "chair");
}

#[test]
#[ignore = "reaches Poly Haven"]
fn a_polyhaven_texture_has_loose_maps_with_md5s() {
    let source = source("polyhaven").unwrap();
    let http = Client::new(Duration::from_secs(60));
    let record = source.record(&http, "red_brick_03").unwrap();
    let maps = record
        .variants
        .iter()
        .find(|variant| variant.package == Package::Maps)
        .expect("loose maps");
    assert!(
        maps.files
            .iter()
            .any(|file| file.map.as_deref() == Some("base_color"))
    );
    assert!(maps.files.iter().all(|file| file.md5.is_some()));
}

#[test]
#[ignore = "reaches Openverse"]
fn openverse_sounds() {
    search_then_record("openverse", Kind::Sound, "door");
}
