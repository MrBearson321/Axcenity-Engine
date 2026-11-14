//! The transport: renet's two channels (reliable-ordered and unreliable)
//! over UDP, through netcode's connection handshake and timeouts, or in
//! memory (the loopback) with the same [`Server`] and [`Client`] interface.
//!
//! Each frame a host calls `update` (receive), reads and sends messages,
//! then `flush` (send). Times are real seconds since the last update.
//!
//! The loopback can pretend to be a worse network ([`Conditions`]: delay,
//! jitter and loss), for the editor's Play settings and for tests.
//!
//! A server on the network is secured unless it's for a LAN (MP4,
//! `secure.rs`): its token port hands out connect tokens and netcode
//! encrypts every packet. A [`Client::join`] gets a token first, on a
//! thread, and connects unsecured only when no token port answers and it
//! has never seen that server secured.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::io;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs, UdpSocket};
use std::rc::Rc;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use renet::{
    ChannelConfig, ClientId, ConnectionConfig, DisconnectReason, RenetClient, RenetServer, SendType,
};
use renet_netcode::{
    ClientAuthentication, ConnectToken, NetcodeClientTransport, NetcodeDisconnectReason,
    NetcodeServerTransport, ServerAuthentication, ServerConfig,
};

use crate::secure::{FetchError, Secured, TokenPort, TokenRequest, TokenRules, Trust, fetch_token};

/// Netcode's id for every Axcenity game. It never changes: games and
/// engine versions are told apart by the join handshake, which can say
/// why it refuses (netcode would drop the packets without a word).
pub(crate) const NETCODE_PROTOCOL_ID: u64 = u64::from_be_bytes(*b"AXCENITY");

/// How many bytes each channel may hold waiting to be sent or read.
const CHANNEL_MEMORY: usize = 5 * 1024 * 1024;

/// A channel messages go on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Channel {
    /// Arrives once and in order, sent again until it does.
    ReliableOrdered,
    /// May be lost or come out of order; the newest state of something.
    Unreliable,
}

impl From<Channel> for u8 {
    fn from(channel: Channel) -> u8 {
        match channel {
            Channel::ReliableOrdered => 0,
            Channel::Unreliable => 1,
        }
    }
}

fn connection_config() -> ConnectionConfig {
    let channels = vec![
        ChannelConfig {
            channel_id: Channel::ReliableOrdered.into(),
            max_memory_usage_bytes: CHANNEL_MEMORY,
            send_type: SendType::ReliableOrdered {
                resend_time: Duration::from_millis(300),
            },
        },
        ChannelConfig {
            channel_id: Channel::Unreliable.into(),
            max_memory_usage_bytes: CHANNEL_MEMORY,
            send_type: SendType::Unreliable,
        },
    ];
    ConnectionConfig {
        available_bytes_per_tick: 60_000,
        server_channels_config: channels.clone(),
        client_channels_config: channels,
    }
}

/// Netcode's clock: both ends use the time since 1970, so a connection
/// request's expiry means the same on each.
fn now() -> Duration {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
}

/// A network as bad as this, emulated on the loopback (Phase 67's Play
/// settings): each packet arrives `latency` late, give or take up to
/// `jitter`, and `loss` of them (0 to 1) never arrive. Both directions.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Conditions {
    pub latency: Duration,
    pub jitter: Duration,
    pub loss: f64,
}

/// How a connection is doing, as renet measures it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LinkStats {
    /// The round trip, in seconds.
    pub rtt: f64,
    /// The share of packets lost, 0 to 1.
    pub packet_loss: f64,
    pub bytes_sent_per_second: f64,
    pub bytes_received_per_second: f64,
}

impl From<renet::NetworkInfo> for LinkStats {
    fn from(info: renet::NetworkInfo) -> LinkStats {
        LinkStats {
            rtt: info.rtt,
            packet_loss: info.packet_loss,
            bytes_sent_per_second: info.bytes_sent_per_second,
            bytes_received_per_second: info.bytes_received_per_second,
        }
    }
}

/// A packet on its way, due when the receiving end's clock reaches `due`.
struct InFlight {
    due: Duration,
    bytes: Vec<u8>,
}

