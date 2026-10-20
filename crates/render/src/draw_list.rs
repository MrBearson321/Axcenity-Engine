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
    /// The sky behind the scene, which also lights and is reflected by it.
    pub sky: Sky,
    pub camera: Camera,
    pub sun: Sun,
    pub ambient: AmbientLight,
    pub shadows: Shadows,
    /// Point and spot lights. When there are more than the renderer can
    /// handle, the ones closest to the camera are used.
    pub lights: Vec<Light>,
    pub post: PostProcessing,
    pub items: Vec<DrawItem>,
    /// Pictures that always face the camera, such as particles, drawn
    /// after every item. They aren't lit or shadowed.
    pub sprites: Vec<Sprite>,
    /// Screen-space UI, drawn after the 3D scene.
    pub ui: Vec<UiDrawItem>,
}

/// A picture in the 3D scene that always faces the camera, such as one
/// particle. It's hidden behind solid items, and never hides anything.
#[derive(Clone, Debug)]
pub struct Sprite {
    pub position: Vec3,
    /// How many studs across it is.
    pub size: f32,
    /// How far it's turned around the line of sight, in degrees.
    pub rotation_degrees: f32,
    /// An sRGB color that tints the texture.
    pub color: [f32; 3],
    /// 1 is solid, 0 invisible. Multiplies the texture's alpha.
    pub opacity: f32,
    /// Multiplies the color: 1 shows it as it is; above 1 glows and blooms.
    pub brightness: f32,
    /// 0 blends it over what's behind, like paint; 1 adds its light, like
    /// fire. In between mixes the two.
    pub light_emission: f32,
    /// Its picture, whose alpha is its shape. `None` is a plain square.
    pub texture: Option<Arc<dyn TextureSource>>,
}

#[derive(Clone, Debug)]
pub enum UiDrawItem {
    Rectangle(UiRectangle),
    Image(UiImage),
    Text(UiText),
}

#[derive(Clone, Copy, Debug)]
pub struct UiRectangle {
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub color: [f32; 4],
    /// The color at the bottom of a vertical gradient. The same as `color`
    /// when the rectangle is a flat fill.
    pub color_end: [f32; 4],
    pub border_color: [f32; 4],
    pub border_width: f32,
    pub border_radius: f32,
}

/// A picture in a UI rectangle. `texture` is filled in by `scene` from
/// `path` before drawing; the UI crate only knows the project-relative path.
#[derive(Clone, Debug)]
pub struct UiImage {
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub tint: [f32; 4],
    pub border_radius: f32,
    pub path: String,
    pub texture: Option<Arc<dyn TextureSource>>,
}

#[derive(Clone, Debug)]
pub struct UiText {
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub text: String,
    pub font_size: f32,
    pub font_family: String,
    pub color: [f32; 4],
    /// Where each line goes across the box.
    pub align: TextAlign,
    /// True to center the text up and down in its box; false puts it at the top.
    pub center_vertically: bool,
}

/// Where lines of UI text go across their box (CSS `text-align`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
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

/// A procedural sky: a gradient from the ground up through the horizon to
/// the top of the sky, with the sun in it. Surfaces are lit by it and metal
/// and glossy surfaces reflect it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sky {
    /// sRGB colors, like every color in the draw list.
    pub top_color: [f32; 3],
    pub horizon_color: [f32; 3],
    pub ground_color: [f32; 3],
    /// How strongly the sky lights and is reflected by surfaces. 0 leaves
    /// only the sun, the ambient light and point lights.
    pub environment_intensity: f32,
}

impl Default for Sky {
    fn default() -> Self {
        Self {
            top_color: [0.25, 0.5, 0.9],
            horizon_color: [0.63, 0.8, 0.95],
            ground_color: [0.35, 0.33, 0.3],
            environment_intensity: 0.6,
        }
    }
}

/// A light at one point that shines on what's near it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Light {
    pub position: Vec3,
    pub color: [f32; 3],
    /// 1 is a modest lamp; the sun's intensity is 3 for comparison.
    pub brightness: f32,
    /// How far the light reaches, in studs. It fades smoothly to nothing there.
    pub range: f32,
    /// `None` shines in every direction (a point light), `Some` in a cone.
    pub cone: Option<Cone>,
}

/// The shape of a spot light's beam.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cone {
    /// The way the beam points. Doesn't need to be normalized.
    pub direction: Vec3,
    /// The full width of the beam, in degrees, from 1 to 179. The edge
    /// fades over the outer fifth of it.
    pub angle_degrees: f32,
}

/// What's done to the scene after it's drawn in high dynamic range, where
/// brightness can go past 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PostProcessing {
    /// Off draws the scene as it is, with anything brighter than 1 clipped
    /// to white and no bloom.
    pub enabled: bool,
    /// Multiplies the scene's brightness before tone mapping.
    pub exposure: f32,
    pub bloom_enabled: bool,
    /// How much of the bloom is added to the scene.
    pub bloom_intensity: f32,
    /// Only light brighter than this blooms.
    pub bloom_threshold: f32,
}

