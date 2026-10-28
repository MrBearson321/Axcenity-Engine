//! Phase 48: the Levels panel, editing a level other than the start level,
//! and Play in levels, on copies of the hello-levels example.

use super::tests::{copy_of_example, play_for, printed};
use super::*;
use crate::levels::LevelAction;

fn names(app: &EditorApp) -> Vec<String> {
    axcenity_project::level_names(&app.project.as_ref().unwrap().folder)
}

fn open_level(app: &EditorApp) -> Option<&str> {
    app.project.as_ref().unwrap().level.as_deref()
}

fn has(app: &EditorApp, name: &str) -> bool {
    app.world
        .find_first_child(app.world.workspace(), name)
        .is_some()
}

fn act(app: &mut EditorApp, action: LevelAction) {
    let ctx = theme::test_context();
    app.level_action(&ctx, action);
}

fn played(app: &mut EditorApp, command: &str) {
    let session = app.play.as_mut().expect("playing");
    session
        .run_command(command, &mut app.output)
        .unwrap()
        .unwrap();
}

#[test]
fn a_level_opens_with_the_projects_services_and_saves_to_its_own_file() {
    let folder = copy_of_example("hello-levels", "level-open");
    let mut app = EditorApp::new(None);
    app.open_in_level(&folder, Some("Castle".to_owned()));
    assert_eq!(open_level(&app), Some("Castle"));
    assert!(has(&app, "Tower") && !has(&app, "Lever"));
    // ReplicatedStorage is the project's, from world.json.
    assert!(
        app.world
            .find_first_child(app.world.replicated_storage(), "Shared")
            .is_some()
    );
    let start_before = std::fs::read_to_string(folder.join("world.json")).unwrap();

    let tower = app
        .world
        .find_first_child(app.world.workspace(), "Tower")
        .unwrap();
    app.apply(tower, Edit::Name("Keep".to_owned()));
    assert!(app.save());
    let castle = std::fs::read_to_string(folder.join("levels/Castle.world.json")).unwrap();
    assert!(castle.contains("\"Keep\"") && !castle.contains("replicated_storage"));
    // world.json keeps the Meadow, and its services.
    let start =
        axcenity_project::parse_world(&std::fs::read_to_string(folder.join("world.json")).unwrap())
            .unwrap();
    assert!(start.find_first_child(start.workspace(), "Lever").is_some());
    assert_eq!(
        axcenity_project::write_world(&start),
        axcenity_project::write_world(&axcenity_project::parse_world(&start_before).unwrap())
    );
    // Opening it again shows the saved edit.
    app.open_in_level(&folder, Some("Castle".to_owned()));
    assert!(has(&app, "Keep"));
}

#[test]
fn opening_a_level_asks_about_unsaved_changes_first() {
    let folder = copy_of_example("hello-levels", "level-ask");
    let mut app = EditorApp::new(Some(&folder));
    let open = Discard::OpenLevel(Some("Castle".to_owned()));
    assert_eq!(app.request(open.clone()), Some(open.clone()));
    let lever = app
        .world
        .find_first_child(app.world.workspace(), "Lever")
        .unwrap();
    app.apply(lever, Edit::Name("Handle".to_owned()));
    assert_eq!(app.request(open.clone()), None);
    assert_eq!(app.asking, Some(open.clone()));
    // Saving in the prompt goes ahead once the start level is saved.
    assert_eq!(app.answer(Answer::Save), Some(open));
    let start = std::fs::read_to_string(folder.join("world.json")).unwrap();
    assert!(start.contains("\"Handle\""));
}

