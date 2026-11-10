//! The Behavior Tree tab: a NavigationAgent's behavior tree, edited in the
//! generic graph editor (`node_graph.rs`) with the nodes in
//! `axcenity_blocks::behavior_tree`.
//!
//! A tree is its own asset, `assets/ai/<Name>.behaviortree.json`, and an
//! agent names it in its `BehaviorTree` property. The tab follows the
//! selected agent's tree, like the Animation tab follows an Animator, and
//! can show any tree in the project. It's checked as it's edited by the
//! same code that runs it in Play (`axcenity_nav::BehaviorTree`), so its
//! problems show on their nodes. Trees are written with the project's
//! Save; each finished edit is one step of the tree's own undo, used while
//! the pointer is over the graph.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use axcenity_blocks::behavior_tree::{BehaviorNodes, behavior_tree_name, new_behavior_tree};
use axcenity_blocks::node_graph::NodeGraph;
use axcenity_core::InstanceId;
use axcenity_nav::{BehaviorTree, TreeError};
use eframe::egui;

use crate::node_graph::{GraphContext, GraphView};
use crate::responsive;
use crate::theme::{self, icons};

/// One tree, as edited.
#[derive(Debug)]
pub struct TreeDocument {
    pub path: String,
    pub graph: NodeGraph,
    /// The graph as last read or written; `None` before it has a file.
    saved: Option<NodeGraph>,
    /// The graph before each edit, newest last, and the ones undone.
    undo: Vec<NodeGraph>,
    redo: Vec<NodeGraph>,
}

impl TreeDocument {
    fn unsaved(&self) -> bool {
        self.saved.as_ref() != Some(&self.graph)
    }
}

/// The selected NavigationAgent, for the tab to follow and offer the
/// shown tree to.
#[derive(Clone, Debug, PartialEq)]
pub struct SelectedAgent {
    pub id: InstanceId,
    pub name: String,
    /// Its `BehaviorTree`.
    pub tree: String,
}

/// What the tab is shown with.
pub struct TreeScene<'a> {
    /// The trees in the project folder, sorted.
    pub files: &'a [String],
    pub agent: Option<&'a SelectedAgent>,
    /// False during Play, or with no project: shown but not edited.
    pub editable: bool,
}

/// What was asked for in the tab, for the app to do.
#[derive(Clone, Debug, PartialEq)]
pub enum TreeAction {
    /// Show this tree.
    Show(String),
    /// Make a tree (for the selected agent, if there is one).
    New,
    /// Have this agent run this tree.
    Use { agent: InstanceId, path: String },
}

/// The tab's documents and what it shows.
#[derive(Default)]
pub struct BehaviorTreeStudio {
    documents: Vec<TreeDocument>,
    current: Option<String>,
    views: HashMap<String, GraphView>,
    /// The agent whose tree was last followed.
    followed: Option<InstanceId>,
    /// Why the tree an agent names can't be shown.
    problem: Option<String>,
    /// The graph has the pointer, so Ctrl+Z is its.
    hovered: bool,
}

/// Checks `graph` as Play would run it.
pub fn check(graph: &NodeGraph) -> Result<(), Vec<TreeError>> {
    let text = serde_json::to_string(graph).expect("a graph is plain data");
    BehaviorTree::from_json(&text).map(|_| ())
}

impl BehaviorTreeStudio {
    /// Whether a tree has changes to save.
    pub fn unsaved(&self) -> bool {
        self.documents.iter().any(TreeDocument::unsaved)
    }

    pub fn has_pointer(&self) -> bool {
        self.hovered
    }

    /// The path of the tree shown.
    #[cfg(test)]
    pub fn current(&self) -> Option<&str> {
        self.current.as_deref()
    }

    /// How far each graph drawn so far is zoomed.
    #[cfg(test)]
    pub fn view_zooms(&self) -> Vec<f32> {
        self.views.values().map(|view| view.zoom).collect()
    }

    /// Where the graph of the tree at `path` is looking, once it's drawn.
    #[cfg(test)]
    pub fn view(&self, path: &str) -> Option<&GraphView> {
        self.views.get(path)
    }

    #[cfg(test)]
    pub fn view_mut(&mut self, path: &str) -> Option<&mut GraphView> {
        self.views.get_mut(path)
    }

    pub fn document(&self, path: &str) -> Option<&TreeDocument> {
        self.documents.iter().find(|doc| doc.path == path)
    }

    pub fn document_mut(&mut self, path: &str) -> Option<&mut TreeDocument> {
        self.documents.iter_mut().find(|doc| doc.path == path)
    }

    /// The paths of trees made here and not saved yet.
    pub fn new_paths(&self) -> impl Iterator<Item = &str> {
        self.documents
            .iter()
            .filter(|doc| doc.saved.is_none())
            .map(|doc| doc.path.as_str())
    }

