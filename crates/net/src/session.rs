//! Hosting and joining over a [`Server`] or [`Client`]: the handshake
//! (`handshake`), then the roster, so every end knows who is in the game,
//! then the game's own messages (Phase 64), which `Network` reads and
//! writes.

use std::collections::BTreeMap;
use std::time::Duration;

use crate::PROTOCOL_VERSION;
use crate::handshake::{Admission, GameInfo, Message, Peer, admit, unique_name};
use crate::limits::{Kind, Limits};
use crate::replicate::Update;
use crate::transport::{Channel, Client, ConnectionId, LinkStats, Server, ServerEvent};
use crate::wire::WireMessage;

/// How long a connection may take to say who it is, and a joining player
/// to hear back, before giving up.
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
/// A refused connection stays open this long, so the reason arrives
/// before it closes.
const REFUSAL_GRACE: Duration = Duration::from_secs(1);
/// A listen server's own player.
const HOST_PLAYER: u32 = 1;
/// A player is flagged at most this often for each reason, however much
/// their device sends.
const FLAG_EVERY: Duration = Duration::from_secs(1);
/// Why a player is flagged for a message the server can't read.
pub const UNREADABLE: &str = "sent a message the server can't read";
/// Why a player is flagged for a message only the server sends.
pub const NOT_A_PLAYERS: &str = "sent a message only the server sends";

/// Something a session tells its game.
#[derive(Clone, Debug, PartialEq)]
pub enum SessionEvent {
    /// Someone else is in the game: they go in `Players`.
    Joined(Peer),
    /// They left.
    Left(u32),
    /// The host took this player in, with this name.
    Welcomed(Peer),
    /// The join failed or was refused, with why.
    Failed(String),
    /// Player `from`'s device sent something breaking a rule (over a
    /// budget, unreadable, or only the server's to send), so it was
    /// dropped: at most once a second for each reason (MP3).
    Flagged { from: u32, reason: &'static str },
    /// The connection to the host was lost after joining (it crashed,
    /// stopped or the network failed), with why: the device may rejoin
    /// (multiplayer readiness MP2).
    Lost(String),
    /// The host turned a player away (logged on the host).
    Refused(String),
    /// The server's world, for this player, whose Player is `you`.
    Snapshot { you: u64, update: Update },
    /// What changed in the server's world.
    Update(Update),
    /// A RemoteEvent or RemoteFunction message from player `from` (on the
    /// host), or from the server (`from` 0, on a player's device).
    Remote { from: u32, message: WireMessage },
    /// What came on the unreliable channel from player `from` (or the
    /// server, 0): motion, newest first wins.
    Datagram { from: u32, bytes: Vec<u8> },
    /// The parts this device now moves besides its character.
    Ownership(Vec<u64>),
    /// Where the server has this device's character.
    Correction(Update),
    /// Player `from` sent a chat message (on the host).
    Chat { from: u32, text: String },
    /// A chat message from the server for everyone: from a Player (a
    /// network ID), or 0 for a system message.
    ChatLine { player: u64, text: String },
    /// Player `from` typed a console command for the server (on the host).
    Console { from: u32, line: String },
    /// Player `from` asked for what's around `position` (on the host).
    StreamAround {
        from: u32,
        request: u64,
        position: [f32; 3],
    },
    /// The server sent what's around a place this device asked for.
    StreamedAround(u64),
}

/// What a host is.
pub struct HostSettings {
    pub game: GameInfo,
    /// Players at most, the host's own among them.
    pub max_players: usize,
    pub password: Option<String>,
    /// A listen server's own player's name; `None` for a dedicated server.
    pub own_player: Option<String>,
}

/// A game hosted on this device.
pub struct Host {
    server: Server,
    settings: HostSettings,
    /// The host's own player, for a listen server.
    own: Option<Peer>,
    /// Connections that haven't said who they are yet, since when.
    waiting: BTreeMap<ConnectionId, (u64, Duration)>,
    /// Connections so far, so they're greeted in the order they came.
    arrivals: u64,
    /// Refused connections, since when, until they close.
    refused: BTreeMap<ConnectionId, Duration>,
    players: BTreeMap<ConnectionId, Peer>,
    /// Each player's rejoin key (see [`Message::Welcome`]).
    rejoin_keys: BTreeMap<ConnectionId, u64>,
    /// What each player may still send (MP3).
    limits: BTreeMap<ConnectionId, Limits>,
    /// When each player was last flagged for each reason.
    flagged: BTreeMap<(ConnectionId, &'static str), Duration>,
    clock: Duration,
    next_id: u32,
}

/// A secret no one can guess: a player's rejoin key. Never 0, which
/// means "no key".
fn fresh_key() -> u64 {
    use ring::rand::SecureRandom;
    let mut bytes = [0; 8];
    loop {
        if ring::rand::SystemRandom::new().fill(&mut bytes).is_err() {
            // No system randomness: no key, so this player can't rejoin
            // in place of a lost connection (they still join as new).
            return 0;
        }
        let key = u64::from_le_bytes(bytes);
        if key != 0 {
            return key;
        }
    }
}

impl Host {
    pub fn new(server: Server, settings: HostSettings) -> Host {
        let own = settings.own_player.as_ref().map(|name| Peer {
            id: HOST_PLAYER,
            name: name.clone(),
            display_name: name.clone(),
            user_id: 0,
        });
        Host {
            server,
            settings,
            own,
            waiting: BTreeMap::new(),
            arrivals: 0,
            refused: BTreeMap::new(),
            players: BTreeMap::new(),
            rejoin_keys: BTreeMap::new(),
            limits: BTreeMap::new(),
            flagged: BTreeMap::new(),
            clock: Duration::ZERO,
            next_id: HOST_PLAYER + 1,
        }
    }

