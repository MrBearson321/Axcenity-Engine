//! The Collision fold (roadmap R15.2b): generated pieces are undo steps,
//! aren't drawn, are saved as `_COL` objects that `assets` reads as the
//! model's collision, and convex pieces keep a hole open.

use axcenity_assets::{ModelFormat, load_model};
use axcenity_mesh_edit::{ModelDoc, PrimitiveSettings, PrimitiveShape};

use super::tests::cube_doc;
use super::*;

fn names(doc: &StudioDoc) -> Vec<&str> {
    doc.model.objects.iter().map(|o| o.name.as_str()).collect()
}

#[test]
fn box_and_hull_collision_are_undo_steps_and_are_saved_as_pieces() {
    let mut doc = cube_doc();
    let drawn = doc.model.to_model().unwrap().primitives.len();
    assert!(doc.remove_collision().is_err(), "nothing to remove");
    let steps = doc.undo.len();

    doc.generate_collision(CollisionShape::Box).unwrap();
    assert_eq!(names(&doc), ["Cube", "Cube_COL1"]);
    assert_eq!(doc.undo_label(), Some("Generate Box Collision"));
    assert_eq!(doc.active, 0);
    let piece = doc.model.objects[1].mesh.as_ref().unwrap();
    assert_eq!((piece.vertex_count(), piece.face_count()), (8, 12));
    assert!(doc.model.is_collision_piece(1));
    // Not drawn as a solid.
    assert_eq!(doc.model.to_model().unwrap().primitives.len(), drawn);
    // A piece has no fold of its own.
    doc.active = 1;
    assert!(doc.generate_collision(CollisionShape::Box).is_err());
    doc.active = 0;

    doc.generate_collision(CollisionShape::Hull).unwrap();
    assert_eq!(doc.undo_label(), Some("Regenerate Collision"));
    assert_eq!(names(&doc), ["Cube", "Cube_COL1"]);

    // Saved, `assets` reads it as the model's collision and doesn't draw it.
    let bytes = doc.model.write_glb();
    let model = load_model(&bytes, std::path::Path::new("."), ModelFormat::Gltf).unwrap();
    assert_eq!(model.collision.len(), 1);
    assert!(model.collision[0].len() >= 8);

    doc.remove_collision().unwrap();
    assert_eq!(doc.undo_label(), Some("Remove Collision"));
    assert_eq!(names(&doc), ["Cube"]);
    assert_eq!(doc.undo.len(), steps + 3);
    assert!(doc.undo());
    assert_eq!(names(&doc), ["Cube", "Cube_COL1"]);
}

#[test]
fn convex_pieces_follow_a_torus_and_keep_its_hole_open() {
    let settings = PrimitiveSettings {
        shape: PrimitiveShape::Torus,
        ..PrimitiveSettings::default()
    };
    let mut doc = StudioDoc::new(String::new(), ModelDoc::primitive(&settings), false);
    doc.generate_collision(CollisionShape::Pieces(6)).unwrap();
    assert_eq!(doc.undo_label(), Some("Generate Convex Pieces"));
    let pieces = doc.model.collision_pieces(0);
    assert!(
        pieces.len() >= 3 && pieces.len() <= 6,
        "{} pieces",
        pieces.len()
    );
    // The middle of the hole is in no piece.
    let center = doc.model.objects[0]
        .mesh
        .as_ref()
        .map(|mesh| mesh.positions().iter().copied().sum::<Vec3>() / mesh.vertex_count() as f32)
        .unwrap();
    for &piece in &pieces {
        let mesh = doc.model.objects[piece].mesh.as_ref().unwrap();
        let outside = (0..mesh.face_count() as u32).any(|face| {
            let normal = mesh.face_normal(face);
            normal.dot(center - mesh.face_center(face)) > 1e-3
        });
        let extent = mesh
            .positions()
            .iter()
            .fold((Vec3::MAX, Vec3::MIN), |(lo, hi), &p| {
                (lo.min(p), hi.max(p))
            });
        assert!(
            outside,
            "piece {piece} of {} fills the hole at {center}: {extent:?}",
            pieces.len()
        );
    }
}
