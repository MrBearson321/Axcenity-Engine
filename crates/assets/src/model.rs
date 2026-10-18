//! Reading glTF and FBX models into plain triangles, materials and
//! textures. glTF is read here; FBX in [`fbx`].
//!
//! A model is fitted into the 1×1×1 cube centered on the origin, like the
//! built-in part meshes, so a MeshPart's `Size` stretches it exactly the way
//! it stretches a block. [`Model::size`] remembers how big it was in the
//! file (1 glTF unit, or 1 meter in an FBX file, is 1 stud), which is the
//! `Size` an imported MeshPart starts with.

mod fbx;

#[cfg(test)]
pub(crate) use fbx::tests::{checker_png, pyramid_fbx};

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use glam::{Mat3, Mat4, Vec3};
use gltf::image::Format;
use gltf::mesh::Mode;

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
    /// One per glTF primitive (and per time its node is used), each with
    /// one material.
    pub primitives: Vec<Primitive>,
    /// Every distinct corner, for the collision hull.
    pub hull_points: Vec<[f32; 3]>,
    /// Problems that didn't stop it loading, such as an FBX texture that
    /// isn't there (the model is drawn without it). They finish the sentence
    /// "The model `…` ".
    pub warnings: Vec<String>,
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
    /// Three per triangle, counter-clockwise seen from outside.
    pub indices: Vec<u32>,
    pub material: ModelMaterial,
}

impl Primitive {
    /// Different for every primitive loaded.
    pub fn key(&self) -> u64 {
        self.key
    }
}

