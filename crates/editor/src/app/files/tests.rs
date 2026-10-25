use std::fs;
use std::time::{Duration, Instant};

use super::super::tests::{copy_of_example, last_line, part, play_for, printed};
use super::*;
use crate::app::{Answer, Discard};
use crate::explorer::Action;
use crate::output::OutputKind;
use crate::properties::Edit;
use crate::scripts::{NEW_SCRIPT_SOURCE, ScriptView};
use crate::surface::Slot;
use axcenity_assets::Channel;
use axcenity_blocks::{Block, BlockKind};

fn script_names(app: &EditorApp) -> Vec<String> {
    let world = &app.world;
    world
        .get(world.server_script_service())
        .unwrap()
        .children()
        .iter()
        .map(|&id| world.get(id).unwrap().name().to_string())
        .collect()
}

fn script(app: &EditorApp, name: &str) -> InstanceId {
    app.world
        .find_first_child(app.world.server_script_service(), name)
        .unwrap()
}

/// Switches the script at `path` between Code and Blocks, as the bar over
/// its tab does.
fn switch(app: &mut EditorApp, path: &str) {
    let view = if project::is_block_script(path) {
        ScriptView::Code
    } else {
        ScriptView::Blocks
    };
    app.switch_script_view(path, view);
}

fn example_model() -> std::path::PathBuf {
    super::super::tests::example("hello-mesh").join("assets/models/pyramid.gltf")
}

#[test]
fn scripts_switch_to_blocks_and_can_be_edited_saved_and_reloaded() {
    let folder = copy_of_example("hello-part", "block-script-editor");
    let mut app = EditorApp::new(Some(&folder));
    app.insert_script(ClassName::Script);
    let code_path = app.tabs.active_path().unwrap().to_owned();
    let id = script(&app, "Script");
    switch(&mut app, &code_path);
    assert!(
        !folder.join(&code_path).exists(),
        "the .lua file is replaced"
    );
    assert_eq!(script(&app, "Script"), id, "it's the same Script");
    let path = app
        .project
        .as_ref()
        .unwrap()
        .block_scripts
        .last()
        .unwrap()
        .path
        .clone();
    let document = app
        .project
        .as_mut()
        .unwrap()
        .block_scripts
        .iter_mut()
        .find(|document| document.path == path)
        .unwrap();
    let BlockKind::WhenStarted { body } = &mut document.script.blocks[0].kind else {
        panic!("new block scripts start with a WhenStarted event");
    };
    let play_sound = Block::new(BlockKind::PlaySound {
        target: "Chime".to_owned(),
    });
    let play_sound_id = play_sound.id;
    body.push(play_sound);
    app.update_block_lua(&path);

    assert!(app.unsaved());
    assert!(app.save());
    assert!(!app.unsaved());
    let saved = project::read_block_script(&folder, &path).unwrap();
    assert_eq!(saved, app.project.as_ref().unwrap().block_scripts[0].script);
    let source = &app
        .project
        .as_ref()
        .unwrap()
        .scripts
        .iter()
        .find(|script| script.path == path)
        .unwrap()
        .source;
    assert!(source.contains("workspace.Chime:Play()"));
    let play_line = source
        .lines()
        .position(|line| line.contains(":Play()"))
        .unwrap()
        + 1;
    app.open_script(&path, Some(play_line));
    assert_eq!(
        app.project
            .as_ref()
            .unwrap()
            .block_scripts
            .iter()
            .find(|document| document.path == path)
            .unwrap()
            .highlighted,
        Some(play_sound_id)
    );

    let reopened = EditorApp::new(Some(&folder));
    assert_eq!(reopened.project.unwrap().block_scripts.len(), 1);

    // Back to Code: the Lua the blocks became, in a .lua file again.
    assert_eq!(app.tabs.active_path(), Some(path.as_str()));
    switch(&mut app, &path);
    assert_eq!(app.tabs.active_path(), Some(code_path.as_str()));
    let code = fs::read_to_string(folder.join(&code_path)).unwrap();
    assert!(code.contains("workspace.Chime:Play()"), "{code}");
    assert!(!folder.join(&path).exists());
    assert!(app.project.as_ref().unwrap().block_scripts.is_empty());
    assert!(!app.unsaved());

    // Typed code with no blocks for it still switches, as grey blocks.
    app.tabs.get_mut(&code_path).unwrap().text = format!(
        "{code}local function twice(x)
return x * 2
end
"
    );
    switch(&mut app, &code_path);
    let document = &app.project.as_ref().unwrap().block_scripts[0];
    assert!(
        document
            .script
            .blocks
            .iter()
            .any(|block| matches!(block.kind, BlockKind::TopCode { .. })),
        "{:#?}",
        document.script
    );
    assert_eq!(script(&app, "Script"), id);

    // Code with a mistake in it stays code.
    switch(&mut app, &path);
    app.tabs.get_mut(&code_path).unwrap().text = "if then".to_owned();
    switch(&mut app, &code_path);
    assert!(folder.join(&code_path).exists());
    assert_eq!(app.tabs.active_path(), Some(code_path.as_str()));
    fs::remove_dir_all(folder).unwrap();
}

