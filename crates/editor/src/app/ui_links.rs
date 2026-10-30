//! The UI Designer's Code & animation section, applied: a handler for a UI
//! element's event written into its ScreenGui's LocalScript and opened
//! there (see `ui_designer/links.rs`). These are methods of
//! [`super::EditorApp`].

use super::*;
use axcenity_project as project;

use super::files::chain;
use crate::ui_designer::links::{self, ScriptText};

impl EditorApp {
    /// The project's Lua scripts as the editor has them: each open tab's
    /// text (unsaved changes and all), else the file's.
    pub(super) fn script_texts(&self) -> Vec<(String, String)> {
        let Some(project) = &self.project else {
            return Vec::new();
        };
        project
            .scripts
            .iter()
            .filter(|script| script.path.ends_with(".lua"))
            .map(|script| {
                let text = self
                    .tabs
                    .get(&script.path)
                    .map_or(&script.source, |tab| &tab.text);
                (script.path.clone(), text.clone())
            })
            .collect()
    }

    /// Writes `path.event:Connect(function() body end)` for the element
    /// `id` at the end of its ScreenGui's LocalScript (the first Lua one
    /// naming the ScreenGui's path; a new one named after the ScreenGui
    /// when none does), and opens it with the cursor in the function. In an
    /// existing script it's an unsaved change in its tab, as if typed.
    pub(super) fn add_ui_handler(&mut self, id: InstanceId, event: &str, body: Option<&str>) {
        if self.playing() {
            return;
        }
        let Some(folder) = self.project.as_ref().map(|project| project.folder.clone()) else {
            return;
        };
        let Some(gui) = crate::ui_designer::screen_gui_of(&self.world, Some(id)) else {
            return;
        };
        let Some(path) = links::lua_path(&self.world, id) else {
            return;
        };
        let element = self.world.get(id).map_or("", |i| i.name()).to_owned();
        let (snippet, snippet_line) = links::handler(&path, event, body);
        let texts = self.script_texts();
        let scripts: Vec<ScriptText> = texts
            .iter()
            .map(|(path, text)| ScriptText { path, text })
            .collect();
        let target = links::handler_script(&self.world, &scripts, gui).map(str::to_owned);
        match target {
            Some(script) => {
                let mut text = texts
                    .iter()
                    .find(|(path, _)| *path == script)
                    .map(|(_, text)| text.clone())
                    .unwrap_or_default();
                if !text.is_empty() && !text.ends_with('\n') {
                    text.push('\n');
                }
                let line = text.lines().count() + snippet_line;
                text.push_str(&snippet);
                let saved = self
                    .project
                    .as_ref()
                    .and_then(|project| project.scripts.iter().find(|s| s.path == script))
                    .map(|script| script.source.clone())
                    .unwrap_or_default();
                self.tabs.open_in_background(&script, &saved).text = text;
                self.open_script(&script, Some(line));
                self.output.info(format!(
                    "Added a {event} handler for {element} to `{script}`."
                ));
            }
            None => {
                let gui_name = self.world.get(gui).map_or("", |i| i.name()).to_owned();
                let stem: String = gui_name
                    .chars()
                    .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect();
                let stem = if stem.is_empty() {
                    "Ui".to_owned()
                } else {
                    stem
                };
                let header = format!(
                    "-- What the {gui_name} ScreenGui does. The UI Designer adds handlers at the end.\n"
                );
                let source = format!("{header}{snippet}");
                let line = header.lines().count() + snippet_line;
                let service = project::script_service(&self.world, ClassName::LocalScript);
                for name in tree::numbered_names(&stem) {
                    if self.world.find_first_child(service, &name).is_some() {
                        continue;
                    }
                    match project::create_script(&folder, &name, ClassName::LocalScript, &source) {
                        Ok(script) => {
                            self.add_script(&script, source.clone());
                            self.tabs.open(&script, &source, Some(line));
                            self.output.info(format!(
                                "Made `{script}` with a {event} handler for {element}."
                            ));
                            return;
                        }
                        Err(err) if err.already_exists() => continue,
                        Err(err) => {
                            let err = chain(&err);
                            self.output
                                .error(format!("Could not make a script for {gui_name}: {err}"));
                            return;
                        }
                    }
                }
            }
        }
    }
}
