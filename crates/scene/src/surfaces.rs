//! The looks of the built-in materials: Wood's grain, Marble's veins, and
//! so on. Each is three small tiling maps made from noise the first time a
//! part of that material is drawn, so neither the engine nor a project
//! carries image files for them. The part's `Color` tints the color map,
//! and its `Roughness` scales the roughness map, as with any other map.

use std::sync::{Arc, OnceLock};

use axcenity_core::Material as Kind;
use axcenity_render::{Channel, ChannelMap, Material, NormalMap, TextureSource};

/// How many pixels wide and tall each map is.
const SIZE: usize = 512;
/// Texture keys of the built-in surfaces. Project images count up from 1,
/// so they never reach these.
const KEY_BASE: u64 = 0xB017_0000_0000_0000;

/// One built-in material's maps.
pub(crate) struct Surface {
    color: Arc<dyn TextureSource>,
    normal: Arc<dyn TextureSource>,
    roughness: Arc<dyn TextureSource>,
}

impl Surface {
    /// Puts these maps onto `material`.
    pub(crate) fn apply(&self, material: &mut Material) {
        material.base_color_texture = Some(self.color.clone());
        material.maps.normal = Some(NormalMap {
            texture: self.normal.clone(),
            scale: 1.0,
            flip_green: false,
        });
        material.maps.roughness = Some(ChannelMap {
            texture: self.roughness.clone(),
            channel: Channel::Red,
            invert: false,
        });
    }
}

/// `kind`'s maps, or `None` for a material drawn flat (Plastic, Metal, Glass
/// and the like). Made once, on first use.
pub(crate) fn of(kind: Kind) -> Option<&'static Surface> {
    static MADE: [OnceLock<Option<Surface>>; Kind::ALL.len()] =
        [const { OnceLock::new() }; Kind::ALL.len()];
    let index = Kind::ALL.iter().position(|&item| item == kind)?;
    MADE[index].get_or_init(|| make(kind, index)).as_ref()
}

/// A map held in memory.
struct Map {
    key: u64,
    rgba: Vec<u8>,
}

impl TextureSource for Map {
    fn key(&self) -> u64 {
        self.key
    }
    fn size(&self) -> (u32, u32) {
        (SIZE as u32, SIZE as u32)
    }
    fn rgba(&self) -> &[u8] {
        &self.rgba
    }
}

/// What a material is at one point of its tile.
#[derive(Clone, Copy)]
struct Texel {
    /// How far the surface stands out, in pixels of the map.
    height: f32,
    /// sRGB, 0 to 1.
    color: [f32; 3],
    /// Multiplies the part's `Roughness`.
    roughness: f32,
}

fn make(kind: Kind, index: usize) -> Option<Surface> {
    let shade = shader_of(kind)?;
    let texels: Vec<Texel> = (0..SIZE * SIZE)
        .map(|i| {
            let (x, y) = (i % SIZE, i / SIZE);
            shade(x as f32 / SIZE as f32, y as f32 / SIZE as f32)
        })
        .collect();
    let mut color = Vec::with_capacity(texels.len() * 4);
    let mut normal = Vec::with_capacity(texels.len() * 4);
    let mut roughness = Vec::with_capacity(texels.len() * 4);
    let height = |x: usize, y: usize| texels[(y % SIZE) * SIZE + (x % SIZE)].height;
    let byte = |value: f32| (value.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    for (i, texel) in texels.iter().enumerate() {
        let (x, y) = (i % SIZE, i / SIZE);
        color.extend(texel.color.map(byte));
        color.push(255);
        // Sloping up to the right tips the surface left; V runs down the
        // image, so sloping up downward tips it up (a green-up normal map).
        let dx = height(x + 1, y) - height(x + SIZE - 1, y);
        let dy = height(x, y + 1) - height(x, y + SIZE - 1);
        let (nx, ny, nz) = (-dx / 2.0, dy / 2.0, 1.0);
        let length = (nx * nx + ny * ny + nz * nz).sqrt();
        normal.extend([nx / length, ny / length, nz / length].map(|n: f32| byte(n * 0.5 + 0.5)));
        normal.push(255);
        let rough = byte(texel.roughness);
        roughness.extend([rough, rough, rough, 255]);
    }
    let map = |slot: u64, rgba: Vec<u8>| -> Arc<dyn TextureSource> {
        Arc::new(Map {
            key: KEY_BASE + index as u64 * 4 + slot,
            rgba,
        })
    };
    Some(Surface {
        color: map(0, color),
        normal: map(1, normal),
        roughness: map(2, roughness),
    })
}

/// How to work out each point of `kind`'s tile.
fn shader_of(kind: Kind) -> Option<Box<dyn Fn(f32, f32) -> Texel>> {
    Some(match kind {
        Kind::Wood => Box::new(wood()),
        Kind::WoodPlanks => Box::new(planks()),
        Kind::Marble => Box::new(marble()),
        Kind::Granite => Box::new(granite()),
        Kind::Concrete => Box::new(concrete()),
        Kind::Slate => Box::new(slate()),
        Kind::Brick => Box::new(brick()),
        Kind::Fabric => Box::new(fabric()),
        Kind::Sand => Box::new(sand()),
        Kind::Plastic
        | Kind::SmoothPlastic
        | Kind::Metal
        | Kind::Glass
        | Kind::Neon
        | Kind::Ice
        | Kind::Rubber => return None,
    })
}

const TAU: f32 = std::f32::consts::TAU;

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    let t = t.clamp(0.0, 1.0);
    std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t)
}

