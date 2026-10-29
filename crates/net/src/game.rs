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

use axcenity_core::{ClassName, InstanceId, Value, Vector3, World};
use axcenity_script::{LanGame, NetworkRequest, ScriptRuntime, Side};

use crate::discovery::{HostInfo, LanAnswerer, LanSearch};
use crate::handshake::{GameInfo, Message};
use crate::motion::{MOTION_BUDGET, Motion, MotionSender, Smoother};
use crate::ownership::{
    CharacterCheck, OWNED_TAG, Owned, OwnedSender, character_of, character_parts,
};
use crate::replicate::{LevelState, Replicator, Update, apply, replicated_parts};
use crate::session::{Host, HostSettings, Join, SessionEvent};
use crate::transport::{Client, Conditions, LinkStats, Server};
use crate::wire::WireMessage;
use crate::{DEFAULT_PORT, DISCOVERY_PORT};

/// Netcode takes this many connections beyond `MaxPlayers`, so a player
/// who finds the game full is told so by the handshake.
const SPARE_CONNECTIONS: usize = 4;

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
        }
    }

    /// Joined: whether this device's player holds Run this frame, so the
    /// server allows their character `RunSpeed`.
    pub fn set_running(&mut self, running: bool) {
        self.running = running;
    }

    /// Whether this device follows a server it joined.
    pub fn is_following(&self) -> bool {
        matches!(self.session, Session::Joined(_))
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
                    .descendants(world.workspace())
                    .into_iter()
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
        let server = Server::listen(port, max_players + SPARE_CONNECTIONS)
            .map_err(|err| format!("Can't host on port {port}: {err}."))?;
        let port = server.port().unwrap_or(port);
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
        Ok(port)
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

    /// Joins the game at `address` (`"host:port"`), or says why it can't
    /// start. A refusal or failure later fires `ConnectionFailed`.
    pub fn join(
        &mut self,
        world: &mut World,
        scripts: &mut ScriptRuntime,
        address: &str,
        password: Option<String>,
    ) -> Result<(), String> {
        let client =
            Client::connect(address).map_err(|err| format!("Can't join {address}: {err}."))?;
        self.join_through(world, scripts, client, password);
        self.log.push(format!("Joining {address}..."));
        Ok(())
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
        let name = local_name(world, scripts);
        let user_id = scripts.user_id() as u64;
        let join = Join::start(client, &self.game, &name, password, user_id);
        self.session = Session::Joined(Box::new(join));
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
        self.replicator = Replicator::default();
        self.need_snapshot.clear();
        self.motion.clear();
        self.sent_levels = None;
        self.smoother = Smoother::default();
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
        let events = match &mut self.session {
            Session::Offline => Vec::new(),
            Session::Hosting(host) => host.update(elapsed),
            Session::Joined(join) => join.update(elapsed),
        };
        for event in events {
            self.apply(world, scripts, event);
        }
        if scripts.is_client_only() && matches!(self.session, Session::Joined(_)) {
            self.smoother.step(world, self.seconds);
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
        let messages = scripts.take_remote_messages();
        let mine = self.moved_here(world, scripts);
        match &mut self.session {
            Session::Offline => {}
            Session::Hosting(host) => {
                let levels = level_state(scripts);
                let mut update = self.replicator.frame(world);
                update.time = self.clock;
                if levels != self.sent_levels {
                    update.levels.clone_from(&levels);
                    self.sent_levels.clone_from(&levels);
                }
                if !update.is_empty() {
                    let message = Message::Update { update };
                    for &peer in self.players.keys() {
                        if !self.need_snapshot.contains(&peer) {
                            host.send_to(peer, &message);
                        }
                    }
                }
                let parts = replicated_parts(world);
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
                    let mut update = self.replicator.snapshot(world);
                    update.time = self.clock;
                    update.levels.clone_from(&levels);
                    host.send_to(
                        peer,
                        &Message::Snapshot {
                            you: you.to_bits(),
                            update,
                        },
                    );
                    self.motion
                        .insert(peer, MotionSender::new(world, &parts, self.motion_budget));
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
                    let (packets, settled) =
                        sender.frame(world, &parts, focus, &theirs, self.seconds, self.clock);
                    if !settled.is_empty() {
                        let update = Update {
                            ops: settled,
                            time: self.clock,
                            levels: None,
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
                scripts.rename_local_player(world, &you.name);
                self.log
                    .push(format!("Joined \"{}\" as {}.", self.game.name, you.name));
            }
            SessionEvent::Refused(why) => self.log.push(format!("warning: {why}")),
            SessionEvent::Snapshot { you, update } => {
                if !self.is_hosting() {
                    self.follow_server(world, scripts, InstanceId::from_bits(you), &update);
                }
            }
            SessionEvent::Update(update) => {
                if !self.is_hosting() && scripts.is_client_only() {
                    let applied = apply(world, &update);
                    for id in applied.placed {
                        self.smoother.settle(id, update.time);
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
                        self.checks.entry(from).or_default().receive(
                            world,
                            player,
                            &assemblies,
                            &owned,
                            self.clock,
                        );
                    }
                } else if scripts.is_client_only()
                    && let Some(motion) = Motion::from_bytes(&bytes)
                {
                    self.smoother.receive(world, &motion);
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
                    self.log
                        .push("The server moved your character back.".to_owned());
                }
            }
            SessionEvent::Failed(reason) => {
                self.session = Session::Offline;
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
        apply(&mut replica, update);
        self.smoother = Smoother::default();
        if let Some(levels) = &update.levels {
            follow_levels(scripts, levels, false);
        }
        let terrains: Vec<InstanceId> = replica
            .descendants(replica.root())
            .into_iter()
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
        scripts.become_client(world, replica, you);
        let mut modules_first = Vec::new();
        for class in [ClassName::ModuleScript, ClassName::LocalScript] {
            for code in &self.client_scripts {
                if world.get(code.instance).map(|i| i.class()) == Some(class) {
                    modules_first.push(code.clone());
                }
            }
        }
        let started = modules_first.len();
        for code in modules_first {
            scripts.start_script(world, code.instance, &code.path, &code.source);
        }
        let missing = self.client_scripts.len() - started;
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
            let is_remote = world.get(remote).is_some_and(|instance| {
                matches!(
                    instance.class(),
                    ClassName::RemoteEvent | ClassName::RemoteFunction
                )
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
                .descendants(character)
                .into_iter()
                .find(|&id| world.get(id).is_some_and(|i| i.class().is_base_part()))
        })?;
    world.get(root)?.property("Position")?.as_vector3()
}

/// The seat this device's character sits in.
fn own_seat(world: &World, scripts: &ScriptRuntime) -> Option<InstanceId> {
    let character = character_of(world, scripts.local_player()?)?;
    world
        .descendants(world.workspace())
        .into_iter()
        .find(|&seat| {
            world.get(seat).is_some_and(|s| {
                matches!(s.class(), ClassName::Seat | ClassName::VehicleSeat)
                    && s.property("Occupant")
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
