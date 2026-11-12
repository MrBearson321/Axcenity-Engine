//! The one list of what's modified and not saved (cohesion pass CW3.1).
//!
//! Every editor that holds a document reports it here: the world's
//! history, script tabs, block scripts, UI Event Graphs, behavior trees,
//! dialogues, data tables, materials, the Animation tab's clip and state
//! machine, and the Model Studio's model. "Is anything unsaved?" (the
//! title's `*`, the close prompts), the prompt's list and "Don't Save" all
//! read it, so a new editor is added in one place.
//!
//! Some documents are written as they're edited and recorded as undo steps
//! (animation clips when another is opened, sequences when the mouse is
//! let go). "Don't Save" puts those files back as they were at the last
//! save ([`EditorApp::discard_unsaved`]); files created, renamed or deleted
//! (imports, prefabs, Animate's new clip) are asset operations and stay.

use super::*;

/// What kind of document is modified, for its icon in "Save changes?".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DocKind {
    /// The world and its levels (`world.json`, a level file).
    World,
    Script,
    BlockScript,
    UiGraph,
    BehaviorTree,
    Dialogue,
    DataTable,
    Material,
    /// An animation clip or state machine open in the Animation tab.
    Animation,
    /// The Model Studio's model.
    Model,
}

impl DocKind {
    pub(crate) fn icon(self) -> &'static str {
        match self {
            DocKind::World => icons::GLOBE_HEMISPHERE_WEST,
            DocKind::Script => icons::FILE_CODE,
            DocKind::BlockScript => icons::PUZZLE_PIECE,
            DocKind::UiGraph | DocKind::BehaviorTree | DocKind::Dialogue => icons::GRAPH,
            DocKind::DataTable => icons::TABLE,
            DocKind::Material => icons::PALETTE,
            DocKind::Animation => icons::PERSON_SIMPLE_RUN,
            DocKind::Model => icons::CUBE,
        }
    }
}

/// One modified document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Modified {
    pub kind: DocKind,
    /// Its project path, or "The world".
    pub name: String,
}

/// Files written as they're edited, which "Don't Save" puts back.
pub(crate) fn written_as_edited(path: &str) -> bool {
    path.ends_with(axcenity_anim::files::CLIP_EXTENSION)
        || path.ends_with(axcenity_anim::files::STATE_MACHINE_EXTENSION)
        || path.ends_with(axcenity_anim::sequence::SEQUENCE_EXTENSION)
}

impl EditorApp {
    /// Everything modified and not saved, the world first.
    pub(crate) fn modified_documents(&self) -> Vec<Modified> {
        let mut list = Vec::new();
        let mut add = |kind, name: String| {
            if !list.iter().any(|item: &Modified| item.name == name) {
                list.push(Modified { kind, name });
            }
        };
        if !self.history.is_saved() {
            add(DocKind::World, "The world".to_owned());
        }
        for path in self.tabs.unsaved_paths() {
            add(DocKind::Script, path);
        }
        if let Some(project) = &self.project {
            for document in &project.block_scripts {
                if document.script != document.saved {
                    add(DocKind::BlockScript, document.path.clone());
                }
            }
        }
        for path in self.unsaved_ui_graphs() {
            add(DocKind::UiGraph, path);
        }
        // A graph whose ScreenGui is gone still counts, without a path.
        if self.ui_graphs_unsaved() && self.unsaved_ui_graphs().is_empty() {
            add(DocKind::UiGraph, "A UI Event Graph".to_owned());
        }
        for path in self.behavior.unsaved_paths() {
            add(DocKind::BehaviorTree, path);
        }
        for path in self.dialogues.unsaved_paths() {
            add(DocKind::Dialogue, path);
        }
        for path in self.data_tables.unsaved_paths() {
            add(DocKind::DataTable, path);
        }
        for path in self.materials.unsaved_paths() {
            add(DocKind::Material, path);
        }
        for path in self.animation.unwritten_paths() {
            add(DocKind::Animation, path);
        }
        if let Some(doc) = &self.model_studio.doc
            && doc.unsaved()
        {
            let name = if doc.path.is_empty() {
                "A new model in the Model Studio".to_owned()
            } else {
                doc.path.clone()
            };
            add(DocKind::Model, name);
        }
        list
    }

    /// "Don't Save": puts back the files written as they were edited since
    /// the last save, and forgets the editors' unwritten changes. The world
    /// itself is dropped by whatever comes next (closing, opening another).
    /// In a Collaborate session the files are everyone's, so they're left.
    pub(crate) fn discard_unsaved(&mut self) {
        if self.team.is_some() {
            return;
        }
        let Some(folder) = self.project.as_ref().map(|project| project.folder.clone()) else {
            return;
        };
        for file in self.history.rewritten_since_saved(written_as_edited) {
            let (Some(now), Some(saved)) = (&file.before, &file.after) else {
                continue;
            };
            let Some(disk) = axcenity_assets::safe_path(&file.path).map(|path| folder.join(path))
            else {
                continue;
            };
            // Changed outside the editor since: theirs, so it's left alone.
            if std::fs::read_to_string(&disk).ok().as_deref() != Some(now.as_str()) {
                self.output.warning(format!(
                    "`{}` changed outside the editor, so it was kept as it is.",
                    file.path
                ));
                continue;
            }
            match std::fs::write(&disk, saved) {
                Ok(()) => self
                    .output
                    .info(format!("Put `{}` back as it was saved.", file.path)),
                Err(err) => self
                    .output
                    .error(format!("Could not put `{}` back: {err}", file.path)),
            }
        }
        self.animation.forget_unwritten();
        if let Some(doc) = &mut self.model_studio.doc
            && doc.unsaved()
        {
            self.model_studio.doc = None;
        }
    }
}
