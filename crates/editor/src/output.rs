//! The Output panel: messages from the editor, such as which project opened
//! and why a file couldn't load, and during Play what scripts `print`, their
//! errors, and engine crashes.
//!
//! Script errors start with `path:line:`. That part is underlined, and
//! clicking the line opens the script at that line.

use eframe::egui;

use crate::theme::{self, icons};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputKind {
    Info,
    Warning,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputLine {
    pub kind: OutputKind,
    pub text: String,
    /// Where in which script an error happened, if the line starts with
    /// `path:line:`.
    pub location: Option<Location>,
}

/// A line in a script file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Location {
    /// The script's path, such as `scripts/pad.server.lua`.
    pub path: String,
    /// Counting from 1.
    pub line: usize,
}

#[derive(Default)]
pub struct Output {
    lines: Vec<OutputLine>,
}

impl Output {
    pub fn info(&mut self, text: impl Into<String>) {
        self.push(OutputKind::Info, text.into());
    }

    pub fn error(&mut self, text: impl Into<String>) {
        self.push(OutputKind::Error, text.into());
    }

    pub fn warning(&mut self, text: impl Into<String>) {
        self.push(OutputKind::Warning, text.into());
    }

    fn push(&mut self, kind: OutputKind, text: String) {
        // Also log it, so messages show up in the terminal when the editor
        // was started from one (handy when the window won't open).
        match kind {
            OutputKind::Info => log::info!("{text}"),
            OutputKind::Warning => log::warn!("{text}"),
            OutputKind::Error => log::error!("{text}"),
        }
        // Only errors link to scripts: a `print` that happens to look like
        // `x.lua:3:` shouldn't.
        let location = match kind {
            OutputKind::Error => script_location(&text),
            OutputKind::Info | OutputKind::Warning => None,
        };
        self.lines.push(OutputLine {
            kind,
            text,
            location,
        });
    }

    #[cfg(test)]
    pub fn lines(&self) -> &[OutputLine] {
        &self.lines
    }

    /// Draws the panel. Returns the location of a script error that was
    /// clicked, to open the script there.
    pub fn show(&mut self, ui: &mut egui::Ui) -> Option<Location> {
        ui.horizontal(|ui| {
            theme::panel_title(ui, icons::TERMINAL_WINDOW, "Output");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .small_button(theme::with_icon(icons::BROOM, "Clear"))
                    .on_hover_text("Remove every line")
                    .clicked()
                {
                    self.lines.clear();
                }
            });
        });
        ui.separator();

        let mut clicked = None;
        egui::ScrollArea::vertical()
            // Keep showing the newest line, unless the user scrolled up to read.
            .stick_to_bottom(true)
            .auto_shrink(false)
            .show(ui, |ui| {
                for line in &self.lines {
                    let visuals = ui.visuals();
                    let (icon, icon_color, color) = match line.kind {
                        OutputKind::Info => {
                            (icons::INFO, visuals.weak_text_color(), visuals.text_color())
                        }
                        OutputKind::Warning => {
                            (icons::INFO, visuals.warn_fg_color, visuals.warn_fg_color)
                        }
                        OutputKind::Error => (
                            icons::X_CIRCLE,
                            visuals.error_fg_color,
                            visuals.error_fg_color,
                        ),
                    };
                    let font = egui::TextStyle::Monospace.resolve(ui.style());
                    // The icon sits left of the text, which wraps beside it.
                    ui.horizontal_top(|ui| {
                        ui.label(
                            egui::RichText::new(icon)
                                .font(font.clone())
                                .color(icon_color),
                        );
                        match &line.location {
                            Some(location) => {
                                if location_link(ui, &line.text, location, font, color) {
                                    clicked = Some(location.clone());
                                }
                            }
                            None => {
                                let text = egui::RichText::new(&line.text).font(font).color(color);
                                ui.add(egui::Label::new(text).wrap());
                            }
                        }
                    });
                }
            });
        clicked
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
/// `<path>.lua:<line>:`, as script errors do (see `crates/script/src/errors.rs`).
pub fn script_location(text: &str) -> Option<Location> {
    let (path, rest) = text.split_once(".lua:")?;
    // A path is one relative file name; this keeps out sentences that
    // happen to contain ".lua:" further along.
    if path.is_empty() || path.contains([':', '\n', '`', '"']) {
        return None;
    }
    let (line, _) = rest.split_once(':')?;
    let line = line.parse().ok().filter(|&line| line > 0)?;
    Some(Location {
        path: format!("{path}.lua"),
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
    fn only_errors_are_links() {
        let mut output = Output::default();
        output.info("scripts/a.server.lua:1: printed by a script");
        output.error("scripts/a.server.lua:2: boom");
        assert_eq!(output.lines()[0].location, None);
        assert_eq!(output.lines()[1].location.as_ref().unwrap().line, 2);
    }
}
