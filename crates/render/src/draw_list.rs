//! Everything the renderer needs to draw one frame.
//!
//! Conventions:
//! - Y is up, and the coordinate system is right-handed.
//! - Colors are sRGB from 0 to 1, as people pick them. The renderer converts
//!   them to linear light itself.

use std::fmt;
use std::sync::Arc;

use glam::{Mat3, Mat4, Quat, Vec3};

pub use axcenity_render_types::Camera;

use crate::{Atmosphere, ToneMapping};

#[derive(Clone, Debug)]
pub struct DrawList {
    /// The sky behind the scene, which also lights and is reflected by it.
    pub sky: Sky,
    /// Air that hides what's far away: items, terrain, sprites and the sky
    /// fade into it with distance.
    pub fog: Fog,
    /// A layer of cloud high above, drawn in the sky, whose shadows fall
    /// on the world.
    pub clouds: Clouds,
    /// How wet and snow-covered surfaces are, which every lit item and
    /// the terrain show.
    pub weather: Weather,
    /// The wind (the Workspace's `GlobalWind`), in studs a second. Items
    /// with a [`DrawItem::sway`] bend along its level part.
    pub wind: Vec3,
    pub camera: Camera,
    pub sun: Sun,
    pub ambient: AmbientLight,
    pub shadows: Shadows,
    /// Point and spot lights. When there are more than the renderer can
    /// handle, the ones closest to the camera are used.
    pub lights: Vec<Light>,
    pub post: PostProcessing,
    /// Lines drawn along the edges of everything solid, found on screen
    /// (Lighting's `OutlineEffect`). `None` draws none, and costs nothing.
    pub edges: Option<Edges>,
    /// The finished picture's colors changed after tone mapping
    /// (Lighting's `ColorCorrectionEffect`). `None` leaves them as they are,
    /// and costs nothing.
    pub grading: Option<ColorGrading>,
    /// Depth of field, a vignette, film grain, chromatic aberration and
    /// sharpening (Lighting's lens effects). The default has none, and
    /// costs nothing.
    pub lens: LensEffects,
    pub items: Vec<DrawItem>,
    /// Items kept between frames by whoever made them, in groups of nearby
    /// items (see [`ItemChunk`]): a painted forest has tens of thousands,
    /// which cost more to list again each frame than to draw.
    pub chunks: Vec<ItemChunk>,
    /// Pictures that always face the camera, such as particles, drawn
    /// after every item. They aren't lit or shadowed.
    pub sprites: Vec<Sprite>,
    /// Particle emitters whose particles live and move on the GPU, drawn
    /// like sprites just before [`DrawList::sprites`]. Only drawn where
    /// [`crate::Capabilities::compute_shaders`] holds.
    pub particle_emitters: Vec<ParticleEmitter>,
    /// Boxes that project pictures onto the solid surfaces inside them
    /// (`ProjectedDecal`s), drawn over solid items before see-through ones.
    /// The graphics settings' quality limits how many and how far
    /// ([`crate::EffectiveGraphics::decals`]), and Low draws none.
    pub decals: Vec<crate::Decal>,
    /// Screen-space UI, drawn after the 3D scene.
    pub ui: Vec<UiDrawItem>,
    /// The project's own fonts, which UI text can name besides the built-in
    /// ones. Each is loaded the first time it's seen.
    pub fonts: Vec<crate::UiFont>,
    /// Baked reflections of parts of the world, reflected by what's inside
    /// their boxes in place of the sky. Only drawn while the reflection
    /// quality bakes environment maps (Medium and above).
    pub reflection_probes: Vec<ReflectionProbe>,
    /// Grids of baked light, which light what's inside their boxes in place
    /// of the sky's diffuse light.
    pub light_probe_volumes: Vec<LightProbeVolume>,
    /// Seconds since the caller started counting, which surface shaders
    /// read to move ([`Material::surface_shader`]). Kept small (the player
    /// and the editor count from when they started), as an `f32` loses
    /// precision as it grows.
    pub time: f32,
    /// The editor's measuring grid. `None` draws none, and costs nothing;
    /// a built game never has one.
    pub grid: Option<GridPlane>,
}

/// A flat rectangle of evenly spaced lines, hidden by what stands in front
/// of it: the editor's measuring grid.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GridPlane {
    /// The Y of the rectangle.
    pub height: f32,
    /// Its smallest X and Z.
    pub min: [f32; 2],
    /// Its largest X and Z.
    pub max: [f32; 2],
    /// Studs between lines.
    pub spacing: f32,
    /// How many lines to each heavier line.
    pub major_every: f32,
    /// How far from the camera the lines have faded out; 0 never.
    pub fade_distance: f32,
    /// The lines' linear color.
    pub color: [f32; 3],
    /// How solid the lines are, as a multiple of the usual soft look.
    pub strength: f32,
    /// A place whose color the lines near it take on.
    pub tint: Option<GridTint>,
}

/// A place on a [`GridPlane`] whose color nearby lines take on: a selected
/// or glowing part standing on it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GridTint {
    /// Its X and Z.
    pub at: [f32; 2],
    /// How far from it (studs) the lines are back to their own color.
    pub radius: f32,
    /// How much of its color the lines right by it take, 0 to 1.
    pub amount: f32,
    /// Its linear color.
    pub color: [f32; 3],
}

/// A box of the world lit by a grid of probes baked inside it, in place of
/// the sky's diffuse light (a `LightProbeVolume`).
#[derive(Clone, Debug, PartialEq)]
pub struct LightProbeVolume {
    /// The middle of its box.
    pub center: Vec3,
    /// How its box is turned.
    pub rotation: Quat,
    /// Half its box's size along its own axes.
    pub half_size: Vec3,
    /// How far inside its box's faces, in studs, its light fades in from
    /// what's around it.
    pub blend_distance: f32,
    pub probes: LightProbeGrid,
}

/// The light a volume's probes saw. The probes stand in a grid filling the
/// box, each in the middle of its cell: probe `(x, y, z)` of `counts` is at
/// `((x + 0.5) / counts.x - 0.5, …)` times the box's size, in the box's own
/// space.
#[derive(Clone)]
pub struct LightProbeGrid {
    /// Different for every bake, so the renderer uploads each one once.
    pub key: u64,
    /// How many probes stand along the box's own X, Y and Z.
    pub counts: [u32; 3],
    /// Each probe's light, X first, then Y, then Z: the light reaching it as
    /// four spherical harmonics coefficients (the constant one, then along
    /// Y, Z and X), each linear red, green and blue, already blurred over a
    /// hemisphere and divided by π.
    pub probes: Arc<Vec<[[f32; 3]; 4]>>,
    /// The probes that stood inside a solid when baked, by their place in
    /// `probes`: surfaces leave them out of their blend (roadmap R18.2).
    pub invalid: Arc<Vec<u32>>,
    /// The share of each probe's light that came from the sky, kept apart
    /// so it can follow the sky drawn now (roadmap R18.3). `None` keeps all
    /// the light as baked.
    pub sky: Option<ProbeSky>,
}

