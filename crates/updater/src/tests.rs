//! Installs and roll backs in scratch folders surrounded by fake user data,
//! with a fake [`Host`] whose "editor" is a text file saying its version.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use axcenity_release::InstallKind;

use crate::apply::{
    self, Action, ApplyError, EDITOR_EXE, Expected, Host, Instruction, UPDATER_EXE, sibling,
};
use crate::stage;

/// Fake processes: an editor file holds "version commit".
#[derive(Default)]
struct Fake {
    closes: bool,
    launched: Vec<PathBuf>,
    recorded: Vec<String>,
}

impl Host for Fake {
    fn wait_for_exit(&mut self, _: u32) -> bool {
        self.closes
    }

    fn version_of(&mut self, editor: &Path) -> Result<(String, String), String> {
        let text = fs::read_to_string(editor).map_err(|e| e.to_string())?;
        let mut words = text.split_whitespace();
        match (words.next(), words.next()) {
            (Some(version), Some(commit)) => Ok((version.to_owned(), commit.to_owned())),
            _ => Err("it crashed".to_owned()),
        }
    }

    fn launch(&mut self, editor: &Path, _: &[String]) -> Result<(), String> {
        self.launched.push(editor.to_owned());
        Ok(())
    }

    fn record_version(&mut self, _: &Path, _: InstallKind, version: &str) -> Result<(), String> {
        self.recorded.push(version.to_owned());
        Ok(())
    }
}

fn closing() -> Fake {
    Fake {
        closes: true,
        ..Fake::default()
    }
}

struct World {
    root: PathBuf,
    install: PathBuf,
    staged: PathBuf,
}

const KIND: &str = r#"{"schema": 1, "kind": "linux-tarball"}"#;

/// A home with projects, settings and saves; an install of 0.2.0-alpha.1
/// in `home/apps/Axcenity`; a staged 0.2.0-alpha.2 in app data.
fn world(name: &str, new_editor: &str) -> World {
    let root = std::env::temp_dir().join(format!("axcenity-updater-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let home = root.join("home");
    let write = |path: PathBuf, text: &str| {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    };
    write(
        home.join("Projects/Farm/manifest.json"),
        "{\"name\":\"Farm\"}",
    );
    write(home.join("Projects/Farm/world.json"), "a world");
    write(
        home.join(".local/share/Axcenity Editor/app.ron"),
        "settings",
    );
    write(
        home.join(".local/share/Axcenity/Saves/slot1.json"),
        "a save",
    );
    write(home.join("apps/notes.txt"), "next to the install");
    let install = home.join("apps/Axcenity");
    write(install.join("install.json"), KIND);
    write(install.join(EDITOR_EXE), "0.2.0-alpha.1 old");
    write(install.join(UPDATER_EXE), "old updater");
    write(install.join("Uninstall.exe"), "uninstaller");
    write(install.join("assets/logo.png"), "old logo");
    write(
        install.join("assets/removed.png"),
        "gone in the new version",
    );
    let staged = home.join(".local/share/Axcenity Editor/Updates/0.2.0-alpha.2/staged");
    write(staged.join("install.json"), KIND);
    write(staged.join(EDITOR_EXE), new_editor);
    write(staged.join(UPDATER_EXE), "new updater");
    write(staged.join("assets/logo.png"), "new logo");
    World {
        root,
        install,
        staged,
    }
}

impl World {
    fn instruction(&self, action: Action) -> Instruction {
        let updates = self.staged.parent().unwrap().parent().unwrap();
        Instruction {
            schema: 1,
            protocol: 1,
            action,
            install: self.install.clone(),
            staged: (action == Action::Install).then(|| self.staged.clone()),
            expect: Some(Expected {
                version: "0.2.0-alpha.2".to_owned(),
                commit: "new".to_owned(),
            }),
            editor_pid: 1,
            relaunch: vec!["/projects/Farm".to_owned()],
            log: updates.join("update.log"),
            result: updates.join("result.json"),
        }
    }

    /// Every file outside the install, its siblings and `Updates/`.
    fn user_data(&self) -> BTreeMap<PathBuf, Vec<u8>> {
        let mut files = BTreeMap::new();
        walk(&self.root, &mut |path| {
            let text = path.to_string_lossy();
            if !text.contains("apps/Axcenity") && !text.contains("Updates") {
                files.insert(path.to_owned(), fs::read(path).unwrap());
            }
        });
        files
    }

    fn read(&self, path: &Path) -> String {
        fs::read_to_string(path).unwrap_or_default()
    }

    fn siblings(&self) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(self.install.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }
}

fn walk(dir: &Path, each: &mut dyn FnMut(&Path)) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            walk(&path, each);
        } else {
            each(&path);
        }
    }
}

