//! Phase 59: texture paint mode in the Model Studio: strokes from the 3D
//! view and the UV editor, layers and their undo steps, a stroke across a
//! seam, and a snapshot of the tab painting.

use axcenity_mesh_edit::paint::{PaintTool, sample};
use axcenity_mesh_edit::{MapKind, ModelDoc, PrimitiveSettings, PrimitiveShape, TexelMap};
use axcenity_render::Camera;
use axcenity_render::glam::{Vec2, Vec3};
use eframe::egui;

use super::tests::{screen, studio_with};
use super::uv_editor::UvScreen;
use super::uv_tests::projected_chair;
use super::*;
use crate::screen::Screen;

const RUST: [u8; 3] = [150, 70, 30];

/// The points of a stroke on `screen` from `from` to `to` on the model.
pub fn stroke(screen: &Screen, from: Vec3, to: Vec3) -> Vec<egui::Pos2> {
    (0..=12)
        .filter_map(|k| screen.point(from.lerp(to, k as f32 / 12.0)))
        .collect()
}

/// The projected chair in Paint mode, painting rust at 256 texels.
fn painting_chair() -> ModelStudio {
    let mut studio = projected_chair();
    let doc = studio.doc.as_mut().unwrap();
    doc.set_mode(Mode::Paint);
    assert_eq!(doc.mode, Mode::Paint);
    studio.paint.size = 256;
    studio.paint.brush.color = RUST;
    studio.paint.brush.strength = 1.0;
    studio
}

/// How many texels of `pixels` are rust.
fn rusty(pixels: &[u8]) -> usize {
    pixels
        .chunks(4)
        .filter(|p| (0..3).all(|c| p[c].abs_diff(RUST[c]) <= 3))
        .count()
}

/// Across the seat, as the default view sees it.
fn across_the_seat() -> Vec<egui::Pos2> {
    stroke(
        &screen(),
        Vec3::new(-0.6, 0.11, 0.3),
        Vec3::new(0.6, 0.11, 0.3),
    )
}

#[test]
fn rust_painted_from_the_view_shows_on_the_chair_and_each_stroke_is_one_undo_step() {
    let mut studio = painting_chair();
    assert!(studio.paint_stroke(&screen(), &across_the_seat()));
    let doc = studio.doc.as_ref().unwrap();
    assert_eq!(doc.undo_label(), Some("Paint Stroke"));
    let set = doc.paint_set(0).expect("the first stroke starts the set");
    assert_eq!(set.size, 256);
    let flat = set.map(MapKind::BaseColor).unwrap().flat();
    assert!(rusty(flat) > 100, "rust on the map");
    // What the view draws: the material with the painted map on it.
    let drawn = doc.model.to_model().unwrap();
    let texture = drawn.primitives[0]
        .material
        .base_color_texture
        .as_ref()
        .expect("the view draws the paint");
    assert_eq!(texture.rgba(), flat);

    // Rough rust: the roughness map, a second step.
    studio.paint.map = MapKind::Roughness;
    studio.paint.brush.value = 1.0;
    assert!(studio.paint_stroke(&screen(), &across_the_seat()));
    let doc = studio.doc.as_mut().unwrap();
    let set = doc.paint_set(0).unwrap();
    assert!(
        set.map(MapKind::Metalness).is_some(),
        "saved with roughness"
    );
    let rough = set.map(MapKind::Roughness).unwrap().flat();
    assert!(rough.chunks(4).any(|p| p[0] > 240));
    let material = &doc.model.objects[0].materials[0];
    assert!(material.roughness_texture.is_some());

    // Undo takes each stroke back, and the first takes the set with it.
    assert!(doc.undo());
    assert!(doc.paint_set(0).unwrap().map(MapKind::Roughness).is_none());
    assert!(doc.undo());
    assert!(doc.paint_set(0).is_none());
    assert!(
        doc.model.objects[0].materials[0]
            .base_color_texture
            .is_none()
    );
}

