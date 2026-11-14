//! Character Studio's Expressions section: pick an expression (the
//! project's or a built-in one), preview it at an intensity and blended
//! with another, change its controls and save it.

use axcenity_anim::{BUILT_IN_EXPRESSIONS, expression_names, find_expression};
use axcenity_assets::{Assets, EXPRESSION_EXTENSION, EXPRESSIONS_FOLDER, FaceExpression};
use axcenity_core::face::{FACE_CONTROLS, face_control};
use eframe::egui;

use super::{StudioAction, file_name};
use crate::theme::{self, icons};
use crate::widgets;

/// The Expressions section's state.
pub struct ExpressionEditor {
    /// The expression it started from, by name.
    pub picked: Option<String>,
    /// The expression as edited.
    pub draft: FaceExpression,
    /// What Save calls the file.
    pub save_name: String,
    /// How strongly the preview shows it, 0 to 1.
    pub intensity: f32,
    /// Another expression the preview blends in, and how much of it.
    pub blend: Option<String>,
    pub blend_amount: f32,
}

impl Default for ExpressionEditor {
    fn default() -> Self {
        Self {
            picked: None,
            draft: FaceExpression::default(),
            save_name: String::new(),
            intensity: 1.0,
            blend: None,
            blend_amount: 0.5,
        }
    }
}

/// Adds `expression` at `amount` (times its own intensity) to `controls`.
pub fn add_expression(expression: &FaceExpression, amount: f32, controls: &mut [f32]) {
    for (name, &value) in &expression.controls {
        if let Some(index) = face_control(name) {
            controls[index] += value * expression.intensity * amount;
        }
    }
}

impl ExpressionEditor {
    /// Starts from the expression `name` (a copy to change).
    pub fn pick(&mut self, assets: &Assets, name: &str) {
        let project = assets.face_expressions();
        let Some(found) = find_expression(&project, name) else {
            return;
        };
        self.draft = found.clone();
        self.draft.name = String::new();
        self.picked = Some(name.to_owned());
        self.save_name = name.to_owned();
    }

    /// The preview's controls: the draft at the intensity, blended toward
    /// the other expression.
    pub fn controls(&self, assets: &Assets) -> Vec<f32> {
        let mut controls = vec![0.0; FACE_CONTROLS.len()];
        let other = self.blend.as_ref().and_then(|name| {
            let project = assets.face_expressions();
            find_expression(&project, name).cloned()
        });
        let t = if other.is_some() {
            self.blend_amount.clamp(0.0, 1.0)
        } else {
            0.0
        };
        add_expression(&self.draft, self.intensity * (1.0 - t), &mut controls);
        if let Some(other) = other {
            add_expression(&other, self.intensity * t, &mut controls);
        }
        for value in &mut controls {
            *value = value.clamp(0.0, 1.0);
        }
        controls
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, assets: &Assets) -> Option<StudioAction> {
        let project = assets.face_expressions();
        let names = expression_names(&project);
        let mut action = None;
        crate::responsive::split(ui, 460.0, 0.32, |ui, side| {
            if side == 0 {
                self.list(ui, assets, &names);
            } else if let Some(made) = self.detail(ui, &names) {
                action = Some(made);
            }
        });
        action
    }

    fn list(&mut self, ui: &mut egui::Ui, assets: &Assets, names: &[String]) {
        let palette = theme::palette(ui.ctx());
        crate::responsive::toolbar(ui, |ui| {
            widgets::section_label(ui, "Expressions");
            if widgets::icon_button(ui, icons::PLUS, "New expression", false).clicked() {
                *self = Self {
                    save_name: "Expression".to_owned(),
                    intensity: self.intensity,
                    ..Self::default()
                };
            }
        });
        let mut picked = None;
        for name in names {
            let built_in = BUILT_IN_EXPRESSIONS
                .iter()
                .any(|known| known.eq_ignore_ascii_case(name))
                && !assets
                    .face_expressions()
                    .expressions
                    .iter()
                    .any(|(path, e)| {
                        axcenity_assets::expression_name(path, e).eq_ignore_ascii_case(name)
                    });
            let mut text = egui::RichText::new(name);
            if built_in {
                text = text.color(palette.text_secondary);
            }
            let selected = self.picked.as_deref() == Some(name.as_str());
            let response = ui.selectable_label(selected, text);
            let response = if built_in {
                response.on_hover_text("Built in: saving makes the project's own copy")
            } else {
                response
            };
            if response.clicked() {
                picked = Some(name.clone());
            }
        }
        if let Some(name) = picked {
            self.pick(assets, &name);
        }
    }

