//! Phase 56: modeling operations in the Model Studio: the adjust panel,
//! values following the mouse, the loop cut and knife tools, proportional
//! editing and pivots.

use axcenity_mesh_edit::{MeshOp, ModelDoc, PrimitiveSettings, PrimitiveShape, SelectMode};
use axcenity_render::glam::Vec3;
use eframe::egui;

use super::tests::{cube_doc, screen, studio_with};
use super::view::{Event, ViewAction, knife_point};
use super::*;

/// An Edit-mode cube with the edges round its top picked.
fn top_loop_doc() -> StudioDoc {
    let mut doc = cube_doc();
    doc.toggle_mode();
    doc.set_select_mode(SelectMode::Edge);
    doc.select(|selection, mesh| {
        selection.edges = mesh
            .edges()
            .into_iter()
            .filter(|&e| {
                let (a, b) = mesh.ends(e);
                mesh.position(a).y > 0.9 && mesh.position(b).y > 0.9
            })
            .collect();
    });
    doc
}

/// What the view reports with the mouse at `pointer` (and a click there,
/// if `clicked`).
fn event(pointer: egui::Pos2, clicked: bool) -> Event {
    Event {
        screen: screen(),
        clicked,
        secondary_clicked: false,
        click_at: clicked.then_some(pointer),
        pointer: Some(pointer),
        action: ViewAction::None,
    }
}

fn faces(studio: &ModelStudio) -> usize {
    studio.doc.as_ref().unwrap().mesh().unwrap().face_count()
}

#[test]
fn changing_the_bevels_segments_in_the_adjust_panel_reruns_it_without_another_undo_step() {
    let mut doc = top_loop_doc();
    let bevel = |segments| MeshOp::Bevel {
        width: 0.2,
        segments,
        profile: 0.5,
        vertices: false,
    };
    doc.run(bevel(1)).unwrap();
    assert_eq!(doc.undo.len(), 1);
    assert_eq!(doc.undo_label(), Some("Bevel"));
    assert_eq!(doc.mesh().unwrap().face_count(), 6 + 4);

    // The panel's Segments changed to 3: the bevel runs again from the
    // cube, in place of the first one.
    doc.adjust(bevel(3)).unwrap();
    assert_eq!(doc.undo.len(), 1, "no second undo step");
    assert_eq!(doc.mesh().unwrap().face_count(), 6 + 4 * 3);
    assert_eq!(doc.last_op.as_ref().unwrap().op, bevel(3));
    axcenity_mesh_edit::check_manifold(doc.mesh().unwrap(), true).unwrap();
    // The strips are still what's picked.
    assert_eq!(doc.selection.mode, SelectMode::Edge);
    assert!(!doc.selection.edges.is_empty());

    // One undo goes back to the cube; redo brings the 3-segment bevel.
    assert!(doc.undo());
    assert_eq!(doc.mesh().unwrap().face_count(), 6);
    assert!(
        doc.last_op.is_none(),
        "nothing left to adjust after an undo"
    );
    assert!(doc.redo());
    assert_eq!(doc.mesh().unwrap().face_count(), 6 + 4 * 3);
}

#[test]
fn the_adjust_panel_draws_the_last_operations_settings() {
    let mut doc = top_loop_doc();
    doc.run(MeshOp::Bevel {
        width: 0.2,
        segments: 2,
        profile: 0.5,
        vertices: false,
    })
    .unwrap();
    let mut studio = studio_with(doc);
    let ctx = crate::theme::test_context();
    let mut drawn = false;
    let _ = crate::theme::run(&ctx, egui::RawInput::default(), |ui| {
        egui::CentralPanel::default().show(ui, |ui| {
            let _ = studio.show(ui, None, &[], Snap::default(), true);
            drawn = true;
        });
    });
    assert!(drawn);
    // A setting changed through the panel's own code path.
    let mut op = studio
        .doc
        .as_ref()
        .unwrap()
        .last_op
        .as_ref()
        .unwrap()
        .op
        .clone();
    let _ = crate::theme::run(&ctx, egui::RawInput::default(), |ui| {
        egui::CentralPanel::default().show(ui, |ui| {
            tools::settings_ui(ui, &mut op);
        });
    });
    if let MeshOp::Bevel { segments, .. } = &mut op {
        *segments = 4;
    }
    assert!(studio.adjust_op(op));
    assert_eq!(faces(&studio), 6 + 4 * 4);
    assert_eq!(studio.doc.as_ref().unwrap().undo.len(), 1);
}

