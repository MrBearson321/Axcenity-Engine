//! Phase 60: Sculpt mode in the Model Studio: a face sculpted from a
//! voxel-remeshed sphere with X symmetry, strokes as undo steps, masks,
//! only touched chunks sent again, and a snapshot of the tab sculpting.

use axcenity_mesh_edit::sculpt::SculptTool;
use axcenity_mesh_edit::{ModelDoc, PrimitiveSettings, PrimitiveShape, check_manifold};
use axcenity_render::Camera;
use axcenity_render::glam::Vec3;
use eframe::egui;

use super::tests::studio_with;
use super::*;
use crate::screen::Screen;

/// Looking at the sphere's front (+Z) from 5 studs away.
pub(super) fn front() -> Screen {
    let camera = Camera {
        position: Vec3::new(0.0, 0.0, 5.0),
        look_at: Vec3::ZERO,
        fov_y_degrees: 50.0,
    };
    Screen::new(
        camera,
        egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0)),
    )
}

/// The points of a stroke on `screen` along the sphere's surface, from
/// `from` to `to` (directions from its middle).
pub(super) fn stroke(screen: &Screen, from: Vec3, to: Vec3) -> Vec<egui::Pos2> {
    (0..=16)
        .filter_map(|k| screen.point(from.lerp(to, k as f32 / 16.0).normalize()))
        .collect()
}

/// A New UV sphere in Sculpt mode, voxel remeshed.
pub(super) fn remeshed_sphere() -> ModelStudio {
    let doc = StudioDoc::new(
        String::new(),
        ModelDoc::primitive(&PrimitiveSettings::of(PrimitiveShape::UvSphere)),
        false,
    );
    let mut studio = studio_with(doc);
    let doc = studio.doc.as_mut().unwrap();
    doc.set_mode(Mode::Sculpt);
    assert_eq!(doc.mode, Mode::Sculpt);
    studio.sculpt.voxel_size = 0.06;
    studio.sculpt_detail(true);
    let doc = studio.doc.as_ref().unwrap();
    assert_eq!(doc.undo_label(), Some("Voxel Remesh"));
    check_manifold(doc.mesh().unwrap(), true).unwrap();
    studio.sculpt.brush.radius = 25.0;
    studio.sculpt.brush.strength = 0.6;
    studio
}

/// For each vertex, the one at its mirror image across X = 0.
fn mirror_pairs(positions: &[Vec3]) -> Vec<usize> {
    positions
        .iter()
        .map(|p| {
            let mirrored = Vec3::new(-p.x, p.y, p.z);
            let (best, distance) = positions
                .iter()
                .enumerate()
                .map(|(k, q)| (k, q.distance(mirrored)))
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .unwrap();
            assert!(distance < 1e-4, "{p} has no mirror image");
            best
        })
        .collect()
}

/// The furthest any vertex is from its mirror image's mirror image.
fn asymmetry(positions: &[Vec3], pairs: &[usize]) -> f32 {
    positions
        .iter()
        .zip(pairs)
        .map(|(p, &other)| {
            let q = positions[other];
            p.distance(Vec3::new(-q.x, q.y, q.z))
        })
        .fold(0.0, f32::max)
}

fn positions(studio: &ModelStudio) -> Vec<Vec3> {
    studio
        .doc
        .as_ref()
        .unwrap()
        .mesh()
        .unwrap()
        .positions()
        .to_vec()
}

/// Each drawn chunk mesh's key and version.
fn drawn(studio: &mut ModelStudio) -> Vec<(u64, u64)> {
    studio
        .sculpt_items()
        .expect("sculpting")
        .iter()
        .map(|item| match &item.shape {
            axcenity_render::Shape::Mesh(mesh) => (mesh.key(), mesh.version()),
            _ => panic!("chunks are meshes"),
        })
        .collect()
}

