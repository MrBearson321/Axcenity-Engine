//! The Instance tree: `game` at the root, services below it, and everything
//! the game is made of below those.
//!
//! Instances never point at each other directly. Each one stores the
//! [`InstanceId`]s of its parent and children, and all access goes through
//! [`World`], which owns every Instance in an [`Arena`].

use std::collections::{BTreeMap, HashMap};

use crate::arena::{Arena, Id};
use crate::class::ClassName;
use crate::value::{Value, ValueKind};

/// A handle to an Instance in a [`World`]. It stays valid until the Instance
/// is destroyed; after that every lookup with it returns `None` or an error.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct InstanceId(Id);

#[derive(Debug, Clone)]
pub struct Instance {
    class: ClassName,
    name: String,
    parent: Option<InstanceId>,
    children: Vec<InstanceId>,
    properties: BTreeMap<&'static str, Value>,
}

impl Instance {
    fn new(class: ClassName) -> Self {
        Self {
            class,
            name: class.as_str().to_owned(),
            parent: None,
            children: Vec::new(),
            properties: class.default_properties().into_iter().collect(),
        }
    }

    pub fn class(&self) -> ClassName {
        self.class
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn parent(&self) -> Option<InstanceId> {
        self.parent
    }

    /// Children in the order they were added.
    pub fn children(&self) -> &[InstanceId] {
        &self.children
    }

    pub fn property(&self, name: &str) -> Option<&Value> {
        self.properties.get(name)
    }

    /// All properties, sorted by name.
    pub fn properties(&self) -> impl Iterator<Item = (&'static str, &Value)> {
        self.properties.iter().map(|(name, value)| (*name, value))
    }
}

#[derive(Debug, PartialEq, thiserror::Error)]
pub enum TreeError {
    #[error("the Instance no longer exists")]
    Stale,
    #[error("{0} is a service, so it can't be created, moved or destroyed")]
    Service(ClassName),
    #[error("an Instance can't be parented to itself or to one of its descendants")]
    Cycle,
}

#[derive(Debug, PartialEq, thiserror::Error)]
pub enum PropertyError {
    #[error("the Instance no longer exists")]
    Stale,
    #[error("{class} has no property named `{name}`")]
    Unknown { class: ClassName, name: String },
    #[error("{class}.{name} must be a {expected}, not a {actual}")]
    WrongType {
        class: ClassName,
        name: &'static str,
        expected: ValueKind,
        actual: ValueKind,
    },
}

/// Something that changed in the tree. Scripts hear about these through
/// `Changed`, `GetPropertyChangedSignal`, `ChildAdded` and `ChildRemoved`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    /// A property of `instance` got a different value. `Name` and `Parent`
    /// count as properties here.
    Property {
        instance: InstanceId,
        name: &'static str,
    },
    /// `child` was put inside `parent`.
    ChildAdded {
        parent: InstanceId,
        child: InstanceId,
    },
    /// `child` was taken out of `parent`.
    ChildRemoved {
        parent: InstanceId,
        child: InstanceId,
    },
}

/// A [`Change`] with its number. Changes are numbered 0, 1, 2, ... in the
/// order they happened, so a script that starts listening after change 5
/// can skip changes 0 to 5.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecordedChange {
    pub number: u64,
    pub change: Change,
}

/// The changes since the last [`World::take_changes`]. Off until
/// [`World::record_changes`], so a world that no script runs in (the one
/// the editor edits) doesn't collect changes nobody reads.
#[derive(Clone, Default)]
struct ChangeLog {
    recording: bool,
    next_number: u64,
    changes: Vec<RecordedChange>,
}

/// `clone()` makes a deep copy: a whole separate tree, with every Instance at
/// the same `InstanceId` as in the original. Play mode runs on such a copy, so
/// IDs (the selection, open Explorer branches) mean the same thing in both.
#[derive(Clone)]
pub struct World {
    instances: Arena<Instance>,
    root: InstanceId,
    workspace: InstanceId,
    players: InstanceId,
    replicated_storage: InstanceId,
    server_script_service: InstanceId,
    starter_player_scripts: InstanceId,
    changes: ChangeLog,
}

