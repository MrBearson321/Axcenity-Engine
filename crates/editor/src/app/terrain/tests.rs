use super::*;

fn app_with_project(name: &str) -> (EditorApp, PathBuf) {
    let parent = std::env::temp_dir().join(format!(
        "axcenity-editor-terrain-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&parent);
    std::fs::create_dir_all(&parent).unwrap();
    let folder = axcenity_project::create_project(&parent, "Terrain Test").unwrap();
    let app = EditorApp::new(Some(&folder));
    assert!(app.project.is_some(), "the project opens");
    (app, parent)
}

fn create(app: &mut EditorApp) -> InstanceId {
    app.terrain_action(TerrainAction::Create {
        width: 128.0,
        length: 64.0,
        height: 32.0,
        samples: 65,
    });
    app.terrain.target.expect("the new terrain is the target")
}

fn pointer(point: Vec3, pressed: bool, down: bool, released: bool) -> TerrainPointer {
    TerrainPointer {
        point: Some(point),
        pressed,
        down,
        released,
        seconds: 1.0 / 60.0,
        ..Default::default()
    }
}

#[test]
fn a_new_terrain_is_one_undo_step_with_square_cells_and_starter_layers() {
    let (mut app, folder) = app_with_project("create");
    let terrain = create(&mut app);
    assert_eq!(app.world.terrain(terrain).unwrap().size(), (65, 33));
    assert_eq!(materials::layers(&app.world, terrain).len(), 3);
    assert_eq!(app.selected, Some(terrain));
    assert_eq!(
        axcenity_project::data_folder(&app.world, terrain).as_deref(),
        Some("assets/terrain/Terrain")
    );
    // The flat ground sits at Y = 0.
    let placement = Placement::of(app.world.get(terrain).unwrap()).unwrap();
    let data = app.world.terrain(terrain).unwrap();
    assert!(placement.ground_y(data, 0.0, 0.0).unwrap().abs() < 1e-4);
    app.undo();
    assert!(crate::terrain_studio::terrains(&app.world).is_empty());
    app.redo();
    assert_eq!(crate::terrain_studio::terrains(&app.world), vec![terrain]);
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn a_stroke_raises_the_ground_and_undoes_as_one_step() {
    let (mut app, folder) = app_with_project("stroke");
    let terrain = create(&mut app);
    app.terrain.tool = Some(TerrainTool::Raise);
    app.terrain.brush.radius = 8.0;
    let ground = |app: &EditorApp, x: f32| {
        let placement = Placement::of(app.world.get(terrain).unwrap()).unwrap();
        placement
            .ground_y(app.world.terrain(terrain).unwrap(), x, 0.0)
            .unwrap()
    };
    app.terrain_pointer(pointer(Vec3::new(-20.0, 0.0, 0.0), true, true, false));
    for step in 1..=20 {
        let x = -20.0 + step as f32 * 2.0;
        app.terrain_pointer(pointer(Vec3::new(x, 0.0, 0.0), false, true, false));
    }
    app.terrain_pointer(pointer(Vec3::new(20.0, 0.0, 0.0), false, false, true));
    assert!(app.terrain.stroke.is_none());
    // Raised all along the path, with no gaps.
    for x in [-20.0, -10.0, 0.0, 10.0, 20.0] {
        assert!(ground(&app, x) > 0.2, "raised at {x}: {}", ground(&app, x));
    }
    assert!(ground(&app, 50.0).abs() < 1e-4, "far away stays");
    assert!(!app.history.is_saved());
    app.undo();
    assert!(
        ground(&app, 0.0).abs() < 1e-4,
        "one undo takes the stroke away"
    );
    app.redo();
    assert!(ground(&app, 0.0) > 0.2);
    // Shift lowers.
    app.terrain_pointer(TerrainPointer {
        modifiers: egui::Modifiers::SHIFT,
        ..pointer(Vec3::new(40.0, 0.0, 0.0), true, true, false)
    });
    app.terrain_pointer(pointer(Vec3::new(40.0, 0.0, 0.0), false, false, true));
    assert!(ground(&app, 40.0) < -0.01);
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn painting_needs_a_layer_and_ctrl_click_picks_one() {
    let (mut app, folder) = app_with_project("paint");
    let terrain = create(&mut app);
    app.terrain.tool = Some(TerrainTool::Paint);
    app.terrain.paint_layer = 2;
    app.terrain.paint_mode = axcenity_terrain::brush::PaintMode::Replace;
    app.terrain.brush.radius = 6.0;
    app.terrain_pointer(pointer(Vec3::ZERO, true, true, false));
    app.terrain_pointer(pointer(Vec3::ZERO, false, false, true));
    let data = app.world.terrain(terrain).unwrap();
    let (x, z) = (data.size().0 / 2, data.size().1 / 2);
    assert!(data.paint()[data.index(x, z)][2] > 200);
    assert_eq!(app.history.undo_label(), Some("Paint Rock"));
    // Ctrl+click on the painted spot picks Rock again.
    app.terrain.paint_layer = 0;
    app.terrain_pointer(TerrainPointer {
        modifiers: egui::Modifiers::COMMAND,
        ..pointer(Vec3::ZERO, true, true, false)
    });
    assert_eq!(app.terrain.paint_layer, 2);
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn painting_foliage_scatters_plants_and_props_and_undo_removes_the_stroke() {
    let (mut app, folder) = app_with_project("foliage");
    let terrain = create(&mut app);
    app.terrain.tool = Some(TerrainTool::Foliage);
    app.terrain.foliage_source =
        axcenity_core::FoliageSource::Plant(axcenity_core::FoliagePlant::Grass);
    app.terrain.foliage_density = 0.4;
    app.terrain.brush.radius = 10.0;
    app.terrain.brush.strength = 1.0;
    app.terrain.brush.falloff = 0.0;
    app.terrain_pointer(pointer(Vec3::ZERO, true, true, false));
    app.terrain_pointer(pointer(Vec3::ZERO, false, false, true));
    let plants = app.world.terrain(terrain).unwrap().foliage().len();
    assert!(plants > 3, "scattered {plants} plants");
    assert_eq!(app.history.undo_label(), Some("Paint Grass Foliage"));
    app.undo();
    assert!(app.world.terrain(terrain).unwrap().foliage().is_empty());
    app.redo();
    assert_eq!(app.world.terrain(terrain).unwrap().foliage().len(), plants);
    assert!(
        app.world
            .terrain(terrain)
            .unwrap()
            .foliage()
            .iter()
            .all(|i| !i.collide)
    );

    app.terrain.foliage_source =
        axcenity_core::FoliageSource::Prop(axcenity_core::FoliageProp::Crate);
    app.terrain.foliage_collide = true;
    app.terrain.foliage_density = 0.15;
    app.terrain_pointer(pointer(Vec3::new(20.0, 0.0, 0.0), true, true, false));
    app.terrain_pointer(pointer(Vec3::new(20.0, 0.0, 0.0), false, false, true));
    let mixed = app.world.terrain(terrain).unwrap().foliage().len();
    assert!(
        mixed > plants,
        "a prop stroke adds more: {mixed} vs {plants}"
    );
    assert!(
        app.world
            .terrain(terrain)
            .unwrap()
            .foliage()
            .iter()
            .any(|i| i.source
                == axcenity_core::FoliageSource::Prop(axcenity_core::FoliageProp::Crate)
                && i.collide)
    );
    app.undo();
    assert_eq!(app.world.terrain(terrain).unwrap().foliage().len(), plants);
    assert!(
        app.world
            .terrain(terrain)
            .unwrap()
            .foliage()
            .iter()
            .all(|i| !i.collide)
    );
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn a_generated_preview_changes_nothing_until_applied_then_saves_and_reloads() {
    let (mut app, folder) = app_with_project("generate");
    let terrain = create(&mut app);
    app.terrain.new_terrain.samples = 65;
    app.terrain.new_terrain.width = 128.0;
    app.terrain.new_terrain.length = 128.0;
    app.terrain.generator = Preset::MountainRange.settings();
    app.terrain.generator.seed = 7;
    app.start_generate();
    let ctx = egui::Context::default();
    while app.terrain.job.is_some() {
        std::thread::sleep(Duration::from_millis(5));
        app.poll_terrain_job(&ctx);
    }
    let preview = app.terrain.preview.as_ref().expect("a preview");
    assert_eq!(preview.data.size(), (65, 65));
    assert_eq!(
        app.world.terrain(terrain).unwrap().size(),
        (65, 33),
        "not applied yet"
    );
    let made = preview.data.heights().to_vec();

    app.terrain_action(TerrainAction::ApplyPreview);
    let data = app.world.terrain(terrain).unwrap().clone();
    assert_eq!(data.heights(), made.as_slice());
    let settings = GeneratorSettings::from_json(data.generation.as_deref().unwrap()).unwrap();
    assert_eq!(
        (settings.seed, settings.version),
        (7, generate::GENERATOR_VERSION)
    );

    assert!(app.save());
    let project = app.project.as_ref().unwrap().folder.clone();
    let reopened = EditorApp::new(Some(&project));
    let id = crate::terrain_studio::terrains(&reopened.world)[0];
    let loaded = reopened.world.terrain(id).unwrap();
    assert_eq!(loaded.size(), (65, 65));
    let worst = loaded
        .heights()
        .iter()
        .zip(&made)
        .map(|(a, b)| (a - b).abs())
        .fold(0.0, f32::max);
    assert!(worst < 1e-4, "saved to 16 bits: off by {worst}");

    // Undo puts the old landscape and size back.
    app.undo();
    assert_eq!(app.world.terrain(terrain).unwrap().size(), (65, 33));
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn texture_names_fill_the_right_maps() {
    let roles = axcenity_assets::texture_roles("Rock_ORM.png");
    let edits = layer_map_edits(&roles, "assets/textures/Rock_ORM.png");
    assert!(edits.contains(&Edit::Property(
        "RoughnessChannel",
        Value::TextureChannel(TextureChannel::Green)
    )));
    assert!(edits.contains(&Edit::Property(
        "AmbientOcclusionMap",
        Value::String("assets/textures/Rock_ORM.png".to_owned())
    )));
}

#[test]
fn layers_move_past_each_other() {
    let (mut app, folder) = app_with_project("layers");
    let terrain = create(&mut app);
    let names = |app: &EditorApp| -> Vec<String> {
        materials::layers(&app.world, terrain)
            .into_iter()
            .map(|id| app.world.get(id).unwrap().name().to_string())
            .collect()
    };
    assert_eq!(names(&app), ["Grass", "Dirt", "Rock"]);
    let rock = materials::layers(&app.world, terrain)[2];
    app.terrain_action(TerrainAction::MoveLayer(rock, true));
    assert_eq!(names(&app), ["Grass", "Rock", "Dirt"]);
    app.terrain_action(TerrainAction::MoveLayer(rock, false));
    assert_eq!(names(&app), ["Grass", "Dirt", "Rock"]);
    app.terrain_action(TerrainAction::AddLayer(
        crate::terrain_studio::LayerPreset {
            ..layers_for(Preset::Island)[0]
        },
    ));
    assert_eq!(names(&app).len(), 4);
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn a_project_texture_set_fills_the_chosen_layer_as_one_undo_step() {
    let (mut app, folder) = app_with_project("texture-set");
    let terrain = create(&mut app);
    let dirt = materials::layers(&app.world, terrain)[1];
    let project = app.project.as_ref().unwrap().folder.clone();
    let set = axcenity_assets::texture_set_from_files(
        "test",
        "Mud",
        "",
        &[
            ("Mud_Color.png", "assets/textures/Mud/Mud_Color.png"),
            ("Mud_NormalGL.png", "assets/textures/Mud/Mud_NormalGL.png"),
        ],
    );
    std::fs::create_dir_all(project.join("assets/textures/Mud")).unwrap();
    let path = "assets/textures/Mud/Mud.texture.json";
    axcenity_assets::write_texture_set(&project.join(path), &set).unwrap();
    let map = |app: &EditorApp, name| {
        app.world
            .get(dirt)
            .unwrap()
            .property(name)
            .and_then(Value::as_str)
            .unwrap()
            .to_owned()
    };
    let before = map(&app, "ColorMap");

    let entry = crate::asset_browser::AssetEntry {
        kind: crate::asset_browser::AssetKind::TextureSet,
        path: path.to_owned(),
        name: "Mud".to_owned(),
    };
    app.terrain_action(TerrainAction::UseTextureSet(dirt, entry));
    assert_eq!(map(&app, "ColorMap"), "assets/textures/Mud/Mud_Color.png");
    assert_eq!(
        map(&app, "NormalMap"),
        "assets/textures/Mud/Mud_NormalGL.png"
    );
    // The other layers are left alone.
    let grass = materials::layers(&app.world, terrain)[0];
    assert_ne!(
        app.world
            .get(grass)
            .unwrap()
            .property("ColorMap")
            .and_then(Value::as_str),
        Some("assets/textures/Mud/Mud_Color.png")
    );
    app.undo();
    assert_eq!(map(&app, "ColorMap"), before);
    std::fs::remove_dir_all(folder).unwrap();
}
