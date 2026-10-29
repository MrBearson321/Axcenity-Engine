//! Undo and redo. Every change to the world (from the Properties panel or a
//! gizmo) is recorded here as a [`Step`] holding each changed value before
//! and after, so undo puts the "before" values back and redo the "after" ones.
//!
//! Changes to the tree (inserting, deleting, duplicating, moving into another
//! parent) are steps too. They record where an Instance was in the tree and
//! where it went, as a [`Move`]. Deleting only detaches an Instance (it keeps
//! its ID, properties and children, out of the tree), so undoing a delete
//! puts back the very same Instances, and older steps that name them still work.
//!
//! One gesture is one step: dragging a gizmo or a number, or typing a name,
//! changes the world every frame, and each of those changes merges into the
//! step the gesture started until the app calls [`History::seal`] (when the
//! mouse is let go and no text box is being typed in).
//!
//! Saving doesn't touch the history. It only remembers which step was on top
//! when it saved, so undoing back to that point counts as saved again.
//!
//! Dragging a UI handle can also rewrite a stylesheet (see `ui_handles.rs`).
//! Its step holds that file's text before and after as a [`TextChange`], and
//! undo and redo hand those back for the app to put in the file's tab.
//!
//! Creating, renaming, duplicating or deleting a clip, an animator controller
//! or a prefab is a file step too. `None` means the file is absent, so undoing
//! a delete writes the old text back and undoing a create removes the file.
//! Renaming a controller can also change Animators' `Controller` paths in the
//! same step ([`PropertyEdit`]).
//!
//! Importing an asset package writes files of any kind, models and
//! pictures too, so its step keeps each file's bytes ([`BlobChange`]).
//! Importing a model does too, along with the Instances it inserts, so
//! undoing it removes the files it copied and wrote. Deleting assets from
//! the Assets panel keeps the deleted files' bytes the same way.
//!
//! A Terrain's landscape isn't a property, so its changes are steps of their
//! own ([`TerrainChange`]): a brush stroke keeps the chunks it changed, and a
//! whole new landscape (generated, eroded, resized) keeps both landscapes.
//!
//! The string table (`strings.json`) is the world's, not an Instance's, so
//! its edits are steps of their own that keep the table before and after
//! ([`History::record_strings`]). Typing into one cell is one step.
//!
//! Attributes and tags aren't properties, so their edits are steps of their
//! own that keep each changed Instance's attributes and tags before and
//! after ([`History::record_annotations`]). Renaming a tag across the world
//! is one such step. Dragging one attribute's number is one step.
//!
//! A Console command can change anything, so its step keeps the whole world
//! from the other side of it ([`History::record_world`]); undo and redo swap
//! it with the world. Copies keep every `InstanceId`, so older steps still work.
//! Applying a prefab copy's overrides is such a step that also keeps the
//! prefab file's text before and after ([`History::record_world_and_files`]).
//!
//! A plugin's edits (Phase 54) are worked out afterwards from the world
//! before and after, Instance by Instance, and merge until its next
//! waypoint ([`History::record_edits`], in `edits`).

use std::collections::BTreeMap;
use std::sync::Arc;

use axcenity_core::{InstanceId, StringTable, TerrainData, TreeError, Value, World};
use axcenity_terrain::brush::TerrainEdit;

/// Something about an Instance that can be changed: its name or one property.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Name,
    Property(&'static str),
}

/// Every field of an Instance with its value, as recorded before and after a change.
pub type Snapshot = Vec<(Field, Value)>;

/// The fields of `id` and their values right now (empty if it no longer exists).
pub fn snapshot(world: &World, id: InstanceId) -> Snapshot {
    let Some(instance) = world.get(id) else {
        return Vec::new();
    };
    let name = (Field::Name, Value::String(instance.name().to_string()));
    let properties = instance
        .properties()
        .map(|(name, value)| (Field::Property(name), value.clone()));
    std::iter::once(name).chain(properties).collect()
}

/// An Instance's attributes and tags, as recorded before and after a change.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Annotations {
    pub attributes: BTreeMap<String, Value>,
    pub tags: Vec<String>,
}

/// The attributes and tags of `id` now (empty if it no longer exists).
pub fn annotations(world: &World, id: InstanceId) -> Annotations {
    world
        .get(id)
        .map_or_else(Annotations::default, |instance| Annotations {
            attributes: instance.attributes().clone(),
            tags: instance.tags().to_vec(),
        })
}

