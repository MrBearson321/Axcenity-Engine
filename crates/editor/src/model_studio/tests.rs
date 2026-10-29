//! Phase 55: the Model Studio's document, undo history, keyboard moves and
//! handles.

use axcenity_mesh_edit::{ModelDoc, PrimitiveSettings, PrimitiveShape, SelectMode};
use axcenity_render::Camera;
use axcenity_render::glam::{Mat4, Quat, Vec3};
use eframe::egui;

use super::view::{boxed, handle_pose, nearest_edge, nearest_vertex, pick_face, pose_change};
use super::*;
use crate::gizmo::{Drag, Handle, Modes, Pointer, Pose};
use crate::screen::Screen;

fn cube_doc() -> StudioDoc {
    StudioDoc::new(
        String::new(),
        ModelDoc::primitive(&PrimitiveSettings::default()),
        false,
    )
}

fn studio_with(doc: StudioDoc) -> ModelStudio {
    let mut studio = ModelStudio::default();
    studio.open(doc);
    studio
}

fn positions(doc: &StudioDoc) -> Vec<Vec3> {
    doc.model
        .objects
        .iter()
        .filter_map(|object| object.mesh.as_ref())
        .flat_map(|mesh| mesh.positions().to_vec())
        .collect()
}

fn screen() -> Screen {
    let camera = Camera {
        position: Vec3::new(6.0, 5.0, 8.0),
        look_at: Vec3::ZERO,
        fov_y_degrees: 50.0,
    };
    Screen::new(
        camera,
        egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0)),
    )
}

/// The top face of the cube.
fn top_face(doc: &StudioDoc) -> u32 {
    let mesh = doc.mesh().unwrap();
    (0..mesh.face_count() as u32)
        .find(|&f| mesh.face_normal(f).y > 0.9)
        .unwrap()
}

#[test]
fn tab_switches_modes_and_number_keys_switch_what_is_picked() {
    let mut doc = cube_doc();
    assert_eq!(doc.mode, Mode::Object);
    doc.toggle_mode();
    assert_eq!(doc.mode, Mode::Edit);
    doc.selection.click(Some(0), false);
    doc.set_select_mode(SelectMode::Edge);
    assert_eq!(doc.selection.mode, SelectMode::Edge);
    doc.toggle_mode();
    assert_eq!(doc.mode, Mode::Object);
}

#[test]
fn every_keyboard_move_undoes_and_redoes() {
    let mut studio = studio_with(cube_doc());
    let screen = screen();
    let snap = Snap::default();
    let start = positions(studio.doc.as_ref().unwrap());

    // Edit mode: scale the top face by 2, typed.
    let doc = studio.doc.as_mut().unwrap();
    doc.toggle_mode();
    doc.set_select_mode(SelectMode::Face);
    let top = top_face(doc);
    doc.selection.click(Some(top), false);
    assert!(studio.begin_modal(ModalKind::Scale, egui::pos2(500.0, 300.0)));
    studio.modal.as_mut().unwrap().0.typed = "2".to_owned();
    studio.update_modal(&screen, egui::pos2(0.0, 0.0), &snap);
    studio.finish_modal();
    let scaled = positions(studio.doc.as_ref().unwrap());
    let max = scaled.iter().fold(Vec3::ZERO, |m, p| m.max(*p));
    assert!(max.abs_diff_eq(Vec3::new(2.0, 1.0, 2.0), 1e-5));
    assert_eq!(studio.doc.as_ref().unwrap().undo_label(), Some("Scale"));

    // A vertex grabbed along Y.
    let doc = studio.doc.as_mut().unwrap();
    doc.set_select_mode(SelectMode::Vertex);
    doc.selection.click(Some(0), false);
    assert!(studio.begin_modal(ModalKind::Grab, egui::pos2(400.0, 300.0)));
    let modal = &mut studio.modal.as_mut().unwrap().0;
    modal.lock = modal.lock.press(1);
    modal.typed = "0.5".to_owned();
    studio.update_modal(&screen, egui::pos2(0.0, 0.0), &snap);
    studio.finish_modal();

    // The whole object turned in Object mode.
    studio.doc.as_mut().unwrap().toggle_mode();
    assert!(studio.begin_modal(ModalKind::Rotate, egui::pos2(500.0, 300.0)));
    let modal = &mut studio.modal.as_mut().unwrap().0;
    modal.lock = modal.lock.press(1);
    modal.typed = "45".to_owned();
    studio.update_modal(&screen, egui::pos2(0.0, 0.0), &snap);
    studio.finish_modal();
    let end = positions(studio.doc.as_ref().unwrap());

    // A cancelled move leaves no trace.
    assert!(studio.begin_modal(ModalKind::Grab, egui::pos2(400.0, 300.0)));
    studio.update_modal(&screen, egui::pos2(100.0, 100.0), &snap);
    studio.cancel_modal();
    assert_eq!(positions(studio.doc.as_ref().unwrap()), end);

    let doc = studio.doc.as_mut().unwrap();
    for label in ["Rotate", "Move", "Scale"] {
        assert_eq!(doc.undo_label(), Some(label));
        assert!(doc.undo());
    }
    assert!(!doc.undo());
    assert_eq!(positions(doc), start);
    assert_eq!(
        doc.mode,
        Mode::Edit,
        "undo puts back the mode the first edit was made in"
    );
    for _ in 0..3 {
        assert!(doc.redo());
    }
    assert!(!doc.redo());
    assert_eq!(positions(doc), end);
}

