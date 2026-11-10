use super::*;
use crate::dialog;

/// The New state machine dialog, named after the followed object if
/// there is one.
pub(super) fn new_controller_dialog(subject: Option<&Subject>) -> Dialog {
    Dialog::NewController {
        name: subject
            .map(|subject| axcenity_anim::files::file_safe(&subject.name))
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "StateMachine".to_owned()),
        with_clips: true,
        assign: subject.is_some(),
    }
}

/// Fills a New clip dialog's takes for its model.
pub(super) fn load_takes(dialog: &mut Dialog, files: &AnimationFiles, assets: &Assets) {
    if let Dialog::NewClip { model, takes, .. } = dialog
        && let Some(path) = files.models.get(*model)
    {
        *takes = takes_of(path, assets);
    }
}

pub(super) fn takes_of(path: &str, assets: &Assets) -> Result<Vec<Take>, String> {
    assets
        .takes(path)
        .map(|takes| {
            takes
                .iter()
                .filter(|take| !take.tracks.is_empty())
                .cloned()
                .collect()
        })
        .ok_or_else(|| format!("`{path}` can't be read"))
}

/// Shows `mesh_id`'s model in the preview, or nothing.
pub(super) fn show_model(preview: &mut Preview, mesh_id: Option<&str>, assets: &Assets) {
    match mesh_id.and_then(|mesh_id| Some((mesh_id, assets.model_blocking(mesh_id)?))) {
        Some((mesh_id, model)) => preview.set_model(mesh_id, &model),
        None => preview.clear(),
    }
}

impl AnimationStudio {
    /// Does what a picker or a ⋯ menu asked for.
    pub(super) fn act(
        &mut self,
        pick: Pick,
        project: &Path,
        assets: &Assets,
        world: &mut World,
        output: &mut Output,
        actions: &mut Vec<StudioAction>,
    ) {
        let is_controller = |path: &str| path.ends_with(axcenity_anim::STATE_MACHINE_EXTENSION);
        let subject = self.subject.and_then(|id| Subject::of(world, id));
        match pick {
            Pick::Open(path) if is_controller(&path) => {
                if self.controller_path().as_deref() == Some(&path) {
                    self.view = View::StateMachine;
                } else {
                    let model = subject
                        .filter(|subject| subject.state_machine == path)
                        .and_then(|subject| subject.rigged_model(assets));
                    self.open_controller(project, assets, &path, model, output);
                }
            }
            Pick::Open(path) => {
                self.open_subject_clip(project, assets, &path, subject.as_ref(), world, output);
                self.from_state = None;
            }
            Pick::ConvertToKeys(path) => {
                if self.clip_path().as_deref() == Some(&path) {
                    self.convert_to_keys(project, assets, output, actions);
                }
            }
            Pick::Rename(path) if is_controller(&path) => {
                self.dialog = Some(Dialog::RenameController {
                    name: store::controller_name(&path).to_owned(),
                    path,
                    error: None,
                });
            }
            Pick::Rename(path) => {
                self.dialog = Some(Dialog::Rename {
                    name: axcenity_anim::files::clip_name(&path).to_owned(),
                    path,
                    error: None,
                });
            }
            Pick::Duplicate(path) if is_controller(&path) => {
                actions.extend(self.write_all(project, output));
                match store::duplicate_controller(project, &path) {
                    Ok(written) => {
                        output.info(format!(
                            "Copied the state machine `{path}` as `{}`.",
                            written.path
                        ));
                        remember(
                            actions,
                            format!("Duplicate {}", store::controller_name(&path)),
                            written.files,
                            None,
                        );
                        let copy = written.path;
                        self.open_controller(project, assets, &copy, None, output);
                    }
                    Err(err) => {
                        output.error(format!("Could not copy the state machine `{path}`: {err}"))
                    }
                }
            }
            Pick::Duplicate(path) => {
                actions.extend(self.write_all(project, output));
                match store::duplicate_clip(project, &path) {
                    Ok(written) => {
                        output.info(format!("Copied the clip `{path}` as `{}`.", written.path));
                        remember(
                            actions,
                            format!("Duplicate {}", axcenity_anim::files::clip_name(&path)),
                            written.files,
                            None,
                        );
                        let copy = written.path;
                        self.open_clip(project, assets, &copy, world, output);
                    }
                    Err(err) => output.error(format!("Could not copy the clip `{path}`: {err}")),
                }
            }
            Pick::Delete(path) => {
                let users = if is_controller(&path) {
                    Vec::new()
                } else {
                    store::controllers_using(project, &path)
                };
                self.dialog = Some(Dialog::Delete { path, users });
            }
        }
    }

