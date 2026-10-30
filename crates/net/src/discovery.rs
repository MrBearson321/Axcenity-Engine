//! Finding games on the local network by broadcast.
//!
//! A host answers on the discovery port ([`DISCOVERY_PORT`](crate::DISCOVERY_PORT)):
//! a [`LanSearch`] broadcasts a question there (and asks this computer
//! too), and each [`LanAnswerer`] replies with its game, version, port and
//! players. Only one host on a computer can answer, the first to take the
//! port; others still take joins by address.

use std::io;
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::time::Duration;

use axcenity_script::LanGame;
use serde::{Deserialize, Serialize};

/// The question, which also tells Axcenity's packets from others'.
const QUESTION: &[u8] = b"AXCENITY-LAN?1";
/// How long a search listens for answers.
pub const SEARCH_TIME: Duration = Duration::from_millis(1000);
/// A search asks again this often, in case a packet was lost.
const ASK_EVERY: Duration = Duration::from_millis(300);

#[derive(Serialize, Deserialize)]
struct Answer {
    /// A number for this host, so one found at two addresses is listed once.
    host: u64,
    game: String,
    version: String,
    port: u16,
    players: u32,
    max_players: u32,
    password: bool,
}

/// What a host says about itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostInfo {
    pub game: String,
    pub version: String,
    /// The game's UDP port, for joining.
    pub port: u16,
    pub players: u32,
    pub max_players: u32,
    pub password: bool,
}

/// A host's answers to searches.
pub struct LanAnswerer {
    socket: UdpSocket,
    host: u64,
}

impl LanAnswerer {
    /// Listens for searches on `port` of every address. An error when
    /// another host on this computer has it.
    pub fn bind(port: u16) -> io::Result<LanAnswerer> {
        let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, port))?;
        socket.set_nonblocking(true)?;
        let host = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64
            ^ u64::from(std::process::id());
        Ok(LanAnswerer { socket, host })
    }

    /// Answers every question that arrived.
    pub fn answer(&self, info: &HostInfo) {
        let mut buffer = [0u8; 64];
        while let Ok((length, from)) = self.socket.recv_from(&mut buffer) {
            if &buffer[..length] != QUESTION {
                continue;
            }
            let answer = Answer {
                host: self.host,
                game: info.game.clone(),
                version: info.version.clone(),
                port: info.port,
                players: info.players,
                max_players: info.max_players,
                password: info.password,
            };
            let bytes = serde_json::to_vec(&answer).expect("answers always serialize");
            if let Err(err) = self.socket.send_to(&bytes, from) {
                log::debug!("LAN answer to {from}: {err}");
            }
        }
    }
}

/// One `FindLanGames`: asks for [`SEARCH_TIME`], then lists what answered.
pub struct LanSearch {
    socket: UdpSocket,
    port: u16,
    /// Only this game's hosts are listed.
    game: String,
    /// Where questions go: the whole network, and this computer (a
    /// broadcast may not come back to the computer that sent it).
    ask_at: Vec<Ipv4Addr>,
    elapsed: Duration,
    asked: Option<Duration>,
    found: Vec<(u64, LanGame)>,
}

impl LanSearch {
    /// Starts asking on the discovery `port` for hosts of `game`.
    pub fn start(port: u16, game: &str) -> io::Result<LanSearch> {
        let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?;
        socket.set_nonblocking(true)?;
        socket.set_broadcast(true)?;
        Ok(LanSearch {
            socket,
            port,
            game: game.to_owned(),
            ask_at: vec![Ipv4Addr::BROADCAST, Ipv4Addr::LOCALHOST],
            elapsed: Duration::ZERO,
            asked: None,
            found: Vec::new(),
        })
    }

    /// Asks only at `address`, as a question from another computer arrives
    /// at this one's network address (tests).
    #[cfg(test)]
    pub(crate) fn asking_only(mut self, address: Ipv4Addr) -> LanSearch {
        self.ask_at = vec![address];
        self
    }

    fn ask(&mut self) {
        for &to in &self.ask_at {
            if let Err(err) = self.socket.send_to(QUESTION, (to, self.port)) {
                log::debug!("LAN search to {to}: {err}");
            }
        }
        self.asked = Some(self.elapsed);
    }

    /// Asks and listens; the games found once the search time is up.
    pub fn update(&mut self, elapsed: Duration) -> Option<Vec<LanGame>> {
        self.elapsed += elapsed;
        if self
            .asked
            .is_none_or(|asked| self.elapsed - asked >= ASK_EVERY)
        {
            self.ask();
        }
        let mut buffer = [0u8; 1024];
        while let Ok((length, from)) = self.socket.recv_from(&mut buffer) {
            if let Ok(answer) = serde_json::from_slice::<Answer>(&buffer[..length]) {
                self.note(answer, from);
            }
        }
        (self.elapsed >= SEARCH_TIME).then(|| self.found.iter().map(|(_, g)| g.clone()).collect())
    }

    fn note(&mut self, answer: Answer, from: SocketAddr) {
        if answer.game != self.game {
            return;
        }
        let address = SocketAddr::new(from.ip(), answer.port);
        let game = LanGame {
            name: answer.game,
            version: answer.version,
            address: address.to_string(),
            players: answer.players,
            max_players: answer.max_players,
            has_password: answer.password,
        };
        match self.found.iter_mut().find(|(host, _)| *host == answer.host) {
            // Found again at another address: the network one is the one
            // other computers can join too.
            Some((_, known)) => {
                let known_is_local = known
                    .address
                    .parse::<SocketAddr>()
                    .is_ok_and(|known| known.ip().is_loopback());
                if known_is_local && !from.ip().is_loopback() {
                    *known = game;
                }
            }
            None => self.found.push((answer.host, game)),
        }
    }
}