#[test]
fn importing_a_model_copies_it_and_inserts_a_mesh_part_in_one_undo_step() {
    let folder = copy_of_example("hello-part", "import-model");
    let mut app = EditorApp::new(Some(&folder));
    app.import_model(&example_model());
    // It has a texture, so its maps are shown first.
    let dialog = app.importing.take().expect("the dialog is open");
    assert!(!dialog.pick_objects);
    app.import_chosen(&dialog);

    let id = app.selected.expect("the new MeshPart is selected");
    let mesh_part = app.world.get(id).unwrap();
    assert_eq!(mesh_part.class(), ClassName::MeshPart);
    assert_eq!(mesh_part.name(), "pyramid");
    let mesh_id = "assets/models/pyramid/pyramid.gltf";
    assert_eq!(
        mesh_part.property("MeshId"),
        Some(&Value::String(mesh_id.to_owned()))
    );
    assert_eq!(
        mesh_part.property("Size"),
        Some(&Value::Vector3(Vector3::new(4.0, 3.0, 4.0)))
    );
    assert!(folder.join(mesh_id).is_file());
    assert!(last_line(&app).text.contains(mesh_id));

    // It's drawn as its model, not as a stand-in box.
    let camera = axcenity_render::Camera {
        position: axcenity_render::glam::Vec3::new(0.0, 20.0, 30.0),
        look_at: axcenity_render::glam::Vec3::ZERO,
        fov_y_degrees: 70.0,
    };
    let list = axcenity_scene::build_draw_list(&app.world, &app.assets, camera);
    assert!(
        list.items
            .iter()
            .any(|item| matches!(item.shape, axcenity_render::Shape::Mesh(_)))
    );

    // Saved and opened again, it's still there.
    assert!(app.save());
    let reopened = EditorApp::new(Some(&folder));
    let again = part(&reopened, "pyramid");
    assert_eq!(
        reopened.world.get(again).unwrap().property("MeshId"),
        Some(&Value::String(mesh_id.to_owned()))
    );

    // One undo takes it out again (the copied file stays).
    app.undo();
    let workspace = app.world.workspace();
    assert!(app.world.find_first_child(workspace, "pyramid").is_none());
    assert!(folder.join(mesh_id).is_file());
    let _ = fs::remove_dir_all(&folder);
}

/// A glTF file in `folder` holding three copies of the example pyramid
/// (4 wide): `Pyramid` at the origin, `Street/Big` 10 along X inside
/// an empty `Street` node, and `Small` 10 along -Z.
fn write_props(folder: &Path) -> std::path::PathBuf {
    let text = fs::read_to_string(example_model()).unwrap();
    let mut gltf: serde_json::Value = serde_json::from_str(&text).unwrap();
    gltf["scenes"][0]["nodes"] = serde_json::json!([0, 1, 3]);
    gltf["nodes"] = serde_json::json!([
        { "name": "Pyramid", "mesh": 0 },
        { "name": "Street", "children": [2] },
        { "name": "Big", "mesh": 0, "translation": [10, 0, 0] },
        { "name": "Small", "mesh": 0, "translation": [0, 0, -10] },
    ]);
    let file = folder.join("props.gltf");
    fs::write(&file, gltf.to_string()).unwrap();
    file
}

#[test]
fn an_animated_fbx_comes_in_with_an_animator_a_state_machine_and_an_animation_per_clip() {
    let folder = copy_of_example("hello-part", "import-animated");
    let mut app = EditorApp::new(Some(&folder));
    let file = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/hello-animation/assets/models/noodle.fbx");
    app.import_model(&file);
    let dialog = app.importing.take().expect("an animated file asks first");
    let clips = dialog
        .animation
        .as_ref()
        .expect("it's animated")
        .clips
        .len();
    assert!(clips > 0);
    app.import_chosen(&dialog);

    let workspace = app.world.workspace();
    let part = app
        .world
        .find_first_child(workspace, "noodle")
        .expect("imported");
    let animator = crate::animation::animator_of(&app.world, part).expect("an Animator");
    let subject = crate::animation::Subject::of(&app.world, part).unwrap();
    assert!(
        subject
            .state_machine
            .starts_with("assets/animations/noodle/"),
        "{}",
        subject.state_machine
    );
    assert_eq!(subject.animations.len(), clips);
    for animation in &subject.animations {
        let instance = app.world.get(animation.id).unwrap();
        assert_eq!(instance.parent(), Some(animator));
        assert!(animation.clip.starts_with("assets/animations/noodle/"));
        assert_eq!(
            axcenity_anim::files::clip_name(&animation.clip),
            animation.name
        );
        assert!(folder.join(&animation.clip).is_file(), "{}", animation.clip);
    }
    let _ = fs::remove_dir_all(&folder);
}

/// Opens the Import Model dialog for `file`, unticks `Small`, and
/// imports the rest as `placement` says.
fn import_without_small(app: &mut EditorApp, file: &Path, placement: Placement) {
    app.import_model(file);
    let mut dialog = app.importing.take().expect("several objects ask first");
    let small = dialog
        .objects
        .iter()
        .position(|object| object.name == "Small")
        .unwrap();
    dialog.set(small, false);
    dialog.placement = placement;
    app.import_chosen(&dialog);
}

fn position(app: &EditorApp, id: InstanceId) -> Vector3 {
    match app.world.get(id).unwrap().property("Position") {
        Some(Value::Vector3(position)) => *position,
        other => panic!("{other:?}"),
    }
}

fn mesh_id(app: &EditorApp, id: InstanceId) -> String {
    match app.world.get(id).unwrap().property("MeshId") {
        Some(Value::String(mesh_id)) => mesh_id.clone(),
        other => panic!("{other:?}"),
    }
}

