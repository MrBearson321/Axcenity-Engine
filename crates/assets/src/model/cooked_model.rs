//! Whole cooked models (roadmap L2): a project's model kept, once loaded,
//! as a file in `.axcenity/imported/models/` that the next load reads
//! instead of parsing the glTF or FBX file: its fitted triangles, simpler
//! copies, materials and skeleton, with each texture named by where it
//! came from in the project (loaded again as `Assets::texture` would, from
//! its cooked copy when it has one).
//!
//! A file is named for the model file and its pick of objects, and is
//! used only while it's current: stamped with the model file, and made by
//! this build's loading code (a hash of the `assets` crate's source and
//! the lockfile, from `build.rs`), so a fixed importer never meets a model
//! an older one cooked. A model that loaded with warnings, or with a
//! texture that has no file in the project (embedded and not cooked yet),
//! isn't kept: loading it again gives the full answer.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use glam::{Mat4, Quat, Vec3};

use super::stamped::Cache;
use super::{
    AlphaMode, Bone, BoneTransform, Channel, ChannelTexture, Image, ImageOrigin, Joint, Model,
    ModelMaterial, Primitive, Rig, new_key,
};
use crate::cooked::{self, SourceStamp, content_hash};

/// Where cooked models go, inside the project folder.
pub const MODEL_CACHE_FOLDER: &str = ".axcenity/imported/models";

const CACHE: Cache = Cache {
    folder: MODEL_CACHE_FOLDER,
    extension: "axmodel",
    magic: b"AXMODEL\x01",
};

/// The loading code this build has (see the module's notes).
fn build() -> u64 {
    u64::from_str_radix(env!("AXCENITY_ASSETS_SOURCE_HASH"), 16).unwrap_or(0)
}

fn path(project: &Path, relative: &str, pick: &str) -> std::path::PathBuf {
    CACHE.path(
        project,
        relative,
        &format!("{:016x}", content_hash(pick.as_bytes())),
    )
}

/// The cooked copy of the project model `relative` with objects `pick`
/// (what follows `#` in its MeshId, or ""), if it has a current one.
pub(crate) fn read(project: &Path, relative: &str, pick: &str) -> Option<Model> {
    let bytes = fs::read(path(project, relative, pick)).ok()?;
    let stamp = CACHE.stamp(&bytes)?;
    if !cooked::is_current(project, relative, stamp) {
        return None;
    }
    let mut input = Input {
        bytes: &bytes,
        at: CACHE.header_len(),
    };
    if input.u64()? != build() {
        return None;
    }
    decode(&mut input, project, relative)
}

/// Whether the project model `relative` with objects `pick` has a current
/// cooked copy, reading only the start of it: when it hasn't, loading it
/// imports the model file (slow for a big one) and cooks it.
pub(crate) fn is_cooked(project: &Path, relative: &str, pick: &str) -> bool {
    use std::io::Read;
    let mut start = vec![0u8; CACHE.header_len() + 8];
    let read = fs::File::open(path(project, relative, pick))
        .and_then(|mut file| file.read_exact(&mut start));
    if read.is_err() {
        return false;
    }
    let build_at = CACHE.header_len();
    let made_by = u64::from_le_bytes(
        start[build_at..build_at + 8]
            .try_into()
            .expect("eight bytes were read"),
    );
    CACHE
        .stamp(&start)
        .is_some_and(|stamp| cooked::is_current(project, relative, stamp))
        && made_by == build()
}

/// Keeps `model`, loaded from the project model `relative` (`bytes`) with
/// objects `pick`, if it can be loaded again from what's kept, and deletes
/// the model's cooked files made from older versions of it. Failing (a
/// read-only folder) only means the next load parses the file again.
pub(crate) fn write(project: &Path, relative: &str, pick: &str, bytes: &[u8], model: &Model) {
    let Some(body) = encode(model) else {
        return;
    };
    let stamp = SourceStamp::of(&project.join(relative), bytes);
    let mut out = CACHE.header(stamp);
    out.extend_from_slice(&build().to_le_bytes());
    out.extend_from_slice(&body);
    let path = path(project, relative, pick);
    if cooked::write_whole(&path, &out).is_ok() {
        CACHE.clear_stale(project, relative, &path);
    }
}

