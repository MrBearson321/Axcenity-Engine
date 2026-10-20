//! The Animation panel (the Animation Studio): the project's clips and
//! controllers in a Library on the left; in the middle, an open clip's
//! preview and timeline, or an open controller's state machine graph with a
//! live preview; and on the right, the settings of what's open (or selected
//! in the graph). A step bar across the top walks through the five Animator
//! steps.
//!
//! Clips and controllers are files. Timeline and graph edits are written as
//! soon as a gesture ends (the mouse is let go and nothing is being typed);
//! those edits aren't undo steps, like scripts. Creating, renaming,
//! duplicating and deleting a clip or controller is one undo step: the panel
//! reports the file changes and the app records them. The previews have
//! their own world, camera and renderer, so they never touch the world being
//! edited.

mod clip_editor;
mod controller_editor;
mod graph;
mod preview;
mod property_editor;
pub mod store;
mod timeline;

use std::path::Path;
use std::time::{Duration, Instant};

use axcenity_assets::{Assets, Take};
use axcenity_core::{InstanceId, World};
use eframe::egui;
use eframe::egui_wgpu::RenderState;

use crate::block_editor::NameKind;
use crate::output::Output;
use crate::theme::icons;
use clip_editor::ClipDoc;
use controller_editor::ControllerDoc;
use preview::Preview;
use property_editor::PropertyDoc;
use store::AnimationFiles;

/// How often the Library looks for new or removed files while shown.
const RESCAN: Duration = Duration::from_secs(2);
/// The preview's longest frame, so a pause doesn't jump it ahead.
const MAX_FRAME_SECONDS: f64 = 0.1;
/// The controller preview's height, under the graph.
const CONTROLLER_PREVIEW: f32 = 240.0;

/// The animated MeshPart selected in the editor (or the one around a
/// selected Animator), for the step bar and New controller.
#[derive(Clone, Debug, PartialEq)]
pub struct SelectedModel {
    pub part: InstanceId,
    pub name: String,
    pub mesh_id: String,
    /// Its model has a skeleton to move.
    pub animated: bool,
    pub animator: Option<InstanceId>,
    /// Its Animator's `Controller` ("" for none).
    pub controller: String,
}

/// Something the panel asks the app to do.
#[derive(Clone, Debug, PartialEq)]
pub enum StudioAction {
    /// File > Import Model…
    ImportModel,
    /// Give the MeshPart's Animator (made if `None`) this controller, as an
    /// undo step.
    SetController {
        part: InstanceId,
        animator: Option<InstanceId>,
        path: String,
    },
    /// Start Play.
    Play,
    /// Record a property-clip file rewrite for undo.
    RecordFile {
        path: String,
        before: String,
        after: String,
        label: String,
    },
    /// Record a clip or controller create, rename, duplicate or delete.
    /// `rebind_controller` is set when a controller file moved: Animators
    /// whose `Controller` was the old path now use the new one, in the same step.
    RecordFiles {
        label: String,
        files: Vec<store::FileDiff>,
        rebind_controller: Option<(String, String)>,
    },
}

/// What's open in the middle of the panel.
#[derive(Default)]
enum Doc {
    #[default]
    Nothing,
    Clip(Box<ClipDoc>),
    Property(Box<PropertyDoc>),
    Controller(Box<ControllerDoc>),
}

#[derive(Default)]
pub struct AnimationStudio {
    files: AnimationFiles,
    scanned: Option<Instant>,
    search: String,
    doc: Doc,
    preview: Preview,
    dialog: Option<Dialog>,
    /// Each controller's names, for the blocks' menus, read again every
    /// [`RESCAN`].
    names: std::collections::HashMap<String, Vec<(NameKind, String)>>,
    names_read: Option<Instant>,
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
    NewController {
        name: String,
        /// Add a state for each clip of the selected model.
        with_clips: bool,
        /// Give the selected model's Animator the new controller.
        assign: bool,
    },
    /// New authored property clip (doors, props).
    NewPropertyClip { name: String },
    Rename {
        path: String,
        name: String,
        error: Option<String>,
    },
    RenameController {
        path: String,
        name: String,
        error: Option<String>,
    },
    /// Delete a clip or controller; for a clip, the controllers that play it.
    Delete { path: String, users: Vec<String> },
}

