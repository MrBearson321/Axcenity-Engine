//! The Output panel: messages from the editor, such as which project opened
//! and why one couldn't load. Script `print`s and errors join them once Play
//! arrives.

use eframe::egui;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputKind {
    Info,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputLine {
    pub kind: OutputKind,
    pub text: String,
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

    fn push(&mut self, kind: OutputKind, text: String) {
        // Also log it, so messages show up in the terminal when the editor
        // was started from one (handy when the window won't open).
        match kind {
            OutputKind::Info => log::info!("{text}"),
            OutputKind::Error => log::error!("{text}"),
        }
        self.lines.push(OutputLine { kind, text });
    }

    #[cfg(test)]
    pub fn lines(&self) -> &[OutputLine] {
        &self.lines
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.strong("Output");
            if ui.small_button("Clear").clicked() {
                self.lines.clear();
            }
        });
        ui.separator();

        egui::ScrollArea::vertical()
            // Keep showing the newest line, unless the user scrolled up to read.
            .stick_to_bottom(true)
            .auto_shrink(false)
            .show(ui, |ui| {
                for line in &self.lines {
                    let visuals = ui.visuals();
                    let color = match line.kind {
                        OutputKind::Info => visuals.text_color(),
                        OutputKind::Error => visuals.error_fg_color,
                    };
                    let text = egui::RichText::new(&line.text).monospace().color(color);
                    ui.add(egui::Label::new(text).wrap());
                }
            });
    }
}
