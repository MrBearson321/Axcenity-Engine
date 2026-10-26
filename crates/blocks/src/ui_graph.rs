//! The UI Event Graph: a ScreenGui's [`NodeGraph`] of UI nodes, and the
//! Lua it compiles to.
//!
//! Each ScreenGui has at most one graph, saved as
//! `ui/<ScreenGui>.uigraph.json`. It compiles to an ordinary LocalScript
//! that sits under the ScreenGui (`script.Parent` is the ScreenGui), so
//! the player runs it like any other script and there is one runtime.
//! Objects are named by their path under the ScreenGui (or the Workspace
//! and ReplicatedStorage, for sounds and RemoteEvents), checked against the
//! objects that are there. A graph that can't compile gives its errors by
//! node, never as Lua errors.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::node_graph::*;
use crate::translate::{quote, valid_identifier};
use crate::{EASING_DIRECTIONS, EASING_STYLES};

/// Where a project keeps its UI Event Graphs.
pub const UI_GRAPHS_FOLDER: &str = "ui";
/// A UI Event Graph's file ending. The part before it is its ScreenGui's name.
pub const UI_GRAPH_SUFFIX: &str = ".uigraph.json";
/// The name of the LocalScript a graph compiles to, under its ScreenGui.
pub const UI_GRAPH_SCRIPT_NAME: &str = "EventGraph";

/// The project path of the graph for the ScreenGui called `gui_name`.
pub fn ui_graph_path(gui_name: &str) -> String {
    format!("{UI_GRAPHS_FOLDER}/{gui_name}{UI_GRAPH_SUFFIX}")
}

/// Whether `path` is a UI Event Graph's file.
pub fn is_ui_graph(path: &str) -> bool {
    path.ends_with(UI_GRAPH_SUFFIX)
}

/// The ScreenGui a graph file at `path` belongs to, by name.
pub fn ui_graph_gui_name(path: &str) -> &str {
    let file = path.rsplit('/').next().unwrap_or(path);
    file.strip_suffix(UI_GRAPH_SUFFIX).unwrap_or(file)
}

const GUI_ELEMENTS: &[&str] = &[];
const BUTTONS: &[&str] = &["TextButton"];
const TEXT_BOXES: &[&str] = &["TextBox"];
const PAGE_LAYOUTS: &[&str] = &["PageLayout"];
const TEXT_ELEMENTS: &[&str] = &["TextLabel", "TextButton", "TextBox"];
const SOUNDS: &[&str] = &["Sound"];
const REMOTES: &[&str] = &["RemoteEvent"];

/// The math node's operators.
pub const MATH_OPERATORS: &[&str] = &["+", "-", "*", "/", "%", "^"];
/// The compare node's operators, as shown; each is a Lua operator.
pub const COMPARE_OPERATORS: &[&str] = &["<", "≤", "=", "≠", ">", "≥"];

const fn object(
    name: &'static str,
    root: ObjectRoot,
    classes: &'static [&'static str],
) -> FieldSpec {
    FieldSpec {
        name,
        kind: FieldKind::Object { root, classes },
        default: "",
    }
}

const fn text_field(name: &'static str, default: &'static str) -> FieldSpec {
    FieldSpec {
        name,
        kind: FieldKind::Text,
        default,
    }
}

const fn choice(
    name: &'static str,
    choices: &'static [&'static str],
    default: &'static str,
) -> FieldSpec {
    FieldSpec {
        name,
        kind: FieldKind::Choice(choices),
        default,
    }
}

const VARIABLE: FieldSpec = FieldSpec {
    name: "Variable",
    kind: FieldKind::Variable,
    default: "",
};

const IN: PinSpec = PinSpec::exec("");
const THEN: PinSpec = PinSpec::exec("");

