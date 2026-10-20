//! Reading FBX models (binary or ASCII, any version) with `ufbx`, into the
//! same triangles and materials as glTF.
//!
//! `ufbx` converts the file to glTF's space as it loads: Y up, right-handed,
//! 1 unit = 1 meter (= 1 stud), whatever axes and units the file was saved
//! with. It reads only the bytes it's given; textures that aren't embedded
//! are looked for here, inside the model's folder.
//!
//! FBX files often name textures by a path on the machine they were made
//! on, or in a format the engine can't decode (`.psd`, `.dds`). Such a
//! texture is left out and noted in [`Model::warnings`]; the model still
//! loads, drawn with its materials' colors.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use glam::{DMat3, DMat4, DVec3, Mat4, Quat, Vec3};
use ufbx::{CoordinateAxes, CoordinateAxis, ShaderType};

use super::maps::{self, Role};
use super::objects::{self, Listed, Siblings};
use super::{
    AlphaMode, Bone, BoneTransform, Channel, ChannelTexture, Image, Joint, Keys, MAX_NODE_DEPTH,
    Model, ModelError, ModelMaterial, ModelObject, Pick, Primitive, Rig, Take, Track, decode_image,
    fit, fit_matrix, missing, new_key, smooth_normals, want,
};

/// Takes are baked at this many frames a second when the file doesn't say.
const DEFAULT_FRAME_RATE: f64 = 30.0;

/// Reads an FBX file's `bytes`, making one model for each of `picks`.
/// Textures that aren't embedded in it are read from `folder`, the folder
/// the model is in.
pub(super) fn load(bytes: &[u8], folder: &Path, picks: &[Pick]) -> Result<Vec<Model>, ModelError> {
    let scene = parse(bytes)?;
    let mut reader = Reader {
        animated: animated_nodes(&scene),
        joints: Vec::new(),
        folder,
        images: HashMap::new(),
        materials: HashMap::new(),
        loose: loose_textures(&scene),
        embedded: embedded_files(&scene)
            .into_iter()
            .flat_map(|(name, _, ids)| ids.into_iter().map(move |id| (id, name.clone())))
            .collect(),
        primitives: Vec::new(),
        warnings: Vec::new(),
    };
    let mut models = Vec::new();
    for pick in picks {
        // Each model notes its own materials' problems.
        reader.materials.clear();
        reader.joints.clear();
        let wanted = pick.paths();
        let mut found = HashSet::new();
        reader.children(&scene.root_node, "", &wanted, &mut found);
        if let Some(path) = missing(&wanted, &found) {
            return Err(ModelError::NoObject(path));
        }
        let mut model = fit(std::mem::take(&mut reader.primitives))?;
        if !reader.joints.is_empty() {
            model.rig = Some(Arc::new(rig(&scene, &reader.joints, fit_matrix(&model))));
        }
        // A texture that can't be used is noted by the first model that has it.
        model.warnings = std::mem::take(&mut reader.warnings);
        models.push(model);
    }
    Ok(models)
}

/// Textures no material links (often embedded maps an exporter didn't
/// connect), each with the element ID of the material it's for, if any:
/// the file's only material, or the one whose name is the longest part
/// of the texture's name (`Bench` for `T_Bench_Normal.png`).
fn loose_textures(scene: &ufbx::Scene) -> Vec<(&ufbx::Texture, Option<u32>)> {
    let keys: Vec<(String, u32)> = scene
        .materials
        .iter()
        .map(|material| {
            (
                maps::name_key(&material.element.name),
                material.element.element_id,
            )
        })
        .filter(|(key, _)| key.len() >= 3)
        .collect();
    let only = match scene.materials.len() {
        1 => Some(scene.materials[0].element.element_id),
        _ => None,
    };
    unlinked_textures(scene)
        .into_iter()
        .map(|texture| {
            let name = maps::name_key(&texture_name(texture));
            let owner = only.or_else(|| {
                keys.iter()
                    .filter(|(key, _)| name.contains(key.as_str()))
                    .max_by_key(|(key, _)| key.len())
                    .map(|(_, id)| *id)
            });
            (texture, owner)
        })
        .collect()
}

fn unlinked_textures(scene: &ufbx::Scene) -> Vec<&ufbx::Texture> {
    let linked: HashSet<u32> = scene
        .materials
        .iter()
        .flat_map(|material| material.textures.iter())
        .flat_map(|link| {
            std::iter::once(&link.texture)
                .chain(link.texture.file_textures.iter())
                .map(|texture| texture.element.element_id)
        })
        .collect();
    scene
        .textures
        .iter()
        .filter(|texture| texture.type_ == ufbx::TextureType::File)
        .filter(|texture| !linked.contains(&texture.element.element_id))
        .map(|texture| &**texture)
        .collect()
}

/// The image files embedded in an FBX file, as file names (made safe, and
/// unique) and their bytes as they are, for copying out on import.
pub(super) fn embedded_textures(bytes: &[u8]) -> Result<Vec<(String, Vec<u8>)>, ModelError> {
    let scene = parse(bytes)?;
    Ok(embedded_files(&scene)
        .into_iter()
        .map(|(name, content, _)| (name, content.to_vec()))
        .collect())
}

/// The scene's embedded images: each one's file name (made safe, and
/// unique), its bytes, and the element IDs of the textures that use it.
/// Loading and importing both name them this way, so an image's
/// [`Image::file`] is where importing copies it.
fn embedded_files(scene: &ufbx::Scene) -> Vec<(String, &[u8], Vec<u32>)> {
    let mut files: Vec<(String, &[u8], Vec<u32>)> = Vec::new();
    for texture in &scene.textures {
        if texture.content.is_empty() {
            continue;
        }
        let content: &[u8] = &texture.content;
        let id = texture.element.element_id;
        // The same image used by several textures is one file.
        if let Some((_, _, ids)) = files.iter_mut().find(|(_, bytes, _)| *bytes == content) {
            ids.push(id);
            continue;
        }
        let name = match crate::import::safe_file_name(file_name(texture)) {
            Some(name) => name,
            None => {
                let extension = image::guess_format(content)
                    .ok()
                    .and_then(|format| format.extensions_str().first().copied())
                    .unwrap_or("bin");
                format!("embedded{}.{extension}", files.len() + 1)
            }
        };
        let name = crate::import::unique_file_name(&name, |taken| {
            files.iter().any(|(n, _, _)| n.eq_ignore_ascii_case(taken))
        });
        files.push((name, content, vec![id]));
    }
    files
}

/// The objects in an FBX file: its visible nodes with meshes, and the
/// nodes they're in.
pub(super) fn list(bytes: &[u8]) -> Result<Vec<ModelObject>, ModelError> {
    let scene = parse(bytes)?;
    fn children(node: &ufbx::Node, path: &str, depth: usize) -> Vec<Listed> {
        let mut siblings = Siblings::default();
        node.children
            .iter()
            .map(|child| {
                let name = node_name(child);
                let path = siblings.path(path, &name);
                let (triangles, materials) = match &child.mesh {
                    Some(mesh) if child.visible => {
                        (mesh.num_triangles, material_names(child, mesh))
                    }
                    _ => (0, Vec::new()),
                };
                let children = children(child, &path, depth + 1);
                Listed {
                    object: ModelObject {
                        path,
                        name,
                        depth,
                        triangles,
                        materials,
                    },
                    children,
                }
            })
            .collect()
    }
    Ok(objects::flatten(children(&scene.root_node, "", 0)))
}

/// A node's name, or "Object" if it has none.
fn node_name(node: &ufbx::Node) -> String {
    match node.element.name.trim() {
        "" => "Object".to_owned(),
        name => name.to_owned(),
    }
}

