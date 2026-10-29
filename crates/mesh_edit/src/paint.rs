//! Texture painting (Phase 59): a material's maps painted in layers on an
//! object's uvs, and flattened into PNGs when the model is saved.
//!
//! - A [`PaintSet`] is one material slot of one object being painted: its
//!   resolution and a [`PaintMap`] for each map painted (base color,
//!   roughness, metalness, height, emissive, ambient occlusion).
//! - A map is its material's own value ([`PaintMap::fill`]) under a stack
//!   of [`Layer`]s, each with a blend mode, opacity and maybe a mask. The
//!   map keeps its layers flattened as they're painted, so the Studio draws
//!   it at once.
//! - Brushes (`paint/brush.rs`) stamp onto a layer or its mask: from the
//!   3D view through a [`TexelMap`] (`paint/texels.rs`), which knows where
//!   on the surface each texel is, so a stroke paints both sides of a uv
//!   seam alike; or in uv space from the UV editor.
//! - Save ([`ModelDoc::bake_paint`]) writes each map as a PNG named by its
//!   role (`Chair_Seat_Wood_BaseColor.png`; roughness and metalness
//!   together as glTF's `_MetallicRoughness.png`, and height as the
//!   `_Normal.png` it makes) and points the material at them. The layers
//!   go in the sidecar folder as PNGs ([`ModelDoc::paint_files`]).
//!
//! Color maps hold sRGB values, as their PNGs do; the others plain values.

mod brush;
pub(crate) mod texels;

pub use brush::{
    Brush, BrushFalloff, BrushImage, Dab, ImageMode, PaintTool, StampView, Target, Touch, stamp,
    uv_touches,
};
pub use texels::TexelMap;

use std::collections::BTreeSet;
use std::io::Cursor;
use std::sync::Arc;

use axcenity_assets::{Channel, ChannelTexture, Image, ModelMaterial, TEXTURES_FOLDER};
use glam::Vec2;
use serde::{Deserialize, Serialize};

use crate::model_doc::ModelDoc;

/// The resolutions a set can be painted at.
pub const SIZES: [u32; 5] = [256, 512, 1024, 2048, 4096];

/// A material map that can be painted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum MapKind {
    BaseColor,
    Roughness,
    Metalness,
    /// Saved as the normal map it makes.
    Height,
    Emissive,
    /// Ambient occlusion.
    Occlusion,
}

impl MapKind {
    pub const ALL: [MapKind; 6] = [
        MapKind::BaseColor,
        MapKind::Roughness,
        MapKind::Metalness,
        MapKind::Height,
        MapKind::Emissive,
        MapKind::Occlusion,
    ];

    pub fn label(self) -> &'static str {
        match self {
            MapKind::BaseColor => "Base Color",
            MapKind::Roughness => "Roughness",
            MapKind::Metalness => "Metalness",
            MapKind::Height => "Height",
            MapKind::Emissive => "Emissive",
            MapKind::Occlusion => "Ambient Occlusion",
        }
    }

    /// The word its files are named with, which `assets` reads as its
    /// role.
    pub fn word(self) -> &'static str {
        match self {
            MapKind::BaseColor => "BaseColor",
            MapKind::Roughness => "Roughness",
            MapKind::Metalness => "Metalness",
            MapKind::Height => "Height",
            MapKind::Emissive => "Emissive",
            MapKind::Occlusion => "Occlusion",
        }
    }

    /// True for maps of colors; the others are one value per texel.
    pub fn is_color(self) -> bool {
        matches!(self, MapKind::BaseColor | MapKind::Emissive)
    }
}

/// How a layer mixes with what's under it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlendMode {
    #[default]
    Normal,
    Multiply,
    Screen,
    Overlay,
    Add,
    Subtract,
    Darken,
    Lighten,
}

impl BlendMode {
    pub const ALL: [BlendMode; 8] = [
        BlendMode::Normal,
        BlendMode::Multiply,
        BlendMode::Screen,
        BlendMode::Overlay,
        BlendMode::Add,
        BlendMode::Subtract,
        BlendMode::Darken,
        BlendMode::Lighten,
    ];

    pub fn label(self) -> &'static str {
        match self {
            BlendMode::Normal => "Normal",
            BlendMode::Multiply => "Multiply",
            BlendMode::Screen => "Screen",
            BlendMode::Overlay => "Overlay",
            BlendMode::Add => "Add",
            BlendMode::Subtract => "Subtract",
            BlendMode::Darken => "Darken",
            BlendMode::Lighten => "Lighten",
        }
    }

    /// `top` mixed onto `under`, one channel from 0 to 1.
    pub fn mix(self, under: f32, top: f32) -> f32 {
        match self {
            BlendMode::Normal => top,
            BlendMode::Multiply => under * top,
            BlendMode::Screen => 1.0 - (1.0 - under) * (1.0 - top),
            BlendMode::Overlay if under < 0.5 => 2.0 * under * top,
            BlendMode::Overlay => 1.0 - 2.0 * (1.0 - under) * (1.0 - top),
            BlendMode::Add => (under + top).min(1.0),
            BlendMode::Subtract => (under - top).max(0.0),
            BlendMode::Darken => under.min(top),
            BlendMode::Lighten => under.max(top),
        }
    }
}

