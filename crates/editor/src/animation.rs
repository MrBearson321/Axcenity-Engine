//! The Animation Studio, as two dock tabs that each have their own
//! document and preview, so a clip and a controller can be open at once.
//!
//! - The **Animation** tab is for clips, top to bottom: a toolbar (a
//!   searchable clip picker, New, the preview model, playback and Record),
//!   the preview with a drawer listing the model's bones or the clip's
//!   objects, the timeline or dope sheet along the bottom, and the clip's
//!   settings in an inspector that can be hidden.
//! - The **Animator** tab is for controllers: a toolbar with a controller
//!   picker, the state machine graph, its inspector, and a live preview with
//!   the parameters under it.
//!
//! With nothing open, a tab shows a checklist of the steps that get there.
//! Narrower than [`STACK_BELOW`], a tab's parts stack and scroll together.
//!
//! Clips and controllers are files. Timeline and graph edits are written as
//! soon as a gesture ends (the mouse is let go and nothing is being typed);
//! those edits aren't undo steps, like scripts. Creating, renaming,
//! duplicating and deleting a clip or controller is one undo step: the
//! studio reports the file changes and the app records them. The previews
//! have their own world, camera and renderer, so they never touch the world
//! being edited.

mod clip_editor;
mod controller_editor;
mod graph;
mod object_preview;
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
use crate::layout::Pane;
use crate::output::Output;
use crate::responsive;
use crate::theme::{self, icons};
use crate::widgets;
use clip_editor::ClipDoc;
use controller_editor::ControllerDoc;
use object_preview::{ObjectPreview, PreviewEvent};
use preview::Preview;
use property_editor::PropertyDoc;
use store::AnimationFiles;

/// How often the pickers look for new or removed files while shown.
const RESCAN: Duration = Duration::from_secs(2);
/// The preview's longest frame, so a pause doesn't jump it ahead.
const MAX_FRAME_SECONDS: f64 = 0.1;
/// The controller preview's height, under the graph.
const CONTROLLER_PREVIEW: f32 = 240.0;
/// Narrower than this, a tab's parts stack and scroll together.
const STACK_BELOW: f32 = 560.0;
/// The controller graph's height when the tab is stacked.
const STACKED_GRAPH: f32 = 280.0;
/// Narrower than this, a controller's live preview goes above its controls.
const SPLIT_PREVIEW_BELOW: f32 = 420.0;
/// The tallest a picker's list gets before it scrolls.
const PICKER_HEIGHT: f32 = 320.0;
/// What a toolbar's preview-model combo box needs on its row.
pub(crate) const MODEL_PICKER_ROOM: f32 = 170.0;

/// The animated MeshPart selected in the editor (or the one around a
/// selected Animator), for the checklists and New controller.
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

/// Something the studio asks the app to do.
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
    /// Bring a tab to the front (the Animation tab after a clip is made,
    /// the Animator tab after a controller is).
    Show(Pane),
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

/// The clip open in the Animation tab.
enum Clip {
    /// A part of a take in a model file.
    Take(Box<ClipDoc>),
    /// Authored keys for Parts, Models and UI elements.
    Property(Box<PropertyDoc>),
}

impl Clip {
    fn path(&self) -> &str {
        match self {
            Clip::Take(doc) => &doc.path,
            Clip::Property(doc) => &doc.path,
        }
    }
}

/// The Animation tab: a clip, its preview, and which parts are shown.
#[derive(Default)]
struct ClipTab {
    doc: Option<Clip>,
    preview: Preview,
    search: String,
    hide_drawer: bool,
    hide_inspector: bool,
    /// With a property clip open, moving something in the preview keys it.
    record: bool,
    /// A property clip's preview: a copy of what it animates.
    objects: ObjectPreview,
    /// Where the tab was last drawn, and in which pass, so its keys (K,
    /// Delete, Ctrl+C) work while the mouse is over it.
    drawn: Option<(egui::Rect, u64)>,
}

/// The Animator tab: a controller, its live preview, and which parts are shown.
#[derive(Default)]
struct AnimatorTab {
    doc: Option<Box<ControllerDoc>>,
    preview: Preview,
    search: String,
    hide_inspector: bool,
}

#[derive(Default)]
pub struct AnimationStudio {
    files: AnimationFiles,
    scanned: Option<Instant>,
    clip: ClipTab,
    animator: AnimatorTab,
    dialog: Option<Dialog>,
    /// The egui pass in which dialogs were last shown, so a frame that
    /// draws both tabs shows them once.
    finished_pass: Option<u64>,
    /// Each controller's names, for the blocks' menus, read again every
    /// [`RESCAN`].
    names: std::collections::HashMap<String, Vec<(NameKind, String)>>,
    names_read: Option<Instant>,
}

/// A question the studio is asking.
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

/// What a picker or a ⋯ menu asked for, on a clip or controller's path.
enum Pick {
    Open(String),
    Rename(String),
    Duplicate(String),
    Delete(String),
}

/// What the Animation tab's New menu makes.
enum New {
    Clip,
    PropertyClip,
}

/// What a picker lists.
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Clip,
    Controller,
}

impl Kind {
    fn icon(self) -> &'static str {
        match self {
            Kind::Clip => icons::FILM_SLATE,
            Kind::Controller => icons::FLOW_ARROW,
        }
    }

    fn name(self, path: &str) -> &str {
        match self {
            Kind::Clip => axcenity_anim::files::clip_name(path),
            Kind::Controller => store::controller_name(path),
        }
    }
}

/// One line of an empty tab's checklist.
struct Step {
    done: bool,
    title: String,
    hint: &'static str,
    /// A button that does the step.
    button: Option<&'static str>,
}

impl AnimationStudio {
    /// Writes what's open if it has changes (before Play, on Save).
    pub fn write_all(&mut self, project: &Path, output: &mut Output) -> Vec<StudioAction> {
        flush(&mut self.clip.doc, &mut self.animator.doc, project, output)
            .into_iter()
            .collect()
    }

