use super::*;
use crate::scripts::ScriptView;

impl EditorApp {
    /// The path of the file of the Script `id`.
    pub(super) fn script_path(&self, id: InstanceId) -> Option<String> {
        let project = self.project.as_ref()?;
        let script = project
            .scripts
            .iter()
            .find(|script| script.instance == id)?;
        Some(script.path.clone())
    }

    /// Adds the script at `path` (whose file has `source` in it) to the
    /// project, with a Script or LocalScript for it, and returns that.
    pub(super) fn add_script(&mut self, path: &str, source: String) -> InstanceId {
        let instance = project::add_script_instance(&mut self.world, path);
        if let Some(project) = &mut self.project {
            project.scripts.push(ScriptFile {
                instance,
                path: path.to_string(),
                source,
            });
        }
        self.sort_scripts();
        instance
    }

    /// Takes the script at `path` out of the project, and its Script out of
    /// the world. Its tab stays (the caller decides).
    pub(super) fn remove_script(&mut self, path: &str) {
        self.remove_script_instance(path);
        if let Some(project) = &mut self.project {
            project
                .block_scripts
                .retain(|document| document.path != path);
        }
    }

    pub(super) fn remove_script_instance(&mut self, path: &str) {
        let Some(project) = &mut self.project else {
            return;
        };
        let Some(index) = project
            .scripts
            .iter()
            .position(|script| script.path == path)
        else {
            return;
        };
        let script = project.scripts.remove(index);
        // Scripts are never in the undo history, so nothing refers to it there.
        let _ = self.world.destroy(script.instance);
        self.selection.retain(|id| *id != script.instance);
        if self.selected == Some(script.instance) {
            self.selected = self.selection.last().copied();
        }
    }

    /// Keeps the scripts in the order they start in (and load in next
    /// time): ModuleScripts, Scripts, then LocalScripts, each by path. Their
    /// Instances in ReplicatedStorage, ServerScriptService and
    /// StarterPlayerScripts follow the same order.
    pub(super) fn sort_scripts(&mut self) {
        let Some(project) = &mut self.project else {
            return;
        };
        project
            .scripts
            .sort_by(|a, b| project::script_order(&a.path).cmp(&project::script_order(&b.path)));
        // Each service's scripts go after its other children (only
        // ReplicatedStorage has any: RemoteEvents, Folders and so on), as
        // they are after loading, where the world file comes first.
        let mut next_index = std::collections::HashMap::new();
        for script in &project.scripts {
            let service = project::script_service(&self.world, project::script_class(&script.path));
            let index = next_index
                .entry(service)
                .or_insert_with(|| non_script_children(&self.world, service));
            let _ = self.world.set_parent_at(script.instance, service, *index);
            *index += 1;
        }
    }

    /// Opens the Script `id` in a tab.
    pub(in crate::app) fn open_script_instance(&mut self, id: InstanceId) {
        if let Some(path) = self.script_path(id) {
            self.open_script(&path, None);
        }
    }

    /// Opens the script at `path` in a tab (or shows its tab), with the
    /// cursor on `line` if there is one.
    pub(in crate::app) fn open_script(&mut self, path: &str, line: Option<usize>) {
        let known = self
            .project
            .as_ref()
            .and_then(|project| project.scripts.iter().find(|script| script.path == path))
            .map(|script| script.source.clone());
        if project::is_block_script(path)
            && let Some(document) = self.project.as_mut().and_then(|project| {
                project
                    .block_scripts
                    .iter_mut()
                    .find(|document| document.path == path)
            })
        {
            document.highlighted =
                line.and_then(|line| translate(&document.script).ok()?.block_at(line));
        }
        match known {
            Some(source) => self.tabs.open(path, &source, line),
            // Its file was deleted, but its tab is still open.
            None if self.tabs.get(path).is_some() => self.tabs.open(path, "", line),
            None => self.output.error(format!(
                "Can't open `{path}`: this project has no such script now (it may have \
                 been renamed or deleted)."
            )),
        }
    }

    /// Makes a new script file for a script of `class`:
    /// `scripts/Script.server.lua` for a Script, or
    /// `scripts/LocalScript.client.lua` for a LocalScript (or `Script2` and
    /// so on), with its Instance, then selects it and opens it.
    pub(in crate::app) fn insert_script(&mut self, class: ClassName) {
        let Some(project) = &self.project else {
            return;
        };
        let folder = project.folder.clone();
        let service = project::script_service(&self.world, class);
        for name in tree::numbered_names(class.as_str()) {
            if self.world.find_first_child(service, &name).is_some() {
                continue;
            }
            let source = new_script_source(class);
            match project::create_script(&folder, &name, class, source) {
                Ok(path) => {
                    let id = self.add_script(&path, source.to_string());
                    // Selects it and shows it in the Explorer.
                    self.select_from_viewport(Some(id));
                    self.tabs.open(&path, source, None);
                    self.output.info(format!("Made a new script, `{path}`."));
                    return;
                }
                // A file with that name is there already (one that couldn't
                // be read, or one that only differs in capitals): next name.
                Err(err) if err.already_exists() => continue,
                Err(err) => {
                    let err = chain(&err);
                    self.output
                        .error(format!("Could not make a new script: {err}"));
                    return;
                }
            }
        }
    }