/// One direction of a loopback: the packets on their way, and the
/// receiving end's clock.
#[derive(Default)]
struct Lane {
    packets: RefCell<VecDeque<InFlight>>,
    clock: Cell<Duration>,
    /// Every byte put on it (lost ones too), for measuring bandwidth.
    bytes: Cell<u64>,
}

impl Lane {
    /// Puts `packets` on their way, late or lost as `pipe` says.
    fn send(&self, pipe: &Pipe, packets: Vec<Vec<u8>>) {
        let conditions = pipe.conditions.get();
        let mut queue = self.packets.borrow_mut();
        for bytes in packets {
            self.bytes.set(self.bytes.get() + bytes.len() as u64);
            if conditions.loss > 0.0 && pipe.random() < conditions.loss {
                continue;
            }
            let jitter = conditions.jitter.as_secs_f64() * (pipe.random() * 2.0 - 1.0);
            let delay = (conditions.latency.as_secs_f64() + jitter).max(0.0);
            queue.push_back(InFlight {
                due: self.clock.get() + Duration::from_secs_f64(delay),
                bytes,
            });
        }
    }

    /// Advances the receiving end's clock by `elapsed` and takes the
    /// packets due by then, in the order they arrive.
    fn arrive(&self, elapsed: Duration) -> Vec<Vec<u8>> {
        let now = self.clock.get() + elapsed;
        self.clock.set(now);
        let mut queue = self.packets.borrow_mut();
        let (due, later): (VecDeque<InFlight>, VecDeque<InFlight>) =
            queue.drain(..).partition(|packet| packet.due <= now);
        *queue = later;
        let mut due: Vec<InFlight> = due.into();
        due.sort_by_key(|packet| packet.due);
        due.into_iter().map(|packet| packet.bytes).collect()
    }
}

/// The two lanes between a loopback client and its server.
#[derive(Default)]
struct Pipe {
    to_server: Lane,
    to_client: Lane,
    closed: Cell<Option<Closed>>,
    conditions: Cell<Conditions>,
    /// xorshift's state, for loss and jitter.
    seed: Cell<u64>,
}

impl Pipe {
    fn new(conditions: Conditions, seed: u64) -> Pipe {
        Pipe {
            conditions: Cell::new(conditions),
            seed: Cell::new(seed | 1),
            ..Pipe::default()
        }
    }

