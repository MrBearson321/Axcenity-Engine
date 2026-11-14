use super::*;

#[test]
fn a_prop_or_model_tile_is_a_foliage_source() {
    let tree = AssetEntry {
        kind: AssetKind::Prop,
        path: "Tree".into(),
        name: "Tree".into(),
    };
    assert_eq!(
        tree.foliage_source(),
        Some(FoliageSource::Prop(axcenity_core::FoliageProp::Tree))
    );
    let model = AssetEntry {
        kind: AssetKind::Model,
        path: "assets/models/Bush.glb".into(),
        name: "Bush".into(),
    };
    assert_eq!(
        model.foliage_source(),
        Some(FoliageSource::Model("assets/models/Bush.glb".into()))
    );
    let sound = AssetEntry {
        kind: AssetKind::Audio,
        path: "assets/audio/wind.ogg".into(),
        name: "wind".into(),
    };
    assert_eq!(sound.foliage_source(), None);
}

#[test]
fn files_are_sorted_into_kinds_by_extension_and_folder() {
    assert_eq!(kind_of("assets/models/chair.glb"), Some(AssetKind::Model));
    assert_eq!(
        kind_of("assets/models/scan/statue.obj"),
        Some(AssetKind::Model)
    );
    assert_eq!(
        kind_of("assets/models/robot/robot.FBX"),
        Some(AssetKind::Model)
    );
    assert_eq!(
        kind_of("assets/prefabs/Lamp.prefab.json"),
        Some(AssetKind::Prefab)
    );
    assert_eq!(kind_of("assets/audio/ding.ogg"), Some(AssetKind::Audio));
    assert_eq!(kind_of("assets/sky/Noon.HDR"), Some(AssetKind::Sky));
    assert_eq!(kind_of("assets/images/sky.hdr"), Some(AssetKind::Sky));
    assert_eq!(kind_of("assets/sky/dusk.exr"), Some(AssetKind::Sky));
    assert_eq!(
        kind_of("assets/particles/spark_01.png"),
        Some(AssetKind::Particle)
    );
    assert_eq!(
        kind_of("assets/textures/wood.jpg"),
        Some(AssetKind::Texture)
    );
    assert_eq!(
        kind_of("assets/models/tree/textures/bark.png"),
        Some(AssetKind::Texture)
    );
    assert_eq!(kind_of("assets/images/logo.png"), Some(AssetKind::Image));
    assert_eq!(kind_of("assets/notes.txt"), None);
    assert_eq!(kind_of("assets/data.json"), None);
}

#[test]
fn names_drop_the_extension() {
    assert_eq!(
        display_name("assets/prefabs/Street Lamp.prefab.json"),
        "Street Lamp"
    );
    assert_eq!(display_name("assets/models/chair.glb"), "chair");
    assert_eq!(prefab_file_stem("Lamp: big/small"), "Lamp_ big_small");
    assert_eq!(prefab_file_stem("  "), "Prefab");
}

