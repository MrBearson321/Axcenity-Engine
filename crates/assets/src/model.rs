//! Reading glTF and FBX models into plain triangles, materials and
//! textures. glTF is read here; FBX in [`fbx`].
//!
//! A model is fitted into the 1×1×1 cube centered on the origin, like the
//! built-in part meshes, so a MeshPart's `Size` stretches it exactly the way
//! it stretches a block. [`Model::size`] remembers how big it was in the
//! file (1 glTF unit, or 1 meter in an FBX file, is 1 stud), which is the
//! `Size` an imported MeshPart starts with.

mod fbx;
mod lod;
mod maps;
mod objects;
mod rig;
#[cfg(test)]
mod test_rig;

pub use maps::{MapSlot, TextureRole, texture_roles};
pub use objects::{ModelObject, Pick};
pub use rig::{
    Blend, Bone, BoneTransform, Joint, Keys, Pose, Posed, PosedPrimitive, Rig, Take, Track,
};

#[cfg(test)]
pub(crate) use fbx::tests::{TestTexture, checker_png, pyramid_fbx, pyramid_fbx_with_textures};

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use glam::{Mat3, Mat4, Vec3};
use gltf::image::Format;
use gltf::mesh::Mode;
use objects::{Listed, Siblings};

/// Nodes inside nodes deeper than this, or more node visits than
/// [`MAX_NODE_VISITS`], mean a broken file (nodes in a loop). Real models
/// are a handful of levels deep.
const MAX_NODE_DEPTH: usize = 64;
const MAX_NODE_VISITS: usize = 100_000;

/// Sizes below this count as flat: the model has no thickness on that axis.
const FLAT: f32 = 1e-6;

/// Every model, primitive and texture gets its own key, so the renderer
/// can keep one GPU copy of each.
static NEXT_KEY: AtomicU64 = AtomicU64::new(1);

fn new_key() -> u64 {
    NEXT_KEY.fetch_add(1, Ordering::Relaxed)
}

/// The kinds of model file the engine reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelFormat {
    /// glTF 2.0: a `.gltf` (JSON) or `.glb` (binary) file.
    Gltf,
    /// FBX, binary or ASCII: a `.fbx` file.
    Fbx,
}

impl ModelFormat {
    /// The format of the file at `path`, by its extension (in any case), or
    /// `None` if it isn't a model file.
    pub fn of(path: &Path) -> Option<Self> {
        let extension = path.extension()?.to_str()?.to_ascii_lowercase();
        match extension.as_str() {
            "gltf" | "glb" => Some(Self::Gltf),
            "fbx" => Some(Self::Fbx),
            _ => None,
        }
    }
}

/// A loaded model: its triangles, fitted into the 1×1×1 cube centered on
/// the origin, with their materials.
pub struct Model {
    key: u64,
    /// The model's size in its file, in studs (1 glTF unit = 1 stud). Zero
    /// on an axis where it's flat.
    pub size: [f32; 3],
    /// The middle of its box in the file, in studs. Objects imported from
    /// one file keep their places by where their centers are.
    pub center: [f32; 3],
    /// One per glTF primitive (and per time its node is used), each with
    /// one material.
    pub primitives: Vec<Primitive>,
    /// Every distinct corner, for the collision hull.
    pub hull_points: Vec<[f32; 3]>,
    /// Problems that didn't stop it loading, such as an FBX texture that
    /// isn't there (the model is drawn without it). They finish the sentence
    /// "The model `…` ".
    pub warnings: Vec<String>,
    /// The skeleton its triangles are weighted to, if it's animated (an
    /// FBX file with a skin or animation). See [`Model::pose`].
    pub rig: Option<Arc<Rig>>,
}

impl Model {
    /// Different for every model loaded, even from the same file.
    pub fn key(&self) -> u64 {
        self.key
    }
}

/// Two models are the same if they're the same load (see [`Model::key`]).
impl PartialEq for Model {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}

impl fmt::Debug for Model {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Model #{} ({} primitives, size {:?})",
            self.key,
            self.primitives.len(),
            self.size
        )
    }
}

/// Triangles that share one material.
pub struct Primitive {
    key: u64,
    pub positions: Vec<[f32; 3]>,
    /// One per position, one unit long.
    pub normals: Vec<[f32; 3]>,
    /// Texture coordinates, one per position, or empty when the material
    /// has no texture. (0, 0) is the top left of the texture.
    pub uvs: Vec<[f32; 2]>,
    /// One per position when the material has a normal map, else empty:
    /// the direction U grows along the surface, and in `w` 1 or -1, the
    /// side of the normal the texture's "up" (V shrinking) is on.
    pub tangents: Vec<[f32; 4]>,
    /// Three per triangle, counter-clockwise seen from outside.
    pub indices: Vec<u32>,
    pub material: ModelMaterial,
    /// For an animated model ([`Model::rig`]), one per position: up to 4
    /// [`Joint`]s (indices into [`Rig::joints`]) it moves with, and how much
    /// each counts (adding up to 1, or all 0 for a corner that never
    /// moves). Empty when the model isn't animated.
    pub joints: Vec<[u16; 4]>,
    pub weights: Vec<[f32; 4]>,
    /// Simpler copies with the same material, each with fewer triangles
    /// than the one before, to draw far away: the file's own `_LOD1` and
    /// `_LOD2` objects, or simplified when the model was loaded (see
    /// [`lod`]). Their own `lods` are empty.
    pub lods: Vec<Primitive>,
    /// While reading: 0 for the full model, or which simpler copy this is.
    level: u8,
}

impl Primitive {
    /// Different for every primitive loaded.
    pub fn key(&self) -> u64 {
        self.key
    }
}

