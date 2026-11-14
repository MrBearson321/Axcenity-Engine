//! [`Network`]: a game's hosting and joining, between the session and the
//! game's scripts and `Players`. The player and `axcenity-server` both run
//! it, in the frame's network receive and network send steps.
//!
//! Hosting, it sends players the world (a snapshot when they join, then
//! each frame's changes, see `replicate`) and carries RemoteEvent and
//! RemoteFunction messages between them and its scripts. Joined, the
//! device follows the server (Phase 64): its world becomes a replica of the
//! server's, and only its LocalScripts run, again from the start.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use axcenity_core::streaming::Settings;
use axcenity_core::{ClassName, InstanceId, Value, Vector3, World};
use axcenity_script::{ConsoleRun, LanGame, NetworkRequest, ScriptRuntime, Side};

use crate::discovery::{HostInfo, LanAnswerer, LanSearch};
use crate::handshake::{GameInfo, Message};
use crate::motion::{MOTION_BUDGET, Motion, MotionSender, Smoother};
use crate::ownership::{
    CharacterCheck, OWNED_TAG, Owned, OwnedSender, Sender, character_of, character_parts,
};
use crate::replay::{BufferPlay, Replay, Replays};
use crate::replicate::{LevelState, Replicator, Update, apply};
use crate::secure::{Secured, ServerCert, TokenRequest, TokenRules, Trust, short, split_join_link};
use crate::session::{Host, HostSettings, Join, SessionEvent};
use crate::streaming::{PeerStream, Streams};
use crate::transport::{Client, Conditions, LinkStats, Server};
use crate::wire::{WireKind, WireMessage};
use crate::{DEFAULT_PORT, DISCOVERY_PORT};

/// Netcode takes this many connections beyond `MaxPlayers`, so a player
/// who finds the game full is told so by the handshake.
const SPARE_CONNECTIONS: usize = 4;

/// A player flagged this many times in a minute is kicked, unless the host
/// says otherwise (`Network::set_kick_flags`; MP3). A game client is never
/// flagged; a device sending junk as fast as it can is flagged about once a
/// second for each rule it breaks.
pub const KICK_FLAGS: usize = 5;

/// How long a device that lost its connection tries to rejoin, in seconds
/// (multiplayer readiness MP2).
pub const REJOIN_SECONDS: f64 = 60.0;
/// The longest wait between two tries, in seconds; the first is 1.
const REJOIN_MOST_WAIT: f64 = 8.0;

/// What a device joined over UDP, to rejoin the same server with.
#[derive(Clone)]
struct JoinedTo {
    address: String,
    password: Option<String>,
    trust: Trust,
}

/// The file in the data folder with this install's server certificate.
const CERT_FILE: &str = "server-certificate.json";
/// The file in the data folder with the servers its player has joined.
const KNOWN_SERVERS_FILE: &str = "known-servers.json";

/// A device that lost its connection, trying to rejoin.
struct Rejoining {
    /// The server clock's time to give up at.
    deadline: f64,
    /// When to try next, and how long the wait after that is.
    next_try: f64,
    wait: f64,
    /// Why the connection was lost.
    reason: String,
}

/// A script's code, which a joined device starts again on the server's
/// world: its LocalScripts and the ModuleScripts they require.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptCode {
    /// Its Instance, at the same ID in the server's world (both load the
    /// same version of the game).
    pub instance: InstanceId,
    pub path: String,
    pub source: String,
}

/// One connection's numbers, for the Profiler's Network section.
#[derive(Clone, Debug, PartialEq)]
pub struct PeerStats {
    /// The other player's Player, while hosting; `None` for the server.
    pub player: Option<InstanceId>,
    pub link: LinkStats,
    /// Every byte sent to them so far, over the loopback.
    pub loopback_bytes_sent: Option<u64>,
}

enum Session {
    Offline,
    Hosting(Box<Host>),
    Joined(Box<Join>),
}

/// One game's place on the network: offline, hosting or joined.
pub struct Network {
    game: GameInfo,
    session: Session,
    /// The Players of the other players in the game, by their id.
    players: BTreeMap<u32, InstanceId>,
    discovery_port: u16,
    answerer: Option<LanAnswerer>,
    search: Option<LanSearch>,
    /// What happened, for the host to print.
    log: Vec<String>,
    /// What each player was last sent of the world, while hosting.
    replicator: Replicator,
    /// Players who joined and haven't been sent the world yet, by id.
    need_snapshot: BTreeSet<u32>,
    /// The code to start again once joined (see [`ScriptCode`]).
    client_scripts: Vec<ScriptCode>,
    /// Seconds since the session started: the server's clock, which
    /// motion and updates carry.
    clock: f64,
    /// The last frame's length, in seconds.
    seconds: f64,
    /// Each player's motion, while hosting, by their id.
    motion: BTreeMap<u32, MotionSender>,
    /// Bytes a second of motion each player may get.
    motion_budget: f64,
    /// The levels players were last told of.
    sent_levels: Option<LevelState>,
    /// Joined: the server's parts, smoothed.
    smoother: Smoother,
    /// Hosting: each player's character check, by their id (Phase 66).
    checks: BTreeMap<u32, CharacterCheck>,
    /// Hosting: the parts each player moves besides their character, as
    /// last worked out and sent them.
    owned_by: BTreeMap<u32, BTreeSet<InstanceId>>,
    /// Joined: the parts the server gives this device besides its
    /// character.
    owned: BTreeSet<InstanceId>,
    /// Joined: what was sent of them.
    owned_sender: OwnedSender,
    /// Joined: whether this device's player holds Run.
    running: bool,
    /// Why the player on this device was kicked (`Player:Kick`), for the
    /// host to end the game with.
    kicked: Option<String>,
    /// Hosting: what each player has of the world, while streaming is on
    /// (Phase 80).
    streams: Streams,
    /// `RequestStreamAroundAsync` calls this host's own scripts made for
    /// other players: each player's id and the request's number.
    script_requests: BTreeSet<(u32, u64)>,
    /// Requests whose regions have been sent, for the next receive to
    /// answer (the scripts wait, and `send` can't resume them).
    finished_requests: Vec<u64>,
    /// `RequestStreamAroundAsync` calls for the player on this device, for
    /// the host to load around (see [`take_viewer_requests`](Self::take_viewer_requests)).
    viewer_requests: Vec<(u64, Vector3)>,
    /// `ReplayService`'s recording and buffer (Phase 81).
    replays: Replays,
    /// The name to join with instead of the local Player's (bots, MP1).
    join_name: Option<String>,
    /// Joined: how many times the server moved this device's character
    /// back (`Correction`), since the session started.
    corrections: u64,
    /// Joined over UDP: the address, password and trusted certificate, to
    /// rejoin with.
    joined_to: Option<JoinedTo>,
    /// Hosting on the network: secured with connect tokens (MP4), unless
    /// it's a LAN game or `--insecure`.
    secure: bool,
    /// Where this install keeps its server certificate and the servers its
    /// player has joined (`None`: a new certificate each time, and any
    /// server is trusted).
    data_folder: Option<std::path::PathBuf>,
    /// The key the server gave this device to rejoin with.
    rejoin_key: u64,
    /// Lost the connection and trying to rejoin.
    rejoining: Option<Rejoining>,
    /// How long to try to rejoin for; 0 never tries.
    rejoin_seconds: f64,
    /// Times this device rejoined since it first joined.
    rejoins: u64,
    /// Times this device's world became the server's (joins and rejoins).
    follows: u64,
    /// Hosting: when each player was flagged in the last minute, by the
    /// network clock (MP3).
    flags: BTreeMap<u32, Vec<f64>>,
    /// Hosting: every flag so far.
    flag_count: u64,
    /// Hosting: a player flagged this many times in a minute is kicked;
    /// 0 never.
    kick_flags: usize,
}

