//! A Terrain's heightfield, hand-painted material weights, and foliage.
//!
//! These aren't Instance properties: a 513×513 heightfield is a quarter of a
//! million numbers. They live in the [`World`](crate::World) beside the
//! Terrain Instance (like an Animator's live state), behind an `Arc`, so
//! Play's copy of the world shares them until something changes one.
//! Painted foliage is a list of plants and props on the surface, not one
//! Instance per plant.
//!
//! The grid has `size_x` samples along X and `size_z` along Z, row by row
//! (`z * size_x + x`). Heights go from 0 (the bottom of the Terrain's
//! `Size.Y`) to 1 (the top), so resizing the Terrain stretches it.
//!
//! The grid is split into square chunks of [`CHUNK_CELLS`] cells. Each chunk
//! counts its edits (`height_version`, `paint_version`), so whatever draws
//! or collides with the terrain only rebuilds the chunks that changed.
//!
//! Besides the painted foliage, each chunk can hold **generated** foliage:
//! what the terrain's ScatterRules place there (procedural worlds PW1). It's
//! made from the rules and the ground on every device, so it's never saved
//! (`foliage()` is only the painted list); each chunk keeps the key of the
//! inputs it was made from, so only chunks whose inputs changed are made
//! again. [`TerrainData::all_foliage`] is both lists, for drawing and
//! colliding.
//!
//! **Holes** (procedural worlds PW7) cut cells out of the surface, for cave
//! mouths and tunnels: cell (`x`, `z`), between samples `x` and `x + 1`
//! and `z` and `z + 1`, is a hole when sample (`x`, `z`)'s hole flag is
//! set. It isn't drawn, collided with, walked on or hit by rays. A hole
//! counts as a height change for the chunk versions, so everything built
//! from the heights is rebuilt. A terrain without holes keeps no mask.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::Vector3;

/// The most material layers a terrain blends (its first eight
/// TerrainLayers). The renderer reads their weights from two RGBA textures.
pub const MAX_TERRAIN_LAYERS: usize = 8;

/// Cells along each side of a chunk. Neighbouring chunks share their edge
/// samples, so a chunk has `CHUNK_CELLS + 1` samples a side.
pub const CHUNK_CELLS: usize = 32;

/// The fewest and most samples along a side. 2049 × 2049 is about 4 million
/// samples: 16 MB of heights and 33 MB of paint.
pub const MIN_TERRAIN_SAMPLES: usize = 9;
pub const MAX_TERRAIN_SAMPLES: usize = 2049;

/// The most plants and props one terrain keeps. Painting stops adding
/// when this is reached; erasing still works.
pub const MAX_FOLIAGE_INSTANCES: usize = 32_768;

/// How a new Terrain starts: 257 × 257 samples, flat a quarter of the way
/// up its height range, so it can be dug into as well as raised.
pub const DEFAULT_TERRAIN_SAMPLES: usize = 257;
pub const DEFAULT_TERRAIN_HEIGHT: f32 = 0.25;

/// The hand-painted weight of each layer at one sample, 0 to 255. They add
/// up to at most 255: the rest is left to the automatic material rules, so
/// unpainted ground (all zeros) is entirely automatic.
pub type PaintWeights = [u8; MAX_TERRAIN_LAYERS];

/// Each terrain's data gets its own number, so caches can tell two
/// terrains (or a copy changed after Play started) apart.
static NEXT_UID: AtomicU64 = AtomicU64::new(1);

fn next_uid() -> u64 {
    NEXT_UID.fetch_add(1, Ordering::Relaxed)
}

#[derive(Debug, PartialEq, thiserror::Error)]
pub enum TerrainError {
    #[error(
        "a terrain must have {MIN_TERRAIN_SAMPLES} to {MAX_TERRAIN_SAMPLES} samples along each side, not {0} × {1}"
    )]
    BadSize(usize, usize),
    #[error("expected {expected} values for a {size_x} × {size_z} terrain, but got {found}")]
    WrongLength {
        size_x: usize,
        size_z: usize,
        expected: usize,
        found: usize,
    },
}

/// Samples `x0..x1` along X and `z0..z1` along Z (the ends left out).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct SampleRect {
    pub x0: usize,
    pub z0: usize,
    pub x1: usize,
    pub z1: usize,
}

impl SampleRect {
    pub fn new(x0: usize, z0: usize, x1: usize, z1: usize) -> Self {
        Self { x0, z0, x1, z1 }
    }

    pub fn is_empty(&self) -> bool {
        self.x0 >= self.x1 || self.z0 >= self.z1
    }

    pub fn width(&self) -> usize {
        self.x1.saturating_sub(self.x0)
    }

    pub fn depth(&self) -> usize {
        self.z1.saturating_sub(self.z0)
    }

    /// The smallest rectangle holding both. An empty one adds nothing.
    pub fn union(self, other: SampleRect) -> SampleRect {
        if self.is_empty() {
            return other;
        }
        if other.is_empty() {
            return self;
        }
        SampleRect {
            x0: self.x0.min(other.x0),
            z0: self.z0.min(other.z0),
            x1: self.x1.max(other.x1),
            z1: self.z1.max(other.z1),
        }
    }

    /// Grown by `by` samples on every side, then kept inside a
    /// `size_x` × `size_z` grid.
    pub fn grown(self, by: usize, size_x: usize, size_z: usize) -> SampleRect {
        SampleRect {
            x0: self.x0.saturating_sub(by),
            z0: self.z0.saturating_sub(by),
            x1: (self.x1 + by).min(size_x),
            z1: (self.z1 + by).min(size_z),
        }
    }
}

