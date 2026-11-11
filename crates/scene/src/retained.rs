//! Retained draw items (M4): what each Instance draws, kept between frames.
//!
//! Without this every frame walks the Workspace and works out every part's
//! draw item from its properties, so the cost grows with the world and not
//! with what changed. A [`DrawCache`] keeps each Instance's items and uses
//! them again until something they were made from changes:
//!
//! - **The Instance itself**: its [`revision`](axcenity_core::Instance::revision).
//! - **What it reads from other Instances**: a part's SurfaceAppearance and
//!   light children, a Decal's part, and the Inventory that decides whether a
//!   carried Tool is out of the world. Their revisions are kept with the items.
//! - **The tree**: moving anything in or out of the Workspace, or among a
//!   parent's children, goes over the list again in tree order. Every kept
//!   entry is checked against the tree (its children, whether it's still in
//!   a character), so a new child or Humanoid updates the parts it affects
//!   without rebuilding the rest.
//! - **Everything at once**: another World (copies keep their revisions),
//!   another or changed [`Assets`] (loads, reloads, material edits, the
//!   blueprint image), or a change in how glowing parts are boosted.
//!
//! Nothing here reads an ancestor's properties except that Inventory, so an
//! edit to a Model or Folder doesn't invalidate what's inside it. Only the
//! ones listed above are kept: lights, probes, ProjectedDecals and joints are
//! worked out each frame (they're few, and joints read other parts), and an
//! animated MeshPart is rebuilt each frame so it can reuse its skin's buffers.
//! Terrain, UI and particles are drawn as ever. Streaming is checked each
//! frame, since what it hides changes with the camera.
//!
//! The items come out in the order a walk of the Workspace gives them.
//! The class index can't supply that order, so a frame in which the tree
//! changed still walks it; a frame in which it didn't does not.

use std::sync::OnceLock;

use axcenity_assets::Assets;
use axcenity_core::csg::{in_union, out_of_world};
use axcenity_core::{ClassName, Instance, InstanceId, World, tool};
use axcenity_render::{DecalRole, DrawItem};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::draw_list::{InstanceFrame, OtherDrawn, add_instance};
use crate::mesh_cache::{MeshCache, mesh_address};
use crate::streaming::Streaming;

/// The environment variable that turns retained draw items off: `off` or
/// `0`. They're on otherwise (measured in `docs/rendering/performance.md`).
const SWITCH: &str = "AXCENITY_RETAINED_DRAW_LIST";

/// Whether a new cache keeps draw items, from [`SWITCH`] (read once).
pub(crate) fn on_by_default() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| !matches!(std::env::var(SWITCH).as_deref(), Ok("off" | "0")))
}

/// See the module docs. Kept with the host's
/// [`TerrainCache`](crate::TerrainCache).
pub(crate) struct DrawCache {
    enabled: bool,
    /// What the kept items were made from: when any of it differs,
    /// everything is made again (see [`Key`]).
    key: Option<Key>,
    /// The tree the entries were listed from.
    structure: Option<(u64, u64)>,
    /// Goes up to make every entry out of date at once.
    epoch: u64,
    /// One per Instance in the Workspace that can draw, in tree order.
    entries: Vec<Entry>,
    /// Instances whose items were made by the last frame, for tests.
    rebuilt: usize,
}

/// What every kept item depends on.
#[derive(Clone)]
struct Key {
    world: u64,
    assets: Assets,
    assets_generation: u64,
    /// The bits, so a NaN can't make it differ from itself.
    glow: u32,
}

impl Key {
    fn matches(&self, other: &Key) -> bool {
        self.world == other.world
            && self.assets.same_as(&other.assets)
            && self.assets_generation == other.assets_generation
            && self.glow == other.glow
    }
}

/// One Instance's place in the draw list.
struct Entry {
    id: InstanceId,
    class: ClassName,
    /// Worked out each frame instead of kept (see the module docs).
    per_frame: bool,
    /// The Inventory that decides whether it's out of the world, if any.
    inventory: Option<InstanceId>,
    /// Its revision when `items` were made.
    revision: u64,
    /// The epoch `items` were made in. 0 means they need making.
    epoch: u64,
    /// The other Instances `items` were made from, with their revisions.
    depends_on: Vec<(InstanceId, u64)>,
    items: Vec<DrawItem>,
}

