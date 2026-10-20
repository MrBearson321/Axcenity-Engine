//! The Animation panel (the Animation Studio): the project's clips in a
//! Library on the left, the open clip's preview and timeline in the middle,
//! and its settings on the right.
//!
//! Clips are files (`*.anim.json`), so changes are written as soon as a
//! gesture ends (the mouse is let go and nothing is being typed); they
//! aren't undo steps, like scripts. The preview has its own world, camera
//! and renderer, so it never touches the world being edited.

mod clip_editor;
mod preview;
pub mod store;
mod timeline;

use std::path::Path;
use std::time::{Duration, Instant};

use axcenity_assets::{Assets, Take};
use eframe::egui;
use eframe::egui_wgpu::RenderState;

use crate::output::Output;
use crate::theme::icons;
use clip_editor::ClipDoc;
use preview::Preview;
use store::AnimationFiles;

/// How often the Library looks for new or removed files while shown.
const RESCAN: Duration = Duration::from_secs(2);
/// The preview's longest frame, so a pause doesn't jump it ahead.
const MAX_FRAME_SECONDS: f64 = 0.1;

#[derive(Default)]
pub struct AnimationStudio {
    files: AnimationFiles,
    scanned: Option<Instant>,
    search: String,
    clip: Option<ClipDoc>,
    preview: Preview,
    dialog: Option<Dialog>,
}

/// A question the panel is asking.
enum Dialog {
    /// New clip: from which model file and take, and its name.
    NewClip {
        model: usize,
        takes: Result<Vec<Take>, String>,
        take: usize,
        name: String,
    },
    Rename {
        path: String,
        name: String,
        error: Option<String>,
    },
    /// Delete, naming the controllers that play it.
    Delete { path: String, users: Vec<String> },
}

impl AnimationStudio {
    /// Writes the open clip if it has changes (before Play, on Save).
    pub fn write_all(&mut self, project: &Path, output: &mut Output) {
        if let Some(clip) = &mut self.clip
            && let Err(err) = clip.write(project)
        {
            output.error(format!("Could not save the clip `{}`: {err}", clip.path));
        }
    }

    /// Opens the clip at `path` (from the project folder) in the panel.
    pub fn open_clip(&mut self, project: &Path, assets: &Assets, path: &str, output: &mut Output) {
        self.write_all(project, output);
        self.rescan(project);
        match store::read_clip(project, path) {
            Ok(file) => {
                self.clip = Some(ClipDoc::open(
                    path.to_owned(),
                    file,
                    assets,
                    &self.files.models,
                ));
            }
            Err(err) => output.error(format!("Could not open the clip `{path}`: {err}")),
        }
    }