/// The cooked models of the project model `relative` made from it as it
/// is now, as project paths with `/` between folders: what Build Game
/// ships with it.
pub fn cooked_models(project: &Path, relative: &str) -> Vec<String> {
    CACHE.current(project, relative)
}

/// `model`'s bytes, or `None` if it can't be loaded again from them.
fn encode(model: &Model) -> Option<Vec<u8>> {
    // Every field is named, so a new one can't be left out by mistake.
    let Model {
        key: _,
        size,
        center,
        primitives,
        hull_points,
        collision,
        warnings,
        rig,
    } = model;
    if !warnings.is_empty() {
        return None;
    }
    let mut out = Output::default();
    let mut images = Images::default();
    // Images first, so reading makes them before the materials name them.
    let mut body = Output::default();
    body.floats(size);
    body.floats(center);
    body.array(hull_points);
    body.u32(collision.len() as u32);
    for piece in collision {
        body.array(piece);
    }
    body.u32(primitives.len() as u32);
    for primitive in primitives {
        encode_primitive(&mut body, &mut images, primitive)?;
    }
    match rig {
        Some(rig) => {
            body.u8(1);
            encode_rig(&mut body, rig);
        }
        None => body.u8(0),
    }
    out.u32(images.list.len() as u32);
    for image in &images.list {
        let Image { file, origin, .. } = &**image;
        match origin {
            ImageOrigin::Unknown => return None,
            ImageOrigin::File(path) => {
                out.u8(1);
                out.text(path);
            }
            ImageOrigin::CookedEmbedded { path, hash } => {
                out.u8(2);
                out.text(path);
                out.u64(*hash);
            }
            ImageOrigin::InModel { hash } => {
                out.u8(3);
                out.u64(*hash);
            }
        }
        out.optional_text(file.as_deref());
    }
    out.0.extend_from_slice(&body.0);
    Some(out.0)
}

fn encode_primitive(out: &mut Output, images: &mut Images, primitive: &Primitive) -> Option<()> {
    let Primitive {
        key: _,
        positions,
        normals,
        uvs,
        tangents,
        indices,
        material,
        joints,
        weights,
        lods,
        level,
    } = primitive;
    out.array(positions);
    out.array(normals);
    out.array(uvs);
    out.array(tangents);
    out.u32s(indices);
    out.u32(joints.len() as u32);
    for joint in joints {
        for part in joint {
            out.0.extend_from_slice(&part.to_le_bytes());
        }
    }
    out.array(weights);
    encode_material(out, images, material)?;
    out.u8(*level);
    out.u32(lods.len() as u32);
    for copy in lods {
        encode_primitive(out, images, copy)?;
    }
    Some(())
}

fn encode_material(out: &mut Output, images: &mut Images, material: &ModelMaterial) -> Option<()> {
    let ModelMaterial {
        name,
        base_color,
        metalness,
        roughness,
        emissive,
        alpha_mode,
        base_color_texture,
        normal_texture,
        normal_scale,
        normal_flip_green,
        metalness_texture,
        roughness_texture,
        occlusion_texture,
        occlusion_strength,
        emissive_texture,
        opacity_texture,
        specular_texture,
        uv_scale,
        uv_rotation,
        uv_offset,
        double_sided,
        sway,
        flutter,
        translucency,
    } = material;
    out.text(name);
    out.floats(base_color);
    out.floats(&[*metalness, *roughness]);
    out.floats(emissive);
    match alpha_mode {
        AlphaMode::Opaque => out.u8(0),
        AlphaMode::Mask { cutoff } => {
            out.u8(1);
            out.floats(&[*cutoff]);
        }
        AlphaMode::Blend => out.u8(2),
    }
    for image in [base_color_texture, normal_texture] {
        images.write(out, image.as_ref());
    }
    out.floats(&[*normal_scale]);
    out.u8(u8::from(*normal_flip_green));
    for texture in [metalness_texture, roughness_texture, occlusion_texture] {
        images.write_channel(out, texture.as_ref());
    }
    out.floats(&[*occlusion_strength]);
    images.write(out, emissive_texture.as_ref());
    images.write_channel(out, opacity_texture.as_ref());
    images.write(out, specular_texture.as_ref());
    out.floats(uv_scale);
    out.floats(&[*uv_rotation]);
    out.floats(uv_offset);
    out.u8(u8::from(*double_sided));
    out.floats(&[*sway, *flutter, *translucency]);
    Some(())
}

