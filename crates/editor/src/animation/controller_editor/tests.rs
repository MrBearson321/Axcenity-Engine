use super::*;

fn doc() -> ControllerDoc {
    let file = StateMachineFile {
        default_state: "Idle".into(),
        states: vec![
            StateDef::new("Idle", Motion::Clip(String::new()), [0.0, 0.0]),
            StateDef::new("Walk", Motion::Clip(String::new()), [200.0, 0.0]),
        ],
        ..StateMachineFile::default()
    };
    ControllerDoc::open(
        "assets/animations/hero.statemachine.json".into(),
        file,
        None,
    )
}

#[test]
fn a_return_setting_keeps_its_transition_in_step() {
    let mut doc = doc();
    let nowhere = std::path::Path::new("no such project");
    doc.file.states[1].returns = Some(ReturnDef {
        to: "Idle".into(),
        when: ReturnWhen::Finished,
        blend: 0.1,
    });
    doc.sync_returns(nowhere);
    let made = &doc.file.transitions[0];
    assert!(made.generated);
    assert_eq!((made.from.as_str(), made.to.as_str()), ("Walk", "Idle"));
    // Renaming the state it returns to keeps it pointing there.
    doc.rename_state(0, "Rest").unwrap();
    doc.sync_returns(nowhere);
    assert_eq!(doc.file.states[1].returns.as_ref().unwrap().to, "Rest");
    assert_eq!(doc.file.transitions[0].to, "Rest");
    // Removing the made transition turns the setting off.
    doc.delete(Selected::Transition(0));
    doc.sync_returns(nowhere);
    assert!(doc.file.states[1].returns.is_none());
    assert!(doc.file.transitions.is_empty());
}

#[test]
fn linking_states_makes_a_transition_and_selects_it() {
    let mut doc = doc();
    doc.apply(vec![GraphEvent::Link(Node::State(0), 1)]);
    assert_eq!(doc.file.transitions[0].from, "Idle");
    assert_eq!(doc.file.transitions[0].to, "Walk");
    assert_eq!(doc.selected, Selected::Transition(0));
    doc.apply(vec![GraphEvent::Link(Node::Any, 0)]);
    assert_eq!(doc.file.transitions[1].from, ANY_STATE);
}

#[test]
fn renaming_and_deleting_states_keep_transitions_right() {
    let mut doc = doc();
    doc.apply(vec![
        GraphEvent::Link(Node::State(0), 1),
        GraphEvent::Link(Node::State(1), 0),
    ]);
    doc.rename_state(0, "Rest").unwrap();
    assert_eq!(doc.file.default_state, "Rest");
    assert_eq!(doc.file.transitions[0].from, "Rest");
    assert_eq!(doc.file.transitions[1].to, "Rest");
    assert!(doc.rename_state(0, "Walk").is_err());
    assert!(doc.rename_state(0, ANY_STATE).is_err());
    doc.delete(Selected::Node(Node::State(1)));
    assert!(doc.file.transitions.is_empty());
    assert_eq!(doc.file.states.len(), 1);
}

#[test]
fn added_states_get_free_names_and_moving_doesnt_change_behaviour() {
    let mut doc = doc();
    doc.apply(vec![
        GraphEvent::AddState([10.0, 20.0], None, None),
        GraphEvent::AddState(
            [10.0, 20.0],
            Some("assets/animations/hero/Walk.anim.json".into()),
            None,
        ),
    ]);
    assert_eq!(doc.file.states[2].name, "New State");
    assert_eq!(doc.file.states[3].name, "Walk 2");
    let before = behaviour(&doc.file);
    doc.apply(vec![GraphEvent::Move(Node::State(0), [99.0, 99.0])]);
    assert_eq!(behaviour(&doc.file), before);
    assert_eq!(doc.file.states[0].position, [99.0, 99.0]);
}

use crate::animation::graph::ENTRY_POSITION;
use crate::animation::graph::words::when_words;
use crate::animation::testing::{click_frames, drag_frames};
use crate::theme;

