//! An animator controller open in the Animation panel: the graph's
//! selection, the inspector for the controller, a state or a transition,
//! and the live preview that runs the state machine with its own
//! parameters (never the game's).

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use axcenity_anim::files::{
    self, BlendChild, BlendTree, Condition, Motion, ParameterDef, StateDef, StateMachineFile, Test,
    TransitionDef,
};
use axcenity_anim::{
    ANY_STATE, Compiled, DEFAULT_BLEND, Fade, Machine, Playing, ReturnDef, ReturnWhen,
};
use axcenity_assets::{Assets, Posed};
use axcenity_core::Parameter;
use eframe::egui;

use super::graph::{GraphEvent, GraphView, Live, Node, Selected, from_node};
use crate::theme::icons;

/// Blend presets: Instant, Smooth and Slow, in seconds.
const BLENDS: [(&str, f64); 3] = [("Instant", 0.0), ("Smooth", DEFAULT_BLEND), ("Slow", 0.75)];

/// A controller being looked at and changed.
pub struct ControllerDoc {
    pub path: String,
    pub file: StateMachineFile,
    written: StateMachineFile,
    pub selected: Selected,
    pub view: GraphView,
    /// A state being renamed, and the name typed so far.
    renaming: Option<(usize, String)>,
    /// The model the preview shows (a `MeshId`).
    pub preview_model: Option<String>,
    preview: Option<PreviewRun>,
    pub playing: bool,
    /// The transition preview's slider: how much of the new pose shows.
    pub scrub: f64,
    /// Each clip's pass length in seconds (0 if it can't be read), for
    /// Return settings given in seconds.
    clip_lengths: std::collections::HashMap<String, f64>,
}

/// The state machine running in the preview.
struct PreviewRun {
    /// What it was made from: the model's key and the controller without
    /// its graph positions (moving a box doesn't start it again).
    made_from: (u64, StateMachineFile),
    compiled: Arc<Compiled>,
    machine: Machine,
    parameters: BTreeMap<String, Parameter>,
    problems: Vec<String>,
}

/// The controller as far as playing it goes: without where boxes are drawn.
fn behaviour(file: &StateMachineFile) -> StateMachineFile {
    let mut file = file.clone();
    file.any_state_position = None;
    for state in &mut file.states {
        state.position = [0.0, 0.0];
    }
    file
}

impl ControllerDoc {
    pub fn open(path: String, file: StateMachineFile, preview_model: Option<String>) -> Self {
        Self {
            path,
            written: file.clone(),
            file,
            selected: Selected::Controller,
            view: GraphView::default(),
            renaming: None,
            preview_model,
            preview: None,
            playing: true,
            scrub: 1.0,
            clip_lengths: std::collections::HashMap::new(),
        }
    }

    /// Makes the Return settings' transitions again, so they always match
    /// the settings (see `files::with_returns`).
    pub fn sync_returns(&mut self, project: &Path) {
        let lengths = &mut self.clip_lengths;
        let made = files::with_returns(&self.file, |state| {
            let clip = state
                .motion
                .clips()
                .into_iter()
                .find(|clip| !clip.is_empty())?;
            let length = *lengths.entry(clip.clone()).or_insert_with(|| {
                super::store::read_any_clip(project, clip)
                    .map(|clip| clip.duration)
                    .unwrap_or(0.0)
            });
            (length > 0.0).then(|| length / state.speed.abs().max(1e-3))
        });
        if made != self.file {
            self.file = made;
            if let Selected::Transition(index) = self.selected
                && index >= self.file.transitions.len()
            {
                self.selected = Selected::Controller;
            }
        }
    }

    pub fn unwritten(&self) -> bool {
        self.file != self.written
    }

    pub fn write(&mut self, project: &Path) -> std::io::Result<bool> {
        if !self.unwritten() {
            return Ok(false);
        }
        files::write_file(&project.join(&self.path), &self.file.to_json())?;
        self.written = self.file.clone();
        Ok(true)
    }

    /// Takes the file as it is on disk now (after a clip rename rewrote it).
    pub fn reload(&mut self, file: StateMachineFile) {
        self.written = file.clone();
        self.file = file;
    }

