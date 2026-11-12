//! Collaborate projects that outlive their sessions (Collaborate 2.0, P3
//! of `docs/research/collaborate-persistent.md`, ADR-0022).
//!
//! - **The project ID** names a project wherever it's hosted: from the
//!   project's `collaborate.json` when it has one, else the first host's
//!   session ID (so folders made before Collaborate 2.0 keep their names).
//!   Every later host hosts under the same ID, and every member keeps one
//!   folder for it.
//! - **The members list travels:** the host sends it ([`MembersList`]) to
//!   every member who may host, so any of them can host later with the
//!   same members, roles and Owner. Ownership never moves with hosting.
//!   A list only ever replaces an older one ([`MembersList::version`]).
//! - **Each computer's record** of the projects it collaborates on
//!   ([`ProjectRecord`], in the Collaborate data folder, never the
//!   project): the folder, the members list, the hosts it last knew, where
//!   its copy was, and the session's state. Rejoining without an invite
//!   reads it.
//! - **The session's state** ([`SessionState`]): Offline, Starting,
//!   Active, Handoff, Recovering, Sync Pending.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::access::Member;
use crate::identity::Fingerprint;
use crate::message::MemberId;
use crate::role::Role;

/// Where project records are kept, inside the Collaborate data folder.
pub const RECORDS: &str = "records";

/// The project's persistent ID: `file_id` (from `collaborate.json`) when
/// set, else the session ID this host would have used before (its
/// certificate and the folder).
pub fn project_id(file_id: Option<&str>, cert: &Fingerprint, folder: &Path) -> String {
    match file_id.map(str::trim).filter(|id| is_project_id(id)) {
        Some(id) => id.to_owned(),
        None => crate::hosted::session_id(cert, folder),
    }
}

/// True for a usable project ID: 8 to 64 of `0-9a-z-`.
pub fn is_project_id(id: &str) -> bool {
    (8..=64).contains(&id.len())
        && id
            .chars()
            .all(|c| c.is_ascii_digit() || c.is_ascii_lowercase() || c == '-')
}

/// The project's members as the host last had them: who may join, as
/// what, and who owns the project. Public keys only: nothing secret, and
/// invites (whose secrets stay with the host that made them) aren't in it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MembersList {
    /// Goes up with every change to who's a member or their role; a list
    /// only replaces an older one.
    pub version: u64,
    pub members: Vec<Member>,
    /// The number the next new member gets.
    pub next_member: u32,
}

impl MembersList {
    /// The project's Owner (the first, if somehow there are several).
    pub fn owner(&self) -> Option<&Member> {
        self.members.iter().find(|m| m.role == Role::Owner)
    }

    pub fn member(&self, id: MemberId) -> Option<&Member> {
        self.members.iter().find(|m| m.id == id)
    }

    /// True if `self` should replace `old` (it's newer).
    pub fn is_newer_than(&self, old: Option<&MembersList>) -> bool {
        old.is_none_or(|old| self.version > old.version)
    }
}

/// Where a live session stands for this computer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SessionState {
    /// No session this computer can see.
    #[default]
    Offline,
    /// This computer is starting one (as the first host or a successor).
    Starting,
    /// Collaborating.
    Active,
    /// The host is changing.
    Handoff,
    /// A new host is rebuilding the latest state before taking edits.
    Recovering,
    /// Collaboration is over or paused and changes still have to reach
    /// the remote repository.
    SyncPending,
}

/// What happened to the session, as far as this computer can tell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// This computer starts hosting (or a successor of it does).
    Hosting,
    /// Joined a session (or took over and opened for edits).
    InSession,
    /// The host said it's handing over, or went quiet past the grace
    /// period: a successor is being picked and reached.
    HostChanging,
    /// This computer is the successor and is collecting the latest state.
    Recovering,
    /// Left, or the session ended; `pending` if changes still have to
    /// reach the remote.
    Left { pending: bool },
    /// The remote has everything.
    Synced,
}

impl SessionState {
    /// The state after `event`.
    pub fn after(self, event: Event) -> SessionState {
        match event {
            Event::Hosting => SessionState::Starting,
            Event::InSession => SessionState::Active,
            Event::HostChanging => SessionState::Handoff,
            Event::Recovering => SessionState::Recovering,
            Event::Left { pending: true } => SessionState::SyncPending,
            Event::Left { pending: false } => SessionState::Offline,
            Event::Synced => match self {
                SessionState::SyncPending => SessionState::Offline,
                other => other,
            },
        }
    }

