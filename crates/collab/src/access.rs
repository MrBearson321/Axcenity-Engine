//! Who may join a hosted project (TC3; design §5 and §6): its members by
//! public key, each with a role, and the invites the owner has made. The
//! host keeps this outside the project folder, so a project pushed to a
//! public repository never carries members' names or keys.
//!
//! An invite is a link naming the host's address, its certificate's
//! fingerprint (so the joiner trusts exactly that host) and a one-time
//! secret. Redeeming it binds the joiner's key to the invite's role. By
//! default an invite works once and for 7 days.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::identity::{self, Fingerprint, PublicKey};
use crate::message::MemberId;
use crate::role::Role;

/// How long an invite works unless the owner says otherwise.
pub const INVITE_LIFETIME_SECONDS: u64 = 7 * 24 * 60 * 60;

const LINK_START: &str = "axcenity://teamcreate/join?";

/// An invite as the joiner gets it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Invite {
    /// Where the host listens, such as `192.168.1.20:47810`.
    pub host: String,
    /// The fingerprint of the host's TLS certificate.
    pub cert: Fingerprint,
    pub secret: [u8; 16],
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum LinkError {
    #[error("that isn't a Team Create invite link (they start with {LINK_START})")]
    NotALink,
    #[error("the invite link is missing its {0}; copy the whole link")]
    Missing(&'static str),
}

impl Invite {
    /// `axcenity://teamcreate/join?host=…&cert=…&invite=…`
    pub fn link(&self) -> String {
        format!(
            "{LINK_START}host={}&cert={}&invite={}",
            self.host,
            self.cert.to_hex(),
            identity::to_hex(&self.secret)
        )
    }

    pub fn parse(link: &str) -> Result<Invite, LinkError> {
        let query = link
            .trim()
            .strip_prefix(LINK_START)
            .ok_or(LinkError::NotALink)?;
        let field = |name: &str| {
            query
                .split('&')
                .find_map(|pair| pair.strip_prefix(name)?.strip_prefix('='))
        };
        let host = field("host")
            .filter(|h| !h.is_empty())
            .ok_or(LinkError::Missing("host"))?;
        let cert = field("cert")
            .and_then(Fingerprint::from_hex)
            .ok_or(LinkError::Missing("certificate"))?;
        let secret = field("invite")
            .and_then(identity::from_hex)
            .and_then(|bytes| bytes.try_into().ok())
            .ok_or(LinkError::Missing("invite code"))?;
        Ok(Invite {
            host: host.to_owned(),
            cert,
            secret,
        })
    }
}

/// Why someone wasn't let in.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum AccessRefusal {
    #[error("you aren't a member of this project: ask its owner for an invite")]
    NotInvited,
    #[error("this invite was already used: ask the project's owner for a new one")]
    InviteUsed,
    #[error("this invite has expired: ask the project's owner for a new one")]
    InviteExpired,
    #[error("this invite isn't one the host made (it may have been revoked)")]
    UnknownInvite,
}

/// A member of a hosted project.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Member {
    pub id: MemberId,
    pub key: PublicKey,
    pub name: String,
    pub role: Role,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Issued {
    /// SHA-256 of the secret: the host never keeps the secret itself.
    secret: Fingerprint,
    role: Role,
    /// Seconds since 1970 after which it no longer works.
    expires: u64,
    used: bool,
}

/// A hosted project's members and invites.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Access {
    members: Vec<Member>,
    invites: Vec<Issued>,
    next_member: u32,
}

impl Access {
    /// A project whose only member is `owner`.
    pub fn owned_by(owner: PublicKey, name: impl Into<String>) -> Access {
        let mut access = Access {
            next_member: 1,
            ..Access::default()
        };
        access.add(owner, name.into(), Role::Owner);
        access
    }

    fn add(&mut self, key: PublicKey, name: String, role: Role) -> Member {
        let member = Member {
            id: MemberId(self.next_member),
            key,
            name,
            role,
        };
        self.next_member += 1;
        self.members.push(member.clone());
        member
    }

    pub fn members(&self) -> &[Member] {
        &self.members
    }

    pub fn member(&self, key: &PublicKey) -> Option<&Member> {
        self.members.iter().find(|m| m.key == *key)
    }

    /// Makes an invite for `role`, working once until `lifetime` seconds
    /// from `now`; returns its secret for the link.
    pub fn invite(&mut self, role: Role, lifetime: u64, now: u64) -> [u8; 16] {
        let secret = identity::random::<16>();
        self.invites.push(Issued {
            secret: Fingerprint::of(&secret),
            role,
            expires: now.saturating_add(lifetime),
            used: false,
        });
        secret
    }

    /// Revokes every invite not used yet.
    pub fn revoke_invites(&mut self) {
        self.invites.retain(|issued| issued.used);
    }

    /// Lets `key` in: as the member it is, or as a new one through
    /// `invite`. A member's name follows what they call themselves now.
    pub fn admit(
        &mut self,
        key: PublicKey,
        name: &str,
        invite: Option<[u8; 16]>,
        now: u64,
    ) -> Result<Member, AccessRefusal> {
        if let Some(member) = self.members.iter_mut().find(|m| m.key == key) {
            member.name = name.to_owned();
            return Ok(member.clone());
        }
        let secret = invite.ok_or(AccessRefusal::NotInvited)?;
        let hash = Fingerprint::of(&secret);
        let issued = self
            .invites
            .iter_mut()
            .find(|issued| issued.secret == hash)
            .ok_or(AccessRefusal::UnknownInvite)?;
        if issued.used {
            return Err(AccessRefusal::InviteUsed);
        }
        if now > issued.expires {
            return Err(AccessRefusal::InviteExpired);
        }
        issued.used = true;
        let role = issued.role;
        Ok(self.add(key, name.to_owned(), role))
    }

    /// Changes a member's role. False if there's no such member.
    pub fn set_role(&mut self, member: MemberId, role: Role) -> bool {
        let found = self.members.iter_mut().find(|m| m.id == member);
        found.map(|m| m.role = role).is_some()
    }

    /// Takes a member out of the project.
    pub fn remove(&mut self, member: MemberId) {
        self.members.retain(|m| m.id != member);
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(folder) = path.parent() {
            std::fs::create_dir_all(folder)?;
        }
        let text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        identity::write_private(path, text.as_bytes())
    }

    pub fn load(path: &Path) -> std::io::Result<Access> {
        let text = std::fs::read_to_string(path)?;
        serde_json::from_str(&text).map_err(std::io::Error::other)
    }
}

/// Seconds since 1970, for invites' expiry.
pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}
