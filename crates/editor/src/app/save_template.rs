//! File > Save as Template…: turns the open project into a template file
//! (`.axtemplate`, `axcenity_project::save_template`) in the user's
//! templates folder, which New Project lists under "Your templates"
//! (`start/dialogs.rs`). See `docs/project/templates.md`.

use std::path::PathBuf;

use egui::RichText;

use crate::theme::space;

use super::*;

/// The Save as Template dialog, while it's open.
#[derive(Default)]
pub(super) struct TemplateDraft {
    pub name: String,
    pub description: String,
    pub error: Option<String>,
}

impl EditorApp {
    /// The editor's own templates folder: the editor's data folder, beside
    /// its settings, global assets and plugins.
    pub fn default_templates_folder() -> Option<PathBuf> {
        eframe::storage_dir(crate::APP_ID).map(|data| data.join("Templates"))
    }

    /// Where templates are saved and listed from. Tests point it at a
    /// folder of their own.
    pub fn set_templates_folder(&mut self, folder: Option<PathBuf>) {
        self.start.set_templates_folder(folder.clone());
        self.templates_folder = folder;
    }

    /// Opens Save as Template…, named after the project.
    pub(super) fn open_save_template(&mut self) {
        let Some(project) = &self.project else {
            return;
        };
        let description = axcenity_project::read_manifest(&project.folder)
            .map(|manifest| manifest.description)
            .unwrap_or_default();
        self.template_draft = Some(TemplateDraft {
            name: project.name.clone(),
            description,
            error: None,
        });
    }

    /// Saves the project (when it has unsaved changes) and writes it as the
    /// template `name`. Returns the template file, or why not.
    pub(super) fn save_as_template(
        &mut self,
        name: &str,
        description: &str,
    ) -> Result<PathBuf, String> {
        let Some(folder) = self.project.as_ref().map(|project| project.folder.clone()) else {
            return Err("No project is open.".to_owned());
        };
        let Some(templates) = self.templates_folder.clone() else {
            return Err("This computer has no folder for the editor's data.".to_owned());
        };
        if name.trim().is_empty() {
            return Err("The template needs a name.".to_owned());
        }
        // A template is made from the files, so what's unsaved is saved first.
        if self.unsaved() && !self.save() {
            return Err("The project couldn't be saved first. Output says why.".to_owned());
        }
        let file = templates.join(axcenity_project::template_file_name(name));
        let template = axcenity_project::save_template(&folder, &file, name, description)
            .map_err(|err| files::chain(&err))?;
        self.start.reload_user_templates();
        self.output.info(format!(
            "Saved \"{}\" as a template ({} files) in `{}`. Pick it under Your templates in \
             New Project.",
            template.info.name,
            template.files,
            file.display()
        ));
        self.toasts
            .success(format!("Saved template \"{}\"", template.info.name));
        Ok(file)
    }

