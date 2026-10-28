//! The navigation mesh: where a character can walk, worked out from the
//! world's solid colliders (Parts and MeshParts with `CanCollide` on,
//! Terrains, and colliding foliage).
//!
//! The world is cut into square columns `cell_size` studs across. Down the
//! centre of each column, every solid gives the stretch of the line inside
//! it, and stretches that overlap merge. The top of a merged stretch is a
//! walkable cell when it is no steeper than `max_slope` and the next solid
//! above leaves `agent_height` of room. Neighbouring cells link when the
//! height between them can be stepped or walked up, and linked cells share a
//! region, so two points in different regions have no path between them.
//!
//! Building reads the world and never changes it. [`NavMesh::find_path`]
//! finds a walk across it, and [`Agents`] walks NavigationAgents along those
//! paths during Play, has them see and hear the player character, and runs
//! their behavior trees.

mod agents;
mod behavior;
mod path;
mod perception;

pub use agents::{AgentStatus, Agents, agent_parts, feet_of};
pub use behavior::{BEHAVIOR_NODE_KINDS, BEHAVIOR_TREE_SUFFIX, BehaviorTree, TreeError};
pub use path::PathError;
pub use perception::{Sense, Sensed};

use std::collections::VecDeque;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::ops::Range;

use axcenity_assets::Assets;
use axcenity_core::{ClassName, Value, Vector3, World};
use axcenity_physics::Column;

/// More columns than this and the cells grow, so a build stays quick: about
/// a 2048×2048-stud world at one stud a cell.
const MAX_COLUMNS: usize = 4 * 1024 * 1024;
/// Solids closer than this count as touching (a part resting on another).
const TOUCHING: f32 = 1e-3;
/// No link that way.
const NONE: u32 = u32::MAX;
/// The four neighbours of a column: -X, +X, -Z, +Z. The opposite of
/// direction `d` is `d ^ 1`.
const DIRECTIONS: [(i64, i64); 4] = [(-1, 0), (1, 0), (0, -1), (0, 1)];

/// What a character can cross, and how finely the world is cut.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NavSettings {
    /// Width of a cell, in studs.
    pub cell_size: f32,
    /// Room a character needs above the ground.
    pub agent_height: f32,
    /// Steepest walkable ground, in degrees.
    pub max_slope: f32,
    /// Tallest ledge a character steps up or down between cells.
    pub max_step: f32,
}

impl Default for NavSettings {
    /// A character controller's defaults (45° slopes and 1-stud steps), with
    /// the room a 5-stud character needs.
    fn default() -> Self {
        Self {
            cell_size: 1.0,
            agent_height: 5.0,
            max_slope: 45.0,
            max_step: 1.0,
        }
    }
}

/// One walkable cell: the ground at the centre of a column.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NavCell {
    pub column_x: u32,
    pub column_z: u32,
    /// The ground's height at the column's centre.
    pub height: f32,
    /// Which way the ground faces.
    pub normal: Vector3,
    /// Cells in the same region are connected; cells in different ones aren't.
    pub region: u32,
    /// The linked cell toward each of [`DIRECTIONS`], or [`NONE`].
    links: [u32; 4],
}

/// The walkable cells of a world, linked to their neighbours.
#[derive(Clone, Debug, Default)]
pub struct NavMesh {
    settings: NavSettings,
    /// World X and Z of the outer corner of column (0, 0).
    origin_x: f32,
    origin_z: f32,
    columns_x: usize,
    columns_z: usize,
    /// Cells ordered by column, lowest first within a column.
    cells: Vec<NavCell>,
    /// Where each column's cells start in `cells`, plus the end.
    column_start: Vec<u32>,
    regions: u32,
}

