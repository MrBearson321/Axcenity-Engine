//! A small glTF 2.0 binary (`.glb`) writer for converted scenes: a node
//! tree, meshes of indexed triangles, metallic-roughness materials and
//! embedded images. Only what the engine's glTF reader uses.

use serde_json::{Value, json};

/// A scene to write. Indices refer into its own lists.
#[derive(Debug, Default)]
pub(crate) struct Scene {
    pub nodes: Vec<Node>,
    /// The nodes at the top of the tree.
    pub roots: Vec<usize>,
    pub meshes: Vec<Mesh>,
    pub materials: Vec<Material>,
    pub images: Vec<EmbeddedImage>,
}

#[derive(Debug)]
pub(crate) struct Node {
    pub name: String,
    /// Relative to its parent, column-major, as glTF stores it.
    pub matrix: [f32; 16],
    pub mesh: Option<usize>,
    pub children: Vec<usize>,
}

#[derive(Debug, Default)]
pub(crate) struct Mesh {
    pub primitives: Vec<Primitive>,
}

#[derive(Debug, Default)]
pub(crate) struct Primitive {
    pub positions: Vec<[f32; 3]>,
    /// One per position, or empty for the reader to make smooth ones.
    pub normals: Vec<[f32; 3]>,
    /// One per position (glTF's top-left origin), or empty.
    pub uvs: Vec<[f32; 2]>,
    /// Three per triangle, counter-clockwise from the front.
    pub indices: Vec<u32>,
    pub material: Option<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Material {
    pub name: String,
    pub base_color: [f32; 4],
    pub metallic: f32,
    pub roughness: f32,
    pub emissive: [f32; 3],
    pub base_color_texture: Option<usize>,
    /// Roughness in green, metalness in blue (glTF's layout).
    pub metallic_roughness_texture: Option<usize>,
    pub normal_texture: Option<usize>,
    /// Occlusion in red.
    pub occlusion_texture: Option<usize>,
    pub emissive_texture: Option<usize>,
    pub alpha: Alpha,
    pub double_sided: bool,
}

impl Default for Material {
    fn default() -> Self {
        Self {
            name: String::new(),
            base_color: [0.18, 0.18, 0.18, 1.0],
            metallic: 0.0,
            roughness: 0.5,
            emissive: [0.0; 3],
            base_color_texture: None,
            metallic_roughness_texture: None,
            normal_texture: None,
            occlusion_texture: None,
            emissive_texture: None,
            alpha: Alpha::Opaque,
            double_sided: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Alpha {
    Opaque,
    /// See-through below this alpha, solid above.
    Mask(f32),
    Blend,
}

/// An image file kept inside the `.glb`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct EmbeddedImage {
    /// A file name, which importing uses when it copies textures out.
    pub name: String,
    /// `image/png` or `image/jpeg`.
    pub mime: &'static str,
    pub bytes: Vec<u8>,
}

/// The binary chunk and the JSON that points into it.
#[derive(Default)]
struct Writer {
    bin: Vec<u8>,
    buffer_views: Vec<Value>,
    accessors: Vec<Value>,
}

const ARRAY_BUFFER: u32 = 34962;
const ELEMENT_ARRAY_BUFFER: u32 = 34963;
const FLOAT: u32 = 5126;
const UNSIGNED_INT: u32 = 5125;

impl Writer {
    /// Adds `bytes` as a buffer view (4-byte aligned), returning its index.
    fn view(&mut self, bytes: &[u8], target: Option<u32>) -> usize {
        while !self.bin.len().is_multiple_of(4) {
            self.bin.push(0);
        }
        let mut view = json!({
            "buffer": 0,
            "byteOffset": self.bin.len(),
            "byteLength": bytes.len(),
        });
        if let Some(target) = target {
            view["target"] = json!(target);
        }
        self.bin.extend_from_slice(bytes);
        self.buffer_views.push(view);
        self.buffer_views.len() - 1
    }

    fn floats<const N: usize>(&mut self, values: &[[f32; N]], kind: &str, bounds: bool) -> usize {
        let bytes: Vec<u8> = values
            .iter()
            .flatten()
            .flat_map(|value| value.to_le_bytes())
            .collect();
        let view = self.view(&bytes, Some(ARRAY_BUFFER));
        let mut accessor = json!({
            "bufferView": view,
            "componentType": FLOAT,
            "count": values.len(),
            "type": kind,
        });
        if bounds {
            let mut min = [f32::INFINITY; N];
            let mut max = [f32::NEG_INFINITY; N];
            for value in values {
                for axis in 0..N {
                    min[axis] = min[axis].min(value[axis]);
                    max[axis] = max[axis].max(value[axis]);
                }
            }
            accessor["min"] = json!(min.to_vec());
            accessor["max"] = json!(max.to_vec());
        }
        self.accessors.push(accessor);
        self.accessors.len() - 1
    }

    fn indices(&mut self, indices: &[u32]) -> usize {
        let bytes: Vec<u8> = indices
            .iter()
            .flat_map(|index| index.to_le_bytes())
            .collect();
        let view = self.view(&bytes, Some(ELEMENT_ARRAY_BUFFER));
        self.accessors.push(json!({
            "bufferView": view,
            "componentType": UNSIGNED_INT,
            "count": indices.len(),
            "type": "SCALAR",
        }));
        self.accessors.len() - 1
    }
}

/// The `.glb` file of `scene`.
pub(crate) fn write(scene: &Scene) -> Vec<u8> {
    let mut writer = Writer::default();
    let meshes: Vec<Value> = scene
        .meshes
        .iter()
        .map(|mesh| {
            let primitives: Vec<Value> = mesh
                .primitives
                .iter()
                .map(|primitive| {
                    let mut attributes =
                        json!({ "POSITION": writer.floats(&primitive.positions, "VEC3", true) });
                    if !primitive.normals.is_empty() {
                        attributes["NORMAL"] =
                            json!(writer.floats(&primitive.normals, "VEC3", false));
                    }
                    if !primitive.uvs.is_empty() {
                        attributes["TEXCOORD_0"] =
                            json!(writer.floats(&primitive.uvs, "VEC2", false));
                    }
                    let mut out = json!({
                        "attributes": attributes,
                        "indices": writer.indices(&primitive.indices),
                    });
                    if let Some(material) = primitive.material {
                        out["material"] = json!(material);
                    }
                    out
                })
                .collect();
            json!({ "primitives": primitives })
        })
        .collect();
    let images: Vec<Value> = scene
        .images
        .iter()
        .map(|image| {
            let view = writer.view(&image.bytes, None);
            json!({ "name": image.name, "mimeType": image.mime, "bufferView": view })
        })
        .collect();
    let textures: Vec<Value> = (0..scene.images.len())
        .map(|image| json!({ "source": image, "sampler": 0 }))
        .collect();
    let materials: Vec<Value> = scene.materials.iter().map(material).collect();
    let nodes: Vec<Value> = scene
        .nodes
        .iter()
        .map(|node| {
            let mut out = json!({ "name": node.name });
            if node.matrix != IDENTITY {
                out["matrix"] = json!(node.matrix);
            }
            if let Some(mesh) = node.mesh {
                out["mesh"] = json!(mesh);
            }
            if !node.children.is_empty() {
                out["children"] = json!(node.children);
            }
            out
        })
        .collect();

    while !writer.bin.len().is_multiple_of(4) {
        writer.bin.push(0);
    }
    let mut document = json!({
        "asset": { "version": "2.0", "generator": "Axcenity convert" },
        "scene": 0,
        "scenes": [{ "nodes": scene.roots }],
        "nodes": nodes,
        "meshes": meshes,
        "buffers": [{ "byteLength": writer.bin.len() }],
        "bufferViews": writer.buffer_views,
        "accessors": writer.accessors,
    });
    if !materials.is_empty() {
        document["materials"] = json!(materials);
    }
    if !images.is_empty() {
        document["images"] = json!(images);
        document["textures"] = json!(textures);
        // Repeat, with mipmaps: what UsdUVTexture does unless told otherwise.
        document["samplers"] = json!([{ "wrapS": 10497, "wrapT": 10497 }]);
    }

    let mut text = serde_json::to_vec(&document).expect("a JSON value always serializes");
    while !text.len().is_multiple_of(4) {
        text.push(b' ');
    }
    let length = 12 + 8 + text.len() + 8 + writer.bin.len();
    let mut out = Vec::with_capacity(length);
    out.extend_from_slice(b"glTF");
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(length as u32).to_le_bytes());
    out.extend_from_slice(&(text.len() as u32).to_le_bytes());
    out.extend_from_slice(b"JSON");
    out.extend_from_slice(&text);
    out.extend_from_slice(&(writer.bin.len() as u32).to_le_bytes());
    out.extend_from_slice(b"BIN\0");
    out.extend_from_slice(&writer.bin);
    out
}

pub(crate) const IDENTITY: [f32; 16] = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
];

fn material(material: &Material) -> Value {
    let texture = |index: Option<usize>| index.map(|index| json!({ "index": index }));
    let mut pbr = json!({
        "baseColorFactor": material.base_color,
        "metallicFactor": material.metallic,
        "roughnessFactor": material.roughness,
    });
    if let Some(texture) = texture(material.base_color_texture) {
        pbr["baseColorTexture"] = texture;
    }
    if let Some(texture) = texture(material.metallic_roughness_texture) {
        pbr["metallicRoughnessTexture"] = texture;
    }
    let mut out = json!({
        "name": material.name,
        "pbrMetallicRoughness": pbr,
        "emissiveFactor": material.emissive,
        "doubleSided": material.double_sided,
    });
    if let Some(texture) = texture(material.normal_texture) {
        out["normalTexture"] = texture;
    }
    if let Some(texture) = texture(material.occlusion_texture) {
        out["occlusionTexture"] = texture;
    }
    if let Some(texture) = texture(material.emissive_texture) {
        out["emissiveTexture"] = texture;
    }
    match material.alpha {
        Alpha::Opaque => {}
        Alpha::Mask(cutoff) => {
            out["alphaMode"] = json!("MASK");
            out["alphaCutoff"] = json!(cutoff);
        }
        Alpha::Blend => out["alphaMode"] = json!("BLEND"),
    }
    out
}
