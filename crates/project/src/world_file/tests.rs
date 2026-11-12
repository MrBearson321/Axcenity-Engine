use super::*;
use axcenity_core::{Color3, Vector3};

fn parse_err(text: &str) -> String {
    match parse_world(text) {
        Ok(_) => panic!("expected an error"),
        Err(err) => err.to_string(),
    }
}

#[test]
fn prefabs_round_trip_and_a_bad_one_adds_nothing() {
    let mut world = World::new();
    let workspace = world.workspace();
    let model = world.create(ClassName::Model).unwrap();
    world.set_name(model, "Lamp").unwrap();
    world.set_parent(model, Some(workspace)).unwrap();
    let bulb = world.create(ClassName::Part).unwrap();
    world.set_name(bulb, "Bulb").unwrap();
    world
        .set_property(bulb, "Material", Value::Material(Material::Neon))
        .unwrap();
    world.set_parent(bulb, Some(model)).unwrap();
    let light = world.create(ClassName::PointLight).unwrap();
    world.set_parent(light, Some(bulb)).unwrap();
    let text = write_prefab(&world, &[model]);

    let mut other = World::new();
    let workspace = other.workspace();
    let added = parse_prefab(&text, &mut other, workspace).unwrap();
    assert_eq!(added.len(), 1);
    assert_eq!(write_prefab(&other, &added), text);

    let before = other.descendants(workspace).len();
    let bad = r#"{ "format_version": 1, "instances": [
            { "class": "Part" }, { "class": "Nope" } ] }"#;
    assert!(parse_prefab(bad, &mut other, workspace).is_err());
    assert_eq!(other.descendants(workspace).len(), before);
}

#[test]
fn a_spline_scatter_and_mesh_save_their_settings_but_not_what_they_made() {
    let mut world = World::new();
    let workspace = world.workspace();
    let spline = world.create(ClassName::Spline).unwrap();
    world.set_parent(spline, Some(workspace)).unwrap();
    let point = world.create(ClassName::SplinePoint).unwrap();
    world.set_parent(point, Some(spline)).unwrap();
    let mut made = Vec::new();
    for class in [ClassName::SplineScatter, ClassName::SplineMesh] {
        let generator = world.create(class).unwrap();
        world.set_parent(generator, Some(spline)).unwrap();
        let result = world.create(ClassName::MeshPart).unwrap();
        world.set_name(result, "Made").unwrap();
        world.set_parent(result, Some(generator)).unwrap();
        made.push(generator);
    }
    world
        .set_property(made[0], "Seed", Value::Number(42.0))
        .unwrap();
    let text = write_world(&world);
    assert!(text.contains("\"SplineScatter\"") && text.contains("\"Seed\": 42"));
    assert!(text.contains("\"SplineMesh\""));
    assert!(
        !text.contains("MeshPart") && !text.contains("Made"),
        "{text}"
    );
    let back = parse_world(&text).unwrap();
    let spline = back.find_first_child(back.workspace(), "Spline").unwrap();
    let generators = back.get(spline).unwrap().children().to_vec();
    assert_eq!(generators.len(), 3);
    assert!(
        generators[1..]
            .iter()
            .all(|&id| back.get(id).unwrap().children().is_empty())
    );
}

#[test]
fn empty_world() {
    let world = parse_world(r#"{ "format_version": 1 }"#).unwrap();
    assert!(world.get(world.workspace()).unwrap().children().is_empty());
}

#[test]
fn lighting_settings_round_trip_and_default_ones_are_not_written() {
    let world = parse_world(r#"{ "format_version": 1 }"#).unwrap();
    assert!(!write_world(&world).contains("lighting"));

    let world = parse_world(
        r#"{ "format_version": 1, "lighting": { "properties": { "ShadowsEnabled": false } } }"#,
    )
    .unwrap();
    let lighting = world.get(world.lighting()).unwrap();
    assert_eq!(
        lighting.property("ShadowsEnabled"),
        Some(&Value::Bool(false))
    );
    let reloaded = parse_world(&write_world(&world)).unwrap();
    let lighting = reloaded.get(reloaded.lighting()).unwrap();
    assert_eq!(
        lighting.property("ShadowsEnabled"),
        Some(&Value::Bool(false))
    );
}

#[test]
fn players_starter_player_and_teams_round_trip() {
    let world = parse_world(r#"{ "format_version": 1 }"#).unwrap();
    let text = write_world(&world);
    for key in ["players", "starter_player", "teams"] {
        assert!(!text.contains(key), "{key} has only defaults");
    }

    let world = parse_world(
        r#"{ "format_version": 1,
             "players": { "properties": { "RespawnTime": 2, "LeaderboardEnabled": false } },
             "starter_player": { "properties": { "StarterCharacter": "assets/prefabs/Hero.prefab.json" } },
             "teams": { "children": [
               { "class": "Team", "name": "Red", "properties": { "TeamColor": [1, 0, 0] } }
             ] } }"#,
    )
    .unwrap();
    let reloaded = parse_world(&write_world(&world)).unwrap();
    let players = reloaded.get(reloaded.players()).unwrap();
    assert_eq!(players.property("RespawnTime"), Some(&Value::Number(2.0)));
    assert_eq!(
        players.property("LeaderboardEnabled"),
        Some(&Value::Bool(false))
    );
    assert_eq!(
        reloaded
            .get(reloaded.starter_player())
            .unwrap()
            .property("StarterCharacter"),
        Some(&Value::String("assets/prefabs/Hero.prefab.json".to_owned()))
    );
    let red = reloaded.find_first_child(reloaded.teams(), "Red").unwrap();
    assert_eq!(
        reloaded.get(red).unwrap().property("TeamColor"),
        Some(&Value::Color3(Color3::new(1.0, 0.0, 0.0)))
    );
    // StarterPlayerScripts is always there, so it isn't written.
    assert!(!write_world(&reloaded).contains("StarterPlayerScripts"));
}

#[test]
fn part_with_properties_and_nested_child() {
    let world = parse_world(
        r#"{
                "format_version": 1,
                "workspace": { "children": [
                    {
                        "class": "Part",
                        "name": "Block",
                        "properties": {
                            "Position": [1, 2, 3],
                            "Color": [1, 0.5, 0],
                            "Metalness": 1
                        },
                        "children": [ { "class": "Part" } ]
                    }
                ] }
            }"#,
    )
    .unwrap();

    let block = world.find_first_child(world.workspace(), "Block").unwrap();
    let block = world.get(block).unwrap();
    assert_eq!(
        block.property("Position"),
        Some(&Value::Vector3(Vector3::new(1.0, 2.0, 3.0)))
    );
    assert_eq!(
        block.property("Color"),
        Some(&Value::Color3(Color3::new(1.0, 0.5, 0.0)))
    );
    assert_eq!(block.property("Metalness"), Some(&Value::Number(1.0)));
    // Left out, so still the default.
    assert_eq!(block.property("Roughness"), Some(&Value::Number(0.5)));

    let child = world.get(block.children()[0]).unwrap();
    assert_eq!(child.name(), "Part");
}

#[test]
fn malformed_json_reports_line_and_column() {
    let message = parse_err("{\n  \"format_version\": 1,\n  \"workspace\": {\n}");
    assert!(message.contains("line"), "{message}");
    assert!(message.contains("column"), "{message}");
}

#[test]
fn misspelled_field_is_an_error() {
    let message = parse_err(r#"{ "format_version": 1, "workspce": {} }"#);
    assert!(message.contains("workspce"), "{message}");
}

#[test]
fn missing_format_version_is_an_error() {
    let message = parse_err(r#"{ "workspace": {} }"#);
    assert!(message.contains("format_version"), "{message}");
}

#[test]
fn newer_format_is_an_error() {
    assert!(matches!(
        parse_world(r#"{ "format_version": 999 }"#),
        Err(WorldError::TooNew { found: 999 })
    ));
}

#[test]
fn unknown_class_names_the_path() {
    let message = parse_err(
        r#"{ "format_version": 1, "workspace": { "children": [
                { "class": "Prat", "name": "Floor" } ] } }"#,
    );
    assert!(message.contains("Workspace.Floor"), "{message}");
    assert!(message.contains("Prat"), "{message}");
}

#[test]
fn service_as_child_is_an_error() {
    assert!(matches!(
        parse_world(
            r#"{ "format_version": 1, "workspace": { "children": [
                    { "class": "Workspace" } ] } }"#
        ),
        Err(WorldError::Service { .. })
    ));
}

