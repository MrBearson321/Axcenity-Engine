use std::collections::BTreeMap;

use axcenity_core::Value;

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

// References (G2 PR 3): Find References, Unused, Rename or Move, Asset
// Check, packages and Build Game know `SourceFile`.

const OPEN: &str = "objects/Door/Open.server.lua";

#[test]
fn every_copy_and_the_prefab_are_references_to_a_shared_file() {
    let folder = door_project("object-scripts-references");
    let project = load(&folder).unwrap();
    let references = find_references(&folder, &project.world, None, OPEN);
    let owners: Vec<&str> = references.iter().map(|r| r.owner.as_str()).collect();
    assert_eq!(references.len(), 3, "{owners:?}");
    assert!(references.iter().all(|r| r.what == "SourceFile"));
    assert_eq!(
        references
            .iter()
            .filter(|r| r.file() == Some("assets/prefabs/Door.prefab.json"))
            .count(),
        1,
        "{owners:?}"
    );
    assert_eq!(
        references.iter().filter(|r| r.instance().is_some()).count(),
        2
    );
    let _ = fs::remove_dir_all(&folder);
}

#[test]
fn unused_finds_object_files_nothing_runs() {
    let folder = door_project("object-scripts-unused");
    fs::write(folder.join("objects/Old.server.lua"), "-- old").unwrap();
    let project = load(&folder).unwrap();
    let unused = unused_assets(
        &folder,
        &project.world,
        None,
        &[
            "objects/Coin/Spin.server.lua".to_owned(),
            "objects/Old.server.lua".to_owned(),
        ],
    );
    assert_eq!(unused, ["objects/Old.server.lua"]);
    let _ = fs::remove_dir_all(&folder);
}

#[test]
fn renaming_an_object_file_points_its_scripts_at_the_new_name() {
    let folder = door_project("object-scripts-rename");
    let project = load(&folder).unwrap();
    let world = &project.world;
    let plan = plan_rename(
        &folder,
        world,
        None,
        "objects/Coin/Spin.server.lua",
        "objects/Coin/Turn.server.lua",
    )
    .unwrap();
    let spin = world.find_first_descendant(world.root(), "Spin").unwrap();
    assert_eq!(plan.properties.len(), 1);
    assert_eq!(plan.properties[0].id, spin);
    assert_eq!(plan.properties[0].property, "SourceFile");
    assert_eq!(plan.properties[0].value, "objects/Coin/Turn.server.lua");
    let moves: Vec<(&str, bool)> = plan
        .files
        .iter()
        .map(|write| (write.path.as_str(), write.after.is_some()))
        .collect();
    assert_eq!(
        moves,
        [
            ("objects/Coin/Spin.server.lua", false),
            ("objects/Coin/Turn.server.lua", true)
        ]
    );

    // The prefab's file is rewritten for its copies.
    let plan = plan_rename(&folder, world, None, OPEN, "objects/Door/Swing.server.lua").unwrap();
    let prefab = plan
        .files
        .iter()
        .find(|write| write.path == "assets/prefabs/Door.prefab.json")
        .expect("the prefab names it");
    let text = String::from_utf8(prefab.after.clone().unwrap()).unwrap();
    assert!(text.contains("objects/Door/Swing.server.lua"), "{text}");

    // It stays a file of the same kind of script, in objects/.
    let to_assets = plan_rename(&folder, world, None, OPEN, "assets/Open.server.lua");
    assert!(to_assets.unwrap_err().contains("objects"));
    let to_client = plan_rename(&folder, world, None, OPEN, "objects/Door/Open.client.lua");
    assert_eq!(to_client.unwrap_err(), "keep the `.server.lua` ending");
    let _ = fs::remove_dir_all(&folder);
}

/// Code ↔ Blocks for a script inside an object replaces its file with the
/// given content at the other ending and points every reference at it.
#[test]
fn switching_an_object_file_moves_it_to_the_other_ending_with_new_content() {
    let folder = door_project("object-scripts-switch");
    let project = load(&folder).unwrap();
    let world = &project.world;
    let blocks = "objects/Door/Open.server.blocks.json";
    let plan = plan_switch(&folder, world, None, OPEN, b"{}".to_vec()).unwrap();
    let moved = plan
        .files
        .iter()
        .find(|write| write.path == blocks)
        .expect("the block file is made");
    assert_eq!(moved.after.as_deref(), Some(&b"{}"[..]));
    assert!(
        plan.files
            .iter()
            .any(|write| write.path == OPEN && write.after.is_none())
    );
    let prefab = plan
        .files
        .iter()
        .find(|write| write.path == "assets/prefabs/Door.prefab.json")
        .expect("the prefab names it");
    let text = String::from_utf8(prefab.after.clone().unwrap()).unwrap();
    assert!(text.contains(blocks), "{text}");

    // Only Scripts and LocalScripts in objects/, and never over a file.
    let module = plan_switch(
        &folder,
        world,
        None,
        "objects/Lamp/Colors.module.lua",
        vec![],
    );
    assert!(module.unwrap_err().contains("only Scripts"));
    fs::write(folder.join(blocks), "{}").unwrap();
    let taken = plan_switch(&folder, world, None, OPEN, vec![]);
    assert!(taken.unwrap_err().contains("is taken"));
    let _ = fs::remove_dir_all(&folder);
}