#[test]
fn each_material_gets_a_surface_appearance_with_its_maps_as_changed_in_the_dialog() {
    let folder = copy_of_example("hello-part", "import-surface");
    let mut app = EditorApp::new(Some(&folder));
    app.import_model(&example_model());
    let mut dialog = app.importing.take().expect("the dialog is open");
    let maps = &mut dialog.materials[0];
    assert_eq!(maps.material, "Sandstone");
    // A roughness map picked from outside the project, read from green.
    let picked = folder.join("..").join("import-surface-rough.png");
    fs::write(&picked, b"not checked until drawn").unwrap();
    maps.set_file(Slot::Roughness, Some(MapFile::Picked(picked.clone())));
    *maps.channel_mut(Slot::Roughness).unwrap() = Channel::Green;
    app.import_chosen(&dialog);

    let part = app.selected.unwrap();
    let surface = app.world.find_first_child(part, "Sandstone").unwrap();
    let surface = app.world.get(surface).unwrap();
    assert_eq!(surface.class(), ClassName::SurfaceAppearance);
    let path = |name| {
        surface
            .property(name)
            .and_then(Value::as_str)
            .unwrap()
            .to_owned()
    };
    assert_eq!(path("MaterialName"), "Sandstone");
    // The model's own texture, copied out of the file into its folder.
    let color = path("ColorMap");
    assert!(
        color.starts_with("assets/models/pyramid/textures/"),
        "{color}"
    );
    assert!(folder.join(&color).is_file());
    // The picked one, copied in beside it.
    assert_eq!(
        path("RoughnessMap"),
        "assets/models/pyramid/textures/import-surface-rough.png"
    );
    assert!(folder.join(path("RoughnessMap")).is_file());
    assert_eq!(
        surface.property("RoughnessChannel"),
        Some(&Value::TextureChannel(axcenity_core::TextureChannel::Green))
    );
    assert_eq!(path("NormalMap"), "");

    // Saved and opened again, it's still there; undo takes it out with the part.
    assert!(app.save());
    let reopened = EditorApp::new(Some(&folder));
    let again = part_named(&reopened, "pyramid");
    assert!(
        reopened
            .world
            .find_first_child(again, "Sandstone")
            .is_some()
    );
    app.undo();
    assert!(
        app.world
            .get(app.world.workspace())
            .unwrap()
            .children()
            .iter()
            .all(|&id| { app.world.get(id).unwrap().name() != "pyramid" })
    );
    let _ = fs::remove_file(&picked);
    let _ = fs::remove_dir_all(&folder);
}

fn part_named(app: &EditorApp, name: &str) -> InstanceId {
    app.world
        .find_first_child(app.world.workspace(), name)
        .unwrap()
}

#[test]
fn a_file_with_several_objects_asks_first_and_copies_nothing_yet() {
    let folder = copy_of_example("hello-part", "import-ask");
    let file = write_props(&folder);
    let mut app = EditorApp::new(Some(&folder));
    app.import_model(&file);
    let dialog = app.importing.as_ref().expect("the dialog is open");
    let names: Vec<&str> = dialog.objects.iter().map(|o| o.name.as_str()).collect();
    assert_eq!(names, ["Pyramid", "Street", "Big", "Small"]);
    assert!(
        !folder
            .join(axcenity_assets::MODELS_FOLDER)
            .join("props")
            .exists()
    );
    let _ = fs::remove_dir_all(&folder);
}

#[test]
fn chosen_objects_import_as_a_model_laid_out_as_in_the_file() {
    let folder = copy_of_example("hello-part", "import-grouped");
    let file = write_props(&folder);
    let mut app = EditorApp::new(Some(&folder));
    import_without_small(&mut app, &file, Placement::Grouped);

    let workspace = app.world.workspace();
    let model = app.world.find_first_child(workspace, "props").unwrap();
    assert_eq!(app.selected, Some(model));
    assert_eq!(app.world.get(model).unwrap().class(), ClassName::Model);
    let pyramid = app.world.find_first_child(model, "Pyramid").unwrap();
    let street = app.world.find_first_child(model, "Street").unwrap();
    assert_eq!(app.world.get(street).unwrap().class(), ClassName::Model);
    let big = app.world.find_first_child(street, "Big").unwrap();
    assert!(app.world.find_first_child(model, "Small").is_none());
    assert_eq!(
        mesh_id(&app, big),
        "assets/models/props/props.gltf#Street/Big"
    );
    assert_eq!(
        mesh_id(&app, pyramid),
        "assets/models/props/props.gltf#Pyramid"
    );
    let (a, b) = (position(&app, pyramid), position(&app, big));
    assert!((b.x - a.x - 10.0).abs() < 1e-4 && (b.z - a.z).abs() < 1e-4);
    assert_eq!(
        app.world.get(big).unwrap().property("Size"),
        Some(&Value::Vector3(Vector3::new(4.0, 3.0, 4.0)))
    );

    // Saved and opened again, each part loads its own object.
    assert!(app.save());
    let reopened = EditorApp::new(Some(&folder));
    let model = reopened.world.find_first_child(workspace, "props").unwrap();
    let street = reopened.world.find_first_child(model, "Street").unwrap();
    let big = reopened.world.find_first_child(street, "Big").unwrap();
    let loaded = reopened.assets.model(&mesh_id(&reopened, big)).unwrap();
    assert!((loaded.center[0] - 10.0).abs() < 1e-4);
    assert!(reopened.assets.take_errors().is_empty());

    // One undo takes it all out.
    app.undo();
    assert!(app.world.find_first_child(workspace, "props").is_none());
    let _ = fs::remove_dir_all(&folder);
}