/// A glTF metallic-roughness material. Colors are linear, as in the file.
/// The textures all use the primitive's one set of texture coordinates.
#[derive(Clone, Debug)]
pub struct ModelMaterial {
    /// Its name in the file, or "" if it has none.
    pub name: String,
    /// Red, green, blue and alpha (opacity).
    pub base_color: [f32; 4],
    pub metalness: f32,
    pub roughness: f32,
    pub emissive: [f32; 3],
    pub alpha_mode: AlphaMode,
    /// Multiplies `base_color` (sRGB, and alpha).
    pub base_color_texture: Option<Arc<Image>>,
    /// A tangent-space normal map (glTF's convention: green points up the
    /// texture).
    pub normal_texture: Option<Arc<Image>>,
    /// Scales how far the normal map tips the normals.
    pub normal_scale: f32,
    /// True if the normal map's green points down the texture (DirectX's
    /// convention) instead.
    pub normal_flip_green: bool,
    /// Multiplies `metalness`.
    pub metalness_texture: Option<ChannelTexture>,
    /// Multiplies `roughness`.
    pub roughness_texture: Option<ChannelTexture>,
    /// Ambient occlusion: how much ambient light reaches each point.
    pub occlusion_texture: Option<ChannelTexture>,
    /// How strongly `occlusion_texture` darkens, from 0 to 1.
    pub occlusion_strength: f32,
    /// Multiplies `emissive` (sRGB).
    pub emissive_texture: Option<Arc<Image>>,
    /// The opacity, instead of `base_color_texture`'s alpha.
    pub opacity_texture: Option<ChannelTexture>,
    /// How strongly the non-metal parts reflect, for older (Phong)
    /// materials: 0.5 is the usual amount, 0 none, 1 twice that.
    pub specular_texture: Option<Arc<Image>>,
    /// Multiplies the texture coordinates: 2 repeats the textures twice
    /// as often.
    pub uv_scale: [f32; 2],
    /// Turns the scaled coordinates, in radians, as glTF's
    /// `KHR_texture_transform` does.
    pub uv_rotation: f32,
    /// Added to the coordinates last, in copies of the texture.
    pub uv_offset: [f32; 2],
    /// Both sides of each triangle are drawn, and lit (leaves, paper).
    pub double_sided: bool,
}

impl Default for ModelMaterial {
    /// Plain white plastic, as drawn for a mesh without a material.
    fn default() -> Self {
        Self {
            name: String::new(),
            base_color: [1.0; 4],
            metalness: 0.0,
            roughness: 1.0,
            emissive: [0.0; 3],
            alpha_mode: AlphaMode::Opaque,
            base_color_texture: None,
            normal_texture: None,
            normal_scale: 1.0,
            normal_flip_green: false,
            metalness_texture: None,
            roughness_texture: None,
            occlusion_texture: None,
            occlusion_strength: 1.0,
            emissive_texture: None,
            opacity_texture: None,
            specular_texture: None,
            uv_scale: [1.0, 1.0],
            uv_rotation: 0.0,
            uv_offset: [0.0, 0.0],
            double_sided: false,
        }
    }
}

impl ModelMaterial {
    /// True if it has any texture, so it needs texture coordinates.
    pub fn has_textures(&self) -> bool {
        self.base_color_texture.is_some()
            || self.normal_texture.is_some()
            || self.metalness_texture.is_some()
            || self.roughness_texture.is_some()
            || self.occlusion_texture.is_some()
            || self.emissive_texture.is_some()
            || self.opacity_texture.is_some()
            || self.specular_texture.is_some()
    }

    /// Takes every texture off, for a mesh with no texture coordinates.
    fn remove_textures(&mut self) {
        self.base_color_texture = None;
        self.normal_texture = None;
        self.metalness_texture = None;
        self.roughness_texture = None;
        self.occlusion_texture = None;
        self.emissive_texture = None;
        self.opacity_texture = None;
        self.specular_texture = None;
    }
}

/// One channel of an image, used as a number from 0 to 1.
#[derive(Clone, Debug)]
pub struct ChannelTexture {
    pub image: Arc<Image>,
    pub channel: Channel,
    /// Use 1 minus the value: a glossiness or smoothness map as roughness.
    pub invert: bool,
}

impl ChannelTexture {
    pub fn new(image: Arc<Image>, channel: Channel) -> Self {
        Self {
            image,
            channel,
            invert: false,
        }
    }
}

/// A color channel of an image. Saved by its name (`"Red"`) in material
/// files.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Channel {
    #[default]
    Red,
    Green,
    Blue,
    Alpha,
}

/// How a material's alpha is used.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AlphaMode {
    /// Solid; alpha is ignored.
    Opaque,
    /// Solid where alpha is at least `cutoff`, cut out elsewhere (leaves, fences).
    Mask { cutoff: f32 },
    /// See-through by its alpha.
    Blend,
}

/// A decoded texture.
pub struct Image {
    key: u64,
    pub width: u32,
    pub height: u32,
    /// 4 bytes per pixel (red, green, blue, alpha), rows from the top:
    /// sRGB-encoded for colors, plain values for the other maps.
    pub rgba: Vec<u8>,
    /// Its file, relative to the model's folder (with `/` between
    /// folders). For an image embedded in the model, where importing
    /// copies it out to (`textures/…`). `None` if it has no file.
    pub file: Option<String>,
}

impl Image {
    /// Different for every texture loaded.
    pub fn key(&self) -> u64 {
        self.key
    }
}

impl fmt::Debug for Image {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Image #{} ({}×{})", self.key, self.width, self.height)
    }
}

