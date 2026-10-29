//! A model file as the Model Studio edits it (Phase 55): its objects, each
//! an [`EditMesh`] in the file's units with its material slots, opened
//! from a `.glb`, `.gltf` or `.fbx` and saved as a `.glb`.
//!
//! Opening goes through `assets`, so the Studio reads a file exactly as a
//! MeshPart does: each object is loaded where it is in the file (its
//! nodes' transforms applied) and welded by position into polygons.
//! Saving writes each object as a node with no transform, under nodes for
//! the groups it was in, so object paths (`file.glb#Group/Chair`) still
//! pick the same objects.

use std::collections::HashMap;
use std::path::Path;

use axcenity_assets::{
    Model, ModelError, ModelFormat, ModelMaterial, Pick, Primitive, list_objects, load_models,
};
use glam::{Vec2, Vec3};
use serde_json::{Value, json};

use crate::glb::{File, Vertices};
use crate::half_edge::{EditMesh, Polygon, newell};
use crate::primitives::PrimitiveSettings;

/// Corner normals this close to the face's own (as a cosine) count as
/// flat shading.
const FLAT: f32 = 0.9999;

/// One object in the file: a mesh, or a group of the objects after it.
#[derive(Clone, Debug)]
pub struct DocObject {
    /// Its name in the file.
    pub name: String,
    /// 0 at the top; the objects right after one with a smaller depth are
    /// inside it.
    pub depth: usize,
    /// `None` for a group.
    pub mesh: Option<EditMesh>,
    /// What each face's material number draws it with.
    pub materials: Vec<ModelMaterial>,
    /// False if the file gave it no texture coordinates, so none are saved.
    pub uvs: bool,
}

/// The objects of one model file.
#[derive(Clone, Debug, Default)]
pub struct ModelDoc {
    pub objects: Vec<DocObject>,
}

impl ModelDoc {
    /// A new model of one shape, named after it, drawn with a plain
    /// material.
    pub fn primitive(settings: &PrimitiveSettings) -> ModelDoc {
        let material = ModelMaterial {
            name: "Material".to_owned(),
            base_color: [0.8, 0.8, 0.8, 1.0],
            roughness: 0.5,
            ..ModelMaterial::default()
        };
        ModelDoc {
            objects: vec![DocObject {
                name: settings.shape.label().replace(' ', ""),
                depth: 0,
                mesh: Some(settings.build()),
                materials: vec![material],
                uvs: true,
            }],
        }
    }

    /// Reads a model file's `bytes` (in `format`), whose other files are in
    /// `folder`.
    pub fn open(bytes: &[u8], folder: &Path, format: ModelFormat) -> Result<ModelDoc, ModelError> {
        let listed = list_objects(bytes, format)?;
        let with_mesh: Vec<usize> = (0..listed.len())
            .filter(|&k| listed[k].has_mesh())
            .collect();
        let picks: Vec<Pick> = with_mesh
            .iter()
            .map(|&k| Pick::Objects(vec![listed[k].path.clone()]))
            .collect();
        // Read once, so textures the objects share are decoded once.
        let mut models = load_models(bytes, folder, format, &picks)?.into_iter();
        let mut objects = Vec::new();
        for (k, object) in listed.iter().enumerate() {
            let (mesh, materials, uvs) = if with_mesh.contains(&k) {
                let model = models.next().ok_or(ModelError::NoTriangles)?;
                let (mesh, materials) = from_model(&model);
                let uvs = model.primitives.iter().any(|p| !p.uvs.is_empty());
                (Some(mesh), materials, uvs)
            } else {
                (None, Vec::new(), false)
            };
            objects.push(DocObject {
                name: object.name.clone(),
                depth: object.depth,
                mesh,
                materials,
                uvs,
            });
        }
        if objects.iter().all(|object| object.mesh.is_none()) {
            return Err(ModelError::NoTriangles);
        }
        Ok(ModelDoc { objects })
    }

    /// The objects with meshes, as one model to draw, fitted into the
    /// 1×1×1 cube as a loaded one is. `None` if there are no triangles.
    pub fn to_model(&self) -> Option<Model> {
        let primitives = self
            .objects
            .iter()
            .filter_map(|object| Some(primitives(object.mesh.as_ref()?, &object.materials, true)))
            .flatten()
            .map(|(vertices, material)| {
                Primitive::new(
                    vertices.positions.iter().map(|p| p.to_array()).collect(),
                    vertices.normals.iter().map(|n| n.to_array()).collect(),
                    vertices.uvs.iter().map(|uv| uv.to_array()).collect(),
                    vertices.indices,
                    material.clone(),
                )
            })
            .collect();
        Model::from_primitives(primitives).ok()
    }

