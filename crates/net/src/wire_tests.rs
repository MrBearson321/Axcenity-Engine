//! Roadmap M7: the binary wire (postcard, JSON for debugging, the greetings
//! every engine reads) and replication that skips parts whose only change
//! is their motion.

use axcenity_core::{ClassName, Color3, Value, Vector3, World};

use crate::handshake::Message;
use crate::replicate::{Op, Replicator, Update, WireChange, WireInstance};
use crate::wire::{WireArg, WireKind, WireMessage, WireValue};
use crate::{LevelState, Peer, WireAnimator, WireParameter};

/// One of every message, with something in each field.
fn every_message() -> Vec<Message> {
    let update = Update {
        ops: vec![
            Op::Create(WireInstance {
                id: 7,
                class: "Part".to_owned(),
                name: "Brick".to_owned(),
                parent: 3,
                properties: vec![
                    ("Size".to_owned(), WireValue::Vector3([4.0, 1.0, 2.0])),
                    ("Anchored".to_owned(), WireValue::Bool(true)),
                ],
                attributes: vec![("Health".to_owned(), WireValue::Number(100.0))],
                tags: vec!["Lava".to_owned()],
            }),
            Op::Change(WireChange {
                id: 7,
                name: Some("Renamed".to_owned()),
                parent: Some(4),
                properties: vec![("Color".to_owned(), WireValue::Color3([1.0, 0.5, 0.25]))],
                attributes: vec![("Health".to_owned(), None)],
                tags_added: vec!["Hot".to_owned()],
                tags_removed: vec!["Lava".to_owned()],
            }),
            Op::Change(WireChange {
                id: 8,
                ..WireChange::default()
            }),
            Op::Destroy { id: 9 },
        ],
        time: 12.5,
        levels: Some(LevelState {
            current: "Lobby".to_owned(),
            added: vec![("Arena".to_owned(), 11)],
        }),
        animators: vec![WireAnimator {
            id: 12,
            parameters: vec![
                ("Running".to_owned(), WireParameter::Bool(true)),
                ("Jump".to_owned(), WireParameter::Trigger),
            ],
            layers: vec![("Upper".to_owned(), 1.0, 0.2)],
        }],
        streamed_out: vec![13],
        streamed_in: vec![14, 15],
    };
    let peer = Peer {
        id: 2,
        name: "Player2".to_owned(),
        display_name: "Player".to_owned(),
        user_id: 42,
    };
    vec![
        Message::Hello {
            protocol: crate::PROTOCOL_VERSION,
            game: "Castle".to_owned(),
            version: "1.0".to_owned(),
            name: "Ann".to_owned(),
            password: Some("moat".to_owned()),
            user_id: 42,
        },
        Message::Welcome {
            you: peer.clone(),
            players: vec![peer.clone()],
        },
        Message::Refused {
            reason: "Wrong password.".to_owned(),
        },
        Message::Joined { player: peer },
        Message::Left { id: 2 },
        Message::Snapshot {
            you: 5,
            update: update.clone(),
        },
        Message::Update {
            update: update.clone(),
        },
        Message::Remote {
            message: WireMessage {
                remote: 20,
                player: 5,
                kind: WireKind::Reply {
                    call: 3,
                    error: Some("oops".to_owned()),
                },
                args: vec![
                    WireArg::Nil,
                    WireArg::Integer(-3),
                    WireArg::String(b"bytes \xff".to_vec()),
                    WireArg::Table(vec![(WireArg::Number(1.5), WireArg::Instance(7))]),
                ],
            },
        },
        Message::Ownership { parts: vec![7, 8] },
        Message::Correction { update },
        Message::Chat {
            text: "hi".to_owned(),
        },
        Message::ChatLine {
            player: 0,
            text: "welcome".to_owned(),
        },
        Message::Console {
            line: "kick Ann".to_owned(),
        },
        Message::StreamAround {
            request: 1,
            position: [1.0, 2.0, 3.0],
        },
        Message::StreamedAround { request: 1 },
    ]
}

#[test]
fn every_message_crosses_the_wire_unchanged_as_postcard_and_as_json() {
    for message in every_message() {
        for json in [false, true] {
            let bytes = message.encode(json);
            assert_eq!(
                Message::from_bytes(&bytes).as_ref(),
                Some(&message),
                "json: {json}, {bytes:?}"
            );
        }
    }
}

