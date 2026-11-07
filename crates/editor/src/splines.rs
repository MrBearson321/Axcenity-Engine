//! Splines in the Scene view (Phase 73).
//!
//! While editing (not during Play), every Spline in the Workspace is drawn
//! as its curve with a dot at each SplinePoint; the game never draws them.
//! Clicking a curve selects its Spline and starts point editing
//! ([`hit_curve`]): its points get handles ([`hit_point`]) to drag along
//! the surfaces under the mouse, Alt-drag on one adds a point after it
//! where it's let go, and Delete removes the selected point. A Free
//! point's handles are drawn as lines.

use axcenity_core::spline::{ControlPoint, SplineCurve, TangentMode};
use axcenity_core::{ClassName, InstanceId, Vector3, World};
use axcenity_render::glam::Vec3;
use eframe::egui;

use crate::screen::Screen;

const CURVE_COLOR: egui::Color32 = egui::Color32::from_rgb(255, 170, 60);
const EDITING_COLOR: egui::Color32 = egui::Color32::from_rgb(255, 210, 90);
const POINT_COLOR: egui::Color32 = egui::Color32::from_rgb(255, 255, 255);
const SELECTED_POINT_COLOR: egui::Color32 = egui::Color32::from_rgb(90, 200, 255);
const HANDLE_COLOR: egui::Color32 = egui::Color32::from_rgb(200, 140, 255);
/// How near the mouse must be to a point's handle or a curve, in points.
pub const GRAB_POINTS: f32 = 8.0;
/// How far apart the curve is sampled for drawing and clicking, in studs.
const SPACING: f32 = 0.5;
/// A point handle's radius, in points.
const POINT_RADIUS: f32 = 5.0;

fn vec3(v: Vector3) -> Vec3 {
    Vec3::new(v.x, v.y, v.z)
}

/// Every Spline in the Workspace.
pub fn splines(world: &World) -> Vec<InstanceId> {
    world
        .descendants(world.workspace())
        .into_iter()
        .filter(|&id| {
            world
                .get(id)
                .is_some_and(|i| i.class() == ClassName::Spline)
        })
        .collect()
}

/// The SplinePoints of `spline`, in order.
pub fn points(world: &World, spline: InstanceId) -> Vec<InstanceId> {
    world
        .get(spline)
        .map(|spline| spline.children().to_vec())
        .unwrap_or_default()
        .into_iter()
        .filter(|&id| {
            world
                .get(id)
                .is_some_and(|i| i.class() == ClassName::SplinePoint)
        })
        .collect()
}

/// The Spline a SplinePoint is in.
pub fn spline_of(world: &World, point: InstanceId) -> Option<InstanceId> {
    let parent = world.get(point)?.parent()?;
    (world.get(parent)?.class() == ClassName::Spline).then_some(parent)
}

/// The point handle of `editing` under `pointer`, nearest first.
pub fn hit_point(
    screen: &Screen,
    world: &World,
    editing: InstanceId,
    pointer: egui::Pos2,
) -> Option<InstanceId> {
    points(world, editing)
        .into_iter()
        .filter_map(|id| {
            let at = screen.point(vec3(ControlPoint::of(world, id)?.position))?;
            let away = at.distance(pointer);
            (away <= GRAB_POINTS).then_some((away, id))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, id)| id)
}

/// The Spline whose curve passes under `pointer`, nearest first.
pub fn hit_curve(screen: &Screen, world: &World, pointer: egui::Pos2) -> Option<InstanceId> {
    splines(world)
        .into_iter()
        .filter_map(|spline| {
            let line = SplineCurve::of(world, spline)?.polyline(SPACING);
            let away = line
                .windows(2)
                .filter_map(|pair| screen.line(vec3(pair[0]), vec3(pair[1])))
                .map(|[a, b]| distance_to_segment(pointer, a, b))
                .chain(
                    // A spline of one point is its dot.
                    line.first()
                        .filter(|_| line.len() == 1)
                        .and_then(|&p| screen.point(vec3(p)))
                        .map(|at| at.distance(pointer)),
                )
                .fold(f32::INFINITY, f32::min);
            (away <= GRAB_POINTS).then_some((away, spline))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, id)| id)
}

fn distance_to_segment(p: egui::Pos2, a: egui::Pos2, b: egui::Pos2) -> f32 {
    let ab = b - a;
    let length = ab.length_sq();
    let t = if length > 0.0 {
        ((p - a).dot(ab) / length).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (a + ab * t).distance(p)
}

/// Draws every Spline. `editing` is the one being point edited: its
/// points get handles, the selected ones blue; `adding` is where an
/// Alt-drag would add a point, after the point it started on.
pub fn paint(
    painter: &egui::Painter,
    screen: &Screen,
    world: &World,
    editing: Option<InstanceId>,
    selection: &[InstanceId],
    adding: Option<(InstanceId, Vec3)>,
) {
    for spline in splines(world) {
        let edited = editing == Some(spline) || selection.contains(&spline);
        let color = if edited { EDITING_COLOR } else { CURVE_COLOR };
        let width: f32 = if edited { 2.5 } else { 1.5 };
        if let Some(curve) = SplineCurve::of(world, spline) {
            let line = curve.polyline(SPACING);
            for pair in line.windows(2) {
                if let Some(segment) = screen.line(vec3(pair[0]), vec3(pair[1])) {
                    painter.line_segment(segment, egui::Stroke::new(width, color));
                }
            }
        }
        for id in points(world, spline) {
            let Some(point) = ControlPoint::of(world, id) else {
                continue;
            };
            let Some(at) = screen.point(vec3(point.position)) else {
                continue;
            };
            if editing == Some(spline) {
                let fill = if selection.contains(&id) {
                    SELECTED_POINT_COLOR
                } else {
                    POINT_COLOR
                };
                painter.circle_filled(at, POINT_RADIUS, fill);
                painter.circle_stroke(at, POINT_RADIUS, egui::Stroke::new(1.5_f32, color));
                if point.mode == TangentMode::Free {
                    for handle in [point.in_tangent, point.out_tangent] {
                        let end = vec3(point.position) + vec3(handle);
                        if let Some(line) = screen.line(vec3(point.position), end) {
                            painter.line_segment(line, egui::Stroke::new(1.0_f32, HANDLE_COLOR));
                            painter.circle_filled(line[1], 3.0, HANDLE_COLOR);
                        }
                    }
                }
            } else {
                painter.circle_filled(at, 2.5, color);
            }
        }
    }
    if let Some((after, at)) = adding
        && let Some(from) = ControlPoint::of(world, after)
        && let Some(line) = screen.line(vec3(from.position), at)
    {
        painter.line_segment(line, egui::Stroke::new(1.5_f32, SELECTED_POINT_COLOR));
        painter.circle_filled(line[1], POINT_RADIUS, SELECTED_POINT_COLOR);
    }
}