/// The names of the materials `mesh` uses, as placed by `node`.
fn material_names(node: &ufbx::Node, mesh: &ufbx::Mesh) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for part in mesh
        .material_parts
        .iter()
        .filter(|part| part.num_triangles > 0)
    {
        let name = node
            .materials
            .get(part.index as usize)
            .or_else(|| mesh.materials.get(part.index as usize))
            .map_or("Default", |material| material.element.name.as_ref())
            .to_owned();
        if !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

/// The texture files an FBX file in `folder` uses that are there, as paths
/// inside `folder`.
pub(super) fn files_used(bytes: &[u8], folder: &Path) -> Result<Vec<PathBuf>, ModelError> {
    let scene = parse(bytes)?;
    let mut files: Vec<PathBuf> = scene
        .textures
        .iter()
        .filter(|texture| texture.content.is_empty())
        .filter_map(|texture| texture_path(texture, folder))
        .collect();
    files.sort();
    files.dedup();
    Ok(files)
}

/// Every take (animation stack) in an FBX file's `bytes`, baked into keys.
pub(super) fn load_takes(bytes: &[u8]) -> Result<Vec<Take>, ModelError> {
    let scene = parse(bytes)?;
    Ok(takes(&scene))
}

/// The scene's takes, each baked at the file's frame rate into keys for
/// every node it moves.
fn takes(scene: &ufbx::Scene) -> Vec<Take> {
    let fps = scene.settings.frames_per_second;
    let fps = if fps.is_finite() && fps > 0.0 {
        fps
    } else {
        DEFAULT_FRAME_RATE
    };
    let mut takes = Vec::new();
    for stack in &scene.anim_stacks {
        let opts = ufbx::BakeOpts {
            // Keys from 0 seconds, wherever the take starts in the file.
            trim_start_time: true,
            resample_rate: fps,
            ..Default::default()
        };
        let Ok(baked) = ufbx::bake_anim(scene, &stack.anim, opts) else {
            continue;
        };
        let mut tracks = Vec::new();
        let mut end = 0.0_f64;
        for node in &baked.nodes {
            let Some(scene_node) = scene.nodes.get(node.typed_id as usize) else {
                continue;
            };
            for key in node.translation_keys.iter().chain(&node.scale_keys) {
                end = end.max(key.time);
            }
            for key in &node.rotation_keys {
                end = end.max(key.time);
            }
            tracks.push(Track {
                bone: scene_node.element.name.to_string(),
                translation: Keys {
                    times: node
                        .translation_keys
                        .iter()
                        .map(|k| k.time as f32)
                        .collect(),
                    values: node
                        .translation_keys
                        .iter()
                        .map(|k| vec3(k.value))
                        .collect(),
                },
                rotation: Keys {
                    times: node.rotation_keys.iter().map(|k| k.time as f32).collect(),
                    values: node.rotation_keys.iter().map(|k| quat(k.value)).collect(),
                },
                scale: Keys {
                    times: node.scale_keys.iter().map(|k| k.time as f32).collect(),
                    values: node.scale_keys.iter().map(|k| vec3(k.value)).collect(),
                },
            });
        }
        let duration = if baked.playback_duration.is_finite() && baked.playback_duration > 0.0 {
            baked.playback_duration
        } else {
            end
        };
        takes.push(Take {
            name: stack.element.name.to_string(),
            duration: duration as f32,
            frame_rate: fps as f32,
            tracks,
        });
    }
    takes
}

/// The typed IDs of the nodes some take really moves. Empty for a file
/// with no animation, so loading a still model bakes nothing.
fn animated_nodes(scene: &ufbx::Scene) -> HashSet<u32> {
    if scene.anim_curves.is_empty() {
        return HashSet::new();
    }
    let mut nodes = HashSet::new();
    for take in takes(scene) {
        for track in &take.tracks {
            let moves = track.translation.values.windows(2).any(|w| w[0] != w[1])
                || track.rotation.values.windows(2).any(|w| w[0] != w[1])
                || track.scale.values.windows(2).any(|w| w[0] != w[1]);
            if !moves {
                continue;
            }
            nodes.extend(
                scene
                    .nodes
                    .iter()
                    .filter(|node| node.element.name.as_ref() == track.bone.as_str())
                    .map(|node| node.element.typed_id),
            );
        }
    }
    nodes
}

/// The skeleton for `joints` (each a node's typed ID and its inverse bind
/// matrix): their nodes and every node above them, parents first.
fn rig(scene: &ufbx::Scene, joints: &[(u32, DMat4)], fit: Mat4) -> Rig {
    let mut wanted = HashSet::new();
    for &(id, _) in joints {
        let mut node = scene.nodes.get(id as usize).map(|node| &**node);
        while let Some(current) = node {
            if !wanted.insert(current.element.typed_id) {
                break;
            }
            node = current.parent.as_deref();
        }
    }
    let mut bones = Vec::new();
    let mut index_of = HashMap::new();
    add_bones(
        &scene.root_node,
        None,
        0,
        &wanted,
        &mut bones,
        &mut index_of,
    );
    let joints = joints
        .iter()
        .map(|&(id, inverse_bind)| Joint {
            bone: index_of.get(&id).copied().unwrap_or(0),
            inverse_bind: inverse_bind.as_mat4(),
        })
        .collect();
    Rig { bones, joints, fit }
}

/// Adds `node` (if wanted) and the wanted nodes inside it, parents first.
fn add_bones(
    node: &ufbx::Node,
    parent: Option<usize>,
    depth: usize,
    wanted: &HashSet<u32>,
    bones: &mut Vec<Bone>,
    index_of: &mut HashMap<u32, usize>,
) {
    if depth > MAX_NODE_DEPTH {
        return;
    }
    let mut parent = parent;
    if wanted.contains(&node.element.typed_id) {
        index_of.insert(node.element.typed_id, bones.len());
        bones.push(Bone {
            name: node.element.name.to_string(),
            parent,
            rest: bone_transform(&node.local_transform),
        });
        parent = Some(bones.len() - 1);
    }
    for child in &node.children {
        add_bones(child, parent, depth + 1, wanted, bones, index_of);
    }
}

fn bone_transform(transform: &ufbx::Transform) -> BoneTransform {
    BoneTransform {
        translation: vec3(transform.translation),
        rotation: quat(transform.rotation),
        scale: vec3(transform.scale),
    }
}

fn vec3(v: ufbx::Vec3) -> Vec3 {
    Vec3::new(v.x as f32, v.y as f32, v.z as f32)
}

fn quat(q: ufbx::Quat) -> Quat {
    Quat::from_xyzw(q.x as f32, q.y as f32, q.z as f32, q.w as f32).normalize()
}

/// A vertex's 4 strongest joints in `skin` (indices into the model's
/// joints, by `cluster_joints`) and their weights, adding up to 1. All 0
/// for a vertex no bone moves.
fn skin_weights(
    skin: &ufbx::SkinDeformer,
    vertex: usize,
    cluster_joints: &[Option<u16>],
) -> ([u16; 4], [f32; 4]) {
    let mut found: Vec<(u16, f32)> = Vec::new();
    if let Some(entry) = skin.vertices.get(vertex) {
        for i in 0..entry.num_weights {
            let Some(weight) = skin.weights.get((entry.weight_begin + i) as usize) else {
                continue;
            };
            let joint = cluster_joints
                .get(weight.cluster_index as usize)
                .copied()
                .flatten();
            if let Some(joint) = joint
                && weight.weight > 0.0
            {
                found.push((joint, weight.weight as f32));
            }
        }
    }
    found.sort_by(|a, b| b.1.total_cmp(&a.1));
    found.truncate(4);
    let total: f32 = found.iter().map(|(_, weight)| weight).sum();
    let (mut joints, mut weights) = ([0; 4], [0.0; 4]);
    if total > 0.0 {
        for (slot, (joint, weight)) in found.into_iter().enumerate() {
            joints[slot] = joint;
            weights[slot] = weight / total;
        }
    }
    (joints, weights)
}

fn parse(bytes: &[u8]) -> Result<ufbx::SceneRoot, ModelError> {
    let opts = ufbx::LoadOpts {
        // Only the bytes given, and only as FBX (not OBJ).
        load_external_files: false,
        file_format: ufbx::FileFormat::Fbx,
        ignore_animation: false,
        generate_missing_normals: true,
        // Blender saves its Principled BSDF's metalness and roughness in
        // its own properties.
        use_blender_pbr_material: true,
        node_depth_limit: MAX_NODE_DEPTH as u32,
        // glTF's space: +X right, +Y up, +Z front, in meters.
        target_axes: CoordinateAxes {
            right: CoordinateAxis::PositiveX,
            up: CoordinateAxis::PositiveY,
            front: CoordinateAxis::PositiveZ,
        },
        target_unit_meters: 1.0,
        ..Default::default()
    };
    ufbx::load_memory(bytes, opts).map_err(|err| {
        let mut message = err.description.to_string();
        let info = err.info();
        if !info.is_empty() {
            message.push_str(&format!(" ({info})"));
        }
        ModelError::Fbx(message)
    })
}

/// Collects each mesh's triangles in model space.
struct Reader<'a> {
    /// The nodes some take moves (see [`animated_nodes`]).
    animated: HashSet<u32>,
    /// The model's joints so far: each a node's typed ID and its inverse
    /// bind matrix (see [`super::Joint`]).
    joints: Vec<(u32, DMat4)>,
    folder: &'a Path,
    /// Textures decoded so far, by element ID, so shared ones decode once.
    /// `None` means it couldn't be, and was noted.
    images: HashMap<u32, Option<Arc<Image>>>,
    /// Materials made so far for this model, by element ID, so each is
    /// worked out (and its problems noted) once.
    materials: HashMap<u32, ModelMaterial>,
    /// The file names of embedded textures, by element ID (see
    /// [`embedded_files`]).
    embedded: HashMap<u32, String>,
    /// Textures no material links, and the materials they're for (see
    /// [`loose_textures`]).
    loose: Vec<(&'a ufbx::Texture, Option<u32>)>,
    primitives: Vec<Primitive>,
    warnings: Vec<String>,
}

impl Reader<'_> {
    /// Collects the wanted meshes inside `node` (whose path is `path`),
    /// adding their paths to `found`. Every one if `wanted` is `None`.
    fn children(
        &mut self,
        node: &ufbx::Node,
        path: &str,
        wanted: &Option<HashSet<String>>,
        found: &mut HashSet<String>,
    ) {
        let mut siblings = Siblings::default();
        for child in &node.children {
            let child_path = siblings.path(path, &node_name(child));
            // Hidden in the program it was made in, so hidden here too.
            if let Some(mesh) = &child.mesh
                && child.visible
                && want(wanted, found, &child_path)
            {
                self.mesh(child, mesh);
            }
            self.children(child, &child_path, wanted, found);
        }
    }

    /// One primitive per material the mesh uses, placed by `node`.
    fn mesh(&mut self, node: &ufbx::Node, mesh: &ufbx::Mesh) {
        let transform = mat4(&node.geometry_to_world);
        // Normals are turned by the inverse transpose, so they stay
        // perpendicular to stretched surfaces.
        let normal_matrix = DMat3::from_mat4(transform).inverse().transpose();
        // A mirrored node turns triangles inside out: swapping two corners
        // turns them back.
        let mirrored = transform.determinant() < 0.0;
        // Room for the triangles of the biggest face (polygon).
        let mut corners = vec![0u32; mesh.max_face_triangles * 3];

        // A skinned mesh moves with its bones; a mesh under a moving node
        // moves with that node, as one rigid joint.
        let skin = mesh.skin_deformers.first().map(|skin| &**skin);
        let mut cluster_joints: Vec<Option<u16>> = Vec::new();
        let mut rigid_joint = None;
        if let Some(skin) = skin {
            let inverse_mesh = transform.inverse();
            for cluster in &skin.clusters {
                let index = u16::try_from(self.joints.len()).ok();
                match (&cluster.bone_node, index) {
                    (Some(bone), Some(index)) => {
                        self.joints.push((
                            bone.element.typed_id,
                            mat4(&cluster.geometry_to_bone) * inverse_mesh,
                        ));
                        cluster_joints.push(Some(index));
                    }
                    _ => cluster_joints.push(None),
                }
            }
        } else if self.moves(node)
            && let Ok(index) = u16::try_from(self.joints.len())
        {
            self.joints
                .push((node.element.typed_id, mat4(&node.node_to_world).inverse()));
            rigid_joint = Some(index);
        }

        for part in &mesh.material_parts {
            if part.num_triangles == 0 {
                continue;
            }
            // An FBX material belongs to the node, so each use of a mesh
            // can have its own; the mesh's are the first node's.
            let material = node
                .materials
                .get(part.index as usize)
                .or_else(|| mesh.materials.get(part.index as usize));
            let mut material = match material {
                Some(material) => self.cached_material(material),
                None => ModelMaterial::default(),
            };
            let want_uvs = material.has_textures() && mesh.vertex_uv.exists;

            let mut positions = Vec::new();
            let mut normals = Vec::new();
            let mut uvs = Vec::new();
            let mut indices = Vec::new();
            let mut joints = Vec::new();
            let mut weights = Vec::new();
            // Corners with the same position, normal and UV become one
            // vertex. The bit patterns compare exactly. Skin weights belong
            // to the file's vertices, so skinned corners of different ones
            // stay apart.
            let mut vertices: HashMap<[u32; 9], u32> = HashMap::new();
            for &face in &part.face_indices {
                let Some(&face) = mesh.faces.get(face as usize) else {
                    continue;
                };
                let triangles = ufbx::triangulate_face(&mut corners, mesh, face) as usize;
                for &index in &corners[..triangles * 3] {
                    let index = index as usize;
                    let position = transform
                        .transform_point3(dvec3(mesh.vertex_position[index]))
                        .as_vec3();
                    let normal = if mesh.vertex_normal.exists {
                        (normal_matrix * dvec3(mesh.vertex_normal[index]))
                            .normalize_or_zero()
                            .as_vec3()
                    } else {
                        Vec3::ZERO
                    };
                    // FBX's V runs up from the bottom of the texture.
                    let uv = if want_uvs {
                        let uv = mesh.vertex_uv[index];
                        [uv.x as f32, 1.0 - uv.y as f32]
                    } else {
                        [0.0; 2]
                    };
                    let vertex_index = mesh.vertex_indices.get(index).copied().unwrap_or(0);
                    let mut key = [0; 9];
                    if skin.is_some() {
                        key[8] = vertex_index;
                    }
                    for (slot, value) in key.iter_mut().zip(
                        position
                            .to_array()
                            .into_iter()
                            .chain(normal.to_array())
                            .chain(uv),
                    ) {
                        *slot = value.to_bits();
                    }
                    let next = positions.len() as u32;
                    let vertex = *vertices.entry(key).or_insert_with(|| {
                        positions.push(position.to_array());
                        normals.push(normal.to_array());
                        if want_uvs {
                            uvs.push(uv);
                        }
                        if let Some(skin) = skin {
                            let (j, w) = skin_weights(skin, vertex_index as usize, &cluster_joints);
                            joints.push(j);
                            weights.push(w);
                        } else if let Some(joint) = rigid_joint {
                            joints.push([joint, 0, 0, 0]);
                            weights.push([1.0, 0.0, 0.0, 0.0]);
                        }
                        next
                    });
                    indices.push(vertex);
                }
            }
            if mirrored {
                for triangle in indices.as_chunks_mut::<3>().0 {
                    triangle.swap(1, 2);
                }
            }
            if !mesh.vertex_normal.exists {
                normals = smooth_normals(&positions, &indices);
            }
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
                joints,
                weights,
            });
        }
    }

    /// Whether a take moves `node` or a node above it.
    fn moves(&self, node: &ufbx::Node) -> bool {
        let mut current = Some(node);
        while let Some(node) = current {
            if self.animated.contains(&node.element.typed_id) {
                return true;
            }
            current = node.parent.as_deref();
        }
        false
    }

    /// An FBX material as metallic-roughness. `ufbx` works out PBR values
    /// for every kind of FBX material, including the old Lambert and Phong.
    /// A value with a map is the map's alone: FBX programs use a map
    /// instead of the value, not multiplied by it.
    fn material(&mut self, material: &ufbx::Material) -> ModelMaterial {
        let pbr = &material.pbr;
        let fbx = &material.fbx;
        // Lambert and Phong, the FBX format's own materials, rather than a
        // PBR one from a renderer's plug-in.
        let classic = matches!(
            material.shader_type,
            ShaderType::Unknown
                | ShaderType::FbxLambert
                | ShaderType::FbxPhong
                | ShaderType::BlenderPhong
        );
        let base = color(&pbr.base_color, Vec3::ONE) * number(&pbr.base_factor, 1.0);
        let base_color_texture = self.map_image(&pbr.base_color);

        let normal_texture = match self
            .map_image(&pbr.normal_map)
            .or_else(|| self.map_image(&fbx.normal_map))
        {
            Some(image) => Some(image),
            // Many exporters put normal maps in the bump slot; a real bump
            // (height) map is grey, and can't be used.
            None => self
                .map_image(&fbx.bump)
                .filter(|image| looks_like_normal_map(image)),
        };

        let metalness_texture = self
            .map_image(&pbr.metalness)
            .map(|image| ChannelTexture::new(image, Channel::Red));
        // Phong's shininess map, and maps the program says are
        // glossiness, are smooth where they're white.
        let roughness_texture = match self.map_image(&pbr.glossiness) {
            Some(image) => Some((image, true)),
            None => self.map_image(&pbr.roughness).map(|image| (image, classic)),
        }
        .map(|(image, invert)| ChannelTexture {
            invert,
            ..ChannelTexture::new(image, Channel::Red)
        });
        let occlusion_texture = self
            .map_image(&pbr.ambient_occlusion)
            .map(|image| ChannelTexture::new(image, Channel::Red));

        let emissive_texture = self.map_image(&pbr.emission_color);
        let mut emissive =
            color(&pbr.emission_color, Vec3::ZERO) * number(&pbr.emission_factor, 1.0);
        // An emission map with no emission color set glows in its own colors.
        if emissive_texture.is_some() && emissive.max_element() <= 0.0 {
            emissive = Vec3::ONE;
        }

        // Opacity is the base texture's alpha, or a map of its own (white
        // is solid). Classic materials keep it in "transparent color".
        let opacity_image = self
            .map_image(&pbr.opacity)
            .or_else(|| self.map_image(&fbx.transparency_color))
            .or_else(|| self.map_image(&fbx.transparency_factor));
        let same_as_base = |image: &Arc<Image>| {
            base_color_texture
                .as_ref()
                .is_some_and(|base| Arc::ptr_eq(base, image))
        };
        let opacity_texture = opacity_image
            .as_ref()
            .filter(|image| !same_as_base(image))
            .map(|image| {
                let channel = if image
                    .rgba
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .any(|pixel| pixel[3] < 255)
                {
                    Channel::Alpha
                } else {
                    Channel::Red
                };
                ChannelTexture::new(image.clone(), channel)
            });
        let opacity = number(&pbr.opacity, 1.0);
        // FBX has no alpha mode. A see-through material blends; an opacity
        // map cuts out (leaves, fences).
        let alpha_mode = if opacity < 1.0 {
            AlphaMode::Blend
        } else if opacity_image.is_some() {
            AlphaMode::Mask { cutoff: 0.5 }
        } else {
            AlphaMode::Opaque
        };

        // A classic material's specular map says how strongly it shines.
        let specular_texture = if classic {
            self.map_image(&pbr.specular_color)
                .or_else(|| self.map_image(&fbx.specular_color))
        } else {
            None
        };

        let mut result = ModelMaterial {
            name: material.element.name.to_string(),
            base_color: [base.x, base.y, base.z, opacity].map(unit),
            metalness: match metalness_texture {
                Some(_) => 1.0,
                None => unit(number(&pbr.metalness, 0.0)),
            },
            roughness: match roughness_texture {
                Some(_) => 1.0,
                None => unit(number(&pbr.roughness, 1.0)),
            },
            emissive: emissive.to_array().map(unit),
            alpha_mode,
            base_color_texture,
            normal_texture,
            normal_scale: 1.0,
            metalness_texture,
            roughness_texture,
            occlusion_texture,
            occlusion_strength: 1.0,
            emissive_texture,
            opacity_texture,
            specular_texture,
            ..ModelMaterial::default()
        };
        self.named_maps(material, classic, &mut result);
        self.loose_maps(material, &mut result);
        result
    }

    /// [`material`](Self::material), worked out once per model.
    fn cached_material(&mut self, material: &ufbx::Material) -> ModelMaterial {
        let id = material.element.element_id;
        if let Some(made) = self.materials.get(&id) {
            return made.clone();
        }
        let made = self.material(material);
        self.materials.insert(id, made.clone());
        made
    }

    /// Goes through every texture `material` links, by its file name (or
    /// else the property it's linked to): a packed map is split into its
    /// channels, a map on a property not read above is used for the role
    /// its name says, and one with no known role is noted and left out.
    /// For Lambert and Phong (`classic`), whose few slots exporters often
    /// misuse, the name also wins over the slot.
    fn named_maps(&mut self, material: &ufbx::Material, classic: bool, result: &mut ModelMaterial) {
        for link in &material.textures {
            let texture = &link.texture;
            let name = texture_name(texture);
            // The property says no more than the slots read above, except
            // that a bump or displacement map is a height map.
            let property = link.material_prop.to_lowercase();
            let height = ["bump", "displacement", "height"]
                .iter()
                .any(|word| property.contains(word));
            let role = maps::role_from_name(&name).or(height.then_some(Role::Height));
            // A texture that couldn't be decoded was noted already.
            let Some(image) = self.image(texture) else {
                continue;
            };
            match (result.role_of(&image), role) {
                (Some(Role::Normal { .. }), Some(Role::Normal { flip_green })) => {
                    result.normal_flip_green = flip_green;
                }
                (Some(linked), Some(named)) if linked == named => {}
                (Some(_), Some(named)) if classic || matches!(named, Role::Packed(_)) => {
                    result.remove_image(&image);
                    self.use_map(result, named, image, &name);
                }
                // Used for its slot.
                (Some(_), _) => {}
                (None, Some(named)) => {
                    if !result.has(named) {
                        self.use_map(result, named, image, &name);
                    }
                }
                (None, None) => self.note(format!(
                    "has a texture `{name}` it doesn't know the use of, so it's left out;                      naming it like `{}_Normal` or `_Roughness` says what it's for",
                    material.element.name
                )),
            }
        }
    }

    /// Uses the textures no material links ([`loose_textures`]) that are
    /// for `material`, for the roles their names say, where it has no map yet.
    fn loose_maps(&mut self, material: &ufbx::Material, result: &mut ModelMaterial) {
        let id = material.element.element_id;
        let loose: Vec<&ufbx::Texture> = self
            .loose
            .iter()
            .filter(|(_, owner)| *owner == Some(id))
            .map(|(texture, _)| *texture)
            .collect();
        for texture in loose {
            let name = texture_name(texture);
            if let Some(role) = maps::role_from_name(&name)
                && !result.has(role)
                && let Some(image) = self.image(texture)
            {
                self.use_map(result, role, image, &name);
            }
        }
    }

    /// Uses `image` for `role`, noting it if that's a height map.
    fn use_map(&mut self, result: &mut ModelMaterial, role: Role, image: Arc<Image>, name: &str) {
        if !result.set_map(role, image) {
            self.note(format!(
                "has a height map `{name}`, which isn't used; a normal map gives the same bumps"
            ));
        }
    }

    /// Notes a problem in the model's warnings, once.
    fn note(&mut self, warning: String) {
        if !self.warnings.contains(&warning) {
            self.warnings.push(warning);
        }
    }

    /// The image of a material value's map, if it has one that can be used.
    fn map_image(&mut self, map: &ufbx::MaterialMap) -> Option<Arc<Image>> {
        map.texture
            .as_ref()
            .filter(|_| map.texture_enabled)
            .and_then(|texture| self.image(texture))
    }

    /// The texture decoded, from the FBX file if it's embedded or from its
    /// file in the model's folder, or `None` (noted once) if it can't be.
    fn image(&mut self, texture: &ufbx::Texture) -> Option<Arc<Image>> {
        // A layered texture is made of file textures; the first is the base.
        let texture = texture
            .file_textures
            .first()
            .map_or(texture, |file| &**file);
        let id = texture.element.element_id;
        if let Some(image) = self.images.get(&id) {
            return image.clone();
        }
        let name = texture_name(texture);
        let image = if texture.content.is_empty() {
            match texture_path(texture, self.folder) {
                Some(path) => match std::fs::read(self.folder.join(&path)) {
                    Ok(bytes) => decode_image(&bytes, &name).map(|image| Image {
                        file: Some(slash_path(&path)),
                        ..image
                    }),
                    Err(err) => Err(err.to_string()),
                },
                None => Err("it isn't there; put it next to the model".to_owned()),
            }
        } else {
            decode_image(&texture.content, &name).map(|image| Image {
                file: self
                    .embedded
                    .get(&id)
                    .map(|file| format!("{}/{file}", crate::import::TEXTURES_FOLDER)),
                ..image
            })
        };
        let image = match image {
            Ok(image) => Some(Arc::new(image)),
            Err(reason) => {
                self.note(format!("is drawn without its texture `{name}`: {reason}"));
                None
            }
        };
        self.images.insert(id, image.clone());
        image
    }
}