    /// The model as a `.glb` file: a node per object, under its groups'
    /// nodes, with a primitive per material it uses. Materials keep their
    /// values and link their textures by file, relative to the model.
    pub fn write_glb(&self) -> Vec<u8> {
        let mut file = File::default();
        // Each distinct material once, so objects sharing one still share it.
        let mut material_of: HashMap<String, usize> = HashMap::new();
        let mut nodes: Vec<Value> = Vec::new();
        let mut meshes: Vec<Value> = Vec::new();
        let mut roots = Vec::new();
        // The node each depth's open object is, to hang children on.
        let mut parents: Vec<usize> = Vec::new();
        for object in &self.objects {
            let node = nodes.len();
            parents.truncate(object.depth);
            match parents.last() {
                Some(&parent) => {
                    let children = nodes[parent]
                        .as_object_mut()
                        .expect("nodes are objects")
                        .entry("children")
                        .or_insert_with(|| json!([]));
                    children.as_array_mut().expect("a list").push(json!(node));
                }
                None => roots.push(node),
            }
            parents.push(node);
            let mut json = json!({ "name": object.name });
            if let Some(mesh) = &object.mesh {
                let parts: Vec<Value> = primitives(mesh, &object.materials, object.uvs)
                    .into_iter()
                    .map(|(vertices, material)| {
                        let key = format!("{material:?}");
                        let index = match material_of.get(&key) {
                            Some(&index) => index,
                            None => {
                                let index = file.add_model_material(material);
                                material_of.insert(key, index);
                                index
                            }
                        };
                        file.add_primitive(&vertices, index)
                    })
                    .collect();
                if !parts.is_empty() {
                    json["mesh"] = json!(meshes.len());
                    meshes.push(json!({ "name": object.name, "primitives": parts }));
                }
            }
            nodes.push(json);
        }
        file.finish(nodes, meshes, roots)
    }
}

/// A loaded model's triangles as one editable mesh in the file's units
/// (loading fitted them into the 1×1×1 cube, so that's undone), with
/// primitive `i` drawn by material slot `i`. Corners at one place become
/// one vertex; each keeps its own uv and normal. A face whose corner
/// normals all match its own is flat; otherwise it's smooth.
pub fn from_model(model: &Model) -> (EditMesh, Vec<ModelMaterial>) {
    let size = Vec3::from(model.size);
    let scale = Vec3::select(size.cmpgt(Vec3::ZERO), size, Vec3::ONE);
    let center = Vec3::from(model.center);
    let mut positions: Vec<Vec3> = Vec::new();
    let mut at: HashMap<[u32; 3], u32> = HashMap::new();
    let mut polygons = Vec::new();
    let mut materials = Vec::new();
    for (slot, primitive) in model.primitives.iter().enumerate() {
        materials.push(primitive.material.clone());
        let mut vertex_of = Vec::with_capacity(primitive.positions.len());
        for &position in &primitive.positions {
            let position = Vec3::from(position) * scale + center;
            let key = crate::position_key(position);
            vertex_of.push(*at.entry(key).or_insert_with(|| {
                positions.push(position);
                (positions.len() - 1) as u32
            }));
        }
        let count = primitive.positions.len() as u32;
        for &triangle in primitive.indices.as_chunks::<3>().0 {
            if triangle.iter().any(|&corner| corner >= count) {
                continue;
            }
            let corners = triangle.map(|k| vertex_of[k as usize]);
            let uvs = triangle.map(|k| {
                primitive
                    .uvs
                    .get(k as usize)
                    .map_or(Vec2::ZERO, |&uv| Vec2::from(uv))
            });
            let normals: Vec<Vec3> = triangle
                .iter()
                .filter_map(|&k| primitive.normals.get(k as usize))
                .map(|&n| (Vec3::from(n) / scale).normalize_or_zero())
                .collect();
            let flat = newell(&corners.map(|v| positions[v as usize]));
            let smooth = normals.len() == 3 && normals.iter().any(|n| n.dot(flat) < FLAT);
            polygons.push(Polygon {
                corners: corners.to_vec(),
                uvs: uvs.to_vec(),
                normals: if normals.len() == 3 {
                    normals
                } else {
                    Vec::new()
                },
                material: slot as u32,
                smooth,
            });
        }
    }
    (EditMesh::new(positions, &polygons), materials)
}

/// `mesh`'s faces as triangles, one set per material slot used (a face
/// whose slot isn't there is drawn with the first, or a plain material),
/// in slot order. Corners sharing a vertex, uv and normal are written
/// once. Without `uvs`, no texture coordinates.
fn primitives<'a>(
    mesh: &EditMesh,
    materials: &'a [ModelMaterial],
    uvs: bool,
) -> Vec<(Vertices, &'a ModelMaterial)> {
    static PLAIN: std::sync::LazyLock<ModelMaterial> =
        std::sync::LazyLock::new(ModelMaterial::default);
    let slot_of = |face: u32| {
        let slot = mesh.face(face).material as usize;
        if slot < materials.len() { slot } else { 0 }
    };
    let mut slots: Vec<usize> = (0..mesh.face_count() as u32).map(slot_of).collect();
    slots.sort_unstable();
    slots.dedup();
    slots
        .into_iter()
        .map(|slot| {
            let mut vertices = Vertices::default();
            let mut written: HashMap<(u32, [u32; 2], [u32; 3]), u32> = HashMap::new();
            for face in (0..mesh.face_count() as u32).filter(|&f| slot_of(f) == slot) {
                for triangle in mesh.triangles(face) {
                    for corner in triangle {
                        let edge = mesh.half_edge(corner);
                        let uv = if uvs { edge.uv } else { Vec2::ZERO };
                        let key = (
                            edge.vertex,
                            uv.to_array().map(f32::to_bits),
                            edge.normal.to_array().map(f32::to_bits),
                        );
                        let index = *written.entry(key).or_insert_with(|| {
                            vertices.positions.push(mesh.position(edge.vertex));
                            vertices.normals.push(edge.normal);
                            if uvs {
                                vertices.uvs.push(uv);
                            }
                            (vertices.positions.len() - 1) as u32
                        });
                        vertices.indices.push(index);
                    }
                }
            }
            (vertices, materials.get(slot).unwrap_or(&PLAIN))
        })
        .collect()
}
