//! `axcenity-relay`: a rendezvous and relay for Axcenity games behind
//! routers (multiplayer readiness MP5, ADR-0026 D2).
//!
//! A host connects over TCP and gets a **join code**, and keeps that
//! connection open; its game socket tells the relay where it is (UDP
//! `Host` packets). A player sends `Bind` with the code from its own game
//! socket, learns the host's address, and both send `Punch` packets to
//! open a direct path. If none opens, the player's netcode packets go to
//! the relay, which passes them to the host inside `Data` packets and
//! back. A player's connect token comes from the host's token port
//! through a TCP connection the relay splices to one the host opens.
//!
//! The relay can't read or change a game: netcode encrypts every packet
//! with keys from the token, and the token exchange is TLS from player to
//! host. It keeps nothing on disk.

pub mod protocol;

use std::collections::HashMap;
use std::io::{self, Write};
use std::net::{IpAddr, Ipv4Addr, Shutdown, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use ring::rand::{SecureRandom, SystemRandom};

use protocol::{
    Bound, CODE_ALPHABET, CODE_LENGTH, FromRelay, Packet, SECRET_BYTES, ToRelay, display_code,
    from_hex, read_line, send_line, to_hex,
};

/// How long a game is kept for its host after the host's connection
/// drops or its game socket goes quiet, so it can take its code back.
pub const ORPHAN_SECONDS: u64 = 60;
/// A host whose game socket hasn't been heard from for this long is gone.
const HOST_QUIET: Duration = Duration::from_secs(30);
/// A player not heard from for this long is forgotten.
const PLAYER_QUIET: Duration = Duration::from_secs(60);
/// How long a player's token request waits for the host to answer.
const OPEN_WAIT: Duration = Duration::from_secs(5);
/// How long a spliced connection may sit with nothing sent.
const SPLICE_IDLE: Duration = Duration::from_secs(30);
/// Spliced connections at once.
const MOST_SPLICES: usize = 256;
/// TCP connections at once (hosts, players and splices); more are closed
/// at once, so nobody can tie the relay up with threads.
const MOST_CONNECTIONS: usize = 1024;
/// Joins by code one address may try a minute (codes can't be guessed
/// quickly); this computer's own addresses aren't limited.
const JOINS_PER_MINUTE: u32 = 30;
/// The biggest UDP packet passed on (netcode's are at most 1,400 bytes, and
/// `Data` adds 8).
const MOST_PACKET_BYTES: usize = 1500;

/// How a relay runs.
#[derive(Clone, Debug)]
pub struct RelaySettings {
    /// The TCP and UDP port (0: any free one).
    pub port: u16,
    /// Games hosted at once.
    pub max_games: usize,
    /// Players bound to one game at once.
    pub max_players: usize,
}

impl Default for RelaySettings {
    fn default() -> RelaySettings {
        RelaySettings {
            port: protocol::DEFAULT_RELAY_PORT,
            max_games: 100,
            max_players: 64,
        }
    }
}

/// A running relay; stops when dropped.
pub struct Relay {
    port: u16,
    shared: Arc<Shared>,
    threads: Vec<JoinHandle<()>>,
}

struct Shared {
    settings: RelaySettings,
    state: Mutex<State>,
    udp: UdpSocket,
    stop: AtomicBool,
    splices: AtomicUsize,
    connections: AtomicUsize,
    log: Mutex<Vec<String>>,
    random: SystemRandom,
}

#[derive(Default)]
struct State {
    games: HashMap<String, Game>,
    by_secret: HashMap<[u8; SECRET_BYTES], String>,
    by_host: HashMap<SocketAddr, String>,
    by_player: HashMap<SocketAddr, String>,
    /// Token requests waiting for the host's `Data` connection.
    waiting: HashMap<u64, (String, Sender<TcpStream>)>,
    next_open: u64,
    joins: HashMap<IpAddr, (u32, Instant)>,
}

struct Game {
    secret: [u8; SECRET_BYTES],
    name: String,
    /// Where the host's game socket is, as the relay sees it.
    host: Option<SocketAddr>,
    host_seen: Instant,
    /// The host's connection, for the relay's requests.
    control: Option<TcpStream>,
    /// When the host went away, if it has.
    orphaned: Option<Instant>,
    players: HashMap<SocketAddr, Player>,
    by_id: HashMap<u32, SocketAddr>,
    next_id: u32,
}

struct Player {
    id: u32,
    seen: Instant,
    /// When the host was last asked to punch towards it.
    punched: Option<Instant>,
}

impl Relay {
    /// Listens on TCP and UDP `settings.port` of every address.
    pub fn start(settings: RelaySettings) -> io::Result<Relay> {
        let udp = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, settings.port))?;
        let port = udp.local_addr()?.port();
        let tcp = TcpListener::bind((Ipv4Addr::UNSPECIFIED, port))?;
        tcp.set_nonblocking(true)?;
        udp.set_read_timeout(Some(Duration::from_millis(100)))?;
        let shared = Arc::new(Shared {
            settings,
            state: Mutex::new(State::default()),
            udp,
            stop: AtomicBool::new(false),
            splices: AtomicUsize::new(0),
            connections: AtomicUsize::new(0),
            log: Mutex::new(Vec::new()),
            random: SystemRandom::new(),
        });
        let udp_shared = shared.clone();
        let tcp_shared = shared.clone();
        let threads = vec![
            std::thread::Builder::new()
                .name("relay udp".to_owned())
                .spawn(move || udp_loop(&udp_shared))?,
            std::thread::Builder::new()
                .name("relay tcp".to_owned())
                .spawn(move || accept_loop(tcp, tcp_shared))?,
        ];
        Ok(Relay {
            port,
            shared,
            threads,
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// Games hosted now (including ones waiting for their host).
    pub fn games(&self) -> usize {
        self.shared.state().games.len()
    }

    /// Players bound to games now.
    pub fn players(&self) -> usize {
        self.shared.state().by_player.len()
    }

    /// What happened since the last call.
    pub fn take_log(&self) -> Vec<String> {
        self.shared
            .log
            .lock()
            .map(|mut log| std::mem::take(&mut *log))
            .unwrap_or_default()
    }
}

impl Drop for Relay {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        // Hosts' connections end, so their links notice.
        for game in self.shared.state().games.values() {
            if let Some(control) = &game.control {
                let _ = control.shutdown(Shutdown::Both);
            }
        }
        for thread in self.threads.drain(..) {
            let _ = thread.join();
        }
    }
}

impl Shared {
    fn state(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn note(&self, line: String) {
        if let Ok(mut log) = self.log.lock() {
            log.push(line);
        }
    }

    fn random_bytes<const N: usize>(&self) -> [u8; N] {
        let mut bytes = [0u8; N];
        self.random
            .fill(&mut bytes)
            .expect("the system's random numbers");
        bytes
    }

    fn new_code(&self, state: &State) -> String {
        loop {
            let bytes: [u8; CODE_LENGTH] = self.random_bytes();
            let code: String = bytes
                .iter()
                .map(|b| CODE_ALPHABET[*b as usize % CODE_ALPHABET.len()] as char)
                .collect();
            if !state.games.contains_key(&code) {
                return code;
            }
        }
    }

    fn send_to(&self, packet: &[u8], to: SocketAddr) {
        let _ = self.udp.send_to(packet, to);
    }
}

/// Sends `message` on a host's connection, outside the lock.
fn tell(control: Option<TcpStream>, message: &FromRelay) {
    if let Some(mut control) = control {
        let _ = control.set_write_timeout(Some(Duration::from_secs(1)));
        let _ = send_line(&mut control, message);
    }
}

fn udp_loop(shared: &Shared) {
    let mut buffer = [0u8; 2048];
    let mut swept = Instant::now();
    while !shared.stop.load(Ordering::Relaxed) {
        match shared.udp.recv_from(&mut buffer) {
            Ok((len, from)) if len <= MOST_PACKET_BYTES => datagram(shared, &buffer[..len], from),
            Ok(_) => {}
            Err(err)
                if matches!(
                    err.kind(),
                    io::ErrorKind::WouldBlock
                        | io::ErrorKind::TimedOut
                        | io::ErrorKind::Interrupted
                ) => {}
            // A packet to a player who has gone shows here on Windows.
            Err(_) => {}
        }
        if swept.elapsed() >= Duration::from_secs(1) {
            swept = Instant::now();
            sweep(shared);
        }
    }
}

fn datagram(shared: &Shared, bytes: &[u8], from: SocketAddr) {
    match Packet::read(bytes) {
        Some(Packet::Host { secret }) => {
            let mut state = shared.state();
            let Some(code) = state.by_secret.get(&secret).cloned() else {
                return;
            };
            let Some(game) = state.games.get_mut(&code) else {
                return;
            };
            game.host_seen = Instant::now();
            let old = game.host.replace(from);
            if old != Some(from) {
                if let Some(old) = old {
                    state.by_host.remove(&old);
                }
                state.by_host.insert(from, code.clone());
                shared.note(format!("Game {} is at {from}", display_code(&code)));
            }
        }
        Some(Packet::Bind { code }) => bind(shared, &code, from),
        Some(Packet::Data { player, payload }) => {
            let state = shared.state();
            let to = state
                .by_host
                .get(&from)
                .and_then(|code| state.games.get(code))
                .and_then(|game| game.by_id.get(&player))
                .copied();
            drop(state);
            if let Some(to) = to {
                shared.send_to(payload, to);
            }
        }
        Some(Packet::Punch | Packet::Bound(_)) => {}
        None => {
            // A bound player's netcode packet: on to its host.
            let mut state = shared.state();
            let Some(code) = state.by_player.get(&from).cloned() else {
                return;
            };
            let Some(game) = state.games.get_mut(&code) else {
                return;
            };
            let Some(player) = game.players.get_mut(&from) else {
                return;
            };
            player.seen = Instant::now();
            let id = player.id;
            let host = game.host;
            drop(state);
            if let Some(host) = host {
                let packet = Packet::Data {
                    player: id,
                    payload: bytes,
                }
                .to_bytes();
                shared.send_to(&packet, host);
            }
        }
    }
}

fn bind(shared: &Shared, code: &[u8; CODE_LENGTH], from: SocketAddr) {
    let code = String::from_utf8_lossy(code).into_owned();
    let mut state = shared.state();
    let max_players = shared.settings.max_players;
    let now = Instant::now();
    let answer;
    let mut punch = None;
    match state.games.get_mut(&code) {
        None => answer = Bound::NoGame,
        Some(game) if !game.players.contains_key(&from) && game.players.len() >= max_players => {
            answer = Bound::Full;
        }
        Some(game) => {
            let next_id = game.next_id;
            let player = game.players.entry(from).or_insert_with(|| Player {
                id: next_id,
                seen: now,
                punched: None,
            });
            if player.id == next_id {
                game.next_id += 1;
                game.by_id.insert(next_id, from);
            }
            player.seen = now;
            if game.host.is_some()
                && player
                    .punched
                    .is_none_or(|at| now.duration_since(at) > Duration::from_secs(2))
            {
                player.punched = Some(now);
                punch = game.control.as_ref().and_then(|c| c.try_clone().ok());
            }
            answer = Bound::To {
                host: game.host,
                you: from,
            };
            if let Some(old) = state.by_player.insert(from, code.clone())
                && old != code
                && let Some(game) = state.games.get_mut(&old)
                && let Some(player) = game.players.remove(&from)
            {
                game.by_id.remove(&player.id);
            }
        }
    }
    drop(state);
    shared.send_to(&Packet::Bound(answer).to_bytes(), from);
    tell(punch, &FromRelay::Punch { to: from });
}

/// Forgets quiet players, and hosts gone for good.
fn sweep(shared: &Shared) {
    let mut state = shared.state();
    let now = Instant::now();
    let mut forgotten_players = Vec::new();
    let mut gone = Vec::new();
    for (code, game) in &mut state.games {
        game.players.retain(|address, player| {
            let keep = now.duration_since(player.seen) < PLAYER_QUIET;
            if !keep {
                forgotten_players.push(*address);
            }
            keep
        });
        let ids: Vec<u32> = game.players.values().map(|p| p.id).collect();
        game.by_id.retain(|id, _| ids.contains(id));
        if game.orphaned.is_none() && now.duration_since(game.host_seen) > HOST_QUIET {
            game.orphaned = Some(now);
            if let Some(control) = game.control.take() {
                let _ = control.shutdown(Shutdown::Both);
            }
            shared.note(format!(
                "Game {}'s host went quiet; keeping its code for {ORPHAN_SECONDS} s",
                display_code(code)
            ));
        }
        if game
            .orphaned
            .is_some_and(|at| now.duration_since(at) > Duration::from_secs(ORPHAN_SECONDS))
        {
            gone.push(code.clone());
        }
    }
    for address in forgotten_players {
        state.by_player.remove(&address);
    }
    for code in gone {
        if let Some(game) = state.games.remove(&code) {
            state.by_secret.remove(&game.secret);
            if let Some(host) = game.host {
                state.by_host.remove(&host);
            }
            for address in game.players.keys() {
                state.by_player.remove(address);
            }
            shared.note(format!(
                "Game {} (\"{}\") ended",
                display_code(&code),
                game.name
            ));
        }
    }
    state
        .joins
        .retain(|_, (_, since)| now.duration_since(*since) < Duration::from_secs(60));
}

fn accept_loop(listener: TcpListener, shared: Arc<Shared>) {
    while !shared.stop.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, from)) => {
                if shared.connections.fetch_add(1, Ordering::Relaxed) >= MOST_CONNECTIONS {
                    shared.connections.fetch_sub(1, Ordering::Relaxed);
                    continue;
                }
                let mine = shared.clone();
                let spawned = std::thread::Builder::new()
                    .name("relay connection".to_owned())
                    .spawn(move || {
                        let _ = connection(stream, from, &mine);
                        mine.connections.fetch_sub(1, Ordering::Relaxed);
                    });
                if spawned.is_err() {
                    shared.connections.fetch_sub(1, Ordering::Relaxed);
                }
            }
            Err(_) => std::thread::sleep(Duration::from_millis(20)),
        }
    }
}

