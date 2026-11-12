//! Phase 61: retopology and baking in the Model Studio. Moves and
//! operations snapping to the reference, Poly Build's clicks as undo
//! steps, Make Low Poly, and a snapshot of the baked low-poly face beside
//! the sculpt.

use axcenity_mesh_edit::sculpt::SculptTool;
use axcenity_mesh_edit::{
    DocObject, EditMesh, ExtrudeMode, MeshOp, ModelDoc, PrimitiveSettings, PrimitiveShape,
    Reference, SelectMode, uv,
};
use axcenity_render::glam::{Mat4, Vec3};

use super::sculpt_tests::{front, remeshed_sphere, stroke};
use super::tests::studio_with;
use super::*;

/// How far `p` is from the reference's surface.
fn off_surface(reference: &Reference, p: Vec3) -> f32 {
    reference.nearest(p).unwrap().position.distance(p)
}

/// A UV sphere (the reference) and a small plane of 3 × 3 quads above its
/// top, the plane active in Edit mode with every vertex picked.
fn sphere_and_plane() -> ModelStudio {
    let mut model = ModelDoc::primitive(&PrimitiveSettings::of(PrimitiveShape::UvSphere));
    let mut plane = PrimitiveSettings {
        segments: 3,
        size: 0.6,
        ..PrimitiveSettings::of(PrimitiveShape::Plane)
    }
    .build();
    let all: Vec<u32> = (0..plane.vertex_count() as u32).collect();
    plane.transform(&all, Mat4::from_translation(Vec3::new(0.0, 1.3, 0.0)));
    model.objects.push(DocObject {
        name: "Patch".to_owned(),
        mesh: Some(plane),
        ..model.objects[0].clone()
    });
    let mut studio = studio_with(StudioDoc::new(String::new(), model, false));
    let doc = studio.doc.as_mut().unwrap();
    doc.reference = Some(0);
    doc.click_object(Some(1), false);
    doc.set_mode(Mode::Edit);
    doc.set_select_mode(SelectMode::Vertex);
    doc.select(|selection, mesh| selection.all(mesh));
    studio
}

/// A grab of what's picked by `offset`, as the G key's move does it.
fn grab(studio: &mut ModelStudio, offset: Vec3) {
    studio.refresh_snap();
    let doc = studio.doc.as_mut().unwrap();
    let (center, _) = doc.pivot().unwrap();
    let moving = Moving::begin(doc, center, &MoveSettings::default()).unwrap();
    moving.update(
        doc,
        Mat4::from_translation(offset),
        &Proportional::default(),
    );
    moving.finish(doc, "Grab");
}

#[test]
fn snap_to_surface_puts_moved_and_new_vertices_on_the_reference() {
    let mut studio = sphere_and_plane();
    let sphere = studio.doc.as_ref().unwrap().model.objects[0]
        .mesh
        .clone()
        .unwrap();
    let reference = Reference::new(&sphere);
    assert!(studio.retopo.snap, "on at first");

    // A grab down onto the sphere: every vertex lands on it.
    grab(&mut studio, Vec3::new(0.0, -0.2, 0.0));
    let doc = studio.doc.as_ref().unwrap();
    let mesh = doc.mesh().unwrap();
    for &p in mesh.positions() {
        assert!(off_surface(&reference, p) < 1e-4, "{p} is off the sphere");
    }
    assert_eq!(doc.undo_label(), Some("Grab"));

    // An extrude's new vertices go onto it too.
    let doc = studio.doc.as_mut().unwrap();
    doc.set_select_mode(SelectMode::Face);
    doc.select(|selection, _| selection.click(Some(4), false));
    let before = doc.mesh().unwrap().vertex_count();
    assert!(studio.run_op(MeshOp::Extrude {
        mode: ExtrudeMode::Region,
        distance: 0.3,
    }));
    let mesh = studio.doc.as_ref().unwrap().mesh().unwrap();
    assert_eq!(mesh.vertex_count(), before + 4);
    for &p in mesh.positions() {
        assert!(off_surface(&reference, p) < 1e-4, "{p} is off the sphere");
    }

    // With snapping off, a move leaves them where it puts them.
    studio.retopo.snap = false;
    let doc = studio.doc.as_mut().unwrap();
    doc.set_select_mode(SelectMode::Vertex);
    doc.select(|selection, mesh| selection.all(mesh));
    grab(&mut studio, Vec3::new(0.0, 0.5, 0.0));
    let mesh = studio.doc.as_ref().unwrap().mesh().unwrap();
    assert!(
        mesh.positions()
            .iter()
            .all(|&p| off_surface(&reference, p) > 0.3)
    );
}