#[test]
fn an_update_swaps_the_folder_keeps_the_old_one_and_touches_nothing_else() {
    let w = world("install", "0.2.0-alpha.2 new");
    let before = w.user_data();
    let mut host = closing();
    let outcome = apply::run(&w.instruction(Action::Install), &mut host);
    assert!(outcome.ok, "{outcome:?}");
    assert_eq!(outcome.version, "0.2.0-alpha.2");

    assert_eq!(w.read(&w.install.join(EDITOR_EXE)), "0.2.0-alpha.2 new");
    assert_eq!(w.read(&w.install.join("assets/logo.png")), "new logo");
    // Files the new version doesn't have are gone; the uninstaller stays.
    assert!(!w.install.join("assets/removed.png").exists());
    assert_eq!(w.read(&w.install.join("Uninstall.exe")), "uninstaller");
    let previous = sibling(&w.install, ".previous");
    assert_eq!(w.read(&previous.join(EDITOR_EXE)), "0.2.0-alpha.1 old");
    assert_eq!(w.siblings(), ["Axcenity", "Axcenity.previous", "notes.txt"]);

    assert_eq!(w.user_data(), before, "user data changed");
    assert_eq!(host.launched, [w.install.join(EDITOR_EXE)]);
    assert_eq!(host.recorded, ["0.2.0-alpha.2"]);
    let result = stage::read_outcome(&w.instruction(Action::Install).result).unwrap();
    assert_eq!(result, outcome);
    let log = fs::read_to_string(w.instruction(Action::Install).log).unwrap();
    assert!(log.contains("swapped") && log.contains("done"), "{log}");

    // And back again.
    let mut host = closing();
    let outcome = apply::run(&w.instruction(Action::RollBack), &mut host);
    assert!(outcome.ok, "{outcome:?}");
    assert_eq!(outcome.version, "0.2.0-alpha.1");
    assert_eq!(w.read(&w.install.join(EDITOR_EXE)), "0.2.0-alpha.1 old");
    assert_eq!(w.read(&previous.join(EDITOR_EXE)), "0.2.0-alpha.2 new");
    assert_eq!(w.siblings(), ["Axcenity", "Axcenity.previous", "notes.txt"]);
    assert_eq!(w.user_data(), before);
    assert_eq!(host.recorded, ["0.2.0-alpha.1"]);
}

#[test]
fn a_new_editor_that_fails_its_check_is_replaced_by_the_old_one() {
    for (name, new_editor) in [("wrong-version", "0.2.0-alpha.9 other"), ("crash", "")] {
        let w = world(name, new_editor);
        let before = w.user_data();
        let mut host = closing();
        let outcome = apply::run(&w.instruction(Action::Install), &mut host);
        assert!(!outcome.ok);
        assert!(
            outcome.message.contains("didn't start correctly"),
            "{}",
            outcome.message
        );
        assert_eq!(outcome.version, "0.2.0-alpha.1");
        assert_eq!(w.read(&w.install.join(EDITOR_EXE)), "0.2.0-alpha.1 old");
        assert_eq!(
            w.read(&w.install.join("assets/removed.png")),
            "gone in the new version"
        );
        assert_eq!(w.siblings(), ["Axcenity", "notes.txt"], "{name}");
        assert_eq!(w.user_data(), before);
        // The old editor comes back.
        assert_eq!(host.launched, [w.install.join(EDITOR_EXE)]);
        assert!(host.recorded.is_empty());
    }
}

#[test]
fn an_editor_that_never_closes_leaves_everything_as_it_was() {
    let w = world("open", "0.2.0-alpha.2 new");
    let mut host = Fake::default();
    let outcome = apply::run(&w.instruction(Action::Install), &mut host);
    assert!(!outcome.ok);
    assert!(outcome.message.contains("didn't close"));
    assert_eq!(w.read(&w.install.join(EDITOR_EXE)), "0.2.0-alpha.1 old");
    assert_eq!(w.siblings(), ["Axcenity", "notes.txt"]);
    // It's still open, so none is started.
    assert!(host.launched.is_empty());
}

