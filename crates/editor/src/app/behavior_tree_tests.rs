//! Behavior trees in the editor: building one in the Behavior Tree tab,
//! saving it, an agent running it in Play, and state machines staying
//! animation.

use axcenity_blocks::node_graph::{NodeGraph, PinRef, Wire};
use axcenity_core::{Value, Vector3};

use super::tests::{copy_of_example, inserted, play_for, printed, property, vector};
use super::*;
use crate::behavior_tree::TreeAction;

const SIZE: egui::Vec2 = egui::vec2(1200.0, 800.0);

fn feet(world: &World, body: InstanceId) -> Vector3 {
    axcenity_nav::feet_of(world, &[body]).unwrap()
}

fn across(a: Vector3, b: Vector3) -> f32 {
    (a.x - b.x).hypot(a.z - b.z)
}

/// Draws the Behavior Tree tab once, with `events`.
fn frame(app: &mut EditorApp, ctx: &egui::Context, events: Vec<egui::Event>, time: &mut f64) {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, SIZE)),
        events,
        time: Some(*time),
        ..Default::default()
    };
    *time += 1.0 / 30.0;
    let _ = ctx.run(input, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| app.behavior_tree_pane(ui));
    });
}

/// Drags a wire out of `pin` (an output) onto empty canvas at `offset`
/// from it, types `search` into the node menu that opens and picks the
/// first match, as a creator would.
fn add_from_wire(
    app: &mut EditorApp,
    ctx: &egui::Context,
    time: &mut f64,
    path: &str,
    pin: PinRef,
    offset: egui::Vec2,
    search: &str,
) {
    let from = app
        .behavior
        .view(path)
        .unwrap()
        .pin_on_screen(&pin, true)
        .expect("the pin is on screen");
    let to = from + offset;
    let press = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    frame(app, ctx, vec![egui::Event::PointerMoved(from)], time);
    frame(app, ctx, vec![press(from, true)], time);
    for step in 1..=4 {
        let at = from.lerp(to, step as f32 / 4.0);
        frame(app, ctx, vec![egui::Event::PointerMoved(at)], time);
    }
    frame(app, ctx, vec![press(to, false)], time);
    frame(app, ctx, Vec::new(), time);
    frame(app, ctx, vec![egui::Event::Text(search.to_owned())], time);
    let enter = egui::Event::Key {
        key: egui::Key::Enter,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Default::default(),
    };
    frame(app, ctx, vec![enter], time);
    frame(app, ctx, Vec::new(), time);
}

fn graph(app: &EditorApp, path: &str) -> NodeGraph {
    app.behavior.document(path).unwrap().graph.clone()
}

fn node_of(graph: &NodeGraph, kind: &str) -> u32 {
    graph
        .nodes
        .iter()
        .find(|node| node.kind == kind)
        .unwrap_or_else(|| panic!("no {kind} in {graph:?}"))
        .id
}