#[test]
fn asset_check_finds_missing_misplaced_and_unused_object_files() {
    let folder = door_project("object-scripts-check");
    fs::create_dir_all(folder.join("objects/Old")).unwrap();
    fs::write(folder.join("objects/Old/Gone.server.lua"), "-- old").unwrap();
    let mut project = load(&folder).unwrap();
    fs::remove_file(folder.join("objects/Lamp/Glow.client.lua")).unwrap();
    let world = &mut project.world;
    let colors = world.find_first_descendant(world.root(), "Colors").unwrap();
    world
        .set_property(
            colors,
            "SourceFile",
            Value::String("scripts/shared.module.lua".to_owned()),
        )
        .unwrap();

    let report = check_assets(&folder, world);
    let found: Vec<(FindingKind, &str)> = report
        .findings
        .iter()
        .map(|finding| (finding.kind, finding.path.as_str()))
        .collect();
    assert_eq!(
        found,
        [
            (FindingKind::MissingFile, "objects/Lamp/Glow.client.lua"),
            (FindingKind::InvalidReference, "scripts/shared.module.lua"),
            (FindingKind::UnusedFile, "objects/Lamp/Colors.module.lua"),
            (FindingKind::UnusedFile, "objects/Old/Gone.server.lua"),
        ],
        "{:#?}",
        report.findings
    );
    let misplaced = &report.findings[1];
    assert!(misplaced.message.contains("service script"));
    assert_eq!(misplaced.wants, Some(AssetKind::ModuleScript));
    let _ = fs::remove_dir_all(&folder);
}

#[test]
fn a_package_of_a_prefab_brings_its_scripts_files_and_keeps_both_on_a_clash() {
    let folder = door_project("object-scripts-package");
    let contents = package_contents(
        &folder,
        &["assets/prefabs/Door.prefab.json".to_owned()],
        &[],
    )
    .unwrap();
    let paths: Vec<&str> = contents.files.keys().map(String::as_str).collect();
    assert_eq!(paths, ["assets/prefabs/Door.prefab.json", OPEN]);
    assert!(contents.missing.is_empty());

    let file = folder.join("door.axpkg");
    let roots = ["assets/prefabs/Door.prefab.json".to_owned()];
    write_package(&file, "door", &roots, &contents.files).unwrap();
    let package = read_package(&file).unwrap();
    // Another project with a different door script: Keep both numbers the
    // incoming one and points the prefab at it.
    let other = project("object-scripts-package-into", &[(OPEN, "-- theirs")]);
    let plan = plan_import(&other, &package, Clashes::KeepBoth);
    let writes: BTreeMap<&str, String> = plan
        .writes
        .iter()
        .map(|(path, bytes)| (path.as_str(), String::from_utf8_lossy(bytes).into_owned()))
        .collect();
    assert_eq!(writes["objects/Door/Open2.server.lua"], "-- open");
    assert!(writes["assets/prefabs/Door.prefab.json"].contains("objects/Door/Open2.server.lua"));
    let _ = fs::remove_dir_all(&folder);
    let _ = fs::remove_dir_all(&other);
}

#[test]
fn a_built_game_runs_block_scripts_inside_objects_as_their_lua() {
    use axcenity_blocks_runtime::{Block, BlockKind, BlockScript, Expression};
    let folder = door_project("object-scripts-build");
    let blocks = BlockScript {
        blocks: vec![Block::new(BlockKind::WhenStarted {
            body: vec![Block::new(BlockKind::Wait {
                seconds: Expression::Number { value: 1.0 },
            })],
        })],
        ..Default::default()
    };
    // The coin's script is blocks now.
    let world = fs::read_to_string(folder.join(WORLD_FILE))
        .unwrap()
        .replace("Spin.server.lua", "Spin.server.blocks.json");
    fs::write(folder.join(WORLD_FILE), world).unwrap();
    fs::remove_file(folder.join("objects/Coin/Spin.server.lua")).unwrap();
    fs::write(
        folder.join("objects/Coin/Spin.server.blocks.json"),
        serde_json::to_vec(&blocks).unwrap(),
    )
    .unwrap();

    let output = temp_folder("object-scripts-build-out");
    let _ = fs::remove_dir_all(&output);
    unpack_game(&pack_game(&folder).unwrap(), &output).unwrap();
    assert_eq!(fs::read_to_string(output.join(OPEN)).unwrap(), "-- open");
    assert!(!output.join("objects/Coin/Spin.server.blocks.json").exists());

    let built = load(&output).unwrap();
    assert!(built.problems.is_empty(), "{:?}", built.problems);
    let spin = built
        .scripts
        .iter()
        .find(|script| script.path == "objects/Coin/Spin.server.blocks.json")
        .expect("the coin's script loads");
    assert!(spin.source.contains("task.wait(1)"), "{}", spin.source);
    let _ = fs::remove_dir_all(&folder);
    let _ = fs::remove_dir_all(&output);
}

#[test]
fn level_code_has_each_file_once_and_leaves_out_what_cant_be_read() {
    let folder = door_project("object-scripts-level-code");
    let project = load(&folder).unwrap();
    fs::remove_file(folder.join("objects/Lamp/Glow.client.lua")).unwrap();
    let code = level_code(&folder, [&project.world, &project.world]);
    let paths: Vec<&str> = code.keys().map(String::as_str).collect();
    assert_eq!(
        paths,
        [
            "objects/Coin/Spin.server.lua",
            "objects/Door/Open.server.lua",
            "objects/Lamp/Colors.module.lua",
        ]
    );
    assert_eq!(code[OPEN], "-- open");
    let _ = fs::remove_dir_all(&folder);
}
