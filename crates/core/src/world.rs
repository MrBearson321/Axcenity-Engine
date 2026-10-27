//! The Instance tree: `game` at the root, services below it, and everything
//! the game is made of below those.
//!
//! Instances never point at each other directly. Each one stores the
//! [`InstanceId`]s of its parent and children, and all access goes through
//! [`World`], which owns every Instance in an [`Arena`].

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use crate::animation::{AnimationTrackState, AnimatorState, PropertyAnimationState};
use crate::arena::{Arena, Id};
use crate::class::ClassName;
use crate::strings::StringTable;
use crate::terrain::TerrainData;
use crate::ui_settings::UiSettings;
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
    /// Values a creator or script attached by name (`SetAttribute`).
    attributes: BTreeMap<String, Value>,
    /// Tags (`CollectionService`), in the order they were added.
    tags: Vec<String>,
}

impl Instance {
    fn new(class: ClassName) -> Self {
        Self {
            class,
            name: class.as_str().to_owned(),
            parent: None,
            children: Vec::new(),
            properties: class.default_properties().into_iter().collect(),
            attributes: BTreeMap::new(),
            tags: Vec::new(),
        }
    }

    pub fn class(&self) -> ClassName {
        self.class
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// The name navigation and scripts use for a UI element: its
    /// `AccessibleName`, or its `Name` when that's empty. `None` for an
    /// Instance that isn't a UI element.
    pub fn accessible_name(&self) -> Option<&str> {
        let own = self.property("AccessibleName").and_then(Value::as_str)?;
        Some(if own.is_empty() { &self.name } else { own })
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

    pub fn attribute(&self, name: &str) -> Option<&Value> {
        self.attributes.get(name)
    }

    /// Every attribute, sorted by name.
    pub fn attributes(&self) -> &BTreeMap<String, Value> {
        &self.attributes
    }

    /// Its tags, in the order they were added.
    pub fn tags(&self) -> &[String] {
        &self.tags
    }

    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.iter().any(|t| t == tag)
    }
}

/// The longest attribute name, as in Roblox.
pub const MAX_ATTRIBUTE_NAME: usize = 100;

#[derive(Debug, PartialEq, thiserror::Error)]
pub enum AttributeError {
    #[error("the Instance no longer exists")]
    Stale,
    #[error(
        "`{0}` isn't an attribute name: use 1 to 100 letters, digits and underscores, not starting with `RBX`"
    )]
    BadName(String),
    #[error("an attribute can't hold an Instance")]
    BadValue,
}

/// Whether `name` can name an attribute: 1 to [`MAX_ATTRIBUTE_NAME`]
/// letters, digits and underscores, not starting with `RBX` (kept for the
/// engine, as in Roblox).
pub fn is_attribute_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_ATTRIBUTE_NAME
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !name.starts_with("RBX")
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
/// `Changed`, `GetPropertyChangedSignal`, `ChildAdded`, `ChildRemoved`,
/// `AttributeChanged` and `CollectionService`'s signals.
#[derive(Clone, Debug, PartialEq, Eq)]
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
    /// The attribute `name` of `instance` was set to a different value or removed.
    Attribute { instance: InstanceId, name: String },
    /// `instance` was given the tag `tag` (`added`), or lost it.
    Tag {
        instance: InstanceId,
        tag: String,
        added: bool,
    },
}

