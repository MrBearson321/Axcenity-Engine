//! Behavior trees: a NavigationAgent's `BehaviorTree` file, checked and
//! run every frame of Play to decide where the agent walks.
//!
//! The file is a node graph (`assets/ai/<Name>.behaviortree.json`, made in
//! the editor's Behavior Tree tab; its nodes are `blocks::behavior_tree`).
//! A Root's pin is wired to one node, and a Selector's or Sequence's pins
//! 1 to 4 to its children. Each frame, before the agent walks, the tree is
//! ticked from its Root, and every node it reaches says it succeeded,
//! failed, or is still running:
//!
//! - **Selector** tries its children in order, from the first every frame,
//!   and stops at the first that succeeds or runs. A child it no longer
//!   reaches is stopped (its Wait starts again next time).
//! - **Sequence** runs its children in order while each succeeds, and
//!   picks up at the one still running next frame. Conditions before that
//!   one are checked again every frame, so a Sees Player? that turns false
//!   stops the chase after it.
//! - **Sees Player?** and **Hears Player?** read `SeesPlayer` and
//!   `HearsPlayer` (from last frame's perception).
//! - **Move To** sets `Destination` and runs until `Status` says
//!   `"Reached"` (success) or `"Unreachable"` or `"Blocked"` (failure).
//!   **Set Destination** sets it and succeeds at once.
//! - **Chase Player** sets `Destination` to the player character's feet,
//!   again whenever they've moved a stud from it, and succeeds once the
//!   agent is beside them (and stops it there).
//! - **Wait** runs for its Seconds of game time.
//!
//! A tree that can't be read or has problems doesn't run, and the agent
//! behaves as if it had none; [`BehaviorTree::from_json`] gives each
//! problem with its node, for the editor to show.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;
use std::sync::Arc;

use axcenity_assets::Assets;
use axcenity_core::{InstanceId, Value, Vector3, World};
use serde::Deserialize;

/// A behavior tree's file ending.
pub const BEHAVIOR_TREE_SUFFIX: &str = ".behaviortree.json";
/// A state machine's file ending: an Animator's, never an agent's.
const STATE_MACHINE_SUFFIX: &str = ".statemachine.json";
/// The newest tree file this engine reads.
const FORMAT_VERSION: u32 = 1;
/// The most nodes a tree unfolds to (a node wired under several parents
/// counts once for each), so a damaged file can't run away.
const MAX_NODES: usize = 4096;
/// How near the player character, across, counts as beside them.
const CATCH_DISTANCE: f32 = 2.5;
/// How far the player character moves from a chase's Destination before
/// it's set again (finding a new path).
const RETARGET_DISTANCE: f32 = 1.0;

/// A node kind: its name, its child pins (in the order they run) and its
/// fields.
pub type NodeKind = (
    &'static str,
    &'static [&'static str],
    &'static [&'static str],
);

/// Every node kind, as `blocks::behavior_tree` must offer them.
pub const BEHAVIOR_NODE_KINDS: &[NodeKind] = &[
    ("root", &[""], &[]),
    ("selector", &["1", "2", "3", "4"], &[]),
    ("sequence", &["1", "2", "3", "4"], &[]),
    ("sees_player", &[], &[]),
    ("hears_player", &[], &[]),
    ("chase_player", &[], &[]),
    ("move_to", &[], &["X", "Y", "Z"]),
    ("set_destination", &[], &["X", "Y", "Z"]),
    ("wait", &[], &["Seconds"]),
];

/// A problem with a tree, on one of its nodes (by id) or the whole tree.
#[derive(Clone, Debug, PartialEq)]
pub struct TreeError {
    pub node: Option<u32>,
    pub message: String,
}

impl TreeError {
    fn on(node: u32, message: impl Into<String>) -> Self {
        Self {
            node: Some(node),
            message: message.into(),
        }
    }

    fn whole(message: impl Into<String>) -> Self {
        Self {
            node: None,
            message: message.into(),
        }
    }
}