/// One layer of a map. Its pixels' alpha is how much of each texel it
/// covers.
#[derive(Clone, PartialEq)]
pub struct Layer {
    pub name: String,
    pub visible: bool,
    /// From 0 to 1.
    pub opacity: f32,
    pub blend: BlendMode,
    /// Red, green, blue and coverage for each texel, rows from the top.
    /// Shared until painted on, so undo steps cost little.
    pub pixels: Arc<Vec<u8>>,
    /// How much of the layer shows at each texel (255 all of it), if it
    /// has a mask.
    pub mask: Option<Arc<Vec<u8>>>,
}

impl std::fmt::Debug for Layer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Layer {:?} ({}, {:?} at {}{})",
            self.name,
            if self.visible { "shown" } else { "hidden" },
            self.blend,
            self.opacity,
            if self.mask.is_some() { ", masked" } else { "" }
        )
    }
}

impl Layer {
    /// An empty layer for a map of `size` by `size` texels.
    pub fn new(name: &str, size: u32) -> Layer {
        Layer {
            name: name.to_owned(),
            visible: true,
            opacity: 1.0,
            blend: BlendMode::Normal,
            pixels: Arc::new(vec![0; texel_count(size) * 4]),
            mask: None,
        }
    }

    /// True if nothing is painted on it.
    pub fn is_empty(&self) -> bool {
        self.pixels.as_chunks::<4>().0.iter().all(|p| p[3] == 0)
    }
}

/// One painted map: the material's value under a stack of layers, bottom
/// first.
#[derive(Clone, Debug)]
pub struct PaintMap {
    pub kind: MapKind,
    /// Texels across (and down).
    pub size: u32,
    /// What's under every layer: the material's own value (sRGB for
    /// colors), and its opacity in alpha.
    pub fill: [u8; 4],
    /// The base color's opacity under the paint, from the material's
    /// texture, if it had one (one byte per texel).
    pub alpha: Option<Arc<Vec<u8>>>,
    pub layers: Vec<Layer>,
    /// The layer brushes paint on.
    pub active: usize,
    /// The layers flattened over `fill`, kept up to date.
    flat: Arc<Vec<u8>>,
}

impl PaintMap {
    /// A map of `fill` with, if the material had a texture for it, that
    /// texture as an opaque "Base" layer (and its alpha), then an empty
    /// layer to paint on.
    fn new(kind: MapKind, size: u32, fill: [u8; 4], base: Option<(Vec<u8>, Vec<u8>)>) -> Self {
        let mut layers = Vec::new();
        let mut alpha = None;
        if let Some((pixels, base_alpha)) = base {
            layers.push(Layer {
                pixels: Arc::new(pixels),
                ..Layer::new("Base", 1)
            });
            if base_alpha.iter().any(|&a| a < 255) {
                alpha = Some(Arc::new(base_alpha));
            }
        }
        layers.push(Layer::new("Paint", size));
        let mut map = PaintMap {
            kind,
            size,
            fill,
            alpha,
            active: layers.len() - 1,
            layers,
            flat: Arc::new(vec![0; texel_count(size) * 4]),
        };
        map.refresh_all();
        map
    }

    /// The layers flattened: red, green, blue, alpha per texel.
    pub fn flat(&self) -> &[u8] {
        &self.flat
    }

    /// The flattened map, shared (a Clone brush's source).
    pub fn flat_shared(&self) -> Arc<Vec<u8>> {
        self.flat.clone()
    }

    /// Flattens the map again at these texels.
    pub fn refresh(&mut self, texels: &[u32]) {
        let mut flat = std::mem::take(&mut self.flat);
        let pixels = Arc::make_mut(&mut flat);
        for &texel in texels {
            let k = texel as usize;
            pixels[k * 4..k * 4 + 4].copy_from_slice(&self.composite(k));
        }
        self.flat = flat;
    }

    /// Flattens the whole map again (after a layer is hidden, moved or
    /// changed).
    pub fn refresh_all(&mut self) {
        let pixels: Vec<u8> = (0..texel_count(self.size))
            .flat_map(|k| self.composite(k))
            .collect();
        self.flat = Arc::new(pixels);
    }