impl Network {
    /// Offline, for `game`.
    pub fn new(game: GameInfo) -> Network {
        Network {
            game,
            session: Session::Offline,
            players: BTreeMap::new(),
            discovery_port: DISCOVERY_PORT,
            answerer: None,
            search: None,
            log: Vec::new(),
            replicator: Replicator::default(),
            need_snapshot: BTreeSet::new(),
            client_scripts: Vec::new(),
            clock: 0.0,
            seconds: 0.0,
            motion: BTreeMap::new(),
            motion_budget: MOTION_BUDGET,
            sent_levels: None,
            smoother: Smoother::default(),
            checks: BTreeMap::new(),
            owned_by: BTreeMap::new(),
            owned: BTreeSet::new(),
            owned_sender: OwnedSender::default(),
            running: false,
            kicked: None,
            streams: Streams::default(),
            script_requests: BTreeSet::new(),
            finished_requests: Vec::new(),
            viewer_requests: Vec::new(),
            replays: Replays::default(),
            join_name: None,
            corrections: 0,
            joined_to: None,
            secure: true,
            data_folder: None,
            rejoin_key: 0,
            rejoining: None,
            rejoin_seconds: REJOIN_SECONDS,
            rejoins: 0,
            follows: 0,
            flags: BTreeMap::new(),
            flag_count: 0,
            kick_flags: KICK_FLAGS,
        }
    }

    /// Where `ReplayService:StopRecording` saves: the player's data
    /// folder's `replays` (Phase 81). Without one, finished replays wait
    /// in [`take_replays`](Self::take_replays).
    pub fn set_replay_folder(&mut self, folder: Option<std::path::PathBuf>) {
        self.replays.set_folder(folder);
    }

    /// Recordings finished with no folder to save them in, by name.
    pub fn take_replays(&mut self) -> Vec<(String, Replay)> {
        self.replays.take_kept()
    }

    /// The buffer a script's `ReplayService:PlayBuffer()` asked to show,
    /// for the host to play over the game.
    pub fn take_buffer_play(&mut self) -> Option<BufferPlay> {
        self.replays.take_play()
    }

    /// `RequestStreamAroundAsync` calls for the player on this device,
    /// each a request and where: the host loads around there, then calls
    /// `ScriptRuntime::finish_stream_around`.
    pub fn take_viewer_requests(&mut self) -> Vec<(u64, Vector3)> {
        std::mem::take(&mut self.viewer_requests)
    }

    /// Hosting: where each joined player's character is, so physics stays
    /// awake around them while streaming (Phase 80).
    pub fn player_focuses(&self, world: &World) -> Vec<Vector3> {
        self.players
            .values()
            .filter_map(|&player| character_position(world, player))
            .collect()
    }

    /// What each joined player has, while streaming is on (tests and the
    /// Profiler).
    pub fn streams(&self) -> &Streams {
        &self.streams
    }

    /// Why the player on this device was kicked, once: the host ends the
    /// game (the player quits; the editor stops Play) with the reason.
    pub fn take_kicked(&mut self) -> Option<String> {
        self.kicked.take()
    }

    /// Joined: whether this device's player holds Run this frame, so the
    /// server allows their character `RunSpeed`.
    pub fn set_running(&mut self, running: bool) {
        self.running = running;
    }

    /// Whether this device follows a server it joined.
    pub fn is_following(&self) -> bool {
        matches!(self.session, Session::Joined(_)) || self.rejoining.is_some()
    }

    /// How long a device that lost its connection tries to rejoin, in
    /// seconds ([`REJOIN_SECONDS`] unless set); 0 gives up at once.
    pub fn set_rejoin_seconds(&mut self, seconds: f64) {
        self.rejoin_seconds = seconds.max(0.0);
    }

    /// Lost the connection and trying to rejoin.
    pub fn is_rejoining(&self) -> bool {
        self.rejoining.is_some()
    }

    /// Times this device rejoined after losing its connection.
    pub fn rejoins(&self) -> u64 {
        self.rejoins
    }

    /// Hosting: kick a player flagged `flags` times in a minute; 0 never
    /// kicks (scripts still hear `Players.PlayerFlagged`).
    pub fn set_kick_flags(&mut self, flags: usize) {
        self.kick_flags = flags;
    }

    /// Hosting: how many times players were flagged since hosting began.
    pub fn flag_count(&self) -> u64 {
        self.flag_count
    }

    /// Times this device's world became the server's: on joining and on
    /// each rejoin. A host starts its physics, characters and Animators
    /// again on the new world whenever this changes.
    pub fn follows(&self) -> u64 {
        self.follows
    }

