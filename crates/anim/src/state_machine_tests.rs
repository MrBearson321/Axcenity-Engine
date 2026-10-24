//! The state machine: conditions, triggers, exit time, priority and
//! interruption, with states that have no clips (each lasts a second and
//! loops).

use std::collections::BTreeMap;

use axcenity_assets::Rig;
use axcenity_core::Parameter;
use glam::Mat4;

use crate::files::{
    ANY_STATE, Condition, Motion, ParameterDef, StateDef, StateMachineFile, Test, TransitionDef,
};
use crate::{Compiled, Machine};

fn rig() -> Rig {
    Rig {
        bones: Vec::new(),
        joints: Vec::new(),
        fit: Mat4::IDENTITY,
    }
}

fn controller(states: &[&str], transitions: Vec<TransitionDef>) -> Compiled {
    let file = StateMachineFile {
        parameters: vec![
            ParameterDef::Bool {
                name: "IsWalking".into(),
                default: false,
            },
            ParameterDef::Number {
                name: "Speed".into(),
                default: 0.0,
            },
            ParameterDef::Trigger {
                name: "Jump".into(),
            },
        ],
        default_state: states[0].into(),
        states: states
            .iter()
            .map(|name| StateDef::new(name, Motion::Clip(String::new()), [0.0, 0.0]))
            .collect(),
        transitions,
        ..StateMachineFile::default()
    };
    let (compiled, problems) = Compiled::new(file, Some(&rig()), |_| Err("no clips".into()));
    assert_eq!(problems, Vec::<String>::new());
    compiled
}

fn when(from: &str, to: &str, conditions: Vec<Condition>) -> TransitionDef {
    TransitionDef {
        conditions,
        ..TransitionDef::new(from, to)
    }
}

fn test(parameter: &str, test: Test, value: f64) -> Condition {
    Condition {
        parameter: parameter.into(),
        test,
        value,
    }
}

fn parameters(compiled: &Compiled) -> BTreeMap<String, Parameter> {
    compiled
        .file
        .parameters
        .iter()
        .map(|def| (def.name().to_owned(), def.initial()))
        .collect()
}

/// Moves `machine` on a frame, then takes any transition due.
fn frame(
    machine: &mut Machine,
    compiled: &Compiled,
    parameters: &mut BTreeMap<String, Parameter>,
    dt: f64,
) -> Option<usize> {
    machine.advance(compiled, Some(&rig()), parameters, dt);
    machine.evaluate(compiled, parameters)
}

fn state(compiled: &Compiled, machine: &Machine) -> String {
    compiled.state_name(machine.current.state).to_owned()
}

#[test]
fn bools_and_numbers_move_between_states() {
    let compiled = controller(
        &["Idle", "Walk", "Run"],
        vec![
            when("Idle", "Walk", vec![test("IsWalking", Test::IsTrue, 0.0)]),
            when("Walk", "Idle", vec![test("IsWalking", Test::IsFalse, 0.0)]),
            when("Walk", "Run", vec![test("Speed", Test::Greater, 4.0)]),
            when("Run", "Walk", vec![test("Speed", Test::Less, 4.0)]),
        ],
    );
    let mut machine = Machine::new(&compiled);
    let mut params = parameters(&compiled);
    assert_eq!(frame(&mut machine, &compiled, &mut params, 0.1), None);
    params.insert("IsWalking".into(), Parameter::Bool(true));
    assert_eq!(frame(&mut machine, &compiled, &mut params, 0.1), Some(0));
    assert_eq!(state(&compiled, &machine), "Walk");
    // A blend that can't be interrupted finishes (a quarter second) first.
    params.insert("Speed".into(), Parameter::Number(5.0));
    assert_eq!(frame(&mut machine, &compiled, &mut params, 0.1), None);
    frame(&mut machine, &compiled, &mut params, 0.3);
    assert_eq!(state(&compiled, &machine), "Run");
    params.insert("Speed".into(), Parameter::Number(1.0));
    frame(&mut machine, &compiled, &mut params, 0.3);
    assert_eq!(state(&compiled, &machine), "Walk");
    params.insert("IsWalking".into(), Parameter::Bool(false));
    frame(&mut machine, &compiled, &mut params, 0.3);
    assert_eq!(state(&compiled, &machine), "Idle");
}

#[test]
fn a_trigger_is_used_up_by_the_transition_it_starts() {
    let compiled = controller(
        &["Idle", "Jump"],
        vec![when(
            ANY_STATE,
            "Jump",
            vec![test("Jump", Test::Triggered, 0.0)],
        )],
    );
    let mut machine = Machine::new(&compiled);
    let mut params = parameters(&compiled);
    params.insert("Jump".into(), Parameter::Trigger(true));
    assert_eq!(frame(&mut machine, &compiled, &mut params, 0.1), Some(0));
    assert_eq!(params["Jump"], Parameter::Trigger(false));
    // Any State never goes to the state already playing.
    params.insert("Jump".into(), Parameter::Trigger(true));
    assert_eq!(frame(&mut machine, &compiled, &mut params, 0.1), None);
    assert_eq!(params["Jump"], Parameter::Trigger(true), "kept for later");
}

