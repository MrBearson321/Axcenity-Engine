//! The 3D viewport: the world drawn by the same renderer as the player, into
//! a texture that egui shows, with a Studio-style camera.
//!
//! Controls: right-drag to look around, WASD to fly, E/Q for up and down,
//! Shift to go slowly, the mouse wheel to zoom toward the focus point,
//! middle-drag to pan, and Alt+left-drag to orbit around the focus point.
//! Shift+wheel, or the wheel while right-dragging, changes the flying speed,
//! and a pop-up in the middle of the viewport shows the new speed.
//!
//! Left-clicking a part selects it, and the selected part gets a blue box:
//! the edges of its sides that face the camera, where nothing hides them.
//! Lights and ParticleEmitters have no shape, so while editing one that
//! stands free (not in a part) shows as an icon where it is, as in Unreal:
//! click one to select it, drag it to move it, and the Move and Rotate
//! handles move and turn it. One inside a part goes with its part. A
//! selected light shows how far it reaches (a SpotLight, its cone; a
//! RectLight, its rectangle and the way it faces).
//! With the Move, Scale or Rotate tool, it also gets handles to drag
//! (see `gizmo.rs`). With any tool, left-dragging a part by its body moves
//! it onto the surface under the mouse (see `placing.rs`).
//!
//! UI is visible as an in-place preview both while editing and during Play.
//! It's drawn over the 3D view, so a click selects the UI element under the
//! mouse before any part. A selected UI element gets an outline and handles
//! to move and resize it, and a drag snaps to other UI edges and centers
//! (see `ui_handles.rs`).
//!
//! With a Terrain Studio brush (see `terrain_studio.rs`), the left button
//! sculpts or paints the terrain instead: the brush shows on the ground
//! under the mouse, and nothing is selected or dragged. The camera works
//! as always (right-drag, Alt+left-drag, the wheel, WASD).
//!
//! While editing, Splines are drawn as their curves; clicking one edits its
//! points (see `splines.rs`): drag a point along the surfaces under the
//! mouse, or Alt-drag one to add a point after it.
//!
//! During Play there's no tool: the camera and selecting still work, but
//! nothing can be dragged. View > Overlays can draw collider wireframes,
//! recent script raycasts, Size boxes and spatial-sound markers on top;
//! they do not change the world, and the player never draws them.

use std::time::Instant;

use axcenity_assets::Assets;
use axcenity_core::{ClassName, Instance, InstanceId, Value, World};
use axcenity_render::TextureRenderer;
use axcenity_render::glam::{Mat4, Quat, Vec3};
use axcenity_render::wgpu;
use axcenity_scene::{FlyCamera, LightMarker, MarkerKind, MoveKeys, Particles, TerrainCache};
use axcenity_script::DebugRay;
use axcenity_ui::{Stylesheet, UiLayout, UiRect};
use eframe::egui;
use eframe::egui_wgpu::RenderState;

use crate::angular;
use crate::gizmo::{self, Drag, Handle, Modes, PartValues, Pose, Snap, Tool};
use crate::placing::PlaceDrag;
use crate::profiler::RenderCost;
use crate::screen::Screen;
use crate::snapping;
use crate::theme::{self, icons};
use crate::ui_handles::{self, Grip, Handles, UiDrag, UiDragEdit};

mod game_input;
mod grid;
mod input;
mod legend;
mod move_overlay;
mod overlays;
mod plugin_drawings;
mod plugin_handles;
mod probe_overlay;
mod ray_badge;
mod streaming_overlay;
pub use plugin_handles::HandleMove;
mod view_cube;
mod view_mode;
use overlays::*;
pub(crate) use overlays::{paint_agent_paths, paint_physics};
pub use view_mode::ViewMode;

/// Where the camera starts, looking at the origin (the same as the player).
const START_CAMERA_POSITION: Vec3 = Vec3::new(12.0, 8.0, 16.0);
/// After a long pause, don't jump the camera by the whole pause; treat the
/// frame as at most this long.
const MAX_FRAME_SECONDS: f32 = 0.1;
/// Zoom per point of scrolling. One wheel notch is 40 points, which moves
/// about a fifth of the way to the focus point.
const ZOOM_PER_POINT: f32 = 0.005;
/// One mouse wheel notch, in egui's scroll points. Each notch is one step of
/// `FlyCamera::change_speed` (20% faster or slower).
const POINTS_PER_NOTCH: f32 = 40.0;
/// How long the speed pop-up stays fully visible after the last change,
/// then how long it takes to fade out, in seconds.
const POPUP_SECONDS: f64 = 1.0;
const POPUP_FADE_SECONDS: f64 = 0.4;
/// The selection box: a bright blue, like Studio's, that stands out against
/// the sky and most part colors.
const SELECTION_COLOR: egui::Color32 = crate::theme::overlay::SELECTED;
const SELECTION_WIDTH: f32 = 2.0;
/// The outline of parts about to be placed.
const PREVIEW_COLOR: egui::Color32 = crate::theme::overlay::PREVIEW;
/// A light's icon, in points, the same size however far away the light is.
const LIGHT_ICON_RADIUS: f32 = 13.0;
/// Hide terrain collider chunks farther than this from the camera.
const TERRAIN_OVERLAY_DISTANCE: f32 = 200.0;
/// Collider wireframes: solid (green) and sensors (amber).
const COLLIDER_COLOR: egui::Color32 = egui::Color32::from_rgb(80, 220, 120);
const SENSOR_COLOR: egui::Color32 = egui::Color32::from_rgb(255, 180, 60);
/// Size boxes, distinct from the collider shape.
const BOUNDS_COLOR: egui::Color32 = egui::Color32::from_rgb(180, 120, 255);
/// Script raycasts and the point they hit.
const RAY_COLOR: egui::Color32 = egui::Color32::from_rgb(255, 220, 80);
const RAY_HIT_COLOR: egui::Color32 = crate::theme::overlay::WARNING;
/// Spatial sound sources.
const EMITTER_COLOR: egui::Color32 = egui::Color32::from_rgb(90, 200, 255);
const EMITTER_ICON_RADIUS: f32 = 11.0;
/// Navigation mesh cells are filled this close to the camera; its edges
/// are drawn out to the terrain overlay distance.
const NAV_FILL_DISTANCE: f32 = 80.0;
const NAV_EDGE_COLOR: egui::Color32 = egui::Color32::from_rgb(40, 230, 210);
/// The culling overlay: bounds drawn (green) and culled (red), and the
/// frozen view's edges out to `FROZEN_VIEW_LENGTH` studs.
const CULL_DRAWN_COLOR: egui::Color32 = egui::Color32::from_rgb(110, 230, 120);
const CULL_CULLED_COLOR: egui::Color32 = crate::theme::overlay::WARNING;
const FROZEN_VIEW_COLOR: egui::Color32 = egui::Color32::from_rgb(255, 220, 80);
const FROZEN_VIEW_LENGTH: f32 = 60.0;
/// The Agent Paths overlay: what's left of each agent's path.
const AGENT_PATH_COLOR: egui::Color32 = egui::Color32::from_rgb(255, 150, 40);
/// Contact points and their push (roadmap G15).
const CONTACT_COLOR: egui::Color32 = egui::Color32::from_rgb(255, 90, 200);
/// A moving body's center of mass while it's simulated.
const AWAKE_COLOR: egui::Color32 = egui::Color32::from_rgb(255, 200, 60);
/// A body rapier has put to sleep.
const ASLEEP_COLOR: egui::Color32 = egui::Color32::from_rgb(120, 160, 255);

/// A left click in the viewport, with the UI element or part under the
/// mouse (`None` for empty space, which clears the selection). Starting to
/// drag something by its body counts as clicking it, so it gets selected.
pub struct Clicked {
    pub part: Option<InstanceId>,
    /// Ctrl, Shift and the other modifier keys held for this click.
    pub modifiers: egui::Modifiers,
    /// True when a selected part was grabbed to drag the whole selection,
    /// which keeps the rest of the selection.
    pub keep_group: bool,
}

/// What a drag in the viewport does to a part, which names its undo step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragKind {
    Move,
    Resize,
    Rotate,
}

impl DragKind {
    pub fn of(handle: Handle) -> DragKind {
        match handle {
            Handle::Move(_) => DragKind::Move,
            Handle::Scale(..) => DragKind::Resize,
            Handle::Rotate(_) => DragKind::Rotate,
        }
    }
}

/// New property values for the parts being dragged (by a body or a handle).
#[derive(Clone, Debug, PartialEq)]
pub struct DragEdit {
    pub kind: DragKind,
    /// Each part changed, with its new values. The last selected part is last.
    pub parts: PartValues,
}