#[test]
fn script_in_the_world_file_is_an_error() {
    let err = parse_err(
        r#"{ "format_version": 1, "workspace": { "children": [
                { "class": "Script", "name": "Spin" } ] } }"#,
    );
    assert!(err.contains("Workspace.Spin"), "{err}");
    assert!(err.contains("scripts"), "{err}");
}

#[test]
fn local_scripts_and_players_in_the_world_file_are_errors() {
    let err = parse_err(
        r#"{ "format_version": 1, "replicated_storage": { "children": [
                { "class": "LocalScript", "name": "Keys" } ] } }"#,
    );
    assert!(err.contains("ReplicatedStorage.Keys"), "{err}");
    assert!(err.contains(".client.lua"), "{err}");
    let err = parse_err(
        r#"{ "format_version": 1, "workspace": { "children": [
                { "class": "Player" } ] } }"#,
    );
    assert!(err.contains("Players are made when someone joins"), "{err}");
}

#[test]
fn scripts_and_players_are_left_out_when_saving() {
    let mut world = World::new();
    for (class, parent) in [
        (ClassName::Script, world.workspace()),
        (ClassName::LocalScript, world.replicated_storage()),
        (ClassName::Player, world.replicated_storage()),
    ] {
        let id = world.create(class).unwrap();
        world.set_parent(id, Some(parent)).unwrap();
    }
    assert_eq!(write_world(&world), write_world(&World::new()));
}

/// A Door model in the Workspace with a Script inside it whose code is
/// in `objects/Door/Open.server.lua` (G2).
fn door_with_script(world: &mut World) -> (InstanceId, InstanceId) {
    let door = world.create(ClassName::Model).unwrap();
    world.set_name(door, "Door").unwrap();
    world.set_parent(door, Some(world.workspace())).unwrap();
    let script = world.create(ClassName::Script).unwrap();
    world.set_name(script, "Open").unwrap();
    world
        .set_property(
            script,
            "SourceFile",
            Value::String("objects/Door/Open.server.lua".into()),
        )
        .unwrap();
    world.set_parent(script, Some(door)).unwrap();
    (door, script)
}

#[test]
fn scripts_inside_objects_round_trip_and_make_the_file_version_2() {
    let mut world = World::new();
    let (_, script) = door_with_script(&mut world);
    // A script from `scripts/` beside it is still left out.
    let service = world.create(ClassName::Script).unwrap();
    world
        .set_parent(service, Some(world.server_script_service()))
        .unwrap();
    let inside = world.create(ClassName::StringValue).unwrap();
    world.set_name(inside, "Sound").unwrap();
    world.set_parent(inside, Some(script)).unwrap();

    let written = write_world(&world);
    assert!(
        written.starts_with("{\n  \"format_version\": 2,"),
        "{written}"
    );
    assert!(
        written.contains("\"class\": \"Script\",\n            \"name\": \"Open\",\n            \"properties\": {\n              \"SourceFile\": \"objects/Door/Open.server.lua\"\n            }"),
        "{written}"
    );
    let loaded = parse_world(&written).unwrap();
    let open = loaded
        .find_first_descendant(loaded.workspace(), "Open")
        .unwrap();
    let open = loaded.get(open).unwrap();
    assert_eq!(open.class(), ClassName::Script);
    assert_eq!(open.source_file(), Some("objects/Door/Open.server.lua"));
    assert_eq!(open.children().len(), 1, "with what's inside it");
    assert_eq!(write_world(&loaded), written);
    // Taking the script out writes version 1 again.
    world.destroy(script).unwrap();
    assert!(write_world(&world).starts_with("{\n  \"format_version\": 1,"));
}

