//! The Console's log: messages from the editor, such as which project opened
//! and why a file couldn't load, during Play what scripts `print`, their
//! errors, and engine crashes, and the commands typed into the Console
//! (`console.rs`) with what they printed.
//!
//! Script errors start with `path:line:`. That part is underlined, and
//! clicking the line opens the script at that line. Engine compile errors
//! (`crates/…/x.rs:line:column:`) link the same way, to the Rust file. Under a script error is
//! its stack trace: the calls that led to it, innermost first, each a link
//! to its own script and line.

use eframe::egui;

use crate::theme::{self, icons};
use crate::widgets;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputKind {
    Info,
    Warning,
    Error,
    /// A command typed into the Console, echoed. Always shown.
    Command,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputLine {
    pub kind: OutputKind,
    pub text: String,
    /// Where in which script an error happened, if the line starts with
    /// `path:line:`.
    pub location: Option<Location>,
    /// For a script error, the calls that led to it, innermost first.
    pub trace: Vec<TraceLine>,
}

/// One call in a script error's stack trace, such as
/// `scripts/util.module.lua:5: in field 'explode'`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceLine {
    pub text: String,
    pub location: Option<Location>,
}

/// A line in a script file, or in an engine source file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Location {
    /// The script's path, such as `scripts/pad.server.lua`, or the engine
    /// file's in its workspace, such as `crates/editor/src/app.rs`.
    pub path: String,
    /// Counting from 1.
    pub line: usize,
}

#[derive(Default)]
pub struct Output {
    lines: Vec<OutputLine>,
    /// Kinds of line switched off with the filter buttons. Only what's
    /// shown changes; every line is kept.
    hidden: Vec<OutputKind>,
    /// How many lines there were when the panel was last drawn: errors
    /// after that haven't been seen.
    seen: usize,
    /// The frame (`theme::frame_number`) the panel was last drawn in.
    drawn_pass: Option<u64>,
    /// A line to scroll to the next time the panel is drawn.
    reveal: Option<usize>,
    /// The line last scrolled to, outlined so the eye finds it.
    marked: Option<usize>,
}

impl Output {
    pub fn info(&mut self, text: impl Into<String>) {
        self.push(OutputKind::Info, text.into());
    }

    pub fn error(&mut self, text: impl Into<String>) {
        self.push(OutputKind::Error, text.into());
    }

    /// A script error, with the stack trace of calls that led to it.
    pub fn script_error(&mut self, text: impl Into<String>, trace: Vec<String>) {
        self.push_with_trace(OutputKind::Error, text.into(), trace);
    }

    pub fn warning(&mut self, text: impl Into<String>) {
        self.push(OutputKind::Warning, text.into());
    }

    pub fn command(&mut self, text: impl Into<String>) {
        self.push(OutputKind::Command, text.into());
    }

    /// Removes every line.
    pub fn clear(&mut self) {
        self.lines.clear();
        self.seen = 0;
        self.reveal = None;
        self.marked = None;
    }

    /// Errors added since the panel was last drawn, or 0 while it's
    /// showing (drawn last frame), for a count on its tab.
    pub fn unseen_errors(&self, ctx: &egui::Context) -> usize {
        let showing = self
            .drawn_pass
            .is_some_and(|pass| pass + 1 >= theme::frame_number(ctx));
        if showing {
            return 0;
        }
        self.lines[self.seen.min(self.lines.len())..]
            .iter()
            .filter(|line| line.kind == OutputKind::Error)
            .count()
    }

    /// Scrolls to the newest error (showing errors if they were filtered
    /// out) the next time the panel is drawn.
    pub fn reveal_last_error(&mut self) {
        self.reveal = self
            .lines
            .iter()
            .rposition(|line| line.kind == OutputKind::Error);
        if self.reveal.is_some() {
            self.hidden.retain(|kind| *kind != OutputKind::Error);
        }
    }

    fn push(&mut self, kind: OutputKind, text: String) {
        self.push_with_trace(kind, text, Vec::new());
    }

