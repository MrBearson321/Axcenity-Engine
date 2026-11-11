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
    assert_eq!(browser.use_here("Table"), Ok(0));

    // The Global chip shows only global assets; the panel draws them.
    browser.category = Category::Global;
    let ctx = theme::test_context();
    for _ in 0..2 {
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| browser.show(ui, true, false, None));
        });
    }
    if let Some(snapshots) = std::env::var_os("AXCENITY_SNAPSHOTS") {
        browser.category = Category::All;
        browser.open_package = Some("Hero".to_owned());
        crate::snapshot::snapshot(
            &PathBuf::from(snapshots).join("assets-panel-global.png"),
            egui::vec2(720.0, 320.0),
            vec![Vec::new(); 4],
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| browser.show(ui, true, false, None));
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
