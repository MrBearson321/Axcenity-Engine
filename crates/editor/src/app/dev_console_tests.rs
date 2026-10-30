//! The Developer Console during Play (Phase 71): F9 opens it over the
//! viewport, it has the keys while open, and its commands reach the game.

use super::tests::{copy_of_example, play_for};
use super::*;

fn key(key: egui::Key) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}

fn type_line(app: &mut EditorApp, line: &str) {
    app.type_dev_console(
        &[egui::Event::Text(line.to_owned()), key(egui::Key::Enter)],
        false,
    );
}

fn console_said(app: &EditorApp, text: &str) -> bool {
    app.dev.console.lines().any(|line| line.text == text)
}

#[test]
fn f9_in_play_opens_the_console_which_runs_timescale_and_the_games_commands() {
    let folder = copy_of_example("hello-part", "dev-console");
    std::fs::write(
        folder.join("scripts/Commands.server.lua"),
        r#"
game:GetService("ConsoleService"):RegisterCommand("heal", "Heals", function(player, amount)
    print("healed", amount)
end)
"#,
    )
    .unwrap();
    let mut app = EditorApp::new(Some(&folder));
    // Not while editing.
    assert!(!app.type_dev_console(&[key(egui::Key::F9)], false));
    assert!(!app.dev.console.open);

    app.play();
    app.viewport.set_pixel_size((800, 600));
    assert!(app.type_dev_console(&[key(egui::Key::F9)], false));
    assert!(app.dev.console.open);
    assert!(!app.dev_console_items().is_empty(), "drawn over the game");
    // An editor text field with the keyboard keeps F9 from opening it.
    app.dev.console.toggle();
    assert!(!app.type_dev_console(&[key(egui::Key::F9)], true));
    assert!(app.type_dev_console(&[key(egui::Key::F9)], false));

    type_line(&mut app, "timescale 0.5");
    assert_eq!(app.dev.time_scale, 0.5);
    type_line(&mut app, "heal 7");
    assert!(
        console_said(&app, "healed\t7") || console_said(&app, "healed 7"),
        "{:?}",
        {
            app.dev
                .console
                .lines()
                .map(|line| line.text.clone())
                .collect::<Vec<_>>()
        }
    );
    type_line(&mut app, "collisions");
    assert!(app.overlays.colliders);
    type_line(&mut app, "wireframe");
    assert_eq!(
        app.view_mode,
        crate::viewport::ViewMode::Debug(axcenity_render::DebugView::Wireframe)
    );
    type_line(&mut app, "workspace.Gravity = 20");
    play_for(&mut app, 0.1);
    assert_eq!(
        shown(&app.world, &app.play)
            .get(shown(&app.world, &app.play).workspace())
            .and_then(|workspace| workspace.property("Gravity"))
            .cloned(),
        Some(axcenity_core::Value::Number(20.0))
    );
    type_line(&mut app, "dance");
    assert!(console_said(
        &app,
        "Unknown command `dance`. Type help for the list."
    ));

    // Stop: the console starts over.
    app.stop();
    app.type_dev_console(&[], false);
    assert!(!app.dev.console.open);
    assert_eq!(app.dev.time_scale, 1.0);
    let _ = std::fs::remove_dir_all(folder);
}
