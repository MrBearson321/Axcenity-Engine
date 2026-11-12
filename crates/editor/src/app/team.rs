//! Collaborate in the editor (TC2 to TC5 of `docs/research/team-create.md`).
//!
//! While in a session, the edit World keeps an edit journal, and each frame
//! [`EditorApp::team_frame`] sends what changed since the last frame as one
//! transaction named after the newest undo step (`collab::Client::capture`),
//! then applies what the others did. So every tool, plugin and AI client
//! syncs without code of its own. Nothing moves while a gesture is open (a
//! drag, typing a name): the frame after it ends sends it and catches up.
//!
//! Undo in a session only changes what's still as the step left it
//! (`History::set_shared`), and its result syncs like any edit. Steps that
//! swap the whole World (Console commands, prefab Apply) can't be undone in
//! a session yet, and terrain isn't shared yet: the Output says so.
//!
//! Files (TC5): every editor in a session keeps a folder of the session's
//! files in step with `collab::Mirror`, every half second and right after
//! an import (whose files and placed Instances then go as one step). The
//! host's folder is its project; a joining editor fetches the files into a
//! folder of its own (under the Collaborate data folder, one per session)
//! and opens it as the project once they're in, keeping the session's
//! World and pointing the project's scripts at the session's Instances
//! ([`EditorApp::open_team_mirror`]); the start screen's Join… does this
//! with no project open ([`EditorApp::join_from_start`]). A document with
//! unsaved changes (a script or stylesheet tab, a block script, a UI Event
//! Graph, a behavior tree, dialogue, data table or material) asks to be
//! the one editing that file; the others' tabs for it are read-only until
//! it's saved or closed. What a teammate saves is read again like a file
//! another program changed (`disk_changed`), the string table and the
//! input map included.
//!
//! Reliability (TC6): the hosting editor's host keeps a journal in the
//! project (`collab::journal`). Others' edits are changes to save in the
//! host's project; ending the session or quitting with them saved (or let
//! go) removes the journal, and opening a project whose session stopped
//! without ending brings its World back ([`EditorApp::recover_team_session`]).
//! A joining editor keeps its edits the host hasn't confirmed in a
//! recovery file in its folder while it's offline ([`EditorApp::keep_unsent`]),
//! and sends them the next time it joins that session. Edits sent again
//! after a gap that no longer apply are listed in the Recovered Changes
//! window. When the host ends the session, the copy stays open, read-only.
//! Following a member's camera is the panel's.
//!
//! What SplineMeshes and SplineScatters make isn't shared: each copy makes
//! its own ([`World::unrecorded`] in `generate_frame`).
//!
//! Over the network (TC3), the hosting editor runs the session's host on
//! its own thread and joins it over the loopback address like any member
//! (`collab::session`); others join with an invite link. Until the Team
//! Create panel (TC4), `--team-host` and `--team-join <link>` start one
//! from the command line too. The tests also meet in a [`Hub`] in this process.

// The in-process hub is for tests; menus that end a session come in TC4.
#![cfg_attr(not(test), allow(dead_code))]

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use axcenity_collab::access::{Access, Invite};
use axcenity_collab::hosted::{members_file, session_id, session_world};
use axcenity_collab::identity::Identity;
use axcenity_collab::journal;
use axcenity_collab::mirror::{Change, Mirror};
use axcenity_collab::session::{HostSession, Hosting, Remote, RemoteEvent};
use axcenity_collab::socket::{DEFAULT_PORT, HostCert};
use axcenity_collab::{
    Camera, Client, FolderStore, Host, MemberId, Notice, Presence, Role, ToClient, ToHost,
};
use axcenity_project as project;

use crate::start::JoinProgress;

use super::*;

/// Most rounds of messages one frame passes back and forth.
const ROUNDS_PER_FRAME: usize = 8;

/// How often the editor looks at the network while nothing else redraws.
const NET_POLL: std::time::Duration = std::time::Duration::from_millis(50);

/// How often an editor in a session looks at its folder for files saved
/// or arrived (TC5).
const MIRROR_EVERY: std::time::Duration = std::time::Duration::from_millis(500);

/// The least time between two presences this editor sends (design §7.9:
/// at most 10 a second).
const PRESENCE_EVERY: std::time::Duration = std::time::Duration::from_millis(100);

/// `axcenity-editor <project> --collab-host`: host a session for the project.
pub const HOST_FLAG: &str = "--collab-host";
/// `axcenity-editor <project> --collab-join <link>`: join a session.
pub const JOIN_FLAG: &str = "--collab-join";
/// The flags from before Collaborate was renamed from Team Create; they
/// still work.
pub const OLD_HOST_FLAG: &str = "--team-host";

/// Version History's name for the project as it was when a session started.
pub(crate) const SESSION_CHECKPOINT: &str = "Before Collaborate session";
/// Version History's name for a member's copy before edits made offline
/// were sent again.
pub(crate) const OFFLINE_CHECKPOINT: &str = "Before sending changes made offline";
pub const OLD_JOIN_FLAG: &str = "--team-join";

/// A session to start once the project has opened (from the command line).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TeamStart {
    Host,
    Join(Invite),
}

impl TeamStart {
    /// What `args` (after the project folder) ask for.
    pub fn from_args(args: &[String]) -> Result<Option<TeamStart>, String> {
        let mut start = None;
        let mut args = args.iter();
        while let Some(arg) = args.next() {
            if arg == HOST_FLAG || arg == OLD_HOST_FLAG {
                start = Some(TeamStart::Host);
            } else if axcenity_collab::access::is_link(arg) {
                // A bare invite link, as the OS passes one it opens.
                let invite = Invite::parse(arg).map_err(|e| e.to_string())?;
                start = Some(TeamStart::Join(invite));
            } else if arg == JOIN_FLAG || arg == OLD_JOIN_FLAG {
                let link = args
                    .next()
                    .ok_or_else(|| format!("{JOIN_FLAG} needs an invite link"))?;
                let invite = Invite::parse(link).map_err(|e| e.to_string())?;
                start = Some(TeamStart::Join(invite));
            }
        }
        Ok(start)
    }
}

/// Where Collaborate keeps this computer's identity, the host's
/// certificate and each hosted project's members: the editor's data
/// folder, never the project folder (which may be pushed somewhere public).
fn data_folder() -> Option<PathBuf> {
    eframe::storage_dir(crate::APP_ID)
        .map(|data| axcenity_collab::hosted::renamed_folder(&data, "Collaborate", "Team Create"))
}

