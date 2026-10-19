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
//! - **Rotate**: a ring around each axis. Dragging along one turns the
//!   parts around it, through the pivot (see [`RotatePivot`]).
//!
//! With snapping on, moves and size changes go in steps of `Snap::move_step`
//! studs and turns in steps of `Snap::rotate_step` degrees, counted from
//! where the drag started (like Studio), so a part off the grid stays off it.
//!
//! The handles keep the same size on screen however far away the part is.
//! Everything here is plain math on rays and points, so tests can drive a
//! drag without a window; `viewport.rs` feeds it the mouse.

use axcenity_core::{Instance, InstanceId, Value, Vector3};
use axcenity_render::glam::{Quat, Vec3};
use axcenity_scene::{Ray, orientation_from_rotation, rotation_from_orientation};
use eframe::egui;

use crate::properties::MIN_SIZE;
use crate::screen::Screen;

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
/// Rays this close to parallel with a handle's line (or a ring's flat side)
/// can't say where along it the mouse is.
const PARALLEL: f32 = 1e-3;

/// The tool picked in the toolbar. Select only selects; Move, Resize
/// (`Scale`) and Rotate also show handles on the selection, and the others
/// (modelled on F3X Building Tools) show an options panel in the viewport
/// that acts on every selected part (see `build_tools.rs`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tool {
    #[default]
    Select,
    Move,
    Scale,
    Rotate,
    Paint,
    Material,
    Anchor,
    Collision,
    Lighting,
    Emission,
}

impl Tool {
    pub const ALL: [Tool; 10] = [
        Tool::Select,
        Tool::Move,
        Tool::Scale,
        Tool::Rotate,
        Tool::Paint,
        Tool::Material,
        Tool::Anchor,
        Tool::Collision,
        Tool::Lighting,
        Tool::Emission,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Tool::Select => "Select",
            Tool::Move => "Move",
            Tool::Scale => "Resize",
            Tool::Rotate => "Rotate",
            Tool::Paint => "Paint",
            Tool::Material => "Material",
            Tool::Anchor => "Anchor",
            Tool::Collision => "Collision",
            Tool::Lighting => "Lighting",
            Tool::Emission => "Emission",
        }
    }

    /// Ctrl+1 to Ctrl+4 (Cmd on macOS), as in Studio, for the first four.
    pub fn shortcut(self) -> Option<egui::KeyboardShortcut> {
        let key = match self {
            Tool::Select => egui::Key::Num1,
            Tool::Move => egui::Key::Num2,
            Tool::Scale => egui::Key::Num3,
            Tool::Rotate => egui::Key::Num4,
            _ => return None,
        };
        Some(egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, key))
    }

    /// The single key that picks the tool in F3X, pressed on its own while
    /// editing (not typing, not during Play).
    pub fn key(self) -> Option<egui::Key> {
        Some(match self {
            Tool::Select => return None,
            Tool::Move => egui::Key::Z,
            Tool::Scale => egui::Key::X,
            Tool::Rotate => egui::Key::C,
            Tool::Paint => egui::Key::V,
            Tool::Material => egui::Key::N,
            Tool::Anchor => egui::Key::M,
            Tool::Collision => egui::Key::K,
            Tool::Lighting => egui::Key::U,
            Tool::Emission => egui::Key::P,
        })
    }

    /// True for the tools with handles to drag.
    pub fn has_handles(self) -> bool {
        matches!(self, Tool::Move | Tool::Scale | Tool::Rotate)
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
}

impl Default for Snap {
    /// Studio's defaults: 1 stud and 15 degrees, both on.
    fn default() -> Self {
        Self {
            move_on: true,
            move_step: 1.0,
            rotate_on: true,
            rotate_step: 15.0,
        }
    }
}

impl Snap {
    /// The smallest steps the toolbar allows, so a step of 0 can't freeze a drag.
    pub const MIN_MOVE_STEP: f32 = MIN_SIZE;
    pub const MIN_ROTATE_STEP: f32 = 0.1;

