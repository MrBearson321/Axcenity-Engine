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
pub(crate) mod object_preview;
mod posing;
pub(crate) mod preview;
pub(crate) mod property_editor;
pub mod store;
#[cfg(test)]
mod testing;
pub(crate) mod timeline;

#[cfg(test)]
pub use graph::{Node, words::when_words};
#[cfg(test)]
pub use property_editor::NewKeys;
#[cfg(test)]
pub use testing::{click_frames, drag_frames};

use std::collections::{HashMap, HashSet};
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
use posing::Poser;
use preview::Preview;
use property_editor::{Drawn, PropertyDoc};

mod chrome;
mod clip_ui;
mod documents;
mod machine_ui;

use store::AnimationFiles;

mod dialogs;

use dialogs::*;

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
            | ClassName::Seat
            | ClassName::VehicleSeat
            | ClassName::DamageZone
            | ClassName::ReverbZone
            | ClassName::Model
            | ClassName::ScreenGui
            | ClassName::Frame
            | ClassName::TextLabel
            | ClassName::TextButton
            | ClassName::TextBox
            | ClassName::ImageLabel
            | ClassName::PageLayout
            | ClassName::Table
            | ClassName::ScrollingFrame
            | ClassName::Slider
            | ClassName::Toggle
            | ClassName::Dropdown
            | ClassName::ProgressBar
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
        self.mesh_id.clone().filter(|id| {
            assets
                .model_blocking(id)
                .is_some_and(|model| model.rig.is_some())
        })
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
    /// Import…, as in the File menu.
    Import,
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

    /// The short form for the outline's rows, where every state of one
    /// model would repeat the same file: a linked clip names only its take
    /// (the file is on hover, and in the clip's toolbar).
    fn short_text(&self) -> String {
        match self {
            Badge::Linked { take, .. } => {
                let take = take.rsplit('|').next().unwrap_or(take);
                format!("{} {take}", icons::LINK)
            }
            _ => self.text(),
        }
    }

    /// The badge as a small, weak label that shortens to fit.
    fn show(&self, ui: &mut egui::Ui) {
        self.label(ui, self.text());
    }

    /// [`Self::show`] in its short form, for the outline (the whole badge
    /// on hover).
    fn show_short(&self, ui: &mut egui::Ui) {
        let short = self.short_text();
        let full = self.text();
        let response = self.label(ui, short.clone());
        if short != full {
            response.on_hover_text(full);
        }
    }

    fn label(&self, ui: &mut egui::Ui, text: String) -> egui::Response {
        let palette = theme::palette(ui.ctx());
        let color = match self {
            Badge::Missing => palette.warning,
            _ => palette.text_weak,
        };
        ui.add(
            egui::Label::new(
                egui::RichText::new(text)
                    .size(theme::type_scale::SMALL)
                    .color(color),
            )
            .truncate(),
        )
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
    /// Moving something (or a bone) in the preview keys it.
    record: bool,
    /// A take clip's bones in its preview: the selected one and its gizmo.
    bones: Poser,
    /// How both previews' gizmos snap and which axes they use, kept while
    /// the editor runs.
    pose: posing::PoseSettings,
    /// The New keys easing, kept from clip to clip while the editor runs.
    new_keys: property_editor::NewKeys,
    /// A property clip's preview: a copy of what it animates.
    objects: ObjectPreview,
    /// Which bones are folded in each model's bone tree, kept while the
    /// editor runs.
    folds: posing::Folds,
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
    outline_folds: OutlineFolds,
}

/// Which of the outline's sections are folded.
#[derive(Default)]
struct OutlineFolds {
    /// The State Machine's states.
    states: bool,
    animations: bool,
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
    /// Bake a linked clip's take into bone keys.
    ConvertToKeys(String),
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
    Import,
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

