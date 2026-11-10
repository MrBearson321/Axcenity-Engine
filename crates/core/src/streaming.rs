//! World streaming (Phase 80): which parts of the Workspace load and
//! unload together, and how far from a player they do.
//!
//! The world is split into [`Unit`]s, the pieces that are loaded or not:
//!
//! - a BasePart loose in the Workspace or a Folder, alone;
//! - a Model with no Model inside it, whole;
//! - an `Atomic` Model, whole, with the Models inside it.
//!
//! A Model with Models inside streams like a Folder, each piece on its
//! own. A `Persistent` Model, a Player's character and anything holding a
//! LocalScript or ModuleScript (whose code has to run) is in no unit: it's
//! always there. So is everything that isn't in the Workspace or isn't a
//! BasePart or Model. A unit's Model stays on every device when its
//! contents are gone, so scripts can name it and ask its `StreamingState`.
//!
//! `net` uses this to send a player only what's near them and `scene` to
//! draw and upload only what's near the camera.

use std::collections::HashMap;

use crate::spline;
use crate::{ClassName, EnumItem, InstanceId, Value, Vector3, World};

/// `Workspace.StreamingMinRadius`'s default, in studs.
pub const DEFAULT_MIN_RADIUS: f64 = 64.0;
/// `Workspace.StreamingTargetRadius`'s default, in studs.
pub const DEFAULT_TARGET_RADIUS: f64 = 1024.0;
/// What's loaded stays loaded this much further than the target radius,
/// so something on the edge doesn't load and unload over and over.
pub const UNLOAD_MARGIN: f32 = 1.25;

/// `ModelStreamingMode`'s items.
pub const STREAMING_MODES: [&str; 3] = ["Default", "Atomic", "Persistent"];

/// A `ModelStreamingMode` item.
pub fn streaming_mode(item: &str) -> Value {
    Value::EnumItem(EnumItem {
        enum_name: "ModelStreamingMode".to_owned(),
        item: item.to_owned(),
    })
}

/// How far streaming reaches, read from the Workspace.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    /// Everything this near is always loaded.
    pub min_radius: f32,
    /// As far as things load. Never less than `min_radius`.
    pub target_radius: f32,
}

impl Settings {
    /// The Workspace's, or `None` while `StreamingEnabled` is off.
    pub fn of(world: &World) -> Option<Settings> {
        let workspace = world.get(world.workspace())?;
        if workspace
            .property("StreamingEnabled")
            .and_then(Value::as_bool)
            != Some(true)
        {
            return None;
        }
        let number = |name: &str, default: f64| {
            workspace
                .property(name)
                .and_then(Value::as_number)
                .unwrap_or(default)
                .max(0.0) as f32
        };
        let min_radius = number("StreamingMinRadius", DEFAULT_MIN_RADIUS);
        let target_radius = number("StreamingTargetRadius", DEFAULT_TARGET_RADIUS).max(min_radius);
        Some(Settings {
            min_radius,
            target_radius,
        })
    }

    /// Past this, what's loaded unloads.
    pub fn unload_radius(&self) -> f32 {
        self.target_radius * UNLOAD_MARGIN
    }
}

/// A piece of the world that loads and unloads whole.
#[derive(Clone, Debug, PartialEq)]
pub struct Unit {
    /// The loose part, or the Model.
    pub root: InstanceId,
    /// Whether `root` is a Model, which stays when its contents go.
    pub keeps_root: bool,
    /// What loads and unloads, parents first: the part and what's in it,
    /// or what's in the Model.
    pub members: Vec<InstanceId>,
    /// The members without their parents among them: what has to be
    /// removed to remove them all.
    pub tops: Vec<InstanceId>,
    min: Vector3,
    max: Vector3,
}

impl Unit {
    /// How far `point` is from the unit's box, 0 inside it.
    pub fn distance(&self, point: Vector3) -> f32 {
        let along = |value: f32, min: f32, max: f32| (min - value).max(value - max).max(0.0);
        let x = along(point.x, self.min.x, self.max.x);
        let y = along(point.y, self.min.y, self.max.y);
        let z = along(point.z, self.min.z, self.max.z);
        (x * x + y * y + z * z).sqrt()
    }
}

