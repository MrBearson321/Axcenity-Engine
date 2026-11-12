//! The state machine graph, drawn like the script Graph view
//! (`block_editor/graph.rs`): each state is a node with an input pin on the
//! left and an output pin on the right, and each transition is a wire from
//! one's output to the other's input, with a label that says when it
//! happens. The file is still states and transitions; only the drawing is
//! a node graph.
//!
//! - **Entry** is wired to the default state: wiring it to another state
//!   makes that one the default. **Any State** has only an output pin.
//! - Drag from an output pin to a node to make a transition; let go on
//!   empty space for a menu that makes a new state there, wired to it.
//! - Click a wire's label to edit it right there (the controller editor
//!   draws that editor: `controller_editor/wire_editor.rs`).
//! - Drop a clip (from Open ▾ or the Assets panel) to make a state playing
//!   it, and a parameter chip ([`parameters_panel`]) on a wire to test it.
//! - It draws on the shared graph canvas (`node_graph::canvas`, Phase 91),
//!   with the same keys as every graph: F frames the selected state (or
//!   all), Space opens the new-state search at the mouse, Delete deletes,
//!   Ctrl+D duplicates a state, and Ctrl+C, X and V copy, cut and paste
//!   one. Drag a node to move it; drag the background with any button, or
//!   the wheel, to pan; Ctrl + wheel zooms around the mouse. Right-click a
//!   state for Make Transition, Set as Default State, Rename and Delete,
//!   and the background to add a state. A state machine selects one thing
//!   at a time, so a left-drag pans rather than drawing a box.
//!
//! It only reports what was done ([`GraphEvent`]); the controller editor
//! changes the file.
//!
//! Where to change behavior:
//! - `draw.rs` — how nodes, pins, wires and labels look
//! - `words.rs` — what labels and headers say
//! - this file — geometry and the pointer

use axcenity_anim::files::ParameterDef;
use axcenity_anim::{ANY_STATE, StateMachineFile};
use eframe::egui::{self, Color32, Pos2, Rect, Stroke, Vec2};

use crate::asset_browser::{AssetKind, AssetPayload};
use crate::node_graph::canvas::{self as graph_canvas, Canvas, MenuChoice, MenuEntry, SearchMenu};
use crate::theme::icons;

mod draw;
pub mod words;

use draw::*;

/// Where the Any State node is drawn until it's moved.
pub const ANY_STATE_POSITION: [f32; 2] = [-220.0, 120.0];
/// Where the Entry node is drawn until it's moved.
pub const ENTRY_POSITION: [f32; 2] = [-220.0, 0.0];
/// A state's node, at zoom 1.
const STATE: Vec2 = Vec2::new(170.0, 52.0);
/// Entry's and Any State's nodes, which are only a title.
const SMALL: Vec2 = Vec2::new(120.0, 30.0);
/// From a node's edge to the middle of its pin.
const PIN_INSET: f32 = 10.0;
/// How near a wire a click selects it, and a chip lands on it.
const WIRE_REACH: f32 = 10.0;
const DROP_REACH: f32 = 28.0;
const MIN_ZOOM: f32 = 0.35;
const MAX_ZOOM: f32 = 2.5;

/// A node in the graph.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Node {
    State(usize),
    Any,
    Entry,
}

/// What's selected in the graph.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Selected {
    /// The controller itself (its parameters).
    #[default]
    Controller,
    Node(Node),
    Transition(usize),
}

/// The kinds of parameter the panel's + adds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParameterKind {
    Bool,
    Number,
    Trigger,
}

impl ParameterKind {
    pub const ALL: [ParameterKind; 3] = [Self::Bool, Self::Number, Self::Trigger];

    pub fn of(def: &ParameterDef) -> Self {
        match def {
            ParameterDef::Bool { .. } => Self::Bool,
            ParameterDef::Number { .. } => Self::Number,
            ParameterDef::Trigger { .. } => Self::Trigger,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Bool => "Bool",
            Self::Number => "Number",
            Self::Trigger => "Trigger",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Self::Bool => icons::TOGGLE_RIGHT,
            Self::Number => icons::HASH,
            Self::Trigger => icons::LIGHTNING,
        }
    }