/// The sky's share of a light probe grid's light, and the sky it came
/// from.
#[derive(Clone, Debug, PartialEq)]
pub struct ProbeSky {
    /// The sky's average light when baked, linear red, green and blue, as
    /// the renderer measures it ([`TextureRenderer::sky_band0`]): the
    /// sky's share is scaled by the sky drawn now over this, per channel.
    ///
    /// [`TextureRenderer::sky_band0`]: crate::TextureRenderer::sky_band0
    pub band0: [f32; 3],
    /// Each probe's light from the sky, as [`LightProbeGrid::probes`].
    pub probes: Arc<Vec<[[f32; 3]; 4]>>,
}

impl fmt::Debug for LightProbeGrid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [x, y, z] = self.counts;
        write!(f, "LightProbeGrid #{} ({x}×{y}×{z})", self.key)
    }
}

impl PartialEq for LightProbeGrid {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}

/// A box of the world that reflects what was baked from its middle, in
/// place of the sky (a `ReflectionProbe`).
#[derive(Clone, Debug, PartialEq)]
pub struct ReflectionProbe {
    /// The middle of its box, where it was baked from.
    pub center: Vec3,
    /// How its box is turned.
    pub rotation: Quat,
    /// Half its box's size along its own axes.
    pub half_size: Vec3,
    /// How far inside its box's faces, in studs, it fades in from what's
    /// around it.
    pub blend_distance: f32,
    pub image: ProbeImage,
}

/// What a reflection probe saw from the middle of its box: six square
/// faces of a cube in linear light.
#[derive(Clone)]
pub struct ProbeImage {
    /// Different for every bake, so the renderer filters each one once.
    pub key: u64,
    /// Each face's width and height in pixels.
    pub size: u32,
    /// Red, green and blue for each pixel of the faces +X, -X, +Y, -Y, +Z
    /// and -Z in turn, each as a cubemap keeps it (looking out from the
    /// middle, rows from the top).
    pub rgb: Arc<Vec<f32>>,
}

impl fmt::Debug for ProbeImage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ProbeImage #{} ({}² × 6)", self.key, self.size)
    }
}

impl PartialEq for ProbeImage {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
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
    /// Shows one frame of the picture cut into a grid (a flipbook, roadmap
    /// R17.2c) in place of the whole of it.
    pub frame: Option<FlipbookFrame>,
}

/// One frame of a picture cut into `grid` × `grid` frames, counted left to
/// right, then top to bottom.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FlipbookFrame {
    pub grid: u32,
    pub frame: u32,
}

/// Which of a flipbook's frames a particle shows as it ages (roadmap
/// R17.2c, `ParticleEmitter.FlipbookMode`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FlipbookMode {
    /// The frames in order at the frame rate, then again from the first.
    #[default]
    Loop,
    /// Every frame once over the particle's life, whatever the frame rate.
    OneShot,
    /// The frames in order at the frame rate, then back again.
    PingPong,
    /// A frame picked at random, again at the frame rate.
    Random,
}

impl FlipbookMode {
    /// The number particles.wgsl's `flipbook_frame` reads.
    pub fn code(self) -> u32 {
        match self {
            FlipbookMode::Loop => 0,
            FlipbookMode::OneShot => 1,
            FlipbookMode::PingPong => 2,
            FlipbookMode::Random => 3,
        }
    }
}

/// A particle picture cut into frames, shown one at a time (roadmap
/// R17.2c).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Flipbook {
    /// Frames along each side: 2, 4 or 8.
    pub grid: u32,
    pub mode: FlipbookMode,
    /// Frames a second (not used by `OneShot`).
    pub framerate: f32,
    /// Each particle starts on a frame of its own instead of the first
    /// (`Loop` and `PingPong`).
    pub start_random: bool,
}

impl Flipbook {
    /// The frame a particle `age` seconds into a `life` of seconds shows,
    /// with `seed` a number that stays the same over its life: the same
    /// sum as particles.wgsl's `flipbook_frame`.
    pub fn frame(&self, age: f32, life: f32, seed: u32) -> u32 {
        let grid = self.grid.clamp(1, 8);
        let frames = grid * grid;
        let start = if self.start_random {
            pcg(seed ^ FLIPBOOK_SALT) % frames
        } else {
            0
        };
        let rate = if self.framerate.is_finite() {
            self.framerate.max(0.0)
        } else {
            0.0
        };
        let steps = (age.max(0.0) * rate).floor().min(16_777_216.0) as u32;
        match self.mode {
            FlipbookMode::OneShot => {
                let t = (age / life.max(1e-4)).clamp(0.0, 1.0);
                ((t * frames as f32).floor() as u32).min(frames - 1)
            }
            FlipbookMode::Loop => (start + steps) % frames,
            FlipbookMode::PingPong => {
                if frames == 1 {
                    return 0;
                }
                let round = 2 * frames - 2;
                let k = (start + steps) % round;
                if k < frames { k } else { round - k }
            }
            FlipbookMode::Random => pcg(seed ^ pcg(steps)) % frames,
        }
    }
}

/// Mixed into a particle's seed for its flipbook's first frame, so it
/// doesn't follow its curves' spreads.
const FLIPBOOK_SALT: u32 = 0x68e3_1da4;

