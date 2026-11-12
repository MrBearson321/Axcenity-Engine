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

/// The host's side of one connection's handshake.
pub struct Gate {
    cert: Fingerprint,
    versions: Versions,
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
            state: GateState::Waiting,
        }
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
                match access.admit(key, &name, color, invite, now) {
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