    fn rescan(&mut self, project: &Path) {
        self.files = store::scan(project);
        self.scanned = Some(Instant::now());
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        project: Option<&Path>,
        assets: &Assets,
        render_state: Option<&RenderState>,
        output: &mut Output,
    ) {
        let Some(project) = project else {
            ui.weak("Open a project to animate its models.");
            return;
        };
        if self.scanned.is_none_or(|at| at.elapsed() >= RESCAN) {
            self.rescan(project);
        }
        egui::SidePanel::left("animation library")
            .resizable(true)
            .default_width(220.0)
            .show_inside(ui, |ui| self.library(ui, project, assets, output));
        if self.clip.is_some() {
            egui::SidePanel::right("animation inspector")
                .resizable(true)
                .default_width(300.0)
                .show_inside(ui, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        let models = self.files.models.clone();
                        let clip = self.clip.as_mut().expect("checked above");
                        if clip.inspector(ui, &models) {
                            self.dialog = Some(Dialog::Rename {
                                path: clip.path.clone(),
                                name: axcenity_anim::files::clip_name(&clip.path).to_owned(),
                                error: None,
                            });
                        }
                    });
                });
        }
        egui::CentralPanel::default().show_inside(ui, |ui| {
            let Some(clip) = &mut self.clip else {
                ui.vertical_centered(|ui| {
                    ui.add_space(40.0);
                    ui.heading(format!("{} Animation Studio", icons::PERSON_SIMPLE_RUN));
                    ui.label("Pick a clip in the Library to preview it and set how it plays.");
                    ui.weak("Import an animated FBX (File > Import Model…) to make clips from its takes.");
                });
                return;
            };
            let dt = f64::from(ui.input(|input| input.unstable_dt)).min(MAX_FRAME_SECONDS);
            clip.tick(dt);
            if clip.playing {
                ui.ctx().request_repaint();
            }
            match clip
                .preview_model
                .as_deref()
                .and_then(|mesh_id| Some((mesh_id, assets.model(mesh_id)?)))
            {
                Some((mesh_id, model)) => self.preview.set_model(mesh_id, &model),
                None => self.preview.clear(),
            }
            let posed = match clip.pose(assets) {
                Ok(posed) => Some(posed),
                Err(problem) => {
                    ui.colored_label(ui.visuals().error_fg_color, problem);
                    None
                }
            };
            let height = (ui.available_height() - 110.0).max(120.0);
            self.preview.show(ui, render_state, assets, posed, height);
            ui.add_space(4.0);
            clip.transport(ui);
        });
        self.ask(ui.ctx(), project, assets, output);

        // Changes are written once the gesture making them ends.
        let busy = ui.input(|input| input.pointer.any_down())
            || ui.ctx().memory(|memory| memory.focused().is_some());
        if !busy {
            self.write_all(project, output);
        }
    }

    /// The clips, by folder, with the buttons that make, rename, copy and
    /// delete them.
    fn library(&mut self, ui: &mut egui::Ui, project: &Path, assets: &Assets, output: &mut Output) {
        ui.horizontal(|ui| {
            ui.strong(format!("{} Library", icons::FILM_STRIP));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .small_button(format!("{} New clip", icons::PLUS))
                    .on_hover_text("Make a clip from a take of a model in the project")
                    .clicked()
                {
                    let mut dialog = Dialog::NewClip {
                        model: 0,
                        takes: Ok(Vec::new()),
                        take: 0,
                        name: String::new(),
                    };
                    load_takes(&mut dialog, &self.files, assets);
                    self.dialog = Some(dialog);
                }
            });
        });
        ui.add(
            egui::TextEdit::singleline(&mut self.search)
                .hint_text(format!("{} Search clips", icons::MAGNIFYING_GLASS))
                .desired_width(f32::INFINITY),
        );
        ui.add_space(4.0);
        let search = self.search.trim().to_lowercase();
        let shown: Vec<String> = self
            .files
            .clips
            .iter()
            .filter(|path| search.is_empty() || path.to_lowercase().contains(&search))
            .cloned()
            .collect();
        if self.files.clips.is_empty() {
            ui.weak("No clips yet. Import an animated FBX, or make one with New clip.");
        }
        let mut open = None;
        let mut action = None;
        egui::ScrollArea::vertical()
            .id_salt("clip list")
            .show(ui, |ui| {
                let mut groups: Vec<(&str, Vec<&String>)> = Vec::new();
                for path in &shown {
                    let group = store::group_of(path);
                    match groups.iter_mut().find(|(name, _)| *name == group) {
                        Some((_, paths)) => paths.push(path),
                        None => groups.push((group, vec![path])),
                    }
                }
                for (group, paths) in groups {
                    let title = if group.is_empty() { "Clips" } else { group };
                    egui::CollapsingHeader::new(format!("{} {title}", icons::FOLDER))
                        .id_salt(("clip group", group))
                        .default_open(true)
                        .show(ui, |ui| {
                            for path in paths {
                                let name = axcenity_anim::files::clip_name(path);
                                let selected = self.clip.as_ref().is_some_and(|c| &c.path == path);
                                let response = ui
                                    .selectable_label(
                                        selected,
                                        format!("{} {name}", icons::FILM_SLATE),
                                    )
                                    .on_hover_text(path.as_str());
                                if response.clicked() && !selected {
                                    open = Some(path.clone());
                                }
                                response.context_menu(|ui| {
                                    if ui
                                        .button(format!("{} Rename", icons::PENCIL_SIMPLE))
                                        .clicked()
                                    {
                                        action = Some(("rename", path.clone()));
                                        ui.close();
                                    }
                                    if ui.button(format!("{} Duplicate", icons::COPY)).clicked() {
                                        action = Some(("duplicate", path.clone()));
                                        ui.close();
                                    }
                                    if ui.button(format!("{} Delete", icons::TRASH)).clicked() {
                                        action = Some(("delete", path.clone()));
                                        ui.close();
                                    }
                                });
                            }
                        });
                }
            });
        if let Some(path) = open {
            self.open_clip(project, assets, &path, output);
        }
        match action {
            Some(("rename", path)) => {
                self.dialog = Some(Dialog::Rename {
                    name: axcenity_anim::files::clip_name(&path).to_owned(),
                    path,
                    error: None,
                });
            }
            Some(("duplicate", path)) => {
                self.write_all(project, output);
                match store::duplicate_clip(project, &path) {
                    Ok(copy) => {
                        output.info(format!("Copied the clip `{path}` as `{copy}`."));
                        self.open_clip(project, assets, &copy, output);
                    }
                    Err(err) => output.error(format!("Could not copy the clip `{path}`: {err}")),
                }
            }
            Some((_, path)) => {
                let users = store::controllers_using(project, &path);
                self.dialog = Some(Dialog::Delete { path, users });
            }
            None => {}
        }
    }

    /// Shows the open dialog, and does what it's answered with.
    fn ask(&mut self, ctx: &egui::Context, project: &Path, assets: &Assets, output: &mut Output) {
        let Some(dialog) = &mut self.dialog else {
            return;
        };
        let mut close = false;
        let mut open = None;
        let modal = egui::Modal::new(egui::Id::new("animation dialog")).show(ctx, |ui| {
            ui.set_width(360.0);
            match dialog {
                Dialog::NewClip {
                    model,
                    takes,
                    take,
                    name,
                } => {
                    ui.heading("New clip");
                    if self.files.models.is_empty() {
                        ui.label("Import an FBX model with animation first.");
                        if ui.button("Close").clicked() {
                            close = true;
                        }
                        return;
                    }
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
                                ui.colored_label(ui.visuals().error_fg_color, problem);
                            }
                            ui.horizontal(|ui| {
                                if ui
                                    .add_enabled(valid.is_ok(), egui::Button::new("Make clip"))
                                    .clicked()
                                {
                                    let source = &self.files.models[*model];
                                    match store::new_clip(
                                        project,
                                        source,
                                        &list[*take],
                                        name.trim(),
                                    ) {
                                        Ok(path) => {
                                            output.info(format!("Made the clip `{path}`."));
                                            open = Some(path);
                                        }
                                        Err(err) => {
                                            output.error(format!("Could not make the clip: {err}"))
                                        }
                                    }
                                    close = true;
                                }
                                if ui.button("Cancel").clicked() {
                                    close = true;
                                }
                            });
                        }
                        Ok(_) => {
                            ui.label("That model has no takes.");
                            if ui.button("Close").clicked() {
                                close = true;
                            }
                        }
                        Err(problem) => {
                            ui.colored_label(ui.visuals().error_fg_color, problem.as_str());
                            if ui.button("Close").clicked() {
                                close = true;
                            }
                        }
                    }
                }
                Dialog::Rename { path, name, error } => {
                    ui.heading("Rename clip");
                    ui.weak("Controllers that play it are updated too.");
                    let response = ui.text_edit_singleline(name);
                    if let Some(problem) = error {
                        ui.colored_label(ui.visuals().error_fg_color, problem.as_str());
                    }
                    let enter =
                        response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    ui.horizontal(|ui| {
                        if ui.button("Rename").clicked() || enter {
                            if let Some(clip) = &mut self.clip
                                && let Err(err) = clip.write(project)
                            {
                                output.error(format!(
                                    "Could not save the clip `{}`: {err}",
                                    clip.path
                                ));
                            }
                            match store::rename_clip(project, path, name) {
                                Ok(new) => {
                                    output.info(format!("Renamed the clip to `{new}`."));
                                    if self.clip.as_ref().is_some_and(|clip| clip.path == *path) {
                                        open = Some(new);
                                    }
                                    close = true;
                                }
                                Err(problem) => *error = Some(problem),
                            }
                        }
                        if ui.button("Cancel").clicked() {
                            close = true;
                        }
                    });
                }
                Dialog::Delete { path, users } => {
                    ui.heading("Delete clip?");
                    ui.label(format!("This deletes `{path}`. It can't be undone."));
                    if !users.is_empty() {
                        ui.colored_label(
                            ui.visuals().warn_fg_color,
                            format!(
                                "It's played by {}; their states will have no clip.",
                                users.join(", ")
                            ),
                        );
                    }
                    ui.horizontal(|ui| {
                        if ui.button(format!("{} Delete", icons::TRASH)).clicked() {
                            match store::delete_clip(project, path) {
                                Ok(()) => {
                                    output.info(format!("Deleted the clip `{path}`."));
                                    if self.clip.as_ref().is_some_and(|clip| clip.path == *path) {
                                        self.clip = None;
                                    }
                                }
                                Err(err) => {
                                    output.error(format!("Could not delete `{path}`: {err}"))
                                }
                            }
                            close = true;
                        }
                        if ui.button("Cancel").clicked() {
                            close = true;
                        }
                    });
                }
            }
        });
        if modal.should_close() {
            close = true;
        }
        if close {
            self.dialog = None;
            self.rescan(project);
        }
        if let Some(path) = open {
            // A renamed clip is opened fresh from its new file.
            self.clip = None;
            self.open_clip(project, assets, &path, output);
        }
    }
}

/// Fills a New clip dialog's takes for its model.
fn load_takes(dialog: &mut Dialog, files: &AnimationFiles, assets: &Assets) {
    if let Dialog::NewClip { model, takes, .. } = dialog
        && let Some(path) = files.models.get(*model)
    {
        *takes = takes_of(path, assets);
    }
}

fn takes_of(path: &str, assets: &Assets) -> Result<Vec<Take>, String> {
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
