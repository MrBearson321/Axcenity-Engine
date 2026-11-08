//! The Move, Resize (`Scale`) and Rotate tools: handles drawn over the
//! selection, and what dragging one does to every selected part.
//!
//! - **Move**: an arrow along each axis. Dragging one slides the parts
//!   along it. With Global axes the arrows are the world's, from the middle
//!   of the selection; with Local or Last they're the last selected part's
//!   own (see [`MoveAxes`]).
//! - **Resize**: a ball on each face of the last selected part, along its
//!   own axes. Dragging one moves that face of every part (along each
//!   part's own axis), and the opposite face stays where it is, or with
//!   Both directions both faces move.
//! - **Rotate**: a ring around each axis. Dragging one turns the parts
//!   around it, through the pivot (see [`RotatePivot`]). A ring that faces
//!   the camera follows the mouse round: the part turns as far as the mouse
//!   has gone around the ring's middle, whatever path it took. A ring seen
//!   nearly edge-on can't say where around it the mouse is, so it turns by
//!   how far the mouse moves along it, at a steady [`TURN_PER_POINT`].
//!   Holding Shift turns at a quarter of the speed.
//! - **Transform**: all three at once, like Studio's: the rings, the Resize
//!   balls, and Move arrows reaching out past the rings.
//!
//! With snapping on, moves and size changes go in steps of `Snap::move_step`
//! studs and turns in steps of `Snap::rotate_step` degrees, counted from
//! where the drag started (like Studio), so a part off the grid stays off it.
//! With Surfaces on, a move or resize that comes near a face of a part in
//! its way lands flush with it instead; with Collisions on, parts stop at
//! solid parts instead of going into them (see `snapping.rs`).
//!
//! The handles keep the same size on screen however far away the part is.
//! Everything here is plain math on rays and points, so tests can drive a
//! drag without a window; `viewport.rs` feeds it the mouse as a [`Pointer`].

use axcenity_core::{Instance, InstanceId, Value, Vector3};
use axcenity_render::glam::{Quat, Vec3};
use axcenity_scene::{Ray, orientation_from_rotation, rotation_from_orientation};
use eframe::egui;

use crate::properties::MIN_SIZE;
use crate::screen::Screen;
use crate::snapping::points::{self, DragKind, DragReport, Freedom, Sites, SnapPoint};
use crate::snapping::{self, Obb, Obstacle};

/// How long the Move arrows and how big the Rotate rings are, in points.
const HANDLE_POINTS: f32 = 90.0;
/// How far the Scale balls sit out from the part's faces, in points.
const SCALE_HANDLE_POINTS: f32 = 35.0;
/// How close (in points) the mouse must be to a handle to grab it.
const GRAB_POINTS: f32 = 8.0;
/// Handles shorter than this on screen point almost straight at the camera,
/// where dragging along them can't work, so they're hidden.
const MIN_VISIBLE_POINTS: f32 = 12.0;
/// The Rotate rings are drawn as this many straight pieces.
const RING_PIECES: usize = 64;
const ARROW_LENGTH: f32 = 14.0;
const ARROW_HALF_WIDTH: f32 = 6.0;
const BALL_RADIUS: f32 = 6.0;
const LINE_WIDTH: f32 = 3.0;
/// The handle under the mouse, or being dragged.
const HIGHLIGHT: egui::Color32 = egui::Color32::from_rgb(255, 215, 0);
/// Rays this close to parallel with a handle's line can't say where along
/// it the mouse is.
const PARALLEL: f32 = 1e-3;
/// How far (radians) a ring turns for each point the mouse moves along it:
/// one ring radius of travel turns one radian, so on a ring facing the
/// camera the mouse keeps up with the ring.
pub const TURN_PER_POINT: f32 = 1.0 / HANDLE_POINTS;
/// Holding Shift turns at this much of the speed.
pub const FINE_TURN: f32 = 0.25;
/// A ring follows the mouse round when the camera looks along its axis by
/// at least this much (the cosine of the angle between them): below it the
/// ring is too thin an ellipse to say where around it the mouse is.
const FOLLOW_MIN_FACING: f32 = 0.3;
/// How far from a ring's middle the mouse must be, as a share of its
/// radius, for its angle to count: right by the middle, a tiny movement
/// would swing the angle right round.
const FOLLOW_DEAD_ZONE: f32 = 0.12;
/// With the Transform tool, how far the Move arrows reach, as a share of
/// [`HANDLE_POINTS`]: they start at the rings and end out past them.
const TRANSFORM_ARROW_REACH: f32 = 1.4;
/// With Collisions, how far apart (radians, about 2°) the angles a turn
/// checks on its way are.
const TURN_CHECK_STEP: f32 = 0.035;

/// The tool picked in the toolbar. Select only selects; Move, Resize
/// (`Scale`), Rotate and Transform (all three) also show handles on the
/// selection, and the others
/// (modelled on F3X Building Tools) show an options panel in the viewport
/// that acts on every selected part (see `build_tools.rs`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tool {
    #[default]
    Select,
    Move,
    Scale,
    Rotate,
    /// Move, Resize and Rotate's handles together, like Studio's Transform.
    Transform,
    Paint,
    Material,
    Anchor,
    Lighting,
    Emission,
    /// Places copies of a part at an angle, like Archimedes (`angular.rs`).
    Angular,
}

impl Tool {
    /// Select, Move, Resize, Rotate and Transform: the tools on the Home tab.
    pub const TRANSFORM: [Tool; 5] = [
        Tool::Select,
        Tool::Move,
        Tool::Scale,
        Tool::Rotate,
        Tool::Transform,
    ];

    /// Paint through Angular: the tools on the Model tab.
    pub const BUILD: [Tool; 6] = [
        Tool::Paint,
        Tool::Material,
        Tool::Anchor,
        Tool::Lighting,
        Tool::Emission,
        Tool::Angular,
    ];

