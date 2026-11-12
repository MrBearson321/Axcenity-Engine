//! A Collaborate session over the network (TC3): the host's side
//! ([`HostSession`]) and a member's ([`Remote`]). Each connection first
//! goes through the [`handshake`](crate::handshake); after that its
//! messages are [`wire`] bytes for the [`Host`] or the
//! member's [`Client`](crate::Client).
//!
//! The hosting editor joins its own session like anyone else, over the
//! loopback address, so there's one code path for every member.

use std::collections::HashMap;
use std::io;
use std::net::{IpAddr, SocketAddr, UdpSocket};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::RecvTimeoutError;
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::access::{self, Access, Invite, Member};
use crate::handshake::{Gate, GateStep, Knock, KnockStep, Versions};
use crate::host::Host;
use crate::identity::{Fingerprint, Identity};
use crate::message::{MemberId, ToClient, ToHost};
use crate::presence::Profile;
use crate::role::Role;
use crate::socket::{ConnId, HostCert, Link, LinkClosed, LinkEvent, Listener, NetEvent};
use crate::wire;

/// The longest a member waits between tries to reconnect.
const RETRY_MOST: Duration = Duration::from_secs(30);

/// This computer's address on the local network, as invites name it: the
/// one it would reach other computers from. Nothing is sent to find it.
pub fn lan_address() -> Option<IpAddr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    // "Connecting" a UDP socket only picks the route; no packet goes out.
    socket.connect("192.0.2.1:9").ok()?;
    let ip = socket.local_addr().ok()?.ip();
    (!ip.is_unspecified()).then_some(ip)
}

/// The host's side of a session: who's connected, who's still knocking,
/// and the [`Host`] that orders everyone's edits.
pub struct HostSession {
    host: Host,
    access: Access,
    /// Where `access` is saved after it changes.
    access_path: Option<PathBuf>,
    listener: Listener,
    cert: Fingerprint,
    versions: Versions,
    /// "address:port" as invites name it.
    address: String,
    gates: HashMap<ConnId, Gate>,
    members: HashMap<ConnId, MemberId>,
    conns: HashMap<MemberId, ConnId>,
    /// Each connection's messages in the current second (rate limit).
    rates: HashMap<ConnId, (Instant, u32)>,
}

/// The most messages a member may send in one second (TC6). An editor
/// sends a few each frame, and a file a message per MiB, so only a broken
/// or hostile client gets near it; its connection is closed.
pub const MAX_MESSAGES_PER_SECOND: u32 = 1_000;

impl HostSession {
    /// Hosts `host` for the members in `access`, listening on `bind`
    /// with `cert`. Invites name `address` ("ip:port"; `None` names this
    /// computer's LAN address and the port listened on).
    pub fn start(
        host: Host,
        access: Access,
        access_path: Option<PathBuf>,
        cert: &HostCert,
        bind: SocketAddr,
        address: Option<String>,
    ) -> io::Result<HostSession> {
        // Everyone who's a member is on the roster, connected or not.
        let mut host = host;
        for member in access.members() {
            if host.role(member.id).is_none() {
                host.add_member(member.id, member.role);
            }
            let profile = Profile {
                name: member.name.clone(),
                color: member.color,
            };
            let _ = host.set_profile(member.id, profile);
        }
        let listener = Listener::bind(bind, cert)?;
        let port = listener.address().port();
        let address = address.unwrap_or_else(|| {
            let ip = lan_address().unwrap_or(IpAddr::from([127, 0, 0, 1]));
            SocketAddr::new(ip, port).to_string()
        });
        Ok(HostSession {
            host,
            access,
            access_path,
            listener,
            cert: cert.fingerprint(),
            versions: Versions::this(),
            address,
            gates: HashMap::new(),
            members: HashMap::new(),
            conns: HashMap::new(),
            rates: HashMap::new(),
        })
    }

    /// Compares joiners with `versions` instead of this build's (tests).
    pub fn with_versions(mut self, versions: Versions) -> HostSession {
        self.versions = versions;
        self
    }

    pub fn host(&self) -> &Host {
        &self.host
    }

    pub fn access(&self) -> &Access {
        &self.access
    }

    /// "address:port" as invites name the host.
    pub fn address(&self) -> &str {
        &self.address
    }

    /// Where it listens.
    pub fn local_address(&self) -> SocketAddr {
        self.listener.address()
    }

    /// The loopback "address:port" the hosting editor joins through.
    pub fn loopback(&self) -> String {
        SocketAddr::new(IpAddr::from([127, 0, 0, 1]), self.local_address().port()).to_string()
    }

    pub fn cert(&self) -> Fingerprint {
        self.cert
    }