/// Where `texture`'s file is, as a path inside `folder`, trying in turn:
/// the path the file gives relative to the model, the file's name next to
/// the model, and its name in a `textures` folder next to the model. `None`
/// if it isn't in any of them.
fn texture_path(texture: &ufbx::Texture, folder: &Path) -> Option<PathBuf> {
    let name = file_name(texture);
    let candidates = [
        crate::safe_path(&texture.relative_filename),
        crate::safe_path(name),
        (!name.is_empty())
            .then(|| crate::safe_path(&format!("textures/{name}")))
            .flatten(),
    ];
    candidates
        .into_iter()
        .flatten()
        .find(|path| folder.join(path).is_file())
}

/// `path` with `/` between folders, whatever the OS.
fn slash_path(path: &Path) -> String {
    let parts: Vec<String> = path
        .components()
        .map(|part| part.as_os_str().to_string_lossy().into_owned())
        .collect();
    parts.join("/")
}

/// The last part of the texture's path, with `/` or `\` between folders.
fn file_name(texture: &ufbx::Texture) -> &str {
    [
        &texture.relative_filename,
        &texture.filename,
        &texture.absolute_filename,
    ]
    .into_iter()
    .map(|path| path.rsplit(['/', '\\']).next().unwrap_or_default())
    .find(|name| !name.is_empty())
    .unwrap_or_default()
}

