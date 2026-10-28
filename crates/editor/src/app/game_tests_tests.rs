//! Phase 53: the Tests panel on copies of the hello-character example: its
//! runs give what `axcenity-test` gives, a failure opens its spec at the
//! line, and saving runs the last tests again.

use std::time::{Duration, Instant};

use axcenity_testing::{Runner, Selection};

use super::tests::copy_of_example;
use super::*;
use crate::game_tests::TestState;

const SPEC: &str = "tests/walk.spec.lua";

/// Waits for the panel's run to finish.
fn finish_run(app: &mut EditorApp) {
    let started = Instant::now();
    while app.game_tests.running() {
        assert!(
            started.elapsed() < Duration::from_secs(300),
            "the run never finished"
        );
        std::thread::sleep(Duration::from_millis(20));
        app.game_tests.poll();
    }
}

/// Each test's file, title and failure message, as the panel has them.
fn panel_results(app: &EditorApp) -> Vec<(String, String, Option<String>)> {
    app.game_tests
        .files
        .iter()
        .flat_map(|file| &file.tests)
        .map(|test| {
            let failure = match &test.state {
                TestState::Passed => None,
                TestState::Failed(failure) => Some(failure.message.clone()),
                other => panic!("{} did not run: {other:?}", test.case.title()),
            };
            (test.case.file.clone(), test.case.title(), failure)
        })
        .collect()
}

#[test]
fn the_panel_runs_the_same_tests_as_the_command_line_with_the_same_results() {
    let folder = copy_of_example("hello-character", "tests-panel");
    let world = folder.join("world.json");
    let text = std::fs::read_to_string(&world).unwrap();
    std::fs::write(
        &world,
        text.replacen("\"WalkSpeed\": 16", "\"WalkSpeed\": 0", 1),
    )
    .unwrap();

    let mut app = EditorApp::new(Some(&folder));
    app.run_tests(Selection::All);
    finish_run(&mut app);
    let panel = panel_results(&app);

    // What `axcenity-test` runs.
    let command_line: Vec<_> = Runner::open(&folder)
        .unwrap()
        .run(&Selection::All, &mut |_| {})
        .into_iter()
        .map(|result| {
            let failure = result
                .failure
                .as_ref()
                .map(|failure| failure.message.clone());
            (result.file.clone(), result.title(), failure)
        })
        .collect();
    assert_eq!(panel, command_line);
    assert_eq!(panel.len(), 3);
    assert_eq!(
        panel[0].2.as_deref(),
        Some("tests/walk.spec.lua:14: expected 0 to be greater than 5")
    );
    assert_eq!(app.game_tests.counts(), (2, 1));
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn a_failures_location_opens_the_spec_at_that_line() {
    let folder = copy_of_example("hello-character", "tests-open");
    let mut app = EditorApp::new(Some(&folder));
    // What clicking `tests/walk.spec.lua:14` does.
    app.open_spec(SPEC, 14);
    let tab = app.tabs.active().unwrap();
    assert_eq!(tab.path, SPEC);
    assert_eq!(tab.go_to_line(), Some(14));
    assert!(tab.text.contains("expect(moved).toBeGreaterThan(5)"));
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn saving_a_spec_reruns_the_last_tests_and_adds_no_script() {
    let folder = copy_of_example("hello-character", "tests-rerun");
    let mut app = EditorApp::new(Some(&folder));
    let scripts = app.project.as_ref().unwrap().scripts.len();
    app.run_tests(Selection::One {
        file: SPEC.to_owned(),
        index: 3,
    });
    finish_run(&mut app);
    assert_eq!(app.game_tests.counts(), (1, 0));

    // Saving runs nothing until Re-run on save is on.
    app.open_spec(SPEC, 1);
    app.tabs
        .get_mut(SPEC)
        .unwrap()
        .text
        .push_str("\n-- saved\n");
    assert!(app.save_tab(SPEC));
    assert!(!app.game_tests.running());

    app.game_tests.rerun_on_save = true;
    app.tabs.get_mut(SPEC).unwrap().text.push_str("-- again\n");
    assert!(app.save_tab(SPEC));
    assert!(app.game_tests.running());
    finish_run(&mut app);
    assert_eq!(app.game_tests.counts(), (1, 0));

    let on_disk = std::fs::read_to_string(folder.join(SPEC)).unwrap();
    assert!(on_disk.ends_with("-- saved\n-- again\n"));
    // A spec isn't a game script: no Script was made for it.
    assert_eq!(app.project.as_ref().unwrap().scripts.len(), scripts);
    let service = app.world.service(ClassName::ServerScriptService).unwrap();
    assert!(app.world.find_first_child(service, "walk.spec").is_none());
    let _ = std::fs::remove_dir_all(&folder);
}
