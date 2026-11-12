//! Fuzzing what the host and members read from the network (C10): random
//! bytes, and real messages with bytes changed, cut short or lengthened,
//! through the session decoders and the handshake's JSON. Nothing may
//! panic or ask for absurd amounts of memory; bad bytes are an error.
//!
//! Plain seeded loops, so they run with every `cargo test` (no fuzzing
//! tool to install). `AXCENITY_FUZZ_ROUNDS` runs more.

use crate::files::{FileHash, Piece};
use crate::handshake::Greeting;
use crate::identity::Identity;
use crate::message::{MemberId, ToClient, ToHost};
use crate::op::Transaction;
use crate::presence::{ChatLine, Presence};
use crate::role::Role;
use crate::testing::*;
use crate::wire;

/// xorshift64*: enough randomness for fuzzing, no dependency.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

fn rounds() -> usize {
    std::env::var("AXCENITY_FUZZ_ROUNDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(20_000)
}

/// `bytes`, changed the way a broken or hostile peer might.
fn mutate(rng: &mut Rng, bytes: &[u8]) -> Vec<u8> {
    let mut out = bytes.to_vec();
    for _ in 0..1 + rng.below(4) {
        match rng.below(6) {
            // Flip a byte (the format byte is kept now and then, so the
            // decoder gets past it).
            0 | 1 if !out.is_empty() => {
                let at = 1 + rng.below(out.len().saturating_sub(1));
                if at < out.len() {
                    out[at] = rng.next() as u8;
                }
            }
            // Cut it short.
            2 => out.truncate(rng.below(out.len() + 1)),
            // A huge length or count (postcard varints are 0xff-prefixed).
            3 => {
                let at = rng.below(out.len() + 1);
                let huge = [0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x7f];
                out.splice(at..at, huge);
            }
            // Random bytes in the middle.
            4 => {
                let at = rng.below(out.len() + 1);
                let n = rng.below(16);
                let noise: Vec<u8> = (0..n).map(|_| rng.next() as u8).collect();
                out.splice(at..at, noise);
            }
            _ => {
                let len = out.len();
                if len > 2 {
                    let a = 1 + rng.below(len - 1);
                    let b = 1 + rng.below(len - 1);
                    out.swap(a, b);
                }
            }
        }
    }
    out
}

fn to_host_samples() -> Vec<ToHost> {
    let hash = FileHash::of(b"x");
    let id = axcenity_core::InstanceId::from_bits(1 << 20);
    let world = axcenity_core::World::new();
    vec![
        ToHost::Hello { have: None },
        ToHost::Submit(Transaction {
            client_seq: 7,
            label: "Add".into(),
            ops: vec![
                create(vec![part(id, world.workspace(), "Crate")]),
                rename(id, "Box"),
                set(id, "Transparency", axcenity_core::Value::Number(0.5)),
            ],
        }),
        ToHost::NeedIds,
        ToHost::Presence(Presence::default()),
        ToHost::Upload {
            hash,
            at: 0,
            total: 3,
            bytes: Piece(b"abc".to_vec().into()),
        },
        ToHost::Fetch(vec![hash, hash]),
        ToHost::Lock {
            path: "scripts/a.lua".into(),
        },
        ToHost::Unlock {
            path: "scripts/a.lua".into(),
        },
        ToHost::DocOpen {
            path: "scripts/a.lua".into(),
            state_vector: vec![1, 2, 3],
        },
        ToHost::DocUpdate {
            path: "scripts/a.lua".into(),
            update: vec![0; 40],
        },
        // Big enough to go packed.
        ToHost::DocUpdate {
            path: "scripts/b.lua".into(),
            update: b"local x = 1\n".repeat(4_000),
        },
        ToHost::Chat("hi".into()),
        ToHost::Checkpoint("mine".into()),
        ToHost::Restore(42),
        ToHost::AuditLog,
    ]
}

fn to_client_samples() -> Vec<ToClient> {
    let net = Net::new(axcenity_core::World::new(), &[Role::Owner]);
    let world = net.host.world().clone();
    vec![
        ToClient::Welcome {
            world: crate::message::Snapshot(Box::new(world)),
            graveyard: Default::default(),
            seq: 3,
            last_client_seq: 2,
            role: Role::Editor,
            ids: None,
            rejected: Vec::new(),
            files: Default::default(),
            session: "s".into(),
            epoch: 9,
        },
        ToClient::Chat(ChatLine {
            member: MemberId(1),
            at: 5,
            text: "hi".into(),
        }),
        ToClient::Missing(FileHash::of(b"x")),
        ToClient::HistoryNotice("no".into()),
        ToClient::AuditLog(Vec::new()),
        ToClient::DocUpdate {
            path: "scripts/b.lua".into(),
            update: b"local x = 1\n".repeat(4_000),
            author: MemberId(2),
        },
    ]
}

#[test]
fn bad_bytes_never_panic_the_decoders() {
    let mut rng = Rng(0x5eed_c0ffee);
    let to_host: Vec<Vec<u8>> = to_host_samples().iter().map(wire::encode_to_host).collect();
    let to_client: Vec<Vec<u8>> = to_client_samples()
        .iter()
        .map(wire::encode_to_client)
        .collect();
    // Every sample reads back as it was written.
    for bytes in &to_host {
        wire::decode_to_host(bytes).expect("a sample reads back");
    }
    for bytes in &to_client {
        wire::decode_to_client(bytes).expect("a sample reads back");
    }
    let mut refused = 0;
    for round in 0..rounds() {
        let bytes = if round % 10 == 0 {
            let n = rng.below(64);
            (0..n).map(|_| rng.next() as u8).collect()
        } else {
            let pick = rng.below(to_host.len());
            mutate(&mut rng, &to_host[pick])
        };
        refused += usize::from(wire::decode_to_host(&bytes).is_err());
        let pick = rng.below(to_client.len());
        let _ = wire::decode_to_client(&mutate(&mut rng, &to_client[pick]));
    }
    // The mutations mostly break the message: the decoder says so.
    assert!(refused > rounds() / 2, "{refused} of {} refused", rounds());
}

#[test]
fn bad_greetings_never_panic_the_handshake() {
    let mut rng = Rng(0xfeed_beef);
    let me = Identity::new("Sam", [1, 2, 3]);
    let greetings = [
        Greeting::Hello {
            protocol: 5,
            engine: "0.2.0".into(),
            class_defaults: 1,
            key: me.public_key(),
            name: "Sam".into(),
            color: [1, 2, 3],
            invite: Some("00ff".repeat(8)),
        },
        Greeting::Challenge {
            nonce: "ab".repeat(32),
        },
        Greeting::Proof {
            signature: "cd".repeat(64),
        },
    ];
    let samples: Vec<Vec<u8>> = greetings.iter().map(Greeting::to_bytes).collect();
    let mut access = crate::access::Access::owned_by(me.public_key(), "Sam");
    for _ in 0..rounds() / 4 {
        let pick = rng.below(samples.len());
        let bytes = mutate(&mut rng, &samples[pick]);
        let _ = Greeting::from_bytes(&bytes);
        // A gate fed anything refuses it or asks for the next step.
        let mut gate = crate::handshake::Gate::new(
            crate::identity::Fingerprint([0; 32]),
            crate::handshake::Versions::this(),
        );
        let _ = gate.receive(&bytes, &mut access, crate::access::now());
        let _ = gate.receive(&mutate(&mut rng, &samples[2]), &mut access, 0);
    }
}

#[test]
fn big_messages_go_packed_and_read_back() {
    let update = |bytes: Vec<u8>| ToHost::DocUpdate {
        path: "scripts/a.lua".into(),
        update: bytes,
    };
    // Text-like: packed, much smaller.
    let big = update(b"print('hello')\n".repeat(8_000));
    let bytes = wire::encode_to_host(&big);
    assert_eq!(bytes[0], wire::FORMAT | 0x80);
    assert!(bytes.len() < 4_000, "{} bytes", bytes.len());
    let ToHost::DocUpdate { update: back, .. } = wire::decode_to_host(&bytes).unwrap() else {
        panic!("a DocUpdate")
    };
    assert_eq!(back, b"print('hello')\n".repeat(8_000));
    // Small, or already packed (random): as it is.
    assert_eq!(wire::encode_to_host(&update(vec![7; 100]))[0], wire::FORMAT);
    let mut rng = Rng(42);
    let noise: Vec<u8> = (0..100_000).map(|_| rng.next() as u8).collect();
    assert_eq!(wire::encode_to_host(&update(noise))[0], wire::FORMAT);
}

#[test]
fn a_packed_message_never_unpacks_past_the_limit() {
    let packed = |body: &[u8]| {
        let mut bytes = vec![wire::FORMAT | 0x80];
        bytes.extend(zstd::bulk::compress(body, 1).unwrap());
        bytes
    };
    // A few KiB that would unpack past what a member may send the host.
    let bomb = packed(&vec![0; wire::TO_HOST_MOST + 1]);
    assert!(bomb.len() < 8_000);
    assert!(matches!(
        wire::decode_to_host(&bomb),
        Err(wire::WireError::Unpack)
    ));
    // A frame that doesn't say how big it unpacks.
    let mut sizeless = vec![wire::FORMAT | 0x80];
    let mut encoder = zstd::stream::Encoder::new(&mut sizeless, 1).unwrap();
    encoder.include_contentsize(false).unwrap();
    std::io::Write::write_all(&mut encoder, &[0; 100]).unwrap();
    encoder.finish().unwrap();
    assert!(matches!(
        wire::decode_to_host(&sizeless),
        Err(wire::WireError::Unpack)
    ));
    // Packed bytes that aren't zstd.
    assert!(wire::decode_to_client(&[wire::FORMAT | 0x80, 1, 2, 3]).is_err());
}