/// A particle emitter whose particles the renderer keeps, moves and sorts
/// on the GPU (compute shaders), for counts too large to send as
/// [`Sprite`]s each frame. The caller decides when particles are born, as
/// for sprites; the renderer moves them as `scene`'s particles move (see
/// `docs/rendering/particles.md`) and draws each as a sprite.
#[derive(Clone, Debug)]
pub struct ParticleEmitter {
    /// Stays the same for as long as its particles should live: the
    /// renderer keeps them by it, and frees them in the first frame drawn
    /// without it.
    pub key: u64,
    /// Counts up each time the caller moves particles on. Drawing the same
    /// step again (a second view, a still) doesn't move them again.
    pub step: u64,
    /// The seconds this step moves particles on by.
    pub seconds: f32,
    /// How many particles it holds, a power of two. A birth past it takes
    /// the oldest particle's place. A new capacity starts it empty.
    pub capacity: u32,
    /// Particles born before this step (counting on past `u32::MAX`).
    pub born: u32,
    /// Particles born this step, at most `capacity`.
    pub births: u32,
    /// Where they're born: the transform that stretches the 1×1×1 cube
    /// (centered on the origin) to fill the emitter's part.
    pub spawn: Mat4,
    /// The way they head when born, in world space, one stud long.
    pub direction: Vec3,
    /// The most they head away from `direction`, in radians.
    pub spread: f32,
    /// Studs a second when born.
    pub speed: f32,
    /// Each lives from 0.8 to 1.2 times this, in seconds.
    pub lifetime: f32,
    /// Studs a second, each second.
    pub acceleration: Vec3,
    /// How fast they slow toward `air`: `v = air + (v − air) / (1 + drag·dt)`.
    pub drag: f32,
    /// The air's speed: still, or the wind.
    pub air: Vec3,
    /// Where in the spawn box they're born (roadmap R17.2).
    pub shape: SpawnShape,
    /// Which way they head from it (roadmap R17.2b).
    pub in_out: ShapeInOut,
    /// Added to every new particle's velocity: the emitter's own velocity
    /// times its `VelocityInheritance` (roadmap R17.2b).
    pub inherit: Vec3,
    /// Lands its particles on what the depth prepass shows in front of
    /// them (roadmap R17.4b), or `None` to let them pass through.
    pub collision: Option<ParticleCollision>,
    pub look: ParticleLook,
}

/// How GPU particles land on surfaces, as `scene`'s colliding particles
/// do on the CPU (`ParticleEmitter.Bounce`, `LifetimeLoss`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ParticleCollision {
    /// How much of their speed into a surface they keep, back out of it.
    pub bounce: f32,
    /// How much of their life each landing takes.
    pub lifetime_loss: f32,
}

/// The shape particles are born in, filling the spawn transform's 1×1×1
/// cube (roadmap R17.2, `ParticleEmitter.Shape` and `ShapeStyle`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SpawnShape {
    pub kind: SpawnKind,
    /// Only on the shape's surface, not anywhere inside it.
    pub surface: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SpawnKind {
    /// The whole cube (the surface: its six faces, by their area).
    #[default]
    Box,
    /// The ball inside the cube (its surface: the sphere).
    Sphere,
    /// The upright cylinder inside the cube, along its Y (its surface:
    /// the round side).
    Cylinder,
    /// The flat disc across the cube's middle, facing its Y (its surface:
    /// the rim).
    Disc,
}

/// Which way particles head from their spawn shape (roadmap R17.2b,
/// `ParticleEmitter.ShapeInOut`), within the spread.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ShapeInOut {
    /// Along the direction for a box; away from the middle for a sphere,
    /// and from the upright axis for a cylinder or disc.
    #[default]
    Outward,
    /// The other way.
    Inward,
    /// Either, at random.
    InAndOut,
}

impl ShapeInOut {
    /// The number particles.wgsl's `born` reads.
    pub fn code(self) -> u32 {
        match self {
            ShapeInOut::Outward => 0,
            ShapeInOut::Inward => 1,
            ShapeInOut::InAndOut => 2,
        }
    }
}

impl SpawnShape {
    /// The way out of the shape from a point born at `inside` (in the
    /// centered unit cube), before the spawn transform: from the middle for
    /// a sphere, from the upright axis for a cylinder or disc, and `None`
    /// for a box (which heads along the direction) or at the middle itself.
    /// The same as particles.wgsl's `born`.
    pub fn outward(self, inside: Vec3) -> Option<Vec3> {
        let out = match self.kind {
            SpawnKind::Box => return None,
            SpawnKind::Sphere => inside,
            SpawnKind::Cylinder | SpawnKind::Disc => Vec3::new(inside.x, 0.0, inside.z),
        };
        (out.length_squared() > 1e-12).then_some(out)
    }

    /// The number particles.wgsl's `born` reads for the kind.
    pub fn code(self) -> u32 {
        match self.kind {
            SpawnKind::Box => 0,
            SpawnKind::Sphere => 1,
            SpawnKind::Cylinder => 2,
            SpawnKind::Disc => 3,
        }
    }

    /// Where a particle is born in the centered 1×1×1 cube, from three
    /// random numbers from 0 up to 1, for a spawn transform that stretches
    /// the cube by `scale`: the same sum as particles.wgsl's `born`.
    pub fn point(self, random: [f32; 3], scale: Vec3) -> Vec3 {
        let [a, b, c] = random;
        let tau = std::f32::consts::TAU;
        match self.kind {
            SpawnKind::Box if self.surface => {
                // A face, by its area, then a point on it.
                let areas = [scale.y * scale.z, scale.x * scale.z, scale.x * scale.y];
                let total: f32 = areas.iter().map(|area| area.abs()).sum();
                if total.is_nan() || total <= 0.0 {
                    return Vec3::new(a, b, c) - 0.5;
                }
                let mut pick = a * total;
                let mut axis = 2;
                for (index, area) in areas.iter().enumerate() {
                    if pick < area.abs() {
                        axis = index;
                        break;
                    }
                    pick -= area.abs();
                }
                let side = if pick < areas[axis].abs() / 2.0 {
                    -0.5
                } else {
                    0.5
                };
                let (u, v) = (b - 0.5, c - 0.5);
                match axis {
                    0 => Vec3::new(side, u, v),
                    1 => Vec3::new(u, side, v),
                    _ => Vec3::new(u, v, side),
                }
            }
            SpawnKind::Box => Vec3::new(a, b, c) - 0.5,
            SpawnKind::Sphere => {
                let z = 1.0 - 2.0 * a;
                let across = (1.0 - z * z).max(0.0).sqrt();
                let around = b * tau;
                let radius = if self.surface { 0.5 } else { 0.5 * c.cbrt() };
                Vec3::new(across * around.cos(), z, across * around.sin()) * radius
            }
            SpawnKind::Cylinder | SpawnKind::Disc => {
                let around = a * tau;
                let radius = if self.surface { 0.5 } else { 0.5 * b.sqrt() };
                let y = if self.kind == SpawnKind::Disc {
                    0.0
                } else {
                    c - 0.5
                };
                Vec3::new(around.cos() * radius, y, around.sin() * radius)
            }
        }
    }
}