#[test]
fn chosen_objects_can_import_as_separate_parts_or_one_merged_part() {
    let folder = copy_of_example("hello-part", "import-separate");
    let file = write_props(&folder);
    let mut app = EditorApp::new(Some(&folder));
    let workspace = app.world.workspace();
    let before = app.world.get(workspace).unwrap().children().len();

    import_without_small(&mut app, &file, Placement::Separate);
    let pyramid = app.world.find_first_child(workspace, "Pyramid").unwrap();
    let big = app.world.find_first_child(workspace, "Big").unwrap();
    assert_eq!(position(&app, pyramid), position(&app, big));
    assert_eq!(
        app.world.get(workspace).unwrap().children().len(),
        before + 2
    );
    app.undo();
    assert_eq!(app.world.get(workspace).unwrap().children().len(), before);

    import_without_small(&mut app, &file, Placement::Merged);
    let merged = app.world.find_first_child(workspace, "props").unwrap();
    // The second import's copy: the first is still in the project.
    assert_eq!(
        mesh_id(&app, merged),
        "assets/models/props2/props2.gltf#Pyramid,Street/Big"
    );
    // From the Pyramid's left edge (-2) to the Big one's right (12).
    assert_eq!(
        app.world.get(merged).unwrap().property("Size"),
        Some(&Value::Vector3(Vector3::new(14.0, 3.0, 4.0)))
    );
    let _ = fs::remove_dir_all(&folder);
}

#[test]
fn a_broken_model_is_reported_and_nothing_is_inserted_or_copied() {
    let folder = copy_of_example("hello-part", "import-broken");
    let broken = folder.join("broken.glb");
    fs::write(&broken, "glTF, but not really").unwrap();
    let mut app = EditorApp::new(Some(&folder));
    let before = app.world.descendants(app.world.workspace()).len();

    app.import_model(&broken);
    assert_eq!(app.world.descendants(app.world.workspace()).len(), before);
    assert!(
        !folder
            .join(axcenity_assets::MODELS_FOLDER)
            .join("broken.glb")
            .exists()
    );
    let line = last_line(&app);
    assert_eq!(line.kind, OutputKind::Error);
    assert!(
        line.text.starts_with("Could not import")
            && line.text.contains("it isn't a valid glTF file"),
        "{}",
        line.text
    );
    let _ = fs::remove_dir_all(&folder);
}

#[test]
fn a_mesh_part_whose_file_is_missing_is_reported_in_output() {
    let folder = copy_of_example("hello-mesh", "missing-model");
    fs::remove_file(folder.join("assets/models/CityPropsCollection2.fbx")).unwrap();
    let app = EditorApp::new(Some(&folder));
    // Drawing asks for the model, which reports it once.
    let camera = axcenity_render::Camera {
        position: axcenity_render::glam::Vec3::new(0.0, 20.0, 30.0),
        look_at: axcenity_render::glam::Vec3::ZERO,
        fov_y_degrees: 70.0,
    };
    axcenity_scene::build_draw_list(&app.world, &app.assets, camera);
    let errors = app.assets.take_errors();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].contains("CityPropsCollection2.fbx"), "{errors:?}");
    let _ = fs::remove_dir_all(&folder);
}

#[test]
fn insert_script_makes_the_file_and_the_script_together_and_opens_it() {
    let folder = copy_of_example("hello-touched", "insert-script");
    let mut app = EditorApp::new(Some(&folder));
    app.act(Action::InsertScript(ClassName::Script));
    app.act(Action::InsertScript(ClassName::Script));

    assert_eq!(script_names(&app), ["Script", "Script2", "pad"]);
    let path = "scripts/Script2.server.lua";
    assert_eq!(
        fs::read_to_string(folder.join(path)).unwrap(),
        NEW_SCRIPT_SOURCE
    );
    assert_eq!(app.selected, Some(script(&app, "Script2")));
    assert_eq!(app.tabs.active().unwrap().path, path);
    // The file is already there, so there's nothing to save.
    assert!(!app.unsaved());
    let _ = fs::remove_dir_all(&folder);
}

#[test]
fn editing_and_saving_a_script_writes_its_file() {
    let folder = copy_of_example("hello-touched", "edit-script");
    let mut app = EditorApp::new(Some(&folder));
    app.act(Action::InsertScript(ClassName::Script));
    let path = "scripts/Script.server.lua";
    app.tabs.get_mut(path).unwrap().text = "print('edited here')\n".to_string();
    assert!(app.unsaved());
    assert_eq!(app.title(), "Hello Touched* - Axcenity Editor");

    assert!(app.save());
    assert!(!app.unsaved());
    assert_eq!(
        fs::read_to_string(folder.join(path)).unwrap(),
        "print('edited here')\n"
    );
    assert!(
        last_line(&app).text.contains(path),
        "{}",
        last_line(&app).text
    );
    // The editor's own save is not an outside change.
    app.disk_changed(&[path.to_string()]);
    assert_eq!(app.tabs.get(path).unwrap().disk, Disk::Same);

    // Play runs the saved code.
    app.play();
    play_for(&mut app, 0.1);
    assert!(printed(&app).contains(&"edited here"));
    let _ = fs::remove_dir_all(&folder);
}