/// Every node the UI Event Graph offers, in the node menu's order.
pub const UI_NODES: &[NodeSpec] = &[
    // Events
    NodeSpec {
        kind: "button_activated",
        title: "Button Activated",
        category: "Events",
        help: "Runs when the button is clicked (its Activated event).",
        fields: &[object("Button", ObjectRoot::Gui, BUTTONS)],
        inputs: &[],
        outputs: &[THEN],
    },
    NodeSpec {
        kind: "hovered",
        title: "Hovered",
        category: "Events",
        help: "Runs when the pointer moves onto the element (its MouseEnter event).",
        fields: &[object("Element", ObjectRoot::Gui, GUI_ELEMENTS)],
        inputs: &[],
        outputs: &[THEN],
    },
    NodeSpec {
        kind: "focus_lost",
        title: "TextBox FocusLost",
        category: "Events",
        help: "Runs when the player finishes typing in the TextBox.",
        fields: &[object("TextBox", ObjectRoot::Gui, TEXT_BOXES)],
        inputs: &[],
        outputs: &[THEN, PinSpec::value("Text", PinType::Text)],
    },
    NodeSpec {
        kind: "screen_shown",
        title: "Screen Shown",
        category: "Events",
        help: "Runs when the game starts with this ScreenGui enabled, and each time it's enabled again.",
        fields: &[],
        inputs: &[],
        outputs: &[THEN],
    },
    NodeSpec {
        kind: "every_frame",
        title: "Every Frame",
        category: "Events",
        help: "Runs once a frame (RunService.Heartbeat), with the frame's length in seconds.",
        fields: &[],
        inputs: &[],
        outputs: &[THEN, PinSpec::value("Delta Time", PinType::Number)],
    },
    // Actions
    NodeSpec {
        kind: "set_property",
        title: "Set Property",
        category: "Actions",
        help: "Sets a property of an element, such as its Text or BackgroundColor3.",
        fields: &[
            object("Object", ObjectRoot::Gui, GUI_ELEMENTS),
            text_field("Property", "Text"),
        ],
        inputs: &[IN, PinSpec::required("Value", PinType::Any)],
        outputs: &[THEN],
    },
    NodeSpec {
        kind: "show_page",
        title: "Show Page",
        category: "Actions",
        help: "Shows a page of a PageLayout, by the page Frame's name (its CurrentPage).",
        fields: &[object("Pages", ObjectRoot::Gui, PAGE_LAYOUTS)],
        inputs: &[IN, PinSpec::required("Page", PinType::Text)],
        outputs: &[THEN],
    },
    NodeSpec {
        kind: "set_visible",
        title: "Set Visible",
        category: "Actions",
        help: "Shows or hides an element (its Visible), or the whole ScreenGui (its Enabled).",
        fields: &[object("Element", ObjectRoot::Gui, GUI_ELEMENTS)],
        inputs: &[
            IN,
            PinSpec::value("Visible", PinType::Bool).with_default("true"),
        ],
        outputs: &[THEN],
    },
    NodeSpec {
        kind: "tween",
        title: "Tween",
        category: "Actions",
        help: "Moves a property to a goal smoothly over some seconds, with TweenService.",
        fields: &[
            object("Object", ObjectRoot::Gui, GUI_ELEMENTS),
            text_field("Property", "BackgroundTransparency"),
            choice("Style", &EASING_STYLES, "Quad"),
            choice("Direction", &EASING_DIRECTIONS, "Out"),
        ],
        inputs: &[
            IN,
            PinSpec::required("Goal", PinType::Any),
            PinSpec::value("Seconds", PinType::Number).with_default("0.5"),
        ],
        outputs: &[THEN],
    },
    NodeSpec {
        kind: "play_sound",
        title: "Play Sound",
        category: "Actions",
        help: "Plays a Sound in the Workspace.",
        fields: &[object("Sound", ObjectRoot::Workspace, SOUNDS)],
        inputs: &[IN],
        outputs: &[THEN],
    },
    NodeSpec {
        kind: "fire_remote",
        title: "Fire RemoteEvent",
        category: "Actions",
        help: "Tells the server, with a RemoteEvent in ReplicatedStorage (FireServer).",
        fields: &[object(
            "RemoteEvent",
            ObjectRoot::ReplicatedStorage,
            REMOTES,
        )],
        inputs: &[IN, PinSpec::value("Value", PinType::Any)],
        outputs: &[THEN],
    },
    NodeSpec {
        kind: "print",
        title: "Print",
        category: "Actions",
        help: "Prints to Output.",
        fields: &[],
        inputs: &[
            IN,
            PinSpec::value("Value", PinType::Text).with_default("Hello"),
        ],
        outputs: &[THEN],
    },
    // Flow
    NodeSpec {
        kind: "branch",
        title: "Branch",
        category: "Flow",
        help: "Runs True or False, by the condition.",
        fields: &[],
        inputs: &[IN, PinSpec::value("Condition", PinType::Bool)],
        outputs: &[PinSpec::exec("True"), PinSpec::exec("False")],
    },
    NodeSpec {
        kind: "sequence",
        title: "Sequence",
        category: "Flow",
        help: "Runs Then 0, then Then 1, then Then 2.",
        fields: &[],
        inputs: &[IN],
        outputs: &[
            PinSpec::exec("Then 0"),
            PinSpec::exec("Then 1"),
            PinSpec::exec("Then 2"),
        ],
    },
    NodeSpec {
        kind: "delay",
        title: "Delay",
        category: "Flow",
        help: "Waits some seconds, then runs Completed (task.wait).",
        fields: &[],
        inputs: &[
            IN,
            PinSpec::value("Seconds", PinType::Number).with_default("1"),
        ],
        outputs: &[PinSpec::exec("Completed")],
    },
    // Data
    NodeSpec {
        kind: "get_property",
        title: "Get Property",
        category: "Data",
        help: "A property of an element, such as its Text or Visible.",
        fields: &[
            object("Object", ObjectRoot::Gui, GUI_ELEMENTS),
            text_field("Property", "Text"),
        ],
        inputs: &[],
        outputs: &[PinSpec::value("Value", PinType::Any)],
    },
    NodeSpec {
        kind: "get_text",
        title: "Get Text",
        category: "Data",
        help: "The Text of a label, button or TextBox.",
        fields: &[object("Element", ObjectRoot::Gui, TEXT_ELEMENTS)],
        inputs: &[],
        outputs: &[PinSpec::value("Text", PinType::Text)],
    },
    NodeSpec {
        kind: "math",
        title: "Math",
        category: "Data",
        help: "Adds, takes away, multiplies, divides, takes the remainder or raises to a power.",
        fields: &[choice("Operator", MATH_OPERATORS, "+")],
        inputs: &[
            PinSpec::value("A", PinType::Number),
            PinSpec::value("B", PinType::Number),
        ],
        outputs: &[PinSpec::value("Result", PinType::Number)],
    },
    NodeSpec {
        kind: "compare",
        title: "Compare",
        category: "Data",
        help: "Whether A is less than, equal to or more than B.",
        fields: &[choice("Operator", COMPARE_OPERATORS, ">")],
        inputs: &[
            PinSpec::value("A", PinType::Number),
            PinSpec::value("B", PinType::Number),
        ],
        outputs: &[PinSpec::value("Result", PinType::Bool)],
    },
    NodeSpec {
        kind: "join",
        title: "Join Text",
        category: "Data",
        help: "A's text followed by B's.",
        fields: &[],
        inputs: &[
            PinSpec::value("A", PinType::Text),
            PinSpec::value("B", PinType::Text),
        ],
        outputs: &[PinSpec::value("Result", PinType::Text)],
    },
    NodeSpec {
        kind: "get_variable",
        title: "Get Variable",
        category: "Variables",
        help: "A variable's value.",
        fields: &[VARIABLE],
        inputs: &[],
        outputs: &[PinSpec::value("Value", PinType::Any)],
    },
    NodeSpec {
        kind: "set_variable",
        title: "Set Variable",
        category: "Variables",
        help: "Changes a variable's value.",
        fields: &[VARIABLE],
        inputs: &[IN, PinSpec::required("Value", PinType::Any)],
        outputs: &[THEN],
    },
];