    /// Forgets the file scan so the next time a tab is shown it lists
    /// files undo or redo just put back.
    pub fn refresh(&mut self) {
        self.scanned = None;
    }

    fn clip_path(&self) -> Option<String> {
        self.clip.doc.as_ref().map(|clip| clip.path().to_owned())
    }

    fn controller_path(&self) -> Option<String> {
        self.animator.doc.as_ref().map(|doc| doc.path.clone())
    }

    /// The clip open in the Animation tab and the controller open in the
    /// Animator tab.
    #[cfg(test)]
    pub fn open_documents(&self) -> [Option<String>; 2] {
        [self.clip_path(), self.controller_path()]
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
        for open in [self.clip_path(), self.controller_path()]
            .into_iter()
            .flatten()
        {
            let Some(change) = files.iter().find(|file| file.path == open) else {
                continue;
            };
            if change.after.is_none() {
                let replacement = files.iter().find(|file| {
                    file.path != open
                        && file.before.is_none()
                        && file.after.is_some()
                        && same_asset_kind(&file.path, &open)
                });
                match replacement {
                    Some(file) => {
                        let path = file.path.clone();
                        self.reopen(project, assets, &path, world, output);
                    }
                    None => self.close(&open),
                }
                continue;
            }
            if open.ends_with(axcenity_anim::CONTROLLER_EXTENSION)
                && let Some(doc) = &mut self.animator.doc
                && let Ok(file) = store::read_controller(project, &open)
            {
                doc.reload(file);
                continue;
            }
            self.reopen(project, assets, &open, world, output);
        }
    }

    /// Closes `path` in whichever tab has it open.
    fn close(&mut self, path: &str) {
        if self.clip_path().as_deref() == Some(path) {
            self.clip.doc = None;
        }
        if self.controller_path().as_deref() == Some(path) {
            self.animator.doc = None;
        }
    }

    /// Opens `path` from disk in its tab, without writing what that tab
    /// has open.
    fn reopen(
        &mut self,
        project: &Path,
        assets: &Assets,
        path: &str,
        world: &mut World,
        output: &mut Output,
    ) {
        if path.ends_with(axcenity_anim::CLIP_EXTENSION) {
            self.clip.objects.refresh(world);
            self.load_clip(project, assets, path, output);
        } else if path.ends_with(axcenity_anim::CONTROLLER_EXTENSION) {
            let preview = self
                .animator
                .doc
                .as_ref()
                .and_then(|doc| doc.preview_model.clone());
            self.animator.doc = None;
            match store::read_controller(project, path) {
                Ok(file) => {
                    let mut doc = ControllerDoc::open(path.to_owned(), file, preview);
                    if doc.preview_model.is_none() {
                        doc.preview_model = doc.first_clip_source(project);
                    }
                    self.animator.doc = Some(Box::new(doc));
                }
                Err(err) => output.error(format!("Could not open the controller `{path}`: {err}")),
            }
        }
    }

    /// Reads the clip at `path` into the Animation tab (nothing is open if
    /// it can't be read).
    fn load_clip(&mut self, project: &Path, assets: &Assets, path: &str, output: &mut Output) {
        self.clip.doc = match store::read_any_clip(project, path) {
            Ok(axcenity_anim::AnimClipFile::Take(file)) => Some(Clip::Take(Box::new(
                ClipDoc::open(path.to_owned(), file, assets, &self.files.models),
            ))),
            Ok(axcenity_anim::AnimClipFile::Property(file)) => Some(Clip::Property(Box::new(
                PropertyDoc::open(path.to_owned(), file),
            ))),
            Err(err) => {
                output.error(format!("Could not open the clip `{path}`: {err}"));
                None
            }
        };
    }

    /// Opens the clip at `path` (from the project folder) in the Animation tab.
    pub fn open_clip(
        &mut self,
        project: &Path,
        assets: &Assets,
        path: &str,
        world: &mut World,
        output: &mut Output,
    ) {
        // A fresh copy, with whatever was changed in the world since.
        self.clip.objects.refresh(world);
        let _ = self.write_all(project, output);
        self.rescan(project);
        self.load_clip(project, assets, path, output);
    }