    /// The UDP port it listens on; `None` in memory.
    pub fn port(&self) -> Option<u16> {
        self.server.port()
    }

    pub fn server(&self) -> &Server {
        &self.server
    }

    pub fn server_mut(&mut self) -> &mut Server {
        &mut self.server
    }

    pub fn settings(&self) -> &HostSettings {
        &self.settings
    }

    /// Everyone in the game, the host's own player first.
    pub fn players(&self) -> Vec<Peer> {
        self.own
            .iter()
            .chain(self.players.values())
            .cloned()
            .collect()
    }

    /// Receives: takes in connections, join requests and departures.
    pub fn update(&mut self, elapsed: Duration) -> Vec<SessionEvent> {
        self.clock += elapsed;
        let mut events = Vec::new();
        for event in self.server.update(elapsed) {
            match event {
                ServerEvent::Connected(id) => {
                    self.arrivals += 1;
                    self.waiting.insert(id, (self.arrivals, self.clock));
                }
                ServerEvent::Disconnected { id, reason } => {
                    self.waiting.remove(&id);
                    self.refused.remove(&id);
                    self.rejoin_keys.remove(&id);
                    self.limits.remove(&id);
                    self.flagged.retain(|&(connection, _), _| connection != id);
                    if let Some(peer) = self.players.remove(&id) {
                        log::info!("{} left: {reason}", peer.name);
                        self.broadcast(&Message::Left { id: peer.id });
                        events.push(SessionEvent::Left(peer.id));
                    }
                }
            }
        }
        let mut waiting: Vec<(u64, ConnectionId, Duration)> = self
            .waiting
            .iter()
            .map(|(&id, &(arrival, since))| (arrival, id, since))
            .collect();
        waiting.sort();
        for (_, id, since) in waiting {
            match self.server.receive(id, Channel::ReliableOrdered) {
                Some(bytes) => {
                    self.waiting.remove(&id);
                    let hello = Message::from_bytes(&bytes);
                    events.extend(self.greet(id, hello));
                }
                None if self.clock - since > HANDSHAKE_TIMEOUT => {
                    self.waiting.remove(&id);
                    self.server.disconnect(id);
                }
                None => {}
            }
        }
        // Then the players' game messages, each within its budget (MP3).
        let clock = self.clock;
        let mut flags: Vec<(ConnectionId, u32, &'static str)> = Vec::new();
        for (&id, peer) in &self.players {
            let limits = self.limits.entry(id).or_insert_with(|| Limits::new(clock));
            let mut flag = |reason: &'static str| flags.push((id, peer.id, reason));
            while let Some(bytes) = self.server.receive(id, Channel::ReliableOrdered) {
                if !limits.take(Kind::Bytes, bytes.len() as f64, clock) {
                    flag(Kind::Bytes.reason());
                    continue;
                }
                let (kind, event) = match Message::from_bytes(&bytes) {
                    Some(Message::Remote { message }) => (
                        Kind::Remote,
                        SessionEvent::Remote {
                            from: peer.id,
                            message,
                        },
                    ),
                    Some(Message::Chat { text }) => (
                        Kind::Chat,
                        SessionEvent::Chat {
                            from: peer.id,
                            text,
                        },
                    ),
                    Some(Message::Console { line }) => (
                        Kind::Console,
                        SessionEvent::Console {
                            from: peer.id,
                            line,
                        },
                    ),
                    Some(Message::StreamAround { request, position }) => (
                        Kind::Stream,
                        SessionEvent::StreamAround {
                            from: peer.id,
                            request,
                            position,
                        },
                    ),
                    // Tables nested too deep, or bytes that aren't a message.
                    None => {
                        flag(UNREADABLE);
                        continue;
                    }
                    Some(_) => {
                        flag(NOT_A_PLAYERS);
                        continue;
                    }
                };
                if limits.take(kind, 1.0, clock) {
                    events.push(event);
                } else {
                    flag(kind.reason());
                }
            }
            while let Some(bytes) = self.server.receive(id, Channel::Unreliable) {
                if !limits.take(Kind::Bytes, bytes.len() as f64, clock) {
                    flag(Kind::Bytes.reason());
                } else if !limits.take(Kind::Datagram, 1.0, clock) {
                    flag(Kind::Datagram.reason());
                } else {
                    events.push(SessionEvent::Datagram {
                        from: peer.id,
                        bytes,
                    });
                }
            }
        }
        for (id, from, reason) in flags {
            let last = self.flagged.entry((id, reason)).or_insert(Duration::MAX);
            if *last == Duration::MAX || clock.saturating_sub(*last) >= FLAG_EVERY {
                *last = clock;
                events.push(SessionEvent::Flagged { from, reason });
            }
        }
        let clock = self.clock;
        let server = &mut self.server;
        self.refused.retain(|&id, &mut since| {
            let keep = clock - since < REFUSAL_GRACE;
            if !keep {
                server.disconnect(id);
            }
            keep
        });
        events
    }

    /// Answers a connection's first message: a Welcome, or a refusal.
    fn greet(&mut self, id: ConnectionId, hello: Option<Message>) -> Vec<SessionEvent> {
        let from = self
            .server
            .address_of(id)
            .map(|address| format!(" from {address}"))
            .unwrap_or_default();
        // A player back after losing their connection, which the host
        // hasn't noticed is gone yet (netcode waits 15 seconds): their old
        // connection doesn't count, and goes once they're let in.
        let stale = match &hello {
            Some(Message::Hello { rejoin, .. }) if *rejoin != 0 => self
                .rejoin_keys
                .iter()
                .find(|&(&connection, &key)| key == *rejoin && connection != id)
                .map(|(&connection, _)| connection),
            _ => None,
        };
        let admission = Admission {
            game: &self.settings.game,
            password: self.settings.password.as_deref(),
            players: self.own.iter().count() + self.players.len() - usize::from(stale.is_some()),
            max_players: self.settings.max_players,
        };
        if let Err(refusal) = admit(hello.as_ref(), &admission) {
            self.send(
                id,
                &Message::Refused {
                    reason: refusal.to_player,
                },
            );
            self.refused.insert(id, self.clock);
            return vec![SessionEvent::Refused(format!("{}{from}", refusal.for_host))];
        }
        let Some(Message::Hello { name, user_id, .. }) = hello else {
            unreachable!("admit only takes a Hello")
        };
        let mut events = Vec::new();
        if let Some(old) = stale {
            self.rejoin_keys.remove(&old);
            self.limits.remove(&old);
            if let Some(peer) = self.players.remove(&old) {
                log::info!("{} is back: their lost connection is dropped", peer.name);
                self.broadcast(&Message::Left { id: peer.id });
                events.push(SessionEvent::Left(peer.id));
            }
            self.server.disconnect(old);
        }
        let everyone = self.players();
        let display_name = match name.trim() {
            "" => "Player".to_owned(),
            name => name.to_owned(),
        };
        let name = unique_name(&name, everyone.iter().map(|peer| peer.name.as_str()));
        let peer = Peer {
            id: self.next_id,
            name,
            display_name,
            user_id,
        };
        self.next_id += 1;
        let rejoin = fresh_key();
        self.rejoin_keys.insert(id, rejoin);
        self.send(
            id,
            &Message::Welcome {
                you: peer.clone(),
                players: everyone,
                rejoin,
            },
        );
        self.broadcast(&Message::Joined {
            player: peer.clone(),
        });
        log::info!("{} joined{from}", peer.name);
        self.players.insert(id, peer.clone());
        self.limits.insert(id, Limits::new(self.clock));
        events.push(SessionEvent::Joined(peer));
        events
    }

    fn send(&mut self, id: ConnectionId, message: &Message) {
        self.server
            .send(id, Channel::ReliableOrdered, message.to_bytes());
    }

    /// Sends `message` to every player in the game.
    fn broadcast(&mut self, message: &Message) {
        let ids: Vec<ConnectionId> = self.players.keys().copied().collect();
        for id in ids {
            self.send(id, message);
        }
    }

    /// Sends `message` to player `peer` (by their id), if they're in.
    pub fn send_to(&mut self, peer: u32, message: &Message) {
        if let Some(connection) = self.connection_of(peer) {
            self.send(connection, message);
        }
    }

    /// Sends `bytes` to player `peer` on the unreliable channel.
    pub fn send_datagram(&mut self, peer: u32, bytes: Vec<u8>) {
        if let Some(connection) = self.connection_of(peer) {
            self.server.send(connection, Channel::Unreliable, bytes);
        }
    }

    fn connection_of(&self, peer: u32) -> Option<ConnectionId> {
        self.players
            .iter()
            .find(|(_, p)| p.id == peer)
            .map(|(&connection, _)| connection)
    }

    /// How player `peer`'s connection is doing.
    pub fn stats_of(&self, peer: u32) -> Option<LinkStats> {
        self.server.stats(self.connection_of(peer)?)
    }

    /// Every byte sent to player `peer` so far, on the loopback.
    pub fn loopback_bytes_sent_to(&self, peer: u32) -> Option<u64> {
        self.server.loopback_bytes_sent(self.connection_of(peer)?)
    }

    /// Takes player `peer` out of the game, as `Player:Kick` does: they're
    /// told `reason`, and the connection closes a moment later.
    pub fn kick(&mut self, peer: u32, reason: &str) {
        if let Some(connection) = self.connection_of(peer) {
            self.send(
                connection,
                &Message::Refused {
                    reason: reason.to_owned(),
                },
            );
            self.refused.insert(connection, self.clock);
        }
    }

    /// Sends what's queued.
    pub fn flush(&mut self) {
        self.server.flush();
    }

    /// Stops hosting, telling every player.
    pub fn shut_down(&mut self) {
        self.server.shut_down();
        self.players.clear();
    }
}

enum JoinState {
    /// Waiting for the connection, then the host's answer.
    Asking,
    /// In the game as `you`.
    In {
        you: Peer,
    },
    Over,
}

/// Joining a game hosted elsewhere.
pub struct Join {
    client: Client,
    hello: Message,
    /// When the Hello went, once it has.
    asked: Option<Duration>,
    state: JoinState,
    clock: Duration,
    /// The key the host gave this player to rejoin with.
    rejoin_key: u64,
}

impl Join {
    /// Joins through `client` as `name` (the host may number it), with this
    /// install's `user_id`.
    pub fn start(
        client: Client,
        game: &GameInfo,
        name: &str,
        password: Option<String>,
        user_id: u64,
    ) -> Join {
        Join {
            client,
            hello: Message::Hello {
                protocol: PROTOCOL_VERSION,
                game: game.name.clone(),
                version: game.version.clone(),
                name: name.to_owned(),
                password,
                user_id,
                defaults: crate::handshake::class_defaults(),
                rejoin: 0,
            },
            asked: None,
            state: JoinState::Asking,
            clock: Duration::ZERO,
            rejoin_key: 0,
        }
    }