const SIZE: egui::Vec2 = egui::vec2(1000.0, 600.0);
const IDLE: &str = "assets/animations/hero/Idle.anim.json";
const WALK: &str = "assets/animations/hero/Walk.anim.json";

/// A state machine's graph drawn in a window, frame by frame, as the
/// Animation tab draws it.
struct Harness {
    ctx: egui::Context,
    doc: ControllerDoc,
    clips: Vec<String>,
}

impl Harness {
    fn new(doc: ControllerDoc) -> Self {
        let mut harness = Self {
            ctx: theme::test_context(),
            doc,
            clips: vec![IDLE.into(), WALK.into()],
        };
        harness.frames(vec![Vec::new(); 2]);
        harness
    }

    /// With a Speed parameter, as most state machines have.
    fn with_speed(mut doc: ControllerDoc) -> Self {
        doc.file.parameters.push(ParameterDef::Number {
            name: "Speed".into(),
            default: 0.0,
        });
        Self::new(doc)
    }

    fn frame(&mut self, events: Vec<egui::Event>, during: impl FnOnce(&egui::Context)) {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, SIZE)),
            events,
            ..Default::default()
        };
        let mut during = Some(during);
        let _ = crate::theme::run(&self.ctx, input, |ui| {
            let ctx = &ui.ctx().clone();
            if let Some(during) = during.take() {
                during(ctx);
            }
            egui::CentralPanel::default().show(ui, |ui| {
                self.doc.graph_ui(ui, &self.clips);
            });
        });
    }

    fn frames(&mut self, frames: Vec<Vec<egui::Event>>) {
        for events in frames {
            self.frame(events, |_| {});
        }
    }

    fn pin(&self, node: Node) -> egui::Pos2 {
        let view = &self.doc.view;
        view.pin(view.rect, &self.doc.file, node, true)
    }

    fn center(&self, node: Node) -> egui::Pos2 {
        let view = &self.doc.view;
        view.node_rect(view.rect, &self.doc.file, node).center()
    }

    fn label(&self, transition: usize) -> egui::Pos2 {
        let (_, rect) = self
            .doc
            .view
            .labels
            .iter()
            .find(|(index, _)| *index == transition)
            .expect("the wire has a label");
        rect.center()
    }

    fn chip(&self, name: &str) -> egui::Pos2 {
        let (_, rect) = self
            .doc
            .view
            .chips
            .iter()
            .find(|(chip, _)| chip == name)
            .expect("the parameter has a chip");
        rect.center()
    }
}

#[test]
fn a_wire_from_idles_pin_to_walk_with_the_speed_chip_on_it_says_speed_over_0_1() {
    let mut graph = Harness::with_speed(doc());
    let (from, to) = (graph.pin(Node::State(0)), graph.center(Node::State(1)));
    graph.frames(drag_frames(from, to));
    let file = &graph.doc.file;
    assert_eq!(file.transitions.len(), 1, "{:?}", file.transitions);
    assert_eq!(
        (
            file.transitions[0].from.as_str(),
            file.transitions[0].to.as_str()
        ),
        ("Idle", "Walk")
    );
    assert_eq!(when_words(&file.transitions[0]), "never: say when");

    // The Speed chip dropped on the wire tests Speed there.
    let (chip, label) = (graph.chip("Speed"), graph.label(0));
    graph.frames(drag_frames(chip, label));
    assert_eq!(when_words(&graph.doc.file.transitions[0]), "Speed > 0.1");

    // Clicking the label opens its editor; clicking the background shuts it.
    let label = graph.label(0);
    graph.frames(click_frames(label));
    assert_eq!(graph.doc.view.editing, Some(0));
    assert_eq!(graph.doc.selected, Selected::Transition(0));
    let empty = graph.doc.view.rect.right_bottom() - egui::vec2(20.0, 20.0);
    graph.frames(click_frames(empty));
    assert_eq!(graph.doc.view.editing, None);
}

