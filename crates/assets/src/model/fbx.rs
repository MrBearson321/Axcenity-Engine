//! Reading FBX models (binary or ASCII, any version) and OBJ models (with
//! their `.mtl` material library) with `ufbx`, into the same triangles and
//! materials as glTF.
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
    AlphaMode, Bone, BoneTransform, Channel, ChannelTexture, Image, InProject, Joint, Keys,
    MAX_NODE_DEPTH, Model, ModelError, ModelMaterial, ModelObject, Pick, Primitive, Rig, Take,
    Track, cooked_texture, decode_image, file_origin, fit, fit_matrix, missing, new_key,
    smooth_normals, want,
};

/// Takes are baked at this many frames a second when the file doesn't say.
const DEFAULT_FRAME_RATE: f64 = 30.0;

/// Reads an FBX file's `bytes`, making one model for each of `picks`.
/// Textures that aren't embedded in it are read from `folder`, the folder
/// the model is in.
pub(super) fn load(
    bytes: &[u8],
    source: Source<'_>,
    folder: &Path,
    in_project: Option<InProject<'_>>,
    picks: &[Pick],
) -> Result<Vec<Model>, ModelError> {
    let scene = parse(bytes, source)?;
    let mut reader = Reader {
        animated: animated_nodes(&scene),
        joints: Vec::new(),
        folder,
        in_project,
        images: HashMap::new(),
        materials: HashMap::new(),
        loose: loose_textures(&scene),
        embedded: embedded_files(&scene)
            .into_iter()
            .flat_map(|(name, _, ids)| ids.into_iter().map(move |id| (id, name.clone())))
            .collect(),
        primitives: Vec::new(),
        warnings: Vec::new(),
        level: 0,
        pieces: super::collision::Pieces::default(),
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
        let mut model = fit(
            std::mem::take(&mut reader.primitives),
            reader.pieces.take(),
            in_project,
        )?;
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
    let scene = parse(bytes, Source::Fbx)?;
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
pub(super) fn list(bytes: &[u8], source: Source<'_>) -> Result<Vec<ModelObject>, ModelError> {
    let scene = parse(bytes, source)?;
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

/// The texture files an FBX or OBJ file in `folder` uses that are there,
/// as paths inside `folder`, and an OBJ file's material library.
pub(super) fn files_used(
    bytes: &[u8],
    source: Source<'_>,
    folder: &Path,
) -> Result<Vec<PathBuf>, ModelError> {
    let scene = parse(bytes, source)?;
    let library = match source {
        Source::Obj { library, .. } => library.map(|(path, _)| path.to_path_buf()),
        Source::Fbx => None,
    };
    let mut files: Vec<PathBuf> = library
        .into_iter()
        .chain(
            scene
                .textures
                .iter()
                .filter(|texture| texture.content.is_empty())
                .filter_map(|texture| texture_path(texture, folder)),
        )
        .collect();
    files.sort();
    files.dedup();
    Ok(files)
}

/// Every take (animation stack) in an FBX file's `bytes`, baked into keys.
pub(super) fn load_takes(bytes: &[u8]) -> Result<Vec<Take>, ModelError> {
    let scene = parse(bytes, Source::Fbx)?;
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
    // The nodes some take moves, for the skeleton every take shares.
    let mut moved = HashSet::new();
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
            moved.insert(node.typed_id);
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
            skeleton: Default::default(),
        });
    }
    let skeleton = std::sync::Arc::new(take_skeleton(scene, &moved));
    for take in &mut takes {
        take.skeleton = skeleton.clone();
    }
    takes
}