impl AnimationStudio {
    /// Writes what's open if it has changes (before Play, on Save).
    pub fn write_all(&mut self, project: &Path, output: &mut Output) -> Vec<StudioAction> {
        let mut actions = Vec::new();
        let result = match &mut self.doc {
            Doc::Nothing => Ok(()),
            Doc::Clip(clip) => clip.write(project).map(|_| ()),
            Doc::Property(clip) => match clip.write(project) {
                Ok(Some((before, after))) => {
                    actions.push(StudioAction::RecordFile {
                        path: clip.path.clone(),
                        before,
                        after,
                        label: format!("Edit {}", axcenity_anim::files::clip_name(&clip.path)),
                    });
                    Ok(())
                }
                Ok(None) => Ok(()),
                Err(err) => Err(err),
            },
            Doc::Controller(controller) => controller.write(project).map(|_| ()),
        };
        if let Err(err) = result {
            output.error(format!(
                "Could not save `{}`: {err}",
                self.open_path().unwrap_or_default()
            ));
        }
        actions
    }

    /// Forgets the library scan so the next time the panel is shown it
    /// lists files undo or redo just put back.
    pub fn refresh(&mut self) {
        self.scanned = None;
    }

    /// Reloads whatever is open after undo or redo wrote `files`. A renamed
    /// file stays open at its other path; a deleted one closes.
    pub fn sync_after_history(
        &mut self,
        project: &Path,
        assets: &Assets,
        world: &mut World,
        files: &[store::FileDiff],
        output: &mut Output,
    ) {
        self.rescan(project);
        let Some(open) = self.open_path() else {
            return;
        };
        let Some(change) = files.iter().find(|file| file.path == open) else {
            return;
        };
        match &change.after {
            None => {
                let replacement = files.iter().find(|file| {
                    file.path != open
                        && file.before.is_none()
                        && file.after.is_some()
                        && same_asset_kind(&file.path, &open)
                });
                if let Some(file) = replacement {
                    let path = file.path.clone();
                    self.reopen(project, assets, &path, world, output);
                } else {
                    self.close_preview(world);
                    self.doc = Doc::Nothing;
                }
            }
            Some(_) => {
                if open.ends_with(axcenity_anim::CONTROLLER_EXTENSION)
                    && let Doc::Controller(doc) = &mut self.doc
                    && let Ok(file) = store::read_controller(project, &open)
                {
                    doc.reload(file);
                    return;
                }
                self.reopen(project, assets, &open, world, output);
            }
        }
    }

    /// Opens `path` from disk without writing the document that's already open.
    fn reopen(
        &mut self,
        project: &Path,
        assets: &Assets,
        path: &str,
        world: &mut World,
        output: &mut Output,
    ) {
        let preview = match &self.doc {
            Doc::Controller(doc) => doc.preview_model.clone(),
            _ => None,
        };
        self.close_preview(world);
        self.doc = Doc::Nothing;
        if path.ends_with(axcenity_anim::CLIP_EXTENSION) {
            match store::read_any_clip(project, path) {
                Ok(axcenity_anim::AnimClipFile::Take(file)) => {
                    self.doc = Doc::Clip(Box::new(ClipDoc::open(
                        path.to_owned(),
                        file,
                        assets,
                        &self.files.models,
                    )));
                }
                Ok(axcenity_anim::AnimClipFile::Property(file)) => {
                    self.doc = Doc::Property(Box::new(PropertyDoc::open(path.to_owned(), file)));
                }
                Err(err) => output.error(format!("Could not open the clip `{path}`: {err}")),
            }
        } else if path.ends_with(axcenity_anim::CONTROLLER_EXTENSION) {
            match store::read_controller(project, path) {
                Ok(file) => {
                    let mut doc = ControllerDoc::open(path.to_owned(), file, preview);
                    if doc.preview_model.is_none() {
                        doc.preview_model = doc.first_clip_source(project);
                    }
                    self.doc = Doc::Controller(Box::new(doc));
                }
                Err(err) => output.error(format!("Could not open the controller `{path}`: {err}")),
            }
        }
    }

    /// Restores property-preview Part transforms when leaving the doc.
    pub fn close_preview(&mut self, world: &mut World) {
        if let Doc::Property(doc) = &mut self.doc {
            doc.restore_baseline(world);
        }
    }

    fn open_path(&self) -> Option<String> {
        match &self.doc {
            Doc::Nothing => None,
            Doc::Clip(clip) => Some(clip.path.clone()),
            Doc::Property(clip) => Some(clip.path.clone()),
            Doc::Controller(controller) => Some(controller.path.clone()),
        }
    }

    /// Opens the clip at `path` (from the project folder) in the panel.
    pub fn open_clip(
        &mut self,
        project: &Path,
        assets: &Assets,
        path: &str,
        world: &mut World,
        output: &mut Output,
    ) {
        self.close_preview(world);
        let _ = self.write_all(project, output);
        self.rescan(project);
        match store::read_any_clip(project, path) {
            Ok(axcenity_anim::AnimClipFile::Take(file)) => {
                self.doc = Doc::Clip(Box::new(ClipDoc::open(
                    path.to_owned(),
                    file,
                    assets,
                    &self.files.models,
                )));
            }
            Ok(axcenity_anim::AnimClipFile::Property(file)) => {
                self.doc = Doc::Property(Box::new(PropertyDoc::open(path.to_owned(), file)));
            }
            Err(err) => output.error(format!("Could not open the clip `{path}`: {err}")),
        }
    }

