//! The Dialogue tab (Phase 77): a `.dialogue.json` asset, edited in the
//! generic graph editor (`node_graph.rs`) with the nodes in
//! `axcenity_blocks::dialogue`.
//!
//! A dialogue is its own asset, `assets/dialogue/<Name>.dialogue.json`. A
//! ProximityPrompt names it in its `Dialogue` property to start it when
//! triggered, with no script; scripts start one with
//! `DialogueService:Start`. The tab follows the selected prompt's dialogue
//! and can show any dialogue in the project. It's checked as it's edited
//! by the same code that runs it in Play (`axcenity_gameplay::dialogue`), so
//! problems show on their nodes. Dialogues are written with the project's
//! Save; each finished edit is one step of the dialogue's own undo, used
//! while the pointer is over the graph.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use axcenity_blocks::dialogue::{DialogueNodes, dialogue_name, is_dialogue, new_dialogue};
use axcenity_blocks::node_graph::NodeGraph;
use axcenity_core::InstanceId;
use axcenity_gameplay::dialogue::{Dialogue, DialogueError};
use eframe::egui;

use crate::node_graph::{GraphContext, GraphView};
use crate::responsive;
use crate::theme::{self, icons};

/// One dialogue, as edited.
#[derive(Debug)]
pub struct DialogueDocument {
    pub path: String,
    pub graph: NodeGraph,
    /// The graph as last read or written; `None` before it has a file.
    saved: Option<NodeGraph>,
    undo: Vec<NodeGraph>,
    redo: Vec<NodeGraph>,
}

impl DialogueDocument {
    fn unsaved(&self) -> bool {
        self.saved.as_ref() != Some(&self.graph)
    }
}

/// The selected ProximityPrompt, for the tab to follow and give the shown
/// dialogue to.
#[derive(Clone, Debug, PartialEq)]
pub struct SelectedPrompt {
    pub id: InstanceId,
    /// Who says it: the prompt's Model (or part).
    pub name: String,
    /// Its `Dialogue`.
    pub dialogue: String,
}

/// What the tab is shown with.
pub struct DialogueScene<'a> {
    /// The dialogues in the project folder, sorted.
    pub files: &'a [String],
    pub prompt: Option<&'a SelectedPrompt>,
    /// False during Play, or with no project.
    pub editable: bool,
}

/// What was asked for in the tab, for the app to do.
#[derive(Clone, Debug, PartialEq)]
pub enum DialogueAction {
    Show(String),
    New,
    Use { prompt: InstanceId, path: String },
}

/// The tab's documents and what it shows.
#[derive(Default)]
pub struct DialogueStudio {
    documents: Vec<DialogueDocument>,
    current: Option<String>,
    views: HashMap<String, GraphView>,
    followed: Option<InstanceId>,
    problem: Option<String>,
    hovered: bool,
}

/// Checks `graph` as Play would run it.
pub fn check(graph: &NodeGraph) -> Result<(), Vec<DialogueError>> {
    let text = serde_json::to_string(graph).expect("a graph is plain data");
    Dialogue::from_json(&text).map(|_| ())
}

impl DialogueStudio {
    /// How far each graph drawn so far is zoomed.
    #[cfg(test)]
    pub fn view_zooms(&self) -> Vec<f32> {
        self.views.values().map(|view| view.zoom).collect()
    }

    pub fn unsaved(&self) -> bool {
        self.documents.iter().any(DialogueDocument::unsaved)
    }

    pub fn has_pointer(&self) -> bool {
        self.hovered
    }

    #[cfg(test)]
    pub fn current(&self) -> Option<&str> {
        self.current.as_deref()
    }

    pub fn document(&self, path: &str) -> Option<&DialogueDocument> {
        self.documents.iter().find(|doc| doc.path == path)
    }

    pub fn document_mut(&mut self, path: &str) -> Option<&mut DialogueDocument> {
        self.documents.iter_mut().find(|doc| doc.path == path)
    }

    fn new_paths(&self) -> impl Iterator<Item = &str> {
        self.documents
            .iter()
            .filter(|doc| doc.saved.is_none())
            .map(|doc| doc.path.as_str())
    }