    /// Shows the tree at `path` in `folder`, reading it the first time.
    pub fn open(&mut self, folder: &Path, path: &str) -> Result<(), String> {
        if self.document(path).is_none() {
            let graph = axcenity_project::read_ui_graph(folder, path)
                .map_err(|err| format!("{:#}", anyhow::Error::new(err)))?;
            self.documents.push(TreeDocument {
                path: path.to_owned(),
                saved: Some(graph.clone()),
                graph,
                undo: Vec::new(),
                redo: Vec::new(),
            });
        }
        self.current = Some(path.to_owned());
        self.problem = None;
        Ok(())
    }

    /// Starts a tree at `path` (with a Root and nothing else) and shows it.
    /// It's written at the next save.
    pub fn create(&mut self, path: &str) {
        if self.document(path).is_none() {
            self.documents.push(TreeDocument {
                path: path.to_owned(),
                graph: new_behavior_tree(),
                saved: None,
                undo: Vec::new(),
                redo: Vec::new(),
            });
        }
        self.current = Some(path.to_owned());
        self.problem = None;
    }

    /// Follows the selected agent: when the selection moves to an agent
    /// with a tree, that tree is shown.
    pub fn follow(&mut self, folder: Option<&Path>, agent: Option<&SelectedAgent>) {
        let id = agent.map(|agent| agent.id);
        if id == self.followed {
            return;
        }
        self.followed = id;
        let (Some(folder), Some(agent)) = (folder, agent) else {
            return;
        };
        let path = agent.tree.trim();
        if path.is_empty() || self.current.as_deref() == Some(path) {
            return;
        }
        if !axcenity_blocks::behavior_tree::is_behavior_tree(path) {
            self.problem = Some(format!(
                "{}'s BehaviorTree, `{path}`, isn't a behavior tree, so it runs none in Play.",
                agent.name
            ));
            return;
        }
        if let Err(why) = self.open(folder, path) {
            self.problem = Some(format!(
                "{}'s tree `{path}` can't be read: {why}",
                agent.name
            ));
        }
    }

    /// Writes every tree with changes into `folder`. Returns the paths
    /// written, or why one couldn't be.
    pub fn save(&mut self, folder: &Path) -> Result<Vec<String>, String> {
        let mut written = Vec::new();
        for doc in &mut self.documents {
            if !doc.unsaved() {
                continue;
            }
            // A tree is a node graph file, written the way UI graphs are.
            axcenity_project::write_ui_graph(folder, &doc.path, &doc.graph).map_err(|err| {
                format!(
                    "Could not save `{}`: {:#}",
                    doc.path,
                    anyhow::Error::new(err)
                )
            })?;
            doc.saved = Some(doc.graph.clone());
            written.push(doc.path.clone());
        }
        Ok(written)
    }

    /// Undoes the last edit of the shown tree. False when there was none.
    pub fn undo(&mut self) -> bool {
        self.history(true)
    }

    pub fn redo(&mut self) -> bool {
        self.history(false)
    }

    fn history(&mut self, undo: bool) -> bool {
        let Some(path) = self.current.clone() else {
            return false;
        };
        let Some(doc) = self.document_mut(&path) else {
            return false;
        };
        let (from, to) = if undo {
            (&mut doc.undo, &mut doc.redo)
        } else {
            (&mut doc.redo, &mut doc.undo)
        };
        let Some(graph) = from.pop() else {
            return false;
        };
        to.push(std::mem::replace(&mut doc.graph, graph));
        true
    }

