//! The Model Studio's keyboard moves (Phase 55), as in Blender: G grabs,
//! R rotates and S scales what's selected, following the mouse until a
//! click or Enter (a right-click or Esc puts it back).
//!
//! While one runs, X, Y or Z locks it to that world axis, the same key
//! again to the selection's own (Local) axis, and a third time frees it.
//! Typing a number moves that many studs, turns that many degrees or
//! scales by that much. Snapping uses the toolbar's steps (Phase 8b):
//! moves in move steps, turns in rotate steps, and scales in tenths.
//! Holding Ctrl turns snapping the other way.

use axcenity_render::glam::{Mat4, Quat, Vec3};
use eframe::egui;

use crate::gizmo::Snap;
use crate::screen::Screen;

/// Which move a key started.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModalKind {
    Grab,
    Rotate,
    Scale,
}

impl ModalKind {
    /// The undo step it makes.
    pub fn label(self) -> &'static str {
        match self {
            ModalKind::Grab => "Move",
            ModalKind::Rotate => "Rotate",
            ModalKind::Scale => "Scale",
        }
    }
}

/// The axis a move is held to, if any.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AxisLock {
    #[default]
    Free,
    /// The world's X, Y or Z (0, 1, 2).
    Global(usize),
    /// The selection's own axis (its normal is Y).
    Local(usize),
}

impl AxisLock {
    /// After pressing the key for `axis`: that world axis, then its Local
    /// axis, then free again.
    pub fn press(self, axis: usize) -> AxisLock {
        match self {
            AxisLock::Global(held) if held == axis => AxisLock::Local(axis),
            AxisLock::Local(held) if held == axis => AxisLock::Free,
            _ => AxisLock::Global(axis),
        }
    }

    /// The locked direction in the world, given the selection's own `frame`.
    fn direction(self, frame: Quat) -> Option<Vec3> {
        match self {
            AxisLock::Free => None,
            AxisLock::Global(axis) => Some(Vec3::AXES[axis]),
            AxisLock::Local(axis) => Some(frame * Vec3::AXES[axis]),
        }
    }

    /// What the status line says.
    pub fn describe(self) -> String {
        let name = |axis: usize| ["X", "Y", "Z"][axis];
        match self {
            AxisLock::Free => String::new(),
            AxisLock::Global(axis) => format!(" along global {}", name(axis)),
            AxisLock::Local(axis) => format!(" along local {}", name(axis)),
        }
    }
}

/// A keyboard move under way.
#[derive(Clone, Debug)]
pub struct Modal {
    pub kind: ModalKind,
    pub lock: AxisLock,
    /// What's been typed so far.
    pub typed: String,
    /// Where the mouse was when it started.
    pub start: egui::Pos2,
    /// The point it moves, turns and scales around.
    pub center: Vec3,
    /// The selection's own axes, for Local locks.
    pub frame: Quat,
}

impl Modal {
    pub fn new(kind: ModalKind, start: egui::Pos2, center: Vec3, frame: Quat) -> Modal {
        Modal {
            kind,
            lock: AxisLock::Free,
            typed: String::new(),
            start,
            center,
            frame,
        }
    }

    /// Takes a typed character: a digit, `.` or `-`. False for others.
    pub fn type_char(&mut self, c: char) -> bool {
        let fits = c.is_ascii_digit() || (c == '.' && !self.typed.contains('.')) || c == '-';
        if !fits {
            return false;
        }
        if c == '-' {
            // Minus flips the sign wherever it's typed.
            match self.typed.strip_prefix('-') {
                Some(rest) => self.typed = rest.to_owned(),
                None => self.typed.insert(0, '-'),
            }
        } else {
            self.typed.push(c);
        }
        true
    }

    /// The typed number, if there's one.
    fn typed_value(&self) -> Option<f32> {
        let text = self.typed.as_str();
        if text.is_empty() || text == "-" {
            return None;
        }
        text.parse().ok()
    }

