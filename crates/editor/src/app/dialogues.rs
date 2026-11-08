//! Dialogues in the editor (Phase 77): the Dialogue tab
//! (`dialogue_editor.rs`) and what it asks for, saving dialogues with the
//! project, and giving a ProximityPrompt a dialogue. These are methods of
//! [`super::EditorApp`].

use axcenity_blocks::dialogue::{DIALOGUE_SUFFIX, dialogue_path, is_dialogue};

use super::*;
use crate::dialogue_editor::{DialogueAction, DialogueScene, SelectedPrompt};

/// The ProximityPrompt `id` is, or the first one in it or in its parts (a
/// Model or part).
pub(super) fn prompt_for(world: &World, id: InstanceId) -> Option<InstanceId> {
    let is_prompt = |id: InstanceId| {
        world
            .get(id)
            .is_some_and(|instance| instance.class() == ClassName::ProximityPrompt)
    };
    if is_prompt(id) {
        return Some(id);
    }
    let children = |id: InstanceId| {
        world
            .get(id)
            .map(|i| i.children().to_vec())
            .unwrap_or_default()
    };
    children(id).into_iter().find_map(|child| {
        if is_prompt(child) {
            Some(child)
        } else {
            children(child).into_iter().find(|&inner| is_prompt(inner))
        }
    })
}

/// Every dialogue file in the project's `assets` folder, sorted.
pub(super) fn dialogue_files(folder: &Path) -> Vec<String> {
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
                if is_dialogue(&relative) {
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
    /// The selected ProximityPrompt (or the one in the selected part or
    /// Model), as the Dialogue tab sees it.
    pub(super) fn selected_prompt(&self) -> Option<SelectedPrompt> {
        let world = shown(&self.world, &self.play);
        let id = prompt_for(world, self.selected?)?;
        let instance = world.get(id)?;
        let dialogue = instance
            .property("Dialogue")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let name = axcenity_script::dialogue::speaker_name(world, id);
        Some(SelectedPrompt { id, name, dialogue })
    }

    /// The Dialogue tab, then what was asked for in it.
    pub(in crate::app) fn dialogue_pane(&mut self, ui: &mut egui::Ui) {
        let prompt = self.selected_prompt();
        let folder = self.project.as_ref().map(|project| project.folder.clone());
        self.dialogues.follow(folder.as_deref(), prompt.as_ref());
        let files = folder.as_deref().map(dialogue_files).unwrap_or_default();
        let scene = DialogueScene {
            files: &files,
            prompt: prompt.as_ref(),
            editable: folder.is_some() && !self.playing(),
        };
        let actions = self.dialogues.show(ui, &scene);
        for action in actions {
            match action {
                DialogueAction::Show(path) => self.open_dialogue(&path),
                DialogueAction::New => self.new_dialogue(prompt.as_ref()),
                DialogueAction::Use { prompt, path } => self.use_dialogue(prompt, &path),
            }
        }
    }

    /// Starts a dialogue named after the prompt's speaker (or "Dialogue"),
    /// with a number when that's taken, and has the prompt start it if it
    /// starts none.
    pub(super) fn new_dialogue(&mut self, prompt: Option<&SelectedPrompt>) {
        let Some(folder) = self.project.as_ref().map(|project| project.folder.clone()) else {
            return;
        };
        if self.playing() {
            return;
        }
        let base: String = prompt
            .map_or("Dialogue", |prompt| prompt.name.as_str())
            .chars()
            .filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_'))
            .collect();
        let base = if base.trim().is_empty() {
            "Dialogue".to_owned()
        } else {
            base.trim().to_owned()
        };
        let taken =
            |path: &str| folder.join(path).exists() || self.dialogues.document(path).is_some();
        let mut path = dialogue_path(&base);
        let mut number = 2;
        while taken(&path) {
            path = dialogue_path(&format!("{base}{number}"));
            number += 1;
        }
        self.dialogues.create(&path);
        self.layout.open(Pane::Dialogue);
        self.output.info(format!(
            "Made the dialogue `{path}`. It's written when the project is saved."
        ));
        if let Some(prompt) = prompt.filter(|prompt| prompt.dialogue.trim().is_empty()) {
            self.use_dialogue(prompt.id, &path);
        }
    }

    /// Has `prompt` start the dialogue at `path`, as one undo step.
    pub(super) fn use_dialogue(&mut self, prompt: InstanceId, path: &str) {
        if self.playing() {
            return;
        }
        let Some(name) = self.world.get(prompt).map(|p| p.name().to_owned()) else {
            return;
        };
        self.change(
            prompt,
            Field::Property("Dialogue"),
            format!("Set Dialogue of {name}"),
            vec![Edit::Property("Dialogue", Value::String(path.to_owned()))],
        );
        self.history.seal();
    }

    /// Shows the dialogue at `path` in the Dialogue tab.
    pub(super) fn open_dialogue(&mut self, path: &str) {
        let Some(folder) = self.project.as_ref().map(|project| project.folder.clone()) else {
            return;
        };
        if !is_dialogue(path) {
            self.output.error(format!(
                "`{path}` isn't a dialogue (a {DIALOGUE_SUFFIX} file)."
            ));
            return;
        }
        match self.dialogues.open(&folder, path) {
            Ok(()) => self.layout.open(Pane::Dialogue),
            Err(why) => self.output.error(format!("Could not open `{path}`: {why}")),
        }
    }

    /// Writes every dialogue with changes. Adds what it wrote to `saved`.
    pub(super) fn save_dialogues(&mut self, saved: &mut Vec<String>) -> bool {
        let Some(folder) = self.project.as_ref().map(|project| project.folder.clone()) else {
            return true;
        };
        match self.dialogues.save(&folder) {
            Ok(written) => {
                for path in written {
                    if let Some(doc) = self.dialogues.document(&path)
                        && let Err(errors) = crate::dialogue_editor::check(&doc.graph)
                    {
                        let problems: Vec<String> =
                            errors.iter().map(ToString::to_string).collect();
                        self.output.warning(format!(
                            "Saved `{path}`, but it won't run until its problems are fixed \
                             (they're shown on its nodes): {}",
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