impl fmt::Display for TreeError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.node {
            Some(node) => write!(f, "node {node}: {}", self.message),
            None => f.write_str(&self.message),
        }
    }
}

// The file, as far as running it goes: positions and comments are the
// editor's, and ignored here.
#[derive(Deserialize)]
struct TreeFile {
    #[serde(default = "first_version")]
    format_version: u32,
    #[serde(default)]
    nodes: Vec<FileNode>,
    #[serde(default)]
    wires: Vec<FileWire>,
}

fn first_version() -> u32 {
    FORMAT_VERSION
}

#[derive(Deserialize)]
struct FileNode {
    id: u32,
    kind: String,
    #[serde(default)]
    values: BTreeMap<String, String>,
}

#[derive(Deserialize)]
struct FileWire {
    from: FileEnd,
    to: FileEnd,
}

#[derive(Deserialize)]
struct FileEnd {
    node: u32,
    pin: String,
}

/// What a node does.
#[derive(Clone, Debug, PartialEq)]
enum Step {
    Selector(Vec<usize>),
    Sequence(Vec<usize>),
    SeesPlayer,
    HearsPlayer,
    ChasePlayer,
    MoveTo(Vector3),
    SetDestination(Vector3),
    Wait(f32),
}

impl Step {
    fn is_condition(&self) -> bool {
        matches!(self, Step::SeesPlayer | Step::HearsPlayer)
    }
}

/// A checked tree, ready to run. A node wired under several parents is
/// unfolded into one copy under each, so each copy keeps its own state.
#[derive(Clone, Debug, PartialEq)]
pub struct BehaviorTree {
    /// The tree's nodes; the first is what Root runs.
    steps: Vec<Step>,
}

impl BehaviorTree {
    /// Reads and checks a tree file's text. Every problem comes back, each
    /// with its node where it has one.
    pub fn from_json(text: &str) -> Result<Self, Vec<TreeError>> {
        let file: TreeFile = serde_json::from_str(text)
            .map_err(|err| vec![TreeError::whole(format!("it isn't a behavior tree: {err}"))])?;
        if file.format_version > FORMAT_VERSION {
            return Err(vec![TreeError::whole(
                "it was made by a newer version of Axcenity",
            )]);
        }
        Checker::new(&file).check()
    }

    /// Reads the tree at `path` in the project. The error says why it can't
    /// run, such as a state machine named where a tree should be.
    pub fn load(assets: &Assets, path: &str) -> Result<Self, String> {
        let lower = path.to_lowercase();
        if lower.ends_with(STATE_MACHINE_SUFFIX) {
            return Err(
                "it's a state machine, which plays animation on an Animator. An agent's \
                 BehaviorTree is a .behaviortree.json file, made in the Behavior Tree tab"
                    .to_owned(),
            );
        }
        if !lower.ends_with(BEHAVIOR_TREE_SUFFIX) {
            return Err("it isn't a behavior tree (a .behaviortree.json file)".to_owned());
        }
        let bytes = assets.read_file(path)?;
        let text = String::from_utf8(bytes).map_err(|_| "it isn't text".to_owned())?;
        Self::from_json(&text).map_err(|errors| {
            let problems: Vec<String> = errors.iter().map(ToString::to_string).collect();
            format!(
                "it has problems, shown on its nodes in the Behavior Tree tab: {}",
                problems.join("; ")
            )
        })
    }

    /// How many nodes it runs, a node under two parents counting twice.
    pub fn len(&self) -> usize {
        self.steps.len()
    }

    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }
}

/// Checks a file and unfolds it from its Root.
struct Checker<'a> {
    nodes: BTreeMap<u32, &'a FileNode>,
    /// Each node's children by pin, from its wires.
    children: BTreeMap<(u32, &'a str), Vec<u32>>,
    errors: Vec<TreeError>,
    steps: Vec<Step>,
}

fn spec(kind: &str) -> Option<&'static NodeKind> {
    BEHAVIOR_NODE_KINDS
        .iter()
        .find(|(name, _, _)| *name == kind)
}