/// Why a model couldn't be loaded. The messages finish the sentence "Could
/// not load the model `…`: ".
#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    /// `gltf`'s own explanation is the error's source, so show the whole
    /// chain (`{:#}` on an `anyhow::Error`) to see it.
    #[error("it isn't a valid glTF file")]
    Gltf(#[from] gltf::Error),
    /// `ufbx`'s explanation, such as "Truncated file".
    #[error("it isn't a valid FBX file: {0}")]
    Fbx(String),
    #[error("it uses `{0}`, which isn't inside its folder; keep the files a model uses next to it")]
    OutsideFolder(String),
    #[error("it uses `{0}`, which isn't there; put it next to the model")]
    MissingFile(String),
    #[error("it has no triangles to draw")]
    NoTriangles,
    #[error("it has no object `{0}`")]
    NoObject(String),
    #[error("a mesh in it has no vertex positions")]
    NoPositions,
    #[error("a mesh in it lists a corner that isn't there")]
    BadIndex,
    #[error("its nodes are nested too deeply, or in a loop")]
    TooDeep,
    #[error("its texture `{0}` doesn't match its own size")]
    BadTexture(String),
    #[error("the file is damaged")]
    Damaged,
}

/// Reads a model file's `bytes`, in `format`. Files it uses (`.bin`
/// buffers, textures) are read from `folder`, the folder the model is in,
/// and must be inside it.
pub fn load_model(bytes: &[u8], folder: &Path, format: ModelFormat) -> Result<Model, ModelError> {
    let mut models = load_models(bytes, folder, format, &[Pick::Whole])?;
    Ok(models.remove(0))
}

/// Like [`load_model`], but makes one model for each of `picks`, reading
/// the file once. Textures they share are decoded once. A pick naming an
/// object the file doesn't have is an error ([`ModelError::NoObject`]).
pub fn load_models(
    bytes: &[u8],
    folder: &Path,
    format: ModelFormat,
    picks: &[Pick],
) -> Result<Vec<Model>, ModelError> {
    // The `gltf` crate trusts some indices after checking the file, and
    // panics (`unwrap`) on a few broken files; `ufbx` turns its own internal
    // errors into panics too. `catch_unwind` turns such a panic into an
    // error, so a bad file can never crash the engine.
    panic::catch_unwind(AssertUnwindSafe(|| match format {
        ModelFormat::Gltf => read(bytes, folder, picks),
        ModelFormat::Fbx => fbx::load(bytes, folder, picks),
    }))
    .unwrap_or(Err(ModelError::Damaged))
}

/// Every take (animation timeline) in a model file's `bytes`, baked into
/// keys per bone. Only FBX files give takes for now; a glTF file has none.
pub fn load_takes(bytes: &[u8], format: ModelFormat) -> Result<Vec<Take>, ModelError> {
    panic::catch_unwind(AssertUnwindSafe(|| match format {
        ModelFormat::Gltf => Ok(Vec::new()),
        ModelFormat::Fbx => fbx::load_takes(bytes),
    }))
    .unwrap_or(Err(ModelError::Damaged))
}

/// The objects in a model file, parents first (see [`ModelObject`]), for
/// choosing which to import. Reads only the file itself, not its textures.
pub fn list_objects(bytes: &[u8], format: ModelFormat) -> Result<Vec<ModelObject>, ModelError> {
    panic::catch_unwind(AssertUnwindSafe(|| match format {
        ModelFormat::Gltf => list(bytes),
        ModelFormat::Fbx => fbx::list(bytes),
    }))
    .unwrap_or(Err(ModelError::Damaged))
}

/// The files a model in `folder` uses besides itself, as paths inside
/// `folder`, for copying it with them. A glTF's must all be there; an
/// FBX's textures are left out when they aren't.
pub(crate) fn files_used(
    bytes: &[u8],
    folder: &Path,
    format: ModelFormat,
) -> Result<Vec<PathBuf>, ModelError> {
    panic::catch_unwind(AssertUnwindSafe(|| match format {
        ModelFormat::Gltf => {
            let document = gltf::Gltf::from_slice(bytes)?;
            external_files(&document)
                .into_iter()
                .map(|uri| uri_path(&uri))
                .collect()
        }
        ModelFormat::Fbx => fbx::files_used(bytes, folder),
    }))
    .unwrap_or(Err(ModelError::Damaged))
}

/// The image files embedded in a model, as file names and bytes, for
/// copying out to its `textures` folder on import (see [`Image::file`]).
/// Their bytes are copied as they are.
pub(crate) fn embedded_files(
    bytes: &[u8],
    format: ModelFormat,
) -> Result<Vec<(String, Vec<u8>)>, ModelError> {
    panic::catch_unwind(AssertUnwindSafe(|| match format {
        ModelFormat::Gltf => gltf_embedded_files(bytes),
        ModelFormat::Fbx => fbx::embedded_textures(bytes),
    }))
    .unwrap_or(Err(ModelError::Damaged))
}

fn gltf_embedded_files(bytes: &[u8]) -> Result<Vec<(String, Vec<u8>)>, ModelError> {
    let gltf::Gltf { document, blob } = gltf::Gltf::from_slice(bytes)?;
    let names = embedded_image_names(&document);
    if names.is_empty() {
        return Ok(Vec::new());
    }
    // Only the file's own buffers: the images in them, and data URIs, are
    // what's embedded. Buffers in other files aren't needed.
    let buffers: Vec<gltf::buffer::Data> = document
        .buffers()
        .map(|buffer| match buffer.source() {
            gltf::buffer::Source::Bin => blob.clone().unwrap_or_default(),
            gltf::buffer::Source::Uri(uri) => data_uri(uri).unwrap_or_default(),
        })
        .map(gltf::buffer::Data)
        .collect();
    let mut files = Vec::new();
    for image in document.images() {
        let Some(name) = names.get(&image.index()) else {
            continue;
        };
        let content = match image.source() {
            gltf::image::Source::View { view, .. } => buffers
                .get(view.buffer().index())
                .and_then(|data| data.0.get(view.offset()..view.offset() + view.length()))
                .map(<[u8]>::to_vec),
            gltf::image::Source::Uri { uri, .. } => data_uri(uri),
        };
        if let Some(content) = content {
            files.push((name.clone(), content));
        }
    }
    Ok(files)
}