fn connection(mut stream: TcpStream, from: SocketAddr, shared: &Shared) -> io::Result<()> {
    stream.set_nonblocking(false)?;
    stream.set_nodelay(true)?;
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    stream.set_write_timeout(Some(Duration::from_secs(10)))?;
    match read_line::<ToRelay>(&mut stream)? {
        ToRelay::Host { game, reclaim } => host(stream, from, shared, game, reclaim),
        ToRelay::Data { id, secret } => {
            let mut state = shared.state();
            let Some((code, _)) = state.waiting.get(&id) else {
                return Ok(());
            };
            let matches = state
                .games
                .get(code)
                .is_some_and(|game| to_hex(&game.secret) == secret);
            if matches && let Some((_, sender)) = state.waiting.remove(&id) {
                let _ = sender.send(stream);
            }
            Ok(())
        }
        ToRelay::Join { code } => join(stream, from, shared, &code),
    }
}

fn host(
    mut stream: TcpStream,
    from: SocketAddr,
    shared: &Shared,
    name: String,
    reclaim: Option<(String, String)>,
) -> io::Result<()> {
    let name: String = name.chars().take(100).collect();
    let mut state = shared.state();
    let reclaimed = reclaim.and_then(|(code, secret)| {
        let game = state.games.get(&code)?;
        (to_hex(&game.secret) == secret && game.control.is_none()).then_some(code)
    });
    let code = match reclaimed {
        Some(code) => code,
        None if state.games.len() >= shared.settings.max_games => {
            drop(state);
            return send_line(
                &mut stream,
                &FromRelay::Refused {
                    reason: "The relay is hosting as many games as it takes.".to_owned(),
                },
            );
        }
        None => {
            let code = shared.new_code(&state);
            let secret: [u8; SECRET_BYTES] = shared.random_bytes();
            state.by_secret.insert(secret, code.clone());
            state.games.insert(
                code.clone(),
                Game {
                    secret,
                    name: name.clone(),
                    host: None,
                    host_seen: Instant::now(),
                    control: None,
                    orphaned: None,
                    players: HashMap::new(),
                    by_id: HashMap::new(),
                    next_id: 1,
                },
            );
            code
        }
    };
    let game = state.games.get_mut(&code).expect("just found or made");
    game.control = Some(stream.try_clone()?);
    game.orphaned = None;
    game.host_seen = Instant::now();
    let secret = to_hex(&game.secret);
    drop(state);
    shared.note(format!(
        "Hosting \"{name}\" from {from} as {}",
        display_code(&code)
    ));
    send_line(
        &mut stream,
        &FromRelay::Hosted {
            code: code.clone(),
            secret,
        },
    )?;
    // The connection stays open until the host leaves; nothing more comes
    // from it.
    stream.set_read_timeout(Some(Duration::from_secs(1)))?;
    let mut byte = [0u8; 64];
    loop {
        if shared.stop.load(Ordering::Relaxed) {
            return Ok(());
        }
        match io::Read::read(&mut stream, &mut byte) {
            Ok(0) => break,
            Ok(_) => {}
            Err(err)
                if matches!(
                    err.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) => {}
            Err(_) => break,
        }
    }
    let mut state = shared.state();
    if let Some(game) = state.games.get_mut(&code)
        && game.orphaned.is_none()
    {
        game.control = None;
        game.orphaned = Some(Instant::now());
        shared.note(format!(
            "Game {}'s host left; keeping its code for {ORPHAN_SECONDS} s",
            display_code(&code)
        ));
    }
    Ok(())
}