/// A [`Change`] with its number. Changes are numbered 0, 1, 2, ... in the
/// order they happened, so a script that starts listening after change 5
/// can skip changes 0 to 5.
#[derive(Clone, Debug, PartialEq, Eq)]
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
    lighting: InstanceId,
    replicated_storage: InstanceId,
    server_script_service: InstanceId,
    starter_player_scripts: InstanceId,
    changes: ChangeLog,
    /// Each Animator's live state (see [`AnimatorState`]), made the first
    /// time something asks for it.
    animators: HashMap<InstanceId, AnimatorState>,
    /// Each property Animation's live playback (see [`PropertyAnimationState`]).
    property_animations: HashMap<InstanceId, PropertyAnimationState>,
    /// Each AnimationTrack's live playback, made by `LoadAnimation`.
    animation_tracks: HashMap<InstanceId, AnimationTrackState>,
    /// Each Terrain's heightfield and paint. Every Terrain has one, made
    /// with it. Shared with copies of the world until one changes it.
    terrains: HashMap<InstanceId, Arc<TerrainData>>,
    /// The project's string table, shared with copies of the world.
    strings: Arc<StringTable>,
    /// The language bound UI text shows (see [`World::language`]).
    language: String,
    /// The player's game UI settings (see [`World::ui_settings`]).
    ui_settings: UiSettings,
}

impl World {
    /// Creates a world with `game` and its services, in the order the
    /// Explorer shows them: `Workspace`, `Players`, `Lighting`, `ReplicatedStorage`,
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
        let lighting = add_service(ClassName::Lighting, root);
        let replicated_storage = add_service(ClassName::ReplicatedStorage, root);
        let server_script_service = add_service(ClassName::ServerScriptService, root);
        let starter_player = add_service(ClassName::StarterPlayer, root);
        let starter_player_scripts = add_service(ClassName::StarterPlayerScripts, starter_player);