#[test]
fn levels_and_prefabs_hold_scripts_inside_objects_too() {
    let mut world = World::new();
    let (door, _) = door_with_script(&mut world);
    let level = write_workspace_only(&world);
    assert!(level.starts_with("{\n  \"format_version\": 2,"), "{level}");
    assert!(level.contains("objects/Door/Open.server.lua"), "{level}");

    let prefab = write_prefab(&world, &[door]);
    assert!(
        prefab.starts_with("{\n  \"format_version\": 2,"),
        "{prefab}"
    );
    let mut placed = World::new();
    let workspace = placed.workspace();
    let roots = parse_prefab(&prefab, &mut placed, workspace).unwrap();
    let children = placed.get(roots[0]).unwrap().children().to_vec();
    assert_eq!(
        placed.get(children[0]).unwrap().source_file(),
        Some("objects/Door/Open.server.lua")
    );

    let empty = World::new();
    assert!(write_workspace_only(&empty).starts_with("{\n  \"format_version\": 1,"));
}

#[test]
fn a_newer_world_format_than_2_asks_for_an_update() {
    let err = parse_err(r#"{ "format_version": 3, "workspace": {} }"#);
    assert!(err.contains("update Axcenity"), "{err}");
}

#[test]
fn remote_events_in_replicated_storage_save_and_load() {
    let mut world = World::new();
    let folder = world.create(ClassName::Folder).unwrap();
    world.set_name(folder, "Remotes").unwrap();
    world
        .set_parent(folder, Some(world.replicated_storage()))
        .unwrap();
    let remote = world.create(ClassName::RemoteEvent).unwrap();
    world.set_name(remote, "Jump").unwrap();
    world.set_parent(remote, Some(folder)).unwrap();

    let written = write_world(&world);
    assert_eq!(
        written,
        "{\n  \"format_version\": 1,\n  \"workspace\": {},\n  \"replicated_storage\": {\n    \"children\": [\n      {\n        \"class\": \"Folder\",\n        \"name\": \"Remotes\",\n        \"children\": [\n          {\n            \"class\": \"RemoteEvent\",\n            \"name\": \"Jump\"\n          }\n        ]\n      }\n    ]\n  }\n}\n"
    );
    let loaded = parse_world(&written).unwrap();
    let jump = loaded
        .find_first_descendant(loaded.replicated_storage(), "Jump")
        .unwrap();
    assert_eq!(loaded.get(jump).unwrap().class(), ClassName::RemoteEvent);
    assert_eq!(write_world(&loaded), written);
}

#[test]
fn server_storage_saves_and_loads_with_references_into_it() {
    let mut world = World::new();
    let model = world.create(ClassName::Model).unwrap();
    world.set_name(model, "Prize").unwrap();
    world
        .set_parent(model, Some(world.server_storage()))
        .unwrap();
    let pointer = world.create(ClassName::ObjectValue).unwrap();
    world.set_name(pointer, "Next").unwrap();
    world.set_parent(pointer, Some(world.workspace())).unwrap();
    world
        .set_property(pointer, "Value", Value::Instance(Some(model)))
        .unwrap();

    let written = write_world(&world);
    assert!(written.contains("\"server_storage\": {"), "{written}");
    let loaded = parse_world(&written).unwrap();
    let prize = loaded
        .find_first_child(loaded.server_storage(), "Prize")
        .unwrap();
    let next = loaded.find_first_child(loaded.workspace(), "Next").unwrap();
    assert_eq!(
        loaded.get(next).unwrap().property("Value"),
        Some(&Value::Instance(Some(prize)))
    );
    assert_eq!(write_world(&loaded), written);
    assert!(!write_world(&World::new()).contains("server_storage"));
}

#[test]
fn widgets_save_and_reload_unchanged() {
    let mut world = World::new();
    let gui = world.create(ClassName::ScreenGui).unwrap();
    world.set_parent(gui, Some(world.workspace())).unwrap();
    let mut widget = |class, values: &[(&str, Value)]| {
        let id = world.create(class).unwrap();
        world.set_parent(id, Some(gui)).unwrap();
        for (name, value) in values {
            world.set_property(id, name, value.clone()).unwrap();
        }
    };
    widget(
        ClassName::Slider,
        &[
            ("Value", Value::Number(0.75)),
            ("MaxValue", Value::Number(1.0)),
            ("Step", Value::Number(0.05)),
            ("Orientation", Value::String("Vertical".into())),
        ],
    );
    widget(
        ClassName::Toggle,
        &[
            ("Value", Value::Bool(true)),
            ("Style", Value::String("Checkbox".into())),
        ],
    );
    widget(
        ClassName::Dropdown,
        &[
            ("Options", Value::String("Low\nHigh \"Ultra\"".into())),
            ("Value", Value::String("Low".into())),
        ],
    );
    widget(ClassName::ProgressBar, &[("Value", Value::Number(10.0))]);

    let written = write_world(&world);
    let loaded = parse_world(&written).unwrap();
    let widgets = |world: &World| {
        let gui = world.get(world.workspace()).unwrap().children()[0];
        world.get(gui).unwrap().children().to_vec()
    };
    let (saved, reloaded) = (widgets(&world), widgets(&loaded));
    assert_eq!(reloaded.len(), 4);
    for (saved, reloaded) in saved.into_iter().zip(reloaded) {
        let (saved, reloaded) = (world.get(saved).unwrap(), loaded.get(reloaded).unwrap());
        assert_eq!(saved.class(), reloaded.class());
        for (name, _) in saved.class().default_properties() {
            assert_eq!(saved.property(name), reloaded.property(name), "{name}");
        }
    }
    assert_eq!(write_world(&loaded), written);
}

#[test]
fn a_scrolling_frame_and_layer_order_save_and_load_and_older_worlds_get_defaults() {
    let mut world = World::new();
    let gui = world.create(ClassName::ScreenGui).unwrap();
    world.set_parent(gui, Some(world.workspace())).unwrap();
    world
        .set_property(gui, "DisplayOrder", Value::Number(3.0))
        .unwrap();
    let list = world.create(ClassName::ScrollingFrame).unwrap();
    world.set_parent(list, Some(gui)).unwrap();
    let at = axcenity_core::Vector2::new(0.0, 120.5);
    world
        .set_property(list, "CanvasPosition", Value::Vector2(at))
        .unwrap();
    world
        .set_property(list, "ZIndex", Value::Number(4.0))
        .unwrap();

    let written = write_world(&world);
    assert!(
        written.contains("\"CanvasPosition\": [0, 120.5]"),
        "{written}"
    );
    let loaded = parse_world(&written).unwrap();
    let list = loaded
        .find_first_descendant(loaded.workspace(), "ScrollingFrame")
        .unwrap();
    let list = loaded.get(list).unwrap();
    assert_eq!(list.property("CanvasPosition"), Some(&Value::Vector2(at)));
    assert_eq!(list.property("ZIndex"), Some(&Value::Number(4.0)));
    assert_eq!(write_world(&loaded), written);

    // A world saved before layer order: a Frame without ZIndex and a
    // ScreenGui without DisplayOrder load with the defaults.
    let old = parse_world(
        r#"{ "format_version": 1, "workspace": { "children": [
                { "class": "ScreenGui", "name": "Hud", "properties": { "Enabled": true },
                  "children": [ { "class": "Frame", "name": "Panel" } ] } ] } }"#,
    )
    .unwrap();
    let panel = old.find_first_descendant(old.workspace(), "Panel").unwrap();
    let hud = old.find_first_descendant(old.workspace(), "Hud").unwrap();
    assert_eq!(
        old.get(panel).unwrap().property("ZIndex"),
        Some(&Value::Number(1.0))
    );
    assert_eq!(
        old.get(hud).unwrap().property("DisplayOrder"),
        Some(&Value::Number(0.0))
    );
}

