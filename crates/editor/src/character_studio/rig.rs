//! Character Studio's Rig section: the profile a face uses, what doesn't
//! fit, the controls as sliders, and the profile editor (shapes to
//! controls, bones and limits).

use axcenity_anim::{BUILT_IN_FACE_PROFILES, FaceBoneRole, FaceProblem, FaceRig, Severity};
use axcenity_assets::{
    FACE_EXTENSION, FACES_FOLDER, FaceLimits, FaceProfile, FaceProfiles, Model, ShapeWeight,
};
use axcenity_core::face::{FACE_CONTROLS, FaceGroup};
use axcenity_core::{Value, World};
use eframe::egui;

use super::{StudioAction, Target, file_name};
use crate::theme::{self, icons};
use crate::widgets;

/// The face's groups of controls, in the editor's order.
const GROUPS: [FaceGroup; 5] = [
    FaceGroup::Eyes,
    FaceGroup::Eyebrows,
    FaceGroup::Mouth,
    FaceGroup::CheeksAndNose,
    FaceGroup::JawAndTongue,
];

/// A profile being edited, from the one the face was matched to.
#[derive(Clone, Debug, PartialEq)]
pub struct ProfileDraft {
    /// The name of the profile it started from.
    pub from: String,
    pub profile: FaceProfile,
    /// Counts edits, so the preview matches it again.
    pub revision: usize,
    /// What Save calls the file.
    pub save_name: String,
}

/// The Rig section's state.
#[derive(Default)]
pub struct RigEditor {
    pub draft: Option<ProfileDraft>,
    /// The profile the face is matched to (its name and itself).
    pub matched: Option<(String, FaceProfile)>,
    /// Why the profile the FaceControls names can't be used.
    pub error: Option<String>,
    filter: String,
}

impl RigEditor {
    /// Starts editing the matched profile, keeping only the shapes
    /// `model` has.
    pub fn edit(&mut self, model: Option<&Model>) {
        let Some((name, mut profile)) = self.matched.clone() else {
            return;
        };
        if let Some(model) = model {
            let has = |shape: &str| model.morph_targets.iter().any(|t| t.name == shape);
            for (_, shapes) in &mut profile.controls {
                shapes.retain(|s| has(&s.shape));
            }
            profile.controls.retain(|(_, shapes)| !shapes.is_empty());
        }
        let stem = name
            .rsplit('/')
            .next()
            .unwrap_or(&name)
            .trim_end_matches(FACE_EXTENSION);
        let save_name = if profile.name.is_empty() {
            stem.to_owned()
        } else {
            profile.name.clone()
        };
        self.draft = Some(ProfileDraft {
            from: name,
            profile,
            revision: 0,
            save_name: file_name(&save_name).unwrap_or_else(|| "Face".to_owned()),
        });
    }

