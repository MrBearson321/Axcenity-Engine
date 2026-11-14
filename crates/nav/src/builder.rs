//! Building the mesh a piece at a time. Each ground part's piece (its
//! stretch down every column centre it covers) is kept between builds, so a
//! rebuild reads again only the parts that changed, and puts the mesh
//! together again only when a piece really changed.
//!
//! Reading the parts needs the world; putting the mesh together (merging,
//! linking and numbering every cell, the slow part on a big world) needs
//! only the pieces. [`MeshBuilder::look`] does the first and hands back the
//! second as an [`Assembly`], which may run on another thread
//! ([`crate::SharedNavMesh`] does that, roadmap item M13).
//!
//! The ground is what holds still: anchored parts, parts welded to one, and
//! Terrains. Unanchored parts are left out, so a crate rolling about never
//! rebuilds the mesh (roadmap item I7). Agents walk through where they lie,
//! as they already did while one was moving.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;

use axcenity_assets::Assets;
use axcenity_core::{ClassName, InstanceId, Value, Vector3, World};
use axcenity_physics::{Column, Solid};

use crate::{MAX_COLUMNS, NavMesh, NavSettings, finite, positive};

/// The properties a part's piece is made from. [`part_key`] hashes them,
/// so keep it in step with what `physics::part_solids` reads.
const SHAPE_PROPERTIES: [&str; 7] = [
    "Position",
    "Orientation",
    "Size",
    "Shape",
    "MeshId",
    "CollisionFidelity",
    "CanCollide",
];

/// What one ground part adds to the mesh.
struct Piece {
    /// The part's revision (and its terrain data's) when last looked at.
    stamp: (u64, u64),
    /// [`part_key`] when the piece was made.
    key: u64,
    /// The X and Z extent of its solids: min X, min Z, max X, max Z.
    bounds: Option<[f32; 4]>,
    /// Its stretch down each column centre it covers, by column counted
    /// from the world's origin. Shared with an [`Assembly`] still running.
    spans: Arc<[Span]>,
}

/// A stretch down one column centre: the column along X and Z, counted
/// from the world's origin, and the stretch.
type Span = (i64, i64, Column);

/// How the world is cut into columns.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Grid {
    cell: f32,
    /// The first column along X and Z, counted from the world's origin.
    first_x: i64,
    first_z: i64,
    columns_x: usize,
    columns_z: usize,
}

/// What [`MeshBuilder::look`] found.
pub enum Look {
    /// Nothing the mesh is built from has changed.
    Unchanged,
    /// Parts changed and were read again, but the mesh would come out the
    /// same.
    Same,
    /// The mesh must be put together again: run the assembly.
    Changed(Assembly),
}

/// Putting a mesh together from the pieces a [`MeshBuilder`] read: the slow
/// part of a build, which needs no world, so it can run on another thread.
pub struct Assembly {
    settings: NavSettings,
    grid: Option<Grid>,
    /// Every piece's spans, in the builder's (Instance) order, so the mesh
    /// comes out the same wherever it runs.
    pieces: Vec<Arc<[Span]>>,
}

/// What [`MeshBuilder::update`] did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refresh {
    /// Nothing the mesh is built from has changed.
    Unchanged,
    /// Parts changed and were read again, but the mesh came out the same.
    Same,
    /// The mesh was built again.
    Rebuilt,
}

/// Keeps a mesh in step with a world, rebuilding only what changed. Hosts
/// keep one and call [`MeshBuilder::update`] when they want the mesh
/// brought up to date; [`NavMesh::build`] makes a mesh in one go.
pub struct MeshBuilder {
    settings: NavSettings,
    pieces: BTreeMap<InstanceId, Piece>,
    /// The grid the pieces' spans are on.
    cell: f32,
    /// The grid of the mesh last assembled (or being assembled).
    grid: Option<Grid>,
    mesh: NavMesh,
    built: bool,
}

impl MeshBuilder {
    pub fn new(settings: NavSettings) -> Self {
        let settings = NavSettings {
            cell_size: positive(settings.cell_size, 1.0).max(0.05),
            agent_height: positive(settings.agent_height, 0.0),
            max_slope: positive(settings.max_slope, 0.0).min(89.0),
            max_step: positive(settings.max_step, 0.0),
        };
        Self {
            settings,
            pieces: BTreeMap::new(),
            cell: settings.cell_size,
            grid: None,
            mesh: NavMesh {
                settings,
                ..NavMesh::default()
            },
            built: false,
        }
    }

    /// The mesh as last built: empty until the first [`Self::update`].
    pub fn mesh(&self) -> &NavMesh {
        &self.mesh
    }