    /// The first clip's model, for a preview when nothing better is known.
    pub fn first_clip_source(&self, project: &Path) -> Option<String> {
        self.file
            .states
            .iter()
            .flat_map(|state| state.motion.clips())
            .filter(|path| !path.is_empty())
            .find_map(|path| super::store::read_clip(project, path).ok())
            .and_then(|clip| clip.link.map(|link| link.source))
    }

    /// Does what the graph reports.
    pub fn apply(&mut self, events: Vec<GraphEvent>) {
        for event in events {
            match event {
                GraphEvent::Select(selected) => self.selected = selected,
                GraphEvent::Move(node, position) => match node {
                    Node::State(index) => {
                        if let Some(state) = self.file.states.get_mut(index) {
                            state.position = position;
                        }
                    }
                    Node::Any => self.file.any_state_position = Some(position),
                },
                GraphEvent::Link(from, to) => {
                    let from = match from {
                        Node::State(index) => self.file.states[index].name.clone(),
                        Node::Any => ANY_STATE.to_owned(),
                    };
                    let to = self.file.states[to].name.clone();
                    self.file.transitions.push(TransitionDef::new(&from, &to));
                    self.selected = Selected::Transition(self.file.transitions.len() - 1);
                }
                GraphEvent::SetDefault(index) => {
                    self.file.default_state = self.file.states[index].name.clone();
                }
                GraphEvent::Rename(index) => {
                    self.renaming = Some((index, self.file.states[index].name.clone()));
                    self.selected = Selected::Node(Node::State(index));
                }
                GraphEvent::Delete(selected) => self.delete(selected),
                GraphEvent::AddBlendTree(at) => {
                    let name = self.free_state_name("Blend Tree");
                    let parameter = self.number_parameter();
                    if self.file.states.is_empty() {
                        self.file.default_state = name.clone();
                    }
                    self.file.states.push(StateDef::new(
                        &name,
                        Motion::BlendTree(BlendTree {
                            parameter,
                            children: Vec::new(),
                        }),
                        at,
                    ));
                    self.selected = Selected::Node(Node::State(self.file.states.len() - 1));
                }
                // The Animation tab opens the state's clip.
                GraphEvent::Open(_) => {}
                GraphEvent::AddState(at, clip) => {
                    let wanted = clip
                        .as_deref()
                        .map_or("New State", axcenity_anim::files::clip_name);
                    let name = self.free_state_name(wanted);
                    if self.file.states.is_empty() {
                        self.file.default_state = name.clone();
                    }
                    self.file.states.push(StateDef::new(
                        &name,
                        Motion::Clip(clip.unwrap_or_default()),
                        at,
                    ));
                    self.selected = Selected::Node(Node::State(self.file.states.len() - 1));
                }
            }
        }
    }

    /// The first Number parameter's name, adding `Speed` if there's none.
    fn number_parameter(&mut self) -> String {
        if let Some(def) = self
            .file
            .parameters
            .iter()
            .find(|def| matches!(def, ParameterDef::Number { .. }))
        {
            return def.name().to_owned();
        }
        let name = self.free_parameter_name("Speed");
        self.file.parameters.push(ParameterDef::Number {
            name: name.clone(),
            default: 0.0,
        });
        name
    }

    fn free_state_name(&self, wanted: &str) -> String {
        (1u32..)
            .map(|n| match n {
                1 => wanted.to_owned(),
                n => format!("{wanted} {n}"),
            })
            .find(|name| *name != ANY_STATE && self.file.state(name).is_none())
            .expect("some number is always free")
    }