/// The bytes of a base64 `data:` URI, or `None` for any other URI.
fn data_uri(uri: &str) -> Option<Vec<u8>> {
    let (_, data) = uri.strip_prefix("data:")?.split_once(";base64,")?;
    let digit = |c: u8| match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' | b'-' => Some(62),
        b'/' | b'_' => Some(63),
        _ => None,
    };
    let mut bytes = Vec::with_capacity(data.len() / 4 * 3);
    let (mut bits, mut count) = (0u32, 0);
    for c in data.bytes().filter(|&c| c != b'=') {
        bits = bits << 6 | u32::from(digit(c)?);
        count += 6;
        if count >= 8 {
            count -= 8;
            bytes.push((bits >> count) as u8);
        }
    }
    Some(bytes)
}

/// The file names (made safe, and unique) that importing gives a glTF's
/// embedded images, by image index: their names, or `image<N>`, with the
/// extension their type has.
fn embedded_image_names(document: &gltf::Document) -> HashMap<usize, String> {
    let mut names: HashMap<usize, String> = HashMap::new();
    for image in document.images() {
        let extension = match image.source() {
            gltf::image::Source::View { mime_type, .. } => match mime_type {
                "image/jpeg" => "jpg",
                _ => "png",
            },
            gltf::image::Source::Uri { uri, .. } if uri.starts_with("data:image/jpeg") => "jpg",
            gltf::image::Source::Uri { uri, .. } if uri.starts_with("data:") => "png",
            gltf::image::Source::Uri { .. } => continue,
        };
        let stem = image
            .name()
            .and_then(crate::import::safe_file_name)
            .map(|name| match name.rsplit_once('.') {
                Some((stem, _)) => stem.to_owned(),
                None => name,
            })
            .unwrap_or_else(|| format!("image{}", image.index()));
        let name = crate::import::unique_file_name(&format!("{stem}.{extension}"), |taken| {
            names.values().any(|name| name.eq_ignore_ascii_case(taken))
        });
        names.insert(image.index(), name);
    }
    names
}

fn read(bytes: &[u8], folder: &Path, picks: &[Pick]) -> Result<Vec<Model>, ModelError> {
    let gltf::Gltf { document, blob } = gltf::Gltf::from_slice(bytes)?;
    // Checked here, not left to `gltf`, so nothing outside `folder` is read
    // and a missing file is named.
    for uri in external_files(&document) {
        if !folder.join(uri_path(&uri)?).is_file() {
            return Err(ModelError::MissingFile(uri));
        }
    }
    let buffers = gltf::import_buffers(&document, Some(folder), blob)?;

    let mut reader = Reader {
        folder,
        buffers: &buffers,
        images: HashMap::new(),
        primitives: Vec::new(),
        visits: 0,
        embedded: embedded_image_names(&document),
        wanted: None,
        found: HashSet::new(),
        level: 0,
    };
    let mut models = Vec::new();
    for pick in picks {
        reader.visits = 0;
        reader.wanted = pick.paths();
        reader.found.clear();
        reader.walk(&document)?;
        if let Some(path) = missing(&reader.wanted, &reader.found) {
            return Err(ModelError::NoObject(path));
        }
        models.push(fit(std::mem::take(&mut reader.primitives))?);
    }
    Ok(models)
}

/// The objects in a glTF file: the default scene's nodes, or every mesh
/// if it has no scenes.
fn list(bytes: &[u8]) -> Result<Vec<ModelObject>, ModelError> {
    let document = gltf::Gltf::from_slice(bytes)?.document;
    let mut visits = 0;
    let mut siblings = Siblings::default();
    let tree = match default_scene(&document) {
        Some(scene) => scene
            .nodes()
            .map(|node| {
                let path = siblings.path("", &node_name(&node));
                list_node(&node, path, 0, &mut visits)
            })
            .collect::<Result<Vec<_>, _>>()?,
        None => document
            .meshes()
            .map(|mesh| {
                let name = mesh_name(&mesh);
                let (triangles, materials) = mesh_summary(&mesh);
                Listed {
                    object: ModelObject {
                        path: siblings.path("", &name),
                        name,
                        depth: 0,
                        triangles,
                        materials,
                    },
                    children: Vec::new(),
                }
            })
            .collect(),
    };
    Ok(objects::flatten(tree))
}

fn list_node(
    node: &gltf::Node,
    path: String,
    depth: usize,
    visits: &mut usize,
) -> Result<Listed, ModelError> {
    *visits += 1;
    if depth > MAX_NODE_DEPTH || *visits > MAX_NODE_VISITS {
        return Err(ModelError::TooDeep);
    }
    let (triangles, materials) = node
        .mesh()
        .map(|mesh| mesh_summary(&mesh))
        .unwrap_or_default();
    let mut siblings = Siblings::default();
    let children = node
        .children()
        .map(|child| {
            let child_path = siblings.path(&path, &node_name(&child));
            list_node(&child, child_path, depth + 1, visits)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Listed {
        object: ModelObject {
            path,
            name: node_name(node),
            depth,
            triangles,
            materials,
        },
        children,
    })
}

/// How many triangles a mesh has, and its materials' names.
fn mesh_summary(mesh: &gltf::Mesh) -> (usize, Vec<String>) {
    let mut triangles = 0;
    let mut materials: Vec<String> = Vec::new();
    for primitive in mesh.primitives() {
        if primitive.mode() != Mode::Triangles {
            continue;
        }
        let corners = primitive
            .indices()
            .or_else(|| primitive.get(&gltf::Semantic::Positions))
            .map_or(0, |accessor| accessor.count());
        triangles += corners / 3;
        let material = primitive.material();
        let name = match (material.name(), material.index()) {
            (Some(name), _) => name.to_owned(),
            (None, Some(index)) => format!("Material {index}"),
            (None, None) => "Default".to_owned(),
        };
        if !materials.contains(&name) {
            materials.push(name);
        }
    }
    (triangles, materials)
}

fn default_scene(document: &gltf::Document) -> Option<gltf::Scene<'_>> {
    document
        .default_scene()
        .or_else(|| document.scenes().next())
}

