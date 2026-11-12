//! Scripts reloaded during Play (unified Phase 24, keeping their state
//! since Phase 89), and Play edits kept at Stop.

use std::time::{Duration, Instant};

use axcenity_core::{Color3, Value};

use super::tests::{copy_of_example, part, play_for, printed, property, vector};
use super::*;
use crate::output::OutputKind;

const GREETER: &str = "scripts/Greeter.server.lua";

/// hello-touched, with a script that prints Hello twice a second and one
/// that slides an anchored `Mover` part along X every frame.
fn project_with_greeter(test: &str) -> PathBuf {
    let folder = copy_of_example("hello-touched", test);
    std::fs::write(
        folder.join(GREETER),
        "while true do\n    print('Hello')\n    task.wait(0.5)\nend\n",
    )
    .unwrap();
    std::fs::write(
        folder.join("scripts/Mover.server.lua"),
        "local mover = Instance.new('Part')\n\
         mover.Name = 'Mover'\n\
         mover.Anchored = true\n\
         mover.Parent = workspace\n\
         game:GetService('RunService').Heartbeat:Connect(function(dt)\n\
             mover.Position = mover.Position + Vector3.new(dt, 0, 0)\n\
         end)\n",
    )
    .unwrap();
    folder
}

/// How far the game's `Mover` part has slid.
fn mover_x(app: &EditorApp) -> f32 {
    let game = shown(&app.world, &app.play);
    let mover = game.find_first_child(game.workspace(), "Mover").unwrap();
    match game.get(mover).unwrap().property("Position") {
        Some(Value::Vector3(position)) => position.x,
        other => panic!("{other:?}"),
    }
}

/// What was printed or reported from line `from` on.
fn since(app: &EditorApp, from: usize) -> Vec<&str> {
    printed(app)[from..].to_vec()
}

/// Takes in changed files and plays frames until `done`, for up to 3 seconds.
fn until(app: &mut EditorApp, done: impl Fn(&EditorApp) -> bool) {
    let start = Instant::now();
    while !done(app) && start.elapsed() < Duration::from_secs(3) {
        std::thread::sleep(Duration::from_millis(20));
        app.check_disk();
        play_for(app, 1.0 / 60.0);
    }
}

/// Opens the script tab for `path` and types `text` into it.
fn type_into(app: &mut EditorApp, path: &str, text: &str) {
    let source = std::fs::read_to_string(app.project.as_ref().unwrap().folder.join(path)).unwrap();
    app.tabs.open(path, &source, None);
    app.tabs.get_mut(path).unwrap().text = text.to_owned();
}

