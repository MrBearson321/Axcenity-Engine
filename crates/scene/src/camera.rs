//! A free-flying camera for looking around the scene, controlled like
//! Studio-style editors: hold the right mouse button to look, WASD to move,
//! E and Q to go up and down, and Shift to move slowly.
//!
//! The flying speed can be changed in steps (the editor uses the mouse wheel).
//! The camera also keeps a *focus point* a little way in front of it. The
//! editor orbits around it, zooms toward it and pans it along with the camera.
//!
//! No windowing code lives here: the player (winit) and the editor (egui)
//! each turn their own key and mouse events into calls on [`FlyCamera`].

use axcenity_core::Vector3;
use axcenity_render::Camera;
use axcenity_render::glam::Vec3;

/// The flying speed a new camera starts with, in studs per second.
const DEFAULT_SPEED: f32 = 20.0;
/// The slowest and fastest flying speeds `change_speed` allows.
const MIN_SPEED: f32 = 1.0;
const MAX_SPEED: f32 = 500.0;
/// How much one step of `change_speed` multiplies the speed by. Multiplying
/// (rather than adding) makes each step feel the same at any speed.
const SPEED_STEP: f32 = 1.2;
/// Speed multiplier while Shift is held, for careful positioning.
const SLOW_FACTOR: f32 = 0.25;
/// Radians turned per pixel of mouse movement.
const MOUSE_SENSITIVITY: f32 = 0.003;
/// Looking straight up or down would flip the view, so stop just short of it.
const MAX_PITCH: f32 = 89.0_f32.to_radians();
const FOV_Y_DEGREES: f32 = 50.0;
/// Zooming in stops this close to the focus point, so it never flips past it.
const MIN_FOCUS_DISTANCE: f32 = 1.0;

/// Which movement keys are held down right now.
#[derive(Clone, Copy, Debug, Default)]
pub struct MoveKeys {
    pub forward: bool,
    pub back: bool,
    pub left: bool,
    pub right: bool,
    pub up: bool,
    pub down: bool,
    pub slow: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct FlyCamera {
    pub position: Vec3,
    /// Turn left and right, in radians. 0 looks toward -Z.
    yaw: f32,
    /// Look up (positive) and down, in radians.
    pitch: f32,
    /// How far in front of the camera its focus point is, in studs.
    focus_distance: f32,
    /// Flying speed in studs per second (before Shift slows it down).
    speed: f32,
}

impl FlyCamera {
    /// A camera at `position` looking at `target`, which becomes its focus point.
    pub fn looking_at(position: Vec3, target: Vec3) -> Self {
        let direction = (target - position).normalize_or(Vec3::NEG_Z);
        Self {
            position,
            yaw: f32::atan2(-direction.x, -direction.z),
            pitch: direction.y.asin().clamp(-MAX_PITCH, MAX_PITCH),
            focus_distance: position.distance(target).max(MIN_FOCUS_DISTANCE),
            speed: DEFAULT_SPEED,
        }
    }

    /// The direction the camera looks in, one stud long.
    pub fn forward(&self) -> Vec3 {
        let (sin_yaw, cos_yaw) = self.yaw.sin_cos();
        let (sin_pitch, cos_pitch) = self.pitch.sin_cos();
        Vec3::new(-sin_yaw * cos_pitch, sin_pitch, -cos_yaw * cos_pitch)
    }

    /// The camera's right, kept level with the ground.
    fn right(&self) -> Vec3 {
        let (sin_yaw, cos_yaw) = self.yaw.sin_cos();
        Vec3::new(cos_yaw, 0.0, -sin_yaw)
    }

    /// The top of the screen's direction in the world.
    fn up(&self) -> Vec3 {
        self.right().cross(self.forward())
    }

    /// The point the camera orbits around and zooms toward.
    pub fn focus(&self) -> Vec3 {
        self.position + self.forward() * self.focus_distance
    }