/// Which classes are worked out each frame rather than kept.
fn drawn_each_frame(class: ClassName) -> bool {
    class.is_light()
        || class.is_joint()
        || matches!(
            class,
            ClassName::ProjectedDecal
                | ClassName::ReflectionProbe
                | ClassName::LightProbeVolume
                | ClassName::Terrain
        )
}

/// The other Instances what `instance` draws is made from, in the order
/// [`Entry::depends_on`] keeps them: a Decal's part, its SurfaceAppearances
/// and lights, and the Inventory carrying it.
fn dependencies(
    world: &World,
    instance: &Instance,
    inventory: Option<InstanceId>,
    found: &mut Vec<InstanceId>,
) {
    found.clear();
    if instance.class() == ClassName::Decal {
        found.extend(instance.parent());
    }
    found.extend(instance.children().iter().copied().filter(|&child| {
        world.get(child).is_some_and(|child| {
            child.class() == ClassName::SurfaceAppearance || child.class().is_light()
        })
    }));
    found.extend(inventory);
}

impl Entry {
    fn new(id: InstanceId, class: ClassName) -> Self {
        Self {
            id,
            class,
            per_frame: drawn_each_frame(class),
            inventory: None,
            revision: 0,
            epoch: 0,
            depends_on: Vec::new(),
            items: Vec::new(),
        }
    }

    /// Whether `items` can be used this frame.
    fn is_current(&self, world: &World, instance: &Instance, epoch: u64) -> bool {
        self.epoch == epoch
            && self.revision == instance.revision()
            && self
                .depends_on
                .iter()
                .all(|&(id, revision)| world.get(id).map(Instance::revision) == Some(revision))
    }
}

impl DrawCache {
    pub(crate) fn new(enabled: bool) -> Self {
        Self {
            enabled,
            key: None,
            structure: None,
            epoch: 1,
            entries: Vec::new(),
            rebuilt: 0,
        }
    }

    pub(crate) fn enabled(&self) -> bool {
        self.enabled
    }

    #[cfg(test)]
    pub(crate) fn rebuilt_last_frame(&self) -> usize {
        self.rebuilt
    }

