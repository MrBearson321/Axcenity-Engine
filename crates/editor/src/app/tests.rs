use axcenity_core::{ClassName, Color3, Material, PartType, Value, Vector3};
use axcenity_render::glam::Vec3;

use crate::gizmo::Pose;
use crate::output::{OutputKind, OutputLine};

use super::*;

pub(super) fn example(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .join(name)
}

pub(super) fn last_line(app: &EditorApp) -> &OutputLine {
    app.output.lines().last().unwrap()
}

/// A project folder in the temp directory with these files in it.
fn temp_project(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let folder = std::env::temp_dir().join(format!(
        "axcenity-editor-{name}-test-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).unwrap();
    for (file, text) in files {
        std::fs::write(folder.join(file), text).unwrap();
    }
    folder
}

#[test]
fn opens_the_milestone_1_demo_and_says_so_in_output() {
    let app = EditorApp::new(Some(&example("hello-touched")));

    let workspace = app.world.workspace();
    for part in ["Floor", "Pad", "Ball"] {
        assert!(app.world.find_first_child(workspace, part).is_some());
    }
    assert_eq!(app.title(), "Hello Touched - Axcenity Editor");
    let line = last_line(&app);
    assert_eq!(line.kind, OutputKind::Info);
    assert!(
        line.text.starts_with("Opened \"Hello Touched\" from")
            && line.text.contains(": 4 objects, 1 script."),
        "{}",
        line.text
    );
}

/// The graphics reference scene opens with its textured model and
/// SurfaceAppearance maps, and drawing it finds no broken asset.
#[test]
fn opens_the_graphics_reference_scene_with_no_asset_errors() {
    let app = EditorApp::new(Some(&example("hello-graphics")));
    assert!(
        app.output
            .lines()
            .iter()
            .all(|line| line.kind != OutputKind::Error),
        "{:?}",
        app.output.lines()
    );
    let camera = axcenity_render::Camera {
        position: Vec3::new(12.0, 8.0, 16.0),
        look_at: Vec3::ZERO,
        fov_y_degrees: 70.0,
    };
    let list = axcenity_scene::build_draw_list(&app.world, &app.assets, camera);
    assert!(app.assets.take_errors().is_empty());
    let textured = list
        .items
        .iter()
        .filter(|item| item.material.base_color_texture.is_some())
        .count();
    // The PBR panel and the tiled SurfaceAppearance block.
    assert!(textured >= 2, "{textured} textured items");
    assert_eq!(list.lights.len(), 2);
}

#[test]
fn unsupported_css_properties_are_reported_as_output_warnings() {
    let manifest = format!(
        r#"{{"name":"CSS","format_version":{}}}"#,
        axcenity_project::FORMAT_VERSION
    );
    let folder = temp_project(
        "unsupported-css",
        &[
            ("axcenity.json", &manifest),
            ("world.json", r#"{"format_version":1}"#),
            ("ui.css", ".hud { animation: fade 1s; opacity: .5 }"),
        ],
    );
    let app = EditorApp::new(Some(&folder));
    let warning = app
        .output
        .lines()
        .iter()
        .find(|line| line.kind == OutputKind::Warning)
        .expect("unknown CSS properties should be reported");

    assert!(warning.text.contains("ui.css"));
    assert!(
        warning
            .text
            .contains("unsupported CSS property `animation`")
    );
    drop(app);
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn stylesheet_edits_restyle_the_viewport_immediately_and_save() {
    let folder = copy_of_example("hello-css", "stylesheet-live-edit");
    let mut app = EditorApp::new(Some(&folder));
    let path = "ui.css";
    let original = std::fs::read_to_string(folder.join(path)).unwrap();
    let updated = original.replace("width: 280px", "width: 320px");
    assert_ne!(original, updated);
    app.tabs.open(path, &original, None);

    let hud = app
        .world
        .find_first_child(app.world.workspace(), "Hud")
        .unwrap();
    let panel = app.world.find_first_child(hud, "HudPanel").unwrap();
    let layout = |app: &EditorApp| {
        axcenity_ui::layout_ui(
            &app.world,
            800,
            600,
            &app.project.as_ref().unwrap().stylesheets,
        )
        .element(panel)
        .unwrap()
        .rect
        .width
    };
    assert_eq!(layout(&app), 280.0);

    app.tabs.get_mut(path).unwrap().text = updated.clone();
    app.refresh_stylesheet(path, &updated);
    assert_eq!(layout(&app), 320.0);
    assert!(app.unsaved());
    assert!(app.save());
    assert_eq!(std::fs::read_to_string(folder.join(path)).unwrap(), updated);
    assert!(!app.unsaved());
    drop(app);
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn the_style_section_rewrites_and_adds_css_for_the_selected_element() {
    let folder = copy_of_example("hello-css", "style-section");
    let mut app = EditorApp::new(Some(&folder));
    let hud = app
        .world
        .find_first_child(app.world.workspace(), "Hud")
        .unwrap();
    let panel = app.world.find_first_child(hud, "HudPanel").unwrap();
    let width = |app: &EditorApp| {
        axcenity_ui::layout_ui(
            &app.world,
            800,
            600,
            &app.project.as_ref().unwrap().stylesheets,
        )
        .element(panel)
        .unwrap()
        .rect
        .width
    };
    let declaration = |app: &EditorApp, property: &str| {
        axcenity_ui::applied_declaration(
            &app.world,
            panel,
            &app.project.as_ref().unwrap().stylesheets,
            property,
        )
    };

    // Changing a value rewrites the rule that set it, live.
    let applied = declaration(&app, "width").unwrap();
    app.apply_style(
        panel,
        StyleEdit::Set {
            path: applied.path,
            span: applied.span,
            value: "300px".to_owned(),
        },
    );
    assert_eq!(width(&app), 300.0);
    assert!(
        app.tabs
            .get("ui.css")
            .unwrap()
            .text
            .contains("width: 300px;")
    );

    // An element with no Id gets one from its name, and its own rule.
    app.apply_style(
        panel,
        StyleEdit::Add {
            property: "opacity".to_owned(),
            value: "0.5".to_owned(),
        },
    );
    let id = app.world.get(panel).unwrap().property("Id").cloned();
    assert_eq!(id, Some(Value::String("HudPanel".to_owned())));
    let added = declaration(&app, "opacity").unwrap();
    assert_eq!(
        (added.selector.as_str(), added.value.as_str()),
        ("#HudPanel", "0.5")
    );
    // A second style joins the same rule.
    app.apply_style(
        panel,
        StyleEdit::Add {
            property: "border-radius".to_owned(),
            value: "4px".to_owned(),
        },
    );
    let text = &app.tabs.get("ui.css").unwrap().text;
    assert_eq!(text.matches("#HudPanel {").count(), 1, "{text}");
    assert!(app.unsaved());
    drop(app);
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn a_stylesheet_edited_in_another_program_restyles_the_viewport() {
    let folder = copy_of_example("hello-css", "stylesheet-outside-edit");
    let mut app = EditorApp::new(Some(&folder));
    let hud = app
        .world
        .find_first_child(app.world.workspace(), "Hud")
        .unwrap();
    let panel = app.world.find_first_child(hud, "HudPanel").unwrap();
    let width = |app: &EditorApp| {
        axcenity_ui::layout_ui(
            &app.world,
            800,
            600,
            &app.project.as_ref().unwrap().stylesheets,
        )
        .element(panel)
        .unwrap()
        .rect
        .width
    };
    assert_eq!(width(&app), 280.0);
    let path = folder.join("ui.css");
    let edited = std::fs::read_to_string(&path)
        .unwrap()
        .replace("width: 280px", "width: 340px");
    std::fs::write(&path, &edited).unwrap();
    app.disk_changed(&["ui.css".to_owned()]);
    assert_eq!(width(&app), 340.0);
    assert!(!app.unsaved(), "nothing to save: the file already has it");
    drop(app);
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn ui_elements_inserted_with_no_screen_gui_get_one() {
    let folder = copy_of_example("hello-part", "ui-insert");
    let mut app = EditorApp::new(Some(&folder));
    app.insert(Insertable::TextButton, None);
    let button = app.selected.unwrap();
    let parent = app.world.get(button).unwrap().parent().unwrap();
    assert_eq!(app.world.get(parent).unwrap().class(), ClassName::ScreenGui);
    assert_eq!(
        app.world.get(button).unwrap().class(),
        ClassName::TextButton
    );
    drop(app);
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn the_whole_editor_draws_in_each_theme_and_density() {
    let snapshots = std::env::var_os("AXCENITY_SNAPSHOTS").map(PathBuf::from);
    for (name, appearance) in [
        ("editor-dark", Appearance::default()),
        (
            "editor-light",
            Appearance {
                theme: theme::ThemeChoice::Light,
                ..Appearance::default()
            },
        ),
        (
            "editor-compact",
            Appearance {
                density: theme::Density::Compact,
                accent: theme::Accent::Violet,
                ..Appearance::default()
            },
        ),
    ] {
        let mut app = EditorApp::new(Some(&example("hello-touched")));
        let pad = app
            .world
            .get(app.world.workspace())
            .and_then(|workspace| workspace.children().last().copied())
            .expect("the example has parts");
        app.select_clicked(Some(pad), egui::Modifiers::NONE, None, false);
        app.output.warning("A warning, to see how warnings look.");
        app.output
            .error("scripts/Pad.server.lua:3: an error, to see how errors look");
        let mut draw = |ctx: &egui::Context| {
            theme::apply(ctx, &appearance);
            app.frame(ctx, None);
        };
        match &snapshots {
            Some(folder) => crate::snapshot::snapshot(
                &folder.join(format!("{name}.png")),
                egui::vec2(1440.0, 900.0),
                vec![Vec::new(); 3],
                &mut draw,
            ),
            None => {
                let ctx = theme::test_context();
                for _ in 0..3 {
                    let _ = ctx.run(Default::default(), &mut draw);
                }
            }
        }
        assert_eq!(app.selected, Some(pad), "drawing keeps the selection");
    }
}

/// Each panel, drawn in a column 260 points wide and in a strip 260
/// tall (inside the dock's tab margins), keeps everything inside it.
/// With `AXCENITY_SNAPSHOTS` set, each is saved as `panel-<name>-tall.png`
/// and `panel-<name>-wide.png`.
#[test]
fn every_panel_fits_a_narrow_column_and_a_short_strip() {
    let snapshots = std::env::var_os("AXCENITY_SNAPSHOTS").map(PathBuf::from);
    let mut app = EditorApp::new(Some(&example("hello-touched")));
    let pad = app
        .world
        .get(app.world.workspace())
        .and_then(|workspace| workspace.children().last().copied())
        .expect("the example has parts");
    app.select_clicked(Some(pad), egui::Modifiers::NONE, None, false);
    app.output.error(
        "scripts/Pad.server.lua:3: an error with a long message, to see it wrap in a \
         narrow column instead of running past it",
    );
    // A new tree, so the Behavior Tree tab shows its graph and a problem.
    app.behavior.create("assets/ai/Fit.behaviortree.json");
    // A terrain with a tool picked, so the Terrain Studio shows its forms.
    let terrain = app.world.create(ClassName::Terrain).unwrap();
    app.world
        .set_parent(terrain, Some(app.world.workspace()))
        .unwrap();
    app.terrain.tool = Some(crate::terrain_studio::TerrainTool::Generate);
    // A few frames of readings, so the Profiler shows all of them.
    app.profiler.start();
    for _ in 0..3 {
        app.profiler
            .record_game(Default::default(), Default::default());
    }

    for pane in Pane::CLOSABLE {
        for (size, shape) in [
            (egui::vec2(260.0, 900.0), "tall"),
            (egui::vec2(900.0, 260.0), "wide"),
        ] {
            // What was drawn, and the room there was for it.
            let mut fit = None;
            let mut draw = |ctx: &egui::Context| {
                let palette = theme::palette(ctx);
                let frame = egui::Frame::new()
                    .fill(palette.panel)
                    .inner_margin(egui::Margin::symmetric(8, 6));
                egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
                    let room = ui.max_rect();
                    match pane {
                        Pane::Explorer => app.explorer_pane(ui),
                        Pane::Properties => app.properties_pane(ui),
                        Pane::Console => app.console_pane(ui),
                        Pane::Assets => app.assets_pane(ui),
                        Pane::Animation => app.animation_pane(ui, None),
                        Pane::UiDesigner => app.ui_designer_pane(ui, None),
                        Pane::Terrain => app.terrain_pane(ui),
                        Pane::Profiler => app.profiler.show(ui),
                        Pane::AssetCheck => {
                            let _ = app.asset_check.show(ui, app.project.is_some(), true);
                        }
                        Pane::BehaviorTree => app.behavior_tree_pane(ui),
                        Pane::Scene | Pane::Script(_) => {}
                    }
                    fit = Some((ui.min_rect(), room));
                });
            };
            let ctx = theme::test_context();
            for _ in 0..3 {
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    ..Default::default()
                };
                let _ = ctx.run(input, &mut draw);
            }
            if let Some(folder) = &snapshots {
                let name = pane.label().to_lowercase();
                crate::snapshot::snapshot(
                    &folder.join(format!("panel-{name}-{shape}.png")),
                    size,
                    vec![Vec::new(); 3],
                    &mut draw,
                );
            }
            let (used, room) = fit.expect("the panel was drawn");
            assert!(
                used.width() <= room.width() + 0.5,
                "{pane:?} at {size:?} drew {} points wide in {}",
                used.width(),
                room.width()
            );
            assert!(
                used.height() <= room.height() + 0.5,
                "{pane:?} at {size:?} drew {} points tall in {}",
                used.height(),
                room.height()
            );
        }
    }
}

/// Draws the Animation tab three times at `size`, and returns what was
/// drawn and the room there was for it.
fn draw_animation_tab(app: &mut EditorApp, size: egui::Vec2) -> (egui::Rect, egui::Rect) {
    let mut fit = None;
    let ctx = theme::test_context();
    for _ in 0..3 {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let room = ui.max_rect();
                app.animation_pane(ui, None);
                fit = Some((ui.min_rect(), room));
            });
        });
    }
    fit.expect("the tab was drawn")
}

/// The last part in the example's Workspace, selected.
fn select_pad(app: &mut EditorApp) -> InstanceId {
    let pad = app
        .world
        .get(app.world.workspace())
        .and_then(|workspace| workspace.children().last().copied())
        .expect("the example has parts");
    app.select_clicked(Some(pad), egui::Modifiers::NONE, None, false);
    pad
}

#[test]
fn animate_makes_an_animator_a_clip_and_an_animation_as_one_undo_step() {
    let folder = copy_of_example("hello-touched", "animate");
    let mut app = EditorApp::new(Some(&folder));
    let pad = select_pad(&mut app);
    let name = app.world.get(pad).unwrap().name().to_owned();
    let safe = axcenity_anim::files::file_safe(&name);

    app.animate(pad);
    let subject = crate::animation::Subject::of(&app.world, pad).unwrap();
    let animator = subject.animator.expect("an Animator was added");
    let [animation] = subject.animations.as_slice() else {
        panic!("one Animation: {:?}", subject.animations);
    };
    let clip = format!("assets/animations/{safe}/{safe}.anim.json");
    assert_eq!(animation.clip, clip);
    assert_eq!(animation.name, safe);
    assert_eq!(
        app.world.get(animation.id).unwrap().parent(),
        Some(animator)
    );
    assert!(folder.join(&clip).is_file());
    assert!(app.animation.recording(), "it opens with Record on");
    assert_eq!(
        app.animation.open_documents()[0].as_deref(),
        Some(clip.as_str())
    );
    assert_eq!(
        app.history.undo_label(),
        Some(format!("Animate {name}").as_str())
    );

    // Again: another clip and Animation in the same Animator.
    app.animate(pad);
    let subject = crate::animation::Subject::of(&app.world, pad).unwrap();
    assert_eq!(subject.animator, Some(animator));
    assert_eq!(subject.animations.len(), 2);
    assert_eq!(
        subject.animations[1].clip,
        format!("assets/animations/{safe}/{safe}2.anim.json")
    );

    // Undo takes back the Animation, the Animator and the file.
    app.undo();
    app.undo();
    assert!(crate::animation::animator_of(&app.world, pad).is_none());
    assert!(!folder.join(&clip).exists());
    app.redo();
    assert!(folder.join(&clip).is_file());
    let subject = crate::animation::Subject::of(&app.world, pad).unwrap();
    assert_eq!(subject.animator, Some(animator));
    assert_eq!(subject.animations.len(), 1);
    let _ = std::fs::remove_dir_all(&folder);
}

/// The breadcrumb names the object and what's being edited, and the tab
/// fits 600×400 and a 260×900 column with a clip or a state machine
/// shown. With `AXCENITY_SNAPSHOTS` set, each is saved as
/// `animation-<view>-<shape>.png`.
#[test]
fn the_animation_tab_says_where_you_are_and_fits_any_shape() {
    let snapshots = std::env::var_os("AXCENITY_SNAPSHOTS").map(PathBuf::from);
    let folder = copy_of_example("hello-touched", "animation-tab");
    let mut app = EditorApp::new(Some(&folder));
    let pad = select_pad(&mut app);
    let name = app.world.get(pad).unwrap().name().to_owned();
    let safe = axcenity_anim::files::file_safe(&name);
    assert_eq!(app.animation.breadcrumb_text(&app.world), "Animation");

    app.animate(pad);
    let clip = format!("assets/animations/{safe}/{safe}.anim.json");
    assert_eq!(
        app.animation.breadcrumb_text(&app.world),
        format!("{name} ▸ {safe}")
    );

    // A state machine playing the clip, given to its Animator.
    let machine =
        crate::animation::store::new_controller(&folder, &safe, std::slice::from_ref(&clip))
            .unwrap()
            .path;
    let animator = crate::animation::animator_of(&app.world, pad).unwrap();
    app.set_controller(pad, Some(animator), machine.clone());
    app.animation
        .open_controller(&folder, &app.assets, &machine, None, &mut app.output);
    assert_eq!(
        app.animation.breadcrumb_text(&app.world),
        format!("{name} ▸ State Machine")
    );

    for view in ["state-machine", "clip"] {
        if view == "clip" {
            // Double-clicking the state opens its clip, under it.
            app.animation
                .open_state(&folder, &app.assets, &safe, &mut app.world, &mut app.output);
            assert_eq!(
                app.animation.breadcrumb_text(&app.world),
                format!("{name} ▸ State Machine ▸ {safe}")
            );
        }
        for (size, shape) in [
            (egui::vec2(600.0, 400.0), "600x400"),
            (egui::vec2(260.0, 900.0), "tall"),
            (egui::vec2(1100.0, 600.0), "wide"),
        ] {
            let (used, room) = draw_animation_tab(&mut app, size);
            assert!(
                used.width() <= room.width() + 0.5 && used.height() <= room.height() + 0.5,
                "{view} at {size:?} drew {used:?} in {room:?}"
            );
            if let Some(snapshots) = &snapshots {
                crate::snapshot::snapshot(
                    &snapshots.join(format!("animation-{view}-{shape}.png")),
                    size,
                    vec![Vec::new(); 3],
                    &mut |ctx: &egui::Context| {
                        egui::CentralPanel::default().show(ctx, |ui| app.animation_pane(ui, None));
                    },
                );
            }
        }
    }
    assert_eq!(
        app.animation.open_documents(),
        [Some(clip), Some(machine)],
        "the clip and the state machine stay open together"
    );

    // The breadcrumb goes back up to the graph.
    app.animation.go_to_state_machine();
    assert_eq!(
        app.animation.breadcrumb_text(&app.world),
        format!("{name} ▸ State Machine")
    );
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn starting_without_a_project_shows_nothing_open() {
    let app = EditorApp::new(None);
    assert_eq!(app.title(), "Axcenity Editor");
    assert!(app.output.lines().is_empty());
    // Only `game`'s services.
    let world = &app.world;
    let descendants = world.descendants(world.root());
    assert!(
        descendants
            .iter()
            .all(|&id| world.get(id).unwrap().class().is_service())
    );
}

#[test]
fn closing_a_project_returns_to_the_start_screen() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-part");
    let mut app = EditorApp::new(Some(&root));
    assert!(app.project.is_some());
    app.finish_closing_project();
    assert!(app.project.is_none());
    assert_eq!(app.title(), "Axcenity Editor");
    // It stays in the project list.
    assert_eq!(app.recent.len(), 1);
}

/// Draws frames until the project opening in the background is read.
fn draw_until_loaded(app: &mut EditorApp, ctx: &egui::Context) {
    let deadline = Instant::now() + std::time::Duration::from_secs(30);
    while app.loading.is_some() {
        assert!(Instant::now() < deadline, "loading never finished");
        let _ = ctx.run(Default::default(), |ctx| app.frame(ctx, None));
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

#[test]
fn a_project_opened_in_the_background_opens_once_it_is_read() {
    let ctx = theme::test_context();
    let mut app = EditorApp::new(None);
    app.begin_open(&ctx, &example("hello-touched"));
    assert_eq!(app.loading.as_ref().unwrap().name, "Hello Touched");
    assert!(app.project.is_none(), "nothing opens before it's read");

    draw_until_loaded(&mut app, &ctx);
    assert_eq!(app.title(), "Hello Touched - Axcenity Editor");
    assert!(last_line(&app).text.starts_with("Opened \"Hello Touched\""));
    // The loading screen stays up while the scene first draws, then goes.
    assert!(app.cover_until.is_some());
    for _ in 0..FINISHING_FRAMES {
        let _ = ctx.run(Default::default(), |ctx| app.frame(ctx, None));
    }
    assert_eq!(app.cover_until, None);
}

#[test]
fn a_folder_that_fails_to_open_in_the_background_is_reported() {
    let ctx = theme::test_context();
    let mut app = EditorApp::new(None);
    app.begin_open(&ctx, &example("no-such-project"));
    draw_until_loaded(&mut app, &ctx);
    assert!(app.project.is_none());
    let line = last_line(&app);
    assert_eq!(line.kind, OutputKind::Error);
    assert!(line.text.contains("axcenity.json"), "{}", line.text);
}

#[test]
fn a_missing_project_folder_is_reported_in_output() {
    let app = EditorApp::new(Some(&example("no-such-project")));
    let line = last_line(&app);
    assert_eq!(line.kind, OutputKind::Error);
    assert!(
        line.text.starts_with("Could not open the project in")
            && line.text.contains("axcenity.json"),
        "{}",
        line.text
    );
}

#[test]
fn a_broken_world_file_is_reported_with_its_line_and_the_rest_still_opens() {
    let manifest = format!(
        r#"{{ "name": "Broken", "format_version": {} }}"#,
        axcenity_project::FORMAT_VERSION
    );
    let folder = temp_project(
        "broken-world",
        &[
            ("axcenity.json", &manifest),
            (
                "world.json",
                "{\n  \"format_version\": 1,\n  \"workspace\": oops\n}\n",
            ),
        ],
    );
    let mut app = EditorApp::new(Some(&example("hello-touched")));
    app.open(&folder);

    let error = app
        .output
        .lines()
        .iter()
        .rfind(|line| line.kind == OutputKind::Error)
        .unwrap();
    assert!(
        error.text.contains("world.json") && error.text.contains("line 3"),
        "{}",
        error.text
    );
    // The project opened anyway, with an empty Workspace.
    assert_eq!(app.title(), "Broken - Axcenity Editor");
    assert!(last_line(&app).text.contains("The Workspace is empty"));
    let workspace = app.world.get(app.world.workspace()).unwrap();
    assert!(workspace.children().is_empty());
    let _ = std::fs::remove_dir_all(&folder);
}

/// A copy of an example project in the temp directory, safe to save over.
pub(super) fn copy_of_example(name: &str, test: &str) -> PathBuf {
    fn copy_folder(from: &Path, to: &Path) {
        std::fs::create_dir_all(to).unwrap();
        for entry in std::fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            let target = to.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy_folder(&entry.path(), &target);
            } else {
                std::fs::copy(entry.path(), target).unwrap();
            }
        }
    }
    let folder = std::env::temp_dir().join(format!(
        "axcenity-editor-{test}-test-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&folder);
    copy_folder(&example(name), &folder);
    folder
}

pub(super) fn part(app: &EditorApp, name: &str) -> InstanceId {
    app.world
        .find_first_child(app.world.workspace(), name)
        .unwrap()
}

pub(super) fn vector(x: f32, y: f32, z: f32) -> Value {
    Value::Vector3(Vector3::new(x, y, z))
}

/// The edits the criteria name: Position, Size, Color, Material and
/// Anchored, plus a new name. Odd numbers check nothing gets rounded.
pub(super) fn edit_the_pad(app: &mut EditorApp) -> InstanceId {
    let pad = part(app, "Pad");
    let edits = [
        Edit::Name("Launch Pad".to_string()),
        Edit::Property("Position", vector(1.25, 0.3, -7.1)),
        Edit::Property("Size", vector(3.3, 0.001, 12.0)),
        Edit::Property("Color", Value::Color3(Color3::new(0.1, 0.725, 0.333))),
        Edit::Property("Material", Value::Material(Material::Neon)),
        Edit::Property("Anchored", Value::Bool(false)),
    ];
    for edit in edits {
        app.apply(pad, edit);
    }
    pad
}

#[test]
fn selecting_a_part_in_the_viewport_highlights_it_in_the_explorer_too() {
    // A part two levels down, inside a closed branch.
    let mut app = EditorApp::new(Some(&example("hello-touched")));
    let pad = part(&app, "Pad");
    let child = app.world.create(ClassName::Part).unwrap();
    app.world.set_parent(child, Some(pad)).unwrap();
    assert!(!app.explorer.rows(&app.world).iter().any(|r| r.id == child));

    app.select_from_viewport(Some(child));
    assert_eq!(app.selected, Some(child));
    // The Explorer highlights the row matching `selected`, and it's visible.
    assert!(app.explorer.rows(&app.world).iter().any(|r| r.id == child));

    // A click on empty space clears the selection.
    app.select_from_viewport(None);
    assert_eq!(app.selected, None);
}

#[test]
fn edits_show_in_the_next_draw_list_and_count_as_unsaved() {
    let mut app = EditorApp::new(Some(&example("hello-touched")));
    assert!(!app.unsaved());
    let pad = edit_the_pad(&mut app);
    assert!(app.unsaved());
    assert_eq!(app.title(), "Hello Touched* - Axcenity Editor");

    let instance = app.world.get(pad).unwrap();
    assert_eq!(instance.name(), "Launch Pad");
    assert_eq!(instance.property("Anchored"), Some(&Value::Bool(false)));
    // The viewport draws from the world each frame, so the next draw list
    // already has the new position, size and neon glow.
    let camera = axcenity_render::Camera {
        position: Vec3::new(0.0, 50.0, 50.0),
        look_at: Vec3::ZERO,
        fov_y_degrees: 70.0,
    };
    let list = axcenity_scene::build_draw_list(&app.world, &app.assets, camera);
    let drawn_pad = list
        .items
        .iter()
        .find(|item| {
            let boosted = [0.25, 1.8125, 0.8325];
            item.material
                .emissive
                .iter()
                .zip(boosted)
                .all(|(a, b)| (a - b).abs() < 1e-5)
        })
        .expect("the pad glows in its new color");
    let center = drawn_pad.transform.transform_point3(Vec3::ZERO);
    assert!(center.abs_diff_eq(Vec3::new(1.25, 0.3, -7.1), 1e-6));
    let corner = drawn_pad.transform.transform_point3(Vec3::splat(0.5));
    assert!((corner - center).abs_diff_eq(Vec3::new(-6.0, 0.0005, 1.65), 1e-6));
}

#[test]
fn setting_a_value_it_already_has_is_not_an_unsaved_change() {
    let mut app = EditorApp::new(Some(&example("hello-touched")));
    let pad = part(&app, "Pad");
    app.apply(pad, Edit::Name("Pad".to_string()));
    app.apply(pad, Edit::Property("Anchored", Value::Bool(true)));
    assert!(!app.unsaved());
}

#[test]
fn scripts_cannot_be_renamed_from_properties() {
    let mut app = EditorApp::new(Some(&example("hello-touched")));
    let script = app
        .world
        .find_first_child(app.world.server_script_service(), "pad")
        .unwrap();
    app.apply(script, Edit::Name("door".to_string()));
    assert_eq!(app.world.get(script).unwrap().name(), "pad");
    assert!(!app.unsaved());
}

#[test]
fn save_close_and_reopen_reproduces_the_edits_exactly() {
    let folder = copy_of_example("hello-touched", "save-reopen");
    let mut app = EditorApp::new(Some(&folder));
    edit_the_pad(&mut app);
    assert!(app.save());
    assert!(!app.unsaved());
    assert_eq!(app.title(), "Hello Touched - Axcenity Editor");
    assert!(last_line(&app).text.starts_with("Saved \"Hello Touched\""));
    let saved = axcenity_project::write_world(&app.world);
    drop(app);

    let reopened = EditorApp::new(Some(&folder));
    // Same world file text means every Instance, name and property
    // value came back exactly, down to the last bit of every number.
    assert_eq!(axcenity_project::write_world(&reopened.world), saved);
    let pad = part(&reopened, "Launch Pad");
    let pad = reopened.world.get(pad).unwrap();
    assert_eq!(pad.property("Position"), Some(&vector(1.25, 0.3, -7.1)));
    assert_eq!(
        pad.property("Color"),
        Some(&Value::Color3(Color3::new(0.1, 0.725, 0.333)))
    );
    assert_eq!(
        pad.property("Material"),
        Some(&Value::Material(Material::Neon))
    );
    // The script file is left alone and still loads.
    assert!(
        reopened
            .world
            .find_first_child(reopened.world.server_script_service(), "pad")
            .is_some()
    );
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn closing_with_unsaved_changes_asks_first() {
    let mut app = EditorApp::new(Some(&example("hello-touched")));
    // Nothing to lose: go straight ahead.
    assert_eq!(app.request(Discard::Quit), Some(Discard::Quit));
    assert_eq!(app.asking, None);

    edit_the_pad(&mut app);
    assert_eq!(app.request(Discard::Quit), None);
    assert_eq!(app.asking, Some(Discard::Quit));
    // Cancel keeps the editor open with the changes.
    assert_eq!(app.answer(Answer::Cancel), None);
    assert_eq!(app.asking, None);
    assert!(app.unsaved());
    // Don't Save goes ahead without saving.
    app.request(Discard::OpenAnother);
    assert_eq!(app.answer(Answer::DontSave), Some(Discard::OpenAnother));
    assert!(app.unsaved());
}

#[test]
fn save_in_the_prompt_saves_then_goes_ahead_but_not_if_saving_fails() {
    let folder = copy_of_example("hello-touched", "prompt-save");
    let mut app = EditorApp::new(Some(&folder));
    edit_the_pad(&mut app);
    app.request(Discard::Quit);
    assert_eq!(app.answer(Answer::Save), Some(Discard::Quit));
    assert!(!app.unsaved());
    assert!(part_exists_on_disk(&folder, "Launch Pad"));

    // With the folder gone, saving fails: say why and stay open.
    std::fs::remove_dir_all(&folder).unwrap();
    app.apply(part(&app, "Floor"), Edit::Name("Ground".to_string()));
    app.request(Discard::Quit);
    assert_eq!(app.answer(Answer::Save), None);
    assert!(app.unsaved());
    let line = last_line(&app);
    assert_eq!(line.kind, OutputKind::Error);
    assert!(line.text.starts_with("Could not save"), "{}", line.text);
}

pub(super) fn part_exists_on_disk(folder: &Path, name: &str) -> bool {
    let world = axcenity_project::load(folder).unwrap().world;
    world.find_first_child(world.workspace(), name).is_some()
}

pub(super) fn property(app: &EditorApp, id: InstanceId, name: &str) -> Value {
    app.world.get(id).unwrap().property(name).unwrap().clone()
}

fn name(app: &EditorApp, id: InstanceId) -> &str {
    app.world.get(id).unwrap().name()
}

fn children(app: &EditorApp, id: InstanceId) -> Vec<InstanceId> {
    app.world.get(id).unwrap().children().to_vec()
}

/// How many parts the viewport would draw.
fn drawn_parts(app: &EditorApp) -> usize {
    let camera = axcenity_render::Camera {
        position: Vec3::new(0.0, 50.0, 50.0),
        look_at: Vec3::ZERO,
        fov_y_degrees: 70.0,
    };
    axcenity_scene::build_draw_list(&app.world, &app.assets, camera)
        .items
        .len()
}

/// The Instance just inserted or duplicated, which is selected.
pub(super) fn inserted(
    app: &mut EditorApp,
    what: Insertable,
    near: Option<InstanceId>,
) -> InstanceId {
    app.act(Action::Insert { what, near });
    app.selected.expect("inserting selects the new Instance")
}

#[test]
fn each_primitive_is_inserted_at_the_camera_focus_point() {
    let mut app = EditorApp::new(Some(&example("hello-touched")));
    let workspace = app.world.workspace();
    app.viewport
        .look_at(Vec3::new(30.0, 20.0, 10.0), Vec3::new(5.25, 3.0, -7.5));
    app.snap.move_on = false;
    let mut shapes = Vec::new();
    for what in Insertable::PARTS {
        let part = inserted(&mut app, what, None);
        assert_eq!(app.world.get(part).unwrap().parent(), Some(workspace));
        assert_eq!(property(&app, part, "Position"), vector(5.25, 3.0, -7.5));
        shapes.push(property(&app, part, "Shape"));
        let label = format!("Insert {}", name(&app, part));
        assert_eq!(app.history.undo_label(), Some(label.as_str()));
    }
    let shape = |shape| Value::PartType(shape);
    assert_eq!(
        shapes,
        [
            shape(PartType::Block),
            shape(PartType::Ball),
            shape(PartType::Cylinder),
            shape(PartType::Wedge)
        ]
    );
    // They're in the world, so the viewport draws them.
    assert_eq!(drawn_parts(&app), 3 + 4);

    // With Snap move on, the point goes to the move grid.
    app.snap.move_on = true;
    let part = inserted(&mut app, Insertable::Part(PartType::Block), None);
    assert_eq!(property(&app, part, "Position"), vector(5.0, 3.0, -8.0));

    // Each insert is one undo step, and undoing it clears the selection.
    app.undo();
    assert!(!app.world.is_descendant_of(part, workspace));
    assert_eq!(app.selected, None);
    app.redo();
    assert_eq!(children(&app, workspace).last(), Some(&part));
}

#[test]
fn inserting_gives_a_unique_default_name_among_siblings() {
    let mut app = EditorApp::new(Some(&example("hello-touched")));
    let block = Insertable::Part(PartType::Block);
    let names: Vec<String> = [block, block, Insertable::Model, Insertable::Folder]
        .into_iter()
        .chain([Insertable::Model, Insertable::Folder, block])
        .map(|what| {
            let id = inserted(&mut app, what, None);
            name(&app, id).to_string()
        })
        .collect();
    assert_eq!(
        names,
        [
            "Part", "Part2", "Model", "Folder", "Model2", "Folder2", "Part3"
        ]
    );

    // Inside a Model, only its own children count.
    let model = part(&app, "Model");
    let inside = inserted(&mut app, block, Some(model));
    assert_eq!(app.world.get(inside).unwrap().parent(), Some(model));
    assert_eq!(name(&app, inside), "Part");
    let inside2 = inserted(&mut app, block, Some(model));
    assert_eq!(name(&app, inside2), "Part2");

    // Duplicates follow the same rule, and go right after the original.
    let pad = part(&app, "Pad");
    app.act(Action::Duplicate(pad));
    let copy = app.selected.unwrap();
    assert_eq!(name(&app, copy), "Pad2");
    let workspace = children(&app, app.world.workspace());
    let at = workspace.iter().position(|&id| id == pad).unwrap();
    assert_eq!(workspace[at + 1], copy);
    assert_eq!(
        property(&app, copy, "Position"),
        property(&app, pad, "Position")
    );
    app.act(Action::Duplicate(copy));
    assert_eq!(name(&app, app.selected.unwrap()), "Pad3");
    assert_eq!(app.history.undo_label(), Some("Duplicate Pad2"));
}

/// Everything about `id` and the Instances inside it: IDs, classes, names,
/// parents, child order and every property value.
pub(super) fn describe(world: &World, id: InstanceId) -> Vec<String> {
    std::iter::once(id)
        .chain(world.descendants(id))
        .map(|id| {
            let instance = world.get(id).unwrap();
            format!(
                "{id:?} {} {:?} in {:?} {:?} {:?}",
                instance.class(),
                instance.name(),
                instance.parent(),
                instance.children(),
                instance.properties().collect::<Vec<_>>()
            )
        })
        .collect()
}

#[test]
fn deleting_a_model_deletes_its_children_and_undo_restores_them_all() {
    let folder = copy_of_example("hello-touched", "delete-model");
    let mut app = EditorApp::new(Some(&folder));
    let workspace = app.world.workspace();
    let model = inserted(&mut app, Insertable::Model, None);
    // A part in the model, a part inside that part, and edited properties.
    let wheel = inserted(&mut app, Insertable::Part(PartType::Cylinder), Some(model));
    app.apply(
        wheel,
        Edit::Property("Color", Value::Color3(Color3::new(0.1, 0.2, 0.3))),
    );
    app.apply(
        wheel,
        Edit::Property("Material", Value::Material(Material::Metal)),
    );
    app.apply(wheel, Edit::Property("Roughness", Value::Number(0.75)));
    let hub = inserted(&mut app, Insertable::Folder, Some(model));
    assert!(app.move_into(wheel, hub));
    app.move_into(part(&app, "Pad"), model);
    // Something after the model, so its place among its siblings matters.
    inserted(&mut app, Insertable::Folder, None);
    app.history.seal();
    let before = describe(&app.world, model);
    let siblings = children(&app, workspace);
    let world_file = axcenity_project::write_world(&app.world);
    let drawn_before = drawn_parts(&app);

    app.select_from_viewport(Some(wheel));
    app.act(Action::Delete(model));
    assert!(!children(&app, workspace).contains(&model));
    for id in [model, wheel, hub] {
        assert!(!app.world.is_descendant_of(id, workspace));
    }
    assert_eq!(app.selected, None, "the selection was inside it");
    assert_eq!(
        drawn_parts(&app),
        drawn_before - 2,
        "the wheel and the pad are gone"
    );
    assert_eq!(app.history.undo_label(), Some("Delete Model"));
    // Saving now leaves them out of the file.
    assert!(app.save());
    assert!(!part_exists_on_disk(&folder, "Pad"));

    app.undo();
    assert_eq!(describe(&app.world, model), before);
    assert_eq!(children(&app, workspace), siblings);
    assert_eq!(axcenity_project::write_world(&app.world), world_file);
    // Redo deletes them again, and undo brings them back again.
    app.redo();
    assert!(!app.world.is_descendant_of(wheel, workspace));
    app.undo();
    assert_eq!(describe(&app.world, model), before);
    // The steps before the delete still work on the same Instances:
    // undo "Insert Folder", "Move Pad into Model", then "Move Part into Folder".
    app.undo();
    app.undo();
    assert_eq!(app.world.get(wheel).unwrap().parent(), Some(hub));
    app.undo();
    assert_eq!(app.world.get(wheel).unwrap().parent(), Some(model));
    assert_eq!(property(&app, wheel, "Roughness"), Value::Number(0.75));
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn moving_into_another_parent_is_one_step_and_moves_into_itself_are_rejected() {
    let mut app = EditorApp::new(Some(&example("hello-touched")));
    let workspace = app.world.workspace();
    let model = inserted(&mut app, Insertable::Model, None);
    let pad = part(&app, "Pad");
    app.act(Action::MoveInto {
        id: pad,
        parent: model,
    });
    assert_eq!(children(&app, model), [pad]);
    assert_eq!(app.history.undo_label(), Some("Move Pad into Model"));

    // Into its own child, into itself, out of the Workspace, or moving a
    // service or a Script: nothing changes, and Output says why.
    let steps = app.history.undo_label().map(str::to_string);
    let world_file = axcenity_project::write_world(&app.world);
    let script = app
        .world
        .find_first_child(app.world.server_script_service(), "pad")
        .unwrap();
    for (id, parent) in [
        (model, pad),
        (model, model),
        (model, app.world.server_script_service()),
        (workspace, model),
        (script, model),
    ] {
        assert!(!app.move_into(id, parent));
        assert_eq!(last_line(&app).kind, OutputKind::Error);
        assert!(
            last_line(&app).text.starts_with("Can't move"),
            "{}",
            last_line(&app).text
        );
    }
    assert_eq!(axcenity_project::write_world(&app.world), world_file);
    assert_eq!(app.history.undo_label().map(str::to_string), steps);

    app.undo();
    assert_eq!(app.world.get(pad).unwrap().parent(), Some(workspace));
    assert!(children(&app, model).is_empty());
}

#[test]
fn services_and_scripts_cannot_be_deleted_or_duplicated() {
    let mut app = EditorApp::new(Some(&example("hello-touched")));
    let script = app
        .world
        .find_first_child(app.world.server_script_service(), "pad")
        .unwrap();
    let count = app.world.len();
    for id in [app.world.workspace(), app.world.root(), script] {
        app.act(Action::Delete(id));
        app.act(Action::Duplicate(id));
    }
    assert_eq!(app.world.len(), count);
    assert!(app.world.is_descendant_of(script, app.world.root()));
    assert!(!app.unsaved());
    assert!(last_line(&app).text.starts_with("Can't duplicate pad"));
}

#[test]
fn models_and_folders_save_and_reopen() {
    let folder = copy_of_example("hello-touched", "save-groups");
    let mut app = EditorApp::new(Some(&folder));
    let props = inserted(&mut app, Insertable::Folder, None);
    let car = inserted(&mut app, Insertable::Model, Some(props));
    inserted(&mut app, Insertable::Part(PartType::Wedge), Some(car));
    app.act(Action::Rename(car, "Car".to_string()));
    assert_eq!(app.history.undo_label(), Some("Rename Model"));
    assert!(app.save());
    let saved = axcenity_project::write_world(&app.world);

    let reopened = EditorApp::new(Some(&folder));
    assert_eq!(axcenity_project::write_world(&reopened.world), saved);
    let wedge = reopened
        .world
        .find_first_descendant(reopened.world.workspace(), "Part")
        .unwrap();
    assert_eq!(
        reopened.world.full_name(wedge).as_deref(),
        Some("Workspace.Folder.Car.Part")
    );
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn delete_ctrl_d_and_f2_act_on_the_selection() {
    let mut app = EditorApp::new(Some(&example("hello-touched")));
    let pad = part(&app, "Pad");
    app.selected = Some(pad);
    press(&mut app, egui::Modifiers::COMMAND, egui::Key::D);
    let copy = app.selected.unwrap();
    assert_eq!(name(&app, copy), "Pad2");
    press(&mut app, egui::Modifiers::NONE, egui::Key::Delete);
    assert!(!app.world.is_descendant_of(copy, app.world.root()));
    assert_eq!(app.selected, None);
    app.selected = Some(pad);
    press(&mut app, egui::Modifiers::NONE, egui::Key::F2);
    assert_eq!(app.explorer.renaming(), Some(pad));
}

/// Runs one egui frame in which `key` is pressed with `modifiers`, and
/// lets the app read its shortcuts. No window or GPU needed.
pub(super) fn press(app: &mut EditorApp, modifiers: egui::Modifiers, key: egui::Key) {
    let input = egui::RawInput {
        modifiers,
        events: vec![egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }],
        ..Default::default()
    };
    let _ = egui::Context::default().run(input, |ctx| app.shortcuts(ctx));
}

#[test]
fn ctrl_1_to_4_pick_the_tools_and_ctrl_z_and_ctrl_y_undo_and_redo() {
    let mut app = EditorApp::new(Some(&example("hello-touched")));
    let command = egui::Modifiers::COMMAND;
    assert_eq!(app.tool, Tool::Select);
    for (key, tool) in [
        (egui::Key::Num2, Tool::Move),
        (egui::Key::Num3, Tool::Scale),
        (egui::Key::Num4, Tool::Rotate),
        (egui::Key::Num1, Tool::Select),
    ] {
        press(&mut app, command, key);
        assert_eq!(app.tool, tool);
    }

    let pad = part(&app, "Pad");
    app.apply(pad, Edit::Property("Anchored", Value::Bool(false)));
    press(&mut app, command, egui::Key::Z);
    assert_eq!(property(&app, pad, "Anchored"), Value::Bool(true));
    press(&mut app, command, egui::Key::Y);
    assert_eq!(property(&app, pad, "Anchored"), Value::Bool(false));
    // Ctrl+Shift+Z redoes too, and isn't taken as Ctrl+Z.
    press(&mut app, command, egui::Key::Z);
    press(&mut app, command | egui::Modifiers::SHIFT, egui::Key::Z);
    assert_eq!(property(&app, pad, "Anchored"), Value::Bool(false));
}

/// Runs the game for `seconds` at 60 frames per second.
pub(super) fn play_for(app: &mut EditorApp, seconds: f64) {
    for _ in 0..(seconds * 60.0).round() as u32 {
        app.play_frame(1.0 / 60.0, &[], None, None);
    }
}

pub(super) fn printed(app: &EditorApp) -> Vec<&str> {
    app.output.lines().iter().map(|l| l.text.as_str()).collect()
}

#[test]
fn set_up_autocomplete_writes_the_files_and_opening_refreshes_old_definitions() {
    let folder = copy_of_example("hello-events", "autocomplete");
    let mut app = EditorApp::new(Some(&folder));
    let scripts = app.project.as_ref().unwrap().scripts.len();
    app.set_up_autocomplete();
    assert_eq!(
        last_line(&app).text,
        "Set up autocomplete: wrote `.axcenity/axcenity.d.luau`, `.vscode/settings.json` \
         and `.vscode/extensions.json`. Open the project folder in VS Code and install \
         the Luau extension (JohnnyMorganz.luau-lsp) when it asks; scripts then \
         autocomplete the engine's API."
    );
    let definitions = folder.join(autocomplete::DEFINITIONS_FILE);
    assert_eq!(
        std::fs::read_to_string(&definitions).unwrap(),
        axcenity_script::lua_definitions()
    );
    // Definitions from an older engine are updated when the project opens.
    std::fs::write(&definitions, "-- old\n").unwrap();
    app.open(&folder);
    assert!(printed(&app).contains(
        &"Updated `.axcenity/axcenity.d.luau` to this version of the engine's script API."
    ),);
    assert_eq!(
        std::fs::read_to_string(&definitions).unwrap(),
        axcenity_script::lua_definitions()
    );
    // The project still opens as before: the new files aren't scripts.
    assert_eq!(app.project.as_ref().unwrap().scripts.len(), scripts);
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn quoted_lists_read_like_a_sentence() {
    assert_eq!(quoted_list(&["a"]), "`a`");
    assert_eq!(quoted_list(&["a", "b"]), "`a` and `b`");
    assert_eq!(quoted_list(&["a", "b", "c"]), "`a`, `b` and `c`");
}

#[test]
fn opening_another_project_stops_the_game() {
    let mut app = EditorApp::new(Some(&example("hello-touched")));
    app.play();
    app.open(&example("hello-part"));
    assert!(!app.playing());
}

#[test]
fn building_tools_change_the_whole_selection_in_one_undo_step() {
    let mut app = EditorApp::new(Some(&example("hello-touched")));
    let first = inserted(&mut app, Insertable::Part(PartType::Block), None);
    let second = inserted(&mut app, Insertable::Part(PartType::Ball), None);
    app.selection = vec![first, second];
    app.selected = Some(second);

    let red = Value::Color3(Color3::new(1.0, 0.0, 0.0));
    app.apply_tool_action(ToolAction::Set {
        property: "Color",
        value: red.clone(),
    });
    app.history.seal();
    assert_eq!(property(&app, first, "Color"), red);
    assert_eq!(property(&app, second, "Color"), red);
    assert_eq!(app.history.undo_label(), Some("Paint 2 parts"));

    app.apply_tool_action(ToolAction::Set {
        property: "CanCollide",
        value: Value::Bool(false),
    });
    app.history.seal();
    assert_eq!(property(&app, first, "CanCollide"), Value::Bool(false));

    app.apply_tool_action(ToolAction::AddLight(ClassName::PointLight));
    assert_eq!(children(&app, first).len(), 1);
    assert_eq!(children(&app, second).len(), 1);
    assert_eq!(app.history.undo_label(), Some("Add 2 PointLights"));
    // Adding again leaves parts that already have one alone.
    app.apply_tool_action(ToolAction::AddLight(ClassName::PointLight));
    assert_eq!(children(&app, first).len(), 1);

    app.undo();
    assert!(children(&app, first).is_empty() && children(&app, second).is_empty());
    app.undo();
    assert_eq!(property(&app, first, "CanCollide"), Value::Bool(true));
    app.undo();
    assert_ne!(property(&app, first, "Color"), red);
    assert_ne!(property(&app, second, "Color"), red);
}

#[test]
fn f3x_keys_pick_tools_while_editing() {
    let mut app = EditorApp::new(Some(&example("hello-touched")));
    for (key, tool) in [
        (egui::Key::Z, Tool::Move),
        (egui::Key::X, Tool::Scale),
        (egui::Key::V, Tool::Paint),
        (egui::Key::M, Tool::Anchor),
        (egui::Key::K, Tool::Collision),
        (egui::Key::U, Tool::Lighting),
        (egui::Key::P, Tool::Emission),
    ] {
        press(&mut app, egui::Modifiers::NONE, key);
        assert_eq!(app.tool, tool);
    }
}

#[test]
fn the_angular_tool_places_its_copies_in_one_undo_step() {
    let mut app = EditorApp::new(Some(&example("hello-touched")));
    let workspace = app.world.workspace();
    let block = inserted(&mut app, Insertable::Part(PartType::Block), None);
    let before = children(&app, workspace).len();
    app.tool = Tool::Angular;
    app.build.angular.count = 23;
    let preview = app.angular_preview();
    assert_eq!(preview.len(), 23);
    app.apply_tool_action(ToolAction::Extend);
    assert_eq!(children(&app, workspace).len(), before + 23);
    let last = app.selected.unwrap();
    assert_ne!(last, block);
    let pose = Pose::of(app.world.get(last).unwrap()).unwrap();
    assert!(pose.position.abs_diff_eq(preview[22].position, 1e-3));
    assert_eq!(app.history.undo_label(), Some("Extend Part 23 times"));
    app.undo();
    assert_eq!(children(&app, workspace).len(), before);
}

#[test]
fn every_tutorial_sample_opens_and_plays_in_the_editor() {
    for name in crate::start::tutorial_samples() {
        let folder = copy_of_example(name, &format!("tutorial-{name}"));
        let mut app = EditorApp::new(Some(&folder));
        assert!(app.project.is_some(), "{name} didn't open");
        app.play();
        assert!(app.playing(), "{name} didn't play");
        play_for(&mut app, 1.0);
        let errors: Vec<&str> = app
            .output
            .lines()
            .iter()
            .filter(|line| line.kind == OutputKind::Error)
            .map(|line| line.text.as_str())
            .collect();
        assert!(errors.is_empty(), "{name}: {errors:?}");
        drop(app);
        let _ = std::fs::remove_dir_all(folder);
    }
}

fn class_of(world: &World, id: InstanceId) -> ClassName {
    world.get(id).unwrap().class()
}

#[test]
fn a_first_person_controller_leaves_a_third_person_one_alone_and_saves_as_its_own_class() {
    let folder = copy_of_example("hello-character", "first-person-insert");
    let mut app = EditorApp::new(Some(&folder));
    let workspace = app.world.workspace();
    let model = part(&app, "Character");
    let third = app
        .world
        .find_first_child(model, "ThirdPersonController")
        .unwrap();

    // The character keeps its controller; the new one gets a new character.
    let first = inserted(&mut app, Insertable::FirstPersonController, Some(model));
    assert_eq!(
        class_of(&app.world, third),
        ClassName::ThirdPersonController
    );
    assert_eq!(
        class_of(&app.world, first),
        ClassName::FirstPersonController
    );
    assert_eq!(name(&app, first), "FirstPersonController");
    let holder = app.world.get(first).unwrap().parent().unwrap();
    assert_ne!(holder, model);
    assert_eq!(app.world.get(holder).unwrap().parent(), Some(workspace));
    assert!(app.world.find_first_child(holder, "Body").is_some());
    // Selecting a part of the first character doesn't add a second controller to it either.
    let body = app.world.find_first_child(model, "Body").unwrap();
    let another = inserted(&mut app, Insertable::FirstPersonController, Some(body));
    assert!(!app.world.is_descendant_of(another, model));

    assert!(app.save());
    let world = std::fs::read_to_string(folder.join("world.json")).unwrap();
    assert!(world.contains(r#""class": "ThirdPersonController""#));
    assert!(world.contains(r#""class": "FirstPersonController""#));
    let reopened = EditorApp::new(Some(&folder));
    let reloaded = &reopened.world;
    let model = reloaded
        .find_first_child(reloaded.workspace(), "Character")
        .unwrap();
    let third = reloaded
        .find_first_child(model, "ThirdPersonController")
        .unwrap();
    assert_eq!(class_of(reloaded, third), ClassName::ThirdPersonController);
    let holder = reloaded
        .find_first_child(reloaded.workspace(), "Character2")
        .unwrap();
    let first = reloaded
        .find_first_child(holder, "FirstPersonController")
        .unwrap();
    assert_eq!(class_of(reloaded, first), ClassName::FirstPersonController);
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn a_first_person_camera_stays_at_the_eyes_during_play_and_stop_restores_the_fly_camera() {
    let folder = copy_of_example("hello-touched", "first-person-play");
    let mut app = EditorApp::new(Some(&folder));
    let first = inserted(&mut app, Insertable::FirstPersonController, None);
    let model = app.world.get(first).unwrap().parent().unwrap();
    let body = app.world.find_first_child(model, "Body").unwrap();
    let fly = app.viewport.camera_view();

    app.play();
    for seconds in [0.1, 0.5, 1.0] {
        play_for(&mut app, seconds);
        let game = shown(&app.world, &app.play);
        let at = game
            .get(body)
            .unwrap()
            .property("Position")
            .and_then(Value::as_vector3)
            .unwrap();
        let eyes = app.play.as_ref().unwrap().character_camera().unwrap();
        // The 5-stud body's feet are 2.5 below its middle; the eyes are 4.5 above them.
        let expected = Vector3::new(at.x, at.y - 2.5 + 4.5, at.z);
        for (got, want) in [
            (eyes.position.x, expected.x),
            (eyes.position.y, expected.y),
            (eyes.position.z, expected.z),
        ] {
            assert!((got - want).abs() < 1e-3, "{eyes:?} at {at:?}");
        }
        assert_eq!(eyes.orientation, Vector3::new(0.0, 0.0, 0.0));
    }

    app.stop();
    assert!(app.play.is_none());
    assert_eq!(app.viewport.camera_view(), fly);
    let _ = std::fs::remove_dir_all(&folder);
}
