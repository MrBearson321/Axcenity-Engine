//! Letting someone into a session (TC3; design §5 and §9). Over the TLS
//! connection, the joiner says hello (versions, public key, name, maybe an
//! invite); the host checks the versions and sends a random challenge; the
//! joiner signs it together with the host certificate's fingerprint, which
//! ties the signature to this connection; the host checks the signature
//! and the member list, and admits or refuses with a sentence both sides
//! understand. Only then do session messages ([`wire`](crate::wire)) flow.
//!
//! These messages are JSON, so a later version can always read a hello or
//! a refusal and say why it refuses.

use serde::{Deserialize, Serialize};

use crate::access::{Access, Member};
use crate::identity::{self, Fingerprint, IdentityProvider, PublicKey};
use crate::message::MemberId;
use crate::role::Role;

/// Raise it when a handshake or session message changes.
pub const PROTOCOL: u32 = 5;

/// This engine's version, as the handshake compares it.
pub const ENGINE: &str = env!("CARGO_PKG_VERSION");

/// What a signature covers, so a key's Collaborate signature can't be
/// taken for anything else.
const SIGNED: &[u8] = b"axcenity team create sign-in v1";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Greeting {
    Hello {
        protocol: u32,
        engine: String,
        class_defaults: u64,
        key: PublicKey,
        name: String,
        color: [u8; 3],
        /// The invite's secret, in hex, when joining for the first time.
        invite: Option<String>,
    },
    Challenge {
        nonce: String,
    },
    Proof {
        signature: String,
    },
    Admitted {
        member: MemberId,
        role: Role,
    },
    Refused {
        reason: String,
    },
}

impl Greeting {
    pub fn to_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("a greeting is plain data")
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Greeting> {
        serde_json::from_slice(bytes).ok()
    }
}

/// What both ends must agree on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Versions {
    pub protocol: u32,
    pub engine: String,
    pub class_defaults: u64,
}

impl Versions {
    /// This build's.
    pub fn this() -> Versions {
        Versions {
            protocol: PROTOCOL,
            engine: ENGINE.to_owned(),
            class_defaults: axcenity_net::class_defaults(),
        }
    }

    /// Why a joiner with `theirs` can't join a host with `self`, worded
    /// for the joiner.
    fn refuse(&self, theirs: &Versions) -> Option<String> {
        if theirs.engine != self.engine {
            return Some(format!(
                "The host runs Axcenity {} and you run {}: use the same version (Settings > Updates).",
                self.engine, theirs.engine
            ));
        }
        if theirs.protocol != self.protocol || theirs.class_defaults != self.class_defaults {
            return Some(
                "The host's Axcenity was built from different source than yours: use the same build."
                    .to_owned(),
            );
        }
        None
    }
}

fn signed(nonce: &[u8], cert: &Fingerprint) -> Vec<u8> {
    [SIGNED, nonce, &cert.0].concat()
}

/// Who the host lets in without asking the person hosting (cohesion
/// pass CW7.2). Whoever is let in still needs a valid invite or to be a
/// member already; asking only adds the host's say.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Trust {
    /// A valid invite lets someone in, and they're a member from then on.
    #[default]
    InviteHolders,
    /// Members rejoin without asking; someone joining with an invite for
    /// the first time waits until the host lets them in.
    AskNewMembers,
    /// Every join waits until the host lets them in.
    AskEveryone,
}

impl Trust {
    pub const ALL: [Trust; 3] = [
        Trust::InviteHolders,
        Trust::AskNewMembers,
        Trust::AskEveryone,
    ];

    /// Whether a join by a member (`known`) or a newcomer waits for the host.
    pub fn asks(self, known: bool) -> bool {
        match self {
            Trust::InviteHolders => false,
            Trust::AskNewMembers => !known,
            Trust::AskEveryone => true,
        }
    }
}

/// Someone waiting for the host to let them in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Asking {
    pub name: String,
    pub key: PublicKey,
    /// The role they'd have: a member's own, or the invite's.
    pub role: Role,
    /// True for a member rejoining, false for someone new.
    pub member: bool,
}

/// The host's side of one connection's handshake.
pub struct Gate {
    cert: Fingerprint,
    versions: Versions,
    trust: Trust,
    state: GateState,
}

enum GateState {
    Waiting,
    Challenged {
        nonce: [u8; 32],
        key: PublicKey,
        name: String,
        color: [u8; 3],
        invite: Option<[u8; 16]>,
    },
    /// Signed in; waiting for the host's answer ([`Gate::answer`]).
    Asking {
        key: PublicKey,
        name: String,
        color: [u8; 3],
        invite: Option<[u8; 16]>,
    },
    Done,
}

/// What the host does after a handshake message.
#[derive(Debug)]
pub enum GateStep {
    /// Send this and wait for the next.
    Reply(Vec<u8>),
    /// Let them in: send `reply`, then session messages.
    Admit { member: Member, reply: Vec<u8> },
    /// Send this and close the connection.
    Refuse { reason: String, reply: Vec<u8> },
    /// Ask the person hosting; send nothing until [`Gate::answer`].
    Ask(Asking),
}

fn admit(
    access: &mut Access,
    key: PublicKey,
    name: &str,
    color: [u8; 3],
    invite: Option<[u8; 16]>,
    now: u64,
) -> GateStep {
    match access.admit(key, name, color, invite, now) {
        Ok(member) => GateStep::Admit {
            reply: Greeting::Admitted {
                member: member.id,
                role: member.role,
            }
            .to_bytes(),
            member,
        },
        Err(why) => refuse(format!("{why}")),
    }
}

