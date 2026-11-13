//! Baking (Phase 61, back in R15) in the Model Studio: Make Low Poly as
//! one undo step, the Bake fold, and a snapshot of the baked low-poly face
//! beside the sculpt.

use axcenity_mesh_edit::sculpt::SculptTool;
use axcenity_mesh_edit::uv;
use axcenity_render::glam::{Mat4, Vec3};

use super::sculpt_tests::{front, remeshed_sphere, stroke};
use super::*;

#[test]
fn make_low_poly_adds_an_unwrapped_copy_under_the_count_as_one_step() {
    let mut studio = remeshed_sphere();
    let doc = studio.doc.as_mut().unwrap();
    doc.set_mode(Mode::Object);
    doc.reference = Some(0);
    let high_faces = doc.mesh().unwrap().face_count();
    studio.bake.target = 800;
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
