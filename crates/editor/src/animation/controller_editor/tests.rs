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
        GraphEvent::AddState([10.0, 20.0], None),
        GraphEvent::AddState(
            [10.0, 20.0],
            Some("assets/animations/hero/Walk.anim.json".into()),
        ),
    ]);
    assert_eq!(doc.file.states[2].name, "New State");
    assert_eq!(doc.file.states[3].name, "Walk 2");
    let before = behaviour(&doc.file);
    doc.apply(vec![GraphEvent::Move(Node::State(0), [99.0, 99.0])]);
    assert_eq!(behaviour(&doc.file), before);
    assert_eq!(doc.file.states[0].position, [99.0, 99.0]);
}
