//! The Profiler's **Scripts** tab (scripting P2 PR 4): which event handlers,
//! scripts and tasks cost the most time, and which script functions, from
//! the script runtime's profiler (`axcenity_script::ScriptProfile`). It
//! records only while **Record** is on, because sampling the scripts' call
//! stacks costs the game a little time. Clicking a row opens its script at
//! the line.

use std::time::{Duration, Instant};

use axcenity_script::{ScriptProfile, WorkKind};
use eframe::egui::{self, RichText};

use super::{Profiler, millis};
use crate::theme::{self, type_scale};
use crate::widgets;

/// How often the panel asks the runtime for its rows while recording.
const FETCH_EVERY: Duration = Duration::from_millis(500);
/// How many rows each list shows.
const ROWS: usize = 40;

/// Which list the Scripts tab shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScriptView {
    /// Each event handler, script body and task.
    #[default]
    Work,
    /// Each script function, sampled.
    Functions,
}

/// The Scripts tab's state, kept in [`Profiler`].
#[derive(Default)]
pub struct ScriptTab {
    /// Whether the scripts are being recorded; kept across Plays.
    pub recording: bool,
    pub view: ScriptView,
    pub profile: ScriptProfile,
    fetched: Option<Instant>,
    clear: bool,
    open: Option<(String, usize)>,
}

impl ScriptTab {
    /// A new Play's tab: still recording if this one was, with nothing
    /// recorded.
    pub(super) fn for_next_play(&self) -> Self {
        ScriptTab {
            recording: self.recording,
            view: self.view,
            ..ScriptTab::default()
        }
    }
}

impl Profiler {
    /// Whether the script runtime should be recording now.
    pub fn scripts_recording(&self) -> bool {
        self.scripts.recording && self.is_live()
    }

    /// Whether it's time to hand over the runtime's rows
    /// ([`Self::record_scripts`]).
    pub fn scripts_due(&self) -> bool {
        self.scripts_recording()
            && self
                .scripts
                .fetched
                .is_none_or(|fetched| fetched.elapsed() >= FETCH_EVERY)
    }

    /// Presses Record (or Stop recording).
    #[cfg(test)]
    pub fn set_scripts_recording(&mut self, on: bool) {
        self.scripts.recording = on;
        self.scripts.fetched = None;
    }

    /// What the Scripts tab shows.
    #[cfg(test)]
    pub fn script_profile(&self) -> &ScriptProfile {
        &self.scripts.profile
    }

    pub fn record_scripts(&mut self, profile: ScriptProfile) {
        self.scripts.profile = profile;
        self.scripts.fetched = Some(Instant::now());
    }

    /// Whether Clear was pressed since last asked; the host clears the
    /// runtime's recording too.
    pub fn take_clear_scripts(&mut self) -> bool {
        std::mem::take(&mut self.scripts.clear)
    }

    /// A script and line to open, from a clicked row.
    pub fn take_open(&mut self) -> Option<(String, usize)> {
        self.scripts.open.take()
    }

    /// The Scripts tab.
    pub(super) fn scripts_tab(&mut self, ui: &mut egui::Ui) {
        let palette = theme::palette(ui.ctx());
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            let tab = &mut self.scripts;
            let (icon, label, hover) = if tab.recording {
                (
                    theme::icons::STOP,
                    "Stop recording",
                    "Keep what was recorded and stop sampling the scripts",
                )
            } else {
                (
                    theme::icons::RECORD,
                    "Record",
                    "Time each event handler, script and task, and sample which script \
                     functions the time goes to. Costs the game a little time while on.",
                )
            };
            if ui
                .button(theme::with_icon(icon, label))
                .on_hover_text(hover)
                .clicked()
            {
                tab.recording = !tab.recording;
                tab.fetched = None;
            }
            if ui
                .add_enabled(
                    !tab.profile.work.is_empty() || !tab.profile.functions.is_empty(),
                    egui::Button::new(theme::with_icon(theme::icons::BROOM, "Clear")),
                )
                .on_hover_text("Forget what was recorded and start again")
                .clicked()
            {
                tab.profile = ScriptProfile::default();
                tab.clear = true;
                tab.fetched = None;
            }
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let tab = &mut self.scripts;
            widgets::segmented(
                ui,
                &mut tab.view,
                &[
                    (ScriptView::Work, None, "Handlers"),
                    (ScriptView::Functions, None, "Functions"),
                ],
            );
        });
        let tab = &mut self.scripts;
        if tab.profile.work.is_empty() {
            ui.add_space(6.0);
            let hint = if tab.recording {
                "Recording. Play the game; the costliest handlers and functions come first."
            } else {
                "Press Record, then play: each event handler, script and task is timed, and \
                 the time is traced to the script functions it's spent in. Click a row to open \
                 its script."
            };
            ui.label(RichText::new(hint).color(palette.text_secondary));
            return;
        }
        ui.add_space(4.0);
        ui.label(
            RichText::new(format!(
                "{} of script time recorded{}",
                millis(tab.profile.total),
                if tab.recording { "" } else { " (stopped)" }
            ))
            .color(palette.text_secondary)
            .size(type_scale::BODY - 1.0),
        );
        ui.add_space(4.0);
        let total = tab.profile.total.max(Duration::from_nanos(1));
        let mut open = None;
        match tab.view {
            ScriptView::Work => work_rows(ui, &tab.profile, total, &mut open),
            ScriptView::Functions => function_rows(ui, &tab.profile, total, &mut open),
        }
        if open.is_some() {
            tab.open = open;
        }
    }
}