#[test]
fn hello_and_refused_stay_the_json_every_engine_reads() {
    // As an engine before roadmap M7 (protocol 9) sent them.
    let old_hello = br#"{"type":"Hello","protocol":9,"game":"Castle","version":"1.0","name":"Ann","password":null,"user_id":42}"#;
    assert_eq!(
        Message::from_bytes(old_hello),
        Some(Message::Hello {
            protocol: 9,
            game: "Castle".to_owned(),
            version: "1.0".to_owned(),
            name: "Ann".to_owned(),
            password: None,
            user_id: 42,
        })
    );
    // From before Phase 64: no user id.
    let older = br#"{"type":"Hello","protocol":1,"game":"Castle","version":"1.0","name":"Ann","password":null}"#;
    assert!(matches!(
        Message::from_bytes(older),
        Some(Message::Hello { user_id: 0, .. })
    ));
    // What this engine sends, an older one reads the same way.
    for json in [false, true] {
        let refused = Message::Refused {
            reason: "Wrong password.".to_owned(),
        }
        .encode(json);
        assert_eq!(
            String::from_utf8(refused).unwrap(),
            r#"{"type":"Refused","reason":"Wrong password."}"#
        );
        let hello = every_message().remove(0).encode(json);
        let text = String::from_utf8(hello).unwrap();
        assert!(text.starts_with(r#"{"type":"Hello","protocol":"#), "{text}");
    }
    // Anything else from an older engine isn't understood (its protocol
    // is refused before that matters).
    assert_eq!(Message::from_bytes(br#"{"type":"Left","id":2}"#), None);
    assert_eq!(Message::from_bytes(&[]), None);
    assert_eq!(Message::from_bytes(&[9, 1, 2]), None);
}

/// A world of `count` parts in the Workspace.
fn parts(count: usize) -> World {
    let mut world = World::new();
    for index in 0..count {
        let part = world.create(ClassName::Part).unwrap();
        world.set_name(part, format!("Part{index}")).unwrap();
        world
            .set_property(
                part,
                "Position",
                Value::Vector3(Vector3::new(index as f32, 2.0, 0.0)),
            )
            .unwrap();
        world
            .set_attribute(part, "Index", Some(Value::Number(index as f64)))
            .unwrap();
        world.add_tag(part, "Brick").unwrap();
        world.set_parent(part, Some(world.workspace())).unwrap();
    }
    world
}

#[test]
fn a_snapshot_in_postcard_is_far_smaller_than_in_json() {
    let world = parts(500);
    let mut replicator = Replicator::default();
    replicator.frame(&world);
    let snapshot = Message::Snapshot {
        you: 1,
        update: replicator.snapshot(&world),
    };
    let postcard = snapshot.encode(false).len();
    let json = snapshot.encode(true).len();
    assert!(
        postcard * 2 < json,
        "postcard {postcard} bytes, JSON {json}"
    );
    assert_eq!(snapshot.to_bytes().len(), postcard);
}

#[test]
fn a_part_that_only_moves_sends_no_change_but_its_other_changes_still_go() {
    let mut world = parts(3);
    let ids: Vec<_> = world
        .get(world.workspace())
        .unwrap()
        .children()
        .iter()
        .copied()
        .filter(|&id| world.get(id).unwrap().class() == ClassName::Part)
        .collect();
    assert_eq!(ids.len(), 3);
    let mut replicator = Replicator::default();
    replicator.frame(&world);
    assert!(replicator.frame(&world).is_empty());

    // Physics moves every part: motion goes on its own, not in updates.
    for (step, &id) in ids.iter().enumerate() {
        for name in ["Position", "AssemblyLinearVelocity"] {
            world
                .set_property(
                    id,
                    name,
                    Value::Vector3(Vector3::new(step as f32, 9.0, 1.0)),
                )
                .unwrap();
        }
        world
            .set_property(
                id,
                "Orientation",
                Value::Vector3(Vector3::new(0.0, 45.0, 0.0)),
            )
            .unwrap();
    }
    let instance = world.get(ids[0]).unwrap();
    assert_ne!(instance.revision(), instance.revision_without_motion());
    assert!(replicator.frame(&world).is_empty());

    // A moving part that also changes color, an attribute and a tag sends
    // only those.
    world
        .set_property(
            ids[1],
            "Position",
            Value::Vector3(Vector3::new(5.0, 5.0, 5.0)),
        )
        .unwrap();
    world
        .set_property(ids[1], "Color", Value::Color3(Color3::new(1.0, 0.0, 0.0)))
        .unwrap();
    world.set_attribute(ids[1], "Index", None).unwrap();
    world
        .set_attribute(ids[1], "Hot", Some(Value::Bool(true)))
        .unwrap();
    world.remove_tag(ids[1], "Brick").unwrap();
    world.add_tag(ids[1], "Lava").unwrap();
    world.set_name(ids[1], "Lava").unwrap();
    let update = replicator.frame(&world);
    assert_eq!(
        update.ops,
        vec![Op::Change(WireChange {
            id: ids[1].to_bits(),
            name: Some("Lava".to_owned()),
            parent: None,
            properties: vec![("Color".to_owned(), WireValue::Color3([1.0, 0.0, 0.0]))],
            attributes: vec![
                ("Hot".to_owned(), Some(WireValue::Bool(true))),
                ("Index".to_owned(), None),
            ],
            tags_added: vec!["Lava".to_owned()],
            tags_removed: vec!["Brick".to_owned()],
        })]
    );
    assert!(replicator.frame(&world).is_empty());
}
