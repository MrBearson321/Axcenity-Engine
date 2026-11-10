//! Brushes (Phase 59): what a stroke's dabs do to a layer or its mask.
//!
//! A dab in the 3D view covers the texels within its radius on the surface
//! ([`TexelMap::touches`]), weighted by the falloff and by how much each
//! faces the way the surface under the brush does, so it doesn't paint
//! through to the far side of something thin. A dab in the UV editor
//! covers the texels within its radius in uv space ([`uv_touches`]).
//! [`stamp`] then paints the touched texels with the brush's tool.

use std::sync::Arc;

use axcenity_assets::Image;
use glam::{Vec2, Vec3};

use super::{MapKind, PaintMap, TexelMap, sample, texel_uv, to_byte};

/// What a brush does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PaintTool {
    #[default]
    Draw,
    /// Blurs what's painted.
    Soften,
    /// Drags what's painted along with the stroke.
    Smear,
    /// Paints a copy of the map from the clone source.
    Clone,
    /// Paints the whole part clicked (or every texel, in the UV editor).
    Fill,
    Erase,
}

impl PaintTool {
    pub const ALL: [PaintTool; 6] = [
        PaintTool::Draw,
        PaintTool::Soften,
        PaintTool::Smear,
        PaintTool::Clone,
        PaintTool::Fill,
        PaintTool::Erase,
    ];

    pub fn label(self) -> &'static str {
        match self {
            PaintTool::Draw => "Draw",
            PaintTool::Soften => "Soften",
            PaintTool::Smear => "Smear",
            PaintTool::Clone => "Clone",
            PaintTool::Fill => "Fill",
            PaintTool::Erase => "Erase",
        }
    }
}

/// How a dab's strength fades from its middle to its edge.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BrushFalloff {
    #[default]
    Smooth,
    Sphere,
    Root,
    Sharp,
    Linear,
    Constant,
}

impl BrushFalloff {
    pub const ALL: [BrushFalloff; 6] = [
        BrushFalloff::Smooth,
        BrushFalloff::Sphere,
        BrushFalloff::Root,
        BrushFalloff::Sharp,
        BrushFalloff::Linear,
        BrushFalloff::Constant,
    ];

    pub fn label(self) -> &'static str {
        match self {
            BrushFalloff::Smooth => "Smooth",
            BrushFalloff::Sphere => "Sphere",
            BrushFalloff::Root => "Root",
            BrushFalloff::Sharp => "Sharp",
            BrushFalloff::Linear => "Linear",
            BrushFalloff::Constant => "Constant",
        }
    }

    /// The strength at `t` of the way out (0 the middle, 1 the edge).
    pub fn weight(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        let s = 1.0 - t;
        match self {
            BrushFalloff::Smooth => s * s * (3.0 - 2.0 * s),
            BrushFalloff::Sphere => (1.0 - t * t).sqrt(),
            BrushFalloff::Root => s.sqrt(),
            BrushFalloff::Sharp => s * s,
            BrushFalloff::Linear => s,
            BrushFalloff::Constant => 1.0,
        }
    }
}

/// How a brush's image is laid on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ImageMode {
    /// Fixed on the view: the brush paints it through where it passes.
    #[default]
    Stencil,
    /// Moves with each dab, filling its circle.
    Texture,
}

/// An image a brush paints with in place of its color.
#[derive(Clone, Debug)]
pub struct BrushImage {
    /// Its project path, to show.
    pub path: String,
    pub image: Arc<Image>,
    pub mode: ImageMode,
    /// A stencil's middle on the view, and how many points wide it is.
    pub center: Vec2,
    pub width: f32,
}

/// The brush's settings.
#[derive(Clone, Debug)]
pub struct Brush {
    pub tool: PaintTool,
    /// The color for color maps (sRGB).
    pub color: [u8; 3],
    /// The value for the other maps, 0 to 1.
    pub value: f32,
    /// In points on the view.
    pub radius: f32,
    /// 0 to 1.
    pub strength: f32,
    pub falloff: BrushFalloff,
    /// How far apart dabs go along a stroke, as a share of the radius.
    pub spacing: f32,
    /// Also paint mirrored across the object's X, Y and Z planes.
    pub symmetry: [bool; 3],
    /// Pen pressure changes the radius, and the strength.
    pub pressure_radius: bool,
    pub pressure_strength: bool,
    pub image: Option<BrushImage>,
}