/// The nodes `moved` names, the file's limb (bone) nodes and every node
/// above them, parents first, where they rest: the skeleton a file's takes
/// animate.
fn take_skeleton(scene: &ufbx::Scene, moved: &HashSet<u32>) -> Vec<Bone> {
    // Bones the takes leave still still shape the body (retargeting measures
    // it), so every limb node counts too.
    let limbs = scene
        .nodes
        .iter()
        .filter(|node| node.bone.is_some())
        .map(|node| node.element.typed_id);
    let mut wanted = HashSet::new();
    for id in moved.iter().copied().chain(limbs) {
        let mut node = scene.nodes.get(id as usize).map(|node| &**node);
        while let Some(current) = node {
            if !wanted.insert(current.element.typed_id) {
                break;
            }
            node = current.parent.as_deref();
        }
    }
    let mut bones = Vec::new();
    add_bones(
        &scene.root_node,
        None,
        0,
        &wanted,
        &mut bones,
        &mut HashMap::new(),
    );
    bones
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

/// What `ufbx` is reading: an FBX file, or an OBJ file with its material
/// library's bytes, if it names one that's there.
#[derive(Clone, Copy)]
pub(super) enum Source<'a> {
    Fbx,
    Obj {
        /// The `.mtl` file named by the OBJ's `mtllib` line: its path inside
        /// the model's folder and its bytes.
        library: Option<(&'a Path, &'a [u8])>,
    },
}

/// The material library an OBJ file names (`mtllib materials.mtl`), as a
/// path inside the model's folder, if it names one that's a safe path.
/// Only the first is used, as `ufbx` reads one.
pub(super) fn obj_library(bytes: &[u8]) -> Option<PathBuf> {
    let text = String::from_utf8_lossy(bytes);
    text.lines().find_map(|line| {
        let name = line.trim_start().strip_prefix("mtllib")?;
        // `mtllib` followed by a space or tab, then the file name (which
        // may itself have spaces in it).
        if !name.starts_with([' ', '\t']) {
            return None;
        }
        crate::safe_path(&name.trim().replace('\\', "/"))
    })
}