/// A node's name, or its mesh's, or its number.
fn node_name(node: &gltf::Node) -> String {
    node.name()
        .or_else(|| node.mesh().and_then(|mesh| mesh.name()))
        .map_or_else(|| format!("Node {}", node.index()), str::to_owned)
}

fn mesh_name(mesh: &gltf::Mesh) -> String {
    mesh.name()
        .map_or_else(|| format!("Mesh {}", mesh.index()), str::to_owned)
}

/// True if the object at `path` is in `wanted` (or everything is wanted),
/// adding it to `found` if so.
fn want(wanted: &Option<HashSet<String>>, found: &mut HashSet<String>, path: &str) -> bool {
    match wanted {
        None => true,
        Some(wanted) if wanted.contains(path) => {
            found.insert(path.to_owned());
            true
        }
        Some(_) => false,
    }
}

/// The first wanted path, alphabetically, that wasn't found.
fn missing(wanted: &Option<HashSet<String>>, found: &HashSet<String>) -> Option<String> {
    wanted
        .iter()
        .flatten()
        .filter(|path| !found.contains(*path))
        .min()
        .cloned()
}

/// The URIs of the files a model uses (buffers and textures), leaving out
/// data embedded in the file itself (`data:` URIs).
fn external_files(document: &gltf::Document) -> Vec<String> {
    let buffers = document
        .buffers()
        .filter_map(|buffer| match buffer.source() {
            gltf::buffer::Source::Uri(uri) => Some(uri),
            gltf::buffer::Source::Bin => None,
        });
    let images = document.images().filter_map(|image| match image.source() {
        gltf::image::Source::Uri { uri, .. } => Some(uri),
        gltf::image::Source::View { .. } => None,
    });
    buffers
        .chain(images)
        .filter(|uri| !uri.starts_with("data:"))
        .map(str::to_owned)
        .collect()
}

/// A model's relative URI (with `%20` and the like decoded) as a path
/// inside the model's folder.
fn uri_path(uri: &str) -> Result<PathBuf, ModelError> {
    percent_decode(uri)
        .as_deref()
        .and_then(crate::safe_path)
        .ok_or_else(|| ModelError::OutsideFolder(uri.to_owned()))
}

/// Decodes `%XX` escapes, or `None` if they're broken or not UTF-8.
fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = text.get(i + 1..i + 3)?;
            decoded.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            decoded.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(decoded).ok()
}

/// Walks the node tree, collecting each mesh's triangles in model space.
struct Reader<'a> {
    folder: &'a Path,
    buffers: &'a [gltf::buffer::Data],
    /// Textures decoded so far, by image index, so shared ones decode once.
    images: HashMap<usize, Arc<Image>>,
    primitives: Vec<Primitive>,
    visits: usize,
    /// The file names importing gives embedded images (see
    /// [`embedded_image_names`]).
    embedded: HashMap<usize, String>,
    /// The paths of the objects to draw, or `None` for all of them.
    wanted: Option<HashSet<String>>,
    /// The wanted paths seen so far.
    found: HashSet<String>,
    /// The level of detail of the objects being read (see [`lod::level_of`]).
    level: u8,
}

