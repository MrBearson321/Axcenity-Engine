//! Phase 54: Lua editor plugins in the editor: the example's Align to Grid
//! button and Part Tools tab, one undo step per waypoint, errors, and
//! reloading when a plugin's files change.

use std::time::{Duration, Instant};

use axcenity_core::{Value, Vector3};
use axcenity_render::UiDrawItem;

use super::tests::copy_of_example;
use super::*;

const SIZE: egui::Vec2 = egui::vec2(320.0, 400.0);

fn opened(test: &str) -> EditorApp {
    let folder = copy_of_example("hello-plugins", test);
    let mut app = EditorApp::new(None);
    app.open(&folder);
    app
}

fn part(app: &EditorApp, name: &str) -> InstanceId {
    app.world
        .find_first_child(app.world.workspace(), name)
        .unwrap()
}

fn position(app: &EditorApp, id: InstanceId) -> Vector3 {
    match app.world.get(id).unwrap().property("Position") {
        Some(Value::Vector3(v)) => *v,
        other => panic!("not a position: {other:?}"),
    }
}

fn plugin_index(app: &EditorApp, key: &str) -> usize {
    app.plugins
        .entries
        .iter()
        .position(|entry| entry.found.key == key)
        .unwrap()
}

/// The first button of the plugin `key`'s first toolbar.
fn first_button(app: &EditorApp, key: &str) -> (usize, u32) {
    let index = plugin_index(app, key);
    let toolbars = app.plugins.entries[index]
        .running
        .as_ref()
        .unwrap()
        .toolbars();
    (index, toolbars[0].buttons[0].id)
}

fn console(app: &EditorApp) -> Vec<String> {
    app.output
        .lines()
        .iter()
        .map(|line| line.text.clone())
        .collect()
}

#[test]
fn align_to_grid_snaps_the_selection_to_one_stud_as_one_undo_step() {
    let mut app = opened("plugin-align");
    assert!(
        app.plugins
            .entries
            .iter()
            .all(|entry| entry.running.is_some())
    );
    let (crate_part, pillar, base) = (
        part(&app, "Crate"),
        part(&app, "Pillar"),
        part(&app, "Baseplate"),
    );
    let before = [position(&app, crate_part), position(&app, pillar)];
    app.select_many(vec![crate_part, pillar]);
    app.plugins_frame(1.0 / 60.0);
    assert_eq!(
        app.history.undo_label(),
        None,
        "telling plugins the selection changed nothing"
    );

    let (index, button) = first_button(&app, "project/align-to-grid");
    app.click_plugin_button(index, button);
    assert_eq!(position(&app, crate_part), Vector3::new(2.0, 2.0, -4.0));
    assert_eq!(position(&app, pillar), Vector3::new(-5.0, 3.0, 2.0));
    assert_eq!(position(&app, base), Vector3::new(0.0, -0.5, 0.0));
    assert_eq!(app.history.undo_label(), Some("Align to Grid"));
    assert!(console(&app).contains(&"Aligned 2 part(s) to the grid".to_owned()));

    // One undo puts both back, and nothing else is left to undo.
    app.undo();
    assert_eq!([position(&app, crate_part), position(&app, pillar)], before);
    assert_eq!(app.history.undo_label(), None);
    app.redo();
    assert_eq!(position(&app, pillar), Vector3::new(-5.0, 3.0, 2.0));

    // A second click on aligned parts changes nothing: no new step.
    app.click_plugin_button(index, button);
    app.undo();
    assert_eq!([position(&app, crate_part), position(&app, pillar)], before);
}

