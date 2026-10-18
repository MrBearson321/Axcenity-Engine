//! Everything the renderer needs to draw one frame.
//!
//! Conventions:
//! - Y is up, and the coordinate system is right-handed.
//! - Colors are sRGB from 0 to 1, as people pick them. The renderer converts
//!   them to linear light itself.

use glam::{Mat4, Vec3};

#[derive(Clone, Debug)]
pub struct DrawList {
    /// The background color where nothing is drawn.
    pub sky_color: [f32; 3],
    pub camera: Camera,
    pub sun: Sun,
    pub ambient: AmbientLight,
    pub items: Vec<DrawItem>,
}

#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub position: Vec3,
    /// The point at the center of the view.
    pub look_at: Vec3,
    /// The vertical field of view.
    pub fov_y_degrees: f32,
}

/// A light infinitely far away that shines in one direction, like the sun.
#[derive(Clone, Copy, Debug)]
pub struct Sun {
    /// The direction from the scene *toward* the sun. Doesn't need to be normalized.
    pub direction: Vec3,
    pub color: [f32; 3],
    pub intensity: f32,
}

/// Light that reaches every surface equally, standing in for light bounced
/// around the scene.
#[derive(Clone, Copy, Debug)]
pub struct AmbientLight {
    pub color: [f32; 3],
    pub intensity: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct DrawItem {
    pub shape: Shape,
    /// Places the shape in the world. Shapes are 1×1×1 and centered on the
    /// origin, so the transform's scale is the size.
    pub transform: Mat4,
    pub material: Material,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Block,
}

/// Physically based material fields. Presets such as plastic or metal are
/// just values for these.
#[derive(Clone, Copy, Debug)]
pub struct Material {
    pub base_color: [f32; 3],
    /// 0 for non-metals such as plastic, 1 for metal.
    pub metalness: f32,
    /// 0 is mirror-smooth, 1 is fully matte.
    pub roughness: f32,
    /// Light the surface gives off by itself.
    pub emissive: [f32; 3],
    /// 1 is solid. Transparency isn't drawn yet; see `docs/rendering.md`.
    pub opacity: f32,
}

impl Default for Material {
    fn default() -> Self {
        Self {
            base_color: [0.64, 0.64, 0.64],
            metalness: 0.0,
            roughness: 0.5,
            emissive: [0.0, 0.0, 0.0],
            opacity: 1.0,
        }
    }
}
