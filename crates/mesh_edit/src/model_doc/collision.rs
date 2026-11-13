//! A model's collision pieces in the Model Studio (roadmap R15.2b):
//! objects named `<name>_COL1`, `_COL2` … that say what `<name>` collides
//! as (decision D2 in `docs/research/model-studio-r15.md`; `assets` reads
//! them as `Model::collision`). Each piece is the convex hull of its
//! corners. The Studio makes them from a box, a hull or a convex
//! decomposition (`physics` works the hulls out); here they're put in the
//! document as ordinary objects, so they're edited, undone and saved like
//! any other.

use axcenity_assets::{ModelMaterial, is_collision_piece};
use glam::{Vec2, Vec3};

use super::{DocObject, ModelDoc};
use crate::half_edge::{EditMesh, Polygon};
use crate::modifiers::Purpose;

/// A convex piece as the Studio hands it in: its corners and triangles, in
/// the file's units.
pub type PieceMesh = (Vec<Vec3>, Vec<[u32; 3]>);

/// The colour the Studio shows pieces in, and the material they're saved
/// with (games never draw them).
pub const PIECE_COLOR: [f32; 4] = [0.25, 0.85, 0.4, 1.0];

impl ModelDoc {
    /// True if object `index` is a collision piece or inside one: it isn't
    /// drawn, in the game or (but as an outline) in the Studio.
    pub fn is_collision_piece(&self, index: usize) -> bool {
        let Some(object) = self.objects.get(index) else {
            return false;
        };
        if is_collision_piece(&object.name) {
            return true;
        }
        // The groups it's inside: the nearest object before it with a
        // smaller depth, then that one's, and so on.
        let mut depth = object.depth;
        for outer in self.objects[..index].iter().rev() {
            if outer.depth < depth {
                if is_collision_piece(&outer.name) {
                    return true;
                }
                depth = outer.depth;
            }
        }
        false
    }

    /// The collision pieces of object `index`: the objects with a mesh at
    /// its depth named like it with `_COL` (and a number) on the end, in
    /// any case. Empty for a group, a LOD copy or a piece.
    pub fn collision_pieces(&self, index: usize) -> Vec<usize> {
        let Some(object) = self.objects.get(index) else {
            return Vec::new();
        };
        if object.mesh.is_none() || self.is_lod_copy(index) || self.is_collision_piece(index) {
            return Vec::new();
        }
        (0..self.objects.len())
            .filter(|&k| {
                let other = &self.objects[k];
                other.mesh.is_some()
                    && other.depth == object.depth
                    && owner(&other.name) == Some(object.name.as_str())
            })
            .collect()
    }

    /// Object `index`'s corners with its modifiers on, as the game gets
    /// them, and its triangles: what Generate Hull and Generate Convex
    /// Pieces work from. `None` for a group.
    pub fn collision_source(&self, index: usize) -> Option<PieceMesh> {
        let mesh = self.objects.get(index)?.evaluated(Purpose::Export)?;
        let triangles = (0..mesh.face_count() as u32)
            .flat_map(|face| {
                mesh.triangles(face)
                    .into_iter()
                    .map(|corners| corners.map(|half_edge| mesh.half_edge(half_edge).vertex))
            })
            .collect();
        Some((mesh.positions().to_vec(), triangles))
    }