#[test]
fn a_bevel_follows_the_mouse_until_a_click_and_a_right_click_takes_it_back() {
    let mut studio = studio_with(top_loop_doc());
    let screen = screen();
    let middle = screen.point(Vec3::Y).unwrap();
    let start = middle + egui::vec2(40.0, 0.0);
    studio.begin_value(
        MeshOp::Bevel {
            width: 0.0,
            segments: 1,
            profile: 0.5,
            vertices: false,
        },
        tools::Value::Width,
        start,
    );
    assert!(studio.is_dragging());
    // Farther from the middle: wider.
    assert!(studio.handle_tools(&event(start + egui::vec2(60.0, 0.0), false)));
    let width = |studio: &ModelStudio| match studio.doc.as_ref().unwrap().last_op.as_ref() {
        Some(LastOp {
            op: MeshOp::Bevel { width, .. },
            ..
        }) => *width,
        _ => panic!("a bevel"),
    };
    let wide = width(&studio);
    assert!(wide > 0.05, "{wide}");
    assert_eq!(studio.doc.as_ref().unwrap().undo.len(), 1);
    // Typing a number sets it exactly.
    studio.tools.drag.as_mut().unwrap().typed = "0.3".to_owned();
    studio.handle_tools(&event(start, true));
    assert!((width(&studio) - 0.3).abs() < 1e-6);
    assert!(!studio.is_dragging(), "the click keeps it");
    assert_eq!(faces(&studio), 10);

    // Another, cancelled: the cube's bevel stays and nothing else.
    studio.begin_value(
        MeshOp::Inset {
            thickness: 0.0,
            depth: 0.0,
            individual: false,
        },
        tools::Value::Thickness,
        start,
    );
    let undo_steps = studio.doc.as_ref().unwrap().undo.len();
    let mut cancel = event(start, false);
    cancel.secondary_clicked = true;
    studio.handle_tools(&cancel);
    assert!(!studio.is_dragging());
    assert_eq!(studio.doc.as_ref().unwrap().undo.len(), undo_steps - 1);
    assert_eq!(faces(&studio), 10);
    assert!(studio.doc.as_ref().unwrap().redo_label().is_none());
}

#[test]
fn an_extrude_follows_the_mouse_along_the_normal() {
    let mut doc = cube_doc();
    doc.toggle_mode();
    doc.set_select_mode(SelectMode::Face);
    let top = (0..6)
        .find(|&f| doc.mesh().unwrap().face_normal(f).y > 0.9)
        .unwrap();
    doc.selection.click(Some(top), false);
    let mut studio = studio_with(doc);
    let screen = screen();
    let at = screen.point(Vec3::Y).unwrap();
    studio.begin_value(
        MeshOp::Extrude {
            mode: axcenity_mesh_edit::ExtrudeMode::Region,
            distance: 0.0,
        },
        tools::Value::Distance,
        at,
    );
    // Up the screen to where a point a stud and a half higher shows.
    let to = screen.point(Vec3::Y * 2.5).unwrap();
    studio.handle_tools(&event(to, true));
    let doc = studio.doc.as_ref().unwrap();
    let mesh = doc.mesh().unwrap();
    let top = mesh
        .positions()
        .iter()
        .map(|p| p.y)
        .fold(f32::MIN, f32::max);
    assert!((top - 2.5).abs() < 1e-3, "{top}");
    assert_eq!(doc.undo_label(), Some("Extrude"));
    assert_eq!(doc.undo.len(), 1);
}

#[test]
fn a_loop_cut_shows_its_loop_under_the_mouse_and_a_click_cuts_and_slides_it() {
    let mut doc = cube_doc();
    doc.toggle_mode();
    let mut studio = studio_with(doc);
    studio.tools.loop_cut = Some(tools::LoopCutChoice::default());
    let screen = screen();
    // Over the middle of the top's front edge.
    let over = screen.point(Vec3::new(0.0, 1.0, 1.0)).unwrap();
    studio.handle_tools(&event(over, false));
    let preview = studio.extras();
    assert_eq!(preview.lines.len(), 4, "a loop round the cube");
    assert_eq!(faces(&studio), 6, "only shown so far");
    studio.handle_tools(&event(over, true));
    assert!(studio.is_dragging(), "now sliding");
    assert_eq!(faces(&studio), 10);
    studio.handle_tools(&event(over + egui::vec2(50.0, 0.0), true));
    assert!(!studio.is_dragging());
    let doc = studio.doc.as_ref().unwrap();
    assert_eq!(doc.undo_label(), Some("Loop Cut"));
    match &doc.last_op.as_ref().unwrap().op {
        MeshOp::LoopCut { slide, .. } => assert!((slide - 0.25).abs() < 1e-5),
        op => panic!("{op:?}"),
    }
}