fn encode_rig(out: &mut Output, rig: &Rig) {
    let Rig { bones, joints, fit } = rig;
    out.u32(bones.len() as u32);
    for bone in bones {
        let Bone { name, parent, rest } = bone;
        out.text(name);
        out.u32(parent.map_or(u32::MAX, |parent| parent as u32));
        encode_transform(out, rest);
    }
    out.u32(joints.len() as u32);
    for joint in joints {
        let Joint { bone, inverse_bind } = joint;
        out.u32(*bone as u32);
        out.floats(&inverse_bind.to_cols_array());
    }
    out.floats(&fit.to_cols_array());
}

fn encode_transform(out: &mut Output, transform: &BoneTransform) {
    let BoneTransform {
        translation,
        rotation,
        scale,
    } = transform;
    out.floats(&translation.to_array());
    out.floats(&rotation.to_array());
    out.floats(&scale.to_array());
}

/// The images a model uses, each once, by where it is in memory.
#[derive(Default)]
struct Images {
    list: Vec<Arc<Image>>,
    index: HashMap<*const Image, u32>,
}

impl Images {
    /// Writes which image `image` is (0 for none, else its index + 1).
    fn write(&mut self, out: &mut Output, image: Option<&Arc<Image>>) {
        let Some(image) = image else {
            out.u32(0);
            return;
        };
        let next = self.list.len() as u32;
        let index = *self.index.entry(Arc::as_ptr(image)).or_insert_with(|| {
            self.list.push(image.clone());
            next
        });
        out.u32(index + 1);
    }

    fn write_channel(&mut self, out: &mut Output, texture: Option<&ChannelTexture>) {
        self.write(out, texture.map(|texture| &texture.image));
        if let Some(ChannelTexture {
            image: _,
            channel,
            invert,
        }) = texture
        {
            out.u8(match channel {
                Channel::Red => 0,
                Channel::Green => 1,
                Channel::Blue => 2,
                Channel::Alpha => 3,
            });
            out.u8(u8::from(*invert));
        }
    }
}

/// Little-endian bytes being written.
#[derive(Default)]
struct Output(Vec<u8>);

impl Output {
    fn u8(&mut self, value: u8) {
        self.0.push(value);
    }

    fn u32(&mut self, value: u32) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }

    fn u64(&mut self, value: u64) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }

    fn floats(&mut self, values: &[f32]) {
        for value in values {
            self.0.extend_from_slice(&value.to_le_bytes());
        }
    }

    /// A count, then each item's floats.
    fn array<const N: usize>(&mut self, items: &[[f32; N]]) {
        self.u32(items.len() as u32);
        self.floats(items.as_flattened());
    }

    fn u32s(&mut self, values: &[u32]) {
        self.u32(values.len() as u32);
        for value in values {
            self.0.extend_from_slice(&value.to_le_bytes());
        }
    }

    fn text(&mut self, text: &str) {
        self.u32(text.len() as u32);
        self.0.extend_from_slice(text.as_bytes());
    }

    fn optional_text(&mut self, text: Option<&str>) {
        match text {
            Some(text) => {
                self.u8(1);
                self.text(text);
            }
            None => self.u8(0),
        }
    }
}

