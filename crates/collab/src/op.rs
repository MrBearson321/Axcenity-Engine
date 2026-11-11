//! The operations a Team Create transaction is made of.

use axcenity_core::{ClassName, InstanceId, Value};

use crate::files::FileHash;

/// One Instance to make, as part of [`Op::CreateTree`].
#[derive(Clone, Debug, PartialEq)]
pub struct NodeSpec {
    /// From the creator's ID block (see [`crate::ids`]).
    pub id: InstanceId,
    pub class: ClassName,
    /// An Instance already in the world, or an earlier node of the same
    /// `CreateTree`.
    pub parent: InstanceId,
    /// Where among the parent's children; `None` is last.
    pub index: Option<usize>,
    pub name: String,
    /// Properties that differ from the class's defaults, in the order to
    /// set them (`Material` before `Metalness` and `Roughness`).
    pub properties: Vec<(String, Value)>,
    pub attributes: Vec<(String, Value)>,
    pub tags: Vec<String>,
}

/// One change to the World. A [`Transaction`] is a list of these.
#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    /// New Instances, parents before children.
    CreateTree { nodes: Vec<NodeSpec> },
    /// Moves `id` under `parent` (at `index` among its children, or last),
    /// or deletes it with `parent: None`. A deleted Instance is kept out of
    /// the tree, so moving it back in restores it.
    ///
    /// `restore` says the author knows `id` is deleted and means to bring
    /// it back (Restore, or undoing their own delete). Without it, a move of
    /// something someone else has deleted meanwhile is dropped: delete wins.
    Reparent {
        id: InstanceId,
        parent: Option<InstanceId>,
        index: Option<usize>,
        restore: bool,
    },
    /// Sets properties of `id` in order; `"Name"` sets its name. Last writer
    /// (in the host's order) wins, property by property.
    SetProps {
        id: InstanceId,
        properties: Vec<(String, Value)>,
    },
    /// Sets an attribute, or removes it with `None`.
    SetAttribute {
        id: InstanceId,
        name: String,
        value: Option<Value>,
    },
    /// Adds a tag (`added`) or removes it.
    Tag {
        id: InstanceId,
        tag: String,
        added: bool,
    },
    /// Sets a project file to the uploaded version `new`, or deletes it
    /// with `None` (TC5). `base` is the version its author changed (`None`
    /// for a new file): if the file has changed since, the whole
    /// transaction is refused, so a save never overwrites one the author
    /// hasn't seen.
    FileVersion {
        path: String,
        base: Option<FileHash>,
        new: Option<FileHash>,
    },
}

impl Op {
    /// True for an op that changes the World (not a file).
    pub fn is_world(&self) -> bool {
        !matches!(self, Op::FileVersion { .. })
    }
}

/// One undo step's worth of edits, as a client sends it.
#[derive(Clone, Debug, PartialEq)]
pub struct Transaction {
    /// Counts up from 1 for each member, so the host can drop one it has
    /// already handled (a resend after a reconnect).
    pub client_seq: u64,
    /// What the Edit menu calls it ("Move Pad").
    pub label: String,
    pub ops: Vec<Op>,
}