    /// The characters other devices move, which this device's controllers
    /// leave alone: hosting, the other players'; joined, everyone's but
    /// this device's own.
    pub fn other_characters(&self, world: &World, scripts: &ScriptRuntime) -> BTreeSet<InstanceId> {
        match &self.session {
            Session::Offline => BTreeSet::new(),
            Session::Hosting(_) => self
                .players
                .values()
                .filter_map(|&player| character_of(world, player))
                .collect(),
            Session::Joined(_) => {
                let me = scripts.local_player();
                world
                    .get(world.players())
                    .map(|players| players.children().to_vec())
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|&player| Some(player) != me)
                    .filter_map(|player| character_of(world, player))
                    .collect()
            }
        }
    }

    /// Parts physics holds still on this device because another moves
    /// them: hosting, the other players' characters and what they own;
    /// joined, every unanchored part but this device's own.
    pub fn held_parts(&self, world: &World, scripts: &ScriptRuntime) -> BTreeSet<InstanceId> {
        match &self.session {
            Session::Offline => BTreeSet::new(),
            Session::Hosting(_) => {
                let mut held: BTreeSet<InstanceId> =
                    self.owned_by.values().flatten().copied().collect();
                for &player in self.players.values() {
                    held.extend(character_parts(world, player));
                }
                held
            }
            Session::Joined(_) => {
                let mine = self.moved_here(world, scripts);
                world
                    .descendants_iter(world.workspace())
                    .filter(|id| !mine.contains(id))
                    .filter(|&id| {
                        world.get(id).is_some_and(|i| {
                            i.class().is_base_part()
                                && i.property("Anchored").and_then(Value::as_bool) == Some(false)
                        })
                    })
                    .collect()
            }
        }
    }

    /// Joined: whether this device simulates anything (an assembly the
    /// server gave it), so it steps physics.
    pub fn simulates_physics(&self) -> bool {
        match &self.session {
            Session::Joined(_) => !self.owned.is_empty(),
            _ => true,
        }
    }

    /// Joined: the parts this device moves: its character's, and those the
    /// server gave it.
    fn moved_here(&self, world: &World, scripts: &ScriptRuntime) -> BTreeSet<InstanceId> {
        let mut parts: BTreeSet<InstanceId> = scripts
            .local_player()
            .map(|me| character_parts(world, me))
            .unwrap_or_default()
            .into_iter()
            .collect();
        parts.extend(self.owned.iter().copied().filter(|&id| world.contains(id)));
        parts
    }

    /// Gives each player `bytes_per_second` of motion (tests).
    pub fn with_motion_budget(mut self, bytes_per_second: f64) -> Network {
        self.motion_budget = bytes_per_second;
        self
    }

    /// The game's ModuleScripts and LocalScripts (in the order they start),
    /// to start again on the server's world once joined.
    pub fn set_client_scripts(&mut self, scripts: Vec<ScriptCode>) {
        self.client_scripts = scripts;
    }

    /// Another loopback player for an in-memory host (tests, and the
    /// editor's Play with several clients).
    pub fn connect_loopback(&mut self) -> Option<Client> {
        match &mut self.session {
            Session::Hosting(host) if host.port().is_none() => {
                Some(host.server_mut().connect_loopback())
            }
            _ => None,
        }
    }

    /// Makes the in-memory host's connections as bad as `conditions`
    /// (the editor's Play settings, and tests). Nothing for UDP.
    pub fn emulate(&mut self, conditions: Conditions) {
        if let Session::Hosting(host) = &mut self.session {
            host.server_mut().emulate(conditions);
        }
    }

    /// How each other player's connection is doing, while hosting; this
    /// device's connection to the server, joined.
    pub fn stats(&self) -> Vec<PeerStats> {
        match &self.session {
            Session::Offline => Vec::new(),
            Session::Hosting(host) => self
                .players
                .iter()
                .map(|(&peer, &player)| PeerStats {
                    player: Some(player),
                    link: host.stats_of(peer).unwrap_or_default(),
                    loopback_bytes_sent: host.loopback_bytes_sent_to(peer),
                })
                .collect(),
            Session::Joined(join) => vec![PeerStats {
                player: None,
                link: join.stats(),
                loopback_bytes_sent: None,
            }],
        }
    }

    /// The Player of the player with session id `peer`, while hosting.
    pub fn player_of(&self, peer: u32) -> Option<InstanceId> {
        self.players.get(&peer).copied()
    }

    /// Uses another port for LAN discovery (tests, so they don't meet a
    /// game running on the same computer).
    pub fn with_discovery_port(mut self, port: u16) -> Network {
        self.discovery_port = port;
        self
    }

    /// The UDP port hosted on, while hosting.
    pub fn port(&self) -> Option<u16> {
        match &self.session {
            Session::Hosting(host) => host.port(),
            _ => None,
        }
    }

    pub fn is_hosting(&self) -> bool {
        matches!(self.session, Session::Hosting(_))
    }

    /// Joined to a server elsewhere: sends `line`, typed in the Developer
    /// Console, to it for its commands (Phase 71). False, and nothing
    /// sent, when not joined.
    pub fn send_console_command(&mut self, line: &str) -> bool {
        match &mut self.session {
            Session::Joined(join) if join.you().is_some() => {
                join.send(&Message::Console {
                    line: line.to_owned(),
                });
                true
            }
            _ => false,
        }
    }

    /// Joined and taken in by the host.
    pub fn is_joined(&self) -> bool {
        matches!(&self.session, Session::Joined(join) if join.you().is_some())
    }

    /// What happened since the last call (players joining and leaving,
    /// refusals, failures), oldest first, for the host to print.
    pub fn take_log(&mut self) -> Vec<String> {
        std::mem::take(&mut self.log)
    }

    /// Hosts the game on UDP `port` (`None`: [`DEFAULT_PORT`]; 0: any free
    /// port). `own_player` is a listen server, whose player on this device
    /// is in the game; a dedicated server has none. `Players.MaxPlayers`
    /// is how many it takes. Returns the port, or why it can't host.
    pub fn host(
        &mut self,
        world: &mut World,
        scripts: &mut ScriptRuntime,
        port: Option<u16>,
        password: Option<String>,
        own_player: bool,
    ) -> Result<u16, String> {
        self.disconnect(world, scripts);
        let max_players = max_players(world);
        let port = port.unwrap_or(DEFAULT_PORT);
        let server = if self.secure {
            let rules = TokenRules {
                game: self.game.clone(),
                password: password.clone(),
            };
            Server::listen_secure(
                port,
                max_players + SPARE_CONNECTIONS,
                Secured::new(self.certificate()),
                rules,
            )
            .map_err(|err| format!("Can't host on port {port} (UDP and TCP): {err}."))?
        } else {
            Server::listen(port, max_players + SPARE_CONNECTIONS)
                .map_err(|err| format!("Can't host on port {port}: {err}."))?
        };
        let port = server.port().unwrap_or(port);
        let fingerprint = server.fingerprint().map(str::to_owned);
        let own_player = own_player.then(|| local_name(world, scripts));
        let host = Host::new(
            server,
            HostSettings {
                game: self.game.clone(),
                max_players,
                password,
                own_player,
            },
        );
        self.session = Session::Hosting(Box::new(host));
        self.answerer = match LanAnswerer::bind(self.discovery_port) {
            Ok(answerer) => Some(answerer),
            Err(err) => {
                self.log.push(format!(
                    "warning: LAN searches won't find this game (discovery port {}: {err}); \
                     players can still join by address",
                    self.discovery_port
                ));
                None
            }
        };
        self.log.push(format!(
            "Hosting \"{}\" (version {}) on UDP port {port}, for {max_players} players.",
            self.game.name, self.game.version
        ));
        match fingerprint {
            Some(fingerprint) => self.log.push(format!(
                "Secured: connect tokens on TCP port {port}, every packet encrypted. \
                 Certificate {}; players join with \
                 axcenity://join/<this computer's address>:{port}#{fingerprint}",
                short(&fingerprint)
            )),
            None => self.log.push(
                "warning: not secured (LAN hosting or --insecure): packets aren't encrypted \
                 and anyone who can reach the port can try to join."
                    .to_owned(),
            ),
        }
        Ok(port)
    }

    /// Hosts secured (the default) or not: LAN games and `--insecure`
    /// aren't. Takes effect at the next [`host`](Self::host).
    pub fn set_secure(&mut self, secure: bool) {
        self.secure = secure;
    }

    /// Where this install keeps its server certificate (so players who
    /// joined before still trust it) and the certificates of servers its
    /// player joined: the engine's data folder, never a project.
    pub fn set_data_folder(&mut self, folder: Option<std::path::PathBuf>) {
        self.data_folder = folder;
    }

    /// While hosting secured: the certificate's fingerprint, which join
    /// links carry.
    pub fn fingerprint(&self) -> Option<String> {
        match &self.session {
            Session::Hosting(host) => host.server().fingerprint().map(str::to_owned),
            _ => None,
        }
    }

    /// While hosting secured: how many connect tokens were handed out.
    pub fn tokens_issued(&self) -> usize {
        match &self.session {
            Session::Hosting(host) => host.server().tokens_issued(),
            _ => 0,
        }
    }

    /// This install's server certificate: the one in the data folder (made
    /// there the first time), else a new one.
    fn certificate(&mut self) -> ServerCert {
        let Some(folder) = &self.data_folder else {
            return ServerCert::generate();
        };
        let path = folder.join(CERT_FILE);
        match ServerCert::load_or_make(&path) {
            Ok(cert) => cert,
            Err(err) => {
                self.log.push(format!(
                    "warning: can't use the server certificate at {} ({err}); using a new \
                     one, so players who joined before are told it changed",
                    path.display()
                ));
                ServerCert::generate()
            }
        }
    }

    /// Which certificate to accept from the server at `link`'s address: the
    /// link's fingerprint, else the one remembered for the address.
    fn trust_for(&self, fingerprint: Option<String>) -> Trust {
        match (fingerprint, &self.data_folder) {
            (Some(fingerprint), _) => Trust::Pinned(fingerprint),
            (None, Some(folder)) => Trust::Remember(folder.join(KNOWN_SERVERS_FILE)),
            (None, None) => Trust::Any,
        }
    }

    /// A client joining `to`, secured when the server is.
    fn client_for(&self, to: &JoinedTo) -> std::io::Result<Client> {
        let game = self.game.clone();
        let password = to.password.clone();
        Client::join(
            &to.address,
            move |address| TokenRequest::new(&game, password, address),
            to.trust.clone(),
        )
    }

    /// Hosts with an in-memory server only loopback clients reach, and
    /// returns one (single-player and editor Play, and tests).
    pub fn host_in_memory(
        &mut self,
        world: &mut World,
        scripts: &mut ScriptRuntime,
        own_player: bool,
    ) -> Client {
        self.disconnect(world, scripts);
        let own_player = own_player.then(|| local_name(world, scripts));
        let mut host = Host::new(
            Server::in_memory(),
            HostSettings {
                game: self.game.clone(),
                max_players: max_players(world),
                password: None,
                own_player,
            },
        );
        let client = host.server_mut().connect_loopback();
        self.session = Session::Hosting(Box::new(host));
        client
    }

    /// Joins the game at `address` (`"host:port"`, or a join link
    /// `axcenity://join/host:port#fingerprint`), or says why it can't
    /// start. A refusal or failure later fires `ConnectionFailed`. A
    /// secured server's certificate must match the link's fingerprint, or
    /// the one remembered for the address (MP4).
    pub fn join(
        &mut self,
        world: &mut World,
        scripts: &mut ScriptRuntime,
        address: &str,
        password: Option<String>,
    ) -> Result<(), String> {
        let (address, fingerprint) = split_join_link(address);
        let to = JoinedTo {
            address,
            password,
            trust: self.trust_for(fingerprint),
        };
        let client = self
            .client_for(&to)
            .map_err(|err| format!("Can't join {}: {err}.", to.address))?;
        self.join_through(world, scripts, client, to.password.clone());
        self.log.push(format!("Joining {}...", to.address));
        self.joined_to = Some(to);
        self.rejoins = 0;
        Ok(())
    }

    /// Joins with `name` instead of the local Player's name, which a
    /// headless device such as a bot (`axcenity-bots`) doesn't have.
    pub fn set_join_name(&mut self, name: impl Into<String>) {
        self.join_name = Some(name.into());
    }

    /// Joined: how many times the server moved this device's character
    /// back since it joined.
    pub fn corrections(&self) -> u64 {
        self.corrections
    }

    /// Joins through `client`, such as a loopback one.
    pub fn join_through(
        &mut self,
        world: &mut World,
        scripts: &mut ScriptRuntime,
        client: Client,
        password: Option<String>,
    ) {
        self.disconnect(world, scripts);
        let join = self.start_join(world, scripts, client, password);
        self.corrections = 0;
        self.session = Session::Joined(Box::new(join));
    }

    fn start_join(
        &self,
        world: &World,
        scripts: &ScriptRuntime,
        client: Client,
        password: Option<String>,
    ) -> Join {
        let name = self
            .join_name
            .clone()
            .unwrap_or_else(|| local_name(world, scripts));
        let user_id = scripts.user_id() as u64;
        Join::start(client, &self.game, &name, password, user_id)
    }

    /// While rejoining, tries again when it's time; gives up at the
    /// deadline, as a failed join does.
    fn try_rejoin(&mut self, world: &mut World, scripts: &mut ScriptRuntime) {
        let Some(rejoining) = &mut self.rejoining else {
            return;
        };
        if !matches!(self.session, Session::Offline) || self.clock < rejoining.next_try {
            return;
        }
        if self.clock >= rejoining.deadline {
            let reason = format!(
                "Lost the connection to the server and couldn't rejoin: {}",
                rejoining.reason
            );
            self.rejoining = None;
            self.joined_to = None;
            self.remove_players(world, scripts);
            self.log.push(format!("warning: {reason}"));
            scripts.fire_connection_failed(world, &reason);
            return;
        }
        rejoining.next_try = self.clock + rejoining.wait;
        rejoining.wait = (rejoining.wait * 2.0).min(REJOIN_MOST_WAIT);
        let Some(to) = self.joined_to.clone() else {
            return;
        };
        match self.client_for(&to) {
            Ok(client) => {
                let join = self
                    .start_join(world, scripts, client, to.password)
                    .rejoining(self.rejoin_key);
                self.session = Session::Joined(Box::new(join));
                self.log.push(format!("Rejoining {}...", to.address));
            }
            Err(err) => self
                .log
                .push(format!("Can't rejoin {} yet: {err}.", to.address)),
        }
    }

    /// Leaves the game or stops hosting; the other players leave `Players`.
    pub fn disconnect(&mut self, world: &mut World, scripts: &mut ScriptRuntime) {
        match std::mem::replace(&mut self.session, Session::Offline) {
            Session::Offline => {}
            Session::Hosting(mut host) => {
                host.shut_down();
                self.log.push("Stopped hosting.".to_owned());
            }
            Session::Joined(mut join) => {
                join.leave();
                self.log.push("Left the game.".to_owned());
            }
        }
        self.answerer = None;
        self.joined_to = None;
        self.rejoining = None;
        self.replicator = Replicator::default();
        self.need_snapshot.clear();
        self.motion.clear();
        self.sent_levels = None;
        self.smoother = Smoother::default();
        self.streams.clear();
        self.script_requests.clear();
        self.finished_requests.clear();
        self.checks.clear();
        self.owned_by.clear();
        self.owned.clear();
        self.owned_sender = OwnedSender::default();
        self.remove_players(world, scripts);
    }

    fn remove_players(&mut self, world: &mut World, scripts: &mut ScriptRuntime) {
        let mut players: Vec<InstanceId> =
            std::mem::take(&mut self.players).into_values().collect();
        // A replica has the server's Players; the device keeps its own.
        if scripts.is_client_only() {
            players.extend(world.get(world.players()).into_iter().flat_map(|service| {
                service.children().iter().copied().filter(|&id| {
                    world
                        .get(id)
                        .is_some_and(|i| i.class() == ClassName::Player)
                })
            }));
        }
        for player in players {
            scripts.remove_player(world, player);
        }
    }

    /// Frame order's network receive, before input: does what scripts
    /// asked of `NetworkService` last frame, then takes in what arrived,
    /// so `Players` has whoever joined or left, and answers LAN searches.
    pub fn receive(&mut self, world: &mut World, scripts: &mut ScriptRuntime, seconds: f64) {
        for request in scripts.take_network_requests() {
            self.follow(world, scripts, request);
        }
        let elapsed = Duration::from_secs_f64(seconds.max(0.0));
        self.seconds = seconds.max(0.0);
        self.clock += self.seconds;
        self.try_rejoin(world, scripts);
        let events = match &mut self.session {
            Session::Offline => Vec::new(),
            Session::Hosting(host) => {
                self.log.extend(host.server().take_token_log());
                host.update(elapsed)
            }
            Session::Joined(join) => join.update(elapsed),
        };
        for event in events {
            self.apply(world, scripts, event);
        }
        self.chat_and_kicks(world, scripts);
        if scripts.is_client_only() && matches!(self.session, Session::Joined(_)) {
            self.smoother.step(world, self.seconds);
        }
        for request in std::mem::take(&mut self.finished_requests) {
            scripts.finish_stream_around(world, request);
        }
        if let (Some(answerer), Session::Hosting(host)) = (&self.answerer, &self.session) {
            let settings = host.settings();
            answerer.answer(&HostInfo {
                game: settings.game.name.clone(),
                version: settings.game.version.clone(),
                port: host.port().unwrap_or_default(),
                players: host.players().len() as u32,
                max_players: settings.max_players as u32,
                password: settings.password.is_some(),
            });
        }
        if let Some(search) = &mut self.search
            && let Some(games) = search.update(elapsed)
        {
            self.search = None;
            scripts.finish_lan_search(world, &games);
        }
    }

    /// Frame order's network send, after the frame's scripts and physics.
    /// Hosting: each player gets what changed in `world` (a player who
    /// just joined, all of it), then the scripts' messages for them.
    /// Joined: the server gets the scripts' messages.
    pub fn send(&mut self, world: &World, scripts: &mut ScriptRuntime) {
        // Replays (Phase 81) note the world as this frame left it.
        let requests = scripts.take_replay_requests();
        self.replays
            .frame(world, requests, self.seconds, &self.game, &mut self.log);
        let messages = scripts.take_remote_messages();
        let mine = self.moved_here(world, scripts);
        match &mut self.session {
            Session::Offline => {}
            Session::Hosting(host) => {
                let levels = level_state(scripts);
                // With nobody to send it to there's nothing to compare yet.
                let mut update = if self.players.is_empty() {
                    Update::default()
                } else {
                    self.replicator.frame(world)
                };
                update.time = self.clock;
                if levels != self.sent_levels {
                    update.levels.clone_from(&levels);
                    self.sent_levels.clone_from(&levels);
                }
                // Streaming (Phase 80): each player gets their share of
                // this, and what's come into range and left it.
                let settings = Settings::of(world);
                let streaming = settings.is_some() || self.streams.is_active();
                if streaming {
                    let tick = self.streams.frame(world, self.seconds);
                    for (&peer, &player) in &self.players {
                        if self.need_snapshot.contains(&peer) {
                            continue;
                        }
                        let character = character_position(world, player);
                        let owned = self.owned_by.get(&peer).cloned().unwrap_or_default();
                        let result = self.streams.for_peer(
                            world,
                            settings.as_ref(),
                            peer,
                            character,
                            &owned,
                            &update,
                            tick,
                            self.clock,
                        );
                        for mut update in result.updates {
                            update.time = self.clock;
                            host.send_to(peer, &Message::Update { update });
                        }
                        for request in result.answered {
                            if self.script_requests.remove(&(peer, request)) {
                                self.finished_requests.push(request);
                            } else {
                                host.send_to(peer, &Message::StreamedAround { request });
                            }
                        }
                    }
                } else if !update.is_empty() {
                    let message = Message::Update { update };
                    for &peer in self.players.keys() {
                        if !self.need_snapshot.contains(&peer) {
                            host.send_to(peer, &message);
                        }
                    }
                }
                let parts = if streaming {
                    Vec::new()
                } else {
                    self.replicator.parts(world)
                };
                // Lag compensation (Phase 81): where moving parts are now,
                // and how late each player sees them: their round trip
                // plus the smoothing delay.
                if !self.players.is_empty() {
                    let rewinds: Vec<(InstanceId, f64)> = self
                        .players
                        .iter()
                        .map(|(&peer, &player)| {
                            let rtt = host.stats_of(peer).map_or(0.0, |stats| stats.rtt);
                            (player, rtt + crate::motion::DELAY)
                        })
                        .collect();
                    scripts.record_lag(world, self.clock, &rewinds);
                }
                // Who moves what (Phase 66): each player their character,
                // and the assemblies the server gives them.
                let owned = scripts.owned_parts(world);
                for (&peer, &player) in &self.players {
                    let now: BTreeSet<InstanceId> =
                        owned.get(&player).into_iter().flatten().copied().collect();
                    if self.owned_by.get(&peer) != Some(&now) {
                        let parts = now.iter().map(|id| id.to_bits()).collect();
                        host.send_to(peer, &Message::Ownership { parts });
                        self.owned_by.insert(peer, now);
                    }
                }
                self.owned_by
                    .retain(|peer, _| self.players.contains_key(peer));
                for (&peer, &player) in &self.players {
                    let check = self.checks.entry(peer).or_default();
                    if let Some(update) = check.correction(world, player, self.clock) {
                        host.send_to(peer, &Message::Correction { update });
                    }
                }
                self.checks
                    .retain(|peer, _| self.players.contains_key(peer));
                for peer in std::mem::take(&mut self.need_snapshot) {
                    let Some(&you) = self.players.get(&peer) else {
                        continue;
                    };
                    let mut update = if let Some(settings) = &settings {
                        let owned = self.owned_by.get(&peer).cloned().unwrap_or_default();
                        let character = character_position(world, you);
                        self.streams
                            .start_peer(world, settings, peer, character, &owned);
                        let streams = &self.streams;
                        let mut update = self
                            .replicator
                            .snapshot_of(world, |id| streams.snapshot_has(peer, id));
                        update.streamed_out = self.streams.out_of(peer);
                        update
                    } else {
                        self.replicator.snapshot(world)
                    };
                    update.time = self.clock;
                    update.levels.clone_from(&levels);
                    host.send_to(
                        peer,
                        &Message::Snapshot {
                            you: you.to_bits(),
                            update,
                        },
                    );
                    let theirs = self
                        .streams
                        .of(peer)
                        .map_or(parts.as_slice(), PeerStream::parts);
                    self.motion
                        .insert(peer, MotionSender::new(world, theirs, self.motion_budget));
                }
                // Then each player's share of the parts that moved.
                for (&peer, sender) in &mut self.motion {
                    let Some(&player) = self.players.get(&peer) else {
                        continue;
                    };
                    let focus = character_position(world, player);
                    // A player isn't sent what they move themselves.
                    let mut theirs: BTreeSet<InstanceId> =
                        character_parts(world, player).into_iter().collect();
                    theirs.extend(self.owned_by.get(&peer).into_iter().flatten().copied());
                    let (packets, settled) = sender.frame(
                        world,
                        self.streams
                            .of(peer)
                            .map_or(parts.as_slice(), PeerStream::parts),
                        focus,
                        &theirs,
                        self.seconds,
                        self.clock,
                    );
                    if !settled.is_empty() {
                        let update = Update {
                            ops: settled,
                            time: self.clock,
                            ..Update::default()
                        };
                        host.send_to(peer, &Message::Update { update });
                    }
                    for packet in packets {
                        host.send_datagram(peer, packet);
                    }
                }
                // Players never hear of the server's own Instances.
                let visible = |id: InstanceId| world.contains(id) && !world.is_server_only(id);
                for message in messages {
                    let peer = self
                        .players
                        .iter()
                        .find(|&(_, &player)| player == message.player)
                        .map(|(&peer, _)| peer);
                    if message.to == Side::Client
                        && let Some(peer) = peer
                    {
                        let message = WireMessage::from_message(&message, &visible);
                        host.send_to(peer, &Message::Remote { message });
                    }
                }
                host.flush();
            }
            Session::Joined(join) => {
                // What this device moves goes to the server, and isn't
                // smoothed from what the server sends back.
                if scripts.is_client_only() && join.you().is_some() {
                    self.smoother.set_owned(mine.clone());
                    let mine: Vec<InstanceId> = mine.into_iter().collect();
                    let seat = own_seat(world, scripts);
                    if let Some(owned) = self.owned_sender.frame(
                        world,
                        &mine,
                        self.running,
                        seat,
                        self.seconds,
                        self.clock,
                    ) {
                        join.send_datagram(owned.to_bytes());
                    }
                }
                // The server never had this device's own Instances.
                let shared = |id: InstanceId| !id.is_local();
                for message in messages {
                    let message = WireMessage::from_message(&message, &shared);
                    join.send(&Message::Remote { message });
                }
                join.flush();
            }
        }
    }

    fn follow(&mut self, world: &mut World, scripts: &mut ScriptRuntime, request: NetworkRequest) {
        let result = match request {
            NetworkRequest::Host { port, password } => {
                self.host(world, scripts, port, password, true).map(|_| ())
            }
            NetworkRequest::Join { address, password } => {
                self.join(world, scripts, &address, password)
            }
            NetworkRequest::Disconnect => {
                self.disconnect(world, scripts);
                Ok(())
            }
            NetworkRequest::StreamAround {
                player,
                position,
                request,
            } => {
                self.stream_around(world, scripts, player, position, request);
                Ok(())
            }
            NetworkRequest::FindLanGames => {
                if self.search.is_none() {
                    match LanSearch::start(self.discovery_port, &self.game.name) {
                        Ok(search) => self.search = Some(search),
                        Err(err) => {
                            self.log
                                .push(format!("warning: can't search the LAN: {err}"));
                            scripts.finish_lan_search(world, &Vec::<LanGame>::new());
                        }
                    }
                }
                Ok(())
            }
        };
        if let Err(reason) = result {
            self.log.push(format!("warning: {reason}"));
            scripts.fire_connection_failed(world, &reason);
        }
    }

    fn apply(&mut self, world: &mut World, scripts: &mut ScriptRuntime, event: SessionEvent) {
        match event {
            SessionEvent::Joined(peer) => {
                let you = match &self.session {
                    Session::Joined(join) => join.you().map(|you| you.id),
                    _ => None,
                };
                if you == Some(peer.id) || self.players.contains_key(&peer.id) {
                    return;
                }
                // A player's device gets everyone's Player with the
                // server's world.
                if !self.is_hosting() {
                    return;
                }
                self.log.push(format!("{} joined.", peer.name));
                let player = scripts.add_remote_player(
                    world,
                    &peer.name,
                    &peer.display_name,
                    peer.user_id as f64,
                );
                self.players.insert(peer.id, player);
                self.need_snapshot.insert(peer.id);
            }
            SessionEvent::Left(id) => {
                self.need_snapshot.remove(&id);
                self.streams.remove(id);
                if let Some(player) = self.players.remove(&id) {
                    if self.is_hosting() {
                        let name = world
                            .get(player)
                            .map(|player| player.name().to_owned())
                            .unwrap_or_default();
                        self.log.push(format!("{name} left."));
                    }
                    scripts.remove_player(world, player);
                }
            }
            SessionEvent::Welcomed(you) => {
                if let Session::Joined(join) = &self.session {
                    self.rejoin_key = join.rejoin_key();
                }
                scripts.rename_local_player(world, &you.name);
                self.log
                    .push(format!("Joined \"{}\" as {}.", self.game.name, you.name));
            }
            SessionEvent::Refused(why) => self.log.push(format!("warning: {why}")),
            SessionEvent::Snapshot { you, update } => {
                if !self.is_hosting() {
                    self.follow_server(world, scripts, InstanceId::from_bits(you), &update);
                    self.follows += 1;
                    if self.rejoining.take().is_some() {
                        self.rejoins += 1;
                        self.log.push("Rejoined the game.".to_owned());
                    }
                }
            }
            SessionEvent::Update(update) => {
                if !self.is_hosting() && scripts.is_client_only() {
                    let applied = apply(world, &update);
                    for id in applied.placed {
                        self.smoother.settle(id, update.time);
                    }
                    if !applied.streamed_out.is_empty() || !applied.streamed_in.is_empty() {
                        scripts.note_streaming(world, &applied.streamed_out, &applied.streamed_in);
                    }
                    if let Some(levels) = &update.levels {
                        follow_levels(scripts, levels, true);
                    }
                    for player in applied.players_leaving {
                        scripts.remove_player(world, player);
                    }
                    for player in applied.players_added {
                        scripts.add_replicated_player(world, player);
                    }
                }
            }
            SessionEvent::Remote { from, message } => self.deliver(world, scripts, from, &message),
            SessionEvent::Datagram { from, bytes } => {
                if self.is_hosting() {
                    if bytes.first() == Some(&OWNED_TAG)
                        && let Some(owned) = Owned::from_bytes(&bytes)
                        && let Some(&player) = self.players.get(&from)
                    {
                        let assemblies = self.owned_by.get(&from).cloned().unwrap_or_default();
                        let characters: Vec<InstanceId> = self
                            .players
                            .values()
                            .filter_map(|&player| character_of(world, player))
                            .collect();
                        let sender = Sender {
                            player,
                            assemblies: &assemblies,
                            characters: &characters,
                        };
                        let flagged = self.checks.entry(from).or_default().receive(
                            world,
                            &scripts.assets(),
                            &sender,
                            &owned,
                            self.clock,
                        );
                        if let Some(reason) = flagged {
                            self.flag(world, scripts, from, reason);
                        }
                    }
                } else if scripts.is_client_only()
                    && let Some(motion) = Motion::from_bytes(&bytes)
                {
                    self.smoother.receive(world, &motion);
                }
            }
            SessionEvent::Flagged { from, reason } => self.flag(world, scripts, from, reason),
            SessionEvent::Chat { from, text } => {
                let Some(&player) = self.players.get(&from) else {
                    return;
                };
                let Some(text) = crate::chat_text(&text) else {
                    return;
                };
                if let Some(text) = scripts.filter_chat(world, Some(player), &text) {
                    self.deliver_chat(scripts, Some(player), text);
                }
            }
            SessionEvent::Console { from, line } => {
                let Some(&player) = self.players.get(&from) else {
                    return;
                };
                let name = world
                    .get(player)
                    .map(|p| p.name().to_owned())
                    .unwrap_or_default();
                match scripts.run_remote_console_command(world, player, &line) {
                    ConsoleRun::Ran => self.log.push(format!("{name} ran `{line}`")),
                    ConsoleRun::NotAllowed => self.log.push(format!(
                        "warning: {name} may not run `{line}`: their UserId isn't in ConsoleService.AllowedUserIds"
                    )),
                    ConsoleRun::Unknown => self
                        .log
                        .push(format!("warning: {name} typed an unknown command `{line}`")),
                }
            }
            SessionEvent::ChatLine { player, text } => {
                if !self.is_hosting() && scripts.is_client_only() {
                    let player = (player != 0)
                        .then(|| InstanceId::from_bits(player))
                        .filter(|&id| world.contains(id));
                    scripts.receive_chat(player, text);
                }
            }
            SessionEvent::Ownership(parts) => {
                if !self.is_hosting() {
                    self.owned = parts.into_iter().map(InstanceId::from_bits).collect();
                }
            }
            SessionEvent::Correction(update) => {
                if !self.is_hosting() && scripts.is_client_only() {
                    apply(world, &update);
                    self.corrections += 1;
                    self.log
                        .push("The server moved your character back.".to_owned());
                }
            }
            SessionEvent::StreamAround {
                from,
                request,
                position,
            } => {
                if self.players.contains_key(&from) {
                    let [x, y, z] = position;
                    self.stream_around_for_peer(world, from, Vector3::new(x, y, z), request, false);
                }
            }
            SessionEvent::StreamedAround(request) => {
                scripts.finish_stream_around(world, request);
            }
            // Lost after joining (the server crashed or stopped, or the
            // network failed): the world stays as it was while this device
            // tries to rejoin (MP2).
            SessionEvent::Lost(reason) if self.rejoin_seconds > 0.0 && self.joined_to.is_some() => {
                self.session = Session::Offline;
                self.owned.clear();
                self.owned_sender = OwnedSender::default();
                self.log.push(format!(
                    "warning: Lost the connection to the server: {reason} Trying to rejoin for {} seconds.",
                    self.rejoin_seconds
                ));
                self.rejoining = Some(Rejoining {
                    deadline: self.clock + self.rejoin_seconds,
                    next_try: self.clock + 1.0,
                    wait: 1.0,
                    reason,
                });
            }
            // A try at rejoining that failed: the next waits longer.
            SessionEvent::Failed(reason) if self.rejoining.is_some() => {
                self.session = Session::Offline;
                self.log.push(format!("Couldn't rejoin yet: {reason}"));
            }
            SessionEvent::Failed(reason) | SessionEvent::Lost(reason) => {
                self.session = Session::Offline;
                self.joined_to = None;
                self.remove_players(world, scripts);
                self.log.push(format!("warning: {reason}"));
                scripts.fire_connection_failed(world, &reason);
            }
        }
    }
}

