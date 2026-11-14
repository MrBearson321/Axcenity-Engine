//! `--flood <kind>`: connections that misbehave, to check that a server
//! keeps serving its players through them (multiplayer readiness MP2).
//! None of them loads the game; each is a bare connection over UDP.
//!
//! Each gets a connect token first, as a modified client would, so the
//! server's own checks are what stop it (MP4), except:
//!
//! - `tokenless`: connects without a token, as anyone on the internet
//!   can try. A secured server never answers it, so it holds no slot.
//! - `silent`: connects and never says who it is. The server drops it
//!   after the handshake's time (`HANDSHAKE_TIMEOUT`), freeing the slot.
//! - `junk`: connects and sends bytes that aren't a join request. The
//!   server refuses it and drops it.
//! - `spam`: joins properly, then sends as much junk as the connection
//!   carries on both channels, every frame. The server must keep its frame
//!   rate for everyone else, and kicks it (MP3).
//! - `deep`: joins, then sends a RemoteEvent message whose argument is
//!   tables nested 200,000 deep (600 KB) every second. Before MP3 one of
//!   these overflowed the server's stack.

use std::time::{Duration, Instant};

use anyhow::{Result, bail};
use axcenity_net::{
    Channel, Client, GameInfo, Message, PROTOCOL_VERSION, TokenRequest, Trust, WireArg, WireKind,
    WireMessage, class_defaults,
};

use crate::stats::BotSample;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Flood {
    Tokenless,
    Silent,
    Junk,
    Spam,
    Deep,
}

impl Flood {
    pub fn parse(text: &str) -> Result<Flood> {
        Ok(match text {
            "tokenless" => Flood::Tokenless,
            "silent" => Flood::Silent,
            "junk" => Flood::Junk,
            "spam" => Flood::Spam,
            "deep" => Flood::Deep,
            _ => bail!("--flood takes tokenless, silent, junk, spam or deep, not `{text}`"),
        })
    }
}

/// Junk messages of `SPAM_BYTES` a spamming connection queues each frame:
/// together about what renet sends in a frame (60 KB), so it's always at
/// its limit without overflowing its own reliable channel's memory (which
/// would end the connection on its side).
const SPAM_RELIABLE: usize = 8;
const SPAM_UNRELIABLE: usize = 48;
const SPAM_BYTES: usize = 1024;

pub struct FloodConnection {
    index: usize,
    kind: Flood,
    client: Client,
    hello: Message,
    said: bool,
    started: Instant,
    joined_after: Option<f64>,
    ended: Option<String>,
    /// Seconds from starting until the server closed it.
    ended_after: Option<f64>,
    seed: u64,
    /// `deep`: when to send the next.
    next_deep: f64,
}

impl FloodConnection {
    pub fn start(
        index: usize,
        kind: Flood,
        address: &str,
        game: &GameInfo,
        password: Option<String>,
        user_id: u64,
    ) -> Result<FloodConnection> {
        let client = if kind == Flood::Tokenless {
            Client::connect(address)?
        } else {
            let (game, password) = (game.clone(), password.clone());
            Client::join(
                address,
                move |to| TokenRequest::new(&game, password, to),
                Trust::Any,
            )?
        };
        Ok(FloodConnection {
            index,
            kind,
            client,
            hello: Message::Hello {
                protocol: PROTOCOL_VERSION,
                game: game.name.clone(),
                version: game.version.clone(),
                name: format!("Spam{}", index + 1),
                password,
                user_id,
                defaults: class_defaults(),
                rejoin: 0,
            },
            said: false,
            started: Instant::now(),
            joined_after: None,
            ended: None,
            ended_after: None,
            seed: 0x9e37_79b9_7f4a_7c15 ^ index as u64,
            next_deep: 0.0,
        })
    }

    fn junk(&mut self, bytes: usize) -> Vec<u8> {
        (0..bytes)
            .map(|_| {
                self.seed ^= self.seed << 13;
                self.seed ^= self.seed >> 7;
                self.seed ^= self.seed << 17;
                self.seed as u8
            })
            .collect()
    }

    /// A RemoteEvent message (to a remote that needn't exist) whose
    /// argument is `depth` tables, each inside the last.
    fn nested(depth: usize) -> Vec<u8> {
        let shallow = Message::Remote {
            message: WireMessage {
                remote: 1,
                player: 0,
                kind: WireKind::Event,
                args: vec![WireArg::Nil],
            },
        };
        let mut bytes = shallow.to_bytes();
        // Postcard: the Nil argument (0) is last; each table is its
        // variant (9) and pair count (1), then its key (the next table)
        // and value (nil).
        bytes.pop();
        bytes.extend(std::iter::repeat_n([9, 1], depth).flatten());
        bytes.push(0);
        bytes.extend(std::iter::repeat_n(0, depth));
        bytes
    }

    pub fn frame(&mut self, seconds: f64) {
        if self.ended.is_some() {
            return;
        }
        self.client.update(Duration::from_secs_f64(seconds));
        if let Some(reason) = self.client.ended() {
            self.ended = Some(reason.to_owned());
            self.ended_after = Some(self.started.elapsed().as_secs_f64());
            return;
        }
        while let Some(bytes) = self.client.receive(Channel::ReliableOrdered) {
            if let Some(Message::Welcome { .. }) = Message::from_bytes(&bytes) {
                self.joined_after = Some(self.started.elapsed().as_secs_f64());
            }
        }
        while self.client.receive(Channel::Unreliable).is_some() {}
        if self.client.is_connected() {
            match self.kind {
                Flood::Tokenless | Flood::Silent => {}
                Flood::Junk if !self.said => {
                    let junk = self.junk(64);
                    self.client.send(Channel::ReliableOrdered, junk);
                }
                Flood::Junk => {}
                Flood::Spam | Flood::Deep if !self.said => {
                    self.client
                        .send(Channel::ReliableOrdered, self.hello.to_bytes());
                }
                Flood::Deep => {
                    let joined = self.joined_after.is_some();
                    let elapsed = self.started.elapsed().as_secs_f64();
                    if joined && elapsed >= self.next_deep {
                        self.next_deep = elapsed + 1.0;
                        self.client
                            .send(Channel::ReliableOrdered, Self::nested(200_000));
                    }
                }
                Flood::Spam => {
                    if self.joined_after.is_some() {
                        for _ in 0..SPAM_RELIABLE {
                            let junk = self.junk(SPAM_BYTES);
                            self.client.send(Channel::ReliableOrdered, junk);
                        }
                        for _ in 0..SPAM_UNRELIABLE {
                            let junk = self.junk(SPAM_BYTES);
                            self.client.send(Channel::Unreliable, junk);
                        }
                    }
                }
            }
            self.said = true;
        }
        self.client.flush();
    }

    pub fn is_over(&self) -> bool {
        self.ended.is_some()
    }

    pub fn leave(&mut self) {
        self.client.disconnect();
    }

    pub fn sample(&self) -> BotSample {
        let link = self.client.stats();
        BotSample {
            bot: self.index,
            joined_after: self.joined_after,
            connected: self.ended.is_none() && self.client.is_connected(),
            ended: self.ended.clone(),
            ended_after: self.ended_after,
            rtt_ms: link.rtt * 1000.0,
            packet_loss: link.packet_loss,
            sent_per_second: link.bytes_sent_per_second,
            received_per_second: link.bytes_received_per_second,
            ..BotSample::default()
        }
    }
}
