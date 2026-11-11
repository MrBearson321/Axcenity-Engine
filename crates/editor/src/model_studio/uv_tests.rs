//! Phase 58: the UV menu and the UV editor in the Model Studio.

use axcenity_mesh_edit::{SelectMode, overlapping_islands, stretch, uv_islands};
use axcenity_render::glam::Vec2;
use eframe::egui;

use super::tests::{cube_doc, studio_with};
use super::tools_tests::model_chair;
use super::uv_editor::{UvAction, UvScreen};
use super::*;

/// The most any face of the active mesh is stretched, by angle and by
/// area, in uv channel 0.
fn worst_stretch(doc: &StudioDoc) -> (f32, f32) {
    let mesh = doc.mesh().unwrap();
    let faces: Vec<u32> = (0..mesh.face_count() as u32).collect();
    stretch(mesh, &faces, 0)
        .into_iter()
        .fold((0.0, 0.0), |(angle, area), s| {
            (angle.max(s.angle), area.max(s.area))
        })
}

/// The chair, smart-projected with nothing picked (so every face).
pub(super) fn projected_chair() -> ModelStudio {
    let mut doc = cube_doc();
    model_chair(&mut doc);
    doc.select(|selection, _| selection.clear());
    let mut studio = studio_with(doc);
    assert!(studio.run_uv(UvAction::SmartProject));
    studio
}

#[test]
fn smart_uv_project_lays_the_chair_out_without_overlap_and_with_even_checker_squares() {
    let studio = projected_chair();
    let doc = studio.doc.as_ref().unwrap();
    assert_eq!(doc.undo_label(), Some("Smart UV Project"));
    let mesh = doc.mesh().unwrap();
    let faces: Vec<u32> = (0..mesh.face_count() as u32).collect();
    assert!(uv_islands(mesh, &faces, 0).len() > 6);
    assert_eq!(
        overlapping_islands(mesh, &faces, 0),
        Vec::<[usize; 2]>::new()
    );
    assert!(
        mesh.half_edges()
            .iter()
            .all(|e| e.uv.cmpge(Vec2::splat(-1e-4)).all() && e.uv.cmple(Vec2::splat(1.0001)).all()),
        "inside the texture"
    );
    // Even squares: no face's corners bent, or its share of the checker
    // grown or shrunk, by a tenth or more.
    let (angle, area) = worst_stretch(doc);
    assert!(angle < 0.1, "angle stretch {angle}");
    assert!(area < 0.1, "area stretch {area}");
}

#[test]
fn seams_marked_from_edit_mode_unwrap_a_cube_into_six_islands() {
    let mut doc = cube_doc();
    doc.toggle_mode();
    doc.set_select_mode(SelectMode::Edge);
    doc.select(|selection, mesh| selection.all(mesh));
    let mut studio = studio_with(doc);
    assert!(studio.run_uv(UvAction::MarkSeam));
    let doc = studio.doc.as_mut().unwrap();
    assert_eq!(doc.mesh().unwrap().seams().len(), 12);
    assert_eq!(doc.undo_label(), Some("Mark Seam"));
    doc.select(|selection, _| selection.clear());
    assert!(studio.run_uv(UvAction::Unwrap));
    let doc = studio.doc.as_mut().unwrap();
    let mesh = doc.mesh().unwrap();
    let faces: Vec<u32> = (0..6).collect();
    assert_eq!(uv_islands(mesh, &faces, 0).len(), 6);
    assert!(overlapping_islands(mesh, &faces, 0).is_empty());
    // Each is one undo step.
    assert!(doc.undo());
    assert!(doc.undo());
    assert!(doc.mesh().unwrap().seams().is_empty());
}

