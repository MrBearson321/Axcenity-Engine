use glam::Vec3;

/// Where a frame is seen from.
#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub position: Vec3,
    /// The point at the center of the view.
    pub look_at: Vec3,
    /// The vertical field of view.
    pub fov_y_degrees: f32,
}