    /// Puts `pieces` in as object `index`'s collision, `<name>_COL1` …,
    /// right after it (and what's inside it), in place of pieces it had.
    /// Returns its index after; `None` for a group, a copy or a piece, or
    /// if no piece has three corners.
    pub fn set_collision(&mut self, index: usize, pieces: Vec<PieceMesh>) -> Option<usize> {
        let meshes: Vec<EditMesh> = pieces
            .into_iter()
            .filter_map(|(corners, triangles)| piece_mesh(corners, &triangles))
            .collect();
        if meshes.is_empty() {
            return None;
        }
        let index = self.remove_collision(index)?;
        let (name, depth) = {
            let object = &self.objects[index];
            (object.name.clone(), object.depth)
        };
        let mut after = index + 1;
        while self
            .objects
            .get(after)
            .is_some_and(|inside| inside.depth > depth)
        {
            after += 1;
        }
        for (k, mesh) in meshes.into_iter().enumerate() {
            self.objects.insert(
                after + k,
                DocObject {
                    name: format!("{name}_COL{}", k + 1),
                    depth,
                    mesh: Some(mesh),
                    materials: vec![piece_material()],
                    uvs: false,
                    modifiers: Vec::new(),
                    paint: Vec::new(),
                    skin: None,
                    mask: Vec::new(),
                },
            );
        }
        Some(index)
    }

    /// Takes object `index`'s collision pieces out (see
    /// [`collision_pieces`](Self::collision_pieces)). Returns its index
    /// after; `None` for a group, a copy or a piece.
    pub fn remove_collision(&mut self, index: usize) -> Option<usize> {
        let object = self.objects.get(index)?;
        if object.mesh.is_none() || self.is_lod_copy(index) || self.is_collision_piece(index) {
            return None;
        }
        let pieces = self.collision_pieces(index);
        let mut at = 0;
        self.objects.retain(|_| {
            at += 1;
            !pieces.contains(&(at - 1))
        });
        Some(index - pieces.iter().filter(|&&piece| piece < index).count())
    }
}

/// The box around `corners`, as a piece.
pub fn box_piece(corners: &[Vec3]) -> Option<PieceMesh> {
    let first = *corners.first()?;
    let (min, max) = corners
        .iter()
        .fold((first, first), |(min, max), &p| (min.min(p), max.max(p)));
    let at = |x: bool, y: bool, z: bool| {
        Vec3::new(
            if x { max.x } else { min.x },
            if y { max.y } else { min.y },
            if z { max.z } else { min.z },
        )
    };
    let corners: Vec<Vec3> = (0..8)
        .map(|k| at(k & 1 != 0, k & 2 != 0, k & 4 != 0))
        .collect();
    // Each face's corners counterclockwise from outside, as two triangles.
    let faces: [[u32; 4]; 6] = [
        [0, 2, 3, 1], // -z
        [4, 5, 7, 6], // +z
        [0, 1, 5, 4], // -y
        [2, 6, 7, 3], // +y
        [0, 4, 6, 2], // -x
        [1, 3, 7, 5], // +x
    ];
    let triangles = faces
        .iter()
        .flat_map(|&[a, b, c, d]| [[a, b, c], [a, c, d]])
        .collect();
    Some((corners, triangles))
}

/// The name of the object a piece named `name` belongs to (`Rock` for
/// `Rock_COL2`), as `assets` reads it.
fn owner(name: &str) -> Option<&str> {
    let name = name.trim_end();
    if !is_collision_piece(name) {
        return None;
    }
    let without_number = name.trim_end_matches(|c: char| c.is_ascii_digit());
    Some(&without_number[..without_number.len() - "_col".len()])
}

/// A piece's triangles as an editable mesh, flat shaded, without uvs.
fn piece_mesh(corners: Vec<Vec3>, triangles: &[[u32; 3]]) -> Option<EditMesh> {
    if corners.len() < 3 || triangles.is_empty() {
        return None;
    }
    let polygons: Vec<Polygon> = triangles
        .iter()
        .map(|triangle| Polygon {
            corners: triangle.to_vec(),
            uvs: vec![Vec2::ZERO; 3],
            uvs2: Vec::new(),
            normals: Vec::new(),
            material: 0,
            smooth: false,
        })
        .collect();
    let mesh = EditMesh::new(corners, &polygons);
    (mesh.face_count() > 0).then_some(mesh)
}

fn piece_material() -> ModelMaterial {
    ModelMaterial {
        name: "Collision".to_owned(),
        base_color: PIECE_COLOR,
        roughness: 1.0,
        ..ModelMaterial::default()
    }
}
