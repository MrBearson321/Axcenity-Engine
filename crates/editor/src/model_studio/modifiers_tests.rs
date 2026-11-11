//! Phase 57: the Studio's modifier stack, its undo steps, Apply, Generate
//! LODs and a clipping Mirror during a move.

use axcenity_mesh_edit::{
    Axis, DeleteKind, MeshOp, ModelDoc, Modifier, ModifierKind, PrimitiveSettings, PrimitiveShape,
    Purpose, SelectMode,
};
use eframe::egui;

use super::tests::{cube_doc, screen, studio_with};
use super::*;

/// A ball cut in half down the middle, its +X half, in Edit mode with
/// nothing picked.
pub(crate) fn half_head_doc() -> StudioDoc {
    let mut settings = PrimitiveSettings::of(PrimitiveShape::UvSphere);
    settings.segments = 16;
    settings.rings = 8;
    let mut doc = StudioDoc::new(String::new(), ModelDoc::primitive(&settings), false);
    doc.toggle_mode();
    doc.set_select_mode(SelectMode::Face);
    doc.select(|selection, mesh| {
        selection.faces = (0..mesh.face_count() as u32)
            .filter(|&f| mesh.face_center(f).x < -1e-4)
            .collect();
    });
    doc.run(MeshOp::Delete(DeleteKind::Faces)).unwrap();
    doc.select(|selection, _| selection.clear());
    doc
}

pub(crate) fn mirror(clip: bool) -> Modifier {
    Modifier::new(ModifierKind::Mirror {
        axis: Axis::X,
        clip,
        merge: true,
        merge_distance: 0.001,
    })
}

pub(crate) fn subdivision() -> Modifier {
    Modifier::new(ModifierKind::Subdivision {
        viewport: 1,
        export: 2,
    })
}

#[test]
fn applying_a_modifier_bakes_it_into_the_mesh_as_one_undo_step() {
    let mut doc = cube_doc();
    doc.set_modifiers("Add Subdivision Surface", vec![subdivision()]);
    doc.end_modifier_edit();
    assert_eq!(
        doc.mesh().unwrap().face_count(),
        6,
        "the stack leaves it be"
    );
    let steps = doc.undo.len();

    doc.apply_modifier(0).unwrap();
    assert_eq!(
        doc.mesh().unwrap().face_count(),
        24,
        "one level, the view's"
    );
    assert!(doc.model.objects[0].modifiers.is_empty());
    assert_eq!(doc.undo.len(), steps + 1);
    assert_eq!(doc.undo_label(), Some("Apply Subdivision Surface"));

    assert!(doc.undo());
    assert_eq!(doc.mesh().unwrap().face_count(), 6);
    assert_eq!(doc.model.objects[0].modifiers, vec![subdivision()]);
    assert!(doc.redo());
    assert_eq!(doc.mesh().unwrap().face_count(), 24);
    assert!(doc.model.objects[0].modifiers.is_empty());
    assert!(doc.apply_modifier(0).is_err(), "nothing left to apply");
}

#[test]
fn a_drag_on_a_modifier_setting_is_one_undo_step() {
    let mut doc = cube_doc();
    doc.set_modifiers(
        "Add Array",
        vec![Modifier::new(ModifierKind::defaults()[2].clone())],
    );
    doc.end_modifier_edit();
    for count in 3..10 {
        let mut stack = doc.model.objects[0].modifiers.clone();
        if let ModifierKind::Array { count: c, .. } = &mut stack[0].kind {
            *c = count;
        }
        doc.set_modifiers("Change Array", stack);
    }
    doc.end_modifier_edit();
    assert_eq!(doc.undo.len(), 2);
    assert_eq!(doc.undo_label(), Some("Change Array"));
    assert!(doc.undo());
    assert_eq!(
        doc.model.objects[0].modifiers[0].kind,
        ModifierKind::defaults()[2]
    );
    // Hiding it is a step of its own, and leaves the mesh drawn as it is.
    let mut stack = doc.model.objects[0].modifiers.clone();
    stack[0].show = false;
    doc.set_modifiers("Hide Array", stack);
    let drawn = doc.model.objects[0].evaluated(Purpose::Viewport).unwrap();
    assert_eq!(drawn.face_count(), 6);
    assert_eq!(doc.undo.len(), 2);
}