    /// Opens the controller at `path` in the Animator tab, previewed on
    /// `model` (a `MeshId`) if given, or else on its first clip's model.
    pub fn open_controller(
        &mut self,
        project: &Path,
        assets: &Assets,
        path: &str,
        model: Option<String>,
        output: &mut Output,
    ) {
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
                self.animator.doc = Some(Box::new(doc));
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

    fn rescan_if_due(&mut self, project: &Path) {
        if self.scanned.is_none_or(|at| at.elapsed() >= RESCAN) {
            self.rescan(project);
        }
    }

    /// After each tab is drawn: shows the open dialog (once per pass, however
    /// many tabs are drawn), keeps Return settings' transitions in step, and
    /// writes changes once the gesture making them ends. Those two run after
    /// every tab, so an edit made in the tab drawn last is written in the
    /// same frame; they only write what changed, so running twice is free.
    #[allow(clippy::too_many_arguments)] // Each is a different input to the studio.
    fn finish(
        &mut self,
        ui: &egui::Ui,
        project: &Path,
        assets: &Assets,
        selected: Option<&SelectedModel>,
        world: &mut World,
        output: &mut Output,
        actions: &mut Vec<StudioAction>,
    ) {
        let pass = ui.ctx().cumulative_pass_nr();
        if self.finished_pass != Some(pass) {
            self.finished_pass = Some(pass);
            self.ask(ui.ctx(), project, assets, selected, world, output, actions);
        }
        if let Some(controller) = &mut self.animator.doc {
            controller.sync_returns(project);
        }
        let busy = ui.input(|input| input.pointer.any_down())
            || ui.ctx().memory(|memory| memory.focused().is_some());
        if !busy {
            actions.extend(self.write_all(project, output));
        }
    }

    /// Does what a picker or a ⋯ menu asked for.
    #[allow(clippy::too_many_arguments)] // Each is a different input to the studio.
    fn act(
        &mut self,
        pick: Pick,
        project: &Path,
        assets: &Assets,
        selected: Option<&SelectedModel>,
        world: &mut World,
        output: &mut Output,
        actions: &mut Vec<StudioAction>,
    ) {
        let is_controller = |path: &str| path.ends_with(axcenity_anim::CONTROLLER_EXTENSION);
        match pick {
            Pick::Open(path) if is_controller(&path) => {
                if self.controller_path().as_deref() != Some(&path) {
                    let model = selected
                        .filter(|model| model.controller == path)
                        .map(|model| model.mesh_id.clone());
                    self.open_controller(project, assets, &path, model, output);
                }
            }
            Pick::Open(path) => {
                if self.clip_path().as_deref() != Some(&path) {
                    self.open_clip(project, assets, &path, world, output);
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
                        self.open_controller(project, assets, &copy, None, output);
                    }
                    Err(err) => {
                        output.error(format!("Could not copy the controller `{path}`: {err}"))
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
    fn new_clip_dialog(&mut self, assets: &Assets) {
        let mut dialog = Dialog::NewClip {
            model: 0,
            takes: Ok(Vec::new()),
            take: 0,
            name: String::new(),
        };
        load_takes(&mut dialog, &self.files, assets);
        self.dialog = Some(dialog);
    }

    fn new_property_clip_dialog(&mut self) {
        self.dialog = Some(Dialog::NewPropertyClip {
            name: "Door".to_owned(),
        });
    }

    // The Animation tab.

    /// Whether the Animation tab takes K, Delete and Ctrl+C/X/V this frame:
    /// a property clip is open and the mouse is over the tab, as drawn last
    /// pass. The app leaves those keys alone then.
    pub fn wants_keys(&self, ctx: &egui::Context) -> bool {
        let Some((rect, pass)) = self.clip.drawn else {
            return false;
        };
        matches!(self.clip.doc, Some(Clip::Property(_)))
            && ctx.cumulative_pass_nr() <= pass + 1
            && ctx
                .input(|input| input.pointer.latest_pos())
                .is_some_and(|pointer| rect.contains(pointer))
            && !ctx.wants_keyboard_input()
    }

    /// Draws the Animation tab. `selected` is the animated MeshPart selected
    /// in the editor, if any. `preview_root` is the Part, Model, ScreenGui
    /// or UI element selected in the Explorer: a property clip's preview
    /// shows a copy of it (or selects it, if it's inside what's shown).
    #[allow(clippy::too_many_arguments)] // Each is a different input to the tab.
    pub fn show_clips(
        &mut self,
        ui: &mut egui::Ui,
        project: Option<&Path>,
        assets: &Assets,
        render_state: Option<&RenderState>,
        selected: Option<&SelectedModel>,
        world: &mut World,
        preview_root: Option<InstanceId>,
        stylesheets: &[axcenity_ui::Stylesheet],
        output: &mut Output,
    ) -> Vec<StudioAction> {
        let mut actions = Vec::new();
        let Some(project) = project else {
            ui.weak("Open a project to animate its models and Parts.");
            return actions;
        };
        self.clip.drawn = Some((ui.max_rect(), ui.ctx().cumulative_pass_nr()));
        self.rescan_if_due(project);
        self.clip.objects.follow(world, preview_root);
        let stacked = ui.available_width() < STACK_BELOW;
        let (pick, new) = egui::TopBottomPanel::top("clip toolbar")
            .show_inside(ui, |ui| self.clip_toolbar(ui, world, stacked))
            .inner;
        if let Some(pick) = pick {
            self.act(pick, project, assets, selected, world, output, &mut actions);
        }
        match new {
            Some(New::Clip) => self.new_clip_dialog(assets),
            Some(New::PropertyClip) => self.new_property_clip_dialog(),
            None => {}
        }

        let dt = f64::from(ui.input(|input| input.unstable_dt)).min(MAX_FRAME_SECONDS);
        let playing = match &mut self.clip.doc {
            Some(Clip::Take(doc)) => {
                doc.tick(dt);
                doc.playing
            }
            Some(Clip::Property(doc)) => {
                doc.tick(dt);
                doc.playing
            }
            None => false,
        };
        if playing {
            ui.ctx().request_repaint();
        }

        // A property clip poses its copy at the playhead (with moves not
        // yet keyed on top), before anything is drawn.
        let mut problem = None;
        let keys = self.wants_keys(ui.ctx());
        if let Some(Clip::Property(doc)) = &mut self.clip.doc {
            if keys
                && doc.shortcuts(ui)
                && let Some(focus) = self.clip.objects.focus()
            {
                doc.key_focus(&focus);
            }
            match doc.evaluated() {
                Ok(evaluated) => self.clip.objects.pose(&evaluated),
                Err(err) => problem = Some(err),
            }
        }

        let mut rename = false;
        let mut step = None;
        let property = matches!(self.clip.doc, Some(Clip::Property(_)));
        let mut events = Vec::new();
        match self.clip.doc {
            None => {
                egui::CentralPanel::default().show_inside(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("clip checklist")
                        .show(ui, |ui| step = self.clip_checklist(ui, preview_root))
                });
            }
            Some(_) if stacked => {
                egui::CentralPanel::default().show_inside(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("clip stacked")
                        .auto_shrink(false)
                        .show(ui, |ui| {
                            let height = (ui.available_width() * 0.75).clamp(160.0, 320.0);
                            events.extend(self.clip_preview(
                                ui,
                                assets,
                                render_state,
                                stylesheets,
                                problem.as_deref(),
                                height,
                            ));
                            egui::CollapsingHeader::new(self.drawer_title())
                                .id_salt("clip drawer stacked")
                                .icon(widgets::chevron)
                                .show(ui, |ui| events.extend(self.clip_drawer(ui, assets)));
                            ui.separator();
                            self.clip_timeline(ui);
                            ui.separator();
                            egui::CollapsingHeader::new(format!("{} Settings", icons::SLIDERS))
                                .id_salt("clip inspector stacked")
                                .default_open(true)
                                .icon(widgets::chevron)
                                .show(ui, |ui| rename = self.clip_inspector(ui));
                        });
                });
            }
            Some(_) => {
                // The dope sheet grows with its tracks, so it can be resized;
                // the timeline is one strip.
                let bottom = if property {
                    egui::TopBottomPanel::bottom("clip dope sheet")
                        .resizable(true)
                        .default_height(170.0)
                        .min_height(80.0)
                } else {
                    egui::TopBottomPanel::bottom("clip timeline").resizable(false)
                };
                bottom.show_inside(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("clip timeline scroll")
                        .show(ui, |ui| self.clip_timeline(ui))
                });
                if !self.clip.hide_inspector {
                    let room = ui.available_width();
                    egui::SidePanel::right("clip inspector")
                        .resizable(true)
                        .default_width(260.0)
                        .max_width(room * 0.45)
                        .show_inside(ui, |ui| {
                            egui::ScrollArea::vertical()
                                .id_salt("clip inspector scroll")
                                .show(ui, |ui| rename = self.clip_inspector(ui))
                        });
                }
                if !self.clip.hide_drawer {
                    let room = ui.available_width();
                    egui::SidePanel::left("clip drawer")
                        .resizable(true)
                        .default_width(170.0)
                        .max_width(room * 0.4)
                        .show_inside(ui, |ui| {
                            ui.strong(self.drawer_title());
                            egui::ScrollArea::vertical()
                                .id_salt("clip drawer scroll")
                                .show(ui, |ui| events.extend(self.clip_drawer(ui, assets)))
                        });
                }
                egui::CentralPanel::default().show_inside(ui, |ui| {
                    let height = ui.available_height();
                    events.extend(self.clip_preview(
                        ui,
                        assets,
                        render_state,
                        stylesheets,
                        problem.as_deref(),
                        height,
                    ));
                });
            }
        }
        if rename && let Some(path) = self.clip_path() {
            self.dialog = Some(Dialog::Rename {
                name: axcenity_anim::files::clip_name(&path).to_owned(),
                path,
                error: None,
            });
        }
        match step {
            Some(0) => actions.push(StudioAction::ImportModel),
            Some(1) => self.new_clip_dialog(assets),
            Some(2) => self.new_property_clip_dialog(),
            _ => {}
        }

        // What was done in the preview: a pick selects there; a move is
        // keyed with Record on, or shown until the playhead moves.
        if let Some(Clip::Property(doc)) = &mut self.clip.doc {
            for event in events {
                match event {
                    PreviewEvent::Select(id) => {
                        self.clip.objects.selected = id;
                        let focus = self.clip.objects.focus();
                        if let Some(track) =
                            doc.file.tracks.iter().find(|t| {
                                focus.as_ref().is_some_and(|focus| focus.target == t.target)
                            })
                        {
                            doc.selected_track = Some(track.id.clone());
                        }
                    }
                    PreviewEvent::Edit(edits) => doc.edit(edits, self.clip.record),
                }
                ui.ctx().request_repaint();
            }
        }

        self.finish(ui, project, assets, selected, world, output, &mut actions);
        actions
    }

    /// The clip picker, New, then what's open's playback, Record, and the
    /// buttons that show or hide the drawer and the inspector. Returns what
    /// was picked, and what New was asked to make.
    fn clip_toolbar(
        &mut self,
        ui: &mut egui::Ui,
        world: &World,
        stacked: bool,
    ) -> (Option<Pick>, Option<New>) {
        let open = self.clip_path();
        let mut new = None;
        let pick = responsive::toolbar(ui, |ui| {
            let shown = open
                .as_deref()
                .map_or("Pick a clip", axcenity_anim::files::clip_name);
            let mut pick = picker(
                ui,
                Kind::Clip,
                shown,
                &mut self.clip.search,
                &self.files.clips,
                open.as_deref(),
            );
            ui.menu_button(format!("{} New", icons::PLUS), |ui| {
                if ui
                    .button(format!("{} Clip from a take…", icons::FILM_SLATE))
                    .on_hover_text("Make a clip from a take of a model in the project")
                    .clicked()
                {
                    new = Some(New::Clip);
                    ui.close();
                }
                if ui
                    .button(format!("{} Property clip…", icons::CUBE))
                    .on_hover_text(
                        "Make a clip that keys Parts, Models and UI elements (doors, props, menus)",
                    )
                    .clicked()
                {
                    new = Some(New::PropertyClip);
                    ui.close();
                }
            });
            let Some(doc) = &mut self.clip.doc else {
                return pick;
            };
            ui.separator();
            match doc {
                Clip::Take(doc) => doc.controls(ui, &self.files.models),
                Clip::Property(doc) => {
                    doc.controls(ui);
                    responsive::room_for(ui, 140.0);
                    let objects = &mut self.clip.objects;
                    let on = format!(
                        "{} {}",
                        icons::CUBE,
                        objects.root_name().unwrap_or("(select something)")
                    );
                    ui.add(egui::Label::new(egui::RichText::new(on).weak()).truncate())
                        .on_hover_text(
                            "The preview shows a copy of the Part, Model or ScreenGui selected in the Explorer",
                        );
                    if objects.root().is_some()
                        && ui
                            .small_button(icons::ARROWS_CLOCKWISE)
                            .on_hover_text("Copy it again from the world, after editing it there")
                            .clicked()
                    {
                        objects.refresh(world);
                    }
                    let record = &mut self.clip.record;
                    let text = format!("{} Record", icons::RECORD);
                    let text = if *record {
                        egui::RichText::new(text).color(theme::palette(ui.ctx()).error)
                    } else {
                        egui::RichText::new(text)
                    };
                    if ui
                        .selectable_label(*record, text)
                        .on_hover_text(
                            "On, moving something in the preview sets a key at the playhead",
                        )
                        .clicked()
                    {
                        *record = !*record;
                    }
                }
            }
            ui.separator();
            if !stacked {
                let drawer = if matches!(doc, Clip::Take(_)) {
                    "Show or hide the bones"
                } else {
                    "Show or hide the objects"
                };
                if widgets::icon_button(ui, icons::SIDEBAR, drawer, !self.clip.hide_drawer)
                    .clicked()
                {
                    self.clip.hide_drawer = !self.clip.hide_drawer;
                }
                if widgets::icon_button(
                    ui,
                    icons::SLIDERS,
                    "Show or hide the settings",
                    !self.clip.hide_inspector,
                )
                .clicked()
                {
                    self.clip.hide_inspector = !self.clip.hide_inspector;
                }
            }
            if let Some(path) = &open {
                ui.menu_button(icons::DOTS_THREE, |ui| asset_menu(ui, path, &mut pick));
            }
            pick
        });
        (pick, new)
    }

    /// The drawer's heading: bones for a take clip, objects for a property clip.
    fn drawer_title(&self) -> String {
        match &self.clip.doc {
            Some(Clip::Property(_)) => format!("{} Objects", icons::CUBE),
            _ => format!("{} Bones", icons::BONE),
        }
    }

    /// The bones, or the objects and their tracks. Picking an object
    /// selects it in the preview.
    fn clip_drawer(&mut self, ui: &mut egui::Ui, assets: &Assets) -> Option<PreviewEvent> {
        match &mut self.clip.doc {
            Some(Clip::Take(doc)) => {
                doc.bones(ui, assets);
                None
            }
            Some(Clip::Property(doc)) => doc
                .objects(ui, &self.clip.objects)
                .map(|id| PreviewEvent::Select(Some(id))),
            None => None,
        }
    }

    /// The open clip's preview, `height` points tall: the model posed at the
    /// playhead, or for a property clip, the copy of what it animates.
    fn clip_preview(
        &mut self,
        ui: &mut egui::Ui,
        assets: &Assets,
        render_state: Option<&RenderState>,
        stylesheets: &[axcenity_ui::Stylesheet],
        problem: Option<&str>,
        height: f32,
    ) -> Option<PreviewEvent> {
        match &mut self.clip.doc {
            Some(Clip::Take(doc)) => {
                show_model(&mut self.clip.preview, doc.preview_model.as_deref(), assets);
                let posed = match doc.pose(assets) {
                    Ok(posed) => Some(posed),
                    Err(problem) => {
                        ui.colored_label(ui.visuals().error_fg_color, problem);
                        None
                    }
                };
                let height = height.min(ui.available_height()).max(80.0);
                self.clip
                    .preview
                    .show(ui, render_state, assets, posed, height);
                None
            }
            Some(Clip::Property(_)) => {
                let objects = &mut self.clip.objects;
                let top = ui.cursor().top();
                responsive::toolbar(ui, |ui| {
                    objects.tools(ui);
                    if let Some(problem) = problem {
                        ui.colored_label(ui.visuals().error_fg_color, problem);
                    }
                    let missing = objects.missing();
                    if !missing.is_empty() {
                        ui.colored_label(
                            theme::palette(ui.ctx()).warning,
                            format!(
                                "{} Not in this copy: {}",
                                icons::WARNING,
                                missing.join(", ")
                            ),
                        )
                        .on_hover_text("Tracks whose object or property this copy hasn't");
                    }
                });
                let used = ui.cursor().top() - top;
                let height = (height - used).min(ui.available_height()).max(80.0);
                objects.show_ui(ui, render_state, assets, stylesheets, height)
            }
            None => None,
        }
    }

    /// The take clip's timeline, or the property clip's dope sheet.
    fn clip_timeline(&mut self, ui: &mut egui::Ui) {
        match &mut self.clip.doc {
            Some(Clip::Take(doc)) => doc.timeline(ui),
            Some(Clip::Property(doc)) => {
                let focus = self.clip.objects.focus();
                if doc.keys(ui, focus.as_ref())
                    && let Some(focus) = &focus
                {
                    doc.key_focus(focus);
                }
            }
            None => {}
        }
    }

    /// The open clip's settings. Returns true if Rename was pressed.
    fn clip_inspector(&mut self, ui: &mut egui::Ui) -> bool {
        match &mut self.clip.doc {
            Some(Clip::Take(doc)) => doc.inspector(ui),
            Some(Clip::Property(doc)) => doc.inspector(ui, self.clip.objects.root_name()),
            None => false,
        }
    }

    /// With no clip open: the steps to one. Returns the step whose button
    /// was pressed.
    fn clip_checklist(&self, ui: &mut egui::Ui, preview_root: Option<InstanceId>) -> Option<usize> {
        let clips = self.files.clips.len();
        checklist(
            ui,
            &format!("{} Animation", icons::PERSON_SIMPLE_RUN),
            "Open a clip with the picker above, or get one:",
            &[
                Step {
                    done: clips > 0,
                    title: format!("Import an animated FBX ({clips} clips)"),
                    hint: "Its takes become clips (File > Import Model…).",
                    button: Some("Import…"),
                },
                Step {
                    done: false,
                    title: "Make a clip from a take".to_owned(),
                    hint: "A range of a take, with its own speed, loop and events.",
                    button: Some("New clip"),
                },
                Step {
                    done: preview_root.is_some(),
                    title: "Or animate a Part, Model or UI".to_owned(),
                    hint: "Select it, make a property clip, then move it in the preview and key it (K).",
                    button: Some("New property clip"),
                },
            ],
        )
    }

    // The Animator tab.

    /// Draws the Animator tab. `selected` is the animated MeshPart selected
    /// in the editor, if any.
    #[allow(clippy::too_many_arguments)] // Each is a different input to the tab.
    pub fn show_controllers(
        &mut self,
        ui: &mut egui::Ui,
        project: Option<&Path>,
        assets: &Assets,
        render_state: Option<&RenderState>,
        selected: Option<&SelectedModel>,
        world: &mut World,
        output: &mut Output,
    ) -> Vec<StudioAction> {
        let mut actions = Vec::new();
        let Some(project) = project else {
            ui.weak("Open a project to give its models controllers.");
            return actions;
        };
        self.rescan_if_due(project);
        let stacked = ui.available_width() < STACK_BELOW;
        let (pick, new) = egui::TopBottomPanel::top("controller toolbar")
            .show_inside(ui, |ui| self.controller_toolbar(ui, stacked))
            .inner;
        if let Some(pick) = pick {
            self.act(pick, project, assets, selected, world, output, &mut actions);
        }
        if new {
            self.dialog = Some(new_controller_dialog(selected));
        }

        let dt = f64::from(ui.input(|input| input.unstable_dt)).min(MAX_FRAME_SECONDS);
        let mut step = None;
        if self.animator.doc.is_none() {
            egui::CentralPanel::default().show_inside(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("controller checklist")
                    .show(ui, |ui| step = self.controller_checklist(ui, selected))
            });
        } else if stacked {
            egui::CentralPanel::default().show_inside(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("controller stacked")
                    .auto_shrink(false)
                    .show(ui, |ui| {
                        let size = egui::vec2(ui.available_width(), STACKED_GRAPH);
                        ui.allocate_ui(size, |ui| self.controller_graph(ui));
                        ui.add_space(6.0);
                        self.controller_live(ui, assets, render_state, dt);
                        ui.separator();
                        egui::CollapsingHeader::new(format!("{} Settings", icons::SLIDERS))
                            .id_salt("controller inspector stacked")
                            .default_open(true)
                            .icon(widgets::chevron)
                            .show(ui, |ui| self.controller_inspector(ui));
                    });
            });
        } else {
            // Across the whole tab, so the preview has room beside its
            // parameters; the graph keeps at least half the height.
            let room = ui.available_height();
            egui::TopBottomPanel::bottom("controller preview")
                .resizable(true)
                .default_height(CONTROLLER_PREVIEW.min(room * 0.5))
                .min_height(100.0)
                .max_height((room * 0.5).max(100.0))
                .show_inside(ui, |ui| self.controller_live(ui, assets, render_state, dt));
            if !self.animator.hide_inspector {
                let room = ui.available_width();
                egui::SidePanel::right("controller inspector")
                    .resizable(true)
                    .default_width(320.0)
                    .max_width(room * 0.5)
                    .show_inside(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt("controller inspector scroll")
                            .show(ui, |ui| self.controller_inspector(ui))
                    });
            }
            egui::CentralPanel::default().show_inside(ui, |ui| self.controller_graph(ui));
        }
        if self.animator.doc.as_ref().is_some_and(|doc| doc.playing) {
            ui.ctx().request_repaint();
        }
        match step {
            Some(1) => actions.push(StudioAction::ImportModel),
            Some(2) => match selected.filter(|model| !model.controller.is_empty()) {
                Some(model) => {
                    let (path, mesh_id) = (model.controller.clone(), model.mesh_id.clone());
                    self.open_controller(project, assets, &path, Some(mesh_id), output);
                }
                None => self.dialog = Some(new_controller_dialog(selected)),
            },
            Some(4) => actions.push(StudioAction::Play),
            _ => {}
        }

