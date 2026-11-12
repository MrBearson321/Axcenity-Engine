//! Live scripts and stylesheets (C7 of `docs/research/collaborate.md`,
//! ADR-0021): a Lua script or stylesheet open in a session is a shared
//! text document that everyone can type in at once, instead of one editor
//! at a time with a lock.
//!
//! Each document is a [`yrs`] (Yjs) document holding one text. Members
//! send `yrs` v1 updates; the host checks the member may edit the file,
//! that the update is small enough and decodes, applies it to its own copy
//! and passes it on, so the session stays host-authoritative even though
//! text merges in any order. Files stay plain text: the host writes the
//! document's text back to the file as an ordinary file version once
//! typing settles ([`WRITE_BACK_AFTER`]).
//!
//! Only the host seeds a document from the file's text; a member's copy
//! starts empty and takes the host's state ([`LiveDoc::update_since`]), so
//! the text is never inserted twice.
//!
//! The host keeps each document's whole state beside its journal
//! ([`Saved`]), so a host that stops and starts again carries on with the
//! same document and members' copies merge back exactly. Each document has
//! an origin number; a copy from another origin (the host lost its state)
//! can't merge, so a member who typed something not written yet makes
//! their text again on top of the host's.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use yrs::updates::decoder::Decode;
use yrs::updates::encoder::Encode;
use yrs::{
    Assoc, Doc, GetString, IndexedSequence, ReadTxn, StateVector, StickyIndex, Text, TextRef,
    Transact, UndoManager, Update,
};

use crate::files::FileHash;
use crate::message::MemberId;

/// The biggest update a member may send (the protocol's limit, §9.2).
pub const UPDATE_MOST: usize = 1 << 20;

/// How long after the last change the host writes a document's text back
/// to its file.
pub const WRITE_BACK_AFTER: Duration = Duration::from_secs(2);

/// Whether `path` is a live document: Lua scripts and stylesheets. Block
/// scripts, graphs, materials and other structured files keep locks.
pub fn is_live(path: &str) -> bool {
    path.ends_with(".lua") || path.ends_with(".css")
}

/// A live document as the host keeps it on disk between its runs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Saved {
    pub path: String,
    /// The document's origin number.
    pub origin: u64,
    /// The file's hash when the document was last written to it (or
    /// opened from it): a file that changed since was saved some other way.
    pub file_at: Option<FileHash>,
    /// Who changed it last, for the next write-back.
    pub editor: MemberId,
    /// Everything in it ([`LiveDoc::state`]).
    pub state: Vec<u8>,
}

/// Why an update wasn't taken.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum LiveError {
    #[error("the change is too big ({0} bytes; at most {UPDATE_MOST})")]
    TooBig(usize),
    #[error("the change couldn't be read: {0}")]
    Malformed(String),
}

/// One shared text document.
pub struct LiveDoc {
    doc: Doc,
    text: TextRef,
    /// This copy's own changes, to undo without touching others' (a
    /// member's copy, [`LiveDoc::track_own_edits`]).
    undo: Option<UndoManager>,
}

/// The origin of changes from other copies: not this member's to undo.
const FROM_ELSEWHERE: &str = "elsewhere";

impl std::fmt::Debug for LiveDoc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LiveDoc")
            .field("length", &self.text().len())
            .finish()
    }
}

impl Default for LiveDoc {
    fn default() -> LiveDoc {
        LiveDoc::new()
    }
}

impl LiveDoc {
    /// An empty document (a member's, before the host's state arrives).
    pub fn new() -> LiveDoc {
        let doc = Doc::new();
        let text = doc.get_or_insert_text("text");
        LiveDoc {
            doc,
            text,
            undo: None,
        }
    }

    /// Keeps this copy's own changes for [`LiveDoc::undo`]: typing within
    /// half a second is one step, and others' changes stay when one is
    /// undone.
    pub fn track_own_edits(&mut self) {
        let mut undo = UndoManager::with_options(yrs::undo::Options::default());
        undo.expand_scope(&self.doc, &self.text);
        self.undo = Some(undo);
    }

    /// Undoes this copy's last own change. Returns the update to send, or
    /// `None` when there was nothing to undo.
    pub fn undo(&mut self) -> Option<Vec<u8>> {
        self.undo_or_redo(true)
    }

    /// Does again what [`LiveDoc::undo`] last undid.
    pub fn redo(&mut self) -> Option<Vec<u8>> {
        self.undo_or_redo(false)
    }

    fn undo_or_redo(&mut self, undo: bool) -> Option<Vec<u8>> {
        let before = self.doc.transact().state_vector();
        let manager = self.undo.as_mut()?;
        let changed = if undo {
            manager.undo_blocking()
        } else {
            manager.redo_blocking()
        };
        changed.then(|| self.doc.transact().encode_state_as_update_v1(&before))
    }

    /// A document holding `text` (the host's, from the file).
    pub fn with_text(text: &str) -> LiveDoc {
        let live = LiveDoc::new();
        if !text.is_empty() {
            let mut txn = live.doc.transact_mut();
            live.text.insert(&mut txn, 0, text);
        }
        live
    }

    /// A document from its whole state, as [`LiveDoc::state`] gave it.
    pub fn from_state(state: &[u8]) -> Result<LiveDoc, LiveError> {
        let mut live = LiveDoc::new();
        live.take(state)?;
        Ok(live)
    }

    /// Everything in this copy, to save or to start another copy from.
    pub fn state(&self) -> Vec<u8> {
        self.update_since(&[])
    }

    pub fn text(&self) -> String {
        self.text.get_string(&self.doc.transact())
    }