/// This computer's identity, made the first time.
fn load_identity(folder: &Path) -> std::io::Result<Identity> {
    let path = folder.join("identity.json");
    match Identity::load(&path) {
        Ok(identity) => Ok(identity),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let name = std::env::var("USERNAME")
                .or_else(|_| std::env::var("USER"))
                .unwrap_or_else(|_| "Member".to_owned());
            let key = Identity::new(name.clone(), [0; 3]).public_key();
            // A colour from the key, so each computer keeps its own.
            let color = [key.0[0] | 0x40, key.0[1] | 0x40, key.0[2] | 0x40];
            let identity = Identity::new(name, color);
            identity.save(&path)?;
            Ok(identity)
        }
        Err(error) => Err(error),
    }
}

/// This computer's identity, for Settings > Collaborate (made on first use,
/// as sharing or joining would).
pub(super) fn load_own_identity() -> Result<Identity, String> {
    if cfg!(test) {
        return Ok(Identity::new("Tester", [90, 160, 230]));
    }
    let folder = data_folder().ok_or("there's no folder for Collaborate's files")?;
    load_identity(&folder).map_err(|e| format!("Couldn't read your identity: {e}"))
}

/// Saves a changed name or colour for the next session.
pub(super) fn save_own_identity(identity: &Identity) -> std::io::Result<()> {
    if cfg!(test) {
        return Ok(());
    }
    let folder = data_folder().ok_or_else(|| std::io::Error::other("no data folder"))?;
    identity.save(&folder.join("identity.json"))
}

/// True for a file the editor takes in when it changes on disk (what its
/// watcher reports): scripts, stylesheets, assets (data tables, materials,
/// behavior trees and dialogues among them), UI Event Graphs, the string
/// table and the input map.
fn editor_reads(path: &str) -> bool {
    if axcenity_blocks::ui_graph::is_ui_graph(path)
        || path == project::STRINGS_FILE
        || path == project::INPUT_FILE
    {
        return true;
    }
    let script = (path.starts_with("scripts/") || path.starts_with("objects/"))
        && [
            project::SERVER_SCRIPT_SUFFIX,
            project::CLIENT_SCRIPT_SUFFIX,
            project::MODULE_SCRIPT_SUFFIX,
            project::SERVER_BLOCK_SCRIPT_SUFFIX,
            project::CLIENT_BLOCK_SCRIPT_SUFFIX,
        ]
        .iter()
        .any(|suffix| path.ends_with(suffix));
    script || project::is_stylesheet(path) || project::is_asset_path(path)
}

/// Where `id` is in `world`: each class and name from `game` down.
fn tree_key(world: &World, id: InstanceId) -> Vec<(ClassName, String)> {
    let mut key = Vec::new();
    let mut at = Some(id);
    while let Some(here) = at.and_then(|id| world.get(id)) {
        key.push((here.class(), here.name().to_owned()));
        at = here.parent();
    }
    key.reverse();
    key
}

/// A session's host and each member's waiting messages, in this process.
pub(crate) struct Hub {
    host: Host,
    inboxes: HashMap<MemberId, VecDeque<ToClient>>,
    next_member: u32,
}

pub(crate) type SharedHub = Arc<Mutex<Hub>>;

impl Hub {
    /// A session for `world`, as the hosting editor has it.
    pub(crate) fn new(world: World) -> SharedHub {
        Arc::new(Mutex::new(Hub {
            host: Host::new(session_world(world)),
            inboxes: HashMap::new(),
            next_member: 1,
        }))
    }

    /// Lets a new member in with `role`.
    pub(crate) fn add_member(&mut self, role: Role) -> MemberId {
        let member = MemberId(self.next_member);
        self.next_member += 1;
        self.host.add_member(member, role);
        self.inboxes.insert(member, VecDeque::new());
        member
    }

    pub(crate) fn host(&self) -> &Host {
        &self.host
    }

    fn send(&mut self, from: MemberId, messages: Vec<ToHost>) {
        for message in messages {
            for out in self.host.receive(from, message) {
                self.inboxes
                    .entry(out.to)
                    .or_default()
                    .push_back(out.message);
            }
        }
    }

    fn take(&mut self, member: MemberId) -> Vec<ToClient> {
        self.inboxes
            .get_mut(&member)
            .map(|inbox| inbox.drain(..).collect())
            .unwrap_or_default()
    }
}

/// How this editor reaches its session.
enum Link {
    /// A [`Hub`] in this process (tests).
    Hub(SharedHub),
    /// The network, and the session's host if this editor runs it.
    Net {
        remote: Box<Remote>,
        hosting: Option<Hosting>,
    },
}

/// This editor's place in a session.
pub(crate) struct Team {
    /// Made once the host has let this editor in.
    client: Option<Client>,
    link: Link,
    /// The host refused this editor: the session is over for it.
    ended: bool,
    /// When this editor last said what it's doing.
    presence_sent: Option<std::time::Instant>,
    /// Why the connection is down, or why the host refused this editor.
    problem: Option<String>,
    /// When hosting over Tailscale: how others reach this computer, for
    /// the panel (C3).
    pub(crate) tailscale: Option<String>,
    /// The host's address, as the panel names it.
    address: String,
    /// The newest invite link this editor made, to copy again.
    invite: Option<String>,
    /// This editor's folder of the session's files (TC5): the project
    /// folder when hosting, a folder of its own when joining.
    mirror: Option<Mirror>,
    /// When the mirror last looked at the folder.
    mirror_synced: Option<std::time::Instant>,
    /// Where a joining editor keeps the folders of the sessions it joins.
    /// `None` when hosting, and in-process sessions without files.
    mirrors_home: Option<PathBuf>,
    /// The mirror's folder opens as the project once the session's files
    /// are in it (a joining editor).
    open_mirror: bool,
    /// The newest transaction this editor's copy had last frame.
    seen_seq: Option<u64>,
    /// What the recovery file in the mirror's folder holds (how many edits
    /// and the newest), while there is one.
    unsent_kept: Option<(usize, Option<u64>)>,
    /// The recovery file found on joining was taken in.
    unsent_loaded: bool,
    /// The session's name, from the invite this editor joined with (C4).
    pub(crate) session_name: Option<String>,
}

impl Team {
    fn new(link: Link, address: String, invite: Option<String>) -> Team {
        Team {
            client: None,
            link,
            ended: false,
            presence_sent: None,
            problem: None,
            address,
            invite,
            tailscale: None,
            mirror: None,
            mirror_synced: None,
            mirrors_home: None,
            open_mirror: false,
            seen_seq: None,
            unsent_kept: None,
            unsent_loaded: false,
            session_name: None,
        }
    }

    /// The folder this editor keeps the session's files in, once it has one.
    pub(crate) fn files_folder(&self) -> Option<&Path> {
        self.mirror.as_ref().map(Mirror::root)
    }

    /// True while the session's files are still arriving.
    pub(crate) fn fetching_files(&self) -> bool {
        match (&self.mirror, &self.client) {
            (Some(mirror), Some(client)) => mirror.is_catching_up(client),
            _ => false,
        }
    }

