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

#[test]
fn a_chair_modeled_from_one_cube_is_saved_and_drawn_by_a_meshpart_in_the_world() {
    let folder = copy_of_example("hello-touched", "studio-chair");
    let mut app = EditorApp::new(Some(&folder));
    let doc = StudioDoc::new(
        String::new(),
        ModelDoc::primitive(&PrimitiveSettings::default()),
        false,
    );
    app.model_studio.open(doc);
    crate::model_studio::tools_tests::model_chair(app.model_studio.doc.as_mut().unwrap());

    let path = app.save_model_studio().unwrap();
    assert_eq!(path, "assets/models/Cube.glb");
    let part = mesh_part(&mut app, &path);
    // Legs to backrest: 1.75 below the seat's middle to 1.95 above.
    let [x, y, z] = drawn_size(&app, part);
    assert!((x - 2.0).abs() < 1e-4 && (y - 3.7).abs() < 1e-4 && (z - 2.0).abs() < 1e-4);

    // Saved as triangles: every polygon cut into (corners − 2).
    let mesh = app.model_studio.doc.as_ref().unwrap().mesh().unwrap();
    let triangles: usize = (0..mesh.face_count() as u32)
        .map(|f| mesh.face_vertices(f).len() - 2)
        .sum();
    let model = app.assets.model(&path).unwrap();
    let saved: usize = model.primitives.iter().map(|p| p.indices.len() / 3).sum();
    assert_eq!(saved, triangles);

    // The Scene view draws it, and the file opens again as the same chair.
    let camera = axcenity_render::Camera {
        position: Vec3::new(8.0, 6.0, 8.0),
        look_at: Vec3::ZERO,
        fov_y_degrees: 50.0,
    };
    let list = axcenity_scene::build_draw_list(&app.world, &app.assets, camera);
    let drawn = model.primitives[0].key();
    assert!(list.items.iter().any(
        |item| matches!(&item.shape, axcenity_render::Shape::Mesh(mesh) if mesh.key() == drawn)
    ));
    let bytes = std::fs::read(folder.join(&path)).unwrap();
    let reopened = ModelDoc::open(
        &bytes,
        &folder.join("assets/models"),
        axcenity_assets::ModelFormat::Gltf,
    )
    .unwrap();
    let again = reopened.objects[0].mesh.as_ref().unwrap();
    axcenity_mesh_edit::check_manifold(again, true).unwrap();
    assert_eq!(again.vertex_count(), mesh.vertex_count());
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn a_half_head_with_mirror_and_subdivision_saves_whole_and_reopens_as_the_half_with_its_stack() {
    use crate::model_studio::modifiers_tests::{half_head_doc, mirror, subdivision};
    let folder = copy_of_example("hello-touched", "studio-head");
    let mut app = EditorApp::new(Some(&folder));
    let mut doc = half_head_doc();
    let half = doc.mesh().unwrap().clone();
    let stack = vec![mirror(true), subdivision()];
    doc.set_modifiers("Add Mirror", stack.clone());
    app.model_studio.open(doc);

    let path = app.save_model_studio().unwrap();
    assert_eq!(path, "assets/models/UVSphere.glb");
    let sidecar = "assets/models/UVSphere.meshedit.json";
    assert!(folder.join(sidecar).is_file());
    // What the game draws is the whole head: as wide as it is deep (the
    // half was half as wide).
    let part = mesh_part(&mut app, &path);
    let [x, y, z] = drawn_size(&app, part);
    assert!((x - z).abs() < 1e-3 && x > 1.8, "{x} {y} {z}");
    let drawn = app.assets.model(&path).unwrap();
    let triangles: usize = drawn.primitives.iter().map(|p| p.indices.len() / 3).sum();
    assert!(
        triangles > half.face_count() * 2 * 16,
        "{triangles}: smoothed twice"
    );

    // Opened again: the half mesh and its stack, still editable.
    app.open_in_model_studio(&path);
    let doc = app.model_studio.doc.as_mut().unwrap();
    assert!(!doc.unsaved());
    assert_eq!(doc.mesh(), Some(&half));
    assert_eq!(doc.model.objects[0].modifiers, stack);
    doc.toggle_mode();
    doc.set_select_mode(SelectMode::Face);
    doc.select(|selection, mesh| {
        selection.faces = (0..mesh.face_count() as u32)
            .filter(|&f| mesh.face_normal(f).y > 0.9)
            .collect();
    });
    let top = Mat4::from_translation(Vec3::Y * 0.5);
    assert!(doc.transform("Move", top));
    app.save_model_studio().unwrap();
    let [_, taller, _] = drawn_size(&app, part);
    assert!(taller > y + 0.2, "{taller} against {y}");

    // Without modifiers the sidecar goes, and undoing the save brings it back.
    let doc = app.model_studio.doc.as_mut().unwrap();
    doc.set_modifiers("Remove Mirror", Vec::new());
    app.save_model_studio().unwrap();
    assert!(!folder.join(sidecar).exists());
    app.undo();
    assert!(folder.join(sidecar).is_file());
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn generated_lods_are_drawn_far_away_by_a_meshpart_without_another_step() {
    let folder = copy_of_example("hello-touched", "studio-lods");
    let mut app = EditorApp::new(Some(&folder));
    let mut settings = PrimitiveSettings::of(axcenity_mesh_edit::PrimitiveShape::UvSphere);
    settings.segments = 32;
    settings.rings = 16;
    let mut doc = StudioDoc::new(String::new(), ModelDoc::primitive(&settings), false);
    doc.generate_lods().unwrap();
    let wanted: Vec<usize> = doc.model.objects[1..]
        .iter()
        .map(|object| {
            let mesh = object
                .evaluated(axcenity_mesh_edit::Purpose::Export)
                .unwrap();
            (0..mesh.face_count() as u32)
                .map(|f| mesh.triangles(f).len())
                .sum()
        })
        .collect();
    app.model_studio.open(doc);
    let path = app.save_model_studio().unwrap();

    for mesh_id in [path.clone(), format!("{path}#UVSphere")] {
        let part = mesh_part(&mut app, &mesh_id);
        let model = app.assets.model(&mesh_id).unwrap();
        assert_eq!(
            model.primitives.len(),
            1,
            "{mesh_id}: the copies aren't parts"
        );
        let levels: Vec<usize> = model.primitives[0]
            .lods
            .iter()
            .map(|lod| lod.indices.len() / 3)
            .collect();
        assert_eq!(levels, wanted, "{mesh_id}: the file's own copies");
        app.world.destroy(part).unwrap();
    }
    let _ = std::fs::remove_dir_all(&folder);
}

/// Each triangle corner of `model` in its file's units: position and uv,
/// rounded so float noise doesn't count, sorted.
fn corner_uvs(model: &axcenity_assets::Model) -> Vec<([i64; 3], [i64; 2])> {
    let size = Vec3::from(model.size);
    let scale = Vec3::select(size.cmpgt(Vec3::ZERO), size, Vec3::ONE);
    let center = Vec3::from(model.center);
    let mut corners: Vec<_> = model
        .primitives
        .iter()
        .flat_map(|primitive| {
            primitive.indices.iter().map(move |&k| {
                let p = Vec3::from(primitive.positions[k as usize]) * scale + center;
                let uv = primitive.uvs[k as usize];
                (
                    (p * 1e4).round().as_i64vec3().to_array(),
                    uv.map(|c| (c * 1e4).round() as i64),
                )
            })
        })
        .collect();
    corners.sort_unstable();
    corners
}

#[test]
fn unwrapped_uvs_save_in_the_glb_and_draw_the_same_texture_in_the_game_as_in_the_studio() {
    use crate::model_studio::uv_editor::UvAction;
    let folder = copy_of_example("hello-touched", "studio-uvs");
    let mut app = EditorApp::new(Some(&folder));
    // A texture beside the model, which the cube's material draws.
    let models = folder.join("assets/models");
    std::fs::create_dir_all(&models).unwrap();
    let pixels = image::RgbaImage::from_fn(4, 4, |x, y| {
        image::Rgba([(x * 60) as u8, (y * 60) as u8, 200, 255])
    });
    pixels.save(models.join("bricks.png")).unwrap();
    let mut texture = axcenity_assets::Image::new(4, 4, pixels.clone().into_raw());
    texture.file = Some("bricks.png".to_owned());
    let mut model = ModelDoc::primitive(&PrimitiveSettings::default());
    model.objects[0].materials[0].base_color_texture = Some(std::sync::Arc::new(texture));
    app.model_studio
        .open(StudioDoc::new(String::new(), model, false));

    // Every edge a seam, unwrapped into six squares.
    let doc = app.model_studio.doc.as_mut().unwrap();
    doc.toggle_mode();
    doc.set_select_mode(SelectMode::Edge);
    doc.select(|selection, mesh| selection.all(mesh));
    assert!(app.model_studio.run_uv(UvAction::MarkSeam));
    let doc = app.model_studio.doc.as_mut().unwrap();
    doc.select(|selection, _| selection.clear());
    assert!(app.model_studio.run_uv(UvAction::Unwrap));
    let studio = app
        .model_studio
        .doc
        .as_ref()
        .unwrap()
        .model
        .to_model()
        .unwrap();

    let path = app.save_model_studio().unwrap();
    let part = mesh_part(&mut app, &path);
    let _ = drawn_size(&app, part);
    // What the game loads: the saved file, read as the player reads it.
    let bytes = std::fs::read(folder.join(&path)).unwrap();
    let game =
        axcenity_assets::load_model(&bytes, &models, axcenity_assets::ModelFormat::Gltf).unwrap();
    for drawn in [&game, &app.assets.model(&path).unwrap()] {
        assert_eq!(corner_uvs(drawn), corner_uvs(&studio), "the same uvs");
        let texture = drawn.primitives[0]
            .material
            .base_color_texture
            .as_ref()
            .expect("the texture is drawn");
        let shown = studio.primitives[0]
            .material
            .base_color_texture
            .as_ref()
            .unwrap();
        assert_eq!((texture.width, texture.height), (shown.width, shown.height));
        assert_eq!(texture.rgba, shown.rgba, "the same texture");
    }
    // Reopened, the seams are back from the sidecar and the uvs from the file.
    app.open_in_model_studio(&path);
    let reopened = app.model_studio.doc.as_ref().unwrap();
    let mesh = reopened.mesh().unwrap();
    assert_eq!(mesh.seams().len(), 12);
    assert_eq!(
        corner_uvs(&reopened.model.to_model().unwrap()),
        corner_uvs(&studio)
    );
    let _ = std::fs::remove_dir_all(&folder);
}

/// The chair made in the Studio, smart-projected, in Paint mode painting
/// at 256 texels.
fn painting_chair(app: &mut EditorApp) {
    use crate::model_studio::uv_editor::UvAction;
    let doc = StudioDoc::new(
        String::new(),
        ModelDoc::primitive(&PrimitiveSettings::default()),
        false,
    );
    app.model_studio.open(doc);
    let doc = app.model_studio.doc.as_mut().unwrap();
    crate::model_studio::tools_tests::model_chair(doc);
    doc.select(|selection, _| selection.clear());
    assert!(app.model_studio.run_uv(UvAction::SmartProject));
    let doc = app.model_studio.doc.as_mut().unwrap();
    doc.set_mode(Mode::Paint);
    app.model_studio.paint.size = 256;
    app.model_studio.paint.brush.strength = 1.0;
}

/// Paints `kind` across the seat with `brush` (a color, or a value).
fn paint_seat(app: &mut EditorApp, kind: axcenity_mesh_edit::MapKind, color: [u8; 3], value: f32) {
    let screen = crate::model_studio::tests::screen();
    let points = crate::model_studio::paint_tests::stroke(
        &screen,
        Vec3::new(-0.6, 0.11, 0.3),
        Vec3::new(0.6, 0.11, 0.3),
    );
    let paint = &mut app.model_studio.paint;
    paint.map = kind;
    paint.brush.color = color;
    paint.brush.value = value;
    assert!(app.model_studio.paint_stroke(&screen, &points));
}

#[test]
fn rust_painted_on_the_chair_saves_maps_the_scene_view_draws_with_the_material_set_up() {
    use axcenity_mesh_edit::MapKind;
    let folder = copy_of_example("hello-touched", "studio-paint");
    let mut app = EditorApp::new(Some(&folder));
    painting_chair(&mut app);
    paint_seat(&mut app, MapKind::BaseColor, [150, 70, 30], 0.0);
    paint_seat(&mut app, MapKind::Roughness, [0; 3], 1.0);
    let set = app
        .model_studio
        .doc
        .as_ref()
        .unwrap()
        .paint_set(0)
        .unwrap()
        .clone();
    let color = set.map(MapKind::BaseColor).unwrap().flat().to_vec();
    let rough: Vec<u8> = set
        .map(MapKind::Roughness)
        .unwrap()
        .flat()
        .chunks(4)
        .map(|p| p[0])
        .collect();

    let path = app.save_model_studio().unwrap();
    // Named by role beside the model, as `assets` reads maps.
    let models = folder.join("assets/models");
    for name in [
        "Cube_Cube_Material_BaseColor.png",
        "Cube_Cube_Material_MetallicRoughness.png",
    ] {
        assert!(models.join("textures").join(name).exists(), "{name}");
    }
    let part = mesh_part(&mut app, &path);
    let _ = drawn_size(&app, part);
    // The game's own load, and the Scene view's model, use the maps with
    // nothing set up by hand.
    let bytes = std::fs::read(folder.join(&path)).unwrap();
    let game =
        axcenity_assets::load_model(&bytes, &models, axcenity_assets::ModelFormat::Gltf).unwrap();
    for model in [&game, &app.assets.model(&path).unwrap()] {
        let material = &model.primitives[0].material;
        assert_eq!(material.base_color, [1.0; 4]);
        assert_eq!(material.base_color_texture.as_ref().unwrap().rgba, color);
        let roughness = material.roughness_texture.as_ref().unwrap();
        assert_eq!(roughness.channel, axcenity_assets::Channel::Green);
        let saved: Vec<u8> = roughness.image.rgba.chunks(4).map(|p| p[1]).collect();
        assert_eq!(saved, rough);
    }
    // The Scene view's draw list has the part with its painted maps.
    let camera = axcenity_render::Camera {
        position: Vec3::new(8.0, 6.0, 8.0),
        look_at: Vec3::ZERO,
        fov_y_degrees: 50.0,
    };
    let list = axcenity_scene::build_draw_list(&app.world, &app.assets, camera);
    let drawn = app.assets.model(&path).unwrap().primitives[0].key();
    let item = list
        .items
        .iter()
        .find(
            |item| matches!(&item.shape, axcenity_render::Shape::Mesh(mesh) if mesh.key() == drawn),
        )
        .expect("the part is drawn");
    let texture = item
        .material
        .base_color_texture
        .as_ref()
        .expect("painted color");
    assert_eq!(
        axcenity_render::TextureSource::rgba(&**texture),
        color.as_slice()
    );
    assert!(item.material.maps.roughness.is_some(), "painted roughness");
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn a_hidden_layer_stays_out_of_the_saved_maps_and_is_still_there_after_reopening() {
    use axcenity_mesh_edit::MapKind;
    let folder = copy_of_example("hello-touched", "studio-paint-layers");
    let mut app = EditorApp::new(Some(&folder));
    painting_chair(&mut app);
    paint_seat(&mut app, MapKind::BaseColor, [150, 70, 30], 0.0);
    let rust_only = {
        let doc = app.model_studio.doc.as_ref().unwrap();
        doc.paint_set(0)
            .unwrap()
            .map(MapKind::BaseColor)
            .unwrap()
            .flat()
            .to_vec()
    };
    let doc = app.model_studio.doc.as_mut().unwrap();
    doc.change_paint(0, "Add Layer", None, |set| {
        set.map_mut(MapKind::BaseColor).unwrap().add_layer();
    });
    paint_seat(&mut app, MapKind::BaseColor, [20, 40, 220], 0.0);
    let doc = app.model_studio.doc.as_mut().unwrap();
    doc.change_paint(0, "Hide Layer", None, |set| {
        let map = set.map_mut(MapKind::BaseColor).unwrap();
        map.layers[1].visible = false;
        map.refresh_all();
    });
    let blue_layer = doc
        .paint_set(0)
        .unwrap()
        .map(MapKind::BaseColor)
        .unwrap()
        .layers[1]
        .clone();
    assert!(!blue_layer.is_empty());

    let path = app.save_model_studio().unwrap();
    let saved = image::open(folder.join("assets/models/textures/Cube_Cube_Material_BaseColor.png"))
        .unwrap()
        .into_rgba8()
        .into_raw();
    assert_eq!(saved, rust_only, "the hidden layer's paint isn't saved");
    assert!(
        folder
            .join("assets/models/Cube.meshedit/0-0-BaseColor-1.png")
            .exists()
    );

    app.open_in_model_studio(&path);
    let doc = app.model_studio.doc.as_ref().unwrap();
    let map = doc
        .paint_set(0)
        .expect("painted again from the sidecar")
        .map(MapKind::BaseColor)
        .unwrap();
    assert_eq!(map.layers.len(), 2);
    assert_eq!(map.layers[1], blue_layer, "still there, still hidden");
    assert_eq!(map.flat(), rust_only.as_slice());
    let _ = std::fs::remove_dir_all(&folder);
}
