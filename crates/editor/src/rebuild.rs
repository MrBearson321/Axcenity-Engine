//! Rebuilding the editor when engine code changes. Only a debug editor run
//! from the source tree can: release builds and exported games have none of
//! this, and the player never depends on the editor.
//!
//! With Settings > Engine > Rebuild when engine code changes on, the editor
//! watches `crates/**` (`.rs`, `Cargo.toml` and shaders) and runs
//! `cargo build -p axcenity-editor` in the background, with the same
//! profile but its own target folder, so the running program's files are
//! never locked (Windows won't overwrite a running program). Progress shows
//! in the bottom-right corner; compiler errors go to Output with
//! `file:line` links, and the running editor carries on as it was. A build
//! that succeeds offers Relaunch (`relaunch.rs`).

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread::JoinHandle;

use axcenity_project::Watcher;

/// The package the background build makes.
pub const EDITOR_PACKAGE: &str = "axcenity-editor";
/// The editor's program name, without the OS's extension.
const EDITOR_PROGRAM: &str = "axcenity-editor";
/// The folder under `target/` that rebuilds go to.
const REBUILD_FOLDER: &str = "relaunch";
/// How many of the build's last error lines (from cargo itself, not the
/// compiler) are kept to explain a build that failed with no compiler errors.
const KEPT_STDERR_LINES: usize = 20;

/// The engine's source tree, which a debug editor knows it was built from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EngineSource {
    /// The Cargo workspace: the folder with the root `Cargo.toml` and `crates/`.
    pub workspace: PathBuf,
}

impl EngineSource {
    /// The workspace this editor was built from, if this is a debug build
    /// and its source tree is still there. Always `None` in release builds,
    /// which don't even keep the path.
    pub fn find() -> Option<EngineSource> {
        #[cfg(debug_assertions)]
        {
            // `CARGO_MANIFEST_DIR` is `<workspace>/crates/editor` at build time.
            let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).parent()?.parent()?;
            Self::at(workspace)
        }
        #[cfg(not(debug_assertions))]
        {
            None
        }
    }

    /// The workspace to rebuild and relaunch from (unified Phase 25): only
    /// in an editor built with the dev-only `engine-dev` feature (Phase 89),
    /// so creators' editors never watch `crates/` or show engine items.
    pub fn for_rebuilds() -> Option<EngineSource> {
        if cfg!(feature = "engine-dev") {
            Self::find()
        } else {
            None
        }
    }

    /// The workspace in `folder`, if it has the editor's sources. Release
    /// builds never look for one.
    #[cfg_attr(not(debug_assertions), allow(dead_code))]
    pub fn at(folder: &Path) -> Option<EngineSource> {
        let has = |path: &str| folder.join(path).is_file();
        (has("Cargo.toml") && has("crates/editor/Cargo.toml")).then(|| EngineSource {
            workspace: folder.to_owned(),
        })
    }

    /// Where rebuilds go: their own target folder, beside the usual one.
    pub fn target_dir(&self) -> PathBuf {
        self.workspace.join("target").join(REBUILD_FOLDER)
    }

    /// The editor a successful rebuild makes.
    pub fn built_editor(&self) -> PathBuf {
        self.target_dir()
            .join("debug")
            .join(format!("{EDITOR_PROGRAM}{}", std::env::consts::EXE_SUFFIX))
    }

    /// Watches the engine's sources (see [`engine_files`]).
    pub fn watch(&self) -> Watcher {
        Watcher::start_listing(self.workspace.clone(), engine_files)
    }

    /// Starts `cargo build -p axcenity-editor` into [`Self::target_dir`].
    /// `wake` is called (on another thread) each time the build says something.
    pub fn build(&self, wake: impl Fn() + Send + 'static) -> std::io::Result<Build> {
        Build::start(
            &self.workspace,
            EDITOR_PACKAGE,
            &self.target_dir(),
            Box::new(wake),
        )
    }
}

/// The engine files a rebuild depends on, by their paths in `workspace`:
/// every `.rs`, `Cargo.toml` and shader under `crates/`. Hidden and `target`
/// folders are skipped.
pub fn engine_files(workspace: &Path) -> Vec<String> {
    let mut found = Vec::new();
    find_engine_files(&workspace.join("crates"), "crates", &mut found);
    found
}

fn find_engine_files(directory: &Path, path: &str, found: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let inner = format!("{path}/{name}");
        match entry.file_type() {
            Ok(kind) if kind.is_dir() && name != "target" => {
                find_engine_files(&entry.path(), &inner, found);
            }
            Ok(kind) if kind.is_file() && is_engine_file(&name) => found.push(inner),
            _ => {}
        }
    }
}

/// True for a file name a rebuild depends on.
fn is_engine_file(name: &str) -> bool {
    name == "Cargo.toml"
        || [".rs", ".wgsl", ".glsl", ".hlsl"]
            .iter()
            .any(|extension| name.ends_with(extension))
}

