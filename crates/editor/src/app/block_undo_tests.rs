//! Undo and the world's keys over a block script's tab (`block_undo.rs`).

use axcenity_blocks::{Block, BlockKind, BlockScript};

use super::block_undo::BlockUndo;
use super::tests::{copy_of_example, part};
use super::*;
use crate::scripts::ScriptView;

const SIZE: egui::Vec2 = egui::vec2(1500.0, 950.0);

/// A project with a new Script switched to Blocks, its tab open, and the
/// part "Block" selected. Returns the app, the script's path and its tab.
fn block_script_open(test: &str) -> (EditorApp, String, u64) {
    let folder = copy_of_example("hello-part", test);
    let mut app = EditorApp::new(Some(&folder));
    app.insert_script(ClassName::Script);
    let code_path = app.tabs.active_path().unwrap().to_owned();
    app.switch_script_view(&code_path, ScriptView::Blocks);
    let path = app
        .project
        .as_ref()
        .unwrap()
        .block_scripts
        .last()
        .unwrap()
        .path
        .clone();
    let tab = app.tabs.active_id().unwrap();
    assert_eq!(app.tabs.path_of(tab), Some(path.as_str()));
    let block = part(&app, "Block");
    app.select_clicked(Some(block), egui::Modifiers::NONE, None, false);
    (app, path, tab)
}

fn stacks(app: &EditorApp, path: &str) -> usize {
    document(app, path).blocks.len()
}

fn document<'a>(app: &'a EditorApp, path: &str) -> &'a BlockScript {
    &app.project
        .as_ref()
        .unwrap()
        .block_scripts
        .iter()
        .find(|document| document.path == path)
        .unwrap()
        .script
}

/// Draws one frame of the editor's shortcuts and then the tab, as
/// `EditorApp::frame` does, with `events`.
fn frame(app: &mut EditorApp, ctx: &egui::Context, tab: u64, time: f64, events: Vec<egui::Event>) {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, SIZE)),
        events,
        time: Some(time),
        ..Default::default()
    };
    let _ = crate::theme::run(ctx, input, |ui| {
        let ctx = &ui.ctx().clone();
        app.shortcuts(ctx);
        app.block_tab_pointer = None;
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| app.script_pane(ui, tab));
    });
}

fn button(pos: egui::Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    }
}