    fn detail(&mut self, ui: &mut egui::Ui, names: &[String]) -> Option<StudioAction> {
        let palette = theme::palette(ui.ctx());
        let mut action = None;
        crate::responsive::form(ui, "expression-preview", |ui| {
            ui.label("Preview at");
            ui.add(egui::Slider::new(&mut self.intensity, 0.0..=1.0));
            ui.end_row();
            ui.label("Blend with");
            egui::ComboBox::from_id_salt("expression-blend")
                .selected_text(self.blend.as_deref().unwrap_or("Nothing"))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.blend, None, "Nothing");
                    for name in names {
                        ui.selectable_value(&mut self.blend, Some(name.clone()), name);
                    }
                });
            ui.end_row();
            if self.blend.is_some() {
                ui.label("Blend amount");
                ui.add(egui::Slider::new(&mut self.blend_amount, 0.0..=1.0))
                    .on_hover_text("How much of the other expression the preview shows");
                ui.end_row();
            }
        });
        widgets::section(icons::SMILEY, "Expression")
            .id_salt("expression-edit")
            .show(ui, |ui| {
                crate::responsive::form(ui, "expression-own", |ui| {
                    ui.label("Intensity");
                    ui.add(egui::Slider::new(&mut self.draft.intensity, 0.0..=2.0))
                        .on_hover_text("Its own strength; SetExpression's intensity multiplies it");
                    ui.end_row();
                    ui.label("Transition");
                    ui.add(
                        egui::DragValue::new(&mut self.draft.transition)
                            .speed(0.01)
                            .range(0.0..=5.0)
                            .suffix(" s"),
                    )
                    .on_hover_text("Seconds it takes to blend in and out");
                    ui.end_row();
                });
                widgets::section_label(ui, "Controls it sets");
                if self.draft.controls.is_empty() {
                    ui.label(
                        egui::RichText::new("None yet: add the controls it moves.")
                            .color(palette.text_weak),
                    );
                }
                let mut remove = None;
                for (name, value) in &mut self.draft.controls {
                    ui.horizontal(|ui| {
                        if widgets::icon_button(ui, icons::X, "Take it out", false).clicked() {
                            remove = Some(name.clone());
                        }
                        ui.add(egui::Slider::new(value, 0.0..=1.0).text(name.as_str()));
                    });
                }
                if let Some(name) = remove {
                    self.draft.controls.remove(&name);
                }
                let mut add = None;
                egui::ComboBox::from_id_salt("expression-add")
                    .selected_text(theme::with_icon(icons::PLUS, "Add a control"))
                    .show_ui(ui, |ui| {
                        for control in FACE_CONTROLS {
                            if !self.draft.controls.contains_key(control.name)
                                && ui.selectable_label(false, control.name).clicked()
                            {
                                add = Some(control.name);
                            }
                        }
                    });
                if let Some(name) = add {
                    self.draft.controls.insert(name.to_owned(), 0.5);
                }
            });
        crate::responsive::toolbar(ui, |ui| {
            ui.label("Save as");
            ui.add(egui::TextEdit::singleline(&mut self.save_name).desired_width(120.0));
            let name = file_name(&self.save_name);
            let tip = match &name {
                Some(name) => format!(
                    "Saves {EXPRESSIONS_FOLDER}/{name}{EXPRESSION_EXTENSION}; play it with \
                     face:SetExpression(\"{name}\")"
                ),
                None => "Name it first".to_owned(),
            };
            if ui
                .add_enabled(
                    name.is_some(),
                    egui::Button::new(theme::with_icon(icons::FLOPPY_DISK, "Save expression")),
                )
                .on_hover_text(tip)
                .clicked()
                && let Some(name) = name
            {
                let mut expression = self.draft.clone();
                expression.name = String::new();
                self.picked = Some(name.clone());
                action = Some(StudioAction::SaveExpression {
                    path: format!("{EXPRESSIONS_FOLDER}/{name}{EXPRESSION_EXTENSION}"),
                    expression,
                });
            }
        });
        action
    }
}
