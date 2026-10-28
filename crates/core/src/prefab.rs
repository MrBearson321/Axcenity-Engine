//! Prefab links (Phase 49): a placed copy of a prefab remembers the file it
//! came from and what it was made from (its *source*), so its overrides can
//! be worked out and it can follow the prefab when the file changes.
//!
//! Only the root of a copy holds a [`PrefabLink`]. Everything inside it is
//! matched to the source by [`Segment`]: a child's name, and which of the
//! children with that name it is (the third `Wheel` of a copy is the third
//! `Wheel` of its source). A copy's **overrides** are what differs from the
//! source: properties, attributes and tags of matched Instances, children
//! the copy added, and children of the source it removed. A child renamed
//! in a copy is removed and added, since names are how children match.
//!
//! [`sync`] rebuilds a copy in place from a (new) source while keeping its
//! overrides, so the same Instances stay selected and named in undo steps.
//! Prefabs nest: a source can hold copies of other prefabs, and a variant
//! is a prefab whose top-level Instance is a copy of its base.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use crate::value::Value;
use crate::world::{InstanceId, World};

/// What a prefab file holds, expanded: its top-level Instances (`roots`, in
/// file order) in the Workspace of a world of their own.
#[derive(Clone)]
pub struct PrefabSource {
    pub world: World,
    pub roots: Vec<InstanceId>,
}

impl std::fmt::Debug for PrefabSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PrefabSource({} roots)", self.roots.len())
    }
}

/// The root of a placed copy: which prefab, and what it was made from.
#[derive(Clone, Debug)]
pub struct PrefabLink {
    /// The prefab file, from the project folder (`assets/prefabs/Car.prefab.json`).
    pub path: String,
    /// Which of the prefab's top-level Instances this is a copy of.
    pub index: usize,
    /// The prefab as it was when the copy was last made or synced.
    pub source: Option<Arc<PrefabSource>>,
    /// Overrides read from a file, kept until the copy is first synced
    /// (afterwards they are worked out from `source`).
    pub pending: Option<Arc<Overrides>>,
}

impl PrefabLink {
    pub fn new(path: impl Into<String>, index: usize) -> Self {
        Self {
            path: path.into(),
            index,
            source: None,
            pending: None,
        }
    }

    /// The source's Instance this copy was made from.
    pub fn source_root(&self) -> Option<(&World, InstanceId)> {
        let source = self.source.as_ref()?;
        Some((&source.world, *source.roots.get(self.index)?))
    }
}

/// A child, by name and by which of its siblings with that name it is
/// (0 for the first).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Segment {
    pub name: String,
    pub nth: usize,
}

impl Segment {
    pub fn new(name: impl Into<String>, nth: usize) -> Self {
        Self {
            name: name.into(),
            nth,
        }
    }
}

/// What one Instance of a copy (and what's inside it) changes from its source.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Overrides {
    /// Properties set differently from the source.
    pub properties: BTreeSet<String>,
    /// The attributes are the copy's own.
    pub attributes: bool,
    /// The tags are the copy's own.
    pub tags: bool,
    /// Children of the source the copy doesn't have.
    pub removed: Vec<Segment>,
    /// Children the copy has that the source doesn't.
    pub added: Vec<Segment>,
    /// Overrides inside children the copy and source share.
    pub children: BTreeMap<Segment, Overrides>,
}

impl Overrides {
    pub fn is_empty(&self) -> bool {
        self.count() == 0
    }

    /// How many overrides there are, counting each property, the
    /// attributes, the tags and each added or removed child once.
    pub fn count(&self) -> usize {
        self.properties.len()
            + usize::from(self.attributes)
            + usize::from(self.tags)
            + self.removed.len()
            + self.added.len()
            + self.children.values().map(Overrides::count).sum::<usize>()
    }

    /// The overrides of the Instance at `path` below this one, if it has any.
    pub fn at(&self, path: &[Segment]) -> Option<&Overrides> {
        match path.split_first() {
            None => Some(self),
            Some((first, rest)) => self.children.get(first)?.at(rest),
        }
    }

    fn at_mut(&mut self, path: &[Segment]) -> &mut Overrides {
        match path.split_first() {
            None => self,
            Some((first, rest)) => self.children.entry(first.clone()).or_default().at_mut(rest),
        }
    }

    /// Drops children with nothing in them.
    pub fn prune(&mut self) {
        for child in self.children.values_mut() {
            child.prune();
        }
        self.children.retain(|_, child| !child.is_empty());
    }

    /// These overrides without the property `name` of the Instance at `path`.
    pub fn without_property(&self, path: &[Segment], name: &str) -> Overrides {
        let mut out = self.clone();
        out.at_mut(path).properties.remove(name);
        out.prune();
        out
    }