/// What a running build said.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BuildMessage {
    /// One crate (or build script) was compiled or found up to date.
    Compiled,
    /// A compiler error, as `path:line:column: error[code]: message`.
    Error(String),
    /// The build ended. A failure with no compiler errors (a broken
    /// `Cargo.toml`, a missing `cargo`) is explained by `why`.
    Finished { success: bool, why: Vec<String> },
}

/// One `cargo build` running in the background.
pub struct Build {
    child: Child,
    /// What the output thread read; `None` once cargo closed its output.
    messages: Receiver<Option<BuildMessage>>,
    /// Cargo's own lines (not the compiler's), the last few of them.
    stderr: Option<JoinHandle<Vec<String>>>,
    finished: bool,
}

impl Build {
    /// Starts `cargo build -p <package>` in `workspace`, into `target_dir`,
    /// with machine-readable messages. Cargo is the one that ran this
    /// program (`CARGO`, set by `cargo run`), or `cargo` on the PATH.
    pub fn start(
        workspace: &Path,
        package: &str,
        target_dir: &Path,
        wake: Box<dyn Fn() + Send>,
    ) -> std::io::Result<Build> {
        let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
        let mut child = Command::new(cargo)
            .current_dir(workspace)
            .args(["build", "-p", package, "--message-format=json"])
            .arg("--target-dir")
            .arg(target_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let stdout = child.stdout.take().expect("piped above");
        let stderr = child.stderr.take().expect("piped above");
        let (sender, messages) = mpsc::channel();
        // Two threads, so neither pipe fills up and stalls cargo.
        std::thread::Builder::new()
            .name("engine build".to_string())
            .spawn(move || {
                for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                    if let Some(message) = parse_line(&line) {
                        if sender.send(Some(message)).is_err() {
                            return;
                        }
                        wake();
                    }
                }
                // Cargo closed its output: it's ending.
                let _ = sender.send(None);
                wake();
            })?;
        let stderr = std::thread::Builder::new()
            .name("engine build errors".to_string())
            .spawn(move || {
                let mut kept = Vec::new();
                for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                    kept.push(line);
                    if kept.len() > KEPT_STDERR_LINES {
                        kept.remove(0);
                    }
                }
                kept
            })?;
        Ok(Build {
            child,
            messages,
            stderr: Some(stderr),
            finished: false,
        })
    }

    /// What the build said since the last call. The last one is
    /// [`BuildMessage::Finished`], once cargo has ended.
    pub fn poll(&mut self) -> Vec<BuildMessage> {
        let mut said = Vec::new();
        let mut ended = false;
        for message in self.messages.try_iter() {
            match message {
                Some(message) => said.push(message),
                None => ended = true,
            }
        }
        if self.finished || !ended {
            return said;
        }
        // Cargo closed its output, so it's ending: wait for it.
        let status = self.child.wait();
        self.finished = true;
        let stderr = self
            .stderr
            .take()
            .and_then(|thread| thread.join().ok())
            .unwrap_or_default();
        let success = status.is_ok_and(|status| status.success());
        let why = if success {
            Vec::new()
        } else {
            stderr
                .into_iter()
                .filter(|line| line.trim_start().starts_with("error"))
                .collect()
        };
        said.push(BuildMessage::Finished { success, why });
        said
    }

    /// Whether cargo has ended (and [`BuildMessage::Finished`] was given).
    pub fn finished(&self) -> bool {
        self.finished
    }
}

