//! What the relay, a host and a player say to each other.
//!
//! **UDP** (the relay's port): packets that start with `AXR` and a kind
//! byte are the relay's own; anything else from a player bound to a game
//! is that player's netcode packet, which the relay passes to the host.
//!
//! **TCP** (the same port number): one JSON line in each direction to
//! start ([`ToRelay`], [`FromRelay`]). A host's connection then stays open
//! for the relay's requests; a player's `Join` and a host's `Data`
//! connection become one byte stream (the token port's TLS, end to end).

use std::io::{self, Read, Write};
use std::net::SocketAddr;

use serde::{Deserialize, Serialize};

/// The relay's TCP and UDP port unless told another.
pub const DEFAULT_RELAY_PORT: u16 = 47810;
/// What every relay packet starts with.
pub const MAGIC: [u8; 3] = *b"AXR";
/// The bytes of a host's secret.
pub const SECRET_BYTES: usize = 16;
/// Letters in a join code.
pub const CODE_LENGTH: usize = 6;
/// Join code letters: no 0, O, 1 or I, which people mix up.
pub const CODE_ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
/// The longest control line read.
pub const MOST_LINE_BYTES: usize = 1024;

const DATA: u8 = 1;
const PUNCH: u8 = 2;
const HOST: u8 = 3;
const BIND: u8 = 4;
const BOUND: u8 = 5;

/// A relay packet.
#[derive(Clone, Debug, PartialEq)]
pub enum Packet<'a> {
    /// Between relay and host: a player's netcode packet, by the number
    /// the relay gave the player.
    Data { player: u32, payload: &'a [u8] },
    /// Between a host and a player, opening a direct path.
    Punch,
    /// From a host's game socket every few seconds: where the relay
    /// reaches it.
    Host { secret: [u8; SECRET_BYTES] },
    /// From a player's game socket: send my packets to this game.
    Bind { code: [u8; CODE_LENGTH] },
    /// The relay's answer to `Bind`.
    Bound(Bound),
}

/// The relay's answer to a player's `Bind`.
#[derive(Clone, Debug, PartialEq)]
pub enum Bound {
    /// Bound: the host's address as the relay sees it (where to punch;
    /// `None` until the host's game socket has reached the relay), and
    /// the player's.
    To {
        host: Option<SocketAddr>,
        you: SocketAddr,
    },
    NoGame,
    Full,
}

impl Packet<'_> {
    /// The packet's bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = MAGIC.to_vec();
        match self {
            Packet::Data { player, payload } => {
                bytes.push(DATA);
                bytes.extend(player.to_le_bytes());
                bytes.extend_from_slice(payload);
            }
            Packet::Punch => bytes.push(PUNCH),
            Packet::Host { secret } => {
                bytes.push(HOST);
                bytes.extend(secret);
            }
            Packet::Bind { code } => {
                bytes.push(BIND);
                bytes.extend(code);
            }
            Packet::Bound(bound) => {
                bytes.push(BOUND);
                match bound {
                    Bound::To { host, you } => {
                        bytes.push(0);
                        put_text(&mut bytes, &host.map(|h| h.to_string()).unwrap_or_default());
                        put_text(&mut bytes, &you.to_string());
                    }
                    Bound::NoGame => bytes.push(1),
                    Bound::Full => bytes.push(2),
                }
            }
        }
        bytes
    }

    /// `bytes` as a relay packet; `None` for anything else (a netcode
    /// packet).
    pub fn read(bytes: &[u8]) -> Option<Packet<'_>> {
        let rest = bytes.strip_prefix(&MAGIC)?;
        let (&kind, rest) = rest.split_first()?;
        match kind {
            DATA => {
                let player = u32::from_le_bytes(rest.get(..4)?.try_into().ok()?);
                Some(Packet::Data {
                    player,
                    payload: &rest[4..],
                })
            }
            PUNCH => Some(Packet::Punch),
            HOST => Some(Packet::Host {
                secret: rest.get(..SECRET_BYTES)?.try_into().ok()?,
            }),
            BIND => Some(Packet::Bind {
                code: rest.get(..CODE_LENGTH)?.try_into().ok()?,
            }),
            BOUND => {
                let (&status, rest) = rest.split_first()?;
                Some(Packet::Bound(match status {
                    0 => {
                        let (host, rest) = take_text(rest)?;
                        let (you, _) = take_text(rest)?;
                        Bound::To {
                            host: if host.is_empty() {
                                None
                            } else {
                                Some(host.parse().ok()?)
                            },
                            you: you.parse().ok()?,
                        }
                    }
                    1 => Bound::NoGame,
                    2 => Bound::Full,
                    _ => return None,
                }))
            }
            _ => None,
        }
    }
}

