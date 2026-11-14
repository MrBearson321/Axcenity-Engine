use super::*;

#[test]
fn the_target_follows_the_selection_and_falls_back_to_the_first_terrain() {
    let mut world = World::new();
    let make = |world: &mut World| {
        let id = world.create(ClassName::Terrain).unwrap();
        world.set_parent(id, Some(world.workspace())).unwrap();
        id
    };
    let a = make(&mut world);
    let b = make(&mut world);
    let layer = world.create(ClassName::TerrainLayer).unwrap();
    world.set_parent(layer, Some(b)).unwrap();
    let mut studio = TerrainStudio::default();
    assert_eq!(studio.resolve_target(&world, None), Some(a));
    // Selecting a layer edits its terrain, and it stays picked after.
    assert_eq!(studio.resolve_target(&world, Some(layer)), Some(b));
    assert_eq!(studio.resolve_target(&world, None), Some(b));
    world.destroy(b).unwrap();
    assert_eq!(studio.resolve_target(&world, None), Some(a));
}

#[test]
fn only_sculpting_and_painting_tools_brush_the_viewport() {
    let brushes: Vec<TerrainTool> = TerrainTool::all().filter(|t| t.is_brush()).collect();
    assert_eq!(brushes.len(), 8);
    assert!(!TerrainTool::Generate.is_brush());
    let keys: Vec<_> = TerrainTool::all().filter_map(TerrainTool::key).collect();
    let mut unique = keys.clone();
    unique.dedup();
    assert_eq!(keys, unique);
}

#[test]
fn the_panel_draws_with_and_without_a_terrain() {
    let ctx = crate::theme::test_context();
    let mut world = World::new();
    let mut studio = TerrainStudio::default();
    let draw = |studio: &mut TerrainStudio, world: &World| {
        let mut actions = Vec::new();
        let _ = crate::theme::run(&ctx, Default::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                actions = studio.show(
                    ui,
                    &StudioContext {
                        world,
                        selected: None,
                        editing: true,
                        time: 0.0,
                        textures: &[],
                        pictures: &HashMap::new(),
                    },
                );
            });
        });
        actions
    };
    assert!(draw(&mut studio, &world).is_empty());
    let terrain = world.create(ClassName::Terrain).unwrap();
    world.set_parent(terrain, Some(world.workspace())).unwrap();
    let layer = world.create(ClassName::TerrainLayer).unwrap();
    world.set_parent(layer, Some(terrain)).unwrap();
    studio.open_layer = Some(layer);
    for tool in TerrainTool::all() {
        studio.tool = Some(tool);
        draw(&mut studio, &world);
    }
}

/// With `AXCENITY_SNAPSHOTS` set, saves the panel with each of a few
/// tools picked, to look at.
#[test]
fn the_panel_snapshots_with_its_main_tools() {
    let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS") else {
        return;
    };
    let mut world = World::new();
    let terrain = world.create(ClassName::Terrain).unwrap();
    world.set_parent(terrain, Some(world.workspace())).unwrap();
    for preset in layers_for(Preset::MountainRange) {
        let layer = world.create(ClassName::TerrainLayer).unwrap();
        for (name, value) in preset.properties(128.0) {
            world.set_property(layer, name, value).unwrap();
        }
        world.set_name(layer, preset.name).unwrap();
        world.set_parent(layer, Some(terrain)).unwrap();
    }
    for (tool, file) in [
        (None, "terrain-none.png"),
        (Some(TerrainTool::Smooth), "terrain-wide.png"),
        (Some(TerrainTool::Raise), "terrain-raise.png"),
        (Some(TerrainTool::Paint), "terrain-paint.png"),
        (Some(TerrainTool::Generate), "terrain-generate.png"),
        (Some(TerrainTool::Erode), "terrain-erode.png"),
    ] {
        let mut studio = TerrainStudio {
            tool,
            ..TerrainStudio::default()
        };
        // The wide one shows the tools' names.
        let width = if file == "terrain-wide.png" {
            520.0
        } else {
            340.0
        };
        crate::snapshot::snapshot(
            &std::path::PathBuf::from(&folder).join(file),
            egui::vec2(width, 900.0),
            vec![Vec::new(); 3],
            |ui| {
                egui::Panel::right("terrain")
                    .default_size(width - 20.0)
                    .show(ui, |ui| {
                        studio.show(
                            ui,
                            &StudioContext {
                                world: &world,
                                selected: None,
                                editing: true,
                                time: 0.0,
                                textures: &[],
                                pictures: &HashMap::new(),
                            },
                        );
                    });
            },
        );
    }
}

