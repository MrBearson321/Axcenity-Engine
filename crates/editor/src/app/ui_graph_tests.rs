//! UI Event Graphs in the editor: the UI Designer's Event Graph tab, its
//! Live Preview, Play, and saving a graph, which makes its LocalScript.

use std::path::PathBuf;

use axcenity_blocks::node_graph::{NodeGraph, PinRef};
use axcenity_blocks::ui_graph::{UI_NODES, UiNodes};

use super::tests::copy_of_example;
use super::ui_designer_tests::{act, drop_in};
use super::*;
use crate::ui_designer::DesignerTab;

const SIZE: egui::Vec2 = egui::vec2(1400.0, 900.0);

fn spec(kind: &str) -> &'static axcenity_blocks::node_graph::NodeSpec {
    UI_NODES.iter().find(|spec| spec.kind == kind).unwrap()
}

/// Draws the UI Designer tab for a frame, with `events`.
fn draw_designer(app: &mut EditorApp, ctx: &egui::Context, events: Vec<egui::Event>) {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, SIZE)),
        events,
        ..Default::default()
    };
    let _ = ctx.run(input, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| app.ui_designer_pane(ui, None));
    });
}

/// Clicks at `at` on the UI Designer tab.
fn click_designer(app: &mut EditorApp, ctx: &egui::Context, at: egui::Pos2) {
    let press = |pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    draw_designer(app, ctx, vec![egui::Event::PointerMoved(at)]);
    draw_designer(app, ctx, vec![press(true)]);
    draw_designer(app, ctx, vec![press(false)]);
    draw_designer(app, ctx, Vec::new());
}

fn child(world: &World, parent: InstanceId, name: &str) -> InstanceId {
    world.find_first_child(parent, name).unwrap()
}