impl World {
    /// Creates a world with `game` and its services, in the order the
    /// Explorer shows them: `Workspace`, `Players`, `ReplicatedStorage`,
    /// `ServerScriptService` and `StarterPlayer` (with
    /// `StarterPlayerScripts` inside).
    pub fn new() -> Self {
        let mut instances = Arena::new();
        let mut game = Instance::new(ClassName::DataModel);
        game.name = "Game".to_owned();
        let root = InstanceId(instances.insert(game));

        // A closure that adds one service under `parent`. It borrows
        // `instances` mutably, so `instances` can't be used directly until
        // its last call.
        let mut add_service = |class, parent: InstanceId| {
            let mut service = Instance::new(class);
            service.parent = Some(parent);
            let id = InstanceId(instances.insert(service));
            instances
                .get_mut(parent.0)
                .expect("parent was inserted first")
                .children
                .push(id);
            id
        };
        let workspace = add_service(ClassName::Workspace, root);
        let players = add_service(ClassName::Players, root);
        let replicated_storage = add_service(ClassName::ReplicatedStorage, root);
        let server_script_service = add_service(ClassName::ServerScriptService, root);
        let starter_player = add_service(ClassName::StarterPlayer, root);
        let starter_player_scripts = add_service(ClassName::StarterPlayerScripts, starter_player);

        Self {
            instances,
            root,
            workspace,
            players,
            replicated_storage,
            server_script_service,
            starter_player_scripts,
            changes: ChangeLog::default(),
        }
    }

    /// Starts keeping a list of every [`Change`] from now on, for
    /// [`take_changes`](Self::take_changes). Calling it again does nothing.
    pub fn record_changes(&mut self) {
        self.changes.recording = true;
    }

    /// The number the next change will get. Something that starts listening
    /// now should only hear about changes numbered this or higher.
    pub fn next_change_number(&self) -> u64 {
        self.changes.next_number
    }

    /// True if changes were recorded since the last [`take_changes`](Self::take_changes).
    pub fn has_changes(&self) -> bool {
        !self.changes.changes.is_empty()
    }

    /// Removes and returns the changes recorded since the last call, oldest first.
    pub fn take_changes(&mut self) -> Vec<RecordedChange> {
        std::mem::take(&mut self.changes.changes)
    }

    /// Notes `change`, if changes are being recorded.
    fn changed(&mut self, change: Change) {
        let log = &mut self.changes;
        if log.recording {
            log.changes.push(RecordedChange {
                number: log.next_number,
                change,
            });
            log.next_number += 1;
        }
    }

    /// The root Instance, called `game` in scripts.
    pub fn root(&self) -> InstanceId {
        self.root
    }

    pub fn workspace(&self) -> InstanceId {
        self.workspace
    }

    /// Holds a Player for everyone in the game (only during play).
    pub fn players(&self) -> InstanceId {
        self.players
    }

    /// Holds what Scripts and LocalScripts share, such as RemoteEvents.
    pub fn replicated_storage(&self) -> InstanceId {
        self.replicated_storage
    }

    pub fn server_script_service(&self) -> InstanceId {
        self.server_script_service
    }

    /// `StarterPlayer.StarterPlayerScripts`, which holds the LocalScripts.
    pub fn starter_player_scripts(&self) -> InstanceId {
        self.starter_player_scripts
    }

    /// The service (or `game` itself) of class `class`, if `class` is one.
    pub fn service(&self, class: ClassName) -> Option<InstanceId> {
        let id = match class {
            ClassName::DataModel => self.root,
            ClassName::Workspace => self.workspace,
            ClassName::Players => self.players,
            ClassName::ReplicatedStorage => self.replicated_storage,
            ClassName::ServerScriptService => self.server_script_service,
            ClassName::StarterPlayer => self.get(self.starter_player_scripts)?.parent?,
            ClassName::StarterPlayerScripts => self.starter_player_scripts,
            _ => return None,
        };
        Some(id)
    }

    pub fn get(&self, id: InstanceId) -> Option<&Instance> {
        self.instances.get(id.0)
    }

    pub fn contains(&self, id: InstanceId) -> bool {
        self.instances.contains(id.0)
    }

