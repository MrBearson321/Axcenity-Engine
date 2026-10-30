//! Find References, Unused and Rename or Move… in the Assets panel
//! (unified Phase 70).

use std::fs;

use axcenity_project::RefPlace;
use axcenity_render::glam::Vec3;

use super::tests::{copy_of_example, play_for, printed};
use super::*;

const WOOD: &str = "assets/textures/wood.png";
const OAK: &str = "assets/textures/oak/oak.png";
const PLANK: &str = "assets/materials/Plank.material.json";
const SIGN_SCRIPT: &str = "scripts/Sign.server.lua";

fn add(app: &mut EditorApp, class: ClassName, parent: InstanceId, name: &str) -> InstanceId {
    let id = app.world.create(class).unwrap();
    app.world.set_name(id, name).unwrap();
    app.world.set_parent(id, Some(parent)).unwrap();
    id
}

fn set(app: &mut EditorApp, id: InstanceId, property: &str, value: Value) {
    app.world.set_property(id, property, value).unwrap();
}

/// A project where `wood.png` is used by a material (on a Part), by a
/// MeshPart's SurfaceAppearance and by a script that puts it on a sign.
fn project(test: &str) -> (EditorApp, PathBuf, InstanceId) {
    let folder = copy_of_example("hello-touched", test);
    let examples = super::tests::example("");
    let copy = |from: &str, to: &str| {
        let to = folder.join(to);
        fs::create_dir_all(to.parent().unwrap()).unwrap();
        fs::copy(examples.join(from), to).unwrap();
    };
    copy("hello-data/assets/textures/sword.png", WOOD);
    copy(
        "hello-csg/assets/models/Doorway.union.glb",
        "assets/models/Crate.glb",
    );
    fs::create_dir_all(folder.join("assets/materials")).unwrap();
    fs::write(
        folder.join(PLANK),
        format!(
            "{{\n  \"maps\": {{\n    \"color\": {{\n      \"path\": \"{WOOD}\"\n    }}\n  }}\n}}\n"
        ),
    )
    .unwrap();
    fs::write(
        folder.join(SIGN_SCRIPT),
        format!(
            "local look = workspace.Sign.Look\nlook.ColorMap = \"{WOOD}\"\nprint(\"sign\", look.ColorMap)\n"
        ),
    )
    .unwrap();
    let mut app = EditorApp::new(Some(&folder));
    let workspace = app.world.workspace();
    let plank = add(&mut app, ClassName::Part, workspace, "Plank");
    set(
        &mut app,
        plank,
        "MaterialVariant",
        Value::String(PLANK.into()),
    );
    set(
        &mut app,
        plank,
        "Position",
        Value::Vector3(Vector3::new(-4.0, 2.0, 0.0)),
    );
    let crate_part = add(&mut app, ClassName::MeshPart, workspace, "Crate");
    set(
        &mut app,
        crate_part,
        "MeshId",
        Value::String("assets/models/Crate.glb".into()),
    );
    set(
        &mut app,
        crate_part,
        "Position",
        Value::Vector3(Vector3::new(0.0, 2.0, 0.0)),
    );
    let look = add(&mut app, ClassName::SurfaceAppearance, crate_part, "Look");
    set(&mut app, look, "ColorMap", Value::String(WOOD.into()));
    // A MeshPart's look is for the model's material of its MaterialName.
    let model = app.assets.model("assets/models/Crate.glb").unwrap();
    let material = model.primitives[0].material.name.clone();
    set(&mut app, look, "MaterialName", Value::String(material));
    let sign = add(&mut app, ClassName::Part, workspace, "Sign");
    set(
        &mut app,
        sign,
        "Position",
        Value::Vector3(Vector3::new(4.0, 2.0, 0.0)),
    );
    add(&mut app, ClassName::SurfaceAppearance, sign, "Look");
    app.save();
    (app, folder, look)
}

/// How many items of `world`'s draw list have a color texture, with no
/// asset failing to load.
fn textured(app: &EditorApp, world: &World) -> usize {
    let camera = axcenity_render::Camera {
        position: Vec3::new(0.0, 8.0, 24.0),
        look_at: Vec3::new(0.0, 2.0, 0.0),
        fov_y_degrees: 70.0,
    };
    let list = axcenity_scene::build_draw_list(world, &app.assets, camera);
    let errors = app.assets.take_errors();
    assert!(errors.is_empty(), "{errors:?}");
    list.items
        .iter()
        .filter(|item| item.material.base_color_texture.is_some())
        .count()
}

