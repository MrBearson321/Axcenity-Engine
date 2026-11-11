use std::path::PathBuf;

use axcenity_assets::{MapFile, MaterialFile, MaterialMaps, material_json};
use axcenity_toolbox::db::Database;
use axcenity_toolbox::provenance::SourcedFile;

use super::*;
use crate::asset_browser::packages::packages;
use crate::asset_browser::scan;

fn record(provider: &str, id: &str, kind: Kind, license: &str) -> AssetRecord {
    AssetRecord {
        provider: provider.into(),
        id: id.into(),
        kind,
        name: id.into(),
        license: license.into(),
        authors: vec!["Someone".into()],
        url: format!("https://example.com/{id}"),
        ..AssetRecord::default()
    }
}

fn download(record: &AssetRecord, variant: &str, bytes: u64) -> Download {
    Download {
        key: record.key(),
        variant: variant.into(),
        folder: format!(
            "{}/{}/{}/{variant}",
            directory::kind_folder(record.kind),
            record.provider,
            record.id
        ),
        files: Vec::new(),
        bytes,
        source_urls: Vec::new(),
        fetched: axcenity_toolbox::db::now(),
    }
}

/// A temporary folder, deleted when dropped.
struct Temp(PathBuf);

impl Temp {
    fn new(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("axcenity-inventory-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A project with an ambientCG texture set got twice (1K, then 2K) and a
/// material made from it, a sky that was installed and then deleted, an
/// imported model with its textures, an imported sound, an imported clip
/// folder, and things made in the editor (a prefab, a plain material).
fn project(temp: &Temp) -> PathBuf {
    let folder = temp.0.join("Project");
    for file in [
        "assets/textures/Rock030/Rock030.texture.json",
        "assets/textures/Rock030/Rock030_Color.jpg",
        "assets/models/Hero/Hero.fbx",
        "assets/models/Hero/textures/Hero_Diffuse.png",
        "assets/models/Hero/textures/Hero_Normal.png",
        "assets/audio/ding.wav",
        "assets/animations/Dance/Dance.anim.json",
        "assets/prefabs/Lamp.prefab.json",
        "assets/materials/Plain.material.json",
    ] {
        let path = folder.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"{}").unwrap();
    }
    let material = MaterialFile {
        maps: MaterialMaps {
            color: Some(MapFile::of("assets/textures/Rock030/Rock030_Color.jpg")),
            ..MaterialMaps::default()
        },
        ..MaterialFile::default()
    };
    std::fs::write(
        folder.join("assets/materials/Rock030.material.json"),
        material_json(&material),
    )
    .unwrap();
    let mut snapshot = Snapshot::default();
    for (path, record) in [
        (
            "assets/textures/Rock030",
            record("ambientcg", "Rock030", Kind::Texture, "CC0"),
        ),
        (
            "assets/sky/sunset_2k.hdr",
            record("polyhaven", "sunset", Kind::Hdri, "CC0"),
        ),
    ] {
        snapshot.files.push(SourcedFile {
            path: path.into(),
            key: record.key(),
            variant: "2K".into(),
            uid: None,
        });
        snapshot.assets.insert(record.key(), record);
    }
    snapshot.write(&folder).unwrap();
    folder
}

fn items(folder: &Path, library: Vec<(AssetRecord, Vec<Download>)>) -> Vec<Item> {
    let files = scan(folder);
    let packages = packages(&files);
    let snapshot = Snapshot::read(folder).unwrap();
    let origins = Origins::work_out(Some(folder), &files, &[], &packages, &snapshot);
    let rock = record("ambientcg", "Rock030", Kind::Texture, "CC0");
    let installs = ["1K", "2K"]
        .iter()
        .map(|variant| Install {
            project: provenance::project_key(folder),
            path: "assets/textures/Rock030".into(),
            key: rock.key(),
            variant: (*variant).into(),
            uid: None,
            installed_at: 1,
        })
        .collect();
    gather(&Inputs {
        library,
        installs,
        snapshot: &snapshot,
        files: &files,
        packages: &packages,
        origins: &origins,
    })
}

fn library() -> Vec<(AssetRecord, Vec<Download>)> {
    let rock = record("ambientcg", "Rock030", Kind::Texture, "CC0");
    let forest = record("polyhaven", "forest", Kind::Hdri, "CC0");
    let rain = record("openverse", "rain", Kind::Sound, "CC-BY-NC");
    vec![
        (
            rock.clone(),
            vec![
                download(&rock, "2K-JPG", 30_000_000),
                download(&rock, "1K-JPG", 8_000_000),
            ],
        ),
        (
            forest.clone(),
            vec![download(&forest, "4K-HDR", 90_000_000)],
        ),
        (rain.clone(), vec![download(&rain, "MP3", 400_000)]),
    ]
}

fn row<'a>(items: &'a [Item], name: &str) -> &'a Item {
    items
        .iter()
        .find(|item| item.name == name)
        .unwrap_or_else(|| panic!("no row {name}: {items:#?}"))
}