    /// Shows the Script or LocalScript at `path` as `view`. Code ↔ Blocks or
    /// Graph replaces its file (see `switch_script_mode`); Blocks ↔ Graph is
    /// the same file, which remembers the view (saved now, unless the
    /// script has other unsaved changes, which are saved with it).
    pub(in crate::app) fn switch_script_view(&mut self, path: &str, view: ScriptView) {
        if self.playing() {
            return;
        }
        let graph = view == ScriptView::Graph;
        let blocks = project::is_block_script(path);
        if view == ScriptView::Code || !blocks {
            self.switch_script_mode(path, graph);
            return;
        }
        let Some(document) = self.project.as_mut().and_then(|project| {
            project
                .block_scripts
                .iter_mut()
                .find(|document| document.path == path)
        }) else {
            return;
        };
        let clean = document.script == document.saved;
        document.script.graph.shown = graph;
        if clean {
            self.write_block_tab(path);
        }
        let name = project::script_name(path).to_owned();
        let shown = if graph { "Graph" } else { "Blocks" };
        self.output.info(format!("Showing {name} as {shown}."));
    }

    /// Switches the Script or LocalScript at `path` between Code and Blocks,
    /// keeping its Instance. Blocks become the Lua they translate to; Lua
    /// becomes blocks, with code that has no blocks kept in grey Lua blocks.
    /// The file is replaced by one with the other ending (`.server.lua` ↔
    /// `.server.blocks.json`), so switching saves the script. Like a rename,
    /// it isn't an undo step. Blocks switched to Code and back, with the code
    /// unchanged, come back exactly as they were, loose blocks and all.
    /// Code switched to blocks is shown as a graph if `graph`.
    fn switch_script_mode(&mut self, path: &str, graph: bool) {
        if self.playing() {
            return;
        }
        let Some(project) = &self.project else {
            return;
        };
        let folder = project.folder.clone();
        let name = project::script_name(path).to_owned();
        let Some(instance) = project
            .scripts
            .iter()
            .find(|script| script.path == path)
            .map(|script| script.instance)
        else {
            return;
        };
        let to_blocks = !project::is_block_script(path);
        let (new_path, source, blocks) = if to_blocks {
            let source = self.tabs.get(path).map_or_else(
                || {
                    project
                        .scripts
                        .iter()
                        .find(|script| script.path == path)
                        .map(|script| script.source.clone())
                        .unwrap_or_default()
                },
                |tab| tab.text.clone(),
            );
            let remembered = project
                .remembered_blocks
                .get(path)
                .filter(|(code, _)| *code == source)
                .map(|(_, blocks)| blocks.clone());
            let mut blocks = match remembered.map_or_else(|| from_lua(&source), Ok) {
                Ok(blocks) => blocks,
                Err(err) => {
                    self.output.error(format!(
                        "Can't show {name} as blocks: {err}. Fix the code first, then switch."
                    ));
                    return;
                }
            };
            blocks.graph.shown = graph;
            let source = match translate(&blocks) {
                Ok(translation) => translation.source,
                Err(err) => {
                    self.output
                        .error(format!("Can't show {name} as blocks: {err}."));
                    return;
                }
            };
            match project::switch_to_blocks(&folder, path, &blocks) {
                Ok(new_path) => (new_path, source, Some(blocks)),
                Err(err) => {
                    self.output.error(format!(
                        "Could not switch {name} to Blocks: {}",
                        chain(&err)
                    ));
                    return;
                }
            }
        } else {
            let Some(document) = project
                .block_scripts
                .iter()
                .find(|document| document.path == path)
            else {
                return;
            };
            let blocks = document.script.clone();
            let source = match translate(&blocks) {
                Ok(translation) => translation.source,
                Err(err) => {
                    self.output
                        .error(format!("Can't switch {name} to Code: {err}."));
                    return;
                }
            };
            match project::switch_to_code(&folder, path, &source) {
                Ok(new_path) => {
                    if let Some(project) = &mut self.project {
                        project
                            .remembered_blocks
                            .insert(new_path.clone(), (source.clone(), blocks));
                    }
                    (new_path, source, None)
                }
                Err(err) => {
                    self.output
                        .error(format!("Could not switch {name} to Code: {}", chain(&err)));
                    return;
                }
            }
        };

        if let Some(project) = &mut self.project {
            project
                .block_scripts
                .retain(|document| document.path != path);
            if let Some(blocks) = blocks {
                project.block_scripts.push(BlockDocument {
                    instance,
                    path: new_path.clone(),
                    script: blocks.clone(),
                    saved: blocks,
                    highlighted: None,
                });
            }
            if let Some(script) = project.scripts.iter_mut().find(|s| s.path == path) {
                script.path = new_path.clone();
                script.source = source.clone();
            }
        }
        self.tabs.rename(path, &new_path);
        if let Some(tab) = self.tabs.get_mut(&new_path) {
            tab.text = source;
            tab.mark_saved();
        }
        self.sort_scripts();
        let mode = match (to_blocks, graph) {
            (false, _) => "Code",
            (true, false) => "Blocks",
            (true, true) => "Graph",
        };
        self.output
            .info(format!("Switched {name} to {mode} (`{new_path}`)."));
    }

