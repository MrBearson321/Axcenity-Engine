//! `axcenity-test` as CI runs it: the example's tests, its exit code and
//! its JUnit XML.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn example(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .join(name)
}

fn copy_folder(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_folder(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

#[test]
fn the_example_passes_and_exits_with_success() {
    let output = Command::new(env!("CARGO_BIN_EXE_axcenity-test"))
        .arg(example("hello-character"))
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{stdout}");
    assert!(stdout.contains(
        "PASS tests/walk.spec.lua > Character > walks more than 5 studs while W is held for a second"
    ));
    assert!(stdout.contains("3 passed, 0 failed"), "{stdout}");
}

#[test]
fn a_failure_exits_with_one_and_is_in_the_junit_xml() {
    let folder = std::env::temp_dir()
        .join("axcenity-testing-tests")
        .join(format!("command-line-{}", std::process::id()));
    let _ = fs::remove_dir_all(&folder);
    copy_folder(&example("hello-character"), &folder);
    let world = folder.join("world.json");
    let text = fs::read_to_string(&world).unwrap();
    fs::write(
        &world,
        text.replacen("\"WalkSpeed\": 16", "\"WalkSpeed\": 0", 1),
    )
    .unwrap();
    let junit = folder.join("results.xml");

    let output = Command::new(env!("CARGO_BIN_EXE_axcenity-test"))
        .arg(&folder)
        .arg("--junit")
        .arg(&junit)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(1), "{stdout}");
    assert!(stdout.contains("tests/walk.spec.lua:14: expected 0 to be greater than 5"));
    let xml = fs::read_to_string(junit).unwrap();
    assert!(xml.contains("<testsuites name=\"Hello Character\" tests=\"3\" failures=\"1\">"));
    assert!(
        xml.contains(
            "<failure message=\"tests/walk.spec.lua:14: expected 0 to be greater than 5\">"
        )
    );
    let _ = fs::remove_dir_all(folder);
}

#[test]
fn a_folder_that_is_not_a_project_exits_with_two() {
    let output = Command::new(env!("CARGO_BIN_EXE_axcenity-test"))
        .arg(example("does-not-exist"))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
}