/// How a [`ParticleEmitter`]'s particles look over their lives: each pair
/// goes from the first value when born to the second when it dies.
#[derive(Clone, Debug)]
pub struct ParticleLook {
    /// Studs across.
    pub size: [f32; 2],
    /// sRGB colors that tint the texture.
    pub color: [[f32; 3]; 2],
    /// 0 is solid, 1 invisible.
    pub transparency: [f32; 2],
    /// The turn when born, in degrees, and how fast it turns, in degrees a
    /// second.
    pub rotation_degrees: f32,
    pub spin_degrees: f32,
    /// As [`Sprite::brightness`].
    pub brightness: f32,
    /// As [`Sprite::light_emission`].
    pub light_emission: f32,
    /// As [`Sprite::texture`].
    pub texture: Option<Arc<dyn TextureSource>>,
    /// Size, transparency and color over life (roadmap R17.1), in place of
    /// the pairs above; `None` blends the pairs.
    pub curves: Option<Arc<ParticleCurves>>,
    /// The texture's frames, shown one at a time (roadmap R17.2c); `None`
    /// shows the whole of it.
    pub flipbook: Option<Flipbook>,
}

/// How many evenly spaced times over a particle's life [`ParticleCurves`]
/// holds, from birth to death.
pub const CURVE_ENTRIES: usize = 32;

/// A particle's size, transparency and color over its life, sampled at
/// [`CURVE_ENTRIES`] times from birth (0) to death (1), with straight
/// lines between: the table particles.wgsl reads, and the CPU's particles
/// too ([`sample`](Self::sample)), so the two look alike.
#[derive(Clone, Debug, PartialEq)]
pub struct ParticleCurves {
    /// Size (studs), its envelope, transparency and its envelope. A
    /// particle's value is the entry's plus its envelope times a spread
    /// from −1 to 1 picked once for it.
    pub shape: [[f32; 4]; CURVE_ENTRIES],
    /// sRGB color.
    pub color: [[f32; 3]; CURVE_ENTRIES],
    /// What `Speed` is multiplied by as they move (roadmap R17.2's
    /// `SpeedOverLife`); 1 when it isn't set.
    pub speed: [f32; CURVE_ENTRIES],
}

impl ParticleCurves {
    /// The table of `at(time)`'s shape and color at each entry's time.
    pub fn from_fn(at: impl Fn(f32) -> ([f32; 4], [f32; 3])) -> Self {
        let mut curves = Self {
            shape: [[0.0; 4]; CURVE_ENTRIES],
            color: [[0.0; 3]; CURVE_ENTRIES],
            speed: [1.0; CURVE_ENTRIES],
        };
        for entry in 0..CURVE_ENTRIES {
            let (shape, color) = at(entry as f32 / (CURVE_ENTRIES - 1) as f32);
            curves.shape[entry] = shape;
            curves.color[entry] = color;
        }
        curves
    }

    /// The size, transparency and sRGB color at `time` (0 to 1) for a
    /// particle whose two spreads (from −1 to 1) are `spread`, as
    /// particles.wgsl's `write` works them out.
    pub fn sample(&self, time: f32, spread: [f32; 2]) -> (f32, f32, [f32; 3]) {
        let x = if time.is_nan() {
            0.0
        } else {
            time.clamp(0.0, 1.0)
        } * (CURVE_ENTRIES - 1) as f32;
        let index = (x.floor() as usize).min(CURVE_ENTRIES - 1);
        let next = (index + 1).min(CURVE_ENTRIES - 1);
        let along = x - index as f32;
        let mix = |a: f32, b: f32| a + (b - a) * along;
        let (a, b) = (self.shape[index], self.shape[next]);
        let shape = [0, 1, 2, 3].map(|i| mix(a[i], b[i]));
        let (a, b) = (self.color[index], self.color[next]);
        let color = [0, 1, 2].map(|i| mix(a[i], b[i]));
        (
            (shape[0] + shape[1] * spread[0]).max(0.0),
            (shape[2] + shape[3] * spread[1]).clamp(0.0, 1.0),
            color,
        )
    }

    /// The speed multiplier at `time` (0 to 1), as particles.wgsl's
    /// `simulate` works it out.
    pub fn speed_at(&self, time: f32) -> f32 {
        curve_at(&self.speed, time)
    }

    /// A particle's two spreads, from −1 to 1, from a number that stays
    /// the same over its life (`pcg`, as particles.wgsl's).
    pub fn spreads(seed: u32) -> [f32; 2] {
        let first = pcg(seed);
        let second = pcg(first);
        [first, second].map(|n| (n >> 8) as f32 / 16_777_216.0 * 2.0 - 1.0)
    }
}

/// `table`'s value at `time` (0 to 1), blending the two nearest entries.
pub fn curve_at(table: &[f32; CURVE_ENTRIES], time: f32) -> f32 {
    let x = if time.is_nan() {
        0.0
    } else {
        time.clamp(0.0, 1.0)
    } * (CURVE_ENTRIES - 1) as f32;
    let index = (x.floor() as usize).min(CURVE_ENTRIES - 1);
    let next = (index + 1).min(CURVE_ENTRIES - 1);
    table[index] + (table[next] - table[index]) * (x - index as f32)
}

/// The PCG hash particles.wgsl uses for its random numbers.
fn pcg(v: u32) -> u32 {
    let state = v.wrapping_mul(747_796_405).wrapping_add(2_891_336_453);
    let word = ((state >> ((state >> 28) + 4)) ^ state).wrapping_mul(277_803_737);
    (word >> 22) ^ word
}