/// What happened in the viewport this frame.
#[derive(Default)]
pub struct ViewportOutput {
    pub clicked: Option<Clicked>,
    /// A UI button clicked during Play, delivered to LocalScripts next frame.
    pub activated: Option<InstanceId>,
    /// The mouse's buttons, movement and wheel and touches over the
    /// viewport during Play, for the game's input.
    pub game_input: Vec<axcenity_core::input::InputEvent>,
    /// `MouseEnter`, `MouseLeave` and buttons' sounds from this frame's UI.
    /// The app passes them to the game during Play, and drops them otherwise.
    pub ui_events: Vec<axcenity_ui::UiEvent>,
    /// A TextBox clicked during Play. The app gives it keyboard focus.
    pub text_box: Option<InstanceId>,
    /// The viewport was clicked during Play, including a click on empty space.
    pub viewport_clicked: bool,
    /// Page and billboard problems from this frame's layout.
    pub ui_warnings: Vec<String>,
    /// ScrollingFrames scrolled during Play by the wheel, a bar or a drag:
    /// the `CanvasPosition` the app sets on each in the game.
    pub scrolled: Vec<(InstanceId, axcenity_core::Vector2)>,
    /// Widgets changed during Play by the pointer (a Slider dragged, a
    /// Toggle flipped, a Dropdown's option picked): the `Value` the app sets
    /// on each in the game.
    pub widget_values: Vec<(InstanceId, axcenity_core::Value)>,
    pub drag: Option<DragEdit>,
    /// A UI element being moved or resized.
    pub ui_drag: Option<UiDragEdit>,
    /// True on the frame a drag is let go, which ends that drag's undo step.
    pub drag_released: bool,
    /// Where something dragged from another panel (an asset) would land,
    /// while it's over the viewport.
    pub drop: Option<DropTarget>,
    /// The mouse over the terrain being brushed, while a Terrain Studio
    /// brush is on.
    pub terrain: Option<TerrainPointer>,
    /// With the Constraint tool on: the part surface clicked, if any.
    pub surface: Option<SurfacePick>,
    /// A Spline's curve or point clicked, or a point dragged or added.
    pub spline: Option<SplineAction>,
    /// The mouse over the Scene View for the editor's plugins
    /// (`plugin:GetMouse()`), while any run and the game doesn't.
    pub plugin_mouse: Option<axcenity_plugins::MouseInput>,
    /// A plugin handle the user dragged (`plugin:CreateHandle`).
    pub plugin_handle: Option<HandleMove>,
    /// What drawing the scene took, for the Profiler.
    pub render: Option<RenderCost>,
    /// Materials whose surface shaders didn't compile, each once, for the
    /// Output window.
    pub shader_errors: Vec<axcenity_render::ShaderError>,
}

/// A Terrain Studio brush, as the viewport shows it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerrainBrush {
    pub terrain: InstanceId,
    /// Studs from the middle to the rim.
    pub radius: f32,
    /// 0 to 1: how much of the radius fades.
    pub falloff: f32,
    pub square: bool,
    pub color: egui::Color32,
}

/// The mouse and left button over the terrain being brushed this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TerrainPointer {
    /// The ground under the mouse, if it's over the terrain.
    pub point: Option<Vec3>,
    /// The left button went down this frame (starting a stroke).
    pub pressed: bool,
    /// A stroke is under way: the button went down over the viewport and
    /// hasn't been let go.
    pub down: bool,
    /// The stroke ended this frame.
    pub released: bool,
    pub modifiers: egui::Modifiers,
    /// Seconds since the last frame.
    pub seconds: f32,
}

/// What the mouse did to a Spline while editing (Phase 73).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SplineAction {
    /// Its curve was clicked: select it and edit its points.
    Edit(InstanceId),
    /// A point of the Spline being edited was pressed: select it.
    Pick(InstanceId),
    /// The point is dragged to `to`, on the surface under the mouse.
    Move { point: InstanceId, to: Vec3 },
    /// An Alt-drag from `after` was let go at `at`: add a point there.
    Add { after: InstanceId, at: Vec3 },
}

/// A Spline point held with the left button.
#[derive(Clone, Copy, Debug)]
struct SplineDrag {
    point: InstanceId,
    /// Alt was held as it went down: the drag adds a point.
    add: bool,
    /// Where the surface under the mouse is, once it has moved.
    at: Option<Vec3>,
}

/// A point clicked on a part's surface, for the Constraint tool.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfacePick {
    pub part: InstanceId,
    pub point: Vec3,
    /// Which way the surface faces there, one stud long.
    pub normal: Vec3,
}

/// What's under the mouse while an asset is dragged over the viewport.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DropTarget {
    /// The point on the surface under the mouse: a part, or the ground
    /// (the plane at Y = 0), or the camera's focus point if neither.
    pub point: Vec3,
    /// The way that surface faces, one stud long.
    pub normal: Vec3,
    /// The part under the mouse.
    pub part: Option<InstanceId>,
    /// The UI element under the mouse, which is in front of any part.
    pub ui: Option<InstanceId>,
    /// True on the frame the mouse button is let go: drop it there.
    pub released: bool,
}

/// The camera during Play, from the Play session.
#[derive(Clone, Copy, Debug)]
pub struct PlayCamera {
    /// What to draw from.
    pub shown: axcenity_script::CameraView,
    /// Whether the game holds the view (a character controller, or a
    /// script's CameraType), so the fly camera doesn't fly.
    pub holds: bool,
}

/// The chat window during Play (Phase 67): its lines (who said each,
/// `None` for a system message, and what), and the line being typed while
/// it's open.
pub type ChatView = (Vec<(Option<String>, String)>, Option<String>);

pub struct ViewportScene<'a> {
    pub world: &'a World,
    pub assets: &'a Assets,
    pub selected: Option<InstanceId>,
    /// Every selected Instance. `selected` is the last of these, and the one
    /// that gets handles. The others are outlined too.
    pub selection: &'a [InstanceId],
    pub tool: Option<Tool>,
    pub snap: Snap,
    /// The Move, Resize and Rotate tools' options.
    pub modes: Modes,
    /// Outlines of parts about to be placed (the Angular tool's copies).
    pub preview: &'a [Pose],
    /// Particle bursts asked for this frame (`Emit` from scripts, or the
    /// Emission tool's Burst): each emitter and how many.
    pub emits: &'a [(InstanceId, u32)],
    pub stylesheets: &'a [Stylesheet],
    /// The TextBox being typed in during Play, so it gets a focus ring.
    pub focused_text: Option<InstanceId>,
    /// The game's loading screen during Play, drawn over the other UI.
    pub loading_screen: Option<InstanceId>,
    /// The player character's Inventory during Play, whose hotbar is drawn
    /// over the game UI.
    pub hotbar: Option<InstanceId>,
    /// The player's Abilities during Play (Phase 76), for the ability bar.
    pub ability_bar: Vec<axcenity_core::ability::AbilitySlot>,
    /// The active quests during Play (Phase 77), for the quest tracker.
    pub quests: Vec<axcenity_core::dialogue::QuestView>,
    /// The dialogue box during Play, while a dialogue runs (Phase 77).
    pub dialogue: Option<axcenity_core::dialogue::DialogueView>,
    /// Whether the default leaderboard is drawn during Play (Tab hides it).
    pub leaderboard: bool,
    /// The chat window during Play (Phase 67): its lines, and the line
    /// being typed while it's open.
    pub chat: Option<ChatView>,
    /// The ProximityPrompts that show during Play, drawn over the game UI.
    pub prompts: Vec<axcenity_ui::PromptView>,
    /// The cursor over a ClickDetector during Play (a CSS cursor name).
    pub click_cursor: Option<String>,
    /// The Developer Console and its `fps` readout during Play (Phase 71),
    /// drawn over everything else.
    pub console: Vec<axcenity_render::UiDrawItem>,
    /// Show every light's reach, not just the selected ones' (View > Light Cones).
    pub light_cones: bool,
    /// Whether the view cube is drawn in the corner while editing.
    pub view_cube: bool,
    /// Whether the ray tracing badge is drawn (Viewport > Ray Tracing Badge).
    pub ray_badge: bool,
    /// Whether the measuring grid is drawn while editing.
    pub grid: bool,
    /// Whether the movement guide (`move_overlay`: the line along a move,
    /// the snap points and the distance) is drawn. Moving works the same
    /// either way.
    pub move_guide: bool,
    /// How solid its lines are, as a multiple of the usual soft look.
    pub grid_strength: f32,
    /// Editor-only visual-debug overlays (View > Overlays).
    pub overlays: OverlayToggles,
    /// What the viewport shows: lit, or a debug view (View > View Mode).
    pub view_mode: ViewMode,
    /// Whether this is the game during Play: world streaming and its
    /// radius only apply then (Phase 80).
    pub playing: bool,
    /// Recent `workspace:Raycast` calls during Play, for the raycast overlay.
    pub debug_rays: &'a [DebugRay],
    /// The navigation mesh, for its overlay.
    pub nav_mesh: Option<&'a axcenity_nav::NavMesh>,
    /// The path each NavigationAgent is walking during Play, from its feet,
    /// for the Agent Paths overlay.
    pub agent_paths: &'a [(InstanceId, Vec<axcenity_core::Vector3>)],
    /// The LightProbeVolumes whose bake was made from a different world,
    /// for the Light Probes overlay.
    pub stale_light_probes: &'a [InstanceId],
    /// The simulation's contacts and bodies during Play, for the Contacts
    /// and Bodies overlays; `None` outside Play or with both off.
    pub physics: Option<&'a axcenity_physics::DebugView>,
    /// Animated MeshParts' poses during Play; `None` draws every model still.
    pub poses: Option<&'a axcenity_scene::Poses>,
    /// Moving parts drawn between physics steps during Play (roadmap G3);
    /// `None` draws every part where the world has it.
    pub blends: Option<axcenity_scene::Blends>,
    /// A Terrain Studio brush, which takes the left button while it's on.
    pub terrain_brush: Option<TerrainBrush>,
    /// The Constraint tool, when it's on: the left button picks part
    /// surfaces instead of selecting or dragging. Holds the first surface
    /// once it's picked, to mark it.
    pub constraint_pick: Option<Option<Vec3>>,
    /// The Spline whose points are being edited: the one selected, or the
    /// one a selected SplinePoint is in.
    pub spline_editing: Option<InstanceId>,
    /// While plugins run (outside Play): whether one has the left button
    /// (`plugin:Activate(true)`), which then neither selects nor drags.
    pub plugin_mouse: Option<bool>,
    /// What running plugins draw over the Scene View (`plugin:DrawLine`
    /// and the rest; roadmap G10). Empty during Play.
    pub plugin_drawings: Vec<&'a axcenity_plugins::Drawing>,
    /// Running plugins' handles, each with its plugin's index in the
    /// editor's list. Empty during Play.
    pub plugin_handles: Vec<(usize, &'a axcenity_plugins::Handle)>,
    /// The camera during Play (`Session::shown_camera`, with whether the
    /// game holds the view). A controller's or a script's camera doesn't
    /// move the fly camera, so Stop shows it where it was.
    pub play_camera: Option<PlayCamera>,
    /// This computer's graphics settings (Settings > Graphics).
    pub graphics: &'a axcenity_render::GraphicsSettings,
    /// What other Collaborate members have selected.
    pub others: &'a [MemberSelection],
}

