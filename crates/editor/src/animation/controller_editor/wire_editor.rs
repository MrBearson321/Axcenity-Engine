//! The graph with its parameter chips, and the editor a wire's label opens
//! right where it was clicked: When… rows, + condition, Wait for the clip
//! to finish, Blend and Delete transition. The inspector keeps the full
//! settings.

use super::*;
use crate::animation::graph::{self, words};

/// A parameter being typed into a wire's label editor: for condition
/// `condition` (or a new one), of `kind`.
#[derive(Clone, Debug)]
pub(super) struct NewParameter {
    transition: usize,
    condition: Option<usize>,
    name: String,
    kind: ParameterKind,
    problem: Option<String>,
}

/// The tests that fit a parameter of `kind`, with their words.
pub(super) fn tests_for(kind: Option<ParameterKind>) -> &'static [(Test, &'static str)] {
    match kind {
        Some(ParameterKind::Bool) => &[(Test::IsTrue, "is true"), (Test::IsFalse, "is false")],
        Some(ParameterKind::Trigger) => &[(Test::Triggered, "is set")],
        Some(ParameterKind::Number) => &[
            (Test::Greater, "is more than"),
            (Test::Less, "is less than"),
        ],
        None => &[],
    }
}

/// The parameter panel beside (or above, when narrow) the graph.
const PANEL_WIDTH: f32 = 132.0;
/// Below this width the chips go above the graph.
const SIDE_PANEL_FROM: f32 = 460.0;