    /// The state's name, in the brief's words.
    pub fn word(self) -> &'static str {
        match self {
            SessionState::Offline => "Offline",
            SessionState::Starting => "Starting",
            SessionState::Active => "Active",
            SessionState::Handoff => "Handoff",
            SessionState::Recovering => "Recovering",
            SessionState::SyncPending => "Sync Pending",
        }
    }

    /// A sentence for the panel.
    pub fn explain(self) -> &'static str {
        match self {
            SessionState::Offline => "No one is collaborating on this project right now.",
            SessionState::Starting => "Starting the session…",
            SessionState::Active => "Collaborating.",
            SessionState::Handoff => {
                "The host is changing. Your edits are kept and sent once the new host is ready."
            }
            SessionState::Recovering => {
                "The new host is gathering everyone's latest changes before editing carries on."
            }
            SessionState::SyncPending => {
                "The session is over; some changes still have to be uploaded."
            }
        }
    }

    /// True while edits may be made and sent.
    pub fn takes_edits(self) -> bool {
        self == SessionState::Active
    }
}

/// How to reach a member's computer if it hosts: its address (a Tailscale
/// name, or a LAN address, with the port) and its host certificate.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostCard {
    pub member: MemberId,
    /// `host:port`.
    pub address: String,
    pub cert: Fingerprint,
}

/// What this computer remembers about a project it collaborates on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectRecord {
    pub id: String,
    pub name: String,
    /// This computer's folder of the project (its own project when it
    /// hosted first, its copy when it joined).
    pub folder: PathBuf,
    /// The members list as last received (or kept, when hosting).
    #[serde(default)]
    pub members: Option<MembersList>,
    /// The hosts and would-be hosts last known, best first.
    #[serde(default)]
    pub hosts: Vec<HostCard>,
    /// Who hosted last, by name, for "Hosted by …".
    #[serde(default)]
    pub last_host: Option<String>,
    #[serde(default)]
    pub state: SessionState,
    /// The remote repository's URL, if the project has one.
    #[serde(default)]
    pub repository: Option<String>,
    /// Seconds since 1970 this record last changed.
    #[serde(default)]
    pub updated: u64,
}