/// Another Collaborate member's selection, as the viewport and Explorer
/// show it.
#[derive(Clone, Debug, PartialEq)]
pub struct MemberSelection {
    pub name: String,
    pub color: egui::Color32,
    pub selection: Vec<InstanceId>,
    /// The Properties field they're dragging, such as "Transparency".
    pub editing: Option<String>,
}

/// Which visual-debug overlays the viewport draws. Off in the player.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OverlayToggles {
    pub colliders: bool,
    pub raycasts: bool,
    pub bounds: bool,
    pub emitters: bool,
    pub nav_mesh: bool,
    pub agent_paths: bool,
    /// Welds and Attachments (selected Attachments always show).
    pub constraints: bool,
    /// During Play: where bodies touch, and how hard (roadmap G15).
    pub contacts: bool,
    /// During Play: each moving body's center of mass, coloured by
    /// whether it's awake or asleep (roadmap G15).
    pub bodies: bool,
    /// With Workspace streaming on, what streams around the camera, while
    /// editing too (roadmap R16).
    pub streaming: bool,
    /// With Workspace streaming on, leave out what's out of range of the
    /// camera while editing as in Play, so a big world edits as light as it
    /// plays (roadmap R16.3). Not an overlay: `any` leaves it out.
    pub stream_editing: bool,
    /// Every light probe volume's probes, filled with their baked light,
    /// and whether each volume's bake is there and up to date (roadmap R18.1).
    pub light_probes: bool,
    /// Freezes culling where the camera is when it goes on, and outlines
    /// what was drawn and what was culled. Not saved between runs.
    pub culling: bool,
}

impl OverlayToggles {
    pub fn any(self) -> bool {
        self.colliders
            || self.raycasts
            || self.bounds
            || self.emitters
            || self.nav_mesh
            || self.agent_paths
            || self.constraints
            || self.contacts
            || self.bodies
            || self.streaming
            || self.light_probes
            || self.culling
    }
}

/// What the left button holds, from going down on it until it's let go.
#[derive(Clone, Debug)]
enum Dragging {
    /// A handle of the selected part.
    Handle(Drag),
    /// A part, by its body, and the other selected parts that move with
    /// it, each with where it was at the start.
    Body(PlaceDrag, Vec<(InstanceId, Vec3)>),
    /// A UI element, by its body or a handle.
    Ui(Box<UiDrag>),
    /// A free-standing light's or emitter's icon, and the other selected
    /// things that move with it, each with where it was at the start.
    Icon(IconDrag, Vec<(InstanceId, Vec3)>),
}

/// A light or ParticleEmitter dragged by its icon: it follows the mouse
/// across the plane facing the camera that it started in.
#[derive(Clone, Debug)]
struct IconDrag {
    id: InstanceId,
    /// Where it was when the drag started.
    start: Vec3,
    /// From where the plane was grabbed to the light.
    grab_offset: Vec3,
    /// The plane's normal: the way the camera looked when it was grabbed.
    normal: Vec3,
    /// How far it has come, for drawing.
    report: Option<Box<crate::snapping::points::DragReport>>,
}

impl IconDrag {
    /// Where the light goes for the mouse on `ray`, its move snapped to
    /// the move grid on each axis; `None` if the ray misses the plane.
    fn update(&self, ray: axcenity_scene::Ray, snap: &Snap) -> Option<Vec3> {
        let grabbed = self.start - self.grab_offset;
        let along = ray.direction.dot(self.normal);
        if along.abs() < 1e-4 {
            return None;
        }
        let distance = (grabbed - ray.origin).dot(self.normal) / along;
        if distance <= 0.0 {
            return None;
        }
        let moved = ray.at(distance) + self.grab_offset - self.start;
        let moved = Vec3::new(
            snap.studs(moved.x),
            snap.studs(moved.y),
            snap.studs(moved.z),
        );
        Some(self.start + moved)
    }
}

/// What's selected, for dragging.
#[derive(Clone, Copy)]
struct DragSelection<'a> {
    selected: Option<InstanceId>,
    selection: &'a [InstanceId],
    modes: Modes,
}

/// The parts the transform tools act on: each selected part, and the parts
/// inside each selected Model or Folder, once each, with the last selected
/// one (or the last part inside it) last. A selected Attachment counts,
/// at its place in the world, unless its part moves too.
pub fn selected_parts(
    world: &World,
    selection: &[InstanceId],
    selected: Option<InstanceId>,
) -> Vec<(InstanceId, Pose)> {
    gather(world, selection, selected, false)
}

/// What the Move and Rotate handles and dragging move: the
/// [`selected_parts`], and the lights and ParticleEmitters standing free
/// (not in a part) that are selected or inside a selected Model or Folder.
pub fn movable_parts(
    world: &World,
    selection: &[InstanceId],
    selected: Option<InstanceId>,
) -> Vec<(InstanceId, Pose)> {
    gather(world, selection, selected, true)
}

/// How big a free-standing light's or emitter's box is for the handles.
const FREE_POSE_SIZE: f32 = 1.0;

/// Where a light or ParticleEmitter that stands free is, as a pose for
/// the handles. `None` for anything else, or one inside a part (it moves
/// with its part).
pub fn free_pose(world: &World, id: InstanceId) -> Option<Pose> {
    let instance = world.get(id)?;
    if !instance.class().can_stand_free() {
        return None;
    }
    match axcenity_scene::mount_of(world, instance)? {
        axcenity_scene::Mount::Free { position, rotation } => Some(Pose {
            position,
            rotation,
            size: Vec3::splat(FREE_POSE_SIZE),
        }),
        axcenity_scene::Mount::Part { .. } => None,
    }
}

/// Keeps a drag from resizing free-standing lights and emitters: they have
/// no Size. Those left with nothing to change are dropped.
pub fn drop_free_sizes(world: &World, parts: &mut PartValues) {
    parts.retain_mut(|(id, values)| {
        if free_pose(world, *id).is_none() {
            return true;
        }
        values.retain(|(name, _)| *name != "Size");
        !values.is_empty()
    });
}

fn gather(
    world: &World,
    selection: &[InstanceId],
    selected: Option<InstanceId>,
    free: bool,
) -> Vec<(InstanceId, Pose)> {
    let mut ids: Vec<InstanceId> = selection.to_vec();
    if let Some(selected) = selected {
        ids.retain(|id| *id != selected);
        ids.push(selected);
    }
    let mut parts: Vec<(InstanceId, Pose)> = Vec::new();
    for id in ids {
        // Selected in the Explorer, a locked thing still can't be changed.
        if world.is_locked(id) {
            continue;
        }
        // A selected Attachment moves in the world, by its offset on its part.
        if let Some(pose) = crate::constraints::tool_pose(world, id) {
            parts.retain(|(seen, _)| *seen != id);
            parts.push((id, pose));
            continue;
        }
        let inside = std::iter::once(id).chain(world.descendants(id));
        for part in inside {
            if world.is_locked(part) {
                continue;
            }
            let pose = world
                .get(part)
                .and_then(Pose::of)
                .or_else(|| free.then(|| free_pose(world, part)).flatten());
            let Some(pose) = pose else {
                continue;
            };
            // Seen again: it goes to the end, so the last selected stays last.
            parts.retain(|(seen, _)| *seen != part);
            parts.push((part, pose));
        }
    }
    // An Attachment on a part that moves follows it already.
    let ids: Vec<InstanceId> = parts.iter().map(|(id, _)| *id).collect();
    parts.retain(|(id, _)| !crate::constraints::on_one_of(world, *id, &ids));
    parts
}

/// The UI as laid out this frame, and the selected element's handles.
struct UiScene<'a> {
    layout: &'a UiLayout,
    handles: Option<&'a Handles>,
    /// Where the viewport's top left is, in points, and the points-to-pixels
    /// scale, to turn the mouse position into the layout's pixels.
    origin: egui::Pos2,
    pixels_per_point: f32,
    /// The screen the UI was laid out into, in physical pixels. Drags snap
    /// to its edges and center.
    viewport: UiRect,
}

impl UiScene<'_> {
    /// The topmost UI element at `pointer` (in points).
    fn hit(&self, pointer: egui::Pos2) -> Option<InstanceId> {
        let pixels = (pointer - self.origin) * self.pixels_per_point;
        self.layout.hit(pixels.x, pixels.y)
    }
}

/// What dragging did this frame, before anything is drawn.
#[derive(Default)]
struct DragInput {
    /// The handles to draw, and which one to highlight.
    shapes: Vec<gizmo::Shape>,
    highlighted: Option<Handle>,
    /// The selected UI element's handle under the mouse or being dragged.
    ui_highlighted: Option<Grip>,
    /// True if the left button is (or just was) dragging something, so the
    /// click that ends it doesn't also select something.
    in_use: bool,
    /// A part grabbed by its body this frame, to select.
    grabbed: Option<InstanceId>,
    edit: Option<DragEdit>,
    ui_edit: Option<UiDragEdit>,
    released: bool,
}