    /// A number from 0 up to 1.
    fn random(&self) -> f64 {
        let mut x = self.seed.get();
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.seed.set(x);
        (x >> 11) as f64 / (1u64 << 53) as f64
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Closed {
    ByClient,
    ByServer,
}

/// A connection's id on its server.
pub type ConnectionId = ClientId;

/// Something that happened to a server's connections.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ServerEvent {
    Connected(ConnectionId),
    Disconnected { id: ConnectionId, reason: String },
}

/// Loopback connections' ids count down from here; UDP clients choose
/// theirs at random.
const FIRST_LOOPBACK: ClientId = u64::MAX;

/// The listening end: connections arrive over UDP (unless it's in memory
/// only) and from loopback clients.
pub struct Server {
    renet: RenetServer,
    udp: Option<NetcodeServerTransport>,
    /// A secured server's token port and certificate fingerprint.
    tokens: Option<(TokenPort, String)>,
    loopbacks: Vec<(ClientId, Rc<Pipe>)>,
    next_loopback: ClientId,
    /// What the loopback emulates.
    conditions: Conditions,
}

impl Server {
    /// Listens on UDP `port` of every address of this computer (0 picks a
    /// free one: see [`Self::port`]), for at most `max_connections` at
    /// once. An error when the port is taken.
    pub fn listen(port: u16, max_connections: usize) -> io::Result<Server> {
        let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, port))?;
        let address = socket.local_addr()?;
        let config = ServerConfig {
            current_time: now(),
            max_clients: max_connections.clamp(1, 1024),
            protocol_id: NETCODE_PROTOCOL_ID,
            public_addresses: vec![address],
            // Direct address or LAN play, with no account service to hand
            // out tokens: netcode's packets aren't encrypted.
            authentication: ServerAuthentication::Unsecure,
        };
        let udp = NetcodeServerTransport::new(config, socket)?;
        Ok(Server {
            udp: Some(udp),
            ..Server::in_memory()
        })
    }

    /// [`Self::listen`], secured (MP4): only players with a connect token
    /// from its token port (TCP, the same port) get in, and netcode
    /// encrypts every packet. The token port checks `rules` first.
    pub fn listen_secure(
        port: u16,
        max_connections: usize,
        secured: Secured,
        rules: TokenRules,
    ) -> io::Result<Server> {
        let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, port))?;
        let port = socket.local_addr()?.port();
        // The token port takes the same number, or the server doesn't
        // start: players look for it there.
        let tokens = TokenPort::start(port, &secured, rules)?;
        let config = ServerConfig {
            current_time: now(),
            max_clients: max_connections.clamp(1, 1024),
            protocol_id: NETCODE_PROTOCOL_ID,
            public_addresses: Secured::public_addresses(port),
            authentication: ServerAuthentication::Secure {
                private_key: secured.key,
            },
        };
        let udp = NetcodeServerTransport::new(config, socket)?;
        Ok(Server {
            udp: Some(udp),
            tokens: Some((tokens, secured.cert.fingerprint())),
            ..Server::in_memory()
        })
    }

    /// A secured server's certificate fingerprint, which join links carry.
    pub fn fingerprint(&self) -> Option<&str> {
        self.tokens
            .as_ref()
            .map(|(_, fingerprint)| fingerprint.as_str())
    }

    /// Tokens its token port handed out; 0 unsecured.
    pub fn tokens_issued(&self) -> usize {
        self.tokens.as_ref().map_or(0, |(port, _)| port.issued())
    }

    /// What the token port did since the last call, for the host's log.
    pub fn take_token_log(&self) -> Vec<String> {
        self.tokens
            .as_ref()
            .map(|(port, _)| port.take_log())
            .unwrap_or_default()
    }

    /// A server only loopback clients can reach.
    pub fn in_memory() -> Server {
        Server {
            renet: RenetServer::new(connection_config()),
            udp: None,
            tokens: None,
            loopbacks: Vec::new(),
            next_loopback: FIRST_LOOPBACK,
            conditions: Conditions::default(),
        }
    }

    /// Makes every loopback connection, and those to come, as bad as
    /// `conditions`. UDP connections are as the network is.
    pub fn emulate(&mut self, conditions: Conditions) {
        self.conditions = conditions;
        for (_, pipe) in &self.loopbacks {
            pipe.conditions.set(conditions);
        }
    }

    /// How the connection `id` is doing.
    pub fn stats(&self, id: ConnectionId) -> Option<LinkStats> {
        self.renet.network_info(id).ok().map(LinkStats::from)
    }

    /// Every byte sent to the loopback connection `id` so far.
    pub fn loopback_bytes_sent(&self, id: ConnectionId) -> Option<u64> {
        self.loopbacks
            .iter()
            .find(|(loopback, _)| *loopback == id)
            .map(|(_, pipe)| pipe.to_client.bytes.get())
    }

    /// The UDP port it listens on; `None` in memory.
    pub fn port(&self) -> Option<u16> {
        let udp = self.udp.as_ref()?;
        if let Some((tokens, _)) = &self.tokens {
            return Some(tokens.port());
        }
        udp.addresses().first().map(SocketAddr::port)
    }

    /// A client in this process, connected at once: the same interface as
    /// a UDP client, its packets carried in memory.
    pub fn connect_loopback(&mut self) -> Client {
        let id = self.next_loopback;
        self.next_loopback -= 1;
        let renet = self.renet.new_local_client(id);
        let pipe = Rc::new(Pipe::new(self.conditions, id));
        self.loopbacks.push((id, pipe.clone()));
        Client {
            renet,
            link: Link::Loopback(pipe),
            secured: false,
            ended: None,
        }
    }

    /// Receives: advances the connections by `elapsed` and takes in every
    /// packet that arrived, then returns who connected and who left.
    pub fn update(&mut self, elapsed: Duration) -> Vec<ServerEvent> {
        self.renet.update(elapsed);
        if let Some(udp) = &mut self.udp
            && let Err(err) = udp.update(elapsed, &mut self.renet)
        {
            log::warn!("network: {err}");
        }
        let renet = &mut self.renet;
        self.loopbacks.retain(|(id, pipe)| {
            if pipe.closed.get() == Some(Closed::ByClient) {
                renet.remove_connection(*id);
                return false;
            }
            for packet in pipe.to_server.arrive(elapsed) {
                let _ = renet.process_packet_from(&packet, *id);
            }
            true
        });
        let mut events = Vec::new();
        while let Some(event) = self.renet.get_event() {
            events.push(match event {
                renet::ServerEvent::ClientConnected { client_id } => {
                    ServerEvent::Connected(client_id)
                }
                renet::ServerEvent::ClientDisconnected { client_id, reason } => {
                    ServerEvent::Disconnected {
                        id: client_id,
                        reason: renet_reason(&reason),
                    }
                }
            });
        }
        events
    }

    /// Sends everything queued, to every connection.
    pub fn flush(&mut self) {
        if let Some(udp) = &mut self.udp {
            udp.send_packets(&mut self.renet);
        }
        let renet = &mut self.renet;
        let closing = renet.disconnections_id();
        self.loopbacks.retain(|(id, pipe)| {
            if let Ok(packets) = renet.get_packets_to_send(*id) {
                pipe.to_client.send(pipe, packets);
            }
            if closing.contains(id) {
                pipe.closed.set(Some(Closed::ByServer));
                renet.remove_connection(*id);
                return false;
            }
            true
        });
    }

    pub fn send(&mut self, id: ConnectionId, channel: Channel, message: Vec<u8>) {
        if self.renet.is_connected(id) {
            self.renet.send_message(id, channel, message);
        }
    }

    pub fn receive(&mut self, id: ConnectionId, channel: Channel) -> Option<Vec<u8>> {
        self.renet
            .receive_message(id, channel)
            .map(|bytes| bytes.to_vec())
    }

    /// Closes a connection; its [`ServerEvent::Disconnected`] follows.
    pub fn disconnect(&mut self, id: ConnectionId) {
        self.renet.disconnect(id);
    }

    /// Where a UDP connection comes from; `None` for a loopback one.
    pub fn address_of(&self, id: ConnectionId) -> Option<SocketAddr> {
        self.udp.as_ref()?.client_addr(id)
    }

    /// Closes every connection now, telling each, for a host that stops.
    pub fn shut_down(&mut self) {
        self.renet.disconnect_all();
        if let Some(udp) = &mut self.udp {
            udp.disconnect_all(&mut self.renet);
        }
        self.flush();
    }
}