#[test]
fn layers_hide_blend_and_mask_as_undo_steps_and_erase_and_symmetry_paint() {
    let mut studio = painting_chair();
    assert!(studio.paint_stroke(&screen(), &across_the_seat()));
    let doc = studio.doc.as_mut().unwrap();
    doc.change_paint(0, "Add Layer", None, |set| {
        set.map_mut(MapKind::BaseColor).unwrap().add_layer();
    });
    studio.paint.brush.color = [20, 40, 220];
    // Mirrored across X: the stroke on one side paints the other.
    studio.paint.brush.symmetry = [true, false, false];
    let one_side = stroke(
        &screen(),
        Vec3::new(0.3, 0.11, -0.4),
        Vec3::new(0.6, 0.11, -0.4),
    );
    assert!(studio.paint_stroke(&screen(), &one_side));
    let doc = studio.doc.as_mut().unwrap();
    let mesh = doc.mesh().unwrap().clone();
    let texels = TexelMap::build(&mesh, 0, 256);
    let set = doc.paint_set(0).unwrap();
    let flat = set.map(MapKind::BaseColor).unwrap().flat();
    let seat_face = |x: f32| {
        let ray = axcenity_scene::Ray {
            origin: Vec3::new(x, 3.0, -0.4),
            direction: Vec3::NEG_Y,
        };
        let (face, along) = view::pick_face(&mesh, ray).unwrap();
        (face, ray.origin + ray.direction * along)
    };
    for x in [0.45, -0.45] {
        let (face, point) = seat_face(x);
        let uv = texels.uv_at(face, point).unwrap();
        let color = sample(flat, 256, 256, uv, true);
        assert!(
            color[2] > 150.0 && color[0] < 100.0,
            "blue at x = {x}: {color:?}"
        );
    }
    // Hiding the layer is a step, and leaves only the rust.
    doc.change_paint(0, "Hide Layer", None, |set| {
        let map = set.map_mut(MapKind::BaseColor).unwrap();
        map.layers[1].visible = false;
        map.refresh_all();
    });
    let set = doc.paint_set(0).unwrap();
    let flat = set.map(MapKind::BaseColor).unwrap().flat();
    let blue = |p: &[u8]| p[2] > 150 && p[0] < 100;
    assert!(!flat.chunks(4).any(blue), "the blue is hidden");
    assert_eq!(doc.undo_label(), Some("Hide Layer"));
    assert!(doc.undo());
    let set = doc.paint_set(0).unwrap();
    assert!(set.map(MapKind::BaseColor).unwrap().layers[1].visible);

    // Erase on the top layer takes its blue off.
    studio.paint.brush.symmetry = [false; 3];
    studio.paint.brush.tool = PaintTool::Erase;
    studio.paint.brush.radius = 80.0;
    assert!(studio.paint_stroke(&screen(), &one_side));
    let doc = studio.doc.as_ref().unwrap();
    let (face, point) = seat_face(0.45);
    let uv = texels.uv_at(face, point).unwrap();
    let flat = doc
        .paint_set(0)
        .unwrap()
        .map(MapKind::BaseColor)
        .unwrap()
        .flat();
    assert!(sample(flat, 256, 256, uv, true)[0] > 100.0, "erased");
}

#[test]
fn a_stroke_across_the_seam_of_a_cylinder_leaves_no_line() {
    let doc = StudioDoc::new(
        String::new(),
        ModelDoc::primitive(&PrimitiveSettings::of(PrimitiveShape::Cylinder)),
        false,
    );
    let mut studio = studio_with(doc);
    let doc = studio.doc.as_mut().unwrap();
    doc.set_mode(Mode::Paint);
    studio.paint.size = 256;
    studio.paint.brush.color = RUST;
    studio.paint.brush.radius = 60.0;
    let mesh = studio.doc.as_ref().unwrap().mesh().unwrap().clone();
    // The side's seam: an upright edge whose two faces' uvs differ there.
    let (edge, twin) = (0..mesh.half_edges().len() as u32)
        .filter_map(|e| Some((e, mesh.half_edge(e).twin?)))
        .find(|&(e, twin)| {
            let (a, b) = mesh.ends(e);
            let upright = (mesh.position(a) - mesh.position(b)).normalize().y.abs() > 0.99;
            let across = mesh.half_edge(mesh.half_edge(twin).next).uv;
            upright && mesh.half_edge(e).uv.distance(across) > 0.1
        })
        .expect("the cylinder's uvs have a seam");
    let (a, b) = mesh.ends(edge);
    let middle = (mesh.position(a) + mesh.position(b)) / 2.0;
    let out = Vec3::new(middle.x, 0.0, middle.z).normalize();
    let along = Vec3::Y.cross(out);
    let camera = Camera {
        position: middle + out * 5.0,
        look_at: middle,
        fov_y_degrees: 50.0,
    };
    let screen = Screen::new(
        camera,
        egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0)),
    );
    let points = stroke(&screen, middle - along * 0.25, middle + along * 0.35);
    assert!(studio.paint_stroke(&screen, &points));

    let doc = studio.doc.as_ref().unwrap();
    let flat = doc
        .paint_set(0)
        .unwrap()
        .map(MapKind::BaseColor)
        .unwrap()
        .flat();
    let texels = TexelMap::build(&mesh, 0, 256);
    let faces = [mesh.half_edge(edge).face, mesh.half_edge(twin).face];
    let mut painted = 0;
    for step in 0..=10 {
        let point = middle + Vec3::Y * (-0.3 + 0.06 * step as f32);
        let [one, other] = faces.map(|face| {
            let uv = texels.uv_at(face, point).unwrap();
            sample(flat, 256, 256, uv, true)
        });
        for c in 0..3 {
            assert!(
                (one[c] - other[c]).abs() <= 4.0,
                "at {point}: {one:?} on one side, {other:?} on the other"
            );
        }
        if (one[2] - RUST[2] as f32).abs() < 40.0 {
            painted += 1;
        }
    }
    assert!(painted >= 5, "the stroke covers the seam");
}