#[test]
fn rotation_text_transparency_and_screen_insets_save_and_older_worlds_get_defaults() {
    let mut world = World::new();
    let gui = world.create(ClassName::ScreenGui).unwrap();
    world.set_parent(gui, Some(world.workspace())).unwrap();
    world
        .set_property(
            gui,
            "ScreenInsets",
            Value::String("DeviceSafeInsets".into()),
        )
        .unwrap();
    let label = world.create(ClassName::TextLabel).unwrap();
    world.set_parent(label, Some(gui)).unwrap();
    world
        .set_property(label, "Rotation", Value::Number(30.0))
        .unwrap();
    world
        .set_property(label, "TextTransparency", Value::Number(0.25))
        .unwrap();
    let written = write_world(&world);
    let loaded = parse_world(&written).unwrap();
    let label = loaded
        .find_first_descendant(loaded.workspace(), "TextLabel")
        .unwrap();
    let label = loaded.get(label).unwrap();
    assert_eq!(label.property("Rotation"), Some(&Value::Number(30.0)));
    assert_eq!(
        label.property("TextTransparency"),
        Some(&Value::Number(0.25))
    );
    assert_eq!(write_world(&loaded), written);

    let old = parse_world(
        r#"{ "format_version": 1, "workspace": { "children": [
                { "class": "ScreenGui", "name": "Hud", "properties": { "Enabled": true },
                  "children": [ { "class": "TextLabel", "name": "Title" } ] } ] } }"#,
    )
    .unwrap();
    let title = old.find_first_descendant(old.workspace(), "Title").unwrap();
    let hud = old.find_first_descendant(old.workspace(), "Hud").unwrap();
    let title = old.get(title).unwrap();
    assert_eq!(title.property("Rotation"), Some(&Value::Number(0.0)));
    assert_eq!(
        title.property("TextTransparency"),
        Some(&Value::Number(0.0))
    );
    assert_eq!(
        old.get(hud).unwrap().property("ScreenInsets"),
        Some(&Value::String("None".into()))
    );
}

#[test]
fn ui_instances_and_udim2_properties_save_and_load() {
    let mut world = World::new();
    let gui = world.create(ClassName::ScreenGui).unwrap();
    world.set_name(gui, "Hud").unwrap();
    world.set_parent(gui, Some(world.workspace())).unwrap();
    let frame = world.create(ClassName::Frame).unwrap();
    world.set_parent(frame, Some(gui)).unwrap();
    world
        .set_property(frame, "Class", Value::String("hud".to_owned()))
        .unwrap();
    world
        .set_property(frame, "Id", Value::String("main".to_owned()))
        .unwrap();
    let position = axcenity_core::UDim2::new(0.5, 12.0, 0.25, -4.0);
    world
        .set_property(frame, "Position", Value::UDim2(position))
        .unwrap();
    let label = world.create(ClassName::TextLabel).unwrap();
    world.set_parent(label, Some(frame)).unwrap();
    world
        .set_property(label, "Text", Value::String("Ready".to_owned()))
        .unwrap();

    let written = write_world(&world);
    assert!(written.contains("\"Position\": [0.5, 12, 0.25, -4]"));
    let loaded = parse_world(&written).unwrap();
    let frame = loaded
        .find_first_descendant(loaded.workspace(), "Frame")
        .unwrap();
    assert_eq!(
        loaded.get(frame).unwrap().property("Position"),
        Some(&Value::UDim2(position))
    );
    assert_eq!(
        loaded.get(frame).unwrap().property("Class"),
        Some(&Value::String("hud".to_owned()))
    );
    assert_eq!(
        loaded.get(frame).unwrap().property("Id"),
        Some(&Value::String("main".to_owned()))
    );
    let label = loaded
        .find_first_descendant(loaded.workspace(), "TextLabel")
        .unwrap();
    assert_eq!(
        loaded.get(label).unwrap().property("Text"),
        Some(&Value::String("Ready".to_owned()))
    );
    assert_eq!(write_world(&loaded), written);
}

#[test]
fn unknown_property_is_an_error() {
    let message = parse_err(
        r#"{ "format_version": 1, "workspace": { "children": [
                { "class": "Part", "properties": { "Colour": [1, 0, 0] } } ] } }"#,
    );
    assert!(message.contains("Colour"), "{message}");
}

#[test]
fn wrong_property_shape_explains_what_was_expected() {
    let message = parse_err(
        r#"{ "format_version": 1, "workspace": { "children": [
                { "class": "Part", "properties": { "Position": [1, 2] } } ] } }"#,
    );
    assert_eq!(
        message,
        "Workspace.Part.Position: expected [x, y, z]: three numbers"
    );
}