enum Link {
    Udp(Box<NetcodeClientTransport>, SocketAddr),
    Loopback(Rc<Pipe>),
    /// Getting a connect token from the server's token port on a thread.
    Fetching {
        answer: Receiver<Result<ConnectToken, FetchError>>,
        server: SocketAddr,
        /// Connect unsecured if no token port answers.
        fallback: bool,
    },
}

/// The joining end, over UDP or the loopback.
pub struct Client {
    renet: RenetClient,
    link: Link,
    /// Encrypted with a connect token (or still getting one).
    secured: bool,
    /// Why the connection ended, once it has.
    ended: Option<String>,
}

impl Client {
    /// Starts connecting, unsecured, to the server at `address`
    /// (`"host:port"`): LAN play. An error for an address that can't be
    /// looked up, or no socket.
    pub fn connect(address: &str) -> io::Result<Client> {
        let server = resolve(address)?;
        Ok(Client {
            renet: RenetClient::new(connection_config()),
            link: unsecured(server)?,
            secured: false,
            ended: None,
        })
    }

    /// Starts joining the server at `address`: a connect token from its
    /// token port first (on a thread; `request` with the address filled
    /// in), checked by `trust`, then an encrypted connection. When nothing
    /// answers on the token port and `trust` doesn't expect the server to
    /// be secured, it connects unsecured, as to a LAN game.
    pub fn join(
        address: &str,
        request: impl FnOnce(SocketAddr) -> TokenRequest,
        trust: Trust,
    ) -> io::Result<Client> {
        let server = resolve(address)?;
        let request = request(server);
        let fallback = match &trust {
            Trust::Pinned(_) => false,
            Trust::Remember(path) => !crate::secure::is_known(path, server),
            Trust::Any => true,
        };
        let (send, answer) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .name("token fetch".to_owned())
            .spawn(move || {
                let _ = send.send(fetch_token(server, &request, &trust));
            })?;
        Ok(Client {
            renet: RenetClient::new(connection_config()),
            link: Link::Fetching {
                answer,
                server,
                fallback,
            },
            secured: true,
            ended: None,
        })
    }