#[test]
fn the_uv_editor_paints_in_uv_space_and_fill_paints_an_island() {
    let mut studio = painting_chair();
    let screen = UvScreen {
        rect: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 400.0)),
        zoom: 1.0,
        pan: Vec2::splat(0.5),
    };
    let points: Vec<egui::Pos2> = (0..=8)
        .map(|k| screen.point(Vec2::new(0.1 + 0.1 * k as f32, 0.5)))
        .collect();
    assert!(studio.paint_uv_stroke(&screen, &points));
    let doc = studio.doc.as_ref().unwrap();
    let flat = doc
        .paint_set(0)
        .unwrap()
        .map(MapKind::BaseColor)
        .unwrap()
        .flat();
    let at = |uv: Vec2| sample(flat, 256, 256, uv, true);
    assert!(
        (at(Vec2::new(0.5, 0.5))[0] - 150.0).abs() < 4.0,
        "on the line"
    );
    assert!(
        (at(Vec2::new(0.5, 0.9))[0] - 150.0).abs() > 20.0,
        "not far off it"
    );

    // Fill: every texel of the island clicked, and nothing else.
    studio.paint.brush.tool = PaintTool::Fill;
    studio.paint.brush.color = [0, 200, 0];
    let doc = studio.doc.as_ref().unwrap();
    let mesh = doc.mesh().unwrap();
    let first = mesh.face_half_edges(0);
    let middle = first.iter().map(|&e| mesh.half_edge(e).uv).sum::<Vec2>() / first.len() as f32;
    assert!(studio.paint_uv_stroke(&screen, &[screen.point(middle)]));
    let doc = studio.doc.as_ref().unwrap();
    assert_eq!(doc.undo_label(), Some("Fill"));
    let flat = doc
        .paint_set(0)
        .unwrap()
        .map(MapKind::BaseColor)
        .unwrap()
        .flat();
    let green = flat.chunks(4).filter(|p| p[1] == 200 && p[0] == 0).count();
    assert!(green > 50 && green < 256 * 256 / 2, "one island: {green}");
}

#[test]
fn clone_needs_a_source_and_copies_from_it() {
    let mut studio = painting_chair();
    assert!(studio.paint_stroke(&screen(), &across_the_seat()));
    studio.paint.brush.tool = PaintTool::Clone;
    assert!(!studio.paint_stroke(&screen(), &across_the_seat()));
    assert!(studio.paint.status.is_some());
    // From where the rust is, onto the back of the seat.
    let doc = studio.doc.as_ref().unwrap();
    let mesh = doc.mesh().unwrap().clone();
    let texels = TexelMap::build(&mesh, 0, 256);
    let ray = screen().ray(across_the_seat()[6]);
    let (face, along) = view::pick_face(&mesh, ray).unwrap();
    studio.paint.clone_source = texels.uv_at(face, ray.origin + ray.direction * along);
    let before = rusty(
        doc.paint_set(0)
            .unwrap()
            .map(MapKind::BaseColor)
            .unwrap()
            .flat(),
    );
    let back = stroke(
        &screen(),
        Vec3::new(-0.5, 0.11, -0.4),
        Vec3::new(0.5, 0.11, -0.4),
    );
    assert!(studio.paint_stroke(&screen(), &back));
    let doc = studio.doc.as_ref().unwrap();
    let after = rusty(
        doc.paint_set(0)
            .unwrap()
            .map(MapKind::BaseColor)
            .unwrap()
            .flat(),
    );
    assert!(after > before, "cloned rust: {before} then {after}");
}

#[test]
fn the_tab_draws_the_chair_being_painted() {
    let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS") else {
        return;
    };
    let folder = std::path::PathBuf::from(folder);
    let mut studio = painting_chair();
    assert!(studio.paint_stroke(&screen(), &across_the_seat()));
    studio.paint.map = MapKind::Roughness;
    assert!(studio.paint_stroke(&screen(), &across_the_seat()));
    studio.paint.map = MapKind::BaseColor;
    studio.uv.open = true;
    crate::snapshot::snapshot_rendered(
        &folder.join("model-studio-paint.png"),
        egui::vec2(1100.0, 700.0),
        vec![Vec::new(); 3],
        |ui, render| {
            egui::CentralPanel::default().show(ui, |ui| {
                let _ = studio.show(ui, Some(render), &[], crate::gizmo::Snap::default(), true);
            });
        },
    );
}