    /// The number of Instances alive, including ones with no parent.
    pub fn len(&self) -> usize {
        self.instances.len()
    }

    pub fn is_empty(&self) -> bool {
        self.instances.is_empty()
    }

    /// Creates an Instance with default properties and no parent, like
    /// `Instance.new` in scripts. Its name starts as the class name.
    pub fn create(&mut self, class: ClassName) -> Result<InstanceId, TreeError> {
        if class.is_service() {
            return Err(TreeError::Service(class));
        }
        Ok(InstanceId(self.instances.insert(Instance::new(class))))
    }

    /// Moves `id` under `new_parent` as its last child, or detaches it when
    /// `new_parent` is `None`.
    pub fn set_parent(
        &mut self,
        id: InstanceId,
        new_parent: Option<InstanceId>,
    ) -> Result<(), TreeError> {
        self.reparent(id, new_parent.map(|parent| (parent, usize::MAX)))
    }

    /// Moves `id` under `new_parent` so that it ends up as child number
    /// `index` (0 is the first), with the children from there on moving down
    /// one. An `index` past the end makes it the last child. Undo uses this
    /// to put an Instance back exactly where it was.
    pub fn set_parent_at(
        &mut self,
        id: InstanceId,
        new_parent: InstanceId,
        index: usize,
    ) -> Result<(), TreeError> {
        self.reparent(id, Some((new_parent, index)))
    }

    /// `set_parent` and `set_parent_at`: `new_parent` is the parent and the
    /// index among its children, or `None` to detach.
    fn reparent(
        &mut self,
        id: InstanceId,
        new_parent: Option<(InstanceId, usize)>,
    ) -> Result<(), TreeError> {
        let instance = self.get(id).ok_or(TreeError::Stale)?;
        if instance.class.is_service() {
            return Err(TreeError::Service(instance.class));
        }
        let old_parent = instance.parent;

        if let Some((new_parent, _)) = new_parent {
            if !self.contains(new_parent) {
                return Err(TreeError::Stale);
            }
            if self.is_ancestor_or_self(id, new_parent) {
                return Err(TreeError::Cycle);
            }
        }

        // Each `get_mut` borrow ends at the end of its line, so the three
        // updates below don't conflict with each other.
        if let Some(old_parent) = old_parent
            && let Some(old) = self.instances.get_mut(old_parent.0)
        {
            old.children.retain(|&child| child != id);
        }
        if let Some((new_parent, index)) = new_parent {
            let parent = self.instances.get_mut(new_parent.0).expect("checked above");
            let index = index.min(parent.children.len());
            parent.children.insert(index, id);
        }
        let new_parent = new_parent.map(|(parent, _)| parent);
        self.instances.get_mut(id.0).expect("checked above").parent = new_parent;

        // Moving among the same parent's children isn't a change of Parent.
        if old_parent != new_parent {
            if let Some(parent) = old_parent {
                self.changed(Change::ChildRemoved { parent, child: id });
            }
            if let Some(parent) = new_parent {
                self.changed(Change::ChildAdded { parent, child: id });
            }
            self.changed(Change::Property {
                instance: id,
                name: "Parent",
            });
        }
        Ok(())
    }

    /// Makes a copy of `id` and everything inside it, with the same classes,
    /// names and properties, and returns the copy. The copy has no parent yet.
    pub fn duplicate(&mut self, id: InstanceId) -> Result<InstanceId, TreeError> {
        let original = self.get(id).ok_or(TreeError::Stale)?;
        if original.class.is_service() {
            return Err(TreeError::Service(original.class));
        }
        let mut copy = original.clone();
        copy.parent = None;
        copy.children.clear();
        let root = InstanceId(self.instances.insert(copy));

        // `descendants` lists parents before their children, so each
        // descendant's parent has already been copied when it's reached.
        let mut copies = HashMap::from([(id, root)]);
        for old in self.descendants(id) {
            let mut copy = self.get(old).expect("descendants are alive").clone();
            let parent = copies[&copy.parent.expect("a descendant has a parent")];
            copy.parent = Some(parent);
            copy.children.clear();
            let new = InstanceId(self.instances.insert(copy));
            self.instances
                .get_mut(parent.0)
                .expect("copied above")
                .children
                .push(new);
            copies.insert(old, new);
        }
        Ok(root)
    }