/// Bytes being read; every read is `None` past the end.
struct Input<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Input<'_> {
    fn take(&mut self, count: usize) -> Option<&[u8]> {
        let part = self.bytes.get(self.at..self.at.checked_add(count)?)?;
        self.at += count;
        Some(part)
    }

    fn u8(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }

    fn bool(&mut self) -> Option<bool> {
        match self.u8()? {
            0 => Some(false),
            1 => Some(true),
            _ => None,
        }
    }

    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }

    fn u64(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }

    fn f32(&mut self) -> Option<f32> {
        Some(f32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }

    fn floats<const N: usize>(&mut self) -> Option<[f32; N]> {
        let mut values = [0.0; N];
        for value in &mut values {
            *value = self.f32()?;
        }
        Some(values)
    }

    /// A count of items, checked against the bytes left so a damaged
    /// count can't ask for more memory than the file holds.
    fn count(&mut self, item_bytes: usize) -> Option<usize> {
        let count = self.u32()? as usize;
        (count.checked_mul(item_bytes.max(1))? <= self.bytes.len() - self.at).then_some(count)
    }

    fn array<const N: usize>(&mut self) -> Option<Vec<[f32; N]>> {
        let count = self.count(N * 4)?;
        let bytes = self.take(count * N * 4)?;
        Some(
            bytes
                .as_chunks::<4>()
                .0
                .as_chunks::<N>()
                .0
                .iter()
                .map(|item| item.map(f32::from_le_bytes))
                .collect(),
        )
    }

    fn u32s(&mut self) -> Option<Vec<u32>> {
        let count = self.count(4)?;
        let bytes = self.take(count * 4)?;
        Some(
            bytes
                .as_chunks::<4>()
                .0
                .iter()
                .map(|value| u32::from_le_bytes(*value))
                .collect(),
        )
    }

    fn text(&mut self) -> Option<String> {
        let len = self.count(1)?;
        String::from_utf8(self.take(len)?.to_vec()).ok()
    }

    fn optional_text(&mut self) -> Option<Option<String>> {
        match self.bool()? {
            true => Some(Some(self.text()?)),
            false => Some(None),
        }
    }
}

fn decode(input: &mut Input<'_>, project: &Path, relative: &str) -> Option<Model> {
    let image_count = input.count(6)?;
    let mut images = Vec::with_capacity(image_count);
    for _ in 0..image_count {
        let mut image = match input.u8()? {
            1 => crate::read_texture(project, &input.text()?).ok()?,
            2 => {
                let path = input.text()?;
                cooked::cooked_embedded_by_hash(project, &path, input.u64()?)?
            }
            3 => {
                // Shared with every other model using the same copy.
                let hash = input.u64()?;
                let file = input.optional_text()?;
                images.push(cooked::cooked_in_model_by_hash(
                    project, relative, hash, file,
                )?);
                continue;
            }
            _ => return None,
        };
        image.file = input.optional_text()?;
        images.push(Arc::new(image));
    }
    let size = input.floats()?;
    let center = input.floats()?;
    let hull_points = input.array()?;
    // Each piece is at least its count.
    let pieces = input.count(4)?;
    let collision = (0..pieces)
        .map(|_| input.array())
        .collect::<Option<Vec<_>>>()?;
    let count = input.count(1)?;
    let mut primitives = Vec::with_capacity(count);
    for _ in 0..count {
        primitives.push(decode_primitive(input, &images)?);
    }
    let rig = match input.u8()? {
        0 => None,
        1 => Some(Arc::new(decode_rig(input)?)),
        _ => return None,
    };
    if input.at != input.bytes.len()
        || !primitives
            .iter()
            .flat_map(|primitive| std::iter::once(primitive).chain(&primitive.lods))
            .all(whole)
    {
        return None;
    }
    Some(Model {
        key: new_key(),
        size,
        center,
        primitives,
        hull_points,
        collision,
        warnings: Vec::new(),
        rig,
    })
}

/// Whether `primitive`'s lists fit together as a loaded one's do: one of
/// each per corner, or none. (Posing skips joints the rig hasn't got.)
fn whole(primitive: &Primitive) -> bool {
    let corners = primitive.positions.len();
    let fits = |len: usize| len == 0 || len == corners;
    fits(primitive.normals.len())
        && fits(primitive.uvs.len())
        && fits(primitive.tangents.len())
        && fits(primitive.joints.len())
        && primitive.weights.len() == primitive.joints.len()
}

fn decode_primitive(input: &mut Input<'_>, images: &[Arc<Image>]) -> Option<Primitive> {
    let positions = input.array()?;
    let normals = input.array()?;
    let uvs = input.array()?;
    let tangents = input.array()?;
    let indices = input.u32s()?;
    let corners = positions.len();
    if indices.iter().any(|&index| index as usize >= corners) {
        return None;
    }
    let joint_count = input.count(8)?;
    let mut joints = Vec::with_capacity(joint_count);
    for _ in 0..joint_count {
        let bytes = input.take(8)?;
        let mut joint = [0u16; 4];
        for (part, pair) in joint.iter_mut().zip(bytes.as_chunks::<2>().0) {
            *part = u16::from_le_bytes(*pair);
        }
        joints.push(joint);
    }
    let weights = input.array()?;
    let material = decode_material(input, images)?;
    let level = input.u8()?;
    let lod_count = input.count(1)?;
    let mut lods = Vec::with_capacity(lod_count);
    for _ in 0..lod_count {
        lods.push(decode_primitive(input, images)?);
    }
    Some(Primitive {
        key: new_key(),
        positions,
        normals,
        uvs,
        tangents,
        indices,
        material,
        joints,
        weights,
        lods,
        level,
    })
}