pub struct Viewport {
    camera: FlyCamera,
    keys: MoveKeys,
    /// True while right-dragging, when the cursor is hidden and held.
    looking: bool,
    /// Made on the first frame, once eframe has set up the GPU.
    target: Option<Target>,
    /// The renderer being made on a thread while a project opens (see
    /// [`Self::make_renderer_in_background`]), taken on the first frame.
    making: Option<std::sync::mpsc::Receiver<TextureRenderer>>,
    /// While a project opens behind the loading screen: every mesh and
    /// texture in its world is sent to the GPU, not only what's in view
    /// (see `app/session.rs`).
    pub opening: bool,
    /// When the flying speed last changed (egui's clock, in seconds), to
    /// show and fade the speed pop-up.
    speed_changed_at: Option<f64>,
    /// What's being dragged, if anything.
    drag: Option<Dragging>,
    /// The ParticleEmitters' particles, shown while editing and during Play.
    particles: Particles,
    /// True from the left button going down with a terrain brush until
    /// it's let go.
    terrain_stroke: bool,
    /// The left button went down over the viewport for a plugin and is held.
    plugin_press: bool,
    /// Terrains' chunk meshes and weights, kept between frames, and the
    /// Terrain Studio's previews and views.
    pub terrain: TerrainCache,
    /// Mouse look, in pixels, saved for the next character step while a
    /// character controller owns the camera.
    character_look: (f32, f32),
    /// Mouse wheel notches, toward the screen positive, saved for the next
    /// character step: a third-person camera zooms by them.
    character_zoom: f32,
    /// The UI's hover and press states, transitions and tooltips over time.
    ui: axcenity_ui::UiInteraction,
    /// The UI took the last press (a widget, a scroll bar, an open
    /// Dropdown's list), so letting it go clicks no button or text box.
    ui_took_press: bool,
    /// The left button held during Play went down on the game's UI, so
    /// its release is the UI's too.
    ui_mouse_taken: bool,
    /// `UserInputService.MouseBehavior` and `MouseIconEnabled` as last
    /// applied to the cursor during Play.
    held_cursor: (axcenity_core::MouseBehavior, bool),
    /// Where the viewport was last drawn, in points.
    last_rect: egui::Rect,
    /// Its size in pixels when last drawn, as the game's UI is laid out.
    last_pixels: (u32, u32),
    /// Where the camera was when the culling overlay went on: culling
    /// stays there until it goes off.
    frozen_culling: Option<axcenity_render::Camera>,
    /// The Streaming overlay's split of the world (roadmap R16).
    streaming_overlay: streaming_overlay::StreamingOverlay,
    /// What a click goes through for the data layers: hidden or locked
    /// ones (R16.2; hidden ones aren't drawn either, through
    /// `Streaming::set_also_hidden`).
    layer_unpickable: std::collections::HashSet<InstanceId>,
    /// The Overdraw view's numbers, once its legend's Measure was clicked;
    /// cleared when the view changes.
    overdraw: Option<axcenity_render::OverdrawStats>,
    /// Measure was clicked: the next frame drawn is read back.
    measure_overdraw: bool,
    /// The ray tracing badge's Restart now was clicked; the app restarts
    /// the editor (`take_restart_request`).
    restart_requested: bool,
    /// The ray tracing badge's Compare is on: the view is drawn without its
    /// ray traced effects, to see what they add.
    rays_hidden: bool,
    /// The LOD inspector's preview: this item (`scene::item_id`) drawn at
    /// this level whatever its size. Only what's drawn changes.
    pub lod_preview: Option<(u64, usize)>,
    /// The camera swinging round after a click on the view cube.
    swing: Option<view_cube::Swing>,
    /// The measuring grid's printed distances.
    grid_labels: grid::Labels,
    grid_glow: grid::NearbyGlow,
    /// A Spline point being dragged.
    spline_drag: Option<SplineDrag>,
    /// The left button went down on a Spline's curve and is held, so it
    /// neither selects nor drags anything else.
    spline_press: bool,
    /// A plugin's handle being dragged.
    handle_drag: Option<plugin_handles::HandleDrag>,
}

/// The texture the scene is drawn into, and egui's name for it.
struct Target {
    renderer: TextureRenderer,
    texture_id: egui::TextureId,
}

impl Viewport {
    /// Whether the ray tracing badge's Restart now was clicked since last
    /// asked.
    pub fn take_restart_request(&mut self) -> bool {
        std::mem::take(&mut self.restart_requested)
    }

    /// The graphics values the viewport draws with, once it has drawn.
    #[cfg(test)]
    pub fn graphics(&self) -> Option<&axcenity_render::EffectiveGraphics> {
        self.target
            .as_ref()
            .map(|target| target.renderer.graphics())
    }

    /// The last frame the viewport drew, as RGBA rows, once it has drawn.
    #[cfg(test)]
    pub fn pixels(&self) -> Option<Vec<u8>> {
        self.target
            .as_ref()
            .map(|target| target.renderer.read_pixels())
    }

    /// How many items' bounds the culling overlay holds (none while
    /// culling isn't held).
    #[cfg(test)]
    pub fn culling_bounds_held(&self) -> usize {
        self.target
            .as_ref()
            .map_or(0, |target| target.renderer.culling_bounds().len())
    }

    /// Starts making the viewport's renderer on a thread, if it isn't made
    /// yet: setting up its pipelines takes a second or more on some cards,
    /// which would otherwise freeze the first frame that draws the scene.
    /// Called while a project loads, so it's ready when the project opens.
    pub fn make_renderer_in_background(&mut self, render_state: &RenderState) {
        if self.target.is_some() || self.making.is_some() {
            return;
        }
        let (sender, receiver) = std::sync::mpsc::channel();
        let device = render_state.device.clone();
        let queue = render_state.queue.clone();
        // The size is a stand-in: the first frame resizes it to the panel.
        let spawned = std::thread::Builder::new()
            .name("viewport renderer".to_owned())
            .spawn(move || {
                let _ = sender.send(TextureRenderer::new(&device, &queue, 640, 360));
            });
        if spawned.is_ok() {
            self.making = Some(receiver);
        }
    }

    /// How many meshes and textures are still on their way to the GPU.
    pub fn pending_uploads(&self) -> usize {
        self.target
            .as_ref()
            .map_or(0, |target| target.renderer.pending_uploads())
    }

    pub fn new() -> Self {
        Self {
            camera: start_camera(),
            keys: MoveKeys::default(),
            looking: false,
            target: None,
            making: None,
            opening: false,
            speed_changed_at: None,
            drag: None,
            spline_drag: None,
            spline_press: false,
            handle_drag: None,
            particles: Particles::new(),
            terrain_stroke: false,
            plugin_press: false,
            terrain: TerrainCache::new(),
            character_look: (0.0, 0.0),
            character_zoom: 0.0,
            ui: axcenity_ui::UiInteraction::default(),
            ui_took_press: false,
            ui_mouse_taken: false,
            held_cursor: (axcenity_core::MouseBehavior::Default, true),
            last_rect: egui::Rect::NOTHING,
            last_pixels: (0, 0),
            frozen_culling: None,
            streaming_overlay: Default::default(),
            layer_unpickable: Default::default(),
            overdraw: None,
            measure_overdraw: false,
            restart_requested: false,
            rays_hidden: false,
            lod_preview: None,
            swing: None,
            grid_labels: Default::default(),
            grid_glow: Default::default(),
        }
    }

    /// Mouse look accumulated while a character controller owns the camera,
    /// and clears it. Right and down are positive.
    pub fn take_character_look(&mut self) -> (f32, f32) {
        std::mem::take(&mut self.character_look)
    }

    /// Mouse wheel notches over the viewport while a character controller
    /// owns the camera, and clears them. Toward the screen is positive.
    pub fn take_character_zoom(&mut self) -> f32 {
        std::mem::take(&mut self.character_zoom)
    }

    /// The particles and world streaming the viewport draws, for Play to
    /// swap its own in while it draws (`Session::swap_effects`).
    pub fn effects_mut(&mut self) -> (&mut Particles, &mut axcenity_scene::Streaming) {
        (&mut self.particles, &mut self.terrain.streaming)
    }

    /// Takes every particle away, for a new world (Play, Stop, another project).
    pub fn clear_particles(&mut self) {
        self.particles.clear();
    }

    /// Uses a key for the game UI's focus during Play, by the last frame's
    /// layout (see `UiInteraction::navigate`).
    pub fn navigate_ui(
        &mut self,
        world: &World,
        key: axcenity_ui::NavKey,
    ) -> axcenity_ui::UiNavigated {
        self.ui.navigate(world, key)
    }

