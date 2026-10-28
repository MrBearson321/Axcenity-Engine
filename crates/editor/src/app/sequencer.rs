//! The Sequencer tab's actions (Phase 51; the tab is `sequencer.rs`):
//! opening and making sequence files, writing each edit as one undo step,
//! putting a Sequence that plays the open file in the Workspace, and the
//! Scene view's preview.

use axcenity_anim::sequence::SequenceFile;

use super::*;
use crate::asset_browser::AssetKind;
use crate::sequencer::{Picked, SequencerAction, SequencerScene, sequence_name, sequence_path};

impl EditorApp {
    pub(in crate::app) fn sequencer_pane(&mut self, ui: &mut egui::Ui) {
        let folder = self.project.as_ref().map(|project| project.folder.clone());
        let entries = self.asset_browser.entries();
        let of_kind = |kind| -> Vec<String> {
            let mut paths: Vec<String> = entries
                .iter()
                .filter(|entry| entry.kind == kind)
                .map(|entry| entry.path.clone())
                .collect();
            paths.sort();
            paths
        };
        let files = of_kind(AssetKind::Sequence);
        let clips = of_kind(AssetKind::Animation);
        let levels = folder
            .as_deref()
            .map(axcenity_project::level_names)
            .unwrap_or_default();
        let picked = self.selected.and_then(|id| {
            let instance = self.world.get(id)?;
            Some(Picked {
                id,
                path: axcenity_anim::sequence::binding_path(&self.world, id)?,
                tags: instance.tags().to_vec(),
                name: instance.name().to_owned(),
            })
        });
        let (position, orientation) = self.viewport.camera_view();
        let scene = SequencerScene {
            files: &files,
            clips: &clips,
            levels: &levels,
            picked,
            world: &self.world,
            camera: (position, orientation),
            editable: folder.is_some() && !self.playing(),
        };
        let actions = self.sequencer.show(ui, &scene);
        for action in actions {
            match action {
                SequencerAction::Open(path) => self.open_sequence(&path),
                SequencerAction::New(name) => self.new_sequence(&name),
                SequencerAction::Insert(path) => self.insert_sequence(&path),
            }
        }
        self.flush_sequence(ui.ctx());
    }

    /// Writes the open sequence once the mouse is let go, as one undo step.
    pub(in crate::app) fn flush_sequence(&mut self, ctx: &egui::Context) {
        for error in std::mem::take(&mut self.sequencer.errors) {
            self.output.error(error);
        }
        let Some(folder) = self.project.as_ref().map(|project| project.folder.clone()) else {
            return;
        };
        match self.sequencer.flush(ctx, &folder) {
            Some(Ok((path, before, after))) => {
                let label = format!("Edit {}", sequence_name(&path));
                self.history.record_files(
                    &mut self.world,
                    label,
                    vec![crate::history::TextChange {
                        path,
                        before: Some(before),
                        after: Some(after),
                    }],
                );
            }
            Some(Err(err)) => self
                .output
                .error(format!("Could not save the sequence: {err}")),
            None => {}
        }
    }

    /// Opens the sequence at `path` in the Sequencer tab.
    pub(in crate::app) fn open_sequence(&mut self, path: &str) {
        let Some(folder) = self.project.as_ref().map(|project| project.folder.clone()) else {
            return;
        };
        let read = std::fs::read_to_string(folder.join(path))
            .map_err(|err| err.to_string())
            .and_then(|text| SequenceFile::from_json(&text).map_err(|err| err.0));
        match read {
            Ok(file) => {
                self.sequencer.open(path.to_owned(), file);
                self.layout.open(Pane::Sequencer);
            }
            Err(why) => self.output.error(format!("Could not open `{path}`: {why}")),
        }
    }

    /// Makes the sequence `name` (`assets/sequences/<name>.sequence.json`)
    /// as one undo step, and opens it.
    pub(in crate::app) fn new_sequence(&mut self, name: &str) {
        let Some(folder) = self.project.as_ref().map(|project| project.folder.clone()) else {
            return;
        };
        let name = axcenity_anim::files::file_safe(name);
        let path = sequence_path(&name);
        if folder.join(&path).exists() {
            self.output
                .error(format!("There's already a sequence named `{name}`."));
            return;
        }
        let file = SequenceFile::new(crate::sequencer::NEW_DURATION);
        let text = file.to_json();
        if let Err(err) = axcenity_anim::files::write_file(&folder.join(&path), &text) {
            self.output.error(format!("Could not make `{path}`: {err}"));
            return;
        }
        self.record_asset_files(
            format!("Create {name}"),
            vec![crate::animation::store::FileDiff {
                path: path.clone(),
                before: None,
                after: Some(text),
            }],
            None,
        );
        self.sequencer.open(path.clone(), file);
        self.layout.open(Pane::Sequencer);
        self.output.info(format!(
            "Made the sequence `{path}`. Add camera cuts, sections and keys, then Insert puts \
             a Sequence that plays it in the Workspace."
        ));
    }

    /// Puts a Sequence named after the file, playing it, in the Workspace.
    pub(in crate::app) fn insert_sequence(&mut self, path: &str) {
        if self.playing() {
            return;
        }
        let id = self
            .world
            .create(ClassName::Sequence)
            .expect("not a service");
        let _ = self.world.set_name(id, sequence_name(path));
        self.world
            .set_property(id, "SequenceId", Value::String(path.to_owned()))
            .expect("every Sequence has a SequenceId");
        let workspace = self.world.workspace();
        self.place_new(workspace, vec![id], "Insert Sequence".to_owned());
    }

    /// After an undo or redo changed the open sequence's file: shows it as
    /// it is now (or closes it, if it's gone).
    pub(in crate::app) fn sync_sequence(&mut self, files: &[crate::animation::store::FileDiff]) {
        let Some(path) = self.sequencer.doc.as_ref().map(|doc| doc.path.clone()) else {
            return;
        };
        let Some(file) = files.iter().find(|file| file.path == path) else {
            return;
        };
        match file.after.as_deref().map(SequenceFile::from_json) {
            Some(Ok(sequence)) => {
                if let Some(doc) = &mut self.sequencer.doc {
                    doc.show_file(sequence);
                }
            }
            Some(Err(_)) | None => self.sequencer.doc = None,
        }
    }

    /// Brings the Sequencer's preview up to date for the Scene view, and
    /// whether it shows: while it's on and the tab is open outside Play.
    pub(in crate::app) fn update_sequence_preview(&mut self) -> bool {
        if self.playing() || !self.layout.is_open(Pane::Sequencer) {
            self.sequencer.preview_on &= self.layout.is_open(Pane::Sequencer);
            return false;
        }
        let revision = self.history.revision();
        self.sequencer
            .preview(&self.world, &self.assets, revision)
            .is_some()
    }
}
