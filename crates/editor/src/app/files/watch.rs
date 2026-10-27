use axcenity_assets::Reload;

use crate::app::OpenProject;

use super::*;

/// True for a changed file that's taken in during Play: an asset (but not
/// a stylesheet: the UI restyles with those at Stop), or a script the game
/// runs that's still there (it runs again in place). A new or deleted
/// script waits for Stop, since it adds or removes an Instance.
fn reloads_in_play(project: &OpenProject, path: &str) -> bool {
    if project::is_asset_path(path) {
        return !project::is_stylesheet(path);
    }
    project.scripts.iter().any(|script| script.path == path) && project.folder.join(path).exists()
}

impl EditorApp {
    /// Takes in the files other programs changed since the last frame.
    pub(in crate::app) fn check_disk(&mut self) {
        let playing = self.playing();
        let Some(project) = &mut self.project else {
            return;
        };
        let mut paths = project.watcher.changes();
        // During Play, assets and changed scripts reload in place, but the
        // world, new or deleted scripts and stylesheets wait until Stop: the
        // game runs on a copy of the edit world, and the two mustn't get
        // mixed up.
        if playing {
            let (now, later): (Vec<String>, Vec<String>) = paths
                .into_iter()
                .partition(|path| reloads_in_play(project, path));
            project.after_play.extend(later);
            paths = now;
        } else if !project.after_play.is_empty() {
            paths.append(&mut project.after_play);
            paths.sort();
            paths.dedup();
        }
        self.disk_changed(&paths);
    }

    /// Takes in changes to the files at `paths` (as the watcher names them).
    /// Files that are the same as the editor has them (its own saves) are
    /// left alone.
    pub(in crate::app) fn disk_changed(&mut self, paths: &[String]) {
        for path in paths {
            if path == project::WORLD_FILE {
                self.world_file_changed();
            } else if project::is_stylesheet(path) {
                self.stylesheet_file_changed(path);
            } else if project::is_asset_path(path) {
                self.asset_file_changed(path);
            } else {
                self.script_file_changed(path);
            }
        }
    }

    /// A file under `assets/` changed: what uses it (MeshParts, textures,
    /// materials, fonts, and during Play Animators and Sounds) takes the
    /// new version, without reopening the project or stopping the game. A
    /// file that can't be loaded now keeps its last good version, reported
    /// once (by `Assets` and the session's errors).
    pub(super) fn asset_file_changed(&mut self, path: &str) {
        let mut reloaded = self.assets.reload(path) == Reload::Reloaded;
        if let Some(session) = &mut self.play {
            reloaded |= session.asset_changed(path, &mut self.output);
        }
        if reloaded {
            self.output.info(format!("Reloaded `{path}`."));
        }
        // A file that failed to reload says so now, once.
        for error in self.assets.take_errors() {
            self.output.error(error);
        }
        self.asset_browser.file_changed(path);
        if axcenity_assets::is_data_table(path)
            && let Some(folder) = self.project.as_ref().map(|project| project.folder.clone())
        {
            self.data_tables.file_changed(&folder, path);
            self.hand_over_tables();
        }
    }

    /// A stylesheet changed on disk, such as one edited in VS Code: restyle
    /// the UI with it, unless its tab has unsaved changes (they're kept, and
    /// saving replaces the other version). A deleted one stops applying.
    pub(super) fn stylesheet_file_changed(&mut self, path: &str) {
        let Some(project) = &mut self.project else {
            return;
        };
        let source = std::fs::read_to_string(project.folder.join(path)).ok();
        if source.as_ref() == project.stylesheet_texts.get(path) {
            // The editor's own save, or touched without a change.
            return;
        }
        let Some(source) = source else {
            project.stylesheet_texts.remove(path);
            project.stylesheets.retain(|sheet| sheet.path() != path);
            self.output
                .info(format!("`{path}` was deleted outside the editor."));
            self.sync_tab(path, None);
            return;
        };
        project
            .stylesheet_texts
            .insert(path.to_owned(), source.clone());
        let unsaved = self.tabs.get(path).is_some_and(|tab| tab.unsaved());
        if !unsaved {
            self.refresh_stylesheet(path, &source);
            self.output
                .info(format!("`{path}` was changed outside the editor."));
        }
        self.sync_tab(path, Some(&source));
    }