fn join(mut player: TcpStream, from: SocketAddr, shared: &Shared, code: &str) -> io::Result<()> {
    let refuse = |player: &mut TcpStream, reason: &str| {
        send_line(
            player,
            &FromRelay::Refused {
                reason: reason.to_owned(),
            },
        )
    };
    let code = protocol::normalize_code(code).unwrap_or_default();
    let mut state = shared.state();
    let now = Instant::now();
    if !from.ip().is_loopback() {
        let (count, _) = state.joins.entry(from.ip()).or_insert((0, now));
        *count += 1;
        if *count > JOINS_PER_MINUTE {
            drop(state);
            return refuse(
                &mut player,
                "Too many joins from your address: wait a minute.",
            );
        }
    }
    let control = match state.games.get(&code) {
        None => None,
        Some(game) => game.control.as_ref().and_then(|c| c.try_clone().ok()),
    };
    let Some(control) = control else {
        drop(state);
        return refuse(
            &mut player,
            &format!(
                "No game is hosted with the code {} on this relay (it may have ended).",
                display_code(&code)
            ),
        );
    };
    if shared.splices.load(Ordering::Relaxed) >= MOST_SPLICES {
        drop(state);
        return refuse(&mut player, "The relay is busy: try again in a moment.");
    }
    let id = state.next_open;
    state.next_open += 1;
    let (send, receive) = mpsc::channel();
    state.waiting.insert(id, (code.clone(), send));
    drop(state);
    tell(Some(control), &FromRelay::Open { id, from });
    let host = receive.recv_timeout(OPEN_WAIT);
    shared.state().waiting.remove(&id);
    let Ok(host) = host else {
        return refuse(&mut player, "The host didn't answer through the relay.");
    };
    send_line(&mut player, &FromRelay::Joined)?;
    splice(player, host, shared);
    Ok(())
}

