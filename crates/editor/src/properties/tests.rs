use super::*;
use axcenity_core::ClassName;

fn titles(sections: &[Section]) -> Vec<&'static str> {
    sections.iter().map(|section| section.title).collect()
}

#[test]
fn part_properties_are_grouped_like_studio_and_none_are_left_out() {
    let mut world = World::new();
    let part = world.create(ClassName::Part).unwrap();
    let instance = world.get(part).unwrap();

    let sections = sections(instance);
    assert_eq!(
        titles(&sections),
        ["Appearance", "Behavior", "Physics", "Part", "Tiling"]
    );
    let names: Vec<&str> = sections
        .iter()
        .flat_map(|section| section.properties.iter().map(|(name, _)| *name))
        .collect();
    assert_eq!(
        names,
        [
            "Color",
            "Material",
            "MaterialVariant",
            "BlueprintTexture",
            "Metalness",
            "Roughness",
            "Transparency",
            "EmissiveColor",
            "Emission",
            "CastShadow",
            "Locked",
            "Anchored",
            "CanCollide",
            "CanTouch",
            "CanQuery",
            "CollisionGroup",
            "WindAffected",
            "CustomPhysicalProperties",
            "Massless",
            "LinearDrag",
            "AngularDrag",
            "RollingFriction",
            "AssemblyLinearVelocity",
            "AssemblyAngularVelocity",
            "Shape",
            "Position",
            "Orientation",
            "Size",
            "TileSize"
        ]
    );
    // Every property shows exactly once, with its current value.
    assert_eq!(names.len(), instance.properties().count());
    for section in &sections {
        for (name, value) in &section.properties {
            assert_eq!(instance.property(name), Some(value));
        }
    }
}

#[test]
fn ui_properties_only_appear_in_the_ui_section_for_ui_instances() {
    let mut world = World::new();
    let frame = world.create(ClassName::Frame).unwrap();
    let sections = sections(world.get(frame).unwrap());

    assert_eq!(titles(&sections), ["UI"]);
    assert!(
        sections[0]
            .properties
            .iter()
            .any(|(name, _)| *name == "Position")
    );
}

#[test]
fn surface_appearances_show_their_maps_then_how_they_are_read() {
    let mut world = World::new();
    let look = world.create(ClassName::SurfaceAppearance).unwrap();
    let sections = sections(world.get(look).unwrap());
    assert_eq!(
        titles(&sections),
        ["Appearance", "Surface", "Map Settings", "Tiling"]
    );
    let placement: Vec<&str> = sections[3]
        .properties
        .iter()
        .map(|(name, _)| *name)
        .collect();
    assert_eq!(placement, ["Tiling", "Offset"]);
    let maps: Vec<&str> = sections[1]
        .properties
        .iter()
        .map(|(name, _)| *name)
        .filter(|name| is_map(name))
        .collect();
    assert_eq!(maps.len(), 8);
    assert!(!is_map("NormalMapDirectX") && !is_map("MaterialName"));
}

#[test]
fn the_workspace_shows_only_its_gravity_wind_and_decal_cap_and_other_services_nothing() {
    let world = World::new();
    let workspace = world.get(world.workspace()).unwrap();
    let sections = sections(workspace);
    assert_eq!(
        titles(&sections),
        ["Physics", "Wind", "Decals", "Streaming"]
    );
    assert_eq!(sections[0].properties[0].0, "Gravity");
    assert_eq!(sections[1].properties[0].0, "GlobalWind");
    assert_eq!(sections[2].properties[0].0, "MaxSpawnedDecals");
    let storage = world.get(world.replicated_storage()).unwrap();
    assert!(super::sections(storage).is_empty());
}

#[test]
fn parts_can_be_renamed_here_scripts_only_in_the_explorer_and_services_never() {
    let mut world = World::new();
    let part = world.create(ClassName::Part).unwrap();
    let script = world.create(ClassName::Script).unwrap();
    assert!(can_rename_here(world.get(part).unwrap()));
    assert!(can_rename(world.get(script).unwrap()));
    assert!(!can_rename_here(world.get(script).unwrap()));
    assert!(!can_rename(world.get(world.workspace()).unwrap()));
    assert!(!can_rename(world.get(world.root()).unwrap()));
}

mod attribute_section_tests {
    use super::*;

