//! Debug › Graphics Benchmark in the app: it never touches the project,
//! and only Apply changes the editor's graphics settings (Undo puts them
//! back).

use crate::graphics_benchmark::Plan;

use super::*;

#[test]
fn a_run_leaves_the_project_and_settings_alone_until_apply() {
    let folder = super::tests::copy_of_example("hello-touched", "benchmark-run");
    let mut app = EditorApp::new(Some(&folder));
    let parts = app.world.descendants(app.world.workspace()).len();
    let lighting = app.world.descendants(app.world.lighting()).len();
    let before = app.graphics.clone();
    let mut finished = false;
    crate::snapshot::snapshot_rendered(
        &std::env::temp_dir().join(format!("axcenity-benchmark-{}.png", std::process::id())),
        egui::vec2(800.0, 600.0),
        vec![Vec::new(); 3],
        |ui, render_state| {
            if !finished && app.benchmark.run.is_none() && app.benchmark.report.is_none() {
                app.benchmark.open = true;
                app.start_graphics_benchmark(Some(render_state), Plan::QUICK);
                assert!(app.benchmark.run.is_some());
            }
            app.graphics_benchmark_frame(ui.ctx(), Some(render_state));
            finished |= app.benchmark.report.is_some();
        },
    );
    if !finished {
        eprintln!("skipped: no graphics adapter on this machine");
        return;
    }
    // Measured, and nothing else changed.
    assert!(app.benchmark.run.is_none());
    assert_eq!(app.world.descendants(app.world.workspace()).len(), parts);
    assert_eq!(app.world.descendants(app.world.lighting()).len(), lighting);
    assert!(!app.unsaved());
    assert_eq!(app.graphics, before);

    // Apply uses the recommendation (keeping what the benchmark doesn't
    // weigh); Undo puts the old settings back.
    let recommended = app.benchmark.applied(&before).unwrap();
    let report = app.benchmark.report.as_ref().unwrap();
    assert_eq!(recommended.quality, report.recommendation.settings.quality);
    assert_eq!(recommended.realtime_gi, before.realtime_gi);
    app.apply_graphics_benchmark();
    assert_eq!(app.graphics, recommended);
    assert_eq!(app.benchmark.previous, Some(before.clone()));
    assert!(
        !app.unsaved(),
        "graphics settings are the machine's, not the project's"
    );
    if let Some(previous) = app.benchmark.previous.take() {
        app.graphics = previous;
    }
    assert_eq!(app.graphics, before);
}

#[test]
fn the_report_survives_a_restart() {
    let Some(report) = crate::graphics_benchmark::testing::quick_report() else {
        return;
    };
    let mut app = EditorApp::new(None);
    app.benchmark.report = Some(report.clone());
    let mut storage = MemoryStorage::default();
    eframe::App::save(&mut app, &mut storage);
    let mut restarted = EditorApp::new(None);
    assert_eq!(restarted.benchmark.report, None);
    restarted.load_settings(&theme::test_context(), Some(&storage));
    assert_eq!(restarted.benchmark.report, Some(report));
}

/// eframe's saved settings, in memory.
#[derive(Default)]
struct MemoryStorage(std::collections::HashMap<String, String>);

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
fn starting_is_refused_during_play() {
    let folder = super::tests::copy_of_example("hello-touched", "benchmark-play");
    let mut app = EditorApp::new(Some(&folder));
    app.play();
    assert!(app.playing());
    app.start_graphics_benchmark(None, Plan::QUICK);
    assert!(app.benchmark.run.is_none());
}
