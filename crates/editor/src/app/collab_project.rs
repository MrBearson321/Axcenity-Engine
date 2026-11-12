//! Collaborate projects that outlive their sessions (Collaborate 2.0, P3;
//! `docs/research/collaborate-persistent.md` §4.2–4.3, ADR-0022).
//!
//! - Every session of a project is hosted under one **project ID**, kept in
//!   the project's `collaborate.json` (written the first time it's
//!   hosted), so members keep one folder for it whoever hosts.
//! - Each computer keeps a **record** of the projects it collaborates on
//!   ([`ProjectRecord`], in the Collaborate data folder, never in the
//!   project): its folder, the members list the host last sent, and how to
//!   reach the host. A member **rejoins** from it without an invite (the
//!   host knows their key), and **hosts for the members** from its own
//!   copy with the same members, roles and Owner.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use axcenity_collab::access::Access;
use axcenity_collab::identity::Fingerprint;
use axcenity_collab::project::{Event, HostCard, MembersList, ProjectRecord, project_id};
use axcenity_collab::session::Remote;
use axcenity_collab::{Capability, MemberId};

use super::team::{Link, Team};
use super::*;

/// The project in `folder`'s persistent ID (see [`project_id`]), written
/// into its `collaborate.json` when the file didn't have it yet. Returns a
/// warning when it couldn't be written (the ID still works this session).
pub(super) fn persistent_project_id(cert: &Fingerprint, folder: &Path) -> (String, Option<String>) {
    let mut file = axcenity_project::load_collaborate_file(folder).unwrap_or_default();
    let id = project_id(file.project.as_deref(), cert, folder);
    if file.project.as_deref() == Some(id.as_str()) {
        return (id, None);
    }
    file.project = Some(id.clone());
    let warning = axcenity_project::save_collaborate_file(folder, &file)
        .err()
        .map(|error| format!("couldn't write the project's ID into collaborate.json: {error}"));
    (id, warning)
}

/// The members a host starts with: its members file, brought up to date
/// by a newer list this computer was sent as a member; else that list (a
/// member hosting from its copy); else just `owner`. The `bool` is true
/// when the list came from the record.
pub(super) fn starting_access(
    loaded: std::io::Result<Access>,
    record: Option<&MembersList>,
    owner: impl FnOnce() -> Access,
) -> Result<(Access, bool), String> {
    match loaded {
        Ok(mut access) => {
            let took = record.is_some_and(|list| access.take_list(list));
            Ok((access, took))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(match record {
            Some(list) => (Access::from_list(list), true),
            None => (owner(), false),
        }),
        Err(error) => Err(format!("the members list: {error}")),
    }
}

/// What a hosting editor writes down about the project it hosts.
pub(super) struct Hosted<'a> {
    pub data: &'a Path,
    pub id: &'a str,
    pub folder: &'a Path,
    pub name: Option<&'a str>,
    pub members: MembersList,
    pub me: MemberId,
    pub my_name: &'a str,
    pub address: String,
    pub cert: Fingerprint,
}

/// Keeps the hosting computer's record of the project: the members, and
/// itself as the host to reach.
pub(super) fn record_hosted(hosted: Hosted<'_>) -> std::io::Result<()> {
    let mut record = ProjectRecord::load(hosted.data, hosted.id).unwrap_or_else(|| {
        ProjectRecord::new(
            hosted.id,
            project_name(hosted.name, hosted.folder),
            hosted.folder,
        )
    });
    if let Some(name) = hosted.name {
        record.name = name.to_owned();
    }
    record.folder = hosted.folder.to_owned();
    record.take_members(&hosted.members);
    put_host_first(
        &mut record,
        HostCard {
            member: Some(hosted.me),
            address: hosted.address,
            cert: hosted.cert,
        },
    );
    record.last_host = Some(hosted.my_name.to_owned());
    record.state = record.state.after(Event::Hosting).after(Event::InSession);
    record.repository = repository_url(hosted.folder);
    record.save(hosted.data)
}