impl Default for ParticleLook {
    fn default() -> Self {
        Self {
            size: [1.0; 2],
            color: [[1.0; 3]; 2],
            transparency: [0.0, 1.0],
            rotation_degrees: 0.0,
            spin_degrees: 0.0,
            brightness: 1.0,
            light_emission: 0.0,
            texture: None,
            curves: None,
            flipbook: None,
        }
    }
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

/// A light infinitely far away that shines in one direction, like the sun.
#[derive(Clone, Copy, Debug)]
pub struct Sun {
    /// The direction from the scene *toward* the sun. Doesn't need to be normalized.
    pub direction: Vec3,
    pub color: [f32; 3],
    pub intensity: f32,
}

/// Fog: what's far from the camera fades into the fog's own light. Two
/// kinds can be combined, and the default is neither:
///
/// - **Distance fog** (Lighting's `FogStart`, `FogEnd` and `FogColor`): none
///   nearer than `start` studs, all fog from `end` on, straight between.
/// - **Height fog** (an Atmosphere's): `density` of the light is lost per
///   stud at and below `height`, thinning by `e` every `falloff` studs above
///   it. Its sun scatters in it, brightest looking toward the sun, which is
///   what the graphics settings' volumetric fog shadows (sun shafts).
///
/// The sky is fogged as if it were [`Fog::SKY_DISTANCE`] studs away.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fog {
    /// The fog's own light (sRGB), or `None` for the sky's color around the
    /// horizon, so far items fade into the sky behind them.
    pub color: Option<[f32; 3]>,
    pub start: f32,
    /// Infinite for no distance fog.
    pub end: f32,
    /// How much light height fog takes away per stud at and below
    /// `height`: 0 for none.
    pub density: f32,
    pub height: f32,
    pub falloff: f32,
    /// How much of the sun's light the height fog scatters toward the
    /// camera (0 none, 1 all of it), and how much of that goes forward,
    /// away from the sun (0 the same every way, toward 1 a tight glow
    /// around it).
    pub sun_scattering: f32,
    pub forward_scattering: f32,
}

impl Fog {
    /// How far away the sky is taken to be, in studs: as far as the camera
    /// sees.
    pub const SKY_DISTANCE: f32 = 5000.0;

    /// Whether any fog is drawn.
    pub fn is_drawn(&self) -> bool {
        self.has_distance_fog() || self.has_height_fog()
    }

    pub fn has_distance_fog(&self) -> bool {
        self.end.is_finite() && self.start.is_finite()
    }

    pub fn has_height_fog(&self) -> bool {
        self.density.is_finite()
            && self.density > 0.0
            && self.height.is_finite()
            && self.falloff.is_finite()
    }
}

impl Default for Fog {
    /// No fog.
    fn default() -> Self {
        Self {
            color: Some([0.75; 3]),
            start: 0.0,
            end: f32::INFINITY,
            density: 0.0,
            height: 0.0,
            falloff: 50.0,
            sun_scattering: 0.0,
            forward_scattering: 0.0,
        }
    }
}

/// A layer of cloud (a `Clouds`), between [`Clouds::BOTTOM`] and
/// [`Clouds::TOP`] studs up, drawn in the sky and lit by the sun, which
/// it shades on the ground. The wind carries it along. The default is
/// none.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Clouds {
    /// How much of the sky is cloud, from 0 (none) to 1 (overcast).
    pub cover: f32,
    /// How thick the clouds are, from 0 (none) to 1: thicker clouds are
    /// whiter on top, darker underneath, and cast deeper shadows.
    pub density: f32,
    /// The clouds' own color (sRGB), which the sun and sky light.
    pub color: [f32; 3],
    /// Studs a second the clouds drift: a `Clouds`' own `Speed` toward its
    /// `Direction`, plus the Workspace's `GlobalWind`. Only its level part
    /// moves them.
    pub wind: Vec3,
}

impl Clouds {
    /// The cloud layer's bottom and top, in studs above the world's origin.
    pub const BOTTOM: f32 = 800.0;
    pub const TOP: f32 = 1200.0;

    /// Whether any cloud is drawn.
    pub fn is_drawn(&self) -> bool {
        self.cover.is_finite() && self.cover > 0.0 && self.density.is_finite() && self.density > 0.0
    }

    /// Whether the clouds drift, so a host should keep drawing frames.
    pub fn is_moving(&self) -> bool {
        self.is_drawn() && self.wind.is_finite() && (self.wind.x != 0.0 || self.wind.z != 0.0)
    }
}

impl Default for Clouds {
    /// No clouds.
    fn default() -> Self {
        Self {
            cover: 0.0,
            density: 0.7,
            color: [1.0; 3],
            wind: Vec3::ZERO,
        }
    }
}

/// Screen-space outlines: lines where the depth or the facing of what's
/// drawn changes sharply (around every silhouette and along creases), found
/// in the depth and normal prepass, so they need no setup per material
/// (`docs/rendering/stylized.md`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Edges {
    /// sRGB.
    pub color: [f32; 3],
    /// How thick the lines are, in pixels of a view 1080 pixels tall
    /// (scaled with the view's height, and never under one pixel).
    pub thickness: f32,
}

impl Default for Edges {
    fn default() -> Self {
        Self {
            color: [0.0; 3],
            thickness: 1.0,
        }
    }
}

/// The most colors a [`ColorGrading::palette`] holds; more are left out.
pub const MAX_PALETTE_COLORS: usize = 64;

/// Changes to the finished picture's colors, made after tone mapping on
/// sRGB-encoded values (as the screen shows them), in this order:
/// brightness, contrast, saturation, tint, the lookup table, posterizing
/// and the palette (`docs/rendering/post.md`). The default changes nothing.
#[derive(Clone, Debug, PartialEq)]
pub struct ColorGrading {
    /// Added to every channel: -1 is black, 1 is white.
    pub brightness: f32,
    /// Spreads (above 0) or squeezes (down to -1, flat grey) colors away
    /// from or toward mid-grey.
    pub contrast: f32,
    /// More (above 0) or less (down to -1, grey) colorful.
    pub saturation: f32,
    /// sRGB, multiplying every color. White changes nothing.
    pub tint: [f32; 3],
    /// A table every color is looked up in.
    pub lookup: Option<ColorLookup>,
    /// How many levels each channel is cut down to, 2 or more. 0 (or 1)
    /// leaves them smooth.
    pub posterize: u32,
    /// sRGB colors each pixel is snapped to the nearest of, up to
    /// [`MAX_PALETTE_COLORS`]. Empty leaves colors as they are.
    pub palette: Vec<[f32; 3]>,
}

impl Default for ColorGrading {
    fn default() -> Self {
        Self {
            brightness: 0.0,
            contrast: 0.0,
            saturation: 0.0,
            tint: [1.0; 3],
            lookup: None,
            posterize: 0,
            palette: Vec::new(),
        }
    }
}