/// The UI Event Graph's nodes, as a [`NodeLibrary`].
#[derive(Clone, Copy, Debug, Default)]
pub struct UiNodes;

impl NodeLibrary for UiNodes {
    fn specs(&self) -> &[NodeSpec] {
        UI_NODES
    }
}

/// An object a graph can name: where it is and its class.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct GraphObject {
    pub root: ObjectRoot,
    /// Its names from `root` down, joined with `/`; empty for the
    /// ScreenGui itself.
    pub path: String,
    pub class: String,
}

/// Why a graph didn't compile: on one node, or on the whole graph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GraphError {
    pub node: Option<u32>,
    pub message: String,
}

impl fmt::Display for GraphError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.node {
            Some(node) => write!(f, "node {node}: {}", self.message),
            None => f.write_str(&self.message),
        }
    }
}

/// A graph's Lua.
#[derive(Clone, Debug, PartialEq)]
pub struct CompiledGraph {
    pub source: String,
    /// The line (from 1) each node's code starts on, with the node's id,
    /// so a runtime error can point at its node.
    pub node_lines: Vec<(usize, u32)>,
}

impl CompiledGraph {
    /// The node whose code is on `line`: the last one starting at or
    /// before it.
    pub fn node_at(&self, line: usize) -> Option<u32> {
        self.node_lines
            .iter()
            .filter(|(start, _)| *start <= line)
            .max_by_key(|(start, _)| *start)
            .map(|(_, id)| *id)
    }
}

