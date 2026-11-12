//! Attaching a script to an object by dragging it there in the Explorer:
//! a Script or LocalScript that is a file in its service becomes a script
//! inside the object (G2), its file moved from `scripts/` to
//! `objects/<object>/`, as one undo step that moves it back.

use super::*;

impl EditorApp {
    /// The script `id` (a file in its service) was dropped on `object`:
    /// checks it can be attached, then asks first, since the file moves.
    pub(in crate::app) fn ask_attach(&mut self, id: InstanceId, object: InstanceId) {
        if self.playing() {
            return;
        }
        let name = self
            .world
            .get(id)
            .map(|instance| instance.name().to_owned())
            .unwrap_or_default();
        if let Err(reason) = tree::check_attach(&self.world, id, object) {
            self.output.error(format!("Can't attach {name}: {reason}"));
            return;
        }
        let Some(path) = self.script_path(id) else {
            return;
        };
        if project::is_block_script(&path) {
            self.output.error(format!(
                "Can't attach {name} yet: it's a block script. Switch it to Code first (the \
                 view bar over its tab), or insert a new script in the object."
            ));
            return;
        }
        if self.tabs.get(&path).is_some_and(|tab| tab.unsaved()) {
            self.output
                .error(format!("Save {name} first: attaching it moves its file."));
            return;
        }
        self.confirm_attach = Some((id, object));
    }

    /// Shows "Attach it?" while a dragged script waits for an answer.
    pub(in crate::app) fn attach_script_prompt(&mut self, ctx: &egui::Context) {
        let Some((id, object)) = self.confirm_attach else {
            return;
        };
        let (Some(path), Some(target)) = (self.script_path(id), self.world.get(object)) else {
            self.confirm_attach = None;
            return;
        };
        let name = project::script_name(&path).to_owned();
        let object_name = target.name().to_owned();
        let title = format!("Attach {name} to {object_name}?");
        let message = format!(
            "`{path}` moves into `{}/{object_name}/`, and {name} runs inside {object_name}: \
             `script.Parent` is {object_name}, and every copy of {object_name} runs its own \
             {name} from the same file. Undo moves it back.",
            project::OBJECTS_FOLDER
        );
        let outcome = dialog::ask(
            ctx,
            &dialog::Question {
                id: "attach script",
                icon: icons::LINK,
                tone: dialog::Tone::Question,
                title: &title,
                message: &message,
                items: Vec::new(),
            },
            &[
                dialog::Button::cancel(),
                dialog::Button::new(icons::LINK, "Attach", dialog::Kind::Main),
            ],
        );
        if let Some(outcome) = outcome {
            self.confirm_attach = None;
            if outcome == dialog::Outcome::Picked(1) {
                self.attach_script(id, object);
            }
        }
    }

