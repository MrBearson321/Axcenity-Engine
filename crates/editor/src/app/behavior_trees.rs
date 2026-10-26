//! Behavior trees in the editor: the Behavior Tree tab (`behavior_tree.rs`)
//! and what it asks for, saving trees with the project, and giving an
//! agent a tree. These are methods of [`super::EditorApp`].

use axcenity_blocks::behavior_tree::{BEHAVIOR_TREE_SUFFIX, behavior_tree_path, is_behavior_tree};

use super::*;
use crate::behavior_tree::{SelectedAgent, TreeAction, TreeScene};

/// The NavigationAgent `id` is, or the one in it (a Model or part), or
/// beside it (a part of an agent's Model).
pub(super) fn agent_for(world: &World, id: InstanceId) -> Option<InstanceId> {
    let is_agent = |id: InstanceId| {
        world
            .get(id)
            .is_some_and(|instance| instance.class() == ClassName::NavigationAgent)
    };
    if is_agent(id) {
        return Some(id);
    }
    let parent = world.get(id)?.parent();
    [Some(id), parent].into_iter().flatten().find_map(|holder| {
        world
            .get(holder)?
            .children()
            .iter()
            .copied()
            .find(|&child| is_agent(child))
    })
}

/// Every behavior tree file in the project's `assets` folder, sorted.
pub(super) fn tree_files(folder: &Path) -> Vec<String> {
    fn walk(folder: &Path, dir: &Path, found: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(folder, &path, found);
            } else if let Ok(relative) = path.strip_prefix(folder) {
                let relative = relative
                    .components()
                    .map(|part| part.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/");
                if is_behavior_tree(&relative) {
                    found.push(relative);
                }
            }
        }
    }
    let mut found = Vec::new();
    walk(folder, &folder.join("assets"), &mut found);
    found.sort();
    found
}

impl EditorApp {
    /// The selected NavigationAgent (or the one in the selected Model), as
    /// the Behavior Tree tab sees it.
    pub(super) fn selected_agent(&self) -> Option<SelectedAgent> {
        let world = shown(&self.world, &self.play);
        let id = agent_for(world, self.selected?)?;
        let instance = world.get(id)?;
        let tree = instance
            .property("BehaviorTree")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        // An agent is named by what it moves, usually its Model.
        let name = instance
            .parent()
            .and_then(|parent| world.get(parent))
            .map_or_else(
                || instance.name().to_owned(),
                |parent| parent.name().to_owned(),
            );
        Some(SelectedAgent { id, name, tree })
    }

    /// The Behavior Tree tab, then what was asked for in it.
    pub(in crate::app) fn behavior_tree_pane(&mut self, ui: &mut egui::Ui) {
        let agent = self.selected_agent();
        let folder = self.project.as_ref().map(|project| project.folder.clone());
        self.behavior.follow(folder.as_deref(), agent.as_ref());
        let files = folder.as_deref().map(tree_files).unwrap_or_default();
        let scene = TreeScene {
            files: &files,
            agent: agent.as_ref(),
            editable: folder.is_some() && !self.playing(),
        };
        let actions = self.behavior.show(ui, &scene);
        for action in actions {
            self.behavior_tree_action(action, agent.as_ref());
        }
    }

    pub(super) fn behavior_tree_action(
        &mut self,
        action: TreeAction,
        agent: Option<&SelectedAgent>,
    ) {
        match action {
            TreeAction::Show(path) => self.open_behavior_tree(&path),
            TreeAction::New => self.new_behavior_tree(agent),
            TreeAction::Use { agent, path } => self.use_behavior_tree(agent, &path),
        }
    }

    /// Starts a tree named after the agent's Model (or "Behavior"), with a
    /// number when that's taken, and has the agent run it if it runs none.
    pub(super) fn new_behavior_tree(&mut self, agent: Option<&SelectedAgent>) {
        let Some(folder) = self.project.as_ref().map(|project| project.folder.clone()) else {
            return;
        };
        if self.playing() {
            return;
        }
        let base: String = agent
            .map_or("Behavior", |agent| agent.name.as_str())
            .chars()
            .filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_'))
            .collect();
        let base = if base.trim().is_empty() {
            "Behavior".to_owned()
        } else {
            base.trim().to_owned()
        };
        let taken =
            |path: &str| folder.join(path).exists() || self.behavior.document(path).is_some();
        let mut path = behavior_tree_path(&base);
        let mut number = 2;
        while taken(&path) {
            path = behavior_tree_path(&format!("{base}{number}"));
            number += 1;
        }
        self.behavior.create(&path);
        self.layout.open(Pane::BehaviorTree);
        self.output.info(format!(
            "Made the behavior tree `{path}`: wire nodes to its Root. It's written when the \
             project is saved."
        ));
        if let Some(agent) = agent.filter(|agent| agent.tree.trim().is_empty()) {
            self.use_behavior_tree(agent.id, &path);
        }
    }

    /// Has `agent` run the tree at `path`, as one undo step.
    pub(super) fn use_behavior_tree(&mut self, agent: InstanceId, path: &str) {
        if self.playing() {
            return;
        }
        let Some(name) = self.world.get(agent).map(|a| a.name().to_owned()) else {
            return;
        };
        self.change(
            agent,
            Field::Property("BehaviorTree"),
            format!("Set BehaviorTree of {name}"),
            vec![Edit::Property(
                "BehaviorTree",
                Value::String(path.to_owned()),
            )],
        );
        self.history.seal();
    }

    /// Shows the tree at `path` in the Behavior Tree tab.
    pub(super) fn open_behavior_tree(&mut self, path: &str) {
        let Some(folder) = self.project.as_ref().map(|project| project.folder.clone()) else {
            return;
        };
        if !path.to_lowercase().ends_with(BEHAVIOR_TREE_SUFFIX) {
            self.output.error(format!(
                "`{path}` isn't a behavior tree (a {BEHAVIOR_TREE_SUFFIX} file)."
            ));
            return;
        }
        match self.behavior.open(&folder, path) {
            Ok(()) => self.layout.open(Pane::BehaviorTree),
            Err(why) => self.output.error(format!("Could not open `{path}`: {why}")),
        }
    }

    /// A tree from the Assets panel: dropped on an agent's body, that agent
    /// runs it; otherwise (or double-clicked) it opens in the tab.
    pub(super) fn drop_behavior_tree(&mut self, path: &str, part: Option<InstanceId>) {
        match part.and_then(|part| agent_for(&self.world, part)) {
            Some(agent) => {
                self.use_behavior_tree(agent, path);
                self.select_from_viewport(Some(agent));
            }
            None => self.open_behavior_tree(path),
        }
    }

    /// Writes every tree with changes. Adds what it wrote to `saved`. False
    /// when one couldn't be written.
    pub(super) fn save_behavior_trees(&mut self, saved: &mut Vec<String>) -> bool {
        let Some(folder) = self.project.as_ref().map(|project| project.folder.clone()) else {
            return true;
        };
        match self.behavior.save(&folder) {
            Ok(written) => {
                for path in written {
                    if let Some(doc) = self.behavior.document(&path)
                        && let Err(errors) = crate::behavior_tree::check(&doc.graph)
                    {
                        let problems: Vec<String> =
                            errors.iter().map(ToString::to_string).collect();
                        self.output.warning(format!(
                            "Saved `{path}`, but agents that use it run no tree until its \
                             problems are fixed (they're shown on its nodes): {}",
                            problems.join("; ")
                        ));
                    }
                    saved.push(path);
                }
                true
            }
            Err(why) => {
                self.output.error(why);
                false
            }
        }
    }
}