    /// The move so far as one matrix, for the mouse at `mouse` in `screen`.
    pub fn matrix(&self, screen: &Screen, mouse: egui::Pos2, snap: &Snap) -> Mat4 {
        let direction = self.lock.direction(self.frame);
        let around = |inner: Mat4| {
            Mat4::from_translation(self.center) * inner * Mat4::from_translation(-self.center)
        };
        match self.kind {
            ModalKind::Grab => {
                let offset = match self.typed_value() {
                    Some(value) => direction.unwrap_or(Vec3::X) * value,
                    None => self.dragged(screen, mouse, direction, snap),
                };
                Mat4::from_translation(offset)
            }
            ModalKind::Rotate => {
                let toward_camera = -screen.ray(self.screen_center(screen)).direction;
                let axis = direction.unwrap_or(toward_camera).normalize_or(Vec3::Y);
                let degrees = match self.typed_value() {
                    Some(value) => value,
                    None => {
                        let turned = self.screen_angle(screen, mouse).to_degrees();
                        // Seen from the side the axis points away to, the
                        // turn runs the other way round.
                        let seen = if axis.dot(toward_camera) >= 0.0 {
                            1.0
                        } else {
                            -1.0
                        };
                        snap_to(turned * seen, snap.rotate_on, snap.rotate_step)
                    }
                };
                around(Mat4::from_quat(Quat::from_axis_angle(
                    axis,
                    degrees.to_radians(),
                )))
            }
            ModalKind::Scale => {
                let factor = match self.typed_value() {
                    Some(value) => value,
                    None => {
                        let middle = self.screen_center(screen);
                        let from = (self.start - middle).length().max(1.0);
                        snap_to((mouse - middle).length() / from, snap.move_on, 0.1)
                    }
                };
                let scale = match self.lock {
                    AxisLock::Free => Mat4::from_scale(Vec3::splat(factor)),
                    AxisLock::Global(axis) => Mat4::from_scale(stretch(axis, factor)),
                    AxisLock::Local(axis) => {
                        let frame = Mat4::from_quat(self.frame);
                        frame * Mat4::from_scale(stretch(axis, factor)) * frame.inverse()
                    }
                };
                around(scale)
            }
        }
    }

    /// Where the center shows, or the start if it's behind the camera.
    fn screen_center(&self, screen: &Screen) -> egui::Pos2 {
        screen.point(self.center).unwrap_or(self.start)
    }

    /// How far the mouse has turned around the center on screen, in
    /// radians, counter-clockwise.
    fn screen_angle(&self, screen: &Screen, mouse: egui::Pos2) -> f32 {
        let middle = self.screen_center(screen);
        let (a, b) = (self.start - middle, mouse - middle);
        // Screen Y points down, so this is negated to count
        // counter-clockwise as seen.
        -(a.x * b.y - a.y * b.x).atan2(a.x * b.x + a.y * b.y)
    }

    /// The grab's offset: the mouse's move carried onto the plane through
    /// the center facing the camera, then onto the locked axis.
    fn dragged(
        &self,
        screen: &Screen,
        mouse: egui::Pos2,
        direction: Option<Vec3>,
        snap: &Snap,
    ) -> Vec3 {
        let normal = screen.ray(self.screen_center(screen)).direction;
        let on_plane = |at: egui::Pos2| {
            let ray = screen.ray(at);
            let along = (self.center - ray.origin).dot(normal) / ray.direction.dot(normal);
            ray.origin + ray.direction * along
        };
        let offset = on_plane(mouse) - on_plane(self.start);
        if !offset.is_finite() {
            return Vec3::ZERO;
        }
        match direction {
            Some(direction) => direction * snap.studs(offset.dot(direction)),
            None => {
                // Snapped along the selection's own axes.
                let local = self.frame.inverse() * offset;
                self.frame
                    * Vec3::new(
                        snap.studs(local.x),
                        snap.studs(local.y),
                        snap.studs(local.z),
                    )
            }
        }
    }

    /// What the status line says while it runs.
    pub fn describe(&self) -> String {
        let typed = if self.typed.is_empty() {
            String::new()
        } else {
            format!(" {}", self.typed)
        };
        format!(
            "{}{}{typed} · click or Enter to finish, right-click or Esc to cancel",
            self.kind.label(),
            self.lock.describe()
        )
    }
}

/// 1 on each axis but `axis`, which is `factor`.
fn stretch(axis: usize, factor: f32) -> Vec3 {
    let mut scale = Vec3::ONE;
    scale[axis] = factor;
    scale
}

/// `value` rounded to whole `step`s, if `on`.
fn snap_to(value: f32, on: bool, step: f32) -> f32 {
    if on && step > 0.0 {
        (value / step).round() * step
    } else {
        value
    }
}

