//! Everything the renderer needs to draw one frame.
//!
//! Conventions:
//! - Y is up, and the coordinate system is right-handed.
//! - Colors are sRGB from 0 to 1, as people pick them. The renderer converts
//!   them to linear light itself.

use std::fmt;
use std::sync::Arc;

use glam::{Mat4, Vec3};

use crate::ToneMapping;

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
    /// The project's own fonts, which UI text can name besides the built-in
    /// ones. Each is loaded the first time it's seen.
    pub fonts: Vec<crate::UiFont>,
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

#[derive(Clone, Copy, Debug, Default)]
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
    /// Each side's own border (top, right, bottom, left), which replaces
    /// `border_color` and `border_width`. Where two sides meet, the corner
    /// is split on the diagonal.
    pub border_sides: Option<[UiBorderSide; 4]>,
    /// Drawn as a soft shadow in `color` instead of a box (CSS `box-shadow`).
    pub shadow: Option<UiShadow>,
    /// Only the part inside this is drawn (CSS `overflow: hidden` on an
    /// element around it).
    pub clip: Option<UiClip>,
    /// How far it's turned, in degrees clockwise, around the middle of
    /// its box (`position` and `size` are the box before it's turned). A
    /// shadow turns with it, around the same point.
    pub rotation_degrees: f32,
}

/// The rounded box a UI item is cut to: what's outside it isn't drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct UiClip {
    /// Its top left corner, in pixels, before it's turned.
    pub position: [f32; 2],
    pub size: [f32; 2],
    /// How round its corners are, in pixels. Text is cut to the box's
    /// square corners.
    pub radius: f32,
    /// How far the box is turned, in degrees clockwise around its middle
    /// (inside a turned element). Text is cut to the box around it.
    pub rotation_degrees: f32,
}

/// `point` turned `degrees` clockwise on screen (y down) around `centre`.
pub fn turn_point(point: [f32; 2], centre: [f32; 2], degrees: f32) -> [f32; 2] {
    let (sin, cos) = degrees.to_radians().sin_cos();
    let [dx, dy] = [point[0] - centre[0], point[1] - centre[1]];
    [
        centre[0] + dx * cos - dy * sin,
        centre[1] + dx * sin + dy * cos,
    ]
}

/// Whether a turn of `degrees` turns anything: not a whole number of turns.
pub fn is_turned(degrees: f32) -> bool {
    let turn = degrees.rem_euclid(360.0);
    degrees.is_finite() && turn > 0.01 && turn < 359.99
}

/// The upright box (left, top, right, bottom) around `[x, y, w, h]`
/// turned `degrees` around its middle.
pub fn turned_bounds([x, y, w, h]: [f32; 4], degrees: f32) -> [f32; 4] {
    if !is_turned(degrees) {
        return [x, y, x + w, y + h];
    }
    let centre = [x + w * 0.5, y + h * 0.5];
    let corners = [[x, y], [x + w, y], [x, y + h], [x + w, y + h]]
        .map(|corner| turn_point(corner, centre, degrees));
    let mut bounds = [
        f32::INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
    ];
    for [cx, cy] in corners {
        bounds = [
            bounds[0].min(cx),
            bounds[1].min(cy),
            bounds[2].max(cx),
            bounds[3].max(cy),
        ];
    }
    bounds
}