#[test]
fn edits_between_waypoints_are_one_step_and_a_plugin_destroy_undoes() {
    let folder = copy_of_example("hello-plugins", "plugin-waypoints");
    let plugin = folder.join("plugins/steps");
    std::fs::create_dir_all(&plugin).unwrap();
    std::fs::write(
        plugin.join("plugin.json"),
        r#"{ "name": "Steps", "version": "1.0.0" }"#,
    )
    .unwrap();
    std::fs::write(
        plugin.join("init.lua"),
        "local bar = plugin:CreateToolbar('Steps')\n\
         local nudge = bar:CreateButton('Nudge')\n\
         nudge.Click:Connect(function()\n\
         \tworkspace.Crate.Position += Vector3.new(1, 0, 0)\n\
         end)\n\
         local done = bar:CreateButton('Done')\n\
         done.Click:Connect(function()\n\
         \tChangeHistoryService:SetWaypoint('Nudge Crate')\n\
         end)\n\
         local both = bar:CreateButton('Both')\n\
         both.Click:Connect(function()\n\
         \tlocal p = Instance.new('Part')\n\
         \tp.Name = 'Made'\n\
         \tp.Parent = workspace\n\
         \tChangeHistoryService:SetWaypoint('Make Part')\n\
         \tworkspace.Pillar:Destroy()\n\
         \tChangeHistoryService:SetWaypoint('Remove Pillar')\n\
         end)\n",
    )
    .unwrap();
    let mut app = EditorApp::new(None);
    app.open(&folder);
    let index = plugin_index(&app, "project/steps");
    let buttons: Vec<u32> = app.plugins.entries[index]
        .running
        .as_ref()
        .unwrap()
        .toolbars()[0]
        .buttons
        .iter()
        .map(|b| b.id)
        .collect();
    let crate_part = part(&app, "Crate");
    let start = position(&app, crate_part);

    // Two clicks, then the waypoint: one step.
    app.click_plugin_button(index, buttons[0]);
    app.click_plugin_button(index, buttons[0]);
    app.click_plugin_button(index, buttons[1]);
    assert_eq!(position(&app, crate_part).x, start.x + 2.0);
    assert_eq!(app.history.undo_label(), Some("Nudge Crate"));
    app.undo();
    assert_eq!(position(&app, crate_part), start);
    assert_eq!(app.history.undo_label(), None);

    // Two waypoints in one call: two steps, and the Destroy undoes.
    let pillar = part(&app, "Pillar");
    app.click_plugin_button(index, buttons[2]);
    assert!(
        app.world
            .find_first_child(app.world.workspace(), "Made")
            .is_some()
    );
    assert!(
        app.world
            .find_first_child(app.world.workspace(), "Pillar")
            .is_none()
    );
    assert_eq!(app.history.undo_label(), Some("Remove Pillar"));
    app.undo();
    assert_eq!(
        app.world.get(pillar).unwrap().parent(),
        Some(app.world.workspace())
    );
    assert!(
        app.world
            .find_first_child(app.world.workspace(), "Made")
            .is_some()
    );
    assert_eq!(app.history.undo_label(), Some("Make Part"));
    app.undo();
    assert!(
        app.world
            .find_first_child(app.world.workspace(), "Made")
            .is_none()
    );
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn an_erroring_plugin_shows_file_and_line_in_the_console_and_panel_and_the_editor_runs_on() {
    let folder = copy_of_example("hello-plugins", "plugin-error");
    let plugin = folder.join("plugins/broken");
    std::fs::create_dir_all(&plugin).unwrap();
    std::fs::write(
        plugin.join("plugin.json"),
        r#"{ "name": "Broken", "version": "0.1.0" }"#,
    )
    .unwrap();
    std::fs::write(
        plugin.join("init.lua"),
        "local bar = plugin:CreateToolbar('Broken')\n\
         local button = bar:CreateButton('Boom')\n\
         button.Click:Connect(function()\n\
         \tlocal nothing = nil\n\
         \tnothing.Name = 'x'\n\
         end)\n",
    )
    .unwrap();
    // One that doesn't even compile.
    let bad = folder.join("plugins/syntax");
    std::fs::create_dir_all(&bad).unwrap();
    std::fs::write(
        bad.join("plugin.json"),
        r#"{ "name": "Syntax", "version": "1" }"#,
    )
    .unwrap();
    std::fs::write(bad.join("init.lua"), "print('a')\nlocal = 1\n").unwrap();
    let mut app = EditorApp::new(None);
    app.open(&folder);

    let syntax = plugin_index(&app, "project/syntax");
    assert!(app.plugins.entries[syntax].running.is_none());
    assert!(
        app.plugins.entries[syntax].errors[0].starts_with("plugins/syntax/init.lua:2:"),
        "{:?}",
        app.plugins.entries[syntax].errors
    );

    let (index, button) = first_button(&app, "project/broken");
    app.click_plugin_button(index, button);
    let error = app.plugins.entries[index].errors.last().unwrap().clone();
    assert!(error.starts_with("plugins/broken/init.lua:5:"), "{error}");
    assert!(console(&app).contains(&error));
    // It points at the plugin's file, so the Console can open it.
    let location = crate::output::script_location(&error).unwrap();
    assert_eq!(location.path, "plugins/broken/init.lua");
    assert_eq!(location.line, 5);

    // The editor and the other plugins carry on.
    let (align, align_button) = first_button(&app, "project/align-to-grid");
    let pillar = part(&app, "Pillar");
    app.select_many(vec![pillar]);
    app.click_plugin_button(index, button);
    app.click_plugin_button(align, align_button);
    assert_eq!(position(&app, pillar), Vector3::new(-5.0, 3.0, 2.0));
    assert!(app.plugins.entries[index].running.is_some());

    // The Plugins panel draws its errors.
    let ctx = theme::test_context();
    let _ = crate::theme::run(&ctx, egui::RawInput::default(), |ui| {
        egui::CentralPanel::default().show(ui, |ui| app.plugins_pane(ui));
    });
    let _ = std::fs::remove_dir_all(&folder);
}

/// Draws the plugin dock tab `pane` once, with `events`.
fn frame(
    app: &mut EditorApp,
    ctx: &egui::Context,
    pane: u64,
    events: Vec<egui::Event>,
    time: &mut f64,
) {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, SIZE)),
        events,
        time: Some(*time),
        ..Default::default()
    };
    *time += 1.0 / 30.0;
    let _ = crate::theme::run(ctx, input, |ui| {
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| app.plugin_pane(ui, pane, None));
    });
}

