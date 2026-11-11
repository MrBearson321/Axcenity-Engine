//! Who's in a session and what they're doing (TC4; design §7.9 and §11).
//!
//! The host keeps the **roster**: every member's name, colour, role and
//! whether they're connected, sent to everyone whenever it changes. Each
//! member also sends their **presence** (what they have selected, where
//! their camera is, the field they're changing) when it changes; the host
//! passes it on to the others and keeps the latest, so someone joining sees
//! it at once. Presence isn't part of the World and never goes in the log.

use axcenity_core::InstanceId;
use serde::{Deserialize, Serialize};

use crate::message::MemberId;
use crate::role::Role;

/// The most Instances a presence lists as selected; a bigger selection is
/// cut to this many.
pub const SELECTION_SHOWN: usize = 256;

/// The longest name, and the longest `editing` text, the host passes on.
pub const TEXT_MOST: usize = 64;

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
}

impl Presence {
    /// Cut to what the host passes on.
    pub(crate) fn tidied(mut self) -> Presence {
        self.selection.truncate(SELECTION_SHOWN);
        self.selection.retain(|id| !id.is_local());
        self.editing = self.editing.as_deref().map(clip);
        self
    }
}

fn clip(text: &str) -> String {
    text.chars().take(TEXT_MOST).collect()
}