#[test]
fn a_move_handle_drag_moves_the_picked_face_as_one_undo_step() {
    let mut doc = cube_doc();
    doc.toggle_mode();
    doc.set_select_mode(SelectMode::Face);
    let top = top_face(&doc);
    doc.selection.click(Some(top), false);
    let pose = handle_pose(&doc, MoveAxes::Global).unwrap();
    assert!(pose.position.abs_diff_eq(Vec3::Y, 1e-6));
    assert!(pose.size.abs_diff_eq(Vec3::new(2.0, 0.0, 2.0), 1e-6));

    // The editor's own Move handle, dragged up its Y arrow by 1.5 studs.
    let screen = screen();
    let id = axcenity_core::World::new()
        .create(axcenity_core::ClassName::Part)
        .unwrap();
    let grab = screen.point(pose.position + Vec3::Y * 0.5).unwrap();
    let mut drag = Drag::start_many(
        Handle::Move(crate::gizmo::Axis::Y),
        pose,
        vec![(id, pose)],
        Modes::default(),
        &screen,
        grab,
    )
    .unwrap();
    let to = screen.point(pose.position + Vec3::Y * 2.0).unwrap();
    let free = Snap {
        move_on: false,
        ..Snap::default()
    };
    let values = drag
        .update_all(Pointer::on(&screen, to, false), &free)
        .unwrap();
    let matrix = pose_change(pose, &values[0].1);
    assert!(
        matrix
            .transform_point3(Vec3::Y)
            .abs_diff_eq(Vec3::new(0.0, 2.5, 0.0), 1e-4)
    );

    let moving = Moving::begin(&doc).unwrap();
    moving.update(&mut doc, matrix);
    moving.finish(&mut doc, "Move");
    let max = positions(&doc)
        .iter()
        .fold(Vec3::splat(-9.0), |m, p| m.max(*p));
    assert!((max.y - 2.5).abs() < 1e-4);
    assert!(doc.undo());
    let max = positions(&doc)
        .iter()
        .fold(Vec3::splat(-9.0), |m, p| m.max(*p));
    assert!((max.y - 1.0).abs() < 1e-6);
}

#[test]
fn a_scale_handle_change_scales_from_the_box() {
    let start = Pose {
        position: Vec3::ZERO,
        rotation: Quat::IDENTITY,
        size: Vec3::new(2.0, 0.0, 2.0),
    };
    let values = vec![
        (
            "Size",
            axcenity_core::Value::Vector3(axcenity_core::Vector3::new(4.0, 0.5, 2.0)),
        ),
        (
            "Position",
            axcenity_core::Value::Vector3(axcenity_core::Vector3::new(1.0, 0.0, 0.0)),
        ),
    ];
    let matrix: Mat4 = pose_change(start, &values);
    // X doubled about the moved middle; the flat Y stays flat.
    assert!(
        matrix
            .transform_point3(Vec3::new(1.0, 0.0, 1.0))
            .abs_diff_eq(Vec3::new(3.0, 0.0, 1.0), 1e-5)
    );
    assert!(
        matrix
            .transform_point3(Vec3::new(-1.0, 0.0, 0.0))
            .abs_diff_eq(Vec3::new(-1.0, 0.0, 0.0), 1e-5)
    );
}