    /// The world file changed: load it, unless the world has unsaved changes.
    pub(super) fn world_file_changed(&mut self) {
        let Some(project) = &mut self.project else {
            return;
        };
        let text = read_world_text(&project.folder);
        if text == project.world_text {
            return;
        }
        project.world_text = text.clone();
        let world_file = project::WORLD_FILE;
        if !self.history.is_saved() {
            project.keep_world_file = true;
            self.output.error(format!(
                "`{world_file}` was changed outside the editor, but the world has unsaved \
                 changes here, so they stay. Saving keeps the other version as `{}`.",
                project::KEPT_WORLD_FILE
            ));
            return;
        }
        let path = project.folder.join(world_file);
        let parsed = match text {
            Some(text) => {
                project::parse_world(&text).map_err(|source| LoadError::World { path, source })
            }
            None => Err(LoadError::Read {
                path,
                source: std::io::Error::new(ErrorKind::NotFound, "it's gone"),
            }),
        };
        match parsed {
            Ok(mut world) => {
                let mut problems = Vec::new();
                let damaged = project::load_terrains(&project.folder, &mut world, &mut problems);
                for problem in &problems {
                    self.output
                        .error(format!("Skipped a damaged file: {}", chain(problem)));
                }
                project.damaged_terrains = damaged;
                // The new world has no Scripts yet: they come from the files.
                for script in &mut project.scripts {
                    script.instance = project::add_script_instance(&mut world, &script.path);
                }
                for document in &mut project.block_scripts {
                    if let Some(script) = project
                        .scripts
                        .iter()
                        .find(|script| script.path == document.path)
                    {
                        document.instance = script.instance;
                    }
                }
                project.keep_world_file = false;
                // The string table is its own file, so it stays.
                world.set_strings(self.world.strings().clone());
                self.replace_world(world);
                self.output.info(format!(
                    "Reloaded `{world_file}`, which was changed outside the editor."
                ));
            }
            Err(err) => {
                project.keep_world_file = true;
                self.output.error(format!(
                    "`{world_file}` was changed outside the editor, but can't be loaded: {}. \
                     The world stays as it is here; saving keeps that file as `{}`.",
                    chain(&err),
                    project::KEPT_WORLD_FILE
                ));
            }
        }
    }

    /// The script file at `path` was made, changed or deleted.
    pub(super) fn script_file_changed(&mut self, path: &str) {
        if project::is_block_script(path) {
            self.block_script_file_changed(path);
            return;
        }
        let Some(project) = &self.project else {
            return;
        };
        let known = project
            .scripts
            .iter()
            .find(|script| script.path == path)
            .map(|script| script.source.clone());
        if !project.folder.join(path).exists() {
            if known.is_some() {
                self.remove_script(path);
                self.output
                    .info(format!("`{path}` was deleted outside the editor."));
                self.sync_tab(path, None);
            }
            return;
        }
        let source = match project::read_script(&project.folder, path) {
            Ok(source) => source,
            Err(err) => {
                self.output
                    .error(format!("Skipped a damaged file: {}", chain(&err)));
                return;
            }
        };
        match known {
            // Saved by the editor itself, or touched without a change.
            Some(known) if known == source => return,
            Some(_) => {
                if let Some(project) = &mut self.project
                    && let Some(script) = project.scripts.iter_mut().find(|s| s.path == path)
                {
                    script.source = source.clone();
                }
                self.output
                    .info(format!("`{path}` was changed outside the editor."));
                self.reload_in_play(path);
            }
            None => {
                self.add_script(path, source.clone());
                self.output.info(format!("Found a new script, `{path}`."));
            }
        }
        self.sync_tab(path, Some(&source));
    }