    fn push_with_trace(&mut self, kind: OutputKind, text: String, trace: Vec<String>) {
        // Also log it, so messages show up in the terminal when the editor
        // was started from one (handy when the window won't open).
        match kind {
            OutputKind::Info | OutputKind::Command => log::info!("{text}"),
            OutputKind::Warning => log::warn!("{text}"),
            OutputKind::Error => {
                log::error!("{text}");
                for frame in &trace {
                    log::error!("    {frame}");
                }
            }
        }
        // Only errors link to scripts: a `print` that happens to look like
        // `x.lua:3:` shouldn't.
        let location = match kind {
            OutputKind::Error => script_location(&text),
            OutputKind::Info | OutputKind::Warning | OutputKind::Command => None,
        };
        let trace = trace
            .into_iter()
            .map(|text| TraceLine {
                location: script_location(&text),
                text,
            })
            .collect();
        self.lines.push(OutputLine {
            kind,
            text,
            location,
            trace,
        });
    }

    pub fn lines(&self) -> &[OutputLine] {
        &self.lines
    }

    /// Draws the panel. Returns the location of a script error that was
    /// clicked, to open the script there.
    pub fn show(&mut self, ui: &mut egui::Ui) -> Option<Location> {
        let palette = theme::palette(ui.ctx());
        self.seen = self.lines.len();
        self.drawn_pass = Some(theme::frame_number(ui.ctx()));
        let reveal = self.reveal.take();
        if reveal.is_some() {
            self.marked = reveal;
        }
        let marked = self.marked;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for (kind, icon, color, name) in [
                (
                    OutputKind::Info,
                    icons::INFO,
                    palette.text_secondary,
                    "messages",
                ),
                (
                    OutputKind::Warning,
                    icons::WARNING,
                    palette.warning,
                    "warnings",
                ),
                (OutputKind::Error, icons::X_CIRCLE, palette.error, "errors"),
            ] {
                let count = self.lines.iter().filter(|line| line.kind == kind).count();
                let shown = !self.hidden.contains(&kind);
                if filter_toggle(ui, icon, color, count, shown)
                    .on_hover_text(if shown {
                        format!("Hide {name}")
                    } else {
                        format!("Show {name}")
                    })
                    .clicked()
                {
                    if shown {
                        self.hidden.push(kind);
                    } else {
                        self.hidden.retain(|hidden| *hidden != kind);
                    }
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(
                        !self.lines.is_empty(),
                        widgets::button(
                            widgets::Role::Ghost,
                            Some(icons::BROOM),
                            "Clear",
                            &palette,
                        )
                        .min_size(egui::vec2(0.0, 22.0)),
                    )
                    .on_hover_text("Remove every line")
                    .clicked()
                {
                    self.clear();
                }
            });
        });
        ui.add_space(2.0);
        widgets::divider(ui);

        let mut clicked = None;
        egui::ScrollArea::vertical()
            // Keep showing the newest line, unless the user scrolled up to read.
            .stick_to_bottom(true)
            .auto_shrink(false)
            .show(ui, |ui| {
                ui.add_space(4.0);
                ui.spacing_mut().item_spacing.y = 2.0;
                if self.lines.is_empty() {
                    ui.label(
                        egui::RichText::new(
                            "Nothing yet. Editor messages, and what scripts print during Play, \
                             show here. Type Lua or a command (try help) below.",
                        )
                        .color(palette.text_weak),
                    );
                }
                for (index, line) in self
                    .lines
                    .iter()
                    .enumerate()
                    .filter(|(_, line)| !self.hidden.contains(&line.kind))
                {
                    let (icon, icon_color, color) = match line.kind {
                        OutputKind::Info => (icons::INFO, palette.text_weak, palette.text),
                        OutputKind::Warning => (icons::WARNING, palette.warning, palette.warning),
                        OutputKind::Error => (icons::X_CIRCLE, palette.error, palette.error),
                        OutputKind::Command => {
                            (icons::CARET_RIGHT, palette.accent_text, palette.text_strong)
                        }
                    };
                    let font = egui::TextStyle::Monospace.resolve(ui.style());
                    // The icon sits left of the text, which wraps beside it.
                    let row = ui.horizontal_top(|ui| {
                        ui.label(
                            egui::RichText::new(icon)
                                .font(egui::FontId::proportional(font.size))
                                .color(icon_color),
                        );
                        // The stack trace goes under the text, beside the icon.
                        ui.vertical(|ui| {
                            if let Some(location) =
                                text_or_link(ui, &line.text, &line.location, font.clone(), color)
                            {
                                clicked = Some(location);
                            }
                            for frame in &line.trace {
                                ui.horizontal_top(|ui| {
                                    ui.add_space(12.0);
                                    if let Some(location) = text_or_link(
                                        ui,
                                        &frame.text,
                                        &frame.location,
                                        font.clone(),
                                        palette.text_weak,
                                    ) {
                                        clicked = Some(location);
                                    }
                                });
                            }
                        });
                    });
                    if reveal == Some(index) {
                        ui.scroll_to_rect(row.response.rect, Some(egui::Align::Center));
                    }
                    if marked == Some(index) {
                        ui.painter().rect_stroke(
                            row.response.rect.expand(2.0),
                            theme::radius::CONTROL,
                            egui::Stroke::new(1.0_f32, palette.error),
                            egui::StrokeKind::Outside,
                        );
                    }
                }
            });
        clicked
    }
}