#[test]
fn play_saves_script_tabs_first() {
    let folder = copy_of_example("hello-touched", "play-saves-scripts");
    let mut app = EditorApp::new(Some(&folder));
    app.open_script_instance(script(&app, "pad"));
    app.tabs.get_mut("scripts/pad.server.lua").unwrap().text = "print('unsaved edit')".to_string();
    app.play();
    play_for(&mut app, 0.1);
    assert!(printed(&app).contains(&"unsaved edit"));
    assert_eq!(
        fs::read_to_string(folder.join("scripts/pad.server.lua")).unwrap(),
        "print('unsaved edit')"
    );
    let _ = fs::remove_dir_all(&folder);
}

#[test]
fn closing_a_tab_with_unsaved_changes_asks_first() {
    let folder = copy_of_example("hello-touched", "close-tab");
    let mut app = EditorApp::new(Some(&folder));
    let path = "scripts/pad.server.lua".to_string();
    app.open_script(&path, None);
    let close = Discard::CloseTab(path.clone());
    assert_eq!(app.request(close.clone()), Some(close.clone()));

    app.tabs.get_mut(&path).unwrap().text.push_str("\n-- more");
    assert_eq!(app.request(close.clone()), None);
    // Saving in the prompt saves only that script, then closes it.
    app.apply(part(&app, "Floor"), Edit::Name("Ground2".to_string()));
    assert_eq!(app.answer(Answer::Save), Some(close));
    assert!(
        fs::read_to_string(folder.join(&path))
            .unwrap()
            .ends_with("-- more")
    );
    assert!(!app.history.is_saved(), "the world wasn't saved");
    let _ = fs::remove_dir_all(&folder);
}

#[test]
fn renaming_a_script_renames_its_file_and_bad_names_are_refused() {
    let folder = copy_of_example("hello-touched", "rename-script");
    let mut app = EditorApp::new(Some(&folder));
    let pad = script(&app, "pad");
    app.open_script_instance(pad);
    app.act(Action::Rename(pad, "launcher".to_string()));

    assert_eq!(app.world.get(pad).unwrap().name(), "launcher");
    assert!(folder.join("scripts/launcher.server.lua").exists());
    assert!(!folder.join("scripts/pad.server.lua").exists());
    assert_eq!(
        app.tabs.active().unwrap().path,
        "scripts/launcher.server.lua"
    );
    assert!(!app.unsaved());

    app.act(Action::Rename(pad, "a/b".to_string()));
    assert_eq!(app.world.get(pad).unwrap().name(), "launcher");
    assert_eq!(last_line(&app).kind, OutputKind::Error);
    // The Properties panel's Name box doesn't rename Scripts.
    app.apply(pad, Edit::Name("other".to_string()));
    assert_eq!(app.world.get(pad).unwrap().name(), "launcher");
    let _ = fs::remove_dir_all(&folder);
}

#[test]
fn deleting_a_script_asks_then_deletes_its_file_and_closes_its_tab() {
    let folder = copy_of_example("hello-touched", "delete-script");
    let mut app = EditorApp::new(Some(&folder));
    let pad = script(&app, "pad");
    app.open_script_instance(pad);
    app.act(Action::Delete(pad));
    // Nothing happens until "Delete script?" is answered.
    assert_eq!(app.confirm_delete, Some(pad));
    assert!(folder.join("scripts/pad.server.lua").exists());

    app.confirm_delete = None;
    app.delete_script(pad);
    assert!(!folder.join("scripts/pad.server.lua").exists());
    assert!(script_names(&app).is_empty());
    assert!(app.tabs.world_shown());
    assert!(app.project.as_ref().unwrap().scripts.is_empty());
    let _ = fs::remove_dir_all(&folder);
}

#[test]
fn outside_edits_update_a_tab_or_keep_unsaved_changes() {
    let folder = copy_of_example("hello-touched", "outside-edit");
    let mut app = EditorApp::new(Some(&folder));
    let path = "scripts/pad.server.lua";
    app.open_script(path, None);

    fs::write(folder.join(path), "print('from VS Code')").unwrap();
    app.disk_changed(&[path.to_string()]);
    let tab = app.tabs.get(path).unwrap();
    assert_eq!(tab.text, "print('from VS Code')");
    assert!(!tab.unsaved());

    // With unsaved changes in the tab, they stay.
    app.tabs.get_mut(path).unwrap().text = "print('mine')".to_string();
    fs::write(folder.join(path), "print('theirs')").unwrap();
    app.disk_changed(&[path.to_string()]);
    let tab = app.tabs.get(path).unwrap();
    assert_eq!(
        (tab.text.as_str(), tab.disk),
        ("print('mine')", Disk::Changed)
    );
    assert_eq!(tab.saved, "print('theirs')");
    let _ = fs::remove_dir_all(&folder);
}

