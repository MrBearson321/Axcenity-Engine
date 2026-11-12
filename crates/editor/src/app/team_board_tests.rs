//! The Team Board in the editor: tasks made, assigned and moved through
//! the panel's actions, kept as files, and shared with a teammate through a
//! Collaborate session's files.

use axcenity_collab::Role;
use axcenity_teamwork::{Board, Copies, Date, Item, Priority, Status, board::item_path};

use super::team_tests::{over_loopback, settle_net, team_folder};
use super::tests::copy_of_example;
use super::*;
use crate::team_board::{BoardAction, View};

/// Draws the Team Board panel (and the open item's window) `size` big,
/// saving a picture when `AXCENITY_SNAPSHOTS` names a folder.
fn draw_board(app: &mut EditorApp, name: &str, size: egui::Vec2) {
    let mut draw = |ui: &mut egui::Ui| {
        app.team_board_pane(ui);
        app.team_board_frame(&ui.ctx().clone());
    };
    match std::env::var_os("AXCENITY_SNAPSHOTS") {
        Some(folder) => crate::snapshot::snapshot(
            &std::path::Path::new(&folder).join(format!("{name}.png")),
            size,
            vec![Vec::new(); 3],
            &mut draw,
        ),
        None => {
            let ctx = theme::test_context();
            for _ in 0..3 {
                let _ = theme::run(&ctx, Default::default(), |ui| draw(ui));
            }
        }
    }
}

/// The board as `app` has it now.
fn board(app: &mut EditorApp) -> Board {
    app.load_board();
    app.team_board.board().cloned().unwrap_or_default()
}

fn task_named(app: &mut EditorApp, title: &str) -> Option<Item> {
    board(app).items().find(|i| i.title == title).cloned()
}

fn as_person(app: &mut EditorApp, name: &str) {
    app.team_board.me = Some(name.to_owned());
}

#[test]
fn a_task_is_made_assigned_and_moved_and_kept_as_a_file() {
    let project = copy_of_example("hello-part", "team-board");
    let mut app = EditorApp::new(Some(&project));
    as_person(&mut app, "Paul");
    draw_board(&mut app, "team-board-empty", egui::vec2(260.0, 420.0));

    app.apply_board_actions(vec![BoardAction::Create {
        kind: axcenity_teamwork::Kind::Task,
        title: "Fix the door hinge".into(),
    }]);
    let task = task_named(&mut app, "Fix the door hinge").expect("the task");
    assert!(project.join(item_path(&task.id)).exists());
    assert_eq!(task.created_by, "Paul");
    // The new task's window opens, to fill in.
    assert_eq!(app.team_board.panel.open.as_deref(), Some(task.id.as_str()));

    let mut changed = task.clone();
    changed.assign("Sam", "Paul", task.created_at + 1);
    changed.set_status(Status::InProgress, "Paul", task.created_at + 2);
    changed.set_priority(Priority::High, "Paul", task.created_at + 3);
    changed.set_due(Some(Date::today().add_days(3)), "Paul", task.created_at + 4);
    changed.comment("The hinge squeaks on open", "Paul", task.created_at + 5);
    changed.add_check("Grease it", "Paul", task.created_at + 6);
    app.apply_board_actions(vec![BoardAction::Save(Box::new(changed.clone()))]);
    for (title, status) in [
        ("Light the barn", Status::Todo),
        ("Script the gate", Status::InReview),
        ("Import the tractor", Status::Done),
    ] {
        app.apply_board_actions(vec![BoardAction::Create {
            kind: axcenity_teamwork::Kind::Task,
            title: title.into(),
        }]);
        let mut item = task_named(&mut app, title).unwrap();
        item.set_status(status, "Paul", item.created_at + 1);
        item.assign("Paul", "Paul", item.created_at + 2);
        app.apply_board_actions(vec![BoardAction::Save(Box::new(item))]);
    }
    app.team_board.panel.open = Some(task.id.clone());

    // Another editor on the same folder reads the same board.
    let mut again = EditorApp::new(Some(&project));
    assert_eq!(task_named(&mut again, "Fix the door hinge"), Some(changed));
    assert_eq!(board(&mut again).len(), 4);

    draw_board(&mut app, "team-board-task", egui::vec2(560.0, 760.0));
    app.team_board.panel.open = None;
    draw_board(&mut app, "team-board-list", egui::vec2(260.0, 400.0));
    app.team_board.panel.view = View::Columns;
    app.team_board.panel.filter.finished = true;
    draw_board(&mut app, "team-board-columns", egui::vec2(1000.0, 560.0));

    // A link to the selected part selects it again.
    let part = crate::console::resolve_path(&app.world, "Workspace.Block").expect("the block");
    let path = app.world.full_name(part).unwrap();
    app.selection.clear();
    app.selected = None;
    app.apply_board_actions(vec![BoardAction::SelectInstance(path)]);
    assert_eq!(app.selected, Some(part));

    // Deleting removes the file.
    app.apply_board_actions(vec![BoardAction::Delete(task.id.clone())]);
    assert!(!project.join(item_path(&task.id)).exists());
    assert_eq!(board(&mut app).len(), 3);
}

