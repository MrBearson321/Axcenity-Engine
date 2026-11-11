//! Searching every source at once, and the Sounds tab's licence notice.

use axcenity_toolbox::http::Fixture;
use axcenity_toolbox::{Licenses, SourceInfo};

use super::*;

/// A source that answers from a list, or fails.
struct Fake {
    info: SourceInfo,
    ids: &'static [&'static str],
    more: bool,
    fails: bool,
}

impl AssetSource for Fake {
    fn info(&self) -> &SourceInfo {
        &self.info
    }

    fn search(&self, _: &dyn Http, query: &Query) -> Result<SearchPage, SourceError> {
        if self.fails {
            return Err(SourceError::Refused("it is down for maintenance"));
        }
        Ok(SearchPage {
            records: self
                .ids
                .iter()
                .map(|id| record(self.info.key, id, query.kind, "CC0"))
                .collect(),
            more: self.more,
            total: None,
        })
    }

    fn record(&self, _: &dyn Http, _: &str) -> Result<AssetRecord, SourceError> {
        Err(SourceError::NotFound)
    }
}

const fn fake(
    key: &'static str,
    name: &'static str,
    ids: &'static [&'static str],
    more: bool,
    fails: bool,
) -> Fake {
    Fake {
        info: SourceInfo {
            key,
            name,
            home: "https://example.com",
            credit: "Example credit.",
            kinds: &[Kind::Texture],
            licenses: Licenses::All("CC0"),
            download_hosts: None,
        },
        ids,
        more,
        fails,
    }
}

static WOOD: Fake = fake("wood", "Wood Source", &["w1", "w2", "w3"], true, false);
static STONE: Fake = fake("stone", "Stone Source", &["s1"], false, false);
static DOWN: Fake = fake("down", "Down Source", &[], false, true);

fn record(provider: &str, id: &str, kind: Kind, license: &str) -> AssetRecord {
    AssetRecord {
        provider: provider.into(),
        id: id.into(),
        kind,
        name: id.into(),
        license: license.into(),
        authors: vec!["Someone".into()],
        variants: vec![Variant {
            label: "1K JPG".into(),
            resolution: Some("1k".into()),
            format: "jpg".into(),
            package: Package::Archive,
            files: vec![axcenity_toolbox::RemoteFile {
                url: "https://example.com/a.zip".into(),
                path: "a.zip".into(),
                ..Default::default()
            }],
        }],
        ..AssetRecord::default()
    }
}

fn keys(listings: &[Listing]) -> Vec<String> {
    listings.iter().map(Listing::key).collect()
}

#[test]
fn every_source_is_asked_and_their_results_take_turns() {
    let found = search(
        &Fixture::default(),
        &[&WOOD, &STONE],
        Kind::Texture,
        "",
        1,
        false,
    )
    .unwrap();
    assert_eq!(
        keys(&found.listings),
        ["wood:w1", "stone:s1", "wood:w2", "wood:w3"]
    );
    // Only the source that said so is asked for a second page.
    assert_eq!(found.more, ["wood"]);
    assert!(found.failed.is_empty());
}

#[test]
fn a_failing_source_is_named_and_the_others_still_show() {
    let found = search(
        &Fixture::default(),
        &[&DOWN, &STONE],
        Kind::Texture,
        "rock",
        1,
        false,
    )
    .unwrap();
    assert_eq!(keys(&found.listings), ["stone:s1"]);
    assert_eq!(found.failed.len(), 1);
    assert!(
        found.failed[0].starts_with("Down Source: "),
        "{:?}",
        found.failed
    );
}

#[test]
fn a_search_fails_only_when_every_source_does() {
    let err = match search(&Fixture::default(), &[&DOWN], Kind::Texture, "", 1, false) {
        Err(err) => err,
        Ok(_) => panic!("the only source failed"),
    };
    assert!(err.starts_with("Down Source: "), "{err}");
}

#[test]
fn a_result_already_shown_is_not_shown_twice() {
    let listing = |provider, id| Listing::from_record(record(provider, id, Kind::Texture, "CC0"));
    let mut shown = vec![listing("wood", "w1")];
    let fresh = merge(
        &mut shown,
        vec![
            listing("wood", "w1"),
            listing("stone", "w1"),
            listing("stone", "w1"),
        ],
    );
    // The same id from another source is another asset.
    assert_eq!(keys(&fresh), ["stone:w1"]);
    assert_eq!(keys(&shown), ["wood:w1", "stone:w1"]);
}

#[test]
fn textures_and_skies_search_every_source_and_sounds_their_only_one() {
    let mut panel = ToolboxPanel::idle();
    assert_eq!(panel.sources.label(), "All sources");
    panel.set_kind(Kind::Sound);
    assert_eq!(panel.sources.label(), "Openverse");
    panel.set_kind(Kind::Hdri);
    assert_eq!(panel.sources.label(), "All sources");
    assert_eq!(panel.sources.list(Kind::Hdri).len(), 2);
}

