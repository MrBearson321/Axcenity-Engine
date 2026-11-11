//! The clip's transport, like a media player's: go to the start, step a
//! frame back, play or pause (resume once paused), step a frame on, go to
//! the end and stop, a seek bar with where the playhead is and how long
//! the clip runs, and how fast the preview plays.
//!
//! Keys, with the mouse over the open clip or sequence and nothing being
//! typed: Space plays or pauses, Left and Right step a frame (Shift: ten),
//! Home and End go to the start and end. They're in the shortcut table
//! (`shortcuts.rs`, Animation and Sequencer) and so in Settings > Shortcuts.
//!
//! Both kinds of clip (an imported take's and a property clip's) and the
//! Sequencer's sequence play through [`Playback`], so they share one bar
//! and one set of keys.

use eframe::egui::{self, RichText, Sense, Stroke, vec2};

use crate::theme::{self, icons};
use crate::widgets;

/// The preview's playback rates offered, as multiples of real time. They
/// change only how fast the preview plays, never the clip's own Speed.
pub const RATES: [f64; 6] = [0.25, 0.5, 1.0, 1.5, 2.0, 4.0];
/// How many frames Shift+Left and Shift+Right step.
pub const BIG_STEP: i64 = 10;

/// A clip that plays: what the transport reads and changes.
pub trait Playback {
    fn playing(&self) -> bool;
    fn set_playing(&mut self, playing: bool);
    /// Seconds into the clip as it plays (at its own Speed).
    fn playhead(&self) -> f64;
    /// Moves the playhead to `seconds`, kept inside the clip.
    fn set_playhead(&mut self, seconds: f64);
    /// How long one pass takes, in seconds as it plays.
    fn length(&self) -> f64;
    /// Frames a second as it plays, for stepping and the frame counter.
    fn frame_rate(&self) -> f64;
}

/// What a button or key asked for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Command {
    /// Play from where it is (from the start, if it's at the end), or pause.
    PlayPause,
    /// Pause and go back to the start.
    Stop,
    ToStart,
    ToEnd,
    /// Pause and move this many frames (negative: back).
    Step(i64),
    /// Move the playhead here (seconds), as dragging the seek bar does.
    Seek(f64),
}

/// Does `command` to `clip`.
pub fn apply(clip: &mut dyn Playback, command: Command) {
    let length = clip.length().max(0.0);
    match command {
        Command::PlayPause => {
            if clip.playing() {
                clip.set_playing(false);
            } else {
                // At the end, play starts over, as a media player's does.
                if clip.playhead() >= length - 1e-9 {
                    clip.set_playhead(0.0);
                }
                clip.set_playing(true);
            }
        }
        Command::Stop => {
            clip.set_playing(false);
            clip.set_playhead(0.0);
        }
        Command::ToStart => clip.set_playhead(0.0),
        Command::ToEnd => {
            clip.set_playing(false);
            clip.set_playhead(length);
        }
        Command::Step(frames) => {
            clip.set_playing(false);
            let rate = clip.frame_rate().max(1.0);
            // From the nearest whole frame, so steps land on frames.
            let frame = (clip.playhead() * rate).round() as i64 + frames;
            let last = (length * rate).round() as i64;
            clip.set_playhead((frame.clamp(0, last.max(0)) as f64 / rate).min(length));
        }
        Command::Seek(seconds) => clip.set_playhead(seconds.clamp(0.0, length)),
    }
}

/// Whether nothing else wants the keyboard: nothing is being typed, no
/// widget has focus (one tabbed to keeps Space and the arrows) and no menu
/// is open. The transport's keys also need the mouse over its tab.
pub fn keys_free(ctx: &egui::Context) -> bool {
    !ctx.egui_wants_keyboard_input()
        && ctx.memory(|memory| memory.focused()).is_none()
        && !egui::Popup::is_any_open(ctx)
}