impl Default for Brush {
    fn default() -> Self {
        Self {
            tool: PaintTool::Draw,
            color: [150, 70, 30],
            value: 0.9,
            radius: 30.0,
            strength: 0.8,
            falloff: BrushFalloff::Smooth,
            spacing: 0.15,
            symmetry: [false; 3],
            pressure_radius: false,
            pressure_strength: true,
            image: None,
        }
    }
}

/// One stamp of the brush on the surface.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dab {
    pub center: Vec3,
    /// The surface's normal under it.
    pub normal: Vec3,
    /// In the model's units.
    pub radius: f32,
    /// 0 to 1, with pen pressure.
    pub strength: f32,
    /// How far the brush moved since the last dab (Smear drags by it).
    pub motion: Vec3,
    /// The axes this copy is mirrored across (symmetry's copies).
    pub mirror: [bool; 3],
}

impl Dab {
    /// This dab and its mirror images across the planes `symmetry` turns
    /// on (through the object's origin).
    pub fn with_symmetry(self, symmetry: [bool; 3]) -> Vec<Dab> {
        let mut dabs = vec![self];
        for axis in 0..3 {
            if !symmetry[axis] {
                continue;
            }
            let copies: Vec<Dab> = dabs
                .iter()
                .map(|dab| {
                    let mut mirror = dab.mirror;
                    mirror[axis] = !mirror[axis];
                    let flip = |v: Vec3| {
                        let mut v = v;
                        v[axis] = -v[axis];
                        v
                    };
                    Dab {
                        center: flip(dab.center),
                        normal: flip(dab.normal),
                        motion: flip(dab.motion),
                        mirror,
                        ..*dab
                    }
                })
                .collect();
            dabs.extend(copies);
        }
        dabs
    }

    /// `point` mirrored back to the side of the dab it's a copy of.
    fn unmirror(&self, point: Vec3) -> Vec3 {
        let mut point = point;
        for axis in 0..3 {
            if self.mirror[axis] {
                point[axis] = -point[axis];
            }
        }
        point
    }
}

/// A texel a dab paints, and how much.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Touch {
    pub texel: u32,
    /// 0 to 1: falloff, facing and strength together.
    pub weight: f32,
    /// Where the texel shows on the view (on the dab's own side, for a
    /// mirrored copy), for image brushes.
    pub at: Vec2,
    /// How far a Smear drags it, in uvs.
    pub smear: Vec2,
}

impl TexelMap {
    /// The texels `dab` covers: within its radius on the surface, weighted
    /// by `falloff`, by the dab's strength, and by how far each faces the
    /// way the surface under the dab does (none facing away). `project`
    /// places a point on the view.
    pub fn touches(
        &self,
        dab: &Dab,
        falloff: BrushFalloff,
        project: &dyn Fn(Vec3) -> Option<Vec2>,
    ) -> Vec<Touch> {
        let radius = dab.radius.max(1e-6);
        self.near(dab.center, radius)
            .into_iter()
            .filter_map(|(texel, place, normal, triangle)| {
                let facing = normal.dot(dab.normal);
                if facing <= 0.0 {
                    return None;
                }
                // Full strength up to 45° off, none side-on.
                let facing = (facing / std::f32::consts::FRAC_1_SQRT_2).min(1.0);
                let weight =
                    falloff.weight(place.distance(dab.center) / radius) * facing * dab.strength;
                (weight > 0.0).then(|| Touch {
                    texel,
                    weight,
                    at: project(dab.unmirror(place)).unwrap_or(Vec2::NAN),
                    smear: self.uv_motion(triangle, dab.motion),
                })
            })
            .collect()
    }
}

/// The texels of a map `size` across within `radius` (in uvs) of `center`
/// (a uv), as the UV editor paints: weighted by `falloff` and `strength`.
/// `to_view` places a uv on the view; `motion` is the stroke's move since
/// the last dab, in uvs.
pub fn uv_touches(
    size: u32,
    center: Vec2,
    radius: f32,
    falloff: BrushFalloff,
    strength: f32,
    motion: Vec2,
    to_view: &dyn Fn(Vec2) -> Vec2,
) -> Vec<Touch> {
    let scale = size as f32;
    let radius = radius.max(1e-6);
    let (min, max) = ((center - radius) * scale, (center + radius) * scale);
    let mut touches = Vec::new();
    for y in
        (min.y - 0.5).floor().max(0.0) as u32..=((max.y - 0.5).ceil().max(0.0) as u32).min(size - 1)
    {
        for x in (min.x - 0.5).floor().max(0.0) as u32
            ..=((max.x - 0.5).ceil().max(0.0) as u32).min(size - 1)
        {
            let texel = y * size + x;
            let uv = texel_uv(texel, size);
            let distance = uv.distance(center);
            if distance > radius {
                continue;
            }
            let weight = falloff.weight(distance / radius) * strength;
            if weight > 0.0 {
                touches.push(Touch {
                    texel,
                    weight,
                    at: to_view(uv),
                    smear: motion,
                });
            }
        }
    }
    touches
}