impl UiClip {
    /// The part of `self` that's also inside `other`, rounded as the
    /// smaller of the two (the inner one, when one is inside the other).
    /// Two boxes turned differently can't be cut to one box: the smaller
    /// of the two is kept.
    pub fn intersect(self, other: UiClip) -> UiClip {
        let area = |clip: UiClip| clip.size[0] * clip.size[1];
        if (self.rotation_degrees - other.rotation_degrees).abs() > 0.01 {
            return if area(self) <= area(other) {
                self
            } else {
                other
            };
        }
        if is_turned(self.rotation_degrees) {
            // Cut in the turned boxes' own frame: unturn both around the
            // same point, cut, and turn the result back.
            let pivot = self.centre();
            let upright = |clip: UiClip| {
                let centre = turn_point(clip.centre(), pivot, -clip.rotation_degrees);
                UiClip {
                    position: [
                        centre[0] - clip.size[0] * 0.5,
                        centre[1] - clip.size[1] * 0.5,
                    ],
                    rotation_degrees: 0.0,
                    ..clip
                }
            };
            let cut = upright(self).intersect(upright(other));
            let centre = turn_point(cut.centre(), pivot, self.rotation_degrees);
            return UiClip {
                position: [centre[0] - cut.size[0] * 0.5, centre[1] - cut.size[1] * 0.5],
                rotation_degrees: self.rotation_degrees,
                ..cut
            };
        }
        let left = self.position[0].max(other.position[0]);
        let top = self.position[1].max(other.position[1]);
        let right = (self.position[0] + self.size[0]).min(other.position[0] + other.size[0]);
        let bottom = (self.position[1] + self.size[1]).min(other.position[1] + other.size[1]);
        let radius = if area(self) <= area(other) {
            self.radius
        } else {
            other.radius
        };
        UiClip {
            position: [left, top],
            size: [(right - left).max(0.0), (bottom - top).max(0.0)],
            radius,
            rotation_degrees: 0.0,
        }
    }

    /// The middle of its box.
    pub fn centre(&self) -> [f32; 2] {
        [
            self.position[0] + self.size[0] * 0.5,
            self.position[1] + self.size[1] * 0.5,
        ]
    }

    /// Whether a point is inside it, rounded corners and all.
    pub fn contains(&self, x: f32, y: f32) -> bool {
        let [x, y] = turn_point([x, y], self.centre(), -self.rotation_degrees);
        let [w, h] = self.size;
        let half = [w * 0.5, h * 0.5];
        let radius = self.radius.clamp(0.0, half[0].min(half[1]));
        let dx = (x - self.position[0] - half[0]).abs() - (half[0] - radius);
        let dy = (y - self.position[1] - half[1]).abs() - (half[1] - radius);
        if dx > radius || dy > radius {
            return false;
        }
        dx <= 0.0 || dy <= 0.0 || dx * dx + dy * dy <= radius * radius
    }
}

/// One side of a [`UiRectangle`]'s border.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct UiBorderSide {
    pub width: f32,
    pub color: [f32; 4],
}

/// A [`UiRectangle`] drawn as a shadow of its rounded box.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct UiShadow {
    /// How far the shadow is moved, in pixels.
    pub offset: [f32; 2],
    /// How far it fades out over, in pixels. 0 is a hard edge.
    pub blur: f32,
    /// How much bigger the shadow is than the box (smaller, when inset).
    pub spread: f32,
    /// Drawn inside the box, around a hole the box's shape moved by
    /// `offset` and shrunk by `spread`, rather than behind it.
    pub inset: bool,
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
    /// Drawn in nine pieces (CSS `border-image`) instead of stretched whole.
    pub slice: Option<UiSlice>,
    /// Only the part inside this is drawn.
    pub clip: Option<UiClip>,
    /// How far it's turned, in degrees clockwise around its middle.
    pub rotation_degrees: f32,
}