    /// Destroys `id` and all of its descendants. Their IDs become stale.
    pub fn destroy(&mut self, id: InstanceId) -> Result<(), TreeError> {
        let instance = self.get(id).ok_or(TreeError::Stale)?;
        if instance.class.is_service() {
            return Err(TreeError::Service(instance.class));
        }

        self.set_parent(id, None)?;
        for descendant in self.descendants(id) {
            self.instances.remove(descendant.0);
        }
        self.instances.remove(id.0);
        Ok(())
    }

    pub fn set_name(&mut self, id: InstanceId, name: impl Into<String>) -> Result<(), TreeError> {
        let instance = self.instances.get_mut(id.0).ok_or(TreeError::Stale)?;
        let name = name.into();
        if instance.name != name {
            instance.name = name;
            self.changed(Change::Property {
                instance: id,
                name: "Name",
            });
        }
        Ok(())
    }

    /// Sets an existing property. The new value must have the same type as the old one.
    /// Setting the value it already has isn't a [`Change`].
    ///
    /// Setting `Material` also resets `Metalness` and `Roughness` to the
    /// material's preset, so set those afterwards to adjust them.
    pub fn set_property(
        &mut self,
        id: InstanceId,
        name: &str,
        value: Value,
    ) -> Result<(), PropertyError> {
        let instance = self.instances.get_mut(id.0).ok_or(PropertyError::Stale)?;
        let class = instance.class;
        // `get_key_value` also returns the stored `&'static str` key, which the
        // error needs because `name` only lives as long as this call.
        let Some((&key, _)) = instance.properties.get_key_value(name) else {
            return Err(PropertyError::Unknown {
                class,
                name: name.to_owned(),
            });
        };
        let slot = instance
            .properties
            .get_mut(key)
            .expect("key was just found");
        if slot.kind() != value.kind() {
            return Err(PropertyError::WrongType {
                class,
                name: key,
                expected: slot.kind(),
                actual: value.kind(),
            });
        }
        // The properties that really change, for the change log.
        let mut changed = Vec::new();
        if *slot != value {
            changed.push(key);
        }
        // Read before `value` moves into the slot below.
        let material = value.as_material();
        *slot = value;

        if let Some(material) = material {
            let preset = material.preset();
            for (name, number) in [
                ("Metalness", preset.metalness),
                ("Roughness", preset.roughness),
            ] {
                if let Some(slot) = instance.properties.get_mut(name)
                    && *slot != Value::Number(number)
                {
                    *slot = Value::Number(number);
                    changed.push(name);
                }
            }
        }
        for name in changed {
            self.changed(Change::Property { instance: id, name });
        }
        Ok(())
    }

    /// The first child of `parent` with this name, in child order.
    pub fn find_first_child(&self, parent: InstanceId, name: &str) -> Option<InstanceId> {
        let parent = self.get(parent)?;
        parent
            .children
            .iter()
            .copied()
            .find(|&child| self.get(child).is_some_and(|c| c.name == name))
    }

    /// The first descendant of `ancestor` with this name, in [`descendants`](Self::descendants) order.
    pub fn find_first_descendant(&self, ancestor: InstanceId, name: &str) -> Option<InstanceId> {
        self.descendants(ancestor)
            .into_iter()
            .find(|&id| self.get(id).is_some_and(|instance| instance.name == name))
    }

    /// Every Instance below `id`, parents before their children.
    /// Empty if `id` is stale.
    pub fn descendants(&self, id: InstanceId) -> Vec<InstanceId> {
        let mut found = Vec::new();
        // An explicit stack instead of recursion, so deep trees can't overflow it.
        let mut stack: Vec<InstanceId> = match self.get(id) {
            Some(instance) => instance.children.iter().rev().copied().collect(),
            None => return found,
        };
        while let Some(next) = stack.pop() {
            if let Some(instance) = self.get(next) {
                found.push(next);
                stack.extend(instance.children.iter().rev().copied());
            }
        }
        found
    }

