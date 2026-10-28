//! Writing fracture pieces as one binary glTF (`.glb`) file.
//!
//! A `.glb` is a 12-byte header, then a JSON chunk describing the scene and
//! a BIN chunk holding the vertex data the JSON points into. Each piece is a
//! node of its own (`Piece1`, `Piece2`, …), so a `MeshId` can pick one with
//! `#Piece3`, and loading the whole file gives the original shape.

use std::collections::HashMap;

use glam::{Vec2, Vec3};
use serde_json::{Value, json};

use crate::{Mesh, Surface, position_key};

/// glTF's numbers for its data types and buffer kinds.
const FLOAT: u32 = 5126;
const UNSIGNED_INT: u32 = 5125;
const ARRAY_BUFFER: u32 = 34962;
const ELEMENT_ARRAY_BUFFER: u32 = 34963;

/// A glTF metallic-roughness material for the pieces.
#[derive(Clone, Debug, PartialEq)]
pub struct GlbMaterial {
    pub name: String,
    /// Linear RGBA (glTF's baseColorFactor).
    pub base_color: [f32; 4],
    pub metallic: f32,
    pub roughness: f32,
    pub emissive: [f32; 3],
    /// alphaMode BLEND when true (see-through), OPAQUE otherwise.
    pub blend: bool,
    /// A texture file's URI, relative to the .glb's folder, for
    /// baseColorTexture (uses TEXCOORD_0), or None.
    pub base_color_texture: Option<String>,
}

/// A .glb with one node per piece named `Piece1`, `Piece2`, … (in order),
/// each with its own mesh: one primitive per Outer material it uses
/// (`materials[i]`) and one for its Interior faces (`interior`). Positions
/// are where the piece is (nodes have no transform), so loading the whole
/// file gives the original shape. Interior faces get flat normals of their
/// plane and planar uvs.
pub fn write_glb(pieces: &[Mesh], materials: &[GlbMaterial], interior: &GlbMaterial) -> Vec<u8> {
    let mut file = File::default();
    for material in materials {
        file.add_material(material);
    }
    let interior_material = file.add_material(interior);
    // An Outer surface without a material is drawn plain white.
    let mut plain = None;

    let mut nodes = Vec::new();
    let mut meshes = Vec::new();
    for (index, piece) in pieces.iter().enumerate() {
        let name = format!("Piece{}", index + 1);
        // Outer surfaces in material order, then the inside.
        let mut surfaces: Vec<Surface> = piece.triangles.iter().map(|t| t.surface).collect();
        surfaces.sort_by_key(|surface| match surface {
            Surface::Outer(material) => (0, *material),
            Surface::Interior => (1, 0),
        });
        surfaces.dedup();
        if surfaces.is_empty() {
            // glTF needs a mesh to have a primitive; an empty piece is a
            // node with nothing in it.
            nodes.push(json!({ "name": name }));
            continue;
        }
        let primitives: Vec<Value> = surfaces
            .into_iter()
            .map(|surface| {
                let material = match surface {
                    Surface::Interior => interior_material,
                    Surface::Outer(material) if (material as usize) < materials.len() => {
                        material as usize
                    }
                    Surface::Outer(_) => {
                        *plain.get_or_insert_with(|| file.add_material(&plain_material()))
                    }
                };
                file.add_primitive(&surface_triangles(piece, surface), material)
            })
            .collect();
        nodes.push(json!({ "name": name, "mesh": meshes.len() }));
        meshes.push(json!({ "name": name, "primitives": primitives }));
    }
    file.finish(nodes, meshes)
}

/// A primitive's vertices and triangles, ready to write.
#[derive(Default)]
struct Vertices {
    positions: Vec<Vec3>,
    normals: Vec<Vec3>,
    uvs: Vec<Vec2>,
    indices: Vec<u32>,
}