#[test]
fn the_knife_cuts_through_the_points_clicked() {
    let mut doc = cube_doc();
    doc.toggle_mode();
    let mut studio = studio_with(doc);
    studio.tools.knife = Some(Vec::new());
    let screen = screen();
    for at in [Vec3::new(0.0, 1.0, 1.0), Vec3::new(1.0, 1.0, 0.0)] {
        studio.handle_tools(&event(screen.point(at).unwrap(), true));
    }
    let points = studio.tools.knife.take().unwrap();
    assert_eq!(points.len(), 2);
    assert!(points[0].1.abs_diff_eq(Vec3::new(0.0, 1.0, 1.0), 0.02));
    let mesh = studio.doc.as_ref().unwrap().mesh().unwrap();
    let corner = screen.point(Vec3::ONE).unwrap();
    let (on_corner, _) = knife_point(mesh, &screen, corner).unwrap();
    assert_eq!(on_corner.a, on_corner.b, "a corner snaps to its vertex");
    assert!(studio.run_op(MeshOp::Knife {
        points: points.into_iter().map(|(p, _)| p).collect(),
    }));
    // The top corner cut off: one more face, two more vertices.
    let mesh = studio.doc.as_ref().unwrap().mesh().unwrap();
    assert_eq!((mesh.face_count(), mesh.vertex_count()), (7, 10));
    axcenity_mesh_edit::check_manifold(mesh, true).unwrap();
}

#[test]
fn proportional_editing_moves_what_is_near_part_of_the_way() {
    let mut settings = PrimitiveSettings::of(PrimitiveShape::Plane);
    settings.segments = 4;
    let mut doc = StudioDoc::new(String::new(), ModelDoc::primitive(&settings), false);
    doc.toggle_mode();
    let middle = (0..doc.mesh().unwrap().vertex_count() as u32)
        .find(|&v| doc.mesh().unwrap().position(v).length() < 1e-5)
        .unwrap();
    doc.selection.vertices.insert(middle);
    let mut studio = studio_with(doc);
    studio.moves.proportional = Proportional {
        on: true,
        falloff: Falloff::Linear,
        radius: 1.0,
    };
    let screen = screen();
    assert!(studio.begin_modal(ModalKind::Grab, egui::Pos2::ZERO));
    let (modal, _) = studio.modal.as_mut().unwrap();
    modal.lock = modal::AxisLock::Global(1);
    modal.typed = "1".to_owned();
    studio.update_modal(&screen, egui::Pos2::ZERO, &Snap::default());
    assert!(studio.extras().circle.is_some(), "the radius shows");
    studio.finish_modal();
    let mesh = studio.doc.as_ref().unwrap().mesh().unwrap();
    let height = |x: f32, z: f32| {
        let v = (0..mesh.vertex_count() as u32)
            .find(|&v| {
                let p = mesh.position(v);
                (p.x - x).abs() < 1e-4 && (p.z - z).abs() < 1e-4
            })
            .unwrap();
        mesh.position(v).y
    };
    assert!((height(0.0, 0.0) - 1.0).abs() < 1e-5);
    // Half a stud out, linear falloff over one: half the way.
    assert!((height(0.5, 0.0) - 0.5).abs() < 1e-5);
    assert!(height(1.0, 0.0).abs() < 1e-6, "the radius's edge stays");
    assert_eq!(studio.doc.as_ref().unwrap().undo.len(), 1);
}