    /// Whether the connection is encrypted with a token (or still getting
    /// one); `false` unsecured and on the loopback.
    pub fn is_secured(&self) -> bool {
        self.secured
    }

    /// While getting a token: takes the answer once it's in.
    fn fetched(&mut self) {
        let Link::Fetching {
            answer,
            server,
            fallback,
        } = &self.link
        else {
            return;
        };
        let (server, fallback) = (*server, *fallback);
        let got = match answer.try_recv() {
            Ok(got) => got,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err(FetchError::Refused(
                "Getting a connect token failed.".to_owned(),
            )),
        };
        let link = match got {
            Ok(token) => secured(server, token),
            Err(FetchError::NoTokenPort(_)) if fallback => {
                self.secured = false;
                unsecured(server)
            }
            Err(FetchError::NoTokenPort(why)) => {
                self.ended = Some(format!(
                    "The server isn't answering on its token port ({why}). It was secured \
                     before: if it's on your LAN now, join it from a LAN search."
                ));
                return;
            }
            Err(FetchError::Refused(reason)) => {
                self.ended = Some(reason);
                return;
            }
        };
        match link {
            Ok(link) => self.link = link,
            Err(err) => self.ended = Some(format!("The connection failed: {err}.")),
        }
    }

    /// Receives: advances the connection by `elapsed` and takes in every
    /// packet that arrived.
    pub fn update(&mut self, elapsed: Duration) {
        self.fetched();
        if self.ended.is_some() && matches!(self.link, Link::Fetching { .. }) {
            return;
        }
        self.renet.update(elapsed);
        match &mut self.link {
            Link::Fetching { .. } => {}
            Link::Udp(transport, server) => {
                if let Err(err) = transport.update(elapsed, &mut self.renet)
                    && self.ended.is_none()
                {
                    let reason = match transport.disconnect_reason() {
                        Some(reason) => netcode_reason(reason, *server),
                        None => match self.renet.disconnect_reason() {
                            Some(reason) => renet_reason(&reason),
                            None => format!("The connection failed: {err}."),
                        },
                    };
                    self.ended = Some(reason);
                }
            }
            Link::Loopback(pipe) => {
                if pipe.closed.get() == Some(Closed::ByServer) && self.ended.is_none() {
                    self.renet.disconnect_due_to_transport();
                    self.ended = Some("The server closed the connection.".to_owned());
                }
                for packet in pipe.to_client.arrive(elapsed) {
                    self.renet.process_packet(&packet);
                }
            }
        }
    }

    /// Sends everything queued.
    pub fn flush(&mut self) {
        if self.ended.is_some() {
            return;
        }
        match &mut self.link {
            Link::Fetching { .. } => {}
            Link::Udp(transport, _) => {
                // A failure here shows in the next update.
                let _ = transport.send_packets(&mut self.renet);
            }
            Link::Loopback(pipe) => {
                let packets = self.renet.get_packets_to_send();
                pipe.to_server.send(pipe, packets);
            }
        }
    }

    pub fn is_connected(&self) -> bool {
        self.ended.is_none() && self.renet.is_connected()
    }

    /// How the connection is doing.
    pub fn stats(&self) -> LinkStats {
        self.renet.network_info().into()
    }

    /// Why the connection ended; `None` while connecting or connected.
    pub fn ended(&self) -> Option<&str> {
        self.ended.as_deref()
    }

    pub fn send(&mut self, channel: Channel, message: Vec<u8>) {
        if self.is_connected() {
            self.renet.send_message(channel, message);
        }
    }