#[test]
fn new_duplicate_rename_and_delete_change_level_files_and_undo() {
    let folder = copy_of_example("hello-levels", "level-files");
    let mut app = EditorApp::new(Some(&folder));
    assert_eq!(names(&app), ["Castle", "Dungeon"]);

    act(&mut app, LevelAction::New);
    assert_eq!(names(&app), ["Castle", "Dungeon", "Level"]);
    let new = axcenity_project::read_level(&folder, "Level").unwrap();
    assert!(new.find_first_child(new.workspace(), "Baseplate").is_some());
    app.undo();
    assert_eq!(names(&app), ["Castle", "Dungeon"]);
    app.redo();
    assert_eq!(names(&app), ["Castle", "Dungeon", "Level"]);

    // Duplicating the start level copies its Workspace only.
    act(&mut app, LevelAction::Duplicate("Meadow".to_owned()));
    let copy = axcenity_project::read_level(&folder, "Meadow copy").unwrap();
    assert!(copy.find_first_child(copy.workspace(), "Lever").is_some());
    assert!(
        copy.find_first_child(copy.replicated_storage(), "Shared")
            .is_none()
    );

    act(
        &mut app,
        LevelAction::Rename("Meadow copy".to_owned(), "Field".to_owned()),
    );
    assert_eq!(names(&app), ["Castle", "Dungeon", "Field", "Level"]);
    // A name another level has is refused.
    act(
        &mut app,
        LevelAction::Rename("Field".to_owned(), "castle".to_owned()),
    );
    assert_eq!(names(&app), ["Castle", "Dungeon", "Field", "Level"]);
    assert!(
        printed(&app)
            .last()
            .unwrap()
            .contains("there's already a level called \"castle\"")
    );

    act(&mut app, LevelAction::Delete("Dungeon".to_owned()));
    assert_eq!(names(&app), ["Castle", "Field", "Level"]);
    app.undo();
    assert_eq!(names(&app), ["Castle", "Dungeon", "Field", "Level"]);
    assert!(axcenity_project::read_level(&folder, "Dungeon").is_ok());
    app.undo();
    assert_eq!(names(&app), ["Castle", "Dungeon", "Level", "Meadow copy"]);

    // Renaming the start level only changes the manifest.
    act(
        &mut app,
        LevelAction::Rename("Meadow".to_owned(), "Valley".to_owned()),
    );
    assert_eq!(app.project.as_ref().unwrap().start_level, "Valley");
    assert_eq!(open_level(&app), None);
    assert_eq!(
        axcenity_project::read_manifest(&folder)
            .unwrap()
            .start_level,
        "Valley"
    );
    app.undo();
    assert_eq!(app.project.as_ref().unwrap().start_level, "Meadow");
    assert_eq!(open_level(&app), None);
}

#[test]
fn set_as_start_swaps_the_files_and_the_open_level_follows_through_undo() {
    let folder = copy_of_example("hello-levels", "level-start");
    let mut app = EditorApp::new(Some(&folder));
    act(&mut app, LevelAction::SetStart("Castle".to_owned()));
    let project = app.project.as_ref().unwrap();
    assert_eq!(project.start_level, "Castle");
    // The Meadow, open, is now a level file of its own.
    assert_eq!(open_level(&app), Some("Meadow"));
    assert_eq!(names(&app), ["Dungeon", "Meadow"]);
    let start = axcenity_project::read_start_level(&folder).unwrap();
    assert!(start.find_first_child(start.workspace(), "Tower").is_some());
    assert!(
        start
            .find_first_child(start.replicated_storage(), "Shared")
            .is_some(),
        "the services stay in world.json"
    );
    let meadow = axcenity_project::read_level(&folder, "Meadow").unwrap();
    assert!(
        meadow
            .find_first_child(meadow.workspace(), "Lever")
            .is_some()
    );
    // Saving now writes the Meadow to its new file.
    let lever = app
        .world
        .find_first_child(app.world.workspace(), "Lever")
        .unwrap();
    app.apply(lever, Edit::Name("Handle".to_owned()));
    assert!(app.save());
    assert!(
        std::fs::read_to_string(folder.join("levels/Meadow.world.json"))
            .unwrap()
            .contains("\"Handle\"")
    );

    let mut app = EditorApp::new(Some(&folder));
    assert!(has(&app, "Tower"), "the Castle opens first now");
    act(&mut app, LevelAction::SetStart("Meadow".to_owned()));
    assert_eq!(open_level(&app), Some("Castle"));
    app.undo();
    assert_eq!(app.project.as_ref().unwrap().start_level, "Castle");
    assert_eq!(open_level(&app), None);
    app.redo();
    assert_eq!(open_level(&app), Some("Castle"));
}