impl Reader<'_> {
    /// Collects the wanted meshes in the default scene, or every mesh if
    /// the file has no scenes.
    fn walk(&mut self, document: &gltf::Document) -> Result<(), ModelError> {
        let mut siblings = Siblings::default();
        match default_scene(document) {
            Some(scene) => {
                for node in scene.nodes() {
                    let path = siblings.path("", &node_name(&node));
                    self.node(&node, Mat4::IDENTITY, 0, &path)?;
                }
            }
            // A file with no scenes: draw every mesh where it is.
            None => {
                for mesh in document.meshes() {
                    let path = siblings.path("", &mesh_name(&mesh));
                    if want(&self.wanted, &mut self.found, &path) {
                        self.mesh(&mesh, Mat4::IDENTITY)?;
                    }
                }
            }
        }
        Ok(())
    }

    fn node(
        &mut self,
        node: &gltf::Node,
        parent: Mat4,
        depth: usize,
        path: &str,
    ) -> Result<(), ModelError> {
        self.visits += 1;
        if depth > MAX_NODE_DEPTH || self.visits > MAX_NODE_VISITS {
            return Err(ModelError::TooDeep);
        }
        let transform = parent * Mat4::from_cols_array_2d(&node.transform().matrix());
        // Everything inside a `_LOD1` node is part of that copy.
        let outer = self.level;
        self.level = self.level.max(lod::level_of(&node_name(node)));
        if let Some(mesh) = node.mesh()
            && want(&self.wanted, &mut self.found, path)
        {
            self.mesh(&mesh, transform)?;
        }
        let mut siblings = Siblings::default();
        for child in node.children() {
            let child_path = siblings.path(path, &node_name(&child));
            self.node(&child, transform, depth + 1, &child_path)?;
        }
        self.level = outer;
        Ok(())
    }

    fn mesh(&mut self, mesh: &gltf::Mesh, transform: Mat4) -> Result<(), ModelError> {
        // Normals are turned by the inverse transpose, so they stay
        // perpendicular to stretched surfaces.
        let normal_matrix = Mat3::from_mat4(transform).inverse().transpose();
        // A mirrored node turns triangles inside out: swapping two corners
        // turns them back.
        let mirrored = transform.determinant() < 0.0;
        for primitive in mesh.primitives() {
            // Points and lines have no surface to draw.
            if primitive.mode() != Mode::Triangles {
                continue;
            }
            let buffers = self.buffers;
            let reader = primitive
                .reader(|buffer| buffers.get(buffer.index()).map(|data| data.0.as_slice()));

            let positions: Vec<[f32; 3]> = reader
                .read_positions()
                .ok_or(ModelError::NoPositions)?
                .map(|p| transform.transform_point3(Vec3::from(p)).to_array())
                .collect();
            let count = positions.len();
            let mut indices: Vec<u32> = match reader.read_indices() {
                Some(indices) => indices.into_u32().collect(),
                None => (0..count as u32).collect(),
            };
            indices.truncate(indices.len() / 3 * 3);
            if indices.iter().any(|&index| index as usize >= count) {
                return Err(ModelError::BadIndex);
            }
            if mirrored {
                for triangle in indices.as_chunks_mut::<3>().0 {
                    triangle.swap(1, 2);
                }
            }

            let normals: Vec<[f32; 3]> = reader
                .read_normals()
                .map(|normals| {
                    normals
                        .map(|n| {
                            (normal_matrix * Vec3::from(n))
                                .normalize_or_zero()
                                .to_array()
                        })
                        .collect()
                })
                .filter(|normals: &Vec<[f32; 3]>| normals.len() == count)
                .unwrap_or_else(|| smooth_normals(&positions, &indices));

            let (mut material, uv_set) = self.material(&primitive.material())?;
            let uvs: Vec<[f32; 2]> = uv_set
                .and_then(|set| reader.read_tex_coords(set))
                .map(|uvs| uvs.into_f32().collect())
                .filter(|uvs: &Vec<[f32; 2]>| uvs.len() == count)
                .unwrap_or_default();
            // A texture needs coordinates to be placed by.
            if uvs.is_empty() {
                material.remove_textures();
            }

            self.primitives.push(Primitive {
                key: new_key(),
                positions,
                normals,
                uvs,
                tangents: Vec::new(),
                indices,
                material,
                joints: Vec::new(),
                weights: Vec::new(),
                lods: Vec::new(),
                level: self.level,
            });
        }
        Ok(())
    }

    /// The material and the texture coordinate set its textures use: the
    /// first texture's. Textures placed by another set are left out, since
    /// a primitive is drawn with one.
    fn material(
        &mut self,
        material: &gltf::Material,
    ) -> Result<(ModelMaterial, Option<u32>), ModelError> {
        let pbr = material.pbr_metallic_roughness();
        let base_color = pbr.base_color_texture();
        let metallic_roughness = pbr.metallic_roughness_texture();
        let normal = material.normal_texture();
        let occlusion = material.occlusion_texture();
        let emissive = material.emissive_texture();
        let uv_set = [
            base_color.as_ref().map(|info| info.tex_coord()),
            metallic_roughness.as_ref().map(|info| info.tex_coord()),
            normal.as_ref().map(|info| info.tex_coord()),
            occlusion.as_ref().map(|info| info.tex_coord()),
            emissive.as_ref().map(|info| info.tex_coord()),
        ]
        .into_iter()
        .flatten()
        .next();
        // One transform places every map: the first texture's that has one
        // (`KHR_texture_transform`), unless it moves that texture to
        // another coordinate set.
        let transform = [
            base_color.as_ref(),
            metallic_roughness.as_ref(),
            emissive.as_ref(),
        ]
        .into_iter()
        .flatten()
        .find_map(|info| info.texture_transform())
        .filter(|transform| transform.tex_coord().is_none_or(|set| Some(set) == uv_set));
        // The image of a texture on `uv_set`, decoded.
        let mut image = |texture: Option<(gltf::Texture, u32)>| match texture {
            Some((texture, set)) if Some(set) == uv_set => self.image(&texture.source()).map(Some),
            _ => Ok(None),
        };
        let base_color_texture = image(base_color.map(|i| (i.texture(), i.tex_coord())))?;
        // glTF keeps roughness in green and metalness in blue.
        let metallic_roughness = image(metallic_roughness.map(|i| (i.texture(), i.tex_coord())))?;
        let normal_scale = normal.as_ref().map_or(1.0, |info| info.scale());
        let normal_texture = image(normal.map(|i| (i.texture(), i.tex_coord())))?;
        let occlusion_strength = occlusion.as_ref().map_or(1.0, |info| info.strength());
        // Occlusion is in red, often in the same image as metalness and
        // roughness (an "ORM" texture).
        let occlusion_texture = image(occlusion.map(|i| (i.texture(), i.tex_coord())))?
            .map(|image| ChannelTexture::new(image, Channel::Red));
        let emissive_texture = image(emissive.map(|i| (i.texture(), i.tex_coord())))?;
        let material = ModelMaterial {
            name: material.name().unwrap_or_default().to_owned(),
            base_color: pbr.base_color_factor(),
            metalness: pbr.metallic_factor(),
            roughness: pbr.roughness_factor(),
            emissive: material.emissive_factor(),
            alpha_mode: match material.alpha_mode() {
                gltf::material::AlphaMode::Opaque => AlphaMode::Opaque,
                gltf::material::AlphaMode::Mask => AlphaMode::Mask {
                    cutoff: material.alpha_cutoff().unwrap_or(0.5),
                },
                gltf::material::AlphaMode::Blend => AlphaMode::Blend,
            },
            base_color_texture,
            normal_texture,
            normal_scale,
            normal_flip_green: false,
            metalness_texture: metallic_roughness
                .clone()
                .map(|image| ChannelTexture::new(image, Channel::Blue)),
            roughness_texture: metallic_roughness
                .map(|image| ChannelTexture::new(image, Channel::Green)),
            occlusion_texture,
            occlusion_strength,
            emissive_texture,
            opacity_texture: None,
            specular_texture: None,
            uv_scale: transform.as_ref().map_or([1.0, 1.0], |t| t.scale()),
            uv_rotation: transform.as_ref().map_or(0.0, |t| t.rotation()),
            uv_offset: transform.as_ref().map_or([0.0, 0.0], |t| t.offset()),
            double_sided: material.double_sided(),
        };
        Ok((material, uv_set))
    }

    fn image(&mut self, image: &gltf::Image) -> Result<Arc<Image>, ModelError> {
        if let Some(decoded) = self.images.get(&image.index()) {
            return Ok(decoded.clone());
        }
        let data = gltf::image::Data::from_source(image.source(), Some(self.folder), self.buffers)?;
        let name = match (image.name(), image.source()) {
            (Some(name), _) => name.to_owned(),
            (None, gltf::image::Source::Uri { uri, .. }) if !uri.starts_with("data:") => {
                uri.to_owned()
            }
            _ => format!("image {}", image.index()),
        };
        let rgba = to_rgba8(&data).ok_or(ModelError::BadTexture(name))?;
        let file = match image.source() {
            gltf::image::Source::Uri { uri, .. } if !uri.starts_with("data:") => {
                percent_decode(uri)
            }
            _ => self
                .embedded
                .get(&image.index())
                .map(|name| format!("{}/{name}", crate::import::TEXTURES_FOLDER)),
        };
        let decoded = Arc::new(Image {
            key: new_key(),
            width: data.width,
            height: data.height,
            rgba,
            file,
        });
        self.images.insert(image.index(), decoded.clone());
        Ok(decoded)
    }
}