    /// Who (other than this editor) is editing the document at `path`.
    pub(crate) fn editing_elsewhere(&self, path: &str) -> Option<String> {
        let client = self.client.as_ref()?;
        let holder = client.lock_holder(path).filter(|&m| m != client.member())?;
        Some(
            client
                .roster()
                .iter()
                .find(|c| c.member == holder)
                .map_or_else(|| "Someone".to_owned(), |c| c.profile.name.clone()),
        )
    }
}

/// How a session is going, for the panel and the title bar chip.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TeamStatus {
    /// Signing in, or waiting for the session's World.
    Joining,
    /// In step with the host, with `waiting` edits not yet confirmed.
    Connected { waiting: usize },
    /// The connection dropped; editing goes on and `waiting` edits are kept.
    Reconnecting { why: String, waiting: usize },
    /// The host refused this editor: the session is over for it.
    Refused(String),
    /// The owner ended the session: this copy is read-only, with `waiting`
    /// edits kept in its recovery file.
    HostLeft { waiting: usize },
}

impl Team {
    /// How the session is going.
    pub(crate) fn status(&self) -> TeamStatus {
        let waiting = self.client.as_ref().map_or(0, Client::pending);
        if self.client.as_ref().is_some_and(Client::host_left) {
            return TeamStatus::HostLeft { waiting };
        }
        if self.ended {
            return TeamStatus::Refused(self.problem.clone().unwrap_or_default());
        }
        match &self.client {
            Some(client) if client.is_online() => TeamStatus::Connected { waiting },
            _ => match &self.problem {
                Some(why) => TeamStatus::Reconnecting {
                    why: why.clone(),
                    waiting,
                },
                None => TeamStatus::Joining,
            },
        }
    }

    /// This editor's role, once let in.
    pub(crate) fn client_role(&self) -> Option<Role> {
        self.client.as_ref().map(Client::role)
    }

    /// This editor's member, once let in.
    pub(crate) fn me(&self) -> Option<MemberId> {
        self.client.as_ref().map(Client::member)
    }

    /// Everyone in the project, as the host last said.
    pub(crate) fn roster(&self) -> &[axcenity_collab::Collaborator] {
        self.client.as_ref().map_or(&[], Client::roster)
    }

    /// Every other connected member's presence.
    pub(crate) fn presences(&self) -> impl Iterator<Item = (MemberId, &Presence)> {
        self.client
            .iter()
            .flat_map(|client| client.presences().iter().map(|(&m, p)| (m, p)))
    }

    pub(crate) fn address(&self) -> &str {
        &self.address
    }

    pub(crate) fn invite(&self) -> Option<&str> {
        self.invite.as_deref()
    }

    pub(crate) fn client(&self) -> &Client {
        self.client.as_ref().expect("joined")
    }

    /// True once the host has let this editor in.
    pub(crate) fn is_joined(&self) -> bool {
        self.client.is_some()
    }

    /// The session's host, when this editor runs it.
    pub(crate) fn hosting(&self) -> Option<&Hosting> {
        match &self.link {
            Link::Net { hosting, .. } => hosting.as_ref(),
            Link::Hub(_) => None,
        }
    }
}

impl EditorApp {
    /// Starts a session for the open world in this process, with this
    /// editor as its owner, and returns the hub others join (tests).
    pub(crate) fn host_team_session(&mut self) -> SharedHub {
        let hub = Hub::new(self.world.clone());
        let member = hub.lock().expect("hub").add_member(Role::Owner);
        self.join_team_session(hub.clone(), member);
        hub
    }

    /// Joins a session in this process as `member` (one the hub let in).
    /// The edit World is replaced with the session's.
    pub(crate) fn join_team_session(&mut self, hub: SharedHub, member: MemberId) {
        let mut client = Client::new(member);
        client.connect();
        let mut team = Team::new(Link::Hub(hub), String::new(), None);
        team.client = Some(client);
        self.team = Some(team);
        self.team_frame();
    }

    /// Starts `start` once the project being opened has opened.
    pub fn start_team_after_open(&mut self, start: TeamStart) {
        self.team_start = Some(start);
    }

    /// Starts the session asked for on the command line, now that the
    /// project is open.
    pub(super) fn start_pending_team(&mut self) {
        let Some(start) = self.team_start.take() else {
            return;
        };
        let result = match start {
            TeamStart::Host => self.host_collaborate_session().map(|invite| {
                let link = invite.link();
                // On stdout too, to copy from the terminal.
                println!("Collaborate invite (works once, for 7 days): {link}");
                self.output.info(format!(
                    "Collaborate: hosting on {}. Invite: {link}",
                    invite.host
                ));
            }),
            TeamStart::Join(invite) => self.join_network_session(&invite, None),
        };
        if let Err(error) = result {
            self.output.error(format!("Collaborate: {error}"));
        }
    }

    /// Hosts a session for the open project on the usual port, from the
    /// panel or the command line. When Tailscale is connected, invites name
    /// this computer's Tailscale name, so they work from anywhere (C3);
    /// otherwise its LAN address. Output says which.
    pub(crate) fn host_collaborate_session(&mut self) -> Result<Invite, String> {
        self.host_named_session(None, DEFAULT_PORT, None)
    }

    /// [`EditorApp::host_collaborate_session`] on `port`, with invites
    /// carrying the session's `name` (Start Session, C4).
    pub(crate) fn host_named_session(
        &mut self,
        folder: Option<&Path>,
        port: u16,
        name: Option<String>,
    ) -> Result<Invite, String> {
        let invite = self.host_session_on(folder, port, true, name)?;
        let reach = self.team.as_ref().and_then(|team| team.tailscale.clone());
        match reach {
            Some(note) => self.output.info(format!("Collaborate: {note}")),
            None => self.output.info(
                "Collaborate: Tailscale isn't connected, so the invite works on this network only",
            ),
        }
        Ok(invite)
    }

    /// Hosts a session for the open project on `port` (0 picks one), with
    /// this editor joined as its owner; returns an Editor invite naming
    /// the LAN address (tests). `folder` is where Collaborate's files go
    /// (the editor's data folder when `None`).
    pub(crate) fn host_network_session(
        &mut self,
        folder: Option<&Path>,
        port: u16,
    ) -> Result<Invite, String> {
        self.host_session_on(folder, port, false, None)
    }