impl<'a> Checker<'a> {
    fn new(file: &'a TreeFile) -> Self {
        let mut checker = Self {
            nodes: BTreeMap::new(),
            children: BTreeMap::new(),
            errors: Vec::new(),
            steps: Vec::new(),
        };
        for node in &file.nodes {
            if checker.nodes.insert(node.id, node).is_some() {
                checker
                    .errors
                    .push(TreeError::on(node.id, "Two nodes have this id."));
            }
        }
        for wire in &file.wires {
            let (Some(from), Some(to)) = (
                checker.nodes.get(&wire.from.node),
                checker.nodes.get(&wire.to.node),
            ) else {
                continue;
            };
            let pins = spec(&from.kind).map_or(&[][..], |(_, pins, _)| pins);
            let takes_parent = spec(&to.kind).is_some_and(|(kind, _, _)| *kind != "root");
            if !pins.contains(&wire.from.pin.as_str()) || !wire.to.pin.is_empty() || !takes_parent {
                checker.errors.push(TreeError::on(
                    from.id,
                    "A wire from here doesn't go from a child pin to a node's input.",
                ));
                continue;
            }
            checker
                .children
                .entry((from.id, wire.from.pin.as_str()))
                .or_default()
                .push(to.id);
        }
        checker
    }

    fn check(mut self) -> Result<BehaviorTree, Vec<TreeError>> {
        let mut roots = Vec::new();
        for node in self.nodes.values() {
            match spec(&node.kind) {
                None => self.errors.push(TreeError::on(
                    node.id,
                    format!("There's no \"{}\" node in a behavior tree.", node.kind),
                )),
                Some(_) if node.kind == "root" => roots.push(node.id),
                Some(_) => {}
            }
            // Every node's numbers are checked, wired up or not, so a
            // typo shows before it's needed.
            for field in spec(&node.kind).map_or(&[][..], |(_, _, fields)| fields) {
                if let Err(message) = number(node, field) {
                    self.errors.push(TreeError::on(node.id, message));
                }
            }
        }
        for (&(node, pin), children) in &self.children {
            if children.len() > 1 {
                let pin = if pin.is_empty() { "its pin" } else { pin };
                self.errors.push(TreeError::on(
                    node,
                    format!("Pin {pin} has more than one wire; it takes one child."),
                ));
            }
        }
        match roots[..] {
            [] => self.errors.push(TreeError::whole(
                "A behavior tree needs a Root node: it starts there.",
            )),
            [root, ref others @ ..] => {
                for &other in others {
                    self.errors.push(TreeError::on(
                        other,
                        "A tree has one Root: delete this one, or wire its branch under the first.",
                    ));
                }
                match self.child(root, "") {
                    Some(first) => {
                        let mut path = BTreeSet::from([root]);
                        self.unfold(first, &mut path);
                    }
                    None => self.errors.push(TreeError::on(
                        root,
                        "Wire what the tree does to Root's pin.",
                    )),
                }
            }
        }
        if self.errors.is_empty() {
            Ok(BehaviorTree { steps: self.steps })
        } else {
            Err(self.errors)
        }
    }

    fn child(&self, node: u32, pin: &str) -> Option<u32> {
        self.children
            .get(&(node, pin))
            .and_then(|children| children.first().copied())
    }