fn put_text(bytes: &mut Vec<u8>, text: &str) {
    bytes.push(text.len().min(255) as u8);
    bytes.extend(text.bytes().take(255));
}

fn take_text(bytes: &[u8]) -> Option<(&str, &[u8])> {
    let (&len, rest) = bytes.split_first()?;
    let text = std::str::from_utf8(rest.get(..len as usize)?).ok()?;
    Some((text, &rest[len as usize..]))
}

/// What a host or player sends first on a TCP connection.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToRelay {
    /// Host a game; `reclaim` is the code and secret (hex) of this host's
    /// game from before its connection dropped.
    Host {
        game: String,
        #[serde(default)]
        reclaim: Option<(String, String)>,
    },
    /// The host's answer to `Open`: this connection is that player's.
    Data { id: u64, secret: String },
    /// A player: connect me to the token port of the game with this code.
    Join { code: String },
}

/// What the relay sends.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FromRelay {
    /// To a host: its game's join code and secret (hex).
    Hosted { code: String, secret: String },
    /// To a host: open a `Data` connection with this id for a player at
    /// `from` who wants a connect token.
    Open { id: u64, from: SocketAddr },
    /// To a host: a player at `to` is opening a direct path; send it
    /// `Punch` packets from the game socket.
    Punch { to: SocketAddr },
    /// To a player: this connection now reaches the host's token port.
    Joined,
    /// Why not, said to the person.
    Refused { reason: String },
}

/// Writes `message` as one line.
pub fn send_line(stream: &mut impl Write, message: &impl Serialize) -> io::Result<()> {
    let mut bytes = serde_json::to_vec(message).map_err(io::Error::other)?;
    bytes.push(b'\n');
    stream.write_all(&bytes)?;
    stream.flush()
}

/// Reads one line, a byte at a time so nothing after it is taken (the
/// stream goes on as TLS), and parses it.
pub fn read_line<T: for<'de> Deserialize<'de>>(stream: &mut impl Read) -> io::Result<T> {
    let mut line = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        match stream.read(&mut byte)? {
            0 => return Err(io::ErrorKind::UnexpectedEof.into()),
            _ if byte[0] == b'\n' => break,
            _ if line.len() >= MOST_LINE_BYTES => {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "line too long"));
            }
            _ => line.push(byte[0]),
        }
    }
    serde_json::from_slice(&line).map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
}

/// `text` as a join code (`"k7q-m2p"` is `"K7QM2P"`), if it is one.
pub fn normalize_code(text: &str) -> Option<String> {
    let code: String = text
        .chars()
        .filter(|c| *c != '-' && !c.is_whitespace())
        .map(|c| c.to_ascii_uppercase())
        .collect();
    (code.len() == CODE_LENGTH && code.bytes().all(|b| CODE_ALPHABET.contains(&b))).then_some(code)
}

/// A code as people read it: `"K7Q-M2P"`.
pub fn display_code(code: &str) -> String {
    if code.len() == CODE_LENGTH && code.is_ascii() {
        format!("{}-{}", &code[..3], &code[3..])
    } else {
        code.to_owned()
    }
}

/// `bytes` in lowercase hex.
pub fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Hex as bytes, if it is hex.
pub fn from_hex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) || !text.is_ascii() {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).ok())
        .collect()
}
