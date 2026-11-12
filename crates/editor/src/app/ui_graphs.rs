//! UI Event Graphs in the editor: each ScreenGui's graph, edited in the
//! UI Designer's Event Graph tab and saved with the project as
//! `ui/<ScreenGui>.uigraph.json`. Saving makes the graph's LocalScript
//! (`EventGraph`, under the ScreenGui) again; it can't be edited, renamed
//! or deleted on its own. Graph edits have their own undo, used while the
//! pointer is over the graph. These are methods of [`super::EditorApp`].

use axcenity_blocks::node_graph::NodeGraph;
use axcenity_blocks::ui_graph::{is_ui_graph, ui_graph_gui_name, ui_graph_path};

use axcenity_project as project;

use super::files::chain;
use super::*;

/// One ScreenGui's Event Graph, as edited.
pub(super) struct UiGraphDocument {
    /// Its ScreenGui. The file is named after it when it's saved, so
    /// renaming the ScreenGui renames the file.
    pub(super) gui: InstanceId,
    /// Its file, as last read or written; `None` before it has one.
    pub(super) saved_path: Option<String>,
    pub(super) graph: NodeGraph,
    pub(super) saved: NodeGraph,
    /// The graph before each edit, newest last, and the ones undone.
    pub(super) undo: Vec<NodeGraph>,
    pub(super) redo: Vec<NodeGraph>,
}

impl UiGraphDocument {
    fn unsaved(&self) -> bool {
        self.graph != self.saved
    }
}

/// The documents for the graphs a project loaded with, each matched to
/// its ScreenGui in `world` by name. One whose ScreenGui isn't there is
/// left out (loading reported it).
pub(super) fn documents(world: &World, graphs: Vec<project::UiGraphFile>) -> Vec<UiGraphDocument> {
    graphs
        .into_iter()
        .filter_map(|file| {
            let gui = project::screen_gui_named(world, ui_graph_gui_name(&file.path))?;
            Some(UiGraphDocument {
                gui,
                saved_path: Some(file.path),
                saved: file.graph.clone(),
                graph: file.graph,
                undo: Vec::new(),
                redo: Vec::new(),
            })
        })
        .collect()
}

impl EditorApp {
    /// The index of `gui`'s graph document, made (empty, with no file)
    /// the first time it's asked for.
    pub(super) fn graph_document(&mut self, gui: InstanceId) -> Option<usize> {
        let project = self.project.as_mut()?;
        if let Some(index) = project.ui_graphs.iter().position(|doc| doc.gui == gui) {
            return Some(index);
        }
        let is_gui = self
            .world
            .get(gui)
            .is_some_and(|instance| instance.class() == ClassName::ScreenGui);
        if !is_gui {
            return None;
        }
        project.ui_graphs.push(UiGraphDocument {
            gui,
            saved_path: None,
            graph: NodeGraph::default(),
            saved: NodeGraph::default(),
            undo: Vec::new(),
            redo: Vec::new(),
        });
        Some(project.ui_graphs.len() - 1)
    }

    /// Whether a graph has changes to save.
    pub(super) fn ui_graphs_unsaved(&self) -> bool {
        self.project
            .as_ref()
            .is_some_and(|project| project.ui_graphs.iter().any(UiGraphDocument::unsaved))
    }

    /// The paths the graphs with changes will be saved at (their
    /// ScreenGui's name), for Collaborate's locks.
    pub(super) fn unsaved_ui_graphs(&self) -> Vec<String> {
        let Some(project) = &self.project else {
            return Vec::new();
        };
        project
            .ui_graphs
            .iter()
            .filter(|doc| doc.unsaved())
            .filter_map(|doc| self.world.get(doc.gui))
            .map(|gui| ui_graph_path(gui.name()))
            .collect()
    }

    /// The graph file `path` changed on disk (a teammate saved it, or
    /// another program did): a graph without unsaved changes reads it
    /// again, and its LocalScript runs the new one. The LocalScript a
    /// teammate's editor made for a new graph is used, not a second one.
    pub(in crate::app) fn ui_graph_file_changed(&mut self, path: &str) {
        let Some(folder) = self.project.as_ref().map(|p| p.folder.clone()) else {
            return;
        };
        let Some(gui) = project::screen_gui_named(&self.world, ui_graph_gui_name(path)) else {
            return;
        };
        // A deleted graph's file stays gone; its document keeps what it had.
        let Ok(graph) = project::read_ui_graph(&folder, path) else {
            return;
        };
        let Some(index) = self.graph_document(gui) else {
            return;
        };
        let world = &self.world;
        let Some(project) = &mut self.project else {
            return;
        };
        let doc = &mut project.ui_graphs[index];
        if doc.unsaved() || (doc.graph == graph && doc.saved_path.as_deref() == Some(path)) {
            return;
        }
        doc.saved = graph.clone();
        doc.graph = graph;
        doc.saved_path = Some(path.to_owned());
        doc.undo.clear();
        doc.redo.clear();
        if !project.scripts.iter().any(|script| script.path == path)
            && let Some(instance) = world.get(gui).and_then(|gui| {
                gui.children().iter().copied().find(|&id| {
                    world.get(id).is_some_and(|script| {
                        script.class() == ClassName::LocalScript
                            && script.name() == axcenity_blocks::ui_graph::UI_GRAPH_SCRIPT_NAME
                    }) && !project.scripts.iter().any(|s| s.instance == id)
                })
            })
        {
            project.scripts.push(ScriptFile {
                instance,
                path: path.to_owned(),
                source: String::new(),
            });
        }
        self.make_graph_script(index, None, path);
        self.output.info(format!("Reloaded `{path}`."));
    }

