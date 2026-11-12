//! Collaborate over real sockets on this computer (TC3): joining with an
//! invite, the handshake's refusals, editing together, and dropped
//! connections. Each test has its own host on a free port.

use std::sync::Arc;
use std::time::{Duration, Instant};

use axcenity_core::{ClassName, Value, World};

use crate::access::{self, Access, INVITE_LIFETIME_SECONDS, Invite};
use crate::client::{Client, Notice};
use crate::handshake::{Greeting, Trust, Versions};
use crate::host::Host;
use crate::identity::{self, Fingerprint, Identity};
use crate::role::Role;
use crate::session::{HostSession, Hosting, Remote, RemoteEvent};
use crate::socket::{HostCert, Link, LinkEvent};
use crate::testing::{dump, dump_doc, dump_files};

/// How long a test waits for the network before failing.
const PATIENCE: Duration = Duration::from_secs(20);

/// A world with Parts "A" and "B" in the Workspace.
fn sample() -> World {
    let mut world = World::new();
    let workspace = world.workspace();
    for name in ["A", "B"] {
        let id = world.create(ClassName::Part).unwrap();
        world.set_name(id, name).unwrap();
        world.set_parent(id, Some(workspace)).unwrap();
    }
    world
}

/// A session owned by `owner`, served on its own thread.
fn host(owner: &Identity, access: Option<Access>) -> (Hosting, HostCert) {
    let cert = HostCert::generate();
    let access = access.unwrap_or_else(|| Access::owned_by(owner.public_key(), &owner.name));
    let session = HostSession::start(
        Host::new(sample()),
        access,
        None,
        &cert,
        "127.0.0.1:0".parse().unwrap(),
        None,
    )
    .unwrap();
    (session.serve(), cert)
}

/// One member's editor: its connection, client and World.
struct Member {
    remote: Remote,
    client: Option<Client>,
    world: World,
    refused: Option<String>,
    notices: Vec<Notice>,
    lost: usize,
}

impl Member {
    fn new(remote: Remote) -> Member {
        Member {
            remote,
            client: None,
            world: World::new(),
            refused: None,
            notices: Vec::new(),
            lost: 0,
        }
    }

    /// The owner, joining over loopback without an invite.
    fn owner(hosting: &Hosting, cert: &HostCert, identity: Arc<Identity>) -> Member {
        let address = hosting.lock().loopback();
        Member::new(Remote::join(address, cert.fingerprint(), None, identity))
    }

    fn invited(invite: &Invite, name: &str) -> Member {
        let identity = Arc::new(Identity::new(name, [1, 2, 3]));
        Member::new(Remote::join_invite(&loopback(invite), identity))
    }

    /// One editor frame: what arrived, then what to send.
    fn step(&mut self) {
        for event in self.remote.poll() {
            match event {
                RemoteEvent::Admitted { member, .. } => {
                    let client = self.client.get_or_insert_with(|| Client::new(member));
                    assert_eq!(client.member(), member, "the same member on every join");
                    client.connect();
                }
                RemoteEvent::Message(message) => {
                    let client = self.client.as_mut().expect("admitted first");
                    client.receive(&mut self.world, message);
                }
                RemoteEvent::Lost { .. } => {
                    self.lost += 1;
                    if let Some(client) = &mut self.client {
                        client.disconnected();
                    }
                }
                RemoteEvent::Refused(reason) => self.refused = Some(reason),
            }
        }
        if let Some(client) = &mut self.client {
            self.remote.send(client.take_outbox());
            self.notices.extend(client.take_notices());
        }
    }

    fn client(&self) -> &Client {
        self.client.as_ref().expect("joined")
    }

    /// A change made straight on the World, then sent as one step.
    fn edit(&mut self, label: &str, change: impl FnOnce(&mut World)) {
        change(&mut self.world);
        let client = self.client.as_mut().expect("joined");
        client.capture(&mut self.world, label);
        self.remote.send(client.take_outbox());
    }

    fn drop_connection(&mut self) {
        let _ = self.remote.drop_connection();
        self.lost += 1;
        if let Some(client) = &mut self.client {
            client.disconnected();
        }
    }
}

/// The invite, naming the loopback address (the host names its LAN one).
fn loopback(invite: &Invite) -> Invite {
    let port = invite.host.rsplit(':').next().unwrap();
    Invite {
        host: format!("127.0.0.1:{port}"),
        ..invite.clone()
    }
}