fn click(app: &mut EditorApp, ctx: &egui::Context, pane: u64, at: egui::Pos2, time: &mut f64) {
    let press = |pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    frame(app, ctx, pane, vec![egui::Event::PointerMoved(at)], time);
    frame(app, ctx, pane, vec![press(true)], time);
    frame(app, ctx, pane, vec![press(false)], time);
}

/// The middle of `id` and its background colour, as the plugin's
/// stylesheets lay its dock tab out at the test's size.
fn element(app: &EditorApp, index: usize, id: InstanceId) -> (egui::Pos2, Option<[f32; 4]>) {
    let entry = &app.plugins.entries[index];
    let gui = entry.widgets[0].widget.gui;
    let pixels = (SIZE.x as u32, SIZE.y as u32);
    let layout =
        axcenity_ui::layout_artboard(&app.world, gui, &entry.stylesheets, pixels, 1.0, [0.0, 0.0]);
    let rect = layout.element(id).unwrap().rect;
    let color = layout.draw_items.iter().find_map(|item| match item {
        UiDrawItem::Rectangle(r) if r.position == [rect.x, rect.y] => Some(r.color),
        _ => None,
    });
    (
        egui::pos2(rect.x + rect.width / 2.0, rect.y + rect.height / 2.0),
        color,
    )
}

