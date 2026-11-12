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
    (project.scripts.iter().any(|script| script.path == path)
        || project.object_files.contains_key(path))
        && project.folder.join(path).exists()
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
            } else if path == project::STRINGS_FILE || path == project::INPUT_FILE {
                self.settings_file_changed(path);
            } else if axcenity_blocks::ui_graph::is_ui_graph(path) {
                self.ui_graph_file_changed(path);
            } else if project::is_stylesheet(path) {
                self.stylesheet_file_changed(path);
            } else if path.starts_with("team/") {
                // Teamwork files; anything else there is the team's own.
                if axcenity_teamwork::board::is_board_path(path) {
                    self.board_file_changed();
                } else if axcenity_teamwork::history::is_history_path(path) {
                    self.team_board.history.stale = true;
                }
            } else if project::is_asset_path(path) {
                self.asset_file_changed(path);
            } else if self.is_object_file(path) {
                self.object_file_changed(path);
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
    pub(in crate::app) fn asset_file_changed(&mut self, path: &str) {
        // A sidecar written, moved or deleted: asset IDs are read again.
        if axcenity_assets::is_import_file(path) {
            self.assets.forget_asset_ids();
        }
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
        // An imported, new, edited or deleted texture or model: cook it
        // (and give it an asset ID) now rather than at the next open.
        if axcenity_assets::takes_asset_id(path)
            && let Some(cooker) = &self.cooker
        {
            cooker.cook_again();
        }
        if axcenity_assets::is_data_table(path)
            && let Some(folder) = self.project.as_ref().map(|project| project.folder.clone())
        {
            self.data_tables.file_changed(&folder, path);
            self.hand_over_tables();
        }
        if axcenity_assets::is_material_file(path)
            && let Some(folder) = self.project.as_ref().map(|project| project.folder.clone())
        {
            self.materials.file_changed(&folder, path);
            self.hand_over_materials();
        }
        if let Some(folder) = self.project.as_ref().map(|project| project.folder.clone()) {
            if axcenity_blocks::behavior_tree::is_behavior_tree(path) {
                self.behavior.file_changed(&folder, path);
            }
            if axcenity_blocks::dialogue::is_dialogue(path) {
                self.dialogues.file_changed(&folder, path);
            }
        }
    }

    /// `strings.json` or `input.json` changed on disk (a teammate saved
    /// it, or another program did): the string table or the input map is
    /// read again. A damaged file keeps what the editor had, and says so.
    pub(in crate::app) fn settings_file_changed(&mut self, path: &str) {
        let Some(folder) = self.project.as_ref().map(|project| project.folder.clone()) else {
            return;
        };
        let text = match std::fs::read_to_string(folder.join(path)) {
            Ok(text) => Some(text),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => {
                self.output
                    .error(format!("Couldn't read `{path}` again: {error}"));
                return;
            }
        };
        if path == project::STRINGS_FILE {
            let table = match text.as_deref().map(project::parse_strings) {
                None => Default::default(),
                Some(Ok(table)) => table,
                Some(Err(error)) => {
                    self.output.error(format!("`{path}`: {error}"));
                    return;
                }
            };
            if **self.world.strings() != table {
                self.world.set_strings(std::sync::Arc::new(table));
                self.output.info(format!("Reloaded `{path}`."));
            }
        } else if let Some(project) = &mut self.project {
            let map = match text.as_deref().map(project::parse_input_map) {
                None => Default::default(),
                Some(Ok(map)) => map,
                Some(Err(error)) => {
                    self.output.error(format!("`{path}`: {error}"));
                    return;
                }
            };
            if project.input_map != map {
                project.input_map = map;
                self.output.info(format!("Reloaded `{path}`."));
            }
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
        // Another level is open: `world.json` is read again when the start
        // level is opened, or Play reads it.
        if project.level.is_some() {
            return;
        }
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

    /// Whether `path` is a file scripts inside objects take their code from
    /// (G2): one the editor has read, or one in `objects/`.
    pub(in crate::app) fn is_object_file(&self, path: &str) -> bool {
        self.project.as_ref().is_some_and(|project| {
            project.object_files.contains_key(path)
                || path.starts_with(&format!("{}/", project::OBJECTS_FOLDER))
        })
    }

    /// The file of scripts inside objects at `path` was made, changed or
    /// deleted. Its scripts' Instances are in the world, so none are added
    /// or removed: a copy whose file is gone does nothing until it's back.
    fn object_file_changed(&mut self, path: &str) {
        let Some(project) = &mut self.project else {
            return;
        };
        let known = project.object_files.get(path).cloned();
        if !project.folder.join(path).exists() {
            if known.is_some() {
                project.object_files.remove(path);
                self.output
                    .info(format!("`{path}` was deleted outside the editor."));
                self.sync_tab(path, None);
            }
            return;
        }
        let source = match project::read_object_file(&project.folder, path) {
            Ok(source) => source,
            Err(err) => {
                self.output
                    .error(format!("Skipped a damaged file: {}", chain(&err)));
                return;
            }
        };
        if known.as_ref() == Some(&source) {
            return;
        }
        project.object_files.insert(path.to_owned(), source.clone());
        if known.is_some() {
            self.output
                .info(format!("`{path}` was changed outside the editor."));
            self.reload_in_play(path);
        }
        self.sync_tab(path, Some(&source));
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
        let reloaded = if let Some(script) = project.scripts.iter().find(|s| s.path == path) {
            session.reload_script(script.instance, path, &script.source, &mut self.output)
        } else if let Some(source) = project.object_files.get(path) {
            // Every copy of a script inside an object uses the file.
            session.reload_copies(path, source, &mut self.output)
        } else {
            return;
        };
        if let Err(crash) = reloaded {
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
        let script = axcenity_blocks::refine(&script).unwrap_or(script);
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
                        document.undo.clear();
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
                        undo: Default::default(),
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
    pub(in crate::app) fn sync_tab(&mut self, path: &str, on_disk: Option<&str>) {
        let Some(tab) = self.tabs.get_mut(path) else {
            return;
        };
        let had_changes = tab.unsaved();
        match on_disk {
            None if had_changes => tab.disk = Disk::Deleted,
            None => self.tabs.close(path),
            // A live tab (C7) shows the session's document; its file is
            // that document written back, so only `saved` follows it.
            Some(source) if tab.live => {
                tab.saved = source.to_string();
                tab.disk = Disk::Same;
            }
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