#[test]
fn color_out_of_range_suggests_dividing_by_255() {
    let message = parse_err(
        r#"{ "format_version": 1, "workspace": { "children": [
                { "class": "Part", "properties": { "Color": [255, 0, 0] } } ] } }"#,
    );
    assert!(message.contains("divide each by 255"), "{message}");
}

#[test]
fn enum_values_are_read_by_name() {
    let world = parse_world(
        r#"{ "format_version": 1, "workspace": { "children": [
                { "class": "Part", "properties": { "Shape": "Ball", "Material": "Neon" } } ] } }"#,
    )
    .unwrap();
    let part = world.get(world.descendants(world.workspace())[0]).unwrap();
    assert_eq!(
        part.property("Shape"),
        Some(&Value::PartType(PartType::Ball))
    );
    assert_eq!(
        part.property("Material"),
        Some(&Value::Material(Material::Neon))
    );
}

#[test]
fn unknown_enum_item_lists_the_choices() {
    let message = parse_err(
        r#"{ "format_version": 1, "workspace": { "children": [
                { "class": "Part", "properties": { "Material": "Cardboard" } } ] } }"#,
    );
    assert_eq!(
        message,
        r#"Workspace.Part.Material: expected one of: "Plastic", "SmoothPlastic", "Wood", "WoodPlanks", "Marble", "Granite", "Concrete", "Slate", "Brick", "Fabric", "Sand", "Metal", "Glass", "Neon", "Ice", "Rubber""#
    );
}

#[test]
fn custom_physical_properties_save_reload_and_are_kept_in_range() {
    let world = parse_world(
        r#"{ "format_version": 1, "workspace": { "properties": { "Gravity": 98.1 }, "children": [
                { "class": "Part", "name": "Ice", "properties": {
                    "Material": "Ice", "AssemblyLinearVelocity": [0, 0, 20] } },
                { "class": "Part", "name": "Tuned", "properties": { "CustomPhysicalProperties":
                    { "Density": 0.45, "Friction": 9, "ElasticityWeight": 2 } } },
                { "class": "MeshPart", "name": "Light", "properties": { "Massless": true } } ] } }"#,
    )
    .unwrap();
    let custom = |name: &str| {
        let id = world.find_first_child(world.workspace(), name).unwrap();
        world
            .get(id)
            .unwrap()
            .property("CustomPhysicalProperties")
            .cloned()
    };
    assert_eq!(custom("Ice"), Some(Value::PhysicalProperties(None)));
    // Left-out numbers are Plastic's; friction is brought down to 2.
    let tuned = axcenity_core::PhysicalProperties::new(0.45, 2.0, 0.25, 1.0, 2.0);
    assert_eq!(
        custom("Tuned"),
        Some(Value::PhysicalProperties(Some(tuned)))
    );
    let text = write_world(&world);
    assert!(text.contains("\"Gravity\": 98.1"), "{text}");
    assert!(
        text.contains(
            "\"CustomPhysicalProperties\": { \"Density\": 0.45, \"Friction\": 2, \
             \"Elasticity\": 0.25, \"FrictionWeight\": 1, \"ElasticityWeight\": 2 }"
        ),
        "{text}"
    );
    assert_eq!(write_world(&parse_world(&text).unwrap()), text);
    // The default gravity isn't written.
    assert!(!write_world(&World::new()).contains("Gravity"));

    let message = parse_err(
        r#"{ "format_version": 1, "workspace": { "children": [
                { "class": "Part", "properties": { "CustomPhysicalProperties": { "Bounce": 1 } } } ] } }"#,
    );
    assert!(message.contains("there is no \"Bounce\""), "{message}");
}

#[test]
fn roughness_in_the_file_overrides_the_material_preset() {
    // "Roughness" is listed before "Material" on purpose: the order in the file doesn't matter.
    let world = parse_world(
            r#"{ "format_version": 1, "workspace": { "children": [
                { "class": "Part", "properties": { "Roughness": 0.8, "Material": "Metal" } } ] } }"#,
        )
        .unwrap();
    let part = world.get(world.descendants(world.workspace())[0]).unwrap();
    assert_eq!(part.property("Metalness"), Some(&Value::Number(1.0)));
    assert_eq!(part.property("Roughness"), Some(&Value::Number(0.8)));
}

#[test]
fn written_world_has_the_hand_written_layout() {
    let world = parse_world(
        r#"{ "format_version": 1, "workspace": { "children": [
                { "class": "Part", "name": "Say \"hi\"",
                  "properties": { "Position": [1, 0.45, -2.5], "Material": "Glass" },
                  "children": [ { "class": "Part" } ] } ] } }"#,
    )
    .unwrap();
    let part = |indent: &str, name: &str, position: &str, material: &str, roughness: &str| {
        [
            format!("{indent}  \"class\": \"Part\","),
            format!("{indent}  \"name\": {name},"),
            format!("{indent}  \"properties\": {{"),
            format!("{indent}    \"Anchored\": false,"),
            format!("{indent}    \"AssemblyAngularVelocity\": [0, 0, 0],"),
            format!("{indent}    \"AssemblyLinearVelocity\": [0, 0, 0],"),
            format!("{indent}    \"CanCollide\": true,"),
            format!("{indent}    \"CanQuery\": true,"),
            format!("{indent}    \"CanTouch\": true,"),
            format!("{indent}    \"CastShadow\": true,"),
            format!("{indent}    \"CollisionGroup\": \"Default\","),
            format!("{indent}    \"Color\": [0.64, 0.64, 0.64],"),
            format!("{indent}    \"CustomPhysicalProperties\": null,"),
            format!("{indent}    \"Emission\": 0,"),
            format!("{indent}    \"EmissiveColor\": [1, 1, 1],"),
            format!("{indent}    \"Massless\": false,"),
            format!("{indent}    \"Material\": \"{material}\","),
            format!("{indent}    \"MaterialVariant\": \"\","),
            format!("{indent}    \"Metalness\": 0,"),
            format!("{indent}    \"Orientation\": [0, 0, 0],"),
            format!("{indent}    \"Position\": {position},"),
            format!("{indent}    \"Roughness\": {roughness},"),
            format!("{indent}    \"Shape\": \"Block\","),
            format!("{indent}    \"Size\": [4, 1, 2],"),
            format!("{indent}    \"TileSize\": 8,"),
            format!("{indent}    \"Transparency\": 0,"),
            format!("{indent}    \"WindAffected\": false"),
            format!("{indent}  }}"),
        ]
        .join("\n")
    };
    let expected = [
        "{".to_owned(),
        "  \"format_version\": 1,".to_owned(),
        "  \"workspace\": {".to_owned(),
        "    \"children\": [".to_owned(),
        "      {".to_owned(),
        part(
            "      ",
            r#""Say \"hi\"""#,
            "[1, 0.45, -2.5]",
            "Glass",
            "0.05",
        ) + ",",
        "        \"children\": [".to_owned(),
        "          {".to_owned(),
        part("          ", "\"Part\"", "[0, 0, 0]", "Plastic", "0.5"),
        "          }".to_owned(),
        "        ]".to_owned(),
        "      }".to_owned(),
        "    ]".to_owned(),
        "  }".to_owned(),
        "}".to_owned(),
        String::new(),
    ]
    .join("\n");
    assert_eq!(write_world(&world), expected);
}