    pub const ALL: [Tool; 11] = [
        Tool::Select,
        Tool::Move,
        Tool::Scale,
        Tool::Rotate,
        Tool::Transform,
        Tool::Paint,
        Tool::Material,
        Tool::Anchor,
        Tool::Lighting,
        Tool::Emission,
        Tool::Angular,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Tool::Select => "Select",
            Tool::Move => "Move",
            Tool::Scale => "Resize",
            Tool::Rotate => "Rotate",
            Tool::Transform => "Transform",
            Tool::Paint => "Paint",
            Tool::Material => "Material",
            Tool::Anchor => "Anchor",
            Tool::Lighting => "Lighting",
            Tool::Emission => "Emission",
            Tool::Angular => "Angular",
        }
    }

    const _TOOLS_SPLIT: () = assert!(Self::TRANSFORM.len() + Self::BUILD.len() == Self::ALL.len());

    /// The single key that picks the tool, pressed on its own while
    /// editing (not typing, not during Play): 1 to 5 for the tools with
    /// handles, as in Studio, and F3X's letters for the building tools.
    /// Escape goes back to Select too (see `EditorApp::shortcuts`).
    pub fn key(self) -> Option<egui::Key> {
        Some(match self {
            Tool::Select => egui::Key::Num1,
            Tool::Move => egui::Key::Num2,
            Tool::Scale => egui::Key::Num3,
            Tool::Rotate => egui::Key::Num4,
            Tool::Transform => egui::Key::Num5,
            Tool::Paint => egui::Key::V,
            Tool::Material => egui::Key::N,
            Tool::Anchor => egui::Key::M,
            Tool::Lighting => egui::Key::U,
            Tool::Emission => egui::Key::P,
            // Not an F3X tool; R is free in F3X and in the camera keys.
            Tool::Angular => egui::Key::R,
        })
    }

    /// True for the tools with handles to drag.
    pub fn has_handles(self) -> bool {
        matches!(
            self,
            Tool::Move | Tool::Scale | Tool::Rotate | Tool::Transform
        )
    }
}

/// The snapping settings in the toolbar. Scale uses the move step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Snap {
    pub move_on: bool,
    /// In studs.
    pub move_step: f32,
    pub rotate_on: bool,
    /// In degrees.
    pub rotate_step: f32,
    /// Moves and resizes land flush with (or in line with) the faces of
    /// parts near them.
    pub surfaces: bool,
    /// Moved, resized and turned parts stop at solid parts instead of
    /// going into them.
    pub collisions: bool,
    /// When a drag ends, the parts it moved are welded to the parts
    /// they're left touching.
    pub join: bool,
    /// Markers show on the parts around a move, and its corners, edge and
    /// face middles and Attachments click to theirs.
    pub points: bool,
}

impl Default for Snap {
    /// Studio's defaults: 1 stud and 15 degrees, both on, and parts snap
    /// to each other's faces; Collisions and Join Surfaces start off.
    fn default() -> Self {
        Self {
            move_on: true,
            move_step: 1.0,
            rotate_on: true,
            rotate_step: 15.0,
            surfaces: true,
            collisions: false,
            join: false,
            points: true,
        }
    }
}

impl Snap {
    /// The smallest steps the toolbar allows, so a step of 0 can't freeze a drag.
    pub const MIN_MOVE_STEP: f32 = MIN_SIZE;
    pub const MIN_ROTATE_STEP: f32 = 0.1;

    /// Snapping turned the other way for both moves and turns, as holding
    /// Ctrl does while posing; the steps stay as they are.
    pub fn flipped(self) -> Self {
        Self {
            move_on: !self.move_on,
            rotate_on: !self.rotate_on,
            ..self
        }
    }

    /// `distance` rounded to the move step, if move snapping is on.
    pub fn grid_step(&self) -> f32 {
        if self.move_on { self.move_step } else { 1.0 }
    }

    pub fn studs(&self, distance: f32) -> f32 {
        snap(distance, self.move_on, self.move_step)
    }

    fn degrees(&self, angle: f32) -> f32 {
        snap(angle, self.rotate_on, self.rotate_step)
    }
}

/// `value` rounded to the nearest whole number of `step`s, if snapping is on.
fn snap(value: f32, on: bool, step: f32) -> f32 {
    if on && step > 0.0 {
        (value / step).round() * step
    } else {
        value
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
    Z,
}

impl Axis {
    const ALL: [Axis; 3] = [Axis::X, Axis::Y, Axis::Z];

    fn vector(self) -> Vec3 {
        match self {
            Axis::X => Vec3::X,
            Axis::Y => Vec3::Y,
            Axis::Z => Vec3::Z,
        }
    }

    /// Where this axis is in a `Vec3`, so `size[axis.index()]` is its size.
    fn index(self) -> usize {
        match self {
            Axis::X => 0,
            Axis::Y => 1,
            Axis::Z => 2,
        }
    }

    /// Red, green and blue for X, Y and Z, as in most 3D tools.
    fn color(self) -> egui::Color32 {
        match self {
            Axis::X => egui::Color32::from_rgb(235, 65, 65),
            Axis::Y => egui::Color32::from_rgb(70, 205, 70),
            Axis::Z => egui::Color32::from_rgb(55, 125, 245),
        }
    }

    /// Two directions across a ring around this axis, a quarter turn apart,
    /// so that turning from the first toward the second is a positive turn
    /// around the axis (the right-hand rule).
    fn ring_directions(self) -> (Vec3, Vec3) {
        match self {
            Axis::X => (Vec3::Y, Vec3::Z),
            Axis::Y => (Vec3::Z, Vec3::X),
            Axis::Z => (Vec3::X, Vec3::Y),
        }
    }
}

/// Which face a Scale handle sits on: the axis's positive or negative side.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Plus,
    Minus,
}

