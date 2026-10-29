//! Hosting and joining over a [`Server`] or [`Client`]: the handshake
//! (`handshake`), then the roster, so every end knows who is in the game,
//! then the game's own messages (Phase 64), which `Network` reads and
//! writes.

use std::collections::BTreeMap;
use std::time::Duration;

use crate::PROTOCOL_VERSION;
use crate::handshake::{Admission, GameInfo, Message, Peer, admit, unique_name};
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

/// Something a session tells its game.
#[derive(Clone, Debug, PartialEq)]
pub enum SessionEvent {
    /// Someone else is in the game: they go in `Players`.
    Joined(Peer),
    /// They left.
    Left(u32),
    /// The host took this player in, with this name.
    Welcomed(Peer),
    /// The join failed, was refused or the connection was lost, with why.
    Failed(String),
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
    clock: Duration,
    next_id: u32,
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
            clock: Duration::ZERO,
            next_id: HOST_PLAYER + 1,
        }
    }

    /// The UDP port it listens on; `None` in memory.
    pub fn port(&self) -> Option<u16> {
        self.server.port()
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
        // Then the players' game messages.
        for (&id, peer) in &self.players {
            while let Some(bytes) = self.server.receive(id, Channel::ReliableOrdered) {
                match Message::from_bytes(&bytes) {
                    Some(Message::Remote { message }) => events.push(SessionEvent::Remote {
                        from: peer.id,
                        message,
                    }),
                    Some(Message::Chat { text }) => events.push(SessionEvent::Chat {
                        from: peer.id,
                        text,
                    }),
                    _ => {}
                }
            }
            while let Some(bytes) = self.server.receive(id, Channel::Unreliable) {
                events.push(SessionEvent::Datagram {
                    from: peer.id,
                    bytes,
                });
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
        let admission = Admission {
            game: &self.settings.game,
            password: self.settings.password.as_deref(),
            players: self.own.iter().count() + self.players.len(),
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
        self.send(
            id,
            &Message::Welcome {
                you: peer.clone(),
                players: everyone,
            },
        );
        self.broadcast(&Message::Joined {
            player: peer.clone(),
        });
        log::info!("{} joined{from}", peer.name);
        self.players.insert(id, peer.clone());
        vec![SessionEvent::Joined(peer)]
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
            },
            asked: None,
            state: JoinState::Asking,
            clock: Duration::ZERO,
        }
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
            self.state = JoinState::Over;
            return vec![SessionEvent::Failed(reason)];
        }
        if self.client.is_connected() && self.asked.is_none() {
            self.client
                .send(Channel::ReliableOrdered, self.hello.to_bytes());
            self.asked = Some(self.clock);
        }
        let mut events = Vec::new();
        while let Some(bytes) = self.client.receive(Channel::ReliableOrdered) {
            match Message::from_bytes(&bytes) {
                Some(Message::Welcome { you, players }) => {
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
                Some(Message::Hello { .. } | Message::Chat { .. }) | None => {}
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