#[test]
fn the_folder_is_walked_for_assets() {
    let folder = std::env::temp_dir().join(format!("axcenity-assets-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    for file in [
        "assets/models/a.glb",
        "assets/audio/b.wav",
        "assets/particles/c.png",
        "assets/.hidden/d.png",
        "assets/readme.md",
    ] {
        let path = folder.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"").unwrap();
    }
    let paths: Vec<String> = scan(&folder).into_iter().map(|entry| entry.path).collect();
    assert_eq!(
        paths,
        [
            "assets/audio/b.wav",
            "assets/models/a.glb",
            "assets/particles/c.png"
        ]
    );
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn a_global_asset_shows_in_every_project_and_is_copied_in_when_used() {
    let temp = |name: &str| {
        let folder = std::env::temp_dir().join(format!(
            "axcenity-panel-global-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        folder
    };
    let (first, second, library) = (temp("first"), temp("second"), temp("library"));
    for file in [
        "assets/models/Hero/Hero.fbx",
        "assets/models/Hero/textures/Hero_Diffuse.png",
        "assets/audio/ding.wav",
    ] {
        let path = first.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"").unwrap();
    }
    let mut browser = AssetBrowser::default();
    browser.set_library(Some(library.clone()));
    browser.open(Some(&first));
    browser.entries();
    assert!(
        !browser.is_global("assets/models/Hero/Hero.fbx"),
        "local by default"
    );

    // Ticking Global asset on the model's folder tile.
    let said = browser
        .set_global("assets/models/Hero/Hero.fbx", true)
        .unwrap();
    assert!(said.contains("Hero is a global asset now"), "{said}");
    browser.entries();
    assert!(browser.is_global("assets/models/Hero/Hero.fbx"));
    assert!(browser.is_global("assets/models/Hero/textures/Hero_Diffuse.png"));
    assert!(!browser.is_global("assets/audio/ding.wav"));

    // Another project lists it (in its folder) before it has a copy.
    browser.open(Some(&second));
    let paths: Vec<String> = browser
        .entries()
        .into_iter()
        .filter(|entry| entry.kind != AssetKind::Prop)
        .map(|entry| entry.path)
        .collect();
    assert_eq!(
        paths,
        [
            "assets/models/Hero/Hero.fbx",
            "assets/models/Hero/textures/Hero_Diffuse.png"
        ]
    );
    assert_eq!(
        packages::packages(&browser.files_and_shared())[0].name,
        "Hero"
    );
    assert_eq!(browser.use_here("assets/models/Hero/Hero.fbx"), Ok(2));
    assert!(second.join("assets/models/Hero/Hero.fbx").is_file());
    assert_eq!(browser.use_here("assets/models/Hero/Hero.fbx"), Ok(0));
    // A prop, or a project file that isn't global, copies nothing.
    assert_eq!(browser.use_here("Crate"), Ok(0));

    // The Global chip shows only global assets; the panel draws them.
    browser.category = Category::Global;
    let ctx = theme::test_context();
    for _ in 0..2 {
        let _ = crate::theme::run(&ctx, Default::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| browser.show(ui, true, false, None));
        });
    }
    if let Some(snapshots) = std::env::var_os("AXCENITY_SNAPSHOTS") {
        browser.category = Category::All;
        browser.open_package = Some("Hero".to_owned());
        crate::snapshot::snapshot(
            &PathBuf::from(snapshots).join("assets-panel-global.png"),
            egui::vec2(720.0, 320.0),
            vec![Vec::new(); 4],
            |ui| {
                egui::CentralPanel::default().show(ui, |ui| browser.show(ui, true, false, None));
            },
        );
    }
    // Unticking takes it out of global assets; both projects keep it.
    browser
        .set_global("assets/models/Hero/Hero.fbx", false)
        .unwrap();
    browser.entries();
    assert!(!browser.is_global("assets/models/Hero/Hero.fbx"));
    assert!(first.join("assets/models/Hero/Hero.fbx").is_file());
    assert!(second.join("assets/models/Hero/Hero.fbx").is_file());
    for folder in [first, second, library] {
        let _ = std::fs::remove_dir_all(folder);
    }
}

#[test]
fn the_search_matches_every_word_in_the_path_or_kind() {
    let door = AssetEntry {
        kind: AssetKind::Audio,
        path: "assets/sounds/Door Creak.ogg".to_owned(),
        name: "Door Creak".to_owned(),
    };
    assert!(search_matches(&door, "door"));
    assert!(search_matches(&door, "sound door"));
    assert!(
        search_matches(&door, "assets/sounds/door creak.ogg"),
        "a revealed path"
    );
    assert!(!search_matches(&door, "texture door"));
}