#[test]
fn a_clipping_mirror_keeps_vertices_from_crossing_it_in_a_grab() {
    let mut doc = half_head_doc();
    doc.set_modifiers("Add Mirror", vec![mirror(true)]);
    doc.end_modifier_edit();
    let mesh = doc.mesh().unwrap();
    let seam = (0..mesh.vertex_count() as u32)
        .find(|&v| mesh.position(v).x.abs() < 1e-4 && mesh.position(v).y.abs() < 0.9)
        .unwrap();
    let side = (0..mesh.vertex_count() as u32)
        .find(|&v| mesh.position(v).x > 0.5)
        .unwrap();
    let mut studio = studio_with(doc);
    let screen = screen();
    let snap = Snap::default();
    for (vertex, typed) in [(seam, "0.5"), (side, "-5")] {
        let doc = studio.doc.as_mut().unwrap();
        doc.set_select_mode(SelectMode::Vertex);
        doc.select(|selection, _| selection.click(Some(vertex), false));
        assert!(studio.begin_modal(ModalKind::Grab, egui::pos2(400.0, 300.0)));
        let modal = &mut studio.modal.as_mut().unwrap().0;
        modal.lock = modal.lock.press(0);
        modal.typed = typed.to_owned();
        studio.update_modal(&screen, egui::pos2(0.0, 0.0), &snap);
        studio.finish_modal();
        let x = studio
            .doc
            .as_ref()
            .unwrap()
            .mesh()
            .unwrap()
            .position(vertex)
            .x;
        assert_eq!(x, 0.0, "moved {typed} along X");
    }
    // Without clipping the side vertex goes through.
    let doc = studio.doc.as_mut().unwrap();
    doc.set_modifiers("Change Mirror", vec![mirror(false)]);
    assert!(studio.begin_modal(ModalKind::Grab, egui::pos2(400.0, 300.0)));
    let modal = &mut studio.modal.as_mut().unwrap().0;
    modal.lock = modal.lock.press(0);
    modal.typed = "-1".to_owned();
    studio.update_modal(&screen, egui::pos2(0.0, 0.0), &snap);
    studio.finish_modal();
    let x = studio
        .doc
        .as_ref()
        .unwrap()
        .mesh()
        .unwrap()
        .position(side)
        .x;
    assert!(x < -0.5);
}

#[test]
fn generate_lods_adds_the_copies_as_one_undo_step() {
    let mut doc = half_head_doc();
    doc.set_modifiers("Add Mirror", vec![mirror(false), subdivision()]);
    doc.end_modifier_edit();
    let steps = doc.undo.len();
    doc.generate_lods().unwrap();
    let names: Vec<&str> = doc.model.objects.iter().map(|o| o.name.as_str()).collect();
    assert_eq!(names, ["UVSphere", "UVSphere_LOD1", "UVSphere_LOD2"]);
    assert_eq!(doc.mode, Mode::Object);
    assert_eq!(doc.active, 0);
    assert_eq!(doc.undo.len(), steps + 1);
    assert_eq!(doc.undo_label(), Some("Generate LODs"));
    let lod = &doc.model.objects[1];
    assert_eq!(lod.modifiers.len(), 3);
    assert_eq!(lod.modifiers[2].kind, ModifierKind::Decimate { ratio: 0.5 });
    assert!(doc.undo());
    assert_eq!(doc.model.objects.len(), 1);
}

#[test]
fn the_tab_draws_a_mirrored_smooth_head_and_its_stack() {
    let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS") else {
        return;
    };
    let folder = std::path::PathBuf::from(folder);
    let mut doc = half_head_doc();
    doc.set_modifiers("Add Mirror", vec![mirror(true), subdivision()]);
    doc.toggle_mode();
    let mut studio = studio_with(doc);
    crate::snapshot::snapshot_rendered(
        &folder.join("model-studio-modifiers.png"),
        egui::vec2(1000.0, 700.0),
        vec![Vec::new(); 3],
        |ui, render| {
            egui::CentralPanel::default().show(ui, |ui| {
                let _ = studio.show(ui, Some(render), &[], Snap::default(), true);
            });
        },
    );
}