/// Passes bytes both ways until either end closes or both go quiet.
fn splice(a: TcpStream, b: TcpStream, shared: &Shared) {
    shared.splices.fetch_add(1, Ordering::Relaxed);
    let pairs = (|| {
        for stream in [&a, &b] {
            stream.set_read_timeout(Some(SPLICE_IDLE))?;
            stream.set_write_timeout(Some(SPLICE_IDLE))?;
        }
        Ok::<_, io::Error>(((a.try_clone()?, b.try_clone()?), (b, a)))
    })();
    if let Ok(((mut a_read, mut b_write), (mut b_read, mut a_write))) = pairs {
        let other = std::thread::spawn(move || {
            let _ = io::copy(&mut a_read, &mut b_write);
            let _ = b_write.shutdown(Shutdown::Write);
        });
        let _ = io::copy(&mut b_read, &mut a_write);
        let _ = a_write.shutdown(Shutdown::Write);
        let _ = a_write.flush();
        let _ = other.join();
    }
    shared.splices.fetch_sub(1, Ordering::Relaxed);
}

/// Hex of a secret, for a host to keep.
pub fn secret_from_hex(text: &str) -> Option<[u8; SECRET_BYTES]> {
    from_hex(text)?.try_into().ok()
}

#[cfg(test)]
mod tests;