    /// A Folder with an attribute of every kind and some tags draws its
    /// Attributes section and Tags row inside a 260-point column, and
    /// clicking nothing makes no edit. With `AXCENITY_SNAPSHOTS` set it's
    /// saved as `properties-attributes.png`.
    #[test]
    fn attributes_and_tags_draw_in_a_narrow_column() {
        let mut world = World::new();
        let folder = world.create(ClassName::Folder).unwrap();
        world.set_parent(folder, Some(world.workspace())).unwrap();
        for (name, make) in ATTRIBUTE_TYPES {
            world.set_attribute(folder, name, Some(make())).unwrap();
        }
        world.add_tag(folder, "Lava").unwrap();
        world.add_tag(folder, "Hazard").unwrap();
        let size = egui::vec2(260.0, 1100.0);
        let mut fit = None;
        let mut edit = None;
        let mut draw = |ui: &mut egui::Ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let room = ui.max_rect();
                edit = show(
                    ui,
                    &world,
                    &Assets::default(),
                    Some(folder),
                    false,
                    &[],
                    |_| {},
                );
                fit = Some((ui.min_rect(), room));
            });
        };
        let ctx = theme::test_context();
        for _ in 0..3 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                ..Default::default()
            };
            let _ = crate::theme::run(&ctx, input, |ui| draw(ui));
        }
        if let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS") {
            crate::snapshot::snapshot(
                &std::path::PathBuf::from(folder).join("properties-attributes.png"),
                size,
                vec![Vec::new(); 3],
                &mut draw,
            );
        }
        let (used, room) = fit.unwrap();
        assert!(
            used.width() <= room.width() + 0.5,
            "{} in {}",
            used.width(),
            room.width()
        );
        assert_eq!(edit, None);
    }
}

mod physics_section_tests {
    use super::*;

    /// A Rubber Part with its own physical properties draws the Physics
    /// section (Custom on, five numbers, Massless and both velocities)
    /// inside a 260-point column. With `AXCENITY_SNAPSHOTS` set it's saved
    /// as `properties-physics.png`.
    #[test]
    fn a_parts_physics_section_draws_in_a_narrow_column() {
        let mut world = World::new();
        let part = world.create(ClassName::Part).unwrap();
        world.set_parent(part, Some(world.workspace())).unwrap();
        world
            .set_property(part, "Material", Value::Material(Material::Rubber))
            .unwrap();
        let custom = PhysicalProperties {
            friction: 0.2,
            ..Material::Rubber.physical()
        };
        world
            .set_property(
                part,
                "CustomPhysicalProperties",
                Value::PhysicalProperties(Some(custom)),
            )
            .unwrap();
        let titles: Vec<&str> = sections(world.get(part).unwrap())
            .iter()
            .map(|section| section.title)
            .collect();
        assert!(titles.contains(&"Physics"), "{titles:?}");
        let size = egui::vec2(260.0, 1300.0);
        let mut fit = None;
        let mut edit = None;
        let mut draw = |ui: &mut egui::Ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let room = ui.max_rect();
                edit = show(
                    ui,
                    &world,
                    &Assets::default(),
                    Some(part),
                    false,
                    &[],
                    |_| {},
                );
                fit = Some((ui.min_rect(), room));
            });
        };
        let ctx = theme::test_context();
        for _ in 0..3 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                ..Default::default()
            };
            let _ = crate::theme::run(&ctx, input, |ui| draw(ui));
        }
        if let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS") {
            crate::snapshot::snapshot(
                &std::path::PathBuf::from(folder).join("properties-physics.png"),
                size,
                vec![Vec::new(); 3],
                &mut draw,
            );
        }
        let (used, room) = fit.unwrap();
        assert!(
            used.width() <= room.width() + 0.5,
            "{} in {}",
            used.width(),
            room.width()
        );
        assert_eq!(edit, None);
    }
}

mod prefab_override_tests {
    use std::sync::Arc;

    use axcenity_core::prefab::{Overrides, PrefabLink, PrefabSource, sync};

    use super::*;