    /// Adds node `id` and everything under it to the steps. Returns its
    /// index there. `path` is the nodes above it, to catch a loop.
    fn unfold(&mut self, id: u32, path: &mut BTreeSet<u32>) -> usize {
        let index = self.steps.len();
        self.steps.push(Step::SeesPlayer);
        if self.steps.len() > MAX_NODES {
            if self.steps.len() == MAX_NODES + 1 {
                self.errors.push(TreeError::whole(format!(
                    "It unfolds to more than {MAX_NODES} nodes."
                )));
            }
            return index;
        }
        if !path.insert(id) {
            self.errors.push(TreeError::on(
                id,
                "This node is wired under itself, so the tree loops.",
            ));
            return index;
        }
        let node = self.nodes[&id];
        let point = || {
            let [x, y, z] = ["X", "Y", "Z"].map(|field| number(node, field).unwrap_or(0.0));
            Vector3::new(x, y, z)
        };
        let step = match node.kind.as_str() {
            "selector" | "sequence" => {
                let pins = spec(&node.kind).map_or(&[][..], |(_, pins, _)| pins);
                let wired: Vec<u32> = pins.iter().filter_map(|pin| self.child(id, pin)).collect();
                if wired.is_empty() {
                    self.errors.push(TreeError::on(
                        id,
                        "Wire at least one step to its pins (1 runs first).",
                    ));
                }
                let children = wired
                    .into_iter()
                    .map(|child| self.unfold(child, path))
                    .collect();
                if node.kind == "selector" {
                    Step::Selector(children)
                } else {
                    Step::Sequence(children)
                }
            }
            "sees_player" => Step::SeesPlayer,
            "hears_player" => Step::HearsPlayer,
            "chase_player" => Step::ChasePlayer,
            "move_to" => Step::MoveTo(point()),
            "set_destination" => Step::SetDestination(point()),
            "wait" => Step::Wait(number(node, "Seconds").unwrap_or(0.0)),
            // Already reported: no such kind. A second Root under a
            // node is an input it doesn't have, reported with its wire.
            _ => Step::Sequence(Vec::new()),
        };
        path.remove(&id);
        self.steps[index] = step;
        index
    }
}

/// Field `field` of `node` as a number. Seconds can't be below zero.
fn number(node: &FileNode, field: &str) -> Result<f32, String> {
    let text = node.values.get(field).map_or("", String::as_str).trim();
    let value: f32 = text
        .parse()
        .ok()
        .filter(|value: &f32| value.is_finite())
        .ok_or_else(|| format!("{field} needs a number, not \"{text}\"."))?;
    if field == "Seconds" && value < 0.0 {
        return Err("Seconds can't be below zero.".to_owned());
    }
    Ok(value)
}

/// What a node said this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Status {
    Success,
    Failure,
    Running,
}

/// What a running node remembers from one frame to the next.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Memory {
    /// A Sequence's child still running.
    Sequence(usize),
    /// A Wait's seconds so far.
    Wait(f32),
    /// A Move To or Chase Player has set its Destination.
    Walking,
}

/// One agent's tree as it runs. Not saved.
pub(crate) struct TreeRun {
    /// The `BehaviorTree` property it was made for.
    pub(crate) path: String,
    /// `None` when it can't run.
    tree: Option<Arc<BehaviorTree>>,
    memory: HashMap<usize, Memory>,
}

impl TreeRun {
    pub(crate) fn new(path: String, tree: Option<Arc<BehaviorTree>>) -> Self {
        Self {
            path,
            tree,
            memory: HashMap::new(),
        }
    }

    /// Ticks the tree once from its Root, for `dt` seconds. A node it ran
    /// last frame and didn't reach now is stopped (forgets its state).
    pub(crate) fn tick(&mut self, world: &mut World, agent: Agent, dt: f32) {
        let Some(tree) = self.tree.clone() else {
            return;
        };
        if tree.steps.is_empty() {
            return;
        }
        let mut tick = Tick {
            tree: &tree,
            memory: &mut self.memory,
            reached: vec![false; tree.steps.len()],
            world,
            agent,
            dt,
        };
        tick.run(0);
        let reached = tick.reached;
        self.memory.retain(|&index, _| reached[index]);
    }
}

/// The agent a tree runs for, this frame.
#[derive(Clone, Copy)]
pub(crate) struct Agent {
    pub(crate) id: InstanceId,
    pub(crate) feet: Vector3,
    /// The player character's feet, when there is one.
    pub(crate) player: Option<Vector3>,
}

struct Tick<'a> {
    tree: &'a BehaviorTree,
    memory: &'a mut HashMap<usize, Memory>,
    reached: Vec<bool>,
    world: &'a mut World,
    agent: Agent,
    dt: f32,
}

fn across(a: Vector3, b: Vector3) -> f32 {
    (a.x - b.x).hypot(a.z - b.z)
}