    pub fn into_mesh(self) -> NavMesh {
        self.mesh
    }

    /// Reads again the ground parts that changed since the last call and
    /// rebuilds the mesh if what they add to it changed. Cheap when nothing
    /// changed: one walk of the Workspace, comparing revisions.
    pub fn update(&mut self, world: &World, assets: &Assets) -> Refresh {
        match self.look(world, assets) {
            Look::Unchanged => Refresh::Unchanged,
            Look::Same => Refresh::Same,
            Look::Changed(assembly) => {
                self.mesh = assembly.run();
                Refresh::Rebuilt
            }
        }
    }

    /// [`Self::update`] without putting the mesh together: reads again the
    /// ground parts that changed and says whether the mesh must be
    /// assembled again. [`Self::mesh`] stays the old mesh; whoever runs the
    /// [`Assembly`] keeps the new one.
    pub fn look(&mut self, world: &World, assets: &Assets) -> Look {
        let ground = ground(world);
        let mut stale = Vec::new();
        let mut added = 0;
        for &id in &ground {
            let stamp = stamp(world, id);
            match self.pieces.get(&id) {
                Some(piece) if piece.stamp == stamp => {}
                Some(_) => stale.push((id, stamp)),
                None => {
                    added += 1;
                    stale.push((id, stamp));
                }
            }
        }
        let removed = self.pieces.len() + added != ground.len();
        if stale.is_empty() && !removed && self.built {
            return Look::Unchanged;
        }

        // A change to a property the piece isn't made from (a color, say)
        // needs nothing more.
        let mut reread: Vec<(InstanceId, (u64, u64), u64)> = Vec::new();
        for (id, stamp) in stale {
            let key = part_key(world, id);
            match self.pieces.get_mut(&id) {
                Some(piece) if piece.key == key => piece.stamp = stamp,
                _ => reread.push((id, stamp, key)),
            }
        }
        let mut changed = !self.built;
        if removed {
            let keep: HashSet<InstanceId> = ground.iter().copied().collect();
            self.pieces.retain(|id, piece| {
                let kept = keep.contains(id);
                changed |= !kept && (piece.bounds.is_some() || !piece.spans.is_empty());
                kept
            });
        }
        if reread.is_empty() && !changed {
            return if removed { Look::Same } else { Look::Unchanged };
        }

        let mut fresh: HashMap<InstanceId, Vec<Solid>> = reread
            .iter()
            .map(|&(id, _, _)| (id, solids_of(world, assets, id)))
            .collect();
        let mut bounds: Option<[f32; 4]> = None;
        for (id, piece) in &self.pieces {
            let piece_bounds = match fresh.get(id) {
                Some(solids) => bounds_of(solids),
                None => piece.bounds,
            };
            bounds = join(bounds, piece_bounds);
        }
        for (id, solids) in &fresh {
            if !self.pieces.contains_key(id) {
                bounds = join(bounds, bounds_of(solids));
            }
        }
        let grid = bounds.map(|bounds| grid(self.settings.cell_size, bounds));

        // A world big enough to need coarser cells puts every piece on the
        // new grid.
        let cell = grid.map_or(self.settings.cell_size, |grid| grid.cell);
        if cell != self.cell {
            self.cell = cell;
            changed = true;
            for (&id, piece) in &self.pieces {
                if let std::collections::hash_map::Entry::Vacant(entry) = fresh.entry(id) {
                    reread.push((id, piece.stamp, piece.key));
                    entry.insert(solids_of(world, assets, id));
                }
            }
        }
        for (id, stamp, key) in reread {
            let solids = fresh.remove(&id).unwrap_or_default();
            let piece = Piece {
                stamp,
                key,
                bounds: bounds_of(&solids),
                spans: spans_of(&solids, cell).into(),
            };
            match self.pieces.insert(id, piece) {
                Some(old) => {
                    let new = &self.pieces[&id];
                    changed |= old.spans != new.spans;
                }
                None => changed |= !self.pieces[&id].spans.is_empty(),
            }
        }
        changed |= grid != self.grid;
        if !changed {
            return Look::Same;
        }
        self.built = true;
        self.grid = grid;
        Look::Changed(Assembly {
            settings: self.settings,
            grid,
            pieces: self
                .pieces
                .values()
                .map(|piece| Arc::clone(&piece.spans))
                .collect(),
        })
    }
}