/// How the texture is named in messages.
fn texture_name(texture: &ufbx::Texture) -> String {
    match file_name(texture) {
        "" if !texture.content.is_empty() => "embedded texture".to_owned(),
        "" => texture.element.name.to_string(),
        name => name.to_owned(),
    }
}

/// True if `image` looks like a tangent-space normal map rather than a
/// bump (height) map: mostly blue (surfaces facing out), with red and
/// green around the middle. A bump map is grey.
fn looks_like_normal_map(image: &Image) -> bool {
    let pixels = image.rgba.len() / 4;
    // Every pixel of a small image, or about 4,096 spread over a big one.
    let step = (pixels / 4096).max(1);
    let mut sums = [0u64; 3];
    let mut count = 0u64;
    for pixel in image.rgba.as_chunks::<4>().0.iter().step_by(step) {
        for (sum, &value) in sums.iter_mut().zip(pixel) {
            *sum += u64::from(value);
        }
        count += 1;
    }
    if count == 0 {
        return false;
    }
    let [red, green, blue] = sums.map(|sum| sum as f32 / count as f32);
    blue > 180.0 && (red - 128.0).abs() < 40.0 && (green - 128.0).abs() < 40.0
}

/// A material value that's one number, or `default` if the file has none.
fn number(map: &ufbx::MaterialMap, default: f32) -> f32 {
    if map.has_value {
        map.value_vec4.x as f32
    } else {
        default
    }
}