#[test]
fn renaming_a_texture_used_by_a_material_a_mesh_part_and_a_script_keeps_all_three_working_in_play()
{
    let (mut app, folder, look) = project("rename-texture");

    // Find References lists all three.
    app.find_asset_references(WOOD);
    let list = &app.references.as_ref().unwrap().list;
    assert!(list.iter().any(|r| r.instance() == Some(look)), "{list:?}");
    assert!(list.iter().any(|r| r.owner == PLANK), "{list:?}");
    assert!(
        list.iter()
            .any(|r| matches!(&r.place, RefPlace::Script { file, line: 2 } if file == SIGN_SCRIPT)),
        "{list:?}"
    );

    // Renamed and moved into a folder of its own, as one step.
    app.rename_asset(WOOD, OAK);
    assert!(!folder.join(WOOD).exists() && folder.join(OAK).is_file());
    assert!(
        fs::read_to_string(folder.join(PLANK))
            .unwrap()
            .contains(OAK)
    );
    assert!(
        fs::read_to_string(folder.join(SIGN_SCRIPT))
            .unwrap()
            .contains(OAK)
    );
    assert_eq!(
        app.world.get(look).unwrap().property("ColorMap"),
        Some(&Value::String(OAK.into()))
    );
    assert_eq!(
        app.history.undo_label(),
        Some("Rename `wood.png` to `oak.png`")
    );
    // The References window follows it.
    assert_eq!(app.references.as_ref().unwrap().asset, OAK);
    assert_eq!(app.references.as_ref().unwrap().list.len(), 3);

    // In Play, all three draw with it: the script set it on the sign.
    app.play();
    assert!(app.playing());
    play_for(&mut app, 0.3);
    let expected = format!("sign\t{OAK}");
    assert!(
        printed(&app)
            .iter()
            .any(|line| *line == expected || line.contains(OAK)),
        "{:?}",
        printed(&app)
    );
    let game = shown(&app.world, &app.play);
    let sign = game.find_first_child(game.workspace(), "Sign").unwrap();
    let sign_look = game.find_first_child(sign, "Look").unwrap();
    assert_eq!(
        game.get(sign_look).unwrap().property("ColorMap"),
        Some(&Value::String(OAK.into()))
    );
    // Each of the three draws textured: taking it out draws fewer.
    let all = textured(&app, game);
    for name in ["Plank", "Crate", "Sign"] {
        let mut without = game.clone();
        let workspace = without.workspace();
        let id = without.find_first_child(workspace, name).unwrap();
        without.set_parent(id, None).unwrap();
        assert!(
            textured(&app, &without) < all,
            "{name} draws with the texture"
        );
    }
    app.stop();

    // One undo puts the file and every reference back.
    app.undo();
    assert!(folder.join(WOOD).is_file() && !folder.join(OAK).exists());
    assert!(!folder.join("assets/textures/oak").exists());
    assert!(
        fs::read_to_string(folder.join(PLANK))
            .unwrap()
            .contains(WOOD)
    );
    assert!(
        fs::read_to_string(folder.join(SIGN_SCRIPT))
            .unwrap()
            .contains(WOOD)
    );
    assert_eq!(
        app.world.get(look).unwrap().property("ColorMap"),
        Some(&Value::String(WOOD.into()))
    );
    let _ = fs::remove_dir_all(&folder);
}

#[test]
fn the_unused_filter_knows_what_nothing_uses() {
    let (mut app, folder, _) = project("unused-assets");
    fs::write(folder.join("assets/textures/spare.png"), b"png").unwrap();
    app.find_unused_assets(&[WOOD.to_owned(), "assets/textures/spare.png".to_owned()]);
    let unused = app.asset_browser.unused_paths();
    assert_eq!(unused, ["assets/textures/spare.png"]);
    let _ = fs::remove_dir_all(&folder);
}

#[test]
fn a_rename_onto_a_taken_name_changes_nothing_and_says_why() {
    let (mut app, folder, look) = project("rename-taken");
    fs::write(folder.join("assets/textures/spare.png"), b"png").unwrap();
    let label = app.history.undo_label().map(str::to_owned);
    app.rename_asset(WOOD, "assets/textures/spare.png");
    assert!(folder.join(WOOD).is_file());
    assert_eq!(app.history.undo_label().map(str::to_owned), label);
    assert_eq!(
        app.world.get(look).unwrap().property("ColorMap"),
        Some(&Value::String(WOOD.into()))
    );
    assert!(app.output.lines().last().unwrap().text.contains("taken"));
    let _ = fs::remove_dir_all(&folder);
}