    /// Texel `k` of the flattened map.
    fn composite(&self, k: usize) -> [u8; 4] {
        let mut color = [0.0f32; 3];
        for (c, value) in color.iter_mut().enumerate() {
            *value = f32::from(self.fill[c]) / 255.0;
        }
        for layer in &self.layers {
            if !layer.visible || layer.opacity <= 0.0 {
                continue;
            }
            let pixel = &layer.pixels[k * 4..k * 4 + 4];
            let mut cover = f32::from(pixel[3]) / 255.0 * layer.opacity;
            if let Some(mask) = &layer.mask {
                cover *= f32::from(mask[k]) / 255.0;
            }
            if cover <= 0.0 {
                continue;
            }
            for (c, under) in color.iter_mut().enumerate() {
                let mixed = layer.blend.mix(*under, f32::from(pixel[c]) / 255.0);
                *under += (mixed - *under) * cover;
            }
        }
        let alpha = match &self.alpha {
            Some(alpha) => (u32::from(alpha[k]) * u32::from(self.fill[3]) / 255) as u8,
            None => self.fill[3],
        };
        [
            to_byte(color[0]),
            to_byte(color[1]),
            to_byte(color[2]),
            alpha,
        ]
    }

    /// Adds an empty layer above the active one, makes it the active one,
    /// and returns its index.
    pub fn add_layer(&mut self) -> usize {
        let taken: BTreeSet<&str> = self.layers.iter().map(|l| l.name.as_str()).collect();
        let name = (1..)
            .map(|n| format!("Layer {n}"))
            .find(|name| !taken.contains(name.as_str()))
            .expect("there's always a free number");
        let at = (self.active + 1).min(self.layers.len());
        self.layers.insert(at, Layer::new(&name, self.size));
        self.active = at;
        self.refresh_all();
        at
    }

    /// Removes layer `index` (a map keeps at least one).
    pub fn remove_layer(&mut self, index: usize) {
        if self.layers.len() <= 1 || index >= self.layers.len() {
            return;
        }
        self.layers.remove(index);
        self.active = self.active.min(self.layers.len() - 1);
        self.refresh_all();
    }

    /// Moves layer `index` up (toward the top) or down one place, keeping
    /// it the active one if it was.
    pub fn move_layer(&mut self, index: usize, up: bool) {
        let other = if up {
            index + 1
        } else {
            match index.checked_sub(1) {
                Some(other) => other,
                None => return,
            }
        };
        if other >= self.layers.len() || index >= self.layers.len() {
            return;
        }
        self.layers.swap(index, other);
        if self.active == index {
            self.active = other;
        } else if self.active == other {
            self.active = index;
        }
        self.refresh_all();
    }

    /// The active layer, made safe to change.
    pub fn active_layer_mut(&mut self) -> &mut Layer {
        let index = self.active.min(self.layers.len() - 1);
        &mut self.layers[index]
    }
}

/// One material slot of one object being painted.
#[derive(Clone, Debug)]
pub struct PaintSet {
    /// The material slot (its faces are what's painted).
    pub slot: u32,
    /// Texels across each map.
    pub size: u32,
    /// The maps painted, in [`MapKind`] order.
    pub maps: Vec<PaintMap>,
    /// How steep the height map's bumps are in the normal map it makes.
    pub bump: f32,
    /// The material placed its textures with a uv transform; its maps are
    /// painted on the mesh's own uvs, so saving takes the transform off.
    pub reset_transform: bool,
}

impl PaintSet {
    /// A new set for `slot`, drawn with `material`, at `size` texels: its
    /// base color to begin with (and every map the material has a
    /// texture for, if it moves its textures with a uv transform).
    pub fn new(slot: u32, size: u32, material: &ModelMaterial) -> PaintSet {
        let reset_transform = has_transform(material);
        let mut set = PaintSet {
            slot,
            size,
            maps: Vec::new(),
            bump: 1.0,
            reset_transform,
        };
        set.ensure_map(MapKind::BaseColor, material);
        if reset_transform {
            for kind in MapKind::ALL {
                if has_texture(material, kind) {
                    set.ensure_map(kind, material);
                }
            }
        }
        set
    }

    pub fn map(&self, kind: MapKind) -> Option<&PaintMap> {
        self.maps.iter().find(|map| map.kind == kind)
    }

    pub fn map_mut(&mut self, kind: MapKind) -> Option<&mut PaintMap> {
        self.maps.iter_mut().find(|map| map.kind == kind)
    }