    /// As in the script Graph view: Bools red, Numbers green.
    pub fn color(self) -> Color32 {
        match self {
            Self::Bool => Color32::from_rgb(196, 64, 64),
            Self::Number => Color32::from_rgb(92, 178, 92),
            Self::Trigger => Color32::from_rgb(226, 178, 58),
        }
    }
}

/// A parameter chip being dragged, by its parameter's name.
#[derive(Clone, Debug)]
pub struct ParameterChip(pub String);

/// The new-state menu: opened by a wire let go on empty space (wired
/// from where it came from) or by Space.
#[derive(Clone, Debug)]
struct DropMenu {
    from: Option<Node>,
    menu: SearchMenu,
}

/// What Ctrl+C puts on the clipboard: one state, as its file has it.
#[derive(serde::Serialize, serde::Deserialize)]
struct StateClip {
    axcenity_state: axcenity_anim::files::StateDef,
}

/// How the graph is panned and zoomed, and what's in progress, kept
/// between frames.
#[derive(Clone, Debug)]
pub struct GraphView {
    /// Pan and zoom; `view.pan` and `view.zoom` reach it through `Deref`.
    canvas: Canvas,
    /// A transition being made from the right-click menu: from here to the
    /// mouse, until a state is clicked.
    pub linking: Option<Node>,
    /// A wire being dragged from this node's output pin.
    wiring: Option<Node>,
    drop_menu: Option<DropMenu>,
    /// Where the background was right-clicked, for Add State.
    menu_at: Option<[f32; 2]>,
    /// The transition whose label editor is open.
    pub editing: Option<usize>,
    /// Where each transition's label was drawn last, on screen.
    pub labels: Vec<(usize, Rect)>,
    /// Where the graph was drawn last, on screen.
    pub rect: Rect,
    /// Where each parameter chip was drawn last, on screen.
    pub chips: Vec<(String, Rect)>,
}

impl Default for GraphView {
    fn default() -> Self {
        Self {
            canvas: Canvas::new(Vec2::new(300.0, 140.0)),
            linking: None,
            wiring: None,
            drop_menu: None,
            menu_at: None,
            editing: None,
            labels: Vec::new(),
            rect: Rect::NOTHING,
            chips: Vec::new(),
        }
    }
}

impl std::ops::Deref for GraphView {
    type Target = Canvas;

    fn deref(&self) -> &Canvas {
        &self.canvas
    }
}

impl std::ops::DerefMut for GraphView {
    fn deref_mut(&mut self) -> &mut Canvas {
        &mut self.canvas
    }
}

/// What changed in the graph this frame.
#[derive(Clone, Debug, PartialEq)]
pub enum GraphEvent {
    Select(Selected),
    /// A node was dragged to this graph position.
    Move(Node, [f32; 2]),
    /// A new transition from a node to a state.
    Link(Node, usize),
    SetDefault(usize),
    Rename(usize),
    Delete(Selected),
    /// Add a state here, playing this clip (or nothing), wired from this
    /// node if one is given.
    AddState([f32; 2], Option<String>, Option<Node>),
    /// Add a state here playing a blend tree.
    AddBlendTree([f32; 2]),
    /// A state was double-clicked, to open its clip.
    Open(usize),
    /// Ctrl+D on this state: a copy of it beside it.
    Duplicate(usize),
    /// Ctrl+V: this state (from the clipboard) added as a new one.
    Paste(axcenity_anim::files::StateDef),
    /// A parameter chip was dropped on this transition's wire.
    AddCondition(usize, String),
    AddParameter(ParameterKind),
}

/// The state playing in the preview, to show in the graph.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Live {
    /// The state playing and how far through its pass, from 0 to 1.
    pub state: Option<(usize, f32)>,
    /// A blend in progress: from this state, by this transition, this far.
    pub fade: Option<(usize, Option<usize>, f32)>,
}

fn node_position(file: &StateMachineFile, node: Node) -> [f32; 2] {
    match node {
        Node::State(index) => file.states.get(index).map_or([0.0; 2], |s| s.position),
        Node::Any => file.any_state_position.unwrap_or(ANY_STATE_POSITION),
        Node::Entry => file.entry_position.unwrap_or(ENTRY_POSITION),
    }
}