#[test]
fn models_and_folders_save_and_load_with_their_children() {
    let text = r#"{ "format_version": 1, "workspace": { "children": [
            { "class": "Folder", "name": "Props", "children": [
                { "class": "Model", "name": "Car", "children": [
                    { "class": "Part", "name": "Body" } ] } ] } ] } }"#;
    let world = parse_world(text).unwrap();
    let body = world
        .find_first_descendant(world.workspace(), "Body")
        .unwrap();
    assert_eq!(
        world.full_name(body).as_deref(),
        Some("Workspace.Props.Car.Body")
    );
    let car = world.get(body).unwrap().parent().unwrap();
    assert_eq!(world.get(car).unwrap().class(), ClassName::Model);

    // They have no properties, so the file lists only their class, name and children.
    let written = write_world(&world);
    assert!(
        written.contains(
            "\"class\": \"Folder\",\n        \"name\": \"Props\",\n        \"children\": ["
        ),
        "{written}"
    );
    assert_eq!(write_world(&parse_world(&written).unwrap()), written);
}

#[test]
fn empty_world_writes_an_empty_workspace() {
    let world = World::new();
    assert_eq!(
        write_world(&world),
        "{\n  \"format_version\": 1,\n  \"workspace\": {}\n}\n"
    );
}

#[test]
fn global_wind_is_saved_only_when_it_blows() {
    let mut world = World::new();
    let workspace = world.workspace();
    world
        .set_property(
            workspace,
            "GlobalWind",
            Value::Vector3(axcenity_core::Vector3::new(10.0, 0.0, -5.0)),
        )
        .unwrap();
    let text = write_world(&world);
    assert!(text.contains("\"GlobalWind\": [10, 0, -5]"), "{text}");
    let reloaded = parse_world(&text).unwrap();
    assert_eq!(
        reloaded
            .get(reloaded.workspace())
            .unwrap()
            .property("GlobalWind"),
        world.get(workspace).unwrap().property("GlobalWind")
    );
}

#[test]
fn values_that_json_cannot_hold_fail_to_load_again() {
    let mut world = World::new();
    let part = world.create(ClassName::Part).unwrap();
    world.set_parent(part, Some(world.workspace())).unwrap();
    world
        .set_property(part, "Metalness", Value::Number(f64::NAN))
        .unwrap();
    assert!(matches!(
        parse_world(&write_world(&world)),
        Err(WorldError::InvalidProperty { .. })
    ));
}

#[test]
fn a_third_person_controller_saves_and_loads() {
    let mut world = World::new();
    let controller = world.create(ClassName::ThirdPersonController).unwrap();
    world
        .set_parent(controller, Some(world.workspace()))
        .unwrap();
    world
        .set_property(controller, "Movement", Value::String("Run".to_owned()))
        .unwrap();
    world
        .set_property(controller, "WalkSpeed", Value::Number(12.0))
        .unwrap();
    world
        .set_property(controller, "RunSpeed", Value::Number(40.0))
        .unwrap();
    let reloaded = parse_world(&write_world(&world)).unwrap();
    let id = reloaded
        .find_first_child(reloaded.workspace(), "ThirdPersonController")
        .unwrap();
    let saved = reloaded.get(id).unwrap();
    assert_eq!(
        saved.property("Movement"),
        Some(&Value::String("Run".to_owned()))
    );
    assert_eq!(saved.property("WalkSpeed"), Some(&Value::Number(12.0)));
    assert_eq!(saved.property("RunSpeed"), Some(&Value::Number(40.0)));
    assert_eq!(saved.property("CameraDistance"), Some(&Value::Number(12.0)));
}

#[test]
fn a_navigation_agents_senses_save_and_load_and_an_older_agent_gets_the_defaults() {
    let mut world = World::new();
    let agent = world.create(ClassName::NavigationAgent).unwrap();
    world.set_parent(agent, Some(world.workspace())).unwrap();
    for (name, value) in [
        ("SightRange", 45.0),
        ("SightAngle", 75.0),
        ("HearingRange", 0.0),
    ] {
        world
            .set_property(agent, name, Value::Number(value))
            .unwrap();
    }
    let reloaded = parse_world(&write_world(&world)).unwrap();
    let id = reloaded
        .find_first_child(reloaded.workspace(), "NavigationAgent")
        .unwrap();
    let saved = reloaded.get(id).unwrap();
    assert_eq!(saved.property("SightRange"), Some(&Value::Number(45.0)));
    assert_eq!(saved.property("SightAngle"), Some(&Value::Number(75.0)));
    assert_eq!(saved.property("HearingRange"), Some(&Value::Number(0.0)));

    // An agent saved before it had senses loads with the defaults.
    let older = r#"{
  "format_version": 1,
  "workspace": {
    "children": [
      {
        "class": "NavigationAgent",
        "name": "NavigationAgent",
        "properties": {
          "Destination": [0, 0, 0],
          "Status": "Idle",
          "Velocity": [0, 0, 0],
          "WalkSpeed": 12
        }
      }
    ]
  }
}"#;
    let loaded = parse_world(older).unwrap();
    let id = loaded
        .find_first_child(loaded.workspace(), "NavigationAgent")
        .unwrap();
    let agent = loaded.get(id).unwrap();
    assert_eq!(agent.property("SightRange"), Some(&Value::Number(60.0)));
    assert_eq!(agent.property("SightAngle"), Some(&Value::Number(120.0)));
    assert_eq!(agent.property("HearingRange"), Some(&Value::Number(30.0)));
    assert_eq!(agent.property("SeesPlayer"), Some(&Value::Bool(false)));
    assert_eq!(
        agent.property("BehaviorTree"),
        Some(&Value::String(String::new()))
    );
}