fn project_name(name: Option<&str>, folder: &Path) -> String {
    name.map(str::to_owned).unwrap_or_else(|| {
        folder.file_name().map_or_else(
            || "Project".to_owned(),
            |n| n.to_string_lossy().into_owned(),
        )
    })
}

fn repository_url(folder: &Path) -> Option<String> {
    axcenity_project::load_collaborate_file(folder)
        .ok()
        .and_then(|file| file.repository)
        .map(|link| link.url)
}

/// Puts `card` first in the record's hosts, keeping the others (by
/// address) after it.
fn put_host_first(record: &mut ProjectRecord, card: HostCard) {
    record.hosts.retain(|h| {
        h.address != card.address && (card.member.is_none() || h.member != card.member)
    });
    record.hosts.insert(0, card);
    record.hosts.truncate(8);
}

impl EditorApp {
    /// Each frame in a session joined over the network: once joined, and
    /// whenever the host sends a newer members list, this computer's
    /// record of the project is written (so it can rejoin, or host, later).
    pub(super) fn record_joined_project(&mut self) {
        let Some(team) = self.team.as_mut() else {
            return;
        };
        if team.hosting().is_some() {
            return;
        }
        let (Some(data), Some(cert), Some(client), Some(folder)) = (
            team.data.clone(),
            team.host_cert,
            team.client(),
            team.files_folder().map(Path::to_owned),
        ) else {
            return;
        };
        let Some(id) = client
            .session()
            .filter(|id| axcenity_collab::project::is_project_id(id))
        else {
            return;
        };
        if !client.is_joined() {
            return;
        }
        let version = client.members_list().map(|list| list.version);
        if team.recorded == Some(version) {
            return;
        }
        let id = id.to_owned();
        let members = client.members_list().cloned();
        let address = team.address().to_owned();
        let name = team.session_name.clone();
        let host_name = client
            .roster()
            .iter()
            .find(|c| c.role == axcenity_collab::Role::Owner && c.online)
            .map(|c| c.profile.name.clone());
        team.recorded = Some(version);
        let mut record = ProjectRecord::load(&data, &id).unwrap_or_else(|| {
            ProjectRecord::new(&id, project_name(name.as_deref(), &folder), &folder)
        });
        if let Some(name) = name {
            record.name = name;
        }
        record.folder = folder;
        if let Some(list) = &members {
            record.take_members(list);
        }
        put_host_first(
            &mut record,
            HostCard {
                member: None,
                address,
                cert,
            },
        );
        if host_name.is_some() {
            record.last_host = host_name;
        }
        record.state = record.state.after(Event::InSession);
        record.repository = repository_url(&record.folder);
        if let Err(error) = record.save(&data) {
            log::warn!("Collaborate: couldn't keep this computer's record of the project: {error}");
        }
    }

    /// Notes in this computer's record of the session's project that the
    /// session is over for it (`pending`: changes still to upload).
    pub(super) fn record_left_project(&self, team: &Team) {
        let (Some(data), Some(id)) = (
            team.data.as_deref(),
            team.client().and_then(|c| c.session()),
        ) else {
            return;
        };
        let Some(mut record) = ProjectRecord::load(data, id) else {
            return;
        };
        let pending = self.repo.link.is_some() && self.repo.schedule.has_unuploaded();
        record.state = record.state.after(Event::Left { pending });
        if let Err(error) = record.save(data) {
            log::warn!(
                "Collaborate: couldn't update this computer's record of the project: {error}"
            );
        }
    }

    /// The projects this computer collaborates on, most recent first
    /// (`data` is the Collaborate data folder; the editor's own when
    /// `None`).
    pub(crate) fn collaborate_projects(&self, data: Option<&Path>) -> Vec<ProjectRecord> {
        data.map(Path::to_owned)
            .or_else(super::team::data_folder)
            .map(|data| ProjectRecord::all(&data))
            .unwrap_or_default()
    }