    /// `distance` rounded to the move step, if move snapping is on.
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
    lines: Vec<[egui::Pos2; 2]>,
    end: End,
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
/// part's size for the Resize balls.
pub fn handle_frame(tool: Tool, modes: Modes, poses: &[Pose]) -> Option<Pose> {
    let last = *poses.last()?;
    let middle = || Pose {
        position: middle_of(poses),
        rotation: Quat::IDENTITY,
        size: last.size,
    };
    Some(match tool {
        Tool::Move if modes.axes == MoveAxes::Global => middle(),
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

/// The handles `tool` shows around `pose` (from [`handle_frame`]), as seen
/// through `screen`.
pub fn shapes(tool: Tool, pose: Pose, screen: &Screen) -> Vec<Shape> {
    let length = screen.studs_per_point(pose.position) * HANDLE_POINTS;
    let center = pose.position;
    let mut shapes = Vec::new();
    match tool {
        Tool::Move => {
            for axis in Axis::ALL {
                let direction = pose.rotation * axis.vector();
                let Some(line) = screen.line(center, center + direction * length) else {
                    continue;
                };
                let along = line[1] - line[0];
                if along.length() < MIN_VISIBLE_POINTS {
                    continue;
                }
                shapes.push(Shape {
                    handle: Handle::Move(axis),
                    lines: vec![line],
                    end: End::Arrow {
                        tip: line[1],
                        toward: along.normalized(),
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
    /// Rotate: the angle around the ring where the mouse grabbed it.
    grabbed: f32,
    /// Rotate only: the angle the mouse was at last frame, and how far it has
    /// turned in all (which can go past a whole turn).
    last_angle: f32,
    turned: f32,
}

/// New property values for each part a drag changes.
pub type PartValues = Vec<(InstanceId, Vec<(&'static str, Value)>)>;

impl Drag {
    /// Starts dragging `handle` of one `part` (at `start`) where `ray`
    /// points, with world axes. `None` if the ray runs alongside the handle
    /// and can't grab it.
    #[cfg(test)]
    pub fn start(handle: Handle, part: InstanceId, start: Pose, ray: Ray) -> Option<Drag> {
        // Resize handles follow the part's own axes; the others the world's.
        let frame = match handle {
            Handle::Scale(..) => start,
            _ => Pose {
                rotation: Quat::IDENTITY,
                ..start
            },
        };
        Self::start_many(handle, frame, vec![(part, start)], Modes::default(), ray)
    }

    /// Starts dragging `handle`, placed at `frame`, for every one of
    /// `parts` (the last is the one the handles are on).
    pub fn start_many(
        handle: Handle,
        frame: Pose,
        parts: Vec<(InstanceId, Pose)>,
        modes: Modes,
        ray: Ray,
    ) -> Option<Drag> {
        parts.last()?;
        let grabbed = match handle {
            Handle::Move(_) | Handle::Scale(..) => {
                let (origin, direction) = handle_line(handle, frame);
                along_line(origin, direction, ray)?
            }
            Handle::Rotate(axis) => ring_angle(frame, axis, ray)?,
        };
        Some(Drag {
            handle,
            frame,
            parts,
            modes,
            grabbed,
            last_angle: grabbed,
            turned: 0.0,
        })
    }

    /// The new property values of the part the handles are on, for the
    /// mouse now pointing along `ray`, or `None` if this ray can't tell.
    #[cfg(test)]
    pub fn update(&mut self, ray: Ray, snap: &Snap) -> Option<Vec<(&'static str, Value)>> {
        let part = self.parts.last()?.0;
        self.update_all(ray, snap)?
            .into_iter()
            .find(|(id, _)| *id == part)
            .map(|(_, values)| values)
    }

    /// The new property values of every part, for the mouse now pointing
    /// along `ray`, or `None` if this ray can't tell (it runs alongside the
    /// handle).
    pub fn update_all(&mut self, ray: Ray, snap: &Snap) -> Option<PartValues> {
        let frame = self.frame;
        let modes = self.modes;
        match self.handle {
            Handle::Move(axis) => {
                let (origin, direction) = handle_line(self.handle, frame);
                let moved = snap.studs(along_line(origin, direction, ray)? - self.grabbed);
                Some(
                    self.parts
                        .iter()
                        .map(|&(id, start)| {
                            let direction = match modes.axes {
                                MoveAxes::Local => start.rotation * axis.vector(),
                                MoveAxes::Global | MoveAxes::Last => direction,
                            };
                            let position = start.position + direction * moved;
                            (id, vec![("Position", to_value(position))])
                        })
                        .collect(),
                )
            }
            Handle::Scale(axis, side) => {
                let (origin, direction) = handle_line(self.handle, frame);
                let pulled = snap.studs(along_line(origin, direction, ray)? - self.grabbed);
                let i = axis.index();
                Some(
                    self.parts
                        .iter()
                        .map(|&(id, start)| {
                            let outward = start.rotation * axis.vector() * side.sign();
                            let mut size = start.size;
                            let position = match modes.directions {
                                ResizeDirections::Normal => {
                                    size[i] = (start.size[i] + pulled).max(MIN_SIZE);
                                    // The center moves half as far as the
                                    // face, so the opposite face stays put.
                                    start.position + outward * (size[i] - start.size[i]) / 2.0
                                }
                                ResizeDirections::Both => {
                                    size[i] = (start.size[i] + pulled * 2.0).max(MIN_SIZE);
                                    start.position
                                }
                            };
                            let values =
                                vec![("Size", to_value(size)), ("Position", to_value(position))];
                            (id, values)
                        })
                        .collect(),
                )
            }
            Handle::Rotate(axis) => {
                let angle = ring_angle(frame, axis, ray)?;
                self.turned += shortest_turn(angle - self.last_angle);
                self.last_angle = angle;
                let radians = snap.degrees(self.turned.to_degrees()).to_radians();
                let world_axis = frame.rotation * axis.vector();
                Some(
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
                            let orientation = orientation_from_rotation(turn * start.rotation);
                            let position = pivot + turn * (start.position - pivot);
                            let mut values = vec![("Orientation", Value::Vector3(orientation))];
                            if !position.abs_diff_eq(start.position, 1e-5) {
                                values.push(("Position", to_value(position)));
                            }
                            (id, values)
                        })
                        .collect(),
                )
            }
        }
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

/// The angle (in radians) around the ring for `axis` through `center` where
/// `ray` meets the ring's flat side. `None` if it doesn't meet it in front
/// of the camera.
fn ring_angle(frame: Pose, axis: Axis, ray: Ray) -> Option<f32> {
    let center = frame.position;
    let normal = frame.rotation * axis.vector();
    let facing = ray.direction.dot(normal);
    if facing.abs() < PARALLEL {
        return None;
    }
    let distance = (center - ray.origin).dot(normal) / facing;
    if distance <= 0.0 {
        return None;
    }
    let offset = ray.at(distance) - center;
    let (across, up) = axis.ring_directions();
    let (across, up) = (frame.rotation * across, frame.rotation * up);
    Some(offset.dot(up).atan2(offset.dot(across)))
}

/// An angle change in radians brought between -half a turn and half a turn,
/// so crossing the point where the angle wraps from +180° to -180° counts
/// as a small turn, not nearly a whole one.
fn shortest_turn(angle: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    (angle + PI).rem_euclid(TAU) - PI
}

#[cfg(test)]
mod tests {
    use axcenity_core::{ClassName, World};
    use axcenity_render::Camera;

    use super::*;

    fn pose(position: Vec3, size: Vec3) -> Pose {
        Pose {
            position,
            rotation: Quat::IDENTITY,
            size,
        }
    }

    fn some_part() -> InstanceId {
        let mut world = World::new();
        world.create(ClassName::Part).unwrap()
    }

    /// A ray from far in front of the scene, straight down -Z, through (x, y).
    fn ray_through(x: f32, y: f32) -> Ray {
        Ray {
            origin: Vec3::new(x, y, 50.0),
            direction: Vec3::NEG_Z,
        }
    }

    fn vector(values: &[(&'static str, Value)], name: &str) -> Vec3 {
        let (_, value) = values.iter().find(|(n, _)| *n == name).unwrap();
        to_vec3(value.as_vector3().unwrap())
    }

    const NO_SNAP: Snap = Snap {
        move_on: false,
        move_step: 1.0,
        rotate_on: false,
        rotate_step: 15.0,
    };

    fn assert_near(a: Vec3, b: Vec3) {
        assert!(a.abs_diff_eq(b, 1e-4), "{a} is not close to {b}");
    }

    #[test]
    fn move_handle_slides_the_part_along_its_axis_only() {
        let start = pose(Vec3::new(1.0, 2.0, 0.0), Vec3::ONE);
        let mut drag = Drag::start(
            Handle::Move(Axis::X),
            some_part(),
            start,
            ray_through(1.5, 2.0),
        )
        .unwrap();
        // The mouse goes 3.25 studs right and 4 up; only the X part counts.
        let values = drag.update(ray_through(4.75, 6.0), &NO_SNAP).unwrap();
        assert_near(vector(&values, "Position"), Vec3::new(4.25, 2.0, 0.0));

        let mut drag = Drag::start(
            Handle::Move(Axis::Y),
            some_part(),
            start,
            ray_through(1.0, 2.5),
        )
        .unwrap();
        let values = drag.update(ray_through(9.0, 1.0), &NO_SNAP).unwrap();
        assert_near(vector(&values, "Position"), Vec3::new(1.0, 0.5, 0.0));
    }

    #[test]
    fn scale_handle_moves_one_face_and_keeps_the_opposite_one() {
        // A 4-stud-wide part from x = -2 to x = 2; grab its right face.
        let start = pose(Vec3::ZERO, Vec3::new(4.0, 1.0, 2.0));
        let handle = Handle::Scale(Axis::X, Side::Plus);
        let mut drag = Drag::start(handle, some_part(), start, ray_through(2.5, 0.0)).unwrap();
        let values = drag.update(ray_through(5.5, 0.0), &NO_SNAP).unwrap();
        assert_near(vector(&values, "Size"), Vec3::new(7.0, 1.0, 2.0));
        // Now from x = -2 to x = 5: the left face hasn't moved.
        assert_near(vector(&values, "Position"), Vec3::new(1.5, 0.0, 0.0));

        // The left face, pulled outward to the left.
        let handle = Handle::Scale(Axis::X, Side::Minus);
        let mut drag = Drag::start(handle, some_part(), start, ray_through(-2.5, 0.0)).unwrap();
        let values = drag.update(ray_through(-3.5, 0.0), &NO_SNAP).unwrap();
        assert_near(vector(&values, "Size"), Vec3::new(5.0, 1.0, 2.0));
        assert_near(vector(&values, "Position"), Vec3::new(-0.5, 0.0, 0.0));
    }

    #[test]
    fn scale_handles_follow_a_turned_part_and_never_shrink_it_to_nothing() {
        // Turned 90° around Z, so the part's X runs along the world's Y.
        let start = Pose {
            position: Vec3::ZERO,
            rotation: Quat::from_rotation_z(90f32.to_radians()),
            size: Vec3::new(4.0, 1.0, 2.0),
        };
        let handle = Handle::Scale(Axis::X, Side::Plus);
        let mut drag = Drag::start(handle, some_part(), start, ray_through(0.0, 2.0)).unwrap();
        let values = drag.update(ray_through(0.0, 3.0), &NO_SNAP).unwrap();
        assert_near(vector(&values, "Size"), Vec3::new(5.0, 1.0, 2.0));
        assert_near(vector(&values, "Position"), Vec3::new(0.0, 0.5, 0.0));

        let values = drag.update(ray_through(0.0, -30.0), &NO_SNAP).unwrap();
        assert_eq!(vector(&values, "Size").x, MIN_SIZE);
    }

    #[test]
    fn rotate_ring_turns_the_part_around_its_axis() {
        let start = pose(Vec3::ZERO, Vec3::ONE);
        // Looking straight down the Z axis at the Z ring: grab it on the
        // right (+X), then drag up to the top (+Y), a quarter turn.
        let handle = Handle::Rotate(Axis::Z);
        let mut drag = Drag::start(handle, some_part(), start, ray_through(3.0, 0.0)).unwrap();
        let values = drag.update(ray_through(0.0, 3.0), &NO_SNAP).unwrap();
        assert_near(vector(&values, "Orientation"), Vec3::new(0.0, 0.0, 90.0));

        // Carrying on around past the left side keeps counting, rather than
        // jumping back when the angle wraps from +180° to -180°.
        drag.update(ray_through(-3.0, 0.1), &NO_SNAP).unwrap();
        let values = drag.update(ray_through(-3.0, -0.1), &NO_SNAP).unwrap();
        let turned = drag.turned.to_degrees();
        assert!((turned - 181.91).abs() < 0.01, "{turned}");
        // 181.91° is the same as -178.09°, which is how Orientation says it.
        let z = vector(&values, "Orientation").z;
        assert!((z + 178.09).abs() < 0.01, "{z}");
    }

    #[test]
    fn rotating_a_turned_part_turns_it_further_around_the_world_axis() {
        // Already turned 30° around Y; another 15° around Y gives 45°.
        let start = Pose {
            position: Vec3::ZERO,
            rotation: Quat::from_rotation_y(30f32.to_radians()),
            size: Vec3::ONE,
        };
        let from_above = |x: f32, z: f32| Ray {
            origin: Vec3::new(x, 50.0, z),
            direction: Vec3::NEG_Y,
        };
        let handle = Handle::Rotate(Axis::Y);
        // Around Y, turning goes from +Z toward +X.
        let mut drag = Drag::start(handle, some_part(), start, from_above(0.0, 3.0)).unwrap();
        let (sin, cos) = 15f32.to_radians().sin_cos();
        let values = drag
            .update(from_above(3.0 * sin, 3.0 * cos), &Snap::default())
            .unwrap();
        assert_eq!(vector(&values, "Orientation"), Vec3::new(0.0, 45.0, 0.0));
    }

    #[test]
    fn snapping_moves_and_turns_in_steps_and_can_be_turned_off() {
        let start = pose(Vec3::new(0.3, 0.0, 0.0), Vec3::ONE);
        let handle = Handle::Move(Axis::X);
        let mut drag = Drag::start(handle, some_part(), start, ray_through(0.3, 0.0)).unwrap();
        let snapped = drag
            .update(ray_through(2.0, 0.0), &Snap::default())
            .unwrap();
        // 1.7 studs rounds to 2 steps of 1 stud, counted from 0.3.
        assert_near(vector(&snapped, "Position"), Vec3::new(2.3, 0.0, 0.0));
        let free = drag.update(ray_through(2.0, 0.0), &NO_SNAP).unwrap();
        assert_near(vector(&free, "Position"), Vec3::new(2.0, 0.0, 0.0));

        let handle = Handle::Rotate(Axis::Z);
        let mut drag = Drag::start(handle, some_part(), start, ray_through(3.3, 0.0)).unwrap();
        // About 20° around: snaps to 15°.
        let (sin, cos) = 20f32.to_radians().sin_cos();
        let values = drag
            .update(ray_through(0.3 + 3.0 * cos, 3.0 * sin), &Snap::default())
            .unwrap();
        assert_eq!(vector(&values, "Orientation"), Vec3::new(0.0, 0.0, 15.0));
    }

    #[test]
    fn changing_the_step_size_changes_the_steps() {
        let start = pose(Vec3::ZERO, Vec3::new(4.0, 1.0, 2.0));
        let quarter = Snap {
            move_step: 0.25,
            rotate_step: 45.0,
            ..Snap::default()
        };
        let handle = Handle::Move(Axis::X);
        let mut drag = Drag::start(handle, some_part(), start, ray_through(0.0, 0.0)).unwrap();
        let values = drag.update(ray_through(1.1, 0.0), &quarter).unwrap();
        assert_near(vector(&values, "Position"), Vec3::new(1.0, 0.0, 0.0));
        // Scale uses the move step too.
        let handle = Handle::Scale(Axis::X, Side::Plus);
        let mut drag = Drag::start(handle, some_part(), start, ray_through(2.0, 0.0)).unwrap();
        let values = drag.update(ray_through(2.4, 0.0), &quarter).unwrap();
        assert_near(vector(&values, "Size"), Vec3::new(4.5, 1.0, 2.0));

        let handle = Handle::Rotate(Axis::Z);
        let mut drag = Drag::start(handle, some_part(), start, ray_through(3.0, 0.0)).unwrap();
        let (sin, cos) = 30f32.to_radians().sin_cos();
        let values = drag
            .update(ray_through(3.0 * cos, 3.0 * sin), &quarter)
            .unwrap();
        assert_eq!(vector(&values, "Orientation"), Vec3::new(0.0, 0.0, 45.0));
    }

    #[test]
    fn a_handle_seen_end_on_cannot_be_grabbed() {
        let start = pose(Vec3::ZERO, Vec3::ONE);
        // Looking straight along Z, the Z arrow is a dot.
        let ray = ray_through(0.0, 0.0);
        assert!(Drag::start(Handle::Move(Axis::Z), some_part(), start, ray).is_none());
    }

    /// The editor's start camera, looking at the origin, in a 1280×720 viewport.
    fn screen() -> Screen {
        let camera = Camera {
            position: Vec3::new(12.0, 8.0, 16.0),
            look_at: Vec3::ZERO,
            fov_y_degrees: 50.0,
        };
        Screen::new(
            camera,
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 720.0)),
        )
    }

    #[test]
    fn each_tool_shows_its_own_handles() {
        let start = pose(Vec3::ZERO, Vec3::new(4.0, 1.0, 2.0));
        let handles = |tool| -> Vec<Handle> {
            shapes(tool, start, &screen())
                .iter()
                .map(|shape| shape.handle)
                .collect()
        };
        assert!(handles(Tool::Select).is_empty());
        assert_eq!(handles(Tool::Move), Axis::ALL.map(Handle::Move).to_vec());
        assert_eq!(handles(Tool::Scale).len(), 6);
        assert_eq!(
            handles(Tool::Rotate),
            Axis::ALL.map(Handle::Rotate).to_vec()
        );
    }

    #[test]
    fn the_mouse_over_a_handle_finds_that_handle() {
        let screen = screen();
        let start = pose(Vec3::ZERO, Vec3::new(4.0, 1.0, 2.0));
        let move_shapes = shapes(Tool::Move, start, &screen);
        for axis in Axis::ALL {
            // Halfway along each arrow, and a few points off to the side.
            let shape = move_shapes
                .iter()
                .find(|s| s.handle == Handle::Move(axis))
                .unwrap();
            let [a, b] = shape.lines[0];
            let near = a + (b - a) / 2.0 + egui::vec2(0.0, 3.0);
            assert_eq!(hovered(&move_shapes, near), Some(Handle::Move(axis)));
        }
        // Far from the part, nothing.
        assert_eq!(hovered(&move_shapes, egui::pos2(5.0, 5.0)), None);

        // The ball on the top face of the part.
        let scale_shapes = shapes(Tool::Scale, start, &screen);
        let top = screen.point(Vec3::new(0.0, 0.5, 0.0)).unwrap();
        let above_top = scale_shapes
            .iter()
            .find(|s| s.handle == Handle::Scale(Axis::Y, Side::Plus))
            .unwrap();
        let End::Ball(ball) = above_top.end else {
            panic!("scale handles end in a ball");
        };
        assert!(ball.y < top.y, "the ball sits above the top face");
        assert_eq!(
            hovered(&scale_shapes, ball),
            Some(Handle::Scale(Axis::Y, Side::Plus))
        );
    }

    #[test]
    fn handles_stay_the_same_size_on_screen_at_any_distance() {
        let screen = screen();
        let near = shapes(Tool::Move, pose(Vec3::ZERO, Vec3::ONE), &screen);
        // Twice as far from the camera along the same line of sight.
        let far_point = Vec3::new(-12.0, -8.0, -16.0);
        let far = shapes(Tool::Move, pose(far_point, Vec3::ONE), &screen);
        let length = |shapes: &[Shape]| {
            let [a, b] = shapes[1].lines[0];
            (b - a).length()
        };
        assert!((length(&near) - length(&far)).abs() < 1.0);
    }

    #[test]
    fn the_pose_comes_from_the_parts_properties() {
        let mut world = World::new();
        let part = world.create(ClassName::Part).unwrap();
        world
            .set_property(
                part,
                "Orientation",
                Value::Vector3(Vector3::new(0.0, 90.0, 0.0)),
            )
            .unwrap();
        let pose = Pose::of(world.get(part).unwrap()).unwrap();
        assert_eq!(pose.size, Vec3::new(4.0, 1.0, 2.0));
        assert_near(pose.rotation * Vec3::X, Vec3::NEG_Z);
        assert!(Pose::of(world.get(world.workspace()).unwrap()).is_none());
    }

    fn two_parts() -> Vec<(InstanceId, Pose)> {
        let mut world = World::new();
        let a = world.create(ClassName::Part).unwrap();
        let b = world.create(ClassName::Part).unwrap();
        vec![
            (a, pose(Vec3::new(-2.0, 0.0, 0.0), Vec3::ONE)),
            (b, pose(Vec3::new(2.0, 0.0, 0.0), Vec3::ONE)),
        ]
    }

    fn value_of(values: &PartValues, id: InstanceId, name: &str) -> Vec3 {
        let (_, values) = values.iter().find(|(part, _)| *part == id).unwrap();
        vector(values, name)
    }

    #[test]
    fn global_move_handles_sit_in_the_middle_and_move_every_part() {
        let parts = two_parts();
        let poses: Vec<Pose> = parts.iter().map(|(_, pose)| *pose).collect();
        let frame = handle_frame(Tool::Move, Modes::default(), &poses).unwrap();
        assert_near(frame.position, Vec3::ZERO);
        let mut drag = Drag::start_many(
            Handle::Move(Axis::Y),
            frame,
            parts.clone(),
            Modes::default(),
            ray_through(0.5, 0.0),
        )
        .unwrap();
        let values = drag.update_all(ray_through(0.5, 3.0), &NO_SNAP).unwrap();
        assert_near(
            value_of(&values, parts[0].0, "Position"),
            Vec3::new(-2.0, 3.0, 0.0),
        );
        assert_near(
            value_of(&values, parts[1].0, "Position"),
            Vec3::new(2.0, 3.0, 0.0),
        );
    }

    #[test]
    fn local_axes_move_each_part_along_its_own_axis() {
        let mut parts = two_parts();
        // The second part is turned 90° around Z: its X runs up the world's Y.
        parts[1].1.rotation = Quat::from_rotation_z(90f32.to_radians());
        let modes = Modes {
            axes: MoveAxes::Local,
            ..Modes::default()
        };
        let poses: Vec<Pose> = parts.iter().map(|(_, pose)| *pose).collect();
        let frame = handle_frame(Tool::Move, modes, &poses).unwrap();
        // The handles are the last part's: its X arrow points up.
        let mut drag = Drag::start_many(
            Handle::Move(Axis::X),
            frame,
            parts.clone(),
            modes,
            ray_through(2.0, 0.5),
        )
        .unwrap();
        let values = drag.update_all(ray_through(2.0, 2.5), &NO_SNAP).unwrap();
        assert_near(
            value_of(&values, parts[0].0, "Position"),
            Vec3::new(0.0, 0.0, 0.0),
        );
        assert_near(
            value_of(&values, parts[1].0, "Position"),
            Vec3::new(2.0, 2.0, 0.0),
        );

        // With Last, both go the last part's way.
        let modes = Modes {
            axes: MoveAxes::Last,
            ..Modes::default()
        };
        let mut drag = Drag::start_many(
            Handle::Move(Axis::X),
            frame,
            parts.clone(),
            modes,
            ray_through(2.0, 0.5),
        )
        .unwrap();
        let values = drag.update_all(ray_through(2.0, 2.5), &NO_SNAP).unwrap();
        assert_near(
            value_of(&values, parts[0].0, "Position"),
            Vec3::new(-2.0, 2.0, 0.0),
        );
    }

    #[test]
    fn resizing_both_directions_keeps_the_center() {
        let parts = two_parts();
        let modes = Modes {
            directions: ResizeDirections::Both,
            ..Modes::default()
        };
        let frame = parts[1].1;
        let mut drag = Drag::start_many(
            Handle::Scale(Axis::Y, Side::Plus),
            frame,
            parts.clone(),
            modes,
            ray_through(2.0, 1.0),
        )
        .unwrap();
        let values = drag.update_all(ray_through(2.0, 2.0), &NO_SNAP).unwrap();
        for (id, start) in &parts {
            assert_near(value_of(&values, *id, "Size"), Vec3::new(1.0, 3.0, 1.0));
            assert_near(value_of(&values, *id, "Position"), start.position);
        }
    }

    #[test]
    fn center_pivot_turns_the_selection_around_its_middle_and_local_turns_each_in_place() {
        let parts = two_parts();
        let poses: Vec<Pose> = parts.iter().map(|(_, pose)| *pose).collect();
        let frame = handle_frame(Tool::Rotate, Modes::default(), &poses).unwrap();
        let turn = |modes: Modes, frame: Pose| {
            let mut drag = Drag::start_many(
                Handle::Rotate(Axis::Z),
                frame,
                parts.clone(),
                modes,
                ray_through(frame.position.x + 3.0, 0.0),
            )
            .unwrap();
            drag.update_all(ray_through(frame.position.x, 3.0), &NO_SNAP)
                .unwrap()
        };
        // A quarter turn around the middle: the parts swing round to ±Y.
        let values = turn(Modes::default(), frame);
        assert_near(
            value_of(&values, parts[0].0, "Position"),
            Vec3::new(0.0, -2.0, 0.0),
        );
        assert_near(
            value_of(&values, parts[1].0, "Position"),
            Vec3::new(0.0, 2.0, 0.0),
        );
        assert_near(
            value_of(&values, parts[0].0, "Orientation"),
            Vec3::new(0.0, 0.0, 90.0),
        );

        // Local: each turns where it is.
        let local = Modes {
            pivot: RotatePivot::Local,
            ..Modes::default()
        };
        let values = turn(local, parts[1].1);
        let (_, first) = values.iter().find(|(id, _)| *id == parts[0].0).unwrap();
        assert!(first.iter().all(|(name, _)| *name != "Position"));
        assert_near(
            value_of(&values, parts[0].0, "Orientation"),
            Vec3::new(0.0, 0.0, 90.0),
        );
    }
}