/// Gives `id` exactly these attributes and tags, in this order.
pub fn put_annotations(world: &mut World, id: InstanceId, annotations: &Annotations) {
    let Some(instance) = world.get(id) else {
        return;
    };
    let old_names: Vec<String> = instance.attributes().keys().cloned().collect();
    let old_tags = instance.tags().to_vec();
    for name in old_names {
        if !annotations.attributes.contains_key(&name) {
            let _ = world.set_attribute(id, &name, None);
        }
    }
    for (name, value) in &annotations.attributes {
        let _ = world.set_attribute(id, name, Some(value.clone()));
    }
    // All off and back on, so the order comes back too.
    if old_tags != annotations.tags {
        for tag in &old_tags {
            let _ = world.remove_tag(id, tag);
        }
        for tag in &annotations.tags {
            let _ = world.add_tag(id, tag);
        }
    }
}

/// Where an Instance sits in the tree: its parent, and its position among
/// the parent's children (0 is the first).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Place {
    pub parent: InstanceId,
    pub index: usize,
}

/// Where `id` is in the tree now, or `None` if it has no parent.
pub fn place_of(world: &World, id: InstanceId) -> Option<Place> {
    let parent = world.get(id)?.parent()?;
    let index = world
        .get(parent)?
        .children()
        .iter()
        .position(|&child| child == id)?;
    Some(Place { parent, index })
}

/// Puts `id` at `place`, or detaches it (with everything inside it) for `None`.
pub fn put(world: &mut World, id: InstanceId, place: Option<Place>) -> Result<(), TreeError> {
    match place {
        Some(place) => world.set_parent_at(id, place.parent, place.index),
        None => world.set_parent(id, None),
    }
}

/// One Instance going from one place in the tree to another. `None` is out
/// of the tree: `before: None` means it was just made (inserted or
/// duplicated), and `after: None` means it was deleted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Move {
    pub id: InstanceId,
    pub before: Option<Place>,
    pub after: Option<Place>,
}

/// What a step did to a Terrain's landscape.
#[derive(Debug)]
pub enum TerrainChange {
    /// A brush stroke: each chunk it changed, before and after.
    Stroke(TerrainEdit),
    /// A whole new landscape.
    Replace {
        before: Arc<TerrainData>,
        after: Arc<TerrainData>,
    },
}

/// A project file's text before and after a change, by its path.
/// `None` means the file is not there: a create has `before: None`, a delete
/// has `after: None`, and a rename is one of each.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextChange {
    pub path: String,
    pub before: Option<String>,
    pub after: Option<String>,
}

/// A project file's bytes before and after a change, for files that may
/// not be text (an imported package's models and pictures). `None` means
/// the file is not there.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlobChange {
    pub path: String,
    pub before: Option<Arc<Vec<u8>>>,
    pub after: Option<Arc<Vec<u8>>>,
}

/// The files undo or redo changed, for the app to write: each goes from
/// its `before` to its `after`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Restored {
    pub texts: Vec<TextChange>,
    pub blobs: Vec<BlobChange>,
}

/// One property changed as part of an asset step, such as an Animator's
/// `Controller` path when that controller file is renamed.
#[derive(Clone, Debug, PartialEq)]
pub struct PropertyEdit {
    pub id: InstanceId,
    pub field: Field,
    pub before: Value,
    pub after: Value,
}

/// One field's value before and after a step.
#[derive(Clone, Debug, PartialEq)]
struct Change {
    field: Field,
    before: Value,
    after: Value,
}

/// One undoable action, such as "Move Pad".
#[derive(Debug)]
struct Step {
    /// What the Edit menu calls it.
    label: String,
    kind: StepKind,
    /// A number no other step has had, to tell whether the world is as saved.
    number: u64,
}