/// The node a transition's `from` names.
pub fn from_node(file: &StateMachineFile, from: &str) -> Option<Node> {
    if from == ANY_STATE {
        return Some(Node::Any);
    }
    file.states
        .iter()
        .position(|s| s.name == from)
        .map(Node::State)
}

/// The state Entry is wired to: the default state, or the first.
fn entry_target(file: &StateMachineFile) -> Option<usize> {
    file.states
        .iter()
        .position(|s| s.name == file.default_state)
        .or((!file.states.is_empty()).then_some(0))
}

/// How far transition `index`'s wire bows up or down, in graph units, so
/// a pair going both ways, or two wires between the same states, don't
/// sit on top of each other.
fn bow(file: &StateMachineFile, index: usize, from: [f32; 2], to: [f32; 2]) -> f32 {
    let transition = &file.transitions[index];
    let lane = file.transitions[..index]
        .iter()
        .filter(|other| other.from == transition.from && other.to == transition.to)
        .count() as f32;
    let reverse = file
        .transitions
        .iter()
        .any(|other| other.from == transition.to && other.to == transition.from);
    let side = if reverse {
        if transition.from < transition.to {
            -1.0
        } else {
            1.0
        }
    } else if to[0] < from[0] || lane > 0.0 {
        1.0
    } else {
        0.0
    };
    side * (34.0 + 26.0 * lane)
}

impl GraphView {
    fn to_screen(&self, rect: Rect, position: [f32; 2]) -> Pos2 {
        self.canvas
            .to_screen(rect, Pos2::new(position[0], position[1]))
    }

    fn to_graph(&self, rect: Rect, point: Pos2) -> [f32; 2] {
        let at = self.canvas.to_graph(rect, point);
        [at.x, at.y]
    }

    /// Where `node` is on screen, in a graph drawn in `rect`.
    pub fn node_rect(&self, rect: Rect, file: &StateMachineFile, node: Node) -> Rect {
        let size = match node {
            Node::State(_) => STATE,
            Node::Any | Node::Entry => SMALL,
        } * self.zoom;
        Rect::from_center_size(self.to_screen(rect, node_position(file, node)), size)
    }

    /// Where `node`'s output pin (or input pin) is on screen.
    pub fn pin(&self, rect: Rect, file: &StateMachineFile, node: Node, output: bool) -> Pos2 {
        let node_rect = self.node_rect(rect, file, node);
        let inset = PIN_INSET * self.zoom;
        let y = match node {
            Node::State(_) => state_pin_y(node_rect, self.zoom),
            Node::Any | Node::Entry => node_rect.center().y,
        };
        if output {
            Pos2::new(node_rect.right() - inset, y)
        } else {
            Pos2::new(node_rect.left() + inset, y)
        }
    }

    /// Transition `index`'s wire, as a cubic Bezier's four points.
    fn wire(&self, rect: Rect, file: &StateMachineFile, index: usize) -> Option<[Pos2; 4]> {
        let transition = &file.transitions[index];
        let from = from_node(file, &transition.from)?;
        let to = file.states.iter().position(|s| s.name == transition.to)?;
        let start = self.pin(rect, file, from, true);
        let end = self.pin(rect, file, Node::State(to), false);
        let bend = bow(
            file,
            index,
            node_position(file, from),
            node_position(file, Node::State(to)),
        ) * self.zoom;
        Some(wire_points(start, end, bend, self.zoom))
    }

    /// The state whose node is under `point`.
    fn state_at(&self, rect: Rect, file: &StateMachineFile, point: Pos2) -> Option<usize> {
        (0..file.states.len()).rev().find(|&index| {
            self.node_rect(rect, file, Node::State(index))
                .expand(6.0 * self.zoom)
                .contains(point)
        })
    }