    /// A placed copy of a one-Part prefab whose Color it overrides draws
    /// Color bold with a ↺ in a 260-point column, and other properties
    /// plainly. With `AXCENITY_SNAPSHOTS` set it's saved as
    /// `properties-prefab-override.png`.
    #[test]
    fn an_overridden_property_is_bold_with_a_revert_button() {
        let mut source = World::new();
        let original = source.create(ClassName::Part).unwrap();
        source
            .set_parent(original, Some(source.workspace()))
            .unwrap();
        let source = Arc::new(PrefabSource {
            world: source,
            roots: vec![original],
        });
        let mut world = World::new();
        let part = world.copy_from(&source.world, original).unwrap();
        world.set_parent(part, Some(world.workspace())).unwrap();
        world
            .set_prefab(
                part,
                Some(PrefabLink::new("assets/prefabs/Crate.prefab.json", 0)),
            )
            .unwrap();
        sync(&mut world, part, &source, 0, &Overrides::default());
        world
            .set_property(part, "Color", Value::Color3(Color3::new(1.0, 0.0, 0.0)))
            .unwrap();

        let size = egui::vec2(260.0, 700.0);
        let mut fit = None;
        let mut edit = None;
        let mut draw = |ui: &mut egui::Ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let room = ui.max_rect();
                edit = show(
                    ui,
                    &world,
                    &Assets::default(),
                    Some(part),
                    false,
                    &[],
                    |_| {},
                );
                fit = Some((ui.min_rect(), room));
            });
        };
        let ctx = theme::test_context();
        for _ in 0..3 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                ..Default::default()
            };
            let _ = crate::theme::run(&ctx, input, |ui| draw(ui));
        }
        if let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS") {
            crate::snapshot::snapshot(
                &std::path::PathBuf::from(folder).join("properties-prefab-override.png"),
                size,
                vec![Vec::new(); 3],
                &mut draw,
            );
        }
        let (used, room) = fit.unwrap();
        assert!(used.width() <= room.width() + 0.5);
        assert_eq!(edit, None);
    }
}

mod decal_section_tests {
    use super::*;