/// A material value that's a color, or `default` if the file has none.
fn color(map: &ufbx::MaterialMap, default: Vec3) -> Vec3 {
    if map.has_value {
        let v = map.value_vec4;
        DVec3::new(v.x, v.y, v.z).as_vec3()
    } else {
        default
    }
}

/// `value` clamped to 0–1, with anything not a number as 0.
fn unit(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn dvec3(v: ufbx::Vec3) -> DVec3 {
    DVec3::new(v.x, v.y, v.z)
}

/// `ufbx`'s 3×4 matrix (columns: X, Y and Z axes, then the translation).
fn mat4(m: &ufbx::Matrix) -> DMat4 {
    DMat4::from_cols_array(&[
        m.m00, m.m10, m.m20, 0.0, //
        m.m01, m.m11, m.m21, 0.0, //
        m.m02, m.m12, m.m22, 0.0, //
        m.m03, m.m13, m.m23, 1.0,
    ])
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::model::{ModelFormat, Pick, load_model};

    /// An ASCII FBX file of a square pyramid 4 wide and 3 tall, with one
    /// red Phong material. `z_up` saves it in centimeters with Z up (as
    /// 3ds Max does), otherwise in meters with Y up. With a `texture` path,
    /// the material's diffuse color uses it.
    pub(crate) fn pyramid_fbx(z_up: bool, texture: Option<&str>) -> String {
        let maps: Vec<(&str, &str)> = texture
            .map(|path| ("DiffuseColor", path))
            .into_iter()
            .collect();
        pyramid_fbx_with_maps(z_up, &maps)
    }

    /// Like [`pyramid_fbx`], with each `(property, path)` of `maps` a
    /// texture linked to that property of the material ("DiffuseColor",
    /// "NormalMap", "SpecularColor"…).
    pub(crate) fn pyramid_fbx_with_maps(z_up: bool, maps: &[(&str, &str)]) -> String {
        let textures: Vec<TestTexture> = maps
            .iter()
            .map(|&(property, path)| TestTexture {
                property: Some(property),
                path,
                embedded: None,
            })
            .collect();
        pyramid_fbx_with_textures(z_up, &textures)
    }

    /// A texture in a test FBX file.
    pub(crate) struct TestTexture<'a> {
        /// The material property it's linked to, or `None` for none.
        pub(crate) property: Option<&'a str>,
        pub(crate) path: &'a str,
        /// The image file's bytes, if it's embedded in the FBX file.
        pub(crate) embedded: Option<&'a [u8]>,
    }

    /// Like [`pyramid_fbx`], with `textures`.
    pub(crate) fn pyramid_fbx_with_textures(z_up: bool, textures: &[TestTexture]) -> String {
        let (up_axis, front_axis, front_sign, unit, vertices) = if z_up {
            (
                2,
                1,
                -1,
                1,
                "-200,200,0, 200,200,0, 200,-200,0, -200,-200,0, 0,0,300",
            )
        } else {
            (1, 2, 1, 100, "-2,0,-2, 2,0,-2, 2,0,2, -2,0,2, 0,3,0")
        };
        let mut texture_object = String::new();
        let mut texture_connection = String::new();
        for (i, texture) in textures.iter().enumerate() {
            let (id, path) = (4000 + i, texture.path);
            texture_object.push_str(&format!(
                "\tTexture: {id}, \"Texture::Map{i}\", \"\" {{\n\
                 \t\tType: \"TextureVideoClip\"\n\
                 \t\tVersion: 202\n\
                 \t\tTextureName: \"Texture::Map{i}\"\n\
                 \t\tFileName: \"C:/Users/artist/Desktop/{path}\"\n\
                 \t\tRelativeFilename: \"{path}\"\n\
                 \t}}\n"
            ));
            if let Some(property) = texture.property {
                texture_connection.push_str(&format!("\tC: \"OP\",{id},3000, \"{property}\"\n"));
            }
            if let Some(content) = texture.embedded {
                let video = 5000 + i;
                let content = base64(content);
                texture_object.push_str(&format!(
                    "\tVideo: {video}, \"Video::Map{i}\", \"Clip\" {{\n\
                     \t\tType: \"Clip\"\n\
                     \t\tFileName: \"C:/Users/artist/Desktop/{path}\"\n\
                     \t\tRelativeFilename: \"{path}\"\n\
                     \t\tContent: , \"{content}\"\n\
                     \t}}\n"
                ));
                texture_connection.push_str(&format!("\tC: \"OO\",{video},{id}\n"));
            }
        }
        format!(
            r#"; FBX 7.4.0 project file
FBXHeaderExtension:  {{
	FBXHeaderVersion: 1003
	FBXVersion: 7400
}}
GlobalSettings:  {{
	Version: 1000
	Properties70:  {{
		P: "UpAxis", "int", "Integer", "",{up_axis}
		P: "UpAxisSign", "int", "Integer", "",1
		P: "FrontAxis", "int", "Integer", "",{front_axis}
		P: "FrontAxisSign", "int", "Integer", "",{front_sign}
		P: "CoordAxis", "int", "Integer", "",0
		P: "CoordAxisSign", "int", "Integer", "",1
		P: "UnitScaleFactor", "double", "Number", "",{unit}
	}}
}}
Objects:  {{
	Geometry: 1000, "Geometry::Pyramid", "Mesh" {{
		Vertices: *15 {{
			a: {vertices}
		}}
		PolygonVertexIndex: *16 {{
			a: 0,1,2,-4,0,4,-2,1,4,-3,2,4,-4,3,4,-1
		}}
		GeometryVersion: 124
		LayerElementUV: 0 {{
			Version: 101
			Name: "UVMap"
			MappingInformationType: "ByPolygonVertex"
			ReferenceInformationType: "IndexToDirect"
			UV: *10 {{
				a: 0,0,1,0,1,1,0,1,0.5,0.5
			}}
			UVIndex: *16 {{
				a: 0,1,2,3,0,4,1,1,4,2,2,4,3,3,4,0
			}}
		}}
		LayerElementMaterial: 0 {{
			Version: 101
			Name: ""
			MappingInformationType: "AllSame"
			ReferenceInformationType: "IndexToDirect"
			Materials: *1 {{
				a: 0
			}}
		}}
		Layer: 0 {{
			Version: 100
			LayerElement:  {{
				Type: "LayerElementUV"
				TypedIndex: 0
			}}
			LayerElement:  {{
				Type: "LayerElementMaterial"
				TypedIndex: 0
			}}
		}}
	}}
	Model: 2000, "Model::Pyramid", "Mesh" {{
		Version: 232
		Properties70:  {{
		}}
	}}
	Material: 3000, "Material::Red", "" {{
		Version: 102
		ShadingModel: "phong"
		Properties70:  {{
			P: "DiffuseColor", "Color", "", "A",1,0,0
		}}
	}}
{texture_object}}}
Connections:  {{
	C: "OO",1000,2000
	C: "OO",2000,0
	C: "OO",3000,2000
{texture_connection}}}
"#
        )
    }

    /// An ASCII FBX file, in meters with Y up, of two copies of a 2×2×2
    /// cube: `Street/Bench` (inside an empty `Street` node, 10 m along X)
    /// and `Lamp` (at the origin, with a second, empty `Lamp` after it).
    pub(crate) fn two_cubes_fbx() -> String {
        r#"; FBX 7.4.0 project file
FBXHeaderExtension:  {
	FBXHeaderVersion: 1003
	FBXVersion: 7400
}
GlobalSettings:  {
	Version: 1000
	Properties70:  {
		P: "UpAxis", "int", "Integer", "",1
		P: "UpAxisSign", "int", "Integer", "",1
		P: "FrontAxis", "int", "Integer", "",2
		P: "FrontAxisSign", "int", "Integer", "",1
		P: "CoordAxis", "int", "Integer", "",0
		P: "CoordAxisSign", "int", "Integer", "",1
		P: "UnitScaleFactor", "double", "Number", "",100
	}
}
Objects:  {
	Geometry: 1000, "Geometry::Cube", "Mesh" {
		Vertices: *24 {
			a: -1,-1,-1, 1,-1,-1, 1,1,-1, -1,1,-1, -1,-1,1, 1,-1,1, 1,1,1, -1,1,1
		}
		PolygonVertexIndex: *24 {
			a: 0,3,2,-2,4,5,6,-8,0,1,5,-5,2,3,7,-7,1,2,6,-6,0,4,7,-4
		}
		GeometryVersion: 124
	}
	Model: 2000, "Model::Street", "Null" {
		Version: 232
	}
	Model: 2001, "Model::Bench", "Mesh" {
		Version: 232
		Properties70:  {
			P: "Lcl Translation", "Lcl Translation", "", "A",10,0,0
		}
	}
	Model: 2002, "Model::Lamp", "Mesh" {
		Version: 232
	}
	Model: 2003, "Model::Lamp", "Null" {
		Version: 232
	}
	Material: 3000, "Material::Stone", "" {
		Version: 102
		ShadingModel: "phong"
	}
}
Connections:  {
	C: "OO",2000,0
	C: "OO",2001,2000
	C: "OO",2002,0
	C: "OO",2003,0
	C: "OO",1000,2001
	C: "OO",1000,2002
	C: "OO",3000,2001
	C: "OO",3000,2002
}
"#
        .to_owned()
    }

    /// `bytes` in base64, as ASCII FBX files embed files.
    fn base64(bytes: &[u8]) -> String {
        const DIGITS: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut text = String::new();
        for chunk in bytes.chunks(3) {
            let n = chunk
                .iter()
                .enumerate()
                .fold(0u32, |n, (i, &b)| n | u32::from(b) << (16 - 8 * i));
            for i in 0..4 {
                if i <= chunk.len() {
                    text.push(DIGITS[(n >> (18 - 6 * i) & 63) as usize] as char);
                } else {
                    text.push('=');
                }
            }
        }
        text
    }

    /// A 2×2 checkered PNG.
    pub(crate) fn checker_png() -> Vec<u8> {
        let image = image::RgbaImage::from_fn(2, 2, |x, y| match (x + y) % 2 {
            0 => image::Rgba([255, 255, 255, 255]),
            _ => image::Rgba([0, 0, 0, 255]),
        });
        let mut png = std::io::Cursor::new(Vec::new());
        image.write_to(&mut png, image::ImageFormat::Png).unwrap();
        png.into_inner()
    }

    fn temp_folder(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!("axcenity-fbx-{name}"));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        folder
    }

    fn load_fbx(text: &str, folder: &Path) -> Result<Model, ModelError> {
        load_model(text.as_bytes(), folder, ModelFormat::Fbx)
    }

    fn assert_size(model: &Model, expected: [f32; 3]) {
        for (size, expected) in model.size.iter().zip(expected) {
            assert!((size - expected).abs() < 1e-4, "{:?}", model.size);
        }
    }

    #[test]
    fn an_fbx_pyramid_loads_fitted_into_the_unit_cube() {
        let folder = temp_folder("pyramid");
        let model = load_fbx(&pyramid_fbx(false, None), &folder).unwrap();
        assert_size(&model, [4.0, 3.0, 4.0]);
        assert_eq!(model.primitives.len(), 1);
        let primitive = &model.primitives[0];
        // The square base is two triangles, plus four sides.
        assert_eq!(primitive.indices.len(), 6 * 3);
        assert_eq!(primitive.normals.len(), primitive.positions.len());
        for corner in &primitive.positions {
            assert!(corner.iter().all(|c| c.abs() <= 0.5 + 1e-6), "{corner:?}");
        }
        assert_eq!(model.hull_points.len(), 5);
        let material = &primitive.material;
        assert_eq!(material.base_color, [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(material.alpha_mode, AlphaMode::Opaque);
        assert!(material.base_color_texture.is_none());
        assert!(primitive.uvs.is_empty());
        assert!(model.warnings.is_empty());
    }

    #[test]
    fn other_axes_and_units_are_converted_to_studs_with_y_up() {
        let folder = temp_folder("z-up");
        let model = load_fbx(&pyramid_fbx(true, None), &folder).unwrap();
        assert_size(&model, [4.0, 3.0, 4.0]);
    }

    #[test]
    fn textures_are_found_next_to_the_model() {
        let folder = temp_folder("texture");
        std::fs::create_dir_all(folder.join("textures")).unwrap();
        std::fs::write(folder.join("textures/checker.png"), checker_png()).unwrap();
        // By its relative path, and by its name in `textures` when the
        // relative path leads outside the folder.
        for path in ["textures/checker.png", "../../maps/checker.png"] {
            let model = load_fbx(&pyramid_fbx(false, Some(path)), &folder).unwrap();
            assert!(model.warnings.is_empty(), "{:?}", model.warnings);
            let primitive = &model.primitives[0];
            let texture = primitive.material.base_color_texture.as_ref().unwrap();
            assert_eq!((texture.width, texture.height), (2, 2));
            assert_eq!(primitive.uvs.len(), primitive.positions.len());
            // V is flipped: FBX's (0, 0) is the bottom left.
            assert!(primitive.uvs.contains(&[0.0, 1.0]), "{:?}", primitive.uvs);
        }
    }

    /// A 2×2 PNG all of one color.
    fn solid_png(rgba: [u8; 4]) -> Vec<u8> {
        let image = image::RgbaImage::from_pixel(2, 2, image::Rgba(rgba));
        let mut png = std::io::Cursor::new(Vec::new());
        image.write_to(&mut png, image::ImageFormat::Png).unwrap();
        png.into_inner()
    }

    #[test]
    fn a_phong_materials_maps_become_pbr_maps() {
        let folder = temp_folder("phong-maps");
        let files = [
            ("color.png", [200, 100, 50, 255]),
            ("normal.png", [128, 128, 255, 255]),
            ("gloss.png", [255, 255, 255, 255]),
            ("spec.png", [64, 64, 64, 255]),
            ("glow.png", [255, 0, 0, 255]),
            ("cutout.png", [255, 255, 255, 0]),
        ];
        for (name, rgba) in files {
            std::fs::write(folder.join(name), solid_png(rgba)).unwrap();
        }
        let fbx = pyramid_fbx_with_maps(
            false,
            &[
                ("DiffuseColor", "color.png"),
                ("NormalMap", "normal.png"),
                ("ShininessExponent", "gloss.png"),
                ("SpecularColor", "spec.png"),
                ("EmissiveColor", "glow.png"),
                ("TransparentColor", "cutout.png"),
            ],
        );
        let model = load_fbx(&fbx, &folder).unwrap();
        assert!(model.warnings.is_empty(), "{:?}", model.warnings);
        let primitive = &model.primitives[0];
        let material = &primitive.material;
        assert_eq!(material.name, "Red");
        assert!(material.base_color_texture.is_some());
        assert!(material.normal_texture.is_some());
        // Every corner gets a tangent for the normal map.
        assert_eq!(primitive.tangents.len(), primitive.positions.len());
        // Shininess is glossiness: inverted, and the map alone sets it.
        let roughness = material.roughness_texture.as_ref().unwrap();
        assert!(roughness.invert);
        assert_eq!(material.roughness, 1.0);
        assert!(material.specular_texture.is_some());
        // An emission map with no emission color glows in its own colors.
        assert!(material.emissive_texture.is_some());
        assert_eq!(material.emissive, [1.0; 3]);
        // The cut-out map has alpha, so that's the channel used.
        let opacity = material.opacity_texture.as_ref().unwrap();
        assert_eq!(opacity.channel, Channel::Alpha);
        assert_eq!(material.alpha_mode, AlphaMode::Mask { cutoff: 0.5 });
    }

    #[test]
    fn a_bump_map_is_used_only_if_it_is_really_a_normal_map() {
        let folder = temp_folder("bump");
        std::fs::write(folder.join("normal.png"), solid_png([120, 135, 250, 255])).unwrap();
        std::fs::write(folder.join("height.png"), solid_png([90, 90, 90, 255])).unwrap();
        let normal = load_fbx(
            &pyramid_fbx_with_maps(false, &[("Bump", "normal.png")]),
            &folder,
        );
        assert!(
            normal.unwrap().primitives[0]
                .material
                .normal_texture
                .is_some()
        );
        let height = load_fbx(
            &pyramid_fbx_with_maps(false, &[("Bump", "height.png")]),
            &folder,
        );
        let height = height.unwrap();
        assert!(height.primitives[0].material.normal_texture.is_none());
        // With no texture left, it needs no texture coordinates.
        assert!(height.primitives[0].uvs.is_empty());
    }

    /// A texture file in `folder`, linked to `property`.
    fn linked<'a>(property: &'a str, path: &'a str) -> TestTexture<'a> {
        TestTexture {
            property: Some(property),
            path,
            embedded: None,
        }
    }

    #[test]
    fn embedded_textures_are_used_and_listed_for_copying_out() {
        let folder = temp_folder("embedded");
        let png = checker_png();
        let fbx = pyramid_fbx_with_textures(
            false,
            &[TestTexture {
                property: Some("DiffuseColor"),
                path: "wood_BaseColor.png",
                embedded: Some(&png),
            }],
        );
        // Nothing is in the folder: the texture comes from the file itself.
        let model = load_fbx(&fbx, &folder).unwrap();
        assert!(model.warnings.is_empty(), "{:?}", model.warnings);
        let texture = model.primitives[0].material.base_color_texture.as_ref();
        assert_eq!(texture.map(|t| (t.width, t.height)), Some((2, 2)));
        let files = embedded_textures(fbx.as_bytes()).unwrap();
        assert_eq!(files, vec![("wood_BaseColor.png".to_owned(), png)]);
    }

    #[test]
    fn packed_textures_are_split_into_maps_by_their_names() {
        let folder = temp_folder("packed");
        std::fs::write(folder.join("rock_ORM.png"), solid_png([255, 128, 0, 255])).unwrap();
        std::fs::write(
            folder.join("crate_MetallicSmoothness.png"),
            solid_png([255, 0, 0, 128]),
        )
        .unwrap();

        // On a property the engine doesn't read: its name says what it is.
        let orm = load_fbx(
            &pyramid_fbx_with_textures(false, &[linked("CustomPacked", "rock_ORM.png")]),
            &folder,
        )
        .unwrap();
        let material = &orm.primitives[0].material;
        let channel = |map: &Option<ChannelTexture>| map.as_ref().map(|map| map.channel);
        assert_eq!(channel(&material.occlusion_texture), Some(Channel::Red));
        assert_eq!(channel(&material.roughness_texture), Some(Channel::Green));
        assert_eq!(channel(&material.metalness_texture), Some(Channel::Blue));
        assert_eq!((material.metalness, material.roughness), (1.0, 1.0));

        // In a Phong material's specular slot, as Unity-made files have it:
        // the name wins, and it's metalness and smoothness.
        let unity = load_fbx(
            &pyramid_fbx_with_textures(
                false,
                &[linked("SpecularColor", "crate_MetallicSmoothness.png")],
            ),
            &folder,
        )
        .unwrap();
        let material = &unity.primitives[0].material;
        assert!(material.specular_texture.is_none());
        assert_eq!(channel(&material.metalness_texture), Some(Channel::Red));
        let roughness = material.roughness_texture.as_ref().unwrap();
        assert_eq!(
            (roughness.channel, roughness.invert),
            (Channel::Alpha, true)
        );
    }

    #[test]
    fn an_unlinked_embedded_texture_is_used_by_its_name() {
        let folder = temp_folder("loose");
        let png = solid_png([128, 128, 255, 255]);
        let fbx = pyramid_fbx_with_textures(
            false,
            &[
                linked("DiffuseColor", "color.png"),
                TestTexture {
                    property: None,
                    path: "Red_Normal.png",
                    embedded: Some(&png),
                },
            ],
        );
        std::fs::write(folder.join("color.png"), checker_png()).unwrap();
        let model = load_fbx(&fbx, &folder).unwrap();
        assert!(model.primitives[0].material.normal_texture.is_some());
    }

    #[test]
    fn textures_of_unknown_use_and_height_maps_are_noted_and_left_out() {
        let folder = temp_folder("unknown");
        std::fs::write(folder.join("wood.png"), checker_png()).unwrap();
        std::fs::write(folder.join("bricks.png"), solid_png([90, 90, 90, 255])).unwrap();
        let fbx = pyramid_fbx_with_textures(
            false,
            &[
                linked("CustomThing", "wood.png"),
                linked("Bump", "bricks.png"),
            ],
        );
        let model = load_fbx(&fbx, &folder).unwrap();
        let warnings = model.warnings.join("\n");
        assert!(
            warnings.contains("has a texture `wood.png` it doesn't know the use of"),
            "{warnings}"
        );
        assert!(
            warnings.contains("has a height map `bricks.png`"),
            "{warnings}"
        );
        assert!(!model.primitives[0].material.has_textures());
    }

    #[test]
    fn a_missing_or_unreadable_texture_is_a_warning_not_an_error() {
        let folder = temp_folder("no-texture");
        std::fs::write(folder.join("paint.psd"), b"8BPS not really").unwrap();
        let missing = load_fbx(&pyramid_fbx(false, Some("gone.png")), &folder).unwrap();
        assert!(missing.primitives[0].material.base_color_texture.is_none());
        assert_eq!(missing.warnings.len(), 1);
        assert!(
            missing.warnings[0].contains("`gone.png`: it isn't there"),
            "{:?}",
            missing.warnings
        );
        let unreadable = load_fbx(&pyramid_fbx(false, Some("paint.psd")), &folder).unwrap();
        assert!(
            unreadable.warnings[0].contains("only PNG, JPEG, TGA and BMP"),
            "{:?}",
            unreadable.warnings
        );
    }

    #[test]
    fn the_objects_in_an_fbx_are_listed_as_a_tree() {
        let objects =
            crate::model::list_objects(two_cubes_fbx().as_bytes(), ModelFormat::Fbx).unwrap();
        let rows: Vec<(&str, usize, usize)> = objects
            .iter()
            .map(|object| (object.path.as_str(), object.depth, object.triangles))
            .collect();
        // The empty second Lamp has nothing to import, so it isn't listed.
        assert_eq!(
            rows,
            [("Street", 0, 0), ("Street/Bench", 1, 12), ("Lamp", 0, 12)]
        );
        assert_eq!(objects[1].materials, ["Stone"]);
    }

    #[test]
    fn picked_fbx_objects_load_on_their_own_and_keep_their_places() {
        let folder = temp_folder("picks");
        let picks = [
            Pick::Objects(vec!["Street/Bench".into()]),
            Pick::Objects(vec!["Lamp".into()]),
            Pick::Whole,
        ];
        let models = crate::model::load_models(
            two_cubes_fbx().as_bytes(),
            &folder,
            ModelFormat::Fbx,
            &picks,
        )
        .unwrap();
        let [bench, lamp, whole] = &models[..] else {
            panic!("one model per pick");
        };
        assert_size(bench, [2.0, 2.0, 2.0]);
        assert!((bench.center[0] - 10.0).abs() < 1e-4, "{:?}", bench.center);
        assert!(
            lamp.center.iter().all(|c| c.abs() < 1e-4),
            "{:?}",
            lamp.center
        );
        assert_size(whole, [12.0, 2.0, 2.0]);
        assert_eq!(whole.primitives.len(), 2);

        let missing = crate::model::load_models(
            two_cubes_fbx().as_bytes(),
            &folder,
            ModelFormat::Fbx,
            &[Pick::Objects(vec!["Street".into()])],
        );
        // A group has no mesh of its own.
        assert_eq!(
            missing.unwrap_err().to_string(),
            "it has no object `Street`"
        );
    }

    #[test]
    fn broken_fbx_files_give_errors_not_crashes() {
        let folder = temp_folder("broken");
        let error = |bytes: &[u8]| {
            load_model(bytes, &folder, ModelFormat::Fbx)
                .unwrap_err()
                .to_string()
        };
        assert!(error(b"").contains("isn't a valid FBX file"));
        assert!(error(b"Kaydara FBX Binary  \0\x1a\0").contains("isn't a valid FBX file"));
        // Cut off halfway.
        let text = pyramid_fbx(false, None);
        assert!(load_fbx(&text[..text.len() / 2], &folder).is_err());
        // Valid, but nothing to draw.
        let empty = "; FBX 7.4.0 project file\nFBXHeaderExtension:  {\n\tFBXVersion: 7400\n}\n";
        assert_eq!(error(empty.as_bytes()), "it has no triangles to draw");
    }

    #[test]
    fn hello_pbr_examples_load_their_maps() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-pbr");
        let models = root.join("assets/models");
        let helmet = std::fs::read(models.join("DamagedHelmet/DamagedHelmet.glb")).unwrap();
        let helmet = load_model(&helmet, &models.join("DamagedHelmet"), ModelFormat::Gltf).unwrap();
        let material = &helmet.primitives[0].material;
        assert!(material.base_color_texture.is_some());
        assert!(material.normal_texture.is_some());
        assert!(material.metalness_texture.is_some());
        assert!(material.roughness_texture.is_some());
        assert!(material.occlusion_texture.is_some());
        assert!(material.emissive_texture.is_some());

        let pbr = std::fs::read_to_string(models.join("pbr_panel.fbx")).unwrap();
        let pbr = load_fbx(&pbr, &models).unwrap();
        assert!(pbr.warnings.is_empty(), "{:?}", pbr.warnings);
        let material = &pbr.primitives[0].material;
        assert!(material.base_color_texture.is_some());
        assert!(material.normal_texture.is_some());
        assert!(material.roughness_texture.is_some());
        assert!(material.metalness_texture.is_some());
        assert!(material.emissive_texture.is_some());
        assert!(material.opacity_texture.is_some());
        assert!(material.occlusion_texture.is_some());

        let phong = std::fs::read_to_string(models.join("phong_panel.fbx")).unwrap();
        let phong = load_fbx(&phong, &models).unwrap();
        assert!(phong.warnings.is_empty(), "{:?}", phong.warnings);
        let material = &phong.primitives[0].material;
        let roughness = material.roughness_texture.as_ref().unwrap();
        assert!(roughness.invert, "glossiness should invert to roughness");
        assert!(material.specular_texture.is_some());
    }
}