/// The transport keys pressed this frame, in order, taken so nothing else
/// sees them. Call only while the clip should have them
/// (`AnimationStudio::wants_transport_keys`, or the mouse over the
/// Sequencer and [`keys_free`]).
pub fn keys(input: &mut egui::InputState) -> Vec<Command> {
    use crate::shortcuts::{Action, consume_first};
    // Shift+Left before Left: a plain key's pattern matches with Shift too.
    const ACTIONS: [Action; 7] = [
        Action::PreviousFrames,
        Action::NextFrames,
        Action::PlayPause,
        Action::PreviousFrame,
        Action::NextFrame,
        Action::ClipStart,
        Action::ClipEnd,
    ];
    let mut commands = Vec::new();
    // Each pass takes one action's presses (a held arrow's repeats too).
    while let Some(action) = consume_first(input, &ACTIONS) {
        commands.push(match action {
            Action::PlayPause => Command::PlayPause,
            Action::PreviousFrames => Command::Step(-BIG_STEP),
            Action::NextFrames => Command::Step(BIG_STEP),
            Action::PreviousFrame => Command::Step(-1),
            Action::NextFrame => Command::Step(1),
            Action::ClipStart => Command::ToStart,
            _ => Command::ToEnd,
        });
    }
    commands
}

/// The preview's playback rate, a multiple of real time (1 by default).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rate(pub f64);

impl Default for Rate {
    fn default() -> Self {
        Rate(1.0)
    }
}

/// `seconds` as a media player shows it: minutes, seconds and hundredths.
pub fn clock(seconds: f64) -> String {
    let seconds = seconds.max(0.0);
    let minutes = (seconds / 60.0).floor();
    format!("{}:{:05.2}", minutes as u64, seconds - minutes * 60.0)
}

/// Below this width the bar takes two rows: the buttons and rate, then
/// the times and seek bar.
const ONE_ROW: f32 = 480.0;

/// The transport bar. `rate` is the preview's playback rate, which it can
/// change. Returns what was clicked or dragged.
pub fn bar(ui: &mut egui::Ui, clip: &dyn Playback, rate: &mut Rate) -> Option<Command> {
    let mut command = None;
    let one_row = ui.available_width() >= ONE_ROW;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        buttons(ui, clip, &mut command);
        if one_row {
            ui.add_space(8.0);
            times(ui, clip, &mut command, RATE_WIDTH + 6.0);
        }
        rate_picker(ui, rate, one_row);
    });
    if !one_row {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            times(ui, clip, &mut command, 0.0);
        });
    }
    command
}

const RATE_WIDTH: f32 = 52.0;

/// Start, back a frame, play, on a frame, end and stop.
fn buttons(ui: &mut egui::Ui, clip: &dyn Playback, command: &mut Option<Command>) {
    use crate::shortcuts::Action;
    let key = |ui: &egui::Ui, action: Action| action.menu_text(ui.ctx());
    let length = clip.length().max(0.0);
    let playhead = clip.playhead().clamp(0.0, length);
    let at_end = playhead >= length - 1e-9;
    let button = |ui: &mut egui::Ui, icon: &str, tooltip: String| {
        widgets::icon_button(ui, icon, &tooltip, false).clicked()
    };
    if button(
        ui,
        icons::SKIP_BACK,
        format!("Go to the start ({})", key(ui, Action::ClipStart)),
    ) {
        *command = Some(Command::ToStart);
    }
    if button(
        ui,
        icons::CARET_LEFT,
        format!(
            "Back one frame ({}; {}: ten)",
            key(ui, Action::PreviousFrame),
            key(ui, Action::PreviousFrames)
        ),
    ) {
        *command = Some(Command::Step(-1));
    }
    let (icon, what) = if clip.playing() {
        (icons::PAUSE, "Pause")
    } else if playhead > 0.0 && !at_end {
        (icons::PLAY, "Resume")
    } else {
        (icons::PLAY, "Play")
    };
    if play_button(
        ui,
        icon,
        &format!("{what} ({})", key(ui, Action::PlayPause)),
    )
    .clicked()
    {
        *command = Some(Command::PlayPause);
    }
    if button(
        ui,
        icons::CARET_RIGHT,
        format!(
            "On one frame ({}; {}: ten)",
            key(ui, Action::NextFrame),
            key(ui, Action::NextFrames)
        ),
    ) {
        *command = Some(Command::Step(1));
    }
    if button(
        ui,
        icons::SKIP_FORWARD,
        format!("Go to the end ({})", key(ui, Action::ClipEnd)),
    ) {
        *command = Some(Command::ToEnd);
    }
    if button(
        ui,
        icons::STOP,
        "Stop: pause and go back to the start".to_owned(),
    ) {
        *command = Some(Command::Stop);
    }
}

