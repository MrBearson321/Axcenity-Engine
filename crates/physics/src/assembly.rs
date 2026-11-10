//! Assemblies: parts held together by enabled WeldConstraints move as one
//! rigid body. Physics gives each assembly one rapier body with a collider
//! per part; scripts ask for the parts with `GetConnectedParts` and the
//! root with `GetRootPart`.
//!
//! The root is the part the others are counted from: an anchored part if
//! there is one, then one that isn't `Massless`, then the largest. Ties go
//! to the part made first (the smallest ID), so it's always the same one.

use std::collections::{BTreeMap, BTreeSet};

use axcenity_core::{ClassName, Instance, InstanceId, Value, World};

/// The pairs of parts each enabled WeldConstraint in the game holds
/// together, in tree order. A weld missing a part, or welding a part to
/// itself, holds nothing. A weld that can break (Phase 42) isn't listed:
/// physics holds its parts with a joint instead (see [`crate::joints`]).
pub fn welds(world: &World) -> Vec<(InstanceId, InstanceId)> {
    world
        .descendants_iter(world.root())
        .filter_map(|id| {
            let weld = world.get(id)?;
            if weld.class() != ClassName::WeldConstraint
                || weld.property("Enabled") != Some(&Value::Bool(true))
                || crate::joints::is_breakable_weld(weld)
            {
                return None;
            }
            let part = |name| match weld.property(name) {
                Some(Value::Instance(Some(part))) if world.get(*part).is_some_and(is_part) => {
                    Some(*part)
                }
                _ => None,
            };
            let (part0, part1) = (part("Part0")?, part("Part1")?);
            (part0 != part1).then_some((part0, part1))
        })
        .collect()
}

/// Splits `parts` into assemblies joined by `welds`. A weld naming a part
/// that isn't in `parts` joins nothing. Each assembly is sorted, and they
/// come in order of their first part.
pub fn group(parts: &[InstanceId], welds: &[(InstanceId, InstanceId)]) -> Vec<Vec<InstanceId>> {
    // Union-find: each part points towards a representative of its group.
    let mut leader: BTreeMap<InstanceId, InstanceId> = parts.iter().map(|&id| (id, id)).collect();
    fn find(leader: &mut BTreeMap<InstanceId, InstanceId>, id: InstanceId) -> InstanceId {
        let mut root = id;
        while leader[&root] != root {
            root = leader[&root];
        }
        // Point everything on the way straight at the root, so the next
        // search is short.
        let mut next = id;
        while next != root {
            next = leader.insert(next, root).expect("in the map");
        }
        root
    }
    for &(a, b) in welds {
        if !leader.contains_key(&a) || !leader.contains_key(&b) {
            continue;
        }
        let (a, b) = (find(&mut leader, a), find(&mut leader, b));
        // The smaller ID leads, so the result doesn't depend on weld order.
        leader.insert(a.max(b), a.min(b));
    }
    let mut groups: BTreeMap<InstanceId, Vec<InstanceId>> = BTreeMap::new();
    for &id in parts {
        let root = find(&mut leader, id);
        groups.entry(root).or_default().push(id);
    }
    // A group's leader is its smallest part, so the map's order is the order
    // of first parts.
    groups
        .into_values()
        .map(|mut members| {
            members.sort();
            members
        })
        .collect()
}

/// The root of an assembly of `members` (sorted), which are in `world`.
pub fn root(world: &World, members: &[InstanceId]) -> InstanceId {
    let rank = |id: InstanceId| world.get(id).map(Rank::of).unwrap_or_default();
    let mut best = members[0];
    for &id in &members[1..] {
        // Strictly better only, so ties keep the smaller ID.
        if rank(id) > rank(best) {
            best = id;
        }
    }
    best
}

/// `part:GetConnectedParts(recursive)`: the part, then the parts welded to
/// it directly, or with `recursive` every other part in its assembly, in
/// the order they were made.
pub fn connected_parts(world: &World, part: InstanceId, recursive: bool) -> Vec<InstanceId> {
    let welds = welds(world);
    let mut found = BTreeSet::new();
    let mut stack = vec![part];
    while let Some(next) = stack.pop() {
        for &(a, b) in &welds {
            let other = if a == next {
                b
            } else if b == next {
                a
            } else {
                continue;
            };
            if other != part && found.insert(other) && recursive {
                stack.push(other);
            }
        }
    }
    std::iter::once(part).chain(found).collect()
}

/// `part:GetRootPart()`: the root of the part's assembly.
pub fn root_part(world: &World, part: InstanceId) -> InstanceId {
    let mut members = connected_parts(world, part, true);
    members.sort();
    root(world, &members)
}

/// How strongly a part claims to be its assembly's root, compared field by
/// field: anchored first, then not Massless, then the largest volume.
#[derive(Default, PartialEq, PartialOrd)]
struct Rank {
    anchored: bool,
    solid: bool,
    volume: f32,
}

impl Rank {
    fn of(part: &Instance) -> Self {
        let flag = |name| part.property(name).and_then(Value::as_bool) == Some(true);
        let volume = part
            .property("Size")
            .and_then(Value::as_vector3)
            .map(|size| (size.x * size.y * size.z).abs())
            .filter(|volume| volume.is_finite())
            .unwrap_or(0.0);
        Self {
            anchored: flag("Anchored"),
            solid: !flag("Massless"),
            volume,
        }
    }
}

fn is_part(instance: &Instance) -> bool {
    instance.class().is_base_part()
}