fn kind_text(kind: WorkKind) -> &'static str {
    match kind {
        WorkKind::Script => "Script",
        WorkKind::Handler => "Handler",
        WorkKind::Task => "Task",
        WorkKind::Other => "Other",
    }
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn share(time: Duration, total: Duration) -> String {
    format!("{:.0}%", 100.0 * time.as_secs_f64() / total.as_secs_f64())
}

/// One row: its name (a link when it has a script to open), what it is,
/// then its numbers over a bar of its share. Returns whether the name was
/// clicked.
fn entry(ui: &mut egui::Ui, row: Row<'_>, share_of_total: f32, numbers: String) -> bool {
    let palette = theme::palette(ui.ctx());
    let text = RichText::new(row.title).color(palette.text);
    let clicked = if row.path.is_empty() {
        ui.add(egui::Label::new(text).truncate())
            .on_hover_text(row.hover)
            .clicked()
    } else {
        ui.add(
            egui::Label::new(text)
                .sense(egui::Sense::click())
                .truncate(),
        )
        .on_hover_text(format!(
            "{}\nClick to open {}:{}",
            row.hover,
            file_name(row.path),
            row.line
        ))
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
    };
    ui.add(
        egui::Label::new(
            RichText::new(row.detail)
                .color(palette.text_weak)
                .size(type_scale::BODY - 2.0),
        )
        .truncate(),
    );
    // The share of all script time, as a bar the width of the column.
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 3.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 1.0, palette.inset);
    let mut filled = rect;
    filled.set_width(rect.width() * share_of_total.clamp(0.0, 1.0));
    ui.painter().rect_filled(filled, 1.0, palette.accent);
    ui.label(
        RichText::new(numbers)
            .color(palette.text_secondary)
            .size(type_scale::BODY - 1.0),
    );
    ui.add_space(6.0);
    clicked && !row.path.is_empty()
}

struct Row<'a> {
    title: &'a str,
    detail: String,
    path: &'a str,
    line: usize,
    hover: String,
}

fn ratio(time: Duration, total: Duration) -> f32 {
    (time.as_secs_f64() / total.as_secs_f64()) as f32
}

fn work_rows(
    ui: &mut egui::Ui,
    profile: &ScriptProfile,
    total: Duration,
    open: &mut Option<(String, usize)>,
) {
    for row in profile.work.iter().take(ROWS) {
        let function = row.function.as_deref().unwrap_or("function");
        let detail = match row.kind {
            WorkKind::Script => format!("Script · {}", side(row.client)),
            WorkKind::Other => side(row.client).to_owned(),
            kind if row.path.is_empty() => format!("{} · {}", kind_text(kind), side(row.client)),
            kind => format!(
                "{} · {function} · {}:{} · {}",
                kind_text(kind),
                file_name(&row.path),
                row.line,
                side(row.client)
            ),
        };
        let hover = format!(
            "Called {} times, resumed {} times. Longest run {}.",
            row.calls,
            row.resumes,
            millis(row.longest)
        );
        let line = row.line.max(1) as usize;
        let per_call = row.time / row.calls.max(1);
        let numbers = format!(
            "{} · {} of script time · {} a call",
            millis(row.time),
            share(row.time, total),
            millis(per_call)
        );
        let shown = Row {
            title: &row.what,
            detail,
            path: &row.path,
            line,
            hover,
        };
        if entry(ui, shown, ratio(row.time, total), numbers) {
            *open = Some((row.path.clone(), line));
        }
    }
}

fn function_rows(
    ui: &mut egui::Ui,
    profile: &ScriptProfile,
    total: Duration,
    open: &mut Option<(String, usize)>,
) {
    if profile.functions.is_empty() {
        ui.label("No script ran long enough to be sampled yet.");
        return;
    }
    for row in profile.functions.iter().take(ROWS) {
        let title = match (&row.name, row.line) {
            (Some(name), _) => name.clone(),
            (None, 0) => "Script body".to_owned(),
            (None, line) => format!("function on line {line}"),
        };
        let detail = format!(
            "{}:{} · busiest line {} · {}",
            file_name(&row.path),
            row.line,
            row.hottest_line,
            side(row.client)
        );
        let hover = "Own time: spent in this function's own lines (and the engine functions \
                     it calls). With calls: also in the script functions it calls."
            .to_owned();
        let line = row.hottest_line.max(1) as usize;
        let numbers = format!(
            "{} own ({}) · {} with calls",
            millis(row.self_time),
            share(row.self_time, total),
            millis(row.total_time)
        );
        let shown = Row {
            title: &title,
            detail,
            path: &row.path,
            line,
            hover,
        };
        if entry(ui, shown, ratio(row.self_time, total), numbers) {
            *open = Some((row.path.clone(), line));
        }
    }
}

fn side(client: bool) -> &'static str {
    if client { "client" } else { "server" }
}
