//! The Levels panel's actions (Phase 48; the panel is `levels.rs`). New,
//! Duplicate, Rename, Delete and Set as Start change level files as one
//! undo step each (`History::record_asset`). Opening a level asks about
//! unsaved changes first, then opens the project again in that level.

use axcenity_project as project;

use crate::levels::{LevelAction, LevelList};

use super::*;

impl EditorApp {
    pub(in crate::app) fn levels_pane(&mut self, ui: &mut egui::Ui) {
        let Some(project) = &self.project else {
            return;
        };
        let others = project::level_names(&project.folder);
        let list = LevelList {
            start: &project.start_level,
            others: &others,
            open: project.level.as_deref(),
        };
        let playing = self.play.is_some();
        // `project` borrows `self.project`; the panel and the box are other
        // fields, so they can be borrowed beside it.
        let action = ui
            .add_enabled_ui(!playing, |ui| {
                self.levels_panel.show(ui, &list, &mut self.play_from_start)
            })
            .inner;
        self.level_action(ui.ctx(), action);
    }

    /// Does what the Levels panel asked.
    pub(in crate::app) fn level_action(&mut self, ctx: &egui::Context, action: LevelAction) {
        match action {
            LevelAction::None => {}
            LevelAction::Open(level) => self.request_and_go_ahead(ctx, Discard::OpenLevel(level)),
            LevelAction::New => {
                let name = self.free_level_name("Level");
                self.change_level_files(
                    format!("New level {name}"),
                    vec![(project::level_path(&name), Some(project::new_level_text()))],
                );
                self.levels_panel.start_renaming(&name);
            }
            LevelAction::Duplicate(source) => self.duplicate_level(&source),
            LevelAction::Rename(old, new) => self.rename_level(&old, &new),
            LevelAction::Delete(name) => {
                let Some(project) = &self.project else {
                    return;
                };
                if project.level.as_deref() == Some(name.as_str()) {
                    return;
                }
                self.change_level_files(
                    format!("Delete level {name}"),
                    vec![(project::level_path(&name), None)],
                );
            }
            LevelAction::SetStart(name) => self.set_start_level(&name),
        }
    }

    /// `base`, or `base 2`, `base 3`… : the first name no level has.
    fn free_level_name(&self, base: &str) -> String {
        let Some(project) = &self.project else {
            return base.to_owned();
        };
        let taken = project::level_names(&project.folder);
        (1..)
            .map(|n| match n {
                1 => base.to_owned(),
                n => format!("{base} {n}"),
            })
            .find(|name| project::check_level_name(name, &project.start_level, &taken).is_ok())
            .expect("some number is free")
    }

    /// The saved text of the level `name`: the start level's Workspace from
    /// `world.json`, or a level's file.
    fn level_text(&self, name: &str) -> Option<String> {
        let project = self.project.as_ref()?;
        if name == project.start_level {
            let text = std::fs::read_to_string(project.folder.join(project::WORLD_FILE)).ok()?;
            let world = project::parse_world(&text).ok()?;
            return Some(project::write_level(&world));
        }
        std::fs::read_to_string(project.folder.join(project::level_path(name))).ok()
    }

    /// Duplicate: a new level with what `source` holds as saved.
    fn duplicate_level(&mut self, source: &str) {
        let Some(text) = self.level_text(source) else {
            self.output.error(format!(
                "Could not read the level \"{source}\" to duplicate it."
            ));
            return;
        };
        let name = self.free_level_name(&format!("{source} copy"));
        self.change_level_files(
            format!("Duplicate level {source}"),
            vec![(project::level_path(&name), Some(text))],
        );
        self.levels_panel.start_renaming(&name);
    }