    /// Rejoins a project's session from this computer's record, without
    /// an invite: the host knows this member by its key. Tries the host
    /// it last knew (P4 adds the others in turn).
    pub(crate) fn rejoin_project(
        &mut self,
        record: &ProjectRecord,
        data: Option<&Path>,
    ) -> Result<(), String> {
        let data = data
            .map(Path::to_owned)
            .or_else(super::team::data_folder)
            .ok_or("there's no folder to keep Collaborate's files in")?;
        let host = record
            .hosts
            .first()
            .ok_or("this computer doesn't know where the project is hosted")?;
        let identity =
            super::team::load_identity(&data).map_err(|e| format!("your identity: {e}"))?;
        if let Some(list) = &record.members
            && !list.members.iter().any(|m| m.key == identity.public_key())
        {
            return Err("you're no longer a member of this project".to_owned());
        }
        self.output.info(format!(
            "Collaborate: rejoining “{}” at {}",
            record.name, host.address
        ));
        let remote = Remote::join(host.address.clone(), host.cert, None, Arc::new(identity));
        let mut team = Team::new(
            Link::Net {
                remote: Box::new(remote),
                hosting: None,
            },
            host.address.clone(),
            None,
        );
        team.mirrors_home = Some(mirrors_home(&data));
        team.session_name = Some(record.name.clone());
        team.data = Some(data);
        team.host_cert = Some(host.cert);
        self.team = Some(team);
        Ok(())
    }

    /// Hosts the open project's session for its members, with no invite:
    /// a member's copy can host with the members list it was sent, and the
    /// members rejoin by key. Only the Owner invites new people.
    pub(crate) fn host_for_members(
        &mut self,
        data: Option<&Path>,
        port: u16,
    ) -> Result<String, String> {
        self.host_session_on(data, port, true, None, false)
            .map(|hosted| hosted.address)
    }
}

impl EditorApp {
    /// The Console's `projects` (the panel's list comes in P6).
    pub(super) fn projects_command(&mut self) {
        let records = self.collaborate_projects(None);
        if records.is_empty() {
            self.output
                .info("This computer hasn't hosted or joined a Collaborate project yet.");
            return;
        }
        for record in records {
            let host = record
                .last_host
                .as_deref()
                .map_or_else(String::new, |name| format!(", last hosted by {name}"));
            self.output.info(format!(
                "{} ({}): {}{host}. `{}`",
                record.name,
                record.id,
                record.state.word(),
                record.folder.display()
            ));
        }
    }

    /// The Console's `rejoin <project>`, by name or ID.
    pub(super) fn rejoin_command(&mut self, name: &str) {
        if self.team.is_some() {
            self.output
                .info("Already in a session: leave it first to rejoin another.");
            return;
        }
        let records = self.collaborate_projects(None);
        let wanted = name.trim();
        let found = records
            .iter()
            .find(|r| r.id == wanted)
            .or_else(|| records.iter().find(|r| r.name.eq_ignore_ascii_case(wanted)));
        let Some(record) = found.cloned() else {
            self.output.error(format!(
                "No Collaborate project named “{wanted}” here: `projects` lists them."
            ));
            return;
        };
        if let Err(error) = self.rejoin_project(&record, None) {
            self.output.error(format!("Collaborate: {error}"));
        }
    }

    /// The Console's `host`: hosts the open project for its members.
    pub(super) fn host_command(&mut self) {
        if self.team.is_some() {
            self.output.info("Already in a session.");
            return;
        }
        match self.host_for_members(None, axcenity_collab::socket::DEFAULT_PORT) {
            Ok(address) => self.output.info(format!(
                "Collaborate: hosting on {address} for the project's members; they rejoin \
                 without an invite"
            )),
            Err(error) => self.output.error(format!("Collaborate: {error}")),
        }
    }
}

/// True when `role` may make invites (the Owner).
pub(super) fn may_invite(role: axcenity_collab::Role) -> bool {
    role.can(Capability::Invite)
}

/// Where a session's members keep their copies: `data`'s `Projects`.
pub(super) fn mirrors_home(data: &Path) -> PathBuf {
    data.join("Projects")
}
