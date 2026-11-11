//! The UI Designer's Styles: built-in style sets and sets imported from
//! CSS, copied into an element's own rules in one undo step.

use std::path::PathBuf;

use axcenity_core::UDim2;

use super::tests::copy_of_example;
use super::ui_designer_tests::{act, drop_in, stylesheets};
use super::*;
use crate::ui_designer::DesignerAction;
use crate::ui_designer::style_sets::{BUILT_IN, built_in_rules, project_sets};

const SIZE: egui::Vec2 = egui::vec2(1400.0, 900.0);

/// A new ScreenGui with a 120×40 button named Play in it.
fn play_button(app: &mut EditorApp) -> (InstanceId, InstanceId) {
    act(app, DesignerAction::NewScreenGui);
    let gui = app.selected.unwrap();
    let play = drop_in(app, Insertable::TextButton, gui, [100.0, 100.0]);
    act(app, DesignerAction::Edit(play, Edit::Name("Play".into())));
    let size = Value::UDim2(UDim2::new(0.0, 120.0, 0.0, 40.0));
    act(
        app,
        DesignerAction::Edit(play, Edit::Property("Size", size)),
    );
    (gui, play)
}

/// The built-in set named `name` for buttons, as the tile applies it.
fn built_in(ids: Vec<InstanceId>, name: &str) -> DesignerAction {
    let built_in = BUILT_IN
        .iter()
        .find(|built_in| {
            built_in.name == name && built_in.kind == crate::ui_designer::style_sets::Kind::Button
        })
        .unwrap();
    DesignerAction::ApplyStyleSet {
        ids,
        name: name.into(),
        rules: built_in_rules(built_in).rules,
    }
}

/// `id`'s own rules: `""`, `:hover` and so on, with their declarations
/// (none without an `Id`).
fn own(app: &EditorApp, id: InstanceId) -> Vec<(String, Vec<(String, String)>)> {
    match crate::ui_designer::arrange::element_id(&app.world, id) {
        Some(element_id) => axcenity_ui::own_rules(&stylesheets(app), element_id),
        None => Vec::new(),
    }
}

fn own_value(app: &EditorApp, id: InstanceId, suffix: &str, property: &str) -> Option<String> {
    own(app, id)
        .into_iter()
        .find(|(known, _)| known == suffix)?
        .1
        .into_iter()
        .find(|(name, _)| name == property)
        .map(|(_, value)| value)
}

