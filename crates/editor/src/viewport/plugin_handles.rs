//! Plugins' handles (roadmap G10, `plugin:CreateHandle`): dots in the Scene
//! View the user drags while editing, freely in the plane facing the camera
//! or along the handle's axis, with the Move snap. Each move goes to the
//! plugin as a [`HandleMove`]; the plugin moves what it likes in `Dragged`.

use axcenity_plugins::Handle;
use axcenity_render::glam::Vec3;
use axcenity_scene::Ray;
use eframe::egui;

use super::Viewport;
use super::overlays::v3;
use crate::screen::Screen;

/// How far outside a handle's dot a press still takes it, in points.
const GRAB_MARGIN: f32 = 3.0;
/// How long an axis handle's guide line is each way, in points.
const AXIS_GUIDE: f32 = 40.0;

/// A plugin handle the user moved: the plugin's index in the editor's list,
/// the handle's number, where it is now, and whether the button was let go.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HandleMove {
    pub plugin: usize,
    pub id: u64,
    pub position: axcenity_core::Vector3,
    pub ended: bool,
}

/// A handle being dragged.
#[derive(Clone, Copy, Debug)]
pub(super) struct HandleDrag {
    plugin: usize,
    id: u64,
    /// Where it was when the press took it.
    start: Vec3,
    /// What it moves along: an axis, or the plane facing the camera.
    along: Along,
    /// Where on its line or plane the press was, so it doesn't jump.
    grab: Vec3,
    /// Where it is now.
    at: Vec3,
}

#[derive(Clone, Copy, Debug)]
enum Along {
    Axis(Vec3),
    Plane(Vec3),
}

impl HandleDrag {
    /// Where the line or plane meets `ray`.
    fn meet(&self, ray: Ray) -> Option<Vec3> {
        match self.along {
            Along::Axis(axis) => closest_on_line(self.start, axis, ray),
            Along::Plane(normal) => {
                let facing = ray.direction.dot(normal);
                if facing.abs() < 1e-5 {
                    return None;
                }
                let distance = (self.start - ray.origin).dot(normal) / facing;
                (distance > 0.0).then(|| ray.at(distance))
            }
        }
    }

    /// Where the handle goes for `ray`, its move from the start snapped to
    /// `step` studs.
    fn dragged_to(&self, ray: Ray, step: Option<f32>) -> Option<Vec3> {
        let offset = self.meet(ray)? - self.grab;
        let snap = |v: f32| match step {
            Some(step) if step > 0.0 => (v / step).round() * step,
            _ => v,
        };
        Some(match self.along {
            Along::Axis(axis) => self.start + axis * snap(offset.dot(axis)),
            Along::Plane(_) => {
                self.start + Vec3::new(snap(offset.x), snap(offset.y), snap(offset.z))
            }
        })
    }
}

/// The point on the line through `origin` along `axis` (length 1) closest
/// to `ray`; `None` when they're parallel.
fn closest_on_line(origin: Vec3, axis: Vec3, ray: Ray) -> Option<Vec3> {
    let across = axis.dot(ray.direction);
    let denominator = 1.0 - across * across;
    if denominator.abs() < 1e-5 {
        return None;
    }
    let w = origin - ray.origin;
    let t = (across * ray.direction.dot(w) - axis.dot(w)) / denominator;
    Some(origin + axis * t)
}