/// The pixels as 8-bit RGBA, whatever the image's format, or `None` if
/// there are too few or too many of them.
fn to_rgba8(data: &gltf::image::Data) -> Option<Vec<u8>> {
    // How many channels each pixel has, and how many bytes each channel takes.
    let (channels, bytes) = match data.format {
        Format::R8 => (1, 1),
        Format::R8G8 => (2, 1),
        Format::R8G8B8 => (3, 1),
        Format::R8G8B8A8 => (4, 1),
        Format::R16 => (1, 2),
        Format::R16G16 => (2, 2),
        Format::R16G16B16 => (3, 2),
        Format::R16G16B16A16 => (4, 2),
        Format::R32G32B32FLOAT => (3, 4),
        Format::R32G32B32A32FLOAT => (4, 4),
    };
    let pixels = data.width as usize * data.height as usize;
    if pixels == 0 || data.pixels.len() != pixels * channels * bytes {
        return None;
    }
    // One channel as a byte. `image` stores wider channels in the machine's
    // own byte order.
    let channel = |at: usize| -> u8 {
        let raw = &data.pixels[at * bytes..][..bytes];
        match bytes {
            1 => raw[0],
            2 => (u16::from_ne_bytes([raw[0], raw[1]]) >> 8) as u8,
            _ => {
                let value = f32::from_ne_bytes([raw[0], raw[1], raw[2], raw[3]]);
                (value.clamp(0.0, 1.0) * 255.0).round() as u8
            }
        }
    };
    let mut rgba = Vec::with_capacity(pixels * 4);
    for pixel in 0..pixels {
        let at = pixel * channels;
        // Grey images repeat their one channel as red, green and blue.
        let rgba_pixel = match channels {
            1 => [channel(at), channel(at), channel(at), 255],
            2 => [channel(at), channel(at), channel(at), channel(at + 1)],
            3 => [channel(at), channel(at + 1), channel(at + 2), 255],
            _ => [
                channel(at),
                channel(at + 1),
                channel(at + 2),
                channel(at + 3),
            ],
        };
        rgba.extend_from_slice(&rgba_pixel);
    }
    Some(rgba)
}

/// Decodes a PNG, JPEG, TGA or BMP texture. TGA files have no signature to
/// recognize, so the name's extension decides when the content can't.
pub(crate) fn decode_image(bytes: &[u8], name: &str) -> Result<Image, String> {
    let format = image::guess_format(bytes)
        .or_else(|_| image::ImageFormat::from_path(name))
        .map_err(|_| "only PNG, JPEG, TGA and BMP textures can be used".to_owned())?;
    let rgba = image::load_from_memory_with_format(bytes, format)
        .map_err(|err| match err {
            image::ImageError::Unsupported(_) => {
                "only PNG, JPEG, TGA and BMP textures can be used".to_owned()
            }
            err => err.to_string(),
        })?
        .into_rgba8();
    if rgba.width() == 0 || rgba.height() == 0 {
        return Err("it has no pixels".to_owned());
    }
    Ok(Image {
        key: new_key(),
        width: rgba.width(),
        height: rgba.height(),
        rgba: rgba.into_raw(),
        file: None,
    })
}

/// Normals for a mesh that has none: each corner gets the average of the
/// triangles around it, weighted by their area.
fn smooth_normals(positions: &[[f32; 3]], indices: &[u32]) -> Vec<[f32; 3]> {
    let mut sums = vec![Vec3::ZERO; positions.len()];
    for triangle in indices.as_chunks::<3>().0 {
        let [a, b, c] = [0, 1, 2].map(|i| Vec3::from(positions[triangle[i] as usize]));
        // The cross product's length is twice the area, so bigger triangles count more.
        let facing = (b - a).cross(c - a);
        for &index in triangle {
            sums[index as usize] += facing;
        }
    }
    sums.into_iter()
        .map(|sum| sum.normalize_or(Vec3::Y).to_array())
        .collect()
}

