//! Debug › Animator Inspector… on the `hello-animation` noodle.

use super::animator_inspector::Showing;
use super::command_palette::Command;
use super::tests::copy_of_example;
use super::*;

#[test]
fn the_inspector_follows_the_selected_animator_during_play() {
    let folder = copy_of_example("hello-animation", "animator-inspector");
    let mut app = EditorApp::new(Some(&folder));
    let ctx = theme::test_context();
    app.run_command(&ctx, Command::AnimatorInspector);
    assert!(app.animator_inspector.open);
    assert!(matches!(app.animator_showing(), Showing::Stopped));

    app.play();
    for _ in 0..10 {
        app.play_frame(1.0 / 60.0, &[], None, None);
    }
    let world = app.play.as_ref().unwrap().world();
    let noodle = world.find_first_child(world.workspace(), "Noodle").unwrap();
    let animator = world.find_first_child(noodle, "Animator").unwrap();
    // Nothing selected: the noodle is offered.
    app.selection.clear();
    match app.animator_showing() {
        Showing::Choose(animated) => {
            assert_eq!(animated, [(noodle, "Workspace.Noodle".to_owned())]);
        }
        _ => panic!("nothing selected offers what's animated"),
    }
    // Its Animator selected shows the noodle's state machine.
    app.selection = vec![animator];
    let Showing::View(view, name) = app.animator_showing() else {
        panic!("the Animator shows its MeshPart");
    };
    assert_eq!(name, "Workspace.Noodle");
    assert_eq!(view.owner, noodle);
    assert_eq!(view.machine.as_ref().unwrap().state, "Locomotion");
    assert_eq!(view.fired.last().unwrap().what, "Started in Locomotion");

    // The window draws it.
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1440.0, 900.0),
        )),
        ..Default::default()
    };
    let _ = crate::theme::run(&ctx, input, |ui| app.animator_inspector_window(ui.ctx()));
    assert!(app.animator_inspector.open);

    app.stop();
    assert!(matches!(app.animator_showing(), Showing::Stopped));
    let _ = std::fs::remove_dir_all(&folder);
}
