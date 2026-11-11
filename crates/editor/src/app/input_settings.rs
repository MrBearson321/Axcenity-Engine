//! File ▸ Input…: the project's input map (`input.json`). Each action
//! lists what it's bound to; a binding is added by pressing a key, or by
//! picking a mouse button, a controller button or an axis, and removed
//! with its ×. Changes are written at once, and the next Play uses them.

use axcenity_core::input::{Action, Input, InputMap};
use axcenity_core::{KeyCode, UserInputType};

use super::*;
use crate::widgets;

/// Whether the Input window is open, and what it's in the middle of.
#[derive(Default)]
pub(super) struct InputWindow {
    pub open: bool,
    /// The action the next key pressed is added to.
    pub listening: Option<String>,
    /// The name typed for a new action, and whether it's an axis.
    new_action: String,
    new_axis: bool,
}

/// One change to the input map.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum InputEdit {
    Add { action: String, input: Input },
    Remove { action: String, index: usize },
    AddAction { name: String, axis: bool },
    RemoveAction(String),
    Reset,
}

/// What an axis can be bound to, besides four keys pressed in turn.
const AXIS_INPUTS: [&str; 6] = [
    "W,A,S,D",
    "Up,Left,Down,Right",
    "DPadUp,DPadLeft,DPadDown,DPadRight",
    "Thumbstick1",
    "Thumbstick2",
    "MouseMovement",
];

/// `map` with `edit` made, or what's wrong with it.
pub(super) fn edited(map: &InputMap, edit: InputEdit) -> Result<InputMap, String> {
    let mut map = map.clone();
    let action = |map: &mut InputMap, name: &str| -> Result<usize, String> {
        map.actions
            .iter()
            .position(|action| action.name == name)
            .ok_or_else(|| format!("there's no action named \"{name}\""))
    };
    match edit {
        InputEdit::Add {
            action: name,
            input,
        } => {
            let index = action(&mut map, &name)?;
            let inputs = &mut map.actions[index].inputs;
            if !inputs.contains(&input) {
                inputs.push(input);
            }
        }
        InputEdit::Remove {
            action: name,
            index,
        } => {
            let at = action(&mut map, &name)?;
            let inputs = &mut map.actions[at].inputs;
            if index < inputs.len() {
                inputs.remove(index);
            }
        }
        InputEdit::AddAction { name, axis } => {
            let name = name.trim().to_owned();
            if name.is_empty() {
                return Err("an action needs a name".to_owned());
            }
            if map.action(&name).is_some() {
                return Err(format!("there's an action named \"{name}\" already"));
            }
            map.actions.push(Action {
                name,
                axis,
                inputs: Vec::new(),
            });
        }
        InputEdit::RemoveAction(name) => {
            let index = action(&mut map, &name)?;
            map.actions.remove(index);
        }
        InputEdit::Reset => map = InputMap::default(),
    }
    Ok(map)
}

impl EditorApp {
    /// Makes `edit` to the project's input map and writes `input.json`, as
    /// one undo step (undo writes the file back; `apply_history_texts`
    /// reads it again).
    pub(super) fn edit_input_map(&mut self, edit: InputEdit) {
        let label = match &edit {
            InputEdit::Add { action, .. } => format!("Add an input to {action}"),
            InputEdit::Remove { action, .. } => format!("Remove an input from {action}"),
            InputEdit::AddAction { name, .. } => format!("Add input action {name}"),
            InputEdit::RemoveAction(name) => format!("Remove input action {name}"),
            InputEdit::Reset => "Restore default input".to_owned(),
        };
        self.edit_input_map_all(label, vec![edit]);
    }

    /// Makes `edits` in turn, as one undo step `label` (all or nothing).
    pub(super) fn edit_input_map_all(&mut self, label: String, edits: Vec<InputEdit>) {
        let Some(project) = &mut self.project else {
            return;
        };
        let mut map = project.input_map.clone();
        for edit in edits {
            map = match edited(&map, edit) {
                Ok(map) => map,
                Err(err) => {
                    self.output.error(format!("Input: {err}."));
                    return;
                }
            };
        }
        let path = project.folder.join(axcenity_project::INPUT_FILE);
        let before = std::fs::read_to_string(&path).ok();
        match axcenity_project::save_input_map(&project.folder, &map) {
            Ok(()) => project.input_map = map,
            Err(err) => {
                self.output.error(format!(
                    "Could not save the input map: {}",
                    files::chain(&err)
                ));
                return;
            }
        }
        let after = std::fs::read_to_string(&path).ok();
        self.history.record_files(
            &mut self.world,
            label,
            vec![history::TextChange {
                path: axcenity_project::INPUT_FILE.to_owned(),
                before,
                after,
            }],
        );
    }