impl Default for PostProcessing {
    fn default() -> Self {
        Self {
            enabled: true,
            exposure: 1.0,
            bloom_enabled: true,
            bloom_intensity: 0.35,
            bloom_threshold: 1.0,
        }
    }
}

/// Shadows cast by the sun. They're drawn only near the camera, in
/// cascades: the nearest part of the view gets the sharpest shadow map.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shadows {
    /// Off for low-end machines: nothing is drawn into a shadow map.
    pub enabled: bool,
    /// How far from the camera shadows reach, in studs.
    pub distance: f32,
    /// How soft shadow edges are, from 0 (crisp everywhere) to 1 (very
    /// soft). Edges are always sharpest where a caster touches what it
    /// shades, and soften with the distance between them, like real ones.
    pub softness: f32,
}

impl Default for Shadows {
    fn default() -> Self {
        Self {
            enabled: true,
            distance: 150.0,
            softness: 0.2,
        }
    }
}

#[derive(Clone, Debug)]
pub struct DrawItem {
    pub shape: Shape,
    /// Places the shape in the world. Shapes are 1×1×1 and centered on the
    /// origin, so the transform's scale is the size.
    pub transform: Mat4,
    pub material: Material,
    /// False draws the item without it casting a sun shadow. It is still
    /// shaded by other items' shadows.
    pub casts_shadow: bool,
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
    /// Tangents for normal maps, one per position, or empty for none: the
    /// direction texture coordinate U grows along the surface (one unit
    /// long), and in `w` 1 or -1, the side of the normal the texture's "up"
    /// is on (`cross(normal, tangent) * w` points up the texture).
    fn tangents(&self) -> &[[f32; 4]] {
        &[]
    }
    /// Three per triangle, counter-clockwise seen from outside.
    fn indices(&self) -> &[u32];
    /// Changes when the corners do (an animated, posed mesh), with the same
    /// key and indices: the renderer then rewrites its copy's corners in
    /// place. A mesh that never changes keeps the default.
    fn version(&self) -> u64 {
        0
    }
}

/// An image the caller owns, to put on a surface ([`Material`]'s textures).
pub trait TextureSource: Send + Sync {
    /// The same for the same image, and different for every other image.
    fn key(&self) -> u64;
    /// Width and height in pixels.
    fn size(&self) -> (u32, u32);
    /// 4 bytes per pixel (red, green, blue, alpha), rows from the top.
    /// Color textures (base color, emission) are sRGB-encoded like the
    /// colors; the others hold plain values (128 is 0.5).
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
    /// More textures, placed like `base_color_texture`. Built-in shapes
    /// ignore them too.
    pub maps: TextureMaps,
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
            maps: TextureMaps::default(),
        }
    }
}

/// The textures of a material besides its base color. Each is optional;
/// without one, the material's own value is used everywhere.
#[derive(Clone, Debug)]
pub struct TextureMaps {
    /// Bumps and grooves, as surface directions.
    pub normal: Option<NormalMap>,
    /// Multiplies [`Material::metalness`].
    pub metalness: Option<ChannelMap>,
    /// Multiplies [`Material::roughness`].
    pub roughness: Option<ChannelMap>,
    /// How much ambient light reaches each point: 1 everywhere, 0 deep in
    /// cracks. Sunlight isn't affected.
    pub occlusion: Option<ChannelMap>,
    /// How strongly `occlusion` darkens, from 0 (not at all) to 1.
    pub occlusion_strength: f32,
    /// sRGB colors that multiply [`Material::emissive`].
    pub emissive: Option<Arc<dyn TextureSource>>,
    /// Replaces the base color texture's alpha as the opacity (which still
    /// multiplies [`Material::opacity`]).
    pub opacity: Option<ChannelMap>,
    /// How strongly non-metal parts reflect, per color, as plain values:
    /// 0.5 is the usual 4% of light, 0 none, 1 twice that. For older
    /// materials' specular maps.
    pub specular: Option<Arc<dyn TextureSource>>,
}

impl Default for TextureMaps {
    fn default() -> Self {
        Self {
            normal: None,
            metalness: None,
            roughness: None,
            occlusion: None,
            occlusion_strength: 1.0,
            emissive: None,
            opacity: None,
            specular: None,
        }
    }
}

/// A tangent-space normal map: red is the U direction, green the texture's
/// "up", blue straight out of the surface, each from 0 (-1) to 255 (+1).
#[derive(Clone, Debug)]
pub struct NormalMap {
    pub texture: Arc<dyn TextureSource>,
    /// Scales how far the normals tip (1 as authored, 0 flat).
    pub scale: f32,
    /// True for maps whose green points down the texture (DirectX's
    /// convention) rather than up (OpenGL's and glTF's).
    pub flip_green: bool,
}

/// One channel of a texture, used as a number from 0 to 1.
#[derive(Clone, Debug)]
pub struct ChannelMap {
    pub texture: Arc<dyn TextureSource>,
    pub channel: Channel,
    /// Use 1 minus the value: a glossiness or smoothness map as roughness.
    pub invert: bool,
}

/// A color channel of a texture.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Channel {
    #[default]
    Red,
    Green,
    Blue,
    Alpha,
}
