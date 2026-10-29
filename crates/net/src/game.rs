//! [`Network`]: a game's hosting and joining, between the session and the
//! game's scripts and `Players`. The player and `axcenity-server` both run
//! it, in the frame's network receive and network send steps.

use std::collections::BTreeMap;
use std::time::Duration;

use axcenity_core::{InstanceId, Value, World};
use axcenity_script::{LanGame, NetworkRequest, ScriptRuntime};

use crate::discovery::{HostInfo, LanAnswerer, LanSearch};
use crate::handshake::GameInfo;
use crate::session::{Host, HostSettings, Join, SessionEvent};
use crate::transport::{Client, Server};
use crate::{DEFAULT_PORT, DISCOVERY_PORT};

/// Netcode takes this many connections beyond `MaxPlayers`, so a player
/// who finds the game full is told so by the handshake.
const SPARE_CONNECTIONS: usize = 4;

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
        }
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
        let join = Join::start(client, &self.game, &name, password);
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
        self.remove_players(world, scripts);
    }

    fn remove_players(&mut self, world: &mut World, scripts: &mut ScriptRuntime) {
        for (_, player) in std::mem::take(&mut self.players) {
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
    pub fn send(&mut self) {
        match &mut self.session {
            Session::Offline => {}
            Session::Hosting(host) => host.flush(),
            Session::Joined(join) => join.flush(),
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
                if self.is_hosting() {
                    self.log.push(format!("{} joined.", peer.name));
                }
                let player = scripts.add_remote_player(world, &peer.name);
                self.players.insert(peer.id, player);
            }
            SessionEvent::Left(id) => {
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
            SessionEvent::Failed(reason) => {
                self.session = Session::Offline;
                self.remove_players(world, scripts);
                self.log.push(format!("warning: {reason}"));
                scripts.fire_connection_failed(world, &reason);
            }
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