    /// Opens the controller at `path` in the panel, previewed on `model`
    /// (a `MeshId`) if given, or else on its first clip's model.
    pub fn open_controller(
        &mut self,
        project: &Path,
        assets: &Assets,
        path: &str,
        model: Option<String>,
        world: &mut World,
        output: &mut Output,
    ) {
        self.close_preview(world);
        let _ = self.write_all(project, output);
        self.rescan(project);
        match store::read_controller(project, path) {
            Ok(file) => {
                let mut doc = ControllerDoc::open(path.to_owned(), file, None);
                let animated = |mesh_id: &String| {
                    assets
                        .model(mesh_id)
                        .is_some_and(|model| model.rig.is_some())
                };
                doc.preview_model = model
                    .filter(animated)
                    .or_else(|| doc.first_clip_source(project).filter(animated))
                    .or_else(|| self.files.models.iter().find(|m| animated(m)).cloned());
                self.doc = Doc::Controller(Box::new(doc));
            }
            Err(err) => output.error(format!("Could not open the controller `{path}`: {err}")),
        }
    }

    /// The parameters (by kind), states and clip events of the controller at
    /// `path`, for the animation blocks' menus. Empty if it can't be read.
    pub fn controller_names(&mut self, project: &Path, path: &str) -> Vec<(NameKind, String)> {
        if self.names_read.is_none_or(|at| at.elapsed() >= RESCAN) {
            self.names.clear();
            self.names_read = Some(Instant::now());
        }
        self.names
            .entry(path.to_owned())
            .or_insert_with(|| {
                let Ok(file) = store::read_controller(project, path) else {
                    return Vec::new();
                };
                let mut names: Vec<(NameKind, String)> = file
                    .parameters
                    .iter()
                    .map(|def| {
                        let kind = match def {
                            axcenity_anim::ParameterDef::Bool { .. } => NameKind::Bool,
                            axcenity_anim::ParameterDef::Number { .. } => NameKind::Number,
                            axcenity_anim::ParameterDef::Trigger { .. } => NameKind::Trigger,
                        };
                        (kind, def.name().to_owned())
                    })
                    .collect();
                names.extend(
                    file.states
                        .iter()
                        .map(|s| (NameKind::State, s.name.clone())),
                );
                for clip in file.states.iter().flat_map(|s| s.motion.clips()) {
                    if let Ok(clip) = store::read_clip(project, clip) {
                        for event in clip.events {
                            let entry = (NameKind::Event, event.name);
                            if !names.contains(&entry) {
                                names.push(entry);
                            }
                        }
                    }
                }
                names
            })
            .clone()
    }

    fn rescan(&mut self, project: &Path) {
        self.files = store::scan(project);
        self.scanned = Some(Instant::now());
    }