/// A glTF metallic-roughness material. Colors are linear, as in the file.
#[derive(Clone, Debug)]
pub struct ModelMaterial {
    /// Red, green, blue and alpha (opacity).
    pub base_color: [f32; 4],
    pub metalness: f32,
    pub roughness: f32,
    pub emissive: [f32; 3],
    pub alpha_mode: AlphaMode,
    /// Multiplies `base_color`.
    pub base_color_texture: Option<Arc<Image>>,
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
    /// 4 bytes per pixel (red, green, blue, alpha), sRGB-encoded, rows from
    /// the top.
    pub rgba: Vec<u8>,
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
    // The `gltf` crate trusts some indices after checking the file, and
    // panics (`unwrap`) on a few broken files; `ufbx` turns its own internal
    // errors into panics too. `catch_unwind` turns such a panic into an
    // error, so a bad file can never crash the engine.
    panic::catch_unwind(AssertUnwindSafe(|| match format {
        ModelFormat::Gltf => read(bytes, folder),
        ModelFormat::Fbx => fbx::load(bytes, folder),
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

fn read(bytes: &[u8], folder: &Path) -> Result<Model, ModelError> {
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
    };
    match document
        .default_scene()
        .or_else(|| document.scenes().next())
    {
        Some(scene) => {
            for node in scene.nodes() {
                reader.node(&node, Mat4::IDENTITY, 0)?;
            }
        }
        // A file with no scenes: draw every mesh where it is.
        None => {
            for mesh in document.meshes() {
                reader.mesh(&mesh, Mat4::IDENTITY)?;
            }
        }
    }
    fit(reader.primitives)
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
}

impl Reader<'_> {
    fn node(&mut self, node: &gltf::Node, parent: Mat4, depth: usize) -> Result<(), ModelError> {
        self.visits += 1;
        if depth > MAX_NODE_DEPTH || self.visits > MAX_NODE_VISITS {
            return Err(ModelError::TooDeep);
        }
        let transform = parent * Mat4::from_cols_array_2d(&node.transform().matrix());
        if let Some(mesh) = node.mesh() {
            self.mesh(&mesh, transform)?;
        }
        for child in node.children() {
            self.node(&child, transform, depth + 1)?;
        }
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

            let gltf_material = primitive.material();
            let mut material = self.material(&gltf_material)?;
            let uv_set = gltf_material
                .pbr_metallic_roughness()
                .base_color_texture()
                .map(|info| info.tex_coord());
            let uvs: Vec<[f32; 2]> = uv_set
                .and_then(|set| reader.read_tex_coords(set))
                .map(|uvs| uvs.into_f32().collect())
                .filter(|uvs: &Vec<[f32; 2]>| uvs.len() == count)
                .unwrap_or_default();
            // A texture needs coordinates to be placed by.
            if uvs.is_empty() {
                material.base_color_texture = None;
            }

            self.primitives.push(Primitive {
                key: new_key(),
                positions,
                normals,
                uvs,
                indices,
                material,
            });
        }
        Ok(())
    }

    fn material(&mut self, material: &gltf::Material) -> Result<ModelMaterial, ModelError> {
        let pbr = material.pbr_metallic_roughness();
        let base_color_texture = match pbr.base_color_texture() {
            Some(info) => Some(self.image(&info.texture().source())?),
            None => None,
        };
        Ok(ModelMaterial {
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
        })
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
        let decoded = Arc::new(Image {
            key: new_key(),
            width: data.width,
            height: data.height,
            rgba,
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

/// Fits the primitives into the 1×1×1 cube centered on the origin and
/// makes the model.
fn fit(mut primitives: Vec<Primitive>) -> Result<Model, ModelError> {
    if primitives
        .iter()
        .all(|primitive| primitive.indices.is_empty())
    {
        return Err(ModelError::NoTriangles);
    }
    let corners = primitives
        .iter()
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
            // Each distinct finite corner once. Bit patterns compare exactly.
            if fitted.is_finite() && hull.insert(fitted.to_array().map(f32::to_bits)) {
                hull_points.push(fitted.to_array());
            }
        }
        for normal in &mut primitive.normals {
            *normal = (Vec3::from(*normal) * normal_scale)
                .normalize_or_zero()
                .to_array();
        }
    }
    Ok(Model {
        key: new_key(),
        size: Vec3::select(thick, size, Vec3::ZERO).to_array(),
        primitives,
        hull_points,
        warnings: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The example project's model: a square pyramid 4 wide and 3 tall,
    /// with a checkered texture, all embedded in one `.gltf` file.
    fn example_folder() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-mesh/assets/models")
    }

    fn example_bytes() -> Vec<u8> {
        std::fs::read(example_folder().join("pyramid.gltf")).unwrap()
    }

    #[test]
    fn the_example_pyramid_loads_fitted_into_the_unit_cube() {
        let model = load_model(&example_bytes(), &example_folder(), ModelFormat::Gltf).unwrap();
        assert_eq!(model.size, [4.0, 3.0, 4.0]);
        assert_eq!(model.primitives.len(), 1);
        let primitive = &model.primitives[0];
        assert_eq!(primitive.normals.len(), primitive.positions.len());
        assert_eq!(primitive.uvs.len(), primitive.positions.len());
        for corner in &primitive.positions {
            assert!(corner.iter().all(|c| c.abs() <= 0.5 + 1e-6), "{corner:?}");
        }
        // The apex and the 4 base corners.
        assert_eq!(model.hull_points.len(), 5);
        let texture = primitive.material.base_color_texture.as_ref().unwrap();
        assert_eq!(
            texture.rgba.len(),
            texture.width as usize * texture.height as usize * 4
        );
    }

    #[test]
    fn each_load_is_a_different_model() {
        let a = load_model(&example_bytes(), &example_folder(), ModelFormat::Gltf).unwrap();
        let b = load_model(&example_bytes(), &example_folder(), ModelFormat::Gltf).unwrap();
        assert_ne!(a, b);
        assert_ne!(a.primitives[0].key(), b.primitives[0].key());
    }

    fn error(text: &str) -> String {
        load_model(text.as_bytes(), &example_folder(), ModelFormat::Gltf)
            .unwrap_err()
            .to_string()
    }

    #[test]
    fn broken_files_give_errors_not_crashes() {
        assert!(error("not a model").contains("isn't a valid glTF file"));
        assert!(error("{}").contains("isn't a valid glTF file"));
        // Valid JSON, but the mesh points at an accessor that isn't there.
        // `gltf` panics while checking it, which is caught.
        let bad_accessor = r#"{ "asset": { "version": "2.0" },
            "meshes": [ { "primitives": [ { "attributes": { "POSITION": 7 } } ] } ] }"#;
        let bad_accessor = error(bad_accessor);
        assert!(
            bad_accessor.contains("isn't a valid glTF file")
                || bad_accessor == "the file is damaged",
            "{bad_accessor}"
        );
        // A model with nothing to draw.
        assert_eq!(
            error(r#"{ "asset": { "version": "2.0" } }"#),
            "it has no triangles to draw"
        );
    }

    #[test]
    fn files_outside_the_folder_or_missing_are_named() {
        let using = |uri: &str| {
            format!(
                r#"{{ "asset": {{ "version": "2.0" }},
                    "buffers": [ {{ "uri": "{uri}", "byteLength": 36 }} ] }}"#
            )
        };
        assert!(
            error(&using("../../secret.bin")).contains("`../../secret.bin`, which isn't inside")
        );
        assert!(error(&using("C:/secret.bin")).contains("isn't inside"));
        assert!(error(&using("%2e%2e/secret.bin")).contains("isn't inside"));
        assert!(error(&using("gone%20away.bin")).contains("`gone%20away.bin`, which isn't there"));
    }

    #[test]
    fn formats_come_from_the_extension() {
        assert_eq!(
            ModelFormat::of(Path::new("a/chair.GLB")),
            Some(ModelFormat::Gltf)
        );
        assert_eq!(
            ModelFormat::of(Path::new("tree.gltf")),
            Some(ModelFormat::Gltf)
        );
        assert_eq!(
            ModelFormat::of(Path::new("Robot.Fbx")),
            Some(ModelFormat::Fbx)
        );
        assert_eq!(ModelFormat::of(Path::new("notes.txt")), None);
        assert_eq!(ModelFormat::of(Path::new("fbx")), None);
    }

    #[test]
    fn uris_are_percent_decoded() {
        assert_eq!(
            percent_decode("my%20model.bin").as_deref(),
            Some("my model.bin")
        );
        assert_eq!(percent_decode("bad%2"), None);
        assert_eq!(percent_decode("bad%zz"), None);
    }

    #[test]
    fn missing_normals_are_worked_out_facing_outward() {
        // One triangle in the XY plane, counter-clockwise seen from +Z.
        let normals = smooth_normals(
            &[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            &[0, 1, 2],
        );
        assert_eq!(normals, vec![[0.0, 0.0, 1.0]; 3]);
    }
}
