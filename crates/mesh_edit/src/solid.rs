//! Combining parts into one solid (Phase 50): what the editor's Union,
//! Negate and Intersect, and scripts' `UnionAsync`, `SubtractAsync` and
//! `IntersectAsync`, call.
//!
//! Each part becomes a [`Solid`]: its shape (a Part's at its `Size`, a
//! MeshPart's or UnionOperation's model stretched to its `Size`), its
//! looks and where it is. [`combine`] puts them in the first part's frame,
//! runs the booleans and centers the result, which is written as a `.glb`
//! and stretched back to its `Size` like any MeshPart's model.

use std::path::Path;

use axcenity_assets::{Assets, MODELS_FOLDER, safe_file_name, unique_file_name};
use axcenity_core::{ClassName, InstanceId, Value, Vector3, World};
use glam::{EulerRot, Quat, Vec3};

use crate::csg::{Operation, boolean};
use crate::generate::{model_materials, part_look, part_mesh};
use crate::{GlbMaterial, Mesh, Surface, write_glb};

/// A part ready to combine: its closed mesh in its own space, the looks
/// its `Surface::Outer` indices name, and where it is.
#[derive(Clone, Debug)]
pub struct Solid {
    pub name: String,
    pub mesh: Mesh,
    pub materials: Vec<GlbMaterial>,
    pub position: Vec3,
    pub rotation: Quat,
}

/// The solid parts combine into: its mesh centered on `position` in the
/// frame `rotation`, its looks, and the box it fills (a UnionOperation's
/// `Size`).
#[derive(Clone, Debug)]
pub struct Combined {
    pub mesh: Mesh,
    pub materials: Vec<GlbMaterial>,
    pub position: Vec3,
    pub rotation: Quat,
    pub size: Vec3,
}

impl Combined {
    /// The result as a `.glb` with one object, at its size in studs.
    pub fn glb(&self) -> Vec<u8> {
        write_glb(
            std::slice::from_ref(&self.mesh),
            &self.materials,
            &crate::glb::plain_material(),
        )
    }

    /// `rotation` as an `Orientation` in degrees (Z, then X, then Y), each
    /// rounded to 3 decimals as the editor shows them.
    pub fn orientation(&self) -> Vector3 {
        let (y, x, z) = self.rotation.to_euler(EulerRot::YXZ);
        let tidy = |radians: f32| (radians.to_degrees() * 1000.0).round() / 1000.0 + 0.0;
        Vector3::new(tidy(x), tidy(y), tidy(z))
    }
}

/// The Part, NegateOperation, MeshPart or UnionOperation `part` as a
/// solid. An error names it when it can't be: a Capsule, a model that
/// can't be loaded, or a mesh that isn't closed.
pub fn solid_of(world: &World, assets: &Assets, part: InstanceId) -> Result<Solid, String> {
    let instance = world
        .get(part)
        .ok_or_else(|| "A part to combine isn't in the game any more".to_owned())?;
    let name = instance.name().to_owned();
    let vector = |property| {
        instance
            .property(property)
            .and_then(Value::as_vector3)
            .map(|v| Vec3::new(v.x, v.y, v.z))
    };
    let (mesh, materials) = if instance.class().has_model() {
        let mesh_id = instance
            .property("MeshId")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let model = assets
            .model(mesh_id)
            .ok_or_else(|| format!("`{name}` has no model to combine (MeshId `{mesh_id}`)"))?;
        // The model in its file's units, stretched to the part's Size.
        let mut mesh = Mesh::from_model(&model);
        let native = Vec3::from(model.size);
        let size = vector("Size").unwrap_or(native).abs();
        let stretch = Vec3::select(native.cmpgt(Vec3::ZERO), size / native, Vec3::ONE);
        let center = Vec3::from(model.center);
        for position in &mut mesh.positions {
            *position = (*position - center) * stretch;
        }
        for normal in &mut mesh.normals {
            *normal = (*normal / stretch).normalize_or_zero();
        }
        let folder = assets.folder().unwrap_or_default().join(MODELS_FOLDER);
        (mesh, model_materials(&model, &folder))
    } else if instance.class().is_part() {
        (part_mesh(instance, "combined")?, vec![part_look(instance)])
    } else {
        return Err(format!("`{name}` isn't a part, so it can't be combined"));
    };
    let open = mesh.open_edges();
    if open > 0 {
        return Err(format!(
            "`{name}` isn't a closed solid ({open} of its edges have a face on one side only), so it can't be combined"
        ));
    }
    let orientation = instance
        .property("Orientation")
        .and_then(Value::as_vector3)
        .unwrap_or_default();
    let [x, y, z] = [orientation.x, orientation.y, orientation.z].map(f32::to_radians);
    Ok(Solid {
        name,
        mesh,
        materials,
        position: vector("Position").unwrap_or_default(),
        rotation: Quat::from_euler(EulerRot::YXZ, y, x, z),
    })
}

