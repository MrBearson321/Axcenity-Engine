//! The join handshake and the roster messages, as JSON on the
//! reliable-ordered channel.
//!
//! A joining player's first message is [`Message::Hello`]: the network
//! protocol, the game and its version, the player's name and the password.
//! The host answers [`Message::Welcome`] (the player's id and name, and
//! everyone in the game) or [`Message::Refused`] with why, and says so
//! itself. Then [`Message::Joined`] and [`Message::Left`] keep everyone's
//! `Players` the same.
//!
//! `Hello` and `Refused` must stay readable by every later engine, so a
//! newer one can still say why it refuses.

use serde::{Deserialize, Serialize};

use crate::PROTOCOL_VERSION;

/// A player in the game, as every end knows them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Peer {
    /// The host's number for them: 1 is a listen server's own player.
    pub id: u32,
    /// Their Player's name, unique in the game.
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Message {
    Hello {
        protocol: u32,
        game: String,
        version: String,
        name: String,
        password: Option<String>,
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