    /// Reads what the outline shows of `subject` that isn't read yet: its
    /// state machine's states, and each clip's badge.
    fn read_outline(&mut self, project: &Path, subject: &Subject) {
        let path = &subject.state_machine;
        self.read_machine(project, path);
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

    /// Reads the states of the state machine at `path`, with their clips,
    /// if they aren't read yet.
    fn read_machine(&mut self, project: &Path, path: &str) {
        if !path.is_empty() && !self.machines.contains_key(path) {
            let states = store::read_controller(project, path)
                .map(|file| {
                    file.states
                        .iter()
                        .map(|state| (state.name.clone(), motion_clips(&state.motion)))
                        .collect()
                })
                .unwrap_or_default();
            self.machines.insert(path.to_owned(), states);
        }
    }

    /// The clips and state machines something in the scene uses: each
    /// Animator's state machine and the clips it plays, and each
    /// Animation's clip. Open ▾ marks the rest **not used**.
    fn used_files(&mut self, project: &Path, world: &World) -> HashSet<String> {
        let mut used = HashSet::new();
        let mut machines = Vec::new();
        for id in world.descendants(world.root()) {
            let Some(instance) = world.get(id) else {
                continue;
            };
            let property = match instance.class() {
                ClassName::Animator => "StateMachine",
                ClassName::Animation => "Clip",
                _ => continue,
            };
            let Some(path) = instance
                .property(property)
                .and_then(Value::as_str)
                .filter(|path| !path.is_empty())
            else {
                continue;
            };
            if instance.class() == ClassName::Animator {
                machines.push(path.to_owned());
            }
            used.insert(path.to_owned());
        }
        for machine in machines {
            self.read_machine(project, &machine);
            used.extend(
                self.states_of(&machine)
                    .into_iter()
                    .flat_map(|(_, clips)| clips),
            );
        }
        used
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
                names.extend(
                    file.layers
                        .iter()
                        .map(|layer| (NameKind::Layer, layer.name.clone())),
                );
                for clip in file.all_states().flat_map(|(_, s)| s.motion.clips()) {
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

    // The tab.
}

/// The Open button, which drops down a searchable list of `paths` (state
/// machines and clips, grouped by folder), each with its kind's icon, and
/// **not used** on those not in `used`. Clicking one opens it; its ⋯ menu
/// renames, duplicates or deletes it.
fn picker(
    ui: &mut egui::Ui,
    search: &mut String,
    paths: &[String],
    open: Option<&str>,
    used: &HashSet<String>,
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
                                // A clip can be dragged onto the state
                                // machine graph, as from the Assets panel.
                                if kind == AnimKind::Clip {
                                    row.interact(egui::Sense::drag()).dnd_set_drag_payload(
                                        crate::asset_browser::AssetPayload(
                                            crate::asset_browser::AssetEntry {
                                                kind: crate::asset_browser::AssetKind::Animation,
                                                path: path.clone(),
                                                name: name.to_owned(),
                                            },
                                        ),
                                    );
                                }
                                if row.clicked() {
                                    pick = Some(Pick::Open(path.clone()));
                                }
                                if !used.contains(path) {
                                    ui.weak("not used").on_hover_text(
                                        "Nothing in the scene uses it: no Animator or Animation, and no state machine an Animator plays. ⋯ deletes it.",
                                    );
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
/// clip edit (keyed or linked) if that write changed its file. State
/// machine edits stay unrecorded.
fn flush(
    clip: &mut Option<Clip>,
    controller: &mut Option<Box<ControllerDoc>>,
    project: &Path,
    output: &mut Output,
) -> Option<StudioAction> {
    let mut record = None;
    if let Some(clip) = clip {
        let path = clip.path().to_owned();
        let written = match clip {
            Clip::Take(doc) => doc.write(project),
            Clip::Property(doc) => doc.write(project),
        };
        let result = written.map(|written| {
            record = written.map(|(before, after)| StudioAction::RecordFile {
                label: format!("Edit {}", axcenity_anim::files::clip_name(&path)),
                path,
                before,
                after,
            });
        });
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
        go = Some(Go::Import);
    }
    ui.weak("Or open a clip or state machine with Open above.");
    go
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_outline_names_a_linked_clip_by_its_take_alone() {
        let linked = Badge::Linked {
            file: "noodle.fbx".into(),
            take: "Armature|Wave".into(),
        };
        assert_eq!(linked.short_text(), format!("{} Wave", icons::LINK));
        assert!(linked.text().contains("Linked to noodle.fbx · Wave"));
        assert_eq!(Badge::Keyed.short_text(), Badge::Keyed.text());
    }

    /// Every piece of text drawn in `shapes`.
    fn texts(shapes: &[egui::epaint::ClippedShape]) -> Vec<String> {
        fn walk(shape: &egui::Shape, found: &mut Vec<String>) {
            match shape {
                egui::Shape::Text(text) => found.push(text.galley.text().to_owned()),
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| walk(s, found)),
                _ => {}
            }
        }
        let mut found = Vec::new();
        for clipped in shapes {
            walk(&clipped.shape, &mut found);
        }
        found
    }

    /// Clicks Open and returns the text drawn with its list showing.
    fn open_picker(paths: &[String], used: &HashSet<String>) -> Vec<String> {
        let ctx = theme::test_context();
        let mut search = String::new();
        // Where the Open button is drawn: a `Cell`, which the drawing
        // closure can set while it's read outside.
        let button = std::cell::Cell::new(egui::Rect::NOTHING);
        let mut drawn = Vec::new();
        let at = |events| egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(600.0, 500.0),
            )),
            events,
            ..Default::default()
        };
        let mut frame = |events: Vec<egui::Event>| {
            ctx.run(at(events), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    button.set(ui.cursor());
                    picker(ui, &mut search, paths, None, used);
                });
            })
        };
        frame(Vec::new());
        let on = button.get().min + egui::vec2(20.0, 10.0);
        let press = |pressed| egui::Event::PointerButton {
            pos: on,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        for events in [
            vec![egui::Event::PointerMoved(on)],
            vec![press(true)],
            vec![press(false)],
            Vec::new(),
        ] {
            drawn = texts(&frame(events).shapes);
        }
        drawn
    }

    #[test]
    fn open_shows_not_used_beside_what_nothing_uses() {
        let paths: Vec<String> = ["Walk", "Spare"]
            .map(|name| format!("assets/animations/noodle/{name}.anim.json"))
            .to_vec();
        let shown = open_picker(&paths, &HashSet::from([paths[0].clone()]));
        assert!(shown.iter().any(|text| text.contains("Spare")), "{shown:?}");
        let marks = |shown: &[String]| shown.iter().filter(|text| *text == "not used").count();
        assert_eq!(marks(&shown), 1, "{shown:?}");
        // Once nothing uses Walk either, both are marked.
        assert_eq!(marks(&open_picker(&paths, &HashSet::new())), 2);
        assert_eq!(
            marks(&open_picker(&paths, &paths.iter().cloned().collect())),
            0
        );
    }
}
