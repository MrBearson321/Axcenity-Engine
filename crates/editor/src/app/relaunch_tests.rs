//! Rebuilding the engine in the background and relaunching with the
//! session (unified Phase 25).

use std::time::{Duration, Instant};

use axcenity_core::{Color3, Value};

use super::tests::{copy_of_example, edit_the_pad, part, play_for, printed};
use super::*;
use crate::output::{Location, OutputKind};
use crate::rebuild::EngineSource;

/// Writes `app`'s handoff into a fresh folder, as Relaunch does.
fn hand_off(app: &EditorApp, test: &str) -> PathBuf {
    let folder = std::env::temp_dir().join(format!(
        "axcenity-handoff-{test}-test-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).unwrap();
    app.handoff(&folder).unwrap().write(&folder).unwrap();
    folder
}

/// A new editor carrying on from the handoff in `folder`, as the relaunched
/// process does once its project has opened.
fn relaunched(folder: &Path) -> EditorApp {
    let handoff = Handoff::read(folder).unwrap();
    let project = handoff.project.clone().unwrap();
    let mut app = EditorApp::new(Some(&project));
    app.apply_handoff(handoff, folder);
    app
}

/// The world as a world file, to compare two worlds.
fn world_text(world: &World) -> String {
    axcenity_project::write_world(world)
}

fn distance(a: Vector3, b: Vector3) -> f32 {
    ((a.x - b.x).powi(2) + (a.y - b.y).powi(2) + (a.z - b.z).powi(2)).sqrt()
}

fn position(world: &World, id: InstanceId) -> Vector3 {
    match world.get(id).unwrap().property("Position") {
        Some(Value::Vector3(position)) => *position,
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_relaunch_carries_the_project_camera_selection_and_unsaved_edits() {
    let folder = copy_of_example("hello-touched", "relaunch-edits");
    let world_file = std::fs::read_to_string(folder.join("world.json")).unwrap();
    let mut app = EditorApp::new(Some(&folder));
    let pad = edit_the_pad(&mut app);
    let ball = part(&app, "Ball");
    app.selection = vec![ball, pad];
    app.selected = Some(pad);
    app.viewport
        .set_camera_view(Vector3::new(3.0, 4.0, 5.0), Vector3::new(-20.0, 45.0, 0.0));
    let script = "scripts/pad.server.lua";
    let source = std::fs::read_to_string(folder.join(script)).unwrap();
    app.tabs.open(script, &source, None);
    app.tabs.get_mut(script).unwrap().text = "print('edited, not saved')".to_owned();
    app.layout.toggle(Pane::Profiler);
    assert!(app.unsaved());

    let handoff = hand_off(&app, "edits");
    let resumed = relaunched(&handoff);

    // The same project, with the same unsaved world, still unsaved.
    assert_eq!(
        resumed.project.as_ref().unwrap().folder,
        app.project.as_ref().unwrap().folder
    );
    assert_eq!(world_text(&resumed.world), world_text(&app.world));
    assert!(resumed.unsaved());
    assert_eq!(resumed.history.undo_label(), None);
    // Nothing was written to the project.
    assert_eq!(
        std::fs::read_to_string(folder.join("world.json")).unwrap(),
        world_file
    );
    assert_eq!(
        std::fs::read_to_string(folder.join(script)).unwrap(),
        source
    );
    // The script tab, its unsaved text, and the script it belongs to.
    let tab = resumed.tabs.get(script).unwrap();
    assert_eq!(tab.text, "print('edited, not saved')");
    assert!(tab.unsaved());
    assert!(resumed.tabs.world_shown());
    let mut resumed_tabs = resumed;
    assert_eq!(resumed_tabs.tabs.take_focus(), None);
    let resumed = resumed_tabs;
    let dock = &resumed.layout.dock;
    let egui_dock::TabPath { surface, node, tab } = dock.find_tab(&Pane::Scene).unwrap();
    let leaf = dock[surface][node].get_leaf().unwrap();
    assert_eq!(leaf.active, tab, "the Scene is in front");
    assert!(
        dock.find_tab(&Pane::Script(resumed.tabs.ids()[0]))
            .is_some()
    );
    let scripts = &resumed.project.as_ref().unwrap().scripts;
    assert!(
        scripts
            .iter()
            .all(|script| resumed.world.get(script.instance).is_some())
    );
    // The camera, the selection and the layout.
    let (at, looking) = resumed.viewport.camera_view();
    let (was_at, was_looking) = app.viewport.camera_view();
    assert!(distance(at, was_at) < 1e-4, "{at:?}");
    assert!(distance(looking, was_looking) < 1e-3, "{looking:?}");
    let names: Vec<&str> = resumed
        .selection
        .iter()
        .map(|&id| resumed.world.get(id).unwrap().name())
        .collect();
    assert_eq!(names, ["Ball", "Launch Pad"]);
    assert_eq!(
        resumed
            .selected
            .map(|id| resumed.world.get(id).unwrap().name()),
        Some("Launch Pad")
    );
    let open = |app: &EditorApp| {
        Pane::CLOSABLE
            .into_iter()
            .filter(|&pane| app.layout.is_open(pane))
            .collect::<Vec<_>>()
    };
    assert_ne!(open(&resumed), open(&EditorApp::new(None)));
    assert_eq!(open(&resumed), open(&app));
    assert!(printed(&resumed).iter().any(|line| line.contains(
        "Relaunched with the rebuilt engine. The project and its unsaved edits carried \
         over; the undo history starts fresh."
    )));
    // The new editor deleted the handoff once it read it.
    assert!(!handoff.exists());

    // Saving writes the carried-over world.
    let mut resumed = resumed;
    assert!(resumed.save());
    let saved = axcenity_project::load(&folder).unwrap();
    assert_eq!(world_text(&saved.world), world_text(&app.world));
}

#[test]
fn relaunching_during_play_continues_play_with_positions_and_properties_as_they_were() {
    let folder = copy_of_example("hello-touched", "relaunch-play");
    let mut app = EditorApp::new(Some(&folder));
    app.play();
    play_for(&mut app, 0.4);
    let session = app.play.as_mut().unwrap();
    let game = session.world_mut();
    let pad = game.find_first_child(game.workspace(), "Pad").unwrap();
    let ball = game.find_first_child(game.workspace(), "Ball").unwrap();
    let red = Value::Color3(Color3::new(0.9, 0.1, 0.1));
    game.set_property(pad, "Color", red.clone()).unwrap();
    let falling = position(game, ball);
    let start = position(&app.world, ball);
    assert!(falling.y < start.y - 0.1, "{falling:?} {start:?}");
    app.selection = vec![ball];
    app.selected = Some(ball);

    let handoff = hand_off(&app, "play");
    let mut resumed = relaunched(&handoff);

    assert!(resumed.playing());
    let game = resumed.play.as_ref().unwrap().world();
    let ball = game.find_first_child(game.workspace(), "Ball").unwrap();
    let pad = game.find_first_child(game.workspace(), "Pad").unwrap();
    assert!(distance(position(game, ball), falling) < 1e-3);
    assert_eq!(game.get(pad).unwrap().property("Color"), Some(&red));
    // Matching Instances have the edit world's IDs, as in any Play, so
    // Keep and the selection still work; the edit world is as saved.
    assert_eq!(
        resumed
            .world
            .find_first_child(resumed.world.workspace(), "Ball"),
        Some(ball)
    );
    assert_eq!(resumed.selected, Some(ball));
    assert_eq!(position(&resumed.world, ball), start);
    assert!(!resumed.unsaved());
    assert!(
        printed(&resumed)
            .iter()
            .any(|line| line.starts_with("Play continues in"))
    );

    // The game carries on from there, and Stop goes back to the edit world.
    play_for(&mut resumed, 0.2);
    assert!(resumed.playing());
    let game = resumed.play.as_ref().unwrap().world();
    assert!(position(game, ball).y < falling.y + 1e-3);
    resumed.stop();
    assert_eq!(position(&resumed.world, ball), start);
    assert!(!handoff.exists());
}

#[test]
fn relaunch_needs_a_rebuilt_editor_first() {
    let folder = copy_of_example("hello-touched", "relaunch-refused");
    let mut app = EditorApp::new(Some(&folder));
    app.relaunch();
    assert!(app.launching.is_none(), "nothing was rebuilt yet");
}

/// A Cargo workspace in a temporary folder, with one tiny package named
/// like the editor, whose `main.rs` is `main`.
pub(super) fn tiny_workspace(test: &str, main: &str) -> PathBuf {
    let folder = std::env::temp_dir().join(format!(
        "axcenity-rebuild-{test}-test-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(folder.join("crates/editor/src")).unwrap();
    std::fs::write(
        folder.join("Cargo.toml"),
        "[workspace]\nmembers = [\"crates/editor\"]\nresolver = \"2\"\n",
    )
    .unwrap();
    std::fs::write(
        folder.join("crates/editor/Cargo.toml"),
        "[package]\nname = \"axcenity-editor\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    std::fs::write(folder.join("crates/editor/src/main.rs"), main).unwrap();
    folder
}

/// Runs the editor's frames' engine step until `done`, for up to two minutes.
pub(super) fn engine_until(
    app: &mut EditorApp,
    ctx: &egui::Context,
    done: impl Fn(&EditorApp) -> bool,
) {
    let start = Instant::now();
    while !done(app) && start.elapsed() < Duration::from_secs(120) {
        std::thread::sleep(Duration::from_millis(50));
        app.engine_frame(ctx);
    }
    assert!(done(app), "timed out");
}

#[test]
fn a_compile_error_is_listed_with_links_and_fixing_it_rebuilds_and_offers_relaunch() {
    let workspace = tiny_workspace(
        "errors",
        "fn main() {\n    let x: u32 = \"not a number\";\n}\n",
    );
    let ctx = theme::test_context();
    let mut app = EditorApp::new(Some(&copy_of_example("hello-touched", "rebuild-errors")));
    app.rebuilds = Rebuilds {
        source: EngineSource::at(&workspace),
        on_change: true,
        ..Rebuilds::default()
    };
    let world = world_text(&app.world);
    // The first frame starts watching; a change is needed to build, so build now.
    app.engine_frame(&ctx);
    assert!(app.rebuilds.watcher.is_some());
    app.start_engine_build(&ctx);
    assert!(app.engine_busy().is_some());
    engine_until(&mut app, &ctx, |app| app.rebuilds.build.is_none());

    let errors: Vec<&crate::output::OutputLine> = app
        .output
        .lines()
        .iter()
        .filter(|line| line.kind == OutputKind::Error)
        .collect();
    assert!(
        errors.iter().any(|line| line.location
            == Some(Location {
                path: "crates/editor/src/main.rs".to_owned(),
                line: 2,
            })
            && line.text.contains("mismatched types")),
        "{errors:?}"
    );
    assert!(errors.iter().any(|line| {
        line.text
            .starts_with("The engine didn't build: 1 error. The editor carries on as it was")
    }));
    // The editor is as it was, and there's nothing to relaunch into.
    assert!(!app.rebuilds.ready);
    assert!(app.toasts.offer.is_none());
    assert_eq!(world_text(&app.world), world);
    assert!(app.project.is_some());
    app.relaunch();
    assert!(app.launching.is_none());

    // Fixing the error: the watcher notices, it builds again, and it works.
    std::fs::write(
        workspace.join("crates/editor/src/main.rs"),
        "fn main() {\n    let _x: u32 = 1;\n}\n",
    )
    .unwrap();
    engine_until(&mut app, &ctx, |app| app.rebuilds.ready);
    let offer = app.toasts.offer.clone().unwrap();
    assert_eq!(offer.button, "Relaunch");
    let built = EngineSource::at(&workspace).unwrap().built_editor();
    assert!(built.is_file(), "{}", built.display());
    assert!(app.rebuilds.expected > 0);
    assert!(
        printed(&app)
            .iter()
            .any(|line| line.starts_with("Engine code changed (`crates/editor/src/main.rs`)"))
    );
    let _ = std::fs::remove_dir_all(&workspace);
}

/// eframe's saved settings, in memory.
#[derive(Default)]
struct MemoryStorage(HashMap<String, String>);

impl eframe::Storage for MemoryStorage {
    fn get_string(&self, key: &str) -> Option<String> {
        self.0.get(key).cloned()
    }
    fn set_string(&mut self, key: &str, value: String) {
        self.0.insert(key.to_owned(), value);
    }
    fn remove_string(&mut self, key: &str) {
        self.0.remove(key);
    }
    fn flush(&mut self) {}
}

#[test]
fn only_an_engine_dev_editor_rebuilds_and_relaunches() {
    // Phase 89: a creator's editor never watches `crates/`, so it shows no
    // Engine settings or menu items.
    let app = EditorApp::new(None);
    assert_eq!(app.rebuilds.source.is_some(), cfg!(feature = "engine-dev"));
}

#[test]
fn a_debug_editor_has_the_engine_settings_and_they_survive_a_restart() {
    let ctx = theme::test_context();
    let mut app = EditorApp::new(None);
    app.rebuilds.source = EngineSource::find();
    assert!(app.rebuilds.source.is_some());
    app.rebuilds.on_change = true;
    app.rebuilds.relaunch_automatically = true;
    app.rebuilds.expected = 412;
    let mut storage = MemoryStorage::default();
    eframe::App::save(&mut app, &mut storage);

    let mut restarted = EditorApp::new(None);
    assert!(!restarted.rebuilds.on_change);
    restarted.load_settings(&ctx, Some(&storage));
    assert!(restarted.rebuilds.on_change);
    assert!(restarted.rebuilds.relaunch_automatically);
    assert_eq!(restarted.rebuilds.expected, 412);
}
