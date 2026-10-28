//! A node graph: nodes with typed pins, wires between them, and comments.
//!
//! It's generic. What nodes there are comes from a library of
//! [`NodeSpec`]s, so the UI Event Graph ([`crate::ui_graph`]) and behavior
//! trees ([`crate::behavior_tree`]) share the data, the edits here and the editor.
//! A node keeps its kind's name, where it sits, and the values typed into
//! its fields and unwired inputs, all as text. Wires go from an output pin
//! to an input pin.
//!
//! Exec wires (white in the editor) say what runs next: an exec output
//! takes one wire, an exec input any number. A value input takes one wire;
//! a value output feeds any number of inputs, and is worked out again
//! wherever it's used.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde::{Deserialize, Serialize};

/// The newest graph file version this engine reads and writes.
pub const GRAPH_FORMAT_VERSION: u32 = 1;

/// What a pin carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PinType {
    /// What runs next.
    Exec,
    Number,
    Text,
    Bool,
    /// Two numbers (a material graph's texture coordinates).
    Vector2,
    /// Three numbers (a material graph's colors, positions and normals).
    Vector3,
    /// Any value.
    Any,
}

impl PinType {
    /// Whether a wire from an output of type `from` fits an input of this
    /// type. Any fits anything; numbers and true-or-false values also fit
    /// text, which they're turned into.
    pub fn accepts(self, from: PinType) -> bool {
        match (self, from) {
            (PinType::Exec, from) | (from, PinType::Exec) => from == PinType::Exec && self == from,
            (PinType::Any, _) | (_, PinType::Any) => true,
            (PinType::Text, PinType::Number | PinType::Bool) => true,
            // A number fills every part of a vector.
            (PinType::Vector2 | PinType::Vector3, PinType::Number) => true,
            (to, from) => to == from,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            PinType::Exec => "exec",
            PinType::Number => "a number",
            PinType::Text => "text",
            PinType::Bool => "true or false",
            PinType::Vector2 => "a Vector2",
            PinType::Vector3 => "a Vector3",
            PinType::Any => "any value",
        }
    }
}

/// One pin of a node kind. An empty name is an exec pin shown without a
/// label (a node's plain in and Then).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PinSpec {
    pub name: &'static str,
    pub ty: PinType,
    /// An unwired input that needs something typed into it: empty is an
    /// error, shown on the node.
    pub required: bool,
    /// What a new node's box for this input holds.
    pub default: &'static str,
    /// False: the input has no box to type in, and is only wired (a
    /// material graph's output, whose unwired inputs keep the material's
    /// own values).
    pub typed: bool,
}

impl PinSpec {
    pub const fn exec(name: &'static str) -> Self {
        Self {
            name,
            ty: PinType::Exec,
            required: false,
            default: "",
            typed: true,
        }
    }

    pub const fn value(name: &'static str, ty: PinType) -> Self {
        Self {
            name,
            ty,
            required: false,
            default: "",
            typed: true,
        }
    }

    pub const fn required(name: &'static str, ty: PinType) -> Self {
        Self {
            name,
            ty,
            required: true,
            default: "",
            typed: true,
        }
    }

    pub const fn with_default(self, default: &'static str) -> Self {
        Self { default, ..self }
    }

    /// This input with no box to type in: it's only wired.
    pub const fn wired_only(self) -> Self {
        Self {
            typed: false,
            ..self
        }
    }
}

/// Where an object field looks for its objects.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ObjectRoot {
    /// The ScreenGui the graph belongs to (an empty path is the ScreenGui).
    Gui,
    Workspace,
    ReplicatedStorage,
}

/// What a field holds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FieldKind {
    /// Typed text.
    Text,
    /// One of these.
    Choice(&'static [&'static str]),
    /// An object under `root`, by its path of names joined with `/`, of
    /// one of `classes` (any, when empty).
    Object {
        root: ObjectRoot,
        classes: &'static [&'static str],
    },
    /// One of the graph's variables, by name.
    Variable,
    /// One of the project's images, by its path.
    Image,
    /// A color, as `r, g, b` (sRGB, 0 to 1).
    Color,
}