fn smooth(from: f32, to: f32, value: f32) -> f32 {
    let t = ((value - from) / (to - from)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Value noise on a grid that wraps around, so what's made from it tiles.
struct Noise {
    columns: usize,
    rows: usize,
    values: Vec<f32>,
}

impl Noise {
    fn new(seed: u32, columns: usize, rows: usize) -> Noise {
        // Xorshift: enough for texture noise, and the same on every machine.
        let mut state = seed.wrapping_mul(2_654_435_761) | 1;
        let values = (0..columns * rows)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                (state >> 8) as f32 / 16_777_216.0
            })
            .collect();
        Noise {
            columns,
            rows,
            values,
        }
    }

    /// From 0 to 1, at `u`, `v` (each wrapping at 1).
    fn at(&self, u: f32, v: f32) -> f32 {
        let (x, y) = (
            u.rem_euclid(1.0) * self.columns as f32,
            v.rem_euclid(1.0) * self.rows as f32,
        );
        let (cell_x, cell_y) = (x.floor(), y.floor());
        let ease = |t: f32| t * t * (3.0 - 2.0 * t);
        let (tx, ty) = (ease(x - cell_x), ease(y - cell_y));
        let value = |dx: usize, dy: usize| {
            let column = (cell_x as usize + dx) % self.columns;
            let row = (cell_y as usize + dy) % self.rows;
            self.values[row * self.columns + column]
        };
        let top = value(0, 0) + (value(1, 0) - value(0, 0)) * tx;
        let bottom = value(0, 1) + (value(1, 1) - value(0, 1)) * tx;
        top + (bottom - top) * ty
    }
}

/// Several [`Noise`] layers, each twice as fine and half as strong as the
/// one before. From 0 to 1.
struct Layers(Vec<Noise>);

impl Layers {
    fn new(seed: u32, columns: usize, rows: usize, count: u32) -> Layers {
        Layers(
            (0..count)
                .map(|i| Noise::new(seed + i * 17, columns << i, rows << i))
                .collect(),
        )
    }

    fn at(&self, u: f32, v: f32) -> f32 {
        let (mut sum, mut total, mut strength) = (0.0, 0.0, 1.0);
        for layer in &self.0 {
            sum += layer.at(u, v) * strength;
            total += strength;
            strength *= 0.5;
        }
        sum / total
    }
}

/// Grain running along V, at `offset` across the rings.
fn grain(warp: &Layers, fibers: &Noise, u: f32, v: f32, rings: f32, offset: f32) -> (f32, f32) {
    let wave = (TAU * (rings * u + 2.2 * warp.at(u + offset, v) + offset)).sin() * 0.5 + 0.5;
    (wave, fibers.at(u + offset, v))
}

fn wood() -> impl Fn(f32, f32) -> Texel {
    let warp = Layers::new(11, 3, 2, 3);
    let fibers = Noise::new(23, 180, 8);
    move |u, v| {
        let (rings, fiber) = grain(&warp, &fibers, u, v, 9.0, 0.0);
        let shade = rings * 0.55 + fiber * 0.45;
        Texel {
            height: shade * 1.2,
            color: mix([0.42, 0.26, 0.14], [0.72, 0.5, 0.3], shade),
            roughness: 0.85 + fiber * 0.15,
        }
    }
}