/// A built-in plant painted onto terrain.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FoliagePlant {
    Grass,
    Bush,
    Flower,
    Tree,
}

impl FoliagePlant {
    pub const ALL: [FoliagePlant; 4] = [
        FoliagePlant::Grass,
        FoliagePlant::Bush,
        FoliagePlant::Flower,
        FoliagePlant::Tree,
    ];

    pub fn label(self) -> &'static str {
        match self {
            FoliagePlant::Grass => "Grass",
            FoliagePlant::Bush => "Bush",
            FoliagePlant::Flower => "Flower",
            FoliagePlant::Tree => "Tree",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|plant| plant.label() == name)
    }
}

/// A built-in prop painted onto terrain (the same names as the Assets panel).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FoliageProp {
    Floor,
    Wall,
    Stairs,
    Ramp,
    Lamp,
    Table,
    Tree,
    Crate,
    /// A boulder, for scatter rules (not in the Assets panel).
    Rock,
    /// A rock face for steep slopes, for scatter rules (not in the Assets
    /// panel).
    Cliff,
}

impl FoliageProp {
    pub const ALL: [FoliageProp; 10] = [
        FoliageProp::Floor,
        FoliageProp::Wall,
        FoliageProp::Stairs,
        FoliageProp::Ramp,
        FoliageProp::Lamp,
        FoliageProp::Table,
        FoliageProp::Tree,
        FoliageProp::Crate,
        FoliageProp::Rock,
        FoliageProp::Cliff,
    ];

    pub fn label(self) -> &'static str {
        match self {
            FoliageProp::Floor => "Floor",
            FoliageProp::Wall => "Wall",
            FoliageProp::Stairs => "Stairs",
            FoliageProp::Ramp => "Ramp",
            FoliageProp::Lamp => "Lamp",
            FoliageProp::Table => "Table",
            FoliageProp::Tree => "Tree",
            FoliageProp::Crate => "Crate",
            FoliageProp::Rock => "Rock",
            FoliageProp::Cliff => "Cliff",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|prop| prop.label() == name)
    }

    /// Axis-aligned box around this prop: size, then the box's center in
    /// instance space (origin on the ground, Y up).
    pub fn bounds(self) -> (Vector3, Vector3) {
        match self {
            FoliageProp::Floor => (Vector3::new(16.0, 1.0, 16.0), Vector3::new(0.0, 0.5, 0.0)),
            FoliageProp::Wall => (Vector3::new(16.0, 8.0, 1.0), Vector3::new(0.0, 4.0, 0.0)),
            FoliageProp::Stairs => (Vector3::new(4.0, 6.0, 9.0), Vector3::new(0.0, 3.0, -3.75)),
            FoliageProp::Ramp => (Vector3::new(4.0, 4.0, 8.0), Vector3::new(0.0, 2.0, 0.0)),
            FoliageProp::Lamp => (Vector3::new(1.6, 9.5, 1.6), Vector3::new(0.0, 4.75, 0.0)),
            FoliageProp::Table => (Vector3::new(6.0, 3.4, 4.0), Vector3::new(0.0, 1.7, 0.0)),
            FoliageProp::Tree => (Vector3::new(6.0, 10.5, 6.0), Vector3::new(0.0, 5.25, 0.0)),
            FoliageProp::Crate => (Vector3::new(3.0, 3.0, 3.0), Vector3::new(0.0, 1.5, 0.0)),
            FoliageProp::Rock => (Vector3::new(5.0, 3.6, 4.4), Vector3::new(0.0, 1.8, 0.0)),
            FoliageProp::Cliff => (Vector3::new(10.0, 12.0, 5.0), Vector3::new(0.0, 6.0, 0.0)),
        }
    }
}

impl FoliagePlant {
    /// Axis-aligned box around this plant: size, then the box's center in
    /// instance space (origin on the ground, Y up).
    pub fn bounds(self) -> (Vector3, Vector3) {
        match self {
            FoliagePlant::Grass => (Vector3::new(0.55, 1.1, 0.55), Vector3::new(0.0, 0.55, 0.0)),
            FoliagePlant::Bush => (Vector3::new(2.2, 2.2, 2.2), Vector3::new(0.0, 1.0, 0.0)),
            FoliagePlant::Flower => (
                Vector3::new(0.7, 1.075, 0.7),
                Vector3::new(0.0, 0.5375, 0.0),
            ),
            FoliagePlant::Tree => (Vector3::new(6.0, 10.5, 6.0), Vector3::new(0.0, 5.25, 0.0)),
        }
    }
}

/// Which plant, prop, or project model a painted instance is.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum FoliageSource {
    /// A built-in plant. Older paints still load; new strokes pick from Assets.
    Plant(FoliagePlant),
    /// A built-in Assets-panel prop.
    Prop(FoliageProp),
    /// A model file in the project (`assets/models/…`), as a MeshId.
    Model(String),
}

impl FoliageSource {
    /// The file kind written to `foliage.json`.
    pub fn kind_key(&self) -> &'static str {
        match self {
            FoliageSource::Plant(_) => "plant",
            FoliageSource::Prop(_) => "prop",
            FoliageSource::Model(_) => "model",
        }
    }

    pub fn name(&self) -> &str {
        match self {
            FoliageSource::Plant(plant) => plant.label(),
            FoliageSource::Prop(prop) => prop.label(),
            FoliageSource::Model(path) => path.rsplit('/').next().unwrap_or(path),
        }
    }

    /// Size and center of a built-in plant or prop, or `None` for a model
    /// (those use the file's hull).
    pub fn bounds(&self) -> Option<(Vector3, Vector3)> {
        match self {
            FoliageSource::Plant(plant) => Some(plant.bounds()),
            FoliageSource::Prop(prop) => Some(prop.bounds()),
            FoliageSource::Model(_) => None,
        }
    }
}

