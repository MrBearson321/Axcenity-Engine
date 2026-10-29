//! Baking (Phase 61): a high-poly object's detail put into maps on a
//! low-poly object's uvs, so the low-poly one looks like the high-poly
//! one in a game.
//!
//! Each texel of the low-poly object's map (through a [`TexelMap`], so the
//! gutter round each uv island is baked too) is a place and a normal on
//! its surface. A ray starts out at the cage (that place pushed out along
//! the normal by the cage distance) and goes back in; the first point of
//! the high-poly surface it meets, within twice the cage, is what the
//! texel shows. A ray that meets nothing takes the nearest point of the
//! surface instead (counted in [`BakedSlot::missed`]).
//!
//! - **Normal**: the high-poly normal there, in the low-poly surface's
//!   tangent frame, worked out as `assets` works out the tangents of a
//!   loaded `.glb` (green up the texture; DirectX's flips it).
//! - **Ambient occlusion**: the share of rays from the high-poly point,
//!   spread over its sky, that meet nothing within the occlusion distance.
//! - **Curvature**: the high-poly surface's mean curvature there, 0.5 flat,
//!   brighter on bulges and darker in hollows.
//! - **Base color**: the high-poly object's paint (or its material's
//!   texture or color) at the point's uv.
//!
//! [`ModelDoc::bake`] writes each as a PNG named by its role beside the
//! model and points the low-poly object's material at them. Islands whose
//! uvs overlap share texels, so only one of them gets its own bake there:
//! the report names them.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use axcenity_assets::{Channel, ChannelTexture, Image, ModelMaterial, TEXTURES_FOLDER};
use glam::{Vec2, Vec3};

use crate::half_edge::EditMesh;
use crate::model_doc::{DocObject, ModelDoc};
use crate::modifiers::Purpose;
use crate::paint::texels::surface_weights;
use crate::paint::{
    MapKind, Png, TexelMap, encode_png, from_srgb, map_stem, sample, texel_count, to_byte, to_srgb,
};
use crate::retopo::Reference;
use crate::uv;

/// The cage's distance, when it's left to the baker, as a share of the
/// high-poly object's size (its box's diagonal).
const CAGE_SHARE: f32 = 0.02;
/// How far ambient occlusion looks, when it's left to the baker.
const OCCLUSION_SHARE: f32 = 0.1;
/// The curvature map's scale: a bulge of this share of the size across
/// (as a radius) is about three quarters of the way to white.
const CURVATURE_SHARE: f32 = 1.0 / 50.0;

/// What to bake, and how.
#[derive(Clone, Debug, PartialEq)]
pub struct BakeSettings {
    /// Texels across each map.
    pub size: u32,
    /// How far out the rays start (the cage), in the model's units; `None`
    /// for a fiftieth of the high-poly object's size.
    pub cage: Option<f32>,
    pub normal: bool,
    pub occlusion: bool,
    pub curvature: bool,
    pub base_color: bool,
    /// A DirectX normal map (green down the texture), saved as
    /// `_NormalDX.png`, which the engine reads as one.
    pub flip_green: bool,
    /// Rays per texel for ambient occlusion.
    pub occlusion_rays: u32,
    /// How far an occluder counts, in the model's units; `None` for a
    /// tenth of the high-poly object's size.
    pub occlusion_distance: Option<f32>,
}

impl Default for BakeSettings {
    fn default() -> Self {
        Self {
            size: 1024,
            cage: None,
            normal: true,
            occlusion: true,
            curvature: true,
            base_color: true,
            flip_green: false,
            occlusion_rays: 16,
            occlusion_distance: None,
        }
    }
}

impl BakeSettings {
    /// The cage distance used for `high`.
    pub fn cage_for(&self, high: &Reference) -> f32 {
        self.cage.unwrap_or(high.size() * CAGE_SHARE).max(1e-6)
    }

    /// The occlusion distance used for `high`.
    pub fn occlusion_distance_for(&self, high: &Reference) -> f32 {
        self.occlusion_distance
            .unwrap_or(high.size() * OCCLUSION_SHARE)
            .max(1e-6)
    }