/// The world split into [`Unit`]s, kept up to date by [`refresh`](Self::refresh).
#[derive(Clone, Debug, Default)]
pub struct Partition {
    units: Vec<Unit>,
    /// Each member's unit.
    unit_of: HashMap<InstanceId, usize>,
    /// The world's structure version this was built from.
    structure: Option<u64>,
    /// The unit whose box is measured next.
    next_bounds: usize,
}

/// Bounds are measured again over this many calls of
/// [`Partition::refresh`], so a big world isn't measured all at once.
const BOUNDS_ROUNDS: usize = 15;

impl Partition {
    /// A partition of `world` as it is now.
    pub fn new(world: &World) -> Partition {
        let mut partition = Partition::default();
        partition.refresh(world);
        partition
    }

    /// Catches up with `world`: splits it again when Instances moved in
    /// or out of the tree, and otherwise measures a share of the units'
    /// boxes again. Cheap enough for every frame.
    pub fn refresh(&mut self, world: &World) {
        // The partition is the Workspace's, and whose characters are in it.
        let version = world.structure_version_of(world.workspace())
            + world.structure_version_of(world.players());
        if self.structure != Some(version) {
            self.structure = Some(version);
            self.next_bounds = 0;
            self.rebuild(world);
            return;
        }
        if self.units.is_empty() {
            return;
        }
        for _ in 0..self.units.len().div_ceil(BOUNDS_ROUNDS) {
            self.next_bounds %= self.units.len();
            measure(world, &mut self.units[self.next_bounds]);
            self.next_bounds += 1;
        }
    }

    fn rebuild(&mut self, world: &World) {
        self.units.clear();
        self.unit_of.clear();
        let characters = characters(world);
        collect(world, world.workspace(), &characters, &mut self.units);
        for (index, unit) in self.units.iter_mut().enumerate() {
            for &member in &unit.members {
                self.unit_of.insert(member, index);
            }
            measure(world, unit);
        }
    }

    pub fn units(&self) -> &[Unit] {
        &self.units
    }

    /// The index of the unit `id` loads and unloads with, or `None` for
    /// an Instance that's always there (or that this partition hasn't
    /// seen yet).
    pub fn unit_of(&self, id: InstanceId) -> Option<usize> {
        self.unit_of.get(&id).copied()
    }

    /// Whether `id` loads and unloads with a unit.
    pub fn is_member(&self, id: InstanceId) -> bool {
        self.unit_of.contains_key(&id)
    }
}

/// Every Player's character Model.
fn characters(world: &World) -> Vec<InstanceId> {
    world
        .get(world.players())
        .map(|players| players.children().to_vec())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|player| world.get(player)?.property("Character")?.as_instance())
        .collect()
}

fn mode_of(world: &World, id: InstanceId) -> &'static str {
    let Some(Value::EnumItem(item)) = world
        .get(id)
        .and_then(|model| model.property("ModelStreamingMode"))
    else {
        return "Default";
    };
    STREAMING_MODES
        .into_iter()
        .find(|mode| *mode == item.item)
        .unwrap_or("Default")
}

/// Whether anything inside `id` is code a device runs.
fn holds_client_code(world: &World, id: InstanceId) -> bool {
    world.descendants_iter(id).any(|inside| {
        world.get(inside).is_some_and(|instance| {
            matches!(
                instance.class(),
                ClassName::LocalScript | ClassName::ModuleScript
            )
        })
    })
}

fn is_model(class: ClassName) -> bool {
    matches!(class, ClassName::Model | ClassName::Tool)
}

/// Adds the units under `parent`, which streams like a Folder.
fn collect(world: &World, parent: InstanceId, characters: &[InstanceId], units: &mut Vec<Unit>) {
    let children = world.get(parent).map(|p| p.children().to_vec());
    for child in children.unwrap_or_default() {
        let Some(class) = world.get(child).map(|instance| instance.class()) else {
            continue;
        };
        if class == ClassName::Folder {
            collect(world, child, characters, units);
        } else if class.is_base_part() {
            if spline::is_generated(world, child) || holds_client_code(world, child) {
                continue;
            }
            units.push(Unit {
                root: child,
                keeps_root: false,
                members: tree_of(world, child, true),
                tops: vec![child],
                min: Vector3::default(),
                max: Vector3::default(),
            });
        } else if is_model(class) {
            if characters.contains(&child) {
                continue;
            }
            let mode = mode_of(world, child);
            if mode == "Persistent" {
                continue;
            }
            if mode == "Default" && holds_model(world, child) {
                collect(world, child, characters, units);
                continue;
            }
            if holds_client_code(world, child) {
                continue;
            }
            let tops = world
                .get(child)
                .map(|model| model.children().to_vec())
                .unwrap_or_default();
            units.push(Unit {
                root: child,
                keeps_root: true,
                members: tree_of(world, child, false),
                tops,
                min: Vector3::default(),
                max: Vector3::default(),
            });
        }
    }
}