    /// An edit of the shown ScreenGui's graph finished: `before` is how it was.
    pub(super) fn graph_step(&mut self, before: NodeGraph) {
        let Some(gui) = self.ui_designer.current_gui() else {
            return;
        };
        if let Some(index) = self.graph_document(gui)
            && let Some(project) = &mut self.project
        {
            let doc = &mut project.ui_graphs[index];
            doc.undo.push(before);
            doc.redo.clear();
        }
    }

    /// Undoes the last edit of the shown graph. Returns false when there
    /// was none.
    pub(super) fn undo_graph(&mut self) -> bool {
        self.graph_history(true)
    }

    pub(super) fn redo_graph(&mut self) -> bool {
        self.graph_history(false)
    }

    fn graph_history(&mut self, undo: bool) -> bool {
        let Some(gui) = self.ui_designer.current_gui() else {
            return false;
        };
        let Some(index) = self.graph_document(gui) else {
            return false;
        };
        let Some(project) = &mut self.project else {
            return false;
        };
        let doc = &mut project.ui_graphs[index];
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

    /// Writes every graph with changes (under its ScreenGui's name now, so
    /// a renamed ScreenGui renames its file) and makes its LocalScript
    /// again. Adds what it wrote to `saved`. False when a file couldn't be
    /// written.
    pub(super) fn save_ui_graphs(&mut self, saved: &mut Vec<String>) -> bool {
        let Some(project) = &self.project else {
            return true;
        };
        let folder = project.folder.clone();
        for index in 0..project.ui_graphs.len() {
            let Some(project) = &self.project else {
                return true;
            };
            let doc = &project.ui_graphs[index];
            let Some(name) = self.world.get(doc.gui).map(|gui| gui.name().to_owned()) else {
                // Its ScreenGui was deleted: the file stays, unused.
                continue;
            };
            let path = ui_graph_path(&name);
            let renamed = doc.saved_path.as_ref().is_some_and(|old| *old != path);
            let empty_and_new = doc.saved_path.is_none() && doc.graph == NodeGraph::default();
            if (!doc.unsaved() && !renamed) || empty_and_new {
                continue;
            }
            if let Err(err) = project::write_ui_graph(&folder, &path, &doc.graph) {
                self.output
                    .error(format!("Could not save `{path}`: {}", chain(&err)));
                return false;
            }
            let old = doc.saved_path.clone();
            if let Some(old) = old.as_ref().filter(|_| renamed) {
                let _ = std::fs::remove_file(folder.join(old));
            }
            if let Some(project) = &mut self.project {
                let doc = &mut project.ui_graphs[index];
                doc.saved = doc.graph.clone();
                doc.saved_path = Some(path.clone());
            }
            self.make_graph_script(index, old.as_deref(), &path);
            saved.push(path);
        }
        true
    }

    /// Compiles graph `index` (saved at `path`, before that at `old`) and
    /// puts its Lua in its LocalScript, making that the first time. A
    /// graph with problems is saved but doesn't run, and Output says so.
    fn make_graph_script(&mut self, index: usize, old: Option<&str>, path: &str) {
        let Some(project) = &self.project else {
            return;
        };
        let doc = &project.ui_graphs[index];
        let gui = doc.gui;
        let compiled = project::compile_graph_for(&self.world, path, &doc.graph);
        let existing = project
            .scripts
            .iter()
            .position(|script| script.path == path || Some(script.path.as_str()) == old);
        match compiled {
            Ok(source) => {
                let instance = match existing.map(|at| project.scripts[at].instance) {
                    Some(instance) if self.world.contains(instance) => instance,
                    _ => project::add_graph_script(&mut self.world, gui),
                };
                let Some(project) = &mut self.project else {
                    return;
                };
                let file = ScriptFile {
                    instance,
                    path: path.to_owned(),
                    source,
                };
                match existing {
                    Some(at) => project.scripts[at] = file,
                    None => project.scripts.push(file),
                }
            }
            Err(errors) => {
                let problems: Vec<String> = errors.iter().map(ToString::to_string).collect();
                self.output.error(format!(
                    "Saved `{path}`, but its Event Graph won't run until its problems are \
                     fixed (they're shown on its nodes in the UI Designer): {}",
                    problems.join("; ")
                ));
            }
        }
    }

    /// The ScreenGui whose Event Graph made the LocalScript `id`, if it's one.
    pub(super) fn graph_script_gui(&self, id: InstanceId) -> Option<InstanceId> {
        let path = self.script_path(id)?;
        if !is_ui_graph(&path) {
            return None;
        }
        self.world.get(id)?.parent()
    }

    /// Shows the Event Graph of the ScreenGui named in the graph file `path`.
    pub(super) fn open_ui_graph(&mut self, path: &str) {
        let gui = project::screen_gui_named(&self.world, ui_graph_gui_name(path));
        self.ui_designer.show_event_graph(gui);
        self.layout.open(Pane::UiDesigner);
    }

    /// Says why a graph's LocalScript can't be renamed, deleted or
    /// duplicated by itself, and shows its graph.
    pub(super) fn explain_graph_script(&mut self, gui: InstanceId) {
        let name = self
            .world
            .get(gui)
            .map_or_else(String::new, |gui| gui.name().to_owned());
        self.output.info(format!(
            "EventGraph is made from {name}'s Event Graph, so it changes with it: edit the \
             graph in the UI Designer (it's open now). Renaming {name} renames the graph's file."
        ));
        self.ui_designer.show_event_graph(Some(gui));
        self.layout.open(Pane::UiDesigner);
    }
}