fn planks() -> impl Fn(f32, f32) -> Texel {
    const BOARDS: f32 = 4.0;
    let warp = Layers::new(31, 3, 2, 3);
    let fibers = Noise::new(37, 180, 8);
    let tone = Noise::new(41, 8, 1);
    move |u, v| {
        let across = u * BOARDS;
        let board = across.floor();
        let inside = across - board;
        // Board ends fall at a different place on each board.
        let shift = tone.at((board + 0.5) / 8.0 + 0.031, 0.5);
        let along = (v + shift).rem_euclid(1.0);
        let offset = (board + 1.0) * 0.37;
        let (rings, fiber) = grain(&warp, &fibers, inside / BOARDS, v, 2.0, offset);
        let edge = inside.min(1.0 - inside).min(along.min(1.0 - along) * 3.0);
        let seam = smooth(0.0, 0.03, edge);
        let shade = (rings * 0.45 + fiber * 0.35 + tone.at(board / 8.0, 0.0) * 0.3) * seam;
        Texel {
            height: seam * 3.0 + fiber * 0.8,
            color: mix([0.2, 0.12, 0.07], [0.74, 0.52, 0.32], shade),
            roughness: 0.85 + fiber * 0.15,
        }
    }
}

fn marble() -> impl Fn(f32, f32) -> Texel {
    let warp = Layers::new(53, 4, 4, 5);
    let clouds = Layers::new(59, 3, 3, 4);
    move |u, v| {
        let line = (TAU * (2.0 * u + v + 1.6 * warp.at(u, v))).sin().abs();
        let vein = 1.0 - smooth(0.0, 0.12, line);
        let fine = (TAU * (5.0 * u - 3.0 * v + 2.4 * warp.at(v, u)))
            .sin()
            .abs();
        let thin = (1.0 - smooth(0.0, 0.05, fine)) * 0.45;
        let dark = (vein + thin).min(1.0);
        Texel {
            height: 0.0,
            color: mix(
                mix([0.9, 0.89, 0.87], [0.98, 0.98, 0.97], clouds.at(u, v)),
                [0.38, 0.38, 0.42],
                dark,
            ),
            roughness: 0.8 + dark * 0.2,
        }
    }
}

fn granite() -> impl Fn(f32, f32) -> Texel {
    let coarse = Layers::new(61, 40, 40, 3);
    let flecks = Noise::new(67, 170, 170);
    let glints = Noise::new(71, 210, 210);
    move |u, v| {
        let base = coarse.at(u, v);
        let fleck = smooth(0.62, 0.72, flecks.at(u, v));
        let glint = smooth(0.8, 0.86, glints.at(u, v));
        let color = mix([0.36, 0.35, 0.36], [0.68, 0.66, 0.65], base);
        let color = mix(color, [0.12, 0.12, 0.13], fleck);
        Texel {
            height: base * 0.6,
            color: mix(color, [0.92, 0.91, 0.9], glint),
            roughness: 1.0 - glint * 0.5,
        }
    }
}

fn concrete() -> impl Fn(f32, f32) -> Texel {
    let clouds = Layers::new(73, 5, 5, 5);
    let grit = Noise::new(79, 200, 200);
    let pits = Noise::new(83, 60, 60);
    move |u, v| {
        let cloud = clouds.at(u, v);
        let pit = smooth(0.78, 0.86, pits.at(u, v));
        let fine = grit.at(u, v);
        let light = cloud * 0.6 + fine * 0.4;
        Texel {
            height: fine * 0.9 - pit * 2.5,
            color: mix(
                mix([0.5, 0.5, 0.49], [0.74, 0.74, 0.72], light),
                [0.28, 0.28, 0.28],
                pit * 0.7,
            ),
            roughness: 0.9 + fine * 0.1,
        }
    }
}

fn slate() -> impl Fn(f32, f32) -> Texel {
    let warp = Layers::new(89, 4, 4, 4);
    let chips = Layers::new(97, 10, 26, 4);
    let tone = Noise::new(101, 2, 12);
    move |u, v| {
        let layered = v * 9.0 + warp.at(u, v) * 2.4;
        let step = layered.fract();
        let layer = layered.floor() / 9.0;
        let lip = smooth(0.0, 0.08, step);
        let chip = chips.at(u, v);
        let shade = chip * 0.55 + tone.at(u, layer) * 0.45;
        Texel {
            height: step * 3.5 * lip + chip * 3.0,
            color: mix(
                [0.17, 0.19, 0.22],
                mix([0.42, 0.45, 0.48], [0.13, 0.14, 0.17], 1.0 - lip),
                shade * 1.3,
            ),
            roughness: 0.85 + chip * 0.15,
        }
    }
}