#[test]
fn rewiring_entry_to_another_state_makes_it_the_default() {
    let mut graph = Harness::new(doc());
    assert_eq!(graph.doc.file.default_state, "Idle");
    let (from, to) = (graph.pin(Node::Entry), graph.center(Node::State(1)));
    graph.frames(drag_frames(from, to));
    assert_eq!(graph.doc.file.default_state, "Walk");
    assert!(
        graph.doc.file.transitions.is_empty(),
        "Entry's wire isn't a transition"
    );
    // Moving Entry is kept in the file, and plays the same.
    let before = behaviour(&graph.doc.file);
    let entry = graph.center(Node::Entry) - egui::vec2(30.0, 0.0);
    graph.frames(drag_frames(entry, entry + egui::vec2(0.0, 60.0)));
    let moved = graph.doc.file.entry_position.expect("Entry was moved");
    assert!(
        (moved[1] - (ENTRY_POSITION[1] + 60.0)).abs() < 1.0,
        "{moved:?}"
    );
    assert_eq!(behaviour(&graph.doc.file), before);
}

#[test]
fn a_wire_let_go_on_empty_space_makes_a_state_wired_from_there() {
    let mut graph = Harness::new(doc());
    let from = graph.pin(Node::State(0));
    graph.frames(drag_frames(from, from + egui::vec2(60.0, 180.0)));
    assert!(graph.doc.view.drop_menu_open());
    graph.frames(vec![
        vec![egui::Event::Text("walk".into())],
        vec![enter(true)],
        vec![enter(false)],
        Vec::new(),
    ]);
    assert!(!graph.doc.view.drop_menu_open());
    let file = &graph.doc.file;
    assert_eq!(file.states.len(), 3);
    assert_eq!(file.states[2].name, "Walk 2");
    assert_eq!(file.states[2].motion, Motion::Clip(WALK.into()));
    assert_eq!(
        (
            file.transitions[0].from.as_str(),
            file.transitions[0].to.as_str()
        ),
        ("Idle", "Walk 2")
    );
}

#[test]
fn a_clip_dropped_on_the_graph_makes_a_state_playing_it() {
    let mut graph = Harness::new(doc());
    let at = graph.doc.view.rect.center() + egui::vec2(0.0, 150.0);
    let payload = || {
        crate::asset_browser::AssetPayload(crate::asset_browser::AssetEntry {
            kind: crate::asset_browser::AssetKind::Animation,
            path: WALK.into(),
            name: "Walk".into(),
        })
    };
    let release = egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: Default::default(),
    };
    graph.frame(vec![egui::Event::PointerMoved(at)], |ctx| {
        egui::DragAndDrop::set_payload(ctx, payload());
    });
    graph.frame(vec![release], |ctx| {
        egui::DragAndDrop::set_payload(ctx, payload());
    });
    graph.frames(vec![Vec::new()]);
    let file = &graph.doc.file;
    assert_eq!(file.states.len(), 3);
    assert_eq!(file.states[2].motion, Motion::Clip(WALK.into()));
    let drawn = graph.center(Node::State(2));
    assert!(drawn.distance(at) < 1.0, "{drawn:?} vs {at:?}");
}

#[test]
fn a_state_machine_saved_before_the_node_graph_opens_the_same() {
    let example =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-animation");
    let path = "assets/animations/noodle/noodle.statemachine.json";
    let text = std::fs::read_to_string(example.join(path))
        .unwrap()
        .replace("\r\n", "\n");
    assert!(!text.contains("entry_position"));
    let saved = StateMachineFile::from_json(&text).unwrap();
    let mut doc = ControllerDoc::open(path.into(), saved.clone(), None);
    doc.sync_returns(&example);
    let mut graph = Harness::new(doc);
    graph.frames(vec![Vec::new(); 3]);
    assert!(
        !graph.doc.unwritten(),
        "opening and drawing it changed nothing"
    );
    let file = &graph.doc.file;
    assert_eq!(file, &saved);
    assert_eq!(file.to_json().trim_end(), text.trim_end());
    // Each state is drawn where it was.
    let view = &graph.doc.view;
    for (index, state) in file.states.iter().enumerate() {
        let at = view.rect.min
            + view.offset
            + egui::vec2(state.position[0], state.position[1]) * view.zoom;
        let drawn = view.node_rect(view.rect, file, Node::State(index)).center();
        assert!(drawn.distance(at) < 0.01, "{}", state.name);
    }
}