/// The handle under `pointer`, nearest first.
fn handle_under<'a>(
    screen: &Screen,
    handles: &[(usize, &'a Handle)],
    pointer: egui::Pos2,
) -> Option<(usize, &'a Handle)> {
    handles
        .iter()
        .filter_map(|&(plugin, handle)| {
            let at = screen.point(v3(handle.position))?;
            let distance = at.distance(pointer);
            (distance <= handle.size + GRAB_MARGIN).then_some((distance, plugin, handle))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, plugin, handle)| (plugin, handle))
}

impl Viewport {
    /// Plugin handles take a press on them and follow the mouse until it's
    /// let go. Returns the move, and whether the left button is theirs this
    /// frame (so it neither selects nor drags). Alt+left-drag still orbits.
    pub(super) fn plugin_handle_input(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        screen: &Screen,
        handles: &[(usize, &Handle)],
        step: Option<f32>,
    ) -> (Option<HandleMove>, bool) {
        let (pressed, down, pointer, alt) = ui.input(|input| {
            (
                input.pointer.primary_pressed(),
                input.pointer.primary_down(),
                input.pointer.latest_pos(),
                input.modifiers.alt,
            )
        });
        if let Some(drag) = self.handle_drag.as_mut() {
            let still_there = handles
                .iter()
                .any(|&(plugin, handle)| plugin == drag.plugin && handle.id == drag.id);
            if !still_there {
                self.handle_drag = None;
                return (None, down);
            }
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            let to_move = |drag: &HandleDrag, ended| HandleMove {
                plugin: drag.plugin,
                id: drag.id,
                position: axcenity_core::Vector3::new(drag.at.x, drag.at.y, drag.at.z),
                ended,
            };
            if !down {
                let ended = to_move(drag, true);
                self.handle_drag = None;
                return (Some(ended), true);
            }
            let to = pointer.and_then(|pointer| drag.dragged_to(screen.ray(pointer), step));
            return match to {
                Some(to) if to != drag.at => {
                    drag.at = to;
                    (Some(to_move(drag, false)), true)
                }
                _ => (None, true),
            };
        }
        let Some(pointer) = pointer.filter(|_| response.contains_pointer()) else {
            return (None, false);
        };
        let Some((plugin, handle)) = handle_under(screen, handles, pointer) else {
            return (None, false);
        };
        if !pressed || alt {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
            return (None, false);
        }
        let start = v3(handle.position);
        let along = match handle.axis {
            Some(axis) => Along::Axis(v3(axis)),
            None => Along::Plane(self.camera.forward()),
        };
        let mut drag = HandleDrag {
            plugin,
            id: handle.id,
            start,
            along,
            grab: start,
            at: start,
        };
        drag.grab = drag.meet(screen.ray(pointer)).unwrap_or(start);
        self.handle_drag = Some(drag);
        (None, true)
    }
}

/// Draws the handles: a dot with a light rim, bigger while dragged (drawn
/// where `drag` has it, before the plugin hears of the move), and an axis
/// handle's line through it.
pub(super) fn paint_handles(
    painter: &egui::Painter,
    screen: &Screen,
    handles: &[(usize, &Handle)],
    drag: Option<HandleDrag>,
) {
    for &(plugin, handle) in handles {
        let dragged = drag
            .filter(|drag| drag.plugin == plugin && drag.id == handle.id)
            .map(|drag| drag.at);
        let position = dragged.unwrap_or(v3(handle.position));
        let Some(at) = screen.point(position) else {
            continue;
        };
        let color = super::plugin_drawings::color32(handle.color);
        if let Some(axis) = handle.axis {
            let reach = v3(axis) * screen.studs_per_point(position) * AXIS_GUIDE;
            if let Some(line) = screen.line(position - reach, position + reach) {
                painter.line_segment(line, egui::Stroke::new(1.5_f32, color.gamma_multiply(0.6)));
            }
        }
        let radius = if dragged.is_some() {
            handle.size + 2.0
        } else {
            handle.size
        };
        painter.circle_filled(at, radius, color);
        painter.circle_stroke(
            at,
            radius,
            egui::Stroke::new(1.5_f32, egui::Color32::from_white_alpha(220)),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drag(along: Along) -> HandleDrag {
        let start = Vec3::new(0.0, 1.0, 0.0);
        HandleDrag {
            plugin: 0,
            id: 1,
            start,
            along,
            grab: start,
            at: start,
        }
    }

    #[test]
    fn a_free_handle_follows_the_mouse_in_the_plane_facing_the_camera() {
        // The camera looks down -Z at the handle.
        let mut drag = drag(Along::Plane(Vec3::new(0.0, 0.0, -1.0)));
        let ray = |x: f32, y: f32| Ray {
            origin: Vec3::new(x, y, 10.0),
            direction: Vec3::new(0.0, 0.0, -1.0),
        };
        // Grabbed half a stud right of its middle: it doesn't jump there.
        drag.grab = drag.meet(ray(0.5, 1.0)).unwrap();
        assert_eq!(
            drag.dragged_to(ray(2.5, 4.0), None),
            Some(Vec3::new(2.0, 4.0, 0.0))
        );
        assert_eq!(
            drag.dragged_to(ray(2.2, 3.7), Some(1.0)),
            Some(Vec3::new(2.0, 4.0, 0.0))
        );
        // Looking away from the plane, nothing.
        let away = Ray {
            origin: Vec3::new(0.0, 0.0, 10.0),
            direction: Vec3::new(0.0, 0.0, 1.0),
        };
        assert_eq!(drag.dragged_to(away, None), None);
    }

    /// One frame of the viewport's handle input over a 1280×720 view.
    fn frame(
        ctx: &egui::Context,
        viewport: &mut Viewport,
        handles: &[(usize, &Handle)],
        events: Vec<egui::Event>,
    ) -> (Option<HandleMove>, bool) {
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 720.0));
        let input = egui::RawInput {
            screen_rect: Some(rect),
            events,
            ..Default::default()
        };
        let mut result = (None, false);
        let _ = crate::theme::run(ctx, input, |ui| {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ui, |ui| {
                    let response = ui.allocate_rect(rect, egui::Sense::click_and_drag());
                    let screen = viewport.screen(viewport.camera.to_render_camera(), rect);
                    result =
                        viewport.plugin_handle_input(ui, &response, &screen, handles, Some(1.0));
                });
        });
        result
    }

    #[test]
    fn a_press_on_a_handle_takes_the_button_and_drags_it_until_let_go() {
        let ctx = egui::Context::default();
        let mut viewport = Viewport::new();
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 720.0));
        let screen = viewport.screen(viewport.camera.to_render_camera(), rect);
        let handle = Handle {
            id: 4,
            position: axcenity_core::Vector3::new(0.0, 0.0, 0.0),
            color: axcenity_plugins::DEFAULT_COLOR,
            size: 7.0,
            axis: Some(axcenity_core::Vector3::new(0.0, 1.0, 0.0)),
        };
        let handles = [(2, &handle)];
        let at = screen.point(Vec3::ZERO).unwrap();
        let press = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };

        // A press away from it isn't the handle's.
        let away = at + egui::vec2(60.0, 0.0);
        frame(
            &ctx,
            &mut viewport,
            &handles,
            vec![egui::Event::PointerMoved(away)],
        );
        assert_eq!(
            frame(&ctx, &mut viewport, &handles, vec![press(away, true)]),
            (None, false)
        );
        frame(&ctx, &mut viewport, &handles, vec![press(away, false)]);

        frame(
            &ctx,
            &mut viewport,
            &handles,
            vec![egui::Event::PointerMoved(at)],
        );
        assert_eq!(
            frame(&ctx, &mut viewport, &handles, vec![press(at, true)]),
            (None, true)
        );
        // Up the screen to where y = 3 shows: it slides up its axis, snapped.
        let up = screen.point(Vec3::new(0.0, 3.2, 0.0)).unwrap();
        let (moved, taken) = frame(
            &ctx,
            &mut viewport,
            &handles,
            vec![egui::Event::PointerMoved(up)],
        );
        assert!(taken);
        let moved = moved.expect("it moved");
        assert_eq!((moved.plugin, moved.id, moved.ended), (2, 4, false));
        assert_eq!(moved.position, axcenity_core::Vector3::new(0.0, 3.0, 0.0));
        let (ended, taken) = frame(&ctx, &mut viewport, &handles, vec![press(up, false)]);
        assert!(taken);
        assert_eq!(
            ended,
            Some(HandleMove {
                ended: true,
                ..moved
            })
        );
        assert!(viewport.handle_drag.is_none());
    }

    #[test]
    fn an_axis_handle_only_slides_along_its_axis() {
        let drag = drag(Along::Axis(Vec3::Y));
        // A ray passing the axis at height 3.4, off to the side.
        let ray = Ray {
            origin: Vec3::new(5.0, 3.4, 10.0),
            direction: Vec3::new(0.0, 0.0, -1.0),
        };
        let to = drag.dragged_to(ray, None).unwrap();
        assert!(to.abs_diff_eq(Vec3::new(0.0, 3.4, 0.0), 1e-5), "{to}");
        let snapped = drag.dragged_to(ray, Some(1.0)).unwrap();
        assert!(
            snapped.abs_diff_eq(Vec3::new(0.0, 3.0, 0.0), 1e-5),
            "{snapped}"
        );
        // Looking straight along the axis, it can't be placed.
        let along = Ray {
            origin: Vec3::new(0.0, 10.0, 0.0),
            direction: Vec3::new(0.0, -1.0, 0.0),
        };
        assert_eq!(drag.dragged_to(along, None), None);
    }
}