fn decode_material(input: &mut Input<'_>, images: &[Arc<Image>]) -> Option<ModelMaterial> {
    let image = |input: &mut Input<'_>| -> Option<Option<Arc<Image>>> {
        match input.u32()? {
            0 => Some(None),
            index => Some(Some(images.get(index as usize - 1)?.clone())),
        }
    };
    let channel = |input: &mut Input<'_>| -> Option<Option<ChannelTexture>> {
        let Some(image) = image(input)? else {
            return Some(None);
        };
        let channel = match input.u8()? {
            0 => Channel::Red,
            1 => Channel::Green,
            2 => Channel::Blue,
            3 => Channel::Alpha,
            _ => return None,
        };
        Some(Some(ChannelTexture {
            image,
            channel,
            invert: input.bool()?,
        }))
    };
    let name = input.text()?;
    let base_color = input.floats()?;
    let [metalness, roughness] = input.floats()?;
    let emissive = input.floats()?;
    let alpha_mode = match input.u8()? {
        0 => AlphaMode::Opaque,
        1 => AlphaMode::Mask {
            cutoff: input.f32()?,
        },
        2 => AlphaMode::Blend,
        _ => return None,
    };
    let base_color_texture = image(input)?;
    let normal_texture = image(input)?;
    let normal_scale = input.f32()?;
    let normal_flip_green = input.bool()?;
    let metalness_texture = channel(input)?;
    let roughness_texture = channel(input)?;
    let occlusion_texture = channel(input)?;
    let occlusion_strength = input.f32()?;
    let emissive_texture = image(input)?;
    let opacity_texture = channel(input)?;
    let specular_texture = image(input)?;
    let uv_scale = input.floats()?;
    let uv_rotation = input.f32()?;
    let uv_offset = input.floats()?;
    let double_sided = input.bool()?;
    let [sway, flutter, translucency] = input.floats()?;
    Some(ModelMaterial {
        name,
        base_color,
        metalness,
        roughness,
        emissive,
        alpha_mode,
        base_color_texture,
        normal_texture,
        normal_scale,
        normal_flip_green,
        metalness_texture,
        roughness_texture,
        occlusion_texture,
        occlusion_strength,
        emissive_texture,
        opacity_texture,
        specular_texture,
        uv_scale,
        uv_rotation,
        uv_offset,
        double_sided,
        sway,
        flutter,
        translucency,
    })
}

fn decode_rig(input: &mut Input<'_>) -> Option<Rig> {
    let bone_count = input.count(4 + 4 + 40)?;
    let mut bones = Vec::with_capacity(bone_count);
    for index in 0..bone_count {
        let name = input.text()?;
        let parent = match input.u32()? {
            u32::MAX => None,
            // Parents come before their children.
            parent if (parent as usize) < index => Some(parent as usize),
            _ => return None,
        };
        bones.push(Bone {
            name,
            parent,
            rest: decode_transform(input)?,
        });
    }
    let joint_count = input.count(4 + 64)?;
    let mut joints = Vec::with_capacity(joint_count);
    for _ in 0..joint_count {
        let bone = input.u32()? as usize;
        if bone >= bones.len() {
            return None;
        }
        joints.push(Joint {
            bone,
            inverse_bind: Mat4::from_cols_array(&input.floats()?),
        });
    }
    let fit = Mat4::from_cols_array(&input.floats()?);
    Some(Rig { bones, joints, fit })
}