    /// Adds what the Workspace's Instances draw to `items` and `other`, in
    /// the order a walk of the tree gives them, as one frame built from
    /// scratch would.
    pub(crate) fn add_to(
        &mut self,
        frame: &InstanceFrame<'_>,
        streaming: &Streaming,
        meshes: &mut MeshCache,
        items: &mut Vec<DrawItem>,
        other: &mut OtherDrawn,
    ) {
        profiling::scope!("DrawCache::add_to");
        let world = frame.world;
        meshes.start_frame();
        // Read before anything draws: a file that finishes loading during
        // this frame makes the next one start over, with it in.
        let key = Key {
            world: world.identity(),
            assets: frame.assets.clone(),
            assets_generation: frame.assets.generation(),
            glow: frame.glow.to_bits(),
        };
        let same_key = self.key.as_ref().is_some_and(|kept| kept.matches(&key));
        let another_world = self.key.as_ref().is_none_or(|kept| kept.world != key.world);
        if another_world {
            self.entries.clear();
        }
        if !same_key {
            self.epoch += 1;
            self.key = Some(key);
        }
        let mut characters = crate::decals::Characters::default();
        let structure = (
            world.structure_version_of(world.workspace()),
            world.child_order_version(),
        );
        let tree_changed = another_world || self.structure != Some(structure);
        if tree_changed {
            self.go_over_tree(world, &mut characters);
            self.structure = Some(structure);
        }

        let hiding = streaming.hides_anything();
        let posing = !frame.poses.is_empty();
        let epoch = self.epoch;
        let mut rebuilt = 0;
        let mut model_use_changed = false;
        items.reserve(self.entries.len());
        for entry in &mut self.entries {
            let Some(instance) = world.get(entry.id) else {
                continue;
            };
            if hiding && streaming.is_hidden(entry.id) {
                continue;
            }
            // Moving parts are drawn between physics steps, without
            // moving the items kept for them.
            let id = entry.id;
            let shift =
                |drawn: &mut [DrawItem]| frame.blends.shift_items(world, id, instance, drawn);
            if entry.per_frame {
                if entry.inventory.is_none() || !out_of_world(world, entry.id) {
                    let first = items.len();
                    add_instance(
                        frame,
                        entry.id,
                        instance,
                        meshes,
                        &mut characters,
                        items,
                        other,
                    );
                    shift(&mut items[first..]);
                }
                continue;
            }
            if posing && frame.poses.contains_key(&entry.id) {
                // A pose changes every frame, and the item made from it
                // mustn't keep the skin's buffers from being reused.
                entry.items.clear();
                entry.epoch = 0;
                if entry.inventory.is_none() || !out_of_world(world, entry.id) {
                    let first = items.len();
                    add_instance(
                        frame,
                        entry.id,
                        instance,
                        meshes,
                        &mut characters,
                        items,
                        other,
                    );
                    shift(&mut items[first..]);
                }
                continue;
            }
            if !entry.is_current(world, instance, epoch) {
                let before = entry
                    .items
                    .first()
                    .and_then(|item| mesh_address(&item.shape));
                entry.items.clear();
                if entry.inventory.is_none() || !out_of_world(world, entry.id) {
                    add_instance(
                        frame,
                        entry.id,
                        instance,
                        meshes,
                        &mut characters,
                        &mut entry.items,
                        other,
                    );
                }
                let mut depends_on = Vec::new();
                dependencies(world, instance, entry.inventory, &mut depends_on);
                entry.depends_on.clear();
                entry.depends_on.extend(
                    depends_on
                        .into_iter()
                        .filter_map(|id| Some((id, world.get(id)?.revision()))),
                );
                entry.revision = instance.revision();
                entry.epoch = epoch;
                rebuilt += 1;
                let after = entry
                    .items
                    .first()
                    .and_then(|item| mesh_address(&item.shape));
                model_use_changed |= before != after;
            }
            let first = items.len();
            items.extend_from_slice(&entry.items);
            shift(&mut items[first..]);
        }
        self.rebuilt = rebuilt;
        // Whichever models the kept items hold are the ones in use.
        let models_in_use = (tree_changed || !same_key || model_use_changed).then(|| {
            self.entries
                .iter()
                .filter_map(|entry| entry.items.first())
                .filter_map(|item| mesh_address(&item.shape))
                .chain(items.iter().filter_map(|item| mesh_address(&item.shape)))
                .collect::<FxHashSet<usize>>()
        });
        meshes.end_frame_retained(models_in_use);
    }

    /// Lists the Workspace's Instances again in tree order, keeping what was
    /// made for those still here, and marks out of date the ones whose
    /// children or character changed.
    fn go_over_tree(&mut self, world: &World, characters: &mut crate::decals::Characters) {
        profiling::scope!("DrawCache::go_over_tree");
        // Most worlds have neither, and then nothing is out of the world.
        let unions_or_tools = world.all_of(ClassName::UnionOperation).next().is_some()
            || world.all_of(ClassName::Tool).next().is_some();
        let mut kept: FxHashMap<InstanceId, Entry> = self
            .entries
            .drain(..)
            .map(|entry| (entry.id, entry))
            .collect();
        let mut depends_on = Vec::new();
        for id in world.descendants_iter(world.workspace()) {
            let Some(instance) = world.get(id) else {
                continue;
            };
            // A part inside a union stays out of the world until Separate
            // moves it, and that's a change of parent.
            if unions_or_tools && in_union(world, id) {
                continue;
            }
            // The class check is for an ID used again by `create_at`.
            let mut entry = kept
                .remove(&id)
                .filter(|entry| entry.class == instance.class())
                .unwrap_or_else(|| Entry::new(id, instance.class()));
            entry.inventory = unions_or_tools
                .then(|| tool::carrying_inventory(world, id))
                .flatten();
            if !entry.per_frame {
                dependencies(world, instance, entry.inventory, &mut depends_on);
                let same_dependencies = entry
                    .depends_on
                    .iter()
                    .map(|&(id, _)| id)
                    .eq(depends_on.iter().copied());
                let same_character = !instance.class().is_base_part()
                    || entry.items.first().is_none_or(|item| {
                        (item.decal_role == DecalRole::Character) == characters.contains(world, id)
                    });
                if !same_dependencies || !same_character {
                    entry.epoch = 0;
                }
            }
            self.entries.push(entry);
        }
    }
}

#[cfg(test)]
#[path = "retained_tests.rs"]
mod tests;