/// One painted plant, prop, or model. `x` and `z` are sample coordinates, so
/// the instance stays on the surface when the heights change. `collide` is
/// whether physics treats it as solid. `lean` (0 upright, 1 square to the
/// ground) tilts it with the slope and `sink` (studs) lowers it into the
/// ground; scatter rules set them (procedural worlds PW5), painted foliage
/// stands upright on the surface.
#[derive(Clone, Debug, PartialEq)]
pub struct FoliageInstance {
    pub source: FoliageSource,
    pub x: f32,
    pub z: f32,
    pub yaw: f32,
    pub scale: f32,
    pub collide: bool,
    pub lean: f32,
    pub sink: f32,
}

impl FoliageInstance {
    /// The way it stands (one unit long, pointing up): straight up, leaned
    /// `lean` of the way to the ground's normal under it. `cell` is studs
    /// between samples along X and Z and `height` the terrain's height
    /// range in studs.
    pub fn up(&self, data: &TerrainData, cell: (f32, f32), height: f32) -> Vector3 {
        let lean = self.lean.clamp(0.0, 1.0);
        if lean <= 0.0 {
            return Vector3::new(0.0, 1.0, 0.0);
        }
        let h = |x: f32, z: f32| data.height_at(x, z) * height;
        let dx = (h(self.x + 1.0, self.z) - h(self.x - 1.0, self.z)) / (2.0 * cell.0.max(1e-6));
        let dz = (h(self.x, self.z + 1.0) - h(self.x, self.z - 1.0)) / (2.0 * cell.1.max(1e-6));
        let normal = Vector3::new(-dx, 1.0, -dz);
        let length = (normal.x * normal.x + 1.0 + normal.z * normal.z).sqrt();
        let (nx, ny, nz) = (normal.x / length, 1.0 / length, normal.z / length);
        let (x, y, z) = (nx * lean, 1.0 - lean + ny * lean, nz * lean);
        let length = (x * x + y * y + z * z).sqrt().max(1e-6);
        Vector3::new(x / length, y / length, z / length)
    }

    /// True when this instance's sample sits inside `rect` (end excluded).
    pub fn in_rect(&self, rect: SampleRect) -> bool {
        self.x >= rect.x0 as f32
            && self.x < rect.x1 as f32
            && self.z >= rect.z0 as f32
            && self.z < rect.z1 as f32
    }
}

/// A copy of part of a terrain: its heights, paint, and foliage, for undo.
#[derive(Clone, Debug, PartialEq)]
pub struct TerrainRegion {
    pub rect: SampleRect,
    pub heights: Vec<f32>,
    pub paint: Vec<PaintWeights>,
    pub foliage: Vec<FoliageInstance>,
    /// The hole flags, row by row; empty when there are none in it.
    pub holes: Vec<bool>,
}

/// What a terrain's scatter rules placed in one chunk, and the key of the
/// inputs it was made from (0: never made). Shared, so copying a terrain
/// doesn't copy every generated plant.
#[derive(Clone, Debug, Default)]
struct GeneratedChunk {
    key: u64,
    instances: Arc<[FoliageInstance]>,
}

/// One Terrain's heightfield and paint. See the module docs.
#[derive(Debug)]
pub struct TerrainData {
    uid: u64,
    size_x: usize,
    size_z: usize,
    heights: Vec<f32>,
    paint: Vec<PaintWeights>,
    foliage: Vec<FoliageInstance>,
    /// One flag a sample, row by row: the cell from it to +X and +Z is cut
    /// out. Empty while the terrain has never had a hole.
    holes: Vec<bool>,
    /// Per chunk, row by row: bumped each time its heights or paint change.
    height_versions: Vec<u64>,
    paint_versions: Vec<u64>,
    /// Per chunk, row by row: what the scatter rules placed there.
    generated: Vec<GeneratedChunk>,
    /// Bumped each time generated foliage changes (it isn't saved, so it
    /// doesn't bump `revision`).
    generated_version: u64,
    /// Bumped by every change, to tell whether the terrain needs saving.
    revision: u64,
    /// The generator settings (JSON) of the last generated landscape, saved
    /// with the terrain so the result can be made again.
    pub generation: Option<String>,
}

/// A copy is a different terrain from then on, with a number of its own.
impl Clone for TerrainData {
    fn clone(&self) -> Self {
        Self {
            uid: next_uid(),
            size_x: self.size_x,
            size_z: self.size_z,
            heights: self.heights.clone(),
            paint: self.paint.clone(),
            foliage: self.foliage.clone(),
            holes: self.holes.clone(),
            height_versions: self.height_versions.clone(),
            paint_versions: self.paint_versions.clone(),
            generated: self.generated.clone(),
            generated_version: self.generated_version,
            revision: self.revision,
            generation: self.generation.clone(),
        }
    }
}

impl Default for TerrainData {
    fn default() -> Self {
        Self::flat(
            DEFAULT_TERRAIN_SAMPLES,
            DEFAULT_TERRAIN_SAMPLES,
            DEFAULT_TERRAIN_HEIGHT,
        )
        .expect("the default size is allowed")
    }
}

/// True if `size_x` × `size_z` samples is an allowed terrain size.
pub fn valid_terrain_size(size_x: usize, size_z: usize) -> bool {
    let allowed = MIN_TERRAIN_SAMPLES..=MAX_TERRAIN_SAMPLES;
    allowed.contains(&size_x) && allowed.contains(&size_z)
}