    pub fn receive(&mut self, channel: Channel) -> Option<Vec<u8>> {
        self.renet
            .receive_message(channel)
            .map(|bytes| bytes.to_vec())
    }

    /// Leaves, telling the server at once.
    pub fn disconnect(&mut self) {
        if self.ended.is_some() {
            return;
        }
        self.flush();
        match &mut self.link {
            Link::Fetching { .. } => {}
            Link::Udp(transport, _) => transport.disconnect(),
            Link::Loopback(pipe) => pipe.closed.set(Some(Closed::ByClient)),
        }
        self.renet.disconnect();
        self.ended = Some("You left the game.".to_owned());
    }
}

/// An unsecured UDP link to `server`.
fn unsecured(server: SocketAddr) -> io::Result<Link> {
    let authentication = ClientAuthentication::Unsecure {
        protocol_id: NETCODE_PROTOCOL_ID,
        client_id: fresh_client_id(),
        server_addr: server,
        user_data: None,
    };
    udp_link(server, authentication)
}

/// A UDP link to `server` encrypted with `token`.
fn secured(server: SocketAddr, token: ConnectToken) -> io::Result<Link> {
    udp_link(
        server,
        ClientAuthentication::Secure {
            connect_token: token,
        },
    )
}

fn udp_link(server: SocketAddr, authentication: ClientAuthentication) -> io::Result<Link> {
    let local: SocketAddr = match server {
        SocketAddr::V4(_) => (Ipv4Addr::UNSPECIFIED, 0).into(),
        SocketAddr::V6(_) => (Ipv6Addr::UNSPECIFIED, 0).into(),
    };
    let socket = UdpSocket::bind(local)?;
    let transport = NetcodeClientTransport::new(now(), authentication, socket)
        .map_err(|err| io::Error::other(err.to_string()))?;
    Ok(Link::Udp(Box::new(transport), server))
}

/// `"host:port"` as an address, IPv4 first (servers listen on IPv4).
fn resolve(address: &str) -> io::Result<SocketAddr> {
    let found: Vec<SocketAddr> = address
        .to_socket_addrs()
        .map_err(|err| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("\"{address}\" isn't an address and port, such as \"192.168.1.20:47800\" ({err})"),
            )
        })?
        .collect();
    found
        .iter()
        .find(|address| address.is_ipv4())
        .or(found.first())
        .copied()
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("\"{address}\" has no address"),
            )
        })
}

/// A client id no other client is likely to have: netcode tells clients
/// apart by it.
fn fresh_client_id() -> u64 {
    let nanos = now().as_nanos() as u64;
    let mixed = nanos ^ (u64::from(std::process::id()) << 32);
    // SplitMix64's finish, so close times give far-apart ids.
    let mut z = mixed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    (z ^ (z >> 31)) % (FIRST_LOOPBACK / 2)
}

fn netcode_reason(reason: NetcodeDisconnectReason, server: SocketAddr) -> String {
    match reason {
        NetcodeDisconnectReason::ConnectTokenExpired
        | NetcodeDisconnectReason::ConnectionRequestTimedOut
        | NetcodeDisconnectReason::ConnectionResponseTimedOut => {
            format!("No game answered at {server}.")
        }
        NetcodeDisconnectReason::ConnectionTimedOut => {
            "The connection timed out: nothing came from the server for 15 seconds.".to_owned()
        }
        NetcodeDisconnectReason::ConnectionDenied => "The server is full.".to_owned(),
        NetcodeDisconnectReason::DisconnectedByClient => "You left the game.".to_owned(),
        NetcodeDisconnectReason::DisconnectedByServer => {
            "The server closed the connection.".to_owned()
        }
    }
}

fn renet_reason(reason: &DisconnectReason) -> String {
    match reason {
        DisconnectReason::DisconnectedByClient => "They left the game.".to_owned(),
        DisconnectReason::DisconnectedByServer => "The server closed the connection.".to_owned(),
        DisconnectReason::Transport => "The connection was lost.".to_owned(),
        other => format!("The connection failed: {other}."),
    }
}