    /// A dotted path such as `Workspace.Block`, for messages. `game` itself is left out.
    pub fn full_name(&self, id: InstanceId) -> Option<String> {
        let mut names = Vec::new();
        let mut current = Some(id);
        while let Some(next) = current {
            if next == self.root {
                break;
            }
            let instance = self.get(next)?;
            names.push(instance.name.as_str());
            current = instance.parent;
        }
        names.reverse();
        Some(names.join("."))
    }

    /// True if `id` is somewhere inside `ancestor` (not `ancestor` itself).
    /// Detached Instances are inside nothing.
    pub fn is_descendant_of(&self, id: InstanceId, ancestor: InstanceId) -> bool {
        id != ancestor && self.is_ancestor_or_self(ancestor, id)
    }

    /// True if `ancestor` is `id` itself or any Instance above it.
    fn is_ancestor_or_self(&self, ancestor: InstanceId, id: InstanceId) -> bool {
        let mut current = Some(id);
        while let Some(next) = current {
            if next == ancestor {
                return true;
            }
            current = self.get(next).and_then(|instance| instance.parent);
        }
        false
    }
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::enums::Material;
    use crate::value::Vector3;

    fn part_in(world: &mut World, parent: InstanceId, name: &str) -> InstanceId {
        let part = world.create(ClassName::Part).unwrap();
        world.set_name(part, name).unwrap();
        world.set_parent(part, Some(parent)).unwrap();
        part
    }

    #[test]
    fn a_cloned_world_has_the_same_ids_and_changes_to_it_stay_in_it() {
        let mut world = World::new();
        let workspace = world.workspace();
        let pad = part_in(&mut world, workspace, "Pad");
        let removed = part_in(&mut world, workspace, "Gone");
        world.destroy(removed).unwrap();

        let mut copy = world.clone();
        assert_eq!(copy.find_first_child(workspace, "Pad"), Some(pad));
        assert!(
            !copy.contains(removed),
            "a destroyed ID stays dead in the copy"
        );
        copy.set_name(pad, "Changed").unwrap();
        copy.destroy(pad).unwrap();
        part_in(&mut copy, workspace, "New");

        assert_eq!(world.get(pad).unwrap().name(), "Pad");
        assert_eq!(world.get(workspace).unwrap().children(), &[pad]);
        // Something made only in the copy isn't in the original.
        let new = copy.find_first_child(workspace, "New").unwrap();
        assert!(!world.contains(new));
    }

    #[test]
    fn new_world_has_game_and_its_services() {
        let world = World::new();
        let game = world.get(world.root()).unwrap();
        assert_eq!(game.class(), ClassName::DataModel);
        let starter_player = world.service(ClassName::StarterPlayer).unwrap();
        assert_eq!(
            game.children(),
            &[
                world.workspace(),
                world.players(),
                world.replicated_storage(),
                world.server_script_service(),
                starter_player,
            ]
        );
        assert_eq!(
            world.get(starter_player).unwrap().children(),
            &[world.starter_player_scripts()]
        );
        for class in ClassName::ALL {
            let service = world.service(class);
            assert_eq!(service.is_some(), class.is_service(), "{class}");
            if let Some(id) = service {
                assert_eq!(world.get(id).unwrap().class(), class);
            }
        }
        let workspace = world.get(world.workspace()).unwrap();
        assert_eq!(workspace.parent(), Some(world.root()));
        let scripts = world.get(world.server_script_service()).unwrap();
        assert_eq!(scripts.class(), ClassName::ServerScriptService);
    }

    #[test]
    fn create_gives_defaults_and_no_parent() {
        let mut world = World::new();
        let part = world.create(ClassName::Part).unwrap();
        let instance = world.get(part).unwrap();
        assert_eq!(instance.name(), "Part");
        assert_eq!(instance.parent(), None);
        assert_eq!(
            instance.property("Size"),
            Some(&Value::Vector3(Vector3::new(4.0, 1.0, 2.0)))
        );
    }