    /// The members connected now.
    pub fn connected(&self) -> Vec<MemberId> {
        let mut members: Vec<MemberId> = self.conns.keys().copied().collect();
        members.sort();
        members
    }

    /// A new invite for `role`, working once for `lifetime` seconds.
    pub fn invite(&mut self, role: Role, lifetime: u64) -> Invite {
        let secret = self.access.invite(role, lifetime, access::now());
        self.save_access();
        Invite {
            host: self.address.clone(),
            cert: self.cert,
            secret,
        }
    }

    /// Revokes every invite not used yet.
    pub fn revoke_invites(&mut self) {
        self.access.revoke_invites();
        self.save_access();
    }

    /// Changes a member's role, at once if they're connected.
    pub fn set_role(&mut self, member: MemberId, role: Role) {
        if self.access.set_role(member, role) {
            self.save_access();
            let out = self.host.set_role(member, role);
            self.route(out);
        }
    }

    /// Takes a member out of the project and closes their connection.
    pub fn remove(&mut self, member: MemberId) {
        self.access.remove(member);
        self.save_access();
        if let Some(conn) = self.conns.remove(&member) {
            self.members.remove(&conn);
            self.listener.close(conn);
        }
        let out = self.host.remove_member(member);
        self.route(out);
    }

    /// Handles what arrived since the last call. Returns how many events
    /// there were.
    pub fn frame(&mut self) -> usize {
        let events = self.listener.poll();
        let count = events.len();
        for event in events {
            self.handle(event);
        }
        self.tick();
        count
    }

    /// Lets the host's clock move on (document locks of members who left
    /// end after a while).
    fn tick(&mut self) {
        let out = self.host.tick(std::time::Instant::now());
        self.route(out);
    }

    /// Runs the session on its own thread until the returned handle is
    /// dropped.
    pub fn serve(mut self) -> Hosting {
        let events = self.listener.take_events().expect("events not taken yet");
        let session = Arc::new(Mutex::new(self));
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let session = session.clone();
            let stop = stop.clone();
            std::thread::Builder::new()
                .name("team-create-host".into())
                .spawn(move || {
                    while !stop.load(Ordering::Relaxed) {
                        match events.recv_timeout(Duration::from_millis(100)) {
                            Ok(event) => {
                                let mut session = session.lock().expect("host session");
                                session.handle(event);
                                for event in events.try_iter() {
                                    session.handle(event);
                                }
                            }
                            Err(RecvTimeoutError::Timeout) => {}
                            Err(RecvTimeoutError::Disconnected) => break,
                        }
                        session.lock().expect("host session").tick();
                    }
                })
                .expect("starting the Collaborate host thread")
        };
        Hosting {
            session,
            stop,
            thread: Some(thread),
            say_ended: true,
        }
    }

    fn handle(&mut self, event: NetEvent) {
        match event {
            NetEvent::Opened(conn) => {
                self.gates
                    .insert(conn, Gate::new(self.cert, self.versions.clone()));
            }
            NetEvent::Message(conn, bytes) => {
                if !self.within_rate(conn) {
                    log::warn!("Collaborate: a connection sent too many messages; it's closed");
                    self.listener.close(conn);
                    return;
                }
                if let Some(&member) = self.members.get(&conn) {
                    match wire::decode_to_host(&bytes) {
                        Ok(message) => {
                            let out = self.host.receive(member, message);
                            self.route(out);
                        }
                        Err(error) => {
                            log::warn!("Collaborate: member {} sent {error}", member.0);
                            self.listener.close(conn);
                        }
                    }
                } else if let Some(gate) = self.gates.get_mut(&conn) {
                    match gate.receive(&bytes, &mut self.access, access::now()) {
                        GateStep::Reply(reply) => self.listener.send(conn, reply),
                        GateStep::Admit { member, reply } => {
                            self.gates.remove(&conn);
                            self.listener.send(conn, reply);
                            self.admit(conn, member);
                        }
                        GateStep::Refuse { reason, reply } => {
                            log::info!("Collaborate: refused a joiner: {reason}");
                            self.gates.remove(&conn);
                            self.listener.send(conn, reply);
                            self.listener.close(conn);
                        }
                    }
                }
            }
            NetEvent::Closed(conn) => {
                self.gates.remove(&conn);
                self.rates.remove(&conn);
                if let Some(member) = self.members.remove(&conn)
                    && self.conns.get(&member) == Some(&conn)
                {
                    self.conns.remove(&member);
                    let out = self.host.disconnected(member);
                    self.route(out);
                    // A dropped or quiet connection, or one closed on
                    // purpose: the joiner reconnects on its own if it can.
                    let name = self
                        .access
                        .members()
                        .iter()
                        .find(|m| m.id == member)
                        .map_or_else(|| format!("member {}", member.0), |m| m.name.clone());
                    log::info!("Collaborate: {name} left (or their connection dropped)");
                }
            }
        }
    }

    /// Counts a message from `conn`: false once it's past
    /// [`MAX_MESSAGES_PER_SECOND`] in this second.
    fn within_rate(&mut self, conn: ConnId) -> bool {
        let now = Instant::now();
        let (since, count) = self.rates.entry(conn).or_insert((now, 0));
        if now.duration_since(*since) >= Duration::from_secs(1) {
            *since = now;
            *count = 0;
        }
        *count += 1;
        *count <= MAX_MESSAGES_PER_SECOND
    }

    /// The session ended cleanly: the host's journal goes (TC6).
    pub fn end_cleanly(&mut self) {
        self.host.end_journal();
    }

    /// Tells everyone connected the session is over ([`ToClient::Ended`]).
    /// Dropping [`Hosting`] does this.
    pub fn say_ended(&mut self) {
        let out = self.host.end();
        self.route(out);
    }

    /// The host, to change (tests, and the hosting editor's journal).
    pub fn host_mut(&mut self) -> &mut Host {
        &mut self.host
    }

    fn admit(&mut self, conn: ConnId, member: Member) {
        match self.host.role(member.id) {
            None => self.host.add_member(member.id, member.role),
            Some(role) if role != member.role => {
                let out = self.host.set_role(member.id, member.role);
                self.route(out);
            }
            Some(_) => {}
        }
        let profile = Profile {
            name: member.name.clone(),
            color: member.color,
        };
        let out = self.host.set_profile(member.id, profile);
        self.route(out);
        // The same member connecting again before their old connection
        // was noticed gone: the new one wins.
        if let Some(old) = self.conns.insert(member.id, conn) {
            self.members.remove(&old);
            self.listener.close(old);
            let out = self.host.disconnected(member.id);
            self.route(out);
        }
        self.members.insert(conn, member.id);
        self.save_access();
        log::info!(
            "Collaborate: {} ({}) joined as {:?}",
            member.name,
            member.key.fingerprint().short(),
            member.role
        );
    }

    fn route(&mut self, out: Vec<crate::host::Outgoing>) {
        for o in out {
            if let Some(&conn) = self.conns.get(&o.to) {
                self.listener.send(conn, wire::encode_to_client(&o.message));
            }
        }
    }

    fn save_access(&self) {
        if let Some(path) = &self.access_path
            && let Err(error) = self.access.save(path)
        {
            log::warn!("Collaborate: couldn't save the members list: {error}");
        }
    }
}