impl Tick<'_> {
    fn run(&mut self, index: usize) -> Status {
        self.reached[index] = true;
        match &self.tree.steps[index] {
            Step::Selector(children) => {
                for &child in children {
                    let status = self.run(child);
                    if status != Status::Failure {
                        return status;
                    }
                }
                Status::Failure
            }
            Step::Sequence(children) => {
                let start = match self.memory.remove(&index) {
                    Some(Memory::Sequence(at)) => at,
                    _ => 0,
                };
                for (at, &child) in children.iter().enumerate() {
                    // Steps already done are skipped; conditions are asked again.
                    if at < start && !self.tree.steps[child].is_condition() {
                        continue;
                    }
                    match self.run(child) {
                        Status::Success => {}
                        Status::Failure => return Status::Failure,
                        Status::Running => {
                            self.memory.insert(index, Memory::Sequence(at));
                            return Status::Running;
                        }
                    }
                }
                Status::Success
            }
            Step::SeesPlayer => self.condition("SeesPlayer"),
            Step::HearsPlayer => self.condition("HearsPlayer"),
            &Step::SetDestination(point) => {
                self.set_destination(point);
                Status::Success
            }
            &Step::MoveTo(point) => {
                if self.memory.remove(&index).is_none() {
                    let there = self
                        .destination()
                        .is_some_and(|destination| close(destination, point))
                        && self.status() == "Reached";
                    if there {
                        return Status::Success;
                    }
                    self.set_destination(point);
                    self.memory.insert(index, Memory::Walking);
                    return Status::Running;
                }
                match self.status().as_str() {
                    "Reached" => Status::Success,
                    "Unreachable" | "Blocked" => Status::Failure,
                    _ => {
                        self.memory.insert(index, Memory::Walking);
                        Status::Running
                    }
                }
            }
            Step::ChasePlayer => {
                let walking = self.memory.remove(&index).is_some();
                let Some(player) = self.agent.player else {
                    return Status::Failure;
                };
                if across(self.agent.feet, player) <= CATCH_DISTANCE {
                    // Beside them: stop here rather than walk into them.
                    self.set_destination(self.agent.feet);
                    return Status::Success;
                }
                let stale = self
                    .destination()
                    .is_none_or(|destination| across(destination, player) > RETARGET_DISTANCE);
                if !walking || stale {
                    self.set_destination(player);
                } else if matches!(self.status().as_str(), "Unreachable" | "Blocked") {
                    return Status::Failure;
                }
                self.memory.insert(index, Memory::Walking);
                Status::Running
            }
            &Step::Wait(seconds) => {
                let so_far = match self.memory.remove(&index) {
                    Some(Memory::Wait(so_far)) => so_far,
                    _ => 0.0,
                } + self.dt;
                if so_far >= seconds {
                    Status::Success
                } else {
                    self.memory.insert(index, Memory::Wait(so_far));
                    Status::Running
                }
            }
        }
    }

    fn condition(&self, property: &str) -> Status {
        let on = self
            .world
            .get(self.agent.id)
            .and_then(|agent| agent.property(property))
            .and_then(Value::as_bool)
            == Some(true);
        if on { Status::Success } else { Status::Failure }
    }

    fn destination(&self) -> Option<Vector3> {
        self.world
            .get(self.agent.id)?
            .property("Destination")?
            .as_vector3()
    }

    fn status(&self) -> String {
        self.world
            .get(self.agent.id)
            .and_then(|agent| agent.property("Status"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    }

    /// Sets `Destination`, unless it's there already (so `Changed` only
    /// fires for a new one, and the walk under way carries on).
    fn set_destination(&mut self, point: Vector3) {
        if self.destination().is_some_and(|now| close(now, point)) {
            return;
        }
        let _ = self
            .world
            .set_property(self.agent.id, "Destination", Value::Vector3(point));
    }
}

fn close(a: Vector3, b: Vector3) -> bool {
    (a.x - b.x).abs() < 1e-3 && (a.y - b.y).abs() < 1e-3 && (a.z - b.z).abs() < 1e-3
}