    #[test]
    fn services_cannot_be_created_moved_or_destroyed() {
        let mut world = World::new();
        let workspace = world.workspace();
        assert_eq!(
            world.create(ClassName::Workspace),
            Err(TreeError::Service(ClassName::Workspace))
        );
        assert!(world.set_parent(workspace, None).is_err());
        assert!(world.destroy(workspace).is_err());
        assert!(world.destroy(world.root()).is_err());
        assert!(world.destroy(world.server_script_service()).is_err());
    }

    #[test]
    fn find_first_descendant_searches_the_whole_subtree() {
        let mut world = World::new();
        let workspace = world.workspace();
        let a = part_in(&mut world, workspace, "A");
        let deep = part_in(&mut world, a, "Target");
        assert_eq!(world.find_first_child(workspace, "Target"), None);
        assert_eq!(world.find_first_descendant(workspace, "Target"), Some(deep));
    }

    #[test]
    fn parent_links_both_ways() {
        let mut world = World::new();
        let workspace = world.workspace();
        let part = part_in(&mut world, workspace, "Block");
        assert_eq!(world.get(part).unwrap().parent(), Some(workspace));
        assert_eq!(world.get(workspace).unwrap().children(), &[part]);
        assert_eq!(world.find_first_child(workspace, "Block"), Some(part));
        assert_eq!(world.full_name(part).as_deref(), Some("Workspace.Block"));
    }

    #[test]
    fn reparent_moves_between_parents() {
        let mut world = World::new();
        let workspace = world.workspace();
        let a = part_in(&mut world, workspace, "A");
        let b = part_in(&mut world, workspace, "B");

        world.set_parent(b, Some(a)).unwrap();
        assert_eq!(world.get(workspace).unwrap().children(), &[a]);
        assert_eq!(world.get(a).unwrap().children(), &[b]);
        assert_eq!(world.get(b).unwrap().parent(), Some(a));

        world.set_parent(b, None).unwrap();
        assert!(world.get(a).unwrap().children().is_empty());
        assert_eq!(world.get(b).unwrap().parent(), None);
    }

    #[test]
    fn reparent_into_own_descendant_is_rejected() {
        let mut world = World::new();
        let workspace = world.workspace();
        let a = part_in(&mut world, workspace, "A");
        let b = part_in(&mut world, a, "B");
        assert_eq!(world.set_parent(a, Some(b)), Err(TreeError::Cycle));
        assert_eq!(world.set_parent(a, Some(a)), Err(TreeError::Cycle));
        // Nothing changed.
        assert_eq!(world.get(a).unwrap().parent(), Some(workspace));
    }

    #[test]
    fn set_parent_at_puts_it_at_that_index() {
        let mut world = World::new();
        let workspace = world.workspace();
        let a = part_in(&mut world, workspace, "A");
        let b = part_in(&mut world, workspace, "B");
        let c = part_in(&mut world, workspace, "C");

        // Within the same parent: C moves to the front.
        world.set_parent_at(c, workspace, 0).unwrap();
        assert_eq!(world.get(workspace).unwrap().children(), &[c, a, b]);
        // Into another parent, past the end: it goes last.
        world.set_parent_at(b, a, 99).unwrap();
        assert_eq!(world.get(a).unwrap().children(), &[b]);
        // Detached, then back in the middle.
        world.set_parent(c, None).unwrap();
        world.set_parent_at(c, workspace, 1).unwrap();
        assert_eq!(world.get(workspace).unwrap().children(), &[a, c]);
        assert_eq!(world.set_parent_at(a, b, 0), Err(TreeError::Cycle));
    }