/// The playhead's time, the seek bar taking the room left but `keep`, and
/// the clip's length, then the frame counter.
fn times(ui: &mut egui::Ui, clip: &dyn Playback, command: &mut Option<Command>, keep: f32) {
    let palette = theme::palette(ui.ctx());
    let length = clip.length().max(0.0);
    let playhead = clip.playhead().clamp(0.0, length);
    let frame_rate = clip.frame_rate().max(1.0);
    let frame = (playhead * frame_rate).round() as u64;
    let frames = (length * frame_rate).round() as u64;
    ui.label(
        RichText::new(clock(playhead))
            .monospace()
            .color(palette.text_strong),
    )
    .on_hover_text("Where the playhead is");
    // The length label and the frame counter after the seek bar.
    let after = 54.0 + 6.0 + 8.0 * frame_text(frames, frames).len() as f32 + keep;
    let width = (ui.available_width() - after).max(40.0);
    if let Some(seconds) = seek_bar(ui, playhead, length, width) {
        *command = Some(Command::Seek(seconds));
    }
    ui.label(
        RichText::new(clock(length))
            .monospace()
            .color(palette.text_secondary),
    )
    .on_hover_text("How long the clip plays");
    ui.add_space(6.0);
    ui.label(
        RichText::new(frame_text(frame, frames))
            .monospace()
            .color(palette.text_secondary),
    )
    .on_hover_text(format!(
        "Frame {frame} of {frames}, at {frame_rate:.0} a second"
    ));
}

fn frame_text(frame: u64, frames: u64) -> String {
    format!("{frame}/{frames}")
}

/// The preview's rate: after the times, or pushed to the row's end.
fn rate_picker(ui: &mut egui::Ui, rate: &mut Rate, follows_times: bool) {
    let mut pick = |ui: &mut egui::Ui| {
        egui::ComboBox::from_id_salt("clip playback rate")
            .selected_text(rate_label(rate.0))
            .width(RATE_WIDTH)
            .show_ui(ui, |ui| {
                for choice in RATES {
                    ui.selectable_value(&mut rate.0, choice, rate_label(choice));
                }
            })
            .response
            .on_hover_text(
                "How fast the preview plays. Only for watching: it doesn't change the \
                 clip or sequence.",
            );
    };
    if follows_times {
        pick(ui);
    } else {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), pick);
    }
}

fn rate_label(rate: f64) -> String {
    format!("{rate}×")
}

/// The accent play button, a little bigger than the others.
fn play_button(ui: &mut egui::Ui, icon: &str, tooltip: &str) -> egui::Response {
    let palette = theme::palette(ui.ctx());
    let side = ui.spacing().interact_size.y + 4.0;
    let (rect, response) = ui.allocate_exact_size(vec2(side, side), Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tooltip));
    let fill = if response.hovered() {
        palette.accent_solid
    } else {
        palette.accent
    };
    ui.painter().circle_filled(rect.center(), side / 2.0, fill);
    if response.has_focus() {
        ui.painter()
            .circle_stroke(rect.center(), side / 2.0 + 2.0, palette.focus_stroke());
    }
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        icon,
        egui::FontId::proportional(theme::size::ICON_MEDIUM),
        palette.on_accent,
    );
    response.on_hover_text(tooltip)
}