    /// The tab: which tree, the agent's button, whether it's ready, and
    /// the graph.
    pub fn show(&mut self, ui: &mut egui::Ui, scene: &TreeScene) -> Vec<TreeAction> {
        let mut actions = Vec::new();
        let palette = theme::palette(ui.ctx());
        self.hovered = false;
        responsive::toolbar(ui, |ui| {
            let shown = self
                .current
                .as_deref()
                .map_or("No tree", behavior_tree_name)
                .to_owned();
            let mut trees: Vec<&str> = scene.files.iter().map(String::as_str).collect();
            trees.extend(self.new_paths());
            trees.sort_unstable();
            trees.dedup();
            egui::ComboBox::from_id_salt("behavior tree picker")
                .selected_text(theme::with_icon(icons::BRAIN, &shown))
                .show_ui(ui, |ui| {
                    if trees.is_empty() {
                        ui.weak("The project has no behavior trees yet.");
                    }
                    for path in trees {
                        let current = self.current.as_deref() == Some(path);
                        if ui
                            .selectable_label(current, behavior_tree_name(path))
                            .on_hover_text(path)
                            .clicked()
                            && !current
                        {
                            actions.push(TreeAction::Show(path.to_owned()));
                        }
                    }
                });
            let new = ui
                .add_enabled(
                    scene.editable,
                    egui::Button::new(theme::with_icon(icons::PLUS, "New tree")),
                )
                .on_hover_text(match scene.agent {
                    Some(agent) => format!(
                        "Makes a tree in assets/ai with a Root, and has {} run it",
                        agent.name
                    ),
                    None => "Makes a tree in assets/ai with a Root".to_owned(),
                });
            if new.clicked() {
                actions.push(TreeAction::New);
            }
            if let (Some(agent), Some(current)) = (scene.agent, &self.current) {
                if agent.tree == *current {
                    ui.weak(format!("{} runs it in Play", agent.name));
                } else if ui
                    .add_enabled(
                        scene.editable,
                        egui::Button::new(format!("Use on {}", agent.name)),
                    )
                    .on_hover_text("Sets the agent's BehaviorTree to this tree")
                    .clicked()
                {
                    actions.push(TreeAction::Use {
                        agent: agent.id,
                        path: current.clone(),
                    });
                }
            }
        });
        if let Some(problem) = &self.problem {
            ui.colored_label(palette.error, problem);
        }
        let Some(path) = self.current.clone() else {
            ui.add_space(8.0);
            ui.label(
                "A behavior tree decides where a NavigationAgent walks during Play. For a \
                 guard: a Selector with Sees Player? then Chase Player under 1, and a patrol \
                 of Move To and Wait under 2.",
            );
            ui.weak(
                "Select an agent and press New tree, or double-click a tree in the Assets \
                 panel.",
            );
            return actions;
        };
        let Some(doc) = self.documents.iter_mut().find(|doc| doc.path == path) else {
            return actions;
        };
        let errors = check(&doc.graph).err().unwrap_or_default();
        if errors.is_empty() {
            ui.label(
                egui::RichText::new(theme::with_icon(
                    icons::CHECK_CIRCLE,
                    "Ready: agents that use it run it in Play",
                ))
                .color(palette.success),
            );
        } else {
            ui.label(
                egui::RichText::new(theme::with_icon(
                    icons::WARNING,
                    &format!(
                        "{} {}: agents that use it run no tree until {} fixed.",
                        errors.len(),
                        if errors.len() == 1 {
                            "problem"
                        } else {
                            "problems"
                        },
                        if errors.len() == 1 { "it's" } else { "they're" },
                    ),
                ))
                .color(palette.error),
            );
            for error in errors.iter().filter(|error| error.node.is_none()) {
                ui.colored_label(palette.error, &error.message);
            }
        }
        let mut by_node: BTreeMap<u32, Vec<String>> = BTreeMap::new();
        for error in &errors {
            if let Some(node) = error.node {
                by_node.entry(node).or_default().push(error.message.clone());
            }
        }
        let rect = ui.available_rect_before_wrap();
        let view = self.views.entry(path.clone()).or_default();
        let response = view.show(
            ui,
            rect,
            &mut doc.graph,
            &GraphContext {
                library: &BehaviorNodes,
                objects: &[],
                images: &[],
                errors: &by_node,
                editable: scene.editable,
                key: &path,
            },
        );
        self.hovered = response.hovered;
        if let Some(before) = response.step {
            doc.undo.push(before);
            doc.redo.clear();
        }
        actions
    }
}

#[cfg(test)]
mod tests {
    use axcenity_blocks::behavior_tree::BEHAVIOR_NODES;

    #[test]
    fn every_node_the_editor_offers_is_one_the_game_runs_with_the_same_pins_and_fields() {
        let offered: Vec<_> = BEHAVIOR_NODES
            .iter()
            .map(|spec| {
                let children: Vec<&str> = spec.outputs.iter().map(|pin| pin.name).collect();
                let fields: Vec<&str> = spec.fields.iter().map(|field| field.name).collect();
                (spec.kind, children, fields)
            })
            .collect();
        let run: Vec<_> = axcenity_nav::BEHAVIOR_NODE_KINDS
            .iter()
            .map(|(kind, children, fields)| (*kind, children.to_vec(), fields.to_vec()))
            .collect();
        assert_eq!(offered, run);
        // Every node but Root is wired from above, by its one plain input.
        for spec in BEHAVIOR_NODES {
            let inputs: Vec<&str> = spec.inputs.iter().map(|pin| pin.name).collect();
            let expected: &[&str] = if spec.kind == "root" { &[] } else { &[""] };
            assert_eq!(inputs, expected, "{}", spec.kind);
        }
        assert_eq!(
            axcenity_nav::BEHAVIOR_TREE_SUFFIX,
            axcenity_blocks::behavior_tree::BEHAVIOR_TREE_SUFFIX
        );
    }
}