impl NavMesh {
    /// Builds the mesh from the Workspace's solids. Sensors (`CanCollide`
    /// off), visual foliage and the parts character controllers and
    /// navigation agents move add nothing. Run it again after the world changes; [`fingerprint`] tells
    /// when.
    pub fn build(world: &World, assets: &Assets, settings: NavSettings) -> Self {
        let settings = NavSettings {
            cell_size: positive(settings.cell_size, 1.0).max(0.05),
            agent_height: positive(settings.agent_height, 0.0),
            max_slope: positive(settings.max_slope, 0.0).min(89.0),
            max_step: positive(settings.max_step, 0.0),
        };
        let skip = moving_parts(world);
        let solids: Vec<_> = axcenity_physics::solids(world, assets, &skip)
            .into_iter()
            .filter(|solid| finite(solid.min) && finite(solid.max))
            .collect();
        let mut mesh = Self {
            settings,
            ..Self::default()
        };
        let Some(first) = solids.first() else {
            return mesh;
        };
        let (mut min_x, mut min_z) = (first.min.x, first.min.z);
        let (mut max_x, mut max_z) = (first.max.x, first.max.z);
        for solid in &solids {
            min_x = min_x.min(solid.min.x);
            min_z = min_z.min(solid.min.z);
            max_x = max_x.max(solid.max.x);
            max_z = max_z.max(solid.max.z);
        }
        loop {
            let cell = mesh.settings.cell_size;
            mesh.origin_x = (min_x / cell).floor() * cell;
            mesh.origin_z = (min_z / cell).floor() * cell;
            mesh.columns_x = (((max_x - mesh.origin_x) / cell).ceil() as usize).max(1);
            mesh.columns_z = (((max_z - mesh.origin_z) / cell).ceil() as usize).max(1);
            let columns = mesh.columns_x * mesh.columns_z;
            if columns <= MAX_COLUMNS {
                break;
            }
            mesh.settings.cell_size *= (columns as f32 / MAX_COLUMNS as f32).sqrt() * 1.01;
        }

        // Every solid's stretch down the centre of each column it covers.
        let mut spans: Vec<(usize, Column)> = Vec::new();
        for solid in &solids {
            let (x0, x1) =
                mesh.centres_within(solid.min.x, solid.max.x, mesh.origin_x, mesh.columns_x);
            let (z0, z1) =
                mesh.centres_within(solid.min.z, solid.max.z, mesh.origin_z, mesh.columns_z);
            for cz in z0..z1 {
                for cx in x0..x1 {
                    let (x, z) = mesh.centre(cx, cz);
                    if let Some(column) = solid.column(x, z) {
                        spans.push((cz * mesh.columns_x + cx, column));
                    }
                }
            }
        }
        spans.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.bottom.total_cmp(&b.1.bottom)));
        mesh.add_cells(&spans);
        mesh.link_cells();
        mesh.find_regions();
        mesh
    }

    /// Merges each column's stretches and keeps the tops that are walkable.
    fn add_cells(&mut self, spans: &[(usize, Column)]) {
        let slope_cos = self.settings.max_slope.to_radians().cos();
        self.column_start = vec![0; self.columns_x * self.columns_z + 1];
        let mut merged: Vec<Column> = Vec::new();
        for group in spans.chunk_by(|a, b| a.0 == b.0) {
            let column = group[0].0;
            merged.clear();
            for &(_, span) in group {
                match merged.last_mut() {
                    Some(last) if span.bottom <= last.top + TOUCHING => {
                        if span.top > last.top {
                            last.top = span.top;
                            last.normal = span.normal;
                        }
                    }
                    _ => merged.push(span),
                }
            }
            for (i, span) in merged.iter().enumerate() {
                let room = merged
                    .get(i + 1)
                    .map_or(f32::INFINITY, |above| above.bottom - span.top);
                if span.normal.y >= slope_cos - 1e-4 && room >= self.settings.agent_height {
                    self.cells.push(NavCell {
                        column_x: (column % self.columns_x) as u32,
                        column_z: (column / self.columns_x) as u32,
                        height: span.top,
                        normal: span.normal,
                        region: 0,
                        links: [NONE; 4],
                    });
                    self.column_start[column + 1] += 1;
                }
            }
        }
        for column in 0..self.columns_x * self.columns_z {
            self.column_start[column + 1] += self.column_start[column];
        }
    }

    /// Links each cell to the neighbour it can step or walk to: a rise no
    /// taller than `max_step`, or, on a slope, no taller than the slope
    /// climbs in one cell. A cliff between two flat cells is never linked.
    fn link_cells(&mut self) {
        let cell_size = self.settings.cell_size;
        let max_step = self.settings.max_step;
        let steepness = |normal: Vector3| {
            let up = normal.y.max(1e-3);
            (1.0 - up * up).max(0.0).sqrt() / up
        };
        let mut links = vec![[NONE; 4]; self.cells.len()];
        for (index, cell) in self.cells.iter().enumerate() {
            for (d, (dx, dz)) in DIRECTIONS.into_iter().enumerate() {
                let Some(column) =
                    self.column_index(i64::from(cell.column_x) + dx, i64::from(cell.column_z) + dz)
                else {
                    continue;
                };
                let mut best: Option<(f32, usize)> = None;
                for other in self.column_cells(column) {
                    let neighbour = &self.cells[other];
                    let rise = (neighbour.height - cell.height).abs();
                    let slope = steepness(cell.normal).max(steepness(neighbour.normal));
                    let allowed = max_step.max(cell_size * slope) + TOUCHING;
                    if rise <= allowed && best.is_none_or(|(r, _)| rise < r) {
                        best = Some((rise, other));
                    }
                }
                if let Some((_, other)) = best {
                    links[index][d] = other as u32;
                }
            }
        }
        // Only links both cells agree on, so every link goes both ways.
        for index in 0..self.cells.len() {
            for d in 0..4 {
                let other = links[index][d];
                if other != NONE && links[other as usize][d ^ 1] == index as u32 {
                    self.cells[index].links[d] = other;
                }
            }
        }
    }

    /// Numbers the groups of linked cells.
    fn find_regions(&mut self) {
        let mut seen = vec![false; self.cells.len()];
        let mut queue = VecDeque::new();
        for start in 0..self.cells.len() {
            if seen[start] {
                continue;
            }
            let region = self.regions;
            self.regions += 1;
            seen[start] = true;
            queue.push_back(start);
            while let Some(index) = queue.pop_front() {
                self.cells[index].region = region;
                for other in self.links(index) {
                    if !seen[other] {
                        seen[other] = true;
                        queue.push_back(other);
                    }
                }
            }
        }
    }

    pub fn settings(&self) -> NavSettings {
        self.settings
    }

    pub fn cells(&self) -> &[NavCell] {
        &self.cells
    }

    /// How many separate groups of connected cells there are.
    pub fn region_count(&self) -> u32 {
        self.regions
    }

    /// The cells linked to cell `index`.
    pub fn links(&self, index: usize) -> impl Iterator<Item = usize> + '_ {
        self.cells[index]
            .links
            .into_iter()
            .filter(|&link| link != NONE)
            .map(|link| link as usize)
    }

    /// The cell a character standing at `point` is on: the highest one in
    /// its column no more than `max_step` above `point` and no more than
    /// `agent_height` below it.
    pub fn cell_at(&self, point: Vector3) -> Option<usize> {
        let cell = self.settings.cell_size;
        let cx = ((point.x - self.origin_x) / cell).floor();
        let cz = ((point.z - self.origin_z) / cell).floor();
        if !cx.is_finite() || !cz.is_finite() || !point.y.is_finite() {
            return None;
        }
        let column = self.column_index(cx as i64, cz as i64)?;
        let reach = self.settings.max_step.max(TOUCHING);
        self.column_cells(column).rev().find(|&index| {
            let height = self.cells[index].height;
            height <= point.y + reach && point.y - height <= self.settings.agent_height
        })
    }

    /// True if a character can stand at `point`.
    pub fn is_walkable(&self, point: Vector3) -> bool {
        self.cell_at(point).is_some()
    }

    /// The region of the cell at `point`.
    pub fn region_at(&self, point: Vector3) -> Option<u32> {
        self.cell_at(point).map(|index| self.cells[index].region)
    }

    /// True if both points are walkable and a character could walk from
    /// one to the other.
    pub fn connected(&self, a: Vector3, b: Vector3) -> bool {
        matches!((self.region_at(a), self.region_at(b)), (Some(a), Some(b)) if a == b)
    }

    /// The four corners of cell `index`, flat at its height.
    pub fn corners(&self, index: usize) -> [Vector3; 4] {
        let cell = &self.cells[index];
        let (x0, z0, x1, z1) = self.bounds(cell);
        [
            Vector3::new(x0, cell.height, z0),
            Vector3::new(x1, cell.height, z0),
            Vector3::new(x1, cell.height, z1),
            Vector3::new(x0, cell.height, z1),
        ]
    }

    /// The sides of cells with no link across them: the edges of the
    /// walkable area, at each cell's height.
    pub fn outline(&self) -> Vec<(Vector3, Vector3)> {
        let mut edges = Vec::new();
        for cell in &self.cells {
            let (x0, z0, x1, z1) = self.bounds(cell);
            let at = |x, z| Vector3::new(x, cell.height, z);
            let sides = [
                (at(x0, z0), at(x0, z1)),
                (at(x1, z0), at(x1, z1)),
                (at(x0, z0), at(x1, z0)),
                (at(x0, z1), at(x1, z1)),
            ];
            for (side, link) in sides.into_iter().zip(cell.links) {
                if link == NONE {
                    edges.push(side);
                }
            }
        }
        edges
    }

    fn bounds(&self, cell: &NavCell) -> (f32, f32, f32, f32) {
        let size = self.settings.cell_size;
        let x0 = self.origin_x + cell.column_x as f32 * size;
        let z0 = self.origin_z + cell.column_z as f32 * size;
        (x0, z0, x0 + size, z0 + size)
    }

    fn centre(&self, cx: usize, cz: usize) -> (f32, f32) {
        let size = self.settings.cell_size;
        (
            self.origin_x + (cx as f32 + 0.5) * size,
            self.origin_z + (cz as f32 + 0.5) * size,
        )
    }

    /// The columns (start and end, end excluded) whose centres lie between
    /// `min` and `max` along one axis, of `count` columns from `origin`.
    fn centres_within(&self, min: f32, max: f32, origin: f32, count: usize) -> (usize, usize) {
        let size = self.settings.cell_size;
        let first = ((min - origin) / size - 0.5).ceil().max(0.0) as usize;
        let end = (((max - origin) / size - 0.5).floor() + 1.0).max(0.0) as usize;
        (first.min(count), end.min(count))
    }

    fn column_index(&self, cx: i64, cz: i64) -> Option<usize> {
        let inside =
            (0..self.columns_x as i64).contains(&cx) && (0..self.columns_z as i64).contains(&cz);
        inside.then(|| cz as usize * self.columns_x + cx as usize)
    }

    fn column_cells(&self, column: usize) -> Range<usize> {
        self.column_start[column] as usize..self.column_start[column + 1] as usize
    }
}

