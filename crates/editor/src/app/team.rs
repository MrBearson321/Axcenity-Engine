//! Team Create in the editor (TC2 and TC3 of `docs/research/team-create.md`).
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
//! a session yet, and terrain, the string table and project files aren't
//! shared yet (TC5): the Output says so.
//!
//! What SplineMeshes and SplineScatters make isn't shared: each copy makes
//! its own ([`World::unrecorded`] in `generate_frame`).
//!
//! Over the network (TC3), the hosting editor runs the session's host on
//! its own thread and joins it over the loopback address like any member
//! (`collab::session`); others join with an invite link. Until the Team
//! Create menus (TC4), `--team-host` and `--team-join <link>` start one
//! from the command line. The tests also meet in a [`Hub`] in this process.

// The in-process hub is for tests; menus that end a session come in TC4.
#![cfg_attr(not(test), allow(dead_code))]

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use axcenity_collab::access::{Access, INVITE_LIFETIME_SECONDS, Invite};
use axcenity_collab::identity::{Fingerprint, Identity};
use axcenity_collab::session::{HostSession, Hosting, Remote, RemoteEvent};
use axcenity_collab::socket::{DEFAULT_PORT, HostCert};
use axcenity_collab::{Camera, Client, Host, MemberId, Notice, Presence, Role, ToClient, ToHost};
use axcenity_core::spline::is_generator;

use super::*;

/// Most rounds of messages one frame passes back and forth.
const ROUNDS_PER_FRAME: usize = 8;

/// How often the editor looks at the network while nothing else redraws.
const NET_POLL: std::time::Duration = std::time::Duration::from_millis(50);

/// The least time between two presences this editor sends (design §7.9:
/// at most 10 a second).
const PRESENCE_EVERY: std::time::Duration = std::time::Duration::from_millis(100);

