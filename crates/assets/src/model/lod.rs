//! Levels of detail: simpler copies of a model's primitives, with fewer
//! triangles, that the renderer draws in their place when they're small on
//! screen ([`Primitive::lods`]).
//!
//! A file's own copies are used when it has them: objects named like the
//! full one with `_LOD1` and `_LOD2` on the end (`_LOD0` is the full one),
//! matched to its primitives by material. Otherwise each big enough
//! primitive is simplified with meshoptimizer to half, then a quarter, of
//! its triangles when it's loaded, and kept with the model in the cache.

use super::Primitive;
use super::new_key;

/// How many simpler copies a primitive can have.
pub(super) const LEVELS: u8 = 2;

/// The share of the full primitive's triangles each simplified copy aims for.
const TARGETS: [f32; LEVELS as usize] = [0.5, 0.25];

/// How far each simplified copy's surface may move, as a share of the
/// primitive's size.
const MAX_ERRORS: [f32; LEVELS as usize] = [0.02, 0.05];

/// Primitives with fewer triangles than this aren't simplified: they cost
/// little to draw already.
const MIN_TRIANGLES: usize = 256;

/// A simplified copy is kept only if it has at most this share of the
/// triangles of the level before it.
const MIN_SAVING: f32 = 0.8;

/// How much normals and texture coordinates count against moving corners
/// when simplifying, so creases and texture seams are kept.
const NORMAL_WEIGHT: f32 = 0.5;
const UV_WEIGHT: f32 = 1.0;

/// The level of detail an object of this name holds: 1 or 2 for a name
/// ending in `_LOD1` or `_LOD2` (in any case), else 0, the full model.
pub(super) fn level_of(name: &str) -> u8 {
    let name = name.trim().to_ascii_lowercase();
    (1..=LEVELS)
        .find(|level| name.ends_with(&format!("_lod{level}")))
        .unwrap_or(0)
}

/// Splits `primitives` into the full model's and their simpler copies, and
/// gives each full one its copies: the file's own, or simplified ones.
/// The primitives must already be fitted, so all of them are placed alike.
pub(super) fn attach(primitives: Vec<Primitive>) -> Vec<Primitive> {
    let (mut full, copies): (Vec<Primitive>, Vec<Primitive>) = primitives
        .into_iter()
        .partition(|primitive| primitive.level == 0);
    for copy in copies {
        // Its full primitive has its material, or is the only one.
        let only = full.len() == 1;
        let Some(owner) = full
            .iter_mut()
            .find(|primitive| only || primitive.material.name == copy.material.name)
        else {
            continue;
        };
        let level = usize::from(copy.level);
        while owner.lods.len() < level {
            owner.lods.push(Primitive::empty_like(owner));
        }
        owner.lods[level - 1].append(copy);
    }
    for primitive in &mut full {
        // A level the file skipped (it has `_LOD2` but no `_LOD1`) is left
        // out, so each copy is simpler than the last.
        primitive.lods.retain(|copy| !copy.indices.is_empty());
        if primitive.lods.is_empty() {
            primitive.lods = simplified(primitive);
        }
    }
    full
}

/// Copies of `primitive` simplified to [`TARGETS`] of its triangles, each
/// kept only if it saves enough on the one before.
fn simplified(primitive: &Primitive) -> Vec<Primitive> {
    let triangles = primitive.indices.len() / 3;
    if triangles < MIN_TRIANGLES {
        return Vec::new();
    }
    let positions: Vec<u8> = primitive
        .positions
        .iter()
        .flatten()
        .flat_map(|c| c.to_ne_bytes())
        .collect();
    let Ok(vertices) = meshopt::VertexDataAdapter::new(&positions, 12, 0) else {
        return Vec::new();
    };
    // Each corner's normal and texture coordinates, weighed against moving it.
    let count = primitive.positions.len();
    let has_uvs = primitive.uvs.len() == count;
    let stride = if has_uvs { 5 } else { 3 };
    let mut attributes = Vec::with_capacity(count * stride);
    for vertex in 0..count {
        attributes.extend(primitive.normals.get(vertex).unwrap_or(&[0.0; 3]));
        if has_uvs {
            attributes.extend(primitive.uvs[vertex]);
        }
    }
    let mut weights = vec![NORMAL_WEIGHT; 3];
    if has_uvs {
        weights.extend([UV_WEIGHT; 2]);
    }
    let locks = vec![false; count];

    let mut lods = Vec::new();
    let mut last = triangles;
    for (target, max_error) in TARGETS.into_iter().zip(MAX_ERRORS) {
        let indices = meshopt::simplify_with_attributes_and_locks(
            &primitive.indices,
            &vertices,
            &attributes,
            &weights,
            stride * size_of::<f32>(),
            &locks,
            (triangles as f32 * target) as usize * 3,
            max_error,
            meshopt::SimplifyOptions::None,
            None,
        );
        let kept = indices.len() / 3;
        if kept == 0 || kept as f32 > last as f32 * MIN_SAVING {
            break;
        }
        last = kept;
        lods.push(primitive.with_triangles(&indices));
    }
    lods
}

impl Primitive {
    /// A primitive with no triangles and `like`'s material, to add to.
    fn empty_like(like: &Primitive) -> Self {
        Self {
            key: new_key(),
            positions: Vec::new(),
            normals: Vec::new(),
            uvs: Vec::new(),
            tangents: Vec::new(),
            indices: Vec::new(),
            material: like.material.clone(),
            joints: Vec::new(),
            weights: Vec::new(),
            lods: Vec::new(),
            level: like.level,
        }
    }

