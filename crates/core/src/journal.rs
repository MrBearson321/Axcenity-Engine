//! The edit journal: every change to a World with the value it replaced,
//! so a change made by any tool can be sent elsewhere and taken back
//! exactly. Collaborate (`collab`) turns it on for the editor's edit World
//! and turns each batch of edits into a transaction.
//!
//! It's off until [`World::record_edits`](crate::World::record_edits), so
//! worlds nobody shares pay nothing. A clone keeps it on or off, but
//! empty: copy a recording World for Play and stop it there.

use crate::value::Value;
use crate::world::InstanceId;

/// One change, with what was there before it.
#[derive(Clone, Debug, PartialEq)]
pub enum Edit {
    /// A new Instance (from `create`, `create_at`, `duplicate` or
    /// `copy_from`), with no parent yet.
    Created(InstanceId),
    /// An Instance destroyed for good (not just taken out of the tree).
    Destroyed(InstanceId),
    Name {
        id: InstanceId,
        before: String,
    },
    Property {
        id: InstanceId,
        name: &'static str,
        before: Value,
    },
    /// A new parent, or a new place among the same parent's children.
    /// `before` is the parent and position it had, or `None` if it had no
    /// parent.
    Parent {
        id: InstanceId,
        before: Option<(InstanceId, usize)>,
    },
    Attribute {
        id: InstanceId,
        name: String,
        before: Option<Value>,
    },
    /// The whole tag list before the change, in order.
    Tags {
        id: InstanceId,
        before: Vec<String>,
    },
}

#[derive(Debug, Default)]
pub(crate) struct EditJournal {
    pub(crate) recording: bool,
    pub(crate) edits: Vec<Edit>,
}

impl Clone for EditJournal {
    /// The copy records if the original does, starting empty: its edits
    /// are its own.
    fn clone(&self) -> Self {
        EditJournal {
            recording: self.recording,
            edits: Vec::new(),
        }
    }
}