    /// Just the property `name` of the Instance at `path` (for applying
    /// that one override).
    pub fn only_property(path: &[Segment], name: &str) -> Overrides {
        let mut out = Overrides::default();
        out.at_mut(path).properties.insert(name.to_owned());
        out
    }

    /// Only the placement of parts (`Position` and `Orientation`), which
    /// Revert all keeps: every part of a copy moved into place has them.
    pub fn placement_only(&self) -> Overrides {
        Overrides {
            properties: self
                .properties
                .iter()
                .filter(|name| PLACEMENT.contains(&name.as_str()))
                .cloned()
                .collect(),
            children: self
                .children
                .iter()
                .map(|(segment, child)| (segment.clone(), child.placement_only()))
                .filter(|(_, child)| !child.is_empty())
                .collect(),
            ..Overrides::default()
        }
    }
}

/// The properties that place a part in the world.
pub const PLACEMENT: [&str; 2] = ["Position", "Orientation"];

/// The children of `id`, each with its [`Segment`].
pub fn child_segments(world: &World, id: InstanceId) -> Vec<(Segment, InstanceId)> {
    let Some(instance) = world.get(id) else {
        return Vec::new();
    };
    let mut seen: HashMap<&str, usize> = HashMap::new();
    instance
        .children()
        .iter()
        .filter_map(|&child| {
            let name = world.get(child)?.name();
            let nth = seen.entry(name).or_default();
            let segment = Segment::new(name, *nth);
            *nth += 1;
            Some((segment, child))
        })
        .collect()
}

/// The child of `id` at `segment`.
fn child_at(world: &World, id: InstanceId, segment: &Segment) -> Option<InstanceId> {
    child_segments(world, id)
        .into_iter()
        .find(|(s, _)| s == segment)
        .map(|(_, child)| child)
}

/// The Instance at `path` below `id`.
pub fn descend(world: &World, id: InstanceId, path: &[Segment]) -> Option<InstanceId> {
    path.iter()
        .try_fold(id, |at, segment| child_at(world, at, segment))
}

/// The overrides of the copy whose root is `root`: worked out from its
/// source, or the ones read from its file until it has been synced. Empty
/// if `root` isn't the root of a copy.
pub fn overrides(world: &World, root: InstanceId) -> Overrides {
    let Some(link) = world.get(root).and_then(|instance| instance.prefab()) else {
        return Overrides::default();
    };
    match link.source_root() {
        Some((source, source_root)) => diff(world, root, source, source_root),
        None => link.pending.as_deref().cloned().unwrap_or_default(),
    }
}

/// How `copy` (in `world`) differs from `original` (in `source`).
pub fn diff(world: &World, copy: InstanceId, source: &World, original: InstanceId) -> Overrides {
    let mut pairs = HashMap::new();
    pair_up(world, copy, source, original, &mut pairs);
    let mut out = diff_node(world, copy, source, original, &pairs);
    out.prune();
    out
}

/// The children of `copy` and `original` that match, by segment and class,
/// and the segments of those only one has (added, removed).
#[allow(clippy::type_complexity)]
fn match_children(
    world: &World,
    copy: InstanceId,
    source: &World,
    original: InstanceId,
) -> (
    Vec<(Segment, InstanceId, InstanceId)>,
    Vec<Segment>,
    Vec<Segment>,
) {
    let ours = child_segments(world, copy);
    let theirs = child_segments(source, original);
    let class = |w: &World, id| w.get(id).map(|i| i.class());
    let mut matched = Vec::new();
    let mut added = Vec::new();
    for (segment, child) in ours {
        match theirs.iter().find(|(s, _)| *s == segment) {
            Some(&(_, other)) if class(world, child) == class(source, other) => {
                matched.push((segment, child, other));
            }
            _ => added.push(segment),
        }
    }
    let removed = theirs
        .into_iter()
        .map(|(segment, _)| segment)
        .filter(|segment| !matched.iter().any(|(s, _, _)| s == segment))
        .collect();
    (matched, added, removed)
}

/// Maps every source Instance to the copy's Instance it matches.
fn pair_up(
    world: &World,
    copy: InstanceId,
    source: &World,
    original: InstanceId,
    pairs: &mut HashMap<InstanceId, InstanceId>,
) {
    pairs.insert(original, copy);
    for (_, ours, theirs) in match_children(world, copy, source, original).0 {
        pair_up(world, ours, source, theirs, pairs);
    }
}

fn diff_node(
    world: &World,
    copy: InstanceId,
    source: &World,
    original: InstanceId,
    pairs: &HashMap<InstanceId, InstanceId>,
) -> Overrides {
    let (Some(ours), Some(theirs)) = (world.get(copy), source.get(original)) else {
        return Overrides::default();
    };
    let mut out = Overrides::default();
    for (name, value) in theirs.properties() {
        let expected = translate(value, pairs);
        if ours.property(name) != Some(&expected) {
            out.properties.insert(name.to_owned());
        }
    }
    out.attributes = ours.attributes() != theirs.attributes();
    out.tags = ours.tags() != theirs.tags();
    let (matched, added, removed) = match_children(world, copy, source, original);
    out.added = added;
    out.removed = removed;
    for (segment, ours, theirs) in matched {
        out.children
            .insert(segment, diff_node(world, ours, source, theirs, pairs));
    }
    out
}