    /// File ▸ Input…
    pub(super) fn input_window(&mut self, ctx: &egui::Context) {
        if !self.input_settings.open {
            return;
        }
        let Some(map) = self
            .project
            .as_ref()
            .map(|project| project.input_map.clone())
        else {
            self.input_settings.open = false;
            return;
        };
        let editing = !self.playing();
        let mut edits = Vec::new();
        // A key pressed while listening is added to that action.
        if let Some(action) = self.input_settings.listening.clone() {
            let pressed = ctx.input(|input| {
                input.events.iter().find_map(|event| match event {
                    egui::Event::Key {
                        key,
                        physical_key,
                        pressed: true,
                        ..
                    } => Some(physical_key.unwrap_or(*key)),
                    _ => None,
                })
            });
            match pressed {
                Some(egui::Key::Escape) => {
                    // Taken, so it stops listening without closing the window.
                    ctx.input_mut(|input| {
                        input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                    });
                    self.input_settings.listening = None;
                }
                Some(key) => {
                    if let Some(key) = crate::play::script_key(key) {
                        edits.push(InputEdit::Add {
                            action,
                            input: Input::Key(key),
                        });
                    }
                    self.input_settings.listening = None;
                }
                None => {}
            }
        }
        let mut open = self.input_settings.open;
        let palette = theme::palette(ctx);
        widgets::tool_window(icons::GAME_CONTROLLER, "Input")
            .size(egui::vec2(460.0, 0.0))
            .show(ctx, &mut open, |ui| {
                ui.label(
                    egui::RichText::new(
                        "What each action is bound to, for the built-in characters, UI \
                         navigation and scripts (UserInputService:IsActionPressed). Saved \
                         in input.json; players can rebind with RebindAction or the \
                         Controls menu (UI > Controls).",
                    )
                    .color(palette.text_weak),
                );
                if !editing {
                    ui.colored_label(palette.warning, "Press Stop to change the input map.");
                }
                ui.add_space(6.0);
                ui.add_enabled_ui(editing, |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(380.0)
                        .show(ui, |ui| {
                            for action in &map.actions {
                                self.action_row(ui, action, &mut edits);
                                ui.separator();
                            }
                        });
                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.input_settings.new_action)
                                .hint_text("New action")
                                .desired_width(140.0),
                        );
                        ui.checkbox(&mut self.input_settings.new_axis, "Axis");
                        if ui.button("Add action").clicked() {
                            edits.push(InputEdit::AddAction {
                                name: std::mem::take(&mut self.input_settings.new_action),
                                axis: self.input_settings.new_axis,
                            });
                        }
                        if ui
                            .button("Restore defaults")
                            .on_hover_text("Put every action back as new projects have them.")
                            .clicked()
                        {
                            edits.push(InputEdit::Reset);
                        }
                    });
                });
            });
        self.input_settings.open = open;
        for edit in edits {
            self.edit_input_map(edit);
        }
    }

    /// One action: its name, a chip for each input (× removes it), and
    /// ways to add one.
    fn action_row(&mut self, ui: &mut egui::Ui, action: &Action, edits: &mut Vec<InputEdit>) {
        let palette = theme::palette(ui.ctx());
        ui.horizontal_wrapped(|ui| {
            ui.strong(&action.name);
            if action.axis {
                ui.label(egui::RichText::new("axis").color(palette.text_weak));
            }
            for (index, input) in action.inputs.iter().enumerate() {
                let chip =
                    egui::Button::new(format!("{}  {}", input.name(), icons::X)).corner_radius(8.0);
                if ui.add(chip).on_hover_text("Remove").clicked() {
                    edits.push(InputEdit::Remove {
                        action: action.name.clone(),
                        index,
                    });
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            let listening = self.input_settings.listening.as_deref() == Some(&action.name);
            if action.axis {
                egui::ComboBox::from_id_salt(("add axis", &action.name))
                    .selected_text("Add…")
                    .show_ui(ui, |ui| {
                        for name in AXIS_INPUTS {
                            if ui.selectable_label(false, name).clicked()
                                && let Some(input) = Input::parse(name)
                            {
                                edits.push(InputEdit::Add {
                                    action: action.name.clone(),
                                    input,
                                });
                            }
                        }
                    });
            } else {
                let label = if listening {
                    "Press a key… (Escape cancels)"
                } else {
                    "Press a key…"
                };
                let role = if listening {
                    widgets::Role::Primary
                } else {
                    widgets::Role::Secondary
                };
                if ui
                    .add(widgets::button(
                        role,
                        Some(icons::KEYBOARD),
                        label,
                        &palette,
                    ))
                    .clicked()
                {
                    self.input_settings.listening = Some(action.name.clone());
                }
                egui::ComboBox::from_id_salt(("add button", &action.name))
                    .selected_text("Add a button…")
                    .show_ui(ui, |ui| {
                        let mouse = [
                            UserInputType::MouseButton1,
                            UserInputType::MouseButton2,
                            UserInputType::MouseButton3,
                        ]
                        .map(Input::Mouse);
                        let pads = KeyCode::ALL
                            .into_iter()
                            .filter(|key| key.is_gamepad() && !key.is_stick())
                            .map(Input::Key);
                        for input in mouse.into_iter().chain(pads) {
                            if ui.selectable_label(false, input.name()).clicked() {
                                edits.push(InputEdit::Add {
                                    action: action.name.clone(),
                                    input,
                                });
                            }
                        }
                    });
            }
            if widgets::icon_button(
                ui,
                icons::TRASH,
                &format!("Delete the action {}", action.name),
                false,
            )
            .clicked()
            {
                edits.push(InputEdit::RemoveAction(action.name.clone()));
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_add_and_remove_inputs_and_actions_and_reset() {
        let map = InputMap::default();
        let jump_e = edited(
            &map,
            InputEdit::Add {
                action: "Jump".into(),
                input: Input::Key(KeyCode::E),
            },
        )
        .unwrap();
        let without_space = edited(
            &jump_e,
            InputEdit::Remove {
                action: "Jump".into(),
                index: 0,
            },
        )
        .unwrap();
        assert_eq!(
            without_space.action("Jump").unwrap().inputs,
            [Input::Key(KeyCode::ButtonA), Input::Key(KeyCode::E)]
        );
        let fly = InputEdit::AddAction {
            name: " Fly ".into(),
            axis: false,
        };
        let with_fly = edited(&map, fly.clone()).unwrap();
        assert!(with_fly.action("Fly").is_some());
        assert!(edited(&with_fly, fly).unwrap_err().contains("already"));
        let without = edited(&with_fly, InputEdit::RemoveAction("Fly".into())).unwrap();
        assert_eq!(without, map);
        assert_eq!(edited(&jump_e, InputEdit::Reset).unwrap(), map);
    }

    #[test]
    fn the_input_window_draws_and_its_edits_are_saved_in_input_json() {
        let folder = super::super::tests::copy_of_example("hello-character", "input-window");
        let mut app = EditorApp::new(Some(&folder));
        app.input_settings.open = true;
        let ctx = theme::test_context();
        let mut draw = |ui: &mut egui::Ui| {
            let ctx = &ui.ctx().clone();
            egui::CentralPanel::default().show(ui, |_| {});
            app.input_window(ctx);
        };
        match std::env::var_os("AXCENITY_SNAPSHOTS").map(PathBuf::from) {
            Some(folder) => crate::snapshot::snapshot(
                &folder.join("input-window.png"),
                egui::vec2(700.0, 640.0),
                vec![Vec::new(); 3],
                &mut draw,
            ),
            None => {
                let _ = crate::theme::run(&ctx, Default::default(), |ui| draw(ui));
            }
        }
        assert!(app.input_settings.open);

        app.edit_input_map(InputEdit::Remove {
            action: "Jump".into(),
            index: 0,
        });
        app.edit_input_map(InputEdit::Add {
            action: "Jump".into(),
            input: Input::Key(KeyCode::E),
        });
        let mut problems = Vec::new();
        let saved = axcenity_project::load_input_map(&folder, &mut problems);
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(
            saved.action("Jump").unwrap().inputs,
            [Input::Key(KeyCode::ButtonA), Input::Key(KeyCode::E)]
        );
        assert_eq!(app.project.as_ref().unwrap().input_map, saved);
    }
}