#[test]
fn each_asset_is_one_row_however_often_it_was_downloaded_or_imported() {
    let temp = Temp::new("rows");
    let folder = project(&temp);
    let items = items(&folder, library());
    let rock = items
        .iter()
        .filter(|item| item.key == "ambientcg:Rock030")
        .collect::<Vec<_>>();
    assert_eq!(rock.len(), 1, "{items:#?}");
    let rock = rock[0];
    assert_eq!(rock.category, Category::Textures);
    assert_eq!(rock.downloads.len(), 2);
    assert_eq!(rock.bytes(), 38_000_000);
    assert_eq!(rock.files, ["assets/textures/Rock030/Rock030.texture.json"]);
    assert_eq!(rock.badges(), ["Downloaded", "Imported", "In project"]);
    assert_eq!(rock.origin, Origin::Toolbox("ambientcg".into()));
}

#[test]
fn rows_are_grouped_by_type_with_their_status_and_relations() {
    let temp = Temp::new("types");
    let folder = project(&temp);
    let items = items(&folder, library());
    let summary: Vec<(Category, &str)> = items
        .iter()
        .map(|item| (item.category, item.name.as_str()))
        .collect();
    assert_eq!(
        summary,
        [
            (Category::Models, "Hero"),
            (Category::Textures, "Rock030"),
            (Category::Materials, "Rock030"),
            (Category::Hdris, "forest"),
            (Category::Hdris, "sunset"),
            (Category::Sounds, "ding"),
            (Category::Sounds, "rain"),
            (Category::Animations, "Dance"),
        ],
        "made-in-editor assets (the prefab, the plain material) aren't listed"
    );
    let hero = row(&items, "Hero");
    assert_eq!(hero.files.len(), 3);
    assert_eq!(hero.note.as_deref(), Some("1 model, 2 textures"));
    assert_eq!(hero.badges(), ["Imported", "In project"]);
    assert_eq!(hero.origin, Origin::Imported);
    let material = items
        .iter()
        .find(|item| item.category == Category::Materials)
        .unwrap();
    assert_eq!(
        material.note.as_deref(),
        Some("Made from the texture set Rock030")
    );
    assert_eq!(material.origin, Origin::Toolbox("ambientcg".into()));
    // Downloaded on this computer, never brought into this project.
    assert_eq!(row(&items, "forest").badges(), ["Downloaded"]);
    // Installed (another computer's snapshot says so), but its file is gone.
    let sunset = row(&items, "sunset");
    assert_eq!(sunset.badges(), ["Removed from project"]);
    assert!(sunset.record.is_some());
}