    /// Draws the graph filling `ui`'s space, and returns what was done.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        file: &StateMachineFile,
        selected: Selected,
        live: Live,
        clips: &[String],
    ) -> Vec<GraphEvent> {
        let mut events = Vec::new();
        let size = ui.available_size();
        let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
        self.rect = rect;
        let colors = crate::theme::canvas(ui.ctx());
        let painter = ui.painter_at(rect);
        let pointer = ui.input(|input| input.pointer.latest_pos());

        // The shared canvas: background, grid, pan and zoom. A left-drag
        // pans too, since there's no box to draw.
        let frame = self.canvas.begin(
            ui,
            rect,
            ui.id().with("state machine canvas"),
            MIN_ZOOM..=MAX_ZOOM,
            self.drop_menu.is_none(),
        );
        let canvas = frame.back.clone();
        if canvas.dragged_by(egui::PointerButton::Primary) {
            self.canvas.pan += canvas.drag_delta();
        }
        let z = self.zoom;

        // Something dragged over the graph: a parameter chip (onto the
        // nearest wire) or a clip (to make a state).
        let over = ui.rect_contains_pointer(rect);
        let released = ui.input(|input| input.pointer.any_released());
        let chip = over
            .then(|| egui::DragAndDrop::payload::<ParameterChip>(ui.ctx()))
            .flatten();
        let clip = over
            .then(|| egui::DragAndDrop::payload::<AssetPayload>(ui.ctx()))
            .flatten()
            .filter(|payload| payload.0.kind == AssetKind::Animation);

        // Until there's a transition, how to make one, under everything.
        if file.transitions.is_empty() && clip.is_none() {
            draw_guide(&painter, rect, file);
        }

        // Wires, under the nodes.
        let mut curves = Vec::new();
        for index in 0..file.transitions.len() {
            if let Some(points) = self.wire(rect, file, index) {
                curves.push((index, points, sample_wire(points)));
            }
        }
        let chip_target = chip.as_ref().and(pointer).and_then(|pointer| {
            nearest_wire(&curves, pointer).filter(|&(_, distance)| distance < DROP_REACH * z)
        });
        for (index, points, samples) in &curves {
            let transition = &file.transitions[*index];
            let active = live.fade.is_some_and(|(_, t, _)| t == Some(*index));
            let chosen = selected == Selected::Transition(*index)
                || chip_target.is_some_and(|(target, _)| target == *index);
            let color = if chosen {
                colors.selected
            } else if active {
                colors.live
            } else {
                colors.wire
            };
            let width = if chosen { 3.0 } else { 2.0 } * z.max(0.7);
            draw_wire(
                &painter,
                *points,
                samples,
                color,
                width,
                transition.generated,
            );
        }
        if let Some(to) = entry_target(file) {
            let start = self.pin(rect, file, Node::Entry, true);
            let end = self.pin(rect, file, Node::State(to), false);
            let points = wire_points(start, end, 0.0, z);
            draw_wire(
                &painter,
                points,
                &sample_wire(points),
                colors.live,
                2.0 * z.max(0.7),
                false,
            );
        }

        // A wire being made follows the mouse.
        if let (Some(from), Some(pointer)) = (self.wiring.or(self.linking), pointer) {
            let start = self.pin(rect, file, from, true);
            let points = wire_points(start, pointer, 0.0, z);
            draw_wire(
                &painter,
                points,
                &sample_wire(points),
                colors.selected,
                2.0 * z.max(0.7),
                false,
            );
        }
        if self.linking.is_some() && ui.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.linking = None;
        }

        // The nodes: states, the Any State and Entry.
        let mut nodes: Vec<Node> = (0..file.states.len()).map(Node::State).collect();
        nodes.extend([Node::Any, Node::Entry]);
        let mut hit_node = false;
        for node in nodes {
            let node_rect = self.node_rect(rect, file, node);
            let id = ui.id().with(("graph node", node_id(node)));
            let response = ui.interact(node_rect, id, egui::Sense::click_and_drag());
            if response.hovered() {
                hit_node = true;
            }
            let playing = match (node, live.state) {
                (Node::State(index), Some((state, progress))) if index == state => Some(progress),
                _ => None,
            };
            let wired_out = match node {
                Node::Entry => entry_target(file).is_some(),
                Node::Any => file.transitions.iter().any(|t| t.from == ANY_STATE),
                Node::State(index) => file
                    .transitions
                    .iter()
                    .any(|t| t.from == file.states[index].name),
            };
            let wired_in = match node {
                Node::State(index) => {
                    let name = &file.states[index].name;
                    entry_target(file) == Some(index)
                        || file.transitions.iter().any(|t| &t.to == name)
                }
                _ => false,
            };
            draw_node(
                &painter,
                NodeLook {
                    node,
                    file,
                    rect: node_rect,
                    zoom: z,
                    selected: selected == Selected::Node(node),
                    hovered: response.hovered(),
                    playing,
                    wired_in,
                    wired_out,
                },
            );

            // Clicks, drags and the right-click menu.
            if response.clicked() {
                match (self.linking.take(), node) {
                    (Some(Node::Entry), Node::State(to)) => events.push(GraphEvent::SetDefault(to)),
                    (Some(from), Node::State(to)) if from != node => {
                        events.push(GraphEvent::Link(from, to));
                    }
                    (Some(_), _) => {}
                    (None, _) => events.push(GraphEvent::Select(Selected::Node(node))),
                }
            }
            if response.double_clicked()
                && let Node::State(index) = node
            {
                events.push(GraphEvent::Open(index));
            }
            if response.drag_started_by(egui::PointerButton::Primary) {
                events.push(GraphEvent::Select(Selected::Node(node)));
            }
            if response.dragged_by(egui::PointerButton::Primary) {
                let [x, y] = node_position(file, node);
                let delta = response.drag_delta() / z;
                events.push(GraphEvent::Move(node, [x + delta.x, y + delta.y]));
            }
            if node != Node::Entry {
                response.context_menu(|ui| {
                    if ui.button("Make Transition").clicked() {
                        self.linking = Some(node);
                        ui.close();
                    }
                    if let Node::State(index) = node {
                        if ui.button("Set as Default State").clicked() {
                            events.push(GraphEvent::SetDefault(index));
                            ui.close();
                        }
                        if ui.button("Rename").clicked() {
                            events.push(GraphEvent::Rename(index));
                            ui.close();
                        }
                        if ui.button("Delete").clicked() {
                            events.push(GraphEvent::Delete(Selected::Node(node)));
                            ui.close();
                        }
                    }
                });
            }

            // The output pin: drag from it to make a wire.
            let pin_at = self.pin(rect, file, node, true);
            let pin = ui
                .interact(
                    Rect::from_center_size(pin_at, Vec2::splat(18.0 * z.max(0.7))),
                    id.with("output pin"),
                    egui::Sense::drag(),
                )
                .on_hover_cursor(egui::CursorIcon::Crosshair)
                .on_hover_text(match node {
                    Node::Entry => "Drag to the state the Animator starts in",
                    _ => "Drag to another state to make a transition",
                });
            if pin.drag_started() {
                self.wiring = Some(node);
                self.linking = None;
            }
            if pin.drag_stopped()
                && let Some(from) = self.wiring.take()
                && let Some(pointer) = pointer
            {
                match self.state_at(rect, file, pointer) {
                    Some(to) if Node::State(to) == from => {}
                    Some(to) if from == Node::Entry => events.push(GraphEvent::SetDefault(to)),
                    Some(to) => events.push(GraphEvent::Link(from, to)),
                    None if rect.contains(pointer) => {
                        self.drop_menu = Some(DropMenu {
                            from: Some(from),
                            menu: SearchMenu::new(pointer, self.canvas.to_graph(rect, pointer)),
                        });
                    }
                    None => {}
                }
            }
        }

        // The labels, over everything: click one to edit its transition.
        self.labels.clear();
        let mut hit_label = false;
        for (index, points, _) in &curves {
            let transition = &file.transitions[*index];
            let label_rect = draw_label(
                &painter,
                bezier_at(*points, 0.5),
                transition,
                z,
                selected == Selected::Transition(*index) || self.editing == Some(*index),
            );
            self.labels.push((*index, label_rect));
            let response = ui
                .interact(
                    label_rect,
                    ui.id().with(("wire label", *index)),
                    egui::Sense::click(),
                )
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text("Click to say when it happens");
            if response.hovered() {
                hit_label = true;
            }
            if response.clicked() {
                events.push(GraphEvent::Select(Selected::Transition(*index)));
                self.editing = if self.editing == Some(*index) {
                    None
                } else {
                    Some(*index)
                };
            }
        }
        if self
            .editing
            .is_some_and(|index| index >= file.transitions.len())
        {
            self.editing = None;
        }

        // A click on the background: a wire under it, or nothing.
        // (Clicking the background while making a transition cancels it.)
        if canvas.clicked() && !hit_node && !hit_label && self.linking.take().is_none() {
            let near = canvas
                .interact_pointer_pos()
                .and_then(|pointer| nearest_wire(&curves, pointer))
                .filter(|&(_, distance)| distance < WIRE_REACH * z.max(0.7));
            events.push(GraphEvent::Select(match near {
                Some((index, _)) => Selected::Transition(index),
                None => Selected::Controller,
            }));
            self.editing = None;
        }
        if canvas.secondary_clicked() {
            self.menu_at = canvas
                .interact_pointer_pos()
                .map(|pointer| self.to_graph(rect, pointer));
        }
        canvas.context_menu(|ui| {
            let at = self.menu_at.unwrap_or([0.0, 0.0]);
            if ui.button("Add Empty State").clicked() {
                events.push(GraphEvent::AddState(at, None, None));
                ui.close();
            }
            if ui
                .button("Add Blend Tree")
                .on_hover_text("A state that mixes related clips by a Number, such as Idle, Walk and Run by Speed")
                .clicked()
            {
                events.push(GraphEvent::AddBlendTree(at));
                ui.close();
            }
            crate::widgets::menu_button(ui, None, "Add State with Clip", |ui| {
                if let Some(clip) = clip_list(ui, clips, "") {
                    events.push(GraphEvent::AddState(at, Some(clip), None));
                    ui.close();
                }
            });
        });

        // Drops.
        if let Some((index, _)) = chip_target
            && released
            && let Some(chip) = egui::DragAndDrop::take_payload::<ParameterChip>(ui.ctx())
        {
            events.push(GraphEvent::AddCondition(index, chip.0.clone()));
        }
        if let Some(payload) = &clip {
            painter.rect_stroke(
                rect.shrink(1.0),
                0.0,
                Stroke::new(2.0_f32, colors.selected),
                egui::StrokeKind::Inside,
            );
            hint(
                &painter,
                rect,
                &format!("Drop to add a state playing {}", payload.0.name),
            );
            if released
                && let Some(pointer) = pointer
                && egui::DragAndDrop::take_payload::<AssetPayload>(ui.ctx()).is_some()
            {
                events.push(GraphEvent::AddState(
                    self.to_graph(rect, pointer),
                    Some(payload.0.path.clone()),
                    None,
                ));
            }
        }

        // The keys every graph shares, while the mouse is over it.
        if frame.hovered && self.drop_menu.is_none() {
            self.keys(ui, file, selected, &frame, &mut events);
        }
        let nodes: Vec<Rect> = (0..file.states.len())
            .map(Node::State)
            .chain([Node::Any, Node::Entry])
            .map(|node| {
                let at = node_position(file, node);
                let size = match node {
                    Node::State(_) => STATE,
                    Node::Any | Node::Entry => SMALL,
                };
                Rect::from_center_size(Pos2::new(at[0], at[1]), size)
            })
            .collect();
        self.canvas.minimap(ui, &frame, &nodes);
        if clip.is_some() {
            // Its own hint is up.
        } else if self.linking.is_some() {
            hint(
                &painter,
                rect,
                "Click the state the transition goes to (Escape cancels)",
            );
        } else if chip.is_some() {
            hint(&painter, rect, "Drop it on a wire to test it there");
        }
        self.show_drop_menu(ui, clips, &mut events);
        events
    }

    /// Delete, Ctrl+D, Ctrl+C, X and V, Escape, F and Space, read by the
    /// shared canvas (`canvas::keys`).
    fn keys(
        &mut self,
        ui: &egui::Ui,
        file: &StateMachineFile,
        selected: Selected,
        frame: &graph_canvas::Frame,
        events: &mut Vec<GraphEvent>,
    ) {
        let keys = graph_canvas::keys(ui.ctx());
        let state = match selected {
            Selected::Node(Node::State(index)) => file.states.get(index).map(|s| (index, s)),
            _ => None,
        };
        if (keys.copy || keys.cut)
            && let Some((_, state)) = state
        {
            let mut copy = state.clone();
            copy.returns = None;
            if let Ok(text) = serde_json::to_string(&StateClip {
                axcenity_state: copy,
            }) {
                ui.ctx().copy_text(text);
            }
        }
        if keys.delete || (keys.cut && state.is_some()) {
            events.push(GraphEvent::Delete(selected));
        }
        if keys.duplicate
            && let Some((index, _)) = state
        {
            events.push(GraphEvent::Duplicate(index));
        }
        if let Some(text) = keys.paste
            && let Ok(clip) = serde_json::from_str::<StateClip>(&text)
        {
            let at = frame.pointer_in_graph();
            let mut state = clip.axcenity_state;
            state.position = [at.x.round(), at.y.round()];
            events.push(GraphEvent::Paste(state));
        }
        if keys.escape {
            events.push(GraphEvent::Select(Selected::Controller));
        }
        if keys.frame {
            let nodes: Vec<Node> = match selected {
                Selected::Node(node) => vec![node],
                _ => (0..file.states.len())
                    .map(Node::State)
                    .chain([Node::Any, Node::Entry])
                    .collect(),
            };
            let area = nodes
                .into_iter()
                .map(|node| {
                    let at = node_position(file, node);
                    Rect::from_center_size(Pos2::new(at[0], at[1]), STATE)
                })
                .reduce(|a, b| a.union(b));
            self.canvas.fit(frame.rect, area, MIN_ZOOM, 1.0);
        }
        if keys.search {
            let at = frame.pointer_in_graph();
            self.drop_menu = Some(DropMenu {
                from: None,
                menu: SearchMenu::new(frame.to_screen(at), at),
            });
        }
    }

    /// The new-state menu: a state there, playing one of the project's
    /// clips (searchable) or nothing, wired from where its wire came from.
    fn show_drop_menu(&mut self, ui: &egui::Ui, clips: &[String], events: &mut Vec<GraphEvent>) {
        let Some(menu) = &mut self.drop_menu else {
            return;
        };
        let titles: Vec<String> = clips
            .iter()
            .map(|clip| axcenity_anim::files::clip_name(clip).to_owned())
            .collect();
        let groups: Vec<String> = clips
            .iter()
            .map(|clip| super::store::group_of(clip).to_owned())
            .collect();
        let words: Vec<String> = titles.iter().map(|title| title.to_lowercase()).collect();
        let color = ui.visuals().weak_text_color();
        let empty = format!("{} Empty state", icons::PLUS);
        let entries: Vec<MenuEntry> = std::iter::once(MenuEntry {
            group: "",
            color,
            title: &empty,
            words: "empty state",
        })
        .chain(
            titles
                .iter()
                .zip(&groups)
                .zip(&words)
                .map(|((title, group), words)| MenuEntry {
                    group,
                    color,
                    title,
                    words,
                }),
        )
        .collect();
        let title = if menu.from.is_some() {
            "New state, wired from here"
        } else {
            "New state"
        };
        let choice = menu.menu.show(
            ui.ctx(),
            ui.id().with("new state menu"),
            title,
            &entries,
            None,
        );
        match choice {
            MenuChoice::Open => {}
            MenuChoice::Close => self.drop_menu = None,
            MenuChoice::Pick(index) => {
                let at = [menu.menu.graph.x, menu.menu.graph.y];
                let clip = index.checked_sub(1).map(|clip| clips[clip].clone());
                events.push(GraphEvent::AddState(at, clip, menu.from));
                self.drop_menu = None;
            }
        }
    }

    /// Whether the new-state menu is open (for tests).
    #[cfg(test)]
    pub fn drop_menu_open(&self) -> bool {
        self.drop_menu.is_some()
    }
}