    fn any(&self) -> bool {
        self.normal || self.occlusion || self.curvature || self.base_color
    }
}

/// The maps baked for one material slot of the low-poly object: RGBA,
/// `size` by `size`, for those asked for.
#[derive(Clone, Debug)]
pub struct BakedSlot {
    pub slot: u32,
    pub size: u32,
    pub normal: Option<Vec<u8>>,
    pub occlusion: Option<Vec<u8>>,
    pub curvature: Option<Vec<u8>>,
    pub base_color: Option<Vec<u8>>,
    /// Texels on the surface whose ray met nothing within the cage.
    pub missed: usize,
    /// Texels baked (the gutter's too).
    pub texels: usize,
}

/// Two uv islands of a slot whose uvs overlap: their numbers (as
/// [`uv::uv_islands`] lists them) and faces.
#[derive(Clone, Debug, PartialEq)]
pub struct Overlap {
    pub slot: u32,
    pub islands: [usize; 2],
    pub faces: [Vec<u32>; 2],
}

/// What [`ModelDoc::bake`] did.
#[derive(Clone, Debug, Default)]
pub struct BakeReport {
    /// Each map's project path and PNG bytes.
    pub files: Vec<(String, Vec<u8>)>,
    pub overlaps: Vec<Overlap>,
    pub missed: usize,
    pub texels: usize,
}

impl BakeReport {
    /// What went wrong enough to say: overlapping islands, and rays that
    /// missed.
    pub fn warnings(&self) -> Vec<String> {
        let mut warnings: Vec<String> = self
            .overlaps
            .iter()
            .map(|overlap| {
                format!(
                    "UV islands {} and {} overlap ({} and {} faces): they share texels, so one shows the other's bake. Unwrap or Pack Islands, then bake again.",
                    overlap.islands[0] + 1,
                    overlap.islands[1] + 1,
                    overlap.faces[0].len(),
                    overlap.faces[1].len()
                )
            })
            .collect();
        if self.texels > 0 && self.missed * 100 > self.texels {
            warnings.push(format!(
                "{}% of texels found no surface within the cage and took the nearest: a larger cage may fit better.",
                self.missed * 100 / self.texels
            ));
        }
        warnings
    }
}

/// What one texel came to.
struct Texel {
    texel: u32,
    normal: [u8; 4],
    occlusion: u8,
    curvature: u8,
    color: [u8; 4],
    missed: bool,
}

/// What every texel's bake reads.
struct Context<'a> {
    texels: &'a TexelMap,
    tangents: &'a [[f32; 4]],
    high: &'a Reference,
    curvature: &'a [f32],
    color: &'a (dyn Fn(u32, Vec2) -> [u8; 4] + Sync),
    settings: &'a BakeSettings,
    cage: f32,
    occlusion_distance: f32,
    /// Ambient occlusion's rays round +Z, spread like the light a matte
    /// surface takes in (more toward its normal).
    sky: Vec<Vec3>,
}