/// A setting on a node that isn't a pin, such as which button or which
/// property.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FieldSpec {
    pub name: &'static str,
    pub kind: FieldKind,
    pub default: &'static str,
}

/// A kind of node, as a library describes it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NodeSpec {
    /// Its name in the file, such as `button_activated`.
    pub kind: &'static str,
    /// Its title on the node, such as `Button Activated`.
    pub title: &'static str,
    /// Its group in the node menu, such as `Events`.
    pub category: &'static str,
    pub help: &'static str,
    pub fields: &'static [FieldSpec],
    pub inputs: &'static [PinSpec],
    pub outputs: &'static [PinSpec],
}

impl NodeSpec {
    pub fn input(&self, name: &str) -> Option<&PinSpec> {
        self.inputs.iter().find(|pin| pin.name == name)
    }

    pub fn output(&self, name: &str) -> Option<&PinSpec> {
        self.outputs.iter().find(|pin| pin.name == name)
    }

    pub fn field(&self, name: &str) -> Option<&FieldSpec> {
        self.fields.iter().find(|field| field.name == name)
    }

    /// Whether it starts a chain (an event): it has no exec input but
    /// has an exec output.
    pub fn is_event(&self) -> bool {
        !self.inputs.iter().any(|pin| pin.ty == PinType::Exec)
            && self.outputs.iter().any(|pin| pin.ty == PinType::Exec)
    }
}

/// The node kinds a graph can hold.
pub trait NodeLibrary {
    fn specs(&self) -> &[NodeSpec];

    fn spec(&self, kind: &str) -> Option<&NodeSpec> {
        self.specs().iter().find(|spec| spec.kind == kind)
    }
}

/// A node in a graph.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraphNode {
    pub id: u32,
    pub kind: String,
    /// Its top left, in graph units.
    pub position: [f32; 2],
    /// Its fields' values and its unwired inputs' typed values, by name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub values: BTreeMap<String, String>,
}

impl GraphNode {
    pub fn value(&self, name: &str) -> &str {
        self.values.get(name).map_or("", String::as_str)
    }
}

/// One end of a wire: a node and one of its pins, by name.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PinRef {
    pub node: u32,
    pub pin: String,
}

impl PinRef {
    pub fn new(node: u32, pin: &str) -> Self {
        Self {
            node,
            pin: pin.to_owned(),
        }
    }
}

/// A wire from an output pin to an input pin.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Wire {
    pub from: PinRef,
    pub to: PinRef,
}

/// A note on the graph: a box with a title, behind the nodes it covers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Comment {
    pub id: u32,
    pub text: String,
    pub position: [f32; 2],
    pub size: [f32; 2],
}

/// A variable the whole graph shares, with the value it starts with, as
/// typed: a number, `true` or `false`, or text.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Variable {
    pub name: String,
    #[serde(default)]
    pub value: String,
}

fn graph_format_version() -> u32 {
    GRAPH_FORMAT_VERSION
}

/// A graph: its file's whole contents.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeGraph {
    #[serde(default = "graph_format_version")]
    pub format_version: u32,
    #[serde(default)]
    pub nodes: Vec<GraphNode>,
    #[serde(default)]
    pub wires: Vec<Wire>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub comments: Vec<Comment>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variables: Vec<Variable>,
}

impl Default for NodeGraph {
    fn default() -> Self {
        Self {
            format_version: GRAPH_FORMAT_VERSION,
            nodes: Vec::new(),
            wires: Vec::new(),
            comments: Vec::new(),
            variables: Vec::new(),
        }
    }
}

/// Why a wire wasn't made, to say at the bottom of the graph.
pub type Refusal = &'static str;

impl NodeGraph {
    pub fn node(&self, id: u32) -> Option<&GraphNode> {
        self.nodes.iter().find(|node| node.id == id)
    }

    pub fn node_mut(&mut self, id: u32) -> Option<&mut GraphNode> {
        self.nodes.iter_mut().find(|node| node.id == id)
    }

    /// An id no node or comment has.
    pub fn next_id(&self) -> u32 {
        let nodes = self.nodes.iter().map(|node| node.id);
        let comments = self.comments.iter().map(|comment| comment.id);
        nodes.chain(comments).max().map_or(1, |id| id + 1)
    }

