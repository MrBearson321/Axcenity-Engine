//! Blueprint-style state machines in the Animation tab: a transition wired
//! from Idle's output pin to Walk, with "Speed > 0.1" on its wire's label,
//! plays in the live preview and in Play.

use std::fs;

use axcenity_anim::{Motion, ParameterDef, StateDef, StateMachineFile};
use axcenity_core::Parameter;

use super::tests::{copy_of_example, part, play_for};
use super::*;
use crate::animation::{Node, click_frames, drag_frames, when_words};

const NOODLE: &str = "assets/models/noodle.fbx";
const STEPS: &str = "assets/animations/noodle/Steps.statemachine.json";
const SIZE: egui::Vec2 = egui::vec2(1100.0, 700.0);

/// Draws the Animation tab once for each frame of input.
fn draw_tab(app: &mut EditorApp, ctx: &egui::Context, frames: Vec<Vec<egui::Event>>) {
    for events in frames {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, SIZE)),
            events,
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| app.animation_pane(ui, None));
        });
    }
}

#[test]
fn a_wire_from_idle_to_walk_saying_speed_over_0_1_plays_in_the_preview_and_in_play() {
    let folder = copy_of_example("hello-animation", "blueprint-machine");
    let machine = StateMachineFile {
        parameters: vec![ParameterDef::Number {
            name: "Speed".into(),
            default: 0.0,
        }],
        default_state: "Idle".into(),
        states: vec![
            StateDef::new(
                "Idle",
                Motion::Clip("assets/animations/noodle/Idle.anim.json".into()),
                [0.0, 0.0],
            ),
            StateDef::new(
                "Walk",
                Motion::Clip("assets/animations/noodle/Walk.anim.json".into()),
                [260.0, 0.0],
            ),
        ],
        ..StateMachineFile::default()
    };
    fs::write(folder.join(STEPS), machine.to_json()).unwrap();
    // The example's keys set Speed every frame; here the test sets it.
    fs::write(
        folder.join("scripts/Keys.client.blocks.json"),
        r#"{ "blocks": [] }"#,
    )
    .unwrap();
    let mut app = EditorApp::new(Some(&folder));
    let noodle = part(&app, "Noodle");
    let animator = crate::animation::animator_of(&app.world, noodle).unwrap();
    app.world
        .set_property(animator, "StateMachine", Value::String(STEPS.into()))
        .unwrap();
    app.animation.open_controller(
        &folder,
        &app.assets,
        STEPS,
        Some(NOODLE.into()),
        &mut app.output,
    );

    // Drag from Idle's output pin to Walk, then the Speed chip onto the
    // wire, and open its label.
    let ctx = theme::test_context();
    draw_tab(&mut app, &ctx, vec![Vec::new(); 3]);
    let doc = app.animation.machine_doc();
    let view = &doc.view;
    let from = view.pin(view.rect, &doc.file, Node::State(0), true);
    let to = view
        .node_rect(view.rect, &doc.file, Node::State(1))
        .center();
    draw_tab(&mut app, &ctx, drag_frames(from, to));
    let doc = app.animation.machine_doc();
    assert_eq!(doc.file.transitions.len(), 1, "{:?}", doc.file.transitions);
    let chip = doc.view.chips[0].1.center();
    let label = doc.view.labels[0].1.center();
    draw_tab(&mut app, &ctx, drag_frames(chip, label));
    let label = app.animation.machine_doc().view.labels[0].1.center();
    draw_tab(&mut app, &ctx, click_frames(label));
    let doc = app.animation.machine_doc();
    assert_eq!(doc.view.editing, Some(0), "the label's editor is open");
    let transition = &doc.file.transitions[0];
    assert_eq!(
        (transition.from.as_str(), transition.to.as_str()),
        ("Idle", "Walk")
    );
    assert_eq!(when_words(transition), "Speed > 0.1");
    if let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS") {
        let path = std::path::PathBuf::from(folder).join("state-machine-wire-editor.png");
        crate::snapshot::snapshot(
            &path,
            SIZE,
            vec![Vec::new(); 3],
            &mut |ctx: &egui::Context| {
                egui::CentralPanel::default().show(ctx, |ui| app.animation_pane(ui, None));
            },
        );
    }

    // The live preview: Idle until the Speed slider passes 0.1, then Walk.
    let playing_after = |app: &mut EditorApp, speed: f64| {
        let assets = app.assets.clone();
        let doc = app.animation.machine_doc();
        doc.set_live_parameter("Speed", Parameter::Number(speed));
        for _ in 0..10 {
            doc.tick(&assets, 1.0 / 60.0).unwrap();
        }
        doc.live().state.map(|(state, _)| state)
    };
    assert_eq!(playing_after(&mut app, 0.05), Some(0), "Idle below 0.1");
    assert_eq!(playing_after(&mut app, 0.2), Some(1), "Walk past 0.1");

    // In Play, with the file as written.
    let _ = app.animation.write_all(&folder, &mut app.output);
    let written =
        StateMachineFile::from_json(&fs::read_to_string(folder.join(STEPS)).unwrap()).unwrap();
    assert_eq!(when_words(&written.transitions[0]), "Speed > 0.1");
    app.play();
    let state_after = |app: &mut EditorApp, speed: f64| {
        let world = app.play.as_mut().expect("playing").world_mut();
        world
            .animator_mut(animator)
            .unwrap()
            .parameters
            .insert("Speed".into(), Parameter::Number(speed));
        play_for(app, 0.2);
        let world = app.play.as_ref().unwrap().world();
        world.animator(animator).unwrap().current_state.clone()
    };
    assert_eq!(state_after(&mut app, 0.05), "Idle");
    assert_eq!(state_after(&mut app, 0.2), "Walk");
    app.stop();
    let _ = fs::remove_dir_all(&folder);
}