/// `keep` put together by `operation` (`Union`, or `Intersect` to keep
/// only where they all overlap; `Subtract` cuts the rest of `keep` out of
/// the first), then everything in `cut` cut out. The result is in the
/// first of `keep`'s frame. An error says why when nothing is left.
pub fn combine(keep: &[Solid], cut: &[Solid], operation: Operation) -> Result<Combined, String> {
    let first = keep
        .first()
        .ok_or_else(|| "Pick at least one part that isn't negated".to_owned())?;
    let to_frame = first.rotation.inverse();
    let mut materials: Vec<GlbMaterial> = Vec::new();
    let mut place = |solid: &Solid| {
        // Parts that look the same share one surface, so where their
        // faces meet in a plane they become one face.
        let mut index_of: Vec<u32> = solid
            .materials
            .iter()
            .map(|material| {
                let found = materials.iter().position(|seen| seen == material);
                found.unwrap_or_else(|| {
                    materials.push(material.clone());
                    materials.len() - 1
                }) as u32
            })
            .collect();
        if index_of.is_empty() {
            materials.push(crate::glb::plain_material());
            index_of.push(materials.len() as u32 - 1);
        }
        let turn = to_frame * solid.rotation;
        let shift = to_frame * (solid.position - first.position);
        let mut mesh = solid.mesh.clone();
        for position in &mut mesh.positions {
            *position = turn * *position + shift;
        }
        for normal in &mut mesh.normals {
            *normal = turn * *normal;
        }
        for triangle in &mut mesh.triangles {
            let index = match triangle.surface {
                Surface::Outer(index) => index as usize,
                Surface::Interior => 0,
            };
            triangle.surface = Surface::Outer(index_of[index.min(index_of.len() - 1)]);
        }
        mesh
    };
    let mut result = place(first);
    for solid in &keep[1..] {
        let mesh = place(solid);
        result = boolean(&result, &mesh, operation);
    }
    for solid in cut {
        let mesh = place(solid);
        result = boolean(&result, &mesh, Operation::Subtract);
    }
    if result.is_empty() || result.volume() <= 0.0 {
        return Err(match operation {
            Operation::Intersect => "The parts don't overlap, so their intersection is empty",
            _ => "Nothing is left once the negated parts are cut out",
        }
        .to_owned());
    }
    // Pieces of one face with a plain look (no texture) join even where
    // their uvs don't meet, such as two blocks' tops side by side.
    let plain = |surface: Surface| match surface {
        Surface::Outer(index) => materials
            .get(index as usize)
            .is_some_and(|material| material.base_color_texture.is_none()),
        Surface::Interior => false,
    };
    let merged = crate::merge::merge_flat_faces(&result, &plain);
    if merged.open_edges() == 0 {
        result = merged;
    }
    let materials = only_used(&mut result, materials);
    let (min, max) = result.bounds();
    let middle = (min + max) / 2.0;
    for position in &mut result.positions {
        *position -= middle;
    }
    Ok(Combined {
        mesh: result,
        materials,
        position: first.position + first.rotation * middle,
        rotation: first.rotation,
        size: max - min,
    })
}

/// The looks `mesh`'s triangles use, in order, with its surfaces
/// renumbered to match.
fn only_used(mesh: &mut Mesh, materials: Vec<GlbMaterial>) -> Vec<GlbMaterial> {
    let mut new_index = vec![None; materials.len()];
    let mut kept = Vec::new();
    for triangle in &mut mesh.triangles {
        if let Surface::Outer(index) = triangle.surface {
            let index = index as usize;
            if index >= materials.len() {
                continue;
            }
            let new = *new_index[index].get_or_insert_with(|| {
                kept.push(materials[index].clone());
                kept.len() - 1
            });
            triangle.surface = Surface::Outer(new as u32);
        }
    }
    kept
}

/// The parts `parts` combined as the editor's Union (`Operation::Union`)
/// or Intersect does: the NegateOperations are cut out of the rest.
pub fn combine_parts(
    world: &World,
    assets: &Assets,
    parts: &[InstanceId],
    operation: Operation,
) -> Result<Combined, String> {
    let (mut keep, mut cut) = (Vec::new(), Vec::new());
    for &part in parts {
        let solid = solid_of(world, assets, part)?;
        let negated = world
            .get(part)
            .is_some_and(|instance| instance.class() == ClassName::NegateOperation);
        if negated {
            cut.push(solid);
        } else {
            keep.push(solid);
        }
    }
    combine(&keep, &cut, operation)
}

/// Writes a combined solid's `.glb` as `assets/models/<name>.union.glb`
/// (numbered if that's taken) and returns its project-relative path.
pub fn save_union(
    project: &Path,
    assets: &Assets,
    name: &str,
    combined: &Combined,
) -> Result<String, String> {
    let folder = project.join(MODELS_FOLDER);
    std::fs::create_dir_all(&folder)
        .map_err(|err| format!("Could not make the folder `{MODELS_FOLDER}`: {err}"))?;
    let stem = safe_file_name(name).unwrap_or_else(|| "Union".to_owned());
    let file = unique_file_name(&format!("{stem}.union.glb"), |file| {
        folder.join(file).exists()
    });
    let path = format!("{MODELS_FOLDER}/{file}");
    std::fs::write(project.join(&path), combined.glb())
        .map_err(|err| format!("Could not write `{path}`: {err}"))?;
    assets.forget(&path);
    Ok(path)
}