    /// Turns the camera by a mouse movement in pixels (right and down are positive).
    pub fn look(&mut self, dx: f32, dy: f32) {
        self.yaw -= dx * MOUSE_SENSITIVITY;
        self.pitch = (self.pitch - dy * MOUSE_SENSITIVITY).clamp(-MAX_PITCH, MAX_PITCH);
    }

    /// Swings the camera around its focus point by a mouse movement in
    /// pixels, as if dragging the scene: dragging right turns the scene right.
    pub fn orbit(&mut self, dx: f32, dy: f32) {
        let focus = self.focus();
        self.look(dx, dy);
        self.position = focus - self.forward() * self.focus_distance;
    }

    /// Slides the camera sideways and up or down without turning, by a mouse
    /// movement in pixels across a view `view_height` pixels tall. Things at
    /// the focus point follow the mouse exactly.
    pub fn pan(&mut self, dx: f32, dy: f32, view_height: f32) {
        // How many studs one pixel covers at the focus distance.
        let visible_height = 2.0 * self.focus_distance * (FOV_Y_DEGREES.to_radians() / 2.0).tan();
        let studs_per_pixel = visible_height / view_height.max(1.0);
        self.position += (self.up() * dy - self.right() * dx) * studs_per_pixel;
    }

    /// Moves toward the focus point (positive `amount`) or away from it
    /// (negative). Each unit covers the same share of the remaining distance,
    /// so zooming feels the same near and far.
    pub fn zoom(&mut self, amount: f32) {
        let travel = self.focus_distance * (1.0 - (-amount).exp());
        let travel = travel.min(self.focus_distance - MIN_FOCUS_DISTANCE);
        self.position += self.forward() * travel;
        self.focus_distance -= travel;
    }

    /// Flying speed in studs per second.
    pub fn speed(&self) -> f32 {
        self.speed
    }

    /// Makes flying faster by `steps` (negative steps make it slower), each
    /// step 20% more or less. Fractions of a step work too, for smooth
    /// trackpad scrolling. Stays between `MIN_SPEED` and `MAX_SPEED`.
    pub fn change_speed(&mut self, steps: f32) {
        self.set_speed(self.speed * SPEED_STEP.powf(steps));
    }

    /// Sets the flying speed in studs per second, kept between `MIN_SPEED`
    /// and `MAX_SPEED`.
    pub fn set_speed(&mut self, speed: f32) {
        self.speed = speed.clamp(MIN_SPEED, MAX_SPEED);
    }

    /// Moves the camera for `seconds` of time with `keys` held. Distance is
    /// speed times time, so the camera moves just as fast at any frame rate.
    pub fn update(&mut self, keys: &MoveKeys, seconds: f32) {
        let axis = |positive: bool, negative: bool| f32::from(positive) - f32::from(negative);
        let direction = self.forward() * axis(keys.forward, keys.back)
            + self.right() * axis(keys.right, keys.left)
            + Vec3::Y * axis(keys.up, keys.down);
        // Moving diagonally shouldn't be faster than moving straight.
        let direction = direction.normalize_or_zero();
        let speed = if keys.slow {
            self.speed * SLOW_FACTOR
        } else {
            self.speed
        };
        self.position += direction * speed * seconds;
    }

    /// Where the camera is and which way it looks, in a Part's terms, for
    /// `workspace.CurrentCamera`: the position, and the orientation in
    /// degrees (X looks up and down, Y turns; there's no roll). Y stays
    /// between -180 and 180, however many times the camera has turned.
    pub fn view(&self) -> (Vector3, Vector3) {
        let yaw = (self.yaw.to_degrees() + 180.0).rem_euclid(360.0) - 180.0;
        let p = self.position;
        (
            Vector3::new(p.x, p.y, p.z),
            Vector3::new(self.pitch.to_degrees(), yaw, 0.0),
        )
    }

    /// Moves the camera to `position`, looking the way `orientation` says
    /// (in the same terms as [`Self::view`]; Z is ignored). The pitch stops
    /// just short of straight up or down, like mouse look.
    pub fn set_view(&mut self, position: Vector3, orientation: Vector3) {
        self.position = Vec3::new(position.x, position.y, position.z);
        self.pitch = orientation.x.to_radians().clamp(-MAX_PITCH, MAX_PITCH);
        self.yaw = orientation.y.to_radians();
    }