#[test]
fn turns_go_round_the_chosen_pivot() {
    let mut doc = cube_doc();
    doc.toggle_mode();
    doc.set_select_mode(SelectMode::Face);
    let top = (0..6)
        .find(|&f| doc.mesh().unwrap().face_normal(f).y > 0.9)
        .unwrap();
    doc.selection.click(Some(top), false);
    let mut studio = studio_with(doc);
    studio.moves.pivot = Pivot::Cursor;
    studio.moves.cursor = Vec3::new(5.0, 1.0, 0.0);
    let screen = screen();
    assert!(studio.begin_modal(ModalKind::Scale, egui::Pos2::ZERO));
    studio.modal.as_mut().unwrap().0.typed = "0".to_owned();
    studio.update_modal(&screen, egui::Pos2::ZERO, &Snap::default());
    studio.finish_modal();
    // Scaled to nothing round the cursor: the top's corners meet there.
    let mesh = studio.doc.as_ref().unwrap().mesh().unwrap();
    let corners = mesh.face_vertices(top);
    assert!(
        corners
            .iter()
            .all(|&v| mesh.position(v).abs_diff_eq(Vec3::new(5.0, 1.0, 0.0), 1e-5))
    );

    // Individual origins: two opposite side faces each shrink in place.
    let mut doc = cube_doc();
    doc.toggle_mode();
    doc.set_select_mode(SelectMode::Face);
    let sides: Vec<u32> = (0..6)
        .filter(|&f| doc.mesh().unwrap().face_normal(f).x.abs() > 0.9)
        .collect();
    doc.selection.faces.extend(&sides);
    let mut studio = studio_with(doc);
    studio.moves.pivot = Pivot::Individual;
    assert!(studio.begin_modal(ModalKind::Scale, egui::Pos2::ZERO));
    studio.modal.as_mut().unwrap().0.typed = "0.5".to_owned();
    studio.update_modal(&screen, egui::Pos2::ZERO, &Snap::default());
    studio.finish_modal();
    let mesh = studio.doc.as_ref().unwrap().mesh().unwrap();
    for face in sides {
        let center = mesh.face_center(face);
        assert!((center.x.abs() - 1.0).abs() < 1e-5, "stays on its side");
        for v in mesh.face_vertices(face) {
            assert!((mesh.position(v).y.abs() - 0.5).abs() < 1e-5);
        }
    }
}

#[test]
fn shading_in_object_mode_covers_whole_objects() {
    let mut studio = studio_with(cube_doc());
    assert!(studio.run_op(MeshOp::AutoSmooth { degrees: 30.0 }));
    let doc = studio.doc.as_ref().unwrap();
    let mesh = doc.mesh().unwrap();
    assert!((0..6).all(|f| mesh.face(f).smooth));
    assert!(mesh.auto_smooth().is_some());
    // A cube's 90° edges stay sharp.
    for edge in mesh.half_edges() {
        assert!(edge.normal.abs_diff_eq(mesh.face_normal(edge.face), 1e-5));
    }
    // Operations other than shading need Edit mode's picks.
    assert!(!studio.run_op(MeshOp::Subdivide { cuts: 1 }));
    assert!(studio.tools.status.is_some());
}

/// The faces of `doc`'s mesh facing `normal` whose middles pass `keep`.
fn pick_faces(doc: &mut StudioDoc, normal: Vec3, keep: impl Fn(Vec3) -> bool) {
    doc.set_select_mode(SelectMode::Face);
    doc.select(|selection, mesh| {
        selection.faces = (0..mesh.face_count() as u32)
            .filter(|&f| mesh.face_normal(f).dot(normal) > 0.99 && keep(mesh.face_center(f)))
            .collect();
    });
}

/// The edge of `doc`'s mesh from `a` to `b`, as its two vertices.
fn edge_between(doc: &StudioDoc, a: Vec3, b: Vec3) -> [u32; 2] {
    let mesh = doc.mesh().unwrap();
    let at = |p: Vec3| {
        (0..mesh.vertex_count() as u32)
            .find(|&v| mesh.position(v).abs_diff_eq(p, 1e-4))
            .unwrap_or_else(|| panic!("no vertex at {p}"))
    };
    [at(a), at(b)]
}

