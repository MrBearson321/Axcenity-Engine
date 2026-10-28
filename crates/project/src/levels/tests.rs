use std::path::PathBuf;

use axcenity_core::ClassName;

use super::*;
use crate::{level_names, load};

fn example(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .join(name)
}

fn has(world: &World, name: &str) -> bool {
    world.find_first_child(world.workspace(), name).is_some()
}

/// A copy of hello-levels in a folder of its own.
fn copy(test: &str) -> PathBuf {
    fn copy_folder(from: &Path, to: &Path) {
        fs::create_dir_all(to).unwrap();
        for entry in fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            let target = to.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy_folder(&entry.path(), &target);
            } else {
                fs::copy(entry.path(), target).unwrap();
            }
        }
    }
    let folder = std::env::temp_dir().join(format!(
        "axcenity-project-levels-{test}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&folder);
    copy_folder(&example("hello-levels"), &folder);
    folder
}

#[test]
fn a_project_with_only_world_json_has_no_other_levels_and_loads_as_before() {
    let folder = example("hello-part");
    assert!(level_names(&folder).is_empty());
    let project = load(&folder).unwrap();
    assert_eq!(project.manifest.start_level, DEFAULT_START_LEVEL);
    let mut problems = Vec::new();
    assert!(load_levels(&folder, &mut problems).is_empty());
    assert!(problems.is_empty());
}

#[test]
fn hello_levels_lists_its_levels_and_each_round_trips_through_saving() {
    let folder = example("hello-levels");
    assert_eq!(level_names(&folder), ["Castle", "Dungeon"]);
    assert_eq!(load(&folder).unwrap().manifest.start_level, "Meadow");
    for name in level_names(&folder) {
        let first = write_level(&read_level(&folder, &name).unwrap());
        let second = write_level(&parse_world(&first).unwrap());
        assert_eq!(first, second, "levels/{name} changed when saved");
        assert!(!first.contains("replicated_storage"));
    }
}

#[test]
fn a_level_loads_with_the_projects_services_and_scripts() {
    let folder = example("hello-levels");
    let project = load_with_level(&folder, Some("Castle"), &mut |_| {}).unwrap();
    assert!(project.problems.is_empty(), "{:?}", project.problems);
    let world = &project.world;
    assert!(has(world, "Tower") && !has(world, "Lever"));
    assert!(
        world
            .find_first_child(world.replicated_storage(), "Shared")
            .is_some()
    );
    assert_eq!(project.scripts.len(), 3);
    // A level that isn't there leaves the Workspace empty, and says so.
    let project = load_with_level(&folder, Some("Moon"), &mut |_| {}).unwrap();
    assert!(!project.world_loaded);
    assert!(matches!(project.problems[..], [LoadError::Read { .. }]));
    assert!(
        project
            .world
            .get(project.world.workspace())
            .unwrap()
            .children()
            .is_empty()
    );
}

#[test]
fn saving_a_level_writes_its_workspace_there_and_the_services_to_world_json() {
    let folder = copy("save");
    let mut world = load_with_level(&folder, Some("Castle"), &mut |_| {})
        .unwrap()
        .world;
    let storage = world.replicated_storage();
    let added = world.create(ClassName::Folder).unwrap();
    world.set_name(added, "Added").unwrap();
    world.set_parent(added, Some(storage)).unwrap();
    let tower = world.find_first_child(world.workspace(), "Tower").unwrap();
    world.set_name(tower, "Keep").unwrap();
    save_level(&folder, Some("Castle"), &world).unwrap();

    let castle = read_level(&folder, "Castle").unwrap();
    assert!(has(&castle, "Keep"));
    let start = read_start_level(&folder).unwrap();
    assert!(has(&start, "Lever") && !has(&start, "Keep"));
    assert!(
        start
            .find_first_child(start.replicated_storage(), "Added")
            .is_some()
    );
}

#[test]
fn set_start_level_swaps_the_workspaces_and_names_the_new_start() {
    let folder = copy("start");
    let changes = set_start_level(&folder, "Meadow", "Castle").unwrap();
    write_files(&folder, &changes).unwrap();
    assert_eq!(level_names(&folder), ["Dungeon", "Meadow"]);
    let start = read_start_level(&folder).unwrap();
    assert!(has(&start, "Tower"));
    assert!(
        start
            .find_first_child(start.replicated_storage(), "Shared")
            .is_some()
    );
    assert!(has(&read_level(&folder, "Meadow").unwrap(), "Lever"));
    assert_eq!(load(&folder).unwrap().manifest.start_level, "Castle");
}

#[test]
fn level_names_follow_file_name_rules_and_must_be_new() {
    let taken = ["Castle".to_owned()];
    assert!(check_level_name("Dungeon", "Meadow", &taken).is_ok());
    assert_eq!(
        check_level_name("castle", "Meadow", &taken),
        Err("there's already a level called \"castle\"".to_owned())
    );
    assert!(check_level_name("Meadow", "Meadow", &taken).is_err());
    assert_eq!(
        check_level_name("a/b", "Meadow", &taken),
        Err(r#"level names can't contain any of / \ : * ? " < > |"#.to_owned())
    );
}
