//! Text bound to the string table (`TextKey`): it shows the entry in the
//! world's active language, or the element's own Text.

use std::sync::Arc;

use axcenity_core::{ClassName, InstanceId, StringTable, Value, World};

use super::*;

/// A ScreenGui with a bound label, a bound button, a label bound to a key
/// the table doesn't have, and a label with no key.
fn menu() -> (World, InstanceId, [InstanceId; 4]) {
    let mut table = StringTable::default();
    table.add_language("English");
    table.add_language("Français");
    table.set("menu.title", "English", "Welcome!");
    table.set("menu.title", "Français", "Bienvenue !");
    table.set("menu.play", "English", "Play");
    table.set("menu.play", "Français", "Jouer");
    let mut world = World::new();
    world.set_strings(Arc::new(table));
    let gui = world.create(ClassName::ScreenGui).unwrap();
    world.set_parent(gui, Some(world.workspace())).unwrap();
    let mut add = |class, text: &str, key: &str, y: f32| {
        let id = world.create(class).unwrap();
        world.set_parent(id, Some(gui)).unwrap();
        world
            .set_property(id, "Text", Value::String(text.to_owned()))
            .unwrap();
        world
            .set_property(id, "TextKey", Value::String(key.to_owned()))
            .unwrap();
        world
            .set_property(id, "Position", Value::UDim2(UDim2::new(0.0, 0.0, 0.0, y)))
            .unwrap();
        world
            .set_property(id, "Size", Value::UDim2(UDim2::new(0.0, 300.0, 0.0, 40.0)))
            .unwrap();
        id
    };
    let title = add(ClassName::TextLabel, "Title source", "menu.title", 0.0);
    let play = add(ClassName::TextButton, "Play source", "menu.play", 50.0);
    let missing = add(
        ClassName::TextLabel,
        "Not translated",
        "menu.missing",
        100.0,
    );
    let plain = add(ClassName::TextLabel, "No key", "", 150.0);
    (world, gui, [title, play, missing, plain])
}

fn texts(layout: &UiLayout) -> Vec<String> {
    layout
        .draw_items
        .iter()
        .filter_map(|item| match item {
            UiDrawItem::Text(text) => Some(text.text.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_bound_label_shows_its_entry_in_the_active_language_in_play_and_on_the_artboard() {
    let (mut world, gui, _) = menu();
    let expected = ["Welcome!", "Play", "Not translated", "No key"];
    assert_eq!(texts(&layout_ui(&world, 800, 600, &[])), expected);
    let artboard = layout_artboard(&world, gui, &[], (800, 600), 0.5, [10.0, 10.0]);
    assert_eq!(texts(&artboard), expected);

    assert!(world.set_language("Français"));
    let expected = ["Bienvenue !", "Jouer", "Not translated", "No key"];
    assert_eq!(texts(&layout_ui(&world, 800, 600, &[])), expected);
    let artboard = layout_artboard(&world, gui, &[], (800, 600), 0.5, [10.0, 10.0]);
    assert_eq!(texts(&artboard), expected);
}

#[test]
fn switching_the_language_updates_every_bound_control_and_text_styles_follow() {
    let (mut world, _, [title, play, missing, plain]) = menu();
    let shown = |world: &World, id| {
        shown_text(world, world.get(id).unwrap())
            .unwrap()
            .to_owned()
    };
    assert!(world.set_language("Français"));
    assert_eq!(shown(&world, title), "Bienvenue !");
    assert_eq!(shown(&world, play), "Jouer");
    assert_eq!(text_style(&world, play, &[]).unwrap().text, "Jouer");
    // An unknown language changes nothing.
    assert!(!world.set_language("Klingon"));
    assert_eq!(world.language(), "Français");
    assert!(world.set_language("English"));
    assert_eq!(shown(&world, title), "Welcome!");
    assert_eq!(shown(&world, play), "Play");
    assert_eq!(shown(&world, missing), "Not translated");
    assert_eq!(shown(&world, plain), "No key");
}

#[test]
fn a_missing_entry_or_language_shows_the_source_text() {
    let (mut world, _, [title, _, missing, _]) = menu();
    let mut table = (**world.strings()).clone();
    table.add_language("Deutsch");
    world.set_strings(Arc::new(table));
    assert!(world.set_language("Deutsch"));
    let shown = |world: &World, id| {
        shown_text(world, world.get(id).unwrap())
            .unwrap()
            .to_owned()
    };
    // The key is there, but not in Deutsch.
    assert_eq!(shown(&world, title), "Title source");
    assert_eq!(shown(&world, missing), "Not translated");

    // A new table without the active language goes back to its first.
    let mut table = StringTable::default();
    table.add_language("English");
    world.set_strings(Arc::new(table));
    assert_eq!(world.language(), "English");
    assert_eq!(shown(&world, title), "Title source");
}
