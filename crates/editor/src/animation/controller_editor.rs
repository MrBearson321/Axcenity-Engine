//! An animator controller open in the Animation panel: the graph's
//! selection, the inspector for the controller, a state or a transition,
//! and the live preview that runs the state machine with its own
//! parameters (never the game's).

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use axcenity_anim::files::{
    self, BlendChild, BlendSpace, BlendSpaceChild, BlendTree, Condition, Motion, ParameterDef,
    StateDef, StateMachineFile, Test, TransitionDef,
};
use axcenity_anim::{
    ANY_STATE, Compiled, DEFAULT_BLEND, Fade, Machine, Playing, ReturnDef, ReturnWhen,
};
use axcenity_assets::{Assets, Posed};
use axcenity_core::Parameter;
use eframe::egui;

use super::graph::{GraphEvent, GraphView, Live, Node, ParameterKind, Selected, from_node};
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
    /// A parameter being named in a wire's label editor.
    new_parameter: Option<wire_editor::NewParameter>,
    /// While a layer is open (see `layers.rs`): the whole file, and the
    /// layer's index. `file` then holds that layer's states.
    base: Option<StateMachineFile>,
    layer: usize,
    /// The preview model's bones, by path, for picking a layer's mask.
    bones: Vec<String>,
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
    file.entry_position = None;
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
            new_parameter: None,
            base: None,
            layer: 0,
            bones: Vec::new(),
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
        self.merged() != self.written
    }

    pub fn write(&mut self, project: &Path) -> std::io::Result<bool> {
        if !self.unwritten() {
            return Ok(false);
        }
        let file = self.merged();
        files::write_file(&project.join(&self.path), &file.to_json())?;
        self.written = file;
        Ok(true)
    }

    /// Takes the file as it is on disk now (after a clip rename rewrote it).
    /// An open layer stays open if the file still has it.
    pub fn reload(&mut self, file: StateMachineFile) {
        let layer = self.open_layer_index();
        self.written = file.clone();
        self.file = file;
        self.base = None;
        if layer.is_some() {
            self.show_layer(layer);
        }
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
                    Node::Entry => self.file.entry_position = Some(position),
                },
                GraphEvent::Link(Node::Entry, to) => {
                    self.file.default_state = self.file.states[to].name.clone();
                }
                GraphEvent::Link(from, to) => {
                    let index = self.link(from, to);
                    self.selected = Selected::Transition(index);
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
                GraphEvent::AddState(at, clip, from) => {
                    let wanted = clip
                        .as_deref()
                        .map_or("New State", axcenity_anim::files::clip_name);
                    let name = self.free_state_name(wanted);
                    if self.file.states.is_empty() || from == Some(Node::Entry) {
                        self.file.default_state = name.clone();
                    }
                    self.file.states.push(StateDef::new(
                        &name,
                        Motion::Clip(clip.unwrap_or_default()),
                        at,
                    ));
                    let index = self.file.states.len() - 1;
                    if let Some(from) = from.filter(|from| *from != Node::Entry) {
                        self.link(from, index);
                    }
                    self.selected = Selected::Node(Node::State(index));
                }
                GraphEvent::Duplicate(index) => {
                    if let Some(state) = self.file.states.get(index) {
                        let mut copy = state.clone();
                        copy.position = [state.position[0] + 30.0, state.position[1] + 30.0];
                        self.add_copy(copy);
                    }
                }
                GraphEvent::Paste(state) => self.add_copy(state),
                GraphEvent::AddCondition(transition, parameter) => {
                    self.add_condition(transition, &parameter);
                    self.selected = Selected::Transition(transition);
                }
                GraphEvent::AddParameter(kind) => {
                    self.add_parameter(kind);
                }
            }
        }
    }

    /// Adds `state` (a copy) under a free name, without its Return
    /// setting, and selects it.
    fn add_copy(&mut self, mut state: StateDef) {
        state.name = self.free_state_name(&state.name);
        state.returns = None;
        if self.file.states.is_empty() {
            self.file.default_state = state.name.clone();
        }
        self.file.states.push(state);
        self.selected = Selected::Node(Node::State(self.file.states.len() - 1));
    }

    /// A new transition from `from` to state `to`, put before the ones
    /// Return settings make (which stay last), so the index stays right.
    /// Returns its index.
    fn link(&mut self, from: Node, to: usize) -> usize {
        let from = match from {
            Node::State(index) => self.file.states[index].name.clone(),
            Node::Any | Node::Entry => ANY_STATE.to_owned(),
        };
        let to = self.file.states[to].name.clone();
        let index = self
            .file
            .transitions
            .iter()
            .position(|t| t.generated)
            .unwrap_or(self.file.transitions.len());
        self.file
            .transitions
            .insert(index, TransitionDef::new(&from, &to));
        index
    }

    /// Adds a parameter of `kind` with a usual name (IsGrounded, Speed or
    /// Jump, numbered if taken). Returns its name.
    pub fn add_parameter(&mut self, kind: ParameterKind) -> String {
        let name = self.free_parameter_name(match kind {
            ParameterKind::Bool => "IsGrounded",
            ParameterKind::Number => "Speed",
            ParameterKind::Trigger => "Jump",
        });
        self.file.parameters.push(parameter_def(kind, name.clone()));
        name
    }

    /// Adds a parameter of `kind` called `name`, as typed.
    pub fn new_parameter(&mut self, kind: ParameterKind, name: &str) -> Result<String, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("A parameter needs a name.".to_owned());
        }
        if self.file.parameters.iter().any(|def| def.name() == name) {
            return Err(format!("There's already a parameter called `{name}`."));
        }
        self.file
            .parameters
            .push(parameter_def(kind, name.to_owned()));
        Ok(name.to_owned())
    }

    /// Adds a condition on `parameter` to transition `index`, with the test
    /// that fits its type: a Bool is true, a Trigger is set, a Number is
    /// more than 0.1.
    pub fn add_condition(&mut self, index: usize, parameter: &str) {
        let kind = self
            .file
            .parameters
            .iter()
            .find(|def| def.name() == parameter)
            .map(ParameterKind::of);
        let Some(transition) = self.file.transitions.get_mut(index) else {
            return;
        };
        if transition.generated {
            return;
        }
        let (test, value) = match kind {
            Some(ParameterKind::Number) => (Test::Greater, 0.1),
            Some(ParameterKind::Trigger) => (Test::Triggered, 0.0),
            _ => (Test::IsTrue, 0.0),
        };
        transition.conditions.push(Condition {
            parameter: parameter.to_owned(),
            test,
            value,
        });
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
}

mod inspector;
mod layers;
mod live;
mod space_editor;
mod wire_editor;

use inspector::*;

fn parameter_def(kind: ParameterKind, name: String) -> ParameterDef {
    match kind {
        ParameterKind::Bool => ParameterDef::Bool {
            name,
            default: false,
        },
        ParameterKind::Number => ParameterDef::Number { name, default: 0.0 },
        ParameterKind::Trigger => ParameterDef::Trigger { name },
    }
}

#[cfg(test)]
mod tests;