    /// Renames the Script `id` and its file to `name`. Not an undo step.
    pub(in crate::app) fn rename_script(&mut self, id: InstanceId, name: &str) {
        let name = name.trim();
        let (Some(project), Some(old)) = (&self.project, self.script_path(id)) else {
            return;
        };
        let old_name = project::script_name(&old).to_string();
        if old_name == name {
            return;
        }
        if let Err(reason) = project::check_script_name(name) {
            self.output
                .error(format!("Can't rename {old_name} to \"{name}\": {reason}."));
            return;
        }
        let rename = if project::is_block_script(&old) {
            project::rename_block_script(&project.folder, &old, name)
        } else {
            project::rename_script(&project.folder, &old, name)
        };
        match rename {
            Ok(new) => {
                self.world.set_name(id, name).expect("it's a Script");
                if let Some(project) = &mut self.project
                    && let Some(script) = project.scripts.iter_mut().find(|s| s.instance == id)
                {
                    script.path = new.clone();
                }
                if let Some(document) = self
                    .project
                    .as_mut()
                    .and_then(|project| project.block_scripts.iter_mut().find(|d| d.instance == id))
                {
                    document.path = new.clone();
                }
                self.tabs.rename(&old, &new);
                self.sort_scripts();
                self.output.info(format!("Renamed `{old}` to `{new}`."));
            }
            Err(err) => {
                let err = chain(&err);
                self.output
                    .error(format!("Could not rename {old_name}: {err}"));
            }
        }
    }

    /// Shows "Delete script?" while a Script is waiting to be deleted.
    pub(in crate::app) fn delete_script_prompt(&mut self, ctx: &egui::Context) {
        let Some(id) = self.confirm_delete else {
            return;
        };
        let Some(path) = self.script_path(id) else {
            // It went away meanwhile (deleted in another program, say).
            self.confirm_delete = None;
            return;
        };
        let title = format!("Delete {}?", project::script_name(&path));
        let message = format!("This deletes `{path}` from the project folder. It can't be undone.");
        let outcome = dialog::ask(
            ctx,
            &dialog::Question {
                id: "delete script",
                icon: icons::TRASH,
                tone: dialog::Tone::Danger,
                title: &title,
                message: &message,
                items: Vec::new(),
            },
            &[
                dialog::Button {
                    icon: icons::X,
                    label: "Cancel",
                    kind: dialog::Kind::Plain,
                },
                dialog::Button {
                    icon: icons::TRASH,
                    label: "Delete",
                    kind: dialog::Kind::Danger,
                },
            ],
        );
        // Escape or a click outside the box means Cancel.
        let delete = outcome.map(|outcome| outcome == dialog::Outcome::Picked(1));
        if let Some(delete) = delete {
            self.confirm_delete = None;
            if delete {
                self.delete_script(id);
            }
        }
    }

    /// Deletes the Script `id` and its file, and closes its tab.
    pub(in crate::app) fn delete_script(&mut self, id: InstanceId) {
        let (Some(project), Some(path)) = (&self.project, self.script_path(id)) else {
            return;
        };
        match project::delete_script(&project.folder, &path) {
            Ok(()) => {}
            // Already gone is as good as deleted.
            Err(err) if err.source.kind() == ErrorKind::NotFound => {}
            Err(err) => {
                self.output
                    .error(format!("Could not delete it: {}", chain(&err)));
                return;
            }
        }
        self.remove_script(&path);
        self.tabs.close(&path);
        self.output.info(format!("Deleted `{path}`."));
    }
}
