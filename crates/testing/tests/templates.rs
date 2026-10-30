//! Phase 69: a project made from each genre template loads without
//! problems and passes its own game tests (`tests/*.spec.lua`), through
//! `Runner` (which the editor's Tests panel uses too) and the
//! `axcenity-test` binary, so the templates keep working as the engine
//! changes.

use std::path::PathBuf;

use axcenity_project::Template;
use axcenity_testing::{Runner, Selection};

/// A new project made from `template` in a fresh temporary folder.
fn new_project(template: Template) -> PathBuf {
    let parent = std::env::temp_dir().join(format!(
        "axcenity-template-tests-{}-{}",
        template.folder().unwrap(),
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&parent);
    std::fs::create_dir_all(&parent).unwrap();
    axcenity_project::create_project_from(&parent, "My Game", template).unwrap()
}

/// Runs every test of a project made from `template`, and fails with each
/// failure, its output, and the game's output.
fn passes_its_game_tests(template: Template) {
    let folder = new_project(template);
    let project = axcenity_project::load(&folder).unwrap();
    assert!(
        project.problems.is_empty(),
        "{}: {:#?}",
        template.name(),
        project.problems
    );
    let mut runner = Runner::open(&folder).unwrap();
    let results = runner.run(&Selection::All, &mut |_| {});
    let failures: Vec<String> = results
        .iter()
        .filter(|result| !result.passed())
        .map(|result| {
            format!(
                "{} > {}: {}\n    {}",
                result.file,
                result.title(),
                result.failure.as_ref().unwrap().message,
                result.output.join("\n    ")
            )
        })
        .collect();
    assert!(
        failures.is_empty(),
        "{}: {} of {} failed:\n{}",
        template.name(),
        failures.len(),
        results.len(),
        failures.join("\n")
    );
    // The shared menu tests and the template's own.
    let files: std::collections::BTreeSet<_> = results.iter().map(|r| r.file.as_str()).collect();
    assert!(files.len() >= 2, "{}: only {files:?}", template.name());
    let errors: Vec<_> = results
        .iter()
        .flat_map(|result| &result.output)
        .filter(|line| line.starts_with("error:") || line.starts_with("warning:"))
        .collect();
    assert!(errors.is_empty(), "{}: {errors:#?}", template.name());

    // `axcenity-test`, as CI runs it, finds and passes the same tests.
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_axcenity-test"))
        .arg(&folder)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{stdout}");
    let counts = format!("{} passed, 0 failed", results.len());
    assert!(stdout.contains(&counts), "{}: {stdout}", template.name());
    let _ = std::fs::remove_dir_all(folder.parent().unwrap());
}

#[test]
fn the_first_person_template_passes_its_game_tests() {
    passes_its_game_tests(Template::FirstPerson);
}

#[test]
fn the_third_person_template_passes_its_game_tests() {
    passes_its_game_tests(Template::ThirdPerson);
}

#[test]
fn the_vehicle_template_passes_its_game_tests() {
    passes_its_game_tests(Template::Vehicle);
}

#[test]
fn the_top_down_template_passes_its_game_tests() {
    passes_its_game_tests(Template::TopDown);
}

#[test]
fn the_arena_template_passes_its_game_tests() {
    passes_its_game_tests(Template::Arena);
}