    /// Holds the cursor during Play as `UserInputService.MouseBehavior`
    /// asks (`mouse`, with `MouseIconEnabled`): hidden in the middle of the
    /// viewport for LockCenter, hidden where it is for
    /// LockCurrentPosition. Stop passes the defaults, which let it go.
    pub fn hold_cursor(
        &mut self,
        ctx: &egui::Context,
        mouse: (axcenity_core::MouseBehavior, bool),
    ) {
        use axcenity_core::MouseBehavior;
        let (behavior, icon) = mouse;
        if mouse != self.held_cursor {
            self.held_cursor = mouse;
            if !self.looking {
                capture_mouse(ctx, behavior != MouseBehavior::Default);
                ctx.send_viewport_cmd(egui::ViewportCommand::CursorVisible(
                    icon && behavior == MouseBehavior::Default,
                ));
            }
        }
        // `Confined` (Windows, X11) lets the cursor drift; put it back.
        if behavior == MouseBehavior::LockCenter && self.last_rect.is_positive() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CursorPosition(
                self.last_rect.center(),
            ));
        }
    }

    /// The game UI's hover, press and focus states.
    pub fn ui(&self) -> &axcenity_ui::UiInteraction {
        &self.ui
    }

    /// Whether `key` is for the game UI's focus rather than the game.
    pub fn ui_takes(&self, world: &World, key: axcenity_ui::NavKey) -> bool {
        self.ui.takes(world, key)
    }

    /// The element the game UI's focus is on.
    #[cfg(test)]
    pub fn ui_selected(&self) -> Option<InstanceId> {
        self.ui.selected()
    }

    /// Lays the UI out as drawing does, without a GPU, for tests.
    #[cfg(test)]
    pub fn lay_out_ui(
        &mut self,
        world: &World,
        stylesheets: &[axcenity_ui::Stylesheet],
        size: (u32, u32),
    ) {
        let input = axcenity_ui::UiInput::default();
        let _ = self.ui.layout(world, size, stylesheets, input, None, 0.0);
    }

    /// Forgets the UI's hover, transitions and tooltip, as Play starts or
    /// stops: the edit world and the game's are different worlds.
    pub fn clear_ui(&mut self) {
        self.ui = axcenity_ui::UiInteraction::default();
    }

    /// Puts the camera back where it starts, for a newly opened project.
    /// The flying speed is a preference, not part of the project, so it stays.
    pub fn reset_camera(&mut self) {
        let speed = self.camera.speed();
        self.camera = start_camera();
        self.camera.set_speed(speed);
        self.swing = None;
        self.drag = None;
        self.particles.clear();
    }

    /// The camera's focus point, where new parts are inserted.
    /// The level of detail the item with this `scene::item_id` was last
    /// drawn at, if its mesh has levels and it was drawn lately.
    pub fn lod_level_of(&self, id: u64) -> Option<usize> {
        self.target.as_ref()?.renderer.lod_level_of(id)
    }

    /// The editing camera, as the renderer takes it.
    pub fn render_camera(&self) -> axcenity_render::Camera {
        self.camera.to_render_camera()
    }

    pub fn focus(&self) -> Vec3 {
        self.camera.focus()
    }

    /// Where a new part goes in `world`: in front of the camera, on what
    /// the middle of the view looks at (`spawning::spot`).
    pub fn spawn_spot(&self, world: &axcenity_core::World) -> crate::spawning::Spot {
        let camera = self.render_camera();
        let (width, height) = self.pixel_size();
        // Before the first frame there's no size yet: a common screen shape.
        let aspect = if width > 0 && height > 0 {
            width as f32 / height as f32
        } else {
            16.0 / 9.0
        };
        let view = axcenity_scene::CameraView::new(camera, aspect);
        crate::spawning::spot(
            world,
            camera.position,
            camera.look_at - camera.position,
            Some(&view),
        )
    }

    /// Where the camera is and which way it looks (degrees, like a Part's
    /// Orientation), for `workspace.CurrentCamera` during Play.
    pub fn camera_view(&self) -> (axcenity_core::Vector3, axcenity_core::Vector3) {
        self.camera.view()
    }

    /// Sets the size the viewport was last drawn at, for tests that don't draw.
    #[cfg(test)]
    pub fn set_pixel_size(&mut self, size: (u32, u32)) {
        self.last_pixels = size;
    }

    /// The viewport's size in pixels when last drawn: the game's view, for
    /// `Camera.ViewportSize` and screen points.
    pub fn pixel_size(&self) -> (u32, u32) {
        self.last_pixels
    }

    /// Moves the camera where a LocalScript put it during Play.
    pub fn set_camera_view(
        &mut self,
        position: axcenity_core::Vector3,
        orientation: axcenity_core::Vector3,
    ) {
        self.swing = None;
        self.camera.set_view(position, orientation);
    }

    /// Points the camera at `target` from `position` (following a Team
    /// Create member's camera, and tests).
    pub fn look_at(&mut self, position: Vec3, target: Vec3) {
        self.swing = None;
        self.camera = FlyCamera::looking_at(position, target);
    }

    /// Swings the camera round to look at a box around `center` reaching
    /// `radius` from it, from the way it looks now, close enough that the
    /// box fills about half the view (Explorer double-click). `now` is
    /// egui's clock; at once when `seconds` is 0 (reduced motion).
    pub fn frame(&mut self, center: Vec3, radius: f32, now: f64, seconds: f32) {
        let forward = self.camera.forward();
        let distance = (radius.max(0.5) * 2.5).max(4.0);
        let mut to = FlyCamera::looking_at(center - forward * distance, center);
        to.set_speed(self.camera.speed());
        if seconds > 0.0 {
            self.swing = Some(view_cube::Swing::new(self.camera, to, now, seconds));
        } else {
            self.swing = None;
            self.camera = to;
        }
    }

    /// What the part being moved has done so far, while one is.
    fn drag_report(&self) -> Option<&crate::snapping::points::DragReport> {
        match self.drag.as_ref()? {
            Dragging::Handle(drag) => drag.report(),
            Dragging::Body(place, _) => place.report.as_deref(),
            Dragging::Icon(drag, _) => drag.report.as_deref(),
            Dragging::Ui(_) => None,
        }
    }

    /// True while a part or a handle is being dragged, or a terrain stroke
    /// is under way.
    pub fn is_dragging(&self) -> bool {
        self.drag.is_some() || self.terrain_stroke
    }

    /// Fills `ui` with the viewport: moves the camera from this frame's
    /// input, then draws `world` from there with a box around `selected`,
    /// and `tool`'s handles on it. Returns what a click picked and what a
    /// drag changed; the app applies those to the world. With no `tool`
    /// (during Play), nothing can be dragged.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        render_state: &RenderState,
        scene: ViewportScene<'_>,
    ) -> ViewportOutput {
        let ViewportScene {
            world,
            assets,
            selected,
            selection,
            tool,
            snap,
            modes,
            preview,
            emits,
            stylesheets,
            focused_text,
            loading_screen,
            hotbar,
            ability_bar,
            quests,
            dialogue,
            leaderboard,
            chat,
            prompts,
            click_cursor,
            console,
            light_cones,
            view_cube: show_cube,
            ray_badge: show_ray_badge,
            grid: show_grid,
            move_guide,
            grid_strength,
            overlays,
            view_mode,
            debug_rays,
            nav_mesh,
            agent_paths,
            stale_light_probes,
            physics,
            poses,
            blends,
            playing: game_playing,
            terrain_brush,
            constraint_pick,
            spline_editing,
            plugin_mouse,
            plugin_drawings,
            plugin_handles,
            play_camera,
            graphics,
            others,
        } = scene;
        // A brush takes the left button: no handles, dragging or picking.
        let brush = terrain_brush.filter(|_| tool.is_some());
        // So does the Constraint tool, which picks surfaces.
        let picking = constraint_pick.filter(|_| tool.is_some());
        let plugin_button = plugin_mouse == Some(true) && play_camera.is_none();
        let drag_tool = if brush.is_some() || picking.is_some() || plugin_button {
            None
        } else {
            tool
        };
        let size = ui.available_size();
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
        self.last_rect = rect;
        let pixels_per_point = ui.ctx().pixels_per_point();
        let pointer = ui.input(|input| input.pointer.latest_pos());
        let pointer_pixels = pointer
            .filter(|_| response.contains_pointer())
            .map(|pointer| (pointer - rect.min) * pixels_per_point)
            .map(|position| [position.x, position.y]);
        // During Play, the wheel over a ScrollingFrame scrolls it (by the
        // last frame's layout) instead of moving the camera.
        let mut scrolled = Vec::new();
        let mut widget_values = Vec::new();
        let wheel_taken = match pointer_pixels.filter(|_| tool.is_none() && response.hovered()) {
            Some(at) => {
                let (smooth, raw) =
                    ui.input(|input| (input.smooth_scroll_delta, crate::wheel::raw(input)));
                if smooth == egui::Vec2::ZERO && raw == egui::Vec2::ZERO {
                    false
                } else {
                    let delta = -smooth * pixels_per_point;
                    let wheel = self.ui.wheel(world, at, [delta.x, delta.y]);
                    scrolled.extend(wheel.moved);
                    wheel.taken
                }
            }
            None => false,
        };
        let unmoved = (self.camera.position, self.camera.forward());
        self.handle_input(
            ui,
            &response,
            rect.height(),
            play_camera.is_some_and(|play| play.holds),
            wheel_taken,
        );
        // Moving the camera yourself stops a view cube swing; otherwise it
        // carries on.
        if let Some(swing) = self.swing {
            if (self.camera.position, self.camera.forward()) != unmoved {
                self.swing = None;
            } else {
                let (camera, done) = swing.at(ui.input(|input| input.time));
                self.camera = camera;
                if done {
                    self.swing = None;
                } else {
                    ui.ctx().request_repaint();
                }
            }
        }
        let camera = match play_camera {
            Some(play) => play.shown.render_camera(),
            None => self.camera.to_render_camera(),
        };
        let screen = self.screen(camera, rect);

        // The texture is sized in physical pixels, so it stays sharp on
        // high-DPI screens where one point is several pixels.
        let pixels = size * pixels_per_point;
        let (width, height) = (pixels.x.round() as u32, pixels.y.round() as u32);
        self.last_pixels = (width, height);
        // During Play a press on a ScrollingFrame's bar drags it, and one on
        // its content scrolls it once it has moved far enough; either way
        // the button under it doesn't fire. A press on a widget changes its
        // value instead, and one while a Dropdown's list is open picks from
        // it or closes it.
        let (primary_pressed, primary_down) = ui.input(|input| {
            (
                input.pointer.primary_pressed(),
                input.pointer.primary_down(),
            )
        });
        if tool.is_none()
            && let Some(at) = pointer_pixels
        {
            let scroll = if primary_pressed {
                let press = self.ui.press(world, at);
                self.ui_took_press = press.taken;
                press
            } else if primary_down {
                self.ui.drag(world, at)
            } else {
                axcenity_ui::UiScrolled::default()
            };
            scrolled.extend(scroll.moved);
            widget_values.extend(scroll.value);
        }
        let scrolling = self.ui.scrolling();
        let input = axcenity_ui::UiInput {
            pointer: pointer_pixels,
            pressed: None,
            focused: focused_text,
            on_top: loading_screen,
        };
        // What a click lands on: the UI with nothing hovered. `self.ui.layout`
        // below reuses it as its first pass (M14).
        let base_layout =
            self.ui
                .hit_layout(world, (width, height), stylesheets, input, Some(&camera));
        // The game hears every click; one on its UI is `gameProcessedEvent`.
        let game_input = if tool.is_none() {
            let events = ui.input(|input| input.events.clone());
            let area = game_input::GameArea {
                rect,
                pixels_per_point,
                hovered: response.contains_pointer(),
                wheel_taken,
            };
            let took = self.ui_took_press;
            area.input(
                &events,
                |[x, y]| {
                    took || base_layout.hit_button(x, y).is_some()
                        || base_layout.hit_text_box(x, y).is_some()
                },
                &mut self.ui_mouse_taken,
            )
        } else {
            Vec::new()
        };
        let held_button = (primary_down && !scrolling)
            .then(|| pointer_pixels.and_then(|[x, y]| base_layout.hit_button(x, y)))
            .flatten();
        let seconds = ui.input(|input| input.unstable_dt).min(MAX_FRAME_SECONDS);
        let ui_layout = self.ui.layout(
            world,
            (width, height),
            stylesheets,
            axcenity_ui::UiInput {
                pressed: held_button,
                ..input
            },
            Some(&camera),
            seconds,
        );
        // Transitions and tooltips move on without input.
        if ui_layout.animating {
            ui.ctx().request_repaint();
        }
        // During Play, the UI's cursor (CSS `cursor`): a hand on buttons.
        if tool.is_none() && response.hovered() && !self.looking {
            ui.ctx().set_cursor_icon(match ui_layout.cursor {
                // Over a ClickDetector's part, its CursorIcon.
                axcenity_ui::UiCursor::Default if click_cursor.is_some() => {
                    css_cursor(click_cursor.as_deref().unwrap_or_default())
                }
                axcenity_ui::UiCursor::Default => egui::CursorIcon::Default,
                axcenity_ui::UiCursor::Pointer => egui::CursorIcon::PointingHand,
                axcenity_ui::UiCursor::Text => egui::CursorIcon::Text,
                axcenity_ui::UiCursor::NotAllowed => egui::CursorIcon::NotAllowed,
            });
        }
        let handles = selected
            .and_then(|id| ui_layout.element(id))
            .map(|&element| Handles::new(element, world, stylesheets, rect.min, pixels_per_point));
        let ui_scene = UiScene {
            layout: &ui_layout,
            handles: handles.as_ref(),
            origin: rect.min,
            pixels_per_point,
            viewport: UiRect {
                x: 0.0,
                y: 0.0,
                width: width as f32,
                height: height as f32,
            },
        };

        // Lights only show as icons while editing (there's a tool), as in
        // Unreal's editor; the game itself doesn't show them.
        let markers = if tool.is_some() {
            axcenity_scene::light_markers(world)
        } else {
            Vec::new()
        };
        // Only lights whose icon is on screen can be clicked, but a light
        // just off screen can still shine into view. Only those standing
        // free have an icon: one in a part is picked and moved by its part.
        let lights: Vec<(LightMarker, egui::Pos2)> = markers
            .iter()
            .filter(|marker| marker.part.is_none())
            .filter_map(|marker| {
                let at = screen.point(marker.position)?;
                rect.contains(at).then(|| (marker.clone(), at))
            })
            .collect();
        // What of the view cube is under the mouse, while editing: a click
        // on it turns the camera instead of selecting what's behind it.
        let editing = play_camera.is_none() && tool.is_some();
        let cube = editing && show_cube;
        let cube_hit = pointer
            .filter(|_| cube && response.hovered())
            .and_then(|at| view_cube::hit_under(rect, self.camera.forward(), at));
        if cube_hit.is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        // Plugin handles take a press on them while editing, before anything else.
        let (plugin_handle, handle_taken) =
            if editing && brush.is_none() && picking.is_none() && !plugin_button {
                let step = snap.move_on.then_some(snap.move_step);
                self.plugin_handle_input(ui, &response, &screen, &plugin_handles, step)
            } else {
                self.handle_drag = None;
                (None, false)
            };
        // Splines take a press on their curve or points while editing.
        let (spline, spline_taken, spline_released) =
            if editing && brush.is_none() && picking.is_none() && !plugin_button && !handle_taken {
                self.spline_input(ui, &response, &screen, world, spline_editing)
            } else {
                self.spline_drag = None;
                self.spline_press = false;
                (None, false, false)
            };
        let drag_tool = if spline_taken || handle_taken {
            None
        } else {
            drag_tool
        };
        let drag = self.drag_input(
            ui,
            &response,
            &screen,
            world,
            DragSelection {
                selected,
                selection,
                modes,
            },
            drag_tool,
            snap,
            &ui_scene,
            stylesheets,
            &lights,
        );
        let modifiers = ui.input(|input| input.modifiers);
        let terrain = brush.map(|brush| self.terrain_input(ui, &response, &screen, world, brush));
        let clicked = match drag.grabbed {
            Some(part) => Some(Clicked {
                part: Some(part),
                modifiers,
                keep_group: selection.contains(&part),
            }),
            None if drag.in_use
                || brush.is_some()
                || picking.is_some()
                || plugin_button
                || spline_taken
                || handle_taken
                || cube_hit.is_some() =>
            {
                None
            }
            None => self.clicked_part(ui, &response, &screen, world, &ui_scene, &lights),
        };
        let playing = tool.is_none() && response.clicked_by(egui::PointerButton::Primary);
        let took_press = self.ui_took_press;
        let clicked_ui = |hit: fn(&axcenity_ui::UiLayout, f32, f32) -> Option<InstanceId>| {
            (playing && !took_press).then(|| {
                response.interact_pointer_pos().and_then(|pointer| {
                    let position = (pointer - rect.min) * pixels_per_point;
                    hit(&ui_layout, position.x, position.y)
                })
            })
        };
        let activated = clicked_ui(axcenity_ui::UiLayout::hit_button)
            .flatten()
            .filter(|_| !scrolling)
            .filter(|&button| self.ui.activate(world, button));
        if !primary_down {
            let released = self.ui.release(world, pointer_pixels);
            widget_values.extend(released.value);
        }
        let text_box = clicked_ui(axcenity_ui::UiLayout::hit_text_box).flatten();
        let making = self.making.take();
        let target = self.target.get_or_insert_with(|| {
            // Made on a thread while the project loaded, if it was: waits
            // for the rest of it (the thread failing makes one here).
            let renderer = making
                .and_then(|receiver| receiver.recv().ok())
                .unwrap_or_else(|| {
                    TextureRenderer::new(&render_state.device, &render_state.queue, width, height)
                });
            let texture_id = render_state.renderer.write().register_native_texture(
                &render_state.device,
                renderer.view(),
                wgpu::FilterMode::Linear,
            );
            Target {
                renderer,
                texture_id,
            }
        });
        // Does nothing unless the settings changed. While the ray tracing
        // badge's Compare is on, the effects are left out of what's drawn.
        if self.rays_hidden {
            target
                .renderer
                .set_graphics(&graphics.without_ray_traced_effects());
        } else {
            target.renderer.set_graphics(graphics);
        }
        target.renderer.set_debug_view(view_mode.debug_view());
        target.renderer.preview_lod(self.lod_preview);
        self.frozen_culling = (overlays.culling || view_mode.holds_culling())
            .then(|| self.frozen_culling.unwrap_or(camera));
        target.renderer.freeze_culling(self.frozen_culling);
        if target.renderer.size() != (width.max(1), height.max(1)) {
            target.renderer.resize(width, height);
            // The resize made a new texture, so point egui's name at it.
            render_state
                .renderer
                .write()
                .update_egui_texture_from_wgpu_texture(
                    &render_state.device,
                    target.renderer.view(),
                    wgpu::FilterMode::Linear,
                    target.texture_id,
                );
        }

        // Big emitters move on the GPU, where it runs compute shaders.
        self.particles
            .set_gpu(target.renderer.capabilities().compute_shaders);
        // During Play the game's frame moves its particles on (they're
        // swapped in for drawing: `Session::swap_effects`).
        if !game_playing {
            for &(emitter, count) in emits {
                self.particles.emit(world, emitter, count);
            }
            self.particles
                .update_against(world, seconds, &mut |skip, paths| {
                    crate::play::particle_surfaces(world, assets, skip, paths)
                });
        }
        // Particles keep moving without input, so keep drawing frames.
        if self.particles.is_active(world) {
            ui.ctx().request_repaint();
        }
        let still = axcenity_scene::Poses::new();
        let poses = poses.unwrap_or(&still);
        self.terrain.set_blends(blends.unwrap_or_default());
        // During Play the game's frame streams the world, as for particles.
        if !game_playing {
            stream_editing(
                &mut self.terrain.streaming,
                world,
                overlays.stream_editing,
                camera.position,
                f64::from(seconds),
                selection,
            );
        }
        // Meshes and textures go to the GPU on the renderer's upload thread
        // and appear a few frames later, rather than stalling the frame
        // that first draws them: turning the camera toward what's not on
        // the GPU yet (or not drawn lately) never freezes the editor.
        target.renderer.set_background_uploads(true);
        let render_started = Instant::now();
        // Blueprint textures are for building by: the game never shows them.
        assets.set_blueprint(
            (!game_playing)
                .then(crate::prototype::blueprint_image)
                .flatten(),
        );
        let mut draw_list =
            axcenity_scene::build_draw_list_posed(world, assets, camera, poses, &mut self.terrain);
        self.particles.draw(&mut draw_list, world, assets);
        self.particles.add_weather(&mut draw_list);
        let grid_spec = (show_grid && !game_playing).then(|| grid::spec(world, snap.grid_step()));
        draw_list.grid = grid_spec.as_ref().map(|spec| {
            let now = ui.input(|input| input.time);
            let glowing = self.grid_glow.update(world, camera.position, now);
            let tint = grid::tint(world, selected, glowing);
            grid::plane(spec, camera.position, grid_strength, tint)
        });
        move_overlay::ghost_moving(&mut draw_list, &moving_parts(self.drag.as_ref(), world));
        draw_list.ui = ui_layout.draw_items.clone();
        // The hotbar, the ability bar, the quest tracker and dialogue box,
        // the leaderboard, the chat window, then ProximityPrompts, drawn as
        // the player draws them.
        draw_list.ui.extend(axcenity_ui::overlay_items(
            world,
            &axcenity_ui::OverlayView {
                inventory: hotbar,
                abilities: &ability_bar,
                quests: &quests,
                dialogue: dialogue.as_ref(),
                leaderboard,
                chat: chat
                    .as_ref()
                    .map(|(lines, typing)| (lines.as_slice(), typing.as_deref())),
                prompts: &prompts,
            },
            stylesheets,
            &camera,
            width,
            height,
        ));
        draw_list.ui.extend(console);
        axcenity_scene::attach_ui_textures(&mut draw_list.ui, assets);
        draw_list.time = shader_time(ui, &draw_list);
        // Behind the loading screen, everything in the world is sent, seen
        // from here or not, so the editor is ready to look around in.
        if self.opening {
            target.renderer.preload(&draw_list);
        }
        if let Err(error) = target.renderer.render(&draw_list) {
            log::error!("could not render the viewport: {error:#}");
        }
        if view_mode != ViewMode::Debug(axcenity_render::DebugView::Overdraw) {
            self.overdraw = None;
        } else if std::mem::take(&mut self.measure_overdraw) {
            // Reading the picture back waits for the GPU: only on request.
            let (width, height) = target.renderer.size();
            self.overdraw = Some(axcenity_render::OverdrawStats::from_pixels(
                &target.renderer.read_pixels(),
                width,
                height,
            ));
        }
        let shader_errors = target.renderer.take_shader_errors();
        let render = RenderCost {
            cpu: render_started.elapsed(),
            lights: draw_list.lights.len(),
            stats: target.renderer.stats(),
        };
        let whole_texture = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
        ui.painter()
            .image(target.texture_id, rect, whole_texture, egui::Color32::WHITE);
        let eye = camera.position;
        // Only gathered once something selected needs them.
        let occluders = std::cell::LazyCell::new(|| axcenity_scene::Occluders::of(world));
        for &id in selection {
            if Some(id) != selected {
                if let Some(element) = ui_layout.element(id) {
                    let outline = egui::Rect::from_min_size(
                        rect.min + egui::vec2(element.rect.x, element.rect.y) / pixels_per_point,
                        egui::vec2(element.rect.width, element.rect.height) / pixels_per_point,
                    );
                    ui.painter_at(rect).rect_stroke(
                        outline,
                        0.0,
                        egui::Stroke::new(SELECTION_WIDTH, SELECTION_COLOR),
                        egui::StrokeKind::Middle,
                    );
                }
                paint_selection_box(ui, rect, &screen, &occluders, id, eye);
            }
        }
        if let Some(selected) = selected {
            paint_selection_box(ui, rect, &screen, &occluders, selected, eye);
        }
        if play_camera.is_none() {
            for member in others {
                paint_member_selection(ui, rect, &screen, &occluders, eye, member);
            }
        }
        let painter = ui.painter_at(rect);
        for marker in &markers {
            let chosen = selection.contains(&marker.id)
                || marker.part.is_some_and(|part| selection.contains(&part));
            if marker.kind != MarkerKind::Emitter && (chosen || light_cones) {
                paint_light_reach(&painter, &screen, world, marker, chosen);
            }
        }
        paint_lights(ui, rect, &lights, selected, pointer);
        crate::constraints::paint(
            &ui.painter_at(rect),
            &screen,
            world,
            selection,
            overlays.constraints,
        );
        if tool.is_some() {
            let adding = self
                .spline_drag
                .filter(|drag| drag.add)
                .and_then(|drag| Some((drag.point, drag.at?)));
            crate::splines::paint(
                &ui.painter_at(rect),
                &screen,
                world,
                spline_editing,
                selection,
                adding,
            );
        }
        if overlays.any() {
            paint_debug_overlays(
                &ui.painter_at(rect),
                &screen,
                world,
                assets,
                overlays,
                debug_rays,
                eye,
            );
        }
        if overlays.nav_mesh
            && let Some(mesh) = nav_mesh
        {
            paint_nav_mesh(&ui.painter_at(rect), &screen, mesh, eye);
        }
        if overlays.agent_paths {
            paint_agent_paths(&ui.painter_at(rect), &screen, agent_paths);
        }
        if !plugin_drawings.is_empty() {
            plugin_drawings::paint(&ui.painter_at(rect), &screen, &plugin_drawings);
        }
        if !plugin_handles.is_empty() {
            plugin_handles::paint_handles(
                &ui.painter_at(rect),
                &screen,
                &plugin_handles,
                self.handle_drag,
            );
        }
        if let Some(physics) = physics {
            paint_physics(&ui.painter_at(rect), &screen, physics, overlays);
        }
        if overlays.streaming {
            self.streaming_overlay.paint(ui, rect, &screen, world, eye);
        } else {
            self.streaming_overlay.clear();
            if game_playing && let Some(settings) = self.terrain.streaming.settings() {
                paint_streaming_radius(&ui.painter_at(rect), &screen, settings, eye);
            }
        }
        if overlays.light_probes {
            probe_overlay::paint(ui, rect, &screen, world, assets, stale_light_probes, eye);
        }
        if legend::paint(ui, rect, view_mode, &render.stats, self.overdraw.as_ref()) {
            self.measure_overdraw = true;
        }
        // Hiding the badge also ends its Compare, so nothing stays hidden
        // with no way back.
        if !show_ray_badge {
            self.rays_hidden = false;
        }
        if let Some(badge) = show_ray_badge
            .then(|| {
                ray_badge::badge(
                    target.renderer.graphics(),
                    graphics.ray_tracing(),
                    axcenity_render::adapters::adapter_can_ray_trace(),
                    axcenity_render::adapters::ray_tracing_override(),
                    self.rays_hidden,
                    &render.stats.ray_tracing,
                )
            })
            .flatten()
        {
            match ray_badge::paint(ui, rect, &badge) {
                Some(ray_badge::Click::Restart) => self.restart_requested = true,
                Some(ray_badge::Click::Compare) => self.rays_hidden = !self.rays_hidden,
                None => {}
            }
        }
        if let Some(frozen) = self.frozen_culling {
            paint_culling(
                &ui.painter_at(rect),
                &screen,
                target.renderer.culling_bounds(),
                frozen,
                camera,
                width as f32 / height.max(1) as f32,
            );
        }
        let painter = ui.painter_at(rect);
        for &pose in preview {
            for (a, b) in crate::angular::box_edges(pose) {
                if let Some(line) = screen.line(a, b) {
                    painter.line_segment(line, egui::Stroke::new(1.5_f32, PREVIEW_COLOR));
                }
            }
        }
        gizmo::paint(&ui.painter_at(rect), &drag.shapes, drag.highlighted);
        if move_guide && let Some(report) = self.drag_report() {
            move_overlay::paint(&ui.painter_at(rect), &screen, rect, report);
        }
        if let (Some(brush), Some(point)) = (brush, terrain.and_then(|t| t.point)) {
            paint_terrain_brush(&ui.painter_at(rect), &screen, world, brush, point);
        }
        // Handles only show with a tool, so not during Play, but the
        // outline always does.
        if let Some(handles) = &handles {
            let painter = ui.painter_at(rect);
            match tool {
                Some(_) => {
                    handles.paint(&painter, SELECTION_COLOR, drag.ui_highlighted);
                    let parent = world
                        .get(handles.element.id)
                        .and_then(|instance| instance.parent())
                        .and_then(|id| ui_layout.element(id))
                        .map(|element| {
                            egui::Rect::from_min_size(
                                rect.min
                                    + egui::vec2(element.rect.x, element.rect.y) / pixels_per_point,
                                egui::vec2(element.rect.width, element.rect.height)
                                    / pixels_per_point,
                            )
                        });
                    handles.paint_measures(&painter, rect, parent, pixels_per_point);
                }
                None => {
                    painter.rect_stroke(
                        handles.rect,
                        0.0,
                        egui::Stroke::new(SELECTION_WIDTH, SELECTION_COLOR),
                        egui::StrokeKind::Middle,
                    );
                }
            }
        }
        if let Some(edit) = &drag.ui_edit {
            ui_handles::paint_guides(
                &edit.guides,
                &ui.painter_at(rect),
                rect.min,
                pixels_per_point,
            );
        }
        // Printed distances show only while a part is moving.
        let moving = self
            .drag_report()
            .is_some_and(|report| report.measures_studs());
        if let Some(spec) = grid_spec.as_ref().filter(|_| moving) {
            self.grid_labels.update(
                world,
                spec,
                &screen,
                camera.position,
                grid::focus(spec, &screen, rect, camera.position),
                ui.input(|input| input.time),
            );
            self.grid_labels
                .paint(&ui.painter_at(rect), &screen, rect, spec);
        }
        self.paint_speed_popup(ui, rect);
        // While editing, the view cube says which way the camera faces, and
        // a click on a face, edge or corner looks from that side.
        if cube {
            view_cube::paint(ui, rect, self.camera.forward(), cube_hit);
            if let Some(hit) = cube_hit.filter(|_| response.clicked()) {
                let side = match hit {
                    view_cube::CubeHit::Side(side) => side,
                    view_cube::CubeHit::Home => view_cube::home_side(),
                };
                // Swung round smoothly, as in Fusion; at once when motion
                // is reduced.
                let to = view_cube::looking_from(&self.camera, side);
                let seconds = if ui.ctx().global_style().animation_time > 0.0 {
                    view_cube::SWING_SECONDS
                } else {
                    0.0
                };
                let now = ui.input(|input| input.time);
                self.swing = Some(view_cube::Swing::new(self.camera, to, now, seconds));
                ui.ctx().request_repaint();
            }
        }
        let drop = self.drop_target(ui, rect, &screen, world, &ui_scene);
        if let Some(target) = &drop
            && let Some(at) = screen.point(target.point)
        {
            let painter = ui.painter_at(rect);
            painter.circle_stroke(at, 9.0, egui::Stroke::new(SELECTION_WIDTH, SELECTION_COLOR));
            painter.circle_filled(at, 3.0, SELECTION_COLOR);
        }
        let surface = picking.and_then(|first| {
            self.surface_input(ui, &response, rect, &screen, world, (first, snap.points))
        });
        let plugin_mouse = plugin_mouse
            .filter(|_| play_camera.is_none())
            .and_then(|_| self.plugin_input(ui, &response, &screen, world));
        ViewportOutput {
            clicked,
            activated,
            game_input,
            ui_events: self.ui.take_events(),
            text_box,
            viewport_clicked: playing,
            ui_warnings: ui_layout.warnings,
            scrolled,
            widget_values,
            drag: drag.edit,
            ui_drag: drag.ui_edit,
            drag_released: drag.released || spline_released,
            drop,
            terrain,
            surface,
            spline,
            plugin_mouse,
            plugin_handle,
            render: Some(render),
            shader_errors,
        }
    }

    /// The camera as it draws into `rect`, for turning between screen and
    /// world positions.
    fn screen(&self, camera: axcenity_render::Camera, rect: egui::Rect) -> Screen {
        Screen::new(camera, rect)
    }

    /// Shows the flying speed in the middle of the viewport for a moment
    /// after it changes, then fades it out.
    fn paint_speed_popup(&self, ui: &egui::Ui, rect: egui::Rect) {
        let Some(changed_at) = self.speed_changed_at else {
            return;
        };
        let opacity = popup_opacity(ui.input(|input| input.time) - changed_at);
        if opacity <= 0.0 {
            return;
        }
        // egui only redraws when something happens, so ask for the frames
        // that keep the pop-up up and fade it.
        ui.ctx().request_repaint();

        let painter = ui.painter_at(rect);
        // A dark scrim, not the theme's colours: it has to read over
        // whatever the scene shows.
        let galley = painter.layout_no_wrap(
            speed_label(self.camera.speed()),
            egui::FontId::new(15.0, crate::theme::medium_family()),
            egui::Color32::WHITE.gamma_multiply(opacity),
        );
        let background =
            egui::Rect::from_center_size(rect.center(), galley.size() + egui::vec2(28.0, 14.0));
        let shade = egui::Color32::from_black_alpha((160.0 * opacity) as u8);
        painter.rect_filled(background, 8.0, shade);
        painter.rect_stroke(
            background,
            8.0,
            egui::Stroke::new(
                1.0_f32,
                egui::Color32::from_white_alpha((28.0 * opacity) as u8),
            ),
            egui::StrokeKind::Inside,
        );
        painter.galley(
            rect.center() - galley.size() / 2.0,
            galley,
            egui::Color32::WHITE,
        );
    }
}