impl TerrainData {
    /// A flat terrain at `height` (0 to 1) with nothing painted.
    pub fn flat(size_x: usize, size_z: usize, height: f32) -> Result<Self, TerrainError> {
        if !valid_terrain_size(size_x, size_z) {
            return Err(TerrainError::BadSize(size_x, size_z));
        }
        let count = size_x * size_z;
        Self::from_parts(
            size_x,
            size_z,
            vec![height.clamp(0.0, 1.0); count],
            vec![[0; MAX_TERRAIN_LAYERS]; count],
        )
    }

    /// A terrain with these heights and paint, row by row. Heights are kept
    /// between 0 and 1 (NaN becomes 0).
    pub fn from_parts(
        size_x: usize,
        size_z: usize,
        mut heights: Vec<f32>,
        paint: Vec<PaintWeights>,
    ) -> Result<Self, TerrainError> {
        if !valid_terrain_size(size_x, size_z) {
            return Err(TerrainError::BadSize(size_x, size_z));
        }
        let expected = size_x * size_z;
        for found in [heights.len(), paint.len()] {
            if found != expected {
                return Err(TerrainError::WrongLength {
                    size_x,
                    size_z,
                    expected,
                    found,
                });
            }
        }
        for height in &mut heights {
            *height = clean_height(*height);
        }
        let chunks = chunk_count(size_x) * chunk_count(size_z);
        Ok(Self {
            uid: next_uid(),
            size_x,
            size_z,
            heights,
            paint,
            foliage: Vec::new(),
            holes: Vec::new(),
            height_versions: vec![1; chunks],
            paint_versions: vec![1; chunks],
            generated: vec![GeneratedChunk::default(); chunks],
            generated_version: 0,
            revision: 0,
            generation: None,
        })
    }

    /// This terrain's number: no other terrain data has had it.
    pub fn uid(&self) -> u64 {
        self.uid
    }

    /// Samples along X and along Z.
    pub fn size(&self) -> (usize, usize) {
        (self.size_x, self.size_z)
    }

    /// The whole grid.
    pub fn bounds(&self) -> SampleRect {
        SampleRect::new(0, 0, self.size_x, self.size_z)
    }

    pub fn index(&self, x: usize, z: usize) -> usize {
        z * self.size_x + x
    }

    pub fn heights(&self) -> &[f32] {
        &self.heights
    }

    pub fn paint(&self) -> &[PaintWeights] {
        &self.paint
    }

    pub fn foliage(&self) -> &[FoliageInstance] {
        &self.foliage
    }

    /// The foliage list, for adding or removing instances. Bumps the
    /// revision so the terrain is saved; it does not rebuild chunk meshes.
    pub fn foliage_mut(&mut self) -> &mut Vec<FoliageInstance> {
        self.revision += 1;
        &mut self.foliage
    }

    /// What the scatter rules placed, chunk by chunk (never saved).
    pub fn generated_foliage(&self) -> impl Iterator<Item = &FoliageInstance> {
        self.generated
            .iter()
            .flat_map(|chunk| chunk.instances.iter())
    }

    /// What the scatter rules placed, one shared list per chunk (row by
    /// row), for whatever keeps work per chunk: a list that's the same
    /// `Arc` as before hasn't changed.
    pub fn generated_chunks(&self) -> impl Iterator<Item = &Arc<[FoliageInstance]>> {
        self.generated.iter().map(|chunk| &chunk.instances)
    }

    /// How many plants, props and models the scatter rules placed.
    pub fn generated_count(&self) -> usize {
        self.generated
            .iter()
            .map(|chunk| chunk.instances.len())
            .sum()
    }

    /// The painted foliage, then the generated: everything drawn and
    /// collided on this terrain.
    pub fn all_foliage(&self) -> impl Iterator<Item = &FoliageInstance> {
        self.foliage.iter().chain(self.generated_foliage())
    }

    /// The key of the inputs chunk (`cx`, `cz`)'s generated foliage was
    /// made from, or 0 if it never was.
    pub fn generated_key(&self, cx: usize, cz: usize) -> u64 {
        let (chunks_x, _) = self.chunk_counts();
        self.generated[cz * chunks_x + cx].key
    }

    /// What chunk (`cx`, `cz`)'s generated foliage is, made from inputs
    /// with this `key`. Doesn't count as an edit (it isn't saved).
    pub fn set_generated(
        &mut self,
        cx: usize,
        cz: usize,
        key: u64,
        instances: Vec<FoliageInstance>,
    ) {
        let (chunks_x, _) = self.chunk_counts();
        self.generated[cz * chunks_x + cx] = GeneratedChunk {
            key,
            instances: instances.into(),
        };
        self.generated_version += 1;
    }

    /// Forgets all generated foliage (it's made again when next asked).
    pub fn clear_generated(&mut self) {
        if self.generated.iter().any(|chunk| chunk.key != 0) {
            self.generated.fill(GeneratedChunk::default());
            self.generated_version += 1;
        }
    }

    /// Changes each time generated foliage does.
    pub fn generated_version(&self) -> u64 {
        self.generated_version
    }

    /// Puts loaded foliage on this terrain without counting as an edit.
    pub fn load_foliage(&mut self, foliage: Vec<FoliageInstance>) {
        self.foliage = foliage;
    }

    /// The height at sample (`x`, `z`), with coordinates past the edges
    /// moved onto the nearest edge.
    pub fn height(&self, x: isize, z: isize) -> f32 {
        let x = x.clamp(0, self.size_x as isize - 1) as usize;
        let z = z.clamp(0, self.size_z as isize - 1) as usize;
        self.heights[z * self.size_x + x]
    }