    /// Adds the map for `kind` if it isn't there, from `material`'s own
    /// value and texture for it. Roughness and metalness come together,
    /// since glTF saves them as one texture.
    pub fn ensure_map(&mut self, kind: MapKind, material: &ModelMaterial) -> &mut PaintMap {
        let kinds = match kind {
            MapKind::Roughness | MapKind::Metalness => vec![MapKind::Roughness, MapKind::Metalness],
            _ => vec![kind],
        };
        for kind in kinds {
            if self.map(kind).is_none() {
                let map = base_map(kind, self.size, material);
                let at = self.maps.partition_point(|other| other.kind < kind);
                self.maps.insert(at, map);
            }
        }
        self.map_mut(kind).expect("just added")
    }

    /// Puts every painted map on `material`, as images without files, in
    /// place of what it had for them.
    pub fn apply_to(&self, material: &mut ModelMaterial) {
        for map in &self.maps {
            self.apply_map(map.kind, material);
        }
    }

    /// Puts the map for `kind` on `material`, if it's painted. Height puts
    /// the normal map it makes.
    pub fn apply_map(&self, kind: MapKind, material: &mut ModelMaterial) {
        let Some(map) = self.map(kind) else {
            return;
        };
        let image = |pixels: Vec<u8>| Arc::new(Image::new(self.size, self.size, pixels));
        let pixels = || map.flat().to_vec();
        self.set_map(kind, material, &|| image(pixels()));
        if self.reset_transform {
            material.uv_scale = [1.0, 1.0];
            material.uv_offset = [0.0, 0.0];
            material.uv_rotation = 0.0;
        }
    }

    /// Points `material`'s map for `kind` at the image `image` makes, with
    /// its factor set so the image is what's drawn.
    fn set_map(&self, kind: MapKind, material: &mut ModelMaterial, image: &dyn Fn() -> Arc<Image>) {
        match kind {
            MapKind::BaseColor => {
                material.base_color = [1.0; 4];
                material.base_color_texture = Some(image());
            }
            MapKind::Roughness => {
                material.roughness = 1.0;
                material.roughness_texture = Some(ChannelTexture::new(image(), Channel::Red));
            }
            MapKind::Metalness => {
                material.metalness = 1.0;
                material.metalness_texture = Some(ChannelTexture::new(image(), Channel::Red));
            }
            MapKind::Emissive => {
                material.emissive = [1.0; 3];
                material.emissive_texture = Some(image());
            }
            MapKind::Occlusion => {
                material.occlusion_texture = Some(ChannelTexture::new(image(), Channel::Red));
            }
            MapKind::Height => {
                let Some(normals) = self.normal_map() else {
                    return;
                };
                material.normal_texture = Some(Arc::new(Image::new(self.size, self.size, normals)));
                material.normal_flip_green = false;
                material.normal_scale = 1.0;
            }
        }
    }

    /// The tangent-space normal map the height map makes (glTF's: green
    /// up the texture), or `None` without a height map.
    pub fn normal_map(&self) -> Option<Vec<u8>> {
        let height = self.map(MapKind::Height)?;
        Some(normals_from_height(height.flat(), self.size, self.bump))
    }
}

/// True if `material` moves its textures with a uv transform.
fn has_transform(material: &ModelMaterial) -> bool {
    material.uv_scale != [1.0, 1.0]
        || material.uv_offset != [0.0, 0.0]
        || material.uv_rotation != 0.0
}

/// True if `material` has a texture for `kind`'s map.
fn has_texture(material: &ModelMaterial, kind: MapKind) -> bool {
    match kind {
        MapKind::BaseColor => material.base_color_texture.is_some(),
        MapKind::Roughness => material.roughness_texture.is_some(),
        MapKind::Metalness => material.metalness_texture.is_some(),
        MapKind::Height => false,
        MapKind::Emissive => material.emissive_texture.is_some(),
        MapKind::Occlusion => material.occlusion_texture.is_some(),
    }
}