/// Where `ray` meets the ground of the Terrain `terrain`.
pub fn ground_point(world: &World, terrain: InstanceId, ray: axcenity_scene::Ray) -> Option<Vec3> {
    let placement = axcenity_terrain::Placement::of(world.get(terrain)?)?;
    let data = world.terrain(terrain)?;
    let distance = axcenity_terrain::raycast::raycast(data, &placement, ray.origin, ray.direction)?;
    Some(ray.at(distance))
}

/// Seconds since the editor started, for surface shaders that move,
/// clouds the wind carries and plants it sways. While `list` has any of
/// them, frames keep being drawn.
pub fn shader_time(ui: &egui::Ui, list: &axcenity_render::DrawList) -> f32 {
    let windy = list.wind.x != 0.0 || list.wind.z != 0.0;
    let moving = list.clouds.is_moving()
        || list
            .items
            .iter()
            .any(|item| item.material.surface_shader.is_some() || (windy && item.sway.is_some()));
    if moving {
        // Swaying plants and drifting clouds don't need every frame the
        // display can show: while nobody touches the editor they're drawn
        // at `SCENERY_FPS`, sparing the GPU. Input brings full-rate frames
        // back (`EditorApp::keep_frames_steady`).
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_secs_f32(1.0 / SCENERY_FPS));
    }
    ui.input(|input| input.time) as f32
}