/// A source value as it is in the copy: a reference to an Instance inside
/// the source points at the copy's Instance in its place, or at nothing.
fn translate(value: &Value, pairs: &HashMap<InstanceId, InstanceId>) -> Value {
    match value {
        Value::Instance(target) => Value::Instance(target.and_then(|t| pairs.get(&t).copied())),
        value => value.clone(),
    }
}

/// Where an Instance is in a copy: the copy's root and the path to it.
#[derive(Clone, Debug)]
pub struct Place {
    pub root: InstanceId,
    pub path: Vec<Segment>,
    /// The whole copy's overrides.
    pub overrides: Overrides,
}

impl Place {
    /// The overrides of the Instance itself (empty when it has none).
    pub fn here(&self) -> Overrides {
        self.overrides
            .at(&self.path)
            .map(|here| Overrides {
                children: BTreeMap::new(),
                ..here.clone()
            })
            .unwrap_or_default()
    }

    /// The Instance of the source this one was made from.
    pub fn original<'a>(&self, world: &'a World) -> Option<(&'a World, InstanceId)> {
        let link = world.get(self.root)?.prefab()?;
        let (source, root) = link.source_root()?;
        Some((source, descend(source, root, &self.path)?))
    }
}

/// The copy `id` is part of, if any: the outermost one, unless `id` was
/// added to it (then the copy it is in inside that, if any).
pub fn place(world: &World, id: InstanceId) -> Option<Place> {
    let mut chain = vec![id];
    let mut at = id;
    while let Some(parent) = world.get(at)?.parent() {
        chain.push(parent);
        at = parent;
    }
    chain.reverse();
    let mut owner: Option<Place> = None;
    for &node in &chain {
        if let Some(place) = owner.as_mut() {
            let segment = segment_of(world, node)?;
            let added = place
                .overrides
                .at(&place.path)
                .is_some_and(|here| here.added.contains(&segment));
            if !added {
                place.path.push(segment);
                continue;
            }
            owner = None;
        }
        if world.get(node)?.prefab().is_some() {
            owner = Some(Place {
                root: node,
                path: Vec::new(),
                overrides: overrides(world, node),
            });
        }
    }
    owner
}

/// `id`'s segment among its parent's children.
pub fn segment_of(world: &World, id: InstanceId) -> Option<Segment> {
    let parent = world.get(id)?.parent()?;
    child_segments(world, parent)
        .into_iter()
        .find(|&(_, child)| child == id)
        .map(|(segment, _)| segment)
}

/// The Instances added to the copy `root` (not what's inside them), which
/// are copies of their own when they hold a prefab link.
pub fn added_children(world: &World, root: InstanceId) -> Vec<InstanceId> {
    fn walk(
        world: &World,
        id: InstanceId,
        overrides: Option<&Overrides>,
        out: &mut Vec<InstanceId>,
    ) {
        for (segment, child) in child_segments(world, id) {
            match overrides {
                Some(here) if here.added.contains(&segment) => out.push(child),
                _ => walk(
                    world,
                    child,
                    overrides.and_then(|o| o.children.get(&segment)),
                    out,
                ),
            }
        }
    }
    let overrides = overrides(world, root);
    let mut out = Vec::new();
    walk(world, root, Some(&overrides), &mut out);
    out
}

/// Makes the copy rooted at `root` match `source`'s top-level Instance
/// `index`, keeping `overrides`, and links it to that source. Instances
/// that match keep their IDs; children the source gained are copied in
/// (at the end) and those it lost are destroyed. The root keeps its name.
/// Does nothing if the source has no such Instance.
pub fn sync(
    world: &mut World,
    root: InstanceId,
    source: &Arc<PrefabSource>,
    index: usize,
    overrides: &Overrides,
) {
    let Some(&original) = source.roots.get(index) else {
        return;
    };
    let mut pairs = HashMap::new();
    let mut nodes = Vec::new();
    sync_tree(
        world,
        root,
        &source.world,
        original,
        Some(overrides),
        &mut pairs,
        &mut nodes,
    );
    for (copy, theirs, here) in nodes {
        sync_values(
            world,
            copy,
            &source.world,
            theirs,
            here,
            &pairs,
            copy == root,
        );
    }
    let path = world
        .get(root)
        .and_then(|instance| instance.prefab())
        .map(|link| link.path.clone())
        .unwrap_or_default();
    let mut link = PrefabLink::new(path, index);
    link.source = Some(source.clone());
    let _ = world.set_prefab(root, Some(link));
}