/// Bakes `high` onto the faces of `low` drawn with material `slot`, by
/// their first uvs. `color` is the high-poly base color (sRGB) at a uv
/// on its faces of a slot.
pub fn bake_slot(
    low: &EditMesh,
    slot: u32,
    high: &Reference,
    color: &(dyn Fn(u32, Vec2) -> [u8; 4] + Sync),
    settings: &BakeSettings,
) -> BakedSlot {
    let size = settings.size.max(1);
    let texels = TexelMap::build(low, slot, size);
    let tangents = corner_tangents(low, slot);
    let curvature = if settings.curvature {
        high.curvature()
    } else {
        Vec::new()
    };
    let rays = settings.occlusion_rays.max(1);
    let sky = (0..rays)
        .map(|i| {
            // Hammersley points, mapped onto the hemisphere by cosine.
            let u = (i as f32 + 0.5) / rays as f32;
            let angle = std::f32::consts::TAU * radical_inverse(i);
            let r = u.sqrt();
            Vec3::new(r * angle.cos(), r * angle.sin(), (1.0 - u).max(0.0).sqrt())
        })
        .collect();
    let context = Context {
        texels: &texels,
        tangents: &tangents,
        high,
        curvature: &curvature,
        color,
        settings,
        cage: settings.cage_for(high),
        occlusion_distance: settings.occlusion_distance_for(high),
        sky,
    };
    let jobs: Vec<(u32, u32, Vec3, Vec3)> = (0..texels.triangle_count() as u32)
        .flat_map(|t| {
            texels
                .texels_of(t)
                .map(move |(texel, place, normal)| (t, texel, place, normal))
        })
        .collect();
    // Each texel is worked out on its own, so threads split them; their
    // results are written in the jobs' order, so it's the same every time.
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get().min(16));
    let share = jobs.len().div_ceil(threads).max(1);
    let baked: Vec<Texel> = std::thread::scope(|scope| {
        let context = &context;
        let handles: Vec<_> = jobs
            .chunks(share)
            .map(|part| {
                scope.spawn(move || {
                    part.iter()
                        .filter_map(|&job| bake_texel(context, job))
                        .collect::<Vec<Texel>>()
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|handle| handle.join().expect("baking a texel doesn't panic"))
            .collect()
    });

    let count = texel_count(size);
    let mut normal = vec![[128, 128, 255, 255]; count];
    let mut occlusion = vec![255u8; count];
    let mut curvature_map = vec![128u8; count];
    let mut color_map: Vec<Option<[u8; 4]>> = vec![None; count];
    let mut missed = BTreeSet::new();
    let mut done = BTreeSet::new();
    for texel in &baked {
        let k = texel.texel as usize;
        normal[k] = texel.normal;
        occlusion[k] = texel.occlusion;
        curvature_map[k] = texel.curvature;
        color_map[k] = Some(texel.color);
        done.insert(texel.texel);
        if texel.missed {
            missed.insert(texel.texel);
        } else {
            missed.remove(&texel.texel);
        }
    }
    // Texels off the islands and their gutters take the islands' mean
    // color, so smaller mip levels don't blur a border color in.
    let mean = {
        let (sum, n) = color_map
            .iter()
            .flatten()
            .fold(([0u64; 4], 0u64), |(mut sum, n), c| {
                for (s, &v) in sum.iter_mut().zip(c) {
                    *s += u64::from(v);
                }
                (sum, n + 1)
            });
        sum.map(|s| s.checked_div(n).unwrap_or(204) as u8)
    };
    let grey =
        |values: Vec<u8>| -> Vec<u8> { values.iter().flat_map(|&v| [v, v, v, 255]).collect() };
    BakedSlot {
        slot,
        size,
        normal: settings
            .normal
            .then(|| normal.into_iter().flatten().collect()),
        occlusion: settings.occlusion.then(|| grey(occlusion)),
        curvature: settings.curvature.then(|| grey(curvature_map)),
        base_color: settings.base_color.then(|| {
            color_map
                .into_iter()
                .flat_map(|c| c.unwrap_or(mean))
                .collect()
        }),
        missed: missed.len(),
        texels: done.len(),
    }
}

/// One texel's bake: the job is its triangle, number, place and normal.
fn bake_texel(
    context: &Context,
    (t, texel, place, normal): (u32, u32, Vec3, Vec3),
) -> Option<Texel> {
    let high = context.high;
    let n = normal.try_normalize()?;
    let (corners, places) = context.texels.triangle(t);
    // The tangent frame here, as the GPU blends the corners' tangents.
    let raw = surface_weights(places, place).unwrap_or([1.0 / 3.0; 3]);
    // A gutter texel is past its triangle's edge, where a ray may well
    // find nothing; it isn't counted as a miss.
    let in_gutter = raw.iter().any(|&w| w < -1e-3);
    let weights = raw.map(|w| w.max(0.0));
    let total = weights.iter().sum::<f32>().max(1e-12);
    let (along, side) =
        corners
            .iter()
            .zip(weights)
            .fold((Vec3::ZERO, 0.0), |(along, side), (&corner, w)| {
                let tangent = context.tangents[corner as usize];
                (
                    along + Vec3::from_slice(&tangent[..3]) * (w / total),
                    side + tangent[3] * w,
                )
            });
    let u_dir = (along - n * n.dot(along))
        .try_normalize()
        .unwrap_or_else(|| n.any_orthonormal_vector());
    // The shader scales the up direction by the blended side, which is
    // short of ±1 inside a face whose corners' sides differ.
    let side = if side.abs() < 1e-3 {
        1e-3_f32.copysign(side)
    } else {
        side
    };
    let up = n.cross(u_dir);

    // From the cage back in to the high-poly surface.
    let found = high
        .ray(place + n * context.cage, -n)
        .filter(|&(_, distance)| distance <= context.cage * 2.0)
        .map(|(at, _)| at);
    let missed = found.is_none() && !in_gutter;
    let at = match found {
        Some(at) => at,
        None => high.nearest(place)?,
    };
    let surface = high.normal(&at);

    let settings = context.settings;
    let mut baked = Texel {
        texel,
        normal: [128, 128, 255, 255],
        occlusion: 255,
        curvature: 128,
        color: [0; 4],
        missed,
    };
    if settings.normal {
        let green = if settings.flip_green { -1.0 } else { 1.0 };
        let local = Vec3::new(
            surface.dot(u_dir),
            surface.dot(up) / side * green,
            surface.dot(n),
        );
        // The shader makes its normal one unit long again, so a value
        // past ±1 (from a short side) is scaled back with the others.
        let local = local / local.abs().max_element().max(1.0);
        baked.normal = [
            to_byte(local[0] * 0.5 + 0.5),
            to_byte(local[1] * 0.5 + 0.5),
            to_byte(local[2] * 0.5 + 0.5),
            255,
        ];
    }
    if settings.occlusion {
        let from = at.position + surface * (high.size() * 1e-4);
        let x = surface.any_orthonormal_vector();
        let y = surface.cross(x);
        // Each texel's rays turned by its own angle, so too few rays make
        // noise rather than bands.
        let turn = std::f32::consts::TAU * (hash(texel) as f32 / u32::MAX as f32);
        let (sin, cos) = turn.sin_cos();
        let open = context
            .sky
            .iter()
            .filter(|ray| {
                let (rx, ry) = (ray.x * cos - ray.y * sin, ray.x * sin + ray.y * cos);
                let direction = x * rx + y * ry + surface * ray.z;
                !high.occluded(from, direction, context.occlusion_distance)
            })
            .count();
        baked.occlusion = to_byte(open as f32 / context.sky.len() as f32);
    }
    if settings.curvature {
        let k = high.blend(context.curvature, &at);
        baked.curvature = to_byte(0.5 + 0.5 * (k * high.size() * CURVATURE_SHARE).tanh());
    }
    if settings.base_color {
        let (uv, slot) = high.uv(&at);
        baked.color = (context.color)(slot, uv);
    }
    Some(baked)
}

/// Each half-edge's tangent and side (`assets::tangents`), for the faces
/// of `slot`, with their corners joined as a saved `.glb` joins them (the
/// same vertex, uvs and normal), so the frames are the ones the game uses.
/// Other half-edges get a made-up one.
pub(crate) fn corner_tangents(mesh: &EditMesh, slot: u32) -> Vec<[f32; 4]> {
    let mut index_of: HashMap<(u32, [u32; 4], [u32; 3]), u32> = HashMap::new();
    let (mut positions, mut normals, mut uvs, mut indices) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut corner_index = vec![u32::MAX; mesh.half_edges().len()];
    for face in 0..mesh.face_count() as u32 {
        if mesh.face(face).material != slot {
            continue;
        }
        for triangle in mesh.triangles(face) {
            for corner in triangle {
                let edge = mesh.half_edge(corner);
                let second = if mesh.has_uv2() { edge.uv2 } else { Vec2::ZERO };
                let key = (
                    edge.vertex,
                    [edge.uv.x, edge.uv.y, second.x, second.y].map(f32::to_bits),
                    edge.normal.to_array().map(f32::to_bits),
                );
                let index = *index_of.entry(key).or_insert_with(|| {
                    positions.push(mesh.position(edge.vertex).to_array());
                    normals.push(edge.normal.to_array());
                    uvs.push(edge.uv.to_array());
                    (positions.len() - 1) as u32
                });
                indices.push(index);
                corner_index[corner as usize] = index;
            }
        }
    }
    let tangents = axcenity_assets::tangents(&positions, &normals, &uvs, &indices);
    corner_index
        .iter()
        .map(|&k| {
            tangents
                .get(k as usize)
                .copied()
                .unwrap_or([1.0, 0.0, 0.0, 1.0])
        })
        .collect()
}

/// `i`'s bits reversed after the point: 0.5, 0.25, 0.75, … (the van der
/// Corput sequence).
fn radical_inverse(i: u32) -> f32 {
    i.reverse_bits() as f32 / 4_294_967_296.0
}

/// A well-mixed number from `x` (a texel's own angle).
fn hash(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^ (x >> 16)
}

/// Where an object's base color comes from, for one material slot.
enum ColorSource {
    Flat([u8; 4]),
    /// sRGB texels, and the material's factor (linear) times them.
    Map {
        pixels: Arc<Vec<u8>>,
        width: u32,
        height: u32,
        factor: [f32; 4],
    },
}

impl ColorSource {
    /// Each slot's: its paint's base color map, or else its material's
    /// texture, or else its color.
    fn of(object: &DocObject) -> Vec<ColorSource> {
        object
            .materials
            .iter()
            .enumerate()
            .map(|(slot, material)| {
                let painted = object
                    .paint
                    .iter()
                    .find(|set| set.slot == slot as u32)
                    .and_then(|set| set.map(MapKind::BaseColor));
                if let Some(map) = painted {
                    ColorSource::Map {
                        pixels: map.flat_shared(),
                        width: map.size,
                        height: map.size,
                        factor: [1.0; 4],
                    }
                } else if let Some(image) = &material.base_color_texture {
                    ColorSource::Map {
                        pixels: Arc::new(image.rgba.clone()),
                        width: image.width,
                        height: image.height,
                        factor: material.base_color,
                    }
                } else {
                    flat(material.base_color)
                }
            })
            .collect()
    }

    fn at(&self, uv: Vec2) -> [u8; 4] {
        match self {
            ColorSource::Flat(color) => *color,
            ColorSource::Map {
                pixels,
                width,
                height,
                factor,
            } => {
                let texel = sample(pixels, *width, *height, uv, true);
                let byte = |v: f32| v.round().clamp(0.0, 255.0) as u8;
                if *factor == [1.0; 4] {
                    return texel.map(byte);
                }
                let mut color = [0; 4];
                for c in 0..3 {
                    color[c] = to_srgb(from_srgb(byte(texel[c])) * factor[c]);
                }
                color[3] = to_byte(texel[3] / 255.0 * factor[3]);
                color
            }
        }
    }
}

/// A linear color as sRGB bytes.
fn flat(color: [f32; 4]) -> ColorSource {
    ColorSource::Flat([
        to_srgb(color[0]),
        to_srgb(color[1]),
        to_srgb(color[2]),
        to_byte(color[3]),
    ])
}

impl ModelDoc {
    /// Bakes object `high` onto object `low`'s uvs (each of its material
    /// slots), both with their modifiers on as saved. Writes the maps
    /// asked for as PNGs named by role in `textures/` beside the model at
    /// `model` (a project path): `<model>_<object>_<material>_Normal.png`
    /// (`_NormalDX` for DirectX's), `_Occlusion`, `_Curvature` and
    /// `_BaseColor`, and points `low`'s materials at the normal, occlusion
    /// and base color maps. Returns the files and what went wrong.
    pub fn bake(
        &mut self,
        model: &str,
        low: usize,
        high: usize,
        settings: &BakeSettings,
    ) -> Result<BakeReport, String> {
        if low == high {
            return Err("Bake onto another object than the reference".to_owned());
        }
        if !settings.any() {
            return Err("Pick a map to bake".to_owned());
        }
        let object = |index: usize| {
            self.objects
                .get(index)
                .filter(|object| object.mesh.is_some())
                .ok_or_else(|| "Pick an object with a mesh".to_owned())
        };
        let (high_object, low_object) = (object(high)?, object(low)?);
        let reference = Reference::new(&high_object.evaluated(Purpose::Export).expect("a mesh"));
        if reference.triangle_count() == 0 {
            return Err(format!("{} has no faces to bake", high_object.name));
        }
        if !low_object.uvs {
            return Err(format!(
                "{} has no uvs to bake onto: unwrap it (U) first",
                low_object.name
            ));
        }
        let mesh = low_object
            .evaluated(Purpose::Export)
            .expect("a mesh")
            .into_owned();
        let sources = ColorSource::of(high_object);
        let plain = flat(ModelMaterial::default().base_color);
        let color = |slot: u32, uv: Vec2| sources.get(slot as usize).unwrap_or(&plain).at(uv);

        let (folder, file) = model.rsplit_once('/').unwrap_or(("", model));
        let stem = file.rsplit_once('.').map_or(file, |(stem, _)| stem);
        let object_name = low_object.name.clone();
        let low_object = &mut self.objects[low];
        if low_object.materials.is_empty() {
            low_object.materials.push(ModelMaterial {
                name: "Material".to_owned(),
                ..ModelMaterial::default()
            });
        }
        let slots: BTreeSet<u32> = (0..mesh.face_count() as u32)
            .map(|face| mesh.face(face).material)
            .filter(|&slot| (slot as usize) < low_object.materials.len())
            .collect();
        let mut report = BakeReport::default();
        let mut names = BTreeSet::new();
        for slot in slots {
            let faces: Vec<u32> = (0..mesh.face_count() as u32)
                .filter(|&face| mesh.face(face).material == slot)
                .collect();
            let islands = uv::uv_islands(&mesh, &faces, 0);
            report
                .overlaps
                .extend(
                    uv::overlapping_islands(&mesh, &faces, 0)
                        .into_iter()
                        .map(|[a, b]| Overlap {
                            slot,
                            islands: [a, b],
                            faces: [islands[a].clone(), islands[b].clone()],
                        }),
                );
            let baked = bake_slot(&mesh, slot, &reference, &color, settings);
            report.missed += baked.missed;
            report.texels += baked.texels;

            let material = &mut low_object.materials[slot as usize];
            let mut name = map_stem(stem, &object_name, low, material, slot);
            if !names.insert(name.clone()) {
                name = format!("{name}_{slot}");
                names.insert(name.clone());
            }
            let size = baked.size;
            let mut add = |role: &str, pixels: Vec<u8>, png: Png| -> Arc<Image> {
                let relative = format!("{TEXTURES_FOLDER}/{name}_{role}.png");
                let path = match folder {
                    "" => relative.clone(),
                    _ => format!("{folder}/{relative}"),
                };
                report.files.push((path, encode_png(size, &pixels, png)));
                let mut image = Image::new(size, size, pixels);
                image.file = Some(relative);
                Arc::new(image)
            };
            if let Some(pixels) = baked.base_color {
                material.base_color = [1.0; 4];
                material.base_color_texture = Some(add("BaseColor", pixels, Png::Rgba));
            }
            if let Some(pixels) = baked.normal {
                let role = if settings.flip_green {
                    "NormalDX"
                } else {
                    "Normal"
                };
                material.normal_texture = Some(add(role, pixels, Png::Rgba));
                material.normal_flip_green = settings.flip_green;
                material.normal_scale = 1.0;
            }
            if let Some(pixels) = baked.occlusion {
                let image = add("Occlusion", pixels, Png::Grey);
                material.occlusion_texture = Some(ChannelTexture::new(image, Channel::Red));
                material.occlusion_strength = 1.0;
            }
            if let Some(pixels) = baked.curvature {
                // Not a map materials draw: for painting with, or masks.
                add("Curvature", pixels, Png::Grey);
            }
            // The maps are on the mesh's own uvs.
            material.uv_scale = [1.0, 1.0];
            material.uv_offset = [0.0, 0.0];
            material.uv_rotation = 0.0;
        }
        Ok(report)
    }
}