        Self {
            instances,
            root,
            workspace,
            players,
            lighting,
            replicated_storage,
            server_script_service,
            starter_player_scripts,
            changes: ChangeLog::default(),
            animators: HashMap::new(),
            property_animations: HashMap::new(),
            animation_tracks: HashMap::new(),
            terrains: HashMap::new(),
            strings: Arc::default(),
            language: String::new(),
            ui_settings: UiSettings::default(),
        }
    }

    /// The project's string table (`strings.json`).
    pub fn strings(&self) -> &Arc<StringTable> {
        &self.strings
    }

    /// Gives the world its string table. The active language stays when
    /// the table still has it, else it's the table's first.
    pub fn set_strings(&mut self, strings: Arc<StringTable>) {
        self.strings = strings;
        if !self.strings.has_language(&self.language) {
            self.language.clear();
        }
    }

    /// The language bound UI text shows: the one last set with
    /// [`World::set_language`], else the string table's first. Empty when
    /// the table has no languages.
    pub fn language(&self) -> &str {
        if self.language.is_empty() {
            self.strings.default_language()
        } else {
            &self.language
        }
    }

    /// Makes `language` the active one. False (and nothing changes) when the
    /// string table doesn't have it.
    pub fn set_language(&mut self, language: &str) -> bool {
        if !self.strings.has_language(language) {
            return false;
        }
        self.language = language.to_owned();
        true
    }

    /// The player's game UI settings: text scale, higher contrast and
    /// reduced motion, which HUDs and menus follow. Copies of the world
    /// (Play) start with them.
    pub fn ui_settings(&self) -> UiSettings {
        self.ui_settings
    }

    pub fn set_ui_settings(&mut self, settings: UiSettings) {
        self.ui_settings = settings;
    }

    /// `key`'s value in the active language, if the string table has one.
    pub fn translate(&self, key: &str) -> Option<&str> {
        self.strings.get(key, self.language())
    }

    /// The heightfield and paint of the Terrain `id`.
    pub fn terrain(&self, id: InstanceId) -> Option<&Arc<TerrainData>> {
        self.terrains.get(&id)
    }

    /// The Terrain `id`'s data, to change. If a copy of the world shares
    /// it, this world gets its own copy first (`Arc::make_mut`).
    pub fn terrain_mut(&mut self, id: InstanceId) -> Option<&mut TerrainData> {
        self.terrains.get_mut(&id).map(Arc::make_mut)
    }

    /// Gives the Terrain `id` this data. False (and nothing changes) if
    /// `id` isn't a Terrain.
    pub fn set_terrain(&mut self, id: InstanceId, data: Arc<TerrainData>) -> bool {
        if self.get(id).map(Instance::class) != Some(ClassName::Terrain) {
            return false;
        }
        self.terrains.insert(id, data);
        true
    }

    /// The live state of the Animator `id`, if it has any yet.
    pub fn animator(&self, id: InstanceId) -> Option<&AnimatorState> {
        self.animators.get(&id)
    }

    /// The live state of the Animator `id`, made (empty) the first time.
    /// `None` if `id` isn't an Animator.
    pub fn animator_mut(&mut self, id: InstanceId) -> Option<&mut AnimatorState> {
        if self.get(id)?.class != ClassName::Animator {
            return None;
        }
        Some(self.animators.entry(id).or_default())
    }

    /// The live playback of the property Animation `id`, if any yet.
    pub fn property_animation(&self, id: InstanceId) -> Option<&PropertyAnimationState> {
        self.property_animations.get(&id)
    }

    /// The live playback of the property Animation `id`, made empty the first
    /// time. `None` if `id` isn't an Animation.
    pub fn property_animation_mut(
        &mut self,
        id: InstanceId,
    ) -> Option<&mut PropertyAnimationState> {
        if self.get(id)?.class != ClassName::Animation {
            return None;
        }
        Some(self.property_animations.entry(id).or_default())
    }

    /// The live playback of the AnimationTrack `id`.
    pub fn animation_track(&self, id: InstanceId) -> Option<&AnimationTrackState> {
        self.animation_tracks.get(&id)
    }

    pub fn animation_track_mut(&mut self, id: InstanceId) -> Option<&mut AnimationTrackState> {
        self.animation_tracks.get_mut(&id)
    }

    /// Makes an AnimationTrack (unparented, like Roblox's) for `animation`
    /// on `animator`, and lists it on the animator. `None` if `animator`
    /// isn't an Animator or `animation` isn't an Animation.
    pub fn load_animation(
        &mut self,
        animator: InstanceId,
        animation: InstanceId,
    ) -> Option<InstanceId> {
        if self.get(animation)?.class != ClassName::Animation {
            return None;
        }
        self.animator_mut(animator)?;
        let track = self
            .create(ClassName::AnimationTrack)
            .expect("AnimationTrack isn't a service");
        let name = self
            .get(animation)
            .map(|a| a.name.clone())
            .unwrap_or_default();
        let _ = self.set_name(track, name);
        self.animation_tracks
            .insert(track, AnimationTrackState::new(animator, animation));
        self.animator_mut(animator)?.tracks.push(track);
        Some(track)
    }

    /// Starts keeping a list of every [`Change`] from now on, for
    /// [`take_changes`](Self::take_changes). Calling it again does nothing.
    pub fn record_changes(&mut self) {
        self.changes.recording = true;
    }

    /// True between [`record_changes`](Self::record_changes) and
    /// [`stop_recording_changes`](Self::stop_recording_changes).
    pub fn is_recording_changes(&self) -> bool {
        self.changes.recording
    }

    /// Stops keeping the list, and forgets what's in it.
    pub fn stop_recording_changes(&mut self) {
        self.changes.recording = false;
        self.changes.changes.clear();
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

    /// Holds the world's lighting settings, such as shadows.
    pub fn lighting(&self) -> InstanceId {
        self.lighting
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
            ClassName::Lighting => self.lighting,
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
        let id = InstanceId(self.instances.insert(Instance::new(class)));
        if class == ClassName::Terrain {
            self.terrains.insert(id, Arc::new(TerrainData::default()));
        }
        Ok(id)
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
        self.copy_terrain(id, root);

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
            self.copy_terrain(old, new);
            copies.insert(old, new);
        }
        // An ObjectValue in the copy that points inside the original points
        // at the same place in the copy, as `Clone` does in Roblox.
        for &copy in copies.values() {
            let instance = self.instances.get_mut(copy.0).expect("copied above");
            for value in instance.properties.values_mut() {
                if let Value::Instance(Some(target)) = value
                    && let Some(&moved) = copies.get(target)
                {
                    *target = moved;
                }
            }
        }
        Ok(root)
    }

    /// Gives a copy of a Terrain its original's landscape. They share it
    /// until one is changed.
    fn copy_terrain(&mut self, original: InstanceId, copy: InstanceId) {
        if let Some(data) = self.terrains.get(&original).cloned() {
            self.terrains.insert(copy, data);
        }
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
            self.animators.remove(&descendant);
            self.property_animations.remove(&descendant);
            self.animation_tracks.remove(&descendant);
            self.terrains.remove(&descendant);
        }
        self.instances.remove(id.0);
        self.animators.remove(&id);
        self.property_animations.remove(&id);
        self.animation_tracks.remove(&id);
        self.terrains.remove(&id);
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
        // An IntValue holds whole numbers only.
        let value = match value {
            Value::Number(n) if class == ClassName::IntValue && name == "Value" => {
                Value::Number(n.round())
            }
            value => value,
        };
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

    /// Sets the attribute `name` of `id` to `value`, or removes it for
    /// `None`. Setting the value it already has isn't a [`Change`].
    pub fn set_attribute(
        &mut self,
        id: InstanceId,
        name: &str,
        value: Option<Value>,
    ) -> Result<(), AttributeError> {
        if !is_attribute_name(name) {
            return Err(AttributeError::BadName(name.to_owned()));
        }
        if value.as_ref().is_some_and(|value| !value.is_attribute()) {
            return Err(AttributeError::BadValue);
        }
        // Attributes keep every enum item the same way.
        let value = value.map(|value| match value.to_enum_item() {
            Some(item) => Value::EnumItem(item),
            None => value,
        });
        let instance = self.instances.get_mut(id.0).ok_or(AttributeError::Stale)?;
        let changed = match value {
            Some(value) => {
                instance.attributes.insert(name.to_owned(), value.clone()) != Some(value)
            }
            None => instance.attributes.remove(name).is_some(),
        };
        if changed {
            self.changed(Change::Attribute {
                instance: id,
                name: name.to_owned(),
            });
        }
        Ok(())
    }

    /// Gives `id` the tag `tag`. True if it didn't have it already.
    pub fn add_tag(&mut self, id: InstanceId, tag: &str) -> Result<bool, TreeError> {
        let instance = self.instances.get_mut(id.0).ok_or(TreeError::Stale)?;
        if instance.has_tag(tag) {
            return Ok(false);
        }
        instance.tags.push(tag.to_owned());
        self.changed(Change::Tag {
            instance: id,
            tag: tag.to_owned(),
            added: true,
        });
        Ok(true)
    }

    /// Takes the tag `tag` off `id`. True if it had it.
    pub fn remove_tag(&mut self, id: InstanceId, tag: &str) -> Result<bool, TreeError> {
        let instance = self.instances.get_mut(id.0).ok_or(TreeError::Stale)?;
        let Some(index) = instance.tags.iter().position(|t| t == tag) else {
            return Ok(false);
        };
        instance.tags.remove(index);
        self.changed(Change::Tag {
            instance: id,
            tag: tag.to_owned(),
            added: false,
        });
        Ok(true)
    }

    /// Every Instance in the game (inside `game`) with the tag `tag`, in
    /// [`descendants`](Self::descendants) order.
    pub fn tagged(&self, tag: &str) -> Vec<InstanceId> {
        self.descendants(self.root)
            .into_iter()
            .filter(|&id| self.get(id).is_some_and(|instance| instance.has_tag(tag)))
            .collect()
    }

    /// Every tag an Instance in the game has, sorted, with how many have it.
    pub fn all_tags(&self) -> BTreeMap<String, usize> {
        let mut tags = BTreeMap::new();
        for id in self.descendants(self.root) {
            for tag in self.get(id).map(Instance::tags).unwrap_or_default() {
                *tags.entry(tag.clone()).or_default() += 1;
            }
        }
        tags
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
mod tests;
