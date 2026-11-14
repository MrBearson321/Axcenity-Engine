//! Levels of detail: simpler copies of a model's primitives, with fewer
//! triangles, that the renderer draws in their place when they're small on
//! screen ([`Primitive::lods`]).
//!
//! A file's own copies are used when it has them: objects named like the
//! full one with `_LOD1` and `_LOD2` on the end (`_LOD0` is the full one),
//! matched to its primitives by material. Otherwise each big enough
//! primitive is simplified with meshoptimizer to half, then a quarter, of
//! its triangles when it's loaded, and kept with the model in the cache.
//! Cut-out primitives (leaves, fences: [`AlphaMode::Mask`]) aren't
//! simplified: moving their corners stretches the texture's holes, so
//! leaves changed shape and bushes thinned or clumped visibly at the
//! switch. A file's own copies of them are still used.
//! A project's model also keeps them on disk ([`super::lod_cache`]), so
//! loading it again doesn't simplify again.

use super::lod_cache::{self, Levels};
use super::{AlphaMode, InProject, Primitive, new_key};
use crate::cooked::ContentHasher;

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
pub fn level_of(name: &str) -> u8 {
    let name = name.trim().to_ascii_lowercase();
    (1..=LEVELS)
        .find(|level| name.ends_with(&format!("_lod{level}")))
        .unwrap_or(0)
}

/// The path of the full object a `_LOD1` or `_LOD2` copy at `path` is a
/// copy of (`Rock` for `Rock_LOD1`), or `None` if it isn't one.
pub(super) fn full_path(path: &str) -> Option<&str> {
    let path = path.trim_end();
    (level_of(path) > 0).then(|| &path[..path.len() - "_lod1".len()])
}

/// Splits `primitives` into the full model's and their simpler copies, and
/// gives each full one its copies: the file's own, or simplified ones
/// (from the mesh cache, for a model `in_project` that has them there).
/// The primitives must already be fitted, so all of them are placed alike.
pub(super) fn attach(
    primitives: Vec<Primitive>,
    in_project: Option<InProject<'_>>,
) -> Vec<Primitive> {
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
    }
    let simplify: Vec<usize> = (0..full.len())
        .filter(|&index| {
            full[index].lods.is_empty()
                && full[index].indices.len() / 3 >= MIN_TRIANGLES
                && !matches!(full[index].material.alpha_mode, AlphaMode::Mask { .. })
        })
        .collect();
    if simplify.is_empty() {
        return full;
    }
    let cache = in_project.map(|at| (at, key(simplify.iter().map(|&index| &full[index]))));
    let corners: Vec<usize> = simplify
        .iter()
        .map(|&index| full[index].positions.len())
        .collect();
    let levels = match cache.and_then(|(at, key)| lod_cache::read(at, key, &corners)) {
        Some(levels) => levels,
        None => {
            let levels: Levels = simplify
                .iter()
                .map(|&index| simplified(&full[index]))
                .collect();
            if let Some((at, key)) = cache {
                lod_cache::write(at, key, &levels);
            }
            levels
        }
    };
    for (index, levels) in simplify.into_iter().zip(levels) {
        let primitive = &full[index];
        let lods = levels
            .iter()
            .map(|indices| primitive.with_triangles(indices))
            .collect();
        full[index].lods = lods;
    }
    full
}

/// What the simplified copies of `primitives` depend on: the settings
/// above and each one's corners, normals, texture coordinates and
/// triangles. Their mesh cache file is found by it.
fn key<'a>(primitives: impl Iterator<Item = &'a Primitive>) -> u64 {
    // Bump to make every cached copy stale, when simplifying changes in a
    // way the settings below don't show.
    const VERSION: u32 = 1;
    let mut hasher = ContentHasher::new(u64::from(VERSION));
    for (target, max_error) in TARGETS.into_iter().zip(MAX_ERRORS) {
        hasher.pair(target.to_bits(), max_error.to_bits());
    }
    hasher.pair(MIN_SAVING.to_bits(), MIN_TRIANGLES as u32);
    hasher.pair(NORMAL_WEIGHT.to_bits(), UV_WEIGHT.to_bits());
    for primitive in primitives {
        hasher.pair(
            primitive.positions.len() as u32,
            primitive.indices.len() as u32,
        );
        hasher.pair(primitive.normals.len() as u32, primitive.uvs.len() as u32);
        let floats = primitive
            .positions
            .iter()
            .chain(&primitive.normals)
            .flatten()
            .chain(primitive.uvs.iter().flatten());
        let mut floats = floats.map(|value| value.to_bits());
        while let Some(low) = floats.next() {
            hasher.pair(low, floats.next().unwrap_or(0));
        }
        for pair in primitive.indices.chunks(2) {
            hasher.pair(pair[0], pair.get(1).copied().unwrap_or(0));
        }
    }
    hasher.finish()
}