    /// Deletes a state (and its transitions) or a transition.
    fn delete(&mut self, selected: Selected) {
        match selected {
            Selected::Node(Node::State(index)) if index < self.file.states.len() => {
                let name = self.file.states.remove(index).name;
                self.file
                    .transitions
                    .retain(|t| t.from != name && t.to != name);
                for state in &mut self.file.states {
                    if state.returns.as_ref().is_some_and(|r| r.to == name) {
                        state.returns = None;
                    }
                }
                if self.file.default_state == name {
                    self.file.default_state = self
                        .file
                        .states
                        .first()
                        .map(|s| s.name.clone())
                        .unwrap_or_default();
                }
                self.selected = Selected::Controller;
            }
            Selected::Transition(index) if index < self.file.transitions.len() => {
                let transition = self.file.transitions.remove(index);
                // A made one goes with its state's Return setting.
                if transition.generated
                    && let Some(state) = self
                        .file
                        .states
                        .iter_mut()
                        .find(|state| state.name == transition.from)
                {
                    state.returns = None;
                }
                self.selected = Selected::Controller;
            }
            _ => {}
        }
    }

    /// Renames state `index` to `name`, and every transition naming it.
    fn rename_state(&mut self, index: usize, name: &str) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("A state needs a name.".to_owned());
        }
        if name == ANY_STATE {
            return Err(format!("`{ANY_STATE}` is taken by the Any State."));
        }
        if self
            .file
            .states
            .iter()
            .enumerate()
            .any(|(i, s)| i != index && s.name == name)
        {
            return Err(format!("There's already a state called `{name}`."));
        }
        let old = std::mem::replace(&mut self.file.states[index].name, name.to_owned());
        for transition in &mut self.file.transitions {
            if transition.from == old {
                transition.from = name.to_owned();
            }
            if transition.to == old {
                transition.to = name.to_owned();
            }
        }
        if self.file.default_state == old {
            self.file.default_state = name.to_owned();
        }
        for state in &mut self.file.states {
            if let Some(returns) = &mut state.returns
                && returns.to == old
            {
                returns.to = name.to_owned();
            }
        }
        Ok(())
    }

    /// Runs the preview's state machine on by `dt` seconds, rebuilding it
    /// when the controller or model changed. Returns the posed model.
    pub fn tick(&mut self, assets: &Assets, dt: f64) -> Result<Arc<Posed>, String> {
        let mesh_id = self
            .preview_model
            .clone()
            .ok_or("Pick a model to preview on below.")?;
        let model = assets
            .model(&mesh_id)
            .ok_or_else(|| format!("`{mesh_id}` can't be loaded"))?;
        let rig = model
            .rig
            .clone()
            .ok_or_else(|| format!("`{mesh_id}` has no skeleton"))?;
        if self.file.states.is_empty() {
            return Err("Add a state to preview it.".to_owned());
        }
        let made_from = (model.key(), behaviour(&self.file));
        if self
            .preview
            .as_ref()
            .is_none_or(|run| run.made_from != made_from)
        {
            let (compiled, problems) = Compiled::new(self.file.clone(), Some(&rig), |path| {
                axcenity_anim::load_clip(assets, path)
            });
            let compiled = Arc::new(compiled);
            // Keep playing where it was if that state is still there; keep
            // the parameters that still mean the same.
            let old = self.preview.take();
            let mut machine = Machine::new(&compiled);
            let mut parameters: BTreeMap<String, Parameter> = self
                .file
                .parameters
                .iter()
                .map(|def| (def.name().to_owned(), def.initial()))
                .collect();
            if let Some(old) = old {
                let name = old
                    .compiled
                    .state_name(old.machine.current.state)
                    .to_owned();
                if let Some(state) = compiled.state_index(&name) {
                    machine.current = Playing {
                        state,
                        phase: old.machine.current.phase,
                    };
                }
                for (name, value) in old.parameters {
                    if let Some(slot) = parameters.get_mut(&name)
                        && slot.kind() == value.kind()
                    {
                        *slot = value;
                    }
                }
            }
            self.preview = Some(PreviewRun {
                made_from,
                compiled,
                machine,
                parameters,
                problems,
            });
        }
        let run = self.preview.as_mut().expect("made above");
        if self.playing {
            run.machine
                .advance(&run.compiled, Some(&rig), &run.parameters, dt);
            run.machine.evaluate(&run.compiled, &mut run.parameters);
        }
        // Scrubbing a selected transition shows its two poses mixed.
        let pose = match self.selected {
            Selected::Transition(index) if self.scrub < 1.0 => {
                let transition = &self.file.transitions[index];
                let to = run.compiled.state_index(&transition.to);
                let from = match from_node(&self.file, &transition.from) {
                    Some(Node::State(state)) => Some(state),
                    _ => Some(run.machine.current.state),
                };
                match (from, to) {
                    (Some(from), Some(to)) => {
                        let phase = run.machine.current.phase;
                        let mixed = Machine {
                            current: Playing { state: to, phase },
                            fade: Some(Fade {
                                from: Playing { state: from, phase },
                                elapsed: 0.0,
                                duration: 1.0,
                                interruptible: true,
                                transition: Some(index),
                            }),
                        };
                        mixed.pose_at(&run.compiled, &rig, &run.parameters, self.scrub)
                    }
                    _ => run.machine.pose(&run.compiled, &rig, &run.parameters),
                }
            }
            _ => run.machine.pose(&run.compiled, &rig, &run.parameters),
        };
        model
            .pose(&pose)
            .map(Arc::new)
            .ok_or_else(|| "the model can't be posed".to_owned())
    }

    /// What the graph shows of the preview: the state playing and the blend.
    pub fn live(&self) -> Live {
        let Some(run) = &self.preview else {
            return Live::default();
        };
        let current = run.machine.current;
        let through = if run.compiled.looping(current.state, &run.parameters) {
            current.phase.rem_euclid(1.0)
        } else {
            current.phase.min(1.0)
        };
        let state = self
            .file
            .states
            .iter()
            .position(|s| s.name == run.compiled.state_name(current.state));
        Live {
            state: state.map(|state| (state, through as f32)),
            fade: run.machine.fade.and_then(|fade| {
                let from = self
                    .file
                    .states
                    .iter()
                    .position(|s| s.name == run.compiled.state_name(fade.from.state))?;
                Some((from, fade.transition, fade.weight() as f32))
            }),
        }
    }

    /// Starts the preview again from the default state with the default
    /// parameters.
    pub fn restart(&mut self) {
        self.preview = None;
        self.playing = true;
    }

    /// The preview's parameter controls, the state playing and its blend.
    pub fn live_controls(&mut self, ui: &mut egui::Ui) {
        let Some(run) = &mut self.preview else {
            return;
        };
        let state = run
            .compiled
            .state_name(run.machine.current.state)
            .to_owned();
        ui.horizontal_wrapped(|ui| {
            ui.strong(format!("{} {state}", icons::PLAY_CIRCLE));
            if let Some(fade) = run.machine.fade {
                ui.weak(format!(
                    "blending from {}: {:.0}% new",
                    run.compiled.state_name(fade.from.state),
                    fade.weight() * 100.0
                ));
            }
        });
        for problem in &run.problems {
            ui.colored_label(ui.visuals().warn_fg_color, problem);
        }
        if self.file.parameters.is_empty() {
            ui.weak("Add parameters (in the state machine's settings) to drive transitions here.");
        }
        let ranges = number_ranges(&self.file);
        crate::responsive::form(ui, "live parameters", |ui| {
            for def in &self.file.parameters {
                let name = def.name().to_owned();
                ui.label(&name);
                let slot = run
                    .parameters
                    .entry(name.clone())
                    .or_insert_with(|| def.initial());
                match slot {
                    Parameter::Bool(value) => {
                        ui.checkbox(value, "");
                    }
                    Parameter::Number(value) => {
                        let (low, high) = ranges.get(&name).copied().unwrap_or((0.0, 10.0));
                        ui.add(egui::Slider::new(value, low..=high));
                    }
                    Parameter::Trigger(set) => {
                        ui.horizontal_wrapped(|ui| {
                            if ui.button("Fire").clicked() {
                                *set = true;
                            }
                            if *set {
                                ui.weak("waiting for a transition");
                            }
                        });
                    }
                }
                ui.end_row();
            }
        });
    }

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
                ui.label("Transitions from the Any State can happen whatever state is playing (but never to the state already playing). Use them for things that can happen at any time, such as Jump or TakeDamage.");
                ui.weak("Right-click it in the graph and pick Make Transition.");
            }
            _ => self.controller_inspector(ui),
        }
    }

    fn controller_inspector(&mut self, ui: &mut egui::Ui) {
        ui.heading(
            axcenity_anim::files::clip_name(&self.path).trim_end_matches(".statemachine.json"),
        );
        ui.weak(&self.path);
        ui.add_space(4.0);
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
        ui.weak("The state it starts in (orange in the graph): usually Idle.");
        ui.separator();
        ui.strong("Parameters");
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
            for kind in ["Bool", "Number", "Trigger"] {
                if ui.button(format!("{} {kind}", icons::PLUS)).clicked() {
                    let name = self.free_parameter_name(match kind {
                        "Bool" => "IsGrounded",
                        "Number" => "Speed",
                        _ => "Jump",
                    });
                    self.file.parameters.push(match kind {
                        "Bool" => ParameterDef::Bool {
                            name,
                            default: false,
                        },
                        "Number" => ParameterDef::Number { name, default: 0.0 },
                        _ => ParameterDef::Trigger { name },
                    });
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
        ui.weak("Right-click the graph to add states; right-click a state to make a transition.");
    }

    fn free_parameter_name(&self, wanted: &str) -> String {
        (1u32..)
            .map(|n| match n {
                1 => wanted.to_owned(),
                n => format!("{wanted}{n}"),
            })
            .find(|name| !self.file.parameters.iter().any(|p| p.name() == name))
            .expect("some number is always free")
    }

    fn state_inspector(&mut self, ui: &mut egui::Ui, index: usize, clips: &[String]) {
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
            ui.weak("None yet. Right-click the state in the graph and pick Make Transition.");
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
    fn return_editor(&mut self, ui: &mut egui::Ui, index: usize) {
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

    fn transition_inspector(&mut self, ui: &mut egui::Ui, index: usize) {
        let parameters = self.file.parameters.clone();
        let transition = &mut self.file.transitions[index];
        ui.heading(format!("{} → {}", transition.from, transition.to));
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
        ui.weak("All must hold for the transition to happen.");
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
                        .find(|def| def.name() == condition.parameter);
                    let tests: &[(Test, &str)] = match kind {
                        Some(ParameterDef::Bool { .. }) => {
                            &[(Test::IsTrue, "is true"), (Test::IsFalse, "is false")]
                        }
                        Some(ParameterDef::Trigger { .. }) => &[(Test::Triggered, "is set")],
                        Some(ParameterDef::Number { .. }) => &[
                            (Test::Greater, "is more than"),
                            (Test::Less, "is less than"),
                        ],
                        None => &[],
                    };
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
            let def = &parameters[0];
            transition.conditions.push(Condition {
                parameter: def.name().to_owned(),
                test: match def {
                    ParameterDef::Bool { .. } => Test::IsTrue,
                    ParameterDef::Number { .. } => Test::Greater,
                    ParameterDef::Trigger { .. } => Test::Triggered,
                },
                value: 0.0,
            });
        }

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
}

/// A combo box picking one of the project's `clips` (or none) for `current`.
fn clip_picker(
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
                    super::store::group_of(clip)
                );
                ui.selectable_value(current, clip.clone(), label);
            }
        });
}

