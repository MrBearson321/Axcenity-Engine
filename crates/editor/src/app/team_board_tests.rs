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

#[test]
fn a_goal_counts_its_tasks_and_my_work_lists_mine_soonest_first() {
    use axcenity_teamwork::Kind;
    let project = copy_of_example("hello-part", "team-board-goals");
    let mut app = EditorApp::new(Some(&project));
    as_person(&mut app, "Sam");
    let today = Date::of(axcenity_teamwork::time::now());

    app.apply_board_actions(vec![BoardAction::Create {
        kind: Kind::Goal,
        title: "Alpha build".into(),
    }]);
    let mut alpha = task_named(&mut app, "Alpha build").expect("the goal");
    assert_eq!(
        app.team_board.panel.open.as_deref(),
        Some(alpha.id.as_str())
    );
    alpha.set_due(Some(today.add_days(10)), "Sam", alpha.created_at + 1);
    app.apply_board_actions(vec![BoardAction::Save(Box::new(alpha.clone()))]);

    // Tasks added from the goal's window belong to it.
    app.apply_board_actions(vec![
        BoardAction::AddToGoal {
            goal: alpha.id.clone(),
            title: "Main menu".into(),
        },
        BoardAction::AddToGoal {
            goal: alpha.id.clone(),
            title: "Door opens".into(),
        },
    ]);
    let mut menu = task_named(&mut app, "Main menu").unwrap();
    assert_eq!(menu.goal.as_deref(), Some(alpha.id.as_str()));
    assert_eq!(
        app.team_board.panel.open.as_deref(),
        Some(alpha.id.as_str())
    );
    menu.set_status(Status::Done, "Sam", menu.created_at + 1);
    let mut door = task_named(&mut app, "Door opens").unwrap();
    door.assign("Sam", "Sam", door.created_at + 1);
    door.set_due(Some(today.add_days(-2)), "Sam", door.created_at + 2);
    // A bug joins the goal from its own window's Goal row.
    let mut crash = Item::bug_from_error("Crash on Play", "", "Paul", door.created_at);
    crash.assign("Sam", "Paul", crash.created_at + 1);
    crash.set_due(Some(today.add_days(3)), "Paul", crash.created_at + 2);
    crash.set_goal(Some(&alpha), "Paul", crash.created_at + 3);
    let mut polish = Item::new(Kind::Task, "Polish", "Sam", door.created_at);
    polish.assign("Sam", "Sam", polish.created_at + 1);
    app.apply_board_actions(vec![
        BoardAction::Save(Box::new(menu)),
        BoardAction::Save(Box::new(door.clone())),
        BoardAction::Save(Box::new(crash)),
        BoardAction::Save(Box::new(polish)),
    ]);
    let progress = board(&mut app).progress(&alpha.id, today);
    assert_eq!((progress.done, progress.total, progress.overdue), (1, 3, 1));
    let mine: Vec<String> = board(&mut app)
        .my_work("Sam")
        .iter()
        .map(|i| i.title.clone())
        .collect();
    assert_eq!(mine, ["Door opens", "Crash on Play", "Polish"]);

    app.team_board.panel.open_item(&alpha);
    draw_board(&mut app, "team-board-goal", egui::vec2(560.0, 760.0));
    app.team_board.panel.open = None;
    app.team_board.panel.view = View::Goals;
    draw_board(&mut app, "team-board-goals", egui::vec2(300.0, 360.0));
    app.team_board.panel.view = View::MyWork;
    draw_board(&mut app, "team-board-my-work", egui::vec2(300.0, 300.0));
    // The list leaves goals to their view.
    app.team_board.panel.view = View::List;
    draw_board(
        &mut app,
        "team-board-list-without-goals",
        egui::vec2(300.0, 300.0),
    );

    // Deleting the goal keeps its tasks, out of the goal.
    app.apply_board_actions(vec![BoardAction::Delete(alpha.id.clone())]);
    let left = board(&mut app);
    assert!(left.get(&alpha.id).is_none());
    assert_eq!(left.len(), 4);
    assert!(left.items().all(|i| i.goal.is_none()));
    assert_eq!(
        left.get(&door.id)
            .unwrap()
            .events
            .last()
            .unwrap()
            .describe(Kind::Task),
        "Sam took it out of its goal"
    );
}

