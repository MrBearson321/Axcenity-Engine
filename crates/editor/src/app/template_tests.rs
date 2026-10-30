//! Phase 69: a project made from each genre template plays in the editor
//! with nothing in Output but what its scripts print, and its game tests
//! pass in the Tests panel with the results `axcenity-test` gives.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use axcenity_core::input::{InputEvent, InputKind};
use axcenity_core::{KeyCode, UserInputType};
use axcenity_project::Template;
use axcenity_testing::{Runner, Selection};

use super::tests::play_for;
use super::*;
use crate::game_tests::TestState;
use crate::output::OutputKind;

/// A new project made from `template` in a fresh temporary folder.
fn new_project(template: Template, test: &str) -> PathBuf {
    let parent = std::env::temp_dir().join(format!(
        "axcenity-editor-{test}-{}-{}",
        template.folder().unwrap(),
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&parent);
    std::fs::create_dir_all(&parent).unwrap();
    axcenity_project::create_project_from(&parent, "My Game", template).unwrap()
}

/// Output's warnings and errors.
fn problems(app: &EditorApp) -> Vec<String> {
    app.output
        .lines()
        .iter()
        .filter(|line| matches!(line.kind, OutputKind::Warning | OutputKind::Error))
        .map(|line| line.text.clone())
        .collect()
}

fn key(key: KeyCode, pressed: bool) -> InputEvent {
    InputEvent::new(InputKind::Key { key, pressed })
}

/// Plays for a few seconds: walking, a click in the middle of the view,
/// and the pause menu opened and closed.
fn plays_without_problems(template: Template) {
    let folder = new_project(template, "template-play");
    let mut app = EditorApp::new(Some(&folder));
    assert!(
        problems(&app).is_empty(),
        "{}: {:#?}",
        template.name(),
        problems(&app)
    );
    app.play();
    assert!(
        app.playing(),
        "{}: {:#?}",
        template.name(),
        app.output.lines()
    );
    play_for(&mut app, 1.0);
    let click = |pressed| {
        InputEvent::new(InputKind::MouseButton {
            button: UserInputType::MouseButton1,
            pressed,
            position: [400.0, 300.0],
        })
    };
    let presses = [
        vec![key(KeyCode::W, true)],
        vec![key(KeyCode::W, false), click(true)],
        vec![click(false), key(KeyCode::Escape, true)],
        vec![key(KeyCode::Escape, false)],
        vec![key(KeyCode::Escape, true)],
        vec![key(KeyCode::Escape, false)],
    ];
    for events in presses {
        app.play_frame(1.0 / 60.0, &events, None, None);
        play_for(&mut app, 0.5);
    }
    play_for(&mut app, 2.0);
    assert!(
        app.playing(),
        "{}: {:#?}",
        template.name(),
        app.output.lines()
    );
    // The game ran: the player has a character, and the menu's LocalScript
    // claimed the menu for them.
    let game = shown(&app.world, &app.play);
    let player = game.get(game.players()).unwrap().children()[0];
    assert!(
        matches!(
            game.get(player).unwrap().property("Character"),
            Some(axcenity_core::Value::Instance(Some(_)))
        ),
        "{}",
        template.name()
    );
    let menu = game.find_first_child(game.workspace(), "GameMenu").unwrap();
    assert_eq!(
        game.get(menu).unwrap().attribute("Owner"),
        Some(&axcenity_core::Value::String("Player".into())),
        "{}",
        template.name()
    );
    assert!(
        problems(&app).is_empty(),
        "{}: {:#?}",
        template.name(),
        problems(&app)
    );
    app.stop();
    let _ = std::fs::remove_dir_all(folder.parent().unwrap());
}

#[test]
fn every_genre_template_plays_in_the_editor_without_errors() {
    for template in Template::GAMES {
        plays_without_problems(template);
    }
}

#[test]
fn every_genre_templates_tests_pass_in_the_tests_panel_as_on_the_command_line() {
    for template in Template::GAMES {
        let folder = new_project(template, "template-tests");
        let mut app = EditorApp::new(Some(&folder));
        app.run_tests(Selection::All);
        let started = Instant::now();
        while app.game_tests.running() {
            assert!(
                started.elapsed() < Duration::from_secs(300),
                "the run never finished"
            );
            std::thread::sleep(Duration::from_millis(20));
            app.game_tests.poll();
        }
        let panel: Vec<(String, String, bool)> = app
            .game_tests
            .files
            .iter()
            .flat_map(|file| &file.tests)
            .map(|test| {
                let passed = match &test.state {
                    TestState::Passed => true,
                    TestState::Failed(failure) => {
                        panic!(
                            "{}: {}: {}",
                            template.name(),
                            test.case.title(),
                            failure.message
                        )
                    }
                    other => panic!("{} did not run: {other:?}", test.case.title()),
                };
                (test.case.file.clone(), test.case.title(), passed)
            })
            .collect();
        let command_line: Vec<_> = Runner::open(&folder)
            .unwrap()
            .run(&Selection::All, &mut |_| {})
            .into_iter()
            .map(|result| (result.file.clone(), result.title(), result.passed()))
            .collect();
        assert_eq!(panel, command_line, "{}", template.name());
        assert!(panel.len() >= 8, "{}: {panel:?}", template.name());
        let _ = std::fs::remove_dir_all(folder.parent().unwrap());
    }
}