#[test]
fn poly_build_clicks_make_quads_on_the_reference_each_an_undo_step() {
    let mut model = ModelDoc::primitive(&PrimitiveSettings::of(PrimitiveShape::UvSphere));
    model.objects.push(DocObject {
        name: "Retopo".to_owned(),
        mesh: Some(EditMesh::default()),
        ..model.objects[0].clone()
    });
    let mut studio = studio_with(StudioDoc::new(String::new(), model, false));
    let doc = studio.doc.as_mut().unwrap();
    doc.reference = Some(0);
    doc.active = 1;
    doc.set_mode(Mode::Edit);
    let reference = studio.reference_surface().unwrap();
    // Where the view's ray from the front meets the sphere.
    let on = |x: f32, y: f32| {
        let origin = Vec3::new(x, y, 5.0);
        reference.ray(origin, -Vec3::Z).unwrap().0.position
    };
    studio.retopo.poly_build = Some(Vec::new());
    for (x, y) in [(-0.2, -0.2), (0.2, -0.2), (0.2, 0.2)] {
        studio.poly_build_click(on(x, y), None, 0.01, Vec3::Z);
    }
    assert_eq!(studio.retopo.poly_build.as_ref().unwrap().len(), 3);
    assert_eq!(studio.doc.as_ref().unwrap().mesh().unwrap().face_count(), 0);
    studio.poly_build_click(on(-0.2, 0.2), None, 0.01, Vec3::Z);
    let doc = studio.doc.as_ref().unwrap();
    let mesh = doc.mesh().unwrap();
    assert_eq!((mesh.vertex_count(), mesh.face_count()), (4, 1));
    assert!(mesh.face_normal(0).z > 0.9, "faces the camera");
    assert_eq!(doc.undo_label(), Some("Poly Build"));
    assert_eq!(doc.selection.faces, [0].into());

    // Near the right-hand open edge: a quad out from it.
    let right = (0..mesh.half_edges().len() as u32)
        .find(|&e| {
            let (a, b) = mesh.ends(e);
            mesh.position(a).x > 0.1 && mesh.position(b).x > 0.1
        })
        .unwrap();
    studio.poly_build_click(on(0.5, 0.0), Some(right), 0.01, Vec3::Z);
    let doc = studio.doc.as_mut().unwrap();
    let mesh = doc.mesh().unwrap();
    assert_eq!((mesh.vertex_count(), mesh.face_count()), (6, 2));
    for &p in mesh.positions() {
        assert!(off_surface(&reference, p) < 1e-4);
    }
    axcenity_mesh_edit::check_manifold(mesh, false).unwrap();
    assert!(doc.undo());
    assert_eq!(doc.mesh().unwrap().face_count(), 1);
}