/// Whether there's a Model anywhere inside `id`.
fn holds_model(world: &World, id: InstanceId) -> bool {
    world
        .descendants_iter(id)
        .any(|inside| world.get(inside).is_some_and(|i| is_model(i.class())))
}

/// `id`'s descendants, parents first, without what a generator made,
/// with `id` itself first if `with_root`.
fn tree_of(world: &World, id: InstanceId, with_root: bool) -> Vec<InstanceId> {
    let mut tree = Vec::new();
    if with_root {
        tree.push(id);
    }
    tree.extend(
        world
            .descendants_iter(id)
            .filter(|&inside| !spline::is_generated(world, inside)),
    );
    tree
}

/// Measures the box around a unit's parts.
fn measure(world: &World, unit: &mut Unit) {
    let mut min = Vector3::new(f32::MAX, f32::MAX, f32::MAX);
    let mut max = Vector3::new(f32::MIN, f32::MIN, f32::MIN);
    let mut any = false;
    for &member in &unit.members {
        let Some(part) = world.get(member) else {
            continue;
        };
        if !part.class().is_base_part() {
            continue;
        }
        let vector = |name| part.property(name).and_then(Value::as_vector3);
        let (Some(position), Some(size)) = (vector("Position"), vector("Size")) else {
            continue;
        };
        // Half the diagonal: the box of a part turned any way.
        let reach = (size.x * size.x + size.y * size.y + size.z * size.z).sqrt() / 2.0;
        any = true;
        min.x = min.x.min(position.x - reach);
        min.y = min.y.min(position.y - reach);
        min.z = min.z.min(position.z - reach);
        max.x = max.x.max(position.x + reach);
        max.y = max.y.max(position.y + reach);
        max.z = max.z.max(position.z + reach);
    }
    if !any {
        // Nothing to place it by: it's near everything.
        min = Vector3::new(f32::MIN, f32::MIN, f32::MIN);
        max = Vector3::new(f32::MAX, f32::MAX, f32::MAX);
    }
    unit.min = min;
    unit.max = max;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part_at(world: &mut World, parent: InstanceId, x: f32) -> InstanceId {
        let part = world.create(ClassName::Part).unwrap();
        world
            .set_property(part, "Position", Value::Vector3(Vector3::new(x, 0.0, 0.0)))
            .unwrap();
        world.set_parent(part, Some(parent)).unwrap();
        part
    }

    fn model_in(world: &mut World, parent: InstanceId, mode: &str) -> InstanceId {
        let model = world.create(ClassName::Model).unwrap();
        world
            .set_property(model, "ModelStreamingMode", streaming_mode(mode))
            .unwrap();
        world.set_parent(model, Some(parent)).unwrap();
        model
    }

    #[test]
    fn streaming_is_off_by_default() {
        assert_eq!(Settings::of(&World::new()), None);
    }

    #[test]
    fn settings_are_the_workspaces_and_the_target_is_never_inside_the_minimum() {
        let mut world = World::new();
        let workspace = world.workspace();
        world
            .set_property(workspace, "StreamingEnabled", Value::Bool(true))
            .unwrap();
        world
            .set_property(workspace, "StreamingMinRadius", Value::Number(200.0))
            .unwrap();
        world
            .set_property(workspace, "StreamingTargetRadius", Value::Number(100.0))
            .unwrap();
        let settings = Settings::of(&world).unwrap();
        assert_eq!(settings.min_radius, 200.0);
        assert_eq!(settings.target_radius, 200.0);
    }

    #[test]
    fn a_loose_part_a_folders_part_and_a_leaf_model_are_units_of_their_own() {
        let mut world = World::new();
        let workspace = world.workspace();
        let loose = part_at(&mut world, workspace, 0.0);
        let folder = world.create(ClassName::Folder).unwrap();
        world.set_parent(folder, Some(workspace)).unwrap();
        let foldered = part_at(&mut world, folder, 10.0);
        let model = model_in(&mut world, workspace, "Default");
        let wall = part_at(&mut world, model, 20.0);
        let partition = Partition::new(&world);
        let roots: Vec<InstanceId> = partition.units().iter().map(|u| u.root).collect();
        assert_eq!(roots, [loose, foldered, model]);
        let unit = &partition.units()[2];
        assert!(unit.keeps_root);
        assert_eq!(unit.members, [wall]);
        assert!(partition.is_member(wall));
        assert!(
            !partition.is_member(model),
            "a Model stays when its parts go"
        );
    }

    #[test]
    fn a_model_with_models_inside_streams_each_piece_on_its_own() {
        let mut world = World::new();
        let workspace = world.workspace();
        let city = model_in(&mut world, workspace, "Default");
        let house = model_in(&mut world, city, "Default");
        let wall = part_at(&mut world, house, 0.0);
        let lamp = part_at(&mut world, city, 50.0);
        let partition = Partition::new(&world);
        let roots: Vec<InstanceId> = partition.units().iter().map(|u| u.root).collect();
        assert_eq!(roots, [house, lamp]);
        assert!(partition.is_member(wall));
    }

    #[test]
    fn an_atomic_model_with_models_inside_is_one_unit() {
        let mut world = World::new();
        let workspace = world.workspace();
        let atomic = model_in(&mut world, workspace, "Atomic");
        let inner = model_in(&mut world, atomic, "Default");
        part_at(&mut world, inner, 0.0);
        let partition = Partition::new(&world);
        assert_eq!(partition.units().len(), 1);
        assert_eq!(partition.units()[0].root, atomic);
    }

    #[test]
    fn persistent_models_characters_and_client_code_are_in_no_unit() {
        let mut world = World::new();
        let workspace = world.workspace();
        let persistent = model_in(&mut world, workspace, "Persistent");
        let kept = part_at(&mut world, persistent, 0.0);
        let with_script = model_in(&mut world, workspace, "Default");
        let code = world.create(ClassName::LocalScript).unwrap();
        world.set_parent(code, Some(with_script)).unwrap();
        let scripted = part_at(&mut world, with_script, 0.0);

        let character = model_in(&mut world, workspace, "Default");
        let body = part_at(&mut world, character, 0.0);
        let player = world.create(ClassName::Player).unwrap();
        world
            .set_property(player, "Character", Value::Instance(Some(character)))
            .unwrap();
        let players = world.players();
        world.set_parent(player, Some(players)).unwrap();

        let partition = Partition::new(&world);
        assert!(partition.units().is_empty());
        for id in [kept, scripted, body] {
            assert!(!partition.is_member(id));
        }
    }

    #[test]
    fn refresh_notices_new_parts_and_moved_ones() {
        let mut world = World::new();
        let workspace = world.workspace();
        let part = part_at(&mut world, workspace, 0.0);
        let mut partition = Partition::new(&world);
        let at = |partition: &Partition, x: f32| {
            partition.units()[0].distance(Vector3::new(x, 0.0, 0.0))
        };
        assert!(at(&partition, 0.0) < 1.0);
        world
            .set_property(
                part,
                "Position",
                Value::Vector3(Vector3::new(500.0, 0.0, 0.0)),
            )
            .unwrap();
        partition.refresh(&world);
        assert!(at(&partition, 0.0) > 450.0, "the box moved with the part");

        let other = part_at(&mut world, workspace, 0.0);
        partition.refresh(&world);
        assert_eq!(partition.units().len(), 2);
        assert!(partition.is_member(other));
    }

    #[test]
    fn distance_is_to_the_box() {
        let unit = Unit {
            root: InstanceId::from_bits(1),
            keeps_root: false,
            members: Vec::new(),
            tops: Vec::new(),
            min: Vector3::new(0.0, 0.0, 0.0),
            max: Vector3::new(10.0, 10.0, 10.0),
        };
        assert_eq!(unit.distance(Vector3::new(5.0, 5.0, 5.0)), 0.0);
        assert_eq!(unit.distance(Vector3::new(13.0, 5.0, 5.0)), 3.0);
    }
}
