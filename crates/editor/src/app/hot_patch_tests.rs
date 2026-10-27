//! Hot-patching engine code while dx serves the editor, and falling back
//! to rebuild and relaunch (unified Phase 26). dx itself isn't here, so a
//! patch arriving is played by adding to the count `subsecond`'s handler
//! keeps.

use std::sync::atomic::Ordering;
use std::time::Duration;

use super::relaunch_tests::{engine_until, tiny_workspace};
use super::tests::{copy_of_example, play_for, printed};
use super::*;
use crate::hot_patch::HotPatch;
use crate::output::OutputKind;
use crate::rebuild::EngineSource;

/// The engine's `main.rs` in the tiny workspace: a struct and a step.
const ENGINE: &str = "struct Body {\n    y: f32,\n}\n\n\
                      fn step(body: &mut Body) {\n    body.y -= 1.0;\n}\n\n\
                      fn main() {\n    let mut body = Body { y: 0.0 };\n    \
                      step(&mut body);\n    let _ = body.y;\n}\n";

/// An editor on `hello-touched`, hot-patching the tiny workspace as if dx
/// served it, with Settings > Engine > Rebuild when engine code changes off.
fn hot_patching(test: &str) -> (EditorApp, PathBuf, egui::Context) {
    let workspace = tiny_workspace(test, ENGINE);
    let mut app = EditorApp::new(Some(&copy_of_example("hello-touched", test)));
    app.rebuilds = Rebuilds {
        source: EngineSource::at(&workspace),
        hot_patch: Some(HotPatch::new(&workspace)),
        hot_patch_checked: true,
        ..Rebuilds::default()
    };
    let ctx = theme::test_context();
    app.engine_frame(&ctx);
    assert!(
        app.rebuilds.watcher.is_some(),
        "hot-patching watches the engine"
    );
    // The watcher's first look, before anything changes.
    std::thread::sleep(Duration::from_millis(300));
    (app, workspace, ctx)
}

fn edit_engine(workspace: &Path, from: &str, to: &str) {
    let main = workspace.join("crates/editor/src/main.rs");
    let text = std::fs::read_to_string(&main).unwrap();
    assert!(text.contains(from));
    std::fs::write(&main, text.replacen(from, to, 1)).unwrap();
}

fn dx_applies_a_patch(app: &EditorApp) {
    let hot = app.rebuilds.hot_patch.as_ref().unwrap();
    hot.applied().fetch_add(1, Ordering::SeqCst);
}

fn said(app: &EditorApp, start: &str) -> bool {
    printed(app).iter().any(|line| line.starts_with(start))
}

#[test]
fn a_changed_function_body_is_patched_in_during_play_with_no_rebuild() {
    let (mut app, workspace, ctx) = hot_patching("hot-body");
    app.play();
    play_for(&mut app, 0.2);
    assert!(app.rebuilds.patched_frames());

    edit_engine(&workspace, "body.y -= 1.0;", "body.y -= 2.0;");
    engine_until(&mut app, &ctx, |app| {
        said(
            app,
            "Engine code changed (`crates/editor/src/main.rs`): only function bodies",
        )
    });
    assert!(app.rebuilds.build.is_none(), "it waits for dx's patch");

    dx_applies_a_patch(&app);
    app.engine_frame(&ctx);
    assert!(said(
        &app,
        "Hot-patched the running editor (`crates/editor/src/main.rs`): the next frame runs \
         the new code, and Play carries on."
    ));
    assert!(app.rebuilds.patched_frames());
    assert!(app.rebuilds.build.is_none() && app.launching.is_none());
    play_for(&mut app, 0.2);
    assert!(app.playing(), "Play keeps running across the patch");

    // Another change to the same body is measured from the patched code.
    edit_engine(&workspace, "body.y -= 2.0;", "body.y -= 3.0;");
    engine_until(&mut app, &ctx, |app| {
        app.rebuilds
            .hot_patch
            .as_ref()
            .is_some_and(|hot| hot.waiting.is_some())
    });
    assert!(app.rebuilds.build.is_none());
    let _ = std::fs::remove_dir_all(&workspace);
}

#[test]
fn changing_a_structs_fields_falls_back_to_rebuild_and_relaunch_and_output_says_so() {
    let (mut app, workspace, ctx) = hot_patching("hot-fields");
    app.play();
    edit_engine(&workspace, "    y: f32,\n", "    y: f32,\n    x: f32,\n");
    edit_engine(&workspace, "Body { y: 0.0 }", "Body { y: 0.0, x: 0.0 }");
    engine_until(&mut app, &ctx, |app| app.rebuilds.ready);

    let warning = app
        .output
        .lines()
        .iter()
        .find(|line| line.text.starts_with("Can't hot-patch this change"))
        .expect("Output says it can't be patched");
    assert_eq!(warning.kind, OutputKind::Warning);
    assert_eq!(
        warning.text,
        "Can't hot-patch this change: `struct Body` changed its definition in \
         `crates/editor/src/main.rs`. The editor keeps running the code it has, and rebuilds \
         and relaunches instead."
    );
    // Phase 25 takes over: the rebuilt editor is offered, and until then the
    // frames don't go through the patch dx made for the new layout.
    assert_eq!(app.toasts.offer.clone().unwrap().button, "Relaunch");
    assert!(!app.rebuilds.patched_frames());
    assert!(app.playing());
    dx_applies_a_patch(&app);
    app.engine_frame(&ctx);
    assert!(!app.rebuilds.patched_frames());
    assert!(!said(&app, "Hot-patched"));
    let _ = std::fs::remove_dir_all(&workspace);
}

#[test]
fn a_patch_that_never_comes_falls_back_to_rebuild_and_relaunch() {
    let (mut app, workspace, ctx) = hot_patching("hot-timeout");
    app.rebuilds.hot_patch.as_mut().unwrap().wait = Duration::from_secs(1);
    edit_engine(&workspace, "body.y -= 1.0;", "body.y -= 2.0;");
    engine_until(&mut app, &ctx, |app| app.rebuilds.ready);
    assert!(said(
        &app,
        "dx sent no patch within 1 s, so the editor rebuilds and relaunches instead."
    ));
    assert_eq!(app.toasts.offer.clone().unwrap().button, "Relaunch");
    // A late patch from those same sources is still taken.
    dx_applies_a_patch(&app);
    app.engine_frame(&ctx);
    assert!(said(&app, "Hot-patched the running editor"));
    assert!(app.rebuilds.patched_frames());
    let _ = std::fs::remove_dir_all(&workspace);
}

#[test]
fn started_normally_the_editor_uses_rebuild_and_relaunch_alone() {
    let mut app = EditorApp::new(Some(&copy_of_example("hello-touched", "hot-off")));
    let ctx = theme::test_context();
    // Tests aren't started by dx, so there's nothing to connect to, with
    // the `hot-patch` feature or without it.
    app.engine_frame(&ctx);
    assert!(app.rebuilds.hot_patch_checked || app.rebuilds.source.is_none());
    assert!(app.rebuilds.hot_patch.is_none());
    assert!(!app.rebuilds.patched_frames());
    assert!(
        app.rebuilds.watcher.is_none(),
        "nothing watched with the setting off"
    );
    assert!(!said(&app, "Hot-patching"));
}