        self.finish(ui, project, assets, selected, world, output, &mut actions);
        actions
    }

    /// The controller picker, New, then what's open's preview model and
    /// playback, and the button that shows or hides the inspector. Returns
    /// what was picked, and whether New was pressed.
    fn controller_toolbar(&mut self, ui: &mut egui::Ui, stacked: bool) -> (Option<Pick>, bool) {
        let open = self.controller_path();
        let mut new = false;
        let pick = responsive::toolbar(ui, |ui| {
            let shown = open
                .as_deref()
                .map_or("Pick a controller", store::controller_name);
            let mut pick = picker(
                ui,
                Kind::Controller,
                shown,
                &mut self.animator.search,
                &self.files.controllers,
                open.as_deref(),
            );
            new = ui
                .button(format!("{} New", icons::PLUS))
                .on_hover_text("Make an animator controller")
                .clicked();
            let Some(doc) = &mut self.animator.doc else {
                return pick;
            };
            ui.separator();
            let shown = doc
                .preview_model
                .as_deref()
                .map_or("(pick a model)", |path| {
                    path.rsplit('/').next().unwrap_or(path)
                });
            let mut picked = doc.preview_model.clone();
            responsive::room_for(ui, MODEL_PICKER_ROOM);
            egui::ComboBox::from_id_salt("controller preview model")
                .selected_text(format!("{} {shown}", icons::CUBE))
                .width(130.0)
                .show_ui(ui, |ui| {
                    for model in &self.files.models {
                        let name = model.rsplit('/').next().unwrap_or(model);
                        ui.selectable_value(&mut picked, Some(model.clone()), name);
                    }
                })
                .response
                .on_hover_text("The model the live preview shows");
            if picked != doc.preview_model {
                doc.preview_model = picked;
                doc.restart();
            }
            let (icon, hover) = if doc.playing {
                (icons::PAUSE, "Pause the preview")
            } else {
                (icons::PLAY, "Play the preview")
            };
            if ui.button(icon).on_hover_text(hover).clicked() {
                doc.playing = !doc.playing;
            }
            if ui
                .button(icons::ARROW_COUNTER_CLOCKWISE)
                .on_hover_text("Start again from the default state")
                .clicked()
            {
                doc.restart();
            }
            ui.separator();
            if !stacked
                && widgets::icon_button(
                    ui,
                    icons::SLIDERS,
                    "Show or hide the inspector",
                    !self.animator.hide_inspector,
                )
                .clicked()
            {
                self.animator.hide_inspector = !self.animator.hide_inspector;
            }
            if let Some(path) = &open {
                ui.menu_button(icons::DOTS_THREE, |ui| asset_menu(ui, path, &mut pick));
            }
            pick
        });
        (pick, new)
    }

    /// The open controller's state machine graph, filling `ui`.
    fn controller_graph(&mut self, ui: &mut egui::Ui) {
        let Some(controller) = &mut self.animator.doc else {
            return;
        };
        let live = controller.live();
        let selected = controller.selected;
        let clips = &self.files.clips;
        let size = ui.available_size();
        let events = ui
            .allocate_ui(size, |ui| {
                controller
                    .view
                    .show(ui, &controller.file, selected, live, clips)
            })
            .inner;
        controller.apply(events);
    }

    /// The live preview beside its parameters, or above them when narrow.
    fn controller_live(
        &mut self,
        ui: &mut egui::Ui,
        assets: &Assets,
        render_state: Option<&RenderState>,
        dt: f64,
    ) {
        let Some(controller) = &mut self.animator.doc else {
            return;
        };
        let preview = &mut self.animator.preview;
        let height = ui.available_height().min(CONTROLLER_PREVIEW);
        let size = egui::vec2(ui.available_width(), height);
        ui.allocate_ui(size, |ui| {
            responsive::split(ui, SPLIT_PREVIEW_BELOW, 0.55, |ui, pane| {
                if pane == 0 {
                    show_model(preview, controller.preview_model.as_deref(), assets);
                    let posed = match controller.tick(assets, dt) {
                        Ok(posed) => Some(posed),
                        Err(problem) => {
                            ui.colored_label(ui.visuals().warn_fg_color, problem);
                            None
                        }
                    };
                    let height = ui.available_height().clamp(40.0, CONTROLLER_PREVIEW);
                    preview.show(ui, render_state, assets, posed, height);
                    return;
                }
                ui.strong("Parameters");
                egui::ScrollArea::vertical()
                    .id_salt("live parameters")
                    .show(ui, |ui| controller.live_controls(ui));
            });
        });
    }

    fn controller_inspector(&mut self, ui: &mut egui::Ui) {
        if let Some(controller) = &mut self.animator.doc {
            controller.inspector(ui, &self.files.clips);
        }
    }

    /// With no controller open: the five Animator steps. Returns the step
    /// whose button was pressed.
    fn controller_checklist(
        &self,
        ui: &mut egui::Ui,
        selected: Option<&SelectedModel>,
    ) -> Option<usize> {
        let clips = self.files.clips.len();
        let has_controller = selected.is_some_and(|model| !model.controller.is_empty());
        checklist(
            ui,
            &format!("{} Animator", icons::FLOW_ARROW),
            "Open a controller with the picker above, or follow the steps:",
            &[
                Step {
                    done: selected.is_some_and(|model| model.animated),
                    title: match selected {
                        Some(model) if model.animated => format!("Model: {}", model.name),
                        _ => "Select a model".to_owned(),
                    },
                    hint: "Select an animated MeshPart (or its Animator) in the Explorer.",
                    button: None,
                },
                Step {
                    done: clips > 0,
                    title: format!("Import an animated FBX ({clips} clips)"),
                    hint: "Its skeleton and clips come with it (File > Import Model…).",
                    button: Some("Import…"),
                },
                Step {
                    done: false,
                    title: if has_controller {
                        "Open its controller".to_owned()
                    } else {
                        "Make a controller".to_owned()
                    },
                    hint: "Its states, transitions and parameters.",
                    button: Some(if has_controller {
                        "Open"
                    } else {
                        "New controller"
                    }),
                },
                Step {
                    done: false,
                    title: "Connect transitions".to_owned(),
                    hint: "Right-click a state in the graph, Make Transition, then click where it goes.",
                    button: None,
                },
                Step {
                    done: false,
                    title: "Test".to_owned(),
                    hint: "Drive the live preview with its parameters, then press Play to see it in the game.",
                    button: Some("Play"),
                },
            ],
        )
    }

    /// Shows the open dialog, and does what it's answered with.
    #[allow(clippy::too_many_arguments)] // Each is a different input to the studio.
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
        let Some(dialog) = &mut self.dialog else {
            return;
        };
        let mut close = false;
        let mut open = None;
        let mut open_controller = None;
        let mut deleted = None;
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
                            if let Some(record) =
                                flush(&mut self.clip.doc, &mut self.animator.doc, project, output)
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
                                    if self.clip.doc.as_ref().is_some_and(|clip| clip.path() == path.as_str()) {
                                        open = Some(written.path);
                                    }
                                    // The rename rewrote the open controller's file.
                                    if let Some(controller) = &mut self.animator.doc
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
                            if let Some(record) =
                                flush(&mut self.clip.doc, &mut self.animator.doc, project, output)
                            {
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
                                    if let Some(controller) = &mut self.animator.doc
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
                            if let Some(record) =
                                flush(&mut self.clip.doc, &mut self.animator.doc, project, output)
                            {
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
                                    deleted = Some(path.clone());
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
            actions.push(StudioAction::Show(Pane::Animation));
        }
        if let Some((path, model)) = open_controller {
            self.open_controller(project, assets, &path, model, output);
            actions.push(StudioAction::Show(Pane::Animator));
        }
    }
}