#[test]
fn a_teammates_task_reaches_the_host_and_says_who_was_assigned() {
    let project = copy_of_example("hello-part", "team-board-host");
    let mut a = EditorApp::new(Some(&project));
    as_person(&mut a, "Paul");
    let invite = over_loopback(
        a.host_network_session(Some(&team_folder("board-host")), 0)
            .expect("hosting"),
    );
    let mut b = EditorApp::new(None);
    as_person(&mut b, "Sam");
    b.join_network_session(&invite, Some(&team_folder("board-joiner")))
        .expect("joining");
    settle_net(&mut a, &mut b, |_, b| b.project.is_some());
    // The host's board is read before anything arrives.
    assert!(board(&mut a).is_empty());

    // Sam makes a task and gives it to Paul.
    b.apply_board_actions(vec![BoardAction::Create {
        kind: axcenity_teamwork::Kind::Task,
        title: "Paint the silo".into(),
    }]);
    let mut task = task_named(&mut b, "Paint the silo").unwrap();
    task.assign("Paul", "Sam", task.created_at + 1);
    b.apply_board_actions(vec![BoardAction::Save(Box::new(task.clone()))]);
    let file = project.join(item_path(&task.id));
    settle_net(&mut a, &mut b, |_, _| file.exists());
    settle_net(&mut a, &mut b, |a, _| a.team_board.stale);
    assert_eq!(task_named(&mut a, "Paint the silo"), Some(task.clone()));
    assert!(
        a.toasts
            .texts()
            .iter()
            .any(|t| t.contains("Sam assigned you “Paint the silo”")),
        "{:?}",
        a.toasts.texts()
    );

    // Paul finishes it; Sam's copy shows it done.
    let mut done = task.clone();
    done.set_status(Status::Done, "Paul", task.updated_at + 1);
    a.apply_board_actions(vec![BoardAction::Save(Box::new(done.clone()))]);
    let theirs = b.project.as_ref().unwrap().folder.join(item_path(&task.id));
    settle_net(&mut a, &mut b, |_, _| {
        std::fs::read_to_string(&theirs).is_ok_and(|t| t.contains("\"done\""))
    });
    assert_eq!(task_named(&mut b, "Paint the silo"), Some(done));
    a.leave_team_session();
}