fn parse(bytes: &[u8], source: Source<'_>) -> Result<ufbx::SceneRoot, ModelError> {
    // glTF's space: +X right, +Y up, +Z front, in meters.
    let gltf_axes = CoordinateAxes {
        right: CoordinateAxis::PositiveX,
        up: CoordinateAxis::PositiveY,
        front: CoordinateAxis::PositiveZ,
    };
    let (file_format, library) = match source {
        Source::Fbx => (ufbx::FileFormat::Fbx, None),
        Source::Obj { library } => (ufbx::FileFormat::Obj, library.map(|(_, bytes)| bytes)),
    };
    let opts = ufbx::LoadOpts {
        // Only the bytes given (and an OBJ's library, read here), in the
        // format its extension says.
        load_external_files: false,
        file_format,
        obj_mtl_data: library.map_or(ufbx::BlobOpt::Unset, ufbx::BlobOpt::Ref),
        // An OBJ file doesn't say its axes or units: take them as glTF's
        // (Y up, 1 unit = 1 meter = 1 stud), as Blender and most tools
        // export it. A Z-up export is turned when it's imported.
        obj_axes: gltf_axes,
        obj_unit_meters: 1.0,
        ignore_animation: false,
        generate_missing_normals: true,
        // Blender saves its Principled BSDF's metalness and roughness in
        // its own properties.
        use_blender_pbr_material: true,
        node_depth_limit: MAX_NODE_DEPTH as u32,
        target_axes: gltf_axes,
        target_unit_meters: 1.0,
        ..Default::default()
    };
    ufbx::load_memory(bytes, opts).map_err(|err| {
        let mut message = err.description.to_string();
        let info = err.info();
        if !info.is_empty() {
            message.push_str(&format!(" ({info})"));
        }
        match source {
            Source::Fbx => ModelError::Fbx(message),
            Source::Obj { .. } => ModelError::Obj(message),
        }
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
    /// Where the model is in its project, for cooked copies.
    in_project: Option<InProject<'a>>,
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
    /// The level of detail of the objects being read (see [`super::lod`]).
    level: u8,
    /// The collision pieces read (see [`super::collision`]).
    pieces: super::collision::Pieces,
}

impl Reader<'_> {
    /// The project folder, where cooked textures are looked for.
    fn project(&self) -> Option<&Path> {
        self.in_project.map(|in_project| in_project.project)
    }

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
            // Everything inside a `_LOD1` node is part of that copy.
            let outer = self.level;
            self.level = self.level.max(super::lod::level_of(&node_name(child)));
            // And everything inside a `_COL` node is part of that piece.
            let outer_piece = self.pieces.enter(&node_name(child));
            // Hidden in the program it was made in, so hidden here too. A
            // level-of-detail group shows one copy at a time, so the others
            // are often hidden, and collision pieces usually are; they're
            // still read.
            if let Some(mesh) = &child.mesh
                && (child.visible || self.level > 0 || self.pieces.reading())
                && want(wanted, found, &child_path)
            {
                if self.pieces.reading() {
                    let transform = mat4(&child.geometry_to_world);
                    self.pieces.add(mesh.vertices.iter().map(|corner| {
                        transform
                            .transform_point3(dvec3(*corner))
                            .as_vec3()
                            .to_array()
                    }));
                } else {
                    self.mesh(child, mesh);
                }
            }
            self.children(child, &child_path, wanted, found);
            self.level = outer;
            self.pieces.leave(outer_piece);
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
                lods: Vec::new(),
                level: self.level,
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
                let channel = if image.has_alpha() {
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

        // Where the textures sit on the coordinates: the base color map's
        // scale and offset, or else the normal map's.
        let (uv_scale, uv_offset) = [&pbr.base_color, &pbr.normal_map, &fbx.normal_map]
            .into_iter()
            .filter(|map| map.texture_enabled)
            .find_map(|map| {
                map.texture
                    .as_ref()
                    .and_then(|texture| uv_transform(texture))
            })
            .unwrap_or(([1.0, 1.0], [0.0, 0.0]));

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
            uv_scale,
            uv_offset,
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
                    "has a texture `{name}` it doesn't know the use of, so it's left out; \
                     naming it like `{}_Normal` or `_Roughness` says what it's for",
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
        let cooked = if texture.content.is_empty() {
            texture_path(texture, self.folder).and_then(|path| {
                cooked_texture(self.in_project, self.folder, &slash_path(&path), None)
            })
        } else {
            self.embedded.get(&id).and_then(|file| {
                let file = format!("{}/{file}", crate::import::TEXTURES_FOLDER);
                cooked_texture(self.in_project, self.folder, &file, Some(&texture.content))
            })
        };
        if let Some(image) = cooked {
            let image = Some(image);
            self.images.insert(id, image.clone());
            return image;
        }
        let image = if texture.content.is_empty() {
            match texture_path(texture, self.folder) {
                Some(path) => match std::fs::read(self.folder.join(&path)) {
                    Ok(bytes) => decode_image(&bytes, &name).map(|image| Image {
                        file: Some(slash_path(&path)),
                        origin: file_origin(self.project(), self.folder, Some(&slash_path(&path))),
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
    let [red, green, blue, _] = image.sampled_average().map(f32::from);
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

/// The scale and offset that place `texture` (or, for a layered texture,
/// its first file) on the mesh's texture coordinates, in the engine's
/// space, or `None` if it has no transform. Its rotation is left out.
fn uv_transform(texture: &ufbx::Texture) -> Option<([f32; 2], [f32; 2])> {
    let file = texture.file_textures.first().map(|file| &**file);
    let texture = std::iter::once(texture)
        .chain(file)
        .find(|texture| texture.has_uv_transform)?;
    let m = &texture.uv_to_texture;
    Some(engine_uv_transform([m.m00, m.m11], [m.m03, m.m13]))
}

/// A texture placement `scale * uv + offset` in FBX's coordinates, whose V
/// runs up from the bottom of the texture, in the engine's, whose V runs
/// down from the top: V is `1 - v` in both the mesh's coordinates and
/// the texture's, so the offset's V becomes `1 - scale_v - offset_v`.
fn engine_uv_transform([su, sv]: [f64; 2], [ou, ov]: [f64; 2]) -> ([f32; 2], [f32; 2]) {
    let finite = |value: f64, default: f64| if value.is_finite() { value } else { default };
    let (su, sv) = (finite(su, 1.0), finite(sv, 1.0));
    let (ou, ov) = (finite(ou, 0.0), finite(ov, 0.0));
    ([su as f32, sv as f32], [ou as f32, (1.0 - sv - ov) as f32])
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
mod obj_tests;
#[cfg(test)]
pub(crate) mod tests;
