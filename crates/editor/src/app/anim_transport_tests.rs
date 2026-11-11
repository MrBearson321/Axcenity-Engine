//! The Animation tab's transport (usability brief §3): its buttons and
//! keys play, pause, resume, step and seek the open clip, the keys only
//! with the mouse over the clip and nothing being typed, and the preview's
//! rate changes only how fast it plays.

use super::tests::{copy_of_example, drawn_texts};
use super::*;
use crate::theme::icons;

const SIZE: egui::Vec2 = egui::vec2(1300.0, 700.0);
/// The clip is drawn right of this; a notes box is left of it.
const CLIP_LEFT: f32 = 300.0;
const OVER_CLIP: egui::Pos2 = egui::pos2(800.0, 300.0);
const OFF_CLIP: egui::Pos2 = egui::pos2(100.0, 400.0);

struct Tab {
    app: EditorApp,
    ctx: egui::Context,
    folder: PathBuf,
    notes: String,
    time: f64,
}

impl Tab {
    /// A part in hello-touched, animated, so its new keyed clip is open.
    fn new(test: &str) -> Tab {
        let folder = copy_of_example("hello-touched", test);
        let mut app = EditorApp::new(Some(&folder));
        let pad = app
            .world
            .get(app.world.workspace())
            .and_then(|workspace| workspace.children().last().copied())
            .expect("the example has parts");
        app.select_clicked(Some(pad), egui::Modifiers::NONE, None, false);
        app.animate(pad);
        let mut tab = Tab {
            app,
            ctx: theme::test_context(),
            folder,
            notes: String::new(),
            time: 0.0,
        };
        for _ in 0..3 {
            tab.frame(OVER_CLIP, Vec::new());
        }
        tab
    }

    /// One frame, 1/30 s after the last, with the pointer at `pointer`.
    fn frame(&mut self, pointer: egui::Pos2, events: Vec<egui::Event>) -> egui::FullOutput {
        self.time += 1.0 / 30.0;
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, SIZE)),
            time: Some(self.time),
            events: std::iter::once(egui::Event::PointerMoved(pointer))
                .chain(events)
                .collect(),
            ..Default::default()
        };
        let (app, notes) = (&mut self.app, &mut self.notes);
        crate::theme::run(&self.ctx, input, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let full = ui.max_rect();
                let (left, right) = full.split_left_right_at_x(CLIP_LEFT);
                ui.scope_builder(egui::UiBuilder::new().max_rect(left), |ui| {
                    ui.add(egui::TextEdit::singleline(notes).id(egui::Id::new("notes")));
                });
                ui.scope_builder(egui::UiBuilder::new().max_rect(right), |ui| {
                    app.animation_pane(ui, None)
                });
            });
        })
    }

    /// Presses `key` with `modifiers`, the pointer at `pointer`, then
    /// lets it go the next frame.
    fn press_at(&mut self, pointer: egui::Pos2, modifiers: egui::Modifiers, key: egui::Key) {
        let event = |pressed| egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers,
        };
        self.frame(
            pointer,
            vec![egui::Event::ModifiersChanged(modifiers), event(true)],
        );
        self.frame(
            pointer,
            vec![
                event(false),
                egui::Event::ModifiersChanged(egui::Modifiers::NONE),
            ],
        );
    }

    fn press(&mut self, key: egui::Key) {
        self.press_at(OVER_CLIP, egui::Modifiers::NONE, key);
    }

    fn state(&mut self) -> (bool, f64, f64, f64) {
        self.app.animation.transport_state()
    }

    fn playing(&mut self) -> bool {
        self.state().0
    }

    /// The playhead, in frames.
    fn frame_at(&mut self) -> f64 {
        let (_, playhead, _, rate) = self.state();
        playhead * rate
    }
}

impl Drop for Tab {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.folder);
    }
}

#[test]
fn the_transport_keys_play_pause_resume_step_and_go_to_the_ends() {
    let mut tab = Tab::new("anim-transport-keys");
    let (playing, playhead, length, rate) = tab.state();
    assert!(!playing, "a new keyed clip waits");
    assert_eq!(playhead, 0.0);
    assert!(length > 1.0 && rate == 30.0, "{length} s at {rate}");

    tab.press(egui::Key::Space);
    assert!(tab.playing(), "Space plays");
    for _ in 0..5 {
        tab.frame(OVER_CLIP, Vec::new());
    }
    tab.press(egui::Key::Space);
    assert!(!tab.playing(), "Space again pauses");
    let paused = tab.state().1;
    assert!(paused > 0.1, "it played: {paused}");
    tab.frame(OVER_CLIP, Vec::new());
    assert_eq!(tab.state().1, paused, "paused, it stays put");

    // Steps land on whole frames.
    let at = tab.frame_at().round();
    tab.press(egui::Key::ArrowRight);
    assert!((tab.frame_at() - (at + 1.0)).abs() < 1e-6);
    tab.press_at(OVER_CLIP, egui::Modifiers::SHIFT, egui::Key::ArrowRight);
    assert!((tab.frame_at() - (at + 11.0)).abs() < 1e-6, "Shift: ten");
    tab.press(egui::Key::ArrowLeft);
    assert!((tab.frame_at() - (at + 10.0)).abs() < 1e-6);
    tab.press_at(OVER_CLIP, egui::Modifiers::SHIFT, egui::Key::ArrowLeft);
    assert!((tab.frame_at() - at).abs() < 1e-6);

    // Space resumes from there.
    tab.press(egui::Key::Space);
    assert!(tab.playing());
    assert!(tab.state().1 >= paused - 1e-6, "resumed, not restarted");

    tab.press(egui::Key::End);
    let (playing, playhead, length, _) = tab.state();
    assert!(!playing && playhead == length, "End: paused at the end");
    tab.press(egui::Key::Home);
    assert_eq!(tab.state().1, 0.0, "Home: the start");
    tab.press(egui::Key::End);
    tab.press(egui::Key::Space);
    let (playing, playhead, _, _) = tab.state();
    assert!(playing && playhead < 0.2, "play at the end starts over");
}

