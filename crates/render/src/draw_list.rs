//! Everything the renderer needs to draw one frame.
//!
//! Conventions:
//! - Y is up, and the coordinate system is right-handed.
//! - Colors are sRGB from 0 to 1, as people pick them. The renderer converts
//!   them to linear light itself.

use std::fmt;
use std::sync::Arc;

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

#[derive(Clone, Debug)]
pub struct DrawItem {
    pub shape: Shape,
    /// Places the shape in the world. Shapes are 1×1×1 and centered on the
    /// origin, so the transform's scale is the size.
    pub transform: Mat4,
    pub material: Material,
}

/// What to draw: a built-in mesh, each fitting the 1×1×1 cube centered on
/// the origin, or a mesh of the caller's (an imported model).
#[derive(Clone, Debug)]
pub enum Shape {
    Block,
    Sphere,
    /// Lies along the X axis, with flat ends at x = ±0.5.
    Cylinder,
    /// A ramp: full bottom and back (+Z) faces, and a slope rising from the
    /// bottom front edge to the top back edge.
    Wedge,
    /// Uploaded to the GPU the first time it's drawn, and kept while it's
    /// drawn. `Arc` shares one copy of the data between draw items.
    Mesh(Arc<dyn MeshSource>),
}

/// Built-in shapes are equal by kind, meshes by [`MeshSource::key`].
impl PartialEq for Shape {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Shape::Mesh(a), Shape::Mesh(b)) => a.key() == b.key(),
            _ => std::mem::discriminant(self) == std::mem::discriminant(other),
        }
    }
}

/// Triangles to draw that the caller owns, such as part of an imported
/// model. Like the built-in shapes, they should fit the 1×1×1 cube centered
/// on the origin; the draw item's transform sizes them.
///
/// A trait, so callers hand over their own data without copying it each frame.
pub trait MeshSource: Send + Sync {
    /// The same for the same mesh, and different for every other mesh: the
    /// renderer keeps one GPU copy per key.
    fn key(&self) -> u64;
    fn positions(&self) -> &[[f32; 3]];
    /// One per position, one unit long.
    fn normals(&self) -> &[[f32; 3]];
    /// Texture coordinates, one per position, or empty for none. (0, 0) is
    /// the top left of the texture.
    fn uvs(&self) -> &[[f32; 2]];
    /// Three per triangle, counter-clockwise seen from outside.
    fn indices(&self) -> &[u32];
}

/// An image the caller owns, to color a surface with
/// ([`Material::base_color_texture`]).
pub trait TextureSource: Send + Sync {
    /// The same for the same image, and different for every other image.
    fn key(&self) -> u64;
    /// Width and height in pixels.
    fn size(&self) -> (u32, u32);
    /// 4 bytes per pixel (red, green, blue, alpha), sRGB-encoded like the
    /// colors, rows from the top.
    fn rgba(&self) -> &[u8];
}

// So `Shape` and `Material` can derive `Debug`, which needs every field to
// be printable. Only the key is shown: the data could be huge.
impl fmt::Debug for dyn MeshSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Mesh #{}", self.key())
    }
}

impl fmt::Debug for dyn TextureSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (width, height) = self.size();
        write!(f, "Texture #{} ({width}×{height})", self.key())
    }
}

/// Physically based material fields. Presets such as plastic or metal are
/// just values for these.
#[derive(Clone, Debug)]
pub struct Material {
    pub base_color: [f32; 3],
    /// Multiplies `base_color` and `opacity`, placed by the mesh's texture
    /// coordinates. Built-in shapes have none, so they ignore it.
    pub base_color_texture: Option<Arc<dyn TextureSource>>,
    /// 0 for non-metals such as plastic, 1 for metal.
    pub metalness: f32,
    /// 0 is mirror-smooth, 1 is fully matte.
    pub roughness: f32,
    /// Light the surface gives off by itself.
    pub emissive: [f32; 3],
    /// 1 is solid, 0 is invisible. Anything below 1 is blended over what's
    /// behind it.
    pub opacity: f32,
    /// Pixels whose opacity (with the texture's) is below this aren't drawn
    /// at all, for cut-out leaves and fences. 0 cuts nothing.
    pub alpha_cutoff: f32,
}

impl Default for Material {
    fn default() -> Self {
        Self {
            base_color: [0.64, 0.64, 0.64],
            base_color_texture: None,
            metalness: 0.0,
            roughness: 0.5,
            emissive: [0.0, 0.0, 0.0],
            opacity: 1.0,
            alpha_cutoff: 0.0,
        }
    }
}