/// A filter button: the kind's icon and how many lines of it there are,
/// dimmed while that kind is hidden.
fn filter_toggle(
    ui: &mut egui::Ui,
    icon: &str,
    color: egui::Color32,
    count: usize,
    shown: bool,
) -> egui::Response {
    let palette = theme::palette(ui.ctx());
    let text = count.to_string();
    let galley = ui.painter().layout_no_wrap(
        text,
        egui::FontId::new(theme::type_scale::SMALL, theme::medium_family()),
        egui::Color32::PLACEHOLDER,
    );
    let size = egui::vec2(galley.size().x + 34.0, 22.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    response
        .widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, shown, icon));
    widgets::paint_hover(ui, rect, &response, false, theme::radius::CONTROL);
    let (icon_color, text_color) = if shown {
        (color, palette.text)
    } else {
        (palette.text_muted, palette.text_muted)
    };
    ui.painter().text(
        rect.left_center() + egui::vec2(13.0, 0.0),
        egui::Align2::CENTER_CENTER,
        icon,
        egui::FontId::proportional(13.0),
        icon_color,
    );
    ui.painter().galley(
        egui::pos2(rect.left() + 24.0, rect.center().y - galley.size().y / 2.0),
        galley,
        text_color,
    );
    response
}

/// `text`, as a link if it has a script location. Returns the location
/// when it's clicked.
fn text_or_link(
    ui: &mut egui::Ui,
    text: &str,
    location: &Option<Location>,
    font: egui::FontId,
    color: egui::Color32,
) -> Option<Location> {
    match location {
        Some(location) => location_link(ui, text, location, font, color).then(|| location.clone()),
        None => {
            let text = egui::RichText::new(text).font(font).color(color);
            ui.add(egui::Label::new(text).wrap());
            None
        }
    }
}

/// A script error's text, with the `path:line` it starts with underlined
/// like a link, then the message. Returns true when it's clicked.
fn location_link(
    ui: &mut egui::Ui,
    text: &str,
    location: &Location,
    font: egui::FontId,
    color: egui::Color32,
) -> bool {
    let prefix = format!("{}:{}", location.path, location.line);
    let rest = text.strip_prefix(&prefix).unwrap_or(text);
    let mut job = egui::text::LayoutJob::default();
    let mut format = egui::TextFormat::simple(font, color);
    format.underline = egui::Stroke::new(1.0_f32, color);
    job.append(&prefix, 0.0, format.clone());
    format.underline = egui::Stroke::NONE;
    job.append(rest, 0.0, format);
    ui.add(egui::Label::new(job).wrap().sense(egui::Sense::click()))
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(format!("Open {} at line {}", location.path, location.line))
        .clicked()
}

/// The script and line at the start of `text`, if it starts with
/// `<path>.lua:<line>:`, as script errors do (see `crates/script/src/errors.rs`),
/// or the engine file and line if it starts with `<path>.rs:<line>:`, as
/// compile errors from a rebuild do (see `rebuild.rs`).
pub fn script_location(text: &str) -> Option<Location> {
    file_location(text, ".lua").or_else(|| file_location(text, ".rs"))
}

