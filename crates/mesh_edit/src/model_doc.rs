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
//!
//! A model's armature and its objects' skin weights (Phase 62, `rig.rs`)
//! are saved as a glTF skin: the bones as nodes under an `Armature` node
//! after the objects, so object paths don't change, and each skinned
//! object's corners with their 4 strongest bones.

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::Path;

use axcenity_assets::{
    BoneTransform, Image, Model, ModelError, ModelFormat, ModelMaterial, Pick, Primitive,
    list_objects, load_models, lod_level,
};
use glam::{Vec2, Vec3};
use serde_json::{Value, json};

use crate::glb::{File, Vertices};
use crate::half_edge::{EditMesh, Polygon, newell};
use crate::modifiers::{Modifier, ModifierKind, Purpose, evaluate};
use crate::paint::PaintSet;
use crate::primitives::PrimitiveSettings;
use crate::rig::{Armature, Skin, vertex_skinning};

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
    /// Its weights to the model's armature (Phase 62), if it's skinned.
    pub skin: Option<Skin>,
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
    /// Its bones (Phase 62), if it's rigged.
    pub armature: Option<Armature>,
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
                skin: None,
            }],
            armature: None,
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
        let loaded = load_models(bytes, folder, format, &picks)?;
        // The skeleton with the most bones is the armature.
        let armature = loaded
            .iter()
            .filter_map(|model| model.rig.as_deref())
            .max_by_key(|rig| rig.bones.len())
            .map(|rig| Armature::from_rig(rig).0);
        let bones = armature.as_ref().map_or(Vec::new(), |arm| {
            let names = arm.bones.iter().map(|bone| bone.name.as_str());
            names.collect::<Vec<_>>()
        });
        let mut models = loaded.iter();
        let mut objects = Vec::new();
        for (k, object) in listed.iter().enumerate() {
            let (mesh, materials, uvs, skin) = if with_mesh.contains(&k) {
                let model = models.next().ok_or(ModelError::NoTriangles)?;
                let (mesh, materials) = from_model(model);
                let uvs = model.primitives.iter().any(|p| !p.uvs.is_empty());
                let skin = armature
                    .as_ref()
                    .and_then(|arm| skin_from_model(model, &mesh, arm));
                (Some(mesh), materials, uvs, skin)
            } else {
                // A group of bones only is the armature, not an object.
                if is_bone_group(&listed, k, &bones) {
                    continue;
                }
                (None, Vec::new(), false, None)
            };
            objects.push(DocObject {
                name: object.name.clone(),
                depth: object.depth,
                mesh,
                materials,
                uvs,
                modifiers: Vec::new(),
                paint: Vec::new(),
                skin,
            });
        }
        if objects.iter().all(|object| object.mesh.is_none()) {
            return Err(ModelError::NoTriangles);
        }
        Ok(ModelDoc { objects, armature })
    }

    /// The objects with meshes and their modifiers, posed by `pose` (one
    /// per rig bone, [`Armature::to_rig`]'s order) as the game skins them,
    /// as one model to draw. With `weights_of`, each skinned object is
    /// drawn with that armature bone's weights as colours (blue none, red
    /// all) and the others grey. `None` if there are no triangles.
    pub fn to_model_posed(
        &self,
        pose: &[BoneTransform],
        weights_of: Option<usize>,
    ) -> Option<Model> {
        let matrices = self
            .armature
            .as_ref()
            .map_or(Vec::new(), |arm| arm.skin_matrices(pose));
        let grey = ModelMaterial {
            name: "Unweighted".to_owned(),
            base_color: [0.35, 0.35, 0.38, 1.0],
            roughness: 0.6,
            ..ModelMaterial::default()
        };
        let ramp = weight_material();
        let mut primitives_out = Vec::new();
        for index in (0..self.objects.len()).filter(|&k| !self.is_lod_copy(k)) {
            let object = &self.objects[index];
            let Some(mesh) = object.evaluated(Purpose::Viewport) else {
                continue;
            };
            let skin = object.skin.as_ref().map(|skin| skin.fitted(&mesh));
            let skinning = skin
                .as_deref()
                .map(|skin| vertex_skinning(skin, mesh.vertex_count(), &matrices));
            for (mut vertices, material, _) in primitives(&mesh, &object.materials, true) {
                if let Some(skinning) = &skinning {
                    for (k, &vertex) in vertices.vertex.iter().enumerate() {
                        if let Some(Some(moved)) = skinning.get(vertex as usize) {
                            vertices.positions[k] = moved.position(vertices.positions[k]);
                            vertices.normals[k] = moved.normal(vertices.normals[k]);
                        }
                    }
                }
                let material = match (weights_of, &skin) {
                    (Some(bone), Some(skin)) => {
                        for (k, &vertex) in vertices.vertex.iter().enumerate() {
                            let w = skin.weight(vertex as usize, bone).clamp(0.0, 1.0);
                            vertices.uvs[k] = Vec2::new(w * 0.99 + 0.005, 0.5);
                        }
                        &ramp
                    }
                    (Some(_), None) => &grey,
                    (None, _) => material,
                };
                primitives_out.push(Primitive::new(
                    vertices.positions.iter().map(|p| p.to_array()).collect(),
                    vertices.normals.iter().map(|n| n.to_array()).collect(),
                    vertices.uvs.iter().map(|uv| uv.to_array()).collect(),
                    vertices.indices,
                    material.clone(),
                ));
            }
        }
        Model::from_primitives(primitives_out).ok()
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
        let mut slots = Vec::new();
        let primitives = (0..self.objects.len())
            .filter(|&index| !self.is_lod_copy(index))
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
        // The saved skeleton, and each armature bone's place in it.
        let rig = self
            .armature
            .as_ref()
            .filter(|arm| !arm.bones.is_empty())
            .map(Armature::to_rig);
        let place = rig.as_ref().map(|(_, place)| place.clone());
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
                let skin = match (&object.skin, &place) {
                    (Some(skin), Some(_)) => Some(skin.fitted(&mesh)),
                    _ => None,
                };
                let parts: Vec<Value> = primitives(&mesh, &object.materials, object.uvs)
                    .into_iter()
                    .map(|(mut vertices, material, _)| {
                        if let (Some(skin), Some(place)) = (&skin, &place) {
                            for &vertex in &vertices.vertex {
                                let (bones, weights) = skin.influences(vertex as usize);
                                // Joint k is the bone after the root at k + 1.
                                let joints = bones.map(|bone| {
                                    place.get(bone as usize).map_or(0, |&at| (at - 1) as u16)
                                });
                                vertices.joints.push(joints);
                                vertices.weights.push(weights);
                            }
                        }
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
                    if skin.is_some() {
                        json["skin"] = json!(0);
                    }
                }
            }
            nodes.push(json);
        }
        let mut skins = Vec::new();
        if let Some((rig, _)) = &rig {
            // The bones after the objects, so object paths stay the same.
            let first = nodes.len();
            roots.push(first);
            for (index, bone) in rig.bones.iter().enumerate() {
                let rest = bone.rest;
                let mut json = json!({
                    "name": bone.name,
                    "translation": rest.translation.to_array(),
                    "rotation": rest.rotation.normalize().to_array(),
                    "scale": rest.scale.to_array(),
                });
                let children: Vec<usize> = (0..rig.bones.len())
                    .filter(|&c| rig.bones[c].parent == Some(index))
                    .map(|c| first + c)
                    .collect();
                if !children.is_empty() {
                    json["children"] = json!(children);
                }
                nodes.push(json);
            }
            let inverse: Vec<glam::Mat4> = rig.joints.iter().map(|j| j.inverse_bind).collect();
            let accessor = file.add_matrices(&inverse);
            let joints: Vec<usize> = rig.joints.iter().map(|j| first + j.bone).collect();
            skins.push(json!({
                "name": crate::rig::ROOT_NAME,
                "joints": joints,
                "inverseBindMatrices": accessor,
                "skeleton": first,
            }));
        }
        file.finish_with(nodes, meshes, roots, skins)
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
                            vertices.vertex.push(edge.vertex);
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