    /// Stops editing, dropping the draft.
    pub fn discard(&mut self) {
        self.draft = None;
    }

    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        world: &World,
        profiles: &FaceProfiles,
        target: Target,
        model: Option<&Model>,
        bound: Option<(&str, &FaceRig, &[FaceProblem])>,
    ) -> Option<StudioAction> {
        let palette = theme::palette(ui.ctx());
        let mut action = None;
        let face = target.face.and_then(|face| world.get(face));
        let wanted = face
            .and_then(|face| face.property("Profile"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        crate::responsive::toolbar(ui, |ui| {
            ui.label("Profile");
            let shown = if wanted.is_empty() {
                "Best fit".to_owned()
            } else {
                wanted.clone()
            };
            ui.add_enabled_ui(target.face.is_some(), |ui| {
                egui::ComboBox::from_id_salt("face-profile")
                    .selected_text(shown)
                    .show_ui(ui, |ui| {
                        let choices = std::iter::once(String::new())
                            .chain(BUILT_IN_FACE_PROFILES.iter().map(|name| name.to_string()))
                            .chain(profiles.profiles.iter().map(|(path, _)| path.clone()));
                        for choice in choices {
                            let label = if choice.is_empty() {
                                "Best fit (the one naming the most shapes)"
                            } else {
                                &choice
                            };
                            if ui.selectable_label(choice == wanted, label).clicked()
                                && let Some(face) = target.face
                            {
                                action = Some(StudioAction::Set {
                                    face,
                                    property: "Profile",
                                    value: Value::String(choice.clone()),
                                    done: true,
                                });
                            }
                        }
                    })
                    .response
                    .on_disabled_hover_text("Insert a FaceControls into the MeshPart first.");
            });
            if let Some(face) = target.face
                && ui
                    .button(theme::with_icon(icons::FILM_STRIP, "Key in Animation"))
                    .on_hover_text(
                        "Select the FaceControls and open the Animation tab, to key its \
                         controls over time in a clip",
                    )
                    .clicked()
            {
                action = Some(StudioAction::KeyInAnimation(face));
            }
        });
        if let Some((name, rig, _)) = bound {
            ui.label(
                egui::RichText::new(format!(
                    "Matched to {name}: {} of {} controls move something.",
                    rig.controls_used(),
                    FACE_CONTROLS.len()
                ))
                .color(palette.text_weak),
            );
        }
        if let Some(error) = &self.error {
            ui.colored_label(palette.error, format!("{} {error}", icons::WARNING));
        }
        if target.face.is_none() {
            ui.label(
                egui::RichText::new(
                    "This MeshPart has no FaceControls: insert one to set its controls, \
                     profile, blinking and gaze. Expressions and lip syncs preview here \
                     without one.",
                )
                .color(palette.text_weak),
            );
        }

        let problems = bound.map_or(&[][..], |(_, _, problems)| problems);
        let title = if problems.is_empty() {
            "Check".to_owned()
        } else {
            format!("Check · {}", problems.len())
        };
        widgets::section(icons::CHECK_CIRCLE, &title)
            .id_salt("face-check")
            .show(ui, |ui| {
                if problems.is_empty() {
                    ui.colored_label(
                        palette.success,
                        format!("{} The profile fits this model.", icons::CHECK),
                    );
                }
                let row = |ui: &mut egui::Ui, problem: &FaceProblem| {
                    let (icon, color) = match problem.severity() {
                        Severity::Error => (icons::X_CIRCLE, palette.error),
                        Severity::Warning => (icons::WARNING, palette.warning),
                        Severity::Note => (icons::INFO, palette.text_weak),
                    };
                    ui.horizontal_wrapped(|ui| {
                        ui.colored_label(color, icon);
                        ui.label(problem.to_string());
                    });
                };
                const SHOWN: usize = 6;
                for problem in problems.iter().take(SHOWN) {
                    row(ui, problem);
                }
                if problems.len() > SHOWN {
                    egui::CollapsingHeader::new(format!("{} more", problems.len() - SHOWN))
                        .id_salt("face-check-more")
                        .show(ui, |ui| {
                            for problem in &problems[SHOWN..] {
                                row(ui, problem);
                            }
                        });
                }
            });

        widgets::section(icons::SLIDERS_HORIZONTAL, "Controls")
            .id_salt("face-controls")
            .hint("The FaceControls' own values: the face's Base layer")
            .show(ui, |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.filter)
                        .hint_text(format!("{} Filter controls", icons::MAGNIFYING_GLASS)),
                );
                let filter = self.filter.to_ascii_lowercase();
                for group in GROUPS {
                    let controls: Vec<(usize, &str, &str)> = FACE_CONTROLS
                        .iter()
                        .enumerate()
                        .filter(|(_, control)| control.group == group)
                        .filter(|(_, control)| {
                            filter.is_empty() || control.name.to_ascii_lowercase().contains(&filter)
                        })
                        .map(|(index, control)| (index, control.name, control.doc))
                        .collect();
                    if controls.is_empty() {
                        continue;
                    }
                    widgets::section_label(ui, group.label());
                    for (index, name, doc) in controls {
                        let moves = bound.is_none_or(|(_, rig, _)| rig.moves(index));
                        let mut value = face
                            .and_then(|face| face.property(name))
                            .and_then(Value::as_number)
                            .unwrap_or(0.0);
                        let label = if moves {
                            egui::RichText::new(name)
                        } else {
                            egui::RichText::new(name).color(palette.text_muted)
                        };
                        let response = ui
                            .add_enabled(
                                target.face.is_some(),
                                egui::Slider::new(&mut value, 0.0..=1.0).text(label),
                            )
                            .on_hover_text(if moves {
                                doc.to_owned()
                            } else {
                                format!("{doc} It moves nothing on this model.")
                            });
                        if let Some(face) = target.face
                            && (response.changed() || response.drag_stopped())
                        {
                            action = Some(StudioAction::Set {
                                face,
                                property: name,
                                value: Value::Number(value),
                                done: !response.dragged(),
                            });
                        }
                    }
                }
            });

        widgets::section(icons::GRAPH, "Profile mapping")
            .id_salt("face-mapping")
            .hint("Which blend shapes each control drives, the eye and jaw bones, and how far they turn")
            .default_open(false)
            .show(ui, |ui| {
                if let Some(made) = self.mapping(ui, model, target) {
                    action = Some(made);
                }
            });
        action
    }

    fn mapping(
        &mut self,
        ui: &mut egui::Ui,
        model: Option<&Model>,
        target: Target,
    ) -> Option<StudioAction> {
        let palette = theme::palette(ui.ctx());
        let Some(draft) = &mut self.draft else {
            ui.label(
                egui::RichText::new(
                    "Edit a copy of the matched profile to map this model's own blend shapes \
                     to the standard controls, name its eye and jaw bones, and set how far \
                     they turn. It saves as a .face.json in assets/faces.",
                )
                .color(palette.text_weak),
            );
            if ui
                .add_enabled(
                    self.matched.is_some(),
                    egui::Button::new(theme::with_icon(icons::PENCIL_SIMPLE, "Edit mapping")),
                )
                .clicked()
            {
                self.edit(model);
            }
            return None;
        };
        let mut changed = false;
        let Some(model) = model else {
            ui.label("The model isn't loaded.");
            return None;
        };
        widgets::section_label(ui, "Blend shapes");
        if model.morph_targets.is_empty() {
            ui.label(
                egui::RichText::new("This model has no blend shapes.").color(palette.text_weak),
            );
        }
        egui::Grid::new("face-shapes")
            .num_columns(3)
            .striped(true)
            .show(ui, |ui| {
                for target in &model.morph_targets {
                    ui.label(&target.name);
                    let current = shape_control(&draft.profile, &target.name);
                    let mut control = current.as_ref().map(|(c, _)| c.clone());
                    let mut weight = current.as_ref().map_or(1.0, |(_, w)| *w);
                    egui::ComboBox::from_id_salt(("shape-control", &target.name))
                        .selected_text(control.as_deref().unwrap_or("—"))
                        .width(150.0)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut control, None, "— (none)");
                            for standard in FACE_CONTROLS {
                                ui.selectable_value(
                                    &mut control,
                                    Some(standard.name.to_owned()),
                                    standard.name,
                                );
                            }
                        });
                    let weighed = ui
                        .add_enabled(
                            control.is_some(),
                            egui::DragValue::new(&mut weight)
                                .speed(0.01)
                                .range(0.0..=2.0),
                        )
                        .on_hover_text("How much of the shape the control's full value gives")
                        .changed();
                    if control != current.as_ref().map(|(c, _)| c.clone()) || weighed {
                        map_shape(&mut draft.profile, &target.name, control.as_deref(), weight);
                        changed = true;
                    }
                    ui.end_row();
                }
            });

        if let Some(rig) = model.rig.as_deref() {
            widgets::section_label(ui, "Bones");
            egui::Grid::new("face-bones").num_columns(2).show(ui, |ui| {
                for role in FaceBoneRole::ALL {
                    let key = role.as_str();
                    ui.label(key);
                    let current = draft.profile.bones.get(key).cloned();
                    let shown = match current.as_deref() {
                        None => "Found by name",
                        Some("") => "None",
                        Some(bone) => bone,
                    }
                    .to_owned();
                    let mut chosen = current.clone();
                    egui::ComboBox::from_id_salt(("face-bone", key))
                        .selected_text(shown)
                        .width(170.0)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut chosen, None, "Found by name");
                            ui.selectable_value(&mut chosen, Some(String::new()), "None");
                            for bone in &rig.bones {
                                ui.selectable_value(
                                    &mut chosen,
                                    Some(bone.name.clone()),
                                    &bone.name,
                                );
                            }
                        });
                    if chosen != current {
                        match chosen {
                            Some(bone) => draft.profile.bones.insert(key.to_owned(), bone),
                            None => draft.profile.bones.remove(key),
                        };
                        changed = true;
                    }
                    ui.end_row();
                }
            });
        }

        widgets::section_label(ui, "How far they turn (degrees)");
        let limits = &mut draft.profile.limits;
        let before = *limits;
        egui::Grid::new("face-limits")
            .num_columns(2)
            .show(ui, |ui| {
                for (label, value) in limits_rows(limits) {
                    ui.label(label);
                    ui.add(
                        egui::DragValue::new(value)
                            .speed(0.5)
                            .range(0.0..=90.0)
                            .suffix("°"),
                    );
                    ui.end_row();
                }
            });
        changed |= *limits != before;

        if changed {
            draft.revision += 1;
        }
        let mut action = None;
        let mut discard = false;
        crate::responsive::toolbar(ui, |ui| {
            ui.label("Save as");
            ui.add(egui::TextEdit::singleline(&mut draft.save_name).desired_width(120.0));
            let name = file_name(&draft.save_name);
            let tip = match &name {
                Some(name) => format!("Saves {FACES_FOLDER}/{name}{FACE_EXTENSION} and uses it"),
                None => "Name it first".to_owned(),
            };
            if ui
                .add_enabled(
                    name.is_some(),
                    egui::Button::new(theme::with_icon(icons::FLOPPY_DISK, "Save profile")),
                )
                .on_hover_text(tip)
                .clicked()
                && let Some(name) = name
            {
                let mut profile = draft.profile.clone();
                profile.name = name.clone();
                action = Some(StudioAction::SaveProfile {
                    face: target.face,
                    path: format!("{FACES_FOLDER}/{name}{FACE_EXTENSION}"),
                    profile,
                });
            }
            discard = ui.button("Discard").clicked();
        });
        if discard {
            self.draft = None;
        }
        action
    }
}