    /// Joins again after losing the connection, with the key the host gave
    /// last time, so the host drops the old connection (MP2).
    pub fn rejoining(mut self, key: u64) -> Join {
        if let Message::Hello { rejoin, .. } = &mut self.hello {
            *rejoin = key;
        }
        self
    }

    /// The key to rejoin with, once the host took this player in; 0 before.
    pub fn rejoin_key(&self) -> u64 {
        self.rejoin_key
    }

    /// Whether the connection is encrypted with a connect token (MP4).
    pub fn is_secured(&self) -> bool {
        self.client.is_secured()
    }

    /// This player, once the host took them in.
    pub fn you(&self) -> Option<&Peer> {
        match &self.state {
            JoinState::In { you } => Some(you),
            _ => None,
        }
    }

    pub fn is_over(&self) -> bool {
        matches!(self.state, JoinState::Over)
    }

    /// Receives: the connection, the host's answer, and the roster.
    pub fn update(&mut self, elapsed: Duration) -> Vec<SessionEvent> {
        self.clock += elapsed;
        self.client.update(elapsed);
        if self.is_over() {
            return Vec::new();
        }
        if let Some(reason) = self.client.ended() {
            let reason = reason.to_owned();
            let was_in = matches!(self.state, JoinState::In { .. });
            self.state = JoinState::Over;
            return vec![if was_in {
                SessionEvent::Lost(reason)
            } else {
                SessionEvent::Failed(reason)
            }];
        }
        if self.client.is_connected() && self.asked.is_none() {
            self.client
                .send(Channel::ReliableOrdered, self.hello.to_bytes());
            self.asked = Some(self.clock);
        }
        let mut events = Vec::new();
        while let Some(bytes) = self.client.receive(Channel::ReliableOrdered) {
            match Message::from_bytes(&bytes) {
                Some(Message::Welcome {
                    you,
                    players,
                    rejoin,
                }) => {
                    self.rejoin_key = rejoin;
                    events.push(SessionEvent::Welcomed(you.clone()));
                    events.extend(players.into_iter().map(SessionEvent::Joined));
                    self.state = JoinState::In { you };
                }
                Some(Message::Refused { reason }) => {
                    self.client.disconnect();
                    self.state = JoinState::Over;
                    events.push(SessionEvent::Failed(reason));
                    return events;
                }
                Some(Message::Joined { player }) => events.push(SessionEvent::Joined(player)),
                Some(Message::Left { id }) => events.push(SessionEvent::Left(id)),
                Some(Message::Snapshot { you, update }) => {
                    events.push(SessionEvent::Snapshot { you, update });
                }
                Some(Message::Update { update }) => events.push(SessionEvent::Update(update)),
                Some(Message::Remote { message }) => {
                    events.push(SessionEvent::Remote { from: 0, message });
                }
                Some(Message::Ownership { parts }) => events.push(SessionEvent::Ownership(parts)),
                Some(Message::Correction { update }) => {
                    events.push(SessionEvent::Correction(update));
                }
                Some(Message::ChatLine { player, text }) => {
                    events.push(SessionEvent::ChatLine { player, text });
                }
                Some(Message::StreamedAround { request }) => {
                    events.push(SessionEvent::StreamedAround(request));
                }
                Some(
                    Message::Hello { .. }
                    | Message::Chat { .. }
                    | Message::Console { .. }
                    | Message::StreamAround { .. },
                )
                | None => {}
            }
        }
        if matches!(self.state, JoinState::In { .. }) {
            while let Some(bytes) = self.client.receive(Channel::Unreliable) {
                events.push(SessionEvent::Datagram { from: 0, bytes });
            }
        } else {
            while self.client.receive(Channel::Unreliable).is_some() {}
        }
        let answered = matches!(self.state, JoinState::In { .. });
        if !answered
            && self
                .asked
                .is_some_and(|asked| self.clock - asked > HANDSHAKE_TIMEOUT)
        {
            self.client.disconnect();
            self.state = JoinState::Over;
            events.push(SessionEvent::Failed(
                "The server didn't answer the join request.".to_owned(),
            ));
        }
        events
    }

    /// Sends `message` to the host, once taken in.
    pub fn send(&mut self, message: &Message) {
        if self.you().is_some() {
            self.client
                .send(Channel::ReliableOrdered, message.to_bytes());
        }
    }

    /// Sends `bytes` as they are, once taken in: for tests of what a
    /// modified client might send.
    #[cfg(test)]
    pub(crate) fn send_bytes(&mut self, channel: Channel, bytes: Vec<u8>) {
        if self.you().is_some() {
            self.client.send(channel, bytes);
        }
    }

    /// Sends `bytes` to the host on the unreliable channel, once taken in.
    pub fn send_datagram(&mut self, bytes: Vec<u8>) {
        if self.you().is_some() {
            self.client.send(Channel::Unreliable, bytes);
        }
    }

    /// How the connection is doing.
    pub fn stats(&self) -> LinkStats {
        self.client.stats()
    }

    /// Sends what's queued.
    pub fn flush(&mut self) {
        self.client.flush();
    }

    /// Leaves the game, telling the host.
    pub fn leave(&mut self) {
        self.client.disconnect();
        self.state = JoinState::Over;
    }
}