/// Matches the tree of `copy` to `original`'s, making and destroying
/// children as needed, and lists each pair to have its values synced.
fn sync_tree<'a>(
    world: &mut World,
    copy: InstanceId,
    source: &World,
    original: InstanceId,
    overrides: Option<&'a Overrides>,
    pairs: &mut HashMap<InstanceId, InstanceId>,
    nodes: &mut Vec<(InstanceId, InstanceId, Option<&'a Overrides>)>,
) {
    pairs.insert(original, copy);
    nodes.push((copy, original, overrides));
    let added: &[Segment] = overrides.map_or(&[], |o| &o.added);
    let removed: &[Segment] = overrides.map_or(&[], |o| &o.removed);
    // The copy's own children, counted again without the added ones.
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut kept: Vec<(Segment, InstanceId)> = child_segments(world, copy)
        .into_iter()
        .filter(|(segment, _)| !added.contains(segment))
        .map(|(segment, child)| {
            let nth = seen.entry(segment.name.clone()).or_default();
            let segment = Segment::new(segment.name, *nth);
            *nth += 1;
            (segment, child)
        })
        .collect();
    for (segment, theirs) in child_segments(source, original) {
        if removed.contains(&segment) {
            continue;
        }
        let class = source.get(theirs).map(|i| i.class());
        let existing = kept
            .iter()
            .position(|(s, id)| *s == segment && world.get(*id).map(|i| i.class()) == class);
        match existing {
            Some(at) => {
                let (_, ours) = kept.remove(at);
                let inner = overrides.and_then(|o| o.children.get(&segment));
                sync_tree(world, ours, source, theirs, inner, pairs, nodes);
            }
            None => {
                let Ok(made) = world.copy_from(source, theirs) else {
                    continue;
                };
                let _ = world.set_parent(made, Some(copy));
                let originals = std::iter::once(theirs).chain(source.descendants(theirs));
                let copies = std::iter::once(made).chain(world.descendants(made));
                for (theirs, ours) in originals.zip(copies) {
                    pairs.insert(theirs, ours);
                    nodes.push((ours, theirs, None));
                }
            }
        }
    }
    // Gone from the source.
    for (_, child) in kept {
        let _ = world.destroy(child);
    }
}

fn sync_values(
    world: &mut World,
    copy: InstanceId,
    source: &World,
    original: InstanceId,
    overrides: Option<&Overrides>,
    pairs: &HashMap<InstanceId, InstanceId>,
    is_root: bool,
) {
    let Some(theirs) = source.get(original) else {
        return;
    };
    let empty = BTreeSet::new();
    let own = overrides.map_or(&empty, |o| &o.properties);
    // The copy's own values, put back after: setting `Material` resets
    // `Metalness` and `Roughness`.
    let kept: Vec<(String, Value)> = world.get(copy).map_or_else(Vec::new, |ours| {
        own.iter()
            .filter_map(|name| Some((name.clone(), ours.property(name)?.clone())))
            .collect()
    });
    let mut values: Vec<(&'static str, Value)> = theirs
        .properties()
        .filter(|(name, _)| !own.contains(*name))
        .map(|(name, value)| (name, translate(value, pairs)))
        .collect();
    values.sort_by_key(|(name, _)| *name != "Material");
    for (name, value) in values {
        if world.get(copy).and_then(|i| i.property(name)) != Some(&value) {
            let _ = world.set_property(copy, name, value);
        }
    }
    for (name, value) in kept {
        if world.get(copy).and_then(|i| i.property(&name)) != Some(&value) {
            let _ = world.set_property(copy, &name, value);
        }
    }
    if !overrides.is_some_and(|o| o.attributes) {
        let ours: Vec<String> = world
            .get(copy)
            .map(|i| i.attributes().keys().cloned().collect())
            .unwrap_or_default();
        for name in ours {
            if theirs.attribute(&name).is_none() {
                let _ = world.set_attribute(copy, &name, None);
            }
        }
        for (name, value) in theirs.attributes() {
            let _ = world.set_attribute(copy, name, Some(value.clone()));
        }
    }
    if !overrides.is_some_and(|o| o.tags)
        && world.get(copy).is_some_and(|i| i.tags() != theirs.tags())
    {
        let ours: Vec<String> = world
            .get(copy)
            .map(|i| i.tags().to_vec())
            .unwrap_or_default();
        for tag in ours {
            let _ = world.remove_tag(copy, &tag);
        }
        for tag in theirs.tags() {
            let _ = world.add_tag(copy, tag);
        }
    }
    // Copies nested inside follow the source's links.
    if !is_root {
        let _ = world.set_prefab(copy, theirs.prefab().cloned());
    }
}

#[cfg(test)]
mod tests;