/// The triangles of `piece` on `surface`, with only the vertices they use.
/// Interior triangles get their plane's normal and uvs across it.
fn surface_triangles(piece: &Mesh, surface: Surface) -> Vertices {
    let mut vertices = Vertices::default();
    // Vertices already written, by the original index (Outer) or by
    // position and normal (Interior).
    let mut by_index: HashMap<u32, u32> = HashMap::new();
    let mut by_place: HashMap<([u32; 3], [u32; 3]), u32> = HashMap::new();
    for triangle in piece.triangles.iter().filter(|t| t.surface == surface) {
        let [a, b, c] = triangle
            .corners
            .map(|corner| piece.positions[corner as usize]);
        // A cap's triangles carry their plane's normal, exactly the same on
        // each, so their corners are shared. Thin triangles' own normals
        // wobble; theirs is only used when the carried one is missing or
        // plainly wrong.
        let carried = piece.normals[triangle.corners[0] as usize].normalize_or_zero();
        let flat = match (b - a).cross(c - a).try_normalize() {
            Some(own) if carried.dot(own) < 0.9 => own,
            _ => carried.normalize_or(Vec3::Y),
        };
        for corner in triangle.corners {
            let index = match surface {
                Surface::Outer(_) => *by_index.entry(corner).or_insert_with(|| {
                    vertices.push(
                        piece.positions[corner as usize],
                        piece.normals[corner as usize],
                        piece.uvs[corner as usize],
                    )
                }),
                Surface::Interior => {
                    let position = piece.positions[corner as usize];
                    let key = (position_key(position), position_key(flat));
                    *by_place.entry(key).or_insert_with(|| {
                        let u = flat.any_orthonormal_vector();
                        let v = flat.cross(u);
                        let uv = Vec2::new(position.dot(u), position.dot(v));
                        vertices.push(position, flat, uv)
                    })
                }
            };
            vertices.indices.push(index);
        }
    }
    vertices
}

impl Vertices {
    fn push(&mut self, position: Vec3, normal: Vec3, uv: Vec2) -> u32 {
        self.positions.push(position);
        self.normals.push(normal);
        self.uvs.push(uv);
        (self.positions.len() - 1) as u32
    }
}

/// A plain white surface, for an Outer material index with no material.
fn plain_material() -> GlbMaterial {
    GlbMaterial {
        name: "Plain".to_owned(),
        base_color: [1.0; 4],
        metallic: 0.0,
        roughness: 0.5,
        emissive: [0.0; 3],
        blend: false,
        base_color_texture: None,
    }
}

/// The glTF document being built, and its binary data.
#[derive(Default)]
struct File {
    bin: Vec<u8>,
    buffer_views: Vec<Value>,
    accessors: Vec<Value>,
    materials: Vec<Value>,
    images: Vec<Value>,
    textures: Vec<Value>,
    /// The texture already made for each image URI.
    texture_of: HashMap<String, usize>,
}

impl File {
    /// Adds `material` and returns its index.
    fn add_material(&mut self, material: &GlbMaterial) -> usize {
        let mut pbr = json!({
            "baseColorFactor": material.base_color.map(finite),
            "metallicFactor": finite(material.metallic).clamp(0.0, 1.0),
            "roughnessFactor": finite(material.roughness).clamp(0.0, 1.0),
        });
        if let Some(uri) = &material.base_color_texture {
            let texture = match self.texture_of.get(uri) {
                Some(&texture) => texture,
                None => {
                    self.images.push(json!({ "uri": encode_uri(uri) }));
                    self.textures
                        .push(json!({ "source": self.images.len() - 1 }));
                    let texture = self.textures.len() - 1;
                    self.texture_of.insert(uri.clone(), texture);
                    texture
                }
            };
            pbr["baseColorTexture"] = json!({ "index": texture });
        }
        self.materials.push(json!({
            "name": material.name,
            "pbrMetallicRoughness": pbr,
            "emissiveFactor": material.emissive.map(|c| finite(c).clamp(0.0, 1.0)),
            "alphaMode": if material.blend { "BLEND" } else { "OPAQUE" },
        }));
        self.materials.len() - 1
    }

