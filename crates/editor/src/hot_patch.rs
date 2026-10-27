//! Hot-patching engine code into the running editor (unified Phase 26).
//! Development only: an editor built with the `hot-patch` feature and
//! started by `dx serve --hot-patch` (see the README) connects to dx, which
//! builds a patch each time engine code changes, and `subsecond` swaps the
//! changed functions in. The editor's frame and the game session's frame
//! are called through [`call`], so the next frame runs the new code, and
//! Play keeps running.
//!
//! A patch can only change what functions do. A change to a struct's
//! fields, a signature, a constant, a dependency or a shader falls back to
//! rebuilding and relaunching (`rebuild.rs`, `relaunch.rs`): [`HotPatch`]
//! keeps the sources the running code was built from, and [`Verdict`] says
//! which kind of change was made. Until the relaunch, frames call the code
//! they were built with, so a patch dx made for the new layout never runs
//! on the old data. Started normally, or built without the feature, the
//! editor has none of this and uses rebuild and relaunch alone.

mod shape;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use eframe::egui;

use crate::rebuild::{EngineSource, engine_files};

/// How long a change dx can patch waits for the patch before the editor is
/// rebuilt instead (dx's build failed, or it doesn't patch that crate).
pub const PATCH_WAIT: Duration = Duration::from_secs(60);

/// Runs `run`, through `subsecond`'s jump table when `patched` is true and
/// the editor has the `hot-patch` feature, so it runs the newest version
/// dx patched in. Otherwise (and in release builds) it's a plain call.
pub fn call<O>(patched: bool, mut run: impl FnMut() -> O) -> O {
    #[cfg(feature = "hot-patch")]
    if patched {
        return subsecond::call(run);
    }
    #[cfg(not(feature = "hot-patch"))]
    let _ = patched;
    run()
}

/// What changed in the engine's sources since the running code was built
/// (or last patched).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Nothing that's part of the running editor (tests, comments only in
    /// files already patched).
    Unchanged,
    /// Only function bodies, or added or removed items: dx can patch these files.
    Patchable(Vec<String>),
    /// A patch can't carry this; the reason names the file and the item.
    Unpatchable(String),
}

/// Hot-patching, while dx serves this editor.
pub struct HotPatch {
    workspace: PathBuf,
    /// Each engine file's text when the running code was built or last
    /// patched, by its path in the workspace.
    baseline: BTreeMap<String, String>,
    /// Patches applied so far, counted by `subsecond`'s handler on dx's thread.
    applied: Arc<AtomicUsize>,
    /// How many of them have been looked at.
    seen: usize,
    /// Since when a change dx can patch has waited for its patch.
    pub waiting: Option<Instant>,
    /// How long it may wait ([`PATCH_WAIT`]; shorter in tests).
    pub wait: Duration,
    /// A change that can't be patched was made: frames call the code they
    /// were built with until the relaunch, or until a patch made from
    /// sources that can be patched arrives.
    pub held: bool,
}

impl HotPatch {
    /// Hot-patching for the engine in `workspace`, from its sources now.
    #[cfg_attr(not(feature = "hot-patch"), allow(dead_code))]
    pub fn new(workspace: &Path) -> HotPatch {
        HotPatch {
            workspace: workspace.to_owned(),
            baseline: read_sources(workspace),
            applied: Arc::default(),
            seen: 0,
            waiting: None,
            wait: PATCH_WAIT,
            held: false,
        }
    }

    /// Connects to dx when it's serving this editor, and starts counting the
    /// patches it applies (each wakes `ctx`). `None` when the editor was
    /// started any other way, or built without the `hot-patch` feature.
    #[cfg(feature = "hot-patch")]
    pub fn connect(source: &EngineSource, ctx: &egui::Context) -> Option<HotPatch> {
        // dx tells the program it serves where its devserver is.
        dioxus_cli_config::devserver_ws_endpoint()?;
        // Applies each patch dx sends for this process on its own thread.
        dioxus_devtools::connect_subsecond();
        let hot = HotPatch::new(&source.workspace);
        let (applied, ctx) = (hot.applied(), ctx.clone());
        subsecond::register_handler(Arc::new(move || {
            applied.fetch_add(1, Ordering::SeqCst);
            ctx.request_repaint();
        }));
        Some(hot)
    }

    /// Without the `hot-patch` feature there's nothing to connect to.
    #[cfg(not(feature = "hot-patch"))]
    pub fn connect(_source: &EngineSource, _ctx: &egui::Context) -> Option<HotPatch> {
        None
    }

    /// The count of patches applied, which `subsecond`'s handler adds to.
    #[cfg_attr(not(feature = "hot-patch"), allow(dead_code))]
    pub fn applied(&self) -> Arc<AtomicUsize> {
        Arc::clone(&self.applied)
    }

    /// True once for each time dx applied patches since the last look.
    pub fn take_patch(&mut self) -> bool {
        let applied = self.applied.load(Ordering::SeqCst);
        let new = applied > self.seen;
        self.seen = applied;
        new
    }

    /// Whether frames go through the patched code.
    pub fn patched_frames(&self) -> bool {
        !self.held
    }