#[test]
fn a_toolbox_assets_tooltip_says_where_it_came_from_and_its_licence() {
    use axcenity_toolbox::provenance::{Snapshot, SourcedFile};
    use axcenity_toolbox::{AssetRecord, Kind};
    let folder =
        std::env::temp_dir().join(format!("axcenity-assets-sources-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(folder.join("assets/audio")).unwrap();
    std::fs::write(folder.join("assets/audio/rain-1a2b.mp3"), b"").unwrap();
    let rain = AssetRecord {
        provider: "openverse".into(),
        id: "1a2b".into(),
        kind: Kind::Sound,
        name: "Rain".into(),
        authors: vec!["Sam".into()],
        license: "CC-BY-NC".into(),
        license_version: "4.0".into(),
        ..AssetRecord::default()
    };
    let mut snapshot = Snapshot::default();
    snapshot.files.push(SourcedFile {
        path: "assets/audio/rain-1a2b.mp3".into(),
        key: rain.key(),
        variant: "MP3".into(),
        uid: None,
    });
    snapshot.assets.insert(rain.key(), rain);
    snapshot.write(&folder).unwrap();
    let mut browser = AssetBrowser::default();
    browser.open(Some(&folder));
    browser.rescan_if_due();
    let line = browser.source_line("assets/audio/rain-1a2b.mp3").unwrap();
    assert!(
        line.starts_with("From Openverse · CC BY-NC 4.0 · by Sam"),
        "{line}"
    );
    assert!(
        line.contains('\n'),
        "a non-commercial licence says so: {line}"
    );
    assert_eq!(browser.source_line("assets/audio/other.mp3"), None);
    let _ = std::fs::remove_dir_all(&folder);
}

/// Roadmap G10: a plugin's item (`plugin:AddAssetAction`) is in a project
/// asset's right-click menu when it's offered for that kind of file, and
/// picking it reports the asset's path.
#[test]
fn a_plugins_item_in_an_assets_menu_reports_its_path() {
    let folder = std::env::temp_dir().join(format!(
        "axcenity-assets-plugin-menu-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(folder.join("assets/audio")).unwrap();
    std::fs::write(folder.join("assets/audio/wind.ogg"), b"").unwrap();
    let mut browser = AssetBrowser::default();
    browser.open(Some(&folder));
    let action = |id, label: &str, ext: &str| axcenity_plugins::AssetAction {
        id,
        label: label.into(),
        tooltip: String::new(),
        extensions: vec![ext.into()],
    };
    browser.plugin_actions = vec![
        (2, action(7, "Loudness", "ogg")),
        (2, action(8, "Not for sounds", "png")),
    ];
    fn texts(output: &egui::FullOutput) -> Vec<(String, egui::Pos2)> {
        fn walk(shape: &egui::Shape, found: &mut Vec<(String, egui::Pos2)>) {
            match shape {
                egui::Shape::Text(text) => found.push((
                    text.galley.text().to_owned(),
                    text.visual_bounding_rect().center(),
                )),
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| walk(s, found)),
                _ => {}
            }
        }
        let mut found = Vec::new();
        for clipped in &output.shapes {
            walk(&clipped.shape, &mut found);
        }
        found
    }
    let ctx = theme::test_context();
    let mut frame = |events: Vec<egui::Event>| {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(720.0, 480.0),
            )),
            events,
            ..Default::default()
        };
        let mut action = None;
        let output = crate::theme::run(&ctx, input, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                if let Some(done) = browser.show(ui, true, false, None) {
                    action = Some(done);
                }
            });
        });
        (texts(&output), action)
    };
    let click = |at: egui::Pos2, button: egui::PointerButton| {
        [true, false].map(|pressed| egui::Event::PointerButton {
            pos: at,
            button,
            pressed,
            modifiers: egui::Modifiers::NONE,
        })
    };
    frame(Vec::new());
    let (shown, _) = frame(Vec::new());
    let tile = shown
        .iter()
        .find(|(text, _)| text == "wind")
        .unwrap_or_else(|| panic!("no wind tile in {shown:?}"))
        .1;
    frame(vec![egui::Event::PointerMoved(tile)]);
    frame(click(tile, egui::PointerButton::Secondary).to_vec());
    let (menu, _) = frame(Vec::new());
    let item = menu
        .iter()
        .find(|(text, _)| text.ends_with("Loudness"))
        .unwrap_or_else(|| panic!("no Loudness item in {menu:?}"))
        .1;
    assert!(
        !menu
            .iter()
            .any(|(text, _)| text.ends_with("Not for sounds"))
    );
    frame(vec![egui::Event::PointerMoved(item)]);
    let [press, release] = click(item, egui::PointerButton::Primary);
    frame(vec![press]);
    let (_, picked) = frame(vec![release]);
    assert!(
        matches!(
            &picked,
            Some(BrowserAction::PluginAction { plugin: 2, action: 7, path }) if path == "assets/audio/wind.ogg"
        ),
        "{picked:?}"
    );
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn loose_texture_maps_offer_organize_and_each_set_is_one_tile_after() {
    let folder =
        std::env::temp_dir().join(format!("axcenity-assets-organize-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    let terrain = folder.join("assets/textures/terrain");
    std::fs::create_dir_all(&terrain).unwrap();
    let picture = std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/hello-data/assets/textures/sword.png"),
    )
    .unwrap();
    let names = [
        "brown_mud_leaves_01_ao_2k.png",
        "brown_mud_leaves_01_diff_2k.png",
        "brown_mud_leaves_01_nor_gl_2k.png",
        "brown_mud_leaves_01_rough_2k.png",
        "Moss002_2K-JPG_Color.png",
        "Moss002_2K-JPG_NormalGL.png",
        "Moss002_2K-JPG_Roughness.png",
    ];
    for name in names {
        std::fs::write(terrain.join(name), &picture).unwrap();
    }
    let mut browser = AssetBrowser::default();
    browser.open(Some(&folder));
    browser.entries();
    assert_eq!(browser.loose_packs, 2);
    browser.category = Category::Textures;
    let ctx = theme::test_context();
    let draw = |browser: &mut AssetBrowser, file: &str| {
        let mut action = None;
        for _ in 0..2 {
            let _ = crate::theme::run(&ctx, Default::default(), |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    action = browser.show(ui, true, false, None).or(action.take());
                });
            });
        }
        if let Some(snapshots) = std::env::var_os("AXCENITY_SNAPSHOTS") {
            crate::snapshot::snapshot(
                &PathBuf::from(snapshots).join(file),
                egui::vec2(900.0, 300.0),
                vec![Vec::new(); 4],
                |ui| {
                    egui::CentralPanel::default()
                        .show(ui, |ui| browser.show(ui, true, false, None));
                },
            );
        }
        action
    };
    draw(&mut browser, "assets-panel-loose-maps.png");

    // What Organize does to the files (the app also repoints references).
    for pack in axcenity_assets::plan_pack_folders(&folder, &loose_images(&browser.files)) {
        let to: Vec<String> = pack.moves.iter().map(|(_, to)| to.clone()).collect();
        for (from, to) in &pack.moves {
            std::fs::create_dir_all(folder.join(to).parent().unwrap()).unwrap();
            std::fs::rename(folder.join(from), folder.join(to)).unwrap();
        }
        let (path, set) = axcenity_assets::pack_texture_set(&folder, &pack.folder, &pack.name, &to);
        axcenity_assets::write_texture_set(&folder.join(path), &set).unwrap();
    }
    browser.refresh();
    browser.entries();
    assert_eq!(browser.loose_packs, 0);
    let sets: Vec<String> = browser
        .files
        .iter()
        .filter(|entry| entry.kind != AssetKind::Prop)
        .map(|entry| entry.name.clone())
        .collect();
    assert_eq!(sets, ["brown_mud_leaves_01", "Moss002"]);
    draw(&mut browser, "assets-panel-organized.png");
    let _ = std::fs::remove_dir_all(&folder);
}