#[test]
fn new_and_deleted_script_files_add_and_remove_scripts() {
    let folder = copy_of_example("hello-touched", "outside-files");
    let mut app = EditorApp::new(Some(&folder));
    fs::write(folder.join("scripts/door.server.lua"), "print('door')").unwrap();
    app.disk_changed(&["scripts/door.server.lua".to_string()]);
    assert_eq!(script_names(&app), ["door", "pad"]);

    app.open_script("scripts/door.server.lua", None);
    fs::remove_file(folder.join("scripts/door.server.lua")).unwrap();
    app.disk_changed(&["scripts/door.server.lua".to_string()]);
    assert_eq!(script_names(&app), ["pad"]);
    assert!(app.tabs.world_shown(), "its unchanged tab closed");
    let _ = fs::remove_dir_all(&folder);
}

#[test]
fn an_outside_change_to_the_world_file_reloads_it_unless_there_are_unsaved_changes() {
    let folder = copy_of_example("hello-touched", "outside-world");
    let mut app = EditorApp::new(Some(&folder));
    let mut other = axcenity_project::load(&folder).unwrap().world;
    let floor = other.find_first_child(other.workspace(), "Floor").unwrap();
    other.set_name(floor, "Ground").unwrap();
    axcenity_project::save_world(&folder, &other).unwrap();

    app.disk_changed(&[axcenity_project::WORLD_FILE.to_string()]);
    assert!(
        app.world
            .find_first_child(app.world.workspace(), "Ground")
            .is_some()
    );
    // Scripts are still there, from their files.
    assert_eq!(script_names(&app), ["pad"]);

    // With unsaved changes, the world here stays, and saving keeps the other file.
    app.apply(part(&app, "Pad"), Edit::Name("Pad2".to_string()));
    other.set_name(floor, "Lava").unwrap();
    axcenity_project::save_world(&folder, &other).unwrap();
    app.disk_changed(&[axcenity_project::WORLD_FILE.to_string()]);
    assert!(
        app.world
            .find_first_child(app.world.workspace(), "Lava")
            .is_none()
    );
    assert!(app.save());
    let kept = fs::read_to_string(folder.join(axcenity_project::KEPT_WORLD_FILE)).unwrap();
    assert!(kept.contains("Lava"));
    let _ = fs::remove_dir_all(&folder);
}

#[test]
fn a_corrupted_script_is_reported_and_the_rest_of_the_project_still_loads() {
    let folder = copy_of_example("hello-touched", "broken-script");
    fs::write(folder.join("scripts/broken.server.lua"), b"print('\xFF')").unwrap();
    let mut app = EditorApp::new(Some(&folder));

    let errors: Vec<&str> = app
        .output
        .lines()
        .iter()
        .filter(|line| line.kind == OutputKind::Error)
        .map(|line| line.text.as_str())
        .collect();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains("broken.server.lua"), "{}", errors[0]);
    assert_eq!(script_names(&app), ["pad"]);
    assert!(
        app.world
            .find_first_child(app.world.workspace(), "Pad")
            .is_some()
    );

    // Fixed in another program, it loads.
    fs::write(folder.join("scripts/broken.server.lua"), "print('fixed')").unwrap();
    app.disk_changed(&["scripts/broken.server.lua".to_string()]);
    assert_eq!(script_names(&app), ["broken", "pad"]);
    let _ = fs::remove_dir_all(&folder);
}

#[test]
fn saving_over_a_damaged_world_file_keeps_it() {
    let folder = copy_of_example("hello-touched", "damaged-world");
    fs::write(folder.join("world.json"), "{ damaged").unwrap();
    let mut app = EditorApp::new(Some(&folder));
    assert_eq!(app.title(), "Hello Touched - Axcenity Editor");
    assert_eq!(script_names(&app), ["pad"]);
    // Nothing changed: Save doesn't touch the damaged file.
    assert!(app.save());
    assert_eq!(
        fs::read_to_string(folder.join("world.json")).unwrap(),
        "{ damaged"
    );

    app.insert(crate::tree::Insertable::Model, None);
    assert!(app.save());
    let kept = fs::read_to_string(folder.join(axcenity_project::KEPT_WORLD_FILE));
    assert_eq!(kept.unwrap(), "{ damaged");
    assert!(axcenity_project::load(&folder).unwrap().world_loaded);
    let _ = fs::remove_dir_all(&folder);
}

#[test]
fn clicking_a_script_error_opens_the_script_at_that_line() {
    let folder = copy_of_example("hello-touched", "error-link");
    fs::write(
        folder.join("scripts/pad.server.lua"),
        "print('one')\nprint('two')\nerror('three')\n",
    )
    .unwrap();
    let mut app = EditorApp::new(Some(&folder));
    app.play();
    play_for(&mut app, 0.1);
    app.stop();
    let error = app
        .output
        .lines()
        .iter()
        .find(|line| line.kind == OutputKind::Error)
        .unwrap();
    assert_eq!(error.text, "scripts/pad.server.lua:3: three");
    let location = error.location.clone().unwrap();

    // What the Output panel's click does.
    app.open_script(&location.path, Some(location.line));
    let tab = app.tabs.active().unwrap();
    assert_eq!(tab.path, "scripts/pad.server.lua");
    assert_eq!(tab.go_to_line(), Some(3));
    let _ = fs::remove_dir_all(&folder);
}