impl Side {
    fn sign(self) -> f32 {
        match self {
            Side::Plus => 1.0,
            Side::Minus => -1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handle {
    Move(Axis),
    Scale(Axis, Side),
    Rotate(Axis),
}

impl Handle {
    fn axis(self) -> Axis {
        match self {
            Handle::Move(axis) | Handle::Scale(axis, _) | Handle::Rotate(axis) => axis,
        }
    }
}

/// Where a part is, how it's turned and how big it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub position: Vec3,
    pub rotation: Quat,
    pub size: Vec3,
}

impl Pose {
    /// The pose of a Part, or `None` for anything without a position,
    /// orientation and size.
    pub fn of(instance: &Instance) -> Option<Pose> {
        let vector = |name| instance.property(name).and_then(Value::as_vector3);
        Some(Pose {
            position: to_vec3(vector("Position")?),
            rotation: rotation_from_orientation(vector("Orientation")?),
            size: to_vec3(vector("Size")?),
        })
    }
}

fn to_vec3(v: Vector3) -> Vec3 {
    Vec3::new(v.x, v.y, v.z)
}

fn to_value(v: Vec3) -> Value {
    Value::Vector3(Vector3::new(v.x, v.y, v.z))
}

/// A handle as it shows in the viewport.
#[derive(Clone, Debug, PartialEq)]
pub struct Shape {
    pub handle: Handle,
    /// Where the handles it's one of are (see [`handle_frame`]): a drag
    /// that grabs it starts from here.
    pub frame: Pose,
    lines: Vec<[egui::Pos2; 2]>,
    end: End,
}

impl Shape {
    /// The pieces of line the handle is drawn with.
    #[cfg(test)]
    pub fn lines(&self) -> &[[egui::Pos2; 2]] {
        &self.lines
    }
}

/// What's drawn at the end of a handle's line.
#[derive(Clone, Copy, Debug, PartialEq)]
enum End {
    /// An arrowhead at `tip`, pointing `toward` (one point long).
    Arrow {
        tip: egui::Pos2,
        toward: egui::Vec2,
    },
    Ball(egui::Pos2),
    None,
}

/// Which way the Move handles point, as in F3X: the world's axes (from the
/// middle of the selection), or the last selected part's own axes, with
/// each part moving along its own axes (Local) or all along the last
/// part's (Last).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MoveAxes {
    #[default]
    Global,
    Local,
    Last,
}

/// Whether dragging a Resize handle moves one face (Normal) or both
/// opposite faces, keeping the center (Both), as in F3X.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ResizeDirections {
    #[default]
    Normal,
    Both,
}

/// What the Rotate tool turns parts around, as in F3X: the middle of the
/// selection on the world's axes (Center), each part's own center and axes
/// (Local), or the last selected part's center and axes (Last).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RotatePivot {
    #[default]
    Center,
    Local,
    Last,
}

/// The tool options that change what the handles do.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modes {
    pub axes: MoveAxes,
    pub directions: ResizeDirections,
    pub pivot: RotatePivot,
}

/// Where `tool`'s handles go for the selected parts `poses` (the last one
/// is the last selected): a center and the axes they follow, with the last
/// part's size for the Resize balls. Transform's handles each go where
/// their own tool's would (see [`transform_shapes`]); this is its arrows'.
pub fn handle_frame(tool: Tool, modes: Modes, poses: &[Pose]) -> Option<Pose> {
    let last = *poses.last()?;
    let middle = || Pose {
        position: middle_of(poses),
        rotation: Quat::IDENTITY,
        size: last.size,
    };
    Some(match tool {
        Tool::Move | Tool::Transform if modes.axes == MoveAxes::Global => middle(),
        Tool::Rotate if modes.pivot == RotatePivot::Center => middle(),
        _ => last,
    })
}

/// The middle of the world-aligned box around the parts at `poses`.
pub fn middle_of(poses: &[Pose]) -> Vec3 {
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for pose in poses {
        let half = pose.size / 2.0;
        let reach = [Vec3::X, Vec3::Y, Vec3::Z]
            .map(|axis| pose.rotation * axis)
            .iter()
            .zip([half.x, half.y, half.z])
            .fold(Vec3::ZERO, |sum, (edge, half)| sum + edge.abs() * half);
        min = min.min(pose.position - reach);
        max = max.max(pose.position + reach);
    }
    (min + max) / 2.0
}

/// The handles `tool` shows for the selected parts `poses` (the last one
/// is the last selected) with `modes`, as seen through `screen`. Each of
/// Transform's goes where its own tool's would.
pub fn shapes_for(tool: Tool, modes: Modes, poses: &[Pose], screen: &Screen) -> Vec<Shape> {
    if tool == Tool::Transform {
        return transform_shapes(modes, poses, screen);
    }
    handle_frame(tool, modes, poses).map_or_else(Vec::new, |frame| shapes(tool, frame, screen))
}

/// The Transform tool's handles: Resize's balls and Rotate's rings, each
/// where their own tool puts them, and Move's arrows out past the rings.
pub fn transform_shapes(modes: Modes, poses: &[Pose], screen: &Screen) -> Vec<Shape> {
    [Tool::Rotate, Tool::Scale, Tool::Transform]
        .into_iter()
        .filter_map(|tool| Some((tool, handle_frame(tool, modes, poses)?)))
        .flat_map(|(tool, frame)| handles_of(tool, frame, screen))
        .collect()
}

/// Where the handles `handle` is one of are, among `shapes`.
pub fn frame_of(shapes: &[Shape], handle: Handle) -> Option<Pose> {
    shapes
        .iter()
        .find(|shape| shape.handle == handle)
        .map(|shape| shape.frame)
}

/// The handles `tool` shows around `pose` (from [`handle_frame`]), as seen
/// through `screen`. Transform shows all three tools' around it.
pub fn shapes(tool: Tool, pose: Pose, screen: &Screen) -> Vec<Shape> {
    if tool == Tool::Transform {
        return [Tool::Rotate, Tool::Scale, Tool::Transform]
            .into_iter()
            .flat_map(|tool| handles_of(tool, pose, screen))
            .collect();
    }
    handles_of(tool, pose, screen)
}