#[test]
fn a_built_in_style_set_styles_every_state_in_one_step_and_stays_editable() {
    let folder = copy_of_example("hello-touched", "ui-styles-built-in");
    let mut app = EditorApp::new(Some(&folder));
    let (_, play) = play_button(&mut app);

    act(&mut app, built_in(vec![play], "Primary"));
    assert_eq!(app.history.undo_label(), Some("Style Play as Primary"));
    assert_eq!(
        own_value(&app, play, "", "background-color").as_deref(),
        Some("#3b82f6")
    );
    assert_eq!(
        own_value(&app, play, ":hover", "background-color").as_deref(),
        Some("#2563eb")
    );
    assert_eq!(
        own_value(&app, play, ":active", "transform").as_deref(),
        Some("scale(0.97)")
    );
    assert_eq!(
        own_value(&app, play, ":disabled", "opacity").as_deref(),
        Some("0.5")
    );

    // Another set replaces all of the first, in place.
    let rules_before = own(&app, play).len();
    act(&mut app, built_in(vec![play], "Outline"));
    assert_eq!(own(&app, play).len(), rules_before);
    assert_eq!(
        own_value(&app, play, "", "border").as_deref(),
        Some("2px solid #60a5fa")
    );
    assert_eq!(
        own_value(&app, play, "", "box-shadow").as_deref(),
        Some("none")
    );
    app.undo();
    assert_eq!(own_value(&app, play, "", "border").as_deref(), Some("none"));
    app.redo();

    // It's still the element's own CSS: the text toolbar changes it in place.
    act(
        &mut app,
        DesignerAction::TextStyle {
            id: play,
            property: "color",
            value: "#ffffff".into(),
            dragging: false,
        },
    );
    assert_eq!(
        own_value(&app, play, "", "color").as_deref(),
        Some("#ffffff")
    );
    let colors = own(&app, play)[0]
        .1
        .iter()
        .filter(|(name, _)| name == "color")
        .count();
    assert_eq!(colors, 1);

    // Drawn with Play selected: the Styles section and its tiles.
    let ctx = theme::test_context();
    act(&mut app, DesignerAction::Select(Some(play)));
    if let Some(snapshots) = std::env::var_os("AXCENITY_SNAPSHOTS").map(PathBuf::from) {
        crate::snapshot::snapshot_rendered(
            &snapshots.join("ui-designer-styles.png"),
            SIZE,
            vec![Vec::new(); 3],
            |ui, render_state| {
                egui::CentralPanel::default()
                    .show(ui, |ui| app.ui_designer_pane(ui, Some(render_state)));
            },
        );
    }
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, SIZE)),
        ..Default::default()
    };
    let _ = crate::theme::run(&ctx, input, |ui| {
        egui::CentralPanel::default().show(ui, |ui| app.ui_designer_pane(ui, None));
    });
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn a_rule_and_its_states_import_from_a_project_stylesheet_onto_several_elements() {
    let folder = copy_of_example("hello-touched", "ui-styles-import");
    std::fs::write(
        folder.join("looks.css"),
        ".fancy { background-color: #123456; border-radius: 6px; }\n\
         .fancy:hover { background-color: #654321; }\n\
         .menu .fancy { color: red; }\n",
    )
    .unwrap();
    let mut app = EditorApp::new(Some(&folder));
    let (gui, play) = play_button(&mut app);
    let quit = drop_in(&mut app, Insertable::TextButton, gui, [100.0, 200.0]);

    let sets = project_sets(&stylesheets(&app));
    let (path, fancy) = sets
        .iter()
        .find(|(_, set)| set.selector == ".fancy")
        .unwrap();
    assert_eq!(path, "looks.css");
    act(
        &mut app,
        DesignerAction::ApplyStyleSet {
            ids: vec![play, quit],
            name: fancy.selector.clone(),
            rules: fancy.rules.clone(),
        },
    );
    assert_eq!(app.history.undo_label(), Some("Style 2 elements as .fancy"));
    for id in [play, quit] {
        assert_eq!(
            own_value(&app, id, "", "background-color").as_deref(),
            Some("#123456")
        );
        assert_eq!(
            own_value(&app, id, ":hover", "background-color").as_deref(),
            Some("#654321")
        );
    }
    // Copied, not linked: the class rule isn't the element's.
    let class = app.world.get(play).unwrap().property("Class").cloned();
    assert_ne!(class, Some(Value::String("fancy".into())));
    app.undo();
    assert_eq!(own_value(&app, quit, "", "background-color"), None);
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn code_and_animation_links_write_handlers_find_mentions_and_animate() {
    use crate::ui_designer::links::{ScriptText, animations_of, lua_path, mentions};
    let folder = copy_of_example("hello-menu", "ui-links");
    let mut app = EditorApp::new(Some(&folder));
    let menu = app
        .world
        .find_first_child(app.world.workspace(), "Menu")
        .unwrap();
    let panel = app.world.find_first_child(menu, "Panel").unwrap();
    let play = app.world.find_first_child(panel, "Play").unwrap();
    assert_eq!(
        lua_path(&app.world, play).as_deref(),
        Some("workspace.Menu.Panel.Play")
    );
    let script = "scripts/menu.client.lua";
    let found = |app: &EditorApp, name: &str| {
        let texts = app.script_texts();
        let scripts: Vec<ScriptText> = texts
            .iter()
            .map(|(path, text)| ScriptText { path, text })
            .collect();
        mentions(&scripts, name)
    };
    // `for _, name in { "Play", "Options", "Quit" } do` names Play.
    assert_eq!(found(&app, "Play"), [(script.to_owned(), 8)]);

    // A handler goes into the script that already names workspace.Menu,
    // as an unsaved change, with the cursor in it.
    act(
        &mut app,
        DesignerAction::AddHandler {
            id: play,
            event: "Activated".into(),
            body: None,
        },
    );
    let tab = app.tabs.get(script).unwrap();
    assert!(tab.unsaved());
    assert!(
        tab.text
            .ends_with("\nworkspace.Menu.Panel.Play.Activated:Connect(function()\n\t\nend)\n")
    );
    let line = tab.go_to_line().unwrap();
    assert_eq!(tab.text.lines().nth(line - 1), Some("\t"));
    assert_eq!(found(&app, "Play").len(), 2);
    // Opening a mention puts the cursor on its line.
    act(
        &mut app,
        DesignerAction::OpenScript {
            path: script.into(),
            line: 8,
        },
    );
    assert_eq!(app.tabs.get(script).unwrap().go_to_line(), Some(8));

    // A ScreenGui no script names gets its own LocalScript.
    let (gui, start) = play_button(&mut app);
    act(
        &mut app,
        DesignerAction::Edit(start, Edit::Name("Start".into())),
    );
    let gui_name = app.world.get(gui).unwrap().name().to_owned();
    act(
        &mut app,
        DesignerAction::AddHandler {
            id: start,
            event: "MouseEnter".into(),
            body: None,
        },
    );
    let made = format!("scripts/{gui_name}.client.lua");
    let source = std::fs::read_to_string(folder.join(&made)).unwrap();
    assert!(source.contains(&format!(
        "workspace.{gui_name}.Start.MouseEnter:Connect(function()"
    )));
    assert!(app.tabs.get(&made).is_some());

    // Animate the button, then play that animation when it's clicked: in
    // the script made above, which now names the ScreenGui.
    act(&mut app, DesignerAction::Edit(start, Edit::Animate));
    let animations = animations_of(&app.world, start);
    assert_eq!(animations.len(), 1);
    let animation_path = lua_path(&app.world, animations[0]).unwrap();
    act(
        &mut app,
        DesignerAction::AddHandler {
            id: start,
            event: "Activated".into(),
            body: Some(format!("{animation_path}:Play()")),
        },
    );
    let text = &app.tabs.get(&made).unwrap().text;
    assert!(text.ends_with(&format!(
        "workspace.{gui_name}.Start.Activated:Connect(function()\n\t{animation_path}:Play()\nend)\n"
    )));
    let _ = std::fs::remove_dir_all(&folder);
}