    fn titles(instance: &Instance) -> Vec<&'static str> {
        sections(instance)
            .iter()
            .map(|section| section.title)
            .collect()
    }

    /// Phase 75: a Decal's and a ProjectedDecal's picture settings are one
    /// section, and a ProjectedDecal's box another; nothing is left over.
    #[test]
    fn decals_show_their_picture_and_box_sections() {
        let mut world = World::new();
        let decal = world.create(ClassName::Decal).unwrap();
        let projected = world.create(ClassName::ProjectedDecal).unwrap();
        assert_eq!(titles(world.get(decal).unwrap()), ["Appearance", "Decal"]);
        assert_eq!(
            titles(world.get(projected).unwrap()),
            ["Appearance", "Decal", "Box"]
        );
    }

    /// Every property the panel shows for the classes beginners meet first
    /// says on hover what it does, from the script API's descriptions.
    #[test]
    fn common_properties_say_what_they_do() {
        let mut world = World::new();
        let mut missing = Vec::new();
        for class in [
            ClassName::Part,
            ClassName::MeshPart,
            ClassName::Model,
            ClassName::PointLight,
            ClassName::SpotLight,
            ClassName::Sound,
            ClassName::ParticleEmitter,
            ClassName::Decal,
            ClassName::Animation,
            ClassName::Animator,
        ] {
            let id = world.create(class).unwrap();
            let instance = world.get(id).unwrap();
            for section in sections(instance) {
                for (name, _) in section.properties {
                    if description(class, name).is_none_or(str::is_empty) {
                        missing.push(format!("{class:?}.{name}"));
                    }
                }
            }
        }
        assert!(missing.is_empty(), "no description: {missing:?}");
        assert!(description(ClassName::Part, "Name").is_some());
        assert!(
            description(ClassName::Part, "Anchored")
                .unwrap()
                .contains("stays where it is")
        );
    }

    #[test]
    fn the_filter_ignores_case_and_spaces() {
        assert!(matches_filter("WalkSpeed", "walk speed"));
        assert!(matches_filter("CanCollide", "COLL"));
        assert!(matches_filter("Anchored", ""));
        assert!(!matches_filter("Anchored", "color"));
    }

    /// Text drawn by the Properties panel for `id` with `filter` typed in
    /// its filter box.
    fn shown_with_filter(world: &World, id: InstanceId, filter: &str) -> Vec<String> {
        fn walk(shape: &egui::Shape, found: &mut Vec<String>) {
            match shape {
                egui::Shape::Text(text) => found.push(text.galley.text().to_owned()),
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| walk(s, found)),
                _ => {}
            }
        }
        let ctx = crate::theme::test_context();
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("properties filter"), filter.to_owned()));
        let mut found = Vec::new();
        for _ in 0..3 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(300.0, 2000.0),
                )),
                ..Default::default()
            };
            let output = crate::theme::run(&ctx, input, |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    let _ = show(ui, world, &Assets::default(), Some(id), false, &[], |_| {});
                });
            });
            found.clear();
            for clipped in &output.shapes {
                walk(&clipped.shape, &mut found);
            }
        }
        found
    }

    /// Draws the Properties panel for `id` with `filter` typed, clicks the
    /// first ↺ it shows, and returns the edit made, or `None` when there's
    /// no ↺.
    fn click_reset(world: &World, id: InstanceId, filter: &str, playing: bool) -> Option<Edit> {
        fn find(shape: &egui::Shape, at: &mut Option<egui::Pos2>) {
            match shape {
                egui::Shape::Text(text)
                    if at.is_none() && text.galley.text() == icons::ARROW_COUNTER_CLOCKWISE =>
                {
                    *at = Some(text.visual_bounding_rect().center());
                }
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| find(s, at)),
                _ => {}
            }
        }
        let ctx = crate::theme::test_context();
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("properties filter"), filter.to_owned()));
        let frame = |events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(300.0, 2000.0),
                )),
                events,
                ..Default::default()
            };
            let mut edit = None;
            let output = crate::theme::run(&ctx, input, |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    edit = show(
                        ui,
                        world,
                        &Assets::default(),
                        Some(id),
                        playing,
                        &[],
                        |_| {},
                    );
                });
            });
            let mut at = None;
            for clipped in &output.shapes {
                find(&clipped.shape, &mut at);
            }
            (at, edit)
        };
        frame(Vec::new());
        let (at, _) = frame(Vec::new());
        let at = at?;
        let press = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(vec![egui::Event::PointerMoved(at)]);
        frame(vec![press(true)]);
        frame(vec![press(false)]).1
    }

    /// A property that differs from its class's default gets ↺, which sets
    /// the default back; one that doesn't, a locked one and one during Play
    /// get none.
    #[test]
    fn a_changed_property_resets_to_its_default_with_one_click() {
        let mut world = World::new();
        let part = world.create(ClassName::Part).unwrap();
        let default = ClassName::Part
            .default_properties()
            .into_iter()
            .find(|(name, _)| *name == "Transparency")
            .unwrap()
            .1;
        assert_eq!(click_reset(&world, part, "transparency", false), None);

        world
            .set_property(part, "Transparency", Value::Number(0.5))
            .unwrap();
        assert_eq!(
            click_reset(&world, part, "transparency", false),
            Some(Edit::Property("Transparency", default))
        );
        // Play's changes are kept or dropped at Stop, not reset.
        assert_eq!(click_reset(&world, part, "transparency", true), None);
        // Only Locked can be changed on something locked.
        world
            .set_property(part, "Locked", Value::Bool(true))
            .unwrap();
        assert_eq!(click_reset(&world, part, "transparency", false), None);
        assert_eq!(
            click_reset(&world, part, "locked", false),
            Some(Edit::Property("Locked", Value::Bool(false)))
        );

        // With AXCENITY_SNAPSHOTS: a moved, half-clear part with the mouse
        // over Transparency's ↺.
        if let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS") {
            world
                .set_property(part, "Locked", Value::Bool(false))
                .unwrap();
            world
                .set_property(
                    part,
                    "Position",
                    Value::Vector3(Vector3::new(4.0, 2.0, 0.0)),
                )
                .unwrap();
            let mut frames = vec![Vec::new(); 6];
            frames[0].push(egui::Event::PointerMoved(egui::pos2(21.0, 427.0)));
            crate::snapshot::snapshot(
                &std::path::PathBuf::from(folder).join("properties-reset.png"),
                egui::vec2(300.0, 560.0),
                frames,
                |ui| {
                    ui.data_mut(|d| {
                        d.insert_temp(egui::Id::new("properties filter"), String::new())
                    });
                    egui::CentralPanel::default().show(ui, |ui| {
                        let _ = show(
                            ui,
                            &world,
                            &Assets::default(),
                            Some(part),
                            false,
                            &[],
                            |_| {},
                        );
                    });
                },
            );
        }
    }

    #[test]
    fn defaults_are_said_briefly() {
        assert_eq!(default_text(&Value::Number(0.25)).as_deref(), Some("0.25"));
        assert_eq!(default_text(&Value::Number(16.0)).as_deref(), Some("16"));
        assert_eq!(default_text(&Value::Bool(true)).as_deref(), Some("on"));
        assert_eq!(
            default_text(&Value::Vector3(Vector3::new(4.0, 1.0, 2.0))).as_deref(),
            Some("4, 1, 2")
        );
        assert_eq!(
            default_text(&Value::String(String::new())).as_deref(),
            Some("empty")
        );
    }

    /// Rare settings start folded under Advanced, whose header counts the
    /// ones changed; the filter finds them and opens it.
    #[test]
    fn rare_settings_fold_under_advanced_and_say_how_many_changed() {
        let mut world = World::new();
        let part = world.create(ClassName::Part).unwrap();
        let has = |shown: &[String], text: &str| shown.iter().any(|t| t == text);
        // A section's header has its icon in front.
        let header =
            |shown: &[String], text: &str| shown.iter().any(|t| t.ends_with(&format!("  {text}")));

        let shown = shown_with_filter(&world, part, "");
        assert!(header(&shown, "Advanced"), "{shown:?}");
        for rare in ["Massless", "CollisionGroup", "MaterialVariant"] {
            assert!(!has(&shown, rare), "{rare} is folded: {shown:?}");
        }
        for common in ["Color", "Anchored", "CanCollide", "Size"] {
            assert!(has(&shown, common), "{common} stays: {shown:?}");
        }

        world
            .set_property(part, "Massless", Value::Bool(true))
            .unwrap();
        let shown = shown_with_filter(&world, part, "");
        assert!(header(&shown, "Advanced · 1 changed"), "{shown:?}");

        let shown = shown_with_filter(&world, part, "mass");
        assert!(has(&shown, "Massless"), "{shown:?}");

        // With AXCENITY_SNAPSHOTS: the part's panel, Advanced closed.
        if let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS") {
            crate::snapshot::snapshot(
                &std::path::PathBuf::from(folder).join("properties-advanced.png"),
                egui::vec2(300.0, 1100.0),
                vec![Vec::new(); 3],
                |ui| {
                    ui.data_mut(|d| {
                        d.insert_temp(egui::Id::new("properties filter"), String::new())
                    });
                    egui::CentralPanel::default().show(ui, |ui| {
                        let _ = show(
                            ui,
                            &world,
                            &Assets::default(),
                            Some(part),
                            false,
                            &[],
                            |_| {},
                        );
                    });
                },
            );
        }

        // Lights keep Brightness in view; an emitter's is rare.
        assert!(!is_advanced(ClassName::PointLight, "Brightness"));
        assert!(is_advanced(ClassName::ParticleEmitter, "Brightness"));
        assert!(is_advanced(
            ClassName::ThirdPersonController,
            "MaxStepHeight"
        ));
        assert!(!is_advanced(ClassName::ThirdPersonController, "WalkSpeed"));
        assert!(is_advanced(ClassName::TextLabel, "RichText"));
        assert!(!is_advanced(ClassName::TextLabel, "Text"));
    }

    /// Typing in Filter properties leaves only the rows whose names match
    /// (and whole sections whose titles do), and says so when none do.
    #[test]
    fn filter_properties_shows_only_matching_rows() {
        let mut world = World::new();
        let part = world.create(ClassName::Part).unwrap();
        world
            .set_attribute(part, "Health", Some(Value::Number(100.0)))
            .unwrap();
        let has = |shown: &[String], text: &str| shown.iter().any(|t| t == text);

        let all = shown_with_filter(&world, part, "");
        for name in ["Name", "Color", "Anchored", "CanCollide", "Size", "Health"] {
            assert!(has(&all, name), "{name} shows unfiltered: {all:?}");
        }

        // The header says the class; its own row is only for the filter.
        assert!(!has(&all, "ClassName"), "{all:?}");
        assert!(has(&shown_with_filter(&world, part, "class"), "ClassName"));

        let shown = shown_with_filter(&world, part, "can coll");
        assert!(has(&shown, "CanCollide"), "{shown:?}");
        for hidden in ["Name", "Color", "Anchored", "Size", "Health"] {
            assert!(!has(&shown, hidden), "{hidden} is filtered out: {shown:?}");
        }

        // A section's title shows all of it.
        let shown = shown_with_filter(&world, part, "appearance");
        assert!(
            has(&shown, "Color") && has(&shown, "Roughness"),
            "{shown:?}"
        );
        assert!(!has(&shown, "Anchored"), "{shown:?}");

        // Attributes match by name too.
        let shown = shown_with_filter(&world, part, "health");
        assert!(has(&shown, "Health"), "{shown:?}");

        let shown = shown_with_filter(&world, part, "zzz");
        assert!(
            shown.iter().any(|t| t.contains("No properties match")),
            "{shown:?}"
        );

        // With AXCENITY_SNAPSHOTS: the panel filtered to "can", and
        // unfiltered with the mouse over Anchored, showing what it does.
        if let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS") {
            let folder = std::path::PathBuf::from(folder);
            for (file, filter, hover) in [
                ("properties-filter.png", "can", None),
                (
                    "properties-description.png",
                    "",
                    Some(egui::pos2(40.0, 427.0)),
                ),
            ] {
                let mut draw = |ui: &mut egui::Ui| {
                    ui.data_mut(|d| {
                        d.insert_temp(egui::Id::new("properties filter"), filter.to_owned())
                    });
                    egui::CentralPanel::default().show(ui, |ui| {
                        let _ = show(
                            ui,
                            &world,
                            &Assets::default(),
                            Some(part),
                            false,
                            &[],
                            |_| {},
                        );
                    });
                };
                let frames = match hover {
                    Some(at) => {
                        let mut frames = vec![Vec::new(); 6];
                        frames[0].push(egui::Event::PointerMoved(at));
                        frames
                    }
                    None => vec![Vec::new(); 3],
                };
                crate::snapshot::snapshot(
                    &folder.join(file),
                    egui::vec2(300.0, 620.0),
                    frames,
                    &mut draw,
                );
            }
        }
    }
}

