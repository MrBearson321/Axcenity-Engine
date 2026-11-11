//! The real `axcenity-updater` binary on this machine: real processes,
//! `--version --json` and renames. The end-to-end test runs everywhere (on
//! Windows too, with a zip and the setup's install kind); the others use a
//! shell script for an editor, so they're Unix only.

use std::fs;
use std::path::Path;
use std::process::Command;

use axcenity_release::InstallKind;
use axcenity_updater::apply::{Action, EDITOR_EXE, Expected, Instruction, Outcome, UPDATER_EXE};
use axcenity_updater::stage;

#[cfg(unix)]
mod scripted {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    fn editor(path: &Path, version: &str, commit: &str) {
        let script = format!(
            "#!/bin/sh\nif [ \"$1\" = --version ]; then echo '{{\"version\":\"{version}\",\"commit\":\"{commit}\"}}'; fi\n"
        );
        fs::write(path, script).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn setup(name: &str, new_commit: &str) -> (PathBuf, Instruction) {
        let root =
            std::env::temp_dir().join(format!("axcenity-real-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let install = root.join("apps/Axcenity");
        let staged = root.join("Updates/0.2.0-alpha.2/staged");
        for dir in [&install, &staged] {
            fs::create_dir_all(dir).unwrap();
            fs::write(
                dir.join("install.json"),
                r#"{"schema":1,"kind":"linux-tarball"}"#,
            )
            .unwrap();
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
        let outcome =
            serde_json::from_str(&fs::read_to_string(&instruction.result).unwrap()).unwrap();
        (status.success(), outcome)
    }

    #[test]
    fn the_binary_installs_and_checks_a_real_new_editor() {
        let (root, instruction) = setup("ok", "new");
        let (success, outcome) = run(&root, &instruction);
        assert!(success && outcome.ok, "{outcome:?}");
        assert_eq!(outcome.version, "0.2.0-alpha.2");
        assert!(
            root.join("apps/Axcenity.previous")
                .join(EDITOR_EXE)
                .is_file()
        );
        let mode = fs::metadata(instruction.install.join(EDITOR_EXE))
            .unwrap()
            .permissions()
            .mode();
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

/// A process that has already exited, standing in for the closed editor.
fn exited_pid() -> u32 {
    let mut done = Command::new(env!("CARGO_BIN_EXE_axcenity-updater"))
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let pid = done.id();
    done.wait().unwrap();
    pid
}

/// Packs `files` the way the release does: on Windows a zip with them at
/// its root, elsewhere a tar.gz with one top folder.
fn pack(archive: &Path, files: &[(&str, &[u8])]) {
    let out = fs::File::create(archive).unwrap();
    if cfg!(windows) {
        let mut zip = zip::ZipWriter::new(out);
        for (name, bytes) in files {
            zip.start_file(
                *name,
                zip::write::SimpleFileOptions::default().unix_permissions(0o755),
            )
            .unwrap();
            std::io::Write::write_all(&mut zip, bytes).unwrap();
        }
        zip.finish().unwrap();
    } else {
        let gz = flate2::write::GzEncoder::new(out, flate2::Compression::fast());
        let mut tar = tar::Builder::new(gz);
        for (name, bytes) in files {
            let mut header = tar::Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            tar.append_data(&mut header, format!("AxcenityEditor/{name}"), *bytes)
                .unwrap();
        }
        tar.into_inner().unwrap().finish().unwrap();
    }
}

/// From a downloaded archive to an installed, checked version, with the
/// real updater binary standing in for the editor too (it answers
/// `--version --json` the same way). Touches only a scratch folder; on
/// Windows the uninstall entry is changed only when it names that folder.
#[test]
fn a_downloaded_archive_is_staged_handed_over_and_installed() {
    let bin = Path::new(env!("CARGO_BIN_EXE_axcenity-updater"));
    let out = Command::new(bin)
        .args(["--version", "--json"])
        .output()
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let (version, commit) = (
        json["version"].as_str().unwrap().to_owned(),
        json["commit"].as_str().unwrap().to_owned(),
    );
    let (kind, marker, file) = if cfg!(windows) {
        (
            InstallKind::WindowsSetup,
            r#"{"schema":1,"kind":"windows-setup"}"#,
            "x-windows-x86_64-update.zip",
        )
    } else {
        (
            InstallKind::LinuxTarball,
            r#"{"schema":1,"kind":"linux-tarball"}"#,
            "x-linux-x86_64.tar.gz",
        )
    };

    let root = std::env::temp_dir().join(format!("axcenity-real-e2e-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    // The install: an "older" editor and updater (the same program; a file
    // only the old version has tells them apart), its marker and the
    // uninstaller the setup adds.
    let program = fs::read(bin).unwrap();
    let install = root.join("apps").join("Axcenity");
    fs::create_dir_all(&install).unwrap();
    fs::write(install.join("install.json"), marker).unwrap();
    fs::write(install.join(EDITOR_EXE), &program).unwrap();
    fs::write(install.join(UPDATER_EXE), &program).unwrap();
    fs::write(install.join("old-only.txt"), "old").unwrap();
    fs::write(install.join("Uninstall.exe"), "uninstaller").unwrap();
    #[cfg(unix)]
    for exe in [EDITOR_EXE, UPDATER_EXE] {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(install.join(exe), fs::Permissions::from_mode(0o755)).unwrap();
    }
    // The release's archive.
    let archive = root.join(file);
    pack(
        &archive,
        &[
            ("install.json", marker.as_bytes()),
            (EDITOR_EXE, &program),
            (UPDATER_EXE, &program),
        ],
    );

    let version_dir = root.join("Updates").join(&version);
    let staged = version_dir.join("staged");
    stage::stage(&archive, file, kind, &staged).unwrap();
    let instruction = Instruction {
        schema: 1,
        protocol: 1,
        action: Action::Install,
        install: install.clone(),
        staged: Some(staged),
        expect: Some(Expected {
            version: version.clone(),
            commit,
        }),
        editor_pid: exited_pid(),
        // The "editor" started afterwards is the updater: give it
        // arguments it answers and exits on.
        relaunch: vec!["--version".to_owned()],
        log: root.join("Updates").join("update.log"),
        result: root.join("Updates").join("result.json"),
    };
    let (updater, path) = stage::hand_over(&version_dir, &instruction).unwrap();
    let status = Command::new(&updater)
        .arg("apply")
        .arg(&path)
        .status()
        .unwrap();
    let outcome: Outcome =
        serde_json::from_str(&fs::read_to_string(&instruction.result).unwrap()).unwrap();
    assert!(status.success() && outcome.ok, "{outcome:?}");
    assert_eq!(outcome.version, version);
    let previous = root.join("apps").join("Axcenity.previous");
    assert!(previous.join("old-only.txt").is_file());
    assert!(!install.join("old-only.txt").exists());
    assert_eq!(
        fs::read(install.join("Uninstall.exe")).unwrap(),
        b"uninstaller"
    );
    let installed = Command::new(install.join(EDITOR_EXE))
        .args(["--version", "--json"])
        .output()
        .unwrap();
    assert!(installed.status.success());

    // And back, with the updater the install now has.
    let rollback = root.join("Updates").join("rollback");
    let back = Instruction {
        action: Action::RollBack,
        staged: None,
        expect: None,
        editor_pid: exited_pid(),
        relaunch: Vec::new(),
        ..instruction
    };
    fs::remove_file(&back.result).unwrap();
    let (updater, path) = stage::hand_over(&rollback, &back).unwrap();
    let status = Command::new(&updater)
        .arg("apply")
        .arg(&path)
        .status()
        .unwrap();
    let outcome: Outcome =
        serde_json::from_str(&fs::read_to_string(&back.result).unwrap()).unwrap();
    assert!(status.success() && outcome.ok, "{outcome:?}");
    assert!(install.join("old-only.txt").is_file());
    assert!(!previous.join("old-only.txt").exists());
    assert!(previous.join(EDITOR_EXE).is_file(), "the newer one is kept");
    let _ = fs::remove_dir_all(&root);
}