/// Steps everyone until `done`, or fails after [`PATIENCE`].
fn until(members: &mut [&mut Member], what: &str, mut done: impl FnMut(&[&mut Member]) -> bool) {
    let start = Instant::now();
    loop {
        for member in members.iter_mut() {
            member.step();
        }
        if done(members) {
            return;
        }
        assert!(start.elapsed() < PATIENCE, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Steps until every member has joined and has everything the host has,
/// then checks every copy is the host's.
fn converge(hosting: &Hosting, members: &mut [&mut Member]) {
    until(members, "everyone to catch up", |members| {
        let seq = hosting.lock().host().seq();
        members.iter().all(|m| {
            m.client
                .as_ref()
                .is_some_and(|c| c.is_online() && c.pending() == 0 && c.seq() == seq)
        })
    });
    let host = dump_doc(hosting.lock().host().doc());
    for (i, member) in members.iter().enumerate() {
        let copy =
            dump(&member.world, member.client().graveyard()) + &dump_files(member.client().files());
        assert_eq!(copy, host, "member {i}'s copy differs from the host's");
    }
}

fn refused(member: &mut Member) -> String {
    until(&mut [member], "a refusal", |m| m[0].refused.is_some());
    assert!(member.remote.is_refused());
    member.refused.clone().unwrap()
}

fn part_named(world: &World, name: &str) -> bool {
    world
        .descendants(world.root())
        .into_iter()
        .any(|id| world.get(id).is_some_and(|i| i.name() == name))
}

fn add_part(name: &'static str) -> impl FnOnce(&mut World) {
    move |world| {
        let workspace = world.workspace();
        let id = world.create(ClassName::Part).unwrap();
        world.set_name(id, name).unwrap();
        world.set_parent(id, Some(workspace)).unwrap();
    }
}

#[test]
fn an_invited_member_joins_and_edits_reach_everyone() {
    let owner_id = Arc::new(Identity::new("Paul", [200, 80, 40]));
    let (hosting, cert) = host(&owner_id, None);
    let mut owner = Member::owner(&hosting, &cert, owner_id);
    let invite = hosting.lock().invite(Role::Editor, INVITE_LIFETIME_SECONDS);
    assert_eq!(invite.cert, cert.fingerprint(), "the link pins the host");
    let mut sam = Member::invited(&Invite::parse(&invite.link()).unwrap(), "Sam");
    converge(&hosting, &mut [&mut owner, &mut sam]);
    assert_eq!(owner.client().role(), Role::Owner);
    assert_eq!(sam.client().role(), Role::Editor);
    assert_eq!(hosting.lock().access().members().len(), 2);

    owner.edit("Insert Part", add_part("Wall"));
    sam.edit("Insert Part", add_part("Door"));
    converge(&hosting, &mut [&mut owner, &mut sam]);
    for member in [&owner, &sam] {
        assert!(part_named(&member.world, "Wall") && part_named(&member.world, "Door"));
    }
    assert_eq!(hosting.lock().connected().len(), 2);
}

#[test]
fn a_dropped_connection_reconnects_and_sends_what_was_made_meanwhile() {
    let owner_id = Arc::new(Identity::new("Paul", [200, 80, 40]));
    let (hosting, cert) = host(&owner_id, None);
    let mut owner = Member::owner(&hosting, &cert, owner_id);
    let invite = hosting.lock().invite(Role::Editor, INVITE_LIFETIME_SECONDS);
    let mut sam = Member::invited(&invite, "Sam");
    converge(&hosting, &mut [&mut owner, &mut sam]);

    sam.drop_connection();
    // Sam keeps working offline while the owner edits too.
    sam.edit("Insert Part", add_part("Offline"));
    owner.edit("Insert Part", add_part("Online"));
    assert_eq!(sam.client().pending(), 1, "kept until the host has it");
    converge(&hosting, &mut [&mut owner, &mut sam]);
    assert!(part_named(&owner.world, "Offline"));
    assert!(part_named(&sam.world, "Online"));
    // The invite was used once; reconnecting used the key alone.
    assert_eq!(hosting.lock().access().members().len(), 2);
}

#[test]
fn the_same_member_connecting_twice_keeps_the_newest_connection() {
    let owner_id = Arc::new(Identity::new("Paul", [200, 80, 40]));
    let (hosting, cert) = host(&owner_id, None);
    let mut first = Member::owner(&hosting, &cert, owner_id.clone());
    converge(&hosting, &mut [&mut first]);
    let mut second = Member::owner(&hosting, &cert, owner_id);
    converge(&hosting, &mut [&mut second]);
    // The first connection is closed by the host; it notices and retries,
    // which takes the member back from the second.
    until(&mut [&mut first], "the old connection to close", |m| {
        m[0].lost > 0
    });
    assert_eq!(hosting.lock().connected().len(), 1);
}

#[test]
fn many_edits_from_three_members_with_drops_end_the_same_everywhere() {
    let owner_id = Arc::new(Identity::new("Paul", [200, 80, 40]));
    let (hosting, cert) = host(&owner_id, None);
    let mut members = vec![Member::owner(&hosting, &cert, owner_id)];
    for name in ["Sam", "Ana"] {
        let invite = hosting.lock().invite(Role::Editor, INVITE_LIFETIME_SECONDS);
        members.push(Member::invited(&invite, name));
    }
    {
        let mut all: Vec<&mut Member> = members.iter_mut().collect();
        converge(&hosting, &mut all);
    }
    // Everyone edits the same two Parts and adds their own, while one of
    // them keeps losing their connection.
    for round in 0..30 {
        for (i, member) in members.iter_mut().enumerate() {
            let value = Value::Number((round * 3 + i) as f64 / 100.0);
            let label = format!("Edit {round}");
            member.edit(&label, |world| {
                let workspace = world.workspace();
                let shared: Vec<_> = world.get(workspace).unwrap().children().to_vec();
                for id in shared.iter().take(2) {
                    world
                        .set_known_property(*id, "Transparency", value.clone())
                        .unwrap();
                }
                if round % 5 == i {
                    let id = world.create(ClassName::Part).unwrap();
                    world.set_name(id, format!("Round {round} by {i}")).unwrap();
                    world.set_parent(id, Some(workspace)).unwrap();
                }
            });
            member.step();
        }
        if round % 10 == 4 {
            members[2].drop_connection();
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    let mut all: Vec<&mut Member> = members.iter_mut().collect();
    converge(&hosting, &mut all);
    for i in 0..3 {
        assert!(part_named(&members[0].world, &format!("Round {i} by {i}")));
    }
}

#[test]
fn a_viewer_sees_edits_but_cant_make_them_until_made_an_editor() {
    let owner_id = Arc::new(Identity::new("Paul", [200, 80, 40]));
    let (hosting, cert) = host(&owner_id, None);
    let mut owner = Member::owner(&hosting, &cert, owner_id);
    let invite = hosting.lock().invite(Role::Viewer, INVITE_LIFETIME_SECONDS);
    let mut viewer = Member::invited(&invite, "Kim");
    converge(&hosting, &mut [&mut owner, &mut viewer]);
    assert_eq!(viewer.client().role(), Role::Viewer);

    viewer.edit("Insert Part", add_part("Sneaky"));
    converge(&hosting, &mut [&mut owner, &mut viewer]);
    assert!(!part_named(&owner.world, "Sneaky"));
    assert!(!part_named(&viewer.world, "Sneaky"), "taken off again");
    assert!(
        viewer
            .notices
            .iter()
            .any(|n| matches!(n, Notice::Refused { .. })),
        "the viewer is told: {:?}",
        viewer.notices
    );

    let member = viewer.client().member();
    hosting.lock().set_role(member, Role::Editor);
    until(&mut [&mut viewer], "the new role", |m| {
        m[0].client().role() == Role::Editor
    });
    viewer.edit("Insert Part", add_part("Allowed"));
    converge(&hosting, &mut [&mut owner, &mut viewer]);
    assert!(part_named(&owner.world, "Allowed"));
}

#[test]
fn a_removed_member_is_disconnected_and_cant_come_back() {
    let owner_id = Arc::new(Identity::new("Paul", [200, 80, 40]));
    let (hosting, _cert) = host(&owner_id, None);
    let invite = hosting.lock().invite(Role::Editor, INVITE_LIFETIME_SECONDS);
    let mut sam = Member::invited(&invite, "Sam");
    converge(&hosting, &mut [&mut sam]);
    let member = sam.client().member();
    hosting.lock().remove(member);
    let reason = refused(&mut sam);
    assert!(reason.contains("aren't a member"), "{reason}");
    assert!(sam.lost > 0);
}

#[test]
fn an_invite_works_once() {
    let owner_id = Arc::new(Identity::new("Paul", [200, 80, 40]));
    let (hosting, _cert) = host(&owner_id, None);
    let invite = hosting.lock().invite(Role::Editor, INVITE_LIFETIME_SECONDS);
    let mut sam = Member::invited(&invite, "Sam");
    converge(&hosting, &mut [&mut sam]);
    let mut eve = Member::invited(&invite, "Eve");
    let reason = refused(&mut eve);
    assert!(reason.contains("already used"), "{reason}");
}

#[test]
fn expired_unknown_and_missing_invites_are_refused_with_a_reason() {
    let owner_id = Arc::new(Identity::new("Paul", [200, 80, 40]));
    let mut access = Access::owned_by(owner_id.public_key(), "Paul");
    let old = access.invite(Role::Editor, 60, access::now() - 3600);
    let (hosting, cert) = host(&owner_id, Some(access));
    let address = hosting.lock().loopback();
    let fp = cert.fingerprint();
    let stranger = || Arc::new(Identity::new("Eve", [0, 0, 0]));

    let mut expired = Member::new(Remote::join(address.clone(), fp, Some(old), stranger()));
    assert!(refused(&mut expired).contains("expired"));

    let made_up = identity::random::<16>();
    let mut unknown = Member::new(Remote::join(address.clone(), fp, Some(made_up), stranger()));
    assert!(refused(&mut unknown).contains("isn't one the host made"));

    let mut uninvited = Member::new(Remote::join(address.clone(), fp, None, stranger()));
    assert!(refused(&mut uninvited).contains("ask its owner for an invite"));

    // Revoked invites stop working.
    let invite = hosting.lock().invite(Role::Editor, INVITE_LIFETIME_SECONDS);
    hosting.lock().revoke_invites();
    let mut late = Member::invited(&invite, "Late");
    assert!(refused(&mut late).contains("isn't one the host made"));
    assert_eq!(hosting.lock().access().members().len(), 1);
}

#[test]
fn a_different_version_is_refused_with_what_to_do() {
    let owner_id = Arc::new(Identity::new("Paul", [200, 80, 40]));
    let (hosting, _cert) = host(&owner_id, None);
    let invite = hosting.lock().invite(Role::Editor, INVITE_LIFETIME_SECONDS);
    let identity = Arc::new(Identity::new("Sam", [1, 2, 3]));
    let old = Versions {
        engine: "0.0.1".into(),
        ..Versions::this()
    };
    let remote = Remote::join_invite(&loopback(&invite), identity.clone()).with_versions(old);
    let reason = refused(&mut Member::new(remote));
    assert!(reason.contains("use the same version"), "{reason}");

    let other_build = Versions {
        class_defaults: Versions::this().class_defaults ^ 1,
        ..Versions::this()
    };
    let remote = Remote::join_invite(&loopback(&invite), identity).with_versions(other_build);
    let reason = refused(&mut Member::new(remote));
    assert!(reason.contains("same build"), "{reason}");
    // A refused version doesn't use the invite up.
    let mut sam = Member::invited(&invite, "Sam");
    converge(&hosting, &mut [&mut sam]);
}

#[test]
fn a_host_with_another_certificate_is_refused_before_anything_is_sent() {
    let owner_id = Arc::new(Identity::new("Paul", [200, 80, 40]));
    let (hosting, _cert) = host(&owner_id, None);
    let invite = hosting.lock().invite(Role::Editor, INVITE_LIFETIME_SECONDS);
    let forged = Invite {
        cert: Fingerprint::of(b"some other certificate"),
        ..loopback(&invite)
    };
    let mut eve = Member::new(Remote::join_invite(
        &forged,
        Arc::new(Identity::new("Eve", [0; 3])),
    ));
    let reason = refused(&mut eve);
    assert!(
        reason.contains("isn't the host the invite names"),
        "{reason}"
    );
    assert_eq!(hosting.lock().access().members().len(), 1, "invite unused");
}

#[test]
fn a_signature_by_another_key_is_refused() {
    let owner_id = Arc::new(Identity::new("Paul", [200, 80, 40]));
    let (hosting, cert) = host(&owner_id, None);
    let link = Link::open(&hosting.lock().loopback(), cert.fingerprint());
    let next = |link: &Link| loop {
        match link.wait(PATIENCE).expect("an answer") {
            LinkEvent::Message(bytes) => return Greeting::from_bytes(&bytes).unwrap(),
            LinkEvent::Opened => {}
            LinkEvent::Closed(why) => panic!("closed: {why}"),
        }
    };
    // Claims to be the owner, but signs with its own key.
    let thief = Identity::new("Eve", [0; 3]);
    let hello = Greeting::Hello {
        protocol: Versions::this().protocol,
        engine: Versions::this().engine,
        class_defaults: Versions::this().class_defaults,
        key: owner_id.public_key(),
        name: "Paul".into(),
        color: [0; 3],
        invite: None,
    };
    link.send(hello.to_bytes());
    let Greeting::Challenge { nonce } = next(&link) else {
        panic!("expected a challenge")
    };
    let nonce = identity::from_hex(&nonce).unwrap();
    let message = [
        b"axcenity team create sign-in v1".as_slice(),
        &nonce,
        &cert.fingerprint().0,
    ]
    .concat();
    let proof = Greeting::Proof {
        signature: identity::to_hex(&thief.sign(&message)),
    };
    link.send(proof.to_bytes());
    let Greeting::Refused { reason } = next(&link) else {
        panic!("expected a refusal")
    };
    assert!(reason.contains("key didn't match"), "{reason}");
}

#[test]
fn a_session_message_before_signing_in_is_refused() {
    let owner_id = Arc::new(Identity::new("Paul", [200, 80, 40]));
    let (hosting, cert) = host(&owner_id, None);
    let link = Link::open(&hosting.lock().loopback(), cert.fingerprint());
    link.send(crate::wire::encode_to_host(&crate::ToHost::Hello {
        have: None,
    }));
    let start = Instant::now();
    loop {
        match link.wait(PATIENCE).expect("an answer") {
            LinkEvent::Message(bytes) => {
                let Some(Greeting::Refused { reason }) = Greeting::from_bytes(&bytes) else {
                    panic!("expected a refusal")
                };
                assert!(reason.contains("isn't how joining"), "{reason}");
                break;
            }
            LinkEvent::Opened => {}
            LinkEvent::Closed(why) => panic!("closed: {why}"),
        }
        assert!(start.elapsed() < PATIENCE);
    }
    assert!(hosting.lock().connected().is_empty());
}

/// A TCP relay between a member and the host whose connections the test
/// can cut, as a network failing between them would.
struct Relay {
    address: String,
    streams: Arc<std::sync::Mutex<Vec<std::net::TcpStream>>>,
}

impl Relay {
    fn to(target: String) -> Relay {
        use std::net::{TcpListener, TcpStream};
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let streams: Arc<std::sync::Mutex<Vec<TcpStream>>> = Arc::default();
        let kept = streams.clone();
        std::thread::spawn(move || {
            for joiner in listener.incoming() {
                let Ok(joiner) = joiner else { return };
                let Ok(host) = TcpStream::connect(&target) else {
                    continue;
                };
                let mut kept = kept.lock().unwrap();
                kept.push(joiner.try_clone().unwrap());
                kept.push(host.try_clone().unwrap());
                for (mut from, mut to) in [
                    (joiner.try_clone().unwrap(), host.try_clone().unwrap()),
                    (host, joiner),
                ] {
                    std::thread::spawn(move || {
                        let _ = std::io::copy(&mut from, &mut to);
                        let _ = to.shutdown(std::net::Shutdown::Both);
                    });
                }
            }
        });
        Relay { address, streams }
    }

    /// Breaks every connection through the relay; new ones still work.
    fn cut(&self) {
        for stream in self.streams.lock().unwrap().drain(..) {
            let _ = stream.shutdown(std::net::Shutdown::Both);
        }
    }
}

#[test]
fn edits_on_both_sides_of_a_network_cut_end_the_same_everywhere() {
    let owner_id = Arc::new(Identity::new("Paul", [200, 80, 40]));
    let (hosting, cert) = host(&owner_id, None);
    let mut owner = Member::owner(&hosting, &cert, owner_id);
    let invite = hosting.lock().invite(Role::Editor, INVITE_LIFETIME_SECONDS);
    let relay = Relay::to(hosting.lock().loopback());
    let through_relay = Invite {
        host: relay.address.clone(),
        ..invite
    };
    let mut sam = Member::new(Remote::join_invite(
        &through_relay,
        Arc::new(Identity::new("Sam", [1, 2, 3])),
    ));
    converge(&hosting, &mut [&mut owner, &mut sam]);

    for cut in 0..2 {
        relay.cut();
        until(&mut [&mut sam], "the cut to be noticed", |m| {
            m[0].lost > cut
        });
        sam.edit("Rename", |world| {
            let a = world.get(world.workspace()).unwrap().children()[0];
            world.set_name(a, format!("Sam's {cut}")).unwrap();
        });
        owner.edit(
            "Insert Part",
            add_part(if cut == 0 { "Owner 0" } else { "Owner 1" }),
        );
        // Both sides wrote the same property while apart: the later write
        // in the host's order (Sam's, sent after reconnecting) wins.
        owner.edit("Rename", |world| {
            let a = world.get(world.workspace()).unwrap().children()[0];
            world.set_name(a, "Owner's").unwrap();
        });
        converge(&hosting, &mut [&mut owner, &mut sam]);
        assert!(part_named(&owner.world, &format!("Sam's {cut}")));
    }
    assert!(part_named(&sam.world, "Owner 1"));
}

#[test]
fn names_colours_and_selections_travel_between_computers() {
    let owner_id = Arc::new(Identity::new("Paul", [200, 80, 40]));
    let (hosting, cert) = host(&owner_id, None);
    let mut owner = Member::owner(&hosting, &cert, owner_id);
    let invite = hosting.lock().invite(Role::Editor, INVITE_LIFETIME_SECONDS);
    let mut sam = Member::invited(&invite, "Sam");
    converge(&hosting, &mut [&mut owner, &mut sam]);
    until(&mut [&mut owner, &mut sam], "both on the roster", |m| {
        m.iter()
            .all(|m| m.client().roster().iter().filter(|c| c.online).count() == 2)
    });
    let roster = sam.client().roster().to_vec();
    let paul = roster.iter().find(|c| c.role == Role::Owner).unwrap();
    assert_eq!(paul.profile.name, "Paul");
    assert_eq!(paul.profile.color, [200, 80, 40]);
    assert!(
        roster
            .iter()
            .any(|c| c.profile.name == "Sam" && c.profile.color == [1, 2, 3])
    );

    let part = owner.world.get(owner.world.workspace()).unwrap().children()[0];
    let presence = crate::Presence {
        selection: vec![part],
        ..Default::default()
    };
    owner
        .client
        .as_mut()
        .unwrap()
        .set_presence(presence.clone());
    let paul = paul.member;
    until(&mut [&mut owner, &mut sam], "the selection", |m| {
        m[1].client().presences().get(&paul) == Some(&presence)
    });
}

#[test]
fn files_and_locks_travel_between_computers() {
    let owner_id = Arc::new(Identity::new("Paul", [200, 80, 40]));
    let (hosting, cert) = host(&owner_id, None);
    let mut owner = Member::owner(&hosting, &cert, owner_id);
    let invite = hosting.lock().invite(Role::Editor, INVITE_LIFETIME_SECONDS);
    let mut sam = Member::invited(&invite, "Sam");
    converge(&hosting, &mut [&mut owner, &mut sam]);

    // Bigger than one piece, so it goes up and down in several.
    let big: Vec<u8> = (0..crate::files::CHUNK + 4321)
        .map(|i| (i % 253) as u8)
        .collect();
    let hash = crate::files::FileHash::of(&big);
    let client = sam.client.as_mut().unwrap();
    client
        .publish(
            &mut sam.world,
            "Import",
            vec![("assets/big.bin".into(), Some(Arc::from(big.clone())))],
        )
        .unwrap()
        .unwrap();
    client.lock("scripts/Door.server.lua");
    converge(&hosting, &mut [&mut owner, &mut sam]);
    assert_eq!(
        hosting.lock().host().files().get("assets/big.bin"),
        Some(&hash)
    );

    owner.client.as_mut().unwrap().fetch(vec![hash]);
    let mut got = Vec::new();
    let start = Instant::now();
    while got.is_empty()
        || owner
            .client()
            .lock_holder("scripts/Door.server.lua")
            .is_none()
    {
        assert!(start.elapsed() < PATIENCE, "timed out waiting for the file");
        owner.step();
        sam.step();
        got.extend(owner.client.as_mut().unwrap().take_files());
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(got[0].1.to_vec(), big);
    assert_eq!(
        owner.client().lock_holder("scripts/Door.server.lua"),
        Some(sam.client().member())
    );
}

#[test]
fn a_member_flooding_the_host_is_cut_off_and_comes_back() {
    use crate::message::ToHost;
    use crate::presence::{Camera, Presence};
    let owner_id = Arc::new(Identity::new("Paul", [200, 80, 40]));
    let (hosting, cert) = host(&owner_id, None);
    let mut owner = Member::owner(&hosting, &cert, owner_id);
    let invite = hosting.lock().invite(Role::Editor, INVITE_LIFETIME_SECONDS);
    let mut sam = Member::invited(&invite, "Sam");
    converge(&hosting, &mut [&mut owner, &mut sam]);

    // Far more messages in one go than any editor sends in a second.
    let flood = (0..crate::session::MAX_MESSAGES_PER_SECOND + 500)
        .map(|i| {
            ToHost::Presence(Presence {
                selection: Vec::new(),
                camera: Some(Camera {
                    position: [i as f32, 0.0, 0.0],
                    look: [0.0, 0.0, 1.0],
                }),
                editing: None,
                script: None,
            })
        })
        .collect();
    sam.remote.send(flood);
    until(
        &mut [&mut owner, &mut sam],
        "the flood is cut off",
        |members| members[1].lost > 0,
    );
    // Its connection was closed, not its membership: it reconnects.
    sam.edit("Insert Part", add_part("After"));
    converge(&hosting, &mut [&mut owner, &mut sam]);
    assert!(part_named(&owner.world, "After"));
}

#[test]
fn an_address_that_keeps_failing_to_join_is_turned_away_for_a_while() {
    let owner_id = Arc::new(Identity::new("Paul", [200, 80, 40]));
    let (hosting, cert) = host(&owner_id, None);
    let address = hosting.lock().loopback();
    let fp = cert.fingerprint();
    for _ in 0..crate::limits::FAILURES_MOST {
        let eve = Arc::new(Identity::new("Eve", [0; 3]));
        let mut guess = Member::new(Remote::join(address.clone(), fp, None, eve));
        assert!(refused(&mut guess).contains("ask its owner for an invite"));
    }
    // From that address, even the owner is closed before signing in now.
    let mut owner = Member::owner(&hosting, &cert, owner_id);
    until(&mut [&mut owner], "being turned away", |m| m[0].lost > 0);
    assert!(owner.refused.is_none() && owner.client.is_none());
    let log = hosting.lock().host().audit_log();
    assert!(
        log.iter()
            .any(|e| e.text.contains("turned away for a minute")),
        "{log:#?}"
    );
    assert_eq!(
        log.iter()
            .filter(|e| e.kind == crate::audit::AuditKind::Refused)
            .count(),
        crate::limits::FAILURES_MOST as usize + 1
    );
}

#[test]
fn a_replayed_sign_in_is_refused() {
    use crate::handshake::{Gate, GateStep, Knock, KnockStep};
    let sam = Identity::new("Sam", [1, 2, 3]);
    let eve = Identity::new("Eve", [0; 3]);
    let cert = Fingerprint([7; 32]);
    let versions = Versions::this();
    let mut access = Access::owned_by(Identity::new("Paul", [0; 3]).public_key(), "Paul");
    let secret = access.invite(Role::Editor, 60, access::now());

    // Sam signs in with the invite; what he sent is recorded.
    let (mut knock, hello) = Knock::new(&sam, cert, Some(secret), &versions);
    let mut gate = Gate::new(cert, versions.clone());
    let GateStep::Reply(challenge) = gate.receive(&hello, &mut access, access::now()) else {
        panic!("expected a challenge")
    };
    let KnockStep::Reply(proof) = knock.receive(&challenge, &sam) else {
        panic!("expected a proof")
    };
    assert!(matches!(
        gate.receive(&proof, &mut access, access::now()),
        GateStep::Admit { .. }
    ));
    let refusal = |step: GateStep| match step {
        GateStep::Refuse { reason, .. } => reason,
        other => panic!("expected a refusal, got {other:?}"),
    };
    // The same proof again on that connection.
    let reason = refusal(gate.receive(&proof, &mut access, access::now()));
    assert!(reason.contains("isn't how joining"), "{reason}");
    // On a new connection: its challenge is new, so the old proof fails.
    let mut again = Gate::new(cert, versions.clone());
    assert!(matches!(
        again.receive(&hello, &mut access, access::now()),
        GateStep::Reply(_)
    ));
    let reason = refusal(again.receive(&proof, &mut access, access::now()));
    assert!(reason.contains("key didn't match"), "{reason}");
    // Someone else with the same (used) invite.
    let (mut knock, hello) = Knock::new(&eve, cert, Some(secret), &versions);
    let mut gate = Gate::new(cert, versions.clone());
    let GateStep::Reply(challenge) = gate.receive(&hello, &mut access, access::now()) else {
        panic!("expected a challenge")
    };
    let KnockStep::Reply(proof) = knock.receive(&challenge, &eve) else {
        panic!("expected a proof")
    };
    let reason = refusal(gate.receive(&proof, &mut access, access::now()));
    assert!(reason.contains("already used"), "{reason}");
    // A proof made for another host's certificate doesn't work here.
    let (mut knock, hello) = Knock::new(&sam, Fingerprint([8; 32]), None, &versions);
    let mut gate = Gate::new(cert, versions);
    let GateStep::Reply(challenge) = gate.receive(&hello, &mut access, access::now()) else {
        panic!("expected a challenge")
    };
    let KnockStep::Reply(proof) = knock.receive(&challenge, &sam) else {
        panic!("expected a proof")
    };
    let reason = refusal(gate.receive(&proof, &mut access, access::now()));
    assert!(reason.contains("key didn't match"), "{reason}");
}

#[test]
fn whoever_may_host_keeps_the_members_list_and_a_viewer_never_gets_it() {
    let owner_id = Arc::new(Identity::new("Paul", [200, 80, 40]));
    let (hosting, cert) = host(&owner_id, None);
    let mut owner = Member::owner(&hosting, &cert, owner_id);
    let invite = hosting.lock().invite(Role::Editor, INVITE_LIFETIME_SECONDS);
    let mut sam = Member::invited(&invite, "Sam");
    let invite = hosting.lock().invite(Role::Viewer, INVITE_LIFETIME_SECONDS);
    let mut kim = Member::invited(&invite, "Kim");
    converge(&hosting, &mut [&mut owner, &mut sam, &mut kim]);
    let newest = hosting.lock().access().version();
    until(
        &mut [&mut owner, &mut sam, &mut kim],
        "the members list",
        |m| {
            m[..2].iter().all(|m| {
                m.client()
                    .members_list()
                    .is_some_and(|list| list.version == newest && list.members.len() == 3)
            })
        },
    );
    assert!(
        kim.client().members_list().is_none(),
        "a Viewer never hosts"
    );
    let list = sam.client().members_list().unwrap().clone();
    assert_eq!(list.owner().map(|m| m.name.as_str()), Some("Paul"));

    // A change reaches them again; a Viewer made an Editor gets it too.
    let kim_id = kim.client().member();
    hosting.lock().set_role(kim_id, Role::Editor);
    until(
        &mut [&mut owner, &mut sam, &mut kim],
        "the changed list",
        |m| {
            m.iter().all(|m| {
                m.client()
                    .members_list()
                    .is_some_and(|list| list.member(kim_id).is_some_and(|k| k.role == Role::Editor))
            })
        },
    );
    // Another computer could host with it: same members, roles and Owner.
    let access = Access::from_list(sam.client().members_list().unwrap());
    assert_eq!(access.members(), hosting.lock().access().members());
}

/// Steps `member` until the host has a join request, and returns it.
fn asked(hosting: &Hosting, member: &mut Member) -> crate::session::JoinRequest {
    let mut found = None;
    until(&mut [member], "the host to be asked", |_| {
        found = hosting.lock().join_requests().first().cloned();
        found.is_some()
    });
    found.unwrap()
}

#[test]
fn asking_new_members_waits_for_the_host_and_lets_members_back_in() {
    let owner_id = Arc::new(Identity::new("Paul", [200, 80, 40]));
    let (hosting, _cert) = host(&owner_id, None);
    hosting.lock().set_trust(Trust::AskNewMembers);
    let invite = hosting.lock().invite(Role::Viewer, INVITE_LIFETIME_SECONDS);
    let mut sam = Member::invited(&invite, "Sam");
    let request = asked(&hosting, &mut sam);
    assert_eq!(request.asking.name, "Sam");
    assert_eq!(request.asking.role, Role::Viewer);
    assert!(!request.asking.member, "Sam is new");
    assert!(request.address.is_loopback());
    // Waiting isn't refused or let in, however long the knock takes.
    for _ in 0..20 {
        sam.step();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(sam.client.is_none() && sam.refused.is_none());
    hosting.lock().answer_join(request.id, true);
    converge(&hosting, &mut [&mut sam]);
    assert!(hosting.lock().join_requests().is_empty());

    // Sam is a member now: a reconnect isn't asked about.
    sam.drop_connection();
    converge(&hosting, &mut [&mut sam]);
    assert!(hosting.lock().join_requests().is_empty());
}

#[test]
fn a_joiner_the_host_turns_away_is_refused_and_the_invite_stays_unused() {
    let owner_id = Arc::new(Identity::new("Paul", [200, 80, 40]));
    let (hosting, _cert) = host(&owner_id, None);
    hosting.lock().set_trust(Trust::AskEveryone);
    let invite = hosting.lock().invite(Role::Editor, INVITE_LIFETIME_SECONDS);
    let mut eve = Member::invited(&invite, "Eve");
    let request = asked(&hosting, &mut eve);
    hosting.lock().answer_join(request.id, false);
    assert!(refused(&mut eve).contains("didn't let you in"));
    assert_eq!(hosting.lock().access().members().len(), 1);
    let audit = hosting.lock().host().audit_log();
    assert!(
        audit.iter().any(|e| e.text.contains("Didn't let Eve in")),
        "{audit:?}"
    );
    // The invite wasn't used: someone the host lets in can still use it.
    let mut sam = Member::invited(&invite, "Sam");
    let request = asked(&hosting, &mut sam);
    hosting.lock().answer_join(request.id, true);
    converge(&hosting, &mut [&mut sam]);
}

#[test]
fn a_bad_invite_is_refused_without_asking_the_host() {
    let owner_id = Arc::new(Identity::new("Paul", [200, 80, 40]));
    let (hosting, cert) = host(&owner_id, None);
    hosting.lock().set_trust(Trust::AskEveryone);
    let address = hosting.lock().loopback();
    let stranger = Arc::new(Identity::new("Eve", [0, 0, 0]));
    let mut eve = Member::new(Remote::join(address, cert.fingerprint(), None, stranger));
    assert!(refused(&mut eve).contains("ask its owner for an invite"));
    assert!(hosting.lock().join_requests().is_empty());
}

#[test]
fn a_host_listens_on_loopback_and_the_network_it_uses_not_everywhere() {
    use crate::session::listen_addresses;
    use std::net::{IpAddr, SocketAddr};
    let tailscale: IpAddr = "100.64.1.2".parse().unwrap();
    assert_eq!(
        listen_addresses(47800, &[tailscale], false),
        [
            SocketAddr::from(([127, 0, 0, 1], 47800)),
            SocketAddr::new(tailscale, 47800)
        ]
    );
    assert_eq!(
        listen_addresses(47800, &[tailscale], true),
        [SocketAddr::from(([0, 0, 0, 0], 47800))]
    );
    // Loopback is never listed twice.
    assert_eq!(
        listen_addresses(0, &["127.0.0.1".parse().unwrap()], false),
        [SocketAddr::from(([127, 0, 0, 1], 0))]
    );
}

/// Two loopback addresses stand in for loopback and a network's address.
#[cfg(target_os = "linux")]
#[test]
fn a_listener_on_two_addresses_takes_connections_on_both_with_one_port() {
    let cert = HostCert::generate();
    let owner_id = Arc::new(Identity::new("Paul", [200, 80, 40]));
    let session = HostSession::start_on(
        Host::new(sample()),
        Access::owned_by(owner_id.public_key(), "Paul"),
        None,
        &cert,
        &[
            "127.0.0.1:0".parse().unwrap(),
            "127.0.0.2:0".parse().unwrap(),
        ],
        None,
    )
    .unwrap();
    let port = session.local_address().port();
    let hosting = session.serve();
    let invite = hosting.lock().invite(Role::Editor, INVITE_LIFETIME_SECONDS);
    let mut sam = Member::new(Remote::join_invite(
        &Invite {
            host: format!("127.0.0.2:{port}"),
            ..invite
        },
        Arc::new(Identity::new("Sam", [1, 2, 3])),
    ));
    let mut owner = Member::owner(&hosting, &cert, owner_id);
    converge(&hosting, &mut [&mut owner, &mut sam]);
    // Nothing listens on a third address.
    assert!(std::net::TcpStream::connect(("127.0.0.3", port)).is_err());
}