    fn rename_level(&mut self, old: &str, new: &str) {
        let Some(project) = &self.project else {
            return;
        };
        let others: Vec<String> = project::level_names(&project.folder)
            .into_iter()
            .filter(|name| name != old)
            .collect();
        let start = if old == project.start_level {
            ""
        } else {
            project.start_level.as_str()
        };
        if let Err(why) = project::check_level_name(new, start, &others) {
            self.output.error(format!(
                "Can't rename the level \"{old}\" to \"{new}\": {why}."
            ));
            return;
        }
        if old == project.start_level {
            // The start level stays in world.json: only its name changes.
            let Some(manifest) = manifest_with_start(&project.folder, new) else {
                self.output
                    .error("Could not read the project manifest to rename the start level.");
                return;
            };
            self.change_level_files(
                format!("Rename level {old} to {new}"),
                vec![(project::MANIFEST_FILE.to_owned(), Some(manifest))],
            );
            if let Some(project) = &mut self.project {
                project.start_level = new.to_owned();
            }
            return;
        }
        if project.level.as_deref() == Some(old) {
            return;
        }
        let Some(text) = self.level_text(old) else {
            return;
        };
        self.change_level_files(
            format!("Rename level {old} to {new}"),
            vec![
                (project::level_path(old), None),
                (project::level_path(new), Some(text)),
            ],
        );
    }

    /// Set as Start: `name`'s Workspace moves into world.json and the old
    /// start level's into `levels/`. The project is saved first, so the
    /// files hold what's open.
    fn set_start_level(&mut self, name: &str) {
        if self.unsaved() && !self.save() {
            self.output.error(format!(
                "\"{name}\" didn't become the start level, because the project couldn't be saved first."
            ));
            return;
        }
        let Some(project) = &self.project else {
            return;
        };
        let changes = match project::set_start_level(&project.folder, &project.start_level, name) {
            Ok(changes) => changes,
            Err(err) => {
                self.output.error(format!(
                    "Could not make \"{name}\" the start level: {:#}",
                    anyhow::Error::new(err)
                ));
                return;
            }
        };
        let files = changes
            .into_iter()
            .map(|change| crate::animation::store::FileDiff {
                path: change.path,
                before: change.before,
                after: change.after,
            })
            .collect();
        self.write_and_record(format!("Set {name} as the start level"), files);
    }

    /// Writes `files` (each a path from the project folder and its new text,
    /// `None` to remove it) and records them as one undo step `label`.
    fn change_level_files(&mut self, label: String, files: Vec<(String, Option<String>)>) {
        let Some(project) = &self.project else {
            return;
        };
        let diffs = files
            .into_iter()
            .map(|(path, after)| crate::animation::store::FileDiff {
                before: std::fs::read_to_string(project.folder.join(&path)).ok(),
                path,
                after,
            })
            .collect();
        self.write_and_record(label, diffs);
    }

    fn write_and_record(&mut self, label: String, files: Vec<crate::animation::store::FileDiff>) {
        let Some(project) = &self.project else {
            return;
        };
        let folder = project.folder.clone();
        if let Err(err) = crate::animation::store::apply_file_diffs(&folder, &files) {
            self.output
                .error(format!("Could not change the levels: {err}"));
            return;
        }
        let touched_manifest = files.iter().any(|file| file.path == project::MANIFEST_FILE);
        self.record_asset_files(label, files, None);
        if touched_manifest {
            self.follow_start_level();
        }
    }

    /// After the manifest's `start_level` changed (Set as Start, a start
    /// level renamed, or either undone): which file the open level is in
    /// now. Called from undo and redo too.
    pub(in crate::app) fn follow_start_level(&mut self) {
        let Some(project) = &mut self.project else {
            return;
        };
        let Some(manifest) = project::read_manifest(&project.folder) else {
            return;
        };
        let open = project
            .level
            .clone()
            .unwrap_or_else(|| project.start_level.clone());
        let open_in_levels = project.folder.join(project::level_path(&open)).is_file();
        project.level =
            if (project.level.is_none() && !open_in_levels) || open == manifest.start_level {
                // Still in world.json (only the start level's name changed), or
                // it became the start level.
                None
            } else {
                Some(open)
            };
        project.start_level = manifest.start_level;
        project.world_text = files::read_world_text(&project.folder);
    }
}

/// The manifest's text with `start_level` set to `name`.
fn manifest_with_start(folder: &Path, name: &str) -> Option<String> {
    let text = std::fs::read_to_string(folder.join(project::MANIFEST_FILE)).ok()?;
    let mut json: serde_json::Value = serde_json::from_str(&text).ok()?;
    json.as_object_mut()?
        .insert("start_level".to_owned(), serde_json::Value::from(name));
    serde_json::to_string_pretty(&json).ok()
}