#[test]
fn a_navigation_agents_behavior_tree_saves_and_loads() {
    let mut world = World::new();
    let agent = world.create(ClassName::NavigationAgent).unwrap();
    world.set_parent(agent, Some(world.workspace())).unwrap();
    let tree = Value::String("assets/ai/Guard.behaviortree.json".to_owned());
    world
        .set_property(agent, "BehaviorTree", tree.clone())
        .unwrap();
    let text = write_world(&world);
    assert!(text.contains("\"BehaviorTree\": \"assets/ai/Guard.behaviortree.json\""));
    let reloaded = parse_world(&text).unwrap();
    let id = reloaded
        .find_first_child(reloaded.workspace(), "NavigationAgent")
        .unwrap();
    assert_eq!(
        reloaded.get(id).unwrap().property("BehaviorTree"),
        Some(&tree)
    );
}

#[test]
fn a_checkpoint_saves_and_loads_with_its_position() {
    let mut world = World::new();
    let checkpoint = world.create(ClassName::Checkpoint).unwrap();
    world.set_name(checkpoint, "Cave").unwrap();
    world
        .set_property(
            checkpoint,
            "Position",
            Value::Vector3(Vector3::new(20.0, 0.0, -10.0)),
        )
        .unwrap();
    world
        .set_parent(checkpoint, Some(world.workspace()))
        .unwrap();
    let reloaded = parse_world(&write_world(&world)).unwrap();
    let id = reloaded
        .find_first_child(reloaded.workspace(), "Cave")
        .unwrap();
    let saved = reloaded.get(id).unwrap();
    assert_eq!(saved.class(), ClassName::Checkpoint);
    assert_eq!(
        saved.property("Position"),
        Some(&Value::Vector3(Vector3::new(20.0, 0.0, -10.0)))
    );
}

#[test]
fn attributes_tags_and_value_objects_round_trip_through_the_world_file() {
    let mut world = World::new();
    let workspace = world.workspace();
    let lava = world.create(ClassName::Part).unwrap();
    world.set_name(lava, "Lava").unwrap();
    world.set_parent(lava, Some(workspace)).unwrap();
    world.add_tag(lava, "Lava").unwrap();
    world.add_tag(lava, "Hot").unwrap();
    let attributes = [
        ("Damage", Value::Number(10.0)),
        ("On", Value::Bool(true)),
        ("Label", Value::String("hot".into())),
        ("Spawn", Value::Vector3(Vector3::new(0.0, 5.0, 0.0))),
        (
            "Offset",
            Value::Vector2(axcenity_core::Vector2::new(1.0, 2.0)),
        ),
        ("Tint", Value::Color3(Color3::new(1.0, 0.5, 0.0))),
        (
            "Box",
            Value::UDim2(axcenity_core::UDim2::new(0.5, 4.0, 1.0, 0.0)),
        ),
        (
            "Pivot",
            Value::CFrame(CFrame {
                position: Vector3::new(1.0, 2.0, 3.0),
                rotation: [0.0, 0.6, 0.0, 0.8],
            }),
        ),
        ("Kind", Value::Material(Material::Neon)),
    ];
    for (name, value) in attributes.clone() {
        world.set_attribute(lava, name, Some(value)).unwrap();
    }
    let target = world.create(ClassName::ObjectValue).unwrap();
    world.set_name(target, "Target").unwrap();
    world.set_parent(target, Some(workspace)).unwrap();
    world
        .set_property(target, "Value", Value::Instance(Some(lava)))
        .unwrap();
    let pivot = world.create(ClassName::CFrameValue).unwrap();
    world.set_parent(pivot, Some(workspace)).unwrap();
    let text = write_world(&world);

    let loaded = parse_world(&text).unwrap();
    assert_eq!(write_world(&loaded), text);
    let lava = loaded.find_first_child(loaded.workspace(), "Lava").unwrap();
    let part = loaded.get(lava).unwrap();
    assert_eq!(part.tags(), ["Lava", "Hot"]);
    for (name, value) in attributes {
        let value = value.to_enum_item().map(Value::EnumItem).unwrap_or(value);
        assert_eq!(part.attribute(name), Some(&value), "{name}");
    }
    let target = loaded
        .find_first_child(loaded.workspace(), "Target")
        .unwrap();
    assert_eq!(
        loaded.get(target).unwrap().property("Value"),
        Some(&Value::Instance(Some(lava)))
    );
    assert!(text.contains(r#""Value": ["Workspace", "Lava"]"#), "{text}");
    assert!(
        text.contains(r#""Kind": { "EnumItem": "Material.Neon" }"#),
        "{text}"
    );
}

#[test]
fn a_prefab_keeps_attributes_tags_and_references_inside_itself() {
    let mut world = World::new();
    let workspace = world.workspace();
    // Something with the prefab's name already there, which a placed
    // copy's references must not find.
    let other = world.create(ClassName::Model).unwrap();
    world.set_name(other, "Door").unwrap();
    world.set_parent(other, Some(workspace)).unwrap();

    let model = world.create(ClassName::Model).unwrap();
    world.set_name(model, "Door").unwrap();
    world.set_parent(model, Some(workspace)).unwrap();
    let hinge = world.create(ClassName::Part).unwrap();
    world.set_name(hinge, "Hinge").unwrap();
    world.set_parent(hinge, Some(model)).unwrap();
    world.add_tag(hinge, "Hinge").unwrap();
    world
        .set_attribute(hinge, "Angle", Some(Value::Number(90.0)))
        .unwrap();
    let link = world.create(ClassName::ObjectValue).unwrap();
    world.set_parent(link, Some(model)).unwrap();
    world
        .set_property(link, "Value", Value::Instance(Some(hinge)))
        .unwrap();
    let text = write_prefab(&world, &[model]);

    let added = parse_prefab(&text, &mut world, workspace).unwrap();
    let [copy] = added[..] else { panic!() };
    let new_hinge = world.find_first_child(copy, "Hinge").unwrap();
    let new_link = world.find_first_child(copy, "ObjectValue").unwrap();
    assert_eq!(world.get(new_hinge).unwrap().tags(), ["Hinge"]);
    assert_eq!(
        world.get(new_hinge).unwrap().attribute("Angle"),
        Some(&Value::Number(90.0))
    );
    assert_eq!(
        world.get(new_link).unwrap().property("Value"),
        Some(&Value::Instance(Some(new_hinge)))
    );
}

#[test]
fn a_world_saved_before_attributes_loads_unchanged() {
    let text = r#"{
  "format_version": 1,
  "workspace": {
    "children": [
      {
        "class": "Part",
        "name": "Block",
        "properties": { "Position": [0, 1, 0] }
      }
    ]
  }
}"#;
    let world = parse_world(text).unwrap();
    let block = world.find_first_child(world.workspace(), "Block").unwrap();
    assert!(world.get(block).unwrap().attributes().is_empty());
    assert!(world.get(block).unwrap().tags().is_empty());
    // Saving it again adds no attributes or tags fields.
    let saved = write_world(&world);
    assert!(
        !saved.contains("attributes") && !saved.contains("tags"),
        "{saved}"
    );
    assert_eq!(write_world(&parse_world(&saved).unwrap()), saved);
}

#[test]
fn a_bad_attribute_names_the_instance_and_attribute() {
    let text = r#"{ "format_version": 1, "workspace": { "children": [
        { "class": "Part", "name": "P", "attributes": { "Spawn": { "Vector4": [1] } } } ] } }"#;
    let err = parse_err(text);
    assert!(err.contains("Workspace.P: attribute `Spawn`"), "{err}");
}

