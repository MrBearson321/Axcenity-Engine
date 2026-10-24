//! The Animation window: one dock tab built around the object being
//! animated.
//!
//! It follows the selected Animator, or the Animator of the selected Part,
//! Model or UI element (a [`Subject`]). On its left is an outline of what
//! that Animator has: its state machine, each state and the clip it plays,
//! and its Animations. Clicking the state machine shows its graph, its
//! inspector and a live preview; clicking a clip or an Animation shows the
//! clip's preview, drawer, timeline or dope sheet, and settings. A
//! breadcrumb across the top always names the object and what's being
//! edited (Hero ▸ State Machine ▸ Walk). When the tab is narrow, the outline
//! goes into the breadcrumb's first crumb.
//!
//! A clip opened from the Assets panel opens on its own, on a model picked
//! with "Preview on…". With nothing open, the tab offers Animate (an
//! Animator, a clip and an Animation in one undo step) and Import….
//! Narrower than [`STACK_BELOW`], the clip or state machine's parts stack
//! and scroll together.
//!
//! Clips and state machines are files. Timeline and graph edits are written
//! as soon as a gesture ends (the mouse is let go and nothing is being
//! typed); those edits aren't undo steps, like scripts. Creating, renaming,
//! duplicating and deleting a clip or state machine is one undo step: the
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

use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, Instant};

use axcenity_assets::{Assets, Take};
use axcenity_core::{ClassName, InstanceId, Value, World};
use eframe::egui;
use eframe::egui_wgpu::RenderState;

use crate::block_editor::NameKind;
use crate::output::Output;
use crate::responsive;
use crate::theme::{self, AnimKind, icons};
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
/// The state machine preview's height, under the graph.
const CONTROLLER_PREVIEW: f32 = 240.0;
/// Narrower than this, a clip's or state machine's parts stack and scroll
/// together.
const STACK_BELOW: f32 = 560.0;
/// Narrower than this, the outline goes into the breadcrumb.
const OUTLINE_BELOW: f32 = 720.0;
/// The outline's starting width.
const OUTLINE_WIDTH: f32 = 180.0;
/// The state machine graph's height when the tab is stacked.
const STACKED_GRAPH: f32 = 280.0;
/// Narrower than this, a state machine's live preview goes above its controls.
const SPLIT_PREVIEW_BELOW: f32 = 420.0;
/// The tallest a picker's list gets before it scrolls.
const PICKER_HEIGHT: f32 = 320.0;
/// What a toolbar's preview-model combo box needs on its row.
pub(crate) const MODEL_PICKER_ROOM: f32 = 200.0;

/// Whether an Instance of `class` can be animated: given an Animator, and
/// keyed by a clip.
pub fn can_animate(class: ClassName) -> bool {
    matches!(
        class,
        ClassName::Part
            | ClassName::MeshPart
            | ClassName::Model
            | ClassName::ScreenGui
            | ClassName::Frame
            | ClassName::TextLabel
            | ClassName::TextButton
            | ClassName::TextBox
            | ClassName::ImageLabel
            | ClassName::PageLayout
            | ClassName::Table
    )
}

/// The object `id` stands for in the Animation tab: itself if it can
/// animate, or the parent of an Animator or Animation (an Animation under
/// an Animator stands for the Animator's parent).
pub fn animatable_of(world: &World, id: InstanceId) -> Option<InstanceId> {
    let mut id = id;
    for _ in 0..2 {
        let instance = world.get(id)?;
        match instance.class() {
            ClassName::Animation | ClassName::Animator => id = instance.parent()?,
            class if can_animate(class) => return Some(id),
            _ => return None,
        }
    }
    world
        .get(id)
        .filter(|instance| can_animate(instance.class()))
        .map(|_| id)
}

/// A new keyed clip, as Animate and New ▸ Keyed clip make: 1.5 seconds,
/// with no tracks yet.
pub fn new_keyed_clip() -> axcenity_anim::ClipFile {
    property_editor::new_property_clip(1.5)
}

/// The first Animator in `object`.
pub fn animator_of(world: &World, object: InstanceId) -> Option<InstanceId> {
    world
        .get(object)?
        .children()
        .iter()
        .copied()
        .find(|&child| {
            world
                .get(child)
                .is_some_and(|c| c.class() == ClassName::Animator)
        })
}

/// What the Animation tab follows: an object that can animate, and what
/// its Animator has.
#[derive(Clone, Debug, PartialEq)]
pub struct Subject {
    pub object: InstanceId,
    pub name: String,
    /// A MeshPart's `MeshId`, the model its state machine and take clips
    /// preview on.
    pub mesh_id: Option<String>,
    pub animator: Option<InstanceId>,
    /// Its Animator's `StateMachine` ("" for none).
    pub state_machine: String,
    /// Its Animations: those under its Animator, then those beside it.
    pub animations: Vec<SubjectAnimation>,
}

/// One of a [`Subject`]'s Animations.
#[derive(Clone, Debug, PartialEq)]
pub struct SubjectAnimation {
    pub id: InstanceId,
    pub name: String,
    /// Its `Clip` ("" for none).
    pub clip: String,
}

impl Subject {
    /// `object` and what its Animator has, if it can animate.
    pub fn of(world: &World, object: InstanceId) -> Option<Subject> {
        let instance = world.get(object)?;
        if !can_animate(instance.class()) {
            return None;
        }
        let text = |id: InstanceId, name: &str| {
            world
                .get(id)
                .and_then(|i| i.property(name))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned()
        };
        let animator = animator_of(world, object);
        let mut animations = Vec::new();
        for parent in animator.into_iter().chain([object]) {
            let children = world
                .get(parent)
                .map(|p| p.children().to_vec())
                .unwrap_or_default();
            for child in children {
                if let Some(c) = world.get(child)
                    && c.class() == ClassName::Animation
                {
                    animations.push(SubjectAnimation {
                        id: child,
                        name: c.name().to_owned(),
                        clip: text(child, "Clip"),
                    });
                }
            }
        }
        Some(Subject {
            object,
            name: instance.name().to_owned(),
            mesh_id: (instance.class() == ClassName::MeshPart).then(|| text(object, "MeshId")),
            animator,
            state_machine: animator
                .map(|a| text(a, "StateMachine"))
                .unwrap_or_default(),
            animations,
        })
    }

    /// The `MeshId` of its model, if it's a MeshPart whose model has a
    /// skeleton.
    fn rigged_model(&self, assets: &Assets) -> Option<String> {
        self.mesh_id
            .clone()
            .filter(|id| assets.model(id).is_some_and(|model| model.rig.is_some()))
    }

    /// Its own clips: those in its model's folder for a MeshPart, or
    /// else in its own (see [`Subject::folder`]).
    fn own_clips(&self, files: &AnimationFiles) -> Vec<String> {
        match self.mesh_id.as_deref().filter(|id| !id.is_empty()) {
            Some(mesh_id) => store::clips_of_model(files, mesh_id),
            None => store::clips_in(files, &self.folder()),
        }
    }