impl Network {
    /// This device joined: its world becomes `update`, the server's world,
    /// with its own copy's Terrain landscapes (the same version of the
    /// game), and its LocalScripts start again there as `you`'s.
    fn follow_server(
        &mut self,
        world: &mut World,
        scripts: &mut ScriptRuntime,
        you: InstanceId,
        update: &Update,
    ) {
        let mut replica = world.empty_replica();
        let applied = apply(&mut replica, update);
        self.smoother = Smoother::default();
        if let Some(levels) = &update.levels {
            follow_levels(scripts, levels, false);
        }
        let terrains: Vec<InstanceId> = replica
            .descendants_iter(replica.root())
            .filter(|&id| {
                replica
                    .get(id)
                    .is_some_and(|i| i.class() == ClassName::Terrain)
            })
            .collect();
        for id in terrains {
            if let Some(data) = world.terrain(id) {
                replica.set_terrain(id, data.clone());
            }
        }
        // Scripts inside objects (G2) that aren't in the server's world
        // yet: templates the server will clone, and those in models not
        // streamed in. They start with their file's code when they arrive.
        let objects: BTreeSet<InstanceId> = self
            .client_scripts
            .iter()
            .filter(|code| {
                world
                    .get(code.instance)
                    .is_some_and(|i| i.source_file().is_some())
            })
            .map(|code| code.instance)
            .collect();
        scripts.become_client(world, replica, you);
        scripts.note_streaming(world, &applied.streamed_out, &[]);
        let mut modules_first = Vec::new();
        for class in [ClassName::ModuleScript, ClassName::LocalScript] {
            for code in &self.client_scripts {
                if world.get(code.instance).map(|i| i.class()) == Some(class) {
                    modules_first.push(code.clone());
                }
            }
        }
        for code in modules_first {
            scripts.start_script(world, code.instance, &code.path, &code.source);
        }
        let mut missing = 0;
        for code in &self.client_scripts {
            if objects.contains(&code.instance) {
                scripts.know_code(&code.path, &code.source);
            } else if !world.contains(code.instance) {
                missing += 1;
            }
        }
        // What the server placed before this device joined, such as a
        // clone of a template, starts with its file's code.
        let root = world.root();
        scripts.start_arrived(world, &[root]);
        if missing > 0 {
            self.log.push(format!(
                "warning: {missing} of this game's scripts aren't in the server's world"
            ));
        }
        self.log.push(format!(
            "Following the server's world ({} Instances).",
            update.ops.len()
        ));
    }

