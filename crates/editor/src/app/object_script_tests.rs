//! Scripts inside objects in the editor (G2): opening, inserting, Make
//! Unique, renaming and deleting, saving, and reloading every copy in Play.

use super::tests::{copy_of_example, play_for, printed};
use super::*;
use crate::scripts::ScriptView;
use axcenity_project as project;

const DOOR: &str = "objects/Door/Open.server.lua";

/// The script `name` inside the Workspace's `object`.
fn script_in(app: &EditorApp, object: &str, name: &str) -> InstanceId {
    let world = &app.world;
    let object = world.find_first_child(world.workspace(), object).unwrap();
    world.find_first_child(object, name).unwrap()
}

fn source_file(app: &EditorApp, id: InstanceId) -> Option<String> {
    app.world
        .get(id)
        .and_then(|instance| instance.source_file())
        .map(str::to_owned)
}

#[test]
fn opening_a_copys_script_opens_the_file_every_copy_shares() {
    let folder = copy_of_example("hello-object-scripts", "object-script-open");
    let mut app = EditorApp::new(Some(&folder));
    let project = app.project.as_ref().unwrap();
    // Only the `scripts` folder's script stands for a file of its own.
    let paths: Vec<&str> = project.scripts.iter().map(|s| s.path.as_str()).collect();
    assert_eq!(paths, ["scripts/coins.server.lua"]);
    assert!(project.object_files.contains_key(DOOR));

    let back = script_in(&app, "BackDoor", "Open");
    app.open_script_instance(back);
    assert_eq!(app.tabs.active().unwrap().path, DOOR);
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn insert_script_on_an_object_makes_its_file_in_objects_as_one_undo_step() {
    let folder = copy_of_example("hello-object-scripts", "object-script-insert");
    let mut app = EditorApp::new(Some(&folder));
    let door = app
        .world
        .find_first_child(app.world.workspace(), "FrontDoor")
        .unwrap();
    app.selected = Some(door);
    app.selection = vec![door];
    app.act(Action::InsertScript(ClassName::Script));

    let path = "objects/FrontDoor/Script.server.lua";
    let script = script_in(&app, "FrontDoor", "Script");
    assert_eq!(source_file(&app, script).as_deref(), Some(path));
    assert!(folder.join(path).exists());
    assert_eq!(app.tabs.active().unwrap().path, path);
    // Not a script from the `scripts` folder.
    assert!(app.project.as_ref().unwrap().scripts.len() == 1);

    app.undo();
    assert!(!folder.join(path).exists(), "undo takes the file away too");
    assert!(app.world.find_first_child(door, "Script").is_none());
    app.redo();
    assert!(folder.join(path).exists());
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn make_unique_gives_one_copy_its_own_file_and_leaves_the_others() {
    let folder = copy_of_example("hello-object-scripts", "object-script-unique");
    let mut app = EditorApp::new(Some(&folder));
    let back = script_in(&app, "BackDoor", "Open");
    let front = script_in(&app, "FrontDoor", "Open");
    app.act(Action::MakeUnique(back));

    let unique = "objects/Door/Open 2.server.lua";
    assert_eq!(source_file(&app, back).as_deref(), Some(unique));
    assert_eq!(source_file(&app, front).as_deref(), Some(DOOR));
    assert_eq!(
        std::fs::read_to_string(folder.join(unique)).unwrap(),
        std::fs::read_to_string(folder.join(DOOR)).unwrap()
    );
    app.undo();
    assert_eq!(source_file(&app, back).as_deref(), Some(DOOR));
    assert!(!folder.join(unique).exists());
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn renaming_or_deleting_a_copys_script_keeps_the_shared_file() {
    let folder = copy_of_example("hello-object-scripts", "object-script-delete");
    let mut app = EditorApp::new(Some(&folder));
    let back = script_in(&app, "BackDoor", "Open");
    app.act(Action::Rename(back, "Slide".into()));
    assert_eq!(app.world.get(back).unwrap().name(), "Slide");
    assert_eq!(source_file(&app, back).as_deref(), Some(DOOR));

    app.act(Action::Delete(back));
    let in_tree = |app: &EditorApp| app.world.get(back).and_then(|i| i.parent()).is_some();
    assert!(!in_tree(&app));
    assert!(app.confirm_delete.is_none(), "no file to ask about");
    assert!(folder.join(DOOR).exists());
    app.undo();
    assert!(in_tree(&app));
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn saving_a_shared_file_during_play_reloads_every_copy() {
    let folder = copy_of_example("hello-object-scripts", "object-script-reload");
    let mut app = EditorApp::new(Some(&folder));
    app.play();
    play_for(&mut app, 0.5);
    assert!(printed(&app).contains(&"FrontDoor is ready"));

    let back = script_in(&app, "BackDoor", "Open");
    app.open_script_instance(back);
    app.tabs.get_mut(DOOR).unwrap().text = "print(script.Parent.Name .. ' reloaded')\n".into();
    let from = printed(&app).len();
    assert!(app.save());
    play_for(&mut app, 0.1);
    let after = printed(&app)[from..].to_vec();
    assert!(after.contains(&"FrontDoor reloaded"), "{after:?}");
    assert!(after.contains(&"BackDoor reloaded"), "{after:?}");
    assert!(
        after.contains(&"Reloaded Open.server.lua in 2 copies"),
        "{after:?}"
    );
    // No script from the `scripts` folder was made for the file.
    assert_eq!(app.project.as_ref().unwrap().scripts.len(), 1);
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn rename_or_move_a_shared_file_points_every_copy_at_it_as_one_undo_step() {
    let folder = copy_of_example("hello-object-scripts", "object-script-move");
    let mut app = EditorApp::new(Some(&folder));
    let back = script_in(&app, "BackDoor", "Open");
    let front = script_in(&app, "FrontDoor", "Open");
    app.open_script_instance(back);
    let swing = "objects/Door/Swing.server.lua";
    app.rename_asset(DOOR, swing);

    for copy in [back, front] {
        assert_eq!(source_file(&app, copy).as_deref(), Some(swing));
    }
    assert!(folder.join(swing).exists() && !folder.join(DOOR).exists());
    let prefab = std::fs::read_to_string(folder.join("assets/prefabs/Door.prefab.json")).unwrap();
    assert!(prefab.contains(swing), "{prefab}");
    let files = &app.project.as_ref().unwrap().object_files;
    assert!(files.contains_key(swing) && !files.contains_key(DOOR));
    assert_eq!(app.tabs.active().unwrap().path, swing, "its tab follows it");

    app.undo();
    assert_eq!(source_file(&app, back).as_deref(), Some(DOOR));
    assert!(folder.join(DOOR).exists() && !folder.join(swing).exists());
    let files = &app.project.as_ref().unwrap().object_files;
    assert!(files.contains_key(DOOR) && !files.contains_key(swing));
    assert_eq!(app.tabs.active().unwrap().path, DOOR);
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn find_in_scripts_and_quick_open_include_the_code_of_scripts_inside_objects() {
    use super::search::SearchTarget;
    use crate::command_palette::Category;

    let folder = copy_of_example("hello-object-scripts", "object-script-find");
    let mut app = EditorApp::new(Some(&folder));
    app.find.query = "slides up".to_owned();
    app.search_scripts();
    let hits = app.find.results.clone().unwrap();
    let paths: Vec<&str> = hits.iter().map(|hit| hit.path.as_str()).collect();
    assert_eq!(paths, [DOOR], "one hit for the file, however many copies");

    app.find.replacement = "rises".to_owned();
    app.replace_all_in_scripts();
    let written = std::fs::read_to_string(folder.join(DOOR)).unwrap();
    assert!(written.contains("the door rises"), "{written}");
    assert!(app.project.as_ref().unwrap().object_files[DOOR].contains("rises"));

    let ctx = theme::test_context();
    app.refresh_search(&ctx, true);
    let target = app
        .search
        .rows(Category::Script)
        .into_iter()
        .find(|(entry, _)| entry.title == "Open")
        .map(|(_, target)| target.clone())
        .expect("the door's code is listed");
    assert!(matches!(
        &target,
        SearchTarget::Open(super::command_palette::OpenTarget::Script(path)) if path == DOOR
    ));
    app.run_search_target(&ctx, target);
    assert_eq!(app.tabs.active_path(), Some(DOOR));
    let _ = std::fs::remove_dir_all(&folder);
}

/// Code ↔ Blocks for a script inside an object (CF11): its shared file is
/// replaced by one with the other ending, every copy and the prefab point
/// at it, its tab follows, and each switch is one undo step.
#[test]
fn switching_a_shared_file_to_blocks_and_back_points_every_copy_at_it() {
    let folder = copy_of_example("hello-object-scripts", "object-script-switch");
    let mut app = EditorApp::new(Some(&folder));
    let back = script_in(&app, "BackDoor", "Open");
    let front = script_in(&app, "FrontDoor", "Open");
    app.open_script_instance(back);
    let original = std::fs::read_to_string(folder.join(DOOR)).unwrap();
    let blocks = "objects/Door/Open.server.blocks.json";

    app.switch_script_view(DOOR, ScriptView::Blocks);
    let state = |app: &EditorApp, path: &str| {
        for copy in [back, front] {
            assert_eq!(source_file(app, copy).as_deref(), Some(path));
        }
        let other = project::switched_script_path(path).unwrap();
        assert!(folder.join(path).exists() && !folder.join(&other).exists());
        let prefab =
            std::fs::read_to_string(folder.join("assets/prefabs/Door.prefab.json")).unwrap();
        assert!(prefab.contains(path), "{prefab}");
        let project = app.project.as_ref().unwrap();
        assert!(project.object_files[path].contains("opens"));
        assert!(!project.object_files.contains_key(&other));
        assert_eq!(app.tabs.active().unwrap().path, path, "its tab follows it");
        let documents: Vec<&str> = project
            .block_scripts
            .iter()
            .map(|document| document.path.as_str())
            .collect();
        let expected: &[&str] = if project::is_block_script(path) {
            &[blocks]
        } else {
            &[]
        };
        assert_eq!(documents, expected);
        assert_eq!(project.scripts.len(), 1, "no Script of its own");
    };
    state(&app, blocks);

    app.switch_script_view(blocks, ScriptView::Code);
    state(&app, DOOR);

    app.undo();
    state(&app, blocks);
    app.undo();
    state(&app, DOOR);
    assert_eq!(
        std::fs::read_to_string(folder.join(DOOR)).unwrap(),
        original
    );
    assert_eq!(app.tabs.active().unwrap().text, original);
    let _ = std::fs::remove_dir_all(&folder);
}

/// A block script inside an object opens in a Blocks tab from any copy,
/// and saving it rewrites the shared file, not a new script.
#[test]
fn a_block_script_inside_an_object_opens_and_saves_its_shared_file() {
    let folder = copy_of_example("hello-object-scripts", "object-script-blocks");
    let mut app = EditorApp::new(Some(&folder));
    let back = script_in(&app, "BackDoor", "Open");
    let front = script_in(&app, "FrontDoor", "Open");
    app.open_script_instance(back);
    app.switch_script_view(DOOR, ScriptView::Blocks);
    let blocks = "objects/Door/Open.server.blocks.json";
    // As a project opened with the block file already there.
    let mut app = EditorApp::new(Some(&folder));
    assert!(app.project.as_ref().unwrap().block_scripts.is_empty());

    app.open_script_instance(front);
    assert_eq!(app.tabs.active().unwrap().path, blocks);
    let document = app
        .project
        .as_mut()
        .unwrap()
        .block_scripts
        .iter_mut()
        .find(|document| document.path == blocks)
        .expect("a Blocks tab");
    document.script.graph.shown = true;
    assert!(app.write_tab(blocks));
    let written = std::fs::read_to_string(folder.join(blocks)).unwrap();
    let saved: axcenity_blocks::BlockScript = serde_json::from_str(&written).unwrap();
    assert!(saved.graph.shown);
    let project = app.project.as_ref().unwrap();
    assert_eq!(project.scripts.len(), 1, "no Script of its own");
    assert!(project.object_files[blocks].contains("opens"));
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn dragging_a_script_onto_an_object_attaches_it_and_undo_moves_it_back() {
    let folder = copy_of_example("hello-object-scripts", "object-script-attach");
    let mut app = EditorApp::new(Some(&folder));
    let coins = app
        .world
        .find_first_child(app.world.server_script_service(), "coins")
        .unwrap();
    let door = app
        .world
        .find_first_child(app.world.workspace(), "FrontDoor")
        .unwrap();
    let old = "scripts/coins.server.lua";
    let new = "objects/FrontDoor/coins.server.lua";
    let source = std::fs::read_to_string(folder.join(old)).unwrap();
    app.open_script(old, None);

    // Dropped on the door in the Explorer: asked first, nothing moves yet.
    app.act(Action::MoveInto {
        id: coins,
        parent: door,
    });
    assert_eq!(app.confirm_attach, Some((coins, door)));
    assert!(folder.join(old).exists());
    app.confirm_attach = None;
    app.attach_script(coins, door);

    let attached = script_in(&app, "FrontDoor", "coins");
    assert_eq!(source_file(&app, attached).as_deref(), Some(new));
    assert!(!folder.join(old).exists());
    assert_eq!(std::fs::read_to_string(folder.join(new)).unwrap(), source);
    assert!(app.project.as_ref().unwrap().scripts.is_empty());
    assert!(!app.world.contains(coins), "no Script left in the service");
    assert_eq!(app.tabs.active().unwrap().path, new, "the tab follows");

    app.undo();
    assert!(folder.join(old).exists());
    assert!(!folder.join(new).exists());
    let paths: Vec<String> = app
        .project
        .as_ref()
        .unwrap()
        .scripts
        .iter()
        .map(|script| script.path.clone())
        .collect();
    assert_eq!(paths, [old]);
    assert!(app.world.contains(coins), "the service's Script is back");
    assert!(app.world.find_first_child(door, "coins").is_none());
    assert!(app.tabs.get(old).is_some());

    app.redo();
    assert!(!folder.join(old).exists());
    assert!(folder.join(new).exists());
    assert!(app.project.as_ref().unwrap().scripts.is_empty());
    assert!(app.world.find_first_child(door, "coins").is_some());
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn only_scripts_and_local_scripts_attach_and_only_to_saved_objects() {
    let folder = copy_of_example("hello-object-scripts", "object-script-attach-refused");
    let mut app = EditorApp::new(Some(&folder));
    let world = &mut app.world;
    let coins = world
        .find_first_child(world.server_script_service(), "coins")
        .unwrap();
    let door = world
        .find_first_child(world.workspace(), "FrontDoor")
        .unwrap();
    let module = project::add_script_instance(world, "scripts/Shared.module.lua");
    assert!(tree::check_attach(world, coins, door).is_ok());
    assert!(tree::check_attach(world, module, door).is_err());
    assert!(tree::check_attach(world, coins, world.workspace()).is_err());
    let lighting = world.lighting();
    assert!(tree::check_attach(world, coins, lighting).is_err());
    let open = world.find_first_child(door, "Open").unwrap();
    assert!(
        tree::check_attach(world, coins, open).is_err(),
        "not into a script"
    );
    let _ = std::fs::remove_dir_all(&folder);
}