/// Frames a second for scenery that moves on its own in an idle editor.
const SCENERY_FPS: f32 = 30.0;

/// How far a part must be from how it started to count as changed.
const CHANGED: f32 = 0.0005;

/// While parts are being moved, resized or turned: where each is now and
/// what turns its transform now into the one it started with.
fn moving_parts(drag: Option<&Dragging>, world: &World) -> Vec<(Vec3, Mat4)> {
    let now_of = |id: InstanceId| Pose::of(world.get(id)?);
    let starts: Vec<(InstanceId, Pose)> = match drag {
        Some(Dragging::Handle(drag)) => drag.starts().to_vec(),
        Some(Dragging::Body(place, others)) => std::iter::once((place.part, place.start))
            .chain(others.iter().copied())
            .filter_map(|(id, position)| {
                Some((
                    id,
                    Pose {
                        position,
                        ..now_of(id)?
                    },
                ))
            })
            .collect(),
        _ => return Vec::new(),
    };
    let matrix =
        |pose: Pose| Mat4::from_scale_rotation_translation(pose.size, pose.rotation, pose.position);
    starts
        .into_iter()
        .filter_map(|(id, start)| {
            let now = now_of(id)?;
            let changed = now.position.distance(start.position) >= CHANGED
                || (now.size - start.size).length() >= CHANGED
                || now.rotation.angle_between(start.rotation) >= CHANGED;
            changed.then(|| (now.position, matrix(start) * matrix(now).inverse()))
        })
        .collect()
}