/// Effects of a camera's lens on the picture, each `None` when the world
/// has none (`docs/rendering/post.md`). Each is drawn only while its
/// graphics setting allows it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LensEffects {
    pub depth_of_field: Option<DepthOfField>,
    pub vignette: Option<Vignette>,
    pub film_grain: Option<FilmGrain>,
    /// How far red and blue part at the screen's edges: 1 is 1% of the
    /// screen's width each way at the left and right edges.
    pub chromatic_aberration: Option<f32>,
    /// How much the edges between colors are sharpened: 0 none, 1 strong.
    pub sharpen: Option<f32>,
    /// How much of what moves across the view in a frame streaks behind
    /// it: 0 none, 1 the whole way it moved (a camera's shutter open the
    /// whole frame).
    pub motion_blur: Option<f32>,
    /// Shafts of light streaming from the sun past what's in front of it.
    pub sun_rays: Option<SunRays>,
}

/// Screen-space sun shafts (god rays): bright sky near the sun smeared
/// toward it, so gaps between leaves or a window's frame cast beams.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SunRays {
    /// How bright the shafts are, 0 to 1.
    pub intensity: f32,
    /// How far they reach from the sun, 0 to 1 (1 the whole picture).
    pub spread: f32,
}

/// Blur by distance from the camera: sharp from `focus_distance -
/// in_focus_radius` to `focus_distance + in_focus_radius` studs, blurring
/// toward the camera by `near_intensity` and away from it by
/// `far_intensity` (each 0 none, 1 full).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DepthOfField {
    pub focus_distance: f32,
    pub in_focus_radius: f32,
    pub near_intensity: f32,
    pub far_intensity: f32,
}

/// The picture darkened (or tinted) toward its corners.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vignette {
    /// How much of `color` the corners take, 0 to 1.
    pub intensity: f32,
    /// Where the darkening starts, as a share of the way from the middle to
    /// a corner (0 to 1).
    pub size: f32,
    /// sRGB.
    pub color: [f32; 3],
}

/// Moving noise over the picture, as on film.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FilmGrain {
    /// How strong the noise is, 0 to 1.
    pub intensity: f32,
    /// Each grain's size, in pixels of a view 1080 pixels tall (scaled with
    /// the view's height, never under one pixel).
    pub size: f32,
}

/// A color lookup table: a cube of `size` entries a side mapping each
/// sRGB-encoded color to another, read between entries (trilinear).
#[derive(Clone)]
pub struct ColorLookup {
    /// Different for every table, so the renderer uploads each one once.
    pub key: u64,
    /// Entries a side, 2 or more.
    pub size: u32,
    /// Red, green and blue for each entry, sRGB-encoded, red changing
    /// fastest, then green, then blue: `size³ × 3` values.
    pub rgb: Arc<Vec<f32>>,
}

impl fmt::Debug for ColorLookup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let size = self.size;
        write!(f, "ColorLookup #{} ({size}×{size}×{size})", self.key)
    }
}

impl PartialEq for ColorLookup {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}

/// How wet and snow-covered surfaces are (Lighting's `Weather`): parts,
/// models and terrain all read it. The default is dry, with no snow.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Weather {
    /// From 0 (dry) to 1 (soaked): wet surfaces are darker and glossier.
    /// Undersides stay drier.
    pub wetness: f32,
    /// From 0 (none) to 1: how much snow lies on surfaces facing up. More
    /// cover whitens flat tops fully and reaches further down slopes; walls
    /// stay bare.
    pub snow: f32,
}

impl Weather {
    /// Whether surfaces are dry and bare, so the shaders skip the weather.
    pub fn is_clear(&self) -> bool {
        !(self.wetness > 0.0 || self.snow > 0.0)
    }
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
#[derive(Clone, Debug, PartialEq)]
pub struct Sky {
    /// sRGB colors, like every color in the draw list.
    pub top_color: [f32; 3],
    pub horizon_color: [f32; 3],
    pub ground_color: [f32; 3],
    /// How strongly the sky lights and is reflected by surfaces. 0 leaves
    /// only the sun, the ambient light and point lights.
    pub environment_intensity: f32,
    /// A picture of the whole sky, drawn and reflected in place of the
    /// colors above while the reflection quality bakes environment maps.
    pub image: Option<SkyImage>,
    /// Air that scatters the sun's and moon's light, drawn and lit in
    /// place of the colors above (but not of an image).
    pub atmosphere: Option<Atmosphere>,
    /// How much of their light the colors above or an image give: 1 by
    /// day, less at night, so a sky picture darkens as the sun sets too
    /// (its colors stay the picture's). Not applied to an atmosphere, which
    /// works out its own.
    pub daylight: f32,
    /// How bright the stars are, from 0 (by day) to 1. Drawn over the
    /// colors or an atmosphere, never over an image.
    pub stars: f32,
}

impl Default for Sky {
    fn default() -> Self {
        Self {
            top_color: [0.25, 0.5, 0.9],
            horizon_color: [0.63, 0.8, 0.95],
            ground_color: [0.35, 0.33, 0.3],
            environment_intensity: 0.6,
            image: None,
            atmosphere: None,
            daylight: 1.0,
            stars: 0.0,
        }
    }
}

/// A high dynamic range picture of the whole sky, wrapped around the world
/// (equirectangular): left to right goes once around, with its middle
/// toward -Z and its edges toward +Z, and top to bottom from straight up to
/// straight down.
#[derive(Clone)]
pub struct SkyImage {
    /// Different for every image, so the renderer bakes each one once.
    pub key: u64,
    pub width: u32,
    pub height: u32,
    /// Linear light, which can be brighter than 1: red, green and blue for
    /// each pixel, rows from the top.
    pub rgb: Arc<Vec<f32>>,
}

impl fmt::Debug for SkyImage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SkyImage #{} ({}×{})", self.key, self.width, self.height)
    }
}

impl PartialEq for SkyImage {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
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
    /// `Some` for a rect light: it shines from a rectangle centered on
    /// `position`, out of its front only. Ignored with a `cone`.
    pub rect: Option<RectArea>,
    /// `Some` if a point light (or a rect light) casts shadows (a
    /// PointLight's `Shadows`), from `position`. Only the nearest few to
    /// the camera do, as the graphics settings allow. A spot light's
    /// shadows are its cone's.
    pub point_shadows: Option<PointShadows>,
}

/// The rectangle a rect light shines from, like a window or a softbox.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RectArea {
    /// The way its front faces. Doesn't need to be normalized.
    pub direction: Vec3,
    /// Along its width, square to `direction`. Doesn't need to be
    /// normalized.
    pub right: Vec3,
    /// Its width (along `right`) and height, in studs.
    pub width: f32,
    pub height: f32,
}