    /// What changed between the sources the running code came from and the
    /// ones on disk now. The first change that can't be patched decides.
    pub fn verdict(&self) -> Verdict {
        let now = read_sources(&self.workspace);
        let mut paths: Vec<&String> = self.baseline.keys().chain(now.keys()).collect();
        paths.sort();
        paths.dedup();
        let mut patchable = Vec::new();
        for path in paths {
            let (old, new) = (self.baseline.get(path), now.get(path));
            if old == new || !in_the_editor(path) {
                continue;
            }
            let (old, new) = (
                old.map_or("", String::as_str),
                new.map_or("", String::as_str),
            );
            if let Some(why) = unpatchable_file(path, old, new) {
                return Verdict::Unpatchable(why);
            }
            patchable.push(path.clone());
        }
        if patchable.is_empty() {
            Verdict::Unchanged
        } else {
            Verdict::Patchable(patchable)
        }
    }

    /// The running code now matches the sources on disk: a patch made from
    /// them was applied.
    pub fn accept(&mut self) {
        self.baseline = read_sources(&self.workspace);
    }
}

/// Every engine file's text, by its path in `workspace`.
fn read_sources(workspace: &Path) -> BTreeMap<String, String> {
    engine_files(workspace)
        .into_iter()
        .filter_map(|path| {
            let text = std::fs::read_to_string(workspace.join(&path)).ok()?;
            // Git may check files out with Windows line endings.
            Some((path, text.replace("\r\n", "\n")))
        })
        .collect()
}

/// False for files the running editor isn't built from: tests, benches and
/// examples.
fn in_the_editor(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    !(name == "tests.rs"
        || name.ends_with("_tests.rs")
        || ["/tests/", "/benches/", "/examples/"]
            .iter()
            .any(|folder| path.contains(folder)))
}

/// Why the change from `old` to `new` in the file at `path` can't be patched.
fn unpatchable_file(path: &str, old: &str, new: &str) -> Option<String> {
    let name = path.rsplit('/').next().unwrap_or(path);
    if name == "Cargo.toml" {
        return Some(format!("`{path}` changed, which can change dependencies"));
    }
    if name == "build.rs" {
        return Some(format!(
            "`{path}` changed, and build scripts only run when the engine is built"
        ));
    }
    if !name.ends_with(".rs") {
        return Some(format!(
            "`{path}` changed, and shaders are built into the program"
        ));
    }
    shape::unpatchable(old, new).map(|why| format!("{why} in `{path}`"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A workspace with one engine file, `crates/physics/src/body.rs`.
    fn workspace(test: &str, body: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!(
            "axcenity-hot-patch-{test}-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(folder.join("crates/physics/src")).unwrap();
        std::fs::write(folder.join("crates/physics/src/body.rs"), body).unwrap();
        folder
    }

    const BODY: &str = "pub struct Body { pub y: f32 }\n\
                        pub fn step(body: &mut Body) { body.y -= 1.0; }\n";

    #[test]
    fn a_body_change_is_patchable_and_accepting_it_starts_again_from_there() {
        let folder = workspace("body", BODY);
        let mut hot = HotPatch::new(&folder);
        assert_eq!(hot.verdict(), Verdict::Unchanged);
        let file = folder.join("crates/physics/src/body.rs");
        std::fs::write(&file, BODY.replace("-= 1.0", "-= 2.0")).unwrap();
        assert_eq!(
            hot.verdict(),
            Verdict::Patchable(vec!["crates/physics/src/body.rs".to_owned()])
        );
        hot.accept();
        assert_eq!(hot.verdict(), Verdict::Unchanged);
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn fields_dependencies_shaders_and_build_scripts_are_not() {
        let folder = workspace("fields", BODY);
        let hot = HotPatch::new(&folder);
        let file = folder.join("crates/physics/src/body.rs");
        std::fs::write(&file, BODY.replace("pub y: f32", "pub y: f32, pub x: f32")).unwrap();
        assert_eq!(
            hot.verdict(),
            Verdict::Unpatchable(
                "`struct Body` changed its definition in `crates/physics/src/body.rs`".to_owned()
            )
        );
        std::fs::write(&file, BODY).unwrap();
        for (path, why) in [
            ("crates/physics/Cargo.toml", "can change dependencies"),
            ("crates/physics/build.rs", "build scripts"),
            ("crates/physics/src/sky.wgsl", "shaders"),
        ] {
            std::fs::write(folder.join(path), "new").unwrap();
            match hot.verdict() {
                Verdict::Unpatchable(reason) => {
                    assert!(reason.contains(path) && reason.contains(why), "{reason}")
                }
                other => panic!("{path}: {other:?}"),
            }
            std::fs::remove_file(folder.join(path)).unwrap();
        }
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn tests_are_not_part_of_the_running_editor() {
        let folder = workspace("tests", BODY);
        let hot = HotPatch::new(&folder);
        std::fs::write(
            folder.join("crates/physics/src/body_tests.rs"),
            "struct Fake { a: u8 }",
        )
        .unwrap();
        assert_eq!(hot.verdict(), Verdict::Unchanged);
        assert!(in_the_editor("crates/physics/src/body.rs"));
        assert!(!in_the_editor("crates/physics/tests/fall.rs"));
        assert!(!in_the_editor("crates/editor/src/app/tests.rs"));
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn patches_are_counted_once_and_a_held_editor_calls_its_own_code() {
        let folder = workspace("count", BODY);
        let mut hot = HotPatch::new(&folder);
        assert!(!hot.take_patch());
        hot.applied().fetch_add(2, Ordering::SeqCst);
        assert!(hot.take_patch());
        assert!(!hot.take_patch());
        assert!(hot.patched_frames());
        hot.held = true;
        assert!(!hot.patched_frames());
        // Without the feature (or held), `call` is a plain call.
        assert_eq!(call(false, || 7), 7);
        let _ = std::fs::remove_dir_all(&folder);
    }
}