impl Drop for Build {
    /// A build still running when the editor closes, or when it's replaced,
    /// is stopped.
    fn drop(&mut self) {
        if !self.finished {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

/// One line of `cargo build --message-format=json`, if it's something the
/// editor shows: a compiled crate, or a compiler error.
pub fn parse_line(line: &str) -> Option<BuildMessage> {
    let json: serde_json::Value = serde_json::from_str(line).ok()?;
    match json.get("reason")?.as_str()? {
        "compiler-artifact" => Some(BuildMessage::Compiled),
        "compiler-message" => error_text(json.get("message")?).map(BuildMessage::Error),
        _ => None,
    }
}

/// A compiler error as one line starting with its primary `path:line:column:`,
/// so Output links it. Warnings and notes are left out, and so is rustc's
/// closing "aborting due to…".
fn error_text(message: &serde_json::Value) -> Option<String> {
    let level = message.get("level")?.as_str()?;
    if !level.starts_with("error") {
        return None;
    }
    let text = message.get("message")?.as_str()?;
    let code = message
        .get("code")
        .and_then(|code| code.get("code"))
        .and_then(serde_json::Value::as_str)
        .map_or_else(String::new, |code| format!("[{code}]"));
    let spans = message.get("spans")?.as_array()?;
    let primary = spans.iter().find(|span| {
        span.get("is_primary")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
    });
    let Some(span) = primary else {
        return (!text.starts_with("aborting due to")).then(|| format!("{level}{code}: {text}"));
    };
    let file = span.get("file_name")?.as_str()?.replace('\\', "/");
    let line = span.get("line_start")?.as_u64()?;
    let column = span.get("column_start")?.as_u64()?;
    Some(format!("{file}:{line}:{column}: {level}{code}: {text}"))
}

/// Opens the source file at `path` (inside `workspace`) at `line`: in VS
/// Code when it's installed (`code --goto`), or else with the program the
/// system opens that kind of file with.
pub fn open_source(workspace: &Path, path: &str, line: usize) -> std::io::Result<()> {
    let file = workspace.join(path);
    let code = if cfg!(windows) { "code.cmd" } else { "code" };
    if let Some(code) = find_on_path(code) {
        let mut goto = file.as_os_str().to_owned();
        goto.push(format!(":{line}"));
        Command::new(code).arg("--goto").arg(goto).spawn()?;
        return Ok(());
    }
    let opener = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    Command::new(opener).arg(file).spawn()?;
    Ok(())
}

/// The program called `name` in one of the PATH's folders.
fn find_on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|folder| folder.join(name))
        .find(|program| program.is_file())
}

/// Background builds, as Settings > Engine sets them up, and where the
/// last one got to. The app drives it each frame (`app/engine.rs`).
#[derive(Default)]
pub struct Rebuilds {
    /// The source tree, in a debug editor run from one.
    pub source: Option<EngineSource>,
    /// Settings > Engine > Rebuild when engine code changes. Saved between runs.
    pub on_change: bool,
    /// Settings > Engine > Relaunch automatically after a build. Saved between runs.
    pub relaunch_automatically: bool,
    /// Watches the sources while `on_change` is on.
    pub watcher: Option<Watcher>,
    /// The build running now.
    pub build: Option<Build>,
    /// Sources changed while a build ran, so another follows it.
    pub again: bool,
    /// Crates compiled (or found up to date) so far in this build.
    pub compiled: usize,
    /// How many the last whole build had, for the progress bar. Saved between runs.
    pub expected: usize,
    /// Compiler errors in this build so far.
    pub errors: usize,
    /// A rebuilt editor, ready to relaunch into.
    pub ready: bool,
}

impl Rebuilds {
    /// How far the running build is, from 0 to 1, if one is running.
    pub fn progress(&self) -> Option<f32> {
        self.build.as_ref()?;
        Some(if self.expected == 0 {
            0.0
        } else {
            (self.compiled as f32 / self.expected as f32).min(1.0)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compiled_crates_and_errors_are_read_from_cargo_and_the_rest_is_left_out() {
        let artifact = r#"{"reason":"compiler-artifact","package_id":"x","target":{"name":"axcenity-core"},"fresh":true}"#;
        assert_eq!(parse_line(artifact), Some(BuildMessage::Compiled));
        let error = r#"{"reason":"compiler-message","message":{"level":"error","message":"mismatched types","code":{"code":"E0308"},"spans":[{"file_name":"crates/editor/src/app.rs","line_start":12,"column_start":5,"is_primary":true}],"rendered":"..."}}"#;
        assert_eq!(
            parse_line(error),
            Some(BuildMessage::Error(
                "crates/editor/src/app.rs:12:5: error[E0308]: mismatched types".to_string()
            ))
        );
        for skipped in [
            r#"{"reason":"compiler-message","message":{"level":"warning","message":"unused","code":null,"spans":[]}}"#,
            r#"{"reason":"compiler-message","message":{"level":"error","message":"aborting due to 1 previous error","code":null,"spans":[]}}"#,
            r#"{"reason":"build-script-executed"}"#,
            r#"{"reason":"build-finished","success":false}"#,
            "   Compiling axcenity-core v0.1.0",
        ] {
            assert_eq!(parse_line(skipped), None, "{skipped}");
        }
    }

    #[test]
    fn rust_sources_manifests_and_shaders_under_crates_are_watched() {
        let folder =
            std::env::temp_dir().join(format!("axcenity-engine-files-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        for file in [
            "Cargo.toml",
            "crates/editor/Cargo.toml",
            "crates/editor/src/app.rs",
            "crates/render/src/shaders/mesh.wgsl",
            "crates/editor/README.md",
            "crates/editor/target/debug/build.rs",
            "crates/editor/.cache/x.rs",
            "examples/hello/scripts/a.server.lua",
        ] {
            let path = folder.join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "x").unwrap();
        }
        let mut found = engine_files(&folder);
        found.sort();
        let source = EngineSource::at(&folder);
        let _ = std::fs::remove_dir_all(&folder);

        assert_eq!(
            found,
            [
                "crates/editor/Cargo.toml",
                "crates/editor/src/app.rs",
                "crates/render/src/shaders/mesh.wgsl"
            ]
        );
        assert_eq!(source.map(|source| source.workspace), Some(folder));
    }

    #[test]
    fn a_debug_editor_knows_its_workspace() {
        let source = EngineSource::find().expect("tests are debug builds in the source tree");
        assert!(
            source
                .workspace
                .join("crates/editor/src/rebuild.rs")
                .is_file()
        );
        assert!(source.target_dir().ends_with("target/relaunch"));
        assert!(
            source
                .built_editor()
                .starts_with(source.workspace.join("target/relaunch/debug"))
        );
    }
}
