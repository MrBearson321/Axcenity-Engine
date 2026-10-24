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
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                actions = studio.show(
                    ui,
                    &StudioContext {
                        world,
                        selected: None,
                        editing: true,
                        time: 0.0,
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
        (Some(TerrainTool::Raise), "terrain-raise.png"),
        (Some(TerrainTool::Paint), "terrain-paint.png"),
        (Some(TerrainTool::Generate), "terrain-generate.png"),
        (Some(TerrainTool::Erode), "terrain-erode.png"),
    ] {
        let mut studio = TerrainStudio {
            tool,
            ..TerrainStudio::default()
        };
        crate::snapshot::snapshot(
            &std::path::PathBuf::from(&folder).join(file),
            egui::vec2(340.0, 900.0),
            vec![Vec::new(); 3],
            |ctx| {
                egui::SidePanel::right("terrain")
                    .default_width(320.0)
                    .show(ctx, |ui| {
                        studio.show(
                            ui,
                            &StudioContext {
                                world: &world,
                                selected: None,
                                editing: true,
                                time: 0.0,
                            },
                        );
                    });
            },
        );
    }
}