/// `tool`'s handles around `pose`; for Transform, only its arrows.
fn handles_of(tool: Tool, pose: Pose, screen: &Screen) -> Vec<Shape> {
    let length = screen.studs_per_point(pose.position) * HANDLE_POINTS;
    let center = pose.position;
    let mut shapes = Vec::new();
    match tool {
        Tool::Move | Tool::Transform => {
            // Transform's arrows start at the rings, so they don't run
            // through the Resize balls inside them.
            let (from, to) = if tool == Tool::Transform {
                (1.0, TRANSFORM_ARROW_REACH)
            } else {
                (0.0, 1.0)
            };
            for axis in Axis::ALL {
                let direction = pose.rotation * axis.vector();
                let Some(whole) = screen.line(center, center + direction * length * to) else {
                    continue;
                };
                if (whole[1] - whole[0]).length() < MIN_VISIBLE_POINTS {
                    continue;
                }
                let Some(line) = screen.line(
                    center + direction * length * from,
                    center + direction * length * to,
                ) else {
                    continue;
                };
                shapes.push(Shape {
                    handle: Handle::Move(axis),
                    frame: pose,
                    lines: vec![line],
                    end: End::Arrow {
                        tip: line[1],
                        toward: (whole[1] - whole[0]).normalized(),
                    },
                });
            }
        }
        Tool::Scale => {
            let out = screen.studs_per_point(center) * SCALE_HANDLE_POINTS;
            for axis in Axis::ALL {
                for side in [Side::Plus, Side::Minus] {
                    let (_, direction) = handle_line(Handle::Scale(axis, side), pose);
                    let face = center + direction * pose.size[axis.index()] / 2.0;
                    let Some(line) = screen.line(face, face + direction * out) else {
                        continue;
                    };
                    if (line[1] - line[0]).length() < MIN_VISIBLE_POINTS / 2.0 {
                        continue;
                    }
                    shapes.push(Shape {
                        handle: Handle::Scale(axis, side),
                        frame: pose,
                        lines: vec![line],
                        end: End::Ball(line[1]),
                    });
                }
            }
        }
        Tool::Rotate => {
            for axis in Axis::ALL {
                let (across, up) = axis.ring_directions();
                let (across, up) = (pose.rotation * across, pose.rotation * up);
                let point = |i: usize| {
                    let angle = i as f32 / RING_PIECES as f32 * std::f32::consts::TAU;
                    center + (across * angle.cos() + up * angle.sin()) * length
                };
                let lines = (0..RING_PIECES)
                    .filter_map(|i| screen.line(point(i), point(i + 1)))
                    .collect();
                shapes.push(Shape {
                    handle: Handle::Rotate(axis),
                    frame: pose,
                    lines,
                    end: End::None,
                });
            }
        }
        // The other tools have no handles.
        _ => {}
    }
    shapes
}

/// The handle under `pointer`, if it's close enough to grab one. The nearest wins.
pub fn hovered(shapes: &[Shape], pointer: egui::Pos2) -> Option<Handle> {
    shapes
        .iter()
        .map(|shape| (shape.handle, distance_to_shape(shape, pointer)))
        .filter(|&(_, distance)| distance <= GRAB_POINTS)
        // `total_cmp` compares floats (which plain `cmp` can't, because of NaN).
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(handle, _)| handle)
}

fn distance_to_shape(shape: &Shape, pointer: egui::Pos2) -> f32 {
    let to_lines = shape
        .lines
        .iter()
        .map(|&[a, b]| distance_to_segment(pointer, a, b));
    let to_end = match shape.end {
        End::Arrow { tip, toward } => {
            distance_to_segment(pointer, tip, tip + toward * ARROW_LENGTH)
        }
        End::Ball(center) => (pointer.distance(center) - BALL_RADIUS).max(0.0),
        End::None => f32::INFINITY,
    };
    to_lines.fold(to_end, f32::min)
}

fn distance_to_segment(point: egui::Pos2, a: egui::Pos2, b: egui::Pos2) -> f32 {
    let along = b - a;
    let length_squared = along.length_sq();
    let t = if length_squared == 0.0 {
        0.0
    } else {
        ((point - a).dot(along) / length_squared).clamp(0.0, 1.0)
    };
    point.distance(a + along * t)
}

/// Draws the handles, with `highlighted` (hovered or being dragged) in yellow.
pub fn paint(painter: &egui::Painter, shapes: &[Shape], highlighted: Option<Handle>) {
    for shape in shapes {
        let color = if highlighted == Some(shape.handle) {
            HIGHLIGHT
        } else {
            shape.handle.axis().color()
        };
        let stroke = egui::Stroke::new(LINE_WIDTH, color);
        for &line in &shape.lines {
            painter.line_segment(line, stroke);
        }
        match shape.end {
            End::Arrow { tip, toward } => {
                let across = toward.rot90() * ARROW_HALF_WIDTH;
                let points = vec![tip + toward * ARROW_LENGTH, tip + across, tip - across];
                painter.add(egui::Shape::convex_polygon(
                    points,
                    color,
                    egui::Stroke::NONE,
                ));
            }
            End::Ball(center) => {
                painter.circle_filled(center, BALL_RADIUS, color);
            }
            End::None => {}
        }
    }
}

/// How far around a move markers show, as a multiple of the snap reach.
const MARKER_REACH: f32 = 5.0;

/// A handle being dragged.
#[derive(Clone, Debug)]
pub struct Drag {
    pub handle: Handle,
    /// Where the handles were when the drag started (see [`handle_frame`]).
    frame: Pose,
    /// Every part being changed, as it was when the drag started. Every
    /// frame works out the new poses from these, so snapping and rounding
    /// errors never add up.
    parts: Vec<(InstanceId, Pose)>,
    modes: Modes,
    /// Move and Scale: how far along the handle's line the mouse grabbed it.
    grabbed: f32,
    /// Rotate only: the way on screen (one point long) the ring turns
    /// forward where it was grabbed, where the mouse was last frame, and
    /// how far it has turned in all, in radians (which can go past a whole
    /// turn).
    spin: egui::Vec2,
    last_at: egui::Pos2,
    turned: f32,
    /// Rotate only: set when the ring faces the camera enough to follow
    /// the mouse round it.
    follow: Option<Follow>,
    /// The other parts, for Surfaces and Collisions (see [`Drag::among`]).
    obstacles: Vec<Obstacle>,
    /// How far (studs) a part is pulled to line up with a face.
    reach: f32,
    /// With Collisions, the last turn (radians) that didn't go into a
    /// solid part: each turn sweeps from here, so it can't pass through one.
    free_turn: f32,
    /// The Attachments around it, as snap points (see [`Drag::among`]).
    sites: Box<Sites>,
    /// A move's distance and markers, for drawing.
    report: Option<Box<DragReport>>,
}