/// A [`HostSession`] running on its own thread. Dropping it ends the
/// session: everyone connected hears so, then every connection closes.
pub struct Hosting {
    session: Arc<Mutex<HostSession>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    /// Tell everyone it ended when dropped.
    say_ended: bool,
}

impl Hosting {
    pub fn lock(&self) -> MutexGuard<'_, HostSession> {
        self.session.lock().expect("host session")
    }

    /// Stops as a crash would: connections close without a word, so
    /// members keep trying to reconnect (tests).
    pub fn stop_without_a_word(mut self) {
        self.say_ended = false;
    }
}

impl Drop for Hosting {
    fn drop(&mut self) {
        // Each connection sends what's queued before it closes.
        if self.say_ended
            && let Ok(mut session) = self.session.lock()
        {
            session.say_ended();
        }
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// What happened to a member's connection to the session.
#[derive(Debug)]
pub enum RemoteEvent {
    /// The host let us in: session messages flow from now
    /// ([`Client::connect`](crate::Client::connect)).
    Admitted {
        member: MemberId,
        role: Role,
    },
    Message(ToClient),
    /// The connection dropped; it tries again after `retry`
    /// ([`Client::disconnected`](crate::Client::disconnected)).
    Lost {
        why: String,
        retry: Duration,
    },
    /// The host refused us, or isn't the host the invite named: it
    /// doesn't try again.
    Refused(String),
}

enum State {
    Waiting { until: Instant },
    Knocking { link: Link, knock: Option<Knock> },
    In { link: Link },
    Refused,
}

/// A member's connection to a hosted session, reconnecting when it drops.
pub struct Remote {
    host: String,
    cert: Fingerprint,
    /// The invite's secret, until it has let us in once.
    invite: Option<[u8; 16]>,
    identity: Arc<Identity>,
    versions: Versions,
    state: State,
    /// Failed tries in a row, for the wait before the next.
    tries: u32,
}

impl Remote {
    /// Starts joining the session at `host` (whose certificate has
    /// fingerprint `cert`), as `identity`, redeeming `invite` if this
    /// identity isn't a member yet.
    pub fn join(
        host: impl Into<String>,
        cert: Fingerprint,
        invite: Option<[u8; 16]>,
        identity: Arc<Identity>,
    ) -> Remote {
        Remote {
            host: host.into(),
            cert,
            invite,
            identity,
            versions: Versions::this(),
            state: State::Waiting {
                until: Instant::now(),
            },
            tries: 0,
        }
    }

    /// Joins through an invite link.
    pub fn join_invite(invite: &Invite, identity: Arc<Identity>) -> Remote {
        Remote::join(
            invite.host.clone(),
            invite.cert,
            Some(invite.secret),
            identity,
        )
    }

    /// Says it has `versions` instead of this build's (tests).
    pub fn with_versions(mut self, versions: Versions) -> Remote {
        self.versions = versions;
        self
    }

    /// True while session messages flow.
    pub fn is_in(&self) -> bool {
        matches!(self.state, State::In { .. })
    }

    /// True once refused: it won't try again.
    pub fn is_refused(&self) -> bool {
        matches!(self.state, State::Refused)
    }

    /// Sends messages to the host. Dropped unless [`is_in`](Self::is_in):
    /// the client sends them again after reconnecting.
    pub fn send(&mut self, messages: Vec<ToHost>) {
        if let State::In { link } = &self.state {
            for message in messages {
                link.send(wire::encode_to_host(&message));
            }
        }
    }

    /// Drops the connection now, as if the network had (tests, and
    /// "Reconnect" in the UI).
    pub fn drop_connection(&mut self) -> Vec<RemoteEvent> {
        let mut events = Vec::new();
        if matches!(self.state, State::In { .. } | State::Knocking { .. }) {
            self.lost("disconnected".to_owned(), &mut events);
        }
        events
    }

    /// What happened since the last call; connects when it's time to.
    pub fn poll(&mut self) -> Vec<RemoteEvent> {
        let mut events = Vec::new();
        if let State::Waiting { until } = self.state
            && Instant::now() >= until
        {
            self.state = State::Knocking {
                link: Link::open(&self.host, self.cert),
                knock: None,
            };
        }
        let incoming = match &self.state {
            State::Knocking { link, .. } | State::In { link } => link.poll(),
            State::Waiting { .. } | State::Refused => Vec::new(),
        };
        for event in incoming {
            self.handle(event, &mut events);
        }
        events
    }

    fn handle(&mut self, event: LinkEvent, events: &mut Vec<RemoteEvent>) {
        match event {
            LinkEvent::Opened => {
                if let State::Knocking { link, knock } = &mut self.state {
                    let (new, hello) =
                        Knock::new(&self.identity, self.cert, self.invite, &self.versions);
                    link.send(hello);
                    *knock = Some(new);
                }
            }
            LinkEvent::Message(bytes) => match &mut self.state {
                State::Knocking {
                    link,
                    knock: Some(knock),
                } => match knock.receive(&bytes, &self.identity) {
                    KnockStep::Reply(reply) => link.send(reply),
                    KnockStep::Admitted { member, role } => {
                        let State::Knocking { link, .. } =
                            std::mem::replace(&mut self.state, State::Refused)
                        else {
                            unreachable!("matched above")
                        };
                        self.state = State::In { link };
                        self.tries = 0;
                        self.invite = None;
                        events.push(RemoteEvent::Admitted { member, role });
                    }
                    KnockStep::Refused(reason) => {
                        self.state = State::Refused;
                        events.push(RemoteEvent::Refused(reason));
                    }
                },
                State::In { .. } => match wire::decode_to_client(&bytes) {
                    Ok(message) => events.push(RemoteEvent::Message(message)),
                    Err(error) => self.lost(format!("the host sent {error}"), events),
                },
                _ => {}
            },
            LinkEvent::Closed(LinkClosed::WrongHost) => {
                if !matches!(self.state, State::Refused) {
                    self.state = State::Refused;
                    events.push(RemoteEvent::Refused(LinkClosed::WrongHost.to_string()));
                }
            }
            LinkEvent::Closed(why) => {
                if matches!(self.state, State::Knocking { .. } | State::In { .. }) {
                    self.lost(why.to_string(), events);
                }
            }
        }
    }

    fn lost(&mut self, why: String, events: &mut Vec<RemoteEvent>) {
        let retry = Duration::from_secs(1 << self.tries.min(5)).min(RETRY_MOST);
        self.tries += 1;
        self.state = State::Waiting {
            until: Instant::now() + retry,
        };
        events.push(RemoteEvent::Lost { why, retry });
    }
}