impl ControllerDoc {
    /// The parameter chips and the graph, filling `ui`, and the label
    /// editor if one is open. Does what was done, and returns the state
    /// double-clicked, to open its clip.
    pub fn graph_ui(&mut self, ui: &mut egui::Ui, clips: &[String]) -> Option<usize> {
        let live = self.live();
        let selected = self.selected;
        let mut events = Vec::new();
        if ui.available_width() >= SIDE_PANEL_FROM {
            let height = ui.available_height();
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(PANEL_WIDTH, height),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_min_size(egui::vec2(PANEL_WIDTH, height));
                        egui::ScrollArea::vertical()
                            .id_salt("parameter chips")
                            .show(ui, |ui| {
                                events.extend(graph::parameters_panel(
                                    ui,
                                    &self.file.parameters,
                                    &mut self.view.chips,
                                ));
                            });
                    },
                );
                let size = ui.available_size();
                ui.allocate_ui(size, |ui| {
                    events.extend(self.view.show(ui, &self.file, selected, live, clips));
                });
            });
        } else {
            events.extend(graph::parameters_panel(
                ui,
                &self.file.parameters,
                &mut self.view.chips,
            ));
            let size = ui.available_size();
            ui.allocate_ui(size, |ui| {
                events.extend(self.view.show(ui, &self.file, selected, live, clips));
            });
        }
        let mut open = None;
        events.retain(|event| match event {
            GraphEvent::Open(index) => {
                open = Some(*index);
                false
            }
            _ => true,
        });
        self.apply(events);
        self.wire_editor(ui);
        open
    }

    /// The editor a wire's label opens, just under the label. Escape or a
    /// click elsewhere closes it.
    fn wire_editor(&mut self, ui: &egui::Ui) {
        let Some(index) = self.view.editing else {
            return;
        };
        let Some(&(_, label)) = self.view.labels.iter().find(|(i, _)| *i == index) else {
            return;
        };
        // A click in one of its own menus isn't a click elsewhere.
        let menu_was_open = egui::Popup::is_any_open(ui.ctx());
        let area = egui::Area::new(ui.id().with("wire editor"))
            .order(egui::Order::Foreground)
            .fixed_pos(label.left_bottom() + egui::vec2(0.0, 6.0))
            .constrain(true)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_width(270.0);
                    self.wire_editor_body(ui, index);
                })
            });
        let clicked_elsewhere = !menu_was_open
            && ui.input(|input| {
                input.pointer.any_click()
                    && input.pointer.interact_pos().is_some_and(|pointer| {
                        !area.response.rect.contains(pointer) && !label.contains(pointer)
                    })
            });
        let typing = ui.ctx().memory(|memory| memory.focused().is_some());
        let escape = !typing && ui.input(|input| input.key_pressed(egui::Key::Escape));
        if clicked_elsewhere || escape || index >= self.file.transitions.len() {
            self.view.editing = None;
            self.new_parameter = None;
        }
    }

    fn wire_editor_body(&mut self, ui: &mut egui::Ui, index: usize) {
        let Some(transition) = self.file.transitions.get(index) else {
            return;
        };
        let (from, to) = (transition.from.clone(), transition.to.clone());
        ui.strong(format!("{from} → {to}"));
        if transition.generated {
            ui.label(format!(
                "Made by {from}'s Return setting, so it's changed there."
            ));
            if ui.button("Edit the Return setting").clicked()
                && let Some(state) = self.file.states.iter().position(|s| s.name == from)
            {
                self.selected = Selected::Node(Node::State(state));
                self.view.editing = None;
            }
            return;
        }
        ui.weak(words::transition_header(transition));
        ui.add_space(2.0);
        let parameters = self.file.parameters.clone();
        let kind_of = |name: &str| {
            parameters
                .iter()
                .find(|def| def.name() == name)
                .map(ParameterKind::of)
        };
        let mut remove = None;
        let mut typing_for = None;
        let transition = &mut self.file.transitions[index];
        for (c, condition) in transition.conditions.iter_mut().enumerate() {
            ui.horizontal_wrapped(|ui| {
                ui.label(if c == 0 { "When" } else { "and" });
                egui::ComboBox::from_id_salt(("wire condition parameter", index, c))
                    .width(96.0)
                    .selected_text(&condition.parameter)
                    .show_ui(ui, |ui| {
                        for def in &parameters {
                            let kind = ParameterKind::of(def);
                            ui.selectable_value(
                                &mut condition.parameter,
                                def.name().to_owned(),
                                format!("{} {}", kind.icon(), def.name()),
                            );
                        }
                        if ui
                            .button(format!("{} New parameter…", icons::PLUS))
                            .clicked()
                        {
                            typing_for = Some(Some(c));
                        }
                    });
                let tests = tests_for(kind_of(&condition.parameter));
                if let Some(&(first, _)) = tests.first()
                    && !tests.iter().any(|(test, _)| *test == condition.test)
                {
                    condition.test = first;
                }
                if tests.len() > 1 {
                    let label = tests
                        .iter()
                        .find(|(test, _)| *test == condition.test)
                        .map_or("", |(_, label)| label);
                    egui::ComboBox::from_id_salt(("wire condition test", index, c))
                        .width(90.0)
                        .selected_text(label)
                        .show_ui(ui, |ui| {
                            for &(test, label) in tests {
                                ui.selectable_value(&mut condition.test, test, label);
                            }
                        });
                } else if let Some((_, label)) = tests.first() {
                    ui.label(*label);
                }
                if matches!(condition.test, Test::Greater | Test::Less) {
                    ui.add(egui::DragValue::new(&mut condition.value).speed(0.05));
                }
                if ui.small_button(icons::X).on_hover_text("Remove").clicked() {
                    remove = Some(c);
                }
            });
        }
        if let Some(c) = remove {
            transition.conditions.remove(c);
        }
        if ui
            .button(format!("{} condition", icons::PLUS))
            .on_hover_text("Another thing that must be true")
            .clicked()
        {
            match parameters.first() {
                Some(def) => self.add_condition(index, def.name()),
                None => typing_for = Some(None),
            }
        }
        if let Some(condition) = typing_for {
            self.new_parameter = Some(NewParameter {
                transition: index,
                condition,
                name: String::new(),
                kind: ParameterKind::Number,
                problem: None,
            });
        }
        self.new_parameter_row(ui, index);

        let transition = &mut self.file.transitions[index];
        ui.separator();
        let mut wait = transition.exit_time.is_some();
        if ui
            .checkbox(&mut wait, "Wait for the clip to finish")
            .on_hover_text(format!(
                "Only once {from} has played its clip through (its exit time)"
            ))
            .changed()
        {
            transition.exit_time = wait.then_some(1.0);
        }
        ui.horizontal_wrapped(|ui| {
            ui.label("Blend");
            for (label, seconds) in BLENDS {
                if ui
                    .selectable_label((transition.blend - seconds).abs() < 1e-9, label)
                    .clicked()
                {
                    transition.blend = seconds;
                }
            }
        });
        if words::never(transition) {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                "This never happens yet: add a condition, or wait for the clip.",
            );
        }
        ui.separator();
        if ui
            .button(format!("{} Delete transition", icons::TRASH))
            .clicked()
        {
            self.delete(Selected::Transition(index));
            self.view.editing = None;
        }
    }

    /// The inline row that names a new parameter for a condition.
    fn new_parameter_row(&mut self, ui: &mut egui::Ui, index: usize) {
        let Some(typing) = &mut self.new_parameter else {
            return;
        };
        if typing.transition != index {
            self.new_parameter = None;
            return;
        }
        let mut add = false;
        let mut cancel = false;
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                let field = ui.add(
                    egui::TextEdit::singleline(&mut typing.name)
                        .hint_text("New parameter")
                        .desired_width(110.0),
                );
                if typing.name.is_empty() && !field.has_focus() && typing.problem.is_none() {
                    field.request_focus();
                }
                add = field.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
                for kind in ParameterKind::ALL {
                    ui.selectable_value(&mut typing.kind, kind, kind.icon())
                        .on_hover_text(kind.label());
                }
            });
            ui.horizontal_wrapped(|ui| {
                add |= ui.button("Add").clicked();
                cancel = ui.button("Cancel").clicked();
            });
            if let Some(problem) = &typing.problem {
                ui.colored_label(ui.visuals().error_fg_color, problem);
            }
        });
        if cancel {
            self.new_parameter = None;
            return;
        }
        if !add {
            return;
        }
        let typing = typing.clone();
        match self.new_parameter(typing.kind, &typing.name) {
            Ok(name) => {
                match typing.condition {
                    Some(c) => {
                        if let Some(condition) = self.file.transitions[index].conditions.get_mut(c)
                        {
                            condition.parameter = name;
                        }
                    }
                    None => self.add_condition(index, &name),
                }
                self.new_parameter = None;
            }
            Err(problem) => {
                if let Some(open) = &mut self.new_parameter {
                    open.problem = Some(problem);
                }
            }
        }
    }
}