    /// The camera as the renderer needs it.
    pub fn to_render_camera(self) -> Camera {
        Camera {
            position: self.position,
            look_at: self.position + self.forward(),
            fov_y_degrees: FOV_Y_DEGREES,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_near(a: Vec3, b: Vec3) {
        assert!(a.abs_diff_eq(b, 1e-4), "{a} is not close to {b}");
    }

    #[test]
    fn looking_at_a_target_faces_it() {
        let camera = FlyCamera::looking_at(Vec3::new(7.0, 5.0, 9.0), Vec3::ZERO);
        assert_near(camera.forward(), Vec3::new(-7.0, -5.0, -9.0).normalize());
        assert_near(camera.focus(), Vec3::ZERO);
    }

    #[test]
    fn view_gives_degrees_like_a_part_and_set_view_puts_them_back() {
        // Looking along -X is a quarter turn left: Y = 90.
        let mut camera = FlyCamera::looking_at(Vec3::new(1.0, 2.0, 3.0), Vec3::new(-9.0, 2.0, 3.0));
        let (position, orientation) = camera.view();
        assert_eq!(position, Vector3::new(1.0, 2.0, 3.0));
        assert!((orientation.y - 90.0).abs() < 1e-3, "{orientation:?}");
        assert!(orientation.x.abs() < 1e-3, "{orientation:?}");

        camera.set_view(Vector3::new(0.0, 10.0, 0.0), Vector3::new(-30.0, 0.0, 0.0));
        let down = Vec3::new(
            0.0,
            -(30f32.to_radians().sin()),
            -(30f32.to_radians().cos()),
        );
        assert_near(camera.forward(), down);
        assert_near(camera.position, Vec3::new(0.0, 10.0, 0.0));
        // Many turns still read between -180 and 180.
        camera.set_view(Vector3::new(0.0, 0.0, 0.0), Vector3::new(0.0, 450.0, 0.0));
        assert!((camera.view().1.y - 90.0).abs() < 1e-3);
    }

    #[test]
    fn default_direction_is_toward_negative_z_with_right_along_x() {
        let camera = FlyCamera::looking_at(Vec3::ZERO, Vec3::NEG_Z);
        assert_near(camera.forward(), Vec3::NEG_Z);
        assert_near(camera.right(), Vec3::X);
        assert_near(camera.up(), Vec3::Y);
    }

    #[test]
    fn moves_at_the_same_speed_at_any_frame_rate() {
        let keys = MoveKeys {
            forward: true,
            right: true,
            ..MoveKeys::default()
        };
        let start = FlyCamera::looking_at(Vec3::ZERO, Vec3::new(1.0, -1.0, -3.0));

        // One second at 30 frames per second, and at 144.
        let mut slow = start;
        for _ in 0..30 {
            slow.update(&keys, 1.0 / 30.0);
        }
        let mut fast = start;
        for _ in 0..144 {
            fast.update(&keys, 1.0 / 144.0);
        }
        assert_near(slow.position, fast.position);
        assert!((slow.position.length() - DEFAULT_SPEED).abs() < 1e-3);
    }

    #[test]
    fn shift_moves_slowly_and_no_keys_stay_still() {
        let mut camera = FlyCamera::looking_at(Vec3::ZERO, Vec3::NEG_Z);
        camera.update(&MoveKeys::default(), 1.0);
        assert_eq!(camera.position, Vec3::ZERO);

        let keys = MoveKeys {
            up: true,
            slow: true,
            ..MoveKeys::default()
        };
        camera.update(&keys, 1.0);
        assert_near(camera.position, Vec3::Y * DEFAULT_SPEED * SLOW_FACTOR);
    }

    #[test]
    fn speed_steps_up_and_down_and_flying_uses_it() {
        let mut camera = FlyCamera::looking_at(Vec3::ZERO, Vec3::NEG_Z);
        camera.change_speed(1.0);
        assert!((camera.speed() - DEFAULT_SPEED * 1.2).abs() < 1e-4);
        camera.change_speed(-2.0);
        assert!((camera.speed() - DEFAULT_SPEED / 1.2).abs() < 1e-4);
        // Half a step each way lands back where it started.
        camera.change_speed(0.5);
        camera.change_speed(0.5);
        assert!((camera.speed() - DEFAULT_SPEED).abs() < 1e-4);

        camera.change_speed(2.0_f32.ln() / 1.2_f32.ln());
        let keys = MoveKeys {
            forward: true,
            ..MoveKeys::default()
        };
        camera.update(&keys, 1.0);
        assert_near(camera.position, Vec3::NEG_Z * DEFAULT_SPEED * 2.0);
    }

    #[test]
    fn speed_stays_within_its_limits() {
        let mut camera = FlyCamera::looking_at(Vec3::ZERO, Vec3::NEG_Z);
        camera.change_speed(1000.0);
        assert_eq!(camera.speed(), MAX_SPEED);
        camera.change_speed(-1000.0);
        assert_eq!(camera.speed(), MIN_SPEED);
    }

    #[test]
    fn mouse_turns_right_and_up_but_never_flips_over() {
        let mut camera = FlyCamera::looking_at(Vec3::ZERO, Vec3::NEG_Z);
        camera.look(10.0, -10.0);
        let forward = camera.forward();
        assert!(forward.x > 0.0, "should turn right");
        assert!(forward.y > 0.0, "should look up");

        camera.look(0.0, -100_000.0);
        assert!((camera.pitch - MAX_PITCH).abs() < 1e-6);
    }

    #[test]
    fn orbiting_circles_the_focus_point_and_keeps_facing_it() {
        let mut camera = FlyCamera::looking_at(Vec3::new(0.0, 0.0, 10.0), Vec3::ZERO);
        // A quarter turn: 90 degrees is about 524 pixels of mouse movement.
        camera.orbit(90.0_f32.to_radians() / MOUSE_SENSITIVITY, 0.0);
        // Dragging right swings the camera to its left, so the scene turns right.
        assert_near(camera.position, Vec3::new(-10.0, 0.0, 0.0));
        assert_near(camera.focus(), Vec3::ZERO);

        camera.orbit(0.0, 200.0);
        assert!(camera.position.y > 0.0, "dragging down looks from above");
        assert_near(camera.focus(), Vec3::ZERO);
        assert!((camera.position.length() - 10.0).abs() < 1e-4);
    }

    #[test]
    fn panning_slides_without_turning_and_things_follow_the_mouse() {
        let mut camera = FlyCamera::looking_at(Vec3::new(0.0, 0.0, 10.0), Vec3::ZERO);
        let forward = camera.forward();
        let height = 2.0 * 10.0 * (FOV_Y_DEGREES.to_radians() / 2.0).tan();
        // Dragging right across a whole view's height moves things right by
        // that much at the focus point, so the camera moves left.
        camera.pan(600.0, 0.0, 600.0);
        assert_near(camera.position, Vec3::new(-height, 0.0, 10.0));
        // Dragging down moves the camera up.
        camera.pan(0.0, 300.0, 600.0);
        assert_near(camera.position, Vec3::new(-height, height / 2.0, 10.0));
        assert_near(camera.forward(), forward);
    }

    #[test]
    fn zooming_approaches_the_focus_point_but_never_passes_it() {
        let mut camera = FlyCamera::looking_at(Vec3::new(0.0, 0.0, 10.0), Vec3::ZERO);
        camera.zoom(2.0_f32.ln());
        assert_near(camera.position, Vec3::new(0.0, 0.0, 5.0));
        assert_near(camera.focus(), Vec3::ZERO);

        camera.zoom(100.0);
        assert_near(camera.position, Vec3::new(0.0, 0.0, MIN_FOCUS_DISTANCE));

        camera.zoom(-(10.0_f32.ln()));
        assert_near(camera.position, Vec3::new(0.0, 0.0, 10.0));
    }
}