    /// During Play, runs the script at `path` again in place, with the code
    /// the editor now has for it (see `Session::reload_script`).
    pub(in crate::app) fn reload_in_play(&mut self, path: &str) {
        let (Some(project), Some(session)) = (&self.project, &mut self.play) else {
            return;
        };
        let Some(script) = project.scripts.iter().find(|script| script.path == path) else {
            return;
        };
        if let Err(crash) =
            session.reload_script(script.instance, path, &script.source, &mut self.output)
        {
            self.crashed(crash);
        }
    }

    pub(super) fn block_script_file_changed(&mut self, path: &str) {
        let Some((folder, old_script)) = self.project.as_ref().map(|project| {
            (
                project.folder.clone(),
                project
                    .block_scripts
                    .iter()
                    .find(|document| document.path == path)
                    .map(|document| (document.script.clone(), document.saved.clone())),
            )
        }) else {
            return;
        };
        if !folder.join(path).exists() {
            if let Some((script, saved)) = old_script {
                if script != saved {
                    self.remove_script_instance(path);
                } else {
                    self.remove_script(path);
                }
                self.output
                    .info(format!("`{path}` was deleted outside the editor."));
                self.sync_tab(path, None);
            }
            return;
        }

        let script = match project::read_block_script(&folder, path) {
            Ok(script) => script,
            Err(err) => {
                self.output
                    .error(format!("Skipped a damaged file: {}", chain(&err)));
                return;
            }
        };
        let translation = match translate(&script) {
            Ok(translation) => translation,
            Err(err) => {
                self.output
                    .error(format!("Skipped a damaged block script `{path}`: {err}"));
                return;
            }
        };
        if old_script
            .as_ref()
            .is_some_and(|(_, saved)| *saved == script)
        {
            return;
        }

        let source = translation.source;
        let was_dirty = old_script
            .as_ref()
            .is_some_and(|(current, saved)| current != saved);
        match old_script {
            Some(_) if was_dirty => {
                if let Some(project) = &mut self.project {
                    if let Some(document) = project
                        .block_scripts
                        .iter_mut()
                        .find(|document| document.path == path)
                    {
                        document.saved = script;
                    }
                    if let Some(file) = project.scripts.iter_mut().find(|file| file.path == path) {
                        file.source = source.clone();
                    }
                }
                self.output.info(format!(
                    "`{path}` changed outside the editor; your unsaved blocks are kept."
                ));
                self.reload_in_play(path);
            }
            Some(_) => {
                if let Some(project) = &mut self.project {
                    if let Some(document) = project
                        .block_scripts
                        .iter_mut()
                        .find(|document| document.path == path)
                    {
                        document.script = script.clone();
                        document.saved = script;
                    }
                    if let Some(file) = project.scripts.iter_mut().find(|file| file.path == path) {
                        file.source = source.clone();
                    }
                }
                self.output
                    .info(format!("`{path}` was changed outside the editor."));
                self.reload_in_play(path);
            }
            None => {
                let id = self.add_script(path, source.clone());
                if let Some(project) = &mut self.project {
                    project.block_scripts.push(BlockDocument {
                        instance: id,
                        path: path.to_owned(),
                        script: script.clone(),
                        saved: script,
                        highlighted: None,
                    });
                }
                self.output
                    .info(format!("Found a new block script, `{path}`."));
            }
        }
        self.sync_tab(path, Some(&source));
    }

    /// Brings the tab for `path`, if open, up to date with its file, which
    /// now has `on_disk` in it (`None`: deleted). A tab with unsaved changes
    /// keeps them, and says the file changed.
    pub(super) fn sync_tab(&mut self, path: &str, on_disk: Option<&str>) {
        let Some(tab) = self.tabs.get_mut(path) else {
            return;
        };
        let had_changes = tab.unsaved();
        match on_disk {
            None if had_changes => tab.disk = Disk::Deleted,
            None => self.tabs.close(path),
            Some(source) => {
                tab.saved = source.to_string();
                if had_changes && tab.text != source {
                    tab.disk = Disk::Changed;
                    self.output.error(format!(
                        "Your unsaved changes to `{path}` are kept in its tab. Saving replaces \
                         the other version; Reload from disk in the tab gets it instead."
                    ));
                } else {
                    tab.text = source.to_string();
                    tab.disk = Disk::Same;
                }
            }
        }
    }
}