impl Assembly {
    /// Puts the mesh together from the pieces.
    pub fn run(self) -> NavMesh {
        let Some(grid) = self.grid else {
            return NavMesh {
                settings: self.settings,
                ..NavMesh::default()
            };
        };
        let mut mesh = NavMesh {
            settings: NavSettings {
                cell_size: grid.cell,
                ..self.settings
            },
            origin_x: grid.first_x as f32 * grid.cell,
            origin_z: grid.first_z as f32 * grid.cell,
            columns_x: grid.columns_x,
            columns_z: grid.columns_z,
            ..NavMesh::default()
        };
        // Every span by column, lowest first: counted into place, as most
        // columns hold one or two.
        let index = |x: i64, z: i64| {
            let (cx, cz) = (x - grid.first_x, z - grid.first_z);
            let inside = (0..grid.columns_x as i64).contains(&cx)
                && (0..grid.columns_z as i64).contains(&cz);
            inside.then(|| cz as usize * grid.columns_x + cx as usize)
        };
        let mut start = vec![0_usize; grid.columns_x * grid.columns_z + 1];
        for piece in &self.pieces {
            for &(x, z, _) in piece.iter() {
                if let Some(column) = index(x, z) {
                    start[column + 1] += 1;
                }
            }
        }
        for column in 1..start.len() {
            start[column] += start[column - 1];
        }
        let empty = Column {
            bottom: 0.0,
            top: 0.0,
            normal: Vector3::new(0.0, 1.0, 0.0),
        };
        let mut spans = vec![(0, empty); start[start.len() - 1]];
        let mut next = start.clone();
        for piece in &self.pieces {
            for &(x, z, column) in piece.iter() {
                if let Some(at) = index(x, z) {
                    spans[next[at]] = (at, column);
                    next[at] += 1;
                }
            }
        }
        for column in start.windows(2) {
            spans[column[0]..column[1]].sort_by(|a, b| a.1.bottom.total_cmp(&b.1.bottom));
        }
        mesh.add_cells(&spans);
        mesh.link_cells();
        mesh.find_regions();
        mesh
    }
}

/// A number that changes whenever something the mesh is built from
/// changes: a ground part's place, size, shape, model or `CanCollide`, a
/// part becoming anchored or unanchored, or a Terrain's heights or
/// foliage. Cheap enough to check every frame, so a host can rebuild only
/// when it changes.
pub fn fingerprint(world: &World) -> u64 {
    let mut hasher = DefaultHasher::new();
    for id in ground(world) {
        id.hash(&mut hasher);
        part_key(world, id).hash(&mut hasher);
    }
    hasher.finish()
}

/// The parts the mesh is built from, in tree order: every part in the
/// Workspace that is anchored or welded to an anchored part, and every
/// Terrain, except the parts characters and agents move (they walk on the
/// ground, so they aren't ground).
fn ground(world: &World) -> Vec<InstanceId> {
    let mut ground = Vec::new();
    let mut loose = Vec::new();
    let mut welds = Vec::new();
    // What character controllers and agents move, as
    // `character::character_parts` and `agent_parts` find it, from the same
    // walk.
    let mut movers = Vec::new();
    for id in world.descendants_iter(world.workspace()) {
        let Some(instance) = world.get(id) else {
            continue;
        };
        let class = instance.class();
        if class == ClassName::WeldConstraint {
            welds.push(id);
        } else if class.is_character_controller() || class == ClassName::NavigationAgent {
            movers.extend(instance.parent());
        } else if class == ClassName::Terrain {
            ground.push(id);
        } else {
            match instance.property("Anchored") {
                Some(Value::Bool(true)) => ground.push(id),
                Some(Value::Bool(false)) => loose.push(id),
                _ => {}
            }
        }
    }
    if !movers.is_empty() {
        let moving: HashSet<InstanceId> = movers
            .into_iter()
            .flat_map(|holder| axcenity_character::body_parts(world, holder))
            .collect();
        ground.retain(|id| !moving.contains(id));
        loose.retain(|id| !moving.contains(id));
    }
    if welds.is_empty() || loose.is_empty() {
        return ground;
    }

    // Loose parts welded to an anchored one hold still with it, as physics
    // moves an assembly with an anchored part as one anchored body.
    let pairs: Vec<_> = welds
        .into_iter()
        .filter_map(|weld| axcenity_physics::assembly::weld_parts(world, weld))
        .collect();
    let mut welded: Vec<InstanceId> = pairs.iter().flat_map(|&(a, b)| [a, b]).collect();
    welded.sort();
    welded.dedup();
    let anchored: HashSet<InstanceId> = ground.iter().copied().collect();
    let mut held = HashSet::new();
    for assembly in axcenity_physics::assembly::group(&welded, &pairs) {
        if assembly.iter().any(|id| anchored.contains(id)) {
            held.extend(assembly);
        }
    }
    if held.is_empty() {
        return ground;
    }
    // Back into tree order, so the fingerprint doesn't depend on weld order.
    let mut ground: HashSet<InstanceId> = ground.into_iter().collect();
    ground.extend(loose.into_iter().filter(|id| held.contains(id)));
    world
        .descendants_iter(world.workspace())
        .filter(|id| ground.contains(id))
        .collect()
}