    /// A script's `RequestStreamAroundAsync` for `player`, once its host
    /// has it: another device's player here, this device's player to the
    /// server, or to the host's drawing for a player on this device.
    fn stream_around(
        &mut self,
        world: &World,
        scripts: &mut ScriptRuntime,
        player: InstanceId,
        position: Vector3,
        request: u64,
    ) {
        if Settings::of(world).is_none() {
            // Nothing streams: it's all here already.
            self.finished_requests.push(request);
            return;
        }
        if let Session::Joined(join) = &mut self.session {
            if scripts.local_player() == Some(player) {
                join.send(&Message::StreamAround {
                    request,
                    position: [position.x, position.y, position.z],
                });
            } else {
                self.finished_requests.push(request);
            }
            return;
        }
        let peer = self
            .players
            .iter()
            .find(|&(_, &p)| p == player)
            .map(|(&peer, _)| peer);
        match (peer, self.is_hosting()) {
            (Some(peer), true) => {
                self.stream_around_for_peer(world, peer, position, request, true);
            }
            _ => self.viewer_requests.push((request, position)),
        }
    }

    fn stream_around_for_peer(
        &mut self,
        world: &World,
        peer: u32,
        position: Vector3,
        request: u64,
        from_script: bool,
    ) {
        if Settings::of(world).is_none() {
            if from_script {
                self.finished_requests.push(request);
            } else if let Session::Hosting(host) = &mut self.session {
                host.send_to(peer, &Message::StreamedAround { request });
            }
            return;
        }
        if from_script {
            self.script_requests.insert((peer, request));
        }
        self.streams
            .request_region(peer, position, request, self.clock);
    }

