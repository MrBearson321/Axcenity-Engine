//! Reaching a host behind a router (multiplayer readiness MP5, ADR-0026
//! D2) through an `axcenity-relay`: the host's link to it, and a player's
//! join by code.
//!
//! The host keeps a TCP connection to the relay, which gave it a join
//! code, and its game socket sends the relay `Host` packets every few
//! seconds so the relay knows where it is (and the router keeps the way
//! open). A player sends `Bind` with the code from its own game socket,
//! then both send `Punch` packets to each other's address as the relay
//! sees it. If the host's punch arrives the player connects directly;
//! otherwise its packets go through the relay. Either way its connect
//! token comes from the host's token port over a TCP connection the
//! relay splices, TLS from end to end.

use std::io::{self, BufRead, BufReader};
use std::net::{Ipv4Addr, SocketAddr, TcpStream, UdpSocket};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use axcenity_relay::protocol::{
    Bound, FromRelay, Packet, ToRelay, display_code, read_line, send_line,
};
use renet_netcode::ConnectToken;

use crate::secure::{FetchError, TokenHandle, TokenRequest, Trust, token_over};

/// How often the host's game socket tells the relay where it is.
const HOST_EVERY: Duration = Duration::from_secs(5);
/// How long a player tries for a direct path before using the relay.
pub const PUNCH_TIME: Duration = Duration::from_secs(2);
/// Between punches.
const PUNCH_EVERY: Duration = Duration::from_millis(100);
/// How long the relay has to answer.
const RELAY_WAIT: Duration = Duration::from_secs(5);

/// What the host's relay link reports.
#[derive(Clone, Debug, PartialEq)]
pub enum RelayEvent {
    /// The game's join code on the relay (again, after a reconnect).
    Hosted { code: String },
    /// Lost the relay, or it refused: said as it is.
    Trouble(String),
}

/// A host's link to its relay; ends when dropped.
pub(crate) struct RelayLink {
    events: Receiver<RelayEvent>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl RelayLink {
    /// Hosts `game` on the relay at `relay`, from the game socket
    /// `socket`, answering token requests with `tokens`.
    pub fn start(
        relay: SocketAddr,
        game: String,
        socket: UdpSocket,
        tokens: TokenHandle,
    ) -> io::Result<RelayLink> {
        let (send, events) = channel();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let thread = std::thread::Builder::new()
            .name("relay link".to_owned())
            .spawn(move || {
                let link = Link {
                    relay,
                    game,
                    socket,
                    tokens,
                    events: send,
                    stop: stopping,
                };
                link.run();
            })?;
        Ok(RelayLink {
            events,
            stop,
            thread: Some(thread),
        })
    }

    pub fn take_events(&self) -> Vec<RelayEvent> {
        self.events.try_iter().collect()
    }
}

impl Drop for RelayLink {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct Link {
    relay: SocketAddr,
    game: String,
    socket: UdpSocket,
    tokens: TokenHandle,
    events: Sender<RelayEvent>,
    stop: Arc<AtomicBool>,
}

impl Link {
    fn stopped(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }

    fn run(&self) {
        // The code and secret to take back after a reconnect.
        let mut hosted: Option<(String, String)> = None;
        let mut wait = Duration::from_secs(1);
        let mut said_trouble = false;
        while !self.stopped() {
            match self.host_once(&mut hosted) {
                Ok(()) => wait = Duration::from_secs(1),
                Err(trouble) => {
                    if !said_trouble {
                        let _ = self.events.send(RelayEvent::Trouble(trouble));
                        said_trouble = true;
                    }
                    wait = (wait * 2).min(Duration::from_secs(15));
                }
            }
            if hosted.is_some() {
                said_trouble = false;
            }
            let until = Instant::now() + wait;
            while !self.stopped() && Instant::now() < until {
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }

    /// Hosts on the relay and serves it until the connection drops.
    fn host_once(&self, hosted: &mut Option<(String, String)>) -> Result<(), String> {
        let relay = self.relay;
        let mut control = TcpStream::connect_timeout(&relay, RELAY_WAIT)
            .map_err(|err| format!("Can't reach the relay at {relay}: {err}."))?;
        let io = |err: io::Error| format!("The relay at {relay} stopped answering: {err}.");
        control.set_read_timeout(Some(RELAY_WAIT)).map_err(io)?;
        control.set_nodelay(true).map_err(io)?;
        send_line(
            &mut control,
            &ToRelay::Host {
                game: self.game.clone(),
                reclaim: hosted.clone(),
            },
        )
        .map_err(io)?;
        let (code, secret) = match read_line::<FromRelay>(&mut control).map_err(io)? {
            FromRelay::Hosted { code, secret } => (code, secret),
            FromRelay::Refused { reason } => {
                return Err(format!("The relay at {relay} refused: {reason}"));
            }
            _ => return Err(format!("The relay at {relay} answered nonsense.")),
        };
        let Some(secret_bytes) = axcenity_relay::secret_from_hex(&secret) else {
            return Err(format!("The relay at {relay} answered nonsense."));
        };
        if hosted.as_ref().map(|(old, _)| old) != Some(&code) {
            let _ = self.events.send(RelayEvent::Hosted { code: code.clone() });
        }
        *hosted = Some((code, secret.clone()));
        let here = Packet::Host {
            secret: secret_bytes,
        }
        .to_bytes();
        control
            .set_read_timeout(Some(Duration::from_millis(250)))
            .map_err(io)?;
        let mut reader = BufReader::new(control);
        let mut line = String::new();
        let mut told = Instant::now() - HOST_EVERY;
        while !self.stopped() {
            if told.elapsed() >= HOST_EVERY {
                let _ = self.socket.send_to(&here, relay);
                told = Instant::now();
            }
            match reader.read_line(&mut line) {
                Ok(0) => return Err(format!("Lost the relay at {relay}; joining again.")),
                Ok(_) if line.ends_with('\n') => {
                    if let Ok(message) = serde_json::from_str::<FromRelay>(line.trim()) {
                        self.follow(message, &secret);
                    }
                    line.clear();
                }
                Ok(_) => {}
                Err(err)
                    if matches!(
                        err.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                    ) => {}
                Err(err) => {
                    return Err(format!("Lost the relay at {relay} ({err}); joining again."));
                }
            }
        }
        Ok(())
    }

    fn follow(&self, message: FromRelay, secret: &str) {
        match message {
            FromRelay::Open { id, from } => {
                let (relay, secret, tokens) = (self.relay, secret.to_owned(), self.tokens.clone());
                let _ = std::thread::Builder::new()
                    .name("relayed token".to_owned())
                    .spawn(move || {
                        let Ok(mut stream) = TcpStream::connect_timeout(&relay, RELAY_WAIT) else {
                            return;
                        };
                        if send_line(&mut stream, &ToRelay::Data { id, secret }).is_ok()
                            && !tokens.answer(stream, from)
                        {
                            tokens.note(format!("Token port too busy for {from} (relayed)"));
                        }
                    });
            }
            FromRelay::Punch { to } => {
                if let Ok(socket) = self.socket.try_clone() {
                    let _ = std::thread::Builder::new()
                        .name("punch".to_owned())
                        .spawn(move || punch(&socket, to, PUNCH_TIME));
                }
            }
            _ => {}
        }
    }
}

/// Sends `Punch` packets to `to` for `time`.
fn punch(socket: &UdpSocket, to: SocketAddr, time: Duration) {
    let packet = Packet::Punch.to_bytes();
    let start = Instant::now();
    while start.elapsed() < time {
        let _ = socket.send_to(&packet, to);
        std::thread::sleep(PUNCH_EVERY);
    }
}

/// How a player reached a host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    /// At the address it was given (or found on the LAN).
    Direct,
    /// Directly, at the host's public address, through a hole both ends
    /// punched.
    Punched(SocketAddr),
    /// Through the relay at this address.
    Relayed(SocketAddr),
}

/// A token, the socket to use it from, and the way to the host.
pub(crate) struct Fetched {
    pub token: ConnectToken,
    pub socket: Option<UdpSocket>,
    pub route: Route,
}

/// Joins the game with `code` on the relay at `relay`: binds, tries a
/// direct path when `try_punch`, then gets a token through the relay.
/// Blocks for a few seconds: run it on a thread.
pub(crate) fn fetch_by_code(
    relay: SocketAddr,
    code: &str,
    request: impl FnOnce(SocketAddr) -> TokenRequest,
    trust: &Trust,
    try_punch: bool,
) -> Result<Fetched, FetchError> {
    let refused = FetchError::Refused;
    let local: SocketAddr = (Ipv4Addr::UNSPECIFIED, 0).into();
    let socket =
        UdpSocket::bind(local).map_err(|err| refused(format!("No socket to join with: {err}.")))?;
    socket
        .set_read_timeout(Some(Duration::from_millis(50)))
        .map_err(|err| refused(format!("No socket to join with: {err}.")))?;
    let shown = display_code(code);
    let code_bytes: [u8; 6] = code
        .as_bytes()
        .try_into()
        .map_err(|_| refused(format!("\"{code}\" isn't a join code.")))?;
    let host = bind(&socket, relay, code_bytes, &shown)?;
    let mut route = Route::Relayed(relay);
    if try_punch && let Some(host) = host {
        let start = Instant::now();
        let mut sent = Instant::now() - PUNCH_EVERY;
        let mut buffer = [0u8; 2048];
        while start.elapsed() < PUNCH_TIME {
            if sent.elapsed() >= PUNCH_EVERY {
                let _ = socket.send_to(&Packet::Punch.to_bytes(), host);
                sent = Instant::now();
            }
            if let Ok((len, from)) = socket.recv_from(&mut buffer)
                && from == host
                && Packet::read(&buffer[..len]) == Some(Packet::Punch)
            {
                route = Route::Punched(host);
                break;
            }
        }
    }
    let to = match route {
        Route::Punched(host) => host,
        _ => relay,
    };
    let stream = join_stream(relay, code, &shown)?;
    let token = token_over(stream, to, &request(to), trust)?;
    let _ = socket.set_read_timeout(None);
    Ok(Fetched {
        token,
        socket: Some(socket),
        route,
    })
}

/// Binds `socket` to the game with `code`; the host's address if the
/// relay knows it.
fn bind(
    socket: &UdpSocket,
    relay: SocketAddr,
    code: [u8; 6],
    shown: &str,
) -> Result<Option<SocketAddr>, FetchError> {
    let packet = Packet::Bind { code }.to_bytes();
    let start = Instant::now();
    let mut sent = Instant::now() - Duration::from_secs(1);
    let mut buffer = [0u8; 2048];
    while start.elapsed() < RELAY_WAIT {
        if sent.elapsed() >= Duration::from_millis(250) {
            let _ = socket.send_to(&packet, relay);
            sent = Instant::now();
        }
        let Ok((len, from)) = socket.recv_from(&mut buffer) else {
            continue;
        };
        if from != relay {
            continue;
        }
        match Packet::read(&buffer[..len]) {
            Some(Packet::Bound(Bound::To { host, .. })) => return Ok(host),
            Some(Packet::Bound(Bound::NoGame)) => {
                return Err(FetchError::Refused(format!(
                    "No game is hosted with the code {shown} on the relay at {relay} (it may \
                     have ended)."
                )));
            }
            Some(Packet::Bound(Bound::Full)) => {
                return Err(FetchError::Refused(format!(
                    "The game {shown} has as many players as the relay takes."
                )));
            }
            _ => {}
        }
    }
    Err(FetchError::Refused(format!(
        "The relay at {relay} isn't answering on UDP port {} (is it running, and is the \
         port open?).",
        relay.port()
    )))
}

/// A TCP connection through the relay to the host's token port.
fn join_stream(relay: SocketAddr, code: &str, shown: &str) -> Result<TcpStream, FetchError> {
    let io = |err: io::Error| {
        FetchError::Refused(format!(
            "Can't reach the relay at {relay} on TCP port {}: {err}.",
            relay.port()
        ))
    };
    let mut stream = TcpStream::connect_timeout(&relay, RELAY_WAIT).map_err(io)?;
    stream.set_read_timeout(Some(RELAY_WAIT * 2)).map_err(io)?;
    stream.set_nodelay(true).map_err(io)?;
    send_line(
        &mut stream,
        &ToRelay::Join {
            code: code.to_owned(),
        },
    )
    .map_err(io)?;
    match read_line::<FromRelay>(&mut stream).map_err(io)? {
        FromRelay::Joined => Ok(stream),
        FromRelay::Refused { reason } => Err(FetchError::Refused(reason)),
        _ => Err(FetchError::Refused(format!(
            "The relay at {relay} answered nonsense for {shown}."
        ))),
    }
}