/// How a [`UiImage`] is cut into nine pieces: the corners keep their size,
/// and the edges and middle stretch or repeat to fill the rest.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct UiSlice {
    /// Where the image is cut, in its own pixels from its top, right,
    /// bottom and left.
    pub insets: [f32; 4],
    /// How thick each side is drawn on screen, in pixels (top, right,
    /// bottom, left): the corners' size.
    pub widths: [f32; 4],
    /// Whether the middle is drawn too (CSS `fill`).
    pub fill: bool,
    /// Edges and the middle are tiled at the corners' scale, from the top
    /// left, instead of stretched.
    pub repeat: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct UiText {
    pub position: [f32; 2],
    pub size: [f32; 2],
    /// What it says, already in the case CSS `text-transform` asks for.
    pub text: String,
    pub font_size: f32,
    pub font_family: String,
    pub color: [f32; 4],
    /// Where each line goes across the box.
    pub align: TextAlign,
    /// Where the lines sit up and down in the box (CSS `vertical-align`).
    pub vertical_align: VerticalAlign,
    /// CSS `font-weight`, 100 to 900: 400 is normal and 700 bold. The
    /// renderer picks the family's nearest face, and thickens it when the
    /// family has nothing bold enough.
    pub weight: u16,
    /// CSS `font-style: italic`. Slanted when the family has no italic face.
    pub italic: bool,
    pub underline: bool,
    pub line_through: bool,
    /// An outline around each letter (CSS `-webkit-text-stroke`).
    pub stroke: Option<TextStroke>,
    /// Copies drawn behind the text, first one on top (CSS `text-shadow`).
    pub shadows: Vec<TextShadow>,
    /// Extra room after each letter, in pixels (CSS `letter-spacing`).
    pub letter_spacing: f32,
    /// From one line to the next, in pixels (CSS `line-height`). `None` is
    /// 1.2 times the font size.
    pub line_height: Option<f32>,
    /// False keeps each line on one line (CSS `white-space: nowrap`).
    pub wrap: bool,
    /// `TextScaled`: drawn at the largest size up to `font_size` that fits
    /// the box.
    pub scaled: bool,
    /// Rich text: the pieces of `text` in their own styles, in order, which
    /// together say `text`. Empty draws `text` in the fields above.
    pub spans: Vec<TextSpan>,
    /// Only the part inside this box is drawn (its corners are square for text).
    pub clip: Option<UiClip>,
    /// How far it's turned, in degrees clockwise around the middle of its
    /// box. Turned text is drawn upright off screen, then turned.
    pub rotation_degrees: f32,
}

/// A piece of rich text (`RichText`) and the style its tags give it.
/// Alignment, spacing, line height, wrapping and shadows stay the whole
/// text's.
#[derive(Clone, Debug, PartialEq)]
pub struct TextSpan {
    pub text: String,
    pub font_family: String,
    /// In pixels, like [`UiText::font_size`]. `TextScaled` text scales it
    /// with the rest.
    pub font_size: f32,
    pub weight: u16,
    pub italic: bool,
    pub underline: bool,
    pub line_through: bool,
    pub color: [f32; 4],
    pub stroke: Option<TextStroke>,
}

impl Default for UiText {
    fn default() -> Self {
        Self {
            position: [0.0; 2],
            size: [0.0; 2],
            text: String::new(),
            font_size: 16.0,
            font_family: "Montserrat".to_owned(),
            color: [1.0; 4],
            align: TextAlign::Left,
            vertical_align: VerticalAlign::Top,
            weight: 400,
            italic: false,
            underline: false,
            line_through: false,
            stroke: None,
            shadows: Vec::new(),
            letter_spacing: 0.0,
            line_height: None,
            wrap: true,
            scaled: false,
            spans: Vec::new(),
            clip: None,
            rotation_degrees: 0.0,
        }
    }
}

/// Where lines of UI text sit up and down in their box (CSS `vertical-align`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VerticalAlign {
    #[default]
    Top,
    Middle,
    Bottom,
}

/// An outline around UI text's letters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextStroke {
    /// In pixels.
    pub width: f32,
    pub color: [f32; 4],
}

/// A copy of UI text drawn behind it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextShadow {
    /// Right and down, in pixels.
    pub offset: [f32; 2],
    /// How soft it is, in pixels. 0 is sharp.
    pub blur: f32,
    pub color: [f32; 4],
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
    /// How strongly screen-space ambient occlusion darkens this light and
    /// the sky's in the corners and creases between solid items, from 0
    /// (not at all) to 2. 1 is normal. Only drawn while the graphics
    /// settings' ambient occlusion is on.
    pub occlusion: f32,
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
    /// `Some` if the beam casts shadows (a SpotLight's `Shadows`). Only the
    /// nearest few to the camera do, as the graphics settings allow.
    pub shadows: Option<SpotShadows>,
}