    /// Adds a node of `spec` at `position`, with its fields' and inputs'
    /// defaults. Returns its id.
    pub fn add_node(&mut self, spec: &NodeSpec, position: [f32; 2]) -> u32 {
        let id = self.next_id();
        let fields = spec.fields.iter().map(|field| (field.name, field.default));
        let inputs = spec.inputs.iter().map(|pin| (pin.name, pin.default));
        let values = fields
            .chain(inputs)
            .filter(|(_, default)| !default.is_empty())
            .map(|(name, default)| (name.to_owned(), default.to_owned()))
            .collect();
        self.nodes.push(GraphNode {
            id,
            kind: spec.kind.to_owned(),
            position,
            values,
        });
        id
    }

    /// Adds a comment box around `area` (top left and size). Returns its id.
    pub fn add_comment(&mut self, position: [f32; 2], size: [f32; 2]) -> u32 {
        let id = self.next_id();
        self.comments.push(Comment {
            id,
            text: "Comment".to_owned(),
            position,
            size,
        });
        id
    }

    /// The pin's type, from its node's kind in `library`.
    pub fn pin_type(
        &self,
        library: &dyn NodeLibrary,
        pin: &PinRef,
        output: bool,
    ) -> Option<PinType> {
        let spec = library.spec(&self.node(pin.node)?.kind)?;
        let found = if output {
            spec.output(&pin.pin)
        } else {
            spec.input(&pin.pin)
        };
        found.map(|pin| pin.ty)
    }