/// `primitive`'s triangles simplified to [`TARGETS`] of them, as lists of
/// its corners, each kept only if it saves enough on the one before.
fn simplified(primitive: &Primitive) -> Vec<Vec<u32>> {
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
        lods.push(indices);
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
        let [full] = attach(vec![grid], None).try_into().ok().unwrap();
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
    fn a_project_model_keeps_its_copies_in_the_mesh_cache() {
        let project =
            std::env::temp_dir().join(format!("axcenity-lod-cache-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&project);
        let model = project.join("assets/models/grid.glb");
        std::fs::create_dir_all(model.parent().unwrap()).unwrap();
        std::fs::write(&model, b"model").unwrap();
        let at = InProject {
            project: &project,
            relative: "assets/models/grid.glb",
            bytes: b"model",
        };
        let cached = || lod_cache::cached_meshes(&project, "assets/models/grid.glb");
        let triangles = |primitive: &Primitive| -> Vec<Vec<u32>> {
            let lods = primitive.lods.iter();
            lods.map(|copy| copy.indices.clone()).collect()
        };

        let [made] = attach(vec![wavy_grid(40)], Some(at))
            .try_into()
            .ok()
            .unwrap();
        assert_eq!(made.lods.len(), 2);
        assert_eq!(cached().len(), 1, "simplifying writes the cache");
        let file = project.join(&cached()[0]);
        let written = std::fs::metadata(&file).unwrap().modified().unwrap();

        let [read] = attach(vec![wavy_grid(40)], Some(at))
            .try_into()
            .ok()
            .unwrap();
        assert_eq!(triangles(&read), triangles(&made), "read back the same");
        assert_eq!(
            std::fs::metadata(&file).unwrap().modified().unwrap(),
            written
        );

        // Another pick of the same file has its own; a damaged one is
        // simplified again and rewritten.
        attach(vec![wavy_grid(30)], Some(at));
        assert_eq!(cached().len(), 2);
        std::fs::write(&file, b"AXLOD").unwrap();
        let [again] = attach(vec![wavy_grid(40)], Some(at))
            .try_into()
            .ok()
            .unwrap();
        assert_eq!(triangles(&again), triangles(&made));
        assert!(std::fs::read(&file).unwrap().len() > 5);

        // Once the model file changes, its old copies aren't shipped, and
        // the next write clears them away.
        std::fs::write(&model, b"edited").unwrap();
        assert!(cached().is_empty());
        let edited = InProject {
            bytes: b"edited",
            ..at
        };
        attach(vec![wavy_grid(20)], Some(edited));
        assert_eq!(cached().len(), 1);
        assert_eq!(
            std::fs::read_dir(project.join(lod_cache::MESH_CACHE_FOLDER))
                .unwrap()
                .count(),
            1
        );
        let _ = std::fs::remove_dir_all(&project);
    }

    #[test]
    fn a_cut_out_primitive_is_not_simplified() {
        let mut leaves = wavy_grid(40);
        leaves.material.alpha_mode = AlphaMode::Mask { cutoff: 0.5 };
        assert!(attach(vec![leaves], None)[0].lods.is_empty());
    }

    #[test]
    fn a_small_primitive_is_not_simplified() {
        let grid = wavy_grid(8);
        assert!(grid.indices.len() / 3 < MIN_TRIANGLES);
        assert!(attach(vec![grid], None)[0].lods.is_empty());
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