    /// The folder Animate puts its clips in, `assets/animations/<name>/`.
    fn folder(&self) -> String {
        format!(
            "{}/{}",
            axcenity_anim::ANIMATIONS_FOLDER,
            axcenity_anim::files::file_safe(&self.name)
        )
    }
}

/// Something the studio asks the app to do.
#[derive(Clone, Debug, PartialEq)]
pub enum StudioAction {
    /// File > Import Model…
    ImportModel,
    /// Animate this object: an Animator if it has none, a new clip and an
    /// Animation playing it, as one undo step, then open it with Record on.
    Animate(InstanceId),
    /// Give the object's Animator (made if `None`) this state machine, as an
    /// undo step.
    SetController {
        part: InstanceId,
        animator: Option<InstanceId>,
        path: String,
    },
    /// Bring the Animation tab to the front.
    Show,
    /// Record a property-clip file rewrite for undo.
    RecordFile {
        path: String,
        before: String,
        after: String,
        label: String,
    },
    /// Record a clip or state machine create, rename, duplicate or delete.
    /// `rebind_controller` is set when a state machine file moved: Animators
    /// whose `StateMachine` was the old path now use the new one, in the
    /// same step.
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

    fn badge(&self) -> Badge {
        match self {
            Clip::Take(doc) => Badge::Linked {
                file: file_name(&doc.file.source).to_owned(),
                take: doc.file.take.clone(),
            },
            Clip::Property(_) => Badge::Keyed,
        }
    }
}

/// How a clip gets its motion, shown beside it.
#[derive(Clone, Debug, PartialEq)]
enum Badge {
    /// Its bones move as a take in a model file does.
    Linked { file: String, take: String },
    /// Only its own keys.
    Keyed,
    /// Its file can't be read.
    Missing,
}

impl Badge {
    fn of(file: &axcenity_anim::ClipFile) -> Badge {
        match &file.link {
            Some(link) => Badge::Linked {
                file: file_name(&link.source).to_owned(),
                take: link.take.clone(),
            },
            None => Badge::Keyed,
        }
    }

    fn text(&self) -> String {
        match self {
            Badge::Linked { file, take } => {
                let take = take.rsplit('|').next().unwrap_or(take);
                format!("{} Linked to {file} · {take}", icons::LINK)
            }
            Badge::Keyed => format!("{} Keyed", icons::KEY),
            Badge::Missing => format!("{} Missing", icons::WARNING),
        }
    }

    /// The badge as a small, weak label that shortens to fit.
    fn show(&self, ui: &mut egui::Ui) {
        let palette = theme::palette(ui.ctx());
        let color = match self {
            Badge::Missing => palette.warning,
            _ => palette.text_weak,
        };
        ui.add(
            egui::Label::new(
                egui::RichText::new(self.text())
                    .size(theme::type_scale::SMALL)
                    .color(color),
            )
            .truncate(),
        );
    }
}

/// What the tab shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum View {
    /// The open clip.
    #[default]
    Clip,
    /// The open state machine.
    StateMachine,
}

/// The open clip, its preview, and which parts are shown.
#[derive(Default)]
struct ClipView {
    doc: Option<Clip>,
    preview: Preview,
    hide_drawer: bool,
    hide_inspector: bool,
    /// With a property clip open, moving something in the preview keys it.
    record: bool,
    /// A property clip's preview: a copy of what it animates.
    objects: ObjectPreview,
    /// Where the clip was last drawn, and in which pass, so its keys (K,
    /// Delete, Ctrl+C) work while the mouse is over it.
    drawn: Option<(egui::Rect, u64)>,
}

/// The open state machine, its live preview, and which parts are shown.
#[derive(Default)]
struct MachineView {
    doc: Option<Box<ControllerDoc>>,
    preview: Preview,
    hide_inspector: bool,
}

#[derive(Default)]
pub struct AnimationStudio {
    files: AnimationFiles,
    scanned: Option<Instant>,
    clip: ClipView,
    machine: MachineView,
    view: View,
    /// The object being followed (see [`Subject`]), if any. A clip opened
    /// from the Assets panel has none.
    subject: Option<InstanceId>,
    /// What the selection stood for when last looked at, so a change of
    /// selection is followed once.
    last_selected: Option<InstanceId>,
    /// The state whose clip was opened by double-clicking it, for the
    /// breadcrumb.
    from_state: Option<String>,
    /// The library picker's search text.
    search: String,
    /// Clips' badges, and state machines' states with their clips, read for
    /// the outline and forgotten at each rescan.
    badges: HashMap<String, Badge>,
    machines: HashMap<String, Vec<(String, Vec<String>)>>,
    dialog: Option<Dialog>,
    /// The egui pass in which dialogs were last shown, so a frame that
    /// draws the tab twice shows them once.
    finished_pass: Option<u64>,
    /// Each state machine's names, for the blocks' menus, read again every
    /// [`RESCAN`].
    names: HashMap<String, Vec<(NameKind, String)>>,
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
        /// Add a state for each clip of the followed object.
        with_clips: bool,
        /// Give the followed object's Animator the new state machine.
        assign: bool,
    },
    /// New keyed clip (doors, props).
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
    /// Delete a clip or state machine; for a clip, the state machines that
    /// play it.
    Delete { path: String, users: Vec<String> },
}

/// What a picker or a ⋯ menu asked for, on a clip or state machine's path.
enum Pick {
    Open(String),
    Rename(String),
    Duplicate(String),
    Delete(String),
}

/// What the New menu makes.
enum New {
    Clip,
    PropertyClip,
    StateMachine,
}

/// Where the outline or the breadcrumb asked to go, or what it asked for.
#[derive(Clone, Debug, PartialEq)]
enum Go {
    /// The followed object's state machine.
    StateMachine,
    /// A state's clip: the state's name and the clip's path.
    State(String, String),
    /// An Animation's clip, by its path.
    Animation(String),
    /// Animate the followed object (again): another clip and Animation.
    Animate,
    ImportModel,
}

/// One step of the breadcrumb.
struct Crumb {
    kind: Option<AnimKind>,
    text: String,
    /// Where clicking it goes, if anywhere.
    go: Option<Go>,
}

/// What a keyed clip's preview shows a copy of.
#[derive(Clone, Copy)]
enum Root {
    /// The object being followed (selecting inside it selects there).
    Followed(InstanceId),
    /// Whatever Part, Model or UI element is selected, for a clip opened on
    /// its own.
    Selected,
}

impl AnimationStudio {
    /// Writes what's open if it has changes (before Play, on Save).
    pub fn write_all(&mut self, project: &Path, output: &mut Output) -> Vec<StudioAction> {
        flush(&mut self.clip.doc, &mut self.machine.doc, project, output)
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
        self.machine.doc.as_ref().map(|doc| doc.path.clone())
    }

    /// The clip and the state machine open in the tab.
    #[cfg(test)]
    pub fn open_documents(&self) -> [Option<String>; 2] {
        [self.clip_path(), self.controller_path()]
    }