/// What a brush paints on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Target {
    #[default]
    Layer,
    /// The active layer's mask: Draw shows the layer, Erase hides it.
    Mask,
}

/// What a stamp needs to know of the stroke.
#[derive(Clone, Copy, Debug)]
pub struct StampView<'a> {
    /// The dab's middle on the view and its radius in points (where a
    /// texture brush's image goes).
    pub center: Vec2,
    pub radius: f32,
    /// A Clone's source: the flattened map as the stroke began, and the
    /// uv shift from each texel to where it copies from.
    pub clone: Option<(&'a [u8], Vec2)>,
}

/// Paints `touches` on `map`'s active layer, or its mask, with `brush`,
/// and flattens the map again there. Returns false if there was nothing to
/// paint (the Clone brush without a source).
pub fn stamp(
    map: &mut PaintMap,
    target: Target,
    touches: &[Touch],
    brush: &Brush,
    view: &StampView,
) -> bool {
    if touches.is_empty() || map.layers.is_empty() {
        return true;
    }
    if brush.tool == PaintTool::Clone && view.clone.is_none() {
        return false;
    }
    let size = map.size;
    let kind = map.kind;
    let layer = map.active_layer_mut();
    if target == Target::Mask && layer.mask.is_none() {
        layer.mask = Some(Arc::new(vec![255; super::texel_count(size)]));
    }
    // Each touched texel's new value, worked out before any is written,
    // since Soften and Smear read their neighbours.
    let canvas = Canvas {
        layer: &*layer,
        target,
        size,
    };
    let changed: Vec<(usize, [f32; 4])> = touches
        .iter()
        .filter(|touch| (touch.texel as usize) < super::texel_count(size))
        .map(|touch| {
            let k = touch.texel as usize;
            let now = canvas.get(k);
            let w = touch.weight.clamp(0.0, 1.0);
            let uv = texel_uv(touch.texel, size);
            let new = match brush.tool {
                PaintTool::Draw | PaintTool::Fill => {
                    let (color, cover) = brush_color(brush, kind, target, touch, view);
                    lerp(now, [color[0], color[1], color[2], 1.0], w * cover)
                }
                PaintTool::Erase => now.map(|c| c * (1.0 - w)),
                PaintTool::Soften => lerp(now, canvas.blurred(touch.texel), w),
                PaintTool::Smear => lerp(now, canvas.at(uv - touch.smear), w),
                PaintTool::Clone => {
                    let (source, shift) = view.clone.expect("checked above");
                    let s = sample(source, size, size, uv + shift, true).map(|c| c / 255.0);
                    let copy = match target {
                        Target::Mask => {
                            let grey = luminance([s[0], s[1], s[2]]);
                            [grey, grey, grey, 1.0]
                        }
                        Target::Layer => [s[0], s[1], s[2], 1.0],
                    };
                    lerp(now, copy, w)
                }
            };
            (k, new)
        })
        .collect();
    match target {
        Target::Layer => {
            let pixels = Arc::make_mut(&mut layer.pixels);
            for &(k, [r, g, b, a]) in &changed {
                let (r, g, b) = if a > 1e-6 {
                    (r / a, g / a, b / a)
                } else {
                    (0.0, 0.0, 0.0)
                };
                pixels[k * 4..k * 4 + 4].copy_from_slice(&[r, g, b, a].map(to_byte));
            }
        }
        Target::Mask => {
            let mask = Arc::make_mut(layer.mask.as_mut().expect("made above"));
            for &(k, value) in &changed {
                mask[k] = to_byte(value[0]);
            }
        }
    }
    let texels: Vec<u32> = changed.iter().map(|&(k, _)| k as u32).collect();
    map.refresh(&texels);
    true
}