    /// Attaches the script `id`, a file in its service, to `object`: its
    /// file moves to `objects/<object>/` (with a number if that name is
    /// taken) and a script inside `object` names it, in place of the one in
    /// the service. One undo step, which moves the file back.
    pub(in crate::app) fn attach_script(&mut self, id: InstanceId, object: InstanceId) {
        let (Some(project), Some(path)) = (&self.project, self.script_path(id)) else {
            return;
        };
        let folder = project.folder.clone();
        let Some(source) = project
            .scripts
            .iter()
            .find(|script| script.instance == id)
            .map(|script| script.source.clone())
        else {
            return;
        };
        let (Some(script), Some(target)) = (self.world.get(id), self.world.get(object)) else {
            return;
        };
        let class = script.class();
        let name = project::script_name(&path).to_owned();
        let object_name = target.name().to_owned();
        // The new file first, so a failure leaves the old one where it is.
        let mut made = None;
        for n in 1..100 {
            let file_name = if n == 1 {
                name.clone()
            } else {
                format!("{name} {n}")
            };
            match project::create_object_script(&folder, &object_name, &file_name, class, &source) {
                Ok(new_path) => {
                    made = Some(new_path);
                    break;
                }
                Err(err) if err.already_exists() => continue,
                Err(err) => {
                    self.output
                        .error(format!("Could not attach {name}: {}", chain(&err)));
                    return;
                }
            }
        }
        let Some(new_path) = made else {
            self.output.error(format!(
                "Could not attach {name}: `{}/{object_name}/` has too many scripts named {name}.",
                project::OBJECTS_FOLDER
            ));
            return;
        };
        match project::delete_script(&folder, &path) {
            Ok(()) => {}
            Err(err) if err.source.kind() == ErrorKind::NotFound => {}
            Err(err) => {
                let _ = fs::remove_file(folder.join(&new_path));
                self.output
                    .error(format!("Could not attach {name}: {}", chain(&err)));
                return;
            }
        }
        let before = self.world.clone();
        self.remove_script(&path);
        let Ok(attached) = self.world.create(class) else {
            return;
        };
        let _ = self.world.set_name(attached, &name);
        let _ = self
            .world
            .set_property(attached, "SourceFile", Value::String(new_path.clone()));
        let _ = self.world.set_parent(attached, Some(object));
        if let Some(project) = &mut self.project {
            project
                .object_files
                .insert(new_path.clone(), source.clone());
        }
        self.history.record_world_and_files(
            &mut self.world,
            format!("Attach {name} to {object_name}"),
            before,
            vec![
                history::TextChange {
                    path: path.clone(),
                    before: Some(source.clone()),
                    after: None,
                },
                history::TextChange {
                    path: new_path.clone(),
                    before: None,
                    after: Some(source),
                },
            ],
        );
        // Its tab and breakpoints follow the file.
        self.tabs.rename(&path, &new_path);
        self.select_from_viewport(Some(attached));
        self.output.info(format!(
            "Attached {name} to {object_name}: `{path}` is now `{new_path}`. Copies of \
             {object_name} share it."
        ));
    }

    /// After undo or redo put back or took away the file of a script in
    /// its service (attaching one, above): the project's list of scripts
    /// follows, using the Script the undone world has for it.
    pub(in crate::app) fn follow_service_script_files(&mut self, files: &[store::FileDiff]) {
        let prefix = format!("{}/", project::SCRIPTS_FOLDER);
        let mut moved_from = None;
        let mut moved_to = None;
        for file in files.iter().filter(|file| file.path.starts_with(&prefix)) {
            let listed = self
                .project
                .as_ref()
                .is_some_and(|project| project.scripts.iter().any(|s| s.path == file.path));
            match &file.after {
                Some(text) if !listed => {
                    let class = project::script_class(&file.path);
                    let name = project::script_name(&file.path);
                    let service = project::script_service(&self.world, class);
                    let instance = self.world.find_first_child(service, name).filter(|&id| {
                        self.world
                            .get(id)
                            .is_some_and(|instance| instance.source_file().is_none())
                    });
                    match (instance, &mut self.project) {
                        (Some(instance), Some(project)) => {
                            project.scripts.push(ScriptFile {
                                instance,
                                path: file.path.clone(),
                                source: text.clone(),
                            });
                            self.sort_scripts();
                        }
                        _ => {
                            self.add_script(&file.path, text.clone());
                        }
                    }
                    moved_to = Some(file.path.clone());
                }
                None if listed => {
                    self.remove_script_instance(&file.path);
                    moved_from = Some(file.path.clone());
                }
                _ => {}
            }
        }
        // The tab follows the file between `scripts/` and `objects/`.
        let object_file = |file: &store::FileDiff, added: bool| {
            !file.path.starts_with(&prefix) && file.after.is_some() == added
        };
        if let Some(to) = moved_to
            && let Some(from) = files.iter().find(|file| object_file(file, false))
        {
            self.tabs.rename(&from.path, &to);
        }
        if let Some(from) = moved_from
            && let Some(to) = files.iter().find(|file| object_file(file, true))
        {
            self.tabs.rename(&from, &to.path);
        }
    }
}