/// Names the generated script declares itself, which variables can't use.
const RESERVED: &[&str] = &[
    "gui",
    "deltaTime",
    "RunService",
    "TweenService",
    "ReplicatedStorage",
    "screenShown",
    "script",
    "game",
    "workspace",
];

/// The first line of every compiled graph.
pub const GENERATED_HEADER: &str =
    "-- Made by the UI Event Graph. Don't edit this: saving the graph makes it again.";

/// Compiles `graph` to the Lua of its ScreenGui's LocalScript. `objects`
/// are the objects there are now (see [`GraphObject`]): a node naming one
/// that isn't there is an error. Nodes not wired to an event don't run,
/// and aren't checked.
pub fn compile_ui_graph(
    graph: &NodeGraph,
    objects: &[GraphObject],
) -> Result<CompiledGraph, Vec<GraphError>> {
    let mut compiler = Compiler {
        graph,
        objects,
        lines: Vec::new(),
        indent: 0,
        node_lines: Vec::new(),
        errors: Vec::new(),
        services: BTreeSet::new(),
        reported: BTreeSet::new(),
    };
    compiler.check_nodes();
    let variables = compiler.variables();
    let mut events: Vec<&GraphNode> = graph
        .nodes
        .iter()
        .filter(|node| UiNodes.spec(&node.kind).is_some_and(NodeSpec::is_event))
        .collect();
    events.sort_by_key(|node| node.id);
    for event in events {
        compiler.event(event);
    }
    if !compiler.errors.is_empty() {
        let mut errors = compiler.errors;
        errors.dedup();
        return Err(errors);
    }

    let mut header = vec![
        GENERATED_HEADER.to_owned(),
        "local gui = script.Parent".to_owned(),
    ];
    for service in &compiler.services {
        header.push(format!("local {service} = game:GetService(\"{service}\")"));
    }
    header.extend(variables);
    if !compiler.lines.is_empty() {
        header.push(String::new());
    }
    let shift = header.len();
    let mut source = header.join("\n");
    source.push('\n');
    for line in &compiler.lines {
        source.push_str(line);
        source.push('\n');
    }
    Ok(CompiledGraph {
        source,
        node_lines: compiler
            .node_lines
            .into_iter()
            .map(|(line, node)| (line + shift, node))
            .collect(),
    })
}

/// Writes a graph's Lua, collecting errors as it goes.
struct Compiler<'a> {
    graph: &'a NodeGraph,
    objects: &'a [GraphObject],
    lines: Vec<String>,
    indent: usize,
    /// The line (from 1, before the header) each node's code starts on.
    node_lines: Vec<(usize, u32)>,
    errors: Vec<GraphError>,
    /// `game:GetService` locals the code uses.
    services: BTreeSet<&'static str>,
    /// Nodes already given an error, so a node reached twice reports once.
    reported: BTreeSet<(u32, String)>,
}

impl<'a> Compiler<'a> {
    fn error(&mut self, node: Option<u32>, message: impl Into<String>) {
        let message = message.into();
        if let Some(id) = node
            && !self.reported.insert((id, message.clone()))
        {
            return;
        }
        self.errors.push(GraphError { node, message });
    }

    fn line(&mut self, text: impl AsRef<str>) {
        let text = text.as_ref();
        self.lines
            .push(format!("{}{text}", "    ".repeat(self.indent)));
    }

    fn mark(&mut self, node: u32) {
        self.node_lines.push((self.lines.len() + 1, node));
    }

    /// Unknown kinds, and wires to pins that aren't there.
    fn check_nodes(&mut self) {
        let mut ids = BTreeSet::new();
        for node in &self.graph.nodes {
            if !ids.insert(node.id) {
                self.error(Some(node.id), "Two nodes have this id. Delete one.");
            }
            if UiNodes.spec(&node.kind).is_none() {
                self.error(
                    Some(node.id),
                    format!("`{}` isn't a UI Event Graph node.", node.kind),
                );
            }
        }
        for wire in &self.graph.wires {
            let from = self.graph.pin_type(&UiNodes, &wire.from, true);
            let to = self.graph.pin_type(&UiNodes, &wire.to, false);
            match (from, to) {
                (Some(from), Some(to)) if to.accepts(from) => {}
                (Some(_), Some(_)) => self.error(
                    Some(wire.to.node),
                    format!("The wire into {} doesn't fit it.", pin_label(&wire.to.pin)),
                ),
                _ => self.error(
                    Some(wire.to.node),
                    "A wire here goes to a pin this node doesn't have.",
                ),
            }
        }
    }