/// A number that changes whenever something the mesh is built from changes:
/// a Part's or MeshPart's place, size, shape, model or `CanCollide`, or a
/// Terrain's heights or foliage. Cheap enough to check every frame, so a
/// host can rebuild only when it changes.
pub fn fingerprint(world: &World) -> u64 {
    let skip = moving_parts(world);
    let mut hasher = DefaultHasher::new();
    for id in world.descendants(world.workspace()) {
        let Some(instance) = world.get(id) else {
            continue;
        };
        let class = instance.class();
        if !matches!(
            class,
            ClassName::Part
                | ClassName::MeshPart
                | ClassName::Seat
                | ClassName::VehicleSeat
                | ClassName::Terrain
        ) || skip.contains(&id)
        {
            continue;
        }
        id.hash(&mut hasher);
        for name in [
            "Position",
            "Orientation",
            "Size",
            "Shape",
            "MeshId",
            "CanCollide",
        ] {
            match instance.property(name) {
                Some(Value::Vector3(v)) => [v.x, v.y, v.z].map(f32::to_bits).hash(&mut hasher),
                Some(Value::Bool(on)) => on.hash(&mut hasher),
                Some(Value::String(text)) => text.hash(&mut hasher),
                Some(other) => format!("{other:?}").hash(&mut hasher),
                None => 0_u8.hash(&mut hasher),
            }
        }
        if let Some(data) = world.terrain(id) {
            data.uid().hash(&mut hasher);
            data.revision().hash(&mut hasher);
        }
    }
    hasher.finish()
}

/// The parts characters and agents move: they walk on the ground, so they
/// aren't ground.
fn moving_parts(world: &World) -> Vec<axcenity_core::InstanceId> {
    let mut parts = axcenity_character::character_parts(world);
    parts.extend(agent_parts(world));
    parts
}

fn positive(value: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        fallback
    }
}

fn finite(v: Vector3) -> bool {
    v.x.is_finite() && v.y.is_finite() && v.z.is_finite()
}

#[cfg(test)]
mod agent_tests;
#[cfg(test)]
mod behavior_tests;
#[cfg(test)]
mod perception_tests;
#[cfg(test)]
mod tests;