#[test]
fn make_low_poly_adds_an_unwrapped_copy_under_the_count_as_one_step() {
    let mut studio = remeshed_sphere();
    let doc = studio.doc.as_mut().unwrap();
    doc.set_mode(Mode::Object);
    doc.reference = Some(0);
    let high_faces = doc.mesh().unwrap().face_count();
    studio.retopo.target = 800;
    studio.make_low_poly();
    let doc = studio.doc.as_ref().unwrap();
    assert_eq!(doc.model.objects.len(), 2);
    assert_eq!(doc.active, 1);
    assert_eq!(doc.reference, Some(0));
    let low = &doc.model.objects[1];
    assert_eq!(low.name, "UVSphere_Low");
    assert!(low.uvs);
    let mesh = low.mesh.as_ref().unwrap();
    assert!(mesh.face_count() <= 800 && mesh.face_count() > 600);
    assert!(mesh.face_count() * 3 < high_faces);
    let faces: Vec<u32> = (0..mesh.face_count() as u32).collect();
    assert!(uv::overlapping_islands(mesh, &faces, 0).is_empty());
    assert_eq!(doc.undo_label(), Some("Make Low Poly"));
    let doc = studio.doc.as_mut().unwrap();
    assert!(doc.undo());
    assert_eq!(doc.model.objects.len(), 1);
}

/// A New UV sphere voxel remeshed, sculpted into a face from the front
/// (brow, nose, mouth) and subdivided: the high-poly reference.
pub fn sculpted_face_studio() -> ModelStudio {
    let mut studio = remeshed_sphere();
    let screen = front();
    for (tool, from, to) in [
        (
            SculptTool::Draw,
            Vec3::new(0.08, 0.35, 0.94),
            Vec3::new(0.5, 0.35, 0.8),
        ),
        (
            SculptTool::Draw,
            Vec3::new(-0.08, 0.35, 0.94),
            Vec3::new(-0.5, 0.35, 0.8),
        ),
        (
            SculptTool::Draw,
            Vec3::new(0.0, 0.2, 1.0),
            Vec3::new(0.0, -0.15, 1.0),
        ),
        (
            SculptTool::Crease,
            Vec3::new(-0.35, -0.42, 0.83),
            Vec3::new(0.35, -0.42, 0.83),
        ),
    ] {
        studio.sculpt.brush.tool = tool;
        studio.sculpt.brush.strength = 1.0;
        assert!(studio.sculpt_stroke(&screen, &stroke(&screen, from, to), false));
    }
    studio.sculpt_detail(false);
    let doc = studio.doc.as_mut().unwrap();
    doc.set_mode(Mode::Object);
    doc.reference = Some(0);
    studio
}

#[test]
fn snapshot_of_the_baked_low_poly_face_beside_the_sculpt() {
    let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS") else {
        return;
    };
    let folder = std::path::PathBuf::from(folder);
    let mut studio = sculpted_face_studio();
    studio.make_low_poly();
    let settings = axcenity_mesh_edit::BakeSettings {
        size: 1024,
        ..Default::default()
    };
    let doc = studio.doc.as_mut().unwrap();
    let report = doc.bake("assets/models/Face.glb", &settings).unwrap();
    studio.bake_done(&report);
    // Side by side, looking at the front: the low-poly mesh alone (left),
    // the sculpt (middle) and the baked low-poly mesh (right).
    let doc = studio.doc.as_mut().unwrap();
    let mut plain = doc.model.objects[1].clone();
    plain.name = "Plain".to_owned();
    plain.materials = doc.model.objects[0].materials.clone();
    doc.model.objects.push(plain);
    for (index, x) in [(1, 2.4), (2, -2.4)] {
        let mesh = doc.model.objects[index].mesh.as_mut().unwrap();
        let all: Vec<u32> = (0..mesh.vertex_count() as u32).collect();
        mesh.transform(&all, Mat4::from_translation(Vec3::new(x, 0.0, 0.0)));
    }
    // Nothing picked, so no object is drawn highlighted.
    doc.active = 1;
    doc.objects.clear();
    doc.revision += 1;
    studio.view.frame_doc(studio.doc.as_ref().unwrap());
    studio.view.look_from(0.0, 5.0, 1.6);
    crate::snapshot::snapshot_rendered(
        &folder.join("model-studio-bake.png"),
        egui::vec2(1300.0, 760.0),
        vec![Vec::new(); 3],
        |ui, render| {
            egui::CentralPanel::default().show(ui, |ui| {
                let _ = studio.show(ui, Some(render), &[], crate::gizmo::Snap::default(), true);
            });
        },
    );
}
