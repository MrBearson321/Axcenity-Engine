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
}

impl FoliageProp {
    pub const ALL: [FoliageProp; 8] = [
        FoliageProp::Floor,
        FoliageProp::Wall,
        FoliageProp::Stairs,
        FoliageProp::Ramp,
        FoliageProp::Lamp,
        FoliageProp::Table,
        FoliageProp::Tree,
        FoliageProp::Crate,
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
/// whether physics treats it as solid.
#[derive(Clone, Debug, PartialEq)]
pub struct FoliageInstance {
    pub source: FoliageSource,
    pub x: f32,
    pub z: f32,
    pub yaw: f32,
    pub scale: f32,
    pub collide: bool,
}

impl FoliageInstance {
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
    /// Per chunk, row by row: bumped each time its heights or paint change.
    height_versions: Vec<u64>,
    paint_versions: Vec<u64>,
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
            height_versions: self.height_versions.clone(),
            paint_versions: self.paint_versions.clone(),
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
            height_versions: vec![1; chunks],
            paint_versions: vec![1; chunks],
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
        for z in rect.z0..rect.z1 {
            let row = z * self.size_x;
            heights.extend_from_slice(&self.heights[row + rect.x0..row + rect.x1]);
            paint.extend_from_slice(&self.paint[row + rect.x0..row + rect.x1]);
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
            && region.paint.len() == count;
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
        self.foliage.clone_from(&other.foliage);
        self.generation.clone_from(&other.generation);
        let chunks = chunk_count(self.size_x) * chunk_count(self.size_z);
        let next = self.newest_version() + 1;
        self.height_versions = vec![next; chunks];
        self.paint_versions = vec![next; chunks];
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
        for z in 0..size_z {
            for x in 0..size_x {
                let (fx, fz) = (x as f32 * scale_x, z as f32 * scale_z);
                heights.push(self.height_at(fx, fz));
                let near = self.index(
                    (fx.round() as usize).min(self.size_x - 1),
                    (fz.round() as usize).min(self.size_z - 1),
                );
                paint.push(self.paint[near]);
            }
        }
        let mut data = TerrainData::from_parts(size_x, size_z, heights, paint)?;
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