#[test]
fn a_script_edited_in_another_program_updates_its_tab_within_a_second() {
    let folder = copy_of_example("hello-touched", "watch-live");
    let mut app = EditorApp::new(Some(&folder));
    let path = "scripts/pad.server.lua";
    app.open_script(path, None);
    // Let the watcher take its first look.
    std::thread::sleep(Duration::from_millis(100));

    fs::write(folder.join(path), "print('saved in VS Code')").unwrap();
    let written = Instant::now();
    while app.tabs.get(path).unwrap().text != "print('saved in VS Code')"
        && written.elapsed() < Duration::from_secs(3)
    {
        std::thread::sleep(Duration::from_millis(20));
        app.check_disk();
    }
    let took = written.elapsed();
    let text = app.tabs.get(path).unwrap().text.clone();
    drop(app);
    let _ = fs::remove_dir_all(&folder);

    assert_eq!(text, "print('saved in VS Code')");
    assert!(took < Duration::from_secs(1), "took {took:?}");
}

#[test]
fn script_changes_wait_while_playing() {
    let folder = copy_of_example("hello-touched", "watch-play");
    let mut app = EditorApp::new(Some(&folder));
    std::thread::sleep(Duration::from_millis(100));
    app.play();
    play_for(&mut app, 0.0);
    fs::write(folder.join("scripts/door.server.lua"), "print('door')").unwrap();
    // Long enough for the watcher to notice.
    std::thread::sleep(Duration::from_millis(1200));
    app.check_disk();
    assert_eq!(script_names(&app), ["pad"]);

    app.stop();
    let stopped = Instant::now();
    while script_names(&app).len() == 1 && stopped.elapsed() < Duration::from_secs(3) {
        std::thread::sleep(Duration::from_millis(20));
        app.check_disk();
    }
    let names = script_names(&app);
    drop(app);
    let _ = fs::remove_dir_all(&folder);
    assert_eq!(names, ["door", "pad"]);
}

/// The names of the Instances in `service`, in order.
fn names_in(app: &EditorApp, service: InstanceId) -> Vec<String> {
    let world = &app.world;
    world
        .get(service)
        .unwrap()
        .children()
        .iter()
        .map(|&id| world.get(id).unwrap().name().to_string())
        .collect()
}

#[test]
fn insert_local_script_makes_a_client_file_in_starter_player_scripts() {
    let folder = copy_of_example("hello-touched", "insert-local-script");
    let mut app = EditorApp::new(Some(&folder));
    app.act(Action::InsertScript(ClassName::LocalScript));

    let path = "scripts/LocalScript.client.lua";
    assert_eq!(
        fs::read_to_string(folder.join(path)).unwrap(),
        NEW_SCRIPT_SOURCE
    );
    let service = app.world.starter_player_scripts();
    assert_eq!(names_in(&app, service), ["LocalScript"]);
    let id = app.world.find_first_child(service, "LocalScript").unwrap();
    assert_eq!(app.world.get(id).unwrap().class(), ClassName::LocalScript);
    assert_eq!(app.tabs.active().unwrap().path, path);
    // Server scripts still start first.
    let paths: Vec<&str> = app
        .project
        .as_ref()
        .unwrap()
        .scripts
        .iter()
        .map(|s| s.path.as_str())
        .collect();
    assert_eq!(paths, ["scripts/pad.server.lua", path]);

    // Renaming keeps it a LocalScript.
    app.act(Action::Rename(id, "Keys".to_string()));
    assert!(folder.join("scripts/Keys.client.lua").exists());
    // A client file made elsewhere becomes a LocalScript too.
    fs::write(folder.join("scripts/Camera.client.lua"), "print(1)").unwrap();
    app.disk_changed(&["scripts/Camera.client.lua".to_string()]);
    assert_eq!(names_in(&app, service), ["Camera", "Keys"]);
    let _ = fs::remove_dir_all(&folder);
}

#[test]
fn insert_module_script_makes_a_module_file_in_replicated_storage() {
    let folder = copy_of_example("hello-touched", "insert-module-script");
    let mut app = EditorApp::new(Some(&folder));
    let what = crate::tree::Insertable::RemoteEvent;
    app.act(Action::Insert { what, near: None });
    app.act(Action::InsertScript(ClassName::ModuleScript));

    let path = "scripts/ModuleScript.module.lua";
    assert_eq!(
        fs::read_to_string(folder.join(path)).unwrap(),
        "local module = {}\n\nreturn module\n"
    );
    let service = app.world.replicated_storage();
    // After the saved things, as it would be after loading.
    assert_eq!(names_in(&app, service), ["RemoteEvent", "ModuleScript"]);
    let id = app.world.find_first_child(service, "ModuleScript").unwrap();
    assert_eq!(app.world.get(id).unwrap().class(), ClassName::ModuleScript);
    // Modules are ready before any script starts.
    let paths: Vec<&str> = app
        .project
        .as_ref()
        .unwrap()
        .scripts
        .iter()
        .map(|s| s.path.as_str())
        .collect();
    assert_eq!(paths, [path, "scripts/pad.server.lua"]);

    // Nothing can be put inside it: it couldn't be saved there.
    let part = app.world.create(ClassName::Part).unwrap();
    app.world
        .set_parent(part, Some(app.world.workspace()))
        .unwrap();
    assert!(tree::check_move(&app.world, part, id).is_err());

    // It's a file, not part of the world file.
    assert!(app.save());
    let world_file = fs::read_to_string(folder.join(project::WORLD_FILE)).unwrap();
    assert!(world_file.contains("RemoteEvent") && !world_file.contains("ModuleScript"));
    let _ = fs::remove_dir_all(&folder);
}

