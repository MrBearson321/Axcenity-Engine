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
}

mod inspector;
mod live;

use inspector::*;

#[cfg(test)]
mod tests;