#[test]
fn the_part_tools_tab_slider_and_button_work_and_restyle_through_css() {
    let mut app = opened("plugin-dock");
    let index = plugin_index(&app, "project/part-tools");
    app.plugins_frame(1.0 / 60.0);
    let view = &app.plugins.entries[index].widgets[0];
    assert_eq!(view.widget.title, "Part Tools");
    let pane = view.pane;
    assert!(app.layout.is_open(Pane::Plugin(pane)));
    assert_eq!(app.plugin_pane_title(pane), "Part Tools");
    let gui = view.widget.gui;
    // Its UI isn't part of the game: not saved, not in the Explorer.
    assert_eq!(app.world.get(gui).unwrap().parent(), None);
    let find = |app: &EditorApp, class| {
        app.world
            .descendants(gui)
            .into_iter()
            .find(|&id| app.world.get(id).unwrap().class() == class)
            .unwrap()
    };
    let (slider, apply) = (
        find(&app, ClassName::Slider),
        find(&app, ClassName::TextButton),
    );

    let ctx = theme::test_context();
    let mut time = 0.0;
    frame(&mut app, &ctx, pane, Vec::new(), &mut time);
    assert!(
        !app.plugins.entries[index].stylesheets.is_empty(),
        "widget.css is read"
    );
    // A press near the slider's right end: 100%.
    let (middle, _) = element(&app, index, slider);
    let right = egui::pos2(SIZE.x - 14.0, middle.y);
    click(&mut app, &ctx, pane, right, &mut time);
    assert_eq!(
        app.world.get(slider).unwrap().property("Value"),
        Some(&Value::Number(100.0))
    );
    let label = find(&app, ClassName::TextLabel);
    assert_eq!(
        app.world.get(label).unwrap().property("Text"),
        Some(&Value::String("Transparency: 100%".into()))
    );
    assert_eq!(
        app.history.undo_label(),
        None,
        "the tab's own UI isn't undone"
    );

    // Apply gives the selected part that transparency, as one undo step.
    let crate_part = part(&app, "Crate");
    app.select_many(vec![crate_part]);
    app.plugins_frame(1.0 / 60.0);
    let (button, blue) = element(&app, index, apply);
    click(&mut app, &ctx, pane, button, &mut time);
    assert_eq!(
        app.world.get(crate_part).unwrap().property("Transparency"),
        Some(&Value::Number(1.0))
    );
    assert_eq!(app.history.undo_label(), Some("Set Transparency"));
    app.undo();
    assert_eq!(
        app.world.get(crate_part).unwrap().property("Transparency"),
        Some(&Value::Number(0.0))
    );

    // Saving a new colour into widget.css restyles the tab.
    let css = app
        .project
        .as_ref()
        .unwrap()
        .folder
        .join("plugins/part-tools/widget.css");
    let text = std::fs::read_to_string(&css).unwrap();
    std::fs::write(
        &css,
        text.replace("--accent: #3b82f6;", "--accent: #ff0000;"),
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut restyled = false;
    while Instant::now() < deadline && !restyled {
        std::thread::sleep(Duration::from_millis(100));
        app.plugins_frame(1.0 / 60.0);
        let index = plugin_index(&app, "project/part-tools");
        let apply = app
            .world
            .descendants(app.plugins.entries[index].widgets[0].widget.gui)
            .into_iter()
            .find(|&id| app.world.get(id).unwrap().class() == ClassName::TextButton)
            .unwrap();
        restyled = element(&app, index, apply).1 == Some([1.0, 0.0, 0.0, 1.0]);
    }
    assert!(restyled, "the button was {blue:?} and stayed that way");
    assert!(console(&app).contains(&"Reloaded the plugin \"Part Tools\".".to_owned()));
    // The slider's value came back from the plugin's saved setting.
    let index = plugin_index(&app, "project/part-tools");
    let gui = app.plugins.entries[index].widgets[0].widget.gui;
    let slider = app
        .world
        .descendants(gui)
        .into_iter()
        .find(|&id| app.world.get(id).unwrap().class() == ClassName::Slider)
        .unwrap();
    assert_eq!(
        app.world.get(slider).unwrap().property("Value"),
        Some(&Value::Number(100.0))
    );
}

/// Roadmap G10: the example's Align to Grid draws where the selected parts
/// would snap to, the Scene View gets those drawings while editing, and
/// they go when the parts are aligned or the plugin is turned off.
#[test]
fn align_to_grid_draws_where_the_selection_would_snap_until_aligned() {
    let mut app = opened("plugin-drawings");
    let index = plugin_index(&app, "project/align-to-grid");
    assert!(app.plugins.entries[index].drawings.is_empty());
    let crate_part = part(&app, "Crate");
    app.select_many(vec![crate_part]);
    app.plugins_frame(1.0 / 60.0);
    // A box where the Crate would go, and a line from it to there.
    let drawings = &app.plugins.entries[index].drawings;
    assert_eq!(drawings.len(), 2, "{drawings:?}");
    assert!(matches!(
        &drawings[0].shape,
        axcenity_plugins::Shape::Box { cframe, .. } if cframe.position == Vector3::new(2.0, 2.0, -4.0)
    ));
    assert!(matches!(
        drawings[1].shape,
        axcenity_plugins::Shape::Line { .. }
    ));
    assert_eq!(app.history.undo_label(), None, "drawing isn't an edit");

    let (_, button) = first_button(&app, "project/align-to-grid");
    app.click_plugin_button(index, button);
    assert!(app.plugins.entries[index].drawings.is_empty());

    app.undo();
    app.select_many(Vec::new());
    app.plugins_frame(1.0 / 60.0);
    assert!(app.plugins.entries[index].drawings.is_empty());
    app.select_many(vec![crate_part]);
    app.plugins_frame(1.0 / 60.0);
    assert_eq!(app.plugins.entries[index].drawings.len(), 2);
    app.plugins.set_enabled("project/align-to-grid", false);
    app.reload_plugin(index);
    assert!(app.plugins.entries[index].drawings.is_empty());
}

/// Roadmap G10: Part Tools puts a height handle on the selected part;
/// dragging it resizes the part, the whole drag is one undo step, and the
/// handle follows the part.
#[test]
fn dragging_part_tools_height_handle_resizes_the_part_as_one_undo_step() {
    let mut app = opened("plugin-handles");
    let index = plugin_index(&app, "project/part-tools");
    assert!(app.plugins.entries[index].handles.is_empty());
    let crate_part = part(&app, "Crate");
    app.select_many(vec![crate_part]);
    app.plugins_frame(1.0 / 60.0);
    let handles = app.plugins.entries[index].handles.clone();
    assert_eq!(handles.len(), 1, "{handles:?}");
    assert_eq!(handles[0].position, Vector3::new(2.3, 2.6, -3.7));
    assert_eq!(handles[0].axis, Some(Vector3::new(0.0, 1.0, 0.0)));

    let size = |app: &EditorApp| match app.world.get(crate_part).unwrap().property("Size") {
        Some(Value::Vector3(v)) => *v,
        other => panic!("{other:?}"),
    };
    let moved = |y: f32, ended: bool| crate::viewport::HandleMove {
        plugin: index,
        id: handles[0].id,
        position: Vector3::new(2.3, y, -3.7),
        ended,
    };
    app.drag_plugin_handle(moved(3.6, false));
    app.drag_plugin_handle(moved(4.6, false));
    app.drag_plugin_handle(moved(4.6, true));
    assert_eq!(size(&app), Vector3::new(2.0, 4.0, 2.0));
    assert!((position(&app, crate_part).y - 2.6).abs() < 1e-5);
    assert_eq!(app.history.undo_label(), Some("Resize Height"));
    // The handle went to the part's new top.
    let handles = &app.plugins.entries[index].handles;
    assert_eq!(handles.len(), 1);
    assert!((handles[0].position.y - 4.6).abs() < 1e-5);

    app.undo();
    assert_eq!(size(&app), Vector3::new(2.0, 2.0, 2.0));
    assert_eq!(position(&app, crate_part), Vector3::new(2.3, 1.6, -3.7));
    assert_eq!(
        app.history.undo_label(),
        None,
        "the whole drag was one step"
    );

    // Nothing selected, no handle.
    app.select_many(Vec::new());
    app.plugins_frame(1.0 / 60.0);
    assert!(app.plugins.entries[index].handles.is_empty());
}

/// Roadmap G10: a plugin's attribute row sets the attribute as an undo
/// step and tells the plugin; Align to Grid's Snap to Grid button row snaps
/// the part it's clicked for.
#[test]
fn plugin_properties_rows_set_attributes_and_buttons_call_the_plugin() {
    let folder = copy_of_example("hello-plugins", "plugin-rows");
    let speed = folder.join("plugins").join("speed");
    std::fs::create_dir_all(&speed).unwrap();
    std::fs::write(
        speed.join("plugin.json"),
        r#"{ "name": "Speed", "version": "1.0.0" }"#,
    )
    .unwrap();
    std::fs::write(
        speed.join("init.lua"),
        "local row = plugin:AddPropertyRow({ Attribute = 'Speed', Default = 16, Class = 'BasePart', Min = 0, Max = 50 })\n\
         row.Changed:Connect(function(part, value) print('speed', part.Name, value) end)\n",
    )
    .unwrap();
    let mut app = EditorApp::new(None);
    app.open(&folder);
    let crate_part = part(&app, "Crate");
    app.select_many(vec![crate_part]);
    let speed = plugin_index(&app, "project/speed");
    let row = app.plugins.entries[speed].rows[0].id;
    let names: Vec<_> = app
        .plugins
        .property_rows()
        .iter()
        .map(|rows| rows.name.to_owned())
        .collect();
    assert_eq!(names, ["Align to Grid", "Speed"]);

    app.plugin_row_action(
        crate_part,
        crate::properties::RowAction::Set {
            plugin: speed,
            row,
            name: "Speed".into(),
            value: Value::Number(20.0),
        },
    );
    let attribute = |app: &EditorApp| {
        app.world
            .get(crate_part)
            .unwrap()
            .attribute("Speed")
            .cloned()
    };
    assert_eq!(attribute(&app), Some(Value::Number(20.0)));
    assert_eq!(
        app.history.undo_label(),
        Some("Set attribute Speed of Crate")
    );
    assert!(
        console(&app).contains(&"speed\tCrate\t20".to_owned()),
        "{:?}",
        console(&app)
    );
    app.undo();
    assert_eq!(attribute(&app), None);

    let align = plugin_index(&app, "project/align-to-grid");
    let snap_row = app.plugins.entries[align].rows[0].id;
    app.plugin_row_action(
        crate_part,
        crate::properties::RowAction::Click {
            plugin: align,
            row: snap_row,
        },
    );
    assert_eq!(position(&app, crate_part), Vector3::new(2.0, 2.0, -4.0));
    assert_eq!(app.history.undo_label(), Some("Snap to Grid"));
}

#[test]
fn turning_a_plugin_off_takes_its_tab_and_buttons_away() {
    let mut app = opened("plugin-off");
    let index = plugin_index(&app, "project/part-tools");
    app.plugins_frame(1.0 / 60.0);
    let pane = app.plugins.entries[index].widgets[0].pane;
    assert!(app.layout.is_open(Pane::Plugin(pane)));
    app.plugins.set_enabled("project/part-tools", false);
    app.reload_plugin(index);
    app.plugins_frame(1.0 / 60.0);
    assert!(app.plugins.entries[index].running.is_none());
    assert!(!app.layout.is_open(Pane::Plugin(pane)));
    // Plugin tabs aren't saved with the layout.
    assert!(!app.layout.to_text().contains("\"Plugin\""));
}