#[test]
fn remote_events_are_inserted_into_replicated_storage_and_saved() {
    let folder = copy_of_example("hello-touched", "insert-remote");
    let mut app = EditorApp::new(Some(&folder));
    let what = crate::tree::Insertable::RemoteEvent;
    app.act(Action::Insert { what, near: None });
    let storage = app.world.replicated_storage();
    assert_eq!(names_in(&app, storage), ["RemoteEvent"]);
    assert!(app.save());
    let saved = axcenity_project::load(&folder).unwrap();
    let world = &saved.world;
    let remote = world
        .find_first_child(world.replicated_storage(), "RemoteEvent")
        .unwrap();
    assert_eq!(world.get(remote).unwrap().class(), ClassName::RemoteEvent);
    // It's an ordinary undo step.
    app.undo();
    assert_eq!(names_in(&app, storage), Vec::<String>::new());
    let _ = fs::remove_dir_all(&folder);
}

#[test]
fn a_script_using_input_gets_a_warning_badge_until_it_is_fixed() {
    let folder = copy_of_example("hello-touched", "side-warning");
    let mut app = EditorApp::new(Some(&folder));
    let pad = script(&app, "pad");
    app.update_warnings();
    assert!(app.explorer.warnings.is_empty());

    // Typed in the tab, before saving: the badge shows straight away.
    app.open_script_instance(pad);
    let path = "scripts/pad.server.lua";
    app.tabs.get_mut(path).unwrap().text =
        "print('hi')\nlocal uis = game:GetService('UserInputService')\n".to_string();
    app.update_warnings();
    let badge = &app.explorer.warnings[&pad];
    assert!(
        badge.starts_with("Line 2: UserInputService only works in a LocalScript"),
        "{badge}"
    );
    assert_eq!(app.tabs.warnings[path][0].line, 2);

    app.tabs.get_mut(path).unwrap().text = "print('hi')\n".to_string();
    app.update_warnings();
    assert!(app.explorer.warnings.is_empty() && app.tabs.warnings.is_empty());
    let _ = fs::remove_dir_all(&folder);
}

#[test]
fn play_runs_hello_remote_and_held_arrow_keys_move_the_block() {
    let folder = copy_of_example("hello-remote", "play-remote");
    let mut app = EditorApp::new(Some(&folder));
    let mover = part(&app, "Mover");
    let start = app.world.get(mover).unwrap().property("Position").cloned();
    app.play();
    let right = axcenity_core::KeyCode::Right;
    let key = |pressed| axcenity_script::KeyEvent {
        key: right,
        pressed,
    };
    app.play_frame(1.0 / 60.0, &[key(true)], None, None);
    play_for(&mut app, 0.5);
    app.play_frame(1.0 / 60.0, &[key(false)], None, None);

    let world = app.play.as_ref().unwrap().world();
    let Some(axcenity_core::Value::Vector3(moved)) =
        world.get(mover).unwrap().property("Position").cloned()
    else {
        panic!("the block has no Position");
    };
    assert!(moved.x > 5.0, "{moved:?}");
    assert!(
        printed(&app).contains(&"Hold the arrow keys to move the blue block (Space for help)."),
        "{:?}",
        printed(&app)
    );
    app.stop();
    assert_eq!(
        app.world.get(mover).unwrap().property("Position").cloned(),
        start,
        "Stop puts it back"
    );
    let _ = fs::remove_dir_all(&folder);
}

#[test]
fn scripts_switch_between_code_graph_and_blocks_and_run_the_same() {
    let folder = copy_of_example("hello-part", "graph-script-editor");
    let mut app = EditorApp::new(Some(&folder));
    app.insert_script(ClassName::Script);
    let code_path = app.tabs.active_path().unwrap().to_owned();
    let id = script(&app, "Script");

    // Code to Graph: a block script shown as a graph, saved as one.
    app.switch_script_view(&code_path, ScriptView::Graph);
    let path = app.tabs.active_path().unwrap().to_owned();
    assert!(project::is_block_script(&path), "{path}");
    assert_eq!(script(&app, "Script"), id, "it's the same Script");
    let saved = project::read_block_script(&folder, &path).unwrap();
    assert!(saved.graph.shown, "the file remembers the graph view");
    let document = &app.project.as_ref().unwrap().block_scripts[0];
    assert_eq!(document.script, saved);

    // Graph to Blocks is the same file, saved straight away.
    app.switch_script_view(&path, ScriptView::Blocks);
    assert_eq!(app.tabs.active_path(), Some(path.as_str()));
    assert!(
        !project::read_block_script(&folder, &path)
            .unwrap()
            .graph
            .shown
    );
    assert!(!app.unsaved());
    app.switch_script_view(&path, ScriptView::Graph);
    assert!(
        project::read_block_script(&folder, &path)
            .unwrap()
            .graph
            .shown
    );

    // Graph back to Code: the Lua the graph is, as Blocks gives.
    let blocks_lua = axcenity_blocks::translate(&saved).unwrap().source;
    app.switch_script_view(&path, ScriptView::Code);
    assert_eq!(app.tabs.active_path(), Some(code_path.as_str()));
    let code = fs::read_to_string(folder.join(&code_path)).unwrap();
    assert_eq!(code, blocks_lua);
    assert!(code.contains("print(\"Hello world!\")"), "{code}");
    assert!(!folder.join(&path).exists());
    assert_eq!(script(&app, "Script"), id);
    fs::remove_dir_all(folder).unwrap();
}
