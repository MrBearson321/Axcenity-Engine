//! The arrange bar's game UI settings menu: text scale, higher contrast and
//! reduced motion, as a player would set them (`World::ui_settings`), so
//! the artboard and Scene View show the UI with them. Like the language
//! picker, it isn't saved or an undo step, and Play starts with them. It's
//! the game's UI, never the editor's own theme.

use axcenity_core::{TEXT_SCALE_RANGE, UiSettings};
use eframe::egui;

use super::{DesignerAction, DesignerScene};
use crate::responsive;
use crate::theme::icons;

pub(super) fn settings_menu(
    ui: &mut egui::Ui,
    scene: &DesignerScene,
    actions: &mut Vec<DesignerAction>,
) {
    let current = scene.world.ui_settings();
    responsive::room_for(ui, 40.0);
    ui.menu_button(icons::PERSON_ARMS_SPREAD, |ui| {
        let mut settings = current;
        ui.label("Show the game UI as a player with:");
        let (min, max) = TEXT_SCALE_RANGE;
        ui.add(
            egui::Slider::new(&mut settings.text_scale, min..=max)
                .step_by(0.05)
                .text("Text scale"),
        );
        ui.checkbox(&mut settings.high_contrast, "Higher contrast");
        ui.checkbox(&mut settings.reduced_motion, "Reduced motion");
        if ui
            .add_enabled(
                settings != UiSettings::default(),
                egui::Button::new("Reset"),
            )
            .clicked()
        {
            settings = UiSettings::default();
        }
        if settings != current {
            actions.push(DesignerAction::UiSettings(settings));
        }
    })
    .response
    .on_hover_text("Game UI settings: preview text scale, contrast and reduced motion");
}