fn current_page(world: &World, pages: InstanceId) -> String {
    world
        .get(pages)
        .and_then(|instance| instance.property("CurrentPage"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

/// The hello-ui-graph example open in the editor, and its Menu, Pages and
/// Settings button.
fn hello_ui_graph(test: &str) -> (PathBuf, EditorApp, InstanceId, InstanceId, InstanceId) {
    let folder = copy_of_example("hello-ui-graph", test);
    let app = EditorApp::new(Some(&folder));
    let menu = child(&app.world, app.world.workspace(), "Menu");
    let pages = child(&app.world, menu, "Pages");
    let settings = child(
        &app.world,
        child(&app.world, pages, "Home"),
        "SettingsButton",
    );
    (folder, app, menu, pages, settings)
}

fn errors_in_output(app: &EditorApp) -> Vec<String> {
    app.output
        .lines()
        .iter()
        .filter(|line| line.kind == crate::output::OutputKind::Error)
        .map(|line| line.text.clone())
        .collect()
}

#[test]
fn button_activated_to_show_page_settings_works_in_live_preview_and_in_play() {
    let (folder, mut app, _, pages, settings) = hello_ui_graph("ui-graph-live");
    let ctx = theme::test_context();
    app.ui_designer.toggle_live_preview();
    draw_designer(&mut app, &ctx, Vec::new());
    draw_designer(&mut app, &ctx, Vec::new());
    assert!(
        app.ui_designer.live().is_some(),
        "{:?}",
        errors_in_output(&app)
    );

    // The Settings button is at 540, 420 to 740, 476 in the 1280 × 720
    // design: click its middle on the artboard.
    let (board, zoom) = app.ui_designer.board().expect("the artboard is drawn");
    let at = board.min + egui::vec2(640.0, 448.0) * zoom;
    click_designer(&mut app, &ctx, at);
    let live = app.ui_designer.live().unwrap();
    assert_eq!(current_page(&live.world, pages), "Settings");
    // A sandbox: the world being edited is as it was.
    assert_eq!(current_page(&app.world, pages), "");
    assert!(!app.unsaved());
    assert!(
        errors_in_output(&app).is_empty(),
        "{:?}",
        errors_in_output(&app)
    );

    // Stopping it shows the world being edited again.
    app.ui_designer.toggle_live_preview();
    draw_designer(&mut app, &ctx, Vec::new());
    assert!(app.ui_designer.live().is_none());

    // The same graph in Play.
    app.play();
    let session = app.play.as_mut().expect("playing");
    session
        .activate_ui_button(settings, &mut app.output)
        .unwrap();
    assert_eq!(current_page(session.world(), pages), "Settings");
    assert!(
        errors_in_output(&app).is_empty(),
        "{:?}",
        errors_in_output(&app)
    );
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn a_graph_with_an_unconnected_required_pin_reports_its_nodes_not_lua_errors() {
    let (folder, mut app, menu, pages, settings) = hello_ui_graph("ui-graph-errors");
    let index = app.graph_document(menu).unwrap();
    let graph = &mut app.project.as_mut().unwrap().ui_graphs[index].graph;
    let set = graph.add_node(spec("set_property"), [880.0, 220.0]);
    graph
        .connect(&UiNodes, PinRef::new(5, ""), PinRef::new(set, ""))
        .unwrap();

    // The Event Graph tab shows it (drawn here for a look), and the Live
    // Preview won't start.
    let ctx = theme::test_context();
    app.ui_designer.set_tab(DesignerTab::EventGraph);
    draw_designer(&mut app, &ctx, Vec::new());
    draw_designer(&mut app, &ctx, Vec::new());
    if let Some(snapshots) = std::env::var_os("AXCENITY_SNAPSHOTS").map(PathBuf::from) {
        crate::snapshot::snapshot(
            &snapshots.join("ui-event-graph-errors.png"),
            SIZE,
            vec![Vec::new(); 3],
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| app.ui_designer_pane(ui, None));
            },
        );
    }
    app.ui_designer.toggle_live_preview();
    draw_designer(&mut app, &ctx, Vec::new());
    assert!(app.ui_designer.live().is_none());
    assert_eq!(app.ui_designer.tab(), DesignerTab::EventGraph);
    assert_eq!(
        errors_in_output(&app),
        ["Live Preview: it didn't start: the Event Graph has 1 error, shown on its nodes."]
    );

    // Saved, it says it won't run, and why, by node.
    assert!(app.save());
    let saved = errors_in_output(&app);
    let last = saved.last().unwrap();
    assert!(
        last.starts_with("Saved `ui/Menu.uigraph.json`, but its Event Graph won't run")
            && last.contains(&format!("node {set}: Value isn't connected")),
        "{last}"
    );

    // In Play it doesn't run at all, so there's no Lua error.
    app.play();
    let play_errors = errors_in_output(&app);
    let new = &play_errors[saved.len()..];
    assert_eq!(new.len(), 1, "{new:?}");
    assert!(
        new[0].starts_with("ui/Menu.uigraph.json won't run: its Event Graph has problems"),
        "{new:?}"
    );
    let session = app.play.as_mut().unwrap();
    session
        .activate_ui_button(settings, &mut app.output)
        .unwrap();
    assert_eq!(current_page(session.world(), pages), "");
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn saving_writes_the_graph_and_makes_its_read_only_local_script_again() {
    let (folder, mut app, menu, _, _) = hello_ui_graph("ui-graph-save");
    let script = child(&app.world, menu, "EventGraph");
    let index = app.graph_document(menu).unwrap();
    {
        let doc = &mut app.project.as_mut().unwrap().ui_graphs[index];
        doc.graph
            .node_mut(2)
            .unwrap()
            .values
            .insert("Page".to_owned(), "Home".to_owned());
    }
    assert!(app.unsaved());
    assert!(app.save());
    assert!(!app.unsaved());
    let text = std::fs::read_to_string(folder.join("ui/Menu.uigraph.json")).unwrap();
    assert!(text.contains("\"Page\": \"Home\""), "{text}");
    let file = app
        .project
        .as_ref()
        .unwrap()
        .scripts
        .iter()
        .find(|file| file.path == "ui/Menu.uigraph.json")
        .unwrap();
    assert_eq!(file.instance, script, "the same LocalScript");
    assert!(
        file.source.contains("gui.Pages.CurrentPage = \"Home\""),
        "{}",
        file.source
    );

    // It can't be renamed, deleted or opened as code: its graph shows.
    app.ui_designer.set_tab(DesignerTab::Design);
    app.act(Action::Rename(script, "Other".to_owned()));
    assert_eq!(app.world.get(script).unwrap().name(), "EventGraph");
    assert_eq!(app.ui_designer.tab(), DesignerTab::EventGraph);
    app.act(Action::Delete(script));
    assert!(app.confirm_delete.is_none() && app.world.contains(script));
    app.open_script("ui/Menu.uigraph.json", Some(3));
    assert!(
        app.tabs.get("ui/Menu.uigraph.json").is_none(),
        "no code tab"
    );

    // Opened again, the project has the saved graph.
    let project = axcenity_project::load(&folder).unwrap();
    assert_eq!(project.ui_graphs.len(), 1);
    assert_eq!(
        project.ui_graphs[0].graph.node(2).unwrap().value("Page"),
        "Home"
    );
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn a_new_screen_guis_first_graph_saves_with_a_new_local_script_that_plays() {
    let folder = copy_of_example("hello-touched", "ui-graph-new");
    let mut app = EditorApp::new(Some(&folder));
    act(&mut app, DesignerAction::NewScreenGui);
    let gui = app.selected.unwrap();
    let name = app.world.get(gui).unwrap().name().to_owned();
    let button = drop_in(&mut app, Insertable::TextButton, gui, [10.0, 10.0]);
    let button_name = app.world.get(button).unwrap().name().to_owned();
    let label = drop_in(&mut app, Insertable::TextLabel, gui, [10.0, 80.0]);
    let label_name = app.world.get(label).unwrap().name().to_owned();

    let index = app.graph_document(gui).unwrap();
    {
        let graph = &mut app.project.as_mut().unwrap().ui_graphs[index].graph;
        let clicked = graph.add_node(spec("button_activated"), [0.0, 0.0]);
        graph
            .node_mut(clicked)
            .unwrap()
            .values
            .insert("Button".into(), button_name);
        let set = graph.add_node(spec("set_property"), [300.0, 0.0]);
        let values = &mut graph.node_mut(set).unwrap().values;
        values.insert("Object".into(), label_name);
        values.insert("Value".into(), "Clicked".into());
        graph
            .connect(&UiNodes, PinRef::new(clicked, ""), PinRef::new(set, ""))
            .unwrap();
    }
    assert!(app.save());
    let path = format!("ui/{name}.uigraph.json");
    assert!(folder.join(&path).is_file());
    let script = child(&app.world, gui, "EventGraph");
    assert_eq!(
        app.world.get(script).unwrap().class(),
        ClassName::LocalScript
    );

    app.play();
    let session = app.play.as_mut().unwrap();
    session.activate_ui_button(button, &mut app.output).unwrap();
    assert_eq!(
        session.world().get(label).unwrap().property("Text"),
        Some(&Value::String("Clicked".into()))
    );
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn ctrl_z_over_the_event_graph_undoes_the_graph_not_the_world() {
    let (folder, mut app, menu, _, _) = hello_ui_graph("ui-graph-undo");
    let ctx = theme::test_context();
    app.ui_designer.set_tab(DesignerTab::EventGraph);
    draw_designer(
        &mut app,
        &ctx,
        vec![egui::Event::PointerMoved(egui::pos2(300.0, 400.0))],
    );
    draw_designer(&mut app, &ctx, Vec::new());
    assert!(app.ui_designer.graph_has_pointer());

    // An edit in the graph, as the tab reports it.
    let index = app.graph_document(menu).unwrap();
    let graph = |app: &EditorApp| -> NodeGraph {
        app.project.as_ref().unwrap().ui_graphs[index].graph.clone()
    };
    let before = graph(&app);
    app.project.as_mut().unwrap().ui_graphs[index]
        .graph
        .add_node(spec("print"), [0.0, 500.0]);
    app.designer_actions(vec![DesignerAction::GraphStep(before.clone())]);
    let after = graph(&app);
    let world_step = app.history.undo_label().map(str::to_owned);

    app.undo();
    assert_eq!(graph(&app), before);
    app.redo();
    assert_eq!(graph(&app), after);
    assert_eq!(
        app.history.undo_label().map(str::to_owned),
        world_step,
        "the world's undo is untouched"
    );
    let _ = std::fs::remove_dir_all(&folder);
}