    /// The height between samples, blended from the four around (`x`, `z`),
    /// in samples. Matches the drawn triangles only at the samples.
    pub fn height_at(&self, x: f32, z: f32) -> f32 {
        let x = x.clamp(0.0, (self.size_x - 1) as f32);
        let z = z.clamp(0.0, (self.size_z - 1) as f32);
        let (x0, z0) = (x.floor(), z.floor());
        let (tx, tz) = (x - x0, z - z0);
        let (x0, z0) = (x0 as isize, z0 as isize);
        let top = lerp(self.height(x0, z0), self.height(x0 + 1, z0), tx);
        let bottom = lerp(self.height(x0, z0 + 1), self.height(x0 + 1, z0 + 1), tx);
        lerp(top, bottom, tz)
    }

    /// The heights, for changing samples inside `rect`. The chunks there
    /// (and those next to it, whose shading depends on their neighbours)
    /// count as changed.
    pub fn heights_mut_in(&mut self, rect: SampleRect) -> &mut [f32] {
        self.touch(rect, true, false);
        &mut self.heights
    }

    /// The paint, for changing samples inside `rect`.
    pub fn paint_mut_in(&mut self, rect: SampleRect) -> &mut [PaintWeights] {
        self.touch(rect, false, true);
        &mut self.paint
    }

    /// Both at once, for changing samples inside `rect`.
    pub fn both_mut_in(&mut self, rect: SampleRect) -> (&mut [f32], &mut [PaintWeights]) {
        self.touch(rect, true, true);
        (&mut self.heights, &mut self.paint)
    }

    /// The hole flags, one a sample row by row (see the module docs), or
    /// empty if the terrain has never had a hole.
    pub fn holes(&self) -> &[bool] {
        &self.holes
    }

    /// Whether any cell is cut out.
    pub fn has_holes(&self) -> bool {
        self.holes.iter().any(|&hole| hole)
    }

    /// Whether cell (`x`, `z`) (from sample (`x`, `z`) to +X and +Z) is
    /// cut out. Off the grid is never a hole.
    pub fn is_hole(&self, x: usize, z: usize) -> bool {
        x < self.size_x && z < self.size_z && self.holes.get(z * self.size_x + x) == Some(&true)
    }

    /// Whether the point (`x`, `z`), in samples, is over a hole.
    pub fn hole_at(&self, x: f32, z: f32) -> bool {
        if self.holes.is_empty() || !(x >= 0.0 && z >= 0.0) {
            return false;
        }
        let (cx, cz) = (x.floor() as usize, z.floor() as usize);
        // A point on the far edge belongs to the last cell.
        let cx = cx.min(self.size_x.saturating_sub(2));
        let cz = cz.min(self.size_z.saturating_sub(2));
        self.is_hole(cx, cz)
    }

    /// Whether chunk (`cx`, `cz`) has a hole in it.
    pub fn chunk_has_holes(&self, cx: usize, cz: usize) -> bool {
        if self.holes.is_empty() {
            return false;
        }
        let rect = self.chunk_rect(cx, cz);
        (rect.z0..rect.z1).any(|z| {
            let row = z * self.size_x;
            self.holes[row + rect.x0..row + rect.x1]
                .iter()
                .any(|&hole| hole)
        })
    }

    /// The hole flags, for changing samples inside `rect` (made if the
    /// terrain had none). The chunks there count as changed heights.
    pub fn holes_mut_in(&mut self, rect: SampleRect) -> &mut [bool] {
        self.touch(rect, true, false);
        if self.holes.is_empty() {
            self.holes = vec![false; self.size_x * self.size_z];
        }
        &mut self.holes
    }

    /// Sets every hole flag at once, as loaded from a file. `holes` must
    /// have one flag a sample, or be empty for none; anything else is
    /// ignored and `false` returned.
    pub fn load_holes(&mut self, holes: Vec<bool>) -> bool {
        if !holes.is_empty() && holes.len() != self.size_x * self.size_z {
            return false;
        }
        let all = self.bounds();
        self.touch(all, true, false);
        self.holes = if holes.iter().any(|&hole| hole) {
            holes
        } else {
            Vec::new()
        };
        true
    }

    /// Puts heights between 0 and 1 again after they were changed in `rect`.
    pub fn clamp_heights_in(&mut self, rect: SampleRect) {
        let rect = self.clip(rect);
        for z in rect.z0..rect.z1 {
            for x in rect.x0..rect.x1 {
                let index = z * self.size_x + x;
                self.heights[index] = clean_height(self.heights[index]);
            }
        }
    }

    /// A copy of the samples in `rect` (clipped to the grid).
    pub fn read_region(&self, rect: SampleRect) -> TerrainRegion {
        let rect = self.clip(rect);
        let mut heights = Vec::with_capacity(rect.width() * rect.depth());
        let mut paint = Vec::with_capacity(rect.width() * rect.depth());
        let mut holes = Vec::new();
        for z in rect.z0..rect.z1 {
            let row = z * self.size_x;
            heights.extend_from_slice(&self.heights[row + rect.x0..row + rect.x1]);
            paint.extend_from_slice(&self.paint[row + rect.x0..row + rect.x1]);
            if !self.holes.is_empty() {
                holes.extend_from_slice(&self.holes[row + rect.x0..row + rect.x1]);
            }
        }
        if !holes.iter().any(|&hole| hole) {
            holes.clear();
        }
        let foliage = self
            .foliage
            .iter()
            .filter(|instance| instance.in_rect(rect))
            .cloned()
            .collect();
        TerrainRegion {
            rect,
            heights,
            paint,
            foliage,
            holes,
        }
    }