#[test]
fn exit_time_waits_and_with_no_conditions_moves_on_by_itself() {
    let mut back = TransitionDef::new("Attack", "Idle");
    back.exit_time = Some(1.0);
    let mut late = when("Idle", "Attack", vec![test("Jump", Test::Triggered, 0.0)]);
    late.exit_time = Some(0.5);
    let compiled = controller(&["Idle", "Attack"], vec![back, late]);
    let mut machine = Machine::new(&compiled);
    let mut params = parameters(&compiled);
    params.insert("Jump".into(), Parameter::Trigger(true));
    // Idle loops: the trigger only counts in the second half of a pass.
    assert_eq!(frame(&mut machine, &compiled, &mut params, 0.2), None);
    assert_eq!(frame(&mut machine, &compiled, &mut params, 0.4), Some(1));
    assert_eq!(state(&compiled, &machine), "Attack");
    // Attack goes back by itself after one whole pass.
    assert_eq!(frame(&mut machine, &compiled, &mut params, 0.5), None);
    assert_eq!(frame(&mut machine, &compiled, &mut params, 0.6), Some(0));
    assert_eq!(state(&compiled, &machine), "Idle");
}

#[test]
fn the_highest_priority_wins_then_the_first() {
    let walking = || vec![test("IsWalking", Test::IsTrue, 0.0)];
    let mut run = when("Idle", "Run", walking());
    run.priority = 1;
    let compiled = controller(
        &["Idle", "Walk", "Run"],
        vec![
            when("Idle", "Walk", walking()),
            run,
            when("Idle", "Walk", walking()),
        ],
    );
    let mut machine = Machine::new(&compiled);
    let mut params = parameters(&compiled);
    params.insert("IsWalking".into(), Parameter::Bool(true));
    assert_eq!(frame(&mut machine, &compiled, &mut params, 0.1), Some(1));
    let equal = controller(
        &["Idle", "Walk", "Run"],
        vec![
            when("Idle", "Walk", walking()),
            when("Idle", "Run", walking()),
        ],
    );
    let mut machine = Machine::new(&equal);
    assert_eq!(frame(&mut machine, &equal, &mut params, 0.1), Some(0));
}

#[test]
fn only_an_interruptible_blend_can_be_overtaken() {
    for interruptible in [false, true] {
        let mut walk = when("Idle", "Walk", vec![test("IsWalking", Test::IsTrue, 0.0)]);
        walk.blend = 1.0;
        walk.interruptible = interruptible;
        let jump = when(ANY_STATE, "Jump", vec![test("Jump", Test::Triggered, 0.0)]);
        let compiled = controller(&["Idle", "Walk", "Jump"], vec![walk, jump]);
        let mut machine = Machine::new(&compiled);
        let mut params = parameters(&compiled);
        params.insert("IsWalking".into(), Parameter::Bool(true));
        frame(&mut machine, &compiled, &mut params, 0.1);
        assert!(machine.fade.is_some());
        params.insert("Jump".into(), Parameter::Trigger(true));
        let taken = frame(&mut machine, &compiled, &mut params, 0.1);
        assert_eq!(
            taken.is_some(),
            interruptible,
            "interruptible: {interruptible}"
        );
    }
}

#[test]
fn blends_mix_the_two_poses_by_their_share() {
    let mut walk = when("Idle", "Walk", vec![test("IsWalking", Test::IsTrue, 0.0)]);
    walk.blend = 0.5;
    let compiled = controller(&["Idle", "Walk"], vec![walk]);
    let mut machine = Machine::new(&compiled);
    let mut params = parameters(&compiled);
    params.insert("IsWalking".into(), Parameter::Bool(true));
    frame(&mut machine, &compiled, &mut params, 0.1);
    let fade = machine.fade.expect("blending");
    assert_eq!(fade.weight(), 0.0);
    machine.advance(&compiled, Some(&rig()), &params, 0.25);
    assert!((machine.fade.unwrap().weight() - 0.5).abs() < 1e-9);
    machine.advance(&compiled, Some(&rig()), &params, 0.3);
    assert!(machine.fade.is_none(), "done after its blend");
}

#[test]
fn transitions_that_cant_work_are_reported() {
    let file = StateMachineFile {
        states: vec![StateDef::new(
            "Idle",
            Motion::Clip(String::new()),
            [0.0, 0.0],
        )],
        transitions: vec![
            TransitionDef::new("Idle", "Nowhere"),
            TransitionDef::new("Idle", "Idle"),
        ],
        ..StateMachineFile::default()
    };
    let (_, problems) = Compiled::new(file, Some(&rig()), |_| Err("no clips".into()));
    assert_eq!(problems.len(), 2, "{problems:?}");
    assert!(problems[0].contains("doesn't have"));
    assert!(problems[1].contains("never happens"));
}
