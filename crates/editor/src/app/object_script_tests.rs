//! Scripts inside objects in the editor (G2): opening, inserting, Make
//! Unique, renaming and deleting, saving, and reloading every copy in Play.

use super::tests::{copy_of_example, play_for, printed};
use super::*;

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