/// The map for `kind` as `material` draws it now: its factor as the fill,
/// and its texture (times the factor, through its uv transform) as the
/// base layer.
fn base_map(kind: MapKind, size: u32, material: &ModelMaterial) -> PaintMap {
    let grey = |value: f32| {
        let byte = to_byte(value);
        [byte, byte, byte, 255]
    };
    let srgb = |color: [f32; 3], alpha: f32| {
        [
            to_srgb(color[0]),
            to_srgb(color[1]),
            to_srgb(color[2]),
            to_byte(alpha),
        ]
    };
    // A color texture times a linear factor, kept in sRGB.
    let colored = |image: &Arc<Image>, factor: [f32; 3]| {
        let mut pixels = resample(image, size, material);
        let mut alpha = Vec::with_capacity(pixels.len() / 4);
        for pixel in pixels.as_chunks_mut::<4>().0 {
            for c in 0..3 {
                pixel[c] = to_srgb(from_srgb(pixel[c]) * factor[c]);
            }
            alpha.push(pixel[3]);
            pixel[3] = 255;
        }
        (pixels, alpha)
    };
    // One channel of a texture times a factor, as grey.
    let channel = |texture: &ChannelTexture, factor: f32| {
        let mut pixels = resample(&texture.image, size, material);
        for pixel in pixels.as_chunks_mut::<4>().0 {
            let index = match texture.channel {
                Channel::Red => 0,
                Channel::Green => 1,
                Channel::Blue => 2,
                Channel::Alpha => 3,
            };
            let mut value = f32::from(pixel[index]) / 255.0;
            if texture.invert {
                value = 1.0 - value;
            }
            let byte = to_byte(value * factor);
            *pixel = [byte, byte, byte, 255];
        }
        let alpha = vec![255; pixels.len() / 4];
        (pixels, alpha)
    };
    let [r, g, b, a] = material.base_color;
    let (fill, base) = match kind {
        MapKind::BaseColor => (
            srgb([r, g, b], a),
            material
                .base_color_texture
                .as_ref()
                .map(|image| colored(image, [r, g, b])),
        ),
        MapKind::Roughness => (
            grey(material.roughness),
            material
                .roughness_texture
                .as_ref()
                .map(|t| channel(t, material.roughness)),
        ),
        MapKind::Metalness => (
            grey(material.metalness),
            material
                .metalness_texture
                .as_ref()
                .map(|t| channel(t, material.metalness)),
        ),
        MapKind::Occlusion => (
            grey(1.0),
            material.occlusion_texture.as_ref().map(|t| channel(t, 1.0)),
        ),
        MapKind::Emissive => {
            let factor = material.emissive.map(|c| c.clamp(0.0, 1.0));
            (
                srgb(factor, 1.0),
                material
                    .emissive_texture
                    .as_ref()
                    .map(|image| colored(image, factor)),
            )
        }
        MapKind::Height => (grey(0.5), None),
    };
    PaintMap::new(kind, size, fill, base)
}

/// `image` as `size` by `size` texels of the mesh's uvs: where each texel
/// shows it through `material`'s uv transform, repeating, read smoothly.
fn resample(image: &Image, size: u32, material: &ModelMaterial) -> Vec<u8> {
    let [su, sv] = material.uv_scale;
    let (sin, cos) = material.uv_rotation.sin_cos();
    let [ou, ov] = material.uv_offset;
    let mut pixels = Vec::with_capacity(texel_count(size) * 4);
    for k in 0..texel_count(size) {
        let uv = texel_uv(k as u32, size);
        // As the renderer moves them (`KHR_texture_transform`).
        let moved = Vec2::new(
            cos * su * uv.x + sin * sv * uv.y + ou,
            -sin * su * uv.x + cos * sv * uv.y + ov,
        );
        let sample = sample(&image.rgba, image.width, image.height, moved, true);
        pixels.extend(sample.map(|c| c.round().clamp(0.0, 255.0) as u8));
    }
    pixels
}

/// The texel numbers of a map `size` texels across.
pub(crate) fn texel_count(size: u32) -> usize {
    (size as usize) * (size as usize)
}

/// The uv at the middle of texel `k` of a map `size` texels across.
pub fn texel_uv(k: u32, size: u32) -> Vec2 {
    Vec2::new((k % size) as f32 + 0.5, (k / size) as f32 + 0.5) / size as f32
}