#[test]
fn new_parameters_need_a_free_name_and_made_transitions_take_no_conditions() {
    let mut doc = doc();
    assert_eq!(
        doc.new_parameter(ParameterKind::Bool, "  Crouch "),
        Ok("Crouch".into())
    );
    assert!(doc.new_parameter(ParameterKind::Number, "Crouch").is_err());
    assert!(doc.new_parameter(ParameterKind::Number, " ").is_err());
    assert_eq!(doc.add_parameter(ParameterKind::Trigger), "Jump");
    assert_eq!(doc.add_parameter(ParameterKind::Trigger), "Jump2");
    doc.apply(vec![GraphEvent::Link(Node::State(0), 1)]);
    doc.add_condition(0, "Crouch");
    doc.add_condition(0, "Jump");
    assert_eq!(when_words(&doc.file.transitions[0]), "Crouch and Jump");
    // A Return setting's transition is changed by its setting only.
    doc.file.states[1].returns = Some(ReturnDef {
        to: "Idle".into(),
        when: ReturnWhen::Finished,
        blend: 0.1,
    });
    doc.sync_returns(std::path::Path::new("no such project"));
    doc.add_condition(1, "Crouch");
    assert!(doc.file.transitions[1].conditions.is_empty());
    // A new wire goes before it, so indices stay put.
    doc.apply(vec![GraphEvent::Link(Node::Any, 1)]);
    assert_eq!(doc.selected, Selected::Transition(1));
    assert_eq!(doc.file.transitions[1].from, ANY_STATE);
    assert!(doc.file.transitions[2].generated);
}

fn enter(pressed: bool) -> egui::Event {
    egui::Event::Key {
        key: egui::Key::Enter,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers: Default::default(),
    }
}

#[test]
fn a_layer_opens_as_its_own_state_machine_and_saves_back_into_the_file() {
    let project = std::env::temp_dir().join(format!("axcenity-layers-doc-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&project);
    std::fs::create_dir_all(project.join("assets/animations")).unwrap();
    let mut doc = doc();
    assert_eq!(doc.open_layer_index(), None);
    let index = doc.add_layer();
    assert_eq!(doc.open_layer_index(), Some(index));
    assert_eq!(doc.open_layer().unwrap().name, "Upper Body");
    // The graph shows the layer's states: an empty one to start in.
    assert_eq!(doc.file.states.len(), 1);
    assert_eq!(doc.file.default_state, "Empty");
    doc.apply(vec![GraphEvent::AddState(
        [200.0, 0.0],
        Some("assets/animations/hero/Reload.anim.json".into()),
        None,
    )]);
    doc.apply(vec![GraphEvent::Link(Node::Any, 1)]);
    let trigger = doc.add_parameter(ParameterKind::Trigger);
    doc.mask_bone("Hips/Spine");
    doc.rename_layer("Arms").unwrap();
    assert!(doc.unwritten());
    doc.write(&project).unwrap();
    assert!(!doc.unwritten());
    let text =
        std::fs::read_to_string(project.join("assets/animations/hero.statemachine.json")).unwrap();
    let saved = StateMachineFile::from_json(&text).unwrap();
    // The base layer is as it was; the layer has its states, transition
    // and mask; the parameter added in the layer is the file's.
    assert_eq!(saved.states.len(), 2);
    assert!(saved.parameters.iter().any(|p| p.name() == trigger));
    let layer = saved.layer("Arms").unwrap();
    assert_eq!(layer.states[1].name, "Reload");
    assert_eq!(layer.transitions[0].to, "Reload");
    assert_eq!(layer.mask, ["Hips/Spine"]);
    // Back on the base layer, the graph shows its states again.
    doc.show_layer(None);
    assert_eq!(doc.file.states[1].name, "Walk");
    assert_eq!(doc.file.layers.len(), 1);
    assert!(!doc.unwritten());
    doc.remove_layer(0);
    assert!(doc.file.layers.is_empty());
    assert!(doc.unwritten());
    let _ = std::fs::remove_dir_all(&project);
}
