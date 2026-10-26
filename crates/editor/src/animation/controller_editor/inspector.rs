use super::*;
use crate::animation::graph::words;

/// A combo box picking one of the project's `clips` (or none) for `current`.
pub(super) fn clip_picker(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    current: &mut String,
    clips: &[String],
) {
    let shown = if current.is_empty() {
        "(none)".to_owned()
    } else {
        axcenity_anim::files::clip_name(current).to_owned()
    };
    egui::ComboBox::from_id_salt(id)
        .selected_text(shown)
        .show_ui(ui, |ui| {
            ui.selectable_value(current, String::new(), "(none)");
            for clip in clips {
                let label = format!(
                    "{}  ({})",
                    axcenity_anim::files::clip_name(clip),
                    super::super::store::group_of(clip)
                );
                ui.selectable_value(current, clip.clone(), label);
            }
        });
}

/// The clip in `clips` whose name has `word` in it, preferring ones in
/// `folder` (a clip's group).
pub(super) fn clip_named(clips: &[String], word: &str, folder: Option<&str>) -> Option<String> {
    let matching: Vec<&String> = clips
        .iter()
        .filter(|clip| {
            axcenity_anim::files::clip_name(clip)
                .to_lowercase()
                .contains(word)
        })
        .collect();
    matching
        .iter()
        .find(|clip| folder.is_some_and(|folder| super::super::store::group_of(clip) == folder))
        .or(matching.first())
        .map(|clip| (*clip).clone())
}

/// The blend tree's thresholds on a line, each clip's name over its tick,
/// with the parameter's value now as a marker.
pub(super) fn threshold_line(
    ui: &mut egui::Ui,
    tree: &BlendTree,
    value: Option<f64>,
    low: f64,
    high: f64,
) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 40.0), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let visuals = ui.visuals();
    let line_y = rect.bottom() - 10.0;
    let x_of = |v: f64| {
        let t = ((v - low) / (high - low).max(1e-9)).clamp(0.0, 1.0) as f32;
        egui::lerp(rect.left() + 8.0..=rect.right() - 8.0, t)
    };
    painter.line_segment(
        [
            egui::pos2(rect.left() + 8.0, line_y),
            egui::pos2(rect.right() - 8.0, line_y),
        ],
        egui::Stroke::new(2.0_f32, visuals.weak_text_color()),
    );
    for child in &tree.children {
        let x = x_of(child.threshold);
        painter.circle_filled(egui::pos2(x, line_y), 4.0, visuals.selection.bg_fill);
        let name = if child.clip.is_empty() {
            "?".to_owned()
        } else {
            axcenity_anim::files::clip_name(&child.clip).to_owned()
        };
        painter.text(
            egui::pos2(x, line_y - 8.0),
            egui::Align2::CENTER_BOTTOM,
            format!("{name} {}", child.threshold),
            egui::FontId::proportional(10.0),
            visuals.text_color(),
        );
    }
    if let Some(value) = value {
        let x = x_of(value);
        painter.line_segment(
            [
                egui::pos2(x, rect.top() + 2.0),
                egui::pos2(x, rect.bottom()),
            ],
            egui::Stroke::new(2.0_f32, egui::Color32::from_rgb(255, 120, 80)),
        );
        painter.text(
            egui::pos2(x + 4.0, rect.bottom()),
            egui::Align2::LEFT_BOTTOM,
            format!("{value:.2}"),
            egui::FontId::proportional(10.0),
            egui::Color32::from_rgb(255, 120, 80),
        );
    }
}

/// A slider range for each Number parameter: from 0 (or lower) to past
/// the biggest value its conditions test and its blend trees' thresholds.
pub(super) fn number_ranges(file: &StateMachineFile) -> BTreeMap<String, (f64, f64)> {
    let mut ranges = BTreeMap::new();
    for def in &file.parameters {
        if let ParameterDef::Number { name, default } = def {
            let mut low: f64 = default.min(0.0);
            let mut high: f64 = default.max(1.0);
            for condition in file.transitions.iter().flat_map(|t| &t.conditions) {
                if condition.parameter == *name {
                    low = low.min(condition.value);
                    high = high.max(condition.value * 1.5);
                }
            }
            for state in &file.states {
                if let Motion::BlendTree(tree) = &state.motion
                    && tree.parameter == *name
                {
                    for child in &tree.children {
                        low = low.min(child.threshold);
                        high = high.max(child.threshold);
                    }
                }
            }
            ranges.insert(name.clone(), (low, high.max(low + 1.0)));
        }
    }
    ranges
}