/// `axcenity-editor <project> --team-host`: host a session for the project.
pub const HOST_FLAG: &str = "--team-host";
/// `axcenity-editor <project> --team-join <link>`: join a session.
pub const JOIN_FLAG: &str = "--team-join";

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
            if arg == HOST_FLAG {
                start = Some(TeamStart::Host);
            } else if arg == JOIN_FLAG {
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

/// Where Team Create keeps this computer's identity, the host's
/// certificate and each hosted project's members: the editor's data
/// folder, never the project folder (which may be pushed somewhere public).
fn data_folder() -> Option<PathBuf> {
    eframe::storage_dir(crate::APP_ID).map(|data| data.join("Team Create"))
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

/// The members file of the project in `project`, named by a hash of its
/// path.
fn access_path(folder: &Path, project: &Path) -> PathBuf {
    let project = project
        .canonicalize()
        .unwrap_or_else(|_| project.to_owned());
    let hash = Fingerprint::of(project.to_string_lossy().as_bytes()).to_hex();
    folder
        .join("projects")
        .join(format!("{}.json", &hash[..16]))
}

/// `world` as a session starts from: without what SplineMeshes and
/// SplineScatters made (every copy makes its own).
fn session_world(mut world: World) -> World {
    world.stop_recording_edits();
    let generated: Vec<InstanceId> = world
        .descendants(world.root())
        .into_iter()
        .filter(|&id| world.get(id).is_some_and(|i| is_generator(i.class())))
        .flat_map(|id| {
            world
                .get(id)
                .map(|i| i.children().to_vec())
                .unwrap_or_default()
        })
        .collect();
    for id in generated {
        let _ = world.destroy(id);
    }
    world
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
}

impl Team {
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
        self.team = Some(Team {
            client: Some(client),
            link: Link::Hub(hub),
            ended: false,
            presence_sent: None,
        });
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
            TeamStart::Host => self.host_network_session(None, DEFAULT_PORT).map(|invite| {
                let link = invite.link();
                // On stdout too, to copy from the terminal.
                println!("Team Create invite (works once, for 7 days): {link}");
                self.output.info(format!(
                    "Team Create: hosting on {}. Invite: {link}",
                    invite.host
                ));
            }),
            TeamStart::Join(invite) => self.join_network_session(&invite, None),
        };
        if let Err(error) = result {
            self.output.error(format!("Team Create: {error}"));
        }
    }

    /// Hosts a session for the open project on `port` (0 picks one), with
    /// this editor joined as its owner; returns an Editor invite.
    /// `folder` is where Team Create's files go (the editor's data folder
    /// when `None`).
    pub(crate) fn host_network_session(
        &mut self,
        folder: Option<&Path>,
        port: u16,
    ) -> Result<Invite, String> {
        let folder = folder
            .map(Path::to_owned)
            .or_else(data_folder)
            .ok_or("there's no folder to keep Team Create's files in")?;
        let project = self
            .project
            .as_ref()
            .map(|p| p.folder.clone())
            .ok_or("open a project first")?;
        let identity = load_identity(&folder).map_err(|e| format!("your identity: {e}"))?;
        let cert = HostCert::load_or_make(&folder.join("host-certificate.json"))
            .map_err(|e| format!("the host certificate: {e}"))?;
        let access_path = access_path(&folder, &project);
        let access = match Access::load(&access_path) {
            Ok(access) => access,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Access::owned_by(identity.public_key(), &identity.name)
            }
            Err(error) => return Err(format!("the members list: {error}")),
        };
        let mut session = HostSession::start(
            Host::new(session_world(self.world.clone())),
            access,
            Some(access_path),
            &cert,
            std::net::SocketAddr::from(([0, 0, 0, 0], port)),
            None,
        )
        .map_err(|e| format!("couldn't listen on port {port}: {e}"))?;
        let invite = session.invite(Role::Editor, INVITE_LIFETIME_SECONDS);
        let loopback = session.loopback();
        let hosting = session.serve();
        let remote = Box::new(Remote::join(
            loopback,
            cert.fingerprint(),
            None,
            Arc::new(identity),
        ));
        self.team = Some(Team {
            client: None,
            link: Link::Net {
                remote,
                hosting: Some(hosting),
            },
            ended: false,
            presence_sent: None,
        });
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
            .ok_or("there's no folder to keep Team Create's files in")?;
        let identity = load_identity(&folder).map_err(|e| format!("your identity: {e}"))?;
        self.output.info(format!(
            "Team Create: joining {} as {} ({})",
            invite.host,
            identity.name,
            identity.public_key().fingerprint().short()
        ));
        self.team = Some(Team {
            client: None,
            link: Link::Net {
                remote: Box::new(Remote::join_invite(invite, Arc::new(identity))),
                hosting: None,
            },
            ended: false,
            presence_sent: None,
        });
        Ok(())
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
        let mut inbox = Vec::new();
        for event in remote.poll() {
            match event {
                RemoteEvent::Admitted { member, role } => {
                    let client = team.client.get_or_insert_with(|| Client::new(member));
                    client.connect();
                    self.output
                        .info(format!("Team Create: joined the session as {role:?}"));
                }
                RemoteEvent::Message(message) => inbox.push(message),
                RemoteEvent::Lost { why, retry } => {
                    if let Some(client) = &mut team.client {
                        client.disconnected();
                    }
                    self.output.warning(format!(
                        "Team Create: {why}; trying again in {} s (your edits are kept)",
                        retry.as_secs()
                    ));
                }
                RemoteEvent::Refused(reason) => {
                    team.ended = true;
                    self.output.error(format!("Team Create: {reason}"));
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
        for _ in 0..ROUNDS_PER_FRAME {
            let mut inbox = self.team_network();
            let label = self.history.undo_label().unwrap_or("Edit").to_owned();
            let team = self.team.as_mut().expect("checked above");
            let Some(client) = team.client.as_mut() else {
                break;
            };
            let sent = client.capture(&mut self.world, label);
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
                }
                Notice::Changed { label, notes, .. } => self.output.warning(format!(
                    "Team Create: \"{label}\" didn't apply in full: someone deleted what it \
                     changed ({} change(s) dropped or moved)",
                    notes.len()
                )),
                Notice::Refused { label, reason, .. } => self
                    .output
                    .warning(format!("Team Create: \"{label}\" was refused: {reason}")),
                Notice::Arrived { name, .. } => self
                    .output
                    .info(format!("Team Create: {name} joined the session")),
                Notice::Left { name, .. } => self
                    .output
                    .info(format!("Team Create: {name} left the session")),
                Notice::Resynced => self.output.warning(
                    "Team Create: this copy disagreed with the session and was reloaded \
                     (your unsent edits were kept)",
                ),
            }
        }
    }

    /// The newest step changed something Team Create doesn't share yet.
    fn warn_unshared_step(&mut self) {
        if let Some(what) = self.history.top_unshared() {
            self.output.warning(format!(
                "Team Create doesn't share {what} yet: others won't see this part of \"{}\"",
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
            "Team Create: this step changed the whole world (a Console command or a prefab \
             Apply) and can't be undone or redone during a session yet",
        );
        true
    }
}