    /// Writes a copy from [`read_region`](Self::read_region) back. A region
    /// that doesn't fit this terrain (it was resized since) is ignored, and
    /// `false` returned.
    pub fn write_region(&mut self, region: &TerrainRegion) -> bool {
        let rect = region.rect;
        let count = rect.width() * rect.depth();
        let fits = rect.x1 <= self.size_x
            && rect.z1 <= self.size_z
            && region.heights.len() == count
            && region.paint.len() == count
            && (region.holes.is_empty() || region.holes.len() == count);
        if !fits {
            return false;
        }
        self.touch(rect, true, true);
        let width = rect.width();
        for (row, z) in (rect.z0..rect.z1).enumerate() {
            let at = z * self.size_x + rect.x0;
            let from = row * width;
            self.heights[at..at + width].copy_from_slice(&region.heights[from..from + width]);
            self.paint[at..at + width].copy_from_slice(&region.paint[from..from + width]);
        }
        if !region.holes.is_empty() && self.holes.is_empty() {
            self.holes = vec![false; self.size_x * self.size_z];
        }
        if !self.holes.is_empty() {
            for (row, z) in (rect.z0..rect.z1).enumerate() {
                let at = z * self.size_x + rect.x0;
                let cut = &mut self.holes[at..at + width];
                match region.holes.get(row * width..(row + 1) * width) {
                    Some(from) => cut.copy_from_slice(from),
                    None => cut.fill(false),
                }
            }
        }
        self.foliage.retain(|instance| !instance.in_rect(rect));
        self.foliage.extend(region.foliage.iter().cloned());
        true
    }

    /// Replaces every height and all the paint with another terrain's, of
    /// any size. Used to apply a generated or eroded landscape and to undo
    /// one, so this terrain keeps its number.
    pub fn replace_with(&mut self, other: &TerrainData) {
        self.size_x = other.size_x;
        self.size_z = other.size_z;
        self.heights.clone_from(&other.heights);
        self.paint.clone_from(&other.paint);
        self.holes.clone_from(&other.holes);
        self.foliage.clone_from(&other.foliage);
        self.generation.clone_from(&other.generation);
        let chunks = chunk_count(self.size_x) * chunk_count(self.size_z);
        let next = self.newest_version() + 1;
        self.height_versions = vec![next; chunks];
        self.paint_versions = vec![next; chunks];
        self.generated = vec![GeneratedChunk::default(); chunks];
        self.generated_version += 1;
        self.revision += 1;
    }

    /// The same landscape with `size_x` × `size_z` samples, blended from
    /// the old ones (bilinear). Paint takes the nearest old sample's.
    pub fn resampled(&self, size_x: usize, size_z: usize) -> Result<TerrainData, TerrainError> {
        if !valid_terrain_size(size_x, size_z) {
            return Err(TerrainError::BadSize(size_x, size_z));
        }
        let scale_x = (self.size_x - 1) as f32 / (size_x - 1) as f32;
        let scale_z = (self.size_z - 1) as f32 / (size_z - 1) as f32;
        let mut heights = Vec::with_capacity(size_x * size_z);
        let mut paint = Vec::with_capacity(size_x * size_z);
        let mut holes = Vec::new();
        for z in 0..size_z {
            for x in 0..size_x {
                let (fx, fz) = (x as f32 * scale_x, z as f32 * scale_z);
                heights.push(self.height_at(fx, fz));
                let near = self.index(
                    (fx.round() as usize).min(self.size_x - 1),
                    (fz.round() as usize).min(self.size_z - 1),
                );
                paint.push(self.paint[near]);
                if !self.holes.is_empty() {
                    // The cell's middle, in the old grid.
                    holes.push(self.hole_at(fx + scale_x / 2.0, fz + scale_z / 2.0));
                }
            }
        }
        let mut data = TerrainData::from_parts(size_x, size_z, heights, paint)?;
        data.load_holes(holes);
        data.generation.clone_from(&self.generation);
        let to_new_x = (size_x - 1) as f32 / (self.size_x - 1) as f32;
        let to_new_z = (size_z - 1) as f32 / (self.size_z - 1) as f32;
        data.foliage = self
            .foliage
            .iter()
            .map(|instance| FoliageInstance {
                source: instance.source.clone(),
                x: instance.x * to_new_x,
                z: instance.z * to_new_z,
                yaw: instance.yaw,
                scale: instance.scale,
                collide: instance.collide,
                lean: 0.0,
                sink: 0.0,
            })
            .collect();
        Ok(data)
    }

    /// Bumped by every change.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Chunks along X and along Z.
    pub fn chunk_counts(&self) -> (usize, usize) {
        (chunk_count(self.size_x), chunk_count(self.size_z))
    }

    /// The samples of chunk (`cx`, `cz`), including the edges it shares.
    pub fn chunk_rect(&self, cx: usize, cz: usize) -> SampleRect {
        let x0 = cx * CHUNK_CELLS;
        let z0 = cz * CHUNK_CELLS;
        SampleRect::new(
            x0,
            z0,
            (x0 + CHUNK_CELLS + 1).min(self.size_x),
            (z0 + CHUNK_CELLS + 1).min(self.size_z),
        )
    }

    /// Changes each time the heights in or next to chunk (`cx`, `cz`) do.
    pub fn chunk_height_version(&self, cx: usize, cz: usize) -> u64 {
        let (chunks_x, _) = self.chunk_counts();
        self.height_versions[cz * chunks_x + cx]
    }

    /// Changes each time the paint in chunk (`cx`, `cz`) does.
    pub fn chunk_paint_version(&self, cx: usize, cz: usize) -> u64 {
        let (chunks_x, _) = self.chunk_counts();
        self.paint_versions[cz * chunks_x + cx]
    }