#[test]
fn a_tree_built_in_the_tab_saves_and_its_agent_chases_the_player_it_sees_in_play() {
    let folder = copy_of_example("hello-perception", "behavior-build");
    let mut app = EditorApp::new(Some(&folder));
    let guard = app
        .world
        .find_first_child(app.world.workspace(), "Guard")
        .unwrap();
    let agent = app
        .world
        .find_first_child(guard, "NavigationAgent")
        .unwrap();
    let body = app.world.find_first_child(guard, "Body").unwrap();
    let character = app
        .world
        .find_first_child(app.world.workspace(), "Character")
        .unwrap();
    let player = app.world.find_first_child(character, "Body").unwrap();

    // Select the Guard and press New tree: a tree named after it, which
    // it runs.
    app.select_clicked(Some(guard), egui::Modifiers::NONE, None, false);
    let ctx = theme::test_context();
    let mut time = 0.0;
    frame(&mut app, &ctx, Vec::new(), &mut time);
    let selected = app.selected_agent();
    app.behavior_tree_action(TreeAction::New, selected.as_ref());
    let path = "assets/ai/Guard.behaviortree.json";
    assert_eq!(app.behavior.current(), Some(path));
    assert_eq!(
        property(&app, agent, "BehaviorTree"),
        Value::String(path.into())
    );
    assert!(app.layout.is_open(Pane::BehaviorTree));

    // Root → Sequence, then Sees Player? under 1 and Chase Player under 2,
    // each dropped off a wire and picked from the node menu.
    frame(&mut app, &ctx, Vec::new(), &mut time);
    frame(&mut app, &ctx, Vec::new(), &mut time);
    let view = app.behavior.view_mut(path).unwrap();
    view.zoom = 1.0;
    view.pan = egui::vec2(40.0, 40.0);
    frame(&mut app, &ctx, Vec::new(), &mut time);
    let root = node_of(&graph(&app, path), "root");
    let right = egui::vec2(220.0, 0.0);
    add_from_wire(
        &mut app,
        &ctx,
        &mut time,
        path,
        PinRef::new(root, ""),
        right,
        "sequence",
    );
    let sequence = node_of(&graph(&app, path), "sequence");
    add_from_wire(
        &mut app,
        &ctx,
        &mut time,
        path,
        PinRef::new(sequence, "1"),
        egui::vec2(230.0, -70.0),
        "sees",
    );
    add_from_wire(
        &mut app,
        &ctx,
        &mut time,
        path,
        PinRef::new(sequence, "2"),
        egui::vec2(230.0, 60.0),
        "chase",
    );
    let built = graph(&app, path);
    let (sees, chase) = (
        node_of(&built, "sees_player"),
        node_of(&built, "chase_player"),
    );
    let wire = |from, pin: &str, to| Wire {
        from: PinRef::new(from, pin),
        to: PinRef::new(to, ""),
    };
    for expected in [
        wire(root, "", sequence),
        wire(sequence, "1", sees),
        wire(sequence, "2", chase),
    ] {
        assert!(built.wires.contains(&expected), "{:?}", built.wires);
    }
    assert!(crate::behavior_tree::check(&built).is_ok());
    assert!(app.unsaved());

    // Each node is a step of the tree's own undo.
    assert!(app.behavior.undo());
    assert!(
        !graph(&app, path)
            .nodes
            .iter()
            .any(|node| node.kind == "chase_player")
    );
    assert!(app.behavior.redo());
    assert_eq!(graph(&app, path), built);

    // Saved with the project, and the same after opening it again, where
    // selecting the Guard shows its tree.
    assert!(app.save());
    assert!(folder.join(path).is_file());
    assert!(!app.unsaved());
    let mut app = EditorApp::new(Some(&folder));
    app.select_clicked(Some(guard), egui::Modifiers::NONE, None, false);
    frame(&mut app, &ctx, Vec::new(), &mut time);
    assert_eq!(app.behavior.current(), Some(path));
    assert_eq!(graph(&app, path), built);

    // In Play: hidden behind the wall, it stays put. Step out to the side
    // and it sees the player, and chases them.
    app.play();
    play_for(&mut app, 0.5);
    let start = feet(shown(&app.world, &app.play), body);
    assert!(
        across(start, Vector3::new(0.0, 0.0, 0.0)) < 0.01,
        "{start:?}"
    );
    app.apply(player, Edit::Property("Position", vector(-15.0, 2.5, 20.0)));
    play_for(&mut app, 3.0);
    let game = shown(&app.world, &app.play);
    assert_eq!(
        game.get(agent).unwrap().property("SeesPlayer"),
        Some(&Value::Bool(true))
    );
    let gap = across(feet(game, body), feet(game, player));
    assert!(gap <= 2.6, "the Guard is {gap} studs away");
    assert!(
        !printed(&app)
            .iter()
            .any(|line| line.contains("behavior tree")),
        "{:?}",
        printed(&app)
    );
    app.stop();
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn hello_behavior_patrols_by_its_tree_and_the_courier_walks_where_its_script_sends_it() {
    let folder = copy_of_example("hello-behavior", "behavior-example");
    let mut app = EditorApp::new(Some(&folder));
    let find = |app: &EditorApp, model: &str| {
        let model = app
            .world
            .find_first_child(app.world.workspace(), model)
            .unwrap();
        app.world.find_first_child(model, "Body").unwrap()
    };
    let (guard, courier) = (find(&app, "Guard"), find(&app, "Courier"));

    // Selecting the Guard shows its tree, with no problems.
    app.select_clicked(Some(guard), egui::Modifiers::NONE, None, false);
    let ctx = theme::test_context();
    let mut time = 0.0;
    frame(&mut app, &ctx, Vec::new(), &mut time);
    let path = "assets/ai/Guard.behaviortree.json";
    assert_eq!(app.behavior.current(), Some(path));
    assert!(crate::behavior_tree::check(&graph(&app, path)).is_ok());
    if let Some(snapshots) = std::env::var_os("AXCENITY_SNAPSHOTS") {
        let file = std::path::PathBuf::from(snapshots).join("behavior-tree-guard.png");
        crate::snapshot::snapshot(
            &file,
            SIZE,
            vec![Vec::new(); 3],
            &mut |ctx: &egui::Context| {
                egui::CentralPanel::default().show(ctx, |ui| app.behavior_tree_pane(ui));
            },
        );
    }
    app.play();
    let (mut east, mut courier_there) = (false, false);
    for _ in 0..9 * 60 {
        play_for(&mut app, 1.0 / 60.0);
        let game = shown(&app.world, &app.play);
        east |= across(feet(game, guard), Vector3::new(15.0, 0.0, -15.0)) < 0.5;
        courier_there |= across(feet(game, courier), Vector3::new(30.0, 0.0, 30.0)) < 0.5;
    }
    assert!(east, "the Guard's tree walks it along its patrol");
    assert!(
        courier_there,
        "the Courier, with no tree, walks where its script says"
    );
    assert!(
        !printed(&app)
            .iter()
            .any(|line| line.contains("behavior tree")),
        "{:?}",
        printed(&app)
    );
    app.stop();
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn a_state_machine_still_opens_as_animation_and_as_an_agents_tree_runs_no_ai() {
    let folder = copy_of_example("hello-character", "behavior-state-machine");
    let mut app = EditorApp::new(Some(&folder));
    let machine = "assets/animations/Walking/Walking.statemachine.json";
    let animator = app
        .world
        .descendants(app.world.workspace())
        .into_iter()
        .find(|&id| {
            app.world.get(id).is_some_and(|instance| {
                instance.class() == ClassName::Animator
                    && instance.property("StateMachine") == Some(&Value::String(machine.into()))
            })
        })
        .unwrap();

    // Opening it from its Animator edits its states in the Animation tab.
    app.apply(animator, Edit::OpenController);
    assert!(app.layout.is_open(Pane::Animation));
    let doc = app.animation.machine_doc();
    assert_eq!(doc.path, machine);
    assert!(!doc.file.states.is_empty());
    assert!(!app.layout.is_open(Pane::BehaviorTree));

    // It isn't offered as a tree, and won't open as one.
    assert!(behavior_trees::tree_files(&folder).is_empty());
    app.open_behavior_tree(machine);
    assert!(!app.layout.is_open(Pane::BehaviorTree));
    assert!(
        printed(&app)
            .last()
            .unwrap()
            .contains("isn't a behavior tree")
    );

    // Named as an agent's BehaviorTree, Play doesn't run it as AI: the
    // agent says why once and stays where it is.
    let agent = inserted(&mut app, Insertable::NavigationAgent, None);
    let model = app.world.get(agent).unwrap().parent().unwrap();
    let body = app.world.find_first_child(model, "Body").unwrap();
    app.apply(
        agent,
        Edit::Property("BehaviorTree", Value::String(machine.into())),
    );
    let destination = property(&app, agent, "Destination");
    app.play();
    play_for(&mut app, 1.0);
    let game = shown(&app.world, &app.play);
    assert_eq!(
        game.get(agent).unwrap().property("Destination"),
        Some(&destination)
    );
    assert!(across(feet(game, body), destination.as_vector3().unwrap()) < 0.01);
    let warnings: Vec<&str> = printed(&app)
        .into_iter()
        .filter(|line| line.contains("doesn't run the behavior tree"))
        .collect();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].contains("it's a state machine"), "{warnings:?}");
    app.stop();

    // And the state machine file is as it was.
    let text = std::fs::read_to_string(folder.join(machine)).unwrap();
    assert!(axcenity_anim::StateMachineFile::from_json(&text).is_ok());
    let _ = std::fs::remove_dir_all(&folder);
}
