//! U20, Model Studio PR 2 (`docs/research/usability/model-studio.md`):
//! Retopology and Bake folded until used, the Mesh menu saying why it's
//! greyed, and Paint warning about uvs that would surprise a beginner.

use axcenity_mesh_edit::{ModelDoc, PrimitiveSettings};
use eframe::egui;

use super::paint::UvProblem;
use super::tests::{button, click, cube_doc, frames, studio_with};
use super::*;

fn says(shown: &[(String, egui::Pos2)], words: &str) -> bool {
    shown.iter().any(|(text, _)| text.contains(words))
}

/// MS4: modeling shows Modifiers and a closed Retopology and Bake; it
/// opens by itself once a reference is set, and a click opens it too.
#[test]
fn retopology_and_bake_stay_folded_until_used() {
    let ctx = crate::theme::test_context();
    let mut studio = studio_with(cube_doc());
    let shown = frames(&ctx, &mut studio, Vec::new());
    assert!(says(&shown, "Add Modifier"), "{shown:?}");
    let header = *button(&shown, retopo::RETOPO_HEADER).expect("the fold's header");
    assert!(!says(&shown, "Make Low Poly") && !says(&shown, "Occlusion"));

    let shown = frames(&ctx, &mut studio, click(header));
    assert!(says(&shown, "Make Low Poly") && says(&shown, "Occlusion"));
    let shown = frames(&ctx, &mut studio, click(header));
    assert!(!says(&shown, "Make Low Poly"), "a second click folds it");

    // A second object as the reference opens it, whatever it was.
    let doc = studio.doc.as_mut().unwrap();
    let copy = doc.model.objects[0].clone();
    doc.model.objects.push(copy);
    doc.reference = Some(1);
    let shown = frames(&ctx, &mut studio, Vec::new());
    assert!(says(&shown, "Make Low Poly"), "{shown:?}");
}

/// MS5: the Mesh menu in Object mode says its operations need Edit mode
/// (shading still works), and doesn't in Edit mode.
#[test]
fn the_mesh_menu_says_why_its_operations_are_greyed() {
    let ctx = crate::theme::test_context();
    let mut studio = studio_with(cube_doc());
    let shown = frames(&ctx, &mut studio, Vec::new());
    let mesh = *button(&shown, "Mesh").unwrap();
    let shown = frames(&ctx, &mut studio, click(mesh));
    assert!(says(&shown, tools::EDIT_ONLY), "{shown:?}");
    assert!(
        says(&shown, "Shade Smooth") && says(&shown, "Bridge Edge Loops"),
        "{shown:?}"
    );

    let mut studio = studio_with(cube_doc());
    studio.doc.as_mut().unwrap().set_mode(Mode::Edit);
    let shown = frames(&ctx, &mut studio, Vec::new());
    let mesh = *button(&shown, "Mesh").unwrap();
    let shown = frames(&ctx, &mut studio, click(mesh));
    assert!(says(&shown, "Bridge Edge Loops") && !says(&shown, tools::EDIT_ONLY));
}

/// MS6: a New cube's faces share the texture square, so Paint warns and
/// one click of Smart UV Project (one undo step) clears it. An object
/// whose file had no uvs is named as such.
#[test]
fn paint_warns_when_faces_share_texture_space_and_one_click_fixes_it() {
    let ctx = crate::theme::test_context();
    let mut studio = studio_with(cube_doc());
    studio.doc.as_mut().unwrap().set_mode(Mode::Paint);
    assert_eq!(studio.paint_uv_problem(), Some(UvProblem::Overlapping));
    let shown = frames(&ctx, &mut studio, Vec::new());
    assert!(says(&shown, "share the same texture space"), "{shown:?}");
    let fix = *button(&shown, "Smart UV Project").expect("the fix beside it");
    let shown = frames(&ctx, &mut studio, click(fix));
    let doc = studio.doc.as_ref().unwrap();
    assert_eq!(doc.undo_label(), Some("Smart UV Project"));
    assert_eq!(doc.mode, Mode::Paint, "still painting");
    assert!(!says(&shown, "share the same texture space"), "{shown:?}");
    assert_eq!(studio.paint_uv_problem(), None);
    // Undo brings the warning back.
    assert!(studio.doc.as_mut().unwrap().undo());
    assert_eq!(studio.paint_uv_problem(), Some(UvProblem::Overlapping));

    let mut model = ModelDoc::primitive(&PrimitiveSettings::default());
    model.objects[0].uvs = false;
    let mut studio = studio_with(StudioDoc::new(String::new(), model, false));
    studio.doc.as_mut().unwrap().set_mode(Mode::Paint);
    assert_eq!(studio.paint_uv_problem(), Some(UvProblem::Missing));
    let shown = frames(&ctx, &mut studio, Vec::new());
    assert!(says(&shown, "no texture layout"), "{shown:?}");
}