#[test]
fn a_face_sculpted_in_the_studio_from_a_voxel_remeshed_sphere_with_x_symmetry_matches_on_both_sides()
 {
    let mut studio = remeshed_sphere();
    assert_eq!(studio.sculpt.brush.symmetry, [true, false, false]);
    let pairs = mirror_pairs(&positions(&studio));
    let start = positions(&studio);
    let screen = front();
    // Brows and a nose (Draw), cheeks (Clay Strips), the brows softened
    // (Smooth) and the mouth (Crease), all drawn on the right side only.
    let strokes = [
        (
            SculptTool::Draw,
            Vec3::new(0.08, 0.35, 0.94),
            Vec3::new(0.5, 0.35, 0.8),
        ),
        (
            SculptTool::Draw,
            Vec3::new(0.0, 0.2, 1.0),
            Vec3::new(0.0, -0.15, 1.0),
        ),
        (
            SculptTool::ClayStrips,
            Vec3::new(0.3, -0.05, 0.95),
            Vec3::new(0.55, -0.2, 0.8),
        ),
        (
            SculptTool::Smooth,
            Vec3::new(0.08, 0.35, 0.94),
            Vec3::new(0.5, 0.35, 0.8),
        ),
        (
            SculptTool::Crease,
            Vec3::new(0.0, -0.45, 0.89),
            Vec3::new(0.35, -0.42, 0.83),
        ),
    ];
    let mut steps = 0;
    for (tool, from, to) in strokes {
        studio.sculpt.brush.tool = tool;
        let before = positions(&studio);
        assert!(studio.sculpt_stroke(&screen, &stroke(&screen, from, to), false));
        steps += 1;
        let doc = studio.doc.as_ref().unwrap();
        assert_eq!(doc.undo_label(), Some("Sculpt Stroke"), "{}", tool.label());
        let now = positions(&studio);
        assert_ne!(now, before, "{} sculpted the mesh", tool.label());
        let off = asymmetry(&now, &pairs);
        assert!(off < 1e-4, "{} left the sides {off} apart", tool.label());
    }
    let moved = positions(&studio)
        .iter()
        .zip(&start)
        .map(|(p, q)| p.distance(*q))
        .fold(0.0, f32::max);
    assert!(moved > 0.03, "a face: moved {moved}");
    check_manifold(studio.doc.as_ref().unwrap().mesh().unwrap(), true).unwrap();

    // Each stroke is its own undo step, and the sculpt follows the undo.
    let sculpted = positions(&studio);
    let doc = studio.doc.as_mut().unwrap();
    for _ in 0..steps {
        assert!(doc.undo());
    }
    assert_eq!(doc.undo_label(), Some("Voxel Remesh"));
    assert_eq!(positions(&studio), start);
    let sculpt = studio.sculpt_of().unwrap();
    assert_eq!(
        sculpt.positions(),
        start.as_slice(),
        "the sculpt is undone too"
    );
    let doc = studio.doc.as_mut().unwrap();
    for _ in 0..steps {
        assert!(doc.redo());
    }
    assert_eq!(positions(&studio), sculpted);
}