    /// Draws the panel. `selected` is the animated MeshPart selected in the
    /// editor, if any. `preview_root` is a Part/MeshPart/Model for property
    /// clip scrubbing (restored when the doc closes).
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        project: Option<&Path>,
        assets: &Assets,
        render_state: Option<&RenderState>,
        selected: Option<&SelectedModel>,
        world: &mut World,
        preview_root: Option<InstanceId>,
        output: &mut Output,
    ) -> Vec<StudioAction> {
        let mut actions = Vec::new();
        let Some(project) = project else {
            ui.weak("Open a project to animate its models.");
            return actions;
        };
        if self.scanned.is_none_or(|at| at.elapsed() >= RESCAN) {
            self.rescan(project);
        }
        egui::TopBottomPanel::top("animator steps").show_inside(ui, |ui| {
            self.steps(ui, project, assets, selected, world, output, &mut actions)
        });
        egui::SidePanel::left("animation library")
            .resizable(true)
            .default_width(220.0)
            .show_inside(ui, |ui| {
                self.library(ui, project, assets, selected, world, output, &mut actions)
            });
        if !matches!(self.doc, Doc::Nothing) {
            egui::SidePanel::right("animation inspector")
                .resizable(true)
                .default_width(320.0)
                .show_inside(ui, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        let models = self.files.models.clone();
                        let clips = self.files.clips.clone();
                        match &mut self.doc {
                            Doc::Clip(clip) => {
                                if clip.inspector(ui, &models) {
                                    self.dialog = Some(Dialog::Rename {
                                        path: clip.path.clone(),
                                        name: axcenity_anim::files::clip_name(&clip.path)
                                            .to_owned(),
                                        error: None,
                                    });
                                }
                            }
                            Doc::Property(clip) => {
                                ui.heading("Property clip");
                                ui.label(&clip.path);
                                ui.checkbox(&mut clip.file.looping, "Loop");
                                ui.add(
                                    egui::DragValue::new(&mut clip.file.duration)
                                        .speed(0.05)
                                        .prefix("Duration "),
                                );
                                if preview_root.is_none() {
                                    ui.weak(
                                        "Select a Part in the Explorer to preview and insert keys.",
                                    );
                                }
                            }
                            Doc::Controller(controller) => controller.inspector(ui, &clips),
                            Doc::Nothing => {}
                        }
                    });
                });
        }
        let dt = f64::from(ui.input(|input| input.unstable_dt)).min(MAX_FRAME_SECONDS);
        egui::CentralPanel::default().show_inside(ui, |ui| match &mut self.doc {
            Doc::Nothing => {
                ui.vertical_centered(|ui| {
                    ui.add_space(40.0);
                    ui.heading(format!("{} Animation Studio", icons::PERSON_SIMPLE_RUN));
                    ui.label("Pick a clip in the Library to preview it and set how it plays,");
                    ui.label("or a controller to edit its states and transitions.");
                    ui.weak("Follow the steps above to animate a model from the start.");
                });
            }
            Doc::Property(clip) => {
                clip.tick(dt);
                if clip.playing {
                    ui.ctx().request_repaint();
                }
                ui.heading(axcenity_anim::files::clip_name(&clip.path));
                if clip.ui(ui, world, preview_root) {
                    // Defer write until gesture ends via write_all on seal.
                }
            }
            Doc::Clip(clip) => {
                clip.tick(dt);
                if clip.playing {
                    ui.ctx().request_repaint();
                }
                show_model(&mut self.preview, clip.preview_model.as_deref(), assets);
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
            }
            Doc::Controller(controller) => {
                let graph_height = (ui.available_height() - CONTROLLER_PREVIEW - 8.0).max(160.0);
                let live = controller.live();
                let selected = controller.selected;
                let clips = self.files.clips.clone();
                let events = ui
                    .allocate_ui(egui::vec2(ui.available_width(), graph_height), |ui| {
                        controller
                            .view
                            .show(ui, &controller.file, selected, live, &clips)
                    })
                    .inner;
                controller.apply(events);
                ui.add_space(6.0);
                ui.horizontal_top(|ui| {
                    let width = ui.available_width();
                    ui.allocate_ui(egui::vec2(width * 0.55, CONTROLLER_PREVIEW), |ui| {
                        show_model(
                            &mut self.preview,
                            controller.preview_model.as_deref(),
                            assets,
                        );
                        let posed = match controller.tick(assets, dt) {
                            Ok(posed) => Some(posed),
                            Err(problem) => {
                                ui.colored_label(ui.visuals().warn_fg_color, problem);
                                None
                            }
                        };
                        let height = ui.available_height().max(100.0);
                        self.preview.show(ui, render_state, assets, posed, height);
                    });
                    ui.separator();
                    ui.allocate_ui(egui::vec2(ui.available_width(), CONTROLLER_PREVIEW), |ui| {
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.strong("Preview");
                                let (icon, hover) = if controller.playing {
                                    (icons::PAUSE, "Pause")
                                } else {
                                    (icons::PLAY, "Play")
                                };
                                if ui.small_button(icon).on_hover_text(hover).clicked() {
                                    controller.playing = !controller.playing;
                                }
                                if ui
                                    .small_button(icons::ARROW_COUNTER_CLOCKWISE)
                                    .on_hover_text("Start again from the default state")
                                    .clicked()
                                {
                                    controller.restart();
                                }
                            });
                            let models = self.files.models.clone();
                            let shown = controller
                                .preview_model
                                .as_deref()
                                .map_or("(pick a model)", |path| {
                                    path.rsplit('/').next().unwrap_or(path)
                                });
                            let mut picked = controller.preview_model.clone();
                            egui::ComboBox::from_label("model")
                                .selected_text(shown)
                                .show_ui(ui, |ui| {
                                    for model in &models {
                                        let name = model.rsplit('/').next().unwrap_or(model);
                                        ui.selectable_value(&mut picked, Some(model.clone()), name);
                                    }
                                });
                            if picked != controller.preview_model {
                                controller.preview_model = picked;
                                controller.restart();
                            }
                            ui.separator();
                            egui::ScrollArea::vertical()
                                .id_salt("live parameters")
                                .show(ui, |ui| controller.live_controls(ui));
                        });
                    });
                });
                if controller.playing {
                    ui.ctx().request_repaint();
                }
            }
        });
        self.ask(
            ui.ctx(),
            project,
            assets,
            selected,
            world,
            output,
            &mut actions,
        );
        // Return settings' transitions follow them.
        if let Doc::Controller(controller) = &mut self.doc {
            controller.sync_returns(project);
        }

        // Changes are written once the gesture making them ends.
        let busy = ui.input(|input| input.pointer.any_down())
            || ui.ctx().memory(|memory| memory.focused().is_some());
        if !busy {
            actions.extend(self.write_all(project, output));
        }
        actions
    }

    /// The five Animator steps, each ticked once it's done.
    fn steps(
        &mut self,
        ui: &mut egui::Ui,
        project: &Path,
        assets: &Assets,
        selected: Option<&SelectedModel>,
        world: &mut World,
        output: &mut Output,
        actions: &mut Vec<StudioAction>,
    ) {
        let controller = match &self.doc {
            Doc::Controller(controller) => Some(controller),
            _ => None,
        };
        let model_done = selected.is_some_and(|model| model.animated);
        let clips = self.files.clips.len();
        let transitions = controller.map_or(0, |c| c.transitions());
        let mut open_selected = None;
        ui.horizontal_wrapped(|ui| {
            let step = |ui: &mut egui::Ui, number: u8, done: bool, title: &str, hover: &str| {
                let color = if done {
                    egui::Color32::from_rgb(70, 190, 110)
                } else {
                    ui.visuals().weak_text_color()
                };
                let mark = if done { icons::CHECK_CIRCLE } else { icons::CIRCLE };
                ui.label(egui::RichText::new(format!("{mark} {number}. {title}")).color(color))
                    .on_hover_text(hover);
            };
            step(
                ui,
                1,
                model_done,
                &match selected {
                    Some(model) if model.animated => format!("Model: {}", model.name),
                    _ => "Select a model".to_owned(),
                },
                "Select an animated MeshPart (or its Animator) in the Explorer: the controller source.",
            );
            ui.weak(icons::CARET_RIGHT);
            step(
                ui,
                2,
                clips > 0,
                &format!("Import FBX ({clips} clips)"),
                "Import an FBX with its skeleton and clips (File > Import Model…).",
            );
            if clips == 0 && ui.small_button("Import…").clicked() {
                actions.push(StudioAction::ImportModel);
            }
            ui.weak(icons::CARET_RIGHT);
            step(
                ui,
                3,
                controller.is_some(),
                "Create controller",
                "Make (or open) the animator controller: its states and parameters.",
            );
            if controller.is_none() {
                if let Some(model) = selected
                    && !model.controller.is_empty()
                {
                    if ui.small_button("Open").clicked() {
                        open_selected = Some((model.controller.clone(), model.mesh_id.clone()));
                    }
                } else if ui.small_button("New").clicked() {
                    self.dialog = Some(new_controller_dialog(selected));
                }
            }
            ui.weak(icons::CARET_RIGHT);
            step(
                ui,
                4,
                transitions > 0,
                &format!("Connect transitions ({transitions})"),
                "Right-click a state in the graph, Make Transition, then click where it goes.",
            );
            ui.weak(icons::CARET_RIGHT);
            step(
                ui,
                5,
                false,
                "Test",
                "Drive the preview with its parameters, then press Play to see it in the game.",
            );
            if ui.small_button(format!("{} Play", icons::PLAY)).clicked() {
                actions.push(StudioAction::Play);
            }
        });
        if let Some((path, mesh_id)) = open_selected {
            self.open_controller(project, assets, &path, Some(mesh_id), world, output);
        }
    }

    /// The clips, by folder, and the controllers, with the buttons that
    /// make, rename, copy and delete them.
    fn library(
        &mut self,
        ui: &mut egui::Ui,
        project: &Path,
        assets: &Assets,
        selected: Option<&SelectedModel>,
        world: &mut World,
        output: &mut Output,
        actions: &mut Vec<StudioAction>,
    ) {
        ui.horizontal(|ui| {
            ui.strong(format!("{} Library", icons::FILM_STRIP));
        });
        ui.horizontal(|ui| {
            if ui
                .small_button(format!("{} Clip", icons::PLUS))
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
            if ui
                .small_button(format!("{} Property", icons::PLUS))
                .on_hover_text("Make an authored property clip for Parts (doors, props)")
                .clicked()
            {
                self.dialog = Some(Dialog::NewPropertyClip {
                    name: "Door".to_owned(),
                });
            }
            if ui
                .small_button(format!("{} Controller", icons::PLUS))
                .on_hover_text("Make an animator controller")
                .clicked()
            {
                self.dialog = Some(new_controller_dialog(selected));
            }
        });
        ui.add(
            egui::TextEdit::singleline(&mut self.search)
                .hint_text(format!("{} Search", icons::MAGNIFYING_GLASS))
                .desired_width(f32::INFINITY),
        );
        ui.add_space(4.0);
        let search = self.search.trim().to_lowercase();
        let matches = |path: &&String| search.is_empty() || path.to_lowercase().contains(&search);
        let clips: Vec<String> = self.files.clips.iter().filter(matches).cloned().collect();
        let controllers: Vec<String> = self
            .files
            .controllers
            .iter()
            .filter(matches)
            .cloned()
            .collect();
        let open_path = self.open_path();
        let mut open = None;
        let mut action = None;
        egui::ScrollArea::vertical()
            .id_salt("clip list")
            .show(ui, |ui| {
                egui::CollapsingHeader::new(format!("{} Controllers", icons::FLOW_ARROW))
                    .id_salt("controllers")
                    .default_open(true)
                    .icon(crate::widgets::chevron)
                    .show(ui, |ui| {
                        if self.files.controllers.is_empty() {
                            ui.weak("None yet: + Controller makes one.");
                        }
                        for path in &controllers {
                            let name = path
                                .rsplit('/')
                                .next()
                                .unwrap_or(path)
                                .trim_end_matches(axcenity_anim::CONTROLLER_EXTENSION);
                            let is_open = open_path.as_ref() == Some(path);
                            let response = ui
                                .selectable_label(is_open, format!("{} {name}", icons::FLOW_ARROW))
                                .on_hover_text(path.as_str());
                            if response.clicked() && !is_open {
                                open = Some((true, path.clone()));
                            }
                            response.context_menu(|ui| {
                                if ui
                                    .button(format!("{} Rename", icons::PENCIL_SIMPLE))
                                    .clicked()
                                {
                                    action = Some(("rename-controller", path.clone()));
                                    ui.close();
                                }
                                if ui.button(format!("{} Duplicate", icons::COPY)).clicked() {
                                    action = Some(("duplicate-controller", path.clone()));
                                    ui.close();
                                }
                                if ui.button(format!("{} Delete", icons::TRASH)).clicked() {
                                    action = Some(("delete", path.clone()));
                                    ui.close();
                                }
                            });
                        }
                    });
                if self.files.clips.is_empty() {
                    ui.weak("No clips yet. Import an animated FBX, or make one with + Clip.");
                }
                let mut groups: Vec<(&str, Vec<&String>)> = Vec::new();
                for path in &clips {
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
                        .icon(crate::widgets::chevron)
                        .show(ui, |ui| {
                            for path in paths {
                                let name = axcenity_anim::files::clip_name(path);
                                let is_open = open_path.as_ref() == Some(path);
                                let response = ui
                                    .selectable_label(
                                        is_open,
                                        format!("{} {name}", icons::FILM_SLATE),
                                    )
                                    .on_hover_text(path.as_str());
                                if response.clicked() && !is_open {
                                    open = Some((false, path.clone()));
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
        match open {
            Some((false, path)) => self.open_clip(project, assets, &path, world, output),
            Some((true, path)) => {
                let model = selected
                    .filter(|model| model.controller == path)
                    .map(|model| model.mesh_id.clone());
                self.open_controller(project, assets, &path, model, world, output);
            }
            None => {}
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
            Some(("rename-controller", path)) => {
                self.dialog = Some(Dialog::RenameController {
                    name: store::controller_name(&path).to_owned(),
                    path,
                    error: None,
                });
            }
            Some(("duplicate-controller", path)) => {
                actions.extend(self.write_all(project, output));
                match store::duplicate_controller(project, &path) {
                    Ok(written) => {
                        output.info(format!(
                            "Copied the controller `{path}` as `{}`.",
                            written.path
                        ));
                        remember(
                            actions,
                            format!("Duplicate {}", store::controller_name(&path)),
                            written.files,
                            None,
                        );
                        let copy = written.path;
                        self.open_controller(project, assets, &copy, None, world, output);
                    }
                    Err(err) => {
                        output.error(format!("Could not copy the controller `{path}`: {err}"))
                    }
                }
            }
            Some((_, path)) => {
                let users = if path.ends_with(axcenity_anim::CLIP_EXTENSION) {
                    store::controllers_using(project, &path)
                } else {
                    Vec::new()
                };
                self.dialog = Some(Dialog::Delete { path, users });
            }
            None => {}
        }
    }

    /// Shows the open dialog, and does what it's answered with.
    fn ask(
        &mut self,
        ctx: &egui::Context,
        project: &Path,
        assets: &Assets,
        selected: Option<&SelectedModel>,
        world: &mut World,
        output: &mut Output,
        actions: &mut Vec<StudioAction>,
    ) {
        let open_path = self.open_path();
        let Some(dialog) = &mut self.dialog else {
            return;
        };
        let mut close = false;
        let mut open = None;
        let mut open_controller = None;
        let mut deleted_open = false;
        let modal = egui::Modal::new(egui::Id::new("animation dialog")).show(ctx, |ui| {
            ui.set_width(380.0);
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
                                ui.selectable_value(model, index, path.rsplit('/').next().unwrap_or(path));
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
                                if ui.add_enabled(valid.is_ok(), egui::Button::new("Make clip")).clicked() {
                                    let source = &self.files.models[*model];
                                    match store::new_clip(project, source, &list[*take], name.trim()) {
                                        Ok(written) => {
                                            output.info(format!("Made the clip `{}`.", written.path));
                                            remember(
                                                actions,
                                                format!("Create {}", axcenity_anim::files::clip_name(&written.path)),
                                                written.files,
                                                None,
                                            );
                                            open = Some(written.path);
                                        }
                                        Err(err) => output.error(format!("Could not make the clip: {err}")),
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
                Dialog::NewPropertyClip { name } => {
                    ui.heading("New property clip");
                    ui.label("Authored Position / Orientation / Size keys for Parts.");
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
                            let file = property_editor::new_property_clip(1.5);
                            match store::create_property_clip(project, name.trim(), &file) {
                                Ok(written) => {
                                    output.info(format!(
                                        "Made the property clip `{}`.",
                                        written.path
                                    ));
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
                        if ui.button("Cancel").clicked() {
                            close = true;
                        }
                    });
                }
                Dialog::NewController {
                    name,
                    with_clips,
                    assign,
                } => {
                    ui.heading("New controller");
                    ui.horizontal(|ui| {
                        ui.label("Name");
                        ui.text_edit_singleline(name);
                    });
                    let valid = store::check_name(name);
                    if let Err(problem) = &valid {
                        ui.colored_label(ui.visuals().error_fg_color, problem);
                    }
                    let model_clips = selected
                        .map(|model| store::clips_of_model(&self.files, &model.mesh_id))
                        .unwrap_or_default();
                    if let Some(model) = selected {
                        if !model_clips.is_empty() {
                            ui.checkbox(
                                with_clips,
                                format!("Add a state for each of {}'s {} clips", model.name, model_clips.len()),
                            );
                        }
                        ui.checkbox(assign, format!("Use it for {}'s Animator", model.name));
                    } else {
                        ui.weak("Select an animated MeshPart first to give it this controller and its clips.");
                    }
                    ui.horizontal(|ui| {
                        if ui.add_enabled(valid.is_ok(), egui::Button::new("Make controller")).clicked() {
                            let clips = if *with_clips { model_clips.clone() } else { Vec::new() };
                            match store::new_controller(project, name.trim(), &clips) {
                                Ok(written) => {
                                    output.info(format!("Made the controller `{}`.", written.path));
                                    remember(
                                        actions,
                                        format!("Create {}", store::controller_name(&written.path)),
                                        written.files,
                                        None,
                                    );
                                    if let Some(model) = selected
                                        && *assign
                                    {
                                        actions.push(StudioAction::SetController {
                                            part: model.part,
                                            animator: model.animator,
                                            path: written.path.clone(),
                                        });
                                    }
                                    open_controller = Some((
                                        written.path,
                                        selected.map(|model| model.mesh_id.clone()),
                                    ));
                                }
                                Err(err) => output.error(format!("Could not make the controller: {err}")),
                            }
                            close = true;
                        }
                        if ui.button("Cancel").clicked() {
                            close = true;
                        }
                    });
                }
                Dialog::Rename { path, name, error } => {
                    ui.heading("Rename clip");
                    ui.weak("Controllers that play it are updated too.");
                    let response = ui.text_edit_singleline(name);
                    if let Some(problem) = error {
                        ui.colored_label(ui.visuals().error_fg_color, problem.as_str());
                    }
                    let enter = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    ui.horizontal(|ui| {
                        if ui.button("Rename").clicked() || enter {
                            if let Some(record) = flush_open(&mut self.doc, project, output) {
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
                                    let new = written.path;
                                    match &mut self.doc {
                                        Doc::Clip(clip) if clip.path == *path => open = Some(new),
                                        Doc::Property(clip) if clip.path == *path => open = Some(new),
                                        // The rename rewrote the open controller's file.
                                        Doc::Controller(controller) => {
                                            if let Ok(file) =
                                                store::read_controller(project, &controller.path)
                                            {
                                                controller.reload(file);
                                            }
                                        }
                                        _ => {}
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
                Dialog::RenameController { path, name, error } => {
                    ui.heading("Rename controller");
                    ui.weak("Animators that use it are updated too.");
                    let response = ui.text_edit_singleline(name);
                    if let Some(problem) = error {
                        ui.colored_label(ui.visuals().error_fg_color, problem.as_str());
                    }
                    let enter =
                        response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    ui.horizontal(|ui| {
                        if ui.button("Rename").clicked() || enter {
                            if let Some(record) = flush_open(&mut self.doc, project, output) {
                                actions.push(record);
                            }
                            let old = path.clone();
                            match store::rename_controller(project, path, name) {
                                Ok(written) => {
                                    output.info(format!(
                                        "Renamed the controller to `{}`.",
                                        written.path
                                    ));
                                    let rebind = (old.clone(), written.path.clone());
                                    remember(
                                        actions,
                                        format!("Rename {}", store::controller_name(&old)),
                                        written.files,
                                        Some(rebind),
                                    );
                                    if let Doc::Controller(controller) = &mut self.doc
                                        && controller.path == old
                                    {
                                        controller.path = written.path;
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
                    ui.heading("Delete?");
                    ui.label(format!("This deletes `{path}`. Undo puts it back."));
                    if !users.is_empty() {
                        ui.colored_label(
                            ui.visuals().warn_fg_color,
                            format!("It's played by {}; their states will have no clip.", users.join(", ")),
                        );
                    }
                    ui.horizontal(|ui| {
                        if ui.button(format!("{} Delete", icons::TRASH)).clicked() {
                            if let Some(record) = flush_open(&mut self.doc, project, output) {
                                actions.push(record);
                            }
                            match store::delete_asset(project, path) {
                                Ok(files) => {
                                    output.info(format!("Deleted `{path}`."));
                                    let label = if path.ends_with(axcenity_anim::CONTROLLER_EXTENSION)
                                    {
                                        format!("Delete {}", store::controller_name(path))
                                    } else {
                                        format!("Delete {}", axcenity_anim::files::clip_name(path))
                                    };
                                    remember(actions, label, files, None);
                                    deleted_open = open_path.as_ref() == Some(path);
                                }
                                Err(err) => output.error(format!("Could not delete `{path}`: {err}")),
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
        if deleted_open {
            self.close_preview(world);
            self.doc = Doc::Nothing;
        }
        if close {
            self.dialog = None;
            self.rescan(project);
        }
        if let Some(path) = open {
            // A renamed clip is opened fresh from its new file.
            self.close_preview(world);
            self.doc = Doc::Nothing;
            self.open_clip(project, assets, &path, world, output);
        }
        if let Some((path, model)) = open_controller {
            self.open_controller(project, assets, &path, model, world, output);
        }
    }
}

/// Writes the open document, and records a property-clip edit when that write
/// changed the file. Clip and controller graph edits stay unrecorded.
fn flush_open(doc: &mut Doc, project: &Path, output: &mut Output) -> Option<StudioAction> {
    let result = match doc {
        Doc::Nothing => Ok(None),
        Doc::Clip(clip) => clip.write(project).map(|_| None),
        Doc::Controller(controller) => controller.write(project).map(|_| None),
        Doc::Property(clip) => match clip.write(project) {
            Ok(Some((before, after))) => Ok(Some(StudioAction::RecordFile {
                path: clip.path.clone(),
                before,
                after,
                label: format!("Edit {}", axcenity_anim::files::clip_name(&clip.path)),
            })),
            Ok(None) => Ok(None),
            Err(err) => Err(err),
        },
    };
    match result {
        Ok(action) => action,
        Err(err) => {
            output.error(format!("Could not save before changing the file: {err}"));
            None
        }
    }
}

fn remember(
    actions: &mut Vec<StudioAction>,
    label: String,
    files: Vec<store::FileDiff>,
    rebind_controller: Option<(String, String)>,
) {
    if files.is_empty() {
        return;
    }
    actions.push(StudioAction::RecordFiles {
        label,
        files,
        rebind_controller,
    });
}

fn same_asset_kind(path: &str, other: &str) -> bool {
    let kind = |path: &str| {
        if path.ends_with(axcenity_anim::CLIP_EXTENSION) {
            0
        } else if path.ends_with(axcenity_anim::CONTROLLER_EXTENSION) {
            1
        } else {
            2
        }
    };
    kind(path) == kind(other)
}

/// The New controller dialog, named after the selected model if there is one.
fn new_controller_dialog(selected: Option<&SelectedModel>) -> Dialog {
    Dialog::NewController {
        name: selected.map_or_else(
            || "Controller".to_owned(),
            |model| axcenity_anim::files::file_safe(&model.name),
        ),
        with_clips: true,
        assign: selected.is_some(),
    }
}

/// Shows `mesh_id`'s model in the preview, or nothing.
fn show_model(preview: &mut Preview, mesh_id: Option<&str>, assets: &Assets) {
    match mesh_id.and_then(|mesh_id| Some((mesh_id, assets.model(mesh_id)?))) {
        Some((mesh_id, model)) => preview.set_model(mesh_id, &model),
        None => preview.clear(),
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