#[test]
fn the_sound_notice_says_what_paul_asked_for() {
    assert_eq!(
        SOUND_LICENCE_NOTICE,
        "Sound assets obtained through this provider are not licensed for commercial use \
         unless their individual source licenses explicitly permit it. Always review the \
         original asset license before using an asset in a project."
    );
}

/// Draws the panel at `width` and returns the text it drew.
fn draw(panel: &mut ToolboxPanel, width: f32, name: &str) -> String {
    draw_with(panel, width, name, false)
}

/// [`draw`] with the sound notice folded.
fn draw_folded(panel: &mut ToolboxPanel, width: f32) -> String {
    draw_with(panel, width, "sounds-folded", true)
}

fn draw_with(panel: &mut ToolboxPanel, width: f32, name: &str, folded: bool) -> String {
    let size = egui::vec2(width, 900.0);
    let mut drawn = String::new();
    let mut draw = |ui: &mut egui::Ui| {
        if folded {
            ui.ctx().data_mut(|data| {
                data.insert_persisted(egui::Id::new("toolbox sound licence notice folded"), true)
            });
        }
        egui::CentralPanel::default().show(ui, |ui| {
            let _ = panel.show(ui, None, true);
        });
    };
    let ctx = theme::test_context();
    for _ in 0..3 {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            ..Default::default()
        };
        let output = theme::run(&ctx, input, &mut draw);
        drawn.clear();
        for shape in output.shapes {
            collect_text(&shape.shape, &mut drawn);
        }
    }
    if let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS") {
        let path = PathBuf::from(folder).join(format!("toolbox-{name}-{width}.png"));
        axcenity_editor_kit::snapshot::snapshot(&path, size, vec![Vec::new(); 3], &mut draw);
    }
    drawn
}

fn collect_text(shape: &egui::Shape, out: &mut String) {
    match shape {
        egui::Shape::Text(text) => {
            out.push_str(text.galley.text());
            out.push('\n');
        }
        egui::Shape::Vec(shapes) => {
            for shape in shapes {
                collect_text(shape, out);
            }
        }
        _ => {}
    }
}

#[test]
fn the_sounds_tab_shows_the_licence_notice_above_each_sounds_own_licence() {
    for width in [260.0, 640.0] {
        let mut panel = ToolboxPanel::idle();
        panel.set_kind(Kind::Sound);
        let mut nc = record("openverse", "rain", Kind::Sound, "CC-BY-NC");
        nc.name = "Rain on a roof".into();
        let mut free = record("openverse", "bell", Kind::Sound, "CC0");
        free.name = "Bell".into();
        panel.page = Some(Page {
            listings: vec![Listing::from_record(nc), Listing::from_record(free)],
            number: 1,
            more: Vec::new(),
        });
        let drawn = draw(&mut panel, width, "sounds");
        let squashed = drawn.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(
            squashed.contains("not licensed for commercial use"),
            "{squashed}"
        );
        // Each sound keeps its own licence: a CC0 one isn't called
        // non-commercial.
        assert!(squashed.contains("CC BY-NC"), "{squashed}");
        assert!(squashed.contains("CC0"), "{squashed}");
    }
    // Folded, it still says the gist on one line.
    let mut panel = ToolboxPanel::idle();
    panel.set_kind(Kind::Sound);
    let drawn = draw_folded(&mut panel, 260.0);
    assert!(drawn.contains("Not for commercial use"), "{drawn}");
    assert!(!drawn.contains("Always review"), "{drawn}");
    // Textures have no such notice.
    let mut panel = ToolboxPanel::idle();
    let drawn = draw(&mut panel, 640.0, "textures-empty");
    assert!(!drawn.contains("not licensed"));
}

#[test]
fn results_from_every_source_name_theirs_and_can_be_narrowed_to_one() {
    for width in [260.0, 900.0] {
        let mut panel = ToolboxPanel::idle();
        panel.page = Some(Page {
            listings: vec![
                Listing::from_record(record("ambientcg", "Rock030", Kind::Texture, "CC0")),
                Listing::from_record(record("polyhaven", "red_brick", Kind::Texture, "CC0")),
                Listing::from_record(record("ambientcg", "Wood049", Kind::Texture, "CC0")),
            ],
            number: 1,
            more: vec!["polyhaven"],
        });
        let drawn = draw(&mut panel, width, "all-sources");
        assert!(drawn.contains("ambientCG 2"), "{drawn}");
        assert!(drawn.contains("Poly Haven 1"), "{drawn}");
        assert!(drawn.contains("Wood049"), "{drawn}");
        panel.only = Some("polyhaven");
        let drawn = draw(&mut panel, width, "only-polyhaven");
        assert!(drawn.contains("red_brick"), "{drawn}");
        assert!(!drawn.contains("Wood049"), "{drawn}");
    }
}