#[test]
fn masked_parts_dont_move_and_masking_isnt_an_undo_step() {
    let mut studio = remeshed_sphere();
    let screen = front();
    studio.sculpt.brush.tool = SculptTool::Mask;
    studio.sculpt.brush.strength = 1.0;
    studio.sculpt.brush.radius = 60.0;
    let across = stroke(&screen, Vec3::new(-0.1, 0.0, 1.0), Vec3::new(0.1, 0.0, 1.0));
    assert!(studio.sculpt_stroke(&screen, &across, false));
    assert_eq!(
        studio.doc.as_ref().unwrap().undo_label(),
        Some("Voxel Remesh"),
        "masking isn't an undo step"
    );
    let mask = studio.sculpt_of().unwrap().mask().to_vec();
    let masked: Vec<usize> = (0..mask.len()).filter(|&v| mask[v] == 1.0).collect();
    assert!(masked.len() > 50);
    let before = positions(&studio);
    for tool in [
        SculptTool::Draw,
        SculptTool::ClayStrips,
        SculptTool::Smooth,
        SculptTool::Crease,
        SculptTool::Grab,
    ] {
        studio.sculpt.brush.tool = tool;
        let through = stroke(
            &screen,
            Vec3::new(0.0, 0.05, 1.0),
            Vec3::new(0.7, 0.05, 0.7),
        );
        assert!(studio.sculpt_stroke(&screen, &through, false));
    }
    let after = positions(&studio);
    assert!(masked.iter().all(|&v| after[v] == before[v]));
    assert_ne!(after, before, "what isn't masked moved");

    // Ctrl takes the mask off where it's painted.
    studio.sculpt.brush.tool = SculptTool::Mask;
    assert!(studio.sculpt_stroke(&screen, &across, true));
    assert!(studio.sculpt_of().unwrap().mask()[masked[0]] < 1.0);
    // Invert, Hide Masked and Show All.
    studio.change_mask(|sculpt| sculpt.invert_mask());
    studio.change_mask(|sculpt| {
        sculpt.hide_masked();
    });
    assert!(studio.sculpt_of().unwrap().hidden_count() > 0);
    studio.change_mask(|sculpt| sculpt.reveal());
    studio.change_mask(|sculpt| sculpt.clear_mask());
    assert!(studio.sculpt_of().unwrap().mask().iter().all(|&m| m == 0.0));
}

#[test]
fn a_stroke_sends_only_the_chunks_it_touched_again() {
    let mut studio = remeshed_sphere();
    // Finer, so the sphere is several chunks.
    studio.sculpt_detail(false);
    let first = drawn(&mut studio);
    assert!(first.len() >= 6, "{} chunks", first.len());
    assert_eq!(
        drawn(&mut studio),
        first,
        "the same meshes until they change"
    );
    let screen = front();
    studio.sculpt.brush.tool = SculptTool::Draw;
    studio.sculpt.brush.symmetry = [false; 3];
    let small = stroke(&screen, Vec3::new(0.0, 0.0, 1.0), Vec3::new(0.05, 0.0, 1.0));
    assert!(studio.sculpt_stroke(&screen, &small, false));
    let after = drawn(&mut studio);
    assert_eq!(after.len(), first.len());
    let sent: Vec<_> = after.iter().filter(|mesh| !first.contains(mesh)).collect();
    assert!(!sent.is_empty() && sent.len() < first.len() / 2, "{sent:?}");
    // Those keep their keys: the GPU writes the new corners over the old.
    for (key, version) in &sent {
        assert!(first.iter().any(|(k, v)| k == key && v < version));
    }
}

#[test]
fn snapshot_of_the_tab_sculpting() {
    let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS") else {
        return;
    };
    let folder = std::path::PathBuf::from(folder);
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
            Vec3::new(0.0, 0.2, 1.0),
            Vec3::new(0.0, -0.15, 1.0),
        ),
        (
            SculptTool::Crease,
            Vec3::new(0.0, -0.45, 0.89),
            Vec3::new(0.35, -0.42, 0.83),
        ),
        (
            SculptTool::Mask,
            Vec3::new(0.6, 0.0, 0.8),
            Vec3::new(0.8, -0.1, 0.6),
        ),
    ] {
        studio.sculpt.brush.tool = tool;
        studio.sculpt.brush.strength = 1.0;
        assert!(studio.sculpt_stroke(&screen, &stroke(&screen, from, to), false));
    }
    studio.sculpt.brush.tool = SculptTool::Draw;
    crate::snapshot::snapshot_rendered(
        &folder.join("model-studio-sculpt.png"),
        egui::vec2(1100.0, 700.0),
        vec![Vec::new(); 3],
        |ctx, render| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let _ = studio.show(ui, Some(render), &[], crate::gizmo::Snap::default(), true);
            });
        },
    );
}
