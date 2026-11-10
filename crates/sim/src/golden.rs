//! Each host's step order, written out: the frame as every host runs it
//! today (roadmap M1). A failure here means a step moved, which changes the
//! game's behaviour. Change these lists only on purpose, with a test of the
//! new behaviour.

use crate::{Hosts, Schedule, Stage};

fn names(host: Hosts) -> Vec<&'static str> {
    Schedule::for_host(host).names()
}

#[rustfmt::skip]
const PLAYER: &[&str] = &[
    "net.receive",
    "characters.step", "characters.landed", "humanoids", "players",
    "agents.step", "agents.sensed",
    "input.events", "prompts", "tools", "abilities",
    "scripts.step", "scripts.stepped",
    "physics.hold", "physics.kinematic", "physics.shatters", "physics.step",
    "touches.began", "touches.ended", "breaks", "health",
    "heartbeat",
    "audio", "particles",
    "animators", "tools.place",
    "levels.loaded", "loading.screen",
    "render_step", "camera.move", "camera.follow", "graphics.changed",
    "streaming", "draw.scene", "draw.ui",
    "net.send",
];

/// Exceptions the editor still has (each one is a line of `DRIFT`):
/// - audio runs after the render step;
/// - particles and streaming aren't steps at all: the Scene viewport does
///   them while it draws (`crates/editor/src/viewport.rs`).
#[rustfmt::skip]
const EDITOR_PLAY: &[&str] = &[
    "net.receive",
    "characters.step", "characters.landed", "humanoids", "players",
    "agents.step", "agents.sensed",
    "input.events", "prompts", "tools", "abilities",
    "scripts.step", "scripts.stepped",
    "physics.hold", "physics.kinematic", "physics.shatters", "physics.step",
    "touches.began", "touches.ended", "breaks", "health",
    "heartbeat",
    "animators", "tools.place",
    "levels.loaded", "loading.screen",
    "render_step", "camera.move", "camera.follow", "graphics.changed",
    "audio",
    "net.send",
];

/// Exceptions the server has: it replaces shattered parts before asking
/// which parts are held still. It has no characters, agents, audio or
/// animation of its own: those belong to players' devices.
#[rustfmt::skip]
const SERVER: &[&str] = &[
    "net.receive",
    "players",
    "scripts.step", "scripts.stepped",
    "physics.shatters", "physics.hold", "physics.step",
    "touches.began", "touches.ended", "breaks", "health",
    "heartbeat",
    "net.send",
];

/// The test runner runs no network and draws nothing.
#[rustfmt::skip]
const TEST: &[&str] = &[
    "characters.step", "characters.landed", "humanoids", "players",
    "agents.step", "agents.sensed",
    "input.events", "prompts", "tools", "abilities",
    "scripts.step", "scripts.stepped",
    "physics.kinematic", "physics.shatters", "physics.step",
    "touches.began", "touches.ended", "breaks", "health",
    "heartbeat",
    "audio", "particles",
    "animators", "tools.place",
    "levels.loaded", "loading.screen",
    "render_step", "camera.move", "camera.follow",
];

#[test]
fn the_player_runs_the_standard_order() {
    assert_eq!(names(Hosts::PLAYER), PLAYER);
}

#[test]
fn the_editor_runs_the_player_order_but_for_its_listed_drift() {
    assert_eq!(names(Hosts::EDITOR_PLAY), EDITOR_PLAY);
}

#[test]
fn the_server_runs_only_the_simulation_it_owns() {
    assert_eq!(names(Hosts::SERVER), SERVER);
}

#[test]
fn the_test_runner_runs_the_player_order_without_network_or_drawing() {
    assert_eq!(names(Hosts::TEST), TEST);
}

#[test]
fn every_host_laps_in_the_benchmark_step_names() {
    let laps = |host| -> Vec<&'static str> {
        Schedule::for_host(host)
            .steps()
            .iter()
            .filter_map(|step| step.lap)
            .collect()
    };
    assert_eq!(
        laps(Hosts::PLAYER),
        [
            "Network",
            "Characters",
            "Agents",
            "Input",
            "Scripts",
            "Physics",
            "Heartbeat",
            "Animation",
            "Draw list",
            "UI"
        ]
    );
    assert_eq!(
        laps(Hosts::SERVER),
        [
            "Network",
            "Input",
            "Scripts",
            "Physics",
            "Heartbeat",
            "Network send"
        ]
    );
    assert_eq!(
        laps(Hosts::TEST),
        [
            "Characters",
            "Agents",
            "Input",
            "Scripts",
            "Physics",
            "Heartbeat",
            "Animation",
            "RenderStep"
        ]
    );
    assert_eq!(
        laps(Hosts::EDITOR_PLAY),
        [
            "Network",
            "Characters",
            "Agents",
            "Input",
            "Scripts",
            "Physics",
            "Heartbeat",
            "Animation",
            "RenderStep",
            "Audio",
            "Network send"
        ]
    );
}

#[test]
fn the_standard_steps_run_stage_by_stage() {
    let stages: Vec<Stage> = Schedule::standard()
        .steps()
        .iter()
        .map(|step| step.stage)
        .collect();
    assert!(
        stages.is_sorted(),
        "a step is listed out of its stage's place: {stages:?}"
    );
}

#[test]
fn a_step_can_be_inserted_after_another() {
    let mut schedule = Schedule::for_host(Hosts::TEST);
    let extra = schedule.steps()[0];
    let name = extra.name;
    schedule.insert_after("camera.follow", extra);
    let names = schedule.names();
    assert_eq!(names[names.len() - 2..], ["camera.follow", name]);
}

#[test]
#[should_panic(expected = "no step named nowhere")]
fn inserting_after_a_missing_step_is_a_bug() {
    let mut schedule = Schedule::for_host(Hosts::TEST);
    let extra = schedule.steps()[0];
    schedule.insert_after("nowhere", extra);
}