    /// The variables' declarations, checking their names.
    fn variables(&mut self) -> Vec<String> {
        let mut seen = BTreeSet::new();
        let mut lines = Vec::new();
        for variable in &self.graph.variables {
            if !valid_identifier(&variable.name) || RESERVED.contains(&variable.name.as_str()) {
                self.error(
                    None,
                    format!(
                        "The variable name `{}` can't be used: use letters, digits and \
                         underscores, not starting with a digit, and not a name the \
                         graph's Lua uses ({}).",
                        variable.name,
                        RESERVED.join(", ")
                    ),
                );
                continue;
            }
            if !seen.insert(variable.name.as_str()) {
                self.error(
                    None,
                    format!("Two variables are called `{}`.", variable.name),
                );
                continue;
            }
            let value = typed_any(&variable.value).unwrap_or_else(|| "0".to_owned());
            lines.push(format!("local {} = {value}", variable.name));
        }
        lines
    }

    fn event(&mut self, event: &GraphNode) {
        let then = PinRef::new(event.id, "");
        // An event with nothing wired after it does nothing.
        if self.graph.wires_from(&then).next().is_none() {
            return;
        }
        self.mark(event.id);
        match event.kind.as_str() {
            "button_activated" | "hovered" | "focus_lost" => {
                let (field, signal) = match event.kind.as_str() {
                    "button_activated" => ("Button", "Activated"),
                    "hovered" => ("Element", "MouseEnter"),
                    _ => ("TextBox", "FocusLost"),
                };
                let Some(target) = self.object(event, field) else {
                    return;
                };
                self.line(format!("{target}.{signal}:Connect(function()"));
                self.body(&then, event.id);
                self.line("end)");
            }
            "every_frame" => {
                self.services.insert("RunService");
                self.line("RunService.Heartbeat:Connect(function(deltaTime)");
                self.body(&then, event.id);
                self.line("end)");
            }
            "screen_shown" => {
                self.line("do");
                self.indent += 1;
                self.line("local function screenShown()");
                self.body(&then, event.id);
                self.line("end");
                self.line("gui:GetPropertyChangedSignal(\"Enabled\"):Connect(function()");
                self.line("    if gui.Enabled then");
                self.line("        screenShown()");
                self.line("    end");
                self.line("end)");
                self.line("if gui.Enabled then");
                self.line("    task.spawn(screenShown)");
                self.line("end");
                self.indent -= 1;
                self.line("end");
            }
            _ => {}
        }
        self.line("");
    }

    /// The chain after exec output `from`, one level in.
    fn body(&mut self, from: &PinRef, event: u32) {
        self.indent += 1;
        self.chain(from, event, &mut Vec::new());
        self.indent -= 1;
    }

    /// The nodes wired after exec output `from`, in order. `stack` is the
    /// chain so far, to refuse a loop.
    fn chain(&mut self, from: &PinRef, event: u32, stack: &mut Vec<u32>) {
        let next: Vec<u32> = self
            .graph
            .wires_from(from)
            .map(|wire| wire.to.node)
            .collect();
        for node in next {
            if stack.contains(&node) {
                self.error(
                    Some(node),
                    "This node's exec wires loop back into it. Use Every Frame to repeat.",
                );
                continue;
            }
            let Some(node) = self.graph.node(node) else {
                continue;
            };
            stack.push(node.id);
            self.statement(node, event, stack);
            stack.pop();
        }
    }