fn decode_transform(input: &mut Input<'_>) -> Option<BoneTransform> {
    Some(BoneTransform {
        translation: Vec3::from_array(input.floats()?),
        rotation: Quat::from_array(input.floats()?),
        scale: Vec3::from_array(input.floats()?),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Assets;
    use std::path::PathBuf;

    fn copy(from: &Path, to: &Path) {
        fs::create_dir_all(to).unwrap();
        for entry in fs::read_dir(from).unwrap().flatten() {
            let target = to.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy(&entry.path(), &target);
            } else {
                fs::copy(entry.path(), target).unwrap();
            }
        }
    }

    /// A project with some of the examples' models (and their textures).
    fn project(name: &str) -> PathBuf {
        let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
        let project = std::env::temp_dir().join(format!(
            "axcenity-cooked-model-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&project);
        for (example, folder) in [
            ("hello-pbr", "assets/models"),
            ("hello-animation", "assets/models"),
            ("hello-mesh", "assets"),
            ("hello-roads", "assets/models"),
        ] {
            copy(&examples.join(example).join(folder), &project.join(folder));
        }
        // DamagedHelmet's embedded maps aren't cooked here.
        fs::remove_dir_all(project.join("assets/models/DamagedHelmet")).unwrap();
        project
    }

    const MODELS: [&str; 6] = [
        "assets/models/pbr_panel.fbx",
        "assets/models/phong_panel.fbx",
        "assets/models/noodle.fbx",
        "assets/models/CityPropsCollection.fbx",
        "assets/models/tree.glb",
        "assets/models/rock.glb",
    ];

    /// Everything about a model that loading decides, as text, with each
    /// image named by its file and size.
    fn describe(model: &Model) -> String {
        fn image(image: &Option<Arc<Image>>) -> String {
            image.as_ref().map_or("-".into(), |image| {
                format!(
                    "{:?} {:?} {}x{}",
                    image.file, image.origin, image.width, image.height
                )
            })
        }
        fn channel(texture: &Option<ChannelTexture>) -> String {
            texture.as_ref().map_or("-".into(), |texture| {
                format!(
                    "{} {:?} {}",
                    image(&Some(texture.image.clone())),
                    texture.channel,
                    texture.invert
                )
            })
        }
        fn describe_primitive(out: &mut String, primitive: &Primitive) {
            let material = &primitive.material;
            out.push_str(&format!(
                "{:?} {:?} {:?} {:?} {:?} {:?} {:?} {} | {} {:?} {:?} {} {} {:?} {} {} {} {} {} {} {:?} {} {:?} {} {} {} {}\n",
                primitive.positions,
                primitive.normals,
                primitive.uvs,
                primitive.tangents,
                primitive.indices,
                primitive.joints,
                primitive.weights,
                primitive.level,
                material.name,
                (material.base_color, material.metalness, material.roughness, material.emissive),
                material.alpha_mode,
                image(&material.base_color_texture),
                image(&material.normal_texture),
                (material.normal_scale, material.normal_flip_green),
                channel(&material.metalness_texture),
                channel(&material.roughness_texture),
                channel(&material.occlusion_texture),
                material.occlusion_strength,
                image(&material.emissive_texture),
                channel(&material.opacity_texture),
                (material.uv_scale, material.uv_rotation, material.uv_offset),
                image(&material.specular_texture),
                (material.double_sided, material.sway),
                material.flutter,
                material.translucency,
                primitive.lods.len(),
                "",
            ));
            for copy in &primitive.lods {
                describe_primitive(out, copy);
            }
        }
        let mut out = format!(
            "{:?} {:?} {:?} {:?} {:?} {:?}\n",
            model.size, model.center, model.hull_points, model.collision, model.warnings, model.rig
        );
        for each in &model.primitives {
            describe_primitive(&mut out, each);
        }
        out
    }

    #[test]
    fn a_cooked_model_is_the_model_its_file_loads_as() {
        let project = project("same");
        let fresh = Assets::new(&project);
        let mut loaded = Vec::new();
        for model in MODELS {
            let first = fresh.model_blocking(model).unwrap_or_else(|| {
                panic!("{model}: {:?}", fresh.take_errors());
            });
            loaded.push((model, first));
        }
        let kept: Vec<&&str> = MODELS
            .iter()
            .filter(|model| !cooked_models(&project, model).is_empty())
            .collect();
        // Every one of these loads cleanly, so every one was kept.
        assert_eq!(kept.len(), MODELS.len(), "{kept:?}");

        let cooked = Assets::new(&project);
        for (model, first) in &loaded {
            assert!(read(&project, model, "").is_some(), "{model}");
            let again = cooked.model_blocking(model).unwrap();
            assert_eq!(describe(&again), describe(first), "{model}");
            assert_ne!(again.key(), first.key());
        }
        let _ = fs::remove_dir_all(project);
    }

    #[test]
    fn a_cooked_model_keeps_its_collision_pieces() {
        let project = std::env::temp_dir().join(format!(
            "axcenity-cooked-model-pieces-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&project);
        let folder = project.join("assets/models");
        let gltf = crate::model::tests::rock_with_pieces(&folder);
        fs::write(folder.join("rock.gltf"), gltf).unwrap();
        let model = "assets/models/rock.gltf";
        let first = Assets::new(&project).model_blocking(model).unwrap();
        assert_eq!(first.collision.len(), 2);
        assert!(read(&project, model, "").is_some());
        let again = Assets::new(&project).model_blocking(model).unwrap();
        assert_eq!(describe(&again), describe(&first));
        assert_eq!(again.collision, first.collision);
        let _ = fs::remove_dir_all(project);
    }

    #[test]
    fn only_a_current_whole_model_is_read_and_each_pick_has_its_own() {
        let project = project("current");
        let model = "assets/models/CityPropsCollection.fbx";
        let assets = Assets::new(&project);
        let whole = assets.model_blocking(model).unwrap();
        let objects = crate::list_objects(
            &fs::read(project.join(model)).unwrap(),
            crate::ModelFormat::of(Path::new(model)).unwrap(),
        )
        .unwrap();
        let pick = format!("{model}#{}", objects[0].path);
        let picked = assets.model_blocking(&pick).unwrap();
        assert_ne!(picked.primitives.len(), whole.primitives.len());
        assert_eq!(cooked_models(&project, model).len(), 2);
        let objects_text = &pick[model.len() + 1..];
        assert_eq!(
            read(&project, model, objects_text)
                .unwrap()
                .primitives
                .len(),
            picked.primitives.len()
        );

        // A damaged file isn't read, and the next load writes it again.
        let file = path(&project, model, "");
        let bytes = fs::read(&file).unwrap();
        fs::write(&file, &bytes[..bytes.len() / 2]).unwrap();
        assert!(read(&project, model, "").is_none());
        Assets::new(&project).model_blocking(model).unwrap();
        assert!(read(&project, model, "").is_some());

        // Nor is one made by other loading code.
        let mut other = bytes.clone();
        other[CACHE.header_len()] ^= 1;
        fs::write(&file, &other).unwrap();
        assert!(read(&project, model, "").is_none());

        // Once the model file changes, its cooked files are stale: not
        // read, not shipped, and cleared by the next write.
        let mut source = fs::read(project.join(model)).unwrap();
        source.extend_from_slice(b"  ");
        fs::write(project.join(model), &source).unwrap();
        assert!(read(&project, model, objects_text).is_none());
        assert!(cooked_models(&project, model).is_empty());
        Assets::new(&project).model_blocking(model).unwrap();
        assert_eq!(cooked_models(&project, model).len(), 1);
        let left = fs::read_dir(project.join(MODEL_CACHE_FOLDER))
            .unwrap()
            .count();
        let others = MODELS.len() - 1;
        assert!(left <= 1 + others, "{left}");
        let _ = fs::remove_dir_all(project);
    }

    #[test]
    fn a_model_with_warnings_or_uncooked_embedded_maps_isnt_kept() {
        let project = project("not-kept");
        // A texture it names is missing.
        fs::remove_file(project.join("assets/models/maps/normal.png")).unwrap();
        let model = "assets/models/pbr_panel.fbx";
        let assets = Assets::new(&project);
        assert!(!assets.model_blocking(model).unwrap().warnings.is_empty());
        assert!(cooked_models(&project, model).is_empty());

        let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
        let helmet = "assets/models/DamagedHelmet/DamagedHelmet.glb";
        copy(
            &examples.join("hello-pbr/assets/models/DamagedHelmet"),
            &project.join("assets/models/DamagedHelmet"),
        );
        assets.model_blocking(helmet).unwrap();
        assert!(cooked_models(&project, helmet).is_empty());
        // Nor is one whose texture is a `data:` URI.
        let pyramid = "assets/models/pyramid.gltf";
        assets.model_blocking(pyramid).unwrap();
        assert!(cooked_models(&project, pyramid).is_empty());
        let _ = fs::remove_dir_all(project);
    }
}