/// A button naming what's open, which drops down a searchable list of
/// `paths` (clips grouped by folder). Clicking one opens it; its ⋯ menu
/// renames, duplicates or deletes it.
fn picker(
    ui: &mut egui::Ui,
    kind: Kind,
    shown: &str,
    search: &mut String,
    paths: &[String],
    open: Option<&str>,
) -> Option<Pick> {
    // A long name is shortened, so the button fits a narrow panel.
    let shown: String = if shown.chars().count() > 24 {
        shown.chars().take(22).chain(['…']).collect()
    } else {
        shown.to_owned()
    };
    let button = ui
        .button(format!("{} {shown} {}", kind.icon(), icons::CARET_DOWN))
        .on_hover_text(match kind {
            Kind::Clip => "Pick a clip to open",
            Kind::Controller => "Pick a controller to open",
        });
    let mut pick = None;
    egui::Popup::menu(&button)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .width(260.0)
        .show(|ui| {
            let field = ui.add(
                egui::TextEdit::singleline(search)
                    .hint_text(format!("{} Search", icons::MAGNIFYING_GLASS))
                    .desired_width(f32::INFINITY),
            );
            if button.clicked() {
                field.request_focus();
            }
            let needle = search.trim().to_lowercase();
            let matches: Vec<&String> = paths
                .iter()
                .filter(|path| needle.is_empty() || path.to_lowercase().contains(&needle))
                .collect();
            if paths.is_empty() {
                ui.weak(match kind {
                    Kind::Clip => "No clips yet. Import an animated FBX, or make one with New.",
                    Kind::Controller => "No controllers yet: New makes one.",
                });
            } else if matches.is_empty() {
                ui.weak("Nothing matches.");
            }
            // Clips by folder, in the order the folders come.
            let mut groups: Vec<(&str, Vec<&String>)> = Vec::new();
            for path in matches {
                let group = match kind {
                    Kind::Clip => store::group_of(path),
                    Kind::Controller => "",
                };
                match groups.iter_mut().find(|(name, _)| *name == group) {
                    Some((_, paths)) => paths.push(path),
                    None => groups.push((group, vec![path])),
                }
            }
            egui::ScrollArea::vertical()
                .max_height(PICKER_HEIGHT)
                .show(ui, |ui| {
                    for (group, paths) in groups {
                        if kind == Kind::Clip {
                            let title = if group.is_empty() { "Clips" } else { group };
                            ui.weak(format!("{} {title}", icons::FOLDER));
                        }
                        for path in paths {
                            ui.horizontal(|ui| {
                                let row = ui
                                    .selectable_label(
                                        open == Some(path.as_str()),
                                        format!("{} {}", kind.icon(), kind.name(path)),
                                    )
                                    .on_hover_text(path.as_str());
                                if row.clicked() {
                                    pick = Some(Pick::Open(path.clone()));
                                }
                                ui.menu_button(icons::DOTS_THREE, |ui| {
                                    asset_menu(ui, path, &mut pick)
                                });
                            });
                        }
                    }
                });
        });
    if pick.is_some() {
        egui::Popup::close_all(ui.ctx());
    }
    pick
}