    fn clip(&self, rect: SampleRect) -> SampleRect {
        SampleRect {
            x0: rect.x0.min(self.size_x),
            z0: rect.z0.min(self.size_z),
            x1: rect.x1.min(self.size_x),
            z1: rect.z1.min(self.size_z),
        }
    }

    fn newest_version(&self) -> u64 {
        let newest = |versions: &[u64]| versions.iter().copied().max().unwrap_or(0);
        newest(&self.height_versions).max(newest(&self.paint_versions))
    }

    /// Marks the chunks holding `rect` as changed. Heights also change the
    /// shading (normals and slopes) one sample around, so those chunks are
    /// marked too.
    fn touch(&mut self, rect: SampleRect, heights: bool, paint: bool) {
        if rect.is_empty() {
            return;
        }
        self.revision += 1;
        let rect = if heights {
            rect.grown(1, self.size_x, self.size_z)
        } else {
            self.clip(rect)
        };
        if rect.is_empty() {
            return;
        }
        let (chunks_x, chunks_z) = self.chunk_counts();
        // A sample on a chunk's edge belongs to the chunks on both sides.
        let first = |sample: usize| sample.saturating_sub(1) / CHUNK_CELLS;
        let last = |sample: usize, chunks: usize| ((sample - 1) / CHUNK_CELLS).min(chunks - 1);
        let next = self.newest_version() + 1;
        for cz in first(rect.z0)..=last(rect.z1, chunks_z) {
            for cx in first(rect.x0)..=last(rect.x1, chunks_x) {
                let chunk = cz * chunks_x + cx;
                if heights {
                    self.height_versions[chunk] = next;
                }
                if paint {
                    self.paint_versions[chunk] = next;
                }
            }
        }
    }
}

/// How many chunks cover `samples` samples (one fewer cells).
fn chunk_count(samples: usize) -> usize {
    (samples - 1).div_ceil(CHUNK_CELLS).max(1)
}