impl ControllerDoc {
    /// The inspector: the selected state or transition, or the controller's
    /// own settings (parameters, default state).
    pub fn inspector(&mut self, ui: &mut egui::Ui, clips: &[String]) {
        match self.selected {
            Selected::Node(Node::State(index)) if index < self.file.states.len() => {
                self.state_inspector(ui, index, clips)
            }
            Selected::Transition(index) if index < self.file.transitions.len() => {
                self.transition_inspector(ui, index)
            }
            Selected::Node(Node::Any) => {
                ui.heading(ANY_STATE);
                ui.label("Any State: its wires can happen whatever state is playing (but never to the state already playing). Use them for things that can happen at any time, such as Jump or TakeDamage.");
                ui.weak("Drag from its pin to a state to wire it.");
            }
            Selected::Node(Node::Entry) => {
                ui.heading("Entry");
                ui.label("Entry: where the Animator starts. It's wired to the default state; drag from its pin to another state to start there instead.");
                ui.add_space(4.0);
                self.default_state_picker(ui);
            }
            _ => self.controller_inspector(ui),
        }
    }

    pub(super) fn controller_inspector(&mut self, ui: &mut egui::Ui) {
        ui.heading(
            axcenity_anim::files::clip_name(&self.path).trim_end_matches(".statemachine.json"),
        );
        ui.weak(&self.path);
        ui.label("State machine: it starts at the state Entry is wired to, and follows a wire when what its label says is true.");
        ui.add_space(4.0);
        self.default_state_picker(ui);
        ui.separator();
        ui.strong("Parameters");
        ui.label("Parameters: values the game sets, from scripts or blocks, which the wires test.");
        ui.weak("Bool: stays true or false (IsGrounded). Trigger: a one-time request, used up by its transition (Jump). Number: changes gradually (Speed).");
        let mut remove = None;
        let mut renamed = None;
        egui::Grid::new("parameters")
            .num_columns(4)
            .spacing([6.0, 4.0])
            .show(ui, |ui| {
                for (index, def) in self.file.parameters.iter_mut().enumerate() {
                    let kind = match def {
                        ParameterDef::Bool { .. } => "Bool",
                        ParameterDef::Number { .. } => "Number",
                        ParameterDef::Trigger { .. } => "Trigger",
                    };
                    ui.weak(kind);
                    let mut name = def.name().to_owned();
                    if ui
                        .add(egui::TextEdit::singleline(&mut name).desired_width(110.0))
                        .changed()
                    {
                        renamed = Some((def.name().to_owned(), name.clone()));
                        match def {
                            ParameterDef::Bool { name: n, .. }
                            | ParameterDef::Number { name: n, .. }
                            | ParameterDef::Trigger { name: n } => *n = name,
                        }
                    }
                    match def {
                        ParameterDef::Bool { default, .. } => {
                            ui.checkbox(default, "")
                                .on_hover_text("Its value when the game starts");
                        }
                        ParameterDef::Number { default, .. } => {
                            ui.add(egui::DragValue::new(default).speed(0.05))
                                .on_hover_text("Its value when the game starts");
                        }
                        ParameterDef::Trigger { .. } => {
                            ui.label("");
                        }
                    }
                    if ui.small_button(icons::X).on_hover_text("Remove").clicked() {
                        remove = Some(index);
                    }
                    ui.end_row();
                }
            });
        // Conditions follow a renamed parameter.
        if let Some((old, new)) = renamed {
            for transition in &mut self.file.transitions {
                for condition in &mut transition.conditions {
                    if condition.parameter == old {
                        condition.parameter = new.clone();
                    }
                }
            }
        }
        if let Some(index) = remove {
            self.file.parameters.remove(index);
        }
        ui.horizontal_wrapped(|ui| {
            for kind in ParameterKind::ALL {
                if ui
                    .button(format!("{} {}", icons::PLUS, kind.label()))
                    .clicked()
                {
                    self.add_parameter(kind);
                }
            }
        });
        let duplicates = self.file.parameters.iter().enumerate().any(|(i, a)| {
            self.file.parameters[..i]
                .iter()
                .any(|b| b.name() == a.name())
        });
        if duplicates {
            ui.colored_label(
                ui.visuals().error_fg_color,
                "Two parameters have the same name.",
            );
        }
        ui.separator();
        ui.weak("Right-click the graph (or drop a clip on it) to add states; drag from a state's right pin to another to wire them.");
    }