/// The selection's own axes: Y along `normal` (world axes when it has
/// none).
pub fn frame_of(normal: Vec3) -> Quat {
    if normal == Vec3::ZERO {
        return Quat::IDENTITY;
    }
    Quat::from_rotation_arc(Vec3::Y, normal.normalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axcenity_render::Camera;

    fn screen() -> Screen {
        let camera = Camera {
            position: Vec3::new(0.0, 0.0, 10.0),
            look_at: Vec3::ZERO,
            fov_y_degrees: 50.0,
        };
        Screen::new(
            camera,
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0)),
        )
    }

    const NO_SNAP: Snap = Snap {
        move_on: false,
        move_step: 1.0,
        rotate_on: false,
        rotate_step: 15.0,
        surfaces: false,
        collisions: false,
        join: false,
    };

    #[test]
    fn axis_keys_go_global_then_local_then_free() {
        let lock = AxisLock::Free.press(0);
        assert_eq!(lock, AxisLock::Global(0));
        assert_eq!(lock.press(0), AxisLock::Local(0));
        assert_eq!(lock.press(0).press(0), AxisLock::Free);
        assert_eq!(lock.press(2), AxisLock::Global(2));
    }

    #[test]
    fn typed_numbers_move_turn_and_scale_exactly() {
        let screen = screen();
        let mut grab = Modal::new(
            ModalKind::Grab,
            egui::pos2(400.0, 300.0),
            Vec3::ZERO,
            Quat::IDENTITY,
        );
        grab.lock = AxisLock::Global(1);
        for c in "2.5".chars() {
            assert!(grab.type_char(c));
        }
        assert!(!grab.type_char('x'));
        let moved = grab.matrix(&screen, egui::pos2(0.0, 0.0), &NO_SNAP);
        assert!(
            moved
                .transform_point3(Vec3::ZERO)
                .abs_diff_eq(Vec3::new(0.0, 2.5, 0.0), 1e-6)
        );
        grab.type_char('-');
        let moved = grab.matrix(&screen, egui::pos2(0.0, 0.0), &NO_SNAP);
        assert!(
            moved
                .transform_point3(Vec3::ZERO)
                .abs_diff_eq(Vec3::new(0.0, -2.5, 0.0), 1e-6)
        );

        let mut turn = Modal::new(
            ModalKind::Rotate,
            egui::pos2(500.0, 300.0),
            Vec3::ZERO,
            Quat::IDENTITY,
        );
        turn.lock = AxisLock::Global(1);
        turn.typed = "90".to_owned();
        let turned = turn.matrix(&screen, egui::pos2(0.0, 0.0), &NO_SNAP);
        assert!(
            turned
                .transform_point3(Vec3::X)
                .abs_diff_eq(Vec3::NEG_Z, 1e-5)
        );

        let mut scale = Modal::new(
            ModalKind::Scale,
            egui::pos2(500.0, 300.0),
            Vec3::Y,
            Quat::IDENTITY,
        );
        scale.lock = AxisLock::Global(0);
        scale.typed = "3".to_owned();
        let scaled = scale.matrix(&screen, egui::pos2(0.0, 0.0), &NO_SNAP);
        // Around the center, along X only.
        assert!(
            scaled
                .transform_point3(Vec3::new(1.0, 2.0, 0.0))
                .abs_diff_eq(Vec3::new(3.0, 2.0, 0.0), 1e-5)
        );
    }

    #[test]
    fn the_mouse_drives_each_move_and_snapping_rounds_it() {
        let screen = screen();
        let start = screen.point(Vec3::new(1.0, 0.0, 0.0)).unwrap();
        // Dragged to where (3.3, 0.4, 0) shows, locked to X.
        let to = screen.point(Vec3::new(3.3, 0.4, 0.0)).unwrap();
        let mut grab = Modal::new(ModalKind::Grab, start, Vec3::ZERO, Quat::IDENTITY);
        grab.lock = AxisLock::Global(0);
        let moved = grab
            .matrix(&screen, to, &NO_SNAP)
            .transform_point3(Vec3::ZERO);
        assert!(moved.abs_diff_eq(Vec3::new(2.3, 0.0, 0.0), 1e-3), "{moved}");
        let snapped = grab
            .matrix(&screen, to, &Snap::default())
            .transform_point3(Vec3::ZERO);
        assert!(
            snapped.abs_diff_eq(Vec3::new(2.0, 0.0, 0.0), 1e-5),
            "{snapped}"
        );

        // A quarter turn counter-clockwise on screen, around the view axis.
        let middle = screen.point(Vec3::ZERO).unwrap();
        let turn = Modal::new(
            ModalKind::Rotate,
            middle + egui::vec2(100.0, 0.0),
            Vec3::ZERO,
            Quat::IDENTITY,
        );
        let turned = turn.matrix(&screen, middle + egui::vec2(0.0, -100.0), &NO_SNAP);
        assert!(turned.transform_point3(Vec3::X).abs_diff_eq(Vec3::Y, 1e-4));

        // Twice as far from the center scales by two.
        let scale = Modal::new(
            ModalKind::Scale,
            middle + egui::vec2(50.0, 0.0),
            Vec3::ZERO,
            Quat::IDENTITY,
        );
        let scaled = scale.matrix(&screen, middle + egui::vec2(0.0, 100.0), &NO_SNAP);
        assert!(
            scaled
                .transform_point3(Vec3::ONE)
                .abs_diff_eq(Vec3::splat(2.0), 1e-4)
        );
    }

    #[test]
    fn local_axes_follow_the_selection() {
        let frame = frame_of(Vec3::X);
        assert!((frame * Vec3::Y).abs_diff_eq(Vec3::X, 1e-6));
        let mut grab = Modal::new(ModalKind::Grab, egui::Pos2::ZERO, Vec3::ZERO, frame);
        grab.lock = AxisLock::Local(1);
        grab.typed = "2".to_owned();
        let moved = grab
            .matrix(&screen(), egui::Pos2::ZERO, &NO_SNAP)
            .transform_point3(Vec3::ZERO);
        assert!(moved.abs_diff_eq(Vec3::new(2.0, 0.0, 0.0), 1e-5));
    }
}