#[test]
fn welds_and_attachments_round_trip_through_the_world_file() {
    let mut world = World::new();
    let workspace = world.workspace();
    let mut part = |name: &str| {
        let id = world.create(ClassName::Part).unwrap();
        world.set_name(id, name).unwrap();
        world.set_parent(id, Some(workspace)).unwrap();
        id
    };
    let (top, leg) = (part("Top"), part("Leg"));
    let weld = world.create(ClassName::WeldConstraint).unwrap();
    world.set_parent(weld, Some(leg)).unwrap();
    world
        .set_property(weld, "Part0", Value::Instance(Some(top)))
        .unwrap();
    world
        .set_property(weld, "Part1", Value::Instance(Some(leg)))
        .unwrap();
    world
        .set_property(weld, "Enabled", Value::Bool(false))
        .unwrap();
    let handle = world.create(ClassName::Attachment).unwrap();
    world.set_name(handle, "Handle").unwrap();
    world.set_parent(handle, Some(top)).unwrap();
    let offset = Vector3::new(1.5, 0.25, -2.0);
    let turn = Vector3::new(0.0, 45.0, 10.0);
    world
        .set_property(handle, "Position", Value::Vector3(offset))
        .unwrap();
    world
        .set_property(handle, "Orientation", Value::Vector3(turn))
        .unwrap();
    let text = write_world(&world);

    let loaded = parse_world(&text).unwrap();
    assert_eq!(write_world(&loaded), text);
    let top = loaded.find_first_child(loaded.workspace(), "Top").unwrap();
    let leg = loaded.find_first_child(loaded.workspace(), "Leg").unwrap();
    let weld = loaded.get(loaded.get(leg).unwrap().children()[0]).unwrap();
    assert_eq!(weld.class(), ClassName::WeldConstraint);
    assert_eq!(weld.property("Part0"), Some(&Value::Instance(Some(top))));
    assert_eq!(weld.property("Part1"), Some(&Value::Instance(Some(leg))));
    assert_eq!(weld.property("Enabled"), Some(&Value::Bool(false)));
    let handle = loaded.find_first_child(top, "Handle").unwrap();
    let handle = loaded.get(handle).unwrap();
    assert_eq!(handle.class(), ClassName::Attachment);
    assert_eq!(handle.property("Position"), Some(&Value::Vector3(offset)));
    assert_eq!(handle.property("Orientation"), Some(&Value::Vector3(turn)));
    assert!(text.contains(r#""Part0": ["Workspace", "Top"]"#), "{text}");
}

/// `name`'s number in the repository's `releases/formats.toml`, which the
/// release tools copy into each release record.
fn released_format(name: &str) -> u32 {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../releases/formats.toml");
    let text = std::fs::read_to_string(&path).unwrap();
    text.lines()
        .find_map(|line| {
            let (key, value) = line.split_once('=')?;
            (key.trim() == name).then(|| value.trim().parse().unwrap())
        })
        .unwrap_or_else(|| panic!("releases/formats.toml has no `{name}`"))
}

#[test]
fn the_release_files_list_this_world_format() {
    assert_eq!(released_format("world"), WORLD_FORMAT_VERSION);
}

/// Reading a parent with many children takes time in proportion to them.
/// Each child used to look up its prefab segment among all its siblings,
/// so S10k's 10,000 took 5.5 s in release and 30 s in debug; 20,000 would
/// take minutes. Now this is about a second in a debug build.
#[test]
fn a_parent_with_many_children_reads_quickly() {
    let mut world = World::new();
    let workspace = world.workspace();
    for _ in 0..20_000 {
        let part = world.create(ClassName::Part).unwrap();
        world.set_parent(part, Some(workspace)).unwrap();
    }
    let text = write_world(&world);
    let started = std::time::Instant::now();
    let read = parse_world(&text).unwrap();
    let spent = started.elapsed();
    let workspace = read.get(read.workspace()).unwrap();
    assert_eq!(workspace.children().len(), 20_000);
    assert!(
        spent < std::time::Duration::from_secs(20),
        "20,000 siblings took {spent:?} to read"
    );
}