#[derive(Debug)]
enum StepKind {
    /// Changes to the name and properties of one Instance.
    Fields {
        id: InstanceId,
        /// The field the gesture is editing. Later changes merge into this
        /// step only if they edit the same field of the same Instance.
        key: Field,
        /// Every field that changed, including side effects (setting
        /// `Material` also sets `Metalness` and `Roughness`).
        changes: Vec<Change>,
        /// Stylesheets the same gesture rewrote.
        texts: Vec<TextChange>,
    },
    /// Instances added to, taken out of, or moved around the tree, in the
    /// order they were made.
    Tree(Vec<Move>),
    /// Changes to several Instances at once, such as moving or painting the
    /// whole selection. Merges like `Fields`, when the same field of the
    /// same Instances is edited again.
    Many {
        key: Field,
        parts: Vec<(InstanceId, Vec<Change>)>,
        /// Stylesheets the same gesture rewrote.
        texts: Vec<TextChange>,
    },
    /// Project files created, rewritten, renamed or deleted, plus property
    /// changes and Instances inserted that belong to the same action (such
    /// as Animate's clip file, Animator and Animation).
    Files {
        texts: Vec<TextChange>,
        properties: Vec<PropertyEdit>,
        moves: Vec<Move>,
    },
    /// Project files of any kind written at once, such as an imported
    /// asset package, and Instances inserted with them (an imported
    /// model's MeshPart), or properties changed with them (an Asset Check
    /// fix that copies a file in and points parts at it).
    Blobs {
        blobs: Vec<BlobChange>,
        moves: Vec<Move>,
        properties: Vec<PropertyEdit>,
    },
    /// A Terrain's landscape, and its fields changed at the same time
    /// (a new `Size` with a generated landscape).
    Terrain {
        id: InstanceId,
        change: TerrainChange,
        fields: Vec<Change>,
    },
    /// The string table before and after. `key` is the cell being typed
    /// in; later edits to the same cell merge into the step.
    Strings {
        key: String,
        before: Arc<StringTable>,
        after: Arc<StringTable>,
    },
    /// Attributes and tags of some Instances, before and after. `key`
    /// names what's being edited (one attribute of one Instance); later
    /// edits with the same key merge into the step.
    Annotations {
        key: String,
        parts: Vec<(InstanceId, Annotations, Annotations)>,
    },
    /// The whole world from the other side of the step: before it while
    /// the step can be undone, after it once it has been. `texts` are
    /// project files changed with it (a prefab file Apply overrides wrote).
    World {
        other: WorldCopy,
        texts: Vec<TextChange>,
    },
    /// What a plugin's Lua did between waypoints (see `edits`).
    Edits(edits::Edits),
}

/// A whole world kept by a step. Its own type, to print as just its size in
/// `Debug` output.
struct WorldCopy(Box<World>);

impl std::fmt::Debug for WorldCopy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "World({} instances)", self.0.len())
    }
}

/// One Instance's snapshots either side of a change, for [`History::record_many`].
pub struct Changed {
    pub id: InstanceId,
    pub before: Snapshot,
    pub after: Snapshot,
}

#[derive(Debug, Default)]
pub struct History {
    undo: Vec<Step>,
    redo: Vec<Step>,
    /// The number of the step that the current gesture is still adding to.
    open: Option<u64>,
    /// Step numbers start at 1, so 0 stands for "no steps" in `saved_at`.
    last_number: u64,
    /// The number of the top undo step when the world was last saved (or
    /// opened), or 0 if there were no steps then.
    saved_at: u64,
}

