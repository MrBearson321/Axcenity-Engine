//! Phase 49: nested prefabs and variants, through the editor's Save prefab,
//! placing, the Explorer's Prefab menu, Properties' ↺ and undo.

use axcenity_core::prefab;

use super::tests::copy_of_example;
use super::*;
use crate::prefab_menu::PrefabAction;

fn color(r: f32, g: f32, b: f32) -> Value {
    Value::Color3(axcenity_core::Color3::new(r, g, b))
}

fn value(app: &EditorApp, id: InstanceId, name: &str) -> Value {
    app.world.get(id).unwrap().property(name).unwrap().clone()
}

fn new_in(app: &mut EditorApp, class: ClassName, name: &str, parent: InstanceId) -> InstanceId {
    let id = app.world.create(class).unwrap();
    app.world.set_name(id, name).unwrap();
    app.world.set_parent(id, Some(parent)).unwrap();
    id
}

/// Selects `id` and saves it as a prefab; returns the new file's path.
fn save_as_prefab(app: &mut EditorApp, id: InstanceId) -> String {
    app.selection = vec![id];
    app.save_prefab();
    app.world.get(id).unwrap().prefab().unwrap().path.clone()
}

fn place(app: &mut EditorApp, path: &str) -> InstanceId {
    let workspace = app.world.workspace();
    app.instantiate_prefab(path, workspace).unwrap()[0]
}

fn file(app: &EditorApp, path: &str) -> Option<String> {
    std::fs::read_to_string(app.project.as_ref().unwrap().folder.join(path)).ok()
}

fn opened(test: &str) -> EditorApp {
    let folder = copy_of_example("hello-part", test);
    let mut app = EditorApp::new(None);
    app.open(&folder);
    app
}

#[test]
fn applying_a_copys_colour_reaches_the_other_copies_and_undo_takes_it_back() {
    let mut app = opened("prefab-apply");
    let workspace = app.world.workspace();
    let first = new_in(&mut app, ClassName::Part, "Crate", workspace);
    let path = save_as_prefab(&mut app, first);
    assert_eq!(path, "assets/prefabs/Crate.prefab.json");
    assert!(prefab::overrides(&app.world, first).is_empty());
    let second = place(&mut app, &path);
    let grey = value(&app, second, "Color");

    let green = color(0.0, 1.0, 0.0);
    app.apply(first, Edit::Property("Color", green.clone()));
    app.history.seal();
    let place = prefab::place(&app.world, first).unwrap();
    assert!(place.here().properties.contains("Color"), "shown bold");

    let before = file(&app, &path);
    app.prefab_action(PrefabAction::ApplyAll(first));
    assert_eq!(value(&app, second, "Color"), green);
    assert!(prefab::overrides(&app.world, first).is_empty());
    assert_eq!(app.history.undo_label(), Some("Apply overrides to Crate"));

    // One undo step puts back the file and the other copy.
    app.undo();
    assert_eq!(file(&app, &path), before);
    assert_eq!(value(&app, second, "Color"), grey);
    assert_eq!(value(&app, first, "Color"), green);
    app.redo();
    assert_eq!(value(&app, second, "Color"), green);
    assert!(file(&app, &path).unwrap() != before.unwrap());
}

#[test]
fn apply_one_revert_one_revert_all_and_unpack_each_do_one_thing() {
    let mut app = opened("prefab-menu");
    let workspace = app.world.workspace();
    let first = new_in(&mut app, ClassName::Part, "Crate", workspace);
    let path = save_as_prefab(&mut app, first);
    let second = place(&mut app, &path);
    let red = color(1.0, 0.0, 0.0);
    app.apply(first, Edit::Property("Color", red.clone()));
    app.history.seal();
    app.apply(first, Edit::Property("Transparency", Value::Number(0.5)));
    app.history.seal();

    // Apply just the colour: the other copy takes it, not the transparency.
    app.apply(first, Edit::ApplyOverride("Color".to_owned()));
    assert_eq!(value(&app, second, "Color"), red);
    assert_eq!(value(&app, second, "Transparency"), Value::Number(0.0));
    let here = prefab::place(&app.world, first).unwrap().here();
    assert_eq!(here.properties.iter().collect::<Vec<_>>(), ["Transparency"]);

    // ↺ on the transparency.
    app.apply(first, Edit::RevertOverride("Transparency".to_owned()));
    assert_eq!(value(&app, first, "Transparency"), Value::Number(0.0));
    assert!(prefab::overrides(&app.world, first).is_empty());
    app.undo();
    assert_eq!(value(&app, first, "Transparency"), Value::Number(0.5));

    // Revert all keeps where it was put, but not its look.
    let moved = Value::Vector3(axcenity_core::Vector3::new(9.0, 1.0, 9.0));
    app.apply(first, Edit::Property("Position", moved.clone()));
    app.history.seal();
    app.prefab_action(PrefabAction::RevertAll(first));
    assert_eq!(value(&app, first, "Position"), moved);
    assert_eq!(value(&app, first, "Transparency"), Value::Number(0.0));

    // Unpacked, it no longer follows.
    app.prefab_action(PrefabAction::Unpack(first));
    assert!(app.world.get(first).unwrap().prefab().is_none());
    app.undo();
    assert!(app.world.get(first).unwrap().prefab().is_some());
}