/// Rename, Duplicate and Delete for the clip or controller at `path`.
fn asset_menu(ui: &mut egui::Ui, path: &str, pick: &mut Option<Pick>) {
    let path = path.to_owned();
    if ui
        .button(format!("{} Rename", icons::PENCIL_SIMPLE))
        .clicked()
    {
        *pick = Some(Pick::Rename(path));
        ui.close();
    } else if ui.button(format!("{} Duplicate", icons::COPY)).clicked() {
        *pick = Some(Pick::Duplicate(path));
        ui.close();
    } else if ui.button(format!("{} Delete", icons::TRASH)).clicked() {
        *pick = Some(Pick::Delete(path));
        ui.close();
    }
}

/// An empty tab's heading, a line under it, and its steps, each ticked
/// once done and with its button if it has one. Returns the index of the
/// step whose button was pressed.
fn checklist(ui: &mut egui::Ui, heading: &str, intro: &str, steps: &[Step]) -> Option<usize> {
    let palette = theme::palette(ui.ctx());
    let mut pressed = None;
    ui.add_space(8.0);
    ui.heading(heading);
    ui.label(intro);
    ui.add_space(6.0);
    for (index, step) in steps.iter().enumerate() {
        let (mark, color) = if step.done {
            (icons::CHECK_CIRCLE, palette.success)
        } else {
            (icons::CIRCLE, palette.text_weak)
        };
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new(mark).color(color));
            ui.strong(format!("{}. {}", index + 1, step.title));
            if let Some(button) = step.button
                && ui.small_button(button).clicked()
            {
                pressed = Some(index);
            }
        });
        ui.weak(step.hint);
        ui.add_space(4.0);
    }
    pressed
}

/// Writes both tabs' documents, and returns a record of the property-clip
/// edit if that write changed its file. Take-clip and controller edits stay
/// unrecorded.
fn flush(
    clip: &mut Option<Clip>,
    controller: &mut Option<Box<ControllerDoc>>,
    project: &Path,
    output: &mut Output,
) -> Option<StudioAction> {
    let mut record = None;
    if let Some(clip) = clip {
        let result = match clip {
            Clip::Take(doc) => doc.write(project).map(|_| ()),
            Clip::Property(doc) => doc.write(project).map(|written| {
                record = written.map(|(before, after)| StudioAction::RecordFile {
                    path: doc.path.clone(),
                    before,
                    after,
                    label: format!("Edit {}", axcenity_anim::files::clip_name(&doc.path)),
                });
            }),
        };
        if let Err(err) = result {
            output.error(format!("Could not save `{}`: {err}", clip.path()));
        }
    }
    if let Some(doc) = controller
        && let Err(err) = doc.write(project)
    {
        output.error(format!("Could not save `{}`: {err}", doc.path));
    }
    record
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