    /// Shows the dialogue at `path` in `folder`, reading it the first time.
    pub fn open(&mut self, folder: &Path, path: &str) -> Result<(), String> {
        if self.document(path).is_none() {
            let graph = axcenity_project::read_ui_graph(folder, path)
                .map_err(|err| format!("{:#}", anyhow::Error::new(err)))?;
            self.documents.push(DialogueDocument {
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

    /// Starts a dialogue at `path` (Start, a Line and an End) and shows it.
    /// It's written at the next save.
    pub fn create(&mut self, path: &str) {
        if self.document(path).is_none() {
            self.documents.push(DialogueDocument {
                path: path.to_owned(),
                graph: new_dialogue(),
                saved: None,
                undo: Vec::new(),
                redo: Vec::new(),
            });
        }
        self.current = Some(path.to_owned());
        self.problem = None;
    }

    /// Follows the selected prompt: when the selection moves to a prompt
    /// with a dialogue, that dialogue is shown.
    pub fn follow(&mut self, folder: Option<&Path>, prompt: Option<&SelectedPrompt>) {
        let id = prompt.map(|prompt| prompt.id);
        if id == self.followed {
            return;
        }
        self.followed = id;
        let (Some(folder), Some(prompt)) = (folder, prompt) else {
            return;
        };
        let path = prompt.dialogue.trim();
        if path.is_empty() || self.current.as_deref() == Some(path) {
            return;
        }
        if !is_dialogue(path) {
            self.problem = Some(format!(
                "{}'s Dialogue, `{path}`, isn't a dialogue, so its prompt starts none.",
                prompt.name
            ));
            return;
        }
        if let Err(why) = self.open(folder, path) {
            self.problem = Some(format!(
                "{}'s dialogue `{path}` can't be read: {why}",
                prompt.name
            ));
        }
    }

    /// Writes every dialogue with changes into `folder`.
    pub fn save(&mut self, folder: &Path) -> Result<Vec<String>, String> {
        let mut written = Vec::new();
        for doc in &mut self.documents {
            if !doc.unsaved() {
                continue;
            }
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

    /// The tab: which dialogue, the prompt's button, whether it's ready,
    /// and the graph.
    pub fn show(&mut self, ui: &mut egui::Ui, scene: &DialogueScene) -> Vec<DialogueAction> {
        let mut actions = Vec::new();
        let palette = theme::palette(ui.ctx());
        self.hovered = false;
        responsive::toolbar(ui, |ui| {
            let shown = self
                .current
                .as_deref()
                .map_or("No dialogue", dialogue_name)
                .to_owned();
            let mut dialogues: Vec<&str> = scene.files.iter().map(String::as_str).collect();
            dialogues.extend(self.new_paths());
            dialogues.sort_unstable();
            dialogues.dedup();
            egui::ComboBox::from_id_salt("dialogue picker")
                .selected_text(theme::with_icon(icons::CHAT_CIRCLE_TEXT, &shown))
                .show_ui(ui, |ui| {
                    if dialogues.is_empty() {
                        ui.weak("The project has no dialogues yet.");
                    }
                    for path in dialogues {
                        let current = self.current.as_deref() == Some(path);
                        if ui
                            .selectable_label(current, dialogue_name(path))
                            .on_hover_text(path)
                            .clicked()
                            && !current
                        {
                            actions.push(DialogueAction::Show(path.to_owned()));
                        }
                    }
                });
            let new = ui
                .add_enabled(
                    scene.editable,
                    egui::Button::new(theme::with_icon(icons::PLUS, "New dialogue")),
                )
                .on_hover_text(match scene.prompt {
                    Some(prompt) => format!(
                        "Makes a dialogue in assets/dialogue, and has {}'s prompt start it",
                        prompt.name
                    ),
                    None => "Makes a dialogue in assets/dialogue".to_owned(),
                });
            if new.clicked() {
                actions.push(DialogueAction::New);
            }
            if let (Some(prompt), Some(current)) = (scene.prompt, &self.current) {
                if prompt.dialogue == *current {
                    ui.weak(format!("{}'s prompt starts it", prompt.name));
                } else if ui
                    .add_enabled(
                        scene.editable,
                        egui::Button::new(format!("Use on {}'s prompt", prompt.name)),
                    )
                    .on_hover_text("Sets the ProximityPrompt's Dialogue to this dialogue")
                    .clicked()
                {
                    actions.push(DialogueAction::Use {
                        prompt: prompt.id,
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
                "A dialogue is lines, choices, conditions and actions wired from a Start. Give \
                 a ProximityPrompt one in its Dialogue property and it runs when the prompt is \
                 triggered, with no script; or call DialogueService:Start.",
            );
            ui.weak("Select a ProximityPrompt and press New dialogue.");
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
                    "Ready: it runs in Play",
                ))
                .color(palette.success),
            );
        } else {
            ui.label(
                egui::RichText::new(theme::with_icon(
                    icons::WARNING,
                    &format!(
                        "{} {}: it won't run until {} fixed.",
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
                library: &DialogueNodes,
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
    use axcenity_blocks::dialogue::DIALOGUE_NODES;

    #[test]
    fn every_node_the_editor_offers_is_one_the_game_runs_with_the_same_pins_and_fields() {
        let offered: Vec<_> = DIALOGUE_NODES
            .iter()
            .map(|spec| {
                let outputs: Vec<&str> = spec.outputs.iter().map(|pin| pin.name).collect();
                let fields: Vec<&str> = spec.fields.iter().map(|field| field.name).collect();
                (spec.kind, outputs, fields)
            })
            .collect();
        let run: Vec<_> = axcenity_gameplay::dialogue::DIALOGUE_NODE_KINDS
            .iter()
            .map(|(kind, outputs, fields)| (*kind, outputs.to_vec(), fields.to_vec()))
            .collect();
        assert_eq!(offered, run);
        for spec in DIALOGUE_NODES {
            let inputs: Vec<&str> = spec.inputs.iter().map(|pin| pin.name).collect();
            let expected: &[&str] = if spec.kind == "start" { &[] } else { &[""] };
            assert_eq!(inputs, expected, "{}", spec.kind);
        }
        assert_eq!(
            axcenity_gameplay::dialogue::DIALOGUE_SUFFIX,
            axcenity_blocks::dialogue::DIALOGUE_SUFFIX
        );
    }

    #[test]
    fn a_new_dialogue_is_ready_to_run() {
        assert_eq!(
            super::check(&axcenity_blocks::dialogue::new_dialogue()),
            Ok(())
        );
    }
}