/// Renames `id` as one undo step.
fn rename(app: &mut EditorApp, id: axcenity_core::InstanceId, name: &str) {
    app.change(
        id,
        axcenity_editor_tools::edit::Field::Name,
        format!("Rename to {name}"),
        vec![crate::properties::Edit::Name(name.into())],
    );
    app.history.seal();
}

#[test]
fn each_persons_saves_board_changes_and_session_edits_are_kept_and_shown() {
    use axcenity_teamwork::history::{History, What};
    use axcenity_teamwork::{Kind, Link};
    let project = copy_of_example("hello-part", "team-history");
    let mut a = EditorApp::new(Some(&project));
    as_person(&mut a, "Paul");
    a.team_board.history.computer = Some("PC".into());

    // Outside a session: a save says what changed, and board changes are
    // kept as they're made.
    let block = crate::console::resolve_path(&a.world, "Workspace.Block").unwrap();
    rename(&mut a, block, "Plank");
    assert!(a.save());
    rename(&mut a, block, "Door");
    assert!(a.save());
    a.apply_board_actions(vec![BoardAction::Create {
        kind: Kind::Task,
        title: "Hinge the door".into(),
    }]);
    let mut task = task_named(&mut a, "Hinge the door").unwrap();
    task.set_status(Status::Done, "Paul", task.created_at + 1);
    a.apply_board_actions(vec![BoardAction::Save(Box::new(task.clone()))]);
    let month = Date::of(axcenity_teamwork::time::now());
    let file = project.join(format!(
        "team/history/paul/{:04}-{:02}.pc.jsonl",
        month.year, month.month
    ));
    assert!(file.exists(), "{}", file.display());
    let read = History::load(&project);
    let saves = read.of("Paul", Some(What::Save));
    assert_eq!(saves.len(), 2, "{:?}", read.entries());
    assert_eq!(saves[1].text, "Saved the first version");
    assert!(
        saves[0]
            .links
            .contains(&Link::Instance("Workspace.Door".into())),
        "{:?}",
        saves[0]
    );
    let board_lines: Vec<&str> = read
        .of("Paul", Some(What::Board))
        .iter()
        .map(|e| e.text.as_str())
        .collect();
    assert_eq!(
        board_lines,
        [
            "“Hinge the door”: moved it to Done",
            "Created task “Hinge the door”"
        ]
    );

    // In a session the host records each member's edits from its log, and
    // the members record nothing themselves.
    let invite = over_loopback(
        a.host_network_session(Some(&team_folder("history-host")), 0)
            .expect("hosting"),
    );
    let mut b = EditorApp::new(None);
    as_person(&mut b, "Sam");
    b.join_network_session(&invite, Some(&team_folder("history-joiner")))
        .expect("joining");
    settle_net(&mut a, &mut b, |_, b| b.project.is_some());
    let joiner = b
        .team
        .as_ref()
        .and_then(|team| team.client())
        .and_then(|client| {
            client
                .roster()
                .iter()
                .find(|c| c.member == client.member())
                .map(|c| c.profile.name.clone())
        })
        .expect("the joiner is in the roster");
    let theirs = crate::console::resolve_path(&b.world, "Workspace.Door").unwrap();
    for name in ["Gate", "Gate 2", "Gate 3"] {
        rename(&mut b, theirs, name);
        let path = format!("Workspace.{name}");
        settle_net(&mut a, &mut b, |a, _| {
            crate::console::resolve_path(&a.world, &path).is_some()
        });
    }
    a.harvest_host_log(axcenity_teamwork::time::now());
    a.flush_history();
    let read = History::load(&project);
    let edits = read.of(&joiner, Some(What::Edit));
    assert_eq!(edits.len(), 3, "{:?}", read.entries());
    assert!(
        edits.iter().any(|e| e.text == "Rename to Gate 2"
            && e.links == [Link::Instance("Workspace.Gate 3".into())])
    );
    // The joiner's own folder has no history of its own.
    let joined = b.project.as_ref().unwrap().folder.clone();
    assert!(!joined.join("team/history").exists());

    a.team_board.panel.open = None;
    a.team_board.panel.people = true;
    a.team_board.history.window.person = Some("Paul".into());
    draw_board(&mut a, "team-board-people", egui::vec2(520.0, 620.0));
    a.team_board.history.window.person = Some(joiner);
    draw_board(
        &mut a,
        "team-board-people-session",
        egui::vec2(520.0, 420.0),
    );
}