impl ProjectRecord {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        folder: impl Into<PathBuf>,
    ) -> ProjectRecord {
        ProjectRecord {
            id: id.into(),
            name: name.into(),
            folder: folder.into(),
            members: None,
            hosts: Vec::new(),
            last_host: None,
            state: SessionState::Offline,
            repository: None,
            updated: crate::access::now(),
        }
    }

    fn path(data: &Path, id: &str) -> PathBuf {
        data.join(RECORDS).join(format!("{id}.json"))
    }

    /// The record of project `id` in the Collaborate data folder `data`.
    pub fn load(data: &Path, id: &str) -> Option<ProjectRecord> {
        if !is_project_id(id) {
            return None;
        }
        let text = std::fs::read_to_string(Self::path(data, id)).ok()?;
        serde_json::from_str(&text).ok()
    }

    /// Writes the record (only this computer's user can read it: it holds
    /// members' public keys).
    pub fn save(&mut self, data: &Path) -> std::io::Result<()> {
        if !is_project_id(&self.id) {
            return Err(std::io::Error::other("not a project ID"));
        }
        self.updated = crate::access::now();
        let path = Self::path(data, &self.id);
        if let Some(folder) = path.parent() {
            std::fs::create_dir_all(folder)?;
        }
        let text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        crate::identity::write_private(&path, text.as_bytes())
    }

    /// Every record in `data`, most recently changed first.
    pub fn all(data: &Path) -> Vec<ProjectRecord> {
        let Ok(entries) = std::fs::read_dir(data.join(RECORDS)) else {
            return Vec::new();
        };
        let mut records: Vec<ProjectRecord> = entries
            .flatten()
            .filter_map(|entry| {
                let text = std::fs::read_to_string(entry.path()).ok()?;
                serde_json::from_str(&text).ok()
            })
            .collect();
        records.sort_by(|a, b| b.updated.cmp(&a.updated).then(a.name.cmp(&b.name)));
        records
    }

    /// Takes `list` if it's newer than the one kept. True if it was.
    pub fn take_members(&mut self, list: &MembersList) -> bool {
        if list.is_newer_than(self.members.as_ref()) {
            self.members = Some(list.clone());
            true
        } else {
            false
        }
    }

    /// True when nobody opened this project here for `days` days.
    pub fn is_old(&self, now: u64, days: u64) -> bool {
        now.saturating_sub(self.updated) > days * 24 * 60 * 60
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::PublicKey;

    fn member(id: u32, role: Role) -> Member {
        Member {
            id: MemberId(id),
            key: PublicKey([id as u8; 32]),
            name: format!("m{id}"),
            color: [1, 2, 3],
            role,
            account: None,
        }
    }

    fn list(version: u64) -> MembersList {
        MembersList {
            version,
            members: vec![member(1, Role::Owner), member(2, Role::Editor)],
            next_member: 3,
        }
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "axcenity-collab-project-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_project_id_comes_from_the_file_else_the_first_hosts_session() {
        let cert = Fingerprint([7; 32]);
        let folder = Path::new("/projects/castle");
        let old = crate::hosted::session_id(&cert, folder);
        assert_eq!(project_id(None, &cert, folder), old);
        assert_eq!(
            project_id(Some("0123456789abcdef"), &cert, folder),
            "0123456789abcdef"
        );
        // Another host, same file: same ID.
        assert_eq!(
            project_id(
                Some("0123456789abcdef"),
                &Fingerprint([9; 32]),
                Path::new("/x")
            ),
            "0123456789abcdef"
        );
        // Junk in the file is ignored, never used as a folder name.
        assert_eq!(project_id(Some("../../etc"), &cert, folder), old);
    }

    #[test]
    fn only_a_newer_members_list_replaces_one() {
        let mut record = ProjectRecord::new("0123456789abcdef", "Castle", "/x");
        assert!(record.take_members(&list(3)));
        assert!(
            !record.take_members(&list(2)),
            "an older list from a stale copy"
        );
        assert!(!record.take_members(&list(3)));
        assert!(record.take_members(&list(4)));
        assert_eq!(record.members.as_ref().unwrap().version, 4);
        assert_eq!(
            record.members.as_ref().unwrap().owner().unwrap().id,
            MemberId(1)
        );
    }

    #[test]
    fn records_are_kept_by_project_and_listed_newest_first() {
        let data = temp("records");
        let mut castle = ProjectRecord::new("0123456789abcdef", "Castle", "/a");
        castle.members = Some(list(1));
        castle.hosts.push(HostCard {
            member: MemberId(2),
            address: "sam-pc.tail1234.ts.net:47810".into(),
            cert: Fingerprint([5; 32]),
        });
        castle.save(&data).unwrap();
        let mut forest = ProjectRecord::new("fedcba9876543210", "Forest", "/b");
        forest.save(&data).unwrap();
        forest.updated += 10;
        let text = serde_json::to_string(&forest).unwrap();
        std::fs::write(data.join(RECORDS).join("fedcba9876543210.json"), text).unwrap();
        let loaded = ProjectRecord::load(&data, "0123456789abcdef").unwrap();
        assert_eq!(loaded.hosts, castle.hosts);
        assert_eq!(loaded.members, castle.members);
        let all = ProjectRecord::all(&data);
        assert_eq!(
            all.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
            vec!["Forest", "Castle"]
        );
        assert!(ProjectRecord::load(&data, "../secrets").is_none());
    }

    #[test]
    fn session_states_follow_the_brief() {
        use SessionState::*;
        let s = Offline.after(Event::Hosting);
        assert_eq!(s, Starting);
        let s = s.after(Event::InSession);
        assert_eq!(s, Active);
        assert!(s.takes_edits());
        let s = s.after(Event::HostChanging);
        assert_eq!(s, Handoff);
        assert!(!s.takes_edits());
        let s = s.after(Event::Recovering);
        assert_eq!(s, Recovering);
        let s = s.after(Event::InSession);
        assert_eq!(s, Active);
        let s = s.after(Event::Left { pending: true });
        assert_eq!(s, SyncPending);
        assert_eq!(s.word(), "Sync Pending");
        assert_eq!(s.after(Event::Synced), Offline);
        assert_eq!(Active.after(Event::Synced), Active);
        assert_eq!(Active.after(Event::Left { pending: false }), Offline);
    }

    #[test]
    fn old_records_are_noticed() {
        let mut record = ProjectRecord::new("0123456789abcdef", "Castle", "/a");
        record.updated = 1_000;
        assert!(!record.is_old(1_000 + 89 * 86_400, 90));
        assert!(record.is_old(1_000 + 91 * 86_400, 90));
    }
}