/// A part's revision, and its terrain data's: the same pair means nothing
/// about it has changed.
fn stamp(world: &World, id: InstanceId) -> (u64, u64) {
    let revision = world.get(id).map_or(0, |instance| instance.revision());
    let terrain = world.terrain(id).map_or(0, |data| {
        let mut hasher = DefaultHasher::new();
        (data.uid(), data.revision(), data.generated_version()).hash(&mut hasher);
        hasher.finish()
    });
    (revision, terrain)
}

/// A hash of the properties a part's piece is made from (and its terrain
/// data), without formatting any of them.
fn part_key(world: &World, id: InstanceId) -> u64 {
    let mut hasher = DefaultHasher::new();
    if let Some(instance) = world.get(id) {
        instance.class().hash(&mut hasher);
        for name in SHAPE_PROPERTIES {
            hash_value(instance.property(name), &mut hasher);
        }
    }
    if let Some(data) = world.terrain(id) {
        data.uid().hash(&mut hasher);
        data.revision().hash(&mut hasher);
        // What its scatter rules placed collides too, and isn't in the revision.
        data.generated_version().hash(&mut hasher);
    }
    hasher.finish()
}

fn hash_value(value: Option<&Value>, hasher: &mut DefaultHasher) {
    let Some(value) = value else {
        0_u8.hash(hasher);
        return;
    };
    std::mem::discriminant(value).hash(hasher);
    match value {
        Value::Vector3(v) => [v.x, v.y, v.z].map(f32::to_bits).hash(hasher),
        Value::Bool(on) => on.hash(hasher),
        Value::String(text) => text.hash(hasher),
        Value::PartType(shape) => shape.hash(hasher),
        Value::EnumItem(item) => item.item.hash(hasher),
        // Nothing else is read from these properties.
        _ => {}
    }
}

/// A part's solids with a finite box.
fn solids_of(world: &World, assets: &Assets, id: InstanceId) -> Vec<Solid> {
    axcenity_physics::part_solids(world, assets, id)
        .into_iter()
        .filter(|solid| finite(solid.min) && finite(solid.max))
        .collect()
}

fn bounds_of(solids: &[Solid]) -> Option<[f32; 4]> {
    solids.iter().fold(None, |bounds, solid| {
        join(
            bounds,
            Some([solid.min.x, solid.min.z, solid.max.x, solid.max.z]),
        )
    })
}

fn join(a: Option<[f32; 4]>, b: Option<[f32; 4]>) -> Option<[f32; 4]> {
    match (a, b) {
        (Some(a), Some(b)) => Some([
            a[0].min(b[0]),
            a[1].min(b[1]),
            a[2].max(b[2]),
            a[3].max(b[3]),
        ]),
        (a, None) => a,
        (None, b) => b,
    }
}

/// The columns over `bounds`, `cell` studs across, or wider cells when that
/// would be more than [`MAX_COLUMNS`].
fn grid(mut cell: f32, [min_x, min_z, max_x, max_z]: [f32; 4]) -> Grid {
    loop {
        let first_x = (min_x / cell).floor();
        let first_z = (min_z / cell).floor();
        let columns_x = (((max_x - first_x * cell) / cell).ceil() as usize).max(1);
        let columns_z = (((max_z - first_z * cell) / cell).ceil() as usize).max(1);
        let columns = columns_x * columns_z;
        if columns <= MAX_COLUMNS {
            return Grid {
                cell,
                first_x: first_x as i64,
                first_z: first_z as i64,
                columns_x,
                columns_z,
            };
        }
        cell *= (columns as f32 / MAX_COLUMNS as f32).sqrt() * 1.01;
    }
}

/// Every solid's stretch down the centre of each column it covers.
fn spans_of(solids: &[Solid], cell: f32) -> Vec<Span> {
    // The columns whose centres lie between `min` and `max` along one axis.
    let centres = |min: f32, max: f32| {
        let first = (min / cell - 0.5).ceil() as i64;
        let end = ((max / cell - 0.5).floor() + 1.0) as i64;
        first..end
    };
    let mut spans = Vec::new();
    for solid in solids {
        for z in centres(solid.min.z, solid.max.z) {
            for x in centres(solid.min.x, solid.max.x) {
                let centre = |i: i64| (i as f32 + 0.5) * cell;
                if let Some(column) = solid.column(centre(x), centre(z)) {
                    spans.push((x, z, column));
                }
            }
        }
    }
    spans
}