/// True if listed object `k` has no mesh and nothing with one inside it,
/// and is a bone or holds one (by name, in `bones`): part of the armature.
fn is_bone_group(listed: &[axcenity_assets::ModelObject], k: usize, bones: &[&str]) -> bool {
    if bones.is_empty() {
        return false;
    }
    let depth = listed[k].depth;
    let inside = listed[k..]
        .iter()
        .enumerate()
        .take_while(|&(at, object)| at == 0 || object.depth > depth);
    let mut any_bone = false;
    for (_, object) in inside {
        if object.has_mesh() {
            return false;
        }
        any_bone |= bones.contains(&object.name.as_str());
    }
    any_bone
}

/// A loaded skinned model's weights on the mesh [`from_model`] made of it,
/// with its skeleton's bones found in `armature` by name. `None` if the
/// model isn't skinned.
fn skin_from_model(model: &Model, mesh: &EditMesh, armature: &Armature) -> Option<Skin> {
    let rig = model.rig.as_deref()?;
    let size = Vec3::from(model.size);
    let scale = Vec3::select(size.cmpgt(Vec3::ZERO), size, Vec3::ONE);
    let center = Vec3::from(model.center);
    let vertex_at: HashMap<[u32; 3], usize> = (0..mesh.vertex_count())
        .map(|v| (crate::position_key(mesh.position(v as u32)), v))
        .collect();
    // Each of the rig's joints' armature bone.
    let bone_of: Vec<Option<u16>> = rig
        .joints
        .iter()
        .map(|joint| {
            let name = &rig.bones.get(joint.bone)?.name;
            armature.bone(name).map(|b| b as u16)
        })
        .collect();
    let mut weights: Vec<Option<Vec<(u16, f32)>>> = vec![None; mesh.vertex_count()];
    let mut any = false;
    for primitive in &model.primitives {
        let count = primitive.joints.len().min(primitive.weights.len());
        for (corner, &position) in primitive.positions.iter().enumerate().take(count) {
            let key = crate::position_key(Vec3::from(position) * scale + center);
            let Some(&vertex) = vertex_at.get(&key) else {
                continue;
            };
            if weights[vertex].is_some() {
                continue;
            }
            let list: Vec<(u16, f32)> = primitive.joints[corner]
                .iter()
                .zip(primitive.weights[corner])
                .filter(|&(_, w)| w > 0.0)
                .filter_map(|(&joint, w)| Some((bone_of.get(joint as usize).copied()??, w)))
                .collect();
            any |= !list.is_empty();
            weights[vertex] = Some(list);
        }
    }
    any.then(|| {
        Skin::new(
            mesh,
            weights.into_iter().map(Option::unwrap_or_default).collect(),
        )
    })
}