/// Tangents for a normal map (see [`Primitive::tangents`]): each corner
/// gets the average U and V directions of the triangles around it,
/// weighted by their area, made perpendicular to its normal.
fn tangents(
    positions: &[[f32; 3]],
    normals: &[[f32; 3]],
    uvs: &[[f32; 2]],
    indices: &[u32],
) -> Vec<[f32; 4]> {
    let mut along_u = vec![Vec3::ZERO; positions.len()];
    let mut along_v = vec![Vec3::ZERO; positions.len()];
    for triangle in indices.as_chunks::<3>().0 {
        let [a, b, c] = triangle.map(|i| i as usize);
        let (pa, pb, pc) = (
            Vec3::from(positions[a]),
            Vec3::from(positions[b]),
            Vec3::from(positions[c]),
        );
        let (ua, ub, uc) = (uvs[a], uvs[b], uvs[c]);
        let (e1, e2) = (pb - pa, pc - pa);
        let (du1, dv1) = (ub[0] - ua[0], ub[1] - ua[1]);
        let (du2, dv2) = (uc[0] - ua[0], uc[1] - ua[1]);
        // Solving e = du·U + dv·V for the directions U and V. Scaled by
        // the area (not divided by it), so bigger triangles count more.
        let area = du1 * dv2 - du2 * dv1;
        if area.abs() < 1e-12 {
            continue;
        }
        let sign = area.signum();
        let u = (e1 * dv2 - e2 * dv1) * sign;
        let v = (e2 * du1 - e1 * du2) * sign;
        for i in [a, b, c] {
            along_u[i] += u;
            along_v[i] += v;
        }
    }
    (0..positions.len())
        .map(|i| {
            let n = Vec3::from(normals[i]);
            let t = (along_u[i] - n * n.dot(along_u[i])).normalize_or(n.any_orthonormal_vector());
            // V grows down the texture, so "up" is -V.
            let w = if n.cross(t).dot(-along_v[i]) < 0.0 {
                -1.0
            } else {
                1.0
            };
            [t.x, t.y, t.z, w]
        })
        .collect()
}

/// Fits the primitives into the 1×1×1 cube centered on the origin and
/// makes the model. The full model's primitives decide its box and hull;
/// simpler copies (see [`lod`]) are placed the same way and attached to them.
fn fit(mut primitives: Vec<Primitive>) -> Result<Model, ModelError> {
    // A file with only `_LOD1` objects has them as its full model.
    if primitives.iter().all(|primitive| primitive.level > 0) {
        for primitive in &mut primitives {
            primitive.level = 0;
        }
    }
    let full = |primitive: &&Primitive| primitive.level == 0;
    if primitives
        .iter()
        .filter(full)
        .all(|primitive| primitive.indices.is_empty())
    {
        return Err(ModelError::NoTriangles);
    }
    let corners = primitives
        .iter()
        .filter(full)
        .flat_map(|primitive| &primitive.positions)
        .map(|&p| Vec3::from(p))
        .filter(|p| p.is_finite());
    let (min, max) = corners.fold((Vec3::INFINITY, Vec3::NEG_INFINITY), |(min, max), p| {
        (min.min(p), max.max(p))
    });
    if !min.cmple(max).all() {
        return Err(ModelError::NoTriangles);
    }
    let size = max - min;
    let center = (min + max) / 2.0;
    // A flat axis (a plane, say) stays flat: there's nothing to stretch.
    let thick = size.cmpgt(Vec3::splat(FLAT));
    let scale = Vec3::select(thick, size.recip(), Vec3::ONE);
    // Shrinking along an axis tips normals toward it, by the inverse amount.
    let normal_scale = Vec3::select(thick, size, Vec3::ONE);

    let mut hull = HashSet::new();
    let mut hull_points = Vec::new();
    for primitive in &mut primitives {
        for position in &mut primitive.positions {
            let fitted = (Vec3::from(*position) - center) * scale;
            *position = fitted.to_array();
            // Each distinct finite corner of the full model once. Bit
            // patterns compare exactly.
            if primitive.level == 0
                && fitted.is_finite()
                && hull.insert(fitted.to_array().map(f32::to_bits))
            {
                hull_points.push(fitted.to_array());
            }
        }
        for normal in &mut primitive.normals {
            *normal = (Vec3::from(*normal) * normal_scale)
                .normalize_or_zero()
                .to_array();
        }
        // Worked out after fitting, so they match the fitted triangles.
        if primitive.material.normal_texture.is_some() && !primitive.uvs.is_empty() {
            primitive.tangents = tangents(
                &primitive.positions,
                &primitive.normals,
                &primitive.uvs,
                &primitive.indices,
            );
        }
    }
    Ok(Model {
        key: new_key(),
        size: Vec3::select(thick, size, Vec3::ZERO).to_array(),
        center: center.to_array(),
        primitives: lod::attach(primitives),
        hull_points,
        warnings: Vec::new(),
        rig: None,
    })
}

/// The transform [`fit`] applied to `model`: from the file's space into
/// the 1×1×1 cube.
fn fit_matrix(model: &Model) -> Mat4 {
    let size = Vec3::from(model.size);
    let scale = Vec3::select(size.cmpgt(Vec3::splat(FLAT)), size.recip(), Vec3::ONE);
    Mat4::from_scale(scale) * Mat4::from_translation(-Vec3::from(model.center))
}

#[cfg(test)]
mod tests;