    /// A copy with only the triangles `indices` (into this one's corners),
    /// keeping only the corners they use.
    pub(super) fn with_triangles(&self, indices: &[u32]) -> Self {
        const UNUSED: u32 = u32::MAX;
        let mut remap = vec![UNUSED; self.positions.len()];
        let mut used = Vec::new();
        let indices = indices
            .iter()
            .map(|&index| {
                let slot = &mut remap[index as usize];
                if *slot == UNUSED {
                    *slot = used.len() as u32;
                    used.push(index as usize);
                }
                *slot
            })
            .collect();
        // Each list keeps its corners, or stays empty if it was.
        fn pick<T: Copy>(values: &[T], used: &[usize]) -> Vec<T> {
            if values.is_empty() {
                return Vec::new();
            }
            used.iter().map(|&index| values[index]).collect()
        }
        Self {
            key: new_key(),
            positions: pick(&self.positions, &used),
            normals: pick(&self.normals, &used),
            uvs: pick(&self.uvs, &used),
            tangents: pick(&self.tangents, &used),
            indices,
            material: self.material.clone(),
            joints: pick(&self.joints, &used),
            weights: pick(&self.weights, &used),
            lods: Vec::new(),
            level: self.level,
        }
    }

    /// Adds `other`'s triangles to this one's. A list one of them has and
    /// the other doesn't is filled in with plain values.
    fn append(&mut self, other: Primitive) {
        let (before, added) = (self.positions.len(), other.positions.len());
        fn join<T: Copy>(ours: &mut Vec<T>, theirs: Vec<T>, before: usize, added: usize, fill: T) {
            if ours.is_empty() && theirs.is_empty() {
                return;
            }
            ours.resize(before, fill);
            if theirs.is_empty() {
                ours.resize(before + added, fill);
            } else {
                ours.extend(theirs);
            }
        }
        join(
            &mut self.normals,
            other.normals,
            before,
            added,
            [0.0, 1.0, 0.0],
        );
        join(&mut self.uvs, other.uvs, before, added, [0.0; 2]);
        join(
            &mut self.tangents,
            other.tangents,
            before,
            added,
            [1.0, 0.0, 0.0, 1.0],
        );
        join(&mut self.joints, other.joints, before, added, [0; 4]);
        join(&mut self.weights, other.weights, before, added, [0.0; 4]);
        self.positions.extend(other.positions);
        self.indices
            .extend(other.indices.into_iter().map(|index| index + before as u32));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A wavy square of `cells` × `cells` squares, two triangles each, with
    /// texture coordinates.
    fn wavy_grid(cells: u32) -> Primitive {
        let side = cells + 1;
        let mut grid = Primitive {
            key: new_key(),
            positions: Vec::new(),
            normals: Vec::new(),
            uvs: Vec::new(),
            tangents: Vec::new(),
            indices: Vec::new(),
            material: Default::default(),
            joints: Vec::new(),
            weights: Vec::new(),
            lods: Vec::new(),
            level: 0,
        };
        for z in 0..side {
            for x in 0..side {
                let (u, v) = (x as f32 / cells as f32, z as f32 / cells as f32);
                let height = 0.1 * (u * 6.0).sin() * (v * 4.0).cos();
                grid.positions.push([u - 0.5, height, v - 0.5]);
                grid.normals.push([0.0, 1.0, 0.0]);
                grid.uvs.push([u, v]);
            }
        }
        for z in 0..cells {
            for x in 0..cells {
                let corner = z * side + x;
                grid.indices.extend([corner, corner + side, corner + 1]);
                grid.indices
                    .extend([corner + 1, corner + side, corner + side + 1]);
            }
        }
        grid
    }

    #[test]
    fn a_big_primitive_gets_copies_with_half_then_a_quarter_of_its_triangles() {
        let grid = wavy_grid(40);
        let triangles = grid.indices.len() / 3;
        let [full] = attach(vec![grid]).try_into().ok().unwrap();
        assert_eq!(full.lods.len(), 2);
        let counts: Vec<usize> = full
            .lods
            .iter()
            .map(|copy| copy.indices.len() / 3)
            .collect();
        assert!(
            counts[0] <= triangles / 2 && counts[0] > 0,
            "{counts:?} of {triangles}"
        );
        assert!(
            counts[1] <= triangles / 4 && counts[1] > 0,
            "{counts:?} of {triangles}"
        );
        for copy in &full.lods {
            // Only the corners it uses, each still one of the full mesh's.
            assert!(copy.positions.len() < full.positions.len());
            assert_eq!(copy.uvs.len(), copy.positions.len());
            assert_eq!(copy.normals.len(), copy.positions.len());
            assert!(
                copy.indices
                    .iter()
                    .all(|&i| (i as usize) < copy.positions.len())
            );
            assert!(copy.positions.iter().all(|p| full.positions.contains(p)));
            assert!(copy.lods.is_empty());
            assert_ne!(copy.key(), full.key());
        }
    }

    #[test]
    fn a_small_primitive_is_not_simplified() {
        let grid = wavy_grid(8);
        assert!(grid.indices.len() / 3 < MIN_TRIANGLES);
        assert!(attach(vec![grid])[0].lods.is_empty());
    }

    #[test]
    fn names_ending_in_lod_1_or_2_are_simpler_copies() {
        assert_eq!(level_of("Rock_LOD1"), 1);
        assert_eq!(level_of("rock_lod2 "), 2);
        assert_eq!(level_of("Rock_LOD0"), 0);
        assert_eq!(level_of("Rock"), 0);
        assert_eq!(level_of("Rock_LOD3"), 0);
        assert_eq!(level_of("LOD1"), 0);
    }
}