/// `pixels` (RGBA, `width` by `height`) at `uv`, read between texel
/// middles as the GPU's linear filter does: 0 to 255 per channel.
/// Repeats past the edges with `wrap`, or else stops at them.
pub fn sample(pixels: &[u8], width: u32, height: u32, uv: Vec2, wrap: bool) -> [f32; 4] {
    if width == 0 || height == 0 || pixels.len() < (width * height * 4) as usize {
        return [0.0; 4];
    }
    let x = uv.x * width as f32 - 0.5;
    let y = uv.y * height as f32 - 0.5;
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let at = |x: i64, y: i64| -> [f32; 4] {
        let (x, y) = if wrap {
            (x.rem_euclid(width.into()), y.rem_euclid(height.into()))
        } else {
            (
                x.clamp(0, i64::from(width) - 1),
                y.clamp(0, i64::from(height) - 1),
            )
        };
        let k = (y as usize * width as usize + x as usize) * 4;
        let p = &pixels[k..k + 4];
        [p[0], p[1], p[2], p[3]].map(f32::from)
    };
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

/// A tangent-space normal map from a height map (its red channel; 0.5 is
/// level), wrapping at the edges. `bump` 1 makes the full range of height
/// a 32nd of the texture's width.
pub fn normals_from_height(height: &[u8], size: u32, bump: f32) -> Vec<u8> {
    let size_i = i64::from(size);
    let h = |x: i64, y: i64| {
        let k = (y.rem_euclid(size_i) * size_i + x.rem_euclid(size_i)) as usize;
        f32::from(height[k * 4]) / 255.0
    };
    let steep = bump * size as f32 / 32.0;
    let mut normals = Vec::with_capacity(texel_count(size) * 4);
    for y in 0..size_i {
        for x in 0..size_i {
            let across = (h(x + 1, y) - h(x - 1, y)) / 2.0;
            let down = (h(x, y + 1) - h(x, y - 1)) / 2.0;
            // Facing away from the slope: rising to the right tips it
            // left; rising down the texture tips it up (green).
            let normal = glam::Vec3::new(-across * steep, down * steep, 1.0).normalize();
            normals.extend([
                to_byte(normal.x * 0.5 + 0.5),
                to_byte(normal.y * 0.5 + 0.5),
                to_byte(normal.z * 0.5 + 0.5),
                255,
            ]);
        }
    }
    normals
}

pub(crate) fn to_byte(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// A linear value (0 to 1) as an sRGB byte.
pub(crate) fn to_srgb(linear: f32) -> u8 {
    let linear = linear.clamp(0.0, 1.0);
    let encoded = if linear <= 0.003_130_8 {
        linear * 12.92
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    };
    to_byte(encoded)
}

/// An sRGB byte as a linear value.
pub(crate) fn from_srgb(byte: u8) -> f32 {
    let encoded = f32::from(byte) / 255.0;
    if encoded <= 0.040_45 {
        encoded / 12.92
    } else {
        ((encoded + 0.055) / 1.055).powf(2.4)
    }
}

/// How a PNG keeps its texels.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Png {
    Rgba,
    /// The red channel, as grey.
    Grey,
}

/// `pixels` (RGBA, `size` by `size`) as a PNG file.
pub(crate) fn encode_png(size: u32, pixels: &[u8], kind: Png) -> Vec<u8> {
    let mut bytes = Vec::new();
    let written = match kind {
        Png::Rgba => image::RgbaImage::from_raw(size, size, pixels.to_vec())
            .map(|image| image.write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)),
        Png::Grey => {
            let grey: Vec<u8> = pixels.as_chunks::<4>().0.iter().map(|p| p[0]).collect();
            image::GrayImage::from_raw(size, size, grey)
                .map(|image| image.write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png))
        }
    };
    debug_assert!(
        matches!(written, Some(Ok(()))),
        "PNGs of the right size encode"
    );
    bytes
}

/// One byte per texel, as a grey PNG.
fn encode_grey(size: u32, values: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    if let Some(image) = image::GrayImage::from_raw(size, size, values.to_vec()) {
        let _ = image.write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png);
    }
    bytes
}

/// A PNG's texels as RGBA, if it reads and is `size` by `size`.
fn decode_png(bytes: &[u8], size: u32) -> Option<Vec<u8>> {
    let image = image::load_from_memory_with_format(bytes, image::ImageFormat::Png).ok()?;
    (image.width() == size && image.height() == size).then(|| image.into_rgba8().into_raw())
}

/// What the sidecar keeps of a [`PaintSet`]; its layers are PNGs in the
/// sidecar folder.
#[derive(Serialize, Deserialize)]
pub(crate) struct PaintData {
    slot: u32,
    size: u32,
    bump: f32,
    #[serde(default)]
    reset_transform: bool,
    maps: Vec<MapData>,
}

#[derive(Serialize, Deserialize)]
struct MapData {
    kind: MapKind,
    fill: [u8; 4],
    active: usize,
    /// The file of the base color's opacity under the paint, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    alpha: Option<String>,
    layers: Vec<LayerData>,
}

#[derive(Serialize, Deserialize)]
struct LayerData {
    name: String,
    visible: bool,
    opacity: f32,
    blend: BlendMode,
    file: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mask: Option<String>,
}

impl PaintSet {
    /// The name of a file of this set's, for object `object`, in the
    /// sidecar folder: `3-0-BaseColor-1.png`.
    fn file(&self, object: usize, kind: MapKind, part: &str) -> String {
        format!("{object}-{}-{}-{part}.png", self.slot, kind.word())
    }

    /// What the sidecar keeps of it, as object `object`'s.
    pub(crate) fn data(&self, object: usize) -> PaintData {
        PaintData {
            slot: self.slot,
            size: self.size,
            bump: self.bump,
            reset_transform: self.reset_transform,
            maps: self
                .maps
                .iter()
                .map(|map| MapData {
                    kind: map.kind,
                    fill: map.fill,
                    active: map.active,
                    alpha: map
                        .alpha
                        .as_ref()
                        .map(|_| self.file(object, map.kind, "alpha")),
                    layers: map
                        .layers
                        .iter()
                        .enumerate()
                        .map(|(k, layer)| LayerData {
                            name: layer.name.clone(),
                            visible: layer.visible,
                            opacity: layer.opacity,
                            blend: layer.blend,
                            file: self.file(object, map.kind, &k.to_string()),
                            mask: layer
                                .mask
                                .as_ref()
                                .map(|_| self.file(object, map.kind, &format!("{k}-mask"))),
                        })
                        .collect(),
                })
                .collect(),
        }
    }

