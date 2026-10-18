//! A free-flying camera for looking around the scene, controlled like
//! Studio-style editors: hold the right mouse button to look, WASD to move,
//! E and Q to go up and down, and Shift to move slowly.

use axcenity_render::Camera;
use axcenity_render::glam::Vec3;
use winit::keyboard::KeyCode;

/// Studs per second.
const SPEED: f32 = 20.0;
/// Speed multiplier while Shift is held, for careful positioning.
const SLOW_FACTOR: f32 = 0.25;
/// Radians turned per pixel of mouse movement.
const MOUSE_SENSITIVITY: f32 = 0.003;
/// Looking straight up or down would flip the view, so stop just short of it.
const MAX_PITCH: f32 = 89.0_f32.to_radians();
const FOV_Y_DEGREES: f32 = 50.0;

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

impl MoveKeys {
    /// Records a key press or release. Keys are physical positions, so the
    /// same keys work on AZERTY and other layouts.
    pub fn set(&mut self, key: KeyCode, pressed: bool) {
        let held = match key {
            KeyCode::KeyW => &mut self.forward,
            KeyCode::KeyS => &mut self.back,
            KeyCode::KeyA => &mut self.left,
            KeyCode::KeyD => &mut self.right,
            KeyCode::KeyE => &mut self.up,
            KeyCode::KeyQ => &mut self.down,
            KeyCode::ShiftLeft | KeyCode::ShiftRight => &mut self.slow,
            _ => return,
        };
        *held = pressed;
    }
}

#[derive(Clone, Copy, Debug)]
pub struct FlyCamera {
    pub position: Vec3,
    /// Turn left and right, in radians. 0 looks toward -Z.
    yaw: f32,
    /// Look up (positive) and down, in radians.
    pitch: f32,
}

impl FlyCamera {
    pub fn looking_at(position: Vec3, target: Vec3) -> Self {
        let direction = (target - position).normalize_or(Vec3::NEG_Z);
        Self {
            position,
            yaw: f32::atan2(-direction.x, -direction.z),
            pitch: direction.y.asin().clamp(-MAX_PITCH, MAX_PITCH),
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

    /// Turns the camera by a mouse movement in pixels (right and down are positive).
    pub fn look(&mut self, dx: f32, dy: f32) {
        self.yaw -= dx * MOUSE_SENSITIVITY;
        self.pitch = (self.pitch - dy * MOUSE_SENSITIVITY).clamp(-MAX_PITCH, MAX_PITCH);
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
            SPEED * SLOW_FACTOR
        } else {
            SPEED
        };
        self.position += direction * speed * seconds;
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
    }

    #[test]
    fn default_direction_is_toward_negative_z_with_right_along_x() {
        let camera = FlyCamera::looking_at(Vec3::ZERO, Vec3::NEG_Z);
        assert_near(camera.forward(), Vec3::NEG_Z);
        assert_near(camera.right(), Vec3::X);
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
        assert!((slow.position.length() - SPEED).abs() < 1e-3);
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
        assert_near(camera.position, Vec3::Y * SPEED * SLOW_FACTOR);
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
    fn keys_are_recorded_and_released() {
        let mut keys = MoveKeys::default();
        keys.set(KeyCode::KeyW, true);
        keys.set(KeyCode::ShiftRight, true);
        keys.set(KeyCode::KeyZ, true);
        assert!(keys.forward && keys.slow);
        keys.set(KeyCode::KeyW, false);
        assert!(!keys.forward);
    }
}