#[test]
fn changing_the_wheel_prefab_updates_the_wheels_inside_every_car_copy() {
    let mut app = opened("prefab-nested");
    let workspace = app.world.workspace();
    let wheel = new_in(&mut app, ClassName::Part, "Wheel", workspace);
    let wheel_path = save_as_prefab(&mut app, wheel);

    // A Car of a Body and two Wheel copies, saved as a prefab of its own.
    let car = new_in(&mut app, ClassName::Model, "Car", workspace);
    new_in(&mut app, ClassName::Part, "Body", car);
    for _ in 0..2 {
        app.instantiate_prefab(&wheel_path, car).unwrap();
    }
    let car_path = save_as_prefab(&mut app, car);
    assert!(
        file(&app, &car_path)
            .unwrap()
            .contains(r#""prefab": "assets/prefabs/Wheel.prefab.json""#)
    );
    let other_car = place(&mut app, &car_path);

    let orange = color(1.0, 0.5, 0.0);
    app.apply(wheel, Edit::Property("Color", orange.clone()));
    app.history.seal();
    app.prefab_action(PrefabAction::ApplyAll(wheel));

    for car in [car, other_car] {
        let wheels: Vec<_> = app.world.get(car).unwrap().children().to_vec();
        let wheels: Vec<_> = wheels
            .into_iter()
            .filter(|&id| app.world.get(id).unwrap().name() == "Wheel")
            .collect();
        assert_eq!(wheels.len(), 2);
        for wheel in wheels {
            assert_eq!(value(&app, wheel, "Color"), orange);
        }
    }

    // And so does the world when it's opened again.
    assert!(app.save());
    let folder = app.project.as_ref().unwrap().folder.clone();
    let mut reopened = EditorApp::new(None);
    reopened.open(&folder);
    let car = reopened
        .world
        .find_first_child(reopened.world.workspace(), "Car")
        .unwrap();
    let wheel = reopened.world.find_first_child(car, "Wheel").unwrap();
    assert_eq!(value(&reopened, wheel, "Color"), orange);
}

#[test]
fn a_red_enemy_variant_keeps_its_colour_and_takes_the_bases_max_health() {
    let mut app = opened("prefab-variant");
    let workspace = app.world.workspace();
    let enemy = new_in(&mut app, ClassName::Model, "Enemy", workspace);
    let body = new_in(&mut app, ClassName::Part, "Body", enemy);
    let health = new_in(&mut app, ClassName::Health, "Health", enemy);
    let enemy_path = save_as_prefab(&mut app, enemy);

    app.new_variant(&enemy_path);
    let variant_path = "assets/prefabs/Enemy Variant.prefab.json";
    assert_eq!(app.history.undo_label(), Some("New variant of Enemy"));
    let red_enemy = place(&mut app, variant_path);
    let red_body = app.world.find_first_child(red_enemy, "Body").unwrap();
    let red = color(1.0, 0.0, 0.0);
    app.apply(red_body, Edit::Property("Color", red.clone()));
    app.history.seal();
    app.prefab_action(PrefabAction::ApplyAll(red_enemy));

    // The base gets a new colour and MaxHealth.
    app.apply(body, Edit::Property("Color", color(0.0, 0.0, 1.0)));
    app.history.seal();
    app.apply(health, Edit::Property("MaxHealth", Value::Number(250.0)));
    app.history.seal();
    app.prefab_action(PrefabAction::ApplyAll(enemy));

    assert_eq!(value(&app, red_body, "Color"), red);
    let red_health = app.world.find_first_child(red_enemy, "Health").unwrap();
    assert_eq!(value(&app, red_health, "MaxHealth"), Value::Number(250.0));
}

#[test]
fn renaming_a_prefab_keeps_its_copies_linked() {
    let mut app = opened("prefab-rename");
    let workspace = app.world.workspace();
    let first = new_in(&mut app, ClassName::Part, "Crate", workspace);
    let path = save_as_prefab(&mut app, first);
    app.rename_prefab(&path, "Box");
    let link = app.world.get(first).unwrap().prefab().unwrap().path.clone();
    assert_eq!(link, "assets/prefabs/Box.prefab.json");
    app.undo();
    let link = app.world.get(first).unwrap().prefab().unwrap().path.clone();
    assert_eq!(link, path);
    assert!(file(&app, &path).is_some());
}

/// Drops the prefab at `path` from the Assets panel, over `ui` if given.
fn drop_prefab_at(app: &mut EditorApp, path: &str, ui: Option<InstanceId>) {
    let entry = crate::asset_browser::AssetEntry {
        kind: crate::asset_browser::AssetKind::Prefab,
        path: path.to_owned(),
        name: crate::asset_browser::display_name(path),
    };
    let target = crate::viewport::DropTarget {
        point: axcenity_render::glam::Vec3::ZERO,
        normal: axcenity_render::glam::Vec3::Y,
        part: None,
        ui,
        released: true,
    };
    app.drop_asset(&entry, Some(target));
}

#[test]
fn a_screengui_with_its_script_saves_as_a_prefab_and_places_as_a_linked_copy() {
    let mut app = opened("prefab-ui-gui");
    let workspace = app.world.workspace();
    let gui = new_in(&mut app, ClassName::ScreenGui, "Hud", workspace);
    let panel = new_in(&mut app, ClassName::Frame, "Panel", gui);
    let script = new_in(&mut app, ClassName::LocalScript, "Menu", gui);
    let source = Value::String("objects/Hud/Menu.client.lua".to_owned());
    app.world
        .set_property(script, "SourceFile", source)
        .unwrap();
    assert!(tree::can_be_prefab(&app.world, gui));
    assert!(tree::can_be_prefab(&app.world, panel));

    let path = save_as_prefab(&mut app, gui);
    assert_eq!(path, "assets/prefabs/Hud.prefab.json");
    let text = file(&app, &path).unwrap();
    assert!(text.contains("objects/Hud/Menu.client.lua"), "{text}");

    drop_prefab_at(&mut app, &path, None);
    let copy = app.selected.expect("the copy is selected");
    assert_ne!(copy, gui);
    let placed = app.world.get(copy).unwrap();
    assert_eq!(placed.class(), ClassName::ScreenGui);
    assert_eq!(placed.parent(), Some(workspace), "a layer of its own");
    assert_eq!(placed.prefab().unwrap().path, path);
    let menu = app.world.find_first_child(copy, "Menu").unwrap();
    assert_eq!(
        app.world.get(menu).unwrap().source_file(),
        Some("objects/Hud/Menu.client.lua"),
        "shares the script's file"
    );
    assert!(prefab::overrides(&app.world, copy).is_empty());
}

#[test]
fn a_dropped_ui_element_prefab_goes_into_the_ui_or_a_new_screengui() {
    let mut app = opened("prefab-ui-element");
    let workspace = app.world.workspace();
    let gui = new_in(&mut app, ClassName::ScreenGui, "Hud", workspace);
    let button = new_in(&mut app, ClassName::TextButton, "Play", gui);
    let path = save_as_prefab(&mut app, button);
    // The original goes, so the Workspace has no ScreenGui.
    app.world.set_parent(gui, None).unwrap();
    let before = app.world.get(workspace).unwrap().children().len();

    drop_prefab_at(&mut app, &path, None);
    let copy = app.selected.expect("the copy is selected");
    let new_gui = app.world.get(copy).unwrap().parent().unwrap();
    assert_eq!(
        app.world.get(new_gui).unwrap().class(),
        ClassName::ScreenGui
    );
    assert_eq!(app.world.get(new_gui).unwrap().parent(), Some(workspace));
    assert_eq!(app.history.undo_label(), Some("Place Play"));
    app.undo();
    assert_eq!(
        app.world.get(workspace).unwrap().children().len(),
        before,
        "the copy and its ScreenGui go in one step"
    );

    // Dropped over a Frame in a ScreenGui, it goes into that Frame.
    let menu = new_in(&mut app, ClassName::ScreenGui, "Menu", workspace);
    let frame = new_in(&mut app, ClassName::Frame, "Buttons", menu);
    drop_prefab_at(&mut app, &path, Some(frame));
    let copy = app.selected.unwrap();
    assert_eq!(app.world.get(copy).unwrap().parent(), Some(frame));
    assert_eq!(app.world.get(copy).unwrap().prefab().unwrap().path, path);
}
