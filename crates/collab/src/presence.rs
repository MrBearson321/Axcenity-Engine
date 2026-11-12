//! Who's in a session and what they're doing (TC4; design §7.9 and §11).
//!
//! The host keeps the **roster**: every member's name, colour, role and
//! whether they're connected, sent to everyone whenever it changes. Each
//! member also sends their **presence** (what they have selected, where
//! their camera is, the field they're changing) when it changes; the host
//! passes it on to the others and keeps the latest, so someone joining sees
//! it at once. Presence isn't part of the World and never goes in the log.
//!
//! C8 adds where a member's cursor is in a live script or stylesheet
//! ([`TextCursor`], as `yrs` sticky places, so it stays on its text while
//! others type) and the session's **chat** ([`ChatLine`]): the host numbers
//! nothing, keeps the last [`CHAT_KEPT`] lines in memory only (never in the
//! project or the journal), and sends them to whoever joins.

use axcenity_core::InstanceId;
use serde::{Deserialize, Serialize};

use crate::message::MemberId;
use crate::role::Role;

/// The most Instances a presence lists as selected; a bigger selection is
/// cut to this many.
pub const SELECTION_SHOWN: usize = 256;

/// The longest name, and the longest `editing` text, the host passes on.
pub const TEXT_MOST: usize = 64;

/// The most chat lines the host keeps for someone joining.
pub const CHAT_KEPT: usize = 200;

/// The longest chat line, in characters; a longer one is cut.
pub const CHAT_MOST: usize = 2000;

/// The most chat lines a member may send in a second; more are dropped.
pub const CHAT_PER_SECOND: usize = 5;

/// The longest sticky place a cursor may carry, and the longest path.
const STICKY_MOST: usize = 64;
const PATH_MOST: usize = 512;

/// One line of the session's chat, as the host passed it on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatLine {
    pub member: MemberId,
    /// When the host took it, in seconds since 1970.
    pub at: u64,
    /// Plain text: never read as markup.
    pub text: String,
}

/// A chat line as the host passes it on: control characters other than
/// line breaks and tabs left out, blank lines at either end trimmed, cut to
/// [`CHAT_MOST`] characters. `None` when nothing is left.
pub fn tidy_chat(text: &str) -> Option<String> {
    let kept: String = text
        .chars()
        .filter(|&c| !c.is_control() || c == '\n' || c == '\t')
        .take(CHAT_MOST)
        .collect();
    let kept = kept.trim();
    (!kept.is_empty()).then(|| kept.to_owned())
}

/// Where a member's cursor is in a live script or stylesheet (C8): the
/// file, and the selection's two ends as sticky places in its document
/// ([`LiveDoc::sticky`](crate::live::LiveDoc::sticky)).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextCursor {
    pub path: String,
    pub anchor: Vec<u8>,
    pub head: Vec<u8>,
}

/// What a member calls themselves, and their colour.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    pub color: [u8; 3],
}

impl Profile {
    /// For a member who hasn't said.
    pub fn unnamed(member: MemberId) -> Profile {
        Profile {
            name: format!("Member {}", member.0),
            color: [128, 128, 128],
        }
    }

    /// Cut to what the host passes on.
    pub(crate) fn tidied(mut self) -> Profile {
        self.name = clip(&self.name);
        self
    }
}

/// One row of the roster.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Collaborator {
    pub member: MemberId,
    pub profile: Profile,
    pub role: Role,
    pub online: bool,
}

/// Where a member's viewport camera is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub position: [f32; 3],
    /// Unit vector the camera looks along.
    pub look: [f32; 3],
}

/// What a member is doing now.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Presence {
    /// What they have selected (shared Instances only).
    pub selection: Vec<InstanceId>,
    pub camera: Option<Camera>,
    /// The Properties field they're changing, such as "Transparency".
    pub editing: Option<String>,
    /// Their cursor in a live script or stylesheet (C8).
    pub script: Option<TextCursor>,
}

impl Presence {
    /// Cut to what the host passes on.
    pub(crate) fn tidied(mut self) -> Presence {
        self.selection.truncate(SELECTION_SHOWN);
        self.selection.retain(|id| !id.is_local());
        self.editing = self.editing.as_deref().map(clip);
        self.script = self.script.filter(|cursor| {
            cursor.path.len() <= PATH_MOST
                && crate::live::is_live(&cursor.path)
                && cursor.anchor.len() <= STICKY_MOST
                && cursor.head.len() <= STICKY_MOST
        });
        self
    }
}

fn clip(text: &str) -> String {
    text.chars().take(TEXT_MOST).collect()
}
