//! The Model Studio's sidecar (Phase 57): what a model's `.glb` can't
//! hold, saved beside it as `<model>.meshedit.json`. Games load the
//! `.glb`, which has each object's modifiers already on it; the sidecar
//! keeps each object's own mesh and its modifier stack, so reopening the
//! model in the Studio shows them still editable.
//!
//! The sidecar names the `.glb` it goes with by a hash of its bytes. If
//! the `.glb` was changed since (exported again from another program), the
//! sidecar is out of date and isn't used.

use axcenity_assets::MESH_EDIT_SUFFIX;
use glam::{Vec2, Vec3};
use serde::{Deserialize, Serialize};

use crate::half_edge::{EditMesh, Polygon};
use crate::model_doc::ModelDoc;
use crate::modifiers::Modifier;

const VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
struct Sidecar {
    version: u32,
    /// The hash of the `.glb` it was saved with ([`hash`]).
    glb: String,
    objects: Vec<SidecarObject>,
}

#[derive(Serialize, Deserialize)]
struct SidecarObject {
    /// Its place in the model's object list, and its name.
    index: usize,
    name: String,
    /// The names of its material slots, which the faces' numbers refer to.
    materials: Vec<String>,
    modifiers: Vec<Modifier>,
    mesh: MeshData,
}

#[derive(Serialize, Deserialize)]
struct MeshData {
    positions: Vec<[f32; 3]>,
    faces: Vec<FaceData>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    auto_smooth: Option<f32>,
}

#[derive(Serialize, Deserialize)]
struct FaceData {
    corners: Vec<u32>,
    uvs: Vec<[f32; 2]>,
    normals: Vec<[f32; 3]>,
    material: u32,
    smooth: bool,
}

/// The project path of the sidecar of the model at `model`
/// (`assets/models/Head.glb` has `assets/models/Head.meshedit.json`).
pub fn sidecar_path(model: &str) -> String {
    let (folder, name) = model.rsplit_once('/').unwrap_or(("", model));
    let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
    match folder {
        "" => format!("{stem}{MESH_EDIT_SUFFIX}"),
        _ => format!("{folder}/{stem}{MESH_EDIT_SUFFIX}"),
    }
}

impl ModelDoc {
    /// The sidecar for this model saved as `glb` (what [`write_glb`]
    /// made), or `None` if no object has modifiers, so there's nothing a
    /// `.glb` can't hold.
    ///
    /// [`write_glb`]: ModelDoc::write_glb
    pub fn write_sidecar(&self, glb: &[u8]) -> Option<Vec<u8>> {
        let objects: Vec<SidecarObject> = self
            .objects
            .iter()
            .enumerate()
            .filter(|(_, object)| !object.modifiers.is_empty())
            .filter_map(|(index, object)| {
                Some(SidecarObject {
                    index,
                    name: object.name.clone(),
                    materials: object.materials.iter().map(|m| m.name.clone()).collect(),
                    modifiers: object.modifiers.clone(),
                    mesh: mesh_data(object.mesh.as_ref()?),
                })
            })
            .collect();
        if objects.is_empty() {
            return None;
        }
        let sidecar = Sidecar {
            version: VERSION,
            glb: hash(glb),
            objects,
        };
        Some(serde_json::to_vec_pretty(&sidecar).expect("plain data serializes"))
    }

    /// Puts back each object's own mesh and modifiers from a `sidecar`
    /// saved with `glb` (the bytes this model was opened from). An object
    /// is found by its place and name, or else its name. Returns how many
    /// objects it put back, or why it couldn't be used.
    pub fn read_sidecar(&mut self, sidecar: &[u8], glb: &[u8]) -> Result<usize, String> {
        let sidecar: Sidecar =
            serde_json::from_slice(sidecar).map_err(|err| format!("it can't be read: {err}"))?;
        if sidecar.version > VERSION {
            return Err("it was saved by a newer Axcenity".to_owned());
        }
        if sidecar.glb != hash(glb) {
            return Err("the model was changed since it was saved".to_owned());
        }
        let mut restored = 0;
        for saved in sidecar.objects {
            let found = self
                .objects
                .get(saved.index)
                .filter(|object| object.name == saved.name && object.mesh.is_some())
                .map(|_| saved.index)
                .or_else(|| {
                    self.objects
                        .iter()
                        .position(|object| object.name == saved.name && object.mesh.is_some())
                });
            let Some(index) = found else {
                continue;
            };
            let object = &mut self.objects[index];
            // The faces' slots as they were saved, by name, in this
            // object's slots now (a slot nothing drew isn't in the `.glb`).
            let slot_now: Vec<u32> = saved
                .materials
                .iter()
                .map(|name| {
                    object
                        .materials
                        .iter()
                        .position(|m| &m.name == name)
                        .unwrap_or(0) as u32
                })
                .collect();
            object.mesh = Some(edit_mesh(saved.mesh, &slot_now));
            object.modifiers = saved.modifiers;
            restored += 1;
        }
        Ok(restored)
    }
}

fn mesh_data(mesh: &EditMesh) -> MeshData {
    MeshData {
        positions: mesh.positions().iter().map(|p| p.to_array()).collect(),
        faces: (0..mesh.face_count() as u32)
            .map(|face| {
                let corners = mesh.face_half_edges(face);
                let corner = |e: &u32| *mesh.half_edge(*e);
                FaceData {
                    corners: corners.iter().map(|e| corner(e).vertex).collect(),
                    uvs: corners.iter().map(|e| corner(e).uv.to_array()).collect(),
                    normals: corners
                        .iter()
                        .map(|e| corner(e).normal.to_array())
                        .collect(),
                    material: mesh.face(face).material,
                    smooth: mesh.face(face).smooth,
                }
            })
            .collect(),
        auto_smooth: mesh.auto_smooth(),
    }
}

/// The mesh `data` holds, its faces' material numbers turned into slots
/// by `slot_now`.
fn edit_mesh(data: MeshData, slot_now: &[u32]) -> EditMesh {
    let positions: Vec<Vec3> = data.positions.into_iter().map(Vec3::from).collect();
    let polygons: Vec<Polygon> = data
        .faces
        .into_iter()
        .map(|face| Polygon {
            corners: face.corners,
            uvs: face.uvs.into_iter().map(Vec2::from).collect(),
            normals: face.normals.into_iter().map(Vec3::from).collect(),
            material: slot_now.get(face.material as usize).copied().unwrap_or(0),
            smooth: face.smooth,
        })
        .collect();
    let mut mesh = EditMesh::new(positions, &polygons);
    mesh.set_auto_smooth(data.auto_smooth);
    mesh
}

/// A hash of `bytes` (64-bit FNV-1a), as hex: the same everywhere, so a
/// sidecar finds its `.glb` on any machine.
fn hash(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{hash:016x}")
}