/// A ring drag that follows the mouse round the ring.
#[derive(Clone, Copy, Debug)]
struct Follow {
    /// Where around the ring the mouse was last frame, in radians.
    last_angle: f32,
    /// How close to the ring's middle (studs) is too close to count.
    dead_zone: f32,
}

/// The mouse during a drag: the ray it points along, where it is in the
/// viewport, and whether Shift is held for fine turns.
#[derive(Clone, Copy, Debug)]
pub struct Pointer {
    pub ray: Ray,
    pub at: egui::Pos2,
    pub fine: bool,
}

impl Pointer {
    pub fn on(screen: &Screen, at: egui::Pos2, fine: bool) -> Self {
        Self {
            ray: screen.ray(at),
            at,
            fine,
        }
    }
}

/// Move and Resize drags only need the ray.
#[cfg(test)]
impl From<Ray> for Pointer {
    fn from(ray: Ray) -> Self {
        Self {
            ray,
            at: egui::Pos2::ZERO,
            fine: false,
        }
    }
}

/// New property values for each part a drag changes.
pub type PartValues = Vec<(InstanceId, Vec<(&'static str, Value)>)>;

impl Drag {
    /// Starts dragging the Move or Resize `handle` of one `part` (at
    /// `start`) where `ray` points, with world axes. `None` if the ray runs
    /// alongside the handle and can't grab it.
    #[cfg(test)]
    pub fn start(handle: Handle, part: InstanceId, start: Pose, ray: Ray) -> Option<Drag> {
        assert!(!matches!(handle, Handle::Rotate(_)), "rings use `turn`");
        // Resize handles follow the part's own axes; the others the world's.
        let frame = match handle {
            Handle::Scale(..) => start,
            _ => Pose {
                rotation: Quat::IDENTITY,
                ..start
            },
        };
        Self::begin(
            handle,
            frame,
            vec![(part, start)],
            Modes::default(),
            ray,
            None,
        )
    }

    /// Starts dragging the Rotate `handle` of one `part` (at `start`),
    /// grabbed at `at` in `screen`, with world axes.
    #[cfg(test)]
    pub fn turn(
        handle: Handle,
        part: InstanceId,
        start: Pose,
        screen: &Screen,
        at: egui::Pos2,
    ) -> Option<Drag> {
        let frame = Pose {
            rotation: Quat::IDENTITY,
            ..start
        };
        Self::start_many(
            handle,
            frame,
            vec![(part, start)],
            Modes::default(),
            screen,
            at,
        )
    }

    /// Starts dragging `handle`, placed at `frame`, for every one of
    /// `parts` (the last is the one the handles are on), grabbed at `at`
    /// in `screen`.
    pub fn start_many(
        handle: Handle,
        frame: Pose,
        parts: Vec<(InstanceId, Pose)>,
        modes: Modes,
        screen: &Screen,
        at: egui::Pos2,
    ) -> Option<Drag> {
        let spin = match handle {
            Handle::Rotate(axis) => Some((ring_spin(frame, axis, screen, at), at)),
            _ => None,
        };
        let mut drag = Self::begin(handle, frame, parts, modes, screen.ray(at), spin)?;
        if let Handle::Rotate(axis) = handle {
            drag.follow = ring_follow(frame, axis, screen, at);
        }
        Some(drag)
    }

    fn begin(
        handle: Handle,
        frame: Pose,
        parts: Vec<(InstanceId, Pose)>,
        modes: Modes,
        ray: Ray,
        spin: Option<(egui::Vec2, egui::Pos2)>,
    ) -> Option<Drag> {
        parts.last()?;
        let grabbed = match handle {
            Handle::Move(_) | Handle::Scale(..) => {
                let (origin, direction) = handle_line(handle, frame);
                along_line(origin, direction, ray)?
            }
            Handle::Rotate(_) => 0.0,
        };
        let (spin, last_at) = spin.unwrap_or((egui::Vec2::ZERO, egui::Pos2::ZERO));
        Some(Drag {
            handle,
            frame,
            parts,
            modes,
            grabbed,
            spin,
            last_at,
            turned: 0.0,
            follow: None,
            obstacles: Vec::new(),
            reach: 0.0,
            free_turn: 0.0,
            sites: Box::default(),
            report: None,
        })
    }

    /// The drag with the other parts around it (see `snapping::obstacles`),
    /// for Surfaces and Collisions, pulling faces into line from up to
    /// `reach` studs away.
    pub fn among(mut self, obstacles: Vec<Obstacle>, reach: f32) -> Self {
        self.obstacles = obstacles;
        self.reach = reach;
        self
    }

    /// The drag with the Attachments it can click to (see `points::sites`).
    pub fn with_sites(mut self, sites: Sites) -> Self {
        self.sites = Box::new(sites);
        self
    }

    /// Each part being changed and how it started.
    pub fn starts(&self) -> &[(InstanceId, Pose)] {
        &self.parts
    }

    /// How far a Move drag has come and what it can click to, once it's
    /// been updated.
    pub fn report(&self) -> Option<&DragReport> {
        self.report.as_deref()
    }

    /// Clicks a move of `boxes` by `raw` along `direction` to the nearest
    /// snap point in reach. Gives the new distance and the two points.
    fn click_to_point(&self, boxes: &[Obb], direction: Vec3, raw: f32) -> Option<(f32, Vec3)> {
        let by = direction * raw;
        let moved: Vec<Obb> = boxes.iter().map(|obb| obb.moved(by)).collect();
        let own = points::own_points(&moved, &self.sites.own, by);
        let around = points::nearby(&moved, &self.obstacles, &self.sites.others, self.reach);
        let link = points::nearest(&own, &around, Freedom::Along(direction), self.reach)?;
        Some((raw + link.shift.dot(direction), link.to))
    }

    /// Clicks the face of `last` being pulled `raw` studs to the nearest
    /// snap point in reach. Gives the new pull and the point.
    fn click_face_to_point(
        &self,
        last: Pose,
        (axis, side): (Axis, Side),
        raw: f32,
    ) -> Option<(f32, Vec3)> {
        let outward = last.rotation * axis.vector() * side.sign();
        let grown = Obb::of(resize(last, axis, side, raw, self.modes));
        let plane = grown.center.dot(outward) + grown.reach(outward);
        let face: Vec<SnapPoint> = grown
            .snap_points()
            .into_iter()
            .filter(|point| (point.at.dot(outward) - plane).abs() < 0.001)
            .collect();
        let around = points::nearby(&[grown], &self.obstacles, &self.sites.others, self.reach);
        let link = points::nearest(&face, &around, Freedom::Along(outward), self.reach)?;
        Some((raw + link.shift.dot(outward), link.to))
    }

    /// Notes how far a Resize drag has pulled its face.
    fn note_resize(
        &mut self,
        (axis, side): (Axis, Side),
        pulled: f32,
        link: Option<Vec3>,
        snap: &Snap,
    ) {
        let Some(&(_, last)) = self.parts.last() else {
            return;
        };
        let outward = last.rotation * axis.vector() * side.sign();
        let face = |pose: Pose| pose.position + outward * pose.size.dot(axis.vector()).abs() / 2.0;
        let grown = resize(last, axis, side, pulled, self.modes);
        let targets = if snap.points {
            let radius = self.reach * MARKER_REACH;
            points::nearby(
                &[Obb::of(grown)],
                &self.obstacles,
                &self.sites.others,
                radius,
            )
        } else {
            Vec::new()
        };
        let (start, moved) = (face(last), face(grown) - face(last));
        let mut report = DragReport::new(start, moved, None, targets, link);
        report.kind = DragKind::Resize {
            pulled: moved.dot(outward),
            outward,
        };
        self.report = Some(Box::new(report));
    }

    /// Notes how far a Rotate drag has turned.
    fn note_turn(&mut self, axis: Axis, radians: f32) {
        let Some(&(_, last)) = self.parts.last() else {
            return;
        };
        let (pivot, rotation) = match self.modes.pivot {
            RotatePivot::Local => (last.position, last.rotation),
            RotatePivot::Center | RotatePivot::Last => (self.frame.position, self.frame.rotation),
        };
        let mut report = DragReport::new(pivot, Vec3::ZERO, None, Vec::new(), None);
        report.kind = DragKind::Turn {
            pivot,
            axis: rotation * axis.vector(),
            from: rotation * axis.ring_directions().0,
            radians,
        };
        self.report = Some(Box::new(report));
    }

    /// Notes how far a Move drag has come, with the markers to show.
    fn note_move(&mut self, way: Vec3, moved: f32, link: Option<Vec3>, snap: &Snap) {
        let Some(&(_, last)) = self.parts.last() else {
            return;
        };
        let by = way * moved;
        let at = Obb::of(last).moved(by);
        let targets = if snap.points {
            // What's near enough to matter: about a hand's width on screen.
            let radius = self.reach * MARKER_REACH;
            points::nearby(&[at], &self.obstacles, &self.sites.others, radius)
        } else {
            Vec::new()
        };
        self.report = Some(Box::new(DragReport::new(
            last.position,
            by,
            Some(&at),
            targets,
            link,
        )));
    }

    /// Each part's box as the drag started.
    fn start_boxes(&self) -> Vec<Obb> {
        self.parts.iter().map(|(_, pose)| Obb::of(*pose)).collect()
    }

    /// A ring drag's forward way on screen, one point long.
    #[cfg(test)]
    pub fn spin(&self) -> egui::Vec2 {
        self.spin
    }

    /// The new property values of the part the handles are on, for the
    /// mouse now at `pointer`, or `None` if it can't tell.
    #[cfg(test)]
    pub fn update(
        &mut self,
        pointer: impl Into<Pointer>,
        snap: &Snap,
    ) -> Option<Vec<(&'static str, Value)>> {
        let part = self.parts.last()?.0;
        self.update_all(pointer.into(), snap)?
            .into_iter()
            .find(|(id, _)| *id == part)
            .map(|(_, values)| values)
    }

    /// The new property values of every part, for the mouse now at
    /// `pointer`, or `None` if it can't tell (its ray runs alongside a Move
    /// or Resize handle).
    pub fn update_all(&mut self, pointer: Pointer, snap: &Snap) -> Option<PartValues> {
        let ray = pointer.ray;
        let frame = self.frame;
        let modes = self.modes;
        match self.handle {
            Handle::Move(axis) => {
                let (origin, direction) = handle_line(self.handle, frame);
                let raw = along_line(origin, direction, ray)? - self.grabbed;
                let mut moved = snap.studs(raw);
                // Each part's own way along the axis.
                let directions: Vec<Vec3> = self
                    .parts
                    .iter()
                    .map(|(_, start)| match modes.axes {
                        MoveAxes::Local => start.rotation * axis.vector(),
                        MoveAxes::Global | MoveAxes::Last => direction,
                    })
                    .collect();
                let boxes = self.start_boxes();
                let mut link = None;
                // Flush faces only when every part goes the same way.
                if snap.surfaces
                    && modes.axes != MoveAxes::Local
                    && let Some(flush) =
                        snapping::flush_moves(&boxes, direction, raw, self.reach, &self.obstacles)
                {
                    moved = flush;
                }
                // A point beats a face: it's the more exact of the two.
                if snap.points
                    && modes.axes != MoveAxes::Local
                    && let Some((clicked, pair)) = self.click_to_point(&boxes, direction, raw)
                {
                    moved = clicked;
                    link = Some(pair);
                }
                if snap.collisions {
                    let wanted = moved;
                    // As far as the part that's stopped soonest can go.
                    moved = boxes
                        .iter()
                        .zip(&directions)
                        .map(|(obb, &way)| {
                            snapping::limit_travel(&[*obb], way, moved, &self.obstacles)
                        })
                        .min_by(|a, b| a.abs().total_cmp(&b.abs()))
                        .unwrap_or(moved);
                    // Stopped short: the two points aren't together.
                    if (moved - wanted).abs() > 1e-4 {
                        link = None;
                    }
                }
                let way = directions.last().copied().unwrap_or(direction);
                self.note_move(way, moved, link, snap);
                Some(
                    self.parts
                        .iter()
                        .zip(&directions)
                        .map(|(&(id, start), &way)| {
                            let position = start.position + way * moved;
                            (id, vec![("Position", to_value(position))])
                        })
                        .collect(),
                )
            }
            Handle::Scale(axis, side) => {
                let (origin, direction) = handle_line(self.handle, frame);
                let raw = along_line(origin, direction, ray)? - self.grabbed;
                let mut pulled = snap.studs(raw);
                let mut link = None;
                if snap.surfaces
                    && modes.directions == ResizeDirections::Normal
                    && let Some(&(_, last)) = self.parts.last()
                {
                    let outward = last.rotation * axis.vector() * side.sign();
                    if let Some(flush) = snapping::flush_faces(
                        &Obb::of(last),
                        outward,
                        raw,
                        self.reach,
                        &self.obstacles,
                    ) {
                        pulled = flush;
                    }
                }
                // A point beats a face: it's the more exact of the two.
                if snap.points
                    && modes.directions == ResizeDirections::Normal
                    && let Some(&(_, last)) = self.parts.last()
                    && let Some((clicked, point)) =
                        self.click_face_to_point(last, (axis, side), raw)
                {
                    pulled = clicked;
                    link = Some(point);
                }
                let resized = |pulled: f32| -> Vec<(InstanceId, Pose)> {
                    self.parts
                        .iter()
                        .map(|&(id, start)| (id, resize(start, axis, side, pulled, modes)))
                        .collect()
                };
                // Growing stops where a face meets a solid part; the most
                // it can grow is found by halving.
                if snap.collisions && pulled > 0.0 {
                    let boxes = self.start_boxes();
                    let blocked = |pulled: f32| {
                        let grown: Vec<Obb> = resized(pulled)
                            .into_iter()
                            .map(|(_, pose)| Obb::of(pose))
                            .collect();
                        snapping::collides(&boxes, &grown, &self.obstacles)
                    };
                    if blocked(pulled) {
                        let (mut free, mut hit) = (0.0, pulled);
                        for _ in 0..16 {
                            let middle = (free + hit) / 2.0;
                            if blocked(middle) {
                                hit = middle;
                            } else {
                                free = middle;
                            }
                        }
                        pulled = free;
                        link = None;
                    }
                }
                let values: PartValues = resized(pulled)
                    .into_iter()
                    .map(|(id, pose)| {
                        let values = vec![
                            ("Size", to_value(pose.size)),
                            ("Position", to_value(pose.position)),
                        ];
                        (id, values)
                    })
                    .collect();
                self.note_resize((axis, side), pulled, link, snap);
                Some(values)
            }
            Handle::Rotate(axis) => {
                // Only this frame's move counts at this frame's speed, so
                // pressing or letting go of Shift never makes the part jump.
                let speed = if pointer.fine { FINE_TURN } else { 1.0 };
                match &mut self.follow {
                    Some(follow) => {
                        // Off the ring's plane or by its middle, the part
                        // holds still until the mouse is somewhere that counts.
                        if let Some(angle) = ring_angle(frame, axis, ray, follow.dead_zone) {
                            self.turned += shortest_turn(angle - follow.last_angle) * speed;
                            follow.last_angle = angle;
                        }
                    }
                    None => {
                        self.turned +=
                            (pointer.at - self.last_at).dot(self.spin) * TURN_PER_POINT * speed;
                    }
                }
                self.last_at = pointer.at;
                let mut radians = snap.degrees(self.turned.to_degrees()).to_radians();
                let world_axis = frame.rotation * axis.vector();
                let turned_by = |radians: f32| -> Vec<(InstanceId, Pose, Pose)> {
                    self.parts
                        .iter()
                        .map(|&(id, start)| {
                            let (turn, pivot) = match modes.pivot {
                                RotatePivot::Local => (
                                    Quat::from_axis_angle(start.rotation * axis.vector(), radians),
                                    start.position,
                                ),
                                RotatePivot::Center | RotatePivot::Last => {
                                    (Quat::from_axis_angle(world_axis, radians), frame.position)
                                }
                            };
                            // `turn * start.rotation` turns the already-turned
                            // part further around the axis.
                            let pose = Pose {
                                position: pivot + turn * (start.position - pivot),
                                rotation: turn * start.rotation,
                                size: start.size,
                            };
                            (id, start, pose)
                        })
                        .collect()
                };
                // A turn stops at the last angle, on the way from the last
                // free one, that didn't go into a solid part.
                if snap.collisions {
                    let from: Vec<Obb> = self.start_boxes();
                    let blocked = |radians: f32| {
                        let to: Vec<Obb> = turned_by(radians)
                            .iter()
                            .map(|(_, _, pose)| Obb::of(*pose))
                            .collect();
                        snapping::collides(&from, &to, &self.obstacles)
                    };
                    let steps =
                        ((radians - self.free_turn).abs() / TURN_CHECK_STEP).ceil() as usize;
                    let mut free = self.free_turn;
                    for step in 1..=steps {
                        let at = self.free_turn
                            + (radians - self.free_turn) * step as f32 / steps as f32;
                        if blocked(at) {
                            break;
                        }
                        free = at;
                    }
                    radians = free;
                    self.free_turn = free;
                }
                let values: PartValues = turned_by(radians)
                    .into_iter()
                    .map(|(id, start, pose)| {
                        let orientation = orientation_from_rotation(pose.rotation);
                        let mut values = vec![("Orientation", Value::Vector3(orientation))];
                        if !pose.position.abs_diff_eq(start.position, 1e-5) {
                            values.push(("Position", to_value(pose.position)));
                        }
                        (id, values)
                    })
                    .collect();
                self.note_turn(axis, radians);
                Some(values)
            }
        }
    }
}

/// `start` with its face on `side` of `axis` pulled `pulled` studs out
/// (both opposite faces, with Both directions), as a Resize drag does.
fn resize(start: Pose, axis: Axis, side: Side, pulled: f32, modes: Modes) -> Pose {
    let i = axis.index();
    let outward = start.rotation * axis.vector() * side.sign();
    let mut size = start.size;
    let position = match modes.directions {
        ResizeDirections::Normal => {
            size[i] = (start.size[i] + pulled).max(MIN_SIZE);
            // The center moves half as far as the face, so the opposite
            // face stays put.
            start.position + outward * (size[i] - start.size[i]) / 2.0
        }
        ResizeDirections::Both => {
            size[i] = (start.size[i] + pulled * 2.0).max(MIN_SIZE);
            start.position
        }
    };
    Pose {
        position,
        size,
        ..start
    }
}

/// The line a Move or Scale handle drags along: through the part's center,
/// pointing the way the handle points (one stud long).
fn handle_line(handle: Handle, pose: Pose) -> (Vec3, Vec3) {
    let direction = match handle {
        Handle::Move(axis) | Handle::Rotate(axis) => pose.rotation * axis.vector(),
        // Scale handles use the part's own (turned) axes, like `Size` does.
        Handle::Scale(axis, side) => pose.rotation * axis.vector() * side.sign(),
    };
    (pose.position, direction)
}

/// How far along the line (from `origin`, going `direction`, one stud long)
/// is the point closest to `ray`. `None` if the ray is nearly parallel to it.
fn along_line(origin: Vec3, direction: Vec3, ray: Ray) -> Option<f32> {
    // The standard closest-points-of-two-lines formula, with both directions
    // one stud long: solve for where the line between them is at right
    // angles to both.
    let between = origin - ray.origin;
    let cos = direction.dot(ray.direction);
    let denominator = 1.0 - cos * cos;
    if denominator < PARALLEL {
        return None;
    }
    let t = (cos * ray.direction.dot(between) - direction.dot(between)) / denominator;
    t.is_finite().then_some(t)
}

/// Where the ring around `axis` of the handles at `frame` is drawn in
/// `screen`, `angle` (radians) around from its first direction.
#[cfg(test)]
pub fn ring_point(screen: &Screen, frame: Pose, axis: Axis, angle: f32) -> egui::Pos2 {
    let length = screen.studs_per_point(frame.position) * HANDLE_POINTS;
    let (across, up) = axis.ring_directions();
    let (across, up) = (frame.rotation * across, frame.rotation * up);
    let point = frame.position + (across * angle.cos() + up * angle.sin()) * length;
    screen.point(point).expect("in front of the camera")
}

/// Where around the ring for `axis` at `frame` the `ray` points, in radians
/// from its first direction, positive the way the ring turns forward. `None`
/// if the ray misses the ring's plane or lands within `dead_zone` studs of
/// its middle.
fn ring_angle(frame: Pose, axis: Axis, ray: Ray, dead_zone: f32) -> Option<f32> {
    let normal = frame.rotation * axis.vector();
    let toward = ray.direction.dot(normal);
    if toward.abs() < PARALLEL {
        return None;
    }
    let along = (frame.position - ray.origin).dot(normal) / toward;
    if along <= 0.0 {
        return None;
    }
    let out = ray.at(along) - frame.position;
    let (across, up) = axis.ring_directions();
    let (x, y) = (
        out.dot(frame.rotation * across),
        out.dot(frame.rotation * up),
    );
    (x * x + y * y >= dead_zone * dead_zone).then(|| y.atan2(x))
}

/// `angle` (radians) as the shortest way round: from -π to π.
fn shortest_turn(angle: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    (angle + PI).rem_euclid(TAU) - PI
}

/// How a ring drag grabbed at `at` follows the mouse, or `None` when the
/// ring is too near edge-on to.
fn ring_follow(frame: Pose, axis: Axis, screen: &Screen, at: egui::Pos2) -> Option<Follow> {
    let middle = screen.point(frame.position)?;
    let facing = screen
        .ray(middle)
        .direction
        .dot(frame.rotation * axis.vector());
    if facing.abs() < FOLLOW_MIN_FACING {
        return None;
    }
    let radius = screen.studs_per_point(frame.position) * HANDLE_POINTS;
    let dead_zone = radius * FOLLOW_DEAD_ZONE;
    let last_angle = ring_angle(frame, axis, screen.ray(at), dead_zone)?;
    Some(Follow {
        last_angle,
        dead_zone,
    })
}

/// The way on screen (one point long) that the ring for `axis` around
/// `frame` turns forward at the point of it nearest `at`: along the ring
/// as drawn. Where the ring runs straight at the camera there, it's the
/// way around the ring's middle instead.
fn ring_spin(frame: Pose, axis: Axis, screen: &Screen, at: egui::Pos2) -> egui::Vec2 {
    let center = frame.position;
    let length = screen.studs_per_point(center) * HANDLE_POINTS;
    let (across, up) = axis.ring_directions();
    let (across, up) = (frame.rotation * across, frame.rotation * up);
    let point = |angle: f32| center + (across * angle.cos() + up * angle.sin()) * length;
    let nearest = (0..RING_PIECES * 4)
        .map(|i| i as f32 / (RING_PIECES * 4) as f32 * std::f32::consts::TAU)
        .filter_map(|angle| Some((angle, screen.point(point(angle))?.distance(at))))
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(angle, _)| angle);
    if let Some(angle) = nearest {
        let ahead = (up * angle.cos() - across * angle.sin()) * length * 0.01;
        let here = point(angle);
        if let (Some(a), Some(b)) = (screen.point(here), screen.point(here + ahead)) {
            let along = b - a;
            if along.length() > 0.05 {
                return along.normalized();
            }
        }
    }
    // Around the middle: anticlockwise on screen when the axis points at
    // the camera (egui's Y runs down).
    let Some(middle) = screen.point(center) else {
        return egui::Vec2::X;
    };
    let out = at - middle;
    if out.length() < 1e-3 {
        return egui::Vec2::X;
    }
    let toward_camera = -screen
        .ray(middle)
        .direction
        .dot(frame.rotation * axis.vector());
    let anticlockwise = egui::vec2(out.y, -out.x).normalized();
    if toward_camera >= 0.0 {
        anticlockwise
    } else {
        -anticlockwise
    }
}

#[cfg(test)]
mod tests;
