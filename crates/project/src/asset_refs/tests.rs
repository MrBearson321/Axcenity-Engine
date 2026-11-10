use std::path::PathBuf;

use super::*;
use crate::world_file::write_prefab;
use axcenity_core::Value;

fn scratch(name: &str) -> PathBuf {
    let folder =
        std::env::temp_dir().join(format!("axcenity-asset-refs-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&folder);
    fs::create_dir_all(&folder).unwrap();
    fs::write(
        folder.join(crate::MANIFEST_FILE),
        format!(
            r#"{{"name": "T", "format_version": {}}}"#,
            crate::FORMAT_VERSION
        ),
    )
    .unwrap();
    fs::write(folder.join(crate::WORLD_FILE), r#"{"format_version": 1}"#).unwrap();
    folder
}

fn write(folder: &Path, path: &str, text: &str) {
    let disk = folder.join(path);
    fs::create_dir_all(disk.parent().unwrap()).unwrap();
    fs::write(disk, text).unwrap();
}

fn add(world: &mut World, class: ClassName, parent: InstanceId, name: &str) -> InstanceId {
    let id = world.create(class).unwrap();
    world.set_name(id, name).unwrap();
    world.set_parent(id, Some(parent)).unwrap();
    id
}

const WOOD: &str = "assets/textures/wood.png";

/// A project naming `wood.png` from a SurfaceAppearance in the open world,
/// a prefab, a material, a Lua script and a block script.
fn project(name: &str) -> (PathBuf, World, InstanceId) {
    let folder = scratch(name);
    write(&folder, WOOD, "png");
    write(&folder, "assets/textures/unused.png", "png");
    write(
        &folder,
        "assets/materials/Plank.material.json",
        &format!(
            "{{\n  \"maps\": {{\n    \"color\": {{\n      \"path\": \"{WOOD}\"\n    }}\n  }}\n}}\n"
        ),
    );
    write(
        &folder,
        "scripts/Sign.server.lua",
        &format!(
            "local look = workspace.Crate.Look\nlook.ColorMap = \"{WOOD}\"\nprint(\"{WOOD}x\")\n"
        ),
    );
    write(
        &folder,
        "scripts/Sign.client.blocks.json",
        &format!(
            r#"{{"blocks": [{{"id": 7, "kind": {{"type": "when_started", "body": [{{"id": 8, "kind": {{"type": "print", "value": {{"type": "text", "value": "{WOOD}"}}}}}}]}}}}]}}"#
        ),
    );
    let mut prefab = World::new();
    let workspace = prefab.workspace();
    let part = add(&mut prefab, ClassName::Part, workspace, "Box");
    let look = add(&mut prefab, ClassName::SurfaceAppearance, part, "Look");
    prefab
        .set_property(look, "ColorMap", Value::String(WOOD.into()))
        .unwrap();
    write(
        &folder,
        "assets/prefabs/Box.prefab.json",
        &write_prefab(&prefab, &[part]),
    );

    let mut world = World::new();
    let workspace = world.workspace();
    let crate_part = add(&mut world, ClassName::MeshPart, workspace, "Crate");
    let look = add(&mut world, ClassName::SurfaceAppearance, crate_part, "Look");
    world
        .set_property(look, "ColorMap", Value::String(WOOD.into()))
        .unwrap();
    (folder, world, look)
}

#[test]
fn every_kind_of_reference_to_a_texture_is_found() {
    let (folder, world, look) = project("find");
    let found = find_references(&folder, &world, None, WOOD);
    let owners: Vec<&str> = found.iter().map(|r| r.owner.as_str()).collect();
    assert!(owners.contains(&"Workspace.Crate.Look"), "{owners:?}");
    assert!(
        owners.contains(&"assets/prefabs/Box.prefab.json (Box.Look)"),
        "{owners:?}"
    );
    assert!(
        owners.contains(&"assets/materials/Plank.material.json"),
        "{owners:?}"
    );
    assert!(owners.contains(&"scripts/Sign.server.lua"), "{owners:?}");
    assert!(
        owners.contains(&"scripts/Sign.client.blocks.json"),
        "{owners:?}"
    );
    // The longer path on line 3 isn't this file.
    let lines: Vec<_> = found
        .iter()
        .filter_map(|r| match r.place {
            RefPlace::Script { line, .. } => Some(line),
            _ => None,
        })
        .collect();
    assert_eq!(lines, [2]);
    let block = found.iter().find_map(|r| match r.place {
        RefPlace::Blocks { block, .. } => Some(block),
        _ => None,
    });
    assert_eq!(block, Some(Some(8)));
    assert_eq!(found.iter().find_map(Reference::instance), Some(look));
}

#[test]
fn unused_lists_only_what_nothing_names() {
    let (folder, world, _) = project("unused");
    let assets = vec![WOOD.to_owned(), "assets/textures/unused.png".to_owned()];
    assert_eq!(
        unused_assets(&folder, &world, None, &assets),
        ["assets/textures/unused.png"]
    );
}

#[test]
fn a_rename_moves_the_file_and_every_reference() {
    let (folder, world, look) = project("rename");
    let to = "assets/textures/oak/oak.png";
    let plan = plan_rename(&folder, &world, None, WOOD, to).unwrap();
    assert_eq!(plan.properties.len(), 1);
    assert_eq!(plan.properties[0].id, look);
    assert_eq!(plan.properties[0].value, to);
    let after = |path: &str| {
        let write = plan.files.iter().find(|w| w.path == path).unwrap();
        write
            .after
            .clone()
            .map(|bytes| String::from_utf8(bytes).unwrap())
    };
    assert_eq!(after(WOOD), None);
    assert_eq!(after(to).as_deref(), Some("png"));
    let material = after("assets/materials/Plank.material.json").unwrap();
    assert!(
        material.contains(to) && !material.contains(WOOD),
        "{material}"
    );
    let lua = after("scripts/Sign.server.lua").unwrap();
    assert!(lua.contains(&format!("\"{to}\"")), "{lua}");
    // Only the path on its own: the longer one stays.
    assert!(lua.contains(&format!("{WOOD}x")), "{lua}");
    let blocks = after("scripts/Sign.client.blocks.json").unwrap();
    assert!(blocks.contains(to) && serde_json::from_str::<BlockScript>(&blocks).is_ok());
    let prefab = after("assets/prefabs/Box.prefab.json").unwrap();
    assert!(prefab.contains(to), "{prefab}");
    // Nothing was written.
    assert!(folder.join(WOOD).is_file() && !folder.join(to).exists());
}

#[test]
fn a_renamed_file_takes_its_sidecar_and_asset_id_along() {
    let (folder, world, _) = project("rename-id");
    axcenity_assets::give_asset_ids(&folder).unwrap();
    let id = axcenity_assets::read_asset_id(&folder, WOOD).unwrap();
    let to = "assets/textures/oak.png";
    let plan = plan_rename(&folder, &world, None, WOOD, to).unwrap();
    let after = |path: &str| {
        plan.files
            .iter()
            .find(|w| w.path == path)
            .map(|w| w.after.clone())
    };
    assert_eq!(after(&axcenity_assets::import_path(WOOD)), Some(None));
    let moved = after(&axcenity_assets::import_path(to)).flatten().unwrap();
    assert_eq!(moved, axcenity_assets::encode_import_file(id));
}

#[test]
fn a_rename_refuses_a_taken_name_another_ending_and_leaving_assets() {
    let (folder, world, _) = project("refuse");
    let plan = |to: &str| plan_rename(&folder, &world, None, WOOD, to).unwrap_err();
    assert!(plan("assets/textures/unused.png").contains("taken"));
    assert!(plan("assets/textures/wood.jpg").contains(".png"));
    assert!(plan("scripts/wood.png").contains("assets folder"));
}