/// How a spot light's beam casts shadows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpotShadows {
    /// Studs along the beam before anything casts a shadow, so the part the
    /// light sits on doesn't shade its own beam.
    pub near: f32,
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
    /// The curve that brings bright light down to what a screen can show.
    pub tone_mapping: ToneMapping,
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
            tone_mapping: ToneMapping::Classic,
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
    /// Studs one copy of the textures covers, for a built-in shape. 0 keeps
    /// the mesh's own coordinates (imported models, and parts with no maps).
    pub uv_tile: f32,
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
    /// coordinates.
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
    /// More textures, placed like `base_color_texture`.
    pub maps: TextureMaps,
    /// Multiplies the texture coordinates, after [`DrawItem::uv_tile`]: 2
    /// repeats the textures twice as often.
    pub uv_scale: [f32; 2],
    /// Turns the scaled texture coordinates around their origin, in
    /// radians, as glTF's `KHR_texture_transform` does (counter-clockwise
    /// in the texture, which turns the image clockwise).
    pub uv_rotation: f32,
    /// Added to the texture coordinates last, in copies of the texture:
    /// 0.5 shifts the textures by half.
    pub uv_offset: [f32; 2],
    /// Draws the backs of triangles too, lit like their fronts (leaves,
    /// paper, cloth). Off, backs aren't drawn at all.
    pub double_sided: bool,
    /// For a terrain chunk: its blended layers, which replace everything
    /// above but `opacity` (terrain is always solid).
    pub terrain: Option<TerrainSurface>,
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
            uv_scale: [1.0, 1.0],
            uv_rotation: 0.0,
            uv_offset: [0.0, 0.0],
            double_sided: false,
            terrain: None,
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

/// One terrain chunk's material: up to eight layers, blended by weights
/// that vary across the chunk. The mesh's texture coordinates place the
/// weight textures; the layers' own textures are placed in the world.
#[derive(Clone, Debug)]
pub struct TerrainSurface {
    /// Shared by every chunk of the terrain.
    pub layers: Arc<TerrainLayers>,
    /// The weights of layers 1 to 4 (red, green, blue, alpha) and 5 to 8,
    /// as plain values adding up to 1 at each texel.
    pub weights: [Arc<dyn TextureSource>; 2],
}

/// What a terrain is drawn with, besides each chunk's weights.
#[derive(Clone, Debug)]
pub struct TerrainLayers {
    /// The same for the same terrain from frame to frame, so its GPU copy
    /// is updated in place.
    pub id: u64,
    /// Changes whenever anything else here does.
    pub key: u64,
    /// At most eight; more are left out.
    pub layers: Vec<TerrainLayer>,
    pub view: TerrainView,
    /// The world Y of the bottom and top of the terrain's height range,
    /// for the Height view.
    pub height_range: [f32; 2],
    /// The world X and Z of the terrain's corner, and the studs between
    /// samples, for the Grid view.
    pub grid_origin: [f32; 2],
    pub grid_cell: [f32; 2],
}

/// How a terrain is shown: shaded, or one of the editor's views of it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TerrainView {
    #[default]
    Shaded,
    /// Colored by height, blue low to white high.
    Height,
    /// Colored by steepness, green flat to red steep.
    Slope,
    /// Each layer in its own color, blended by its weights.
    Weights,
    /// Shaded, with lines along the grid of samples.
    Grid,
}

/// One material layer of a terrain. Maps are optional, as on other
/// materials; without one, the plain value is used everywhere.
#[derive(Clone, Debug)]
pub struct TerrainLayer {
    /// An sRGB tint that multiplies the base color texture.
    pub color: [f32; 3],
    pub base_color_texture: Option<Arc<dyn TextureSource>>,
    pub normal: Option<NormalMap>,
    /// Multiply `metalness` and `roughness`.
    pub metalness_map: Option<ChannelMap>,
    pub roughness_map: Option<ChannelMap>,
    pub occlusion: Option<ChannelMap>,
    /// Light is high: where layers meet, the higher one shows through.
    pub height: Option<ChannelMap>,
    pub metalness: f32,
    pub roughness: f32,
    /// Studs one copy of the textures covers.
    pub tile_size: f32,
    /// The textures' turn around the up axis.
    pub rotation_degrees: f32,
    /// Also project the textures from the sides, for cliffs.
    pub triplanar: bool,
    /// 0 to 1: how much a larger, turned copy is mixed in to hide repeats.
    pub anti_tiling: f32,
    /// 0 to 1: how sharply its height map decides where it meets others.
    pub height_blend: f32,
    /// Its sRGB color in the Weights view.
    pub debug_color: [f32; 3],
}