/// Reading the layer (or mask) being painted: premultiplied red, green,
/// blue and coverage, 0 to 1 (a mask is its value as grey, covering).
struct Canvas<'a> {
    layer: &'a super::Layer,
    target: Target,
    size: u32,
}

impl Canvas<'_> {
    fn get(&self, k: usize) -> [f32; 4] {
        match self.target {
            Target::Layer => {
                let p = &self.layer.pixels[k * 4..k * 4 + 4];
                let a = f32::from(p[3]) / 255.0;
                [
                    f32::from(p[0]) / 255.0 * a,
                    f32::from(p[1]) / 255.0 * a,
                    f32::from(p[2]) / 255.0 * a,
                    a,
                ]
            }
            Target::Mask => {
                let m = self
                    .layer
                    .mask
                    .as_ref()
                    .map_or(1.0, |mask| f32::from(mask[k]) / 255.0);
                [m, m, m, 1.0]
            }
        }
    }

    /// The average of the texel and the eight round it (wrapping).
    fn blurred(&self, texel: u32) -> [f32; 4] {
        let size = i64::from(self.size);
        let (x, y) = (i64::from(texel) % size, i64::from(texel) / size);
        let mut sum = [0.0; 4];
        for dy in -1..=1 {
            for dx in -1..=1 {
                let k = ((y + dy).rem_euclid(size) * size + (x + dx).rem_euclid(size)) as usize;
                let value = self.get(k);
                for c in 0..4 {
                    sum[c] += value[c] / 9.0;
                }
            }
        }
        sum
    }

    /// The canvas at `uv`, between texel middles (wrapping).
    fn at(&self, uv: Vec2) -> [f32; 4] {
        let scale = self.size as f32;
        let x = uv.x * scale - 0.5;
        let y = uv.y * scale - 0.5;
        let (x0, y0) = (x.floor(), y.floor());
        let (fx, fy) = (x - x0, y - y0);
        let size = i64::from(self.size);
        let at =
            |x: i64, y: i64| self.get((y.rem_euclid(size) * size + x.rem_euclid(size)) as usize);
        let (x0, y0) = (x0 as i64, y0 as i64);
        let [a, b, c, d] = [
            at(x0, y0),
            at(x0 + 1, y0),
            at(x0, y0 + 1),
            at(x0 + 1, y0 + 1),
        ];
        std::array::from_fn(|i| {
            let top = a[i] + (b[i] - a[i]) * fx;
            let bottom = c[i] + (d[i] - c[i]) * fx;
            top + (bottom - top) * fy
        })
    }
}

/// What Draw and Fill paint at `touch`: a color (0 to 1) and how much of
/// it (the brush image's alpha, or none outside a stencil).
fn brush_color(
    brush: &Brush,
    kind: MapKind,
    target: Target,
    touch: &Touch,
    view: &StampView,
) -> ([f32; 3], f32) {
    let plain = match target {
        Target::Mask => [1.0; 3],
        Target::Layer if kind.is_color() => brush.color.map(|c| f32::from(c) / 255.0),
        Target::Layer => [brush.value.clamp(0.0, 1.0); 3],
    };
    let Some(image) = &brush.image else {
        return (plain, 1.0);
    };
    if !touch.at.is_finite() {
        return (plain, 0.0);
    }
    let (width, height) = (image.image.width as f32, image.image.height as f32);
    let uv = match image.mode {
        ImageMode::Stencil => {
            let across = image.width.max(1.0);
            let down = across * height / width.max(1.0);
            let uv = (touch.at - image.center) / Vec2::new(across, down) + 0.5;
            if uv.cmplt(Vec2::ZERO).any() || uv.cmpgt(Vec2::ONE).any() {
                return (plain, 0.0);
            }
            uv
        }
        ImageMode::Texture => (touch.at - view.center) / (view.radius.max(1e-3) * 2.0) + 0.5,
    };
    let wrap = image.mode == ImageMode::Texture;
    let s = sample(
        image.image.rgba(),
        image.image.width,
        image.image.height,
        uv,
        wrap,
    )
    .map(|c| c / 255.0);
    let color = [s[0], s[1], s[2]];
    let color = if target == Target::Mask || !kind.is_color() {
        [luminance(color); 3]
    } else {
        color
    };
    (color, s[3])
}

fn luminance([r, g, b]: [f32; 3]) -> f32 {
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

fn lerp(from: [f32; 4], to: [f32; 4], t: f32) -> [f32; 4] {
    std::array::from_fn(|i| from[i] + (to[i] - from[i]) * t)
}
