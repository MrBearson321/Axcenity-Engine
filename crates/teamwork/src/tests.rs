use std::path::PathBuf;

use crate::board::{is_board_path, item_path};
use crate::*;

/// An empty folder of its own under the temp folder.
fn temp(name: &str) -> PathBuf {
    let folder = std::env::temp_dir().join(format!(
        "axcenity-teamwork-{name}-{}-{}",
        std::process::id(),
        new_id()
    ));
    std::fs::create_dir_all(&folder).unwrap();
    folder
}

#[test]
fn ids_are_new_each_time_and_only_ids_name_items() {
    let ids: std::collections::BTreeSet<String> = (0..2_000).map(|_| new_id()).collect();
    assert_eq!(ids.len(), 2_000);
    assert!(ids.iter().all(|id| is_id(id)));
    assert!(!is_id("../secret"));
    assert!(!is_id("ABCDEFGHJK"));
    let id = ids.iter().next().unwrap();
    assert!(is_board_path(&item_path(id)));
    assert!(is_board_path(&format!("team/board/{id}.conflict-sam.json")));
    assert!(!is_board_path(&format!("team/board/{id}.json.tmp")));
    assert!(!is_board_path("team/board/notes.json"));
    assert!(!is_board_path(&format!("team/board/x/{id}.json")));
}