fn clean_height(height: f32) -> f32 {
    if height.is_nan() {
        0.0
    } else {
        height.clamp(0.0, 1.0)
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_outside_the_limits_are_refused() {
        assert_eq!(
            TerrainData::flat(4, 100, 0.0).unwrap_err(),
            TerrainError::BadSize(4, 100)
        );
        assert!(TerrainData::flat(MAX_TERRAIN_SAMPLES + 1, 9, 0.0).is_err());
        let wrong = TerrainData::from_parts(9, 9, vec![0.0; 80], vec![[0; 8]; 81]);
        assert!(matches!(wrong, Err(TerrainError::WrongLength { .. })));
    }

    #[test]
    fn samples_are_indexed_row_by_row_and_heights_stay_between_0_and_1() {
        let mut heights = vec![0.5; 9 * 10];
        heights[2 * 9 + 3] = 7.0;
        heights[0] = f32::NAN;
        let data = TerrainData::from_parts(9, 10, heights, vec![[0; 8]; 90]).unwrap();
        assert_eq!(data.size(), (9, 10));
        assert_eq!(data.index(3, 2), 21);
        assert_eq!(data.height(3, 2), 1.0);
        assert_eq!(data.height(0, 0), 0.0);
        // Past the edge is the edge.
        assert_eq!(data.height(-5, 2), data.height(0, 2));
        assert_eq!(data.height(100, 100), data.height(8, 9));
    }

    #[test]
    fn heights_between_samples_blend() {
        let mut data = TerrainData::flat(9, 9, 0.0).unwrap();
        let all = data.bounds();
        data.heights_mut_in(all)[1] = 1.0;
        assert_eq!(data.height_at(0.5, 0.0), 0.5);
        assert_eq!(data.height_at(1.0, 0.0), 1.0);
        assert_eq!(data.height_at(1.0, 0.5), 0.5);
    }

    #[test]
    fn edits_mark_only_the_chunks_they_touch_and_their_neighbours_edges() {
        // 3 × 3 chunks.
        let size = CHUNK_CELLS * 3 + 1;
        let mut data = TerrainData::flat(size, size, 0.0).unwrap();
        let versions = |data: &TerrainData| {
            let mut all = Vec::new();
            for cz in 0..3 {
                for cx in 0..3 {
                    all.push(data.chunk_height_version(cx, cz));
                }
            }
            all
        };
        let before = versions(&data);
        // The middle of the middle chunk.
        let middle = CHUNK_CELLS + CHUNK_CELLS / 2;
        data.heights_mut_in(SampleRect::new(middle, middle, middle + 1, middle + 1));
        let after = versions(&data);
        let changed: Vec<usize> = (0..9).filter(|&i| before[i] != after[i]).collect();
        assert_eq!(changed, vec![4]);

        // The sample shared by the four chunks at the top left's corner
        // changes all four.
        let corner = CHUNK_CELLS;
        let before = versions(&data);
        data.heights_mut_in(SampleRect::new(corner, corner, corner + 1, corner + 1));
        let after = versions(&data);
        let changed: Vec<usize> = (0..9).filter(|&i| before[i] != after[i]).collect();
        assert_eq!(changed, vec![0, 1, 3, 4]);

        // Paint doesn't change the mesh.
        let before = versions(&data);
        data.paint_mut_in(data.bounds());
        assert_eq!(versions(&data), before);
    }

    #[test]
    fn holes_are_cells_that_regions_undo_and_resizing_keeps() {
        let mut data = TerrainData::flat(65, 65, 0.5).unwrap();
        assert!(!data.has_holes() && data.holes().is_empty());
        let before = data.read_region(SampleRect::new(30, 30, 40, 40));
        assert!(before.holes.is_empty());
        let version = data.chunk_height_version(1, 1);
        let rect = SampleRect::new(33, 33, 36, 35);
        let (size_x, _) = data.size();
        let holes = data.holes_mut_in(rect);
        for z in 33..35 {
            for x in 33..36 {
                holes[z * size_x + x] = true;
            }
        }
        assert!(data.chunk_height_version(1, 1) > version);
        assert!(data.has_holes());
        assert!(data.is_hole(33, 33) && data.is_hole(35, 34));
        assert!(!data.is_hole(36, 34) && !data.is_hole(33, 35));
        // A point inside cell (33, 33) is over the hole; one in (32, 33) isn't.
        assert!(data.hole_at(33.5, 33.9));
        assert!(!data.hole_at(32.9, 33.5));
        assert!(data.chunk_has_holes(1, 1) && !data.chunk_has_holes(0, 0));
        // Undo: the region from before has none, so they go.
        let after = data.read_region(SampleRect::new(30, 30, 40, 40));
        assert_eq!(after.holes.iter().filter(|&&h| h).count(), 6);
        assert!(data.write_region(&before));
        assert!(!data.has_holes());
        assert!(data.write_region(&after));
        assert!(data.is_hole(34, 34));
        // Half the samples a side: the holes land in the same place.
        let small = data.resampled(33, 33).unwrap();
        assert!(small.hole_at(16.8, 16.8));
        assert!(!small.hole_at(10.0, 10.0));
        // A copy and a replace keep them.
        assert!(data.clone().is_hole(34, 34));
        let mut other = TerrainData::flat(65, 65, 0.0).unwrap();
        other.replace_with(&data);
        assert!(other.is_hole(34, 34));
        assert!(!other.load_holes(vec![true; 3]));
        assert!(other.load_holes(Vec::new()));
        assert!(!other.has_holes());
    }

    #[test]
    fn a_region_reads_and_writes_back() {
        let mut data = TerrainData::flat(17, 17, 0.2).unwrap();
        let rect = SampleRect::new(2, 3, 6, 5);
        let saved = data.read_region(rect);
        assert_eq!(saved.heights.len(), 8);
        let all = data.bounds();
        data.heights_mut_in(all).fill(0.9);
        data.paint_mut_in(all).fill([255, 0, 0, 0, 0, 0, 0, 0]);
        assert!(data.write_region(&saved));
        assert_eq!(data.height(2, 3), 0.2);
        assert_eq!(data.height(6, 3), 0.9);
        assert_eq!(data.paint()[data.index(5, 4)], [0; 8]);

        let mut small = TerrainData::flat(9, 9, 0.0).unwrap();
        assert!(!small.write_region(&TerrainData::flat(17, 17, 0.0).unwrap().read_region(all)));
    }

    #[test]
    fn a_copy_is_a_different_terrain() {
        let data = TerrainData::default();
        let copy = data.clone();
        assert_ne!(data.uid(), copy.uid());
        assert_eq!(data.heights(), copy.heights());
    }

    #[test]
    fn resampling_keeps_the_shape() {
        let mut data = TerrainData::flat(9, 9, 0.0).unwrap();
        let all = data.bounds();
        for (i, height) in data.heights_mut_in(all).iter_mut().enumerate() {
            *height = (i % 9) as f32 / 8.0;
        }
        let bigger = data.resampled(17, 17).unwrap();
        assert_eq!(bigger.size(), (17, 17));
        assert!((bigger.height(16, 0) - 1.0).abs() < 1e-6);
        assert!((bigger.height(8, 5) - 0.5).abs() < 1e-6);
        assert!(data.resampled(3, 3).is_err());
    }

    fn grass_at(x: f32, z: f32) -> FoliageInstance {
        FoliageInstance {
            source: FoliageSource::Plant(FoliagePlant::Grass),
            x,
            z,
            yaw: 0.5,
            scale: 1.0,
            collide: false,
            lean: 0.0,
            sink: 0.0,
        }
    }

    #[test]
    fn a_region_reads_and_writes_foliage_with_the_heights() {
        let mut data = TerrainData::flat(17, 17, 0.2).unwrap();
        data.foliage_mut()
            .extend([grass_at(2.5, 3.2), grass_at(10.0, 10.0), grass_at(5.0, 4.0)]);
        let rect = SampleRect::new(2, 3, 6, 5);
        let saved = data.read_region(rect);
        assert_eq!(saved.foliage.len(), 2);
        data.foliage_mut().clear();
        data.foliage_mut().push(grass_at(3.0, 3.5));
        assert!(data.write_region(&saved));
        assert_eq!(data.foliage().len(), 2);
        assert!(data.foliage().iter().any(|i| (i.x - 2.5).abs() < 1e-6));
        assert!(data.foliage().iter().any(|i| (i.x - 5.0).abs() < 1e-6));
        assert!(!data.foliage().iter().any(|i| (i.x - 3.0).abs() < 1e-6));
        // Outside the rect was left alone, then cleared above — the far
        // plant was outside and stays gone after the write.
        assert!(!data.foliage().iter().any(|i| (i.x - 10.0).abs() < 1e-6));
    }

    #[test]
    fn resampling_keeps_foliage_on_the_same_spot() {
        let mut data = TerrainData::flat(9, 9, 0.0).unwrap();
        data.foliage_mut().push(grass_at(8.0, 0.0));
        let bigger = data.resampled(17, 17).unwrap();
        assert_eq!(bigger.foliage().len(), 1);
        assert!((bigger.foliage()[0].x - 16.0).abs() < 1e-5);
        assert!((bigger.foliage()[0].z).abs() < 1e-5);
    }
}