#[test]
fn clicks_and_boxes_pick_what_is_under_the_mouse() {
    let doc = cube_doc();
    let mesh = doc.mesh().unwrap();
    let screen = screen();
    // The corner nearest the camera.
    let corner = (0..8u32)
        .find(|&v| mesh.position(v).abs_diff_eq(Vec3::ONE, 1e-6))
        .unwrap();
    let at = screen.point(Vec3::ONE).unwrap();
    assert_eq!(
        nearest_vertex(mesh, &screen, at + egui::vec2(3.0, 2.0)),
        Some(corner)
    );
    let edge_middle = screen.point(Vec3::new(1.0, 1.0, 0.0)).unwrap();
    let edge = nearest_edge(mesh, &screen, edge_middle, 8.0).unwrap();
    let (a, b) = mesh.ends(edge);
    assert_eq!(
        mesh.position(a).midpoint(mesh.position(b)),
        Vec3::new(1.0, 1.0, 0.0)
    );
    let top = screen.point(Vec3::new(0.0, 1.0, 0.0)).unwrap();
    let (face, _) = pick_face(mesh, screen.ray(top)).unwrap();
    assert!(mesh.face_normal(face).y > 0.9);
    // A box round the whole view picks every vertex.
    let all = boxed(
        mesh,
        SelectMode::Vertex,
        &screen,
        egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0)),
    );
    assert_eq!(all.len(), 8);
}

#[test]
fn a_new_shape_with_unsaved_changes_asks_before_it_is_replaced() {
    let mut studio = studio_with(cube_doc());
    assert!(
        studio.doc.as_ref().unwrap().unsaved(),
        "a New shape isn't saved yet"
    );
    let sphere = ModelDoc::primitive(&PrimitiveSettings::of(PrimitiveShape::UvSphere));
    studio.open(StudioDoc::new(
        "assets/models/ball.glb".to_owned(),
        sphere,
        true,
    ));
    assert!(studio.pending.is_some());
    assert_eq!(studio.doc.as_ref().unwrap().model.objects[0].name, "Cube");
}

#[test]
fn the_tab_draws_a_loop_picked_on_a_cylinder_and_the_handles_on_a_face() {
    let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS") else {
        return;
    };
    let folder = std::path::PathBuf::from(folder);
    let mut settings = PrimitiveSettings::of(PrimitiveShape::Cylinder);
    settings.rings = 3;
    let mut doc = StudioDoc::new(String::new(), ModelDoc::primitive(&settings), false);
    doc.toggle_mode();
    doc.set_select_mode(SelectMode::Edge);
    let mesh = doc.mesh().unwrap();
    let ring = mesh
        .edges()
        .into_iter()
        .find(|&e| {
            let (a, b) = mesh.ends(e);
            (mesh.position(a).y + 1.0 / 3.0).abs() < 1e-4
                && (mesh.position(b).y + 1.0 / 3.0).abs() < 1e-4
        })
        .unwrap();
    doc.select(|selection, mesh| selection.select_loop(mesh, ring, false));
    let mut studio = studio_with(doc);
    let size = egui::vec2(900.0, 600.0);
    crate::snapshot::snapshot_rendered(
        &folder.join("model-studio-loop.png"),
        size,
        vec![Vec::new(); 3],
        |ctx, render| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let _ = studio.show(ui, Some(render), &[], Snap::default(), true);
            });
        },
    );

    let mut cube = cube_doc();
    cube.toggle_mode();
    cube.set_select_mode(SelectMode::Face);
    let top = top_face(&cube);
    cube.selection.click(Some(top), false);
    let mut studio = studio_with(cube);
    studio.tool = Tool::Move;
    studio.show_normals = true;
    crate::snapshot::snapshot_rendered(
        &folder.join("model-studio-face.png"),
        size,
        vec![Vec::new(); 3],
        |ctx, render| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let _ = studio.show(ui, Some(render), &[], Snap::default(), true);
            });
        },
    );
}
