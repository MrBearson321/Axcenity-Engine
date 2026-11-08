//! The join handshake and the roster messages, as JSON on the
//! reliable-ordered channel.
//!
//! A joining player's first message is [`Message::Hello`]: the network
//! protocol, the game and its version, the player's name and the password.
//! The host answers [`Message::Welcome`] (the player's id and name, and
//! everyone in the game) or [`Message::Refused`] with why, and says so
//! itself. Then [`Message::Joined`] and [`Message::Left`] keep everyone's
//! roster the same. The game itself follows (Phase 64): the player's
//! [`Message::Snapshot`], each frame's [`Message::Update`] (see
//! `replicate`), and [`Message::Remote`] for RemoteEvents and
//! RemoteFunctions both ways.
//!
//! `Hello` and `Refused` must stay readable by every later engine, so a
//! newer one can still say why it refuses.

use serde::{Deserialize, Serialize};

use crate::PROTOCOL_VERSION;
use crate::replicate::Update;
use crate::wire::WireMessage;

/// A player in the game, as every end knows them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Peer {
    /// The host's number for them: 1 is a listen server's own player.
    pub id: u32,
    /// Their Player's name, unique in the game.
    pub name: String,
    /// The name they joined with (their Player's `DisplayName`).
    #[serde(default)]
    pub display_name: String,
    /// Their install's identity (their Player's `UserId`).
    #[serde(default)]
    pub user_id: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Message {
    Hello {
        protocol: u32,
        game: String,
        version: String,
        name: String,
        password: Option<String>,
        /// The player's install's identity (Phase 64); 0 from an engine
        /// before it.
        #[serde(default)]
        user_id: u64,
    },
    Welcome {
        you: Peer,
        players: Vec<Peer>,
    },
    Refused {
        reason: String,
    },
    Joined {
        player: Peer,
    },
    Left {
        id: u32,
    },
    /// The server's world, for a player who just joined, and which of its
    /// Players is theirs (a network ID).
    Snapshot {
        you: u64,
        update: Update,
    },
    /// What changed in the server's world this frame.
    Update {
        update: Update,
    },
    /// A RemoteEvent or RemoteFunction message.
    Remote {
        message: WireMessage,
    },
    /// Every part the player's device now moves besides their character
    /// (Phase 66), as network IDs.
    Ownership {
        parts: Vec<u64>,
    },
    /// Where the server has the player's character: their device moved it
    /// further than it may, or a server Script moved it.
    Correction {
        update: Update,
    },
    /// A chat message the player sent (Phase 67).
    Chat {
        text: String,
    },
    /// A chat message for everyone, from a Player (a network ID), or 0 for
    /// a system message.
    ChatLine {
        player: u64,
        text: String,
    },
    /// A line the player typed in their Developer Console for the
    /// server's commands (Phase 71).
    Console {
        line: String,
    },
    /// A player's `RequestStreamAroundAsync`: send what's around
    /// `position` before the player's teleport (Phase 80).
    StreamAround {
        request: u64,
        position: [f32; 3],
    },
    /// The answer to [`Message::StreamAround`]: what's around it has been
    /// sent ahead of this message.
    StreamedAround {
        request: u64,
    },
}

impl Message {
    pub fn to_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("messages always serialize")
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Message> {
        serde_json::from_slice(bytes).ok()
    }
}

/// Which game this is, as the handshake compares it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameInfo {
    /// The project's name.
    pub name: String,
    /// The game's version (`version` in `axcenity.json`).
    pub version: String,
}

/// Why a host turns a player away, worded for each end.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    /// Sent to the player: their `ConnectionFailed` reason.
    pub to_player: String,
    /// What the host logs.
    pub for_host: String,
}

/// What a host lets in.
pub struct Admission<'a> {
    pub game: &'a GameInfo,
    pub password: Option<&'a str>,
    /// Players in the game now, the host's own among them.
    pub players: usize,
    pub max_players: usize,
}

/// Whether the host takes the player whose first message was `hello`
/// (`None` when it couldn't be read).
pub fn admit(hello: Option<&Message>, admission: &Admission) -> Result<(), Refusal> {
    let Some(Message::Hello {
        protocol,
        game,
        version,
        password,
        ..
    }) = hello
    else {
        return Err(Refusal {
            to_player: "The server didn't understand the join request.".to_owned(),
            for_host: "refused a player whose first message wasn't a join request".to_owned(),
        });
    };
    let ours = admission.game;
    if *protocol != PROTOCOL_VERSION {
        return Err(Refusal {
            to_player: format!(
                "The server uses network protocol {PROTOCOL_VERSION} and you use {protocol}: \
                 play the game with the same Axcenity version."
            ),
            for_host: format!(
                "refused a player using network protocol {protocol}; this server uses \
                 {PROTOCOL_VERSION}"
            ),
        });
    }
    if *game != ours.name {
        return Err(Refusal {
            to_player: format!("The server runs \"{}\", not \"{game}\".", ours.name),
            for_host: format!("refused a player of another game, \"{game}\""),
        });
    }
    if *version != ours.version {
        return Err(Refusal {
            to_player: format!(
                "Version mismatch: the server runs version {} of \"{}\" and you have version \
                 {version}.",
                ours.version, ours.name
            ),
            for_host: format!(
                "refused a player with version {version} of \"{}\": this server runs version {}",
                ours.name, ours.version
            ),
        });
    }
    if let Some(wanted) = admission.password
        && password.as_deref() != Some(wanted)
    {
        let to_player = if password.is_none() {
            "This game needs a password."
        } else {
            "Wrong password."
        };
        return Err(Refusal {
            to_player: to_player.to_owned(),
            for_host: "refused a player with the wrong password".to_owned(),
        });
    }
    if admission.players >= admission.max_players {
        return Err(Refusal {
            to_player: format!(
                "The game is full ({} of {} players).",
                admission.players, admission.max_players
            ),
            for_host: format!(
                "refused a player: the game is full ({} of {})",
                admission.players, admission.max_players
            ),
        });
    }
    Ok(())
}

/// `wanted`, or with a number after it when another player has it already:
/// a second `Player` is `Player2`.
pub fn unique_name<'a>(wanted: &str, taken: impl Iterator<Item = &'a str> + Clone) -> String {
    let wanted = match wanted.trim() {
        "" => "Player",
        name => name,
    };
    let free = |name: &str| !taken.clone().any(|other| other == name);
    if free(wanted) {
        return wanted.to_owned();
    }
    (2..)
        .map(|n| format!("{wanted}{n}"))
        .find(|name| free(name))
        .expect("some number is free")
}
