//! The Animation tab's drawer and outline say each thing once: what plays
//! a clip isn't listed with what it moves, and the outline's rows don't
//! repeat the model file a linked clip comes from (AN7).

use super::tests::{copy_of_example, drawn_texts};
use super::*;

#[test]
fn the_drawer_lists_what_the_clip_moves_not_its_animator() {
    let folder = copy_of_example("hello-touched", "anim-drawer-objects");
    let mut app = EditorApp::new(Some(&folder));
    let pad = app
        .world
        .get(app.world.workspace())
        .and_then(|workspace| workspace.children().last().copied())
        .expect("the example has parts");
    app.select_clicked(Some(pad), egui::Modifiers::NONE, None, false);
    app.animate(pad);
    let pad_row = {
        let instance = app.world.get(pad).unwrap();
        format!(
            "{} {}",
            theme::class_icon(instance.class()),
            instance.name()
        )
    };
    let animators: Vec<String> = app
        .world
        .descendants(pad)
        .into_iter()
        .filter_map(|id| app.world.get(id))
        .filter(|i| matches!(i.class(), ClassName::Animator | ClassName::Animation))
        .map(|i| format!("{} {}", theme::class_icon(i.class()), i.name()))
        .collect();
    assert!(!animators.is_empty(), "Animate gives it an Animator");

    let ctx = theme::test_context();
    let size = egui::vec2(1100.0, 600.0);
    let mut output = None;
    for _ in 0..3 {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            ..Default::default()
        };
        output = Some(ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| app.animation_pane(ui, None));
        }));
    }
    let texts = drawn_texts(&output.unwrap());
    assert!(
        texts.iter().any(|(text, _)| text == &pad_row),
        "the part is in the drawer: {texts:?}"
    );
    for animator in animators {
        assert!(
            !texts.iter().any(|(text, _)| text == &animator),
            "{animator} isn't listed as something the clip moves: {texts:?}"
        );
    }
    let _ = std::fs::remove_dir_all(&folder);
}