fn refuse(reason: impl Into<String>) -> GateStep {
    let reason = reason.into();
    GateStep::Refuse {
        reply: Greeting::Refused {
            reason: reason.clone(),
        }
        .to_bytes(),
        reason,
    }
}

impl Gate {
    /// For a connection on the host whose certificate has fingerprint
    /// `cert`.
    pub fn new(cert: Fingerprint, versions: Versions) -> Gate {
        Gate {
            cert,
            versions,
            trust: Trust::default(),
            state: GateState::Waiting,
        }
    }

    /// Asks the person hosting before letting in whom `trust` says.
    pub fn with_trust(mut self, trust: Trust) -> Gate {
        self.trust = trust;
        self
    }

    /// True while the joiner waits for the host's answer.
    pub fn is_asking(&self) -> bool {
        matches!(self.state, GateState::Asking { .. })
    }

    /// The host's answer to an [`GateStep::Ask`]: lets them in (if their
    /// invite still works) or turns them away.
    pub fn answer(&mut self, allow: bool, access: &mut Access, now: u64) -> GateStep {
        let state = std::mem::replace(&mut self.state, GateState::Done);
        let GateState::Asking {
            key,
            name,
            color,
            invite,
        } = state
        else {
            return refuse("That isn't how joining a Collaborate session starts.");
        };
        if !allow {
            return refuse("The host didn't let you in.");
        }
        admit(access, key, &name, color, invite, now)
    }

    pub fn receive(&mut self, bytes: &[u8], access: &mut Access, now: u64) -> GateStep {
        let state = std::mem::replace(&mut self.state, GateState::Done);
        match (state, Greeting::from_bytes(bytes)) {
            (
                GateState::Waiting,
                Some(Greeting::Hello {
                    protocol,
                    engine,
                    class_defaults,
                    key,
                    name,
                    color,
                    invite,
                }),
            ) => {
                let theirs = Versions {
                    protocol,
                    engine,
                    class_defaults,
                };
                if let Some(reason) = self.versions.refuse(&theirs) {
                    return refuse(reason);
                }
                let invite = match invite.map(|hex| identity::from_hex(&hex)) {
                    None => None,
                    Some(Some(bytes)) if bytes.len() == 16 => bytes.try_into().ok(),
                    Some(_) => return refuse("The invite code is damaged: copy the whole link."),
                };
                let nonce = identity::random::<32>();
                self.state = GateState::Challenged {
                    nonce,
                    key,
                    name: crate::access::tidy_name(&name),
                    color,
                    invite,
                };
                GateStep::Reply(
                    Greeting::Challenge {
                        nonce: identity::to_hex(&nonce),
                    }
                    .to_bytes(),
                )
            }
            (
                GateState::Challenged {
                    nonce,
                    key,
                    name,
                    color,
                    invite,
                },
                Some(Greeting::Proof { signature }),
            ) => {
                let signature = identity::from_hex(&signature).unwrap_or_default();
                if !key.verifies(&signed(&nonce, &self.cert), &signature) {
                    return refuse("Signing in failed: the key didn't match.");
                }
                // Only someone the access rules would let in is asked
                // about, so a bad invite is turned away at once.
                let role = match access.would_admit(&key, invite, now) {
                    Ok(role) => role,
                    Err(why) => return refuse(format!("{why}")),
                };
                let member = access.member(&key).is_some();
                if self.trust.asks(member) {
                    self.state = GateState::Asking {
                        key,
                        name: name.clone(),
                        color,
                        invite,
                    };
                    return GateStep::Ask(Asking {
                        name,
                        key,
                        role,
                        member,
                    });
                }
                admit(access, key, &name, color, invite, now)
            }
            _ => refuse("That isn't how joining a Collaborate session starts."),
        }
    }
}

/// The joiner's side of the handshake.
pub struct Knock {
    cert: Fingerprint,
}

/// What the joiner does after a handshake message.
#[derive(Debug, PartialEq, Eq)]
pub enum KnockStep {
    Reply(Vec<u8>),
    Admitted { member: MemberId, role: Role },
    Refused(String),
}

impl Knock {
    /// For a host whose certificate has fingerprint `cert` (the TLS
    /// connection has checked it). Returns the hello to send first.
    pub fn new(
        identity: &dyn IdentityProvider,
        cert: Fingerprint,
        invite: Option<[u8; 16]>,
        versions: &Versions,
    ) -> (Knock, Vec<u8>) {
        let hello = Greeting::Hello {
            protocol: versions.protocol,
            engine: versions.engine.clone(),
            class_defaults: versions.class_defaults,
            key: identity.public_key(),
            name: identity.display_name().to_owned(),
            color: identity.display_color(),
            invite: invite.map(|secret| identity::to_hex(&secret)),
        };
        (Knock { cert }, hello.to_bytes())
    }

    pub fn receive(&mut self, bytes: &[u8], identity: &dyn IdentityProvider) -> KnockStep {
        match Greeting::from_bytes(bytes) {
            Some(Greeting::Challenge { nonce }) => {
                let nonce = identity::from_hex(&nonce).unwrap_or_default();
                let signature = identity.sign(&signed(&nonce, &self.cert));
                KnockStep::Reply(
                    Greeting::Proof {
                        signature: identity::to_hex(&signature),
                    }
                    .to_bytes(),
                )
            }
            Some(Greeting::Admitted { member, role }) => KnockStep::Admitted { member, role },
            Some(Greeting::Refused { reason }) => KnockStep::Refused(reason),
            _ => KnockStep::Refused("The host answered with something unexpected.".to_owned()),
        }
    }
}