fn brick() -> impl Fn(f32, f32) -> Texel {
    const COURSES: f32 = 8.0;
    const PER_COURSE: f32 = 4.0;
    let tone = Noise::new(103, 8, 8);
    let grit = Layers::new(107, 90, 90, 2);
    move |u, v| {
        let row = (v * COURSES).floor();
        let shift = if (row as i32).rem_euclid(2) == 0 {
            0.0
        } else {
            0.5
        };
        let across = (u * PER_COURSE + shift).rem_euclid(PER_COURSE);
        let column = across.floor();
        let (x, y) = (across - column, v * COURSES - row);
        // The distance to the nearest mortar line, in the brick's height.
        let edge = (x.min(1.0 - x) * COURSES / PER_COURSE * 2.0).min(y.min(1.0 - y));
        let brick = smooth(0.05, 0.11, edge);
        let shade = tone.at((column + 0.5) / 8.0, (row + 0.5) / 8.0);
        let rough = grit.at(u, v);
        let body = mix(
            [0.4, 0.15, 0.1],
            [0.68, 0.3, 0.2],
            shade * 0.7 + rough * 0.3,
        );
        Texel {
            height: brick * 5.0 + rough * 1.2,
            color: mix(
                mix([0.58, 0.55, 0.5], [0.72, 0.7, 0.66], rough),
                body,
                brick,
            ),
            roughness: 0.9 + rough * 0.1,
        }
    }
}

fn fabric() -> impl Fn(f32, f32) -> Texel {
    const THREADS: f32 = 64.0;
    let tone = Layers::new(109, 6, 6, 3);
    move |u, v| {
        let warp = (TAU * u * THREADS).sin();
        let weft = (TAU * v * THREADS).sin();
        // Over and under, in turns.
        let over = ((u * THREADS).floor() + (v * THREADS).floor()) as i32 % 2 == 0;
        let thread = if over { warp.abs() } else { weft.abs() };
        let shade = thread * 0.6 + tone.at(u, v) * 0.4;
        Texel {
            height: thread * 0.9,
            color: mix([0.62, 0.62, 0.62], [0.92, 0.92, 0.92], shade),
            roughness: 1.0,
        }
    }
}

fn sand() -> impl Fn(f32, f32) -> Texel {
    let warp = Layers::new(113, 3, 3, 3);
    let grains = Noise::new(127, 230, 230);
    let drift = Layers::new(131, 4, 4, 3);
    move |u, v| {
        let ripple = (TAU * (6.0 * v + u + 1.3 * warp.at(u, v))).sin() * 0.5 + 0.5;
        let grain = grains.at(u, v);
        let shade = drift.at(u, v) * 0.5 + grain * 0.5;
        Texel {
            height: ripple * 2.2 + grain * 0.9,
            color: mix([0.66, 0.54, 0.36], [0.9, 0.79, 0.59], shade),
            roughness: 1.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn surfaces() -> impl Iterator<Item = (Kind, &'static Surface)> {
        Kind::ALL.into_iter().filter_map(|k| Some((k, of(k)?)))
    }

    #[test]
    fn the_textured_materials_have_maps_and_the_flat_ones_have_none() {
        let textured: Vec<Kind> = surfaces().map(|(kind, _)| kind).collect();
        assert_eq!(textured.len(), 9);
        for kind in [Kind::Plastic, Kind::SmoothPlastic, Kind::Metal, Kind::Glass] {
            assert!(of(kind).is_none());
        }
    }

    #[test]
    fn each_built_in_surface_looks_different() {
        let looks: Vec<&[u8]> = surfaces().map(|(_, s)| s.color.rgba()).collect();
        for (i, a) in looks.iter().enumerate() {
            for b in &looks[i + 1..] {
                let differing = a.iter().zip(b.iter()).filter(|(x, y)| x != y).count();
                assert!(differing > a.len() / 2, "two surfaces look the same");
            }
        }
    }

    #[test]
    fn maps_have_the_right_size_and_their_own_keys() {
        let mut keys = Vec::new();
        for (_, surface) in surfaces() {
            for map in [&surface.color, &surface.normal, &surface.roughness] {
                assert_eq!(map.size(), (SIZE as u32, SIZE as u32));
                assert_eq!(map.rgba().len(), SIZE * SIZE * 4);
                keys.push(map.key());
            }
        }
        let count = keys.len();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), count);
    }

    #[test]
    fn noise_wraps_around_so_tiles_meet() {
        let noise = Layers::new(5, 4, 4, 3);
        for i in 0..32 {
            let t = i as f32 / 32.0;
            assert!((noise.at(0.0, t) - noise.at(1.0, t)).abs() < 1e-4);
            assert!((noise.at(t, 0.0) - noise.at(t, 1.0)).abs() < 1e-4);
        }
    }
}