/// The clip in `clips` whose name has `word` in it, preferring ones in
/// `folder` (a clip's group).
fn clip_named(clips: &[String], word: &str, folder: Option<&str>) -> Option<String> {
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
        .find(|clip| folder.is_some_and(|folder| super::store::group_of(clip) == folder))
        .or(matching.first())
        .map(|clip| (*clip).clone())
}

impl ControllerDoc {
    /// The blend tree of state `index`: its parameter, each clip with its
    /// threshold (a slider and a number) and its share now in the preview,
    /// a line of the thresholds with the value now, and Add Locomotion.
    fn blend_tree_editor(&mut self, ui: &mut egui::Ui, index: usize, clips: &[String]) {
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
                .map(|clip| super::store::group_of(clip).to_owned());
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

/// The blend tree's thresholds on a line, each clip's name over its tick,
/// with the parameter's value now as a marker.
fn threshold_line(ui: &mut egui::Ui, tree: &BlendTree, value: Option<f64>, low: f64, high: f64) {
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
fn number_ranges(file: &StateMachineFile) -> BTreeMap<String, (f64, f64)> {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn doc() -> ControllerDoc {
        let file = StateMachineFile {
            default_state: "Idle".into(),
            states: vec![
                StateDef::new("Idle", Motion::Clip(String::new()), [0.0, 0.0]),
                StateDef::new("Walk", Motion::Clip(String::new()), [200.0, 0.0]),
            ],
            ..StateMachineFile::default()
        };
        ControllerDoc::open(
            "assets/animations/hero.statemachine.json".into(),
            file,
            None,
        )
    }

    #[test]
    fn a_return_setting_keeps_its_transition_in_step() {
        let mut doc = doc();
        let nowhere = std::path::Path::new("no such project");
        doc.file.states[1].returns = Some(ReturnDef {
            to: "Idle".into(),
            when: ReturnWhen::Finished,
            blend: 0.1,
        });
        doc.sync_returns(nowhere);
        let made = &doc.file.transitions[0];
        assert!(made.generated);
        assert_eq!((made.from.as_str(), made.to.as_str()), ("Walk", "Idle"));
        // Renaming the state it returns to keeps it pointing there.
        doc.rename_state(0, "Rest").unwrap();
        doc.sync_returns(nowhere);
        assert_eq!(doc.file.states[1].returns.as_ref().unwrap().to, "Rest");
        assert_eq!(doc.file.transitions[0].to, "Rest");
        // Removing the made transition turns the setting off.
        doc.delete(Selected::Transition(0));
        doc.sync_returns(nowhere);
        assert!(doc.file.states[1].returns.is_none());
        assert!(doc.file.transitions.is_empty());
    }

    #[test]
    fn linking_states_makes_a_transition_and_selects_it() {
        let mut doc = doc();
        doc.apply(vec![GraphEvent::Link(Node::State(0), 1)]);
        assert_eq!(doc.file.transitions[0].from, "Idle");
        assert_eq!(doc.file.transitions[0].to, "Walk");
        assert_eq!(doc.selected, Selected::Transition(0));
        doc.apply(vec![GraphEvent::Link(Node::Any, 0)]);
        assert_eq!(doc.file.transitions[1].from, ANY_STATE);
    }

    #[test]
    fn renaming_and_deleting_states_keep_transitions_right() {
        let mut doc = doc();
        doc.apply(vec![
            GraphEvent::Link(Node::State(0), 1),
            GraphEvent::Link(Node::State(1), 0),
        ]);
        doc.rename_state(0, "Rest").unwrap();
        assert_eq!(doc.file.default_state, "Rest");
        assert_eq!(doc.file.transitions[0].from, "Rest");
        assert_eq!(doc.file.transitions[1].to, "Rest");
        assert!(doc.rename_state(0, "Walk").is_err());
        assert!(doc.rename_state(0, ANY_STATE).is_err());
        doc.delete(Selected::Node(Node::State(1)));
        assert!(doc.file.transitions.is_empty());
        assert_eq!(doc.file.states.len(), 1);
    }

    #[test]
    fn added_states_get_free_names_and_moving_doesnt_change_behaviour() {
        let mut doc = doc();
        doc.apply(vec![
            GraphEvent::AddState([10.0, 20.0], None),
            GraphEvent::AddState(
                [10.0, 20.0],
                Some("assets/animations/hero/Walk.anim.json".into()),
            ),
        ]);
        assert_eq!(doc.file.states[2].name, "New State");
        assert_eq!(doc.file.states[3].name, "Walk 2");
        let before = behaviour(&doc.file);
        doc.apply(vec![GraphEvent::Move(Node::State(0), [99.0, 99.0])]);
        assert_eq!(behaviour(&doc.file), before);
        assert_eq!(doc.file.states[0].position, [99.0, 99.0]);
    }
}