    /// A remote message arrived. On the host it's from player `from`, so
    /// it's theirs whatever it says, and it must name a RemoteEvent or
    /// RemoteFunction players can see; Instances in it that they can't
    /// arrive as nil. On a player's device it's from the server.
    pub(crate) fn deliver(
        &mut self,
        world: &mut World,
        scripts: &mut ScriptRuntime,
        from: u32,
        message: &WireMessage,
    ) {
        if self.is_hosting() {
            let Some(&player) = self.players.get(&from) else {
                return;
            };
            let remote = InstanceId::from_bits(message.remote);
            let reachable = |id: InstanceId| world.contains(id) && !world.is_server_only(id);
            // An activation names an Ability (Phase 76); the server checks
            // it's theirs, its cost, cooldown and tags.
            let is_remote = world
                .get(remote)
                .is_some_and(|instance| match message.kind {
                    WireKind::Activate => instance.class() == ClassName::Ability,
                    _ => matches!(
                        instance.class(),
                        ClassName::RemoteEvent | ClassName::RemoteFunction
                    ),
                });
            if !is_remote || !reachable(remote) {
                log::info!("dropped a message from player {from} for no remote it can reach");
                return;
            }
            let message = message.to_message(Side::Server, player, &reachable);
            scripts.receive_remote_messages(vec![message]);
        } else if scripts.is_client_only() {
            let player = InstanceId::from_bits(message.player);
            let exists = |id: InstanceId| world.contains(id);
            let message = message.to_message(Side::Client, player, &exists);
            scripts.receive_remote_messages(vec![message]);
        }
    }
}

