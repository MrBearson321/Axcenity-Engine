//! View › Layout (design system V17): switch to a preset or a saved
//! layout, save the current one by name, delete saved ones. The layouts
//! themselves are in `layout/presets.rs`.

use super::*;
use crate::dialog::{self, Button, Enter, Kind, Tone};
use crate::layout::{Preset, SavedLayouts};

/// The name being typed in Save Current Layout…, and why the last one was
/// refused.
#[derive(Default)]
pub(super) struct LayoutName {
    pub text: String,
    pub problem: Option<&'static str>,
}

impl EditorApp {
    /// The Layout submenu: Default and the presets, then the saved ones
    /// (each with ×), then Save Current Layout….
    pub(super) fn layout_menu(&mut self, ui: &mut egui::Ui) {
        for preset in Preset::ALL {
            if ui
                .button(theme::with_icon(preset.icon(), preset.label()))
                .on_hover_text(preset.about())
                .clicked()
            {
                self.layout.use_preset(preset);
                ui.close();
            }
        }
        if !self.saved_layouts.0.is_empty() {
            ui.separator();
        }
        let mut delete = None;
        for (name, _) in &self.saved_layouts.0 {
            ui.horizontal(|ui| {
                if ui
                    .button(theme::with_icon(icons::BOOKMARK_SIMPLE, name))
                    .on_hover_text("Switch to this saved layout.")
                    .clicked()
                {
                    self.pending_layout = Some(name.clone());
                    ui.close();
                }
                if widgets::icon_button(ui, icons::X, "Delete this saved layout", false).clicked() {
                    delete = Some(name.clone());
                }
            });
        }
        if let Some(name) = delete {
            self.saved_layouts.delete(&name);
        }
        if let Some(name) = self.pending_layout.take() {
            self.use_saved_layout(&name);
        }
        ui.separator();
        if ui
            .button(theme::with_icon(icons::FLOPPY_DISK, "Save Current Layout…"))
            .on_hover_text("Keep where the panels are now, by name, to switch back to later.")
            .clicked()
        {
            self.layout_name = Some(LayoutName::default());
            ui.close();
        }
    }

    pub(super) fn use_saved_layout(&mut self, name: &str) {
        if let Some(text) = self.saved_layouts.get(name) {
            let text = text.to_owned();
            self.layout.use_saved(&text);
        }
    }

    /// Saves the current layout as `name`, or says why it can't.
    pub(super) fn save_layout(&mut self, name: &str) -> Result<(), &'static str> {
        self.saved_layouts.save(name, &self.layout)
    }

    /// The Save Current Layout… box, while it's open.
    pub(super) fn save_layout_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut name) = self.layout_name.take() else {
            return;
        };
        let mut close = false;
        let mut save = false;
        let ((), dismissed) = dialog::modal(ctx, "save layout", dialog::WIDTH, |ui| {
            let palette = theme::palette(ui.ctx());
            dialog::header(
                ui,
                icons::FLOPPY_DISK,
                Tone::Question,
                "Save Current Layout",
                Some("It's listed in View › Layout. Saving with a name already there replaces it."),
            );
            let field = ui.add(dialog::text_field(&mut name.text).hint_text("Name"));
            if name.text.is_empty() && !field.has_focus() {
                field.request_focus();
            }
            if let Some(problem) = name.problem {
                ui.colored_label(palette.error, problem);
            }
            match dialog::buttons(
                ui,
                &[
                    Button::cancel(),
                    Button::new(icons::FLOPPY_DISK, "Save", Kind::Main),
                ],
                Enter::FromField,
            ) {
                Some(1) => save = true,
                Some(_) => close = true,
                None => {}
            }
        });
        if save {
            match self.save_layout(&name.text) {
                Ok(()) => close = true,
                Err(problem) => name.problem = Some(problem),
            }
        }
        if !close && !dismissed {
            self.layout_name = Some(name);
        }
    }
}

/// Where saved layouts are kept in eframe's storage.
pub(super) const SAVED_LAYOUTS_KEY: &str = "saved layouts";

pub(super) fn load_saved_layouts(storage: &dyn eframe::Storage) -> SavedLayouts {
    storage
        .get_string(SAVED_LAYOUTS_KEY)
        .map(|text| SavedLayouts::from_text(&text))
        .unwrap_or_default()
}