    #[test]
    fn duplicate_copies_the_whole_subtree_without_a_parent() {
        let mut world = World::new();
        let workspace = world.workspace();
        let model = world.create(ClassName::Model).unwrap();
        world.set_name(model, "Car").unwrap();
        world.set_parent(model, Some(workspace)).unwrap();
        let wheel = part_in(&mut world, model, "Wheel");
        let hub = part_in(&mut world, wheel, "Hub");
        world
            .set_property(wheel, "Metalness", Value::Number(0.7))
            .unwrap();

        let copy = world.duplicate(model).unwrap();
        let instance = world.get(copy).unwrap();
        assert_eq!(
            (instance.class(), instance.name()),
            (ClassName::Model, "Car")
        );
        assert_eq!(instance.parent(), None);
        let copied_wheel = instance.children()[0];
        assert_ne!(copied_wheel, wheel);
        let copied_wheel = world.get(copied_wheel).unwrap();
        assert_eq!(copied_wheel.name(), "Wheel");
        assert_eq!(copied_wheel.parent(), Some(copy));
        assert_eq!(
            copied_wheel.property("Metalness"),
            Some(&Value::Number(0.7))
        );
        let copied_hub = world.get(copied_wheel.children()[0]).unwrap();
        assert_eq!(copied_hub.name(), "Hub");
        // The original is untouched.
        assert_eq!(world.get(wheel).unwrap().children(), &[hub]);
        assert_eq!(world.get(workspace).unwrap().children(), &[model]);
        assert!(world.duplicate(workspace).is_err());
    }

    #[test]
    fn is_descendant_of_follows_parents_up() {
        let mut world = World::new();
        let workspace = world.workspace();
        let a = part_in(&mut world, workspace, "A");
        let b = part_in(&mut world, a, "B");
        assert!(world.is_descendant_of(b, world.root()));
        assert!(world.is_descendant_of(b, a));
        assert!(!world.is_descendant_of(a, b));
        assert!(!world.is_descendant_of(a, a));
        world.set_parent(a, None).unwrap();
        assert!(!world.is_descendant_of(b, world.root()));
    }

    #[test]
    fn destroy_removes_descendants_and_makes_ids_stale() {
        let mut world = World::new();
        let workspace = world.workspace();
        let a = part_in(&mut world, workspace, "A");
        let b = part_in(&mut world, a, "B");
        let c = part_in(&mut world, b, "C");
        let before = world.len();

        world.destroy(a).unwrap();
        assert_eq!(world.len(), before - 3);
        assert!(world.get(workspace).unwrap().children().is_empty());
        for id in [a, b, c] {
            assert!(world.get(id).is_none());
            assert_eq!(world.set_parent(id, None), Err(TreeError::Stale));
            assert_eq!(world.destroy(id), Err(TreeError::Stale));
            assert_eq!(
                world.set_property(id, "Metalness", Value::Number(1.0)),
                Err(PropertyError::Stale)
            );
        }
    }

    #[test]
    fn stale_id_stays_stale_after_its_slot_is_reused() {
        let mut world = World::new();
        let old = world.create(ClassName::Part).unwrap();
        world.destroy(old).unwrap();
        let new = world.create(ClassName::Part).unwrap();
        assert_ne!(old, new);
        assert!(world.get(old).is_none());
        assert!(world.get(new).is_some());
    }

    #[test]
    fn descendants_are_parents_first_in_child_order() {
        let mut world = World::new();
        let workspace = world.workspace();
        let a = part_in(&mut world, workspace, "A");
        let a1 = part_in(&mut world, a, "A1");
        let b = part_in(&mut world, workspace, "B");
        assert_eq!(world.descendants(workspace), vec![a, a1, b]);
    }

    #[test]
    fn set_property_checks_name_and_type() {
        let mut world = World::new();
        let part = world.create(ClassName::Part).unwrap();

        world
            .set_property(part, "Metalness", Value::Number(1.0))
            .unwrap();
        assert_eq!(
            world.get(part).unwrap().property("Metalness"),
            Some(&Value::Number(1.0))
        );

        assert!(matches!(
            world.set_property(part, "Shininess", Value::Number(1.0)),
            Err(PropertyError::Unknown { .. })
        ));
        assert!(matches!(
            world.set_property(part, "Metalness", Value::Bool(true)),
            Err(PropertyError::WrongType { .. })
        ));
    }

    #[test]
    fn setting_material_applies_its_preset_which_can_then_be_adjusted() {
        let mut world = World::new();
        let part = world.create(ClassName::Part).unwrap();
        let number = |world: &World, name| world.get(part).unwrap().property(name).cloned();

        world
            .set_property(part, "Material", Value::Material(Material::Metal))
            .unwrap();
        assert_eq!(number(&world, "Metalness"), Some(Value::Number(1.0)));
        assert_eq!(number(&world, "Roughness"), Some(Value::Number(0.3)));

        world
            .set_property(part, "Roughness", Value::Number(0.8))
            .unwrap();
        assert_eq!(number(&world, "Roughness"), Some(Value::Number(0.8)));
        assert_eq!(
            number(&world, "Material"),
            Some(Value::Material(Material::Metal))
        );
    }