    /// Its layers', masks' and alphas' PNGs, by their names in the sidecar
    /// folder, as object `object`'s.
    pub(crate) fn files(&self, object: usize) -> Vec<(String, Vec<u8>)> {
        let mut files = Vec::new();
        for map in &self.maps {
            if let Some(alpha) = &map.alpha {
                files.push((
                    self.file(object, map.kind, "alpha"),
                    encode_grey(self.size, alpha),
                ));
            }
            for (k, layer) in map.layers.iter().enumerate() {
                files.push((
                    self.file(object, map.kind, &k.to_string()),
                    encode_png(self.size, &layer.pixels, Png::Rgba),
                ));
                if let Some(mask) = &layer.mask {
                    files.push((
                        self.file(object, map.kind, &format!("{k}-mask")),
                        encode_grey(self.size, mask),
                    ));
                }
            }
        }
        files
    }

    /// The set `data` describes, its layers read with `read` (a file name
    /// in the sidecar folder to its bytes). A layer whose file is missing
    /// or damaged comes back empty.
    pub(crate) fn load(data: PaintData, read: &mut dyn FnMut(&str) -> Option<Vec<u8>>) -> PaintSet {
        let size = data.size.clamp(1, SIZES[SIZES.len() - 1]);
        let grey = |read: &mut dyn FnMut(&str) -> Option<Vec<u8>>, file: &str| {
            let pixels = decode_png(&read(file)?, size)?;
            Some(
                pixels
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|p| p[0])
                    .collect::<Vec<u8>>(),
            )
        };
        let mut maps = Vec::new();
        for map in data.maps {
            let alpha = map
                .alpha
                .as_deref()
                .and_then(|file| grey(read, file))
                .map(Arc::new);
            let mut layers = Vec::new();
            for layer in map.layers {
                let pixels = read(&layer.file)
                    .and_then(|bytes| decode_png(&bytes, size))
                    .unwrap_or_else(|| vec![0; texel_count(size) * 4]);
                layers.push(Layer {
                    name: layer.name,
                    visible: layer.visible,
                    opacity: layer.opacity.clamp(0.0, 1.0),
                    blend: layer.blend,
                    pixels: Arc::new(pixels),
                    mask: layer
                        .mask
                        .as_deref()
                        .and_then(|file| grey(read, file))
                        .map(Arc::new),
                });
            }
            if layers.is_empty() {
                layers.push(Layer::new("Paint", size));
            }
            let mut map = PaintMap {
                kind: map.kind,
                size,
                fill: map.fill,
                alpha,
                active: map.active.min(layers.len() - 1),
                layers,
                flat: Arc::new(Vec::new()),
            };
            map.refresh_all();
            if !maps.iter().any(|other: &PaintMap| other.kind == map.kind) {
                maps.push(map);
            }
        }
        maps.sort_by_key(|map| map.kind);
        PaintSet {
            slot: data.slot,
            size,
            maps,
            bump: data.bump,
            reset_transform: data.reset_transform,
        }
    }
}

/// The project path of the sidecar folder of the model at `model`, which
/// keeps its paint layers (`assets/models/Head.glb` has
/// `assets/models/Head.meshedit`).
pub fn paint_folder(model: &str) -> String {
    let (folder, name) = model.rsplit_once('/').unwrap_or(("", model));
    let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
    match folder {
        "" => format!("{stem}{}", axcenity_assets::MESH_EDIT_FOLDER_SUFFIX),
        _ => format!(
            "{folder}/{stem}{}",
            axcenity_assets::MESH_EDIT_FOLDER_SUFFIX
        ),
    }
}

/// The start of the names of maps made for `material` (slot `slot`) of
/// object `index`, named `object`, of the model whose file's stem is
/// `stem`: `Chair_Seat_Wood`, to which `_BaseColor.png` and the rest are
/// added.
pub(crate) fn map_stem(
    stem: &str,
    object: &str,
    index: usize,
    material: &ModelMaterial,
    slot: u32,
) -> String {
    let material_name = match material.name.as_str() {
        "" => format!("Slot{}", slot + 1),
        name => name.to_owned(),
    };
    axcenity_assets::safe_file_name(&format!("{stem}_{object}_{material_name}"))
        .unwrap_or_else(|| format!("{stem}_{index}"))
        .replace(['.', ' '], "_")
}