/// The clips whose names have `needle` in them.
fn matching<'a>(clips: &'a [String], needle: &str) -> Vec<&'a String> {
    clips
        .iter()
        .filter(|clip| {
            needle.is_empty()
                || axcenity_anim::files::clip_name(clip)
                    .to_lowercase()
                    .contains(needle)
        })
        .collect()
}

/// The project's clips matching `needle` as buttons. Returns the one clicked.
fn clip_list(ui: &mut egui::Ui, clips: &[String], needle: &str) -> Option<String> {
    let found = matching(clips, needle);
    if clips.is_empty() {
        ui.weak("No clips in the project yet.");
    } else if found.is_empty() {
        ui.weak("Nothing matches.");
    }
    let mut chosen = None;
    egui::ScrollArea::vertical()
        .max_height(240.0)
        .show(ui, |ui| {
            for clip in found {
                let label = format!(
                    "{}  ({})",
                    axcenity_anim::files::clip_name(clip),
                    super::store::group_of(clip)
                );
                if ui.button(label).clicked() {
                    chosen = Some(clip.clone());
                }
            }
        });
    chosen
}

/// The parameters as chips with type icons, each of which can be dragged
/// onto a wire, and + to add one. Returns what was asked for, and puts
/// where each chip was drawn in `chips`.
pub fn parameters_panel(
    ui: &mut egui::Ui,
    parameters: &[ParameterDef],
    chips: &mut Vec<(String, Rect)>,
) -> Vec<GraphEvent> {
    let mut events = Vec::new();
    chips.clear();
    ui.horizontal(|ui| {
        ui.strong("Parameters");
        ui.menu_button(icons::PLUS, |ui| {
            for kind in ParameterKind::ALL {
                if ui
                    .button(format!("{} {}", kind.icon(), kind.label()))
                    .clicked()
                {
                    events.push(GraphEvent::AddParameter(kind));
                    ui.close();
                }
            }
        })
        .response
        .on_hover_text("Add a parameter: a value the game sets, which wires test");
    });
    if parameters.is_empty() {
        ui.weak("None yet. + adds one.");
        return events;
    }
    ui.horizontal_wrapped(|ui| {
        // Where rows end. A wrapping row reports its whole width as
        // available, so what's left of it is measured from here.
        let right = ui.max_rect().right();
        for def in parameters {
            let kind = ParameterKind::of(def);
            // A chip that doesn't fit what's left of the row starts the
            // next one, rather than its name breaking letter by letter.
            if chip_width(ui, kind, def.name()) > right - ui.cursor().left()
                && ui.cursor().left() > ui.max_rect().left()
            {
                ui.end_row();
            }
            let id = ui.id().with(("parameter chip", def.name()));
            let response = ui
                .dnd_drag_source(id, ParameterChip(def.name().to_owned()), |ui| {
                    draw_chip(ui, kind, def.name());
                })
                .response;
            chips.push((def.name().to_owned(), response.rect));
            response.on_hover_text(format!(
                "{}: drag it onto a wire to test it there",
                kind.label()
            ));
        }
    });
    events
}