/// [`script_location`] for files ending in `extension`.
fn file_location(text: &str, extension: &str) -> Option<Location> {
    let (path, rest) = text.split_once(&format!("{extension}:"))?;
    // A path is one relative file name; this keeps out sentences that
    // happen to contain ".lua:" further along.
    if path.is_empty() || path.contains([':', '\n', '`', '"']) {
        return None;
    }
    let (line, _) = rest.split_once(':')?;
    let line = line.parse().ok().filter(|&line| line > 0)?;
    Some(Location {
        path: format!("{path}{extension}"),
        line,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_errors_link_to_their_path_and_line() {
        assert_eq!(
            script_location("scripts/doors/open door.server.lua:12: attempt to index nil"),
            Some(Location {
                path: "scripts/doors/open door.server.lua".to_string(),
                line: 12
            })
        );
        assert_eq!(
            script_location("crates/editor/src/app.rs:12:5: error[E0308]: mismatched types"),
            Some(Location {
                path: "crates/editor/src/app.rs".to_string(),
                line: 12
            })
        );
        for text in [
            "scripts/pad.server.lua won't run: could not read",
            "Could not open `scripts/a.lua:3:`",
            "scripts/pad.server.lua:x: no line",
            "scripts/pad.server.lua:0: no line zero",
            "hello",
        ] {
            assert_eq!(script_location(text), None, "{text}");
        }
    }

    #[test]
    fn warnings_are_visible_but_are_not_script_links() {
        let mut output = Output::default();
        output.warning("ui.css: unsupported CSS property `animation`");

        assert_eq!(output.lines()[0].kind, OutputKind::Warning);
        assert_eq!(output.lines()[0].location, None);
    }

    #[test]
    fn each_call_in_a_stack_trace_links_to_its_script() {
        let mut output = Output::default();
        output.script_error(
            "scripts/Fuse.module.lua:3: boom",
            vec![
                "scripts/Fuse.module.lua:3: in field 'light'".to_string(),
                "scripts/a.server.lua:2: in main chunk".to_string(),
            ],
        );
        let line = &output.lines()[0];
        assert_eq!(line.location.as_ref().unwrap().line, 3);
        let links: Vec<_> = line
            .trace
            .iter()
            .map(|frame| frame.location.clone().unwrap())
            .collect();
        assert_eq!(
            links,
            [
                Location {
                    path: "scripts/Fuse.module.lua".to_string(),
                    line: 3
                },
                Location {
                    path: "scripts/a.server.lua".to_string(),
                    line: 2
                }
            ]
        );
    }

    #[test]
    fn only_errors_are_links() {
        let mut output = Output::default();
        output.info("scripts/a.server.lua:1: printed by a script");
        output.error("scripts/a.server.lua:2: boom");
        assert_eq!(output.lines()[0].location, None);
        assert_eq!(output.lines()[1].location.as_ref().unwrap().line, 2);
    }
}

#[cfg(test)]
mod unseen_tests {
    use super::*;

    fn draw(ctx: &egui::Context, output: &mut Output) {
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                output.show(ui);
            });
        });
    }

    fn hidden_frame(ctx: &egui::Context) {
        let _ = ctx.run(Default::default(), |_| {});
    }

    #[test]
    fn errors_while_the_panel_is_hidden_are_counted_until_it_shows() {
        let ctx = theme::test_context();
        let mut output = Output::default();
        output.error("before");
        draw(&ctx, &mut output);
        // Showing: nothing to count, even for a new error.
        output.error("while shown");
        assert_eq!(output.unseen_errors(&ctx), 0);
        draw(&ctx, &mut output);

        // Hidden: errors (not warnings) count up.
        hidden_frame(&ctx);
        hidden_frame(&ctx);
        output.error("a");
        output.warning("w");
        output.error("b");
        assert_eq!(output.unseen_errors(&ctx), 2);
        draw(&ctx, &mut output);
        hidden_frame(&ctx);
        hidden_frame(&ctx);
        assert_eq!(output.unseen_errors(&ctx), 0);
    }

    #[test]
    fn revealing_the_last_error_shows_errors_again_and_marks_it() {
        let ctx = theme::test_context();
        let mut output = Output::default();
        output.error("first");
        output.info("note");
        output.error("last");
        output.hidden.push(OutputKind::Error);
        output.reveal_last_error();
        assert!(!output.hidden.contains(&OutputKind::Error));
        draw(&ctx, &mut output);
        assert_eq!(output.marked, Some(2));
        assert_eq!(output.reveal, None);
    }
}