    fn statement(&mut self, node: &GraphNode, event: u32, stack: &mut Vec<u32>) {
        let Some(spec) = UiNodes.spec(&node.kind) else {
            return;
        };
        if spec.is_event() || !spec.inputs.iter().any(|pin| pin.ty == PinType::Exec) {
            self.error(
                Some(node.id),
                "Only nodes with a white exec input can run in a chain.",
            );
            return;
        }
        self.mark(node.id);
        let then = PinRef::new(node.id, "");
        match node.kind.as_str() {
            "set_property" => {
                let target = self.object(node, "Object");
                let property = self.property(node);
                let value = self.input(node, "Value", event);
                if let (Some(target), Some(property), Some(value)) = (target, property, value) {
                    self.line(format!("{target}.{property} = {value}"));
                }
            }
            "show_page" => {
                let target = self.object(node, "Pages");
                let page = self.input(node, "Page", event);
                if let (Some(target), Some(page)) = (target, page) {
                    self.line(format!("{target}.CurrentPage = {page}"));
                }
            }
            "set_visible" => {
                let target = self.object(node, "Element");
                let visible = self.input(node, "Visible", event);
                let property = if self.class_of(node, "Element") == Some("ScreenGui") {
                    "Enabled"
                } else {
                    "Visible"
                };
                if let (Some(target), Some(visible)) = (target, visible) {
                    self.line(format!("{target}.{property} = {visible}"));
                }
            }
            "tween" => {
                let target = self.object(node, "Object");
                let property = self.property(node);
                let goal = self.input(node, "Goal", event);
                let seconds = self.input(node, "Seconds", event);
                let style = self.choice(node, "Style", &EASING_STYLES);
                let direction = self.choice(node, "Direction", &EASING_DIRECTIONS);
                self.services.insert("TweenService");
                if let (
                    Some(target),
                    Some(property),
                    Some(goal),
                    Some(seconds),
                    Some(style),
                    Some(direction),
                ) = (target, property, goal, seconds, style, direction)
                {
                    self.line(format!(
                        "TweenService:Create({target}, TweenInfo.new({seconds}, \
                         Enum.EasingStyle.{style}, Enum.EasingDirection.{direction}), \
                         {{{property} = {goal}}}):Play()"
                    ));
                }
            }
            "play_sound" => {
                if let Some(sound) = self.object(node, "Sound") {
                    self.line(format!("{sound}:Play()"));
                }
            }
            "fire_remote" => {
                let remote = self.object(node, "RemoteEvent");
                let wired = self
                    .graph
                    .wires_into(&PinRef::new(node.id, "Value"))
                    .next()
                    .is_some();
                let value = if wired || !node.value("Value").is_empty() {
                    self.input(node, "Value", event)
                } else {
                    Some(String::new())
                };
                if let (Some(remote), Some(value)) = (remote, value) {
                    self.line(format!("{remote}:FireServer({value})"));
                }
            }
            "print" => {
                if let Some(value) = self.input(node, "Value", event) {
                    self.line(format!("print({value})"));
                }
            }
            "set_variable" => {
                let name = self.variable(node);
                let value = self.input(node, "Value", event);
                if let (Some(name), Some(value)) = (name, value) {
                    self.line(format!("{name} = {value}"));
                }
            }
            "branch" => {
                let condition = self
                    .input(node, "Condition", event)
                    .unwrap_or_else(|| "false".to_owned());
                self.line(format!("if {condition} then"));
                self.indent += 1;
                self.chain(&PinRef::new(node.id, "True"), event, stack);
                self.indent -= 1;
                let otherwise = PinRef::new(node.id, "False");
                if self.graph.wires_from(&otherwise).next().is_some() {
                    self.line("else");
                    self.indent += 1;
                    self.chain(&otherwise, event, stack);
                    self.indent -= 1;
                }
                self.line("end");
                return;
            }
            "sequence" => {
                for out in ["Then 0", "Then 1", "Then 2"] {
                    self.chain(&PinRef::new(node.id, out), event, stack);
                }
                return;
            }
            "delay" => {
                if let Some(seconds) = self.input(node, "Seconds", event) {
                    self.line(format!("task.wait({seconds})"));
                }
                self.chain(&PinRef::new(node.id, "Completed"), event, stack);
                return;
            }
            _ => {}
        }
        self.chain(&then, event, stack);
    }

    /// The Lua for input `pin` of `node`: what's wired into it, or what's
    /// typed in its box. `None` (with an error) when it can't be made.
    fn input(&mut self, node: &GraphNode, pin: &str, event: u32) -> Option<String> {
        self.input_within(node, pin, event, &mut Vec::new())
    }