    #[test]
    fn default_part_matches_the_plastic_preset() {
        let mut world = World::new();
        let part = world.create(ClassName::Part).unwrap();
        let instance = world.get(part).unwrap();
        let preset = Material::Plastic.preset();
        assert_eq!(
            instance.property("Material"),
            Some(&Value::Material(Material::Plastic))
        );
        assert_eq!(
            instance.property("Metalness"),
            Some(&Value::Number(preset.metalness))
        );
        assert_eq!(
            instance.property("Roughness"),
            Some(&Value::Number(preset.roughness))
        );
    }

    fn changes(world: &mut World) -> Vec<Change> {
        world
            .take_changes()
            .into_iter()
            .map(|recorded| recorded.change)
            .collect()
    }

    #[test]
    fn changes_are_only_recorded_once_asked() {
        let mut world = World::new();
        let workspace = world.workspace();
        part_in(&mut world, workspace, "Before");
        assert!(world.take_changes().is_empty());

        world.record_changes();
        let part = part_in(&mut world, workspace, "After");
        let property = |name| Change::Property {
            instance: part,
            name,
        };
        assert_eq!(
            changes(&mut world),
            [
                property("Name"),
                Change::ChildAdded {
                    parent: workspace,
                    child: part
                },
                property("Parent"),
            ]
        );
        // Taken, so the list starts again.
        assert!(world.take_changes().is_empty());
    }

    #[test]
    fn only_real_changes_are_recorded_and_numbered_in_order() {
        let mut world = World::new();
        let workspace = world.workspace();
        let part = part_in(&mut world, workspace, "Pad");
        world.record_changes();
        let first = world.next_change_number();

        // The values it already has: no change.
        world.set_name(part, "Pad").unwrap();
        world
            .set_property(part, "Anchored", Value::Bool(false))
            .unwrap();
        world.set_parent(part, Some(workspace)).unwrap();
        assert!(world.take_changes().is_empty());
        assert_eq!(world.next_change_number(), first);

        world
            .set_property(part, "Anchored", Value::Bool(true))
            .unwrap();
        // Metal's preset differs in both, so all three change.
        world
            .set_property(part, "Material", Value::Material(Material::Metal))
            .unwrap();
        let recorded = world.take_changes();
        let names: Vec<_> = recorded
            .iter()
            .map(|recorded| match recorded.change {
                Change::Property { name, .. } => name,
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(names, ["Anchored", "Material", "Metalness", "Roughness"]);
        let numbers: Vec<u64> = recorded.iter().map(|recorded| recorded.number).collect();
        assert_eq!(numbers, [first, first + 1, first + 2, first + 3]);
        assert_eq!(world.next_change_number(), first + 4);
    }

    #[test]
    fn moving_records_removed_added_and_parent_but_reordering_does_not() {
        let mut world = World::new();
        let workspace = world.workspace();
        let folder = world.create(ClassName::Folder).unwrap();
        world.set_parent(folder, Some(workspace)).unwrap();
        let part = part_in(&mut world, workspace, "Pad");
        world.record_changes();

        world.set_parent_at(part, workspace, 0).unwrap();
        assert!(world.take_changes().is_empty());

        world.set_parent(part, Some(folder)).unwrap();
        world.destroy(part).unwrap();
        assert_eq!(
            changes(&mut world),
            [
                Change::ChildRemoved {
                    parent: workspace,
                    child: part
                },
                Change::ChildAdded {
                    parent: folder,
                    child: part
                },
                Change::Property {
                    instance: part,
                    name: "Parent"
                },
                // Destroying takes it out of its parent first.
                Change::ChildRemoved {
                    parent: folder,
                    child: part
                },
                Change::Property {
                    instance: part,
                    name: "Parent"
                },
            ]
        );
    }
}