#[test]
fn a_task_is_one_readable_file_that_reads_back_the_same() {
    let root = temp("round-trip");
    let (mut board, _) = Board::load(&root, Copies::Merge);
    assert!(board.is_empty());
    let mut task = Item::new(Kind::Task, "  Fix the door\n", "Paul", 1_000);
    assert_eq!(task.title, "Fix the door");
    task.assign("Sam", "Paul", 1_010);
    task.assign("sam", "Paul", 1_011);
    task.set_status(Status::InProgress, "Sam", 1_020);
    task.set_priority(Priority::High, "Paul", 1_030);
    task.set_due(Date::parse("2026-11-20"), "Paul", 1_040);
    task.comment("On it", "Sam", 1_050);
    task.add_check("Hinge", "Sam", 1_060);
    task.add_link(Link::Instance("Workspace.Door".into()), "Paul", 1_070);
    task.add_link(
        Link::File {
            path: "scripts/door.server.lua".into(),
            line: Some(12),
        },
        "Paul",
        1_080,
    );
    let id = task.id.clone();
    board.save(&root, task.clone()).unwrap();

    let text = std::fs::read_to_string(root.join(item_path(&id))).unwrap();
    assert!(text.contains("\"title\": \"Fix the door\""), "{text}");
    assert!(text.contains("\"due\": \"2026-11-20\""), "{text}");
    assert!(text.contains("\"what\": \"status\""), "{text}");
    assert!(!root.join(format!("team/board/{id}.json.tmp")).exists());

    let (again, loaded) = Board::load(&root, Copies::Merge);
    assert_eq!(again.get(&id), Some(&task));
    assert!(loaded.merged.is_empty());
    assert_eq!(task.assignees, ["Sam"], "the same name once");
    assert_eq!(task.events.len(), 5);
    assert_eq!(
        task.events[1].describe(Kind::Task),
        "Paul assigned it to Sam"
    );
    assert_eq!(
        task.events[2].describe(Kind::Task),
        "Sam moved it to In progress"
    );

    // Saving it unchanged leaves the file alone.
    let stamp = std::fs::metadata(root.join(item_path(&id)))
        .unwrap()
        .modified()
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    board.save(&root, task.clone()).unwrap();
    let after = std::fs::metadata(root.join(item_path(&id)))
        .unwrap()
        .modified()
        .unwrap();
    assert_eq!(stamp, after);

    board.delete(&root, &id).unwrap();
    assert!(board.get(&id).is_none());
    assert!(!root.join(item_path(&id)).exists());
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn a_damaged_file_is_reported_and_the_rest_still_load() {
    let root = temp("damaged");
    let (mut board, _) = Board::load(&root, Copies::Merge);
    board
        .save(&root, Item::new(Kind::Task, "Good", "Paul", 1))
        .unwrap();
    let bad = new_id();
    std::fs::write(root.join(item_path(&bad)), "{ not json").unwrap();
    std::fs::write(root.join("team/board/readme.txt"), "notes").unwrap();
    let (board, _) = Board::load(&root, Copies::Merge);
    assert_eq!(board.len(), 1);
    assert_eq!(board.problems.len(), 1);
    assert!(board.problems[0].contains(&bad));
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn two_peoples_changes_to_one_task_both_survive() {
    let root = temp("merge");
    let (mut board, _) = Board::load(&root, Copies::Merge);
    let mut task = Item::new(Kind::Task, "Lighting pass", "Paul", 100);
    task.add_check("Sun", "Paul", 101);
    board.save(&root, task.clone()).unwrap();
    let id = task.id.clone();

    // Paul and Sam each change the same version.
    let mut pauls = task.clone();
    pauls.comment("Use the new HDRI", "Paul", 200);
    pauls.set_priority(Priority::Urgent, "Paul", 210);
    let mut sams = task.clone();
    sams.comment("Clouds look flat", "Sam", 205);
    sams.set_status(Status::InReview, "Sam", 220);
    sams.checklist[0].done = true;
    sams.add_check("Fog", "Sam", 221);

    // Paul's save won; Sam's was kept beside it, as a session does.
    board.save(&root, pauls).unwrap();
    let copy = root.join(format!("team/board/{id}.conflict-sam.json"));
    std::fs::write(&copy, serde_json::to_string_pretty(&sams).unwrap()).unwrap();

    // Left alone while another computer decides.
    let (left, loaded) = Board::load(&root, Copies::Leave);
    assert!(loaded.merged.is_empty());
    assert!(copy.exists());
    assert_eq!(left.get(&id).unwrap().comments.len(), 1);

    let (merged, loaded) = Board::load(&root, Copies::Merge);
    assert_eq!(loaded.merged, std::slice::from_ref(&id));
    assert!(!copy.exists(), "the copy is gone once merged");
    let item = merged.get(&id).unwrap();
    let comments: Vec<&str> = item.comments.iter().map(|c| c.text.as_str()).collect();
    assert_eq!(comments, ["Use the new HDRI", "Clouds look flat"]);
    // Sam changed it last: his fields win, Paul's priority event stays.
    assert_eq!(item.status, Status::InReview);
    assert_eq!(item.checked(), (1, 2));
    assert!(item.events.iter().any(|e| e.change
        == Change::Priority {
            to: Priority::Urgent
        }));
    assert_eq!(item.created_by, "Paul");
    // And the merge was written, so everyone gets it.
    let (again, _) = Board::load(&root, Copies::Merge);
    assert_eq!(again.get(&id), Some(item));

    // A copy of an item deleted meanwhile brings it back.
    let lone = Item::new(Kind::Task, "Orphan", "Kim", 300);
    std::fs::write(
        root.join(format!("team/board/{}.remote-1a2b3c4.json", lone.id)),
        serde_json::to_string(&lone).unwrap(),
    )
    .unwrap();
    let (again, loaded) = Board::load(&root, Copies::Merge);
    assert_eq!(loaded.merged, std::slice::from_ref(&lone.id));
    assert_eq!(again.get(&lone.id).unwrap().title, "Orphan");
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn the_filter_and_order_follow_the_board() {
    let root = temp("filter");
    let (mut board, _) = Board::load(&root, Copies::Merge);
    let today = Date::parse("2026-11-12").unwrap();
    let mut a = Item::new(Kind::Task, "Paint the barn", "Paul", 1);
    a.assign("Sam", "Paul", 2);
    a.set_due(Some(today.add_days(-1)), "Paul", 3);
    a.labels.push("Art".into());
    let mut b = Item::new(Kind::Task, "Barn physics", "Paul", 4);
    b.set_priority(Priority::Urgent, "Paul", 5);
    let mut c = Item::new(Kind::Task, "Ship it", "Paul", 6);
    c.assign("Paul", "Paul", 7);
    c.set_status(Status::Done, "Paul", 8);
    for item in [&a, &b, &c] {
        board.save(&root, (*item).clone()).unwrap();
    }
    let titles = |filter: &Filter| -> Vec<String> {
        board
            .filtered(filter)
            .iter()
            .map(|i| i.title.clone())
            .collect()
    };
    let mut filter = Filter::default();
    assert_eq!(titles(&filter), ["Barn physics", "Paint the barn"]);
    filter.finished = true;
    assert_eq!(
        titles(&filter),
        ["Barn physics", "Paint the barn", "Ship it"]
    );
    filter.whose = Whose::Person("SAM".into());
    assert_eq!(titles(&filter), ["Paint the barn"]);
    filter.whose = Whose::Unassigned;
    assert_eq!(titles(&filter), ["Barn physics"]);
    filter.whose = Whose::Everyone;
    filter.text = "barn PAINT".into();
    assert_eq!(titles(&filter), ["Paint the barn"]);
    filter.text.clear();
    filter.label = Some("art".into());
    assert_eq!(titles(&filter), ["Paint the barn"]);

    assert_eq!(board.open_for("sam", today), (1, 1));
    assert_eq!(board.open_for("Paul", today), (0, 0));
    assert!(a.is_overdue(today));
    assert_eq!(board.labels(), ["Art"]);
    assert_eq!(
        board.people(&["kim".to_owned(), "Sam".to_owned()]),
        ["kim", "Paul", "Sam"]
    );
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn text_is_tidied_and_kept_short() {
    let mut task = Item::new(Kind::Task, "a\u{7}b\tc", "Paul", 1);
    assert_eq!(task.title, "ab c");
    assert!(!task.comment("   ", "Paul", 2));
    let long = "x".repeat(item::MAX_TEXT + 50);
    assert!(task.comment(&long, "Paul", 3));
    assert_eq!(task.comments[0].text.len(), item::MAX_TEXT);
    task.rename("", "Paul", 4);
    assert_eq!(task.title, "ab c", "an empty title isn't taken");
    task.unassign("Nobody", "Paul", 5);
    assert_eq!(task.events.len(), 1, "nothing happened");
}

#[test]
fn a_bug_reports_what_happened_and_sorts_by_severity() {
    let root = temp("bugs");
    let (mut board, _) = Board::load(&root, Copies::Merge);
    let message = "scripts/door.server.lua:12: attempt to index nil with 'Open'\nstack traceback:\n  scripts/door.server.lua:12";
    let mut crash = Item::bug_from_error(message, "0.2.0-alpha.1 on Linux", "Sam", 10);
    assert_eq!(crash.kind, Kind::Bug);
    assert_eq!(
        crash.title,
        "scripts/door.server.lua:12: attempt to index nil with 'Open'"
    );
    assert_eq!(crash.bug.as_ref().unwrap().actual, message);
    assert_eq!(
        crash.bug.as_ref().unwrap().found_in,
        "0.2.0-alpha.1 on Linux"
    );
    crash.set_severity(Severity::Blocker, "Sam", 11);
    crash.set_severity(Severity::Blocker, "Sam", 12);
    assert_eq!(crash.events.len(), 2, "one change, one event");
    assert_eq!(
        crash.events[1].describe(Kind::Bug),
        "Sam set the severity to Blocker"
    );
    crash.set_status(Status::InReview, "Sam", 13);
    assert_eq!(
        crash.events[2].describe(Kind::Bug),
        "Sam moved it to Fix in review"
    );
    crash.set_status(Status::Todo, "Sam", 14);

    let long = "x".repeat(200);
    let typo = Item::bug_from_error(&long, "", "Paul", 20);
    assert_eq!(typo.title.chars().count(), 90);
    assert!(typo.title.ends_with('…'));
    assert_eq!(typo.severity(), Severity::Minor);
    let task = Item::new(Kind::Task, "Not a bug", "Paul", 30);
    assert!(task.bug.is_none());

    for item in [&crash, &typo, &task] {
        board.save(&root, (*item).clone()).unwrap();
    }
    let text = std::fs::read_to_string(root.join(item_path(&crash.id))).unwrap();
    assert!(text.contains("\"severity\": \"blocker\""), "{text}");
    let task_text = std::fs::read_to_string(root.join(item_path(&task.id))).unwrap();
    assert!(!task_text.contains("\"bug\""), "{task_text}");
    let (board, _) = Board::load(&root, Copies::Merge);
    assert_eq!(board.get(&crash.id), Some(&crash));
    assert_eq!(board.open_bugs(), (2, 1));

    let bugs = Filter {
        kinds: vec![Kind::Bug],
        ..Filter::default()
    };
    let order: Vec<&str> = board
        .filtered(&bugs)
        .iter()
        .map(|i| i.id.as_str())
        .collect();
    assert_eq!(order, [crash.id.as_str(), typo.id.as_str()]);
    let found = Filter {
        text: "index nil".into(),
        ..Filter::default()
    };
    assert_eq!(
        board.filtered(&found).len(),
        1,
        "the error text is searched"
    );
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn a_goals_progress_is_its_items_and_my_work_comes_soonest_first() {
    let root = temp("goals");
    let (mut board, _) = Board::load(&root, Copies::Merge);
    let today = Date::parse("2026-11-12").unwrap();
    let mut alpha = Item::new(Kind::Goal, "Alpha build", "Paul", 10);
    alpha.set_due(Some(today.add_days(14)), "Paul", 11);
    assert_eq!(Status::Done.name(Kind::Goal), "Reached");

    let mut door = Item::new(Kind::Task, "Door opens", "Paul", 20);
    door.set_goal(Some(&alpha), "Paul", 21);
    assert_eq!(door.goal.as_deref(), Some(alpha.id.as_str()));
    assert_eq!(
        door.events[1].describe(Kind::Task),
        "Paul put it under “Alpha build”"
    );
    door.set_goal(Some(&alpha), "Paul", 22);
    assert_eq!(door.events.len(), 2, "no change, no event");
    door.assign("Sam", "Paul", 23);
    door.set_due(Some(today.add_days(3)), "Paul", 24);

    let mut crash = Item::bug_from_error("Crash on Play", "", "Sam", 30);
    crash.set_goal(Some(&alpha), "Sam", 31);
    crash.assign("Sam", "Sam", 32);
    crash.set_due(Some(today.add_days(-1)), "Sam", 33);
    let mut menu = Item::new(Kind::Task, "Main menu", "Paul", 40);
    menu.set_goal(Some(&alpha), "Paul", 41);
    menu.set_status(Status::Done, "Paul", 42);
    let mut later = Item::new(Kind::Task, "Polish", "Sam", 50);
    later.assign("Sam", "Sam", 51);

    // Goals don't nest, and only goals hold things.
    let mut beta = Item::new(Kind::Goal, "Beta", "Paul", 60);
    beta.set_goal(Some(&alpha), "Paul", 61);
    assert_eq!(beta.goal, None);
    let mut stray = Item::new(Kind::Task, "Stray", "Paul", 70);
    stray.set_goal(Some(&door), "Paul", 71);
    assert_eq!(stray.goal, None);

    for item in [&alpha, &door, &crash, &menu, &later, &beta] {
        board.save(&root, (*item).clone()).unwrap();
    }
    let (board, _) = Board::load(&root, Copies::Merge);
    assert_eq!(board.get(&door.id), Some(&door));
    let progress = board.progress(&alpha.id, today);
    assert_eq!(
        progress,
        Progress {
            done: 1,
            total: 3,
            overdue: 1
        }
    );
    assert!((progress.fraction() - 1.0 / 3.0).abs() < 1e-6);
    assert_eq!(board.progress(&beta.id, today).fraction(), 0.0);
    let goals: Vec<&str> = board.goals().iter().map(|g| g.title.as_str()).collect();
    assert_eq!(goals, ["Alpha build", "Beta"]);
    let titles =
        |items: Vec<&Item>| -> Vec<String> { items.iter().map(|i| i.title.clone()).collect() };
    assert_eq!(
        titles(board.in_goal(&alpha.id)),
        ["Crash on Play", "Door opens", "Main menu"]
    );
    assert_eq!(
        titles(board.my_work("sam")),
        ["Crash on Play", "Door opens", "Polish"]
    );
    assert_eq!(board.open_for("Sam", today), (3, 1));

    // Taken out of the goal, it no longer counts.
    door.set_goal(None, "Sam", 80);
    assert_eq!(
        door.events.last().unwrap().describe(Kind::Task),
        "Sam took it out of its goal"
    );
    let text = serde_json::to_string_pretty(&door).unwrap();
    assert!(!text.contains("\"goal\": \""), "{text}");
}

#[test]
fn each_persons_history_is_kept_per_computer_and_read_back_by_day() {
    use crate::history::{self, Entry, History, Recorder, What};
    let root = temp("history");
    let day = 1_794_441_600; // 2026-11-12 00:00 UTC
    assert_eq!(Date::of(day), Date::parse("2026-11-12").unwrap());
    assert_eq!(history::person_folder("Paul J."), "paul-j");
    assert_eq!(history::person_folder("  "), "someone");
    assert_eq!(
        history::file_for("Paul J.", day, "AB12-CD34"),
        "team/history/paul-j/2026-11.ab12-cd34.jsonl"
    );
    assert!(history::is_history_path("team/history/sam/2026-11.x.jsonl"));
    assert!(!history::is_history_path("team/historyx"));
    assert!(!history::is_history_path("team/board/a.json"));

    // A drag sends the same change many times: one line, counted.
    let mut recorder = Recorder::default();
    let block = Link::Instance("Workspace.Block".into());
    for second in 0..5 {
        recorder.note(
            Entry::new(day + 10 + second, "Sam", What::Edit, "Move Block")
                .with_links([block.clone()]),
        );
    }
    recorder.note(Entry::new(day + 12, "Paul", What::Edit, "Move Block"));
    assert!(recorder.take(day + 20, false).is_empty(), "still folding");
    let written = recorder.take(day + 100, false);
    assert_eq!(written.len(), 2);
    assert_eq!(written[0].who, "Sam");
    assert_eq!(written[0].count, 5);
    assert_eq!(written[0].links, std::slice::from_ref(&block));
    assert!(recorder.is_empty());
    history::append(&root, "pc", &written).unwrap();

    // Another computer's lines for Sam, the next day, in a file of its own.
    let mut task = Item::new(Kind::Task, "Door opens", "Sam", day + 86_400);
    let made = history::board_entry(None, &task, "Sam", day + 86_400);
    assert_eq!(made.text, "Created task “Door opens”");
    let before = task.clone();
    task.set_status(Status::Done, "Sam", day + 86_460);
    let moved = history::board_entry(Some(&before), &task, "Sam", day + 86_460);
    assert_eq!(moved.text, "“Door opens”: moved it to Done");
    assert_eq!(moved.item.as_deref(), Some(task.id.as_str()));
    let saved = Entry::new(day + 86_500, "sam", What::Save, "Saved 2 changes").with_details([
        "Moved Part \"Block\" in Workspace".to_owned(),
        "Edited scripts/door.server.lua".to_owned(),
    ]);
    history::append(&root, "laptop", &[made, moved, saved.clone()]).unwrap();
    assert!(root.join("team/history/sam/2026-11.laptop.jsonl").exists());
    assert!(root.join("team/history/sam/2026-11.pc.jsonl").exists());
    std::fs::write(
        root.join("team/history/sam/2026-11.broken.jsonl"),
        "{not json\n",
    )
    .unwrap();

    let read = History::load(&root);
    assert_eq!(read.problems.len(), 1, "{:?}", read.problems);
    assert_eq!(read.people(), ["Sam", "Paul"]);
    let days = read.days("SAM", None);
    assert_eq!(days.len(), 2);
    assert_eq!(days[0].0, Date::parse("2026-11-13").unwrap());
    assert_eq!(days[0].1[0], &saved);
    assert_eq!(days[1].1[0].count, 5);
    assert_eq!(read.of("Sam", Some(What::Board)).len(), 2);
    assert_eq!(read.changes_since("Sam", day + 86_400), 3);
    assert_eq!(read.changes_since("Sam", 0), 8);

    // A copied file doesn't count twice.
    std::fs::copy(
        root.join("team/history/sam/2026-11.pc.jsonl"),
        root.join("team/history/sam/2026-11.pc-copy.jsonl"),
    )
    .unwrap();
    assert_eq!(History::load(&root).changes_since("Sam", 0), 8);
}