/// The weight colours: a ramp across a texture, blue for 0 through green
/// to red for 1, which the corners' uvs pick from.
fn weight_material() -> ModelMaterial {
    static RAMP: std::sync::LazyLock<std::sync::Arc<Image>> = std::sync::LazyLock::new(|| {
        let width = 256u32;
        let rgba = (0..width * 4)
            .flat_map(|k| {
                let t = (k % width) as f32 / (width - 1) as f32;
                let [r, g, b] = weight_color(t);
                [r, g, b, 255]
            })
            .collect();
        std::sync::Arc::new(Image::new(width, 4, rgba))
    });
    ModelMaterial {
        name: "Weights".to_owned(),
        base_color: [1.0; 4],
        base_color_texture: Some(RAMP.clone()),
        roughness: 0.7,
        ..ModelMaterial::default()
    }
}

/// The colour of weight `t` (0 to 1), in sRGB bytes: blue, cyan, green,
/// yellow, red.
pub fn weight_color(t: f32) -> [u8; 3] {
    let stops = [
        [0.0, 0.0, 1.0],
        [0.0, 1.0, 1.0],
        [0.0, 1.0, 0.0],
        [1.0, 1.0, 0.0],
        [1.0, 0.0, 0.0],
    ];
    let at = t.clamp(0.0, 1.0) * 4.0;
    let k = (at.floor() as usize).min(3);
    let f = at - k as f32;
    let mix = |c: usize| stops[k][c] + (stops[k + 1][c] - stops[k][c]) * f;
    [0, 1, 2].map(|c| (mix(c) * 255.0).round() as u8)
}