/// A thin bar showing how far through the clip the playhead is, with a
/// knob to drag. Clicking or dragging anywhere on it seeks there.
fn seek_bar(ui: &mut egui::Ui, playhead: f64, length: f64, width: f32) -> Option<f64> {
    let palette = theme::palette(ui.ctx());
    let height = ui.spacing().interact_size.y;
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::click_and_drag());
    response.widget_info(|| {
        egui::WidgetInfo::slider(true, playhead, "Seek: where the playhead is in the clip")
    });
    let track = egui::Rect::from_center_size(rect.center(), vec2(rect.width() - 8.0, 4.0));
    let share = if length > 0.0 {
        (playhead / length).clamp(0.0, 1.0) as f32
    } else {
        0.0
    };
    let x = track.left() + track.width() * share;
    ui.painter().rect_filled(track, 2.0, palette.inset);
    ui.painter().rect_filled(
        egui::Rect::from_min_max(track.min, egui::pos2(x, track.bottom())),
        2.0,
        palette.accent,
    );
    let knob = if response.hovered() || response.dragged() {
        6.0
    } else {
        4.5
    };
    ui.painter().circle(
        egui::pos2(x, track.center().y),
        knob,
        palette.text_strong,
        Stroke::new(1.0_f32, palette.border),
    );
    let pointer = response.interact_pointer_pos()?;
    if !(response.clicked() || response.dragged()) || length <= 0.0 {
        return None;
    }
    let share = ((pointer.x - track.left()) / track.width().max(1.0)).clamp(0.0, 1.0);
    Some(f64::from(share) * length)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A clip 2 seconds long at 30 frames a second.
    #[derive(Default)]
    struct Clip {
        playing: bool,
        playhead: f64,
    }

    impl Playback for Clip {
        fn playing(&self) -> bool {
            self.playing
        }
        fn set_playing(&mut self, playing: bool) {
            self.playing = playing;
        }
        fn playhead(&self) -> f64 {
            self.playhead
        }
        fn set_playhead(&mut self, seconds: f64) {
            self.playhead = seconds.clamp(0.0, 2.0);
        }
        fn length(&self) -> f64 {
            2.0
        }
        fn frame_rate(&self) -> f64 {
            30.0
        }
    }

    #[test]
    fn play_pauses_resumes_and_starts_over_at_the_end() {
        let mut clip = Clip::default();
        apply(&mut clip, Command::PlayPause);
        assert!(clip.playing);
        clip.playhead = 0.5;
        apply(&mut clip, Command::PlayPause);
        assert!(!clip.playing, "paused");
        assert_eq!(clip.playhead, 0.5, "where it was");
        apply(&mut clip, Command::PlayPause);
        assert!(clip.playing && clip.playhead == 0.5, "resumed from there");
        apply(&mut clip, Command::ToEnd);
        assert!(!clip.playing && clip.playhead == 2.0);
        apply(&mut clip, Command::PlayPause);
        assert!(clip.playing && clip.playhead == 0.0, "from the start again");
    }

    #[test]
    fn stop_pauses_and_goes_back_to_the_start() {
        let mut clip = Clip {
            playing: true,
            playhead: 1.2,
        };
        apply(&mut clip, Command::Stop);
        assert!(!clip.playing);
        assert_eq!(clip.playhead, 0.0);
    }

    #[test]
    fn steps_land_on_frames_and_stay_inside_the_clip() {
        let mut clip = Clip {
            playing: true,
            playhead: 0.51,
        };
        apply(&mut clip, Command::Step(1));
        assert!(!clip.playing, "stepping pauses");
        // 0.51 s is nearest frame 15; one on is frame 16.
        assert!(
            (clip.playhead - 16.0 / 30.0).abs() < 1e-9,
            "{}",
            clip.playhead
        );
        apply(&mut clip, Command::Step(-BIG_STEP));
        assert!((clip.playhead - 6.0 / 30.0).abs() < 1e-9);
        apply(&mut clip, Command::Step(-BIG_STEP));
        assert_eq!(clip.playhead, 0.0, "not before the start");
        apply(&mut clip, Command::Step(1000));
        assert_eq!(clip.playhead, 2.0, "not past the end");
    }

    #[test]
    fn seeking_stays_inside_the_clip() {
        let mut clip = Clip::default();
        apply(&mut clip, Command::Seek(1.25));
        assert_eq!(clip.playhead, 1.25);
        apply(&mut clip, Command::Seek(-3.0));
        assert_eq!(clip.playhead, 0.0);
    }

    #[test]
    fn the_clock_reads_like_a_media_players() {
        assert_eq!(clock(0.0), "0:00.00");
        assert_eq!(clock(1.25), "0:01.25");
        assert_eq!(clock(75.5), "1:15.50");
    }
}