fn limits_rows(limits: &mut FaceLimits) -> [(&'static str, &mut f32); 5] {
    [
        ("Eyes sideways", &mut limits.look_sideways),
        ("Eyes up", &mut limits.look_up),
        ("Eyes down", &mut limits.look_down),
        ("Jaw open", &mut limits.jaw_open),
        ("Jaw sideways", &mut limits.jaw_sideways),
    ]
}

/// The first control in `profile` driving `shape`, and how much.
pub fn shape_control(profile: &FaceProfile, shape: &str) -> Option<(String, f32)> {
    profile.controls.iter().find_map(|(control, shapes)| {
        shapes
            .iter()
            .find(|s| s.shape == shape)
            .map(|s| (control.clone(), s.weight))
    })
}

/// Makes `control` (or none) the one driving `shape`, at `weight`: the
/// shape is taken from every other control, and a control left driving
/// nothing is dropped.
pub fn map_shape(profile: &mut FaceProfile, shape: &str, control: Option<&str>, weight: f32) {
    for (_, shapes) in &mut profile.controls {
        shapes.retain(|s| s.shape != shape);
    }
    profile.controls.retain(|(_, shapes)| !shapes.is_empty());
    let Some(control) = control else {
        return;
    };
    let entry = match profile.controls.iter().position(|(c, _)| c == control) {
        Some(at) => &mut profile.controls[at].1,
        None => {
            profile.controls.push((control.to_owned(), Vec::new()));
            &mut profile.controls.last_mut().expect("pushed").1
        }
    };
    entry.push(ShapeWeight::new(shape, weight));
}