#[test]
fn the_transport_keys_leave_typing_and_the_rest_of_the_editor_alone() {
    let mut tab = Tab::new("anim-transport-focus");

    // The mouse elsewhere: Space and the arrows aren't the clip's.
    tab.press_at(OFF_CLIP, egui::Modifiers::NONE, egui::Key::Space);
    tab.press_at(OFF_CLIP, egui::Modifiers::NONE, egui::Key::ArrowRight);
    let (playing, playhead, _, _) = tab.state();
    assert!(!playing && playhead == 0.0, "not over the clip");

    // Typing in a box, even with the mouse over the clip.
    tab.ctx
        .memory_mut(|memory| memory.request_focus(egui::Id::new("notes")));
    tab.frame(OVER_CLIP, Vec::new());
    assert_eq!(
        tab.ctx.memory(|memory| memory.focused()),
        Some(egui::Id::new("notes")),
        "the box is being typed in"
    );
    tab.press(egui::Key::Space);
    tab.press(egui::Key::ArrowRight);
    let (playing, playhead, _, _) = tab.state();
    assert!(!playing && playhead == 0.0, "typing keeps its keys");

    // Once nothing has the keyboard, they're the clip's again.
    tab.ctx.memory_mut(|memory| {
        if let Some(id) = memory.focused() {
            memory.surrender_focus(id);
        }
    });
    tab.press(egui::Key::ArrowRight);
    assert!((tab.frame_at() - 1.0).abs() < 1e-6, "{:?}", tab.state());
}

#[test]
fn the_transport_bar_has_the_only_play_button_and_it_plays() {
    let mut tab = Tab::new("anim-transport-button");
    let output = tab.frame(OVER_CLIP, Vec::new());
    let texts = drawn_texts(&output);
    let plays: Vec<egui::Pos2> = texts
        .iter()
        .filter(|(text, at)| text == icons::PLAY && at.x > CLIP_LEFT)
        .map(|(_, at)| *at)
        .collect();
    assert_eq!(plays.len(), 1, "one play button: {texts:?}");
    let shown = |text: &str| texts.iter().any(|(drawn, _)| drawn == text);
    assert!(shown("0:00.00"), "where the playhead is: {texts:?}");
    assert!(shown("0:01.50"), "how long the clip plays: {texts:?}");
    assert!(shown("0/45"), "the frame counter: {texts:?}");

    let button = |pressed| egui::Event::PointerButton {
        pos: plays[0],
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    tab.frame(plays[0], Vec::new());
    tab.frame(plays[0], vec![button(true)]);
    tab.frame(plays[0], vec![button(false)]);
    assert!(tab.playing(), "clicking it plays");
}

#[test]
fn the_preview_rate_changes_how_fast_it_plays_but_not_the_clip() {
    let mut tab = Tab::new("anim-transport-rate");
    assert_eq!(tab.app.animation.preview_rate(), 1.0);
    let file = tab.app.animation.open_keyed_file();
    tab.app.animation.set_preview_rate(2.0);
    tab.press(egui::Key::Space);
    let start = tab.state().1;
    for _ in 0..6 {
        tab.frame(OVER_CLIP, Vec::new());
    }
    let moved = tab.state().1 - start;
    // Six frames of 1/30 s at twice real time: 0.4 s.
    assert!((moved - 0.4).abs() < 0.02, "{moved}");
    assert_eq!(
        tab.app.animation.open_keyed_file(),
        file,
        "the clip is as it was"
    );
}

/// With `AXCENITY_SNAPSHOTS`, saves the tab wide (`anim-transport.png`, one
/// row) and narrow (`anim-transport-narrow.png`, two rows) to look at.
#[test]
fn the_transport_bar_snapshots() {
    let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS").map(PathBuf::from) else {
        return;
    };
    for (name, size) in [
        ("anim-transport.png", egui::vec2(1100.0, 640.0)),
        ("anim-transport-narrow.png", egui::vec2(420.0, 900.0)),
    ] {
        // A tab each: its preview keeps textures of the renderer it drew with.
        let mut tab = Tab::new(name.trim_end_matches(".png"));
        tab.app.animation.set_playhead(0.6);
        crate::snapshot::snapshot_rendered(
            &folder.join(name),
            size,
            vec![Vec::new(); 3],
            |ui, render_state| {
                egui::CentralPanel::default()
                    .show(ui, |ui| tab.app.animation_pane(ui, Some(render_state)));
            },
        );
    }
}