#[test]
fn play_starts_in_the_open_level_or_the_start_level_and_adds_the_dungeon() {
    let folder = copy_of_example("hello-levels", "level-play");
    let mut app = EditorApp::new(None);
    app.open_in_level(&folder, Some("Castle".to_owned()));
    app.play();
    played(&mut app, "print(game.LevelService.CurrentLevel)");
    assert_eq!(printed(&app).last(), Some(&"Castle"));
    play_for(&mut app, 0.2);
    // The character spawns in the Castle.
    played(
        &mut app,
        "print(workspace:FindFirstChild('Tower') ~= nil, workspace:FindFirstChild('Lever') == nil)",
    );
    assert_eq!(printed(&app).last(), Some(&"true\ttrue"));
    // AddLevel in the editor's Play, and RemoveLevel.
    played(&mut app, "game.LevelService:AddLevel('Dungeon')");
    played(&mut app, "print(workspace.Dungeon.Cell.ClassName)");
    assert_eq!(printed(&app).last(), Some(&"Part"));
    played(&mut app, "game.LevelService:RemoveLevel('Dungeon')");
    played(&mut app, "print(workspace:FindFirstChild('Dungeon'))");
    assert_eq!(printed(&app).last(), Some(&"nil"));
    app.stop();
    assert!(has(&app, "Tower"), "Stop puts the open level back");

    app.play_from_start = true;
    app.play();
    played(
        &mut app,
        "print(game.LevelService.CurrentLevel, workspace:FindFirstChild('Lever') ~= nil)",
    );
    assert_eq!(printed(&app).last(), Some(&"Meadow\ttrue"));
    app.stop();
    assert!(has(&app, "Tower") && !has(&app, "Lever"));
}

#[test]
fn asset_check_checks_the_levels_that_are_not_open() {
    let folder = copy_of_example("hello-levels", "level-check");
    let castle = folder.join("levels/Castle.world.json");
    let text = std::fs::read_to_string(&castle).unwrap().replace(
        "\"name\": \"Tower\",",
        "\"name\": \"Tower\",\n        \"children\": [{ \"class\": \"Sound\", \"name\": \"Bell\", \"properties\": { \"SoundId\": \"assets/audio/bell.ogg\" } }],",
    );
    std::fs::write(&castle, text).unwrap();
    let app = EditorApp::new(Some(&folder));
    let report = axcenity_project::check_assets_in_level(&folder, &app.world, None);
    let finding = report
        .findings
        .iter()
        .find(|finding| finding.path == "assets/audio/bell.ogg")
        .expect("the Castle's missing sound is found");
    assert!(
        matches!(&finding.site, axcenity_project::Site::Level { file, .. } if file == "levels/Castle.world.json"),
        "{finding:?}"
    );
    assert!(finding.owner.contains("levels/Castle.world.json"));
    // With the Castle open, it's checked as the open world instead, and the
    // start level from world.json.
    let mut app = EditorApp::new(None);
    app.open_in_level(&folder, Some("Castle".to_owned()));
    let report = axcenity_project::check_assets_in_level(&folder, &app.world, Some("Castle"));
    let finding = report
        .findings
        .iter()
        .find(|finding| finding.path == "assets/audio/bell.ogg")
        .unwrap();
    assert!(matches!(
        finding.site,
        axcenity_project::Site::Instance { .. }
    ));
}

#[test]
fn the_levels_panel_lists_the_start_level_and_the_others_in_a_narrow_column() {
    let folder = copy_of_example("hello-levels", "level-panel");
    let mut app = EditorApp::new(None);
    app.open_in_level(&folder, Some("Castle".to_owned()));
    let size = egui::vec2(260.0, 220.0);
    let mut fit = None;
    let mut draw = |ctx: &egui::Context| {
        let palette = theme::palette(ctx);
        let frame = egui::Frame::new()
            .fill(palette.panel)
            .inner_margin(egui::Margin::symmetric(8, 6));
        egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
            let room = ui.max_rect();
            app.levels_pane(ui);
            fit = Some((ui.min_rect(), room));
        });
    };
    let ctx = theme::test_context();
    let mut shown = String::new();
    for _ in 0..3 {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            ..Default::default()
        };
        let output = ctx.run(input, &mut draw);
        shown = format!("{:?}", output.shapes);
    }
    for name in [
        "Meadow",
        "Castle",
        "Dungeon",
        "New Level",
        "Play from start level",
    ] {
        assert!(shown.contains(name), "{name} isn't drawn");
    }
    if let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS").map(PathBuf::from) {
        crate::snapshot::snapshot(
            &folder.join("panel-levels-open.png"),
            size,
            vec![Vec::new(); 3],
            &mut draw,
        );
    }
    let (used, room) = fit.unwrap();
    assert!(used.width() <= room.width() + 0.5);
}