fn key(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

#[test]
fn ctrl_z_over_blocks_undoes_the_blocks_not_the_world() {
    let (mut app, path, tab) = block_script_open("block-undo");
    // A world edit first, which Ctrl+Z over the blocks must leave alone.
    let block = part(&app, "Block");
    app.apply(block, crate::properties::Edit::Name("Moved".to_owned()));
    let ctx = theme::test_context();
    let mut time = 0.0;
    let mut run = |app: &mut EditorApp, events| {
        time += 0.5;
        frame(app, &ctx, tab, time, events);
    };
    run(
        &mut app,
        vec![egui::Event::PointerMoved(egui::pos2(900.0, 500.0))],
    );
    run(&mut app, Vec::new());
    // The bar that switches Code, Blocks and Graph is above the blocks.
    let top = egui::containers::panel::PanelState::load(&ctx, egui::Id::new(("script mode", tab)))
        .expect("the mode bar was drawn")
        .outer_rect
        .bottom();
    assert_eq!(stacks(&app, &path), 1, "a new script is one stack");

    // Drag "when game starts", the palette's first block (under its search
    // box), onto the canvas. The stack being dragged is one step, made when
    // it's let go.
    let from = egui::pos2(150.0, top + 157.0);
    let to = egui::pos2(900.0, top + 500.0);
    run(&mut app, vec![egui::Event::PointerMoved(from)]);
    run(&mut app, vec![button(from, true)]);
    for step in 1..=4 {
        run(
            &mut app,
            vec![egui::Event::PointerMoved(from.lerp(to, step as f32 / 4.0))],
        );
    }
    assert_eq!(stacks(&app, &path), 1, "not dropped yet");
    run(&mut app, vec![button(to, false)]);
    run(&mut app, Vec::new());
    assert_eq!(stacks(&app, &path), 2, "dropped on the canvas");
    let document_steps = |app: &EditorApp| {
        app.project
            .as_ref()
            .unwrap()
            .block_scripts
            .iter()
            .find(|d| d.path == path)
            .unwrap()
            .undo
            .steps()
    };
    assert_eq!(document_steps(&app), (1, 0));

    // Ctrl+Z with the mouse over the blocks takes the stack away again,
    // and the Lua with it; the part keeps its new name.
    run(&mut app, vec![key(egui::Key::Z, egui::Modifiers::COMMAND)]);
    assert_eq!(stacks(&app, &path), 1, "the drop is undone");
    assert_eq!(
        app.world.get(block).unwrap().name(),
        "Moved",
        "the world is left alone"
    );
    assert!(!app.tabs.get(&path).unwrap().text.contains("task.spawn"));
    // Ctrl+Y puts it back.
    run(&mut app, vec![key(egui::Key::Y, egui::Modifiers::COMMAND)]);
    assert_eq!(stacks(&app, &path), 2, "the drop is redone");
    assert!(app.tabs.get(&path).unwrap().text.contains("task.spawn"));
    // With nothing left to undo there, Ctrl+Z still doesn't reach the world.
    run(&mut app, vec![key(egui::Key::Z, egui::Modifiers::COMMAND)]);
    run(&mut app, vec![key(egui::Key::Z, egui::Modifiers::COMMAND)]);
    assert_eq!(stacks(&app, &path), 1);
    assert_eq!(app.world.get(block).unwrap().name(), "Moved");

    // Away from the tab, Ctrl+Z is the world's again.
    run(&mut app, vec![egui::Event::PointerGone]);
    run(&mut app, Vec::new());
    run(&mut app, vec![key(egui::Key::Z, egui::Modifiers::COMMAND)]);
    assert_eq!(app.world.get(block).unwrap().name(), "Block");
    remove_copy(&app);
}

#[test]
fn delete_and_ctrl_d_over_blocks_leave_the_selected_part_alone() {
    let (mut app, _path, tab) = block_script_open("block-keys");
    let block = part(&app, "Block");
    let ctx = theme::test_context();
    let mut time = 0.0;
    let mut run = |app: &mut EditorApp, events| {
        time += 0.5;
        frame(app, &ctx, tab, time, events);
    };
    run(
        &mut app,
        vec![egui::Event::PointerMoved(egui::pos2(900.0, 500.0))],
    );
    run(&mut app, Vec::new());
    let before = app_parts(&app);
    run(&mut app, vec![key(egui::Key::D, egui::Modifiers::COMMAND)]);
    run(
        &mut app,
        vec![key(egui::Key::Delete, egui::Modifiers::NONE)],
    );
    assert_eq!(app_parts(&app), before, "nothing was duplicated");
    let parent = |app: &EditorApp| app.world.get(block).and_then(|part| part.parent());
    assert!(parent(&app).is_some(), "the part is still there");

    // Away from the tab, Delete deletes it as before.
    run(&mut app, vec![egui::Event::PointerGone]);
    run(&mut app, Vec::new());
    run(
        &mut app,
        vec![key(egui::Key::Delete, egui::Modifiers::NONE)],
    );
    assert_eq!(parent(&app), None, "deleted");
    remove_copy(&app);
}

/// Deletes the test's copy of the example.
fn remove_copy(app: &EditorApp) {
    let _ = std::fs::remove_dir_all(&app.project.as_ref().unwrap().folder);
}

fn app_parts(app: &EditorApp) -> usize {
    app.world
        .get(app.world.workspace())
        .unwrap()
        .children()
        .len()
}

fn script_with(count: usize) -> BlockScript {
    BlockScript {
        blocks: (0..count)
            .map(|_| Block::new(BlockKind::WhenStarted { body: Vec::new() }))
            .collect(),
        ..Default::default()
    }
}

#[test]
fn an_edit_over_several_frames_is_one_step() {
    let mut undo = BlockUndo::default();
    let mut script = script_with(1);
    // A drag: the stack leaves the script when it's picked up, and comes
    // back where it's dropped, with the button held in between.
    let start = script.clone();
    script.blocks.clear();
    undo.track(start.clone(), &script, true);
    undo.track(script.clone(), &script, true);
    let mut moved = start.clone();
    moved.blocks[0].position = Some([40.0, 60.0]);
    undo.track(script.clone(), &moved, false);
    script = moved.clone();
    assert_eq!(undo.steps(), (1, 0));
    // A drag that ends where it began is no step.
    let mut same = script.clone();
    same.blocks.clear();
    undo.track(script.clone(), &same, true);
    undo.track(same, &script, false);
    assert_eq!(undo.steps(), (1, 0));

    assert!(undo.step(&mut script, true));
    assert_eq!(script, start);
    assert!(!undo.step(&mut script, true), "nothing more to undo");
    assert!(undo.step(&mut script, false));
    assert_eq!(script, moved);
    // A new edit after an undo forgets what could be redone.
    assert!(undo.step(&mut script, true));
    let more = script_with(2);
    undo.track(script.clone(), &more, false);
    assert_eq!(undo.steps(), (1, 0));
}

#[test]
fn undo_stays_in_the_view_shown() {
    let mut undo = BlockUndo::default();
    let mut script = script_with(1);
    let edited = script_with(2);
    undo.track(script.clone(), &edited, false);
    script = edited;
    // Switched to the Graph view after the edit.
    script.graph.shown = true;
    assert!(undo.step(&mut script, true));
    assert_eq!(script.blocks.len(), 1);
    assert!(script.graph.shown, "still the Graph view");
}
