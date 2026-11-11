//! The Sequencer's transport: the Animation tab's bar and keys over the
//! open sequence. Its buttons and keys play, pause, resume, step and seek,
//! the keys only with the mouse over the tab and nothing being typed, and
//! the preview's rate changes only how fast it plays.

use super::tests::{copy_of_example, drawn_texts};
use super::*;
use crate::theme::icons;

const INTRO: &str = "assets/sequences/Intro.sequence.json";
const SIZE: egui::Vec2 = egui::vec2(1300.0, 800.0);
/// The tab is drawn right of this; a notes box is left of it.
const TAB_LEFT: f32 = 300.0;
const OVER_TAB: egui::Pos2 = egui::pos2(800.0, 500.0);
const OFF_TAB: egui::Pos2 = egui::pos2(100.0, 400.0);

struct Tab {
    app: EditorApp,
    ctx: egui::Context,
    folder: PathBuf,
    notes: String,
    time: f64,
}

impl Tab {
    /// hello-cutscene's Intro (20 s) open in the Sequencer, Preview off.
    fn new(test: &str) -> Tab {
        let folder = copy_of_example("hello-cutscene", test);
        let mut app = EditorApp::new(Some(&folder));
        app.open_sequence(INTRO);
        app.sequencer.preview_on = false;
        let mut tab = Tab {
            app,
            ctx: theme::test_context(),
            folder,
            notes: String::new(),
            time: 0.0,
        };
        for _ in 0..3 {
            tab.frame(OVER_TAB, Vec::new());
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
                let (left, right) = ui.max_rect().split_left_right_at_x(TAB_LEFT);
                ui.scope_builder(egui::UiBuilder::new().max_rect(left), |ui| {
                    ui.add(egui::TextEdit::singleline(notes).id(egui::Id::new("notes")));
                });
                ui.scope_builder(egui::UiBuilder::new().max_rect(right), |ui| {
                    app.sequencer_pane(ui)
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
        self.press_at(OVER_TAB, egui::Modifiers::NONE, key);
    }

    fn keys(&self) -> &crate::animation::property_editor::PropertyDoc {
        &self.app.sequencer.doc.as_ref().expect("Intro is open").keys
    }

    fn playing(&self) -> bool {
        self.keys().playing
    }

    fn playhead(&self) -> f64 {
        self.keys().playhead
    }

    /// The playhead, in frames.
    fn frame_at(&self) -> f64 {
        self.playhead() * self.keys().fps
    }
}

impl Drop for Tab {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.folder);
    }
}

#[test]
fn the_sequencer_keys_play_pause_resume_step_and_go_to_the_ends() {
    let mut tab = Tab::new("seq-transport-keys");
    assert!(!tab.playing() && tab.playhead() == 0.0, "it opens paused");

    tab.press(egui::Key::Space);
    assert!(tab.playing(), "Space plays");
    for _ in 0..5 {
        tab.frame(OVER_TAB, Vec::new());
    }
    tab.press(egui::Key::Space);
    assert!(!tab.playing(), "Space again pauses");
    let paused = tab.playhead();
    assert!(paused > 0.1, "it played: {paused}");

    let at = tab.frame_at().round();
    tab.press(egui::Key::ArrowRight);
    assert!((tab.frame_at() - (at + 1.0)).abs() < 1e-6);
    tab.press_at(OVER_TAB, egui::Modifiers::SHIFT, egui::Key::ArrowRight);
    assert!((tab.frame_at() - (at + 11.0)).abs() < 1e-6, "Shift: ten");
    tab.press_at(OVER_TAB, egui::Modifiers::SHIFT, egui::Key::ArrowLeft);
    tab.press(egui::Key::ArrowLeft);
    assert!((tab.frame_at() - at).abs() < 1e-6);

    tab.press(egui::Key::Space);
    assert!(tab.playing() && tab.playhead() >= paused - 1e-6, "resumed");
    tab.press(egui::Key::End);
    assert!(
        !tab.playing() && tab.playhead() == 20.0,
        "End: paused at the end"
    );
    tab.press(egui::Key::Home);
    assert_eq!(tab.playhead(), 0.0, "Home: the start");
}

#[test]
fn the_sequencer_keys_leave_typing_and_the_rest_of_the_editor_alone() {
    let mut tab = Tab::new("seq-transport-focus");
    tab.press_at(OFF_TAB, egui::Modifiers::NONE, egui::Key::Space);
    tab.press_at(OFF_TAB, egui::Modifiers::NONE, egui::Key::ArrowRight);
    assert!(!tab.playing() && tab.playhead() == 0.0, "not over the tab");

    tab.ctx
        .memory_mut(|memory| memory.request_focus(egui::Id::new("notes")));
    tab.frame(OVER_TAB, Vec::new());
    tab.press(egui::Key::Space);
    tab.press(egui::Key::ArrowRight);
    assert!(
        !tab.playing() && tab.playhead() == 0.0,
        "typing keeps its keys"
    );

    tab.ctx.memory_mut(|memory| {
        if let Some(id) = memory.focused() {
            memory.surrender_focus(id);
        }
    });
    tab.press(egui::Key::ArrowRight);
    assert!((tab.frame_at() - 1.0).abs() < 1e-6, "{}", tab.playhead());
}

#[test]
fn the_sequencer_has_one_play_button_and_the_bar_reads_the_sequence() {
    let mut tab = Tab::new("seq-transport-button");
    let output = tab.frame(OVER_TAB, Vec::new());
    let texts = drawn_texts(&output);
    let plays: Vec<egui::Pos2> = texts
        .iter()
        .filter(|(text, at)| text == icons::PLAY && at.x > TAB_LEFT)
        .map(|(_, at)| *at)
        .collect();
    assert_eq!(plays.len(), 1, "one play button: {texts:?}");
    let shown = |text: &str| texts.iter().any(|(drawn, _)| drawn == text);
    assert!(shown("0:00.00"), "where the playhead is: {texts:?}");
    assert!(shown("0:20.00"), "how long the sequence plays: {texts:?}");

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
fn the_sequencer_preview_rate_changes_how_fast_it_plays_but_not_the_file() {
    let mut tab = Tab::new("seq-transport-rate");
    let before = std::fs::read_to_string(tab.folder.join(INTRO)).unwrap();
    tab.app.sequencer.rate = crate::animation::transport::Rate(2.0);
    tab.press(egui::Key::Space);
    let start = tab.playhead();
    for _ in 0..6 {
        tab.frame(OVER_TAB, Vec::new());
    }
    let moved = tab.playhead() - start;
    // Six frames of 1/30 s at twice real time: 0.4 s.
    assert!((moved - 0.4).abs() < 0.02, "{moved}");
    tab.app.flush_sequence(&tab.ctx);
    assert_eq!(
        std::fs::read_to_string(tab.folder.join(INTRO)).unwrap(),
        before,
        "the sequence is as it was"
    );
}