#[test]
fn folders_that_arent_an_updatable_install_are_refused() {
    let w = world("refused", "0.2.0-alpha.2 new");
    let refused = |install: &Path| match apply::check_install(install) {
        Err(ApplyError::Refused(why)) => why,
        other => panic!("{other:?}"),
    };
    // No install.json.
    let bare = w.root.join("home/apps/bare");
    fs::create_dir_all(&bare).unwrap();
    assert!(refused(&bare).contains("no install.json"));
    // Top-level folders and relative paths.
    assert!(refused(Path::new("/")).contains("not an install folder"));
    assert!(refused(Path::new("apps/Axcenity")).contains("not an install folder"));
    // A system package updates through its package manager.
    fs::write(
        w.install.join("install.json"),
        r#"{"schema":1,"kind":"deb"}"#,
    )
    .unwrap();
    assert!(refused(&w.install).contains("Debian package"));
    // A link to an install is not followed.
    #[cfg(unix)]
    {
        fs::write(w.install.join("install.json"), KIND).unwrap();
        let link = w.root.join("home/apps/link");
        std::os::unix::fs::symlink(&w.install, &link).unwrap();
        assert!(refused(&link).contains("link"));
    }

    // A staged update of another kind.
    fs::write(w.install.join("install.json"), KIND).unwrap();
    fs::write(
        w.staged.join("install.json"),
        r#"{"schema":1,"kind":"windows-setup"}"#,
    )
    .unwrap();
    let mut host = closing();
    let outcome = apply::run(&w.instruction(Action::Install), &mut host);
    assert!(
        outcome.message.contains("isn't a Linux tar.gz install"),
        "{}",
        outcome.message
    );
    assert_eq!(w.read(&w.install.join(EDITOR_EXE)), "0.2.0-alpha.1 old");

    // An updater too old for the instruction.
    let mut newer = w.instruction(Action::Install);
    newer.protocol = 99;
    assert!(
        apply::run(&newer, &mut closing())
            .message
            .contains("newer updater")
    );
}

#[test]
fn something_else_named_like_the_old_version_is_never_deleted() {
    let w = world("in-the-way", "0.2.0-alpha.2 new");
    let previous = sibling(&w.install, ".previous");
    fs::create_dir_all(&previous).unwrap();
    fs::write(previous.join("thesis.docx"), "years of work").unwrap();
    let outcome = apply::run(&w.instruction(Action::Install), &mut closing());
    assert!(!outcome.ok);
    assert!(
        outcome.message.contains("isn't an older install"),
        "{}",
        outcome.message
    );
    assert_eq!(w.read(&previous.join("thesis.docx")), "years of work");
    assert_eq!(w.read(&w.install.join(EDITOR_EXE)), "0.2.0-alpha.1 old");
    // Nothing to roll back to, either.
    let outcome = apply::run(&w.instruction(Action::RollBack), &mut closing());
    assert!(
        outcome.message.contains("no earlier version"),
        "{}",
        outcome.message
    );
}

#[test]
fn a_download_is_staged_checked_and_handed_over() {
    let w = world("stage", "0.2.0-alpha.2 new");
    // The archive the release workflow makes from a staged install.
    let archive = w
        .root
        .join("AxcenityEditor-0.2.0-alpha.2-linux-x86_64.tar.gz");
    let gz = flate2::write::GzEncoder::new(
        fs::File::create(&archive).unwrap(),
        flate2::Compression::fast(),
    );
    let mut tar = tar::Builder::new(gz);
    tar.append_dir_all("AxcenityEditor-0.2.0-alpha.2-linux-x86_64", &w.staged)
        .unwrap();
    tar.into_inner().unwrap().finish().unwrap();

    let version_dir = w.root.join("Updates/0.2.0-alpha.2");
    let staged = version_dir.join("staged");
    stage::stage(
        &archive,
        "x-linux-x86_64.tar.gz",
        InstallKind::LinuxTarball,
        &staged,
    )
    .unwrap();
    assert_eq!(w.read(&staged.join(EDITOR_EXE)), "0.2.0-alpha.2 new");
    assert!(matches!(
        stage::stage(
            &archive,
            "x-linux-x86_64.tar.gz",
            InstallKind::WindowsSetup,
            &staged
        ),
        Err(stage::StageError::Wrong(_))
    ));

    let mut instruction = w.instruction(Action::Install);
    instruction.staged = Some(staged.clone());
    let (updater, path) = stage::hand_over(&version_dir, &instruction).unwrap();
    assert_eq!(updater, version_dir.join("run").join(UPDATER_EXE));
    assert_eq!(w.read(&updater), "new updater");
    let back: Instruction = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(back, instruction);
}

#[test]
fn the_payload_is_this_platforms_update_archive() {
    let manifest: axcenity_release::Manifest =
        serde_json::from_slice(include_bytes!("../../release/src/testdata/manifest.json")).unwrap();
    let windows = stage::payload(&manifest, InstallKind::WindowsSetup);
    assert_eq!(
        windows.map(|a| a.file.as_str()),
        cfg!(all(windows, target_arch = "x86_64"))
            .then_some("AxcenityEditor-0.2.0-alpha.2-windows-x86_64-update.zip")
    );
    // The fixture has no Linux payload; packages never update themselves.
    assert!(stage::payload(&manifest, InstallKind::LinuxTarball).is_none());
    assert!(stage::payload(&manifest, InstallKind::Deb).is_none());
}