/// With `AXCENITY_SNAPSHOTS` set, saves an open layer and the menu its
/// texture pickers show, to look at.
#[test]
fn an_open_layer_and_its_texture_menu_snapshot() {
    let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS") else {
        return;
    };
    let folder = std::path::PathBuf::from(folder);
    let mut world = World::new();
    let terrain = world.create(ClassName::Terrain).unwrap();
    world.set_parent(terrain, Some(world.workspace())).unwrap();
    let mut first = None;
    for preset in layers_for(Preset::MountainRange) {
        let layer = world.create(ClassName::TerrainLayer).unwrap();
        for (name, value) in preset.properties(128.0) {
            world.set_property(layer, name, value).unwrap();
        }
        world.set_name(layer, preset.name).unwrap();
        world.set_parent(layer, Some(terrain)).unwrap();
        first.get_or_insert(layer);
    }
    // A layer with a roughness map, whose slider then scales it.
    world
        .set_property(
            first.unwrap(),
            "RoughnessMap",
            Value::String("assets/textures/Ground108/Ground108_Roughness.jpg".into()),
        )
        .unwrap();
    let pick = |kind, path: &str| TexturePick {
        entry: AssetEntry {
            kind,
            path: path.to_owned(),
            name: crate::asset_browser::display_name(path),
        },
        thumbnail: None,
    };
    let textures = [
        pick(
            AssetKind::TextureSet,
            "assets/textures/Grass001/Grass001.texture.json",
        ),
        pick(
            AssetKind::TextureSet,
            "assets/textures/Ground108/Ground108.texture.json",
        ),
        pick(AssetKind::Texture, "assets/textures/checker.png"),
    ];
    let mut studio = TerrainStudio {
        open_layer: first,
        ..TerrainStudio::default()
    };
    crate::snapshot::snapshot(
        &folder.join("terrain-layer.png"),
        egui::vec2(300.0, 900.0),
        vec![Vec::new(); 3],
        |ui| {
            egui::Panel::right("terrain")
                .default_size(280.0)
                .show(ui, |ui| {
                    studio.show(
                        ui,
                        &StudioContext {
                            world: &world,
                            selected: None,
                            editing: true,
                            time: 0.0,
                            textures: &textures,
                            pictures: &HashMap::new(),
                        },
                    );
                });
        },
    );
    let sets: Vec<&TexturePick> = textures.iter().take(2).collect();
    let mut search = String::new();
    crate::snapshot::snapshot(
        &folder.join("terrain-texture-menu.png"),
        egui::vec2(280.0, 160.0),
        vec![Vec::new(); 3],
        |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let _ = layers::texture_menu(ui, &sets, &mut search, "None");
            });
        },
    );
}

/// A wide panel names the tools beside their icons; a narrow one keeps
/// the icons (names on hover), so it still fits a 260-point column.
#[test]
fn a_wide_panel_names_its_tools() {
    let mut world = World::new();
    let terrain = world.create(ClassName::Terrain).unwrap();
    world.set_parent(terrain, Some(world.workspace())).unwrap();
    let names_drawn = |width: f32| {
        let ctx = crate::theme::test_context();
        let mut studio = TerrainStudio::default();
        let mut output = None;
        for _ in 0..2 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(width, 900.0),
                )),
                ..Default::default()
            };
            output = Some(crate::theme::run(&ctx, input, |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    studio.show(
                        ui,
                        &StudioContext {
                            world: &world,
                            selected: None,
                            editing: true,
                            time: 0.0,
                            textures: &[],
                            pictures: &HashMap::new(),
                        },
                    );
                });
            }));
        }
        fn walk(shape: &egui::Shape, found: &mut Vec<String>) {
            match shape {
                egui::Shape::Text(text) => found.push(text.galley.text().to_owned()),
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| walk(s, found)),
                _ => {}
            }
        }
        let mut found = Vec::new();
        for clipped in &output.unwrap().shapes {
            walk(&clipped.shape, &mut found);
        }
        found.iter().any(|text| text.contains("Raise / Lower"))
    };
    assert!(names_drawn(600.0));
    assert!(!names_drawn(260.0));
}

/// The texts drawn by the panel at 260 points wide with `selected`.
fn texts_drawn(
    world: &World,
    selected: Option<InstanceId>,
    studio: &mut TerrainStudio,
) -> Vec<String> {
    let ctx = crate::theme::test_context();
    let mut output = None;
    for _ in 0..2 {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(260.0, 2400.0),
            )),
            ..Default::default()
        };
        output = Some(crate::theme::run(&ctx, input, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                studio.show(
                    ui,
                    &StudioContext {
                        world,
                        selected,
                        editing: true,
                        time: 0.0,
                        textures: &[],
                        pictures: &HashMap::new(),
                    },
                );
            });
        }));
    }
    fn walk(shape: &egui::Shape, found: &mut Vec<String>) {
        match shape {
            egui::Shape::Text(text) => found.push(text.galley.text().to_owned()),
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| walk(s, found)),
            _ => {}
        }
    }
    let mut found = Vec::new();
    for clipped in &output.unwrap().shapes {
        walk(&clipped.shape, &mut found);
    }
    found
}

#[test]
fn a_selected_biome_shows_its_settings_and_paint_and_its_rules_say_where_they_are() {
    let mut world = World::new();
    let terrain = world.create(ClassName::Terrain).unwrap();
    world.set_parent(terrain, Some(world.workspace())).unwrap();
    for name in ["Grass", "Forest Floor"] {
        let layer = world.create(ClassName::TerrainLayer).unwrap();
        world.set_parent(layer, Some(terrain)).unwrap();
        world.set_name(layer, name).unwrap();
    }
    let biome = world.create(ClassName::Biome).unwrap();
    world.set_parent(biome, Some(terrain)).unwrap();
    world.set_name(biome, "Lowland Forest").unwrap();
    world
        .set_property(biome, "GroundLayer", Value::String("Forest Floor".into()))
        .unwrap();
    let rule = world.create(ClassName::ScatterRule).unwrap();
    world.set_parent(rule, Some(biome)).unwrap();
    world.set_name(rule, "Trees").unwrap();
    let mut studio = TerrainStudio::default();
    let found = texts_drawn(&world, Some(biome), &mut studio);
    for text in [
        "Biomes",
        "Lowland Forest",
        "Paint this biome",
        "1. Trees (in Lowland Forest)",
    ] {
        assert!(
            found.iter().any(|t| t.contains(text)),
            "{text} in {found:?}"
        );
    }
    assert!(
        found
            .iter()
            .any(|t| t.contains("Forest Floor · any height · 1 rule")),
        "{found:?}"
    );
    assert!(
        found
            .iter()
            .any(|t| t.contains("Add rule to Lowland Forest"))
    );
    assert_eq!(studio.target, Some(terrain), "a biome selects its terrain");
}