#[test]
fn saving_a_changed_script_during_play_prints_the_new_text_and_a_moving_part_keeps_moving() {
    let folder = project_with_greeter("live-script-save");
    let mut app = EditorApp::new(Some(&folder));
    app.play();
    play_for(&mut app, 1.0);
    assert!(printed(&app).contains(&"Hello"));
    let moved = mover_x(&app);
    assert!(moved > 0.5, "{moved}");

    type_into(
        &mut app,
        GREETER,
        "while true do\n    print('Goodbye')\n    task.wait(0.5)\nend\n",
    );
    let from = printed(&app).len();
    assert!(app.save());
    assert!(app.playing(), "Save didn't stop the game");
    play_for(&mut app, 1.0);
    let after = since(&app, from);
    assert!(after.contains(&"Reloaded Greeter.server.lua"), "{after:?}");
    assert!(after.contains(&"Goodbye"), "{after:?}");
    assert!(!after.contains(&"Hello"), "the old loop stopped: {after:?}");
    assert!(
        mover_x(&app) > moved + 0.9,
        "the other script kept moving its part"
    );
    drop(app);
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn a_script_changed_in_another_editor_during_play_drops_its_old_touched_handler_and_wait() {
    let folder = copy_of_example("hello-touched", "live-script-disk");
    let mut app = EditorApp::new(Some(&folder));
    app.play();
    play_for(&mut app, 0.1);
    // As VS Code would save it: the pad's script now says something else
    // when touched, drops a ball of its own, and no longer waits to drop
    // the blue one.
    std::fs::write(
        folder.join("scripts/pad.server.lua"),
        "workspace.Pad.Touched:Connect(function(other)
             print('New: ' .. other.Name .. ' landed')
         end)
         local ball = Instance.new('Part')
         ball.Name = 'LateBall'
         ball.Shape = Enum.PartType.Ball
         ball.Size = Vector3.new(2, 2, 2)
         ball.Position = Vector3.new(2.5, 8, -2.5)
         ball.Parent = workspace
",
    )
    .unwrap();
    until(&mut app, |app| {
        printed(app).contains(&"Reloaded pad.server.lua")
    });
    let from = printed(&app).len();
    assert!(printed(&app).contains(&"Reloaded pad.server.lua"));

    play_for(&mut app, 9.0);
    let after = since(&app, from);
    assert!(after.contains(&"New: LateBall landed"), "{after:?}");
    // The old handler never printed again, and the old `task.wait(3)`
    // never came back to drop the blue ball or stop listening.
    assert!(
        !after.iter().any(|line| line.ends_with("touched the pad")),
        "{after:?}"
    );
    assert!(!after.contains(&"stopped listening for touches"));
    let game = shown(&app.world, &app.play);
    assert!(
        game.find_first_child(game.workspace(), "BlueBall")
            .is_none()
    );
    drop(app);
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn saving_a_script_during_play_keeps_its_state_like_s_and_box() {
    // Phase 89: the Mover's part and its counter carry over; only the code
    // changes, so there's still one Mover and the count goes on.
    let folder = project_with_greeter("live-script-state");
    let mover = "scripts/Mover.server.lua";
    let counting = |word: &str| {
        format!(
            "local mover = Instance.new('Part')\n\
             mover.Name = 'Mover'\n\
             mover.Anchored = true\n\
             mover.Parent = workspace\n\
             local frames = 0\n\
             game:GetService('RunService').Heartbeat:Connect(function()\n\
                 frames += 1\n\
                 if frames == 90 then print('{word} ' .. frames) end\n\
             end)\n"
        )
    };
    std::fs::write(folder.join(mover), counting("old")).unwrap();
    let mut app = EditorApp::new(Some(&folder));
    app.play();
    play_for(&mut app, 1.0);
    assert!(!printed(&app).iter().any(|line| line.starts_with("old")));

    type_into(&mut app, mover, &counting("new"));
    let from = printed(&app).len();
    assert!(app.save());
    play_for(&mut app, 1.0);
    let after = since(&app, from);
    assert!(
        after.contains(&"Reloaded Mover.server.lua, kept frames and mover"),
        "{after:?}"
    );
    // 60 frames before the save and 30 after: the count carried on.
    assert!(after.contains(&"new 90"), "{after:?}");
    let game = shown(&app.world, &app.play);
    let movers = game
        .get(game.workspace())
        .unwrap()
        .children()
        .iter()
        .filter(|&&id| game.get(id).is_some_and(|i| i.name() == "Mover"))
        .count();
    assert_eq!(movers, 1, "the part wasn't made again");
    drop(app);
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn saving_a_syntax_error_during_play_keeps_the_old_version_and_shows_the_line() {
    let folder = project_with_greeter("live-script-error");
    let mut app = EditorApp::new(Some(&folder));
    app.play();
    play_for(&mut app, 0.2);
    type_into(&mut app, GREETER, "print('Goodbye')\nlocal = 5\n");
    let from = app.output.lines().len();
    assert!(app.save());
    let error = app.output.lines()[from..]
        .iter()
        .find(|line| line.kind == OutputKind::Error)
        .expect("the error is shown");
    assert!(
        error.text.starts_with("scripts/Greeter.server.lua:2:"),
        "{}",
        error.text
    );
    assert!(error.location.is_some(), "it links to the line");
    let from = printed(&app).len();
    play_for(&mut app, 1.0);
    let after = since(&app, from);
    assert!(
        after.contains(&"Hello"),
        "the old version runs on: {after:?}"
    );
    assert!(!after.contains(&"Goodbye"));
    // The file was still saved.
    let saved = std::fs::read_to_string(folder.join(GREETER)).unwrap();
    assert!(saved.contains("local = 5"));
    drop(app);
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn a_kept_play_edit_is_in_the_edit_world_after_stop_as_one_undo_step() {
    let folder = copy_of_example("hello-touched", "keep-play-edit");
    let mut app = EditorApp::new(Some(&folder));
    let pad = part(&app, "Pad");
    let color_before = property(&app, pad, "Color");
    let size_before = property(&app, pad, "Size");
    let undo_before = app.history.undo_label().map(str::to_owned);
    app.play();
    app.select_from_viewport(Some(pad));
    let red = Value::Color3(Color3::new(1.0, 0.0, 0.0));
    app.apply(pad, Edit::Property("Color", red.clone()));
    app.apply(pad, Edit::Property("Size", vector(9.0, 1.0, 9.0)));
    let session = app.play.as_ref().unwrap();
    assert_eq!(
        session.kept().fields(pad),
        [
            (Field::Property("Color"), false),
            (Field::Property("Size"), false)
        ],
        "each edited value has a Keep pin"
    );
    app.apply(pad, Edit::Keep(Field::Property("Size"), true));
    play_for(&mut app, 0.5);
    app.stop();

    let big = vector(9.0, 1.0, 9.0);
    assert_eq!(property(&app, pad, "Size"), big, "kept");
    assert_eq!(property(&app, pad, "Color"), color_before, "not kept");
    assert!(app.unsaved());
    assert_eq!(app.history.undo_label(), Some("Keep Play changes to Pad"));
    assert!(
        printed(&app)
            .last()
            .unwrap()
            .starts_with("Stopped. Kept 1 value from Play")
    );

    app.undo();
    assert_eq!(property(&app, pad, "Size"), size_before);
    assert_eq!(app.history.undo_label().map(str::to_owned), undo_before);
    app.redo();
    assert_eq!(property(&app, pad, "Size"), big);
    drop(app);
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn values_that_were_not_kept_are_discarded_at_stop() {
    let folder = copy_of_example("hello-touched", "discard-play-edit");
    let mut app = EditorApp::new(Some(&folder));
    let pad = part(&app, "Pad");
    let color_before = property(&app, pad, "Color");
    app.play();
    let red = Value::Color3(Color3::new(1.0, 0.0, 0.0));
    app.apply(pad, Edit::Property("Color", red));
    // Pinned, then unpinned again.
    app.apply(pad, Edit::Keep(Field::Property("Color"), true));
    app.apply(pad, Edit::Keep(Field::Property("Color"), false));
    app.stop();
    assert_eq!(property(&app, pad, "Color"), color_before);
    assert!(!app.unsaved());
    assert_eq!(
        printed(&app).last(),
        Some(&"Stopped. The world is back as it was before Play.")
    );
    drop(app);
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn keep_changes_to_an_object_keeps_what_the_game_did_to_it_and_nothing_else() {
    let folder = copy_of_example("hello-touched", "keep-object");
    let mut app = EditorApp::new(Some(&folder));
    let ball = part(&app, "Ball");
    let pad = part(&app, "Pad");
    let ball_before = property(&app, ball, "Position");
    let pad_before = property(&app, pad, "Color");
    app.play();
    app.act(Action::KeepPlayChanges(ball));
    // The ball falls onto the pad, and the pad's script recolours it.
    play_for(&mut app, 2.9);
    let landed = shown(&app.world, &app.play)
        .get(ball)
        .unwrap()
        .property("Position")
        .cloned()
        .unwrap();
    assert_ne!(landed, ball_before);
    app.stop();

    assert_eq!(property(&app, ball, "Position"), landed);
    assert_eq!(
        property(&app, pad, "Color"),
        pad_before,
        "the pad wasn't kept"
    );
    app.undo();
    assert_eq!(property(&app, ball, "Position"), ball_before);
    drop(app);
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn properties_show_a_keep_pin_beside_each_value_edited_in_play_and_fit_a_narrow_panel() {
    let folder = copy_of_example("hello-touched", "keep-pins-drawn");
    let mut app = EditorApp::new(Some(&folder));
    let pad = part(&app, "Pad");
    app.play();
    app.select_from_viewport(Some(pad));
    app.apply(pad, Edit::Property("Size", vector(9.0, 1.0, 9.0)));
    app.apply(pad, Edit::Property("Anchored", Value::Bool(false)));
    app.apply(pad, Edit::Keep(Field::Property("Size"), true));
    let size = egui::vec2(260.0, 900.0);
    let mut fit = None;
    let mut draw = |ui: &mut egui::Ui| {
        egui::CentralPanel::default().show(ui, |ui| {
            let room = ui.max_rect();
            app.properties_pane(ui);
            fit = Some((ui.min_rect(), room));
        });
    };
    let ctx = theme::test_context();
    for _ in 0..3 {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            ..Default::default()
        };
        let _ = crate::theme::run(&ctx, input, |ui| draw(ui));
    }
    if let Some(snapshots) = std::env::var_os("AXCENITY_SNAPSHOTS") {
        let path = PathBuf::from(snapshots).join("properties-keep-pins.png");
        crate::snapshot::snapshot(&path, size, vec![Vec::new(); 3], &mut draw);
    }
    let (used, room) = fit.expect("the panel was drawn");
    assert!(used.width() <= room.width() + 0.5, "{used:?} in {room:?}");
    drop(app);
    let _ = std::fs::remove_dir_all(&folder);
}