    /// The New clip dialog, on the first model's takes.
    pub(super) fn new_clip_dialog(&mut self, assets: &Assets) {
        let mut dialog = Dialog::NewClip {
            model: 0,
            takes: Ok(Vec::new()),
            take: 0,
            name: String::new(),
        };
        load_takes(&mut dialog, &self.files, assets);
        self.dialog = Some(dialog);
    }

    /// The New keyed clip dialog, named after the followed object.
    pub(super) fn new_property_clip_dialog(&mut self, subject: Option<&Subject>) {
        let name = subject
            .map(|s| axcenity_anim::files::file_safe(&s.name))
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "Door".to_owned());
        self.dialog = Some(Dialog::NewPropertyClip { name });
    }

    /// Shows the open dialog, and does what it's answered with.
    #[allow(clippy::too_many_arguments)] // Each is a different input to the studio.
    pub(super) fn ask(
        &mut self,
        ctx: &egui::Context,
        project: &Path,
        assets: &Assets,
        subject: Option<&Subject>,
        world: &mut World,
        output: &mut Output,
        actions: &mut Vec<StudioAction>,
    ) {
        let Some(dialog) = &mut self.dialog else {
            return;
        };
        let mut close = false;
        let mut open = None;
        let mut open_controller = None;
        let mut deleted = None;
        let ((), dismissed) = dialog::modal(ctx, "animation dialog", 380.0, |ui| {
            let error_color = theme::palette(ui.ctx()).error;
            let close_button = [dialog::Button::new(icons::X, "Close", dialog::Kind::Plain)];
            match dialog {
                Dialog::NewClip {
                    model,
                    takes,
                    take,
                    name,
                } => {
                    if self.files.models.is_empty() {
                        dialog::header(
                            ui,
                            icons::FILM_STRIP,
                            dialog::Tone::Question,
                            "New clip",
                            Some("Import an FBX model with animation first."),
                        );
                        close |= dialog::buttons(ui, &close_button, dialog::Enter::Main).is_some();
                        return;
                    }
                    dialog::header(
                        ui,
                        icons::FILM_STRIP,
                        dialog::Tone::Question,
                        "New clip",
                        Some("A take from an imported model, saved as its own clip."),
                    );
                    let before = *model;
                    egui::ComboBox::from_label("Model")
                        .selected_text(self.files.models[*model].rsplit('/').next().unwrap_or(""))
                        .show_ui(ui, |ui| {
                            for (index, path) in self.files.models.iter().enumerate() {
                                ui.selectable_value(
                                    model,
                                    index,
                                    path.rsplit('/').next().unwrap_or(path),
                                );
                            }
                        });
                    if *model != before {
                        *takes = takes_of(&self.files.models[*model], assets);
                        *take = 0;
                        name.clear();
                    }
                    match takes {
                        Ok(list) if !list.is_empty() => {
                            egui::ComboBox::from_label("Take")
                                .selected_text(&list[(*take).min(list.len() - 1)].name)
                                .show_ui(ui, |ui| {
                                    for (index, item) in list.iter().enumerate() {
                                        ui.selectable_value(take, index, &item.name);
                                    }
                                });
                            if name.is_empty() {
                                *name = axcenity_anim::files::file_safe(
                                    list[*take].name.rsplit('|').next().unwrap_or_default(),
                                );
                            }
                            ui.horizontal(|ui| {
                                ui.label("Name");
                                ui.text_edit_singleline(name);
                            });
                            let valid = store::check_name(name);
                            if let Err(problem) = &valid {
                                ui.colored_label(error_color, problem);
                            }
                            let picked = dialog::buttons(
                                ui,
                                &[
                                    dialog::Button::cancel(),
                                    dialog::Button::new(
                                        icons::PLUS,
                                        "Make clip",
                                        dialog::Kind::Main,
                                    )
                                    .enabled(valid.is_ok()),
                                ],
                                dialog::Enter::FromField,
                            );
                            close |= picked == Some(0);
                            if picked == Some(1) {
                                let source = &self.files.models[*model];
                                match store::new_clip(project, source, &list[*take], name.trim()) {
                                    Ok(written) => {
                                        output.info(format!("Made the clip `{}`.", written.path));
                                        remember(
                                            actions,
                                            format!(
                                                "Create {}",
                                                axcenity_anim::files::clip_name(&written.path)
                                            ),
                                            written.files,
                                            None,
                                        );
                                        open = Some(written.path);
                                    }
                                    Err(err) => {
                                        output.error(format!("Could not make the clip: {err}"))
                                    }
                                }
                                close = true;
                            }
                        }
                        Ok(_) => {
                            ui.label("That model has no takes.");
                            close |=
                                dialog::buttons(ui, &close_button, dialog::Enter::Main).is_some();
                        }
                        Err(problem) => {
                            ui.colored_label(error_color, problem.as_str());
                            close |=
                                dialog::buttons(ui, &close_button, dialog::Enter::Main).is_some();
                        }
                    }
                }
                Dialog::NewPropertyClip { name } => {
                    dialog::header(
                        ui,
                        icons::FILM_STRIP,
                        dialog::Tone::Question,
                        "New property clip",
                        Some("Authored Position / Orientation / Size keys for Parts."),
                    );
                    ui.horizontal(|ui| {
                        ui.label("Name");
                        ui.text_edit_singleline(name);
                    });
                    let valid = store::check_name(name);
                    if let Err(problem) = &valid {
                        ui.colored_label(error_color, problem);
                    }
                    let picked = dialog::buttons(
                        ui,
                        &[
                            dialog::Button::cancel(),
                            dialog::Button::new(icons::PLUS, "Make clip", dialog::Kind::Main)
                                .enabled(valid.is_ok()),
                        ],
                        dialog::Enter::FromField,
                    );
                    close |= picked == Some(0);
                    if picked == Some(1) {
                        let file = property_editor::new_property_clip(1.5);
                        match store::create_property_clip(project, name.trim(), &file) {
                            Ok(written) => {
                                output.info(format!("Made the property clip `{}`.", written.path));
                                remember(
                                    actions,
                                    format!(
                                        "Create {}",
                                        axcenity_anim::files::clip_name(&written.path)
                                    ),
                                    written.files,
                                    None,
                                );
                                open = Some(written.path);
                            }
                            Err(err) => output.error(format!("Could not make the clip: {err}")),
                        }
                        close = true;
                    }
                }
                Dialog::NewController {
                    name,
                    with_clips,
                    assign,
                } => {
                    dialog::header(
                        ui,
                        icons::FLOW_ARROW,
                        dialog::Tone::Question,
                        "New state machine",
                        None,
                    );
                    ui.horizontal(|ui| {
                        ui.label("Name");
                        ui.text_edit_singleline(name);
                    });
                    let valid = store::check_name(name);
                    if let Err(problem) = &valid {
                        ui.colored_label(error_color, problem);
                    }
                    let model_clips = subject
                        .map(|subject| subject.own_clips(&self.files))
                        .unwrap_or_default();
                    if let Some(subject) = subject {
                        if !model_clips.is_empty() {
                            ui.checkbox(
                                with_clips,
                                format!(
                                    "Add a state for each of {}'s {} clips",
                                    subject.name,
                                    model_clips.len()
                                ),
                            );
                        }
                        ui.checkbox(assign, format!("Use it for {}'s Animator", subject.name));
                    } else {
                        ui.weak("Select a Part, Model or UI element first to give it this state machine and its clips.");
                    }
                    let picked = dialog::buttons(
                        ui,
                        &[
                            dialog::Button::cancel(),
                            dialog::Button::new(
                                icons::PLUS,
                                "Make state machine",
                                dialog::Kind::Main,
                            )
                            .enabled(valid.is_ok()),
                        ],
                        dialog::Enter::FromField,
                    );
                    close |= picked == Some(0);
                    if picked == Some(1) {
                        let clips = if *with_clips {
                            model_clips.clone()
                        } else {
                            Vec::new()
                        };
                        match store::new_controller(project, name.trim(), &clips) {
                            Ok(written) => {
                                output.info(format!("Made the state machine `{}`.", written.path));
                                remember(
                                    actions,
                                    format!("Create {}", store::controller_name(&written.path)),
                                    written.files,
                                    None,
                                );
                                if let Some(subject) = subject
                                    && *assign
                                {
                                    actions.push(StudioAction::SetController {
                                        part: subject.object,
                                        animator: subject.animator,
                                        path: written.path.clone(),
                                    });
                                }
                                open_controller = Some((
                                    written.path,
                                    subject.and_then(|subject| subject.rigged_model(assets)),
                                ));
                            }
                            Err(err) => {
                                output.error(format!("Could not make the state machine: {err}"))
                            }
                        }
                        close = true;
                    }
                }
                Dialog::Rename { path, name, error } => {
                    dialog::header(
                        ui,
                        icons::PENCIL_SIMPLE,
                        dialog::Tone::Question,
                        "Rename clip",
                        Some("State machines that play it are updated too."),
                    );
                    ui.add(dialog::text_field(name));
                    if let Some(problem) = error {
                        ui.colored_label(error_color, problem.as_str());
                    }
                    let picked = dialog::buttons(
                        ui,
                        &[
                            dialog::Button::cancel(),
                            dialog::Button::new(icons::CHECK, "Rename", dialog::Kind::Main),
                        ],
                        dialog::Enter::FromField,
                    );
                    close |= picked == Some(0);
                    if picked == Some(1) {
                        if let Some(record) =
                            flush(&mut self.clip.doc, &mut self.machine.doc, project, output)
                        {
                            actions.push(record);
                        }
                        match store::rename_clip(project, path, name) {
                            Ok(written) => {
                                output.info(format!("Renamed the clip to `{}`.", written.path));
                                remember(
                                    actions,
                                    format!("Rename {}", axcenity_anim::files::clip_name(path)),
                                    written.files,
                                    None,
                                );
                                if self
                                    .clip
                                    .doc
                                    .as_ref()
                                    .is_some_and(|clip| clip.path() == path.as_str())
                                {
                                    open = Some(written.path);
                                }
                                // The rename rewrote the open controller's file.
                                if let Some(controller) = &mut self.machine.doc
                                    && let Ok(file) =
                                        store::read_controller(project, &controller.path)
                                {
                                    controller.reload(file);
                                }
                                close = true;
                            }
                            Err(problem) => *error = Some(problem),
                        }
                    }
                }
                Dialog::RenameController { path, name, error } => {
                    dialog::header(
                        ui,
                        icons::PENCIL_SIMPLE,
                        dialog::Tone::Question,
                        "Rename state machine",
                        Some("Animators that use it are updated too."),
                    );
                    ui.add(dialog::text_field(name));
                    if let Some(problem) = error {
                        ui.colored_label(error_color, problem.as_str());
                    }
                    let picked = dialog::buttons(
                        ui,
                        &[
                            dialog::Button::cancel(),
                            dialog::Button::new(icons::CHECK, "Rename", dialog::Kind::Main),
                        ],
                        dialog::Enter::FromField,
                    );
                    close |= picked == Some(0);
                    if picked == Some(1) {
                        if let Some(record) =
                            flush(&mut self.clip.doc, &mut self.machine.doc, project, output)
                        {
                            actions.push(record);
                        }
                        let old = path.clone();
                        match store::rename_controller(project, path, name) {
                            Ok(written) => {
                                output.info(format!(
                                    "Renamed the state machine to `{}`.",
                                    written.path
                                ));
                                let rebind = (old.clone(), written.path.clone());
                                remember(
                                    actions,
                                    format!("Rename {}", store::controller_name(&old)),
                                    written.files,
                                    Some(rebind),
                                );
                                if let Some(controller) = &mut self.machine.doc
                                    && controller.path == old
                                {
                                    controller.path = written.path;
                                }
                                close = true;
                            }
                            Err(problem) => *error = Some(problem),
                        }
                    }
                }
                Dialog::Delete { path, users } => {
                    let name = path.rsplit('/').next().unwrap_or(path);
                    dialog::header(
                        ui,
                        icons::TRASH,
                        dialog::Tone::Danger,
                        &format!("Delete {name}?"),
                        Some(&format!("This deletes `{path}`. Undo puts it back.")),
                    );
                    if !users.is_empty() {
                        ui.colored_label(
                            theme::palette(ui.ctx()).warning,
                            format!(
                                "It's played by {}; their states will have no clip.",
                                users.join(", ")
                            ),
                        );
                    }
                    let picked = dialog::buttons(
                        ui,
                        &[
                            dialog::Button::cancel(),
                            dialog::Button::new(icons::TRASH, "Delete", dialog::Kind::Danger),
                        ],
                        dialog::Enter::Main,
                    );
                    close |= picked == Some(0);
                    if picked == Some(1) {
                        if let Some(record) =
                            flush(&mut self.clip.doc, &mut self.machine.doc, project, output)
                        {
                            actions.push(record);
                        }
                        match store::delete_asset(project, path) {
                            Ok(files) => {
                                output.info(format!("Deleted `{path}`."));
                                let label =
                                    if path.ends_with(axcenity_anim::STATE_MACHINE_EXTENSION) {
                                        format!("Delete {}", store::controller_name(path))
                                    } else {
                                        format!("Delete {}", axcenity_anim::files::clip_name(path))
                                    };
                                remember(actions, label, files, None);
                                deleted = Some(path.clone());
                            }
                            Err(err) => output.error(format!("Could not delete `{path}`: {err}")),
                        }
                        close = true;
                    }
                }
            }
        });
        close |= dismissed;
        if let Some(path) = deleted {
            self.close(&path);
        }
        if close {
            self.dialog = None;
            self.rescan(project);
        }
        if let Some(path) = open {
            // A renamed clip is opened fresh from its new file.
            self.clip.doc = None;
            self.open_clip(project, assets, &path, world, output);
            actions.push(StudioAction::Show);
        }
        if let Some((path, model)) = open_controller {
            self.open_controller(project, assets, &path, model, output);
            actions.push(StudioAction::Show);
        }
    }
}