    /// The default state, which Entry is wired to.
    fn default_state_picker(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.label("Default state");
            let names: Vec<String> = self.file.states.iter().map(|s| s.name.clone()).collect();
            egui::ComboBox::from_id_salt("default state")
                .selected_text(&self.file.default_state)
                .show_ui(ui, |ui| {
                    for name in names {
                        ui.selectable_value(&mut self.file.default_state, name.clone(), name);
                    }
                });
        });
        ui.weak("The state it starts in (Entry is wired to it, and it's orange): usually Idle.");
    }

    pub(super) fn free_parameter_name(&self, wanted: &str) -> String {
        (1u32..)
            .map(|n| match n {
                1 => wanted.to_owned(),
                n => format!("{wanted}{n}"),
            })
            .find(|name| !self.file.parameters.iter().any(|p| p.name() == name))
            .expect("some number is always free")
    }

    pub(super) fn state_inspector(&mut self, ui: &mut egui::Ui, index: usize, clips: &[String]) {
        let name = self.file.states[index].name.clone();
        match &mut self.renaming {
            Some((renaming, text)) if *renaming == index => {
                let mut done = false;
                let mut text = text.clone();
                ui.horizontal_wrapped(|ui| {
                    let response = ui.text_edit_singleline(&mut text);
                    if ui.button("Rename").clicked()
                        || (response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                    {
                        done = true;
                    }
                    if ui.button("Cancel").clicked() {
                        self.renaming = None;
                    }
                });
                if done {
                    match self.rename_state(index, &text) {
                        Ok(()) => self.renaming = None,
                        Err(problem) => {
                            ui.colored_label(ui.visuals().error_fg_color, problem);
                            self.renaming = Some((index, text));
                        }
                    }
                } else if self.renaming.is_some() {
                    self.renaming = Some((index, text));
                }
            }
            _ => {
                ui.horizontal_wrapped(|ui| {
                    ui.heading(&name);
                    if ui
                        .small_button(icons::PENCIL_SIMPLE)
                        .on_hover_text("Rename this state")
                        .clicked()
                    {
                        self.renaming = Some((index, name.clone()));
                    }
                });
            }
        }
        ui.label(words::state_header(&self.file, &name));
        let is_default = self.file.default_state == name;
        if is_default {
            ui.weak("The default state: the animator starts here.");
        } else if ui.button("Set as default state").clicked() {
            self.file.default_state = name.clone();
        }
        ui.add_space(4.0);
        // Clip or blend tree.
        let is_tree = matches!(self.file.states[index].motion, Motion::BlendTree(_));
        let mut make_tree = false;
        ui.horizontal_wrapped(|ui| {
            ui.label("Plays");
            if ui.selectable_label(!is_tree, "A clip").clicked() && is_tree {
                let first = self.file.states[index]
                    .motion
                    .clips()
                    .first()
                    .map(|clip| (*clip).clone())
                    .unwrap_or_default();
                self.file.states[index].motion = Motion::Clip(first);
            }
            if ui
                .selectable_label(is_tree, "A blend tree")
                .on_hover_text(
                    "Mix related clips by a Number parameter, such as Idle, Walk and Run by Speed",
                )
                .clicked()
                && !is_tree
            {
                make_tree = true;
            }
        });
        if make_tree {
            let parameter = self.number_parameter();
            let children = match &self.file.states[index].motion {
                Motion::Clip(clip) if !clip.is_empty() => vec![BlendChild {
                    clip: clip.clone(),
                    threshold: 0.0,
                }],
                _ => Vec::new(),
            };
            self.file.states[index].motion = Motion::BlendTree(BlendTree {
                parameter,
                children,
            });
        }
        if matches!(self.file.states[index].motion, Motion::BlendTree(_)) {
            self.blend_tree_editor(ui, index, clips);
        }
        crate::responsive::form(ui, "state settings", |ui| {
            let state = &mut self.file.states[index];
            if let Motion::Clip(current) = &mut state.motion {
                ui.label("Clip");
                clip_picker(ui, "state clip", current, clips);
                ui.end_row();
            }
            ui.label("Speed");
            ui.add(
                egui::DragValue::new(&mut state.speed)
                    .range(-10.0..=10.0)
                    .speed(0.01)
                    .suffix("×"),
            );
            ui.end_row();
        });
        self.return_editor(ui, index);
        ui.separator();
        ui.strong("Transitions");
        let outgoing: Vec<(usize, String, bool)> = self
            .file
            .transitions
            .iter()
            .enumerate()
            .filter(|(_, t)| t.from == name || t.to == name)
            .map(|(i, t)| {
                let out = t.from == name;
                (
                    i,
                    if out {
                        format!("→ {}", t.to)
                    } else {
                        format!("← {}", t.from)
                    },
                    out,
                )
            })
            .collect();
        if outgoing.is_empty() {
            ui.weak("None yet. Drag from its right pin to another state.");
        }
        for (index, label, _) in outgoing {
            if ui.selectable_label(false, label).clicked() {
                self.selected = Selected::Transition(index);
            }
        }
        ui.separator();
        if ui
            .button(format!("{} Delete state", icons::TRASH))
            .clicked()
        {
            self.delete(Selected::Node(Node::State(index)));
        }
    }

    /// A state's Return setting: when it goes back, to which state, and how
    /// smoothly. It makes a transition (dashed in the graph).
    pub(super) fn return_editor(&mut self, ui: &mut egui::Ui, index: usize) {
        ui.separator();
        let others: Vec<String> = self
            .file
            .states
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != index)
            .map(|(_, s)| s.name.clone())
            .collect();
        let parameters = self.file.parameters.clone();
        let default = self.file.default_state.clone();
        let state = &mut self.file.states[index];
        let mut on = state.returns.is_some();
        if ui
            .checkbox(&mut on, egui::RichText::new("Return when…").strong())
            .on_hover_text("Go back to another state by itself, such as Attack back to Idle when its clip finishes. This makes the transition for you.")
            .changed()
        {
            state.returns = on.then(|| ReturnDef {
                to: others
                    .iter()
                    .find(|name| **name == default)
                    .or(others.first())
                    .cloned()
                    .unwrap_or_default(),
                when: ReturnWhen::Finished,
                blend: DEFAULT_BLEND,
            });
        }
        let Some(returns) = &mut state.returns else {
            return;
        };
        let names = |of: fn(&ParameterDef) -> bool| -> Vec<String> {
            parameters
                .iter()
                .filter(|def| of(def))
                .map(|def| def.name().to_owned())
                .collect()
        };
        let bools = names(|def| matches!(def, ParameterDef::Bool { .. }));
        let triggers = names(|def| matches!(def, ParameterDef::Trigger { .. }));
        crate::responsive::form(ui, "return settings", |ui| {
            ui.label("When");
            let label = match &returns.when {
                ReturnWhen::Finished => "its clip finishes",
                ReturnWhen::BoolIs { .. } => "a Bool becomes",
                ReturnWhen::Triggered { .. } => "a Trigger is set",
                ReturnWhen::After { .. } => "some seconds pass",
            };
            egui::ComboBox::from_id_salt("return when")
                .selected_text(label)
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(
                            matches!(returns.when, ReturnWhen::Finished),
                            "its clip finishes",
                        )
                        .clicked()
                    {
                        returns.when = ReturnWhen::Finished;
                    }
                    if ui
                        .selectable_label(
                            matches!(returns.when, ReturnWhen::BoolIs { .. }),
                            "a Bool becomes",
                        )
                        .clicked()
                    {
                        returns.when = ReturnWhen::BoolIs {
                            parameter: bools.first().cloned().unwrap_or_default(),
                            value: false,
                        };
                    }
                    if ui
                        .selectable_label(
                            matches!(returns.when, ReturnWhen::Triggered { .. }),
                            "a Trigger is set",
                        )
                        .clicked()
                    {
                        returns.when = ReturnWhen::Triggered {
                            parameter: triggers.first().cloned().unwrap_or_default(),
                        };
                    }
                    if ui
                        .selectable_label(
                            matches!(returns.when, ReturnWhen::After { .. }),
                            "some seconds pass",
                        )
                        .clicked()
                    {
                        returns.when = ReturnWhen::After { seconds: 1.0 };
                    }
                });
            ui.end_row();
            match &mut returns.when {
                ReturnWhen::Finished => {}
                ReturnWhen::BoolIs { parameter, value } => {
                    ui.label("Bool");
                    ui.horizontal_wrapped(|ui| {
                        egui::ComboBox::from_id_salt("return bool")
                            .selected_text(parameter.as_str())
                            .show_ui(ui, |ui| {
                                for name in &bools {
                                    ui.selectable_value(parameter, name.clone(), name);
                                }
                            });
                        ui.label("is");
                        ui.selectable_value(value, true, "true");
                        ui.selectable_value(value, false, "false");
                    });
                    ui.end_row();
                }
                ReturnWhen::Triggered { parameter } => {
                    ui.label("Trigger");
                    egui::ComboBox::from_id_salt("return trigger")
                        .selected_text(parameter.as_str())
                        .show_ui(ui, |ui| {
                            for name in &triggers {
                                ui.selectable_value(parameter, name.clone(), name);
                            }
                        });
                    ui.end_row();
                }
                ReturnWhen::After { seconds } => {
                    ui.label("After");
                    ui.add(
                        egui::DragValue::new(seconds)
                            .range(0.0..=600.0)
                            .speed(0.05)
                            .suffix(" s"),
                    );
                    ui.end_row();
                }
            }
            ui.label("Back to");
            egui::ComboBox::from_id_salt("return to")
                .selected_text(returns.to.as_str())
                .show_ui(ui, |ui| {
                    for name in &others {
                        ui.selectable_value(&mut returns.to, name.clone(), name);
                    }
                });
            ui.end_row();
            ui.label("Blend");
            ui.horizontal_wrapped(|ui| {
                ui.add(egui::Slider::new(&mut returns.blend, 0.0..=2.0).suffix(" s"));
            });
            ui.end_row();
        });
        let missing = match &returns.when {
            ReturnWhen::BoolIs { parameter, .. } | ReturnWhen::Triggered { parameter } => {
                parameter.is_empty()
            }
            _ => false,
        };
        if missing {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                "Add a parameter of that kind (in the state machine's settings) to test.",
            );
        } else {
            ui.weak("Makes its transition for you (dashed in the graph).");
        }
    }

    pub(super) fn transition_inspector(&mut self, ui: &mut egui::Ui, index: usize) {
        let parameters = self.file.parameters.clone();
        let transition = &mut self.file.transitions[index];
        ui.heading(format!("{} → {}", transition.from, transition.to));
        ui.label(words::transition_header(transition));
        if transition.generated {
            ui.label("Made by its state's Return setting, so it can't be changed here.");
            let from = transition.from.clone();
            if ui.button("Edit the Return setting").clicked()
                && let Some(state) = self.file.states.iter().position(|s| s.name == from)
            {
                self.selected = Selected::Node(Node::State(state));
            }
            if ui
                .button(format!("{} Remove it", icons::TRASH))
                .on_hover_text("Turns off its state's Return setting")
                .clicked()
            {
                self.delete(Selected::Transition(index));
            }
            return;
        }
        ui.add_space(4.0);
        ui.strong("Conditions");
        ui.weak("All must hold for the transition to happen. Its wire's label says them in words.");
        let mut remove = None;
        egui::Grid::new("conditions")
            .num_columns(4)
            .spacing([6.0, 4.0])
            .show(ui, |ui| {
                for (c, condition) in transition.conditions.iter_mut().enumerate() {
                    egui::ComboBox::from_id_salt(("condition parameter", c))
                        .width(100.0)
                        .selected_text(&condition.parameter)
                        .show_ui(ui, |ui| {
                            for def in &parameters {
                                ui.selectable_value(
                                    &mut condition.parameter,
                                    def.name().to_owned(),
                                    def.name(),
                                );
                            }
                        });
                    let kind = parameters
                        .iter()
                        .find(|def| def.name() == condition.parameter)
                        .map(ParameterKind::of);
                    let tests = super::wire_editor::tests_for(kind);
                    if let Some(&(first, _)) = tests.first()
                        && !tests.iter().any(|(test, _)| *test == condition.test)
                    {
                        condition.test = first;
                    }
                    let label = tests
                        .iter()
                        .find(|(test, _)| *test == condition.test)
                        .map_or("(pick a parameter)", |(_, label)| label);
                    egui::ComboBox::from_id_salt(("condition test", c))
                        .width(90.0)
                        .selected_text(label)
                        .show_ui(ui, |ui| {
                            for &(test, label) in tests {
                                ui.selectable_value(&mut condition.test, test, label);
                            }
                        });
                    if matches!(condition.test, Test::Greater | Test::Less) {
                        ui.add(egui::DragValue::new(&mut condition.value).speed(0.05));
                    } else {
                        ui.label("");
                    }
                    if ui.small_button(icons::X).on_hover_text("Remove").clicked() {
                        remove = Some(c);
                    }
                    ui.end_row();
                }
            });
        if let Some(c) = remove {
            transition.conditions.remove(c);
        }
        if parameters.is_empty() {
            ui.weak("Add parameters in the state machine's settings (click the graph's background) to test them here.");
        } else if ui
            .button(format!("{} Add condition", icons::PLUS))
            .clicked()
        {
            self.add_condition(index, parameters[0].name());
        }
        let transition = &mut self.file.transitions[index];

        ui.separator();
        ui.strong("Blend");
        ui.horizontal_wrapped(|ui| {
            ui.add(
                egui::Slider::new(&mut transition.blend, 0.0..=2.0)
                    .suffix(" s")
                    .text("duration"),
            )
            .on_hover_text("How long the old pose takes to turn into the new one");
        });
        ui.horizontal_wrapped(|ui| {
            for (label, seconds) in BLENDS {
                if ui
                    .selectable_label((transition.blend - seconds).abs() < 1e-9, label)
                    .clicked()
                {
                    transition.blend = seconds;
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("Preview the blend:");
            ui.add(egui::Slider::new(&mut self.scrub, 0.0..=1.0).show_value(false));
        });
        let new = (self.scrub * 100.0).round();
        ui.weak(format!(
            "{}: {:.0}%  ·  {}: {new:.0}%{}",
            transition.from,
            100.0 - new,
            transition.to,
            if self.scrub >= 1.0 {
                "  (drag left to see the mix)"
            } else {
                ""
            }
        ));

        ui.separator();
        crate::responsive::form(ui, "transition settings", |ui| {
            ui.label("Exit time");
            ui.horizontal_wrapped(|ui| {
                let mut on = transition.exit_time.is_some();
                if ui
                    .checkbox(&mut on, "")
                    .on_hover_text(
                        "Only once the state has played this much of its clip (1 is once through)",
                    )
                    .changed()
                {
                    transition.exit_time = on.then_some(0.9);
                }
                if let Some(exit) = &mut transition.exit_time {
                    ui.add(egui::DragValue::new(exit).range(0.0..=10.0).speed(0.01));
                    ui.weak("of the clip");
                }
            });
            ui.end_row();
            ui.label("Interruptible");
            ui.checkbox(&mut transition.interruptible, "")
                .on_hover_text("On, another transition can take over while this one blends");
            ui.end_row();
            ui.label("Priority");
            ui.add(egui::DragValue::new(&mut transition.priority).range(-100..=100))
                .on_hover_text("When several transitions could happen at once, the highest goes");
            ui.end_row();
        });
        if transition.conditions.is_empty() && transition.exit_time.is_none() {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                "This never happens: add a condition, or turn on Exit time to go on by itself.",
            );
        } else if transition.conditions.is_empty() {
            ui.weak("With no conditions, it happens by itself at its exit time.");
        }
        ui.separator();
        if ui
            .button(format!("{} Delete transition", icons::TRASH))
            .clicked()
        {
            self.delete(Selected::Transition(index));
        }
    }

    /// The blend tree of state `index`: its parameter, each clip with its
    /// threshold (a slider and a number) and its share now in the preview,
    /// a line of the thresholds with the value now, and Add Locomotion.
    pub(super) fn blend_tree_editor(&mut self, ui: &mut egui::Ui, index: usize, clips: &[String]) {
        // The preview's mix now, and its parameter's value.
        let state_name = self.file.states[index].name.clone();
        let (weights, value) = match &self.preview {
            Some(run) => {
                let state = run.compiled.state_index(&state_name);
                let weights = state.and_then(|s| run.compiled.tree_weights(s, &run.parameters));
                let Motion::BlendTree(tree) = &self.file.states[index].motion else {
                    return;
                };
                let value = match run.parameters.get(&tree.parameter) {
                    Some(Parameter::Number(value)) => Some(*value),
                    _ => None,
                };
                (weights, value)
            }
            None => (None, None),
        };
        let numbers: Vec<String> = self
            .file
            .parameters
            .iter()
            .filter(|def| matches!(def, ParameterDef::Number { .. }))
            .map(|def| def.name().to_owned())
            .collect();
        let mut locomotion = false;
        let Motion::BlendTree(tree) = &mut self.file.states[index].motion else {
            return;
        };
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.strong(format!("{} Blend tree", icons::SLIDERS_HORIZONTAL));
            ui.horizontal_wrapped(|ui| {
                ui.label("By");
                egui::ComboBox::from_id_salt("tree parameter")
                    .selected_text(&tree.parameter)
                    .show_ui(ui, |ui| {
                        for name in &numbers {
                            ui.selectable_value(&mut tree.parameter, name.clone(), name);
                        }
                    });
                ui.weak("(a Number parameter)");
            });
            let low = tree
                .children
                .iter()
                .map(|c| c.threshold)
                .fold(0.0_f64, f64::min);
            let high = tree
                .children
                .iter()
                .map(|c| c.threshold)
                .fold(6.0_f64, f64::max);
            let mut remove = None;
            egui::Grid::new("tree children")
                .num_columns(4)
                .spacing([6.0, 4.0])
                .show(ui, |ui| {
                    ui.weak("Clip");
                    ui.weak("Threshold");
                    ui.weak("Share");
                    ui.end_row();
                    for (c, child) in tree.children.iter_mut().enumerate() {
                        clip_picker(ui, ("tree clip", c), &mut child.clip, clips);
                        ui.horizontal_wrapped(|ui| {
                            ui.add(
                                egui::Slider::new(&mut child.threshold, low..=high)
                                    .show_value(false),
                            );
                            ui.add(egui::DragValue::new(&mut child.threshold).speed(0.05));
                        });
                        let share = weights.as_ref().and_then(|w| w.get(c)).copied().unwrap_or(0.0);
                        ui.add(
                            egui::ProgressBar::new(share as f32)
                                .desired_width(60.0)
                                .text(format!("{:.0}%", share * 100.0)),
                        );
                        if ui.small_button(icons::X).on_hover_text("Remove").clicked() {
                            remove = Some(c);
                        }
                        ui.end_row();
                    }
                });
            if let Some(c) = remove {
                tree.children.remove(c);
            }
            ui.horizontal_wrapped(|ui| {
                if ui.button(format!("{} Add clip", icons::PLUS)).clicked() {
                    let threshold = tree
                        .children
                        .iter()
                        .map(|c| c.threshold + 1.0)
                        .fold(0.0, f64::max);
                    tree.children.push(BlendChild {
                        clip: String::new(),
                        threshold,
                    });
                }
                if ui
                    .button("Sort")
                    .on_hover_text("Put the clips in threshold order")
                    .clicked()
                {
                    tree.children
                        .sort_by(|a, b| a.threshold.total_cmp(&b.threshold));
                }
                if ui
                    .button("Add Locomotion")
                    .on_hover_text("Speed (0–6 m/s): Idle at 0, Walk at 2, Run at 6, with the project's clips of those names")
                    .clicked()
                {
                    locomotion = true;
                }
            });
            threshold_line(ui, tree, value, low, high);
        });
        if locomotion {
            let folder = self.file.states[index]
                .motion
                .clips()
                .into_iter()
                .find(|clip| !clip.is_empty())
                .map(|clip| super::super::store::group_of(clip).to_owned());
            let parameter = if self
                .file
                .parameters
                .iter()
                .any(|def| def.name() == "Speed" && matches!(def, ParameterDef::Number { .. }))
            {
                "Speed".to_owned()
            } else if self.file.parameters.iter().any(|def| def.name() == "Speed") {
                self.number_parameter()
            } else {
                self.file.parameters.push(ParameterDef::Number {
                    name: "Speed".to_owned(),
                    default: 0.0,
                });
                "Speed".to_owned()
            };
            let children = [("idle", 0.0), ("walk", 2.0), ("run", 6.0)]
                .into_iter()
                .map(|(word, threshold)| BlendChild {
                    clip: clip_named(clips, word, folder.as_deref()).unwrap_or_default(),
                    threshold,
                })
                .collect();
            self.file.states[index].motion = Motion::BlendTree(BlendTree {
                parameter,
                children,
            });
        }
    }
}
