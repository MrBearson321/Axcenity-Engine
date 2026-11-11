//! Roles and what they allow. A role is a set of [`Capability`]s, so new
//! roles (a Builder who only edits the world, a Scripter) are a new row,
//! not new checks.

/// A member's role in a collaborative project.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Role {
    /// Full control: content, members, settings, ending the session.
    Owner,
    /// Changes the project's content.
    Editor,
    /// Sees everything, changes nothing.
    Viewer,
}

/// One thing a member may do. The host checks these, never the client.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Capability {
    /// Open the session, see presence, read everything.
    View,
    /// Change Instances, terrain and the string table.
    EditWorld,
    /// Change scripts and other documents.
    EditScripts,
    /// Add, change and delete asset files.
    EditAssets,
    /// Ask the host to save a checkpoint.
    SaveCheckpoint,
    /// Make and revoke invites.
    Invite,
    /// Change roles and remove members.
    ManageMembers,
    /// Project settings, levels, Build Game settings, plugins.
    ProjectSettings,
    /// End the session, hand over ownership.
    EndSession,
}

impl Role {
    pub fn can(self, capability: Capability) -> bool {
        use Capability::*;
        match self {
            Role::Owner => true,
            Role::Editor => matches!(
                capability,
                View | EditWorld | EditScripts | EditAssets | SaveCheckpoint
            ),
            Role::Viewer => capability == View,
        }
    }
}
