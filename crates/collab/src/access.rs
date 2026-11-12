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
use crate::project::MembersList;
use crate::role::Role;

/// How long an invite works unless the owner says otherwise.
pub const INVITE_LIFETIME_SECONDS: u64 = 7 * 24 * 60 * 60;

const LINK_START: &str = "axcenity://collaborate/join?";

/// Links made before Collaborate was renamed from Team Create still work.
const OLD_LINK_START: &str = "axcenity://teamcreate/join?";

/// Whether `text` looks like an invite link (new or old), as the OS hands
/// one to the editor when someone opens it.
pub fn is_link(text: &str) -> bool {
    let text = text.trim();
    text.starts_with(LINK_START) || text.starts_with(OLD_LINK_START)
}

/// An invite as the joiner gets it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Invite {
    /// Where the host listens, such as `192.168.1.20:47810`.
    pub host: String,
    /// The fingerprint of the host's TLS certificate.
    pub cert: Fingerprint,
    pub secret: [u8; 16],
    /// The session's name as the host set it ("Castle Game"), shown to the
    /// joiner before joining (C4). Only a label: nothing is checked by it.
    pub name: Option<String>,
}

/// The longest session name a link carries.
const NAME_MOST: usize = 80;

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum LinkError {
    #[error("that isn't a Collaborate invite link (they start with {LINK_START})")]
    NotALink,
    #[error("the invite link is missing its {0}; copy the whole link")]
    Missing(&'static str),
}

impl Invite {
    /// `axcenity://collaborate/join?host=…&cert=…&invite=…[&name=…]`
    pub fn link(&self) -> String {
        let mut link = format!(
            "{LINK_START}host={}&cert={}&invite={}",
            self.host,
            self.cert.to_hex(),
            identity::to_hex(&self.secret)
        );
        if let Some(name) = self.name.as_deref().filter(|n| !n.trim().is_empty()) {
            link.push_str("&name=");
            link.push_str(&percent_encode(name));
        }
        link
    }

    pub fn parse(link: &str) -> Result<Invite, LinkError> {
        let link = link.trim();
        let query = link
            .strip_prefix(LINK_START)
            .or_else(|| link.strip_prefix(OLD_LINK_START))
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
        let name = field("name")
            .and_then(percent_decode)
            .map(|n| n.trim().chars().take(NAME_MOST).collect::<String>())
            .filter(|n| !n.is_empty() && !n.chars().any(char::is_control));
        Ok(Invite {
            host: host.to_owned(),
            cert,
            secret,
            name,
        })
    }
}

/// `text` with everything but unreserved URL characters as `%XX` bytes.
fn percent_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// Undoes [`percent_encode`] (and reads `+` as a space); `None` for bad
/// escapes or text that isn't UTF-8.
fn percent_decode(text: &str) -> Option<String> {
    let mut bytes = Vec::with_capacity(text.len());
    let mut rest = text.bytes();
    while let Some(byte) = rest.next() {
        match byte {
            b'%' => {
                let high = (rest.next()? as char).to_digit(16)?;
                let low = (rest.next()? as char).to_digit(16)?;
                bytes.push((high * 16 + low) as u8);
            }
            b'+' => bytes.push(b' '),
            other => bytes.push(other),
        }
    }
    String::from_utf8(bytes).ok()
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
    /// Their colour for presence, as they last said.
    #[serde(default = "grey")]
    pub color: [u8; 3],
    pub role: Role,
    /// The account this member's key belongs to, once an account provider
    /// exists (design §6; nothing sets it yet). Many keys may name one
    /// account; a key with none is a member by its key alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
}

fn grey() -> [u8; 3] {
    [128, 128, 128]
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
    /// Goes up whenever who's a member, or their role, name or colour,
    /// changes; it orders the [`MembersList`] copies the host sends out
    /// (Collaborate 2.0, P3). Older files have none.
    #[serde(default)]
    version: u64,
}

impl Access {
    /// A project with no members yet: its first comes through an invite
    /// (the self-hosted server makes one for an Owner).
    pub fn empty() -> Access {
        Access {
            next_member: 1,
            ..Access::default()
        }
    }

    /// A project whose only member is `owner`.
    pub fn owned_by(owner: PublicKey, name: impl Into<String>) -> Access {
        let mut access = Access {
            next_member: 1,
            ..Access::default()
        };
        access.add(owner, name.into(), grey(), Role::Owner);
        access
    }

    fn add(&mut self, key: PublicKey, name: String, color: [u8; 3], role: Role) -> Member {
        let member = Member {
            id: MemberId(self.next_member),
            key,
            name,
            color,
            role,
            account: None,
        };
        self.next_member += 1;
        self.version += 1;
        self.members.push(member.clone());
        member
    }

    /// Which change of the members this is (see [`MembersList::version`]).
    pub fn version(&self) -> u64 {
        self.version
    }

    /// The members, without the invites, to hand to members who may host
    /// later.
    pub fn members_list(&self) -> MembersList {
        MembersList {
            version: self.version,
            members: self.members.clone(),
            next_member: self.next_member,
        }
    }

    /// Access for a host starting from a members list it was sent (no
    /// invites: those stay with the host that made them).
    pub fn from_list(list: &MembersList) -> Access {
        Access {
            members: list.members.clone(),
            invites: Vec::new(),
            next_member: list.next_member.max(1),
            version: list.version,
        }
    }

    /// Takes `list`'s members if it's newer than ours, keeping our own
    /// invites; true if it did. Used when a host that had been away comes
    /// back to a project whose members changed meanwhile.
    pub fn take_list(&mut self, list: &MembersList) -> bool {
        if list.version <= self.version {
            return false;
        }
        self.members = list.members.clone();
        self.next_member = self.next_member.max(list.next_member);
        self.version = list.version;
        true
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
    /// `invite`. A member's name and colour follow what they say now.
    pub fn admit(
        &mut self,
        key: PublicKey,
        name: &str,
        color: [u8; 3],
        invite: Option<[u8; 16]>,
        now: u64,
    ) -> Result<Member, AccessRefusal> {
        let name = &tidy_name(name);
        if let Some(member) = self.members.iter_mut().find(|m| m.key == key) {
            if member.name != *name || member.color != color {
                member.name = name.to_owned();
                member.color = color;
                self.version += 1;
            }
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
        Ok(self.add(key, name.to_owned(), color, role))
    }

    /// The role [`admit`](Self::admit) would give `key` now, without
    /// using the invite.
    pub fn would_admit(
        &self,
        key: &PublicKey,
        invite: Option<[u8; 16]>,
        now: u64,
    ) -> Result<Role, AccessRefusal> {
        if let Some(member) = self.member(key) {
            return Ok(member.role);
        }
        let secret = invite.ok_or(AccessRefusal::NotInvited)?;
        let hash = Fingerprint::of(&secret);
        let issued = self
            .invites
            .iter()
            .find(|issued| issued.secret == hash)
            .ok_or(AccessRefusal::UnknownInvite)?;
        if issued.used {
            return Err(AccessRefusal::InviteUsed);
        }
        if now > issued.expires {
            return Err(AccessRefusal::InviteExpired);
        }
        Ok(issued.role)
    }

    /// Changes a member's role. False if there's no such member.
    pub fn set_role(&mut self, member: MemberId, role: Role) -> bool {
        let Some(found) = self.members.iter_mut().find(|m| m.id == member) else {
            return false;
        };
        if found.role != role {
            found.role = role;
            self.version += 1;
        }
        true
    }

    /// Takes a member out of the project.
    pub fn remove(&mut self, member: MemberId) {
        let before = self.members.len();
        self.members.retain(|m| m.id != member);
        if self.members.len() != before {
            self.version += 1;
        }
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

/// A joiner's name as the host keeps and shows it: one line, at most 64
/// characters, never empty.
pub fn tidy_name(name: &str) -> String {
    let name: String = name
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(64)
        .collect();
    let name = name.trim();
    if name.is_empty() {
        "Someone".to_owned()
    } else {
        name.to_owned()
    }
}

/// Seconds since 1970, for invites' expiry.
pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

#[cfg(test)]
mod link_tests {
    use super::*;

    #[test]
    fn links_use_the_collaborate_name_and_old_links_still_work() {
        let invite = Invite {
            host: "pauls-pc.example.ts.net:47810".to_owned(),
            cert: Fingerprint([7; 32]),
            secret: [9; 16],
            name: None,
        };
        let link = invite.link();
        assert!(link.starts_with("axcenity://collaborate/join?"), "{link}");
        assert!(is_link(&link));
        assert_eq!(Invite::parse(&link), Ok(invite.clone()));
        let old = link.replace("collaborate", "teamcreate");
        assert!(is_link(&old));
        assert_eq!(Invite::parse(&format!("  {old}\n")), Ok(invite));
        assert!(!is_link("https://example.com"));
        assert_eq!(
            Invite::parse("axcenity://collaborate/other?host=x"),
            Err(LinkError::NotALink)
        );
    }

    #[test]
    fn a_link_can_carry_the_session_name() {
        let invite = Invite {
            host: "100.101.102.103:47810".to_owned(),
            cert: Fingerprint([1; 32]),
            secret: [2; 16],
            name: Some("Castle Game: Zoë's & Sam's".to_owned()),
        };
        let link = invite.link();
        assert!(!link.contains(' '), "{link}");
        assert!(
            link.contains("&name=Castle%20Game%3A%20Zo%C3%AB%27s%20%26%20Sam%27s"),
            "{link}"
        );
        assert_eq!(Invite::parse(&link), Ok(invite.clone()));
        // Links without a name, and names that are bad or too long.
        let plain = Invite {
            name: None,
            ..invite.clone()
        };
        assert_eq!(Invite::parse(&plain.link()), Ok(plain.clone()));
        let bad = format!("{}&name=%E2%28", plain.link());
        assert_eq!(Invite::parse(&bad).unwrap().name, None);
        let control = format!("{}&name=a%0Ab", plain.link());
        assert_eq!(Invite::parse(&control).unwrap().name, None);
        let long = format!("{}&name={}", plain.link(), "x".repeat(500));
        assert_eq!(Invite::parse(&long).unwrap().name.unwrap().len(), 80);
    }

    #[test]
    fn members_files_without_accounts_still_read_and_names_are_one_line() {
        let key = crate::identity::Identity::new("Sam", [1, 2, 3]).public_key();
        let mut access = Access::owned_by(key, "Sam");
        let json = serde_json::to_string(&access).unwrap();
        // Nothing sets an account yet, so none is written.
        assert!(!json.contains("account"), "{json}");
        let back: Access = serde_json::from_str(&json).unwrap();
        assert_eq!(back.members(), access.members());
        // A file from a later version that links an account reads too.
        let linked = json.replace("\"role\":", "\"account\":\"sam@example\",\"role\":");
        let back: Access = serde_json::from_str(&linked).unwrap();
        assert_eq!(back.members()[0].account.as_deref(), Some("sam@example"));

        let secret = access.invite(Role::Editor, 60, now());
        let other = crate::identity::Identity::new("x", [0; 3]).public_key();
        let joined = access
            .admit(other, "Eve\nOwner joined\u{7}", [0; 3], Some(secret), now())
            .unwrap();
        assert_eq!(joined.name, "Eve Owner joined");
        assert_eq!(tidy_name("  \n "), "Someone");
        assert_eq!(tidy_name(&"x".repeat(200)).len(), 64);
    }

    #[test]
    fn the_members_list_version_counts_changes_and_older_lists_dont_replace_newer() {
        let owner = crate::identity::Identity::new("Paul", [1, 2, 3]).public_key();
        let mut access = Access::owned_by(owner, "Paul");
        let first = access.version();
        // The same member joining again unchanged isn't a change.
        access.admit(owner, "Paul", grey(), None, now()).unwrap();
        assert_eq!(access.version(), first);
        let secret = access.invite(Role::Editor, 60, now());
        let sam = crate::identity::Identity::new("Sam", [0; 3]).public_key();
        let joined = access
            .admit(sam, "Sam", [0; 3], Some(secret), now())
            .unwrap();
        assert!(access.version() > first);
        let with_sam = access.members_list();
        assert!(access.set_role(joined.id, Role::Viewer));
        assert!(
            access.set_role(joined.id, Role::Viewer),
            "no change, still found"
        );
        let demoted = access.members_list();
        assert_eq!(demoted.version, with_sam.version + 1);

        // A host starting from a list has the same members and numbers on.
        let mut other = Access::from_list(&demoted);
        assert_eq!(other.members(), access.members());
        assert!(!other.take_list(&with_sam), "an older list is ignored");
        access.remove(joined.id);
        assert!(other.take_list(&access.members_list()));
        assert_eq!(other.members().len(), 1);
        // Older members files (no version) still read.
        let json = serde_json::to_string(&Access::owned_by(owner, "Paul")).unwrap();
        let json = json.replace(",\"version\":1", "");
        assert!(!json.contains("version"), "{json}");
        assert_eq!(serde_json::from_str::<Access>(&json).unwrap().version(), 0);
    }
}