    /// Writes a primitive's data and returns its JSON.
    fn add_primitive(&mut self, vertices: &Vertices, material: usize) -> Value {
        let (min, max) = vertices
            .positions
            .iter()
            .fold((Vec3::INFINITY, Vec3::NEG_INFINITY), |(min, max), &p| {
                (min.min(p), max.max(p))
            });
        let positions: Vec<f32> = vertices.positions.iter().flat_map(Vec3::to_array).collect();
        let normals: Vec<f32> = vertices.normals.iter().flat_map(Vec3::to_array).collect();
        let uvs: Vec<f32> = vertices.uvs.iter().flat_map(Vec2::to_array).collect();
        let count = vertices.positions.len();
        let position = self.add_accessor(&positions, count, "VEC3", ARRAY_BUFFER);
        self.accessors[position]["min"] = json!(min.to_array().map(finite));
        self.accessors[position]["max"] = json!(max.to_array().map(finite));
        let normal = self.add_accessor(&normals, count, "VEC3", ARRAY_BUFFER);
        let uv = self.add_accessor(&uvs, count, "VEC2", ARRAY_BUFFER);
        let indices = self.add_view(
            vertices.indices.iter().flat_map(|i| i.to_le_bytes()),
            ELEMENT_ARRAY_BUFFER,
        );
        self.accessors.push(json!({
            "bufferView": indices,
            "componentType": UNSIGNED_INT,
            "count": vertices.indices.len(),
            "type": "SCALAR",
        }));
        json!({
            "attributes": { "POSITION": position, "NORMAL": normal, "TEXCOORD_0": uv },
            "indices": self.accessors.len() - 1,
            "material": material,
        })
    }

    /// Writes `values` (`count` items of `kind`, such as "VEC3") and
    /// returns the accessor that reads them.
    fn add_accessor(&mut self, values: &[f32], count: usize, kind: &str, target: u32) -> usize {
        let view = self.add_view(
            values.iter().flat_map(|value| finite(*value).to_le_bytes()),
            target,
        );
        self.accessors.push(json!({
            "bufferView": view,
            "componentType": FLOAT,
            "count": count,
            "type": kind,
        }));
        self.accessors.len() - 1
    }

    /// Adds `bytes` to the binary data and returns the view onto them.
    /// Every value is 4 bytes, so views stay 4-byte aligned.
    fn add_view(&mut self, bytes: impl Iterator<Item = u8>, target: u32) -> usize {
        let start = self.bin.len();
        self.bin.extend(bytes);
        self.buffer_views.push(json!({
            "buffer": 0,
            "byteOffset": start,
            "byteLength": self.bin.len() - start,
            "target": target,
        }));
        self.buffer_views.len() - 1
    }

    /// The finished file: header, JSON chunk and BIN chunk.
    fn finish(self, nodes: Vec<Value>, meshes: Vec<Value>) -> Vec<u8> {
        let mut root = json!({
            "asset": { "version": "2.0", "generator": "Axcenity Engine" },
            "scene": 0,
            "scenes": [{ "nodes": (0..nodes.len()).collect::<Vec<_>>() }],
            "nodes": nodes,
            "meshes": meshes,
            "materials": self.materials,
            "accessors": self.accessors,
            "bufferViews": self.buffer_views,
        });
        if !self.bin.is_empty() {
            root["buffers"] = json!([{ "byteLength": self.bin.len() }]);
        }
        if !self.images.is_empty() {
            root["images"] = json!(self.images);
            root["textures"] = json!(self.textures);
        }
        let mut json = serde_json::to_vec(&root).expect("glTF JSON is plain data");
        // Chunks are padded to 4 bytes: JSON with spaces, BIN with zeros.
        while !json.len().is_multiple_of(4) {
            json.push(b' ');
        }
        let mut bin = self.bin;
        while !bin.len().is_multiple_of(4) {
            bin.push(0);
        }
        let mut total = 12 + 8 + json.len();
        if !bin.is_empty() {
            total += 8 + bin.len();
        }

        let mut glb = Vec::with_capacity(total);
        glb.extend_from_slice(b"glTF");
        glb.extend_from_slice(&2u32.to_le_bytes());
        glb.extend_from_slice(&(total as u32).to_le_bytes());
        glb.extend_from_slice(&(json.len() as u32).to_le_bytes());
        glb.extend_from_slice(b"JSON");
        glb.extend_from_slice(&json);
        if !bin.is_empty() {
            glb.extend_from_slice(&(bin.len() as u32).to_le_bytes());
            glb.extend_from_slice(b"BIN\0");
            glb.extend_from_slice(&bin);
        }
        glb
    }
}

/// `value`, or 0 if it isn't a number JSON can hold.
fn finite(value: f32) -> f32 {
    if value.is_finite() { value } else { 0.0 }
}

/// `path` as a URI: anything but letters, digits, `-`, `_`, `.`, `~` and
/// `/` written as `%XX`, so a file name with spaces still works.
fn encode_uri(path: &str) -> String {
    let mut uri = String::with_capacity(path.len());
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~' | b'/') {
            uri.push(byte as char);
        } else {
            uri.push_str(&format!("%{byte:02X}"));
        }
    }
    uri
}
