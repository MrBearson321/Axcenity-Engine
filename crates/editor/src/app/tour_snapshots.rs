//! Draws the whole editor window (viewport included) in a few states a
//! beginner meets first and saves each as a PNG, for usability reviews
//! (`docs/research/usability/usability-audit.md`). Ignored, since it needs
//! a GPU and only saves pictures:
//!
//! `AXCENITY_SNAPSHOTS=<folder> cargo test -p axcenity-editor editor_tour -- --ignored`
//!
//! `TOUR_ONLY=<part of a name>` draws only the states whose names have it.
//! The examples are copied to a temporary folder first, so nothing in
//! `examples/` changes.

use super::*;
use crate::layout::Pane;

fn copy_example(name: &str) -> PathBuf {
    let from = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .join(name);
    let to = std::env::temp_dir().join(format!("axcenity-tour-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&to);
    copy_dir(&from, &to);
    to
}

fn copy_dir(from: &std::path::Path, to: &std::path::Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            copy_dir(&path, &to.join(entry.file_name()));
        } else {
            std::fs::copy(&path, to.join(entry.file_name())).unwrap();
        }
    }
}

fn shoot(app: &mut EditorApp, name: &str, setup: impl Fn(&mut EditorApp)) {
    let Ok(folder) = std::env::var("AXCENITY_SNAPSHOTS") else {
        return;
    };
    let folder = PathBuf::from(folder);
    std::fs::create_dir_all(&folder).unwrap();
    setup(app);
    crate::snapshot::snapshot_rendered(
        &folder.join(format!("tour-{name}.png")),
        egui::vec2(1600.0, 900.0),
        vec![Vec::new(); 8],
        |ctx, rs| {
            app.frame(ctx, Some(rs));
        },
    );
}

fn find(app: &EditorApp, name: &str) -> InstanceId {
    let root = app.world.workspace();
    app.world.find_first_descendant(root, name).expect(name)
}

fn select(app: &mut EditorApp, id: InstanceId) {
    app.selection = vec![id];
    app.selected = Some(id);
}

fn fresh(example: Option<&str>) -> EditorApp {
    match example {
        None => EditorApp::new(None),
        Some(name) => EditorApp::new(Some(&copy_example(name))),
    }
}

fn first_child(app: &EditorApp) -> InstanceId {
    app.world.get(app.world.workspace()).unwrap().children()[0]
}

#[test]
#[ignore]
fn editor_tour_snapshots() {
    let only = std::env::var("TOUR_ONLY").unwrap_or_default();
    let run = |name: &str, example: Option<&str>, setup: &dyn Fn(&mut EditorApp)| {
        if !only.is_empty() && !name.contains(only.as_str()) {
            return;
        }
        let mut app = fresh(example);
        shoot(&mut app, name, setup);
    };
    run("00-start", None, &|_| {});
    run("01-open", Some("hello-part"), &|_| {});
    run("02-part-selected", Some("hello-part"), &|app| {
        let part = first_child(app);
        select(app, part)
    });
    for tab in [
        crate::ribbon_tabs::BuiltInTab::Model,
        crate::ribbon_tabs::BuiltInTab::World,
        crate::ribbon_tabs::BuiltInTab::Ui,
        crate::ribbon_tabs::BuiltInTab::Script,
        crate::ribbon_tabs::BuiltInTab::Test,
    ] {
        run(
            &format!("03-ribbon-{}", tab.label()),
            Some("hello-part"),
            &|app| app.ribbon_tabs.set_active_for_tour(tab),
        );
    }
    run("04-assets", Some("hello-part"), &|app| {
        app.layout.open(Pane::Assets)
    });
    run("10-anim-empty", Some("hello-animation"), &|app| {
        app.layout.open(Pane::Animation)
    });
    run("11-anim-noodle", Some("hello-animation"), &|app| {
        app.layout.open(Pane::Animation);
        let id = find(app, "Noodle");
        select(app, id)
    });
    run("12-anim-door", Some("hello-door"), &|app| {
        app.layout.open(Pane::Animation);
        let id = find(app, "Door");
        select(app, id)
    });
    run("13-anim-floor", Some("hello-door"), &|app| {
        app.layout.open(Pane::Animation);
        let id = find(app, "Floor");
        select(app, id)
    });
    run("14-animate-floor", Some("hello-door"), &|app| {
        app.layout.open(Pane::Animation);
        let id = find(app, "Floor");
        select(app, id);
        app.animate(id);
    });
}