/// How wide a parameter chip for `name` is drawn, margins included.
fn chip_width(ui: &egui::Ui, kind: ParameterKind, name: &str) -> f32 {
    let text = ui.painter().layout_no_wrap(
        format!("{} {name}", kind.icon()),
        egui::TextStyle::Body.resolve(ui.style()),
        egui::Color32::WHITE,
    );
    text.size().x + draw::CHIP_MARGIN_X * 2.0 + 2.0
}

fn node_id(node: Node) -> i64 {
    match node {
        Node::State(index) => index as i64,
        Node::Any => -1,
        Node::Entry => -2,
    }
}

#[cfg(test)]
mod chip_tests {
    use super::*;

    /// In the state machine's narrow parameter column, a chip that doesn't
    /// fit what's left of a row starts the next row, whole, instead of its
    /// name breaking letter by letter ("Wav/e", "H/o/p").
    #[test]
    fn parameter_chips_start_a_new_row_instead_of_breaking_their_names() {
        let parameters = vec![
            ParameterDef::Number {
                name: "Speed".into(),
                default: 0.0,
            },
            ParameterDef::Trigger {
                name: "Wave".into(),
            },
            ParameterDef::Trigger { name: "Hop".into() },
        ];
        let ctx = crate::theme::test_context();
        let mut chips = Vec::new();
        let column = Rect::from_min_size(Pos2::ZERO, egui::vec2(132.0, 400.0));
        for _ in 0..2 {
            let _ = crate::theme::run(&ctx, Default::default(), |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    ui.scope_builder(egui::UiBuilder::new().max_rect(column), |ui| {
                        parameters_panel(ui, &parameters, &mut chips);
                    });
                });
            });
        }
        assert_eq!(chips.len(), 3);
        let line = chips[0].1.height();
        for (name, rect) in &chips {
            assert!(
                (rect.height() - line).abs() < 0.5,
                "{name} is one line: {rect:?}"
            );
            assert!(
                rect.right() <= column.right() + 0.5,
                "{name} fits: {rect:?}"
            );
        }
        // Speed and Wave don't fit side by side in 132 points.
        assert!(chips[1].1.top() > chips[0].1.top(), "{chips:?}");
    }
}