    /// The wires into `pin` (an input).
    pub fn wires_into<'a>(&'a self, pin: &'a PinRef) -> impl Iterator<Item = &'a Wire> + 'a {
        self.wires.iter().filter(move |wire| wire.to == *pin)
    }

    /// The wires out of `pin` (an output).
    pub fn wires_from<'a>(&'a self, pin: &'a PinRef) -> impl Iterator<Item = &'a Wire> + 'a {
        self.wires.iter().filter(move |wire| wire.from == *pin)
    }

    /// Wires output `from` to input `to`. An exec output and a value input
    /// take one wire, so the wire there before goes. Refused when the
    /// types don't fit, or when it would make a loop.
    pub fn connect(
        &mut self,
        library: &dyn NodeLibrary,
        from: PinRef,
        to: PinRef,
    ) -> Result<(), Refusal> {
        let from_type = self
            .pin_type(library, &from, true)
            .ok_or("Wire an output to an input.")?;
        let to_type = self
            .pin_type(library, &to, false)
            .ok_or("Wire an output to an input.")?;
        if from.node == to.node {
            return Err("A node can't be wired to itself.");
        }
        if from_type == PinType::Exec || to_type == PinType::Exec {
            if from_type != to_type {
                return Err("White exec pins only wire to exec pins.");
            }
        } else if !to_type.accepts(from_type) {
            return Err(match to_type {
                PinType::Number => "That input needs a number.",
                PinType::Bool => "That input needs true or false (a red pin).",
                PinType::Vector2 => "That input needs a Vector2 or a number.",
                PinType::Vector3 => "That input needs a Vector3 (such as a color) or a number.",
                _ => "That value doesn't fit this input.",
            });
        }
        // `to`'s node reaching `from`'s node already, by the same kind of
        // wire, would close a loop.
        let exec = from_type == PinType::Exec;
        let loops = if exec {
            self.reaches(library, to.node, from.node, true)
        } else {
            self.reaches(library, from.node, to.node, false)
        };
        if loops {
            return Err(if exec {
                "That wire would loop back into its own chain."
            } else {
                "That wire would make a value depend on itself."
            });
        }
        if exec {
            self.wires.retain(|wire| wire.from != from);
        } else {
            self.wires.retain(|wire| wire.to != to);
        }
        self.wires.push(Wire { from, to });
        Ok(())
    }

    /// Whether `start` leads to `goal`: along exec wires forwards, or
    /// along value wires backwards (to the nodes a value comes from).
    fn reaches(
        &self,
        library: &dyn NodeLibrary,
        start: u32,
        goal: u32,
        exec_forwards: bool,
    ) -> bool {
        let mut seen = BTreeSet::new();
        let mut stack = vec![start];
        while let Some(node) = stack.pop() {
            if node == goal {
                return true;
            }
            if !seen.insert(node) {
                continue;
            }
            for wire in &self.wires {
                let exec = self.is_exec_wire(library, wire);
                if exec_forwards && exec && wire.from.node == node {
                    stack.push(wire.to.node);
                }
                if !exec_forwards && !exec && wire.to.node == node {
                    stack.push(wire.from.node);
                }
            }
        }
        false
    }

    /// Whether `wire` leaves an exec output.
    pub fn is_exec_wire(&self, library: &dyn NodeLibrary, wire: &Wire) -> bool {
        self.pin_type(library, &wire.from, true) == Some(PinType::Exec)
    }

    /// Breaks every wire on `pin` (an output when `output`).
    pub fn disconnect(&mut self, pin: &PinRef, output: bool) {
        self.wires.retain(|wire| {
            if output {
                wire.from != *pin
            } else {
                wire.to != *pin
            }
        });
    }

    /// Removes these nodes and their wires.
    pub fn remove_nodes(&mut self, ids: &BTreeSet<u32>) {
        self.nodes.retain(|node| !ids.contains(&node.id));
        self.wires
            .retain(|wire| !ids.contains(&wire.from.node) && !ids.contains(&wire.to.node));
    }

    /// These nodes, with the wires between them, as their own graph (for
    /// copying). The graph's variables come too, so pasting into another
    /// graph keeps them.
    pub fn copy(&self, ids: &BTreeSet<u32>, comments: &BTreeSet<u32>) -> NodeGraph {
        NodeGraph {
            nodes: self
                .nodes
                .iter()
                .filter(|node| ids.contains(&node.id))
                .cloned()
                .collect(),
            wires: self
                .wires
                .iter()
                .filter(|wire| ids.contains(&wire.from.node) && ids.contains(&wire.to.node))
                .cloned()
                .collect(),
            comments: self
                .comments
                .iter()
                .filter(|comment| comments.contains(&comment.id))
                .cloned()
                .collect(),
            variables: self.variables.clone(),
            ..NodeGraph::default()
        }
    }

    /// Adds a copy of `copied` (from [`Self::copy`]) with fresh ids, moved
    /// so its top-left node is at `at`. Variables it uses that this graph
    /// doesn't have are added. Returns the new nodes' and comments' ids.
    pub fn paste(&mut self, copied: &NodeGraph, at: [f32; 2]) -> (BTreeSet<u32>, BTreeSet<u32>) {
        let corner = copied
            .nodes
            .iter()
            .map(|node| node.position)
            .chain(copied.comments.iter().map(|comment| comment.position))
            .reduce(|a, b| [a[0].min(b[0]), a[1].min(b[1])])
            .unwrap_or(at);
        let shift = |p: [f32; 2]| [p[0] - corner[0] + at[0], p[1] - corner[1] + at[1]];
        let mut next = self.next_id();
        let mut renamed = HashMap::new();
        let mut nodes = BTreeSet::new();
        for node in &copied.nodes {
            renamed.insert(node.id, next);
            nodes.insert(next);
            self.nodes.push(GraphNode {
                id: next,
                position: shift(node.position),
                ..node.clone()
            });
            next += 1;
        }
        for wire in &copied.wires {
            if let (Some(&from), Some(&to)) =
                (renamed.get(&wire.from.node), renamed.get(&wire.to.node))
            {
                self.wires.push(Wire {
                    from: PinRef::new(from, &wire.from.pin),
                    to: PinRef::new(to, &wire.to.pin),
                });
            }
        }
        let mut comments = BTreeSet::new();
        for comment in &copied.comments {
            comments.insert(next);
            self.comments.push(Comment {
                id: next,
                position: shift(comment.position),
                ..comment.clone()
            });
            next += 1;
        }
        for variable in &copied.variables {
            if !self
                .variables
                .iter()
                .any(|known| known.name == variable.name)
            {
                self.variables.push(variable.clone());
            }
        }
        (nodes, comments)
    }
}