    /// What the breadcrumb says, as text (`Door ▸ Door`).
    #[cfg(test)]
    pub fn breadcrumb_text(&self, world: &World) -> String {
        let subject = self.subject.and_then(|id| Subject::of(world, id));
        self.crumbs(subject.as_ref())
            .iter()
            .map(|crumb| crumb.text.as_str())
            .collect::<Vec<_>>()
            .join(" ▸ ")
    }

    /// Whether Record is on.
    #[cfg(test)]
    pub fn recording(&self) -> bool {
        self.clip.record
    }

    /// Shows the state machine, as clicking it in the outline does.
    #[cfg(test)]
    pub fn go_to_state_machine(&mut self) {
        self.view = View::StateMachine;
        self.from_state = None;
    }

    /// Opens a state's clip, as double-clicking the state does.
    #[cfg(test)]
    pub fn open_state(
        &mut self,
        project: &Path,
        assets: &Assets,
        state: &str,
        world: &mut World,
        output: &mut Output,
    ) {
        let clip = self.machine.doc.as_ref().and_then(|doc| {
            let state = doc.file.state(state)?;
            state
                .motion
                .clips()
                .into_iter()
                .next()
                .map(|c| c.to_string())
        });
        if let Some(clip) = clip {
            let go = Go::State(state.to_owned(), clip);
            let mut actions = Vec::new();
            self.go(go, project, assets, world, output, &mut actions);
        }
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
            if open.ends_with(axcenity_anim::STATE_MACHINE_EXTENSION)
                && let Some(doc) = &mut self.machine.doc
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
            self.machine.doc = None;
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
        } else if path.ends_with(axcenity_anim::STATE_MACHINE_EXTENSION) {
            let preview = self
                .machine
                .doc
                .as_ref()
                .and_then(|doc| doc.preview_model.clone());
            self.machine.doc = None;
            match store::read_controller(project, path) {
                Ok(file) => {
                    let mut doc = ControllerDoc::open(path.to_owned(), file, preview);
                    if doc.preview_model.is_none() {
                        doc.preview_model = doc.first_clip_source(project);
                    }
                    self.machine.doc = Some(Box::new(doc));
                }
                Err(err) => {
                    output.error(format!("Could not open the state machine `{path}`: {err}"))
                }
            }
        }
    }

    /// Reads the clip at `path` into the Animation tab (nothing is open if
    /// it can't be read).
    fn load_clip(&mut self, project: &Path, assets: &Assets, path: &str, output: &mut Output) {
        // A linked clip opens on its take; a keyed one in the property editor.
        self.clip.doc = match store::read_any_clip(project, path) {
            Ok(file) if file.link.is_some() => Some(Clip::Take(Box::new(ClipDoc::open(
                path.to_owned(),
                file,
                assets,
                &self.files.models,
            )))),
            Ok(file) => Some(Clip::Property(Box::new(PropertyDoc::open(
                path.to_owned(),
                file,
            )))),
            Err(err) => {
                output.error(format!("Could not open the clip `{path}`: {err}"));
                None
            }
        };
    }

    /// Opens the clip at `path` (from the project folder) and shows it.
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
        self.view = View::Clip;
    }

    /// Opens the clip at `path` on its own, as the Assets panel does: it
    /// follows no object, and previews on the model picked with "Preview
    /// on…" (or on the Part, Model or UI selected, for a keyed clip).
    pub fn open_clip_alone(
        &mut self,
        project: &Path,
        assets: &Assets,
        path: &str,
        world: &mut World,
        output: &mut Output,
    ) {
        self.subject = None;
        self.from_state = None;
        self.clip.objects.forget();
        self.open_clip(project, assets, path, world, output);
    }

    /// After Animate made the clip at `path` for `object`: follows `object`
    /// and opens the clip with Record on.
    pub fn animated(
        &mut self,
        project: &Path,
        assets: &Assets,
        object: InstanceId,
        path: &str,
        world: &mut World,
        output: &mut Output,
    ) {
        self.subject = Some(object);
        self.from_state = None;
        self.open_clip(project, assets, path, world, output);
        self.clip.objects.show_root(world, object);
        self.clip.record = true;
    }

    /// Opens the state machine at `path` and shows it, previewed on `model`
    /// (a `MeshId`) if given, or else on its first clip's model.
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
                self.machine.doc = Some(Box::new(doc));
                self.view = View::StateMachine;
                self.from_state = None;
            }
            Err(err) => output.error(format!("Could not open the state machine `{path}`: {err}")),
        }
    }

    /// Follows the selection. Selecting something that stands for another
    /// object (see [`animatable_of`]) with an Animator opens what that
    /// Animator has: its state machine, or else its first Animation's clip.
    /// Selecting inside the followed object keeps it (so a Door's Hinge can
    /// be picked in its preview). Selecting another object with no Animator
    /// follows it with nothing open, ready for Animate, unless a clip was
    /// opened on its own.
    #[allow(clippy::too_many_arguments)] // Each is a different input to the studio.
    fn follow(
        &mut self,
        project: &Path,
        assets: &Assets,
        world: &mut World,
        selected: Option<InstanceId>,
        output: &mut Output,
        actions: &mut Vec<StudioAction>,
    ) {
        if self.subject.is_some_and(|id| !world.contains(id)) {
            self.subject = None;
        }
        if selected == self.last_selected {
            return;
        }
        self.last_selected = selected;
        let Some(object) = selected.and_then(|id| animatable_of(world, id)) else {
            return;
        };
        let animator = animator_of(world, object);
        if let Some(current) = self.subject
            && (object == current
                || (animator.is_none() && world.is_descendant_of(object, current)))
        {
            // An Animation of the followed object opens its clip.
            let clip = selected
                .and_then(|id| world.get(id))
                .filter(|instance| instance.class() == ClassName::Animation)
                .and_then(|instance| instance.property("Clip").and_then(Value::as_str))
                .filter(|clip| !clip.is_empty())
                .map(str::to_owned);
            if let Some(clip) = clip {
                self.go(Go::Animation(clip), project, assets, world, output, actions);
            }
            return;
        }
        let open = self.clip.doc.is_some() || self.machine.doc.is_some();
        if animator.is_none() && self.subject.is_none() && open {
            return;
        }
        actions.extend(self.write_all(project, output));
        self.subject = Some(object);
        self.from_state = None;
        self.clip.doc = None;
        self.clip.record = false;
        self.machine.doc = None;
        let Some(subject) = Subject::of(world, object) else {
            return;
        };
        if !subject.state_machine.is_empty() {
            self.go(Go::StateMachine, project, assets, world, output, actions);
        } else if let Some(animation) = subject.animations.iter().find(|a| !a.clip.is_empty()) {
            let clip = animation.clip.clone();
            self.go(Go::Animation(clip), project, assets, world, output, actions);
        }
    }

    /// Goes where the outline or the breadcrumb asked.
    fn go(
        &mut self,
        go: Go,
        project: &Path,
        assets: &Assets,
        world: &mut World,
        output: &mut Output,
        actions: &mut Vec<StudioAction>,
    ) {
        let subject = self.subject.and_then(|id| Subject::of(world, id));
        match go {
            Go::StateMachine => {
                let Some(subject) = subject.filter(|s| !s.state_machine.is_empty()) else {
                    return;
                };
                if self.controller_path().as_deref() == Some(subject.state_machine.as_str()) {
                    self.view = View::StateMachine;
                    self.from_state = None;
                } else {
                    let model = subject.rigged_model(assets);
                    self.open_controller(project, assets, &subject.state_machine, model, output);
                }
            }
            Go::State(state, clip) => {
                self.open_subject_clip(project, assets, &clip, subject.as_ref(), world, output);
                self.from_state = Some(state);
            }
            Go::Animation(clip) => {
                self.open_subject_clip(project, assets, &clip, subject.as_ref(), world, output);
                self.from_state = None;
            }
            Go::Animate => {
                if let Some(subject) = subject {
                    actions.push(StudioAction::Animate(subject.object));
                }
            }
            Go::ImportModel => actions.push(StudioAction::ImportModel),
        }
    }

    /// Opens the followed object's clip at `path` (or shows it, if it's
    /// open), on the object's model if its take fits that skeleton.
    fn open_subject_clip(
        &mut self,
        project: &Path,
        assets: &Assets,
        path: &str,
        subject: Option<&Subject>,
        world: &mut World,
        output: &mut Output,
    ) {
        if self.clip_path().as_deref() == Some(path) {
            self.view = View::Clip;
        } else {
            self.open_clip(project, assets, path, world, output);
        }
        if let Some(Clip::Take(doc)) = &mut self.clip.doc
            && let Some(model) = subject.and_then(|s| s.rigged_model(assets))
        {
            doc.prefer_model(assets, &model);
        }
    }

    /// The states of the state machine at `path` and the clips each plays:
    /// the open one's, or as read for the outline.
    fn states_of(&self, path: &str) -> Vec<(String, Vec<String>)> {
        if let Some(doc) = &self.machine.doc
            && doc.path == path
        {
            return doc
                .file
                .states
                .iter()
                .map(|state| (state.name.clone(), motion_clips(&state.motion)))
                .collect();
        }
        self.machines.get(path).cloned().unwrap_or_default()
    }

    /// Reads what the outline shows of `subject` that isn't read yet: its
    /// state machine's states, and each clip's badge.
    fn read_outline(&mut self, project: &Path, subject: &Subject) {
        let path = &subject.state_machine;
        if !path.is_empty() && !self.machines.contains_key(path) {
            let states = store::read_controller(project, path)
                .map(|file| {
                    file.states
                        .iter()
                        .map(|state| (state.name.clone(), motion_clips(&state.motion)))
                        .collect()
                })
                .unwrap_or_default();
            self.machines.insert(path.clone(), states);
        }
        let clips: Vec<String> = self
            .states_of(path)
            .into_iter()
            .flat_map(|(_, clips)| clips)
            .chain(subject.animations.iter().map(|a| a.clip.clone()))
            .filter(|clip| !clip.is_empty())
            .collect();
        for clip in clips {
            self.badges.entry(clip).or_insert_with_key(|clip| {
                store::read_any_clip(project, clip).map_or(Badge::Missing, |file| Badge::of(&file))
            });
        }
    }

    /// The badge of the clip at `path`: the open clip's as it is now, or as
    /// read for the outline.
    fn badge(&self, path: &str) -> Badge {
        match &self.clip.doc {
            Some(clip) if clip.path() == path => clip.badge(),
            _ => self.badges.get(path).cloned().unwrap_or(Badge::Missing),
        }
    }

    /// The breadcrumb: the followed object, then its state machine and the
    /// state or Animation whose clip is open. A clip or state machine that
    /// isn't the followed object's is shown on its own.
    fn crumbs(&self, subject: Option<&Subject>) -> Vec<Crumb> {
        let clip = self.clip_path();
        let machine = self.controller_path();
        let open = match self.view {
            View::Clip => clip.clone().map(|path| (AnimKind::Clip, path)),
            View::StateMachine => machine.clone().map(|path| (AnimKind::StateMachine, path)),
        };
        let alone = |kind: AnimKind, path: &str| {
            let text = match kind {
                AnimKind::StateMachine => store::controller_name(path),
                _ => axcenity_anim::files::clip_name(path),
            };
            Crumb {
                kind: Some(kind),
                text: text.to_owned(),
                go: None,
            }
        };
        let Some(subject) = subject else {
            return vec![match open {
                Some((kind, path)) => alone(kind, &path),
                None => Crumb {
                    kind: None,
                    text: "Animation".to_owned(),
                    go: None,
                },
            }];
        };
        let object = Crumb {
            kind: subject.animator.map(|_| AnimKind::Animator),
            text: subject.name.clone(),
            go: None,
        };
        let state_machine = |go| Crumb {
            kind: Some(AnimKind::StateMachine),
            text: AnimKind::StateMachine.label().to_owned(),
            go,
        };
        let ours = !subject.state_machine.is_empty();
        match open {
            None => vec![object],
            Some((AnimKind::StateMachine, path)) if ours && path == subject.state_machine => {
                vec![object, state_machine(None)]
            }
            Some((AnimKind::StateMachine, path)) => vec![alone(AnimKind::StateMachine, &path)],
            Some((kind, path)) => {
                let states = if ours {
                    self.states_of(&subject.state_machine)
                } else {
                    Vec::new()
                };
                let plays = |state: &str| {
                    states
                        .iter()
                        .any(|(name, clips)| name == state && clips.contains(&path))
                };
                let state_crumb = |state: &str| Crumb {
                    kind: Some(AnimKind::Clip),
                    text: state.to_owned(),
                    go: None,
                };
                if let Some(state) = self.from_state.as_deref().filter(|s| plays(s)) {
                    return vec![
                        object,
                        state_machine(Some(Go::StateMachine)),
                        state_crumb(state),
                    ];
                }
                if let Some(animation) = subject.animations.iter().find(|a| a.clip == path) {
                    let crumb = Crumb {
                        kind: Some(AnimKind::Animation),
                        text: animation.name.clone(),
                        go: None,
                    };
                    return vec![object, crumb];
                }
                if let Some((state, _)) = states.iter().find(|(_, clips)| clips.contains(&path)) {
                    return vec![
                        object,
                        state_machine(Some(Go::StateMachine)),
                        state_crumb(state),
                    ];
                }
                vec![alone(kind, &path)]
            }
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
        self.badges.clear();
        self.machines.clear();
    }

    fn rescan_if_due(&mut self, project: &Path) {
        if self.scanned.is_none_or(|at| at.elapsed() >= RESCAN) {
            self.rescan(project);
        }
    }

    /// After the tab is drawn: shows the open dialog (once per pass, however
    /// often the tab is drawn), keeps Return settings' transitions in step,
    /// and writes changes once the gesture making them ends. Those two only
    /// write what changed, so running twice is free.
    #[allow(clippy::too_many_arguments)] // Each is a different input to the studio.
    fn finish(
        &mut self,
        ui: &egui::Ui,
        project: &Path,
        assets: &Assets,
        subject: Option<&Subject>,
        world: &mut World,
        output: &mut Output,
        actions: &mut Vec<StudioAction>,
    ) {
        let pass = ui.ctx().cumulative_pass_nr();
        if self.finished_pass != Some(pass) {
            self.finished_pass = Some(pass);
            self.ask(ui.ctx(), project, assets, subject, world, output, actions);
        }
        if let Some(controller) = &mut self.machine.doc {
            controller.sync_returns(project);
        }
        let busy = ui.input(|input| input.pointer.any_down())
            || ui.ctx().memory(|memory| memory.focused().is_some());
        if !busy {
            actions.extend(self.write_all(project, output));
        }
    }

    /// Does what a picker or a ⋯ menu asked for.
    fn act(
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

    /// The New keyed clip dialog, named after the followed object.
    fn new_property_clip_dialog(&mut self, subject: Option<&Subject>) {
        let name = subject
            .map(|s| axcenity_anim::files::file_safe(&s.name))
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "Door".to_owned());
        self.dialog = Some(Dialog::NewPropertyClip { name });
    }

    // The tab.

    /// Whether the Animation tab takes K, Delete and Ctrl+C/X/V this frame:
    /// a property clip is shown and the mouse is over it, as drawn last
    /// pass. The app leaves those keys alone then.
    pub fn wants_keys(&self, ctx: &egui::Context) -> bool {
        let Some((rect, pass)) = self.clip.drawn else {
            return false;
        };
        self.view == View::Clip
            && matches!(self.clip.doc, Some(Clip::Property(_)))
            && ctx.cumulative_pass_nr() <= pass + 1
            && ctx
                .input(|input| input.pointer.latest_pos())
                .is_some_and(|pointer| rect.contains(pointer))
            && !ctx.wants_keyboard_input()
    }

    /// Draws the Animation tab. `selected` is the Explorer's primary
    /// selection, which the tab follows (see [`AnimationStudio::follow`]).
    #[allow(clippy::too_many_arguments)] // Each is a different input to the tab.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        project: Option<&Path>,
        assets: &Assets,
        render_state: Option<&RenderState>,
        selected: Option<InstanceId>,
        world: &mut World,
        stylesheets: &[axcenity_ui::Stylesheet],
        output: &mut Output,
    ) -> Vec<StudioAction> {
        let mut actions = Vec::new();
        let Some(project) = project else {
            ui.weak("Open a project to animate its models, Parts and UI.");
            return actions;
        };
        self.rescan_if_due(project);
        self.follow(project, assets, world, selected, output, &mut actions);
        let subject = self.subject.and_then(|id| Subject::of(world, id));
        if let Some(subject) = &subject {
            self.read_outline(project, subject);
        }
        // Show what's open if the view's own document isn't.
        self.view = match self.view {
            View::Clip if self.clip.doc.is_none() && self.machine.doc.is_some() => {
                View::StateMachine
            }
            View::StateMachine if self.machine.doc.is_none() => View::Clip,
            view => view,
        };
        if self.view == View::StateMachine {
            self.clip.drawn = None;
        }

        let narrow = ui.available_width() < OUTLINE_BELOW;
        let (mut go, pick, new) = egui::TopBottomPanel::top("animation breadcrumb")
            .show_inside(ui, |ui| self.breadcrumb(ui, subject.as_ref(), narrow))
            .inner;
        if !narrow && let Some(subject) = &subject {
            let room = ui.available_width();
            let clicked = egui::SidePanel::left("animation outline")
                .resizable(true)
                .default_width(OUTLINE_WIDTH)
                .max_width(room * 0.35)
                .show_inside(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("animation outline scroll")
                        .show(ui, |ui| self.outline(ui, subject))
                        .inner
                })
                .inner;
            go = go.or(clicked);
        }
        if let Some(pick) = pick {
            self.act(pick, project, assets, world, output, &mut actions);
        }
        match new {
            Some(New::Clip) => self.new_clip_dialog(assets),
            Some(New::PropertyClip) => self.new_property_clip_dialog(subject.as_ref()),
            Some(New::StateMachine) => {
                self.dialog = Some(new_controller_dialog(subject.as_ref()));
            }
            None => {}
        }

        // What Animate would animate when nothing is open.
        let animatable = subject
            .as_ref()
            .map(|s| (s.object, s.name.clone()))
            .or_else(|| {
                let id = selected.and_then(|id| animatable_of(world, id))?;
                Some((id, world.get(id)?.name().to_owned()))
            });
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show_inside(ui, |ui| {
                let open = match self.view {
                    View::Clip => self.clip.doc.is_some(),
                    View::StateMachine => self.machine.doc.is_some(),
                };
                if !open {
                    egui::ScrollArea::vertical()
                        .id_salt("animation empty")
                        .show(ui, |ui| {
                            if let Some(asked) = empty_state(ui, animatable.as_ref()) {
                                go = Some(asked);
                            }
                        });
                    return;
                }
                match self.view {
                    View::Clip => {
                        let root = match &subject {
                            Some(subject) => Root::Followed(subject.object),
                            None => Root::Selected,
                        };
                        let select = selected.and_then(|id| animatable_of(world, id));
                        self.clip_body(
                            ui,
                            project,
                            assets,
                            render_state,
                            world,
                            (root, select),
                            stylesheets,
                            output,
                            &mut actions,
                        );
                    }
                    View::StateMachine => {
                        if let Some(asked) = self.machine_body(
                            ui,
                            project,
                            assets,
                            render_state,
                            world,
                            output,
                            &mut actions,
                        ) {
                            go = Some(asked);
                        }
                    }
                }
            });
        if let Some(go) = go {
            // "Animate" with nothing followed animates what's selected.
            if go == Go::Animate && self.subject.is_none() {
                if let Some((object, _)) = animatable {
                    actions.push(StudioAction::Animate(object));
                }
            } else {
                self.go(go, project, assets, world, output, &mut actions);
            }
            ui.ctx().request_repaint();
        }
        self.finish(
            ui,
            project,
            assets,
            subject.as_ref(),
            world,
            output,
            &mut actions,
        );
        actions
    }

    /// The breadcrumb, then the library picker and New. When `narrow`, the
    /// first crumb drops down the outline. Returns where a crumb or the
    /// outline went, what was picked, and what New was asked to make.
    fn breadcrumb(
        &mut self,
        ui: &mut egui::Ui,
        subject: Option<&Subject>,
        narrow: bool,
    ) -> (Option<Go>, Option<Pick>, Option<New>) {
        let crumbs = self.crumbs(subject);
        let mut go = None;
        let mut new = None;
        let pick = responsive::toolbar(ui, |ui| {
            let visuals = ui.visuals().clone();
            let palette = theme::palette(ui.ctx());
            for (index, crumb) in crumbs.iter().enumerate() {
                if index > 0 {
                    ui.label(egui::RichText::new(icons::CARET_RIGHT).color(palette.text_muted));
                }
                let last = index + 1 == crumbs.len();
                let text = egui::RichText::new(&crumb.text);
                let text = if last { text.strong() } else { text };
                if index == 0
                    && narrow
                    && let Some(subject) = subject
                {
                    // The outline, folded into the first crumb.
                    let icon = crumb
                        .kind
                        .map_or_else(|| egui::RichText::new(""), |kind| kind.rich_icon(&visuals));
                    let menu = ui.menu_button((icon, text, icons::CARET_DOWN), |ui| {
                        ui.set_min_width(220.0);
                        let clicked = self.outline(ui, subject);
                        if clicked.is_some() {
                            ui.close();
                        }
                        clicked
                    });
                    if let Some(Some(clicked)) = menu.inner {
                        go = Some(clicked);
                    }
                    menu.response.on_hover_text("What its Animator has");
                    continue;
                }
                if let Some(kind) = crumb.kind {
                    ui.label(kind.rich_icon(&visuals));
                }
                match &crumb.go {
                    Some(target) => {
                        if ui
                            .add(egui::Button::new(text).frame(false))
                            .on_hover_text("Go back up to it")
                            .clicked()
                        {
                            go = Some(target.clone());
                        }
                    }
                    None => {
                        ui.add(egui::Label::new(text).truncate());
                    }
                }
            }
            if let Some(path) = self.clip_path().filter(|_| self.view == View::Clip) {
                self.badge(&path).show(ui);
            }
            ui.separator();
            let open = match self.view {
                View::Clip => self.clip_path(),
                View::StateMachine => self.controller_path(),
            };
            let mut paths = self.files.controllers.clone();
            paths.extend(self.files.clips.iter().cloned());
            let pick = picker(ui, &mut self.search, &paths, open.as_deref());
            ui.menu_button(format!("{} New", icons::PLUS), |ui| {
                let visuals = ui.visuals().clone();
                let item =
                    |kind: AnimKind, text: &str| (kind.rich_icon(&visuals), format!("  {text}"));
                if ui
                    .button(item(AnimKind::Clip, "Keyed clip…"))
                    .on_hover_text(
                        "Make a clip that keys Parts, Models and UI elements (doors, props, menus)",
                    )
                    .clicked()
                {
                    new = Some(New::PropertyClip);
                    ui.close();
                }
                if ui
                    .button(item(AnimKind::Clip, "Clip from a take…"))
                    .on_hover_text("Make a clip from a take of a model in the project")
                    .clicked()
                {
                    new = Some(New::Clip);
                    ui.close();
                }
                if ui
                    .button(item(AnimKind::StateMachine, "State machine…"))
                    .on_hover_text("Make a state machine: states, transitions and parameters")
                    .clicked()
                {
                    new = Some(New::StateMachine);
                    ui.close();
                }
            });
            pick
        });
        (go, pick, new)
    }

    /// The outline of what the followed object's Animator has. Returns
    /// where a click went.
    fn outline(&self, ui: &mut egui::Ui, subject: &Subject) -> Option<Go> {
        let mut go = None;
        let visuals = ui.visuals().clone();
        let row = |ui: &mut egui::Ui, kind: AnimKind, text: &str, selected: bool| {
            ui.add(
                egui::Button::selectable(selected, (kind.rich_icon(&visuals), format!("  {text}")))
                    .truncate(),
            )
        };
        ui.add(egui::Label::new(egui::RichText::new(&subject.name).strong()).truncate());
        let Some(_) = subject.animator else {
            ui.weak("No Animator yet.");
            if ui
                .button((
                    AnimKind::Animator.rich_icon(&visuals),
                    format!("  Animate {}", subject.name),
                ))
                .on_hover_text("Give it an Animator, a clip and an Animation playing it")
                .clicked()
            {
                go = Some(Go::Animate);
            }
            return go;
        };
        ui.horizontal(|ui| {
            ui.label(AnimKind::Animator.rich_icon(&visuals));
            ui.weak(AnimKind::Animator.label());
        });
        let clip = self.clip_path();
        let showing_clip = self.view == View::Clip;
        if !subject.state_machine.is_empty() {
            let open = self.view == View::StateMachine
                && self.controller_path().as_deref() == Some(subject.state_machine.as_str());
            if row(
                ui,
                AnimKind::StateMachine,
                AnimKind::StateMachine.label(),
                open,
            )
            .on_hover_text(&subject.state_machine)
            .clicked()
            {
                go = Some(Go::StateMachine);
            }
            ui.indent("outline states", |ui| {
                for (state, clips) in self.states_of(&subject.state_machine) {
                    let first = clips.first().cloned();
                    let open = showing_clip
                        && first.is_some()
                        && clip == first
                        && self.from_state.as_deref() == Some(state.as_str());
                    let response = row(ui, AnimKind::Clip, &state, open);
                    match &first {
                        Some(path) => {
                            let response = response.on_hover_text(path);
                            self.badge(path).show(ui);
                            if response.clicked() {
                                go = Some(Go::State(state.clone(), path.clone()));
                            }
                        }
                        None => {
                            response.on_hover_text("This state plays no clip");
                        }
                    }
                }
            });
        }
        ui.add_space(4.0);
        ui.weak("Animations");
        for animation in &subject.animations {
            let open = showing_clip
                && !animation.clip.is_empty()
                && clip.as_deref() == Some(animation.clip.as_str())
                && self.from_state.is_none();
            let response = row(ui, AnimKind::Animation, &animation.name, open);
            if animation.clip.is_empty() {
                response.on_hover_text("It has no Clip: pick one in Properties");
                continue;
            }
            let response = response.on_hover_text(&animation.clip);
            ui.indent(("outline animation", animation.id), |ui| {
                self.badge(&animation.clip).show(ui);
            });
            if response.clicked() {
                go = Some(Go::Animation(animation.clip.clone()));
            }
        }
        if subject.animations.is_empty() {
            ui.weak("None yet.");
        }
        if ui
            .button(format!("{} New Animation", icons::PLUS))
            .on_hover_text("Make another clip for it, and an Animation playing it")
            .clicked()
        {
            go = Some(Go::Animate);
        }
        go
    }

    /// The open clip: its toolbar, preview, drawer, timeline or dope sheet,
    /// and settings. `root` says what a property clip's preview shows, with
    /// the object the Explorer selection stands for.
    #[allow(clippy::too_many_arguments)] // Each is a different input to the tab.
    fn clip_body(
        &mut self,
        ui: &mut egui::Ui,
        project: &Path,
        assets: &Assets,
        render_state: Option<&RenderState>,
        world: &mut World,
        (root, select): (Root, Option<InstanceId>),
        stylesheets: &[axcenity_ui::Stylesheet],
        output: &mut Output,
        actions: &mut Vec<StudioAction>,
    ) {
        self.clip.drawn = Some((ui.max_rect(), ui.ctx().cumulative_pass_nr()));
        match root {
            Root::Followed(object) => self.clip.objects.follow_within(world, object, select),
            Root::Selected => self.clip.objects.follow(world, select),
        }
        let stacked = ui.available_width() < STACK_BELOW;
        let pick = egui::TopBottomPanel::top("clip toolbar")
            .show_inside(ui, |ui| self.clip_toolbar(ui, world, stacked, root))
            .inner;
        if let Some(pick) = pick {
            self.act(pick, project, assets, world, output, actions);
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
        let property = matches!(self.clip.doc, Some(Clip::Property(_)));
        let mut events = Vec::new();
        if stacked {
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
        } else {
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
        if rename && let Some(path) = self.clip_path() {
            self.dialog = Some(Dialog::Rename {
                name: axcenity_anim::files::clip_name(&path).to_owned(),
                path,
                error: None,
            });
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
    }

    /// What's open's playback, the preview model ("Preview on…") or the
    /// copy shown, Record, the buttons that show or hide the drawer and the
    /// inspector, and the ⋯ menu. Returns what the ⋯ menu asked for.
    fn clip_toolbar(
        &mut self,
        ui: &mut egui::Ui,
        world: &World,
        stacked: bool,
        root: Root,
    ) -> Option<Pick> {
        let open = self.clip_path();
        responsive::toolbar(ui, |ui| {
            let mut pick = None;
            let Some(doc) = &mut self.clip.doc else {
                return pick;
            };
            match doc {
                Clip::Take(doc) => doc.controls(ui, &self.files.models),
                Clip::Property(doc) => {
                    doc.controls(ui);
                    responsive::room_for(ui, 140.0);
                    let objects = &mut self.clip.objects;
                    let on = format!(
                        "{} Preview on {}",
                        icons::CUBE,
                        objects.root_name().unwrap_or("(select something)")
                    );
                    let hover = match root {
                        Root::Followed(_) => {
                            "The preview shows a copy of the object being animated"
                        }
                        Root::Selected => {
                            "The preview shows a copy of the Part, Model or ScreenGui selected in the Explorer"
                        }
                    };
                    ui.add(egui::Label::new(egui::RichText::new(on).weak()).truncate())
                        .on_hover_text(hover);
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
        })
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

    /// The open state machine: its toolbar, graph, inspector and live
    /// preview. Returns where double-clicking a state went (its clip).
    #[allow(clippy::too_many_arguments)] // Each is a different input to the studio.
    fn machine_body(
        &mut self,
        ui: &mut egui::Ui,
        project: &Path,
        assets: &Assets,
        render_state: Option<&RenderState>,
        world: &mut World,
        output: &mut Output,
        actions: &mut Vec<StudioAction>,
    ) -> Option<Go> {
        let stacked = ui.available_width() < STACK_BELOW;
        let pick = egui::TopBottomPanel::top("state machine toolbar")
            .show_inside(ui, |ui| self.controller_toolbar(ui, stacked))
            .inner;
        if let Some(pick) = pick {
            self.act(pick, project, assets, world, output, actions);
        }

        let dt = f64::from(ui.input(|input| input.unstable_dt)).min(MAX_FRAME_SECONDS);
        let mut go = None;
        if stacked {
            egui::CentralPanel::default().show_inside(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("state machine stacked")
                    .auto_shrink(false)
                    .show(ui, |ui| {
                        let size = egui::vec2(ui.available_width(), STACKED_GRAPH);
                        ui.allocate_ui(size, |ui| go = self.controller_graph(ui));
                        ui.add_space(6.0);
                        self.controller_live(ui, assets, render_state, dt);
                        ui.separator();
                        egui::CollapsingHeader::new(format!("{} Settings", icons::SLIDERS))
                            .id_salt("state machine inspector stacked")
                            .default_open(true)
                            .icon(widgets::chevron)
                            .show(ui, |ui| self.controller_inspector(ui));
                    });
            });
        } else {
            // Across the whole body, so the preview has room beside its
            // parameters; the graph keeps at least half the height.
            let room = ui.available_height();
            egui::TopBottomPanel::bottom("state machine preview")
                .resizable(true)
                .default_height(CONTROLLER_PREVIEW.min(room * 0.5))
                .min_height(100.0)
                .max_height((room * 0.5).max(100.0))
                .show_inside(ui, |ui| self.controller_live(ui, assets, render_state, dt));
            if !self.machine.hide_inspector {
                let room = ui.available_width();
                egui::SidePanel::right("state machine inspector")
                    .resizable(true)
                    .default_width(320.0)
                    .max_width(room * 0.5)
                    .show_inside(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt("state machine inspector scroll")
                            .show(ui, |ui| self.controller_inspector(ui))
                    });
            }
            egui::CentralPanel::default().show_inside(ui, |ui| go = self.controller_graph(ui));
        }
        if self.machine.doc.as_ref().is_some_and(|doc| doc.playing) {
            ui.ctx().request_repaint();
        }
        go
    }

    /// What's open's preview model and playback, the button that shows or
    /// hides the inspector, and the ⋯ menu. Returns what the ⋯ menu asked for.
    fn controller_toolbar(&mut self, ui: &mut egui::Ui, stacked: bool) -> Option<Pick> {
        let open = self.controller_path();
        responsive::toolbar(ui, |ui| {
            let mut pick = None;
            let Some(doc) = &mut self.machine.doc else {
                return pick;
            };
            let shown = doc
                .preview_model
                .as_deref()
                .map_or("(pick a model)", file_name);
            let mut picked = doc.preview_model.clone();
            responsive::room_for(ui, MODEL_PICKER_ROOM);
            egui::ComboBox::from_id_salt("state machine preview model")
                .selected_text(format!("{} Preview on {shown}", icons::CUBE))
                .width(160.0)
                .show_ui(ui, |ui| {
                    for model in &self.files.models {
                        ui.selectable_value(&mut picked, Some(model.clone()), file_name(model));
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
                    !self.machine.hide_inspector,
                )
                .clicked()
            {
                self.machine.hide_inspector = !self.machine.hide_inspector;
            }
            if let Some(path) = &open {
                ui.menu_button(icons::DOTS_THREE, |ui| asset_menu(ui, path, &mut pick));
            }
            pick
        })
    }

    /// The open state machine's graph, filling `ui`. Returns where
    /// double-clicking a state went: the clip it plays (a blend tree's
    /// first).
    fn controller_graph(&mut self, ui: &mut egui::Ui) -> Option<Go> {
        let controller = self.machine.doc.as_mut()?;
        let live = controller.live();
        let selected = controller.selected;
        let clips = &self.files.clips;
        let size = ui.available_size();
        let mut events = ui
            .allocate_ui(size, |ui| {
                controller
                    .view
                    .show(ui, &controller.file, selected, live, clips)
            })
            .inner;
        let mut go = None;
        events.retain(|event| {
            let graph::GraphEvent::Open(index) = event else {
                return true;
            };
            if let Some(state) = controller.file.states.get(*index)
                && let Some(clip) = motion_clips(&state.motion).into_iter().next()
            {
                go = Some(Go::State(state.name.clone(), clip));
            }
            false
        });
        controller.apply(events);
        go
    }

    /// The live preview beside its parameters, or above them when narrow.
    fn controller_live(
        &mut self,
        ui: &mut egui::Ui,
        assets: &Assets,
        render_state: Option<&RenderState>,
        dt: f64,
    ) {
        let Some(controller) = &mut self.machine.doc else {
            return;
        };
        let preview = &mut self.machine.preview;
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
        if let Some(controller) = &mut self.machine.doc {
            controller.inspector(ui, &self.files.clips);
        }
    }

    /// Shows the open dialog, and does what it's answered with.
    #[allow(clippy::too_many_arguments)] // Each is a different input to the studio.
    fn ask(
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
                    ui.heading("New state machine");
                    ui.horizontal(|ui| {
                        ui.label("Name");
                        ui.text_edit_singleline(name);
                    });
                    let valid = store::check_name(name);
                    if let Err(problem) = &valid {
                        ui.colored_label(ui.visuals().error_fg_color, problem);
                    }
                    let model_clips = subject
                        .map(|subject| subject.own_clips(&self.files))
                        .unwrap_or_default();
                    if let Some(subject) = subject {
                        if !model_clips.is_empty() {
                            ui.checkbox(
                                with_clips,
                                format!("Add a state for each of {}'s {} clips", subject.name, model_clips.len()),
                            );
                        }
                        ui.checkbox(assign, format!("Use it for {}'s Animator", subject.name));
                    } else {
                        ui.weak("Select a Part, Model or UI element first to give it this state machine and its clips.");
                    }
                    ui.horizontal(|ui| {
                        if ui.add_enabled(valid.is_ok(), egui::Button::new("Make state machine")).clicked() {
                            let clips = if *with_clips { model_clips.clone() } else { Vec::new() };
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
                                Err(err) => output.error(format!("Could not make the state machine: {err}")),
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
                    ui.weak("State machines that play it are updated too.");
                    let response = ui.text_edit_singleline(name);
                    if let Some(problem) = error {
                        ui.colored_label(ui.visuals().error_fg_color, problem.as_str());
                    }
                    let enter = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    ui.horizontal(|ui| {
                        if ui.button("Rename").clicked() || enter {
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
                                    if self.clip.doc.as_ref().is_some_and(|clip| clip.path() == path.as_str()) {
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
                        if ui.button("Cancel").clicked() {
                            close = true;
                        }
                    });
                }
                Dialog::RenameController { path, name, error } => {
                    ui.heading("Rename state machine");
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
                                flush(&mut self.clip.doc, &mut self.machine.doc, project, output)
                            {
                                actions.push(record);
                            }
                            match store::delete_asset(project, path) {
                                Ok(files) => {
                                    output.info(format!("Deleted `{path}`."));
                                    let label = if path.ends_with(axcenity_anim::STATE_MACHINE_EXTENSION)
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
            actions.push(StudioAction::Show);
        }
        if let Some((path, model)) = open_controller {
            self.open_controller(project, assets, &path, model, output);
            actions.push(StudioAction::Show);
        }
    }
}

/// The Open button, which drops down a searchable list of `paths` (state
/// machines and clips, grouped by folder), each with its kind's icon.
/// Clicking one opens it; its ⋯ menu renames, duplicates or deletes it.
fn picker(
    ui: &mut egui::Ui,
    search: &mut String,
    paths: &[String],
    open: Option<&str>,
) -> Option<Pick> {
    let button = ui
        .button(format!("{} Open {}", icons::FOLDER_OPEN, icons::CARET_DOWN))
        .on_hover_text("Open a clip or state machine in the project");
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
                ui.weak("No clips or state machines yet. Animate something, import an animated FBX, or use New.");
            } else if matches.is_empty() {
                ui.weak("Nothing matches.");
            }
            // By folder, in the order the folders come.
            let mut groups: Vec<(&str, Vec<&String>)> = Vec::new();
            for path in matches {
                let group = store::group_of(path);
                match groups.iter_mut().find(|(name, _)| *name == group) {
                    Some((_, paths)) => paths.push(path),
                    None => groups.push((group, vec![path])),
                }
            }
            let visuals = ui.visuals().clone();
            egui::ScrollArea::vertical()
                .max_height(PICKER_HEIGHT)
                .show(ui, |ui| {
                    for (group, paths) in groups {
                        let title = if group.is_empty() { "Animations" } else { group };
                        ui.weak(format!("{} {title}", icons::FOLDER));
                        for path in paths {
                            let kind = AnimKind::of_path(path).unwrap_or(AnimKind::Clip);
                            let name = match kind {
                                AnimKind::StateMachine => store::controller_name(path),
                                _ => axcenity_anim::files::clip_name(path),
                            };
                            ui.horizontal(|ui| {
                                let row = ui
                                    .add(egui::Button::selectable(
                                        open == Some(path.as_str()),
                                        (kind.rich_icon(&visuals), format!("  {name}")),
                                    ))
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

/// Rename, Duplicate and Delete for the clip or state machine at `path`.
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

/// Writes the open clip and state machine, and returns a record of the
/// property-clip edit if that write changed its file. Take-clip and state
/// machine edits stay unrecorded.
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
        } else if path.ends_with(axcenity_anim::STATE_MACHINE_EXTENSION) {
            1
        } else {
            2
        }
    };
    kind(path) == kind(other)
}

/// The New state machine dialog, named after the followed object if
/// there is one.
fn new_controller_dialog(subject: Option<&Subject>) -> Dialog {
    Dialog::NewController {
        name: subject
            .map(|subject| axcenity_anim::files::file_safe(&subject.name))
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "StateMachine".to_owned()),
        with_clips: true,
        assign: subject.is_some(),
    }
}

/// The clips `motion` plays, leaving out empty paths.
fn motion_clips(motion: &axcenity_anim::Motion) -> Vec<String> {
    motion
        .clips()
        .into_iter()
        .filter(|clip| !clip.is_empty())
        .cloned()
        .collect()
}

/// The last part of `path`: `noodle.fbx` for `assets/models/noodle.fbx`.
fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// The tab with nothing open: Animate what's selected (or followed), and
/// Import…. Returns which was pressed.
fn empty_state(ui: &mut egui::Ui, animatable: Option<&(InstanceId, String)>) -> Option<Go> {
    let palette = theme::palette(ui.ctx());
    let visuals = ui.visuals().clone();
    let mut go = None;
    ui.add_space(8.0);
    ui.heading(theme::with_icon(AnimKind::Animator.icon(), "Animation"));
    match animatable {
        Some((_, name)) => {
            ui.label(format!(
                "Animate {name} to give it an Animator and a clip, then move it in the preview with Record on."
            ));
            ui.add_space(4.0);
            let button = egui::Button::new((
                AnimKind::Animator.rich_icon(&visuals),
                egui::RichText::new(format!("  Animate {name}")).strong(),
            ))
            .truncate();
            if ui
                .add(button)
                .on_hover_text("An Animator, a clip and an Animation playing it, as one undo step")
                .clicked()
            {
                go = Some(Go::Animate);
            }
        }
        None => {
            ui.label(
                egui::RichText::new(
                    "Select a Part, Model or UI element to animate it, or an Animator to see what it plays.",
                )
                .color(palette.text_weak),
            );
        }
    }
    ui.add_space(6.0);
    if ui
        .button(format!("{} Import…", icons::DOWNLOAD_SIMPLE))
        .on_hover_text("Import an animated FBX: its takes become clips")
        .clicked()
    {
        go = Some(Go::ImportModel);
    }
    ui.weak("Or open a clip or state machine with Open above.");
    go
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