    fn input_within(
        &mut self,
        node: &GraphNode,
        pin: &str,
        event: u32,
        stack: &mut Vec<u32>,
    ) -> Option<String> {
        let spec = UiNodes.spec(&node.kind)?.input(pin)?;
        let wire = self
            .graph
            .wires_into(&PinRef::new(node.id, pin))
            .next()
            .cloned();
        if let Some(wire) = wire {
            let from_type = self.graph.pin_type(&UiNodes, &wire.from, true)?;
            let value = self.output(node, &wire.from, event, stack)?;
            return Some(match (spec.ty, from_type) {
                (PinType::Text, PinType::Number | PinType::Bool) => format!("tostring({value})"),
                _ => value,
            });
        }
        let typed = node.value(pin);
        let label = pin_label(pin);
        let value = match spec.ty {
            PinType::Number => {
                let text = if typed.trim().is_empty() {
                    "0"
                } else {
                    typed.trim()
                };
                match text.parse::<f64>() {
                    Ok(number) if number.is_finite() => number.to_string(),
                    _ => {
                        self.error(Some(node.id), format!("{label} isn't a number: `{typed}`."));
                        return None;
                    }
                }
            }
            PinType::Bool => match typed.trim() {
                "" | "false" => "false".to_owned(),
                "true" => "true".to_owned(),
                _ => {
                    self.error(Some(node.id), format!("{label} must be true or false."));
                    return None;
                }
            },
            PinType::Text if spec.required && typed.is_empty() => {
                self.error(
                    Some(node.id),
                    format!("{label} needs a value: type one, or wire one in."),
                );
                return None;
            }
            PinType::Text => quote(typed),
            PinType::Any => match typed_any(typed) {
                Some(value) => value,
                None => {
                    self.error(
                        Some(node.id),
                        format!("{label} isn't connected: wire a value in, or type one."),
                    );
                    return None;
                }
            },
            PinType::Exec => return None,
        };
        Some(value)
    }

    /// The Lua for output `pin`, used by `user`.
    fn output(
        &mut self,
        user: &GraphNode,
        pin: &PinRef,
        event: u32,
        stack: &mut Vec<u32>,
    ) -> Option<String> {
        if stack.contains(&pin.node) {
            self.error(Some(pin.node), "This value depends on itself.");
            return None;
        }
        let node = self.graph.node(pin.node)?;
        let spec = UiNodes.spec(&node.kind)?;
        if spec.is_event() {
            if node.id != event {
                self.error(
                    Some(user.id),
                    format!(
                        "{} comes from another event ({}), so it isn't known here.",
                        pin_label(&pin.pin),
                        spec.title
                    ),
                );
                return None;
            }
            return match (node.kind.as_str(), pin.pin.as_str()) {
                ("every_frame", "Delta Time") => Some("deltaTime".to_owned()),
                ("focus_lost", "Text") => Some(format!("{}.Text", self.object(node, "TextBox")?)),
                _ => None,
            };
        }
        stack.push(node.id);
        let value = match node.kind.as_str() {
            "get_property" => {
                let target = self.object(node, "Object");
                let property = self.property(node);
                Some(format!("{}.{}", target?, property?))
            }
            "get_text" => self
                .object(node, "Element")
                .map(|target| format!("{target}.Text")),
            "math" => {
                let a = self.input_within(node, "A", event, stack);
                let b = self.input_within(node, "B", event, stack);
                let operator = self.choice(node, "Operator", MATH_OPERATORS);
                Some(format!("({} {} {})", a?, operator?, b?))
            }
            "compare" => {
                let a = self.input_within(node, "A", event, stack);
                let b = self.input_within(node, "B", event, stack);
                let operator = self
                    .choice(node, "Operator", COMPARE_OPERATORS)
                    .map(|shown| match shown {
                        "≤" => "<=",
                        "=" => "==",
                        "≠" => "~=",
                        "≥" => ">=",
                        other => other,
                    });
                Some(format!("({} {} {})", a?, operator?, b?))
            }
            "join" => {
                let a = self.input_within(node, "A", event, stack);
                let b = self.input_within(node, "B", event, stack);
                Some(format!("({} .. {})", a?, b?))
            }
            "get_variable" => self.variable(node),
            _ => {
                self.error(Some(node.id), "This node has no value to wire out.");
                None
            }
        };
        stack.pop();
        value
    }