/// How a point light casts shadows, on every side of it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointShadows {
    /// Studs from the light before anything casts a shadow, along each
    /// world axis: +X, -X, +Y, -Y, +Z, -Z. So the part the light sits on
    /// doesn't shade everything on its side.
    pub near: [f32; 6],
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

/// A group of nearby items kept between frames. A group no view reaches
/// is passed over whole, so a frame's cost follows what's near what it
/// sees and not everything painted. Never holds water.
#[derive(Clone, Debug)]
pub struct ItemChunk {
    /// The middle of a ball holding every item, as far as each can sway.
    pub center: Vec3,
    pub radius: f32,
    /// The farthest [`DrawItem::cull_distance`] of its items, or 0 if one
    /// of them is drawn at any distance.
    pub cull_distance: f32,
    pub items: Arc<[DrawItem]>,
}

impl ItemChunk {
    /// A group of `items`, its ball and reach worked out from them.
    pub fn of(items: Vec<DrawItem>) -> Self {
        let mut low = Vec3::splat(f32::INFINITY);
        let mut high = Vec3::splat(f32::NEG_INFINITY);
        let mut widest = 0.0f32;
        for item in &items {
            let (center, radius) = item.resting_ball();
            low = low.min(center);
            high = high.max(center);
            widest = widest.max(radius + item.sway.map_or(0.0, |sway| sway.reach()));
        }
        let unlimited = items.iter().any(|item| item.cull_distance <= 0.0);
        Self {
            center: (low + high) * 0.5,
            radius: (high - low).length() * 0.5 + widest,
            cull_distance: if unlimited {
                0.0
            } else {
                items
                    .iter()
                    .map(|item| item.cull_distance)
                    .fold(0.0, f32::max)
            },
            items: items.into(),
        }
    }
}

impl DrawItem {
    /// The ball its shape fills where it rests: every shape fits the
    /// 1×1×1 cube its transform scales.
    pub fn resting_ball(&self) -> (Vec3, f32) {
        let axes = [
            self.transform.x_axis.truncate(),
            self.transform.y_axis.truncate(),
            self.transform.z_axis.truncate(),
        ];
        let corner = axes[0].length_squared() + axes[1].length_squared() + axes[2].length_squared();
        (self.transform.w_axis.truncate(), corner.sqrt() * 0.5)
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
    /// The same every frame for the same thing drawn, and different for
    /// every other (`scene` hashes the Instance's ID), so a mesh with
    /// levels of detail ([`MeshSource::lod`]) keeps its level and
    /// cross-fades when it switches. 0 is none: the level is picked afresh
    /// each frame, with no fade.
    pub id: u64,
    /// Studs from the camera past which the item isn't drawn, fading out
    /// over the last tenth (painted foliage). 0 draws it at any distance.
    pub cull_distance: f32,
    /// Where an animated mesh's joints are this frame: the GPU moves each
    /// corner by the joints it's weighted to ([`MeshSource::skin_joints`]).
    /// `None` draws the mesh as it is. Items posed alike share one `Arc`, so
    /// its joints are uploaded once.
    pub skin: Option<Arc<Skin>>,
    /// How the item sways in [`DrawList::wind`] (plants), or `None` to keep
    /// it still. Its shadow sways with it.
    pub sway: Option<Sway>,
    /// How it meets decals: a surface they land on, a character's, or a
    /// decal itself (see [`crate::DecalRole`]).
    pub decal_role: crate::DecalRole,
}

/// How a plant bends in the wind. Its corners move by their height above
/// `base`: not at all at the ground, most at `base + height`. A plant drawn
/// as several items gives each the same values, so its pieces bend as one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sway {
    /// The world height of the ground the plant stands on, in studs.
    pub base: f32,
    /// How tall the plant is, in studs.
    pub height: f32,
    /// How far it gives: 1 is grass, which leans about a quarter of its
    /// height in a strong wind. 0 keeps it still.
    pub strength: f32,
    /// How far each corner shivers on top of the bend, in studs (leaves on
    /// a branch), from 0 up to just under 1. Needs a `strength` above 0.
    pub flutter: f32,
}

impl Sway {
    /// The farthest a corner can move from where it rests, in studs, in a
    /// wind of any speed (see `swayed` in shader.wgsl).
    pub fn reach(&self) -> f32 {
        if self.is_still() {
            0.0
        } else {
            self.height * self.strength * SWAY_REACH + self.flutter.max(0.0) * FLUTTER_REACH
        }
    }

    /// Whether the item doesn't move at all.
    pub fn is_still(&self) -> bool {
        !(self.strength > 0.0 && self.height > 0.0 && self.base.is_finite())
    }
}

/// The most a plant of strength 1 leans, as a share of its height, with
/// its gusts and flutter at their strongest. Matches `swayed` in
/// shader.wgsl: its lean (at most `SWAY_LEAN`) times its gusts and flutter
/// (at most 1.2 + 0.15).
const SWAY_REACH: f32 = 0.25 * 1.35;
/// The farthest a corner shivers per stud of [`Sway::flutter`]. Matches
/// `swayed` in shader.wgsl: two waves of at most 1 on each level axis and
/// 0.6 of one upward, about 1.2 together.
const FLUTTER_REACH: f32 = 1.3;

/// One pose of an animated mesh, for skinning on the GPU: where each joint
/// moves the corners weighted to it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Skin {
    joints: Vec<Mat4>,
    /// The matching turn for normals (each joint's inverse transpose), so
    /// they stay perpendicular to stretched surfaces.
    normals: Vec<Mat3>,
}

impl Skin {
    /// `joints` are from a corner's place in the mesh to its posed place,
    /// both in the mesh's own space (before the item's transform), in the
    /// order [`MeshSource::skin_joints`] indexes them.
    pub fn new(joints: Vec<Mat4>) -> Self {
        let normals = joints
            .iter()
            .map(|joint| Mat3::from_mat4(*joint).inverse().transpose())
            .collect();
        Self { joints, normals }
    }

    /// Changes this to another pose, reusing its buffers: `normals` are
    /// `joints`' inverse transposes, already worked out by the caller (an
    /// animated model's pose keeps them). A host keeps one `Skin` per
    /// animated mesh and calls this each frame instead of [`Skin::new`].
    pub fn set_pose(&mut self, joints: &[Mat4], normals: &[Mat3]) {
        debug_assert_eq!(joints.len(), normals.len());
        self.joints.clear();
        self.joints.extend_from_slice(joints);
        self.normals.clear();
        self.normals.extend_from_slice(normals);
    }