/// A chair modeled from the Studio's New cube in Edit mode: flattened
/// into a seat, cut into a 3×3 grid with loop cuts (the inner loops moved
/// out so the corners are thin), four legs extruded down from the corners
/// underneath, a backrest extruded up from the back row, the seat inset
/// into a cushion, and the backrest's top beveled round.
pub fn model_chair(doc: &mut StudioDoc) {
    let scale = |doc: &mut StudioDoc, keep: &dyn Fn(Vec3) -> bool, by: Vec3| {
        doc.set_select_mode(SelectMode::Vertex);
        doc.select(|selection, mesh| {
            selection.vertices = (0..mesh.vertex_count() as u32)
                .filter(|&v| keep(mesh.position(v)))
                .collect();
        });
        assert!(doc.transform("Scale", axcenity_render::glam::Mat4::from_scale(by)));
    };
    doc.toggle_mode();
    assert_eq!(doc.mode, Mode::Edit);
    scale(doc, &|_| true, Vec3::new(1.0, 0.15, 1.0));
    let along_x = edge_between(doc, Vec3::new(-1.0, 0.15, 1.0), Vec3::new(1.0, 0.15, 1.0));
    doc.run(MeshOp::LoopCut {
        edge: along_x,
        cuts: 2,
        slide: 0.0,
    })
    .unwrap();
    let along_z = edge_between(doc, Vec3::new(1.0, 0.15, -1.0), Vec3::new(1.0, 0.15, 1.0));
    doc.run(MeshOp::LoopCut {
        edge: along_z,
        cuts: 2,
        slide: 0.0,
    })
    .unwrap();
    scale(doc, &|p| p.x.abs() < 0.5, Vec3::new(2.4, 1.0, 1.0));
    scale(doc, &|p| p.z.abs() < 0.5, Vec3::new(1.0, 1.0, 2.4));

    pick_faces(doc, Vec3::NEG_Y, |c| c.x.abs() > 0.85 && c.z.abs() > 0.85);
    assert_eq!(doc.selection.faces.len(), 4);
    doc.run(MeshOp::Extrude {
        mode: axcenity_mesh_edit::ExtrudeMode::Individual,
        distance: 1.6,
    })
    .unwrap();

    pick_faces(doc, Vec3::Y, |c| c.z < -0.85);
    assert_eq!(doc.selection.faces.len(), 3);
    doc.run(MeshOp::Extrude {
        mode: axcenity_mesh_edit::ExtrudeMode::Region,
        distance: 1.8,
    })
    .unwrap();

    pick_faces(doc, Vec3::Y, |c| c.y < 0.2);
    assert_eq!(doc.selection.faces.len(), 6);
    doc.run(MeshOp::Inset {
        thickness: 0.08,
        depth: -0.04,
        individual: false,
    })
    .unwrap();

    doc.set_select_mode(SelectMode::Edge);
    doc.select(|selection, mesh| {
        selection.edges = mesh
            .edges()
            .into_iter()
            .filter(|&e| {
                let (a, b) = mesh.ends(e);
                mesh.position(a).y > 1.9 && mesh.position(b).y > 1.9
            })
            .collect();
    });
    doc.run(MeshOp::Bevel {
        width: 0.06,
        segments: 3,
        profile: 0.5,
        vertices: false,
    })
    .unwrap();
}

#[test]
fn a_chair_is_modeled_from_one_cube() {
    let mut doc = cube_doc();
    model_chair(&mut doc);
    let mesh = doc.mesh().unwrap();
    axcenity_mesh_edit::check_manifold(mesh, true).unwrap();
    let euler = mesh.vertex_count() as i64 - mesh.edges().len() as i64 + mesh.face_count() as i64;
    assert_eq!(euler, 2, "one closed piece");
    let (min, max) = mesh
        .positions()
        .iter()
        .fold((Vec3::INFINITY, Vec3::NEG_INFINITY), |(min, max), &p| {
            (min.min(p), max.max(p))
        });
    assert!(min.abs_diff_eq(Vec3::new(-1.0, -1.75, -1.0), 1e-4), "{min}");
    assert!(max.abs_diff_eq(Vec3::new(1.0, 1.95, 1.0), 1e-4), "{max}");
    // Each step is one undo step: the flattening, two loop cuts, two
    // scales, the legs, the backrest, the inset and the bevel.
    assert_eq!(doc.undo.len(), 9);
    while doc.undo() {}
    assert_eq!(doc.mesh().unwrap().face_count(), 6);
}

#[test]
fn the_tab_draws_a_chair() {
    let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS") else {
        return;
    };
    let folder = std::path::PathBuf::from(folder);
    let mut doc = cube_doc();
    model_chair(&mut doc);
    doc.set_select_mode(SelectMode::Face);
    doc.select(|selection, _| selection.faces.clear());
    let mut studio = studio_with(doc);
    crate::snapshot::snapshot_rendered(
        &folder.join("model-studio-chair.png"),
        egui::vec2(900.0, 600.0),
        vec![Vec::new(); 3],
        |ui, render| {
            egui::CentralPanel::default().show(ui, |ui| {
                let _ = studio.show(ui, Some(render), &[], Snap::default(), true);
            });
        },
    );
}