/// `LevelService`'s levels as players are told them; `None` for a game
/// without levels.
fn level_state(scripts: &ScriptRuntime) -> Option<LevelState> {
    let levels = scripts.levels();
    levels.saved()?;
    Some(LevelState {
        current: levels.current().to_owned(),
        added: levels
            .added_models()
            .iter()
            .map(|(name, model)| (name.clone(), model.to_bits()))
            .collect(),
    })
}

fn follow_levels(scripts: &mut ScriptRuntime, levels: &LevelState, loaded: bool) {
    let added = levels
        .added
        .iter()
        .map(|(name, model)| (name.clone(), InstanceId::from_bits(*model)))
        .collect();
    scripts.follow_levels(&levels.current, added, loaded);
}

/// Where `player`'s character is: its PrimaryPart, else its
/// HumanoidRootPart, else its first part.
pub(crate) fn character_position(world: &World, player: InstanceId) -> Option<Vector3> {
    let character = world.get(player)?.property("Character")?.as_instance()?;
    let model = world.get(character)?;
    let root = model
        .property("PrimaryPart")
        .and_then(Value::as_instance)
        .or_else(|| world.find_first_child(character, "HumanoidRootPart"))
        .or_else(|| {
            world
                .descendants_iter(character)
                .find(|&id| world.get(id).is_some_and(|i| i.class().is_base_part()))
        })?;
    world.get(root)?.property("Position")?.as_vector3()
}