    /// The Lua for the object in `field` of `node`, checked against the
    /// objects there are.
    fn object(&mut self, node: &GraphNode, field: &str) -> Option<String> {
        let spec = UiNodes.spec(&node.kind)?.field(field)?;
        let FieldKind::Object { root, classes } = spec.kind else {
            return None;
        };
        let path = node.value(field);
        let what = classes.first().copied().unwrap_or("element");
        // Empty is the ScreenGui itself, where one fits.
        let gui_itself = root == ObjectRoot::Gui
            && path.is_empty()
            && (classes.is_empty() || classes.contains(&"ScreenGui"));
        if path.is_empty() && !gui_itself {
            self.error(Some(node.id), format!("Pick a {what} for {field}."));
            return None;
        }
        let Some(found) = self
            .objects
            .iter()
            .find(|object| object.root == root && object.path == path)
        else {
            let place = match root {
                ObjectRoot::Gui => "this ScreenGui",
                ObjectRoot::Workspace => "the Workspace",
                ObjectRoot::ReplicatedStorage => "ReplicatedStorage",
            };
            self.error(
                Some(node.id),
                format!("There's no `{path}` in {place} now. Pick another {what}."),
            );
            return None;
        };
        if !classes.is_empty() && !classes.contains(&found.class.as_str()) {
            self.error(
                Some(node.id),
                format!(
                    "`{path}` is a {}, not a {}.",
                    found.class,
                    classes.join(" or ")
                ),
            );
            return None;
        }
        let base = match root {
            ObjectRoot::Gui => "gui".to_owned(),
            ObjectRoot::Workspace => "workspace".to_owned(),
            ObjectRoot::ReplicatedStorage => {
                self.services.insert("ReplicatedStorage");
                "ReplicatedStorage".to_owned()
            }
        };
        Some(
            path.split('/')
                .filter(|name| !name.is_empty())
                .fold(base, |prefix, name| {
                    if valid_identifier(name) {
                        format!("{prefix}.{name}")
                    } else {
                        format!("{prefix}[{}]", quote(name))
                    }
                }),
        )
    }

    /// The class of the object in `field` of `node`, if it's there.
    fn class_of(&self, node: &GraphNode, field: &str) -> Option<&'a str> {
        let path = node.value(field);
        self.objects
            .iter()
            .find(|object| object.root == ObjectRoot::Gui && object.path == path)
            .map(|object| object.class.as_str())
    }

    /// The node's Property field, if it's a property name.
    fn property(&mut self, node: &GraphNode) -> Option<String> {
        let name = node.value("Property").trim();
        if valid_identifier(name) {
            Some(name.to_owned())
        } else {
            self.error(
                Some(node.id),
                format!("`{name}` isn't a property name, such as Text or Visible."),
            );
            None
        }
    }

    fn choice(
        &mut self,
        node: &GraphNode,
        field: &str,
        choices: &[&'static str],
    ) -> Option<&'static str> {
        let value = node.value(field);
        let found = choices.iter().copied().find(|choice| *choice == value);
        if found.is_none() {
            self.error(
                Some(node.id),
                format!("{field} must be one of {}.", choices.join(", ")),
            );
        }
        found
    }

    /// The node's Variable field, if the graph has that variable.
    fn variable(&mut self, node: &GraphNode) -> Option<String> {
        let name = node.value("Variable");
        if name.is_empty() {
            self.error(Some(node.id), "Pick a variable.");
            return None;
        }
        if !self
            .graph
            .variables
            .iter()
            .any(|variable| variable.name == name)
        {
            self.error(
                Some(node.id),
                format!("The graph has no variable called `{name}`."),
            );
            return None;
        }
        Some(name.to_owned())
    }
}

/// A pin's name as messages say it.
fn pin_label(pin: &str) -> String {
    if pin.is_empty() {
        "The exec input".to_owned()
    } else {
        pin.to_owned()
    }
}

/// A value typed into an Any box, as Lua: a number, `true` or `false`, or
/// text. `None` when nothing's typed.
fn typed_any(typed: &str) -> Option<String> {
    let trimmed = typed.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(number) = trimmed.parse::<f64>()
        && number.is_finite()
    {
        return Some(number.to_string());
    }
    Some(match trimmed {
        "true" | "false" => trimmed.to_owned(),
        _ => quote(typed),
    })
}

/// Errors grouped by node, for showing on them.
pub fn errors_by_node(errors: &[GraphError]) -> BTreeMap<u32, Vec<String>> {
    let mut by_node: BTreeMap<u32, Vec<String>> = BTreeMap::new();
    for error in errors {
        if let Some(node) = error.node {
            by_node.entry(node).or_default().push(error.message.clone());
        }
    }
    by_node
}