impl ModelDoc {
    /// Save's part for painting: each painted map flattened into a PNG
    /// named by its role, in `textures/` beside the model at `model` (a
    /// project path), with each material pointed at its maps (and other
    /// objects' materials of the same name, which the `.glb` shares).
    /// Returns each file's project path and bytes.
    pub fn bake_paint(&mut self, model: &str) -> Vec<(String, Vec<u8>)> {
        let (folder, file) = model.rsplit_once('/').unwrap_or(("", model));
        let stem = file.rsplit_once('.').map_or(file, |(stem, _)| stem);
        let mut files = Vec::new();
        let mut names = BTreeSet::new();
        // Each painted material's name and what it became.
        let mut baked: Vec<(String, ModelMaterial)> = Vec::new();
        let mut painted: BTreeSet<(usize, u32)> = BTreeSet::new();
        for (index, object) in self.objects.iter_mut().enumerate() {
            for set in &object.paint {
                let Some(material) = object.materials.get_mut(set.slot as usize) else {
                    continue;
                };
                painted.insert((index, set.slot));
                let wanted = map_stem(stem, &object.name, index, material, set.slot);
                let mut name = wanted.clone();
                if !names.insert(name.clone()) {
                    name = format!("{wanted}_{index}_{}", set.slot);
                    names.insert(name.clone());
                }
                let mut add = |word: &str, pixels: Vec<u8>, png: Png| -> Arc<Image> {
                    let relative = format!("{TEXTURES_FOLDER}/{name}_{word}.png");
                    let path = match folder {
                        "" => relative.clone(),
                        _ => format!("{folder}/{relative}"),
                    };
                    files.push((path, encode_png(set.size, &pixels, png)));
                    let mut image = Image::new(set.size, set.size, pixels);
                    image.file = Some(relative);
                    Arc::new(image)
                };
                for map in &set.maps {
                    let pixels = map.flat().to_vec();
                    match map.kind {
                        MapKind::BaseColor => {
                            let image = add("BaseColor", pixels, Png::Rgba);
                            set.set_map(map.kind, material, &|| image.clone());
                        }
                        MapKind::Emissive => {
                            let image = add("Emissive", pixels, Png::Rgba);
                            set.set_map(map.kind, material, &|| image.clone());
                        }
                        MapKind::Occlusion => {
                            let image = add("Occlusion", pixels, Png::Grey);
                            set.set_map(map.kind, material, &|| image.clone());
                        }
                        MapKind::Height => {
                            if let Some(normals) = set.normal_map() {
                                material.normal_texture = Some(add("Normal", normals, Png::Rgba));
                                material.normal_flip_green = false;
                                material.normal_scale = 1.0;
                            }
                        }
                        // Saved together below.
                        MapKind::Roughness | MapKind::Metalness => {}
                    }
                }
                if let (Some(rough), Some(metal)) =
                    (set.map(MapKind::Roughness), set.map(MapKind::Metalness))
                {
                    // glTF's metallic-roughness texture: roughness in
                    // green, metalness in blue.
                    let pixels: Vec<u8> = rough
                        .flat()
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .zip(metal.flat().as_chunks::<4>().0)
                        .flat_map(|(r, m)| [255, r[0], m[0], 255])
                        .collect();
                    let image = add("MetallicRoughness", pixels, Png::Rgba);
                    material.roughness = 1.0;
                    material.metalness = 1.0;
                    material.roughness_texture =
                        Some(ChannelTexture::new(image.clone(), Channel::Green));
                    material.metalness_texture = Some(ChannelTexture::new(image, Channel::Blue));
                }
                if set.reset_transform {
                    material.uv_scale = [1.0, 1.0];
                    material.uv_offset = [0.0, 0.0];
                    material.uv_rotation = 0.0;
                }
                if !material.name.is_empty() {
                    baked.push((material.name.clone(), material.clone()));
                }
            }
        }
        // Copies of a painted material elsewhere (a level of detail, or an
        // object sharing it) draw the same maps.
        for (index, object) in self.objects.iter_mut().enumerate() {
            for (slot, material) in object.materials.iter_mut().enumerate() {
                if painted.contains(&(index, slot as u32)) {
                    continue;
                }
                if let Some((_, done)) = baked.iter().find(|(name, _)| *name == material.name) {
                    *material = done.clone();
                }
            }
        }
        files
    }

    /// Every object's paint layers as PNGs, by their names in the sidecar
    /// folder ([`paint_folder`]).
    pub fn paint_files(&self) -> Vec<(String, Vec<u8>)> {
        self.objects
            .iter()
            .enumerate()
            .flat_map(|(index, object)| object.paint.iter().flat_map(move |set| set.files(index)))
            .collect()
    }
}