fn start_camera() -> FlyCamera {
    FlyCamera::looking_at(START_CAMERA_POSITION, Vec3::ZERO)
}

/// Where a newly opened project's camera starts, as the renderer takes it.
pub fn start_view() -> axcenity_render::Camera {
    start_camera().to_render_camera()
}

/// How visible the speed pop-up is, from 1 (fully) to 0 (gone), `age`
/// seconds after the speed last changed.
/// The egui cursor for a CSS cursor name, as a ClickDetector's
/// `CursorIcon` gives it. A name it doesn't know is the pointing hand.
fn css_cursor(name: &str) -> egui::CursorIcon {
    match name.trim().to_ascii_lowercase().as_str() {
        "default" | "auto" => egui::CursorIcon::Default,
        "none" => egui::CursorIcon::None,
        "help" => egui::CursorIcon::Help,
        "wait" => egui::CursorIcon::Wait,
        "progress" => egui::CursorIcon::Progress,
        "crosshair" => egui::CursorIcon::Crosshair,
        "cell" => egui::CursorIcon::Cell,
        "text" => egui::CursorIcon::Text,
        "move" => egui::CursorIcon::Move,
        "not-allowed" => egui::CursorIcon::NotAllowed,
        "grab" => egui::CursorIcon::Grab,
        "grabbing" => egui::CursorIcon::Grabbing,
        "zoom-in" => egui::CursorIcon::ZoomIn,
        "zoom-out" => egui::CursorIcon::ZoomOut,
        _ => egui::CursorIcon::PointingHand,
    }
}

fn popup_opacity(age: f64) -> f32 {
    let fading = (age - POPUP_SECONDS) / POPUP_FADE_SECONDS;
    (1.0 - fading).clamp(0.0, 1.0) as f32
}

/// The pop-up text, with a decimal only for slow speeds where it matters.
fn speed_label(speed: f32) -> String {
    if speed < 10.0 {
        format!("Fly speed: {speed:.1} studs/s")
    } else {
        format!("Fly speed: {speed:.0} studs/s")
    }
}

/// What's streamed while editing, once a frame outside Play. With Stream
/// While Editing on (roadmap R16.3), what Play would leave out around the
/// camera isn't drawn, but what's selected always is, so nothing being
/// edited vanishes. Off, editing shows everything, whatever Play streamed,
/// but the hidden data layers (R16.2).
fn stream_editing(
    streaming: &mut axcenity_scene::Streaming,
    world: &World,
    on: bool,
    focus: Vec3,
    seconds: f64,
    selection: &[InstanceId],
) {
    if on {
        streaming.step(world, focus, seconds);
        streaming.show_anyway(world, selection.iter().copied());
    } else if streaming.settings().is_some() {
        // Editing shows everything, whatever Play streamed, but the hidden
        // data layers.
        let layers = streaming.take_also_hidden();
        *streaming = axcenity_scene::Streaming::default();
        streaming.set_also_hidden(layers);
    }
}

/// Records a movement key press or release.
fn set_move_key(keys: &mut MoveKeys, key: egui::Key, pressed: bool) {
    let held = match key {
        egui::Key::W => &mut keys.forward,
        egui::Key::S => &mut keys.back,
        egui::Key::A => &mut keys.left,
        egui::Key::D => &mut keys.right,
        egui::Key::E => &mut keys.up,
        egui::Key::Q => &mut keys.down,
        _ => return,
    };
    *held = pressed;
}

/// While looking around, hides the cursor and keeps it in the window.
fn capture_mouse(ctx: &egui::Context, captured: bool) {
    // macOS can only hold the cursor still (`Locked`); Windows and X11 can
    // only keep it inside the window (`Confined`). Wayland can do both.
    let grab = match (captured, cfg!(target_os = "macos")) {
        (false, _) => egui::CursorGrab::None,
        (true, true) => egui::CursorGrab::Locked,
        (true, false) => egui::CursorGrab::Confined,
    };
    ctx.send_viewport_cmd(egui::ViewportCommand::CursorGrab(grab));
    ctx.send_viewport_cmd(egui::ViewportCommand::CursorVisible(!captured));
}

#[cfg(test)]
mod tests;
