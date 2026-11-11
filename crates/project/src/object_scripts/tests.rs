use super::*;

/// An empty folder in the system's temporary folder, unique to this test run.
fn temp_folder(name: &str) -> PathBuf {
    let folder = std::env::temp_dir().join(format!("axcenity-{name}-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&folder);
    fs::create_dir_all(&folder).unwrap();
    folder
}

/// A project with a manifest and these files.
fn project(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let folder = temp_folder(name);
    fs::write(
        folder.join(MANIFEST_FILE),
        format!(r#"{{"name": "T", "format_version": {FORMAT_VERSION}}}"#),
    )
    .unwrap();
    for (path, text) in files {
        let path = folder.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    folder
}

const DOOR_PREFAB: &str = r#"{ "format_version": 2, "instances": [
    { "class": "Model", "name": "Door", "children": [
        { "class": "Part", "name": "Panel" },
        { "class": "Script", "name": "Open",
          "properties": { "SourceFile": "objects/Door/Open.server.lua" } } ] } ] }"#;

/// Two copies of a door prefab, a lamp with a LocalScript, a ModuleScript
/// in a model and a template in ServerStorage, beside the `scripts`
/// folder's own.
const WORLD: &str = r#"{ "format_version": 2,
  "workspace": { "children": [
    { "class": "Model", "name": "Door", "prefab": "assets/prefabs/Door.prefab.json" },
    { "class": "Model", "name": "Lamp", "children": [
        { "class": "LocalScript", "name": "Glow",
          "properties": { "SourceFile": "objects/Lamp/Glow.client.lua" } },
        { "class": "ModuleScript", "name": "Colors",
          "properties": { "SourceFile": "objects/Lamp/Colors.module.lua" } } ] },
    { "class": "Model", "name": "Door", "prefab": "assets/prefabs/Door.prefab.json" } ] },
  "server_storage": { "children": [
    { "class": "Model", "name": "Coin", "children": [
        { "class": "Script", "name": "Spin",
          "properties": { "SourceFile": "objects/Coin/Spin.server.lua" } } ] } ] } }"#;

fn door_project(name: &str) -> PathBuf {
    project(
        name,
        &[
            (WORLD_FILE, WORLD),
            ("assets/prefabs/Door.prefab.json", DOOR_PREFAB),
            ("objects/Door/Open.server.lua", "-- open"),
            ("objects/Lamp/Glow.client.lua", "-- glow"),
            ("objects/Lamp/Colors.module.lua", "return {}"),
            ("objects/Coin/Spin.server.lua", "-- spin"),
            ("scripts/main.server.lua", "-- main"),
            ("scripts/keys.client.lua", "-- keys"),
            ("scripts/shared.module.lua", "return {}"),
        ],
    )
}

#[test]
fn scripts_inside_objects_load_in_start_order_with_each_copy_its_own() {
    let folder = door_project("object-scripts-order");
    let project = load(&folder).unwrap();
    assert!(
        project.problems.is_empty(),
        "{:?}",
        project
            .problems
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
    );
    let order: Vec<(String, &str)> = project
        .scripts
        .iter()
        .map(|script| {
            (
                project.world.full_name(script.instance).unwrap(),
                script.path.as_str(),
            )
        })
        .collect();
    // Modules, then templates (kept, not started, so clones have code),
    // then Scripts, then LocalScripts: the `scripts` folder's by path, then
    // those inside objects in tree order.
    let expected = [
        ("ReplicatedStorage.shared", "scripts/shared.module.lua"),
        ("Workspace.Lamp.Colors", "objects/Lamp/Colors.module.lua"),
        ("ServerStorage.Coin.Spin", "objects/Coin/Spin.server.lua"),
        ("ServerScriptService.main", "scripts/main.server.lua"),
        ("Workspace.Door.Open", "objects/Door/Open.server.lua"),
        ("Workspace.Door.Open", "objects/Door/Open.server.lua"),
        (
            "StarterPlayer.StarterPlayerScripts.keys",
            "scripts/keys.client.lua",
        ),
        ("Workspace.Lamp.Glow", "objects/Lamp/Glow.client.lua"),
    ];
    let expected: Vec<(String, &str)> = expected
        .iter()
        .map(|(name, path)| (name.to_string(), *path))
        .collect();
    assert_eq!(order, expected);
    // The two doors are different Instances sharing one file.
    let doors: Vec<_> = project
        .scripts
        .iter()
        .filter(|script| script.path == "objects/Door/Open.server.lua")
        .collect();
    assert_ne!(doors[0].instance, doors[1].instance);
    assert_eq!(doors[0].source, "-- open");
    let _ = fs::remove_dir_all(&folder);
}

#[test]
fn a_missing_or_mismatched_source_file_is_a_problem_and_the_script_stays() {
    let world = r#"{ "format_version": 2, "workspace": { "children": [
        { "class": "Script", "name": "Gone",
          "properties": { "SourceFile": "objects/Gone.server.lua" } },
        { "class": "Script", "name": "Wrong",
          "properties": { "SourceFile": "objects/Wrong.client.lua" } },
        { "class": "Script", "name": "Outside",
          "properties": { "SourceFile": "../secret.server.lua" } } ] } }"#;
    let folder = project(
        "object-scripts-missing",
        &[(WORLD_FILE, world), ("objects/Wrong.client.lua", "-- x")],
    );
    let project = load(&folder).unwrap();
    assert!(project.scripts.is_empty());
    let problems: Vec<String> = project.problems.iter().map(ToString::to_string).collect();
    assert_eq!(problems.len(), 3, "{problems:?}");
    assert!(problems[0].contains("Gone.server.lua"), "{problems:?}");
    assert!(
        problems[1].contains("Workspace.Wrong") && problems[1].contains(".server.lua"),
        "{problems:?}"
    );
    assert!(
        problems[2].contains("inside the project folder"),
        "{problems:?}"
    );
    let workspace = project.world.workspace();
    for name in ["Gone", "Wrong", "Outside"] {
        assert!(project.world.find_first_child(workspace, name).is_some());
    }
    let _ = fs::remove_dir_all(&folder);
}

#[test]
fn a_project_with_scripts_inside_objects_saves_them_back() {
    let folder = door_project("object-scripts-save");
    let project = load(&folder).unwrap();
    let written = write_world(&project.world);
    assert!(written.starts_with("{\n  \"format_version\": 2,"));
    let again = parse_world(&written).unwrap();
    let copies = again
        .descendants_iter(again.root())
        .filter(|&id| again.get(id).unwrap().source_file().is_some())
        .count();
    assert_eq!(copies, 5);
    let _ = fs::remove_dir_all(&folder);
}

#[test]
fn make_unique_numbers_the_file_before_its_ending() {
    assert_eq!(
        numbered_path("objects/Door/Open.server.lua", 2),
        "objects/Door/Open 2.server.lua"
    );
    assert_eq!(
        numbered_path("objects/Lamp/Glow.client.blocks.json", 3),
        "objects/Lamp/Glow 3.client.blocks.json"
    );
}