    /// A place in the text, as a character index, that stays on the same
    /// text while others type before it (a `yrs` sticky index), to send in
    /// presence (C8). `None` past the end.
    pub fn sticky(&self, chars: usize) -> Option<Vec<u8>> {
        let txn = self.doc.transact();
        let text = self.text.get_string(&txn);
        let byte = text
            .char_indices()
            .nth(chars)
            .map_or(text.len(), |(i, _)| i);
        // At the end there's no text after it to stick to: it sticks to
        // the end of the text instead.
        let assoc = if byte == text.len() {
            Assoc::Before
        } else {
            Assoc::After
        };
        self.text
            .sticky_index(&txn, byte as u32, assoc)
            .map(|sticky| sticky.encode_v1())
    }

    /// Where a [`LiveDoc::sticky`] place is now, as a character index.
    /// `None` when it doesn't read or this copy hasn't seen its text.
    pub fn resolve(&self, sticky: &[u8]) -> Option<usize> {
        let sticky = StickyIndex::decode_v1(sticky).ok()?;
        let txn = self.doc.transact();
        let offset = sticky.get_offset(&txn)?;
        let text = self.text.get_string(&txn);
        let mut byte = (offset.index as usize).min(text.len());
        while !text.is_char_boundary(byte) {
            byte -= 1;
        }
        Some(text[..byte].chars().count())
    }

    /// What this copy has seen, for [`LiveDoc::update_since`] elsewhere.
    pub fn state_vector(&self) -> Vec<u8> {
        self.doc.transact().state_vector().encode_v1()
    }

    /// Everything this copy has that one with `state_vector` lacks (all of
    /// it for an empty or unreadable state vector).
    pub fn update_since(&self, state_vector: &[u8]) -> Vec<u8> {
        let seen = StateVector::decode_v1(state_vector).unwrap_or_default();
        self.doc.transact().encode_state_as_update_v1(&seen)
    }

    /// Whether this copy has something one with `state_vector` lacks.
    pub fn is_ahead_of(&self, state_vector: &[u8]) -> bool {
        let Ok(seen) = StateVector::decode_v1(state_vector) else {
            return true;
        };
        let mine = self.doc.transact().state_vector();
        mine.iter().any(|(client, clock)| seen.get(client) < *clock)
    }

    /// Takes in an update from another copy. Refused whole when it's too
    /// big or doesn't decode; applying the same update twice changes
    /// nothing.
    pub fn apply(&mut self, update: &[u8]) -> Result<(), LiveError> {
        check(update)?;
        self.take(update)
    }

    fn take(&mut self, update: &[u8]) -> Result<(), LiveError> {
        let update =
            Update::decode_v1(update).map_err(|error| LiveError::Malformed(error.to_string()))?;
        self.doc
            .transact_mut_with(FROM_ELSEWHERE)
            .apply_update(update)
            .map_err(|error| LiveError::Malformed(error.to_string()))
    }

    /// Makes the text `new` with the fewest inserts and deletes from what
    /// it is (one changed span, as typing or a paste makes). Returns the
    /// update to send, or `None` when nothing changed.
    pub fn set_text(&mut self, new: &str) -> Option<Vec<u8>> {
        let old = self.text();
        if old == new {
            return None;
        }
        let before = self.doc.transact().state_vector();
        let (start, old_end, new_end) = changed_span(&old, new);
        {
            let mut txn = self.doc.transact_mut();
            if old_end > start {
                self.text
                    .remove_range(&mut txn, start as u32, (old_end - start) as u32);
            }
            if new_end > start {
                self.text
                    .insert(&mut txn, start as u32, &new[start..new_end]);
            }
        }
        Some(self.doc.transact().encode_state_as_update_v1(&before))
    }
}

/// Refuses an update over the size limit.
pub fn check(update: &[u8]) -> Result<(), LiveError> {
    if update.len() > UPDATE_MOST {
        return Err(LiveError::TooBig(update.len()));
    }
    Ok(())
}

/// The byte span of `old` that differs from `new`: `old[start..old_end]`
/// became `new[start..new_end]`. Both ends fall on character boundaries.
pub fn changed_span(old: &str, new: &str) -> (usize, usize, usize) {
    let (a, b) = (old.as_bytes(), new.as_bytes());
    let mut start = common_prefix(a, b);
    while !old.is_char_boundary(start) || !new.is_char_boundary(start) {
        start -= 1;
    }
    let mut same_end = common_suffix(&a[start..], &b[start..]);
    while !old.is_char_boundary(old.len() - same_end) || !new.is_char_boundary(new.len() - same_end)
    {
        same_end -= 1;
    }
    (start, old.len() - same_end, new.len() - same_end)
}

/// Compared a block at a time (`memcmp`), so a keystroke in a long script
/// costs little even in a debug build.
const BLOCK: usize = 256;

fn common_prefix(a: &[u8], b: &[u8]) -> usize {
    let most = a.len().min(b.len());
    let mut at = 0;
    while at + BLOCK <= most && a[at..at + BLOCK] == b[at..at + BLOCK] {
        at += BLOCK;
    }
    while at < most && a[at] == b[at] {
        at += 1;
    }
    at
}

fn common_suffix(a: &[u8], b: &[u8]) -> usize {
    let most = a.len().min(b.len());
    let mut same = 0;
    while same + BLOCK <= most
        && a[a.len() - same - BLOCK..a.len() - same] == b[b.len() - same - BLOCK..b.len() - same]
    {
        same += BLOCK;
    }
    while same < most && a[a.len() - same - 1] == b[b.len() - same - 1] {
        same += 1;
    }
    same
}