    fn host_session_on(
        &mut self,
        folder: Option<&Path>,
        port: u16,
        tailscale: bool,
        name: Option<String>,
    ) -> Result<Invite, String> {
        let folder = folder
            .map(Path::to_owned)
            .or_else(data_folder)
            .ok_or("there's no folder to keep Collaborate's files in")?;
        let project = self
            .project
            .as_ref()
            .map(|p| p.folder.clone())
            .ok_or("open a project first")?;
        let identity = load_identity(&folder).map_err(|e| format!("your identity: {e}"))?;
        let cert = HostCert::load_or_make(&folder.join("host-certificate.json"))
            .map_err(|e| format!("the host certificate: {e}"))?;
        let access_path = members_file(&folder, &project);
        let access = match Access::load(&access_path) {
            Ok(access) => access,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Access::owned_by(identity.public_key(), &identity.name)
            }
            Err(error) => return Err(format!("the members list: {error}")),
        };
        // The project's files as they are now; the host keeps what members
        // upload beside them, in `.axcenity` (never synced).
        let (store, files) = FolderStore::open(&project)
            .map_err(|e| format!("couldn't read the project's files: {e}"))?;
        let session_id = session_id(&cert.fingerprint(), &project);
        let mirror = Mirror::open(&project, &session_id, &identity.name);
        // What a session that stopped without ending left is taken in
        // first: starting the journal writes over it.
        self.recover_team_session();
        // The project as saved before anyone else changes it, in Version
        // History (C5). Unpinned: an unchanged project adds nothing.
        if let Err(error) = axcenity_project::record_version(&project, SESSION_CHECKPOINT) {
            log::warn!("Collaborate: couldn't keep a version before the session: {error}");
        }
        let mut host = Host::new(session_world(self.world.clone()))
            .with_files(files, Box::new(store))
            .with_session(session_id);
        // Every accepted change is written down before it's sent, so a
        // crash loses nothing (TC6).
        if let Err(error) = host.start_journal(&project) {
            self.output.warning(format!(
                "Collaborate: couldn't start the session's journal ({error}); save often, \
                 as a crash would lose the changes since the last save"
            ));
        }
        let mut session = HostSession::start(
            host,
            access,
            Some(access_path),
            &cert,
            std::net::SocketAddr::from(([0, 0, 0, 0], port)),
            None,
        )
        .map_err(|e| format!("couldn't listen on port {port}: {e}"))?;
        session.set_name(name);
        let reach = if tailscale {
            match axcenity_collab::tailscale::advertise(&mut session) {
                axcenity_collab::tailscale::State::Running(net) => Some(format!(
                    "reachable over Tailscale as {} ({})",
                    session.address(),
                    net.login.as_deref().unwrap_or("signed in")
                )),
                _ => None,
            }
        } else {
            None
        };
        let invite = session.invite(
            self.team_panel.invite_role.role(),
            self.team_panel.invite_lifetime.seconds(),
        );
        let loopback = session.loopback();
        let hosting = session.serve();
        let remote = Box::new(Remote::join(
            loopback,
            cert.fingerprint(),
            None,
            Arc::new(identity),
        ));
        let mut team = Team::new(
            Link::Net {
                remote,
                hosting: Some(hosting),
            },
            invite.host.clone(),
            Some(invite.link()),
        );
        team.tailscale = reach;
        team.mirror = Some(mirror);
        self.team = Some(team);
        Ok(invite)
    }

    /// Joins the session `invite` names. The edit World is replaced with
    /// the session's once the host lets this editor in.
    pub(crate) fn join_network_session(
        &mut self,
        invite: &Invite,
        folder: Option<&Path>,
    ) -> Result<(), String> {
        let folder = folder
            .map(Path::to_owned)
            .or_else(data_folder)
            .ok_or("there's no folder to keep Collaborate's files in")?;
        let identity = load_identity(&folder).map_err(|e| format!("your identity: {e}"))?;
        self.output.info(format!(
            "Collaborate: joining {} as {} ({})",
            invite.host,
            identity.name,
            identity.public_key().fingerprint().short()
        ));
        let mut team = Team::new(
            Link::Net {
                remote: Box::new(Remote::join_invite(invite, Arc::new(identity))),
                hosting: None,
            },
            invite.host.clone(),
            None,
        );
        team.mirrors_home = Some(folder.join("Projects"));
        team.session_name = invite.name.clone();
        self.team = Some(team);
        Ok(())
    }

    /// The start screen's Join: joins the session `invite` names, with no
    /// project open. The session's files open as the project once they're
    /// in (`open_team_mirror_when_ready`). `folder` is as for
    /// [`Self::join_network_session`].
    pub(crate) fn join_from_start(&mut self, invite: &Invite, folder: Option<&Path>) {
        if self.team.is_some() {
            self.cancel_team_join();
        }
        self.start.show_joining();
        if let Err(error) = self.join_network_session(invite, folder) {
            self.start.join_failed(error);
        }
    }

    /// An invite link the system handed over: the start screen's Join
    /// dialog shows it, with whose session it is, until the user joins.
    pub fn offer_join(&mut self, link: String) {
        self.start.offer_join(link);
    }

    /// Stops joining from the start screen, without a word in Output.
    pub(crate) fn cancel_team_join(&mut self) {
        self.team = None;
        self.world.stop_recording_edits();
        self.history.set_shared(false);
        self.replace_world(World::new());
    }

    /// Each frame on the start screen: tells the Join dialog how joining
    /// goes, and why it stopped if the host refused this editor.
    pub(super) fn follow_start_join(&mut self) {
        let Some(team) = &self.team else {
            self.start.set_join_progress(None);
            return;
        };
        let host = match &team.session_name {
            Some(name) => format!("“{name}” ({})", team.address()),
            None => team.address().to_owned(),
        };
        let progress = match team.status() {
            TeamStatus::HostLeft { .. } => {
                self.cancel_team_join();
                self.start
                    .join_failed("the host ended the session".to_owned());
                return;
            }
            TeamStatus::Refused(why) => {
                self.cancel_team_join();
                let why = if why.is_empty() {
                    "the host refused this computer".to_owned()
                } else {
                    why
                };
                self.start.join_failed(why);
                return;
            }
            TeamStatus::Joining => JoinProgress::Connecting { host },
            TeamStatus::Reconnecting { why, .. } if team.client.is_none() => {
                // The usual reasons, in the user's words (C4).
                let why = if axcenity_collab::tailscale::is_tailscale_host(team.address()) {
                    format!(
                        "{why}. This session is reached over Tailscale: check Tailscale is on \
                         and signed in on this computer, and if you use your own Tailscale \
                         account, that the host shared their computer with you"
                    )
                } else {
                    why
                };
                JoinProgress::Retrying { host, why }
            }
            TeamStatus::Connected { .. } | TeamStatus::Reconnecting { .. } => {
                JoinProgress::Fetching { host }
            }
        };
        self.start.set_join_progress(Some(progress));
    }

    /// A new invite for `role`, when this editor hosts; its link.
    pub(crate) fn make_team_invite(&mut self, role: Role) -> Option<String> {
        let team = self.team.as_mut()?;
        let link = team
            .hosting()?
            .lock()
            .invite(role, self.team_panel.invite_lifetime.seconds())
            .link();
        team.invite = Some(link.clone());
        Some(link)
    }

    /// Revokes every invite not used yet, when this editor hosts.
    pub(crate) fn revoke_team_invites(&mut self) {
        if let Some(hosting) = self.team.as_ref().and_then(Team::hosting) {
            hosting.lock().revoke_invites();
            if let Some(team) = &mut self.team {
                team.invite = None;
            }
            self.output
                .info("Collaborate: invites not used yet no longer work");
        }
    }

    /// Changes a member's role, when this editor hosts.
    pub(crate) fn set_team_role(&mut self, member: MemberId, role: Role) {
        if let Some(hosting) = self.team.as_ref().and_then(Team::hosting) {
            hosting.lock().set_role(member, role);
        }
    }

    /// Takes a member out of the project, when this editor hosts: they're
    /// disconnected and their key no longer gets in.
    pub(crate) fn remove_team_member(&mut self, member: MemberId) {
        if let Some(hosting) = self.team.as_ref().and_then(Team::hosting) {
            hosting.lock().remove(member);
        }
    }

    /// Leaves the session (or ends it, when this editor hosts). The World
    /// stays as it is, for this editor alone. A hosted session with changes
    /// not saved yet keeps its journal: saving lets it go, and if the
    /// editor stops first, opening the project brings the changes back.
    pub(crate) fn leave_team_session(&mut self) {
        let Some(team) = self.team.take() else {
            return;
        };
        let hosted = team.hosting().is_some();
        let unsaved = !self.history.is_saved();
        // A member's edits the host never confirmed go with them next time.
        if !hosted
            && let (Some(client), Some(mirror)) = (&team.client, &team.mirror)
            && client.pending() > 0
            && let Err(error) = journal::save_unsent(mirror.root(), &client.unsent())
        {
            log::warn!("Collaborate: couldn't keep the unsent edits: {error}");
        }
        if let Some(hosting) = team.hosting()
            && !unsaved
        {
            hosting.lock().end_cleanly();
        }
        // Dropping the hosting ends the session for everyone.
        drop(team);
        self.world.stop_recording_edits();
        self.history.set_shared(false);
        self.output.info(match (hosted, unsaved) {
            (true, false) => "Collaborate: you ended the session",
            (true, true) => {
                "Collaborate: you ended the session. Save to keep its changes (they're kept \
                 for you until you do, even if the editor stops)"
            }
            _ => "Collaborate: you left the session; your copy stays open here",
        });
    }

    /// The editor is closing, its changes saved or let go: a hosted
    /// session ends with nothing left to recover.
    pub(crate) fn end_team_for_quit(&mut self) {
        let Some(team) = self.team.take() else {
            return;
        };
        if let Some(hosting) = team.hosting() {
            hosting.lock().end_cleanly();
        }
    }

    /// The World was saved: outside a session, what an ended one left to
    /// recover is in the file now, so it goes.
    pub(super) fn saved_after_team_session(&mut self) {
        if self
            .team
            .as_ref()
            .is_some_and(|team| team.hosting().is_some())
        {
            return;
        }
        if let Some(project) = &self.project
            && journal::has_recovery(&project.folder)
            && let Err(error) = journal::discard(&project.folder)
        {
            log::warn!("Collaborate: couldn't remove the old session's journal: {error}");
        }
    }

    /// Opening a project that hosted a session which stopped without
    /// ending (the editor crashed, or the computer turned off): the
    /// session's World, from its journal, takes the loaded one's place, as
    /// changes to save. Nothing it accepted is lost.
    pub(super) fn recover_team_session(&mut self) {
        let Some(folder) = self.project.as_ref().map(|p| p.folder.clone()) else {
            return;
        };
        if self.team.is_some() || !journal::has_recovery(&folder) {
            return;
        }
        let recovered = match journal::recover(&folder) {
            Ok(Some(recovered)) => recovered,
            Ok(None) => return,
            Err(error) => {
                self.output.warning(format!(
                    "Collaborate: the last session hosted from this project stopped without \
                     ending, and its changes couldn't be read back ({error}). Its files are \
                     kept in `{}`.",
                    journal::FOLDER
                ));
                return;
            }
        };
        let mut world = recovered.world;
        // The string table isn't part of a session's World: the file's.
        world.set_strings(self.world.strings().clone());
        let loaded = std::mem::replace(&mut self.world, World::new());
        self.replace_world(world);
        self.generators = Default::default();
        self.relink_to_session(&loaded);
        self.history.mark_unsaved();
        if let Err(error) = journal::discard(&folder) {
            log::warn!("Collaborate: couldn't remove the recovered journal: {error}");
        }
        self.output.warning(format!(
            "Collaborate: the last session hosted from this project stopped without ending. \
             Its World is back as it was ({} change(s) after its last snapshot): save to keep it.",
            recovered.replayed
        ));
        self.toasts
            .warning("Recovered the last Collaborate session's changes. Save to keep them.");
    }

    /// What the other members have selected, with their names and
    /// colours, for the viewport and Explorer. Only what this copy has.
    pub(crate) fn team_selections(&self) -> Vec<crate::viewport::MemberSelection> {
        let Some(team) = &self.team else {
            return Vec::new();
        };
        let mut out: Vec<_> = team
            .presences()
            .filter(|(_, presence)| !presence.selection.is_empty())
            .filter_map(|(member, presence)| {
                let who = team.roster().iter().find(|c| c.member == member)?;
                Some(crate::viewport::MemberSelection {
                    name: who.profile.name.clone(),
                    color: super::team_panel::member_color(who.profile.color),
                    selection: presence
                        .selection
                        .iter()
                        .copied()
                        .filter(|&id| self.world.get(id).is_some())
                        .collect(),
                })
            })
            .collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    pub(crate) fn team(&self) -> Option<&Team> {
        self.team.as_ref()
    }

    pub(crate) fn in_team_session(&self) -> bool {
        self.team.is_some()
    }

    /// True while a network session needs frames to look at the network.
    pub(crate) fn team_wants_frames(&self) -> Option<std::time::Duration> {
        self.team
            .as_ref()
            .filter(|team| !team.ended && matches!(team.link, Link::Net { .. }))
            .map(|_| NET_POLL)
    }

    /// What the network said since the last frame: messages for the
    /// client, after making it on joining.
    fn team_network(&mut self) -> Vec<ToClient> {
        let Some(team) = self.team.as_mut() else {
            return Vec::new();
        };
        let Link::Net { remote, .. } = &mut team.link else {
            return Vec::new();
        };
        // The session is over: nothing more to hear, nobody to reconnect to.
        if team.client.as_ref().is_some_and(Client::host_left) {
            return Vec::new();
        }
        let mut inbox = Vec::new();
        for event in remote.poll() {
            match event {
                RemoteEvent::Admitted { member, role } => {
                    let client = team.client.get_or_insert_with(|| Client::new(member));
                    client.connect();
                    team.problem = None;
                    self.output
                        .info(format!("Collaborate: joined the session as {role:?}"));
                }
                RemoteEvent::Message(message) => inbox.push(message),
                RemoteEvent::Lost { why, retry } => {
                    if let Some(client) = &mut team.client {
                        client.disconnected();
                    }
                    team.problem = Some(why.clone());
                    self.output.warning(format!(
                        "Collaborate: {why}; trying again in {} s (your edits are kept)",
                        retry.as_secs()
                    ));
                }
                RemoteEvent::Refused(reason) => {
                    team.ended = true;
                    team.problem = Some(reason.clone());
                    self.output.error(format!("Collaborate: {reason}"));
                }
            }
        }
        inbox
    }

    /// Sends this frame's edits and applies the others', unless a gesture
    /// is open. Runs each frame.
    pub(crate) fn team_frame(&mut self) {
        if self.team.is_none()
            || self.history.is_open()
            || self.viewport.is_dragging()
            || self.ui_designer.is_dragging()
        {
            return;
        }
        self.send_presence();
        let mut changed = false;
        let mut file_changes = Vec::new();
        let mut file_problem = None;
        for _ in 0..ROUNDS_PER_FRAME {
            let mut inbox = self.team_network();
            let label = self.history.undo_label().unwrap_or("Edit").to_owned();
            let team = self.team.as_mut().expect("checked above");
            let Some(client) = team.client.as_mut() else {
                break;
            };
            let sent = client.capture(&mut self.world, label);
            // The folder: saves go up, arrived files go in (TC5).
            let due = team
                .mirror_synced
                .is_none_or(|at| at.elapsed() >= MIRROR_EVERY)
                || client.files_waiting();
            if let Some(mirror) = team.mirror.as_mut()
                && due
                && client.is_joined()
                && !client.host_left()
            {
                team.mirror_synced = Some(std::time::Instant::now());
                match mirror.sync(client, &mut self.world) {
                    Ok(changes) => file_changes.extend(changes),
                    Err(error) => file_problem = Some(error.to_string()),
                }
            }
            let outbox = client.take_outbox();
            let busy = !outbox.is_empty();
            match &mut team.link {
                Link::Hub(hub) => {
                    let mut hub = hub.lock().expect("hub");
                    hub.send(client.member(), outbox);
                    inbox.extend(hub.take(client.member()));
                }
                Link::Net { remote, .. } => remote.send(outbox),
            }
            let received = !inbox.is_empty();
            for message in inbox {
                client.receive(&mut self.world, message);
            }
            if sent.is_some() {
                self.warn_unshared_step();
            }
            self.team_notices();
            changed |= received;
            // Over the network, answers come in a later frame.
            let net = matches!(self.team.as_ref().map(|t| &t.link), Some(Link::Net { .. }));
            if net || (!busy && !received) {
                break;
            }
        }
        if changed {
            self.forget_selection_outside_tree();
            self.generate_frame();
        }
        // The host's project is the session's: others' edits are changes
        // to save there, so quitting asks first.
        let seq = self
            .team
            .as_ref()
            .and_then(|team| team.client.as_ref())
            .map(Client::seq);
        if let Some(team) = &mut self.team
            && seq != team.seen_seq
        {
            if team.seen_seq.is_some() && team.hosting().is_some() {
                self.history.mark_unsaved();
            }
            team.seen_seq = seq;
        }
        self.team_file_changes(file_changes, file_problem);
        self.open_team_mirror_when_ready();
        self.team_locks();
        self.keep_unsent();
        self.follow_member();
    }

    /// A joining editor's recovery file (TC6): while edits wait for the
    /// host (offline, or the host gone), they're written to its folder, so
    /// an editor that stops first sends them next time. Written again when
    /// they change, removed once the host has them all.
    pub(super) fn keep_unsent(&mut self) {
        let Some(team) = self.team.as_mut() else {
            return;
        };
        let (Some(client), Some(mirror)) = (&team.client, &team.mirror) else {
            return;
        };
        if team.hosting().is_some() || !client.is_joined() {
            return;
        }
        let now = (client.pending(), client.newest_unsent());
        if team.unsent_kept == Some(now) {
            return;
        }
        let root = mirror.root().to_owned();
        if now.0 == 0 {
            if team.unsent_kept.take().is_some()
                && let Err(error) = journal::discard_unsent(&root)
            {
                log::warn!("Collaborate: couldn't remove the recovery file: {error}");
            }
            return;
        }
        // While online they're confirmed in moments: written only once the
        // connection is down, or to keep a file there already up to date.
        if client.is_online() && team.unsent_kept.is_none() {
            return;
        }
        match journal::save_unsent(&root, &client.unsent()) {
            Ok(()) => team.unsent_kept = Some(now),
            Err(error) => {
                log::warn!("Collaborate: couldn't write the recovery file: {error}")
            }
        }
    }

    /// On joining a session: edits an earlier run of this editor kept in
    /// the session's folder and never had confirmed are sent now.
    fn recover_unsent_edits(&mut self) {
        let Some(team) = self.team.as_mut() else {
            return;
        };
        if team.unsent_loaded || team.hosting().is_some() {
            return;
        }
        let (Some(client), Some(mirror)) = (team.client.as_mut(), &team.mirror) else {
            return;
        };
        team.unsent_loaded = true;
        let unsent = match journal::load_unsent(mirror.root()) {
            Ok(unsent) => unsent,
            Err(error) => {
                log::warn!("Collaborate: couldn't read the recovery file: {error}");
                return;
            }
        };
        if unsent.is_empty() {
            return;
        }
        // This copy as it is before they go, to compare with the session
        // afterwards in Version History (C5).
        let checkpoint = axcenity_project::record_checkpoint(mirror.root(), OFFLINE_CHECKPOINT);
        if let Err(error) = &checkpoint {
            log::warn!(
                "Collaborate: couldn't keep a checkpoint before sending offline edits: {error}"
            );
        }
        self.team_panel.offline_checkpoint = checkpoint.is_ok();
        let count = client.recover_unsent(&mut self.world, unsent);
        // The file now holds edits that are pending again: it's written
        // anew next frame, and removed once they're confirmed.
        team.unsent_kept = Some((usize::MAX, None));
        if count > 0 {
            self.output.info(format!(
                "Collaborate: sent {count} change(s) this editor made in the session before it \
                 stopped"
            ));
        }
    }

    /// Drops the network connection as a cut cable would (tests): it comes
    /// back by itself after a second or more.
    #[cfg(test)]
    pub(crate) fn drop_team_connection(&mut self) {
        let Some(team) = self.team.as_mut() else {
            return;
        };
        if let Link::Net { remote, .. } = &mut team.link
            && !remote.drop_connection().is_empty()
            && let Some(client) = &mut team.client
        {
            client.disconnected();
            team.problem = Some("disconnected".to_owned());
        }
    }

    /// Publishes the files an import just wrote, with the Instances it
    /// placed, as one transaction named `label` (one undo step for
    /// everyone).
    pub(super) fn team_publish_files(&mut self, label: &str) {
        let Some(team) = self.team.as_mut() else {
            return;
        };
        let (Some(client), Some(mirror)) = (team.client.as_mut(), team.mirror.as_mut()) else {
            return;
        };
        if !client.is_joined()
            || mirror.root() != self.project.as_ref().map_or(Path::new(""), |p| &p.folder)
        {
            return;
        }
        team.mirror_synced = Some(std::time::Instant::now());
        let result = mirror.sync_as(client, &mut self.world, Some(label));
        let (changes, problem) = match result {
            Ok(changes) => (changes, None),
            Err(error) => (Vec::new(), Some(error.to_string())),
        };
        self.team_file_changes(changes, problem);
    }

    /// Takes in what the mirror did: files the others changed are read
    /// again, as if another program changed them.
    fn team_file_changes(&mut self, changes: Vec<Change>, problem: Option<String>) {
        if let Some(problem) = problem {
            log::warn!("Collaborate: the session's files: {problem}");
        }
        let in_project = match (&self.team, &self.project) {
            (Some(team), Some(project)) => team.files_folder() == Some(project.folder.as_path()),
            _ => false,
        };
        let mut read_again = Vec::new();
        for change in changes {
            match change {
                Change::Wrote(path) | Change::Removed(path) => {
                    if in_project && editor_reads(&path) {
                        read_again.push(path);
                    }
                }
                Change::Conflict { path, copy } => self.output.warning(format!(
                    "Collaborate: someone else saved `{path}` first. Yours is kept as `{copy}`."
                )),
                Change::Failed { path, reason } => self
                    .output
                    .warning(format!("Collaborate: `{path}` wasn't shared: {reason}")),
                Change::Published { .. } => {}
            }
        }
        if !read_again.is_empty() {
            self.disk_changed(&read_again);
            self.asset_browser.refresh();
        }
    }

    /// A joining editor: once the session's files are in its folder, opens
    /// the folder as the project, with the session's World.
    fn open_team_mirror_when_ready(&mut self) {
        let Some(team) = self.team.as_mut() else {
            return;
        };
        let ready = team.open_mirror
            && !team.fetching_files()
            && team
                .files_folder()
                .is_some_and(|root| root.join(project::MANIFEST_FILE).exists());
        if !ready {
            return;
        }
        team.open_mirror = false;
        let root = team.files_folder().expect("checked above").to_owned();
        self.open_team_mirror(&root);
    }

    /// Opens `root` (this editor's folder of the session's files) as the
    /// project, keeping the session's World in place of the one it loads.
    pub(super) fn open_team_mirror(&mut self, root: &Path) {
        // A world file, so the folder opens as any project does. It stays
        // on this computer (the World itself travels as operations).
        let world_file = root.join(project::WORLD_FILE);
        if let Err(error) = std::fs::write(&world_file, project::write_world(&self.world)) {
            self.output.error(format!(
                "Collaborate: couldn't write `{}`: {error}",
                world_file.display()
            ));
            return;
        }
        let session = std::mem::replace(&mut self.world, World::new());
        self.open(root);
        if self.project.as_ref().is_none_or(|p| p.folder != root) {
            self.world = session;
            self.output
                .error("Collaborate: couldn't open the session's files as a project");
            return;
        }
        let loaded = std::mem::replace(&mut self.world, World::new());
        self.replace_world(session);
        self.history.set_shared(true);
        self.generators = Default::default();
        self.relink_to_session(&loaded);
        self.output.info(format!(
            "Collaborate: opened the session's files (kept in `{}`)",
            root.display()
        ));
    }

    /// After the session's World took the place of the one a project load
    /// made, points the project's scripts and UI graphs at the session's
    /// Instances for them (the same class and names from `game` down). A
    /// script the session has no Instance for gets one, which syncs.
    fn relink_to_session(&mut self, loaded: &World) {
        let Some(project) = self.project.as_mut() else {
            return;
        };
        let world = &mut self.world;
        let mut index: HashMap<Vec<(ClassName, String)>, VecDeque<InstanceId>> = HashMap::new();
        for id in world.descendants(world.root()) {
            index.entry(tree_key(world, id)).or_default().push_back(id);
        }
        let mut find = |id: InstanceId| index.get_mut(&tree_key(loaded, id))?.pop_front();
        for script in &mut project.scripts {
            script.instance = match find(script.instance) {
                Some(id) => id,
                None => project::add_script_instance(world, &script.path),
            };
        }
        for document in &mut project.block_scripts {
            if let Some(script) = project.scripts.iter().find(|s| s.path == document.path) {
                document.instance = script.instance;
            }
        }
        for document in &mut project.ui_graphs {
            if let Some(id) = find(document.gui) {
                document.gui = id;
            }
        }
    }

    /// For a script file that just appeared in a session: the Script the
    /// author's editor made for it (it arrives as an edit before the file
    /// does), if no other file has it.
    pub(super) fn team_script_instance(&self, path: &str) -> Option<InstanceId> {
        self.team.as_ref()?;
        let project = self.project.as_ref()?;
        let class = project::script_class(path);
        let service = project::script_service(&self.world, class);
        let name = project::script_name(path);
        self.world
            .get(service)?
            .children()
            .iter()
            .copied()
            .find(|&id| {
                self.world
                    .get(id)
                    .is_some_and(|i| i.class() == class && i.name() == name)
                    && !project.scripts.iter().any(|s| s.instance == id)
            })
    }

    /// Asks to be the one editing each document with unsaved changes, lets
    /// go of the rest, and marks script tabs someone else is editing
    /// read-only (the other tabs ask as they're drawn: `locked_banner`).
    fn team_locks(&mut self) {
        if self.team.is_none() {
            return;
        }
        let wanted: std::collections::BTreeSet<String> = self
            .unsaved_documents()
            .into_iter()
            .filter(|path| axcenity_collab::files::is_synced(path))
            .collect();
        let Some(team) = self.team.as_mut() else {
            return;
        };
        let Some(client) = team.client.as_mut().filter(|c| c.is_joined()) else {
            return;
        };
        let held = client.my_locks().clone();
        for path in held.difference(&wanted) {
            client.unlock(path);
        }
        for path in wanted.difference(&held) {
            client.lock(path);
        }
        let team = self.team.as_ref().expect("checked above");
        for tab in self.tabs.iter_mut() {
            tab.editing_elsewhere = team.editing_elsewhere(&tab.path);
        }
    }

    /// Every document with changes to save, by the path it's saved at:
    /// script and stylesheet tabs, block scripts, UI Event Graphs, and the
    /// behavior tree, dialogue, data table and material tabs. Each is
    /// locked for this editor while it has changes (design §7.7).
    fn unsaved_documents(&self) -> Vec<String> {
        let mut paths = self.tabs.unsaved_paths();
        if let Some(project) = &self.project {
            paths.extend(
                project
                    .block_scripts
                    .iter()
                    .filter(|doc| doc.script != doc.saved)
                    .map(|doc| doc.path.clone()),
            );
        }
        paths.extend(self.unsaved_ui_graphs());
        paths.extend(self.behavior.unsaved_paths());
        paths.extend(self.dialogues.unsaved_paths());
        paths.extend(self.data_tables.unsaved_paths());
        paths.extend(self.materials.unsaved_paths());
        paths
    }

    /// For a document tab: when someone else is editing the document at
    /// `path`, says so at the top of the tab and returns true (the tab is
    /// then read-only).
    pub(super) fn locked_banner(&self, ui: &mut egui::Ui, path: Option<&str>) -> bool {
        let who = self
            .team
            .as_ref()
            .zip(path)
            .and_then(|(team, path)| team.editing_elsewhere(path));
        match who {
            Some(who) => {
                crate::scripts::locked_banner(ui, &who);
                true
            }
            None => false,
        }
    }

    /// What this editor is doing: its selection and camera.
    fn presence(&self) -> Presence {
        let camera = self.viewport.render_camera();
        let look = (camera.look_at - camera.position).normalize_or_zero();
        Presence {
            selection: self.selection.clone(),
            camera: Some(Camera {
                position: camera.position.to_array(),
                look: look.to_array(),
            }),
            editing: None,
        }
    }

    /// Tells the others what this editor is doing, at most 10 times a
    /// second (the client sends only what changed).
    fn send_presence(&mut self) {
        let due = self.team.as_ref().is_some_and(|team| {
            team.client.is_some()
                && team
                    .presence_sent
                    .is_none_or(|sent| sent.elapsed() >= PRESENCE_EVERY)
        });
        if !due {
            return;
        }
        let presence = self.presence();
        let team = self.team.as_mut().expect("checked above");
        team.presence_sent = Some(std::time::Instant::now());
        team.client
            .as_mut()
            .expect("checked above")
            .set_presence(presence);
    }

    fn team_notices(&mut self) {
        let Some(team) = self.team.as_mut() else {
            return;
        };
        let notices = team
            .client
            .as_mut()
            .map(Client::take_notices)
            .unwrap_or_default();
        for notice in notices {
            match notice {
                Notice::Joined => {
                    // A fresh copy of the session's World: older steps and
                    // what generators made name what's gone.
                    self.history = History::new();
                    self.history.set_shared(true);
                    self.generators = Default::default();
                    self.selection.clear();
                    self.selected = None;
                    self.start_team_mirror();
                    self.recover_unsent_edits();
                }
                Notice::Changed {
                    label,
                    notes,
                    recovered,
                    ..
                } => {
                    let what = format!(
                        "someone deleted what it changed ({} change(s) dropped or moved)",
                        notes.len()
                    );
                    self.output.warning(format!(
                        "Collaborate: \"{label}\" didn't apply in full: {what}"
                    ));
                    if recovered {
                        self.team_panel
                            .recovered
                            .push(super::team_panel::RecoveredChange {
                                label,
                                reason: format!("Didn't apply in full: {what}."),
                            });
                    }
                }
                Notice::Refused {
                    label,
                    reason,
                    recovered,
                    ..
                } => {
                    self.output
                        .warning(format!("Collaborate: \"{label}\" was refused: {reason}"));
                    // From a while ago: listed where they can't be missed.
                    if recovered {
                        self.team_panel
                            .recovered
                            .push(super::team_panel::RecoveredChange {
                                label,
                                reason: reason.to_string(),
                            });
                    }
                }
                Notice::HostLeft => {
                    if let Some(team) = self.team.as_mut() {
                        team.ended = true;
                        team.problem = Some("The host left the session".to_owned());
                        if let Link::Net { remote, .. } = &mut team.link {
                            remote.drop_connection();
                        }
                    }
                    self.team_panel.following = None;
                    self.output.warning(
                        "Collaborate: the host ended the session. Your copy stays open, \
                         read-only; changes not sent yet are kept and go to the host the next \
                         time you join. Leave the session to work on your copy alone.",
                    );
                    self.toasts
                        .warning("The host ended the Collaborate session: read-only now.");
                }
                Notice::Arrived { name, .. } => self
                    .output
                    .info(format!("Collaborate: {name} joined the session")),
                Notice::Left { name, .. } => self
                    .output
                    .info(format!("Collaborate: {name} left the session")),
                Notice::Resynced => self.output.warning(
                    "Collaborate: this copy disagreed with the session and was reloaded \
                     (your unsent edits were kept)",
                ),
            }
        }
    }

    /// A joining editor's folder for the session's files: one per session,
    /// under the Collaborate data folder, kept for next time.
    fn start_team_mirror(&mut self) {
        let Some(team) = self.team.as_mut() else {
            return;
        };
        let (Some(home), Some(client)) = (team.mirrors_home.clone(), team.client.as_ref()) else {
            return;
        };
        if team.mirror.is_some() {
            return;
        }
        let session = client.session().unwrap_or_default().to_owned();
        if session.is_empty() || !session.chars().all(|c| c.is_ascii_alphanumeric()) {
            return;
        }
        let me = client.member();
        let name = client
            .roster()
            .iter()
            .find(|c| c.member == me)
            .map_or_else(|| "member".to_owned(), |c| c.profile.name.clone());
        let root = home.join(&session);
        if let Err(error) = std::fs::create_dir_all(&root) {
            self.output.error(format!(
                "Collaborate: couldn't make a folder for the session's files: {error}"
            ));
            return;
        }
        team.mirror = Some(Mirror::open(&root, &session, &name));
        team.open_mirror = self.project.as_ref().is_none_or(|p| p.folder != root);
        self.output.info(format!(
            "Collaborate: fetching the session's files into `{}`",
            root.display()
        ));
    }

    /// The newest step changed something Collaborate doesn't share yet.
    fn warn_unshared_step(&mut self) {
        if let Some(what) = self.history.top_unshared() {
            self.output.warning(format!(
                "Collaborate doesn't share {what} yet: others won't see this part of \"{}\"",
                self.history.undo_label().unwrap_or("Edit")
            ));
        }
    }

    /// Undo or redo left some changes alone because someone else changed
    /// them since.
    pub(super) fn report_skipped(&mut self, skipped: usize) {
        if skipped > 0 {
            self.output.warning(format!(
                "Some changes weren't undone or redone: someone else changed {skipped} of them since"
            ));
        }
    }

    /// In a session, refuses an undo or redo that would swap the whole
    /// World, which can't be sent yet. True if refused.
    pub(super) fn team_refuses_world_swap(&mut self, redo: bool) -> bool {
        if self.team.is_none() || !self.history.swaps_world(redo) {
            return false;
        }
        self.output.warning(
            "Collaborate: this step changed the whole world (a Console command or a prefab \
             Apply) and can't be undone or redone during a session yet",
        );
        true
    }
}