    pub fn joints(&self) -> &[Mat4] {
        &self.joints
    }

    pub(crate) fn normals(&self) -> &[Mat3] {
        &self.normals
    }
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
    /// A corner wedge: full bottom face, rising to a point at the top back
    /// right corner (+X, +Y, +Z).
    CornerWedge,
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
    /// For an animated mesh, one per position: up to 4 joints of the
    /// item's [`Skin`] the corner moves with, and in [`skin_weights`] how
    /// much each counts (all 0 for a corner that never moves). Empty for a
    /// mesh that isn't animated, which is the default.
    ///
    /// [`skin_weights`]: MeshSource::skin_weights
    fn skin_joints(&self) -> &[[u16; 4]] {
        &[]
    }
    fn skin_weights(&self) -> &[[f32; 4]] {
        &[]
    }
    /// Changes when the corners do (an animated, posed mesh), with the same
    /// key and indices: the renderer then rewrites its copy's corners in
    /// place. A mesh that never changes keeps the default.
    fn version(&self) -> u64 {
        0
    }
    /// How many simpler copies of this mesh there are ([`MeshSource::lod`]).
    /// A mesh with none keeps the default.
    fn lod_count(&self) -> usize {
        0
    }
    /// Simpler copy `level` of this mesh (1 is the first), with fewer
    /// triangles than the level before, drawn in its place when it's small
    /// on screen: the renderer asks only for the level it draws. Each copy
    /// has its own key, fits the same box, and has no copies of its own.
    /// Asked for once per draw, so keep the copies rather than making one
    /// each time.
    fn lod(&self, level: usize) -> Option<&Arc<dyn MeshSource>> {
        let _ = level;
        None
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
    /// Its mip levels made ahead in BC7 blocks (a cooked texture), largest
    /// first (at [`size`](Self::size)), each half the last, down to 1×1.
    /// Surface textures are uploaded from these when the GPU reads BC7,
    /// with no mips to make and a quarter of the bytes; otherwise, and
    /// when this is empty (the default), from [`rgba`](Self::rgba), which
    /// must still work.
    fn bc7_levels(&self) -> &[Vec<u8>] {
        &[]
    }
    /// A normal map's mip levels made ahead in BC5 blocks, red and green
    /// only, like [`bc7_levels`](Self::bc7_levels) otherwise. Blue is
    /// worked out from them when drawn (`MapsUniform::specular`'s y), so
    /// give these only for a texture used as a [`NormalMap`], with a key of
    /// its own. Uploaded like BC7 levels, which come first if both are
    /// given.
    fn bc5_levels(&self) -> &[Vec<u8>] {
        &[]
    }
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
    /// A WGSL `surface` function that works out the item's surface in
    /// place of the standard one (a material file's shader). One with an
    /// error draws in glowing magenta checks, and is reported once
    /// ([`crate::TextureRenderer::take_shader_errors`]).
    pub surface_shader: Option<crate::SurfaceShader>,
    /// Values its surface shader reads with `parameter(i)` (a material
    /// file's `parameters`, in name order), so materials that differ only
    /// in them share the shader's pipelines. Unused ones are 0.
    pub surface_parameters: [[f32; 4]; SURFACE_PARAMETERS],
    /// Draws the item as water (a material file's `water`): waves, what's
    /// under it seen through it, reflections and foam, in place of the
    /// standard surface and any surface shader. Its `base_color` tints
    /// what's seen through it. Draw it on [`crate::water_surface`] so the
    /// waves have corners to move.
    pub water: Option<crate::Water>,
    /// How it's lit. Water and terrain ignore it.
    pub shading: Shading,
    /// How much light from behind passes through it, from 0 (none) to 1,
    /// for thin things like leaves. Only the standard shading has it.
    pub translucency: f32,
    /// A line around its silhouette (an inverted hull), for solid items.
    /// Water and terrain ignore it.
    pub outline: Option<Outline>,
}

/// A material's outline: its item is drawn again behind itself, back faces
/// only, pushed out along its normals by the same number of pixels however
/// far away it is (`docs/rendering/stylized.md`). Smooth meshes (spheres,
/// characters) get an unbroken line; a hard-edged one opens at its corners.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Outline {
    /// In pixels of a view 1080 pixels tall, scaled with the view's height
    /// (and never under one pixel). 0 draws none.
    pub width: f32,
    /// sRGB, drawn as it is (unlit, but fogged).
    pub color: [f32; 3],
}

impl Default for Outline {
    fn default() -> Self {
        Self {
            width: 2.0,
            color: [0.0; 3],
        }
    }
}

/// How a [`Material`] is lit. Every model takes the same lights, shadows,
/// fog and post-processing, in the same pass
/// (`docs/rendering/stylized.md`).
#[derive(Clone, Debug, Default)]
pub enum Shading {
    /// Physically based (the default).
    #[default]
    Standard,
    /// The surface's color (and glow) as it is, whatever the light.
    Unlit,
    /// Cel shading: see [`Toon`].
    Toon(Toon),
}

/// Toon shading: each light's share (how squarely it falls, times its
/// shadow) is cut into flat bands, with a rim light around the outline and
/// a sharp highlight. Metalness and roughness are left out.
#[derive(Clone, Debug)]
pub struct Toon {
    /// How many bands from dark to fully lit, at least 1 (two-tone).
    pub steps: u32,
    /// An image whose colors, left to right along its middle row, are the
    /// light from none to full, in place of `steps`. sRGB, read without
    /// blending its pixels. It takes the specular map's place, so a toon
    /// material has no specular map.
    pub ramp: Option<Arc<dyn TextureSource>>,
    /// 0 is a hard edge between bands (the shadow's too), 1 a smooth blend.
    pub softness: f32,
    /// How bright the rim light is, 0 for none.
    pub rim: f32,
    /// How big the highlight is, 0 for none, 1 a wide one.
    pub highlight: f32,
}

impl Default for Toon {
    fn default() -> Self {
        Self {
            steps: 3,
            ramp: None,
            softness: 0.05,
            rim: 0.3,
            highlight: 0.2,
        }
    }
}

/// How many values a surface shader can read with `parameter(i)`
/// ([`Material::surface_parameters`]).
pub const SURFACE_PARAMETERS: usize = 8;

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
            surface_shader: None,
            surface_parameters: [[0.0; 4]; SURFACE_PARAMETERS],
            water: None,
            shading: Shading::Standard,
            translucency: 0.0,
            outline: None,
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