impl Network {
    /// Player `from` was flagged: logged, `Players.PlayerFlagged` fires,
    /// and they're kicked if it's their `kick_flags`th in a minute (MP3).
    fn flag(
        &mut self,
        world: &mut World,
        scripts: &mut ScriptRuntime,
        from: u32,
        reason: &'static str,
    ) {
        let Some(&player) = self.players.get(&from) else {
            return;
        };
        self.flag_count += 1;
        let name = world
            .get(player)
            .map(|p| p.name().to_owned())
            .unwrap_or_default();
        self.log.push(format!(
            "warning: {name} was flagged: their device {reason}"
        ));
        let now = self.clock;
        let times = self.flags.entry(from).or_default();
        times.retain(|&at| now - at < 60.0);
        times.push(now);
        let count = times.len();
        scripts.fire_player_flagged(world, player, reason);
        if self.kick_flags > 0
            && count >= self.kick_flags
            && let Session::Hosting(host) = &mut self.session
        {
            self.flags.remove(&from);
            let why = format!("Your device {reason}.");
            self.log.push(format!(
                "Kicked {name}: flagged {count} times in a minute ({reason})"
            ));
            host.kick(from, &why);
        }
    }

    /// Chat sent on this device and Scripts' system messages, and
    /// `Player:Kick`s (Phase 67). Joined, messages go to the server;
    /// otherwise the server's `OnIncomingMessage` decides on them here.
    fn chat_and_kicks(&mut self, world: &mut World, scripts: &mut ScriptRuntime) {
        let sends = scripts.take_chat_sends();
        let system = scripts.take_system_messages();
        if let Session::Joined(join) = &mut self.session {
            for text in sends {
                join.send(&Message::Chat { text });
            }
        } else {
            let me = scripts.local_player();
            for text in sends {
                if let Some(text) = scripts.filter_chat(world, me, &text) {
                    self.deliver_chat(scripts, me, text);
                }
            }
            for text in system {
                self.deliver_chat(scripts, None, text);
            }
        }
        for (player, reason) in scripts.take_kicks() {
            if Some(player) == scripts.local_player() {
                self.log.push(format!("warning: kicked: {reason}"));
                self.kicked = Some(reason);
                continue;
            }
            let peer = self
                .players
                .iter()
                .find(|&(_, &p)| p == player)
                .map(|(&peer, _)| peer);
            if let (Some(peer), Session::Hosting(host)) = (peer, &mut self.session) {
                let name = world
                    .get(player)
                    .map(|p| p.name().to_owned())
                    .unwrap_or_default();
                self.log.push(format!("Kicked {name}: {reason}"));
                host.kick(peer, &reason);
            }
        }
    }

    /// A message for every device, from `player` (or a system message).
    fn deliver_chat(
        &mut self,
        scripts: &mut ScriptRuntime,
        player: Option<InstanceId>,
        text: String,
    ) {
        if let Session::Hosting(host) = &mut self.session {
            let message = Message::ChatLine {
                player: player.map_or(0, InstanceId::to_bits),
                text: text.clone(),
            };
            for &peer in self.players.keys() {
                host.send_to(peer, &message);
            }
        }
        scripts.receive_chat(player, text);
    }
}

/// The seat this device's character sits in.
fn own_seat(world: &World, scripts: &ScriptRuntime) -> Option<InstanceId> {
    let character = character_of(world, scripts.local_player()?)?;
    world
        .all_below(world.workspace(), ClassName::is_seat)
        .into_iter()
        .find(|&seat| {
            world.get(seat).is_some_and(|s| {
                s.property("Occupant")
                    .and_then(Value::as_instance)
                    .and_then(|controller| world.get(controller))
                    .and_then(|controller| controller.parent())
                    == Some(character)
            })
        })
}

/// `Players.MaxPlayers`, at least 1.
fn max_players(world: &World) -> usize {
    world
        .get(world.players())
        .and_then(|players| players.property("MaxPlayers"))
        .and_then(Value::as_number)
        .map(|max| max.round().max(1.0) as usize)
        .unwrap_or(8)
}

/// The name of the player on this device, as the handshake sends it.
fn local_name(world: &World, scripts: &ScriptRuntime) -> String {
    scripts
        .local_player()
        .and_then(|player| world.get(player))
        .map(|player| player.name().to_owned())
        .unwrap_or_else(|| "Player".to_owned())
}
