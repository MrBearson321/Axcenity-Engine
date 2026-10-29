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

use axcenity_core::{ClassName, InstanceId, Value, World};
use axcenity_script::{LanGame, NetworkRequest, ScriptRuntime, Side};

use crate::discovery::{HostInfo, LanAnswerer, LanSearch};
use crate::handshake::{GameInfo, Message};
use crate::replicate::{Replicator, Update, apply};
use crate::session::{Host, HostSettings, Join, SessionEvent};
use crate::transport::{Client, Server};
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
        }
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
        let events = match &mut self.session {
            Session::Offline => Vec::new(),
            Session::Hosting(host) => host.update(elapsed),
            Session::Joined(join) => join.update(elapsed),
        };
        for event in events {
            self.apply(world, scripts, event);
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
        match &mut self.session {
            Session::Offline => {}
            Session::Hosting(host) => {
                let update = self.replicator.frame(world);
                if !update.is_empty() {
                    let message = Message::Update { update };
                    for &peer in self.players.keys() {
                        if !self.need_snapshot.contains(&peer) {
                            host.send_to(peer, &message);
                        }
                    }
                }
                for peer in std::mem::take(&mut self.need_snapshot) {
                    let Some(&you) = self.players.get(&peer) else {
                        continue;
                    };
                    let update = self.replicator.snapshot(world);
                    host.send_to(
                        peer,
                        &Message::Snapshot {
                            you: you.to_bits(),
                            update,
                        },
                    );
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
                    for player in applied.players_leaving {
                        scripts.remove_player(world, player);
                    }
                    for player in applied.players_added {
                        scripts.add_replicated_player(world, player);
                    }
                }
            }
            SessionEvent::Remote { from, message } => self.deliver(world, scripts, from, &message),
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