#[test]
fn a_viewer_sees_the_board_but_cannot_change_it() {
    let project = copy_of_example("hello-part", "team-board-viewer");
    let mut a = EditorApp::new(Some(&project));
    let hub = a.host_team_session();
    as_person(&mut a, "Paul");
    a.apply_board_actions(vec![BoardAction::Create {
        kind: axcenity_teamwork::Kind::Task,
        title: "Only Paul's".into(),
    }]);
    let mut b = EditorApp::new(Some(&project));
    let member = hub.lock().unwrap().add_member(Role::Viewer);
    b.join_team_session(hub.clone(), member);
    for _ in 0..4 {
        a.team_frame();
        b.team_frame();
    }
    as_person(&mut b, "Kim");
    assert!(!b.board_writable());
    let task = task_named(&mut b, "Only Paul's").expect("seen");
    let mut changed = task.clone();
    changed.set_status(Status::Done, "Kim", task.created_at + 1);
    b.apply_board_actions(vec![
        BoardAction::Save(Box::new(changed)),
        BoardAction::Create {
            kind: axcenity_teamwork::Kind::Task,
            title: "Kim's".into(),
        },
    ]);
    let (on_disk, _) = Board::load(&project, Copies::Leave);
    assert_eq!(on_disk.len(), 1);
    assert_eq!(on_disk.get(&task.id), Some(&task));
    draw_board(&mut b, "team-board-viewer", egui::vec2(260.0, 300.0));
}

#[test]
fn bugs_are_reported_from_the_last_error_and_from_the_explorer() {
    let project = copy_of_example("hello-part", "team-board-bugs");
    let mut app = EditorApp::new(Some(&project));
    as_person(&mut app, "Sam");
    app.output
        .error("scripts/door.server.lua:12: attempt to index nil with 'Open'");
    app.output.info("Something after it");

    app.team_board.panel.new_kind = axcenity_teamwork::Kind::Bug;
    app.apply_board_actions(vec![BoardAction::ReportLastError]);
    let crash = task_named(
        &mut app,
        "scripts/door.server.lua:12: attempt to index nil with 'Open'",
    )
    .expect("the bug");
    assert_eq!(crash.kind, axcenity_teamwork::Kind::Bug);
    let report = crash.bug.clone().unwrap();
    assert!(report.actual.contains("attempt to index nil"));
    assert!(report.found_in.contains(std::env::consts::OS), "{report:?}");
    assert_eq!(
        crash.links,
        [axcenity_teamwork::Link::File {
            path: "scripts/door.server.lua".into(),
            line: Some(12)
        }]
    );
    assert_eq!(
        app.team_board.panel.open.as_deref(),
        Some(crash.id.as_str())
    );
    assert!(app.layout.is_open(crate::layout::Pane::TeamBoard));

    // The Explorer's Report a Bug links the part.
    let block = crate::console::resolve_path(&app.world, "Workspace.Block").unwrap();
    app.act(crate::explorer::Action::ReportBug(block));
    let about = task_named(&mut app, "Problem with Block").expect("the bug");
    assert_eq!(
        about.links,
        [axcenity_teamwork::Link::Instance("Workspace.Block".into())]
    );
    let mut blocker = crash.clone();
    blocker.set_severity(
        axcenity_teamwork::Severity::Blocker,
        "Sam",
        crash.created_at + 1,
    );
    blocker.bug_mut().steps = "1. Press Play\n2. Open the door".into();
    app.apply_board_actions(vec![BoardAction::Save(Box::new(blocker))]);
    app.apply_board_actions(vec![BoardAction::Create {
        kind: axcenity_teamwork::Kind::Task,
        title: "Fix the door script".into(),
    }]);
    assert!(task_named(&mut app, "Fix the door script").is_some());
    assert_eq!(board(&mut app).open_bugs(), (2, 1));

    app.team_board.panel.open = Some(crash.id.clone());
    draw_board(&mut app, "team-board-bug", egui::vec2(560.0, 860.0));
    app.team_board.panel.open = None;
    app.team_board.panel.filter.kinds = vec![axcenity_teamwork::Kind::Bug];
    draw_board(&mut app, "team-board-bugs", egui::vec2(260.0, 360.0));
}
