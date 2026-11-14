//! Model import presets in the Import dialog (P7.3): the Preset row above
//! the tabs, with Save as preset…, and the Settings tab (scale, up axis,
//! Anchored and collision). The presets themselves are
//! `axcenity_assets::model_presets`.

use axcenity_assets::{BringIn, ImportCollision, ModelSettings, UpAxis};
use eframe::egui;

use super::{ImportDialog, Placement};
use crate::theme;

impl ImportDialog {
    /// Makes the dialog's choices the preset `name`'s (`settings`). Bring
    /// in as only changes a file with more than one object; By file is
    /// what the dialog picks without a preset.
    pub fn apply_preset(&mut self, name: &str, settings: ModelSettings) {
        self.preset = name.to_owned();
        self.settings = settings;
        if self.pick_objects {
            self.placement = match settings.bring_in {
                BringIn::ByFile if self.animation.is_some() => Placement::Merged,
                BringIn::ByFile => Placement::default(),
                BringIn::Model => Placement::Grouped,
                BringIn::Separate => Placement::Separate,
                BringIn::Merged => Placement::Merged,
            };
        }
        self.weld = settings.weld;
        if let Some(animation) = &mut self.animation {
            animation.add_animator = settings.animator;
        }
    }

    /// The dialog's choices as a preset, for Save as preset….
    pub fn current_settings(&self) -> ModelSettings {
        let bring_in = if self.pick_objects {
            match self.placement {
                Placement::Grouped => BringIn::Model,
                Placement::Separate => BringIn::Separate,
                Placement::Merged => BringIn::Merged,
            }
        } else {
            self.settings.bring_in
        };
        ModelSettings {
            bring_in,
            weld: self.weld,
            animator: self
                .animation
                .as_ref()
                .map_or(self.settings.animator, |animation| animation.add_animator),
            ..self.settings
        }
    }

    /// Whether the choices are still the picked preset's.
    pub fn matches_preset(&self) -> bool {
        self.presets
            .iter()
            .find(|(name, _)| *name == self.preset)
            .is_some_and(|(_, settings)| {
                self.as_placed(*settings) == self.as_placed(self.current_settings())
            })
    }

    /// `settings` with By file turned into what it places this file as.
    fn as_placed(&self, mut settings: ModelSettings) -> ModelSettings {
        if self.pick_objects && settings.bring_in == BringIn::ByFile {
            settings.bring_in = if self.animation.is_some() {
                BringIn::Merged
            } else {
                BringIn::Model
            };
        }
        settings
    }
}

/// The Preset row: a menu of the presets (each says what it does on hover),
/// "(changed)" once a choice differs from it, and Save as preset….
pub(super) fn preset_row(dialog: &mut ImportDialog, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.label("Preset");
        let changed = !dialog.matches_preset();
        let shown = if changed {
            format!("{} (changed)", dialog.preset)
        } else {
            dialog.preset.clone()
        };
        let mut picked = None;
        egui::ComboBox::from_id_salt("import model preset")
            .selected_text(shown)
            .show_ui(ui, |ui| {
                for (name, settings) in &dialog.presets {
                    if ui
                        .selectable_label(*name == dialog.preset && !changed, name)
                        .on_hover_text(settings.describe())
                        .clicked()
                    {
                        picked = Some((name.clone(), *settings));
                    }
                }
            });
        if let Some((name, settings)) = picked {
            dialog.apply_preset(&name, settings);
        }
        match &mut dialog.saving {
            None => {
                if ui
                    .button("Save as preset…")
                    .on_hover_text(
                        "Keep these choices under a name, in the project's \
                         import-presets.json, to pick for the next models.",
                    )
                    .clicked()
                {
                    dialog.saving = Some(if changed {
                        String::new()
                    } else {
                        dialog.preset.clone()
                    });
                }
            }
            Some(name) => {
                let field = ui.add(
                    egui::TextEdit::singleline(name)
                        .hint_text("Preset name")
                        .desired_width(160.0),
                );
                let name = name.trim().to_owned();
                let enter =
                    field.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
                let save = ui.add_enabled(!name.is_empty(), egui::Button::new("Save"));
                if (save.clicked() || enter) && !name.is_empty() {
                    let settings = dialog.current_settings();
                    dialog.save_preset = Some((name.clone(), settings));
                    match dialog.presets.iter_mut().find(|(known, _)| *known == name) {
                        Some((_, known)) => *known = settings,
                        None => dialog.presets.push((name.clone(), settings)),
                    }
                    dialog.preset = name;
                    dialog.saving = None;
                } else if ui.button("Cancel").clicked() {
                    dialog.saving = None;
                }
            }
        }
    });
}

/// The Settings tab: scale, up axis, Anchored and collision.
pub(super) fn settings(dialog: &mut ImportDialog, ui: &mut egui::Ui) {
    let palette = theme::palette(ui.ctx());
    let hint = |ui: &mut egui::Ui, text: &str| {
        ui.label(egui::RichText::new(text).color(palette.text_secondary));
    };
    let settings = &mut dialog.settings;
    hint(
        ui,
        "Applied to every part the import makes. A preset holds these too.",
    );
    ui.add_space(8.0);
    egui::Grid::new("import model settings")
        .num_columns(2)
        .spacing([12.0, 8.0])
        .show(ui, |ui| {
            ui.label("Scale");
            ui.horizontal(|ui| {
                ui.add(
                    egui::DragValue::new(&mut settings.scale)
                        .range(0.0001..=10000.0)
                        .speed(0.01)
                        .max_decimals(4),
                )
                .on_hover_text(
                    "Every size and distance times this. glTF, FBX and USD files already \
                     come in at their real size; use 0.01 for a file in centimetres that \
                     doesn't say so.",
                );
                if ui.small_button("1").clicked() {
                    settings.scale = 1.0;
                }
                if ui
                    .small_button("0.01")
                    .on_hover_text("Centimetres to studs")
                    .clicked()
                {
                    settings.scale = 0.01;
                }
            });
            ui.end_row();

            ui.label("Up axis");
            ui.horizontal(|ui| {
                ui.radio_value(&mut settings.up_axis, UpAxis::Y, "Y")
                    .on_hover_text("As glTF says, and as FBX and USD files are turned on load.");
                ui.radio_value(&mut settings.up_axis, UpAxis::Z, "Z")
                    .on_hover_text(
                        "For a file exported with Z up that doesn't say so (some OBJ and glTF \
                     exports): it's turned a quarter round so Z points up.",
                    );
            });
            ui.end_row();

            ui.label("Anchored");
            ui.checkbox(&mut settings.anchored, "")
                .on_hover_text("Parts stay where they're put: scenery, buildings, props.");
            ui.end_row();

            ui.label("Collision");
            egui::ComboBox::from_id_salt("import collision")
                .selected_text(settings.collision.item())
                .show_ui(ui, |ui| {
                    for collision in ImportCollision::ALL {
                        ui.selectable_value(&mut settings.collision, collision, collision.item());
                    }
                })
                .response
                .on_hover_text(
                    "Each part's CollisionFidelity. Default uses the file's _COL pieces, \
                     else the part's hull; Box is cheapest; PreciseConvexDecomposition fits \
                     concave shapes such as rooms and arches.",
                );
            ui.end_row();
        });
}
