//! Phase 55 in the editor: the Model Studio tab making, opening and saving
//! models, and MeshParts taking in each save at once.

use std::time::{Duration, Instant};

use axcenity_mesh_edit::{ModelDoc, PrimitiveSettings, SelectMode};
use axcenity_render::glam::{Mat4, Vec3};

use super::tests::copy_of_example;
use super::*;
use crate::model_studio::{Mode, StudioDoc};
use crate::properties::Edit;

/// A MeshPart in the Workspace using `mesh_id`.
fn mesh_part(app: &mut EditorApp, mesh_id: &str) -> InstanceId {
    let workspace = app.world.workspace();
    let part = app.world.create(ClassName::MeshPart).unwrap();
    app.world
        .set_property(part, "MeshId", Value::String(mesh_id.to_owned()))
        .unwrap();
    app.world.set_parent(part, Some(workspace)).unwrap();
    part
}

/// The MeshPart's model as the Scene view draws it this frame.
fn drawn_size(app: &EditorApp, part: InstanceId) -> [f32; 3] {
    let mesh_id = app
        .world
        .get(part)
        .unwrap()
        .property("MeshId")
        .and_then(Value::as_str)
        .unwrap();
    app.assets.model(mesh_id).unwrap().size
}

#[test]
fn a_cube_made_in_the_studio_with_a_face_scaled_up_is_saved_and_used_by_a_meshpart_at_once() {
    let folder = copy_of_example("hello-touched", "studio-cube");
    let mut app = EditorApp::new(Some(&folder));
    let doc = StudioDoc::new(
        String::new(),
        ModelDoc::primitive(&PrimitiveSettings::default()),
        false,
    );
    app.model_studio.open(doc);

    let path = app.save_model_studio().unwrap();
    assert_eq!(path, "assets/models/Cube.glb");
    assert!(folder.join(&path).is_file());
    assert_eq!(app.history.undo_label(), Some("Save Cube.glb"));
    let part = mesh_part(&mut app, &path);
    assert_eq!(drawn_size(&app, part), [2.0, 2.0, 2.0]);
    let first = app.assets.model(&path).unwrap().key();

    // One face scaled up in Edit mode, and saved again.
    let doc = app.model_studio.doc.as_mut().unwrap();
    doc.toggle_mode();
    assert_eq!(doc.mode, Mode::Edit);
    doc.set_select_mode(SelectMode::Face);
    let mesh = doc.mesh().unwrap();
    let top = (0..mesh.face_count() as u32)
        .find(|&f| mesh.face_normal(f).y > 0.9)
        .unwrap();
    doc.selection.click(Some(top), false);
    let center = Vec3::Y;
    let scale = Mat4::from_translation(center)
        * Mat4::from_scale(Vec3::splat(2.0))
        * Mat4::from_translation(-center);
    assert!(doc.transform("Scale", scale));
    assert!(doc.unsaved());

    let saved = Instant::now();
    app.save_model_studio().unwrap();
    // The MeshPart draws the new model now, without waiting for the watcher.
    assert_ne!(app.assets.model(&path).unwrap().key(), first);
    assert_eq!(drawn_size(&app, part), [4.0, 2.0, 4.0]);
    let camera = axcenity_render::Camera {
        position: Vec3::new(10.0, 10.0, 10.0),
        look_at: Vec3::ZERO,
        fov_y_degrees: 50.0,
    };
    let list = axcenity_scene::build_draw_list(&app.world, &app.assets, camera);
    let drawn = app.assets.model(&path).unwrap().primitives[0].key();
    assert!(
        list.items.iter().any(
            |item| matches!(&item.shape, axcenity_render::Shape::Mesh(mesh) if mesh.key() == drawn)
        ),
        "the Scene view draws the saved version"
    );
    assert!(saved.elapsed() < Duration::from_secs(1));
    assert!(!app.model_studio.doc.as_ref().unwrap().unsaved());

    // Undoing the save in the editor puts the first version back, in use.
    app.undo();
    assert_eq!(drawn_size(&app, part), [2.0, 2.0, 2.0]);
    app.redo();
    assert_eq!(drawn_size(&app, part), [4.0, 2.0, 4.0]);
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn an_fbx_opened_from_a_meshpart_lists_its_objects_and_saves_as_a_glb_beside_it() {
    let folder = copy_of_example("hello-graphics", "studio-fbx");
    let mut app = EditorApp::new(Some(&folder));
    let fbx = "assets/models/pbr_panel.fbx";
    let part = mesh_part(&mut app, fbx);
    app.apply(part, Edit::OpenInModelStudio);
    assert!(app.layout.is_open(Pane::ModelStudio));
    let doc = app.model_studio.doc.as_ref().expect("opened");
    assert_eq!(doc.path, "assets/models/pbr_panel.glb");
    assert!(!doc.model.objects.is_empty());
    assert!(doc.mesh().is_some());

    let saved = app.save_model_studio().unwrap();
    assert_eq!(saved, "assets/models/pbr_panel.glb");
    assert!(folder.join(fbx).is_file(), "the FBX stays");
    let original = app.assets.model(fbx).unwrap();
    let copy = app.assets.model(&saved).unwrap();
    assert_eq!(copy.primitives.len(), original.primitives.len());
    for (a, b) in copy.size.iter().zip(original.size) {
        assert!((a - b).abs() < 1e-4, "{:?} {:?}", copy.size, original.size);
    }
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn studio_undo_is_its_own_while_the_mouse_is_over_it() {
    let folder = copy_of_example("hello-touched", "studio-undo");
    let mut app = EditorApp::new(Some(&folder));
    app.model_studio.open(StudioDoc::new(
        String::new(),
        ModelDoc::primitive(&PrimitiveSettings::default()),
        false,
    ));
    let doc = app.model_studio.doc.as_mut().unwrap();
    assert!(doc.transform("Move", Mat4::from_translation(Vec3::X)));
    // Elsewhere, Ctrl+Z is the world's.
    app.undo();
    assert_eq!(
        app.model_studio.doc.as_ref().unwrap().undo_label(),
        Some("Move")
    );
    // Over the tab, it's the Studio's.
    app.model_studio.set_pointer(true);
    app.undo();
    assert_eq!(app.model_studio.doc.as_ref().unwrap().undo_label(), None);
    app.redo();
    assert_eq!(
        app.model_studio.doc.as_ref().unwrap().undo_label(),
        Some("Move")
    );
    let _ = std::fs::remove_dir_all(&folder);
}
