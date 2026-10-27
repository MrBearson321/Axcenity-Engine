//! Attributes, tags and Value objects in the editor (Phase 29): the
//! Properties panel's Attributes section and Tags row, the Tags panel, and
//! their undo steps.

use axcenity_core::{ClassName, Value};

use super::tests::{copy_of_example, part, play_for, printed};
use super::*;
use crate::properties::Edit;
use crate::tags::{TagAction, TagsPanel};

#[test]
fn an_attribute_set_in_properties_is_read_in_play_and_its_signal_fires_when_another_script_changes_it()
 {
    let folder = copy_of_example("hello-touched", "attribute-play");
    std::fs::write(
        folder.join("scripts/reader.server.lua"),
        r#"
local pad = workspace.Pad
print("speed " .. pad:GetAttribute("Speed"))
pad:GetAttributeChangedSignal("Speed"):Connect(function()
    print("speed now " .. pad:GetAttribute("Speed"))
end)
"#,
    )
    .unwrap();
    std::fs::write(
        folder.join("scripts/writer.server.lua"),
        "task.wait(0.5)\nworkspace.Pad:SetAttribute(\"Speed\", 20)\n",
    )
    .unwrap();
    let mut app = EditorApp::new(Some(&folder));
    let pad = part(&app, "Pad");

    // Added in the Attributes section as a number, then given its value:
    // two undo steps.
    app.apply(
        pad,
        Edit::Attribute("Speed".into(), Some(Value::Number(0.0))),
    );
    app.history.seal();
    app.apply(
        pad,
        Edit::Attribute("Speed".into(), Some(Value::Number(12.0))),
    );
    app.history.seal();
    assert_eq!(app.history.undo_label(), Some("Set attribute Speed of Pad"));
    app.undo();
    assert_eq!(
        app.world.get(pad).unwrap().attribute("Speed"),
        Some(&Value::Number(0.0))
    );
    app.redo();
    assert!(app.unsaved());

    app.play();
    play_for(&mut app, 1.0);
    let lines = printed(&app);
    assert!(lines.contains(&"speed 12"), "{lines:?}");
    assert!(lines.contains(&"speed now 20"), "{lines:?}");
    app.stop();
    // Stop puts the edit world's value back.
    assert_eq!(
        app.world.get(pad).unwrap().attribute("Speed"),
        Some(&Value::Number(12.0))
    );
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn tags_added_and_removed_in_properties_are_one_undo_step_each() {
    let folder = copy_of_example("hello-touched", "tag-properties");
    let mut app = EditorApp::new(Some(&folder));
    let pad = part(&app, "Pad");
    app.apply(pad, Edit::AddTag("Lava".into()));
    app.apply(pad, Edit::AddTag("Hot".into()));
    app.apply(pad, Edit::RemoveTag("Lava".into()));
    assert_eq!(app.world.get(pad).unwrap().tags(), ["Hot"]);
    app.undo();
    assert_eq!(app.world.get(pad).unwrap().tags(), ["Lava", "Hot"]);
    app.undo();
    assert_eq!(app.world.get(pad).unwrap().tags(), ["Lava"]);
    app.undo();
    assert!(app.world.get(pad).unwrap().tags().is_empty());
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn renaming_a_tag_in_the_tags_panel_renames_it_everywhere_as_one_undo_step() {
    let folder = copy_of_example("hello-touched", "tag-rename");
    let mut app = EditorApp::new(Some(&folder));
    let pad = part(&app, "Pad");
    let ball = app.world.create(ClassName::Part).unwrap();
    app.world.set_name(ball, "Ball2").unwrap();
    app.world
        .set_parent(ball, Some(app.world.workspace()))
        .unwrap();
    app.world.add_tag(pad, "Lava").unwrap();
    app.world.add_tag(pad, "Hot").unwrap();
    app.world.add_tag(ball, "Hot").unwrap();
    app.world.add_tag(ball, "Lava").unwrap();
    // Already has the new name: it just loses the old one.
    app.world.add_tag(ball, "Magma").unwrap();

    // The panel lists each tag; its Select button selects them all.
    let tags = app.world.all_tags();
    assert_eq!(tags.get("Lava"), Some(&2));
    app.select_tagged("Lava");
    assert_eq!(app.selection, [pad, ball]);

    // Click Rename on the Lava row, type, press Enter.
    let ctx = theme::test_context();
    let mut panel = TagsPanel::default();
    let mut action = TagAction::None;
    let frame = |events: Vec<egui::Event>, panel: &mut TagsPanel| {
        let input = egui::RawInput {
            events,
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(260.0, 400.0),
            )),
            ..Default::default()
        };
        let mut result = TagAction::None;
        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                result = panel.show(ui, &tags);
            });
        });
        result
    };
    frame(Vec::new(), &mut panel);
    // Start renaming without a pointer: the state the Rename button sets.
    panel.start_renaming("Lava");
    frame(Vec::new(), &mut panel);
    frame(
        vec![
            egui::Event::Key {
                key: egui::Key::A,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::COMMAND,
            },
            egui::Event::Text("Magma".into()),
        ],
        &mut panel,
    );
    for _ in 0..2 {
        let made = frame(
            vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            &mut panel,
        );
        if made != TagAction::None {
            action = made;
        }
    }
    assert_eq!(action, TagAction::Rename("Lava".into(), "Magma".into()));
    let TagAction::Rename(old, new) = action else {
        unreachable!()
    };
    app.rename_tag(&old, &new);

    assert_eq!(app.world.get(pad).unwrap().tags(), ["Magma", "Hot"]);
    assert_eq!(app.world.get(ball).unwrap().tags(), ["Hot", "Magma"]);
    assert!(app.world.tagged("Lava").is_empty());
    assert_eq!(app.history.undo_label(), Some("Rename tag Lava to Magma"));
    // One undo puts every Instance back.
    app.undo();
    assert_eq!(app.world.get(pad).unwrap().tags(), ["Lava", "Hot"]);
    assert_eq!(
        app.world.get(ball).unwrap().tags(),
        ["Hot", "Lava", "Magma"]
    );
    app.redo();
    assert_eq!(app.world.get(pad).unwrap().tags(), ["Magma", "Hot"]);
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn attributes_tags_and_value_objects_survive_save_and_reopen() {
    let folder = copy_of_example("hello-touched", "attribute-save");
    let mut app = EditorApp::new(Some(&folder));
    let pad = part(&app, "Pad");
    app.apply(
        pad,
        Edit::Attribute("Damage".into(), Some(Value::Number(5.0))),
    );
    app.apply(pad, Edit::AddTag("Lava".into()));
    let link = app.world.create(ClassName::ObjectValue).unwrap();
    app.world
        .set_parent(link, Some(app.world.workspace()))
        .unwrap();
    app.apply(link, Edit::Property("Value", Value::Instance(Some(pad))));
    app.save();

    let reopened = EditorApp::new(Some(&folder));
    let pad = part(&reopened, "Pad");
    let instance = reopened.world.get(pad).unwrap();
    assert_eq!(instance.attribute("Damage"), Some(&Value::Number(5.0)));
    assert_eq!(instance.tags(), ["Lava"]);
    let link = part(&reopened, "ObjectValue");
    assert_eq!(
        reopened.world.get(link).unwrap().property("Value"),
        Some(&Value::Instance(Some(pad)))
    );
    let _ = std::fs::remove_dir_all(&folder);
}
