//! The real `axcenity-updater` binary on this machine: real processes,
//! `--version --json` and renames, with a shell script for an editor.
//! Unix only; Windows is checked by hand (docs/project/releases.md).
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use axcenity_updater::apply::{Action, EDITOR_EXE, Expected, Instruction, Outcome};

fn editor(path: &Path, version: &str, commit: &str) {
    let script = format!(
        "#!/bin/sh\nif [ \"$1\" = --version ]; then echo '{{\"version\":\"{version}\",\"commit\":\"{commit}\"}}'; fi\n"
    );
    fs::write(path, script).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

fn setup(name: &str, new_commit: &str) -> (PathBuf, Instruction) {
    let root = std::env::temp_dir().join(format!("axcenity-real-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let install = root.join("apps/Axcenity");
    let staged = root.join("Updates/0.2.0-alpha.2/staged");
    for dir in [&install, &staged] {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join("install.json"), r#"{"schema":1,"kind":"linux-tarball"}"#).unwrap();
    }
    editor(&install.join(EDITOR_EXE), "0.2.0-alpha.1", "old");
    editor(&staged.join(EDITOR_EXE), "0.2.0-alpha.2", new_commit);
    // A process that has already exited stands in for the closed editor.
    let mut done = Command::new("true").spawn().unwrap();
    let pid = done.id();
    done.wait().unwrap();
    let instruction = Instruction {
        schema: 1,
        protocol: 1,
        action: Action::Install,
        install,
        staged: Some(staged),
        expect: Some(Expected {
            version: "0.2.0-alpha.2".to_owned(),
            commit: "new".to_owned(),
        }),
        editor_pid: pid,
        relaunch: Vec::new(),
        log: root.join("Updates/update.log"),
        result: root.join("Updates/result.json"),
    };
    (root, instruction)
}

fn run(root: &Path, instruction: &Instruction) -> (bool, Outcome) {
    let path = root.join("instruction.json");
    fs::write(&path, serde_json::to_string(instruction).unwrap()).unwrap();
    let status = Command::new(env!("CARGO_BIN_EXE_axcenity-updater"))
        .arg("apply")
        .arg(&path)
        .status()
        .unwrap();
    let outcome = serde_json::from_str(&fs::read_to_string(&instruction.result).unwrap()).unwrap();
    (status.success(), outcome)
}

#[test]
fn the_binary_installs_and_checks_a_real_new_editor() {
    let (root, instruction) = setup("ok", "new");
    let (success, outcome) = run(&root, &instruction);
    assert!(success && outcome.ok, "{outcome:?}");
    assert_eq!(outcome.version, "0.2.0-alpha.2");
    assert!(root.join("apps/Axcenity.previous").join(EDITOR_EXE).is_file());
    let mode = fs::metadata(instruction.install.join(EDITOR_EXE)).unwrap().permissions().mode();
    assert_eq!(mode & 0o111, 0o111, "the new editor is still executable");
}

#[test]
fn the_binary_puts_the_old_editor_back_when_the_new_one_is_wrong() {
    let (root, instruction) = setup("bad", "someone-else");
    let (success, outcome) = run(&root, &instruction);
    assert!(!success && !outcome.ok);
    assert_eq!(outcome.version, "0.2.0-alpha.1");
    assert!(!root.join("apps/Axcenity.previous").exists());
    assert!(!root.join("apps/Axcenity.failed").exists());
}

#[test]
fn the_binary_says_its_version() {
    let out = Command::new(env!("CARGO_BIN_EXE_axcenity-updater"))
        .args(["--version", "--json"])
        .output()
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["version"], env!("CARGO_PKG_VERSION"));
}