#[test]
fn the_filters_pick_downloaded_imported_type_source_and_words() {
    let temp = Temp::new("filters");
    let folder = project(&temp);
    let mut panel = AssetInventory::default();
    panel.set_items(items(&folder, library()));
    let names = |panel: &AssetInventory| -> Vec<String> {
        panel.shown().iter().map(|item| item.name.clone()).collect()
    };
    assert_eq!(names(&panel).len(), 8);
    panel.showing = Showing::Downloaded;
    assert_eq!(names(&panel), ["Rock030", "forest", "rain"]);
    panel.showing = Showing::Imported;
    assert_eq!(
        names(&panel),
        ["Hero", "Rock030", "Rock030", "sunset", "ding", "Dance"]
    );
    panel.showing = Showing::All;
    panel.category = Some(Category::Sounds);
    assert_eq!(names(&panel), ["ding", "rain"]);
    panel.category = None;
    panel.source = Some(Origin::Toolbox("polyhaven".into()));
    assert_eq!(names(&panel), ["forest", "sunset"]);
    panel.source = None;
    panel.search = "someone rock".into();
    assert_eq!(names(&panel), ["Rock030"]);
}

#[test]
fn remove_download_deletes_the_librarys_copies_and_keeps_the_record() {
    let temp = Temp::new("remove");
    let dir = AssetDirectory::open(&temp.0.join("Assets")).unwrap();
    let db = Database::in_memory().unwrap();
    let forest = record("polyhaven", "forest", Kind::Hdri, "CC0");
    db.put_asset(&forest).unwrap();
    let fetched = download(&forest, "4K-HDR", 9);
    db.put_download(&fetched).unwrap();
    let files = dir.path(&fetched.folder);
    std::fs::create_dir_all(&files).unwrap();
    std::fs::write(files.join("forest_4k.hdr"), b"hdr").unwrap();
    let item = Item {
        key: forest.key(),
        name: "forest".into(),
        category: Category::Hdris,
        origin: Origin::Toolbox("polyhaven".into()),
        record: Some(forest.clone()),
        downloads: vec![fetched.clone()],
        imported: false,
        files: Vec::new(),
        note: None,
    };
    remove_from(&dir, &db, &item).unwrap();
    assert!(!files.exists());
    assert!(db.downloads_of(&forest.key()).unwrap().is_empty());
    assert_eq!(db.asset(&forest.key()).unwrap(), Some(forest.clone()));
    // A folder outside the library is refused.
    let mut outside = item.clone();
    outside.downloads[0].folder = "../elsewhere".into();
    assert!(remove_from(&dir, &db, &outside).is_err());
}

/// The tab drawn narrow and wide, with a row's details open (PNGs with
/// `AXCENITY_SNAPSHOTS`); the text drawn is checked.
#[test]
fn the_tab_lists_types_and_shows_the_selected_assets_details() {
    let temp = Temp::new("draw");
    let folder = project(&temp);
    let all = items(&folder, library());
    for width in [260.0, 900.0] {
        let mut panel = AssetInventory::default();
        panel.set_items(all.clone());
        panel.select("openverse:rain");
        let size = egui::vec2(width, 900.0);
        let mut drawn = String::new();
        let mut draw = |ui: &mut egui::Ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let _ = panel.show(ui, true);
            });
        };
        let ctx = theme::test_context();
        for _ in 0..3 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                ..Default::default()
            };
            let output = theme::run(&ctx, input, &mut draw);
            drawn = format!("{:?}", output.shapes);
        }
        if let Some(snapshots) = std::env::var_os("AXCENITY_SNAPSHOTS") {
            crate::snapshot::snapshot(
                &PathBuf::from(snapshots).join(format!("inventory-{width}.png")),
                size,
                vec![Vec::new(); 3],
                &mut draw,
            );
        }
        for text in [
            "MODELS",
            "TEXTURES",
            "SOUNDS",
            "IN THE ASSET LIBRARY",
            "Not for a commercial game",
        ] {
            assert!(drawn.contains(text), "{text} at {width}");
        }
    }
}