#[test]
fn a_uv_grab_moves_the_picked_island_and_is_one_undo_step() {
    let mut studio = projected_chair();
    let screen = UvScreen {
        rect: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 400.0)),
        zoom: 1.0,
        pan: Vec2::splat(0.5),
    };
    studio.uv.select = super::uv_editor::UvSelect::Island;
    let doc = studio.doc.as_ref().unwrap();
    let mesh = doc.mesh().unwrap();
    // The middle of the first face's uvs, on screen.
    let corners = mesh.face_half_edges(0);
    let middle = corners.iter().map(|&c| mesh.uv(c, 0)).sum::<Vec2>() / corners.len() as f32;
    let before: Vec<Vec2> = (0..mesh.half_edges().len() as u32)
        .map(|c| mesh.uv(c, 0))
        .collect();
    let at = screen.point(middle);
    studio.click_uvs(&screen, at, false);
    let picked = studio.uv.selected.len();
    assert!(picked >= corners.len(), "the face's island");
    let steps = studio.doc.as_ref().unwrap().undo.len();

    assert!(studio.begin_uv_modal(ModalKind::Grab, at, screen.uv(at)));
    studio.update_uv_modal(&screen, at + egui::vec2(40.0, 0.0));
    studio.update_uv_modal(&screen, at + egui::vec2(80.0, 0.0));
    studio.finish_uv_modal();
    let doc = studio.doc.as_mut().unwrap();
    assert_eq!(doc.undo.len(), steps + 1);
    assert_eq!(doc.undo_label(), Some("Move UVs"));
    let mesh = doc.mesh().unwrap();
    let moved = (0..mesh.half_edges().len() as u32)
        .filter(|&c| (mesh.uv(c, 0) - before[c as usize]).abs_diff_eq(Vec2::new(0.2, 0.0), 1e-4))
        .count();
    assert_eq!(
        moved, picked,
        "the island went a fifth of the texture right"
    );
    assert!(doc.undo());
    let mesh = doc.mesh().unwrap();
    assert!((0..mesh.half_edges().len() as u32).all(|c| mesh.uv(c, 0) == before[c as usize]));
}

#[test]
fn pins_from_the_uv_editor_hold_through_an_unwrap() {
    let mut doc = cube_doc();
    doc.toggle_mode();
    doc.set_select_mode(SelectMode::Edge);
    doc.select(|selection, mesh| selection.all(mesh));
    let mut studio = studio_with(doc);
    assert!(studio.run_uv(UvAction::MarkSeam));
    studio
        .doc
        .as_mut()
        .unwrap()
        .select(|selection, _| selection.clear());
    assert!(studio.run_uv(UvAction::Unwrap));
    let mesh = studio.doc.as_ref().unwrap().mesh().unwrap();
    let corner = mesh.face_half_edges(0)[0];
    let held = mesh.uv(corner, 0);
    studio.uv.selected = [corner].into();
    assert!(studio.run_uv(UvAction::Pin));
    assert!(studio.run_uv(UvAction::Unwrap));
    let mesh = studio.doc.as_ref().unwrap().mesh().unwrap();
    assert!(mesh.uv(corner, 0).abs_diff_eq(held, 1e-5));
}

#[test]
fn the_tab_draws_the_chair_with_a_checker_and_its_uvs() {
    let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS") else {
        return;
    };
    let folder = std::path::PathBuf::from(folder);
    let mut studio = projected_chair();
    studio.uv.open = true;
    studio.uv.checker = true;
    crate::snapshot::snapshot_rendered(
        &folder.join("model-studio-uvs.png"),
        egui::vec2(1200.0, 600.0),
        vec![Vec::new(); 3],
        |ui, render| {
            egui::CentralPanel::default().show(ui, |ui| {
                let _ = studio.show(ui, Some(render), &[], crate::gizmo::Snap::default(), true);
            });
        },
    );
    let mut studio = projected_chair();
    studio.uv.open = true;
    studio.uv.stretch = super::uv_editor::StretchShow::Area;
    crate::snapshot::snapshot_rendered(
        &folder.join("model-studio-uv-stretch.png"),
        egui::vec2(1200.0, 600.0),
        vec![Vec::new(); 3],
        |ui, render| {
            egui::CentralPanel::default().show(ui, |ui| {
                let _ = studio.show(ui, Some(render), &[], crate::gizmo::Snap::default(), true);
            });
        },
    );
}
