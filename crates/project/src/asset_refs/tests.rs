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

#[test]
fn a_reference_by_asset_id_names_its_file_and_a_rename_leaves_it_alone() {
    let (folder, mut world, look) = project("by-id");
    axcenity_assets::give_asset_ids(&folder).unwrap();
    let id = axcenity_assets::read_asset_id(&folder, WOOD).unwrap();
    world
        .set_property(look, "ColorMap", Value::String(id.to_string()))
        .unwrap();
    // Found as a reference to the file, and so not unused.
    let found = find_references(&folder, &world, None, WOOD);
    assert!(
        found.iter().any(|r| r.instance() == Some(look)),
        "{found:?}"
    );
    let assets = vec![WOOD.to_owned()];
    assert!(unused_assets(&folder, &world, None, &assets).is_empty());
    // A rename doesn't touch it: the ID goes with the file's sidecar.
    let plan = plan_rename(&folder, &world, None, WOOD, "assets/textures/oak.png").unwrap();
    assert!(
        plan.properties.iter().all(|change| change.id != look),
        "{:?}",
        plan.properties
    );
}

#[test]
fn asset_check_follows_an_id_and_reports_one_no_file_has() {
    let (folder, mut world, look) = project("check-id");
    axcenity_assets::give_asset_ids(&folder).unwrap();
    let id = axcenity_assets::read_asset_id(&folder, WOOD).unwrap();
    world
        .set_property(look, "ColorMap", Value::String(id.to_string()))
        .unwrap();
    let report = crate::check_assets(&folder, &world);
    // The test texture isn't a real PNG, so it may fail to decode, but the
    // ID must name the file: nothing reports it missing.
    let look_findings: Vec<_> = report
        .findings
        .iter()
        .filter(|f| f.owner == "Workspace.Crate.Look")
        .collect();
    assert!(
        look_findings
            .iter()
            .all(|f| f.kind != crate::FindingKind::MissingFile && f.message.contains(WOOD)),
        "{look_findings:?}"
    );
    let unknown = axcenity_assets::AssetId::new().to_string();
    world
        .set_property(look, "ColorMap", Value::String(unknown.clone()))
        .unwrap();
    let report = crate::check_assets(&folder, &world);
    let finding = report
        .findings
        .iter()
        .find(|f| f.owner == "Workspace.Crate.Look")
        .expect("reported");
    assert_eq!(finding.kind, crate::FindingKind::MissingFile);
    assert!(finding.message.contains(&unknown), "{}", finding.message);
}

#[test]
fn identical_files_are_found_and_the_shortest_path_is_kept() {
    let folder = scratch("duplicates");
    let image = "PNG-bytes-of-a-tree";
    write(&folder, "assets/textures/tree.png", image);
    write(&folder, "assets/textures/imported/tree (1).png", image);
    write(&folder, "assets/textures/imported/tree (2).png", image);
    // Same size, other bytes: not a copy.
    write(&folder, "assets/textures/bush.png", "PNG-bytes-of-a-bush");
    // Left out: empty stand-ins, sidecars, JSON assets and terrain data.
    write(&folder, "assets/textures/a.png", "");
    write(&folder, "assets/textures/b.png", "");
    write(&folder, "assets/textures/tree.png.import.json", "{}");
    write(&folder, "assets/textures/bush.png.import.json", "{}");
    write(&folder, "assets/prefabs/A.prefab.json", "{}x");
    write(&folder, "assets/prefabs/B.prefab.json", "{}x");
    write(&folder, "assets/animations/A.anim.json", "{}x");
    write(&folder, "assets/animations/B.anim.json", "{}x");
    write(&folder, "assets/terrain/One/color.png", "same");
    write(&folder, "assets/terrain/Two/color.png", "same");
    let found = crate::find_duplicates(&folder);
    assert_eq!(
        found,
        [crate::Duplicates {
            keep: "assets/textures/tree.png".to_owned(),
            copies: vec![
                "assets/textures/imported/tree (1).png".to_owned(),
                "assets/textures/imported/tree (2).png".to_owned(),
            ],
            bytes: image.len() as u64,
        }]
    );
    // Asset Check lists each extra copy, offering Merge rather than Delete.
    let report = crate::check_assets(&folder, &World::new());
    let copies: Vec<&Finding> = report
        .findings
        .iter()
        .filter(|f| f.kind == FindingKind::DuplicateFile)
        .collect();
    assert_eq!(copies.len(), 2, "{:?}", report.findings);
    assert_eq!(copies[0].owner, "assets/textures/imported/tree (1).png");
    assert_eq!(copies[0].path, "assets/textures/tree.png");
    assert!(
        copies[0].message.contains("19 bytes"),
        "{}",
        copies[0].message
    );
    assert!(!copies[0].can_delete() && !copies[0].can_point_elsewhere());
}

#[test]
fn a_merge_points_every_reference_at_the_kept_file_and_deletes_the_copy() {
    let (folder, mut world, look) = project("merge");
    // `copy.png` is `wood.png` again, named by path from the material, the
    // script and the prefab, and by its asset ID from the open world.
    let copy = "assets/textures/copy.png";
    write(&folder, copy, "png");
    for file in [
        "assets/materials/Plank.material.json",
        "scripts/Sign.server.lua",
        "assets/prefabs/Box.prefab.json",
    ] {
        let text = fs::read_to_string(folder.join(file)).unwrap();
        fs::write(folder.join(file), text.replace(WOOD, copy)).unwrap();
    }
    axcenity_assets::give_asset_ids(&folder).unwrap();
    let copy_id = axcenity_assets::read_asset_id(&folder, copy).unwrap();
    let wood_id = axcenity_assets::read_asset_id(&folder, WOOD).unwrap();
    world
        .set_property(look, "ColorMap", Value::String(copy_id.to_string()))
        .unwrap();
    // A script naming the copy by ID gets the kept file's ID.
    write(
        &folder,
        "scripts/Id.server.lua",
        &format!("workspace.Crate.Look.ColorMap = \"{copy_id}\"\n"),
    );

    let plan = plan_merge(&folder, &world, None, copy, WOOD).unwrap();
    assert_eq!(plan.properties.len(), 1);
    assert_eq!(plan.properties[0].id, look);
    assert_eq!(plan.properties[0].value, WOOD);
    let after = |path: &str| {
        plan.files
            .iter()
            .find(|w| w.path == path)
            .unwrap_or_else(|| panic!("{path} isn't in the plan"))
            .after
            .clone()
            .map(|bytes| String::from_utf8(bytes).unwrap())
    };
    for file in [
        "assets/materials/Plank.material.json",
        "scripts/Sign.server.lua",
        "assets/prefabs/Box.prefab.json",
    ] {
        let text = after(file).unwrap();
        // Only the path on its own: the script's longer `copy.pngx` stays.
        assert!(
            text.contains(WOOD) && !text.contains(&format!("{copy}\"")),
            "{file}: {text}"
        );
    }
    let script = after("scripts/Id.server.lua").unwrap();
    assert!(script.contains(&wood_id.to_string()), "{script}");
    // The copy and its sidecar go; the kept file stays as it is.
    assert_eq!(after(copy), None);
    assert_eq!(after(&axcenity_assets::import_path(copy)), None);
    assert!(plan.files.iter().all(|w| w.path != WOOD));
    // Nothing was written.
    assert!(folder.join(copy).is_file());

    // Files that differ aren't merged.
    write(&folder, copy, "other");
    let refused = plan_merge(&folder, &world, None, copy, WOOD).unwrap_err();
    assert!(refused.contains("aren't the same"), "{refused}");
}