    /// Draws Save as Template… while it's open.
    pub(super) fn save_template_dialog(&mut self, ctx: &egui::Context) {
        if self.template_draft.is_none() {
            return;
        }
        let unsaved = !self.modified_documents().is_empty();
        let Some(draft) = &mut self.template_draft else {
            return;
        };
        let exists = self.templates_folder.as_ref().is_some_and(|folder| {
            folder
                .join(axcenity_project::template_file_name(&draft.name))
                .exists()
        });
        let mut picked = None;
        let ((), dismissed) = dialog::modal(ctx, "save as template", dialog::WIDTH, |ui| {
            let palette = theme::palette(ui.ctx());
            dialog::header(
                ui,
                icons::COPY_SIMPLE,
                dialog::Tone::Question,
                "Save as Template",
                Some(
                    "New Project will offer this project as a starting point, under Your \
                     templates: its world, scripts, assets and settings, under each new \
                     project's own name.",
                ),
            );
            ui.label(RichText::new("Name").color(palette.text_secondary));
            ui.add(dialog::text_field(&mut draft.name).hint_text("My Template"));
            ui.add_space(space::S);
            ui.label(RichText::new("Description").color(palette.text_secondary));
            ui.add(
                egui::TextEdit::multiline(&mut draft.description)
                    .desired_rows(2)
                    .margin(space::margin(space::S, 6.0))
                    .desired_width(f32::INFINITY),
            );
            let mut notes = Vec::new();
            if unsaved {
                notes.push("Your unsaved changes are saved first.");
            }
            if exists {
                notes.push("It replaces your template of the same name.");
            }
            if !notes.is_empty() {
                ui.add_space(space::S);
                ui.label(RichText::new(notes.join(" ")).color(palette.text_weak));
            }
            if let Some(error) = &draft.error {
                ui.add_space(space::S);
                ui.add(
                    egui::Label::new(
                        RichText::new(theme::with_icon(icons::WARNING_CIRCLE, error))
                            .color(palette.error),
                    )
                    .wrap(),
                );
            }
            picked = dialog::buttons(
                ui,
                &[
                    dialog::Button::new(icons::X, "Cancel", dialog::Kind::Plain),
                    dialog::Button::new(icons::COPY_SIMPLE, "Save Template", dialog::Kind::Main)
                        .enabled(!draft.name.trim().is_empty()),
                ],
                dialog::Enter::FromField,
            );
        });
        match picked {
            Some(1) => {
                let (name, description) = (draft.name.clone(), draft.description.clone());
                match self.save_as_template(&name, &description) {
                    Ok(_) => self.template_draft = None,
                    Err(error) => {
                        if let Some(draft) = &mut self.template_draft {
                            draft.error = Some(error);
                        }
                    }
                }
            }
            Some(_) => self.template_draft = None,
            None if dismissed => self.template_draft = None,
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Save as Template saves unsaved changes first, writes the template
    /// where New Project lists it, and a project made from it has them.
    #[test]
    fn save_as_template_makes_a_template_new_project_lists() {
        let parent = std::env::temp_dir().join(format!(
            "axcenity-save-template-editor-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&parent);
        std::fs::create_dir_all(&parent).unwrap();
        let folder = axcenity_project::create_project(&parent, "Base Game").unwrap();
        let mut app = EditorApp::new(Some(&folder));
        let templates = parent.join("Templates");
        app.set_templates_folder(Some(templates.clone()));

        let workspace = app.world.workspace();
        app.apply(workspace, Edit::Property("Gravity", Value::Number(50.0)));
        app.history.seal();
        assert!(app.unsaved());

        app.open_save_template();
        assert_eq!(
            app.template_draft.as_ref().map(|d| d.name.as_str()),
            Some("Base Game")
        );
        let file = app.save_as_template("Low Gravity", "Floaty").unwrap();
        assert!(!app.unsaved(), "saved first");
        assert_eq!(file, templates.join("Low Gravity.axtemplate"));
        let listed = app.start.user_templates();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].info.description, "Floaty");

        let made =
            axcenity_project::create_project_from_template_file(&parent, "Moon", &file).unwrap();
        let project = axcenity_project::load(&made).unwrap();
        assert_eq!(project.manifest.name, "Moon");
        assert_eq!(
            project
                .world
                .get(project.world.workspace())
                .and_then(|w| w.property("Gravity")),
            Some(&Value::Number(50.0))
        );
        assert!(
            app.save_as_template("  ", "").is_err(),
            "a template needs a name"
        );
        drop(app);
        let _ = std::fs::remove_dir_all(parent);
    }

    /// The dialog draws, with its notes, and Escape closes it.
    #[test]
    fn the_save_as_template_dialog_draws_and_closes() {
        let mut app = EditorApp::new(Some(&super::super::tests::example("hello-graphics")));
        app.open_save_template();
        if let Some(folder) = std::env::var_os("AXCENITY_SNAPSHOTS") {
            let mut shown = EditorApp::new(Some(&super::super::tests::example("hello-graphics")));
            shown.open_save_template();
            crate::snapshot::snapshot(
                &std::path::PathBuf::from(folder).join("save-as-template.png"),
                egui::vec2(700.0, 460.0),
                vec![Vec::new(); 3],
                |ui| {
                    let ctx = &ui.ctx().clone();
                    egui::CentralPanel::default().show(ui, |_| {});
                    shown.save_template_dialog(ctx);
                },
            );
        }
        let ctx = crate::theme::test_context();
        let escape = egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Default::default(),
        };
        for events in [vec![], vec![escape]] {
            let input = egui::RawInput {
                events,
                ..Default::default()
            };
            let _ = crate::theme::run(&ctx, input, |ui| {
                let ctx = &ui.ctx().clone();
                app.save_template_dialog(ctx)
            });
        }
        assert!(app.template_draft.is_none());
    }
}