mod sky_control_tests {
    use super::*;

    /// Lighting's SkyImage row with a long file name, in a Properties form
    /// `width` wide: the hint and the Follow time of day button go under the
    /// file, as wide as the column, so the row stays a few lines tall.
    /// Returns the control's rect and the panel's.
    fn draw_sky_row(width: f32, snapshot: &str) -> (egui::Rect, egui::Rect) {
        let path = "assets/sky/kloofendal_overcast_puresky_2k.hdr";
        let size = egui::vec2(width, 300.0);
        let mut drawn = None;
        let mut draw = |ui: &mut egui::Ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let room = ui.max_rect();
                crate::responsive::form(ui, "sky row", |ui| {
                    ui.label("SkyImage");
                    let control = ui.scope(|ui| sky_control(ui, path, &Assets::default()));
                    assert_eq!(control.inner, None);
                    drawn = Some((control.response.rect, room));
                    ui.end_row();
                });
            });
        };
        let ctx = theme::test_context();
        for _ in 0..3 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                ..Default::default()
            };
            let _ = crate::theme::run(&ctx, input, |ui| draw(ui));
        }
        if let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS") {
            crate::snapshot::snapshot(
                &std::path::PathBuf::from(folder).join(snapshot),
                size,
                vec![Vec::new(); 3],
                &mut draw,
            );
        }
        drawn.unwrap()
    }

    #[test]
    fn the_sky_hint_goes_under_the_file_instead_of_wrapping_letter_by_letter() {
        // A grid (wide panel) and a stacked form (narrow panel).
        for (width, snapshot) in [
            (440.0, "properties-sky-image-wide.png"),
            (260.0, "properties-sky-image-narrow.png"),
        ] {
            let (control, room) = draw_sky_row(width, snapshot);
            // The file row, two lines of hint and the button: about 70
            // points. Squeezed beside the file it was hundreds.
            assert!(
                control.height() < 110.0,
                "{width} wide: the row is {} tall",
                control.height()
            );
            assert!(
                control.right() <= room.right() + 0.5,
                "{width} wide: drawn to {} in {room:?}",
                control.right()
            );
        }
    }
}
