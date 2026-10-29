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
//!
//! An object's modifiers (Phase 57) are on it in what's drawn and saved;
//! its own mesh and the stack are kept in the sidecar (`sidecar.rs`). Its
//! paint (Phase 59, `paint.rs`) is on its materials as images; saving
//! flattens them to PNGs and keeps the layers in the sidecar folder.

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::Path;

use axcenity_assets::{
    Model, ModelError, ModelFormat, ModelMaterial, Pick, Primitive, list_objects, load_models,
    lod_level,
};
use glam::{Vec2, Vec3};
use serde_json::{Value, json};

use crate::glb::{File, Vertices};
use crate::half_edge::{EditMesh, Polygon, newell};
use crate::modifiers::{Modifier, ModifierKind, Purpose, evaluate};
use crate::paint::PaintSet;
use crate::primitives::PrimitiveSettings;

/// The share of triangles Generate LODs keeps in `_LOD1` and `_LOD2`, as
/// the renderer's own simplified copies do.
pub const LOD_RATIOS: [f32; 2] = [0.5, 0.25];

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
    /// Its modifier stack, first to last.
    pub modifiers: Vec<Modifier>,
    /// Its material slots being painted (Phase 59), one set each.
    pub paint: Vec<PaintSet>,
}

impl DocObject {
    /// Its mesh with its shown modifiers on it, for `purpose`; `None` for
    /// a group.
    pub fn evaluated(&self, purpose: Purpose) -> Option<Cow<'_, EditMesh>> {
        let mesh = self.mesh.as_ref()?;
        if self.modifiers.iter().all(|modifier| !modifier.show) {
            return Some(Cow::Borrowed(mesh));
        }
        Some(Cow::Owned(evaluate(mesh, &self.modifiers, purpose).mesh))
    }
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
                modifiers: Vec::new(),
                paint: Vec::new(),
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
                modifiers: Vec::new(),
                paint: Vec::new(),
            });
        }
        if objects.iter().all(|object| object.mesh.is_none()) {
            return Err(ModelError::NoTriangles);
        }
        Ok(ModelDoc { objects })
    }

    /// The objects with meshes and their modifiers, as one model to draw,
    /// fitted into the 1×1×1 cube as a loaded one is; `_LOD1` and `_LOD2`
    /// copies would draw over their full objects, so they're left out.
    /// `None` if there are no triangles.
    pub fn to_model(&self) -> Option<Model> {
        self.to_model_on(0)
    }

    /// [`to_model`](Self::to_model), textured by uv `channel` (1 draws the
    /// second uvs, as a lightmap would be placed).
    pub fn to_model_on(&self, channel: usize) -> Option<Model> {
        self.to_model_slots(channel).map(|(model, _)| model)
    }

    /// [`to_model_on`](Self::to_model_on), and the object and material
    /// slot each primitive draws (a slot past the object's last is drawn
    /// plain), so the view can show painted materials on it again.
    pub fn to_model_slots(&self, channel: usize) -> Option<(Model, Vec<(usize, usize)>)> {
        self.model_of(channel, None)
    }

    /// [`to_model`](Self::to_model) without object `skip` (which the
    /// Studio draws itself while it's sculpted). `None` if nothing's left.
    pub fn to_model_except(&self, skip: usize) -> Option<Model> {
        self.model_of(0, Some(skip)).map(|(model, _)| model)
    }

    fn model_of(
        &self,
        channel: usize,
        skip: Option<usize>,
    ) -> Option<(Model, Vec<(usize, usize)>)> {
        let mut slots = Vec::new();
        let primitives = (0..self.objects.len())
            .filter(|&index| !self.is_lod_copy(index) && Some(index) != skip)
            .filter_map(|index| {
                let object = &self.objects[index];
                let mut mesh = object.evaluated(Purpose::Viewport)?;
                if channel == 1 {
                    mesh.to_mut().swap_uv_channels();
                }
                let drawn = primitives(&mesh, &object.materials, true);
                Some(
                    drawn
                        .into_iter()
                        .map(move |(vertices, material, slot)| (vertices, material, (index, slot))),
                )
            })
            .flatten()
            .map(|(vertices, material, slot)| {
                slots.push(slot);
                Primitive::new(
                    vertices.positions.iter().map(|p| p.to_array()).collect(),
                    vertices.normals.iter().map(|n| n.to_array()).collect(),
                    vertices.uvs.iter().map(|uv| uv.to_array()).collect(),
                    vertices.indices,
                    material.clone(),
                )
            })
            .collect();
        Some((Model::from_primitives(primitives).ok()?, slots))
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
            if let Some(mesh) = object.evaluated(Purpose::Export) {
                let parts: Vec<Value> = primitives(&mesh, &object.materials, object.uvs)
                    .into_iter()
                    .map(|(vertices, material, _)| {
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

    /// True if object `index` is a `_LOD1` or `_LOD2` copy of another
    /// object with a mesh.
    pub fn is_lod_copy(&self, index: usize) -> bool {
        let name = self.objects[index].name.trim_end();
        lod_level(name) > 0 && {
            let full = &name[..name.len() - "_lod1".len()];
            self.objects
                .iter()
                .any(|object| object.mesh.is_some() && object.name == full)
        }
    }

    /// Generate LODs: `<name>_LOD1` and `<name>_LOD2` copies of object
    /// `index`, each its mesh and modifiers with a Decimate to
    /// [`LOD_RATIOS`] of the triangles added at the end, named as the
    /// renderer finds them. They go right after the object (and what's
    /// inside it), at its depth, in place of copies already there. Returns
    /// the object's index after; `None` for a group or a copy.
    pub fn generate_lods(&mut self, index: usize) -> Option<usize> {
        let object = self.objects.get(index)?;
        if object.mesh.is_none() || lod_level(&object.name) > 0 {
            return None;
        }
        let object = object.clone();
        let names: Vec<String> = (1..=LOD_RATIOS.len())
            .map(|level| format!("{}_LOD{level}", object.name))
            .collect();
        let mut index = index;
        let mut at = 0;
        self.objects.retain(|other| {
            let old =
                other.depth == object.depth && other.mesh.is_some() && names.contains(&other.name);
            if old && at < index {
                index -= 1;
            }
            at += 1;
            !old
        });
        let mut after = index + 1;
        while self
            .objects
            .get(after)
            .is_some_and(|inside| inside.depth > object.depth)
        {
            after += 1;
        }
        for (k, (name, ratio)) in names.into_iter().zip(LOD_RATIOS).enumerate() {
            let mut modifiers = object.modifiers.clone();
            modifiers.push(Modifier::new(ModifierKind::Decimate { ratio }));
            self.objects.insert(
                after + k,
                // Saving points the copies at the painted maps; they aren't
                // painted themselves.
                DocObject {
                    name,
                    modifiers,
                    paint: Vec::new(),
                    ..object.clone()
                },
            );
        }
        Some(index)
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
                uvs2: Vec::new(),
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
/// in slot order, with the slot (`usize::MAX` for the plain one). Corners
/// sharing a vertex, uvs and normal are written once. Without `uvs`, no
/// texture coordinates (and no second ones).
fn primitives<'a>(
    mesh: &EditMesh,
    materials: &'a [ModelMaterial],
    uvs: bool,
) -> Vec<(Vertices, &'a ModelMaterial, usize)> {
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
            let uvs2 = uvs && mesh.has_uv2();
            let mut written: HashMap<(u32, [u32; 4], [u32; 3]), u32> = HashMap::new();
            for face in (0..mesh.face_count() as u32).filter(|&f| slot_of(f) == slot) {
                for triangle in mesh.triangles(face) {
                    for corner in triangle {
                        let edge = mesh.half_edge(corner);
                        let uv = if uvs { edge.uv } else { Vec2::ZERO };
                        let uv2 = if uvs2 { edge.uv2 } else { Vec2::ZERO };
                        let both = [uv.x, uv.y, uv2.x, uv2.y];
                        let key = (
                            edge.vertex,
                            both.map(f32::to_bits),
                            edge.normal.to_array().map(f32::to_bits),
                        );
                        let index = *written.entry(key).or_insert_with(|| {
                            vertices.positions.push(mesh.position(edge.vertex));
                            vertices.normals.push(edge.normal);
                            if uvs {
                                vertices.uvs.push(uv);
                            }
                            if uvs2 {
                                vertices.uvs2.push(uv2);
                            }
                            (vertices.positions.len() - 1) as u32
                        });
                        vertices.indices.push(index);
                    }
                }
            }
            match materials.get(slot) {
                Some(material) => (vertices, material, slot),
                None => (vertices, &*PLAIN, usize::MAX),
            }
        })
        .collect()
}