impl History {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records that `id` changed from `before` to `after` (snapshots taken
    /// either side of the change). `key` is the field being edited and
    /// `label` names the step, such as "Move Pad". Does nothing if nothing
    /// actually changed. `world` is only needed to clean up after steps that
    /// can no longer be redone.
    pub fn record(
        &mut self,
        world: &mut World,
        id: InstanceId,
        key: Field,
        label: String,
        before: Snapshot,
        after: Snapshot,
    ) {
        self.record_with_texts(world, id, key, label, before, after, Vec::new());
    }

    /// [`History::record`], for a change that also rewrote stylesheets.
    #[allow(clippy::too_many_arguments)] // `record`'s, and the texts.
    pub fn record_with_texts(
        &mut self,
        world: &mut World,
        id: InstanceId,
        key: Field,
        label: String,
        before: Snapshot,
        after: Snapshot,
        texts: Vec<TextChange>,
    ) {
        let changes = differences(before, after);
        let texts: Vec<_> = texts
            .into_iter()
            .filter(|text| text.before != text.after)
            .collect();
        if changes.is_empty() && texts.is_empty() {
            return;
        }
        let open = self.open;
        // `matches!` with an `if` checks the step's shape and its values in one go.
        let merging = self.undo.last().is_some_and(|top| {
            matches!(&top.kind, StepKind::Fields { id: top_id, key: top_key, .. }
                if open == Some(top.number) && *top_id == id && *top_key == key)
        });
        if !merging {
            let number = self.push(
                world,
                label,
                StepKind::Fields {
                    id,
                    key,
                    changes,
                    texts,
                },
            );
            self.open = Some(number);
            return;
        }

        let number = self.next_number();
        let top = self.undo.last_mut().expect("checked by `merging`");
        let StepKind::Fields {
            changes: top_changes,
            texts: top_texts,
            ..
        } = &mut top.kind
        else {
            unreachable!("checked by `merging`");
        };
        for change in changes {
            // Keep the oldest "before" and the newest "after" of each field.
            match top_changes.iter_mut().find(|old| old.field == change.field) {
                Some(old) => old.after = change.after,
                None => top_changes.push(change),
            }
        }
        for text in texts {
            match top_texts.iter_mut().find(|old| old.path == text.path) {
                Some(old) => old.after = text.after,
                None => top_texts.push(text),
            }
        }
        top_changes.retain(|change| change.before != change.after);
        top_texts.retain(|text| text.before != text.after);
        if top_changes.is_empty() && top_texts.is_empty() {
            // Dragged back to where it started: there's nothing left to undo.
            self.undo.pop();
            self.open = None;
        } else {
            // A new number, because the world is no longer as it was when
            // this step was last on top.
            top.number = number;
            self.open = Some(number);
        }
    }

    /// Records project files written as bytes (created, or overwritten) as
    /// one undo step named `label`. Does nothing if nothing changed.
    pub fn record_blobs(&mut self, world: &mut World, label: String, blobs: Vec<BlobChange>) {
        self.record_blobs_and_tree(world, label, blobs, Vec::new());
    }

    /// Records project files written as bytes and Instances inserted or
    /// moved in the tree (both already done) as one undo step named
    /// `label`, such as importing a model. Does nothing if nothing changed.
    pub fn record_blobs_and_tree(
        &mut self,
        world: &mut World,
        label: String,
        blobs: Vec<BlobChange>,
        moves: Vec<Move>,
    ) {
        let blobs: Vec<_> = blobs
            .into_iter()
            .filter(|blob| blob.before != blob.after)
            .collect();
        let moves: Vec<_> = moves
            .into_iter()
            .filter(|step| step.before != step.after)
            .collect();
        if blobs.is_empty() && moves.is_empty() {
            return;
        }
        self.seal();
        self.push(
            world,
            label,
            StepKind::Blobs {
                blobs,
                moves,
                properties: Vec::new(),
            },
        );
        self.seal();
    }

    /// Records project files written as bytes and property changes made
    /// with them (both already done) as one undo step named `label`, such
    /// as an Asset Check fix. Does nothing if nothing changed.
    pub fn record_blobs_and_properties(
        &mut self,
        world: &mut World,
        label: String,
        blobs: Vec<BlobChange>,
        properties: Vec<PropertyEdit>,
    ) {
        let blobs: Vec<_> = blobs
            .into_iter()
            .filter(|blob| blob.before != blob.after)
            .collect();
        let properties: Vec<_> = properties
            .into_iter()
            .filter(|edit| edit.before != edit.after)
            .collect();
        if blobs.is_empty() && properties.is_empty() {
            return;
        }
        self.seal();
        self.push(
            world,
            label,
            StepKind::Blobs {
                blobs,
                moves: Vec::new(),
                properties,
            },
        );
        self.seal();
    }

    /// Records rewritten project file text as one undo step named `label`
    /// (for example editing a property animation clip).
    pub fn record_files(&mut self, world: &mut World, label: String, texts: Vec<TextChange>) {
        self.record_asset(world, label, texts, Vec::new());
    }

    /// Records project file changes, and property changes that are part of
    /// the same action, as one step named `label`. A missing file is `None`.
    /// The step does not merge with the next change. Does nothing if nothing
    /// actually changed.
    pub fn record_asset(
        &mut self,
        world: &mut World,
        label: String,
        texts: Vec<TextChange>,
        properties: Vec<PropertyEdit>,
    ) {
        let texts: Vec<_> = texts
            .into_iter()
            .filter(|text| text.before != text.after)
            .collect();
        let properties: Vec<_> = properties
            .into_iter()
            .filter(|edit| edit.before != edit.after)
            .collect();
        if texts.is_empty() && properties.is_empty() {
            return;
        }
        self.seal();
        self.push(
            world,
            label,
            StepKind::Files {
                texts,
                properties,
                moves: Vec::new(),
            },
        );
    }

    /// Records Instances inserted or moved, and property changes that are
    /// part of the same action (both already done), as one step named
    /// `label`. The step does not merge with the next change.
    pub fn record_tree_and_properties(
        &mut self,
        world: &mut World,
        label: String,
        moves: Vec<Move>,
        properties: Vec<PropertyEdit>,
    ) {
        let moves: Vec<_> = moves
            .into_iter()
            .filter(|step| step.before != step.after)
            .collect();
        let properties: Vec<_> = properties
            .into_iter()
            .filter(|edit| edit.before != edit.after)
            .collect();
        if moves.is_empty() && properties.is_empty() {
            return;
        }
        if properties.is_empty() {
            self.record_tree(world, label, moves);
            return;
        }
        if moves.is_empty() {
            self.record_asset(world, label, Vec::new(), properties);
            return;
        }
        self.seal();
        self.push(
            world,
            label,
            StepKind::Files {
                texts: Vec::new(),
                properties,
                moves,
            },
        );
    }

    /// Records project files written and Instances inserted or moved in the
    /// tree (both already done) as one step named `label`, which never
    /// merges with the next change.
    pub fn record_files_and_tree(
        &mut self,
        world: &mut World,
        label: String,
        texts: Vec<TextChange>,
        moves: Vec<Move>,
    ) {
        let texts: Vec<_> = texts
            .into_iter()
            .filter(|text| text.before != text.after)
            .collect();
        let moves: Vec<_> = moves
            .into_iter()
            .filter(|step| step.before != step.after)
            .collect();
        if texts.is_empty() && moves.is_empty() {
            return;
        }
        self.seal();
        self.push(
            world,
            label,
            StepKind::Files {
                texts,
                properties: Vec::new(),
                moves,
            },
        );
    }

    /// Records a change to several Instances as one step named `label`,
    /// such as "Paint 3 parts". `key` is the field the gesture edits; the
    /// next change merges into this step while the gesture is open and it
    /// edits the same field of the same Instances. One Instance is recorded
    /// as an ordinary [`record`](Self::record) step.
    pub fn record_many(
        &mut self,
        world: &mut World,
        key: Field,
        label: String,
        changed: Vec<Changed>,
    ) {
        self.record_many_with_texts(world, key, label, changed, Vec::new());
    }

    /// [`History::record_many`], for a change that also rewrote
    /// stylesheets, such as resizing several UI elements whose CSS sets
    /// their width.
    pub fn record_many_with_texts(
        &mut self,
        world: &mut World,
        key: Field,
        label: String,
        changed: Vec<Changed>,
        texts: Vec<TextChange>,
    ) {
        if let [only] = changed.as_slice() {
            let (id, before, after) = (only.id, only.before.clone(), only.after.clone());
            self.record_with_texts(world, id, key, label, before, after, texts);
            return;
        }
        let parts: Vec<(InstanceId, Vec<Change>)> = changed
            .into_iter()
            .map(|c| (c.id, differences(c.before, c.after)))
            .collect();
        let texts: Vec<_> = texts
            .into_iter()
            .filter(|text| text.before != text.after)
            .collect();
        if parts.iter().all(|(_, changes)| changes.is_empty()) && texts.is_empty() {
            return;
        }
        let open = self.open;
        let same_ids = |top: &[(InstanceId, Vec<Change>)]| {
            top.len() == parts.len() && top.iter().zip(&parts).all(|(a, b)| a.0 == b.0)
        };
        let merging = self.undo.last().is_some_and(|top| {
            matches!(&top.kind, StepKind::Many { key: top_key, parts: top_parts, .. }
                if open == Some(top.number) && *top_key == key && same_ids(top_parts))
        });
        if !merging {
            let number = self.push(world, label, StepKind::Many { key, parts, texts });
            self.open = Some(number);
            return;
        }
        let number = self.next_number();
        let top = self.undo.last_mut().expect("checked by `merging`");
        let StepKind::Many {
            parts: top_parts,
            texts: top_texts,
            ..
        } = &mut top.kind
        else {
            unreachable!("checked by `merging`");
        };
        for ((_, top_changes), (_, changes)) in top_parts.iter_mut().zip(parts) {
            for change in changes {
                match top_changes.iter_mut().find(|old| old.field == change.field) {
                    Some(old) => old.after = change.after,
                    None => top_changes.push(change),
                }
            }
            top_changes.retain(|change| change.before != change.after);
        }
        for text in texts {
            match top_texts.iter_mut().find(|old| old.path == text.path) {
                Some(old) => old.after = text.after,
                None => top_texts.push(text),
            }
        }
        top_texts.retain(|text| text.before != text.after);
        if top_parts.iter().all(|(_, changes)| changes.is_empty()) && top_texts.is_empty() {
            self.undo.pop();
            self.open = None;
        } else {
            top.number = number;
            self.open = Some(number);
        }
    }

    /// Records Instances that were inserted, deleted or moved in the tree
    /// (already done to `world`) as one step named `label`, such as
    /// "Delete Pad". A tree step never merges with the next change.
    pub fn record_tree(&mut self, world: &mut World, label: String, moves: Vec<Move>) {
        let moves: Vec<Move> = moves
            .into_iter()
            .filter(|step| step.before != step.after)
            .collect();
        if moves.is_empty() {
            return;
        }
        self.push(world, label, StepKind::Tree(moves));
        self.open = None;
    }

    /// Records a change to the Terrain `id`'s landscape as one step, with
    /// its fields' snapshots either side (for a `Size` changed with it).
    pub fn record_terrain(
        &mut self,
        world: &mut World,
        id: InstanceId,
        label: String,
        change: TerrainChange,
        before: Snapshot,
        after: Snapshot,
    ) {
        let fields = differences(before, after);
        self.push(world, label, StepKind::Terrain { id, change, fields });
        self.open = None;
    }

    /// Records that the world's string table went from `before` to what it
    /// is now. `key` names what's being edited (one cell, say): a change to
    /// the same thing while the gesture is open merges into its step, as
    /// [`History::record`] does. Does nothing if nothing changed.
    pub fn record_strings(
        &mut self,
        world: &mut World,
        key: String,
        label: String,
        before: Arc<StringTable>,
    ) {
        let after = world.strings().clone();
        let open = self.open;
        let merging = self.undo.last().is_some_and(|top| {
            matches!(&top.kind, StepKind::Strings { key: top_key, .. }
                if open == Some(top.number) && *top_key == key)
        });
        if !merging {
            if before == after {
                return;
            }
            let number = self.push(world, label, StepKind::Strings { key, before, after });
            self.open = Some(number);
            return;
        }
        let number = self.next_number();
        let top = self.undo.last_mut().expect("checked by `merging`");
        let StepKind::Strings {
            before: top_before,
            after: top_after,
            ..
        } = &mut top.kind
        else {
            unreachable!("checked by `merging`");
        };
        if *top_before == after {
            // Typed back to how it was: there's nothing left to undo.
            self.undo.pop();
            self.open = None;
        } else {
            *top_after = after;
            top.number = number;
            self.open = Some(number);
        }
    }

    /// Records attribute and tag changes (already made) as one step named
    /// `label`: each Instance's [`annotations`] before, and as they are
    /// now. Edits with the same `key` while the step is open (a drag)
    /// merge into it. Does nothing if nothing changed.
    pub fn record_annotations(
        &mut self,
        world: &mut World,
        key: String,
        label: String,
        before: Vec<(InstanceId, Annotations)>,
    ) {
        let parts: Vec<_> = before
            .into_iter()
            .map(|(id, before)| {
                let after = annotations(world, id);
                (id, before, after)
            })
            .collect();
        let open = self.open;
        let merging = self.undo.last().is_some_and(|top| {
            matches!(&top.kind, StepKind::Annotations { key: top_key, .. }
                if open == Some(top.number) && *top_key == key)
        });
        if !merging {
            let parts: Vec<_> = parts
                .into_iter()
                .filter(|(_, before, after)| before != after)
                .collect();
            if parts.is_empty() {
                return;
            }
            self.seal();
            let number = self.push(world, label, StepKind::Annotations { key, parts });
            self.open = Some(number);
            return;
        }
        let number = self.next_number();
        let top = self.undo.last_mut().expect("checked by `merging`");
        let StepKind::Annotations {
            parts: top_parts, ..
        } = &mut top.kind
        else {
            unreachable!("checked by `merging`");
        };
        for (id, before, after) in parts {
            // Keep the oldest "before" and the newest "after".
            match top_parts.iter_mut().find(|(top_id, _, _)| *top_id == id) {
                Some(part) => part.2 = after,
                None => top_parts.push((id, before, after)),
            }
        }
        top_parts.retain(|(_, before, after)| before != after);
        if top_parts.is_empty() {
            self.undo.pop();
            self.open = None;
        } else {
            top.number = number;
            self.open = Some(number);
        }
    }

    /// Records a change that could have touched anything, such as a Console
    /// command, as one step named `label`. `before` is the whole world as
    /// it was; `world` is already changed.
    pub fn record_world(&mut self, world: &mut World, label: String, before: World) {
        self.seal();
        self.record_world_and_files(world, label, before, Vec::new());
    }

    /// [`record_world`](Self::record_world) with project files that
    /// changed in the same action, such as a prefab whose overrides were
    /// applied and every copy of it brought up to date (Phase 49).
    pub fn record_world_and_files(
        &mut self,
        world: &mut World,
        label: String,
        before: World,
        texts: Vec<TextChange>,
    ) {
        let texts = texts
            .into_iter()
            .filter(|text| text.before != text.after)
            .collect();
        self.seal();
        self.push(
            world,
            label,
            StepKind::World {
                other: WorldCopy(Box::new(before)),
                texts,
            },
        );
    }

    /// Adds a new step on top and returns its number. The undone steps can't
    /// be redone after that, so they're dropped.
    fn push(&mut self, world: &mut World, label: String, kind: StepKind) -> u64 {
        // An undone insert left its Instance out of the tree. With the step
        // gone, nothing can bring it back, so destroy it.
        for step in self.redo.drain(..) {
            if let StepKind::Tree(moves)
            | StepKind::Files { moves, .. }
            | StepKind::Blobs { moves, .. } = step.kind
            {
                for made in moves.iter().filter(|step| step.before.is_none()) {
                    let _ = world.destroy(made.id);
                }
            }
        }
        let number = self.next_number();
        self.undo.push(Step {
            label,
            kind,
            number,
        });
        number
    }

    /// Ends the current gesture, so the next change starts a new step.
    pub fn seal(&mut self) {
        self.open = None;
    }

    /// What Undo would undo, such as "Move Pad".
    pub fn undo_label(&self) -> Option<&str> {
        self.undo.last().map(|step| step.label.as_str())
    }

    pub fn redo_label(&self) -> Option<&str> {
        self.redo.last().map(|step| step.label.as_str())
    }

    /// Puts the last step's "before" values back. Returns `None` if there
    /// was nothing to undo, or the files to change back, each from its
    /// `before` to its `after`.
    pub fn undo(&mut self, world: &mut World) -> Option<Restored> {
        self.seal();
        let mut step = self.undo.pop()?;
        let mut undone = Vec::new();
        let mut blobs = Vec::new();
        match &mut step.kind {
            StepKind::Fields {
                id, changes, texts, ..
            } => {
                restore(world, *id, changes.iter().map(|c| (c.field, &c.before)));
                undone = texts
                    .iter()
                    .map(|text| TextChange {
                        path: text.path.clone(),
                        before: text.after.clone(),
                        after: text.before.clone(),
                    })
                    .collect();
            }
            // Backwards, so each Instance goes back to where it was at the time.
            StepKind::Tree(moves) => {
                for step in moves.iter().rev() {
                    put_back(world, step.id, step.before);
                }
            }
            StepKind::Many { parts, texts, .. } => {
                for (id, changes) in parts {
                    restore(world, *id, changes.iter().map(|c| (c.field, &c.before)));
                }
                undone = texts
                    .iter()
                    .map(|text| TextChange {
                        path: text.path.clone(),
                        before: text.after.clone(),
                        after: text.before.clone(),
                    })
                    .collect();
            }
            StepKind::Files {
                texts,
                properties,
                moves,
            } => {
                for edit in properties {
                    restore(world, edit.id, std::iter::once((edit.field, &edit.before)));
                }
                for step in moves.iter().rev() {
                    put_back(world, step.id, step.before);
                }
                undone = texts
                    .iter()
                    .map(|text| TextChange {
                        path: text.path.clone(),
                        before: text.after.clone(),
                        after: text.before.clone(),
                    })
                    .collect();
            }
            StepKind::Terrain { id, change, fields } => {
                restore(world, *id, fields.iter().map(|c| (c.field, &c.before)));
                if let Some(data) = world.terrain_mut(*id) {
                    match change {
                        TerrainChange::Stroke(edit) => edit.undo(data),
                        TerrainChange::Replace { before, .. } => data.replace_with(before),
                    }
                }
            }
            StepKind::Blobs {
                blobs: changes,
                moves,
                properties,
            } => {
                for edit in properties {
                    restore(world, edit.id, std::iter::once((edit.field, &edit.before)));
                }
                for step in moves.iter().rev() {
                    put_back(world, step.id, step.before);
                }
                blobs = changes
                    .iter()
                    .map(|blob| BlobChange {
                        path: blob.path.clone(),
                        before: blob.after.clone(),
                        after: blob.before.clone(),
                    })
                    .collect();
            }
            StepKind::Strings { before, .. } => world.set_strings(before.clone()),
            StepKind::Annotations { parts, .. } => {
                for (id, before, _) in parts {
                    put_annotations(world, *id, before);
                }
            }
            StepKind::Edits(edits) => edits.undo(world),
            StepKind::World { other, texts } => {
                std::mem::swap(world, &mut *other.0);
                undone = texts
                    .iter()
                    .map(|text| TextChange {
                        path: text.path.clone(),
                        before: text.after.clone(),
                        after: text.before.clone(),
                    })
                    .collect();
            }
        }
        self.redo.push(step);
        Some(Restored {
            texts: undone,
            blobs,
        })
    }

    /// Puts the last undone step's "after" values back. Returns `None` if
    /// there was nothing to redo, or the files to change, as [`History::undo`].
    pub fn redo(&mut self, world: &mut World) -> Option<Restored> {
        self.seal();
        let mut step = self.redo.pop()?;
        let mut redone = Vec::new();
        let mut blobs = Vec::new();
        match &mut step.kind {
            StepKind::Fields {
                id, changes, texts, ..
            } => {
                restore(world, *id, changes.iter().map(|c| (c.field, &c.after)));
                redone = texts.clone();
            }
            StepKind::Tree(moves) => {
                for step in moves {
                    put_back(world, step.id, step.after);
                }
            }
            StepKind::Many { parts, texts, .. } => {
                for (id, changes) in parts {
                    restore(world, *id, changes.iter().map(|c| (c.field, &c.after)));
                }
                redone = texts.clone();
            }
            StepKind::Files {
                texts,
                properties,
                moves,
            } => {
                for step in moves.iter() {
                    put_back(world, step.id, step.after);
                }
                for edit in properties {
                    restore(world, edit.id, std::iter::once((edit.field, &edit.after)));
                }
                redone = texts.clone();
            }
            StepKind::Terrain { id, change, fields } => {
                restore(world, *id, fields.iter().map(|c| (c.field, &c.after)));
                if let Some(data) = world.terrain_mut(*id) {
                    match change {
                        TerrainChange::Stroke(edit) => edit.redo(data),
                        TerrainChange::Replace { after, .. } => data.replace_with(after),
                    }
                }
            }
            StepKind::Blobs {
                blobs: changes,
                moves,
                properties,
            } => {
                for step in moves.iter() {
                    put_back(world, step.id, step.after);
                }
                for edit in properties {
                    restore(world, edit.id, std::iter::once((edit.field, &edit.after)));
                }
                blobs = changes.clone();
            }
            StepKind::Strings { after, .. } => world.set_strings(after.clone()),
            StepKind::Annotations { parts, .. } => {
                for (id, _, after) in parts {
                    put_annotations(world, *id, after);
                }
            }
            StepKind::Edits(edits) => edits.redo(world),
            StepKind::World { other, texts } => {
                std::mem::swap(world, &mut *other.0);
                redone = texts.clone();
            }
        }
        self.undo.push(step);
        Some(Restored {
            texts: redone,
            blobs,
        })
    }

    /// Remembers that the world as it is now is what's saved. The history
    /// itself stays, so saving never takes away undo.
    pub fn mark_saved(&mut self) {
        self.seal();
        self.saved_at = self.top_number();
    }

    /// Counts the world as unsaved until it's next saved, although nothing
    /// is in the history: its edits were made before a relaunch.
    pub fn mark_unsaved(&mut self) {
        self.saved_at = u64::MAX;
    }

    /// True if the world is the same as when it was last saved (or opened).
    pub fn is_saved(&self) -> bool {
        self.top_number() == self.saved_at
    }

    /// Different after every change, undo and redo, and when a gesture
    /// ends: the top step's number, and whether a gesture is still adding
    /// to it.
    pub fn revision(&self) -> (u64, bool) {
        (self.top_number(), self.open.is_some())
    }

    fn top_number(&self) -> u64 {
        self.undo.last().map_or(0, |step| step.number)
    }

    fn next_number(&mut self) -> u64 {
        self.last_number += 1;
        self.last_number
    }
}

/// The fields whose values differ between two snapshots of one Instance.
fn differences(before: Snapshot, after: Snapshot) -> Vec<Change> {
    after
        .into_iter()
        .filter_map(|(field, after)| {
            let (_, before) = before.iter().find(|(old, _)| *old == field)?;
            (*before != after).then(|| Change {
                field,
                before: before.clone(),
                after,
            })
        })
        .collect()
}

/// [`put`] for undo and redo, which can only fail if the history is wrong.
fn put_back(world: &mut World, id: InstanceId, place: Option<Place>) {
    if let Err(err) = put(world, id, place) {
        log::warn!("Could not put {id:?} back at {place:?}: {err}");
    }
}

/// Sets each field of `id` to its value. `Material` goes first, because
/// setting it resets `Metalness` and `Roughness`, which come after it and
/// put their own recorded values back. A missing Instance is skipped.
fn restore<'a>(
    world: &mut World,
    id: InstanceId,
    values: impl Iterator<Item = (Field, &'a Value)>,
) {
    // `'a` says the values are borrowed from the step for as long as this
    // call runs; they're cloned into the world one by one.
    let mut values: Vec<(Field, &Value)> = values.collect();
    // `sort_by_key` keeps the order otherwise; `false` sorts before `true`.
    values.sort_by_key(|(_, value)| value.as_material().is_none());
    for (field, value) in values {
        let restored = match (field, value) {
            (Field::Name, Value::String(name)) => world.set_name(id, name.clone()).is_ok(),
            (Field::Property(name), value) => world.set_property(id, name, value.clone()).is_ok(),
            // `snapshot` always records names as strings.
            (Field::Name, _) => false,
        };
        // Can't happen: deleting only detaches, so recorded Instances stay alive.
        if !restored {
            log::warn!("Could not put back {field:?} on {id:?}");
        }
    }
}

mod edits;
pub use edits::changed_ids;

#[cfg(test)]
mod tests;
