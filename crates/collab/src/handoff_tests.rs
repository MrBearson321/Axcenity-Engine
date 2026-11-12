//! The session going on under another host (Collaborate 2.0, P4), over
//! real sockets on 127.0.0.1: the brief's scenario A (the host leaves) and
//! B (the host is lost without a word), with the successor list, members'
//! unconfirmed edits, and a new host recovering transactions it missed
//! from another member.

use std::sync::Arc;
use std::time::{Duration, Instant};

use axcenity_core::{ClassName, World};

use crate::access::{Access, INVITE_LIFETIME_SECONDS, Invite};
use crate::client::Client;
use crate::election::{self, Choice, Timing};
use crate::host::Host;
use crate::identity::Identity;
use crate::project::HostCard;
use crate::role::Role;
use crate::session::{HostSession, Hosting, Remote, RemoteEvent};
use crate::socket::HostCert;
use crate::testing::{dump, dump_doc};

const PATIENCE: Duration = Duration::from_secs(20);

/// One member's editor, which can also host.
struct Member {
    identity: Arc<Identity>,
    cert: HostCert,
    /// The address it would host on.
    address: String,
    remote: Remote,
    client: Option<Client>,
    world: World,
    /// Drops what arrives, as if it was lost on the way (tests a member
    /// that's behind).
    deaf: bool,
}

/// A port nothing listens on now.
fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

impl Member {
    fn new(name: &str, remote: impl FnOnce(Arc<Identity>) -> Remote) -> Member {
        Member::with(Arc::new(Identity::new(name, [1, 2, 3])), remote)
    }

    fn with(identity: Arc<Identity>, remote: impl FnOnce(Arc<Identity>) -> Remote) -> Member {
        Member {
            remote: remote(identity.clone()),
            identity,
            cert: HostCert::generate(),
            address: format!("127.0.0.1:{}", free_port()),
            client: None,
            world: World::new(),
            deaf: false,
        }
    }

    fn step(&mut self) {
        for event in self.remote.poll() {
            match event {
                RemoteEvent::Admitted { member, .. } => {
                    let client = self.client.get_or_insert_with(|| Client::new(member));
                    assert_eq!(client.member(), member, "the same member on every host");
                    client.set_card(self.address.clone(), self.cert.fingerprint());
                    client.connect();
                }
                RemoteEvent::Message(_) if self.deaf => {}
                RemoteEvent::Message(message) => {
                    let client = self.client.as_mut().expect("admitted first");
                    client.receive(&mut self.world, message);
                }
                RemoteEvent::Lost { .. } => {
                    if let Some(client) = &mut self.client {
                        client.disconnected();
                    }
                }
                RemoteEvent::Refused(reason) => panic!("refused: {reason}"),
            }
        }
        if let Some(client) = &mut self.client {
            self.remote.send(client.take_outbox());
        }
    }

    fn client(&self) -> &Client {
        self.client.as_ref().expect("joined")
    }

    fn edit(&mut self, name: &'static str) {
        let world = &mut self.world;
        let workspace = world.workspace();
        let id = world.create(ClassName::Part).unwrap();
        world.set_name(id, name).unwrap();
        world.set_parent(id, Some(workspace)).unwrap();
        let client = self.client.as_mut().expect("joined");
        client.capture(world, format!("Insert {name}"));
        self.remote.send(client.take_outbox());
    }

    /// Goes to the host `card` names, as an editor does when the host
    /// changes: by key, no invite.
    fn move_to(&mut self, card: &HostCard) {
        self.remote = Remote::join(card.address.clone(), card.cert, None, self.identity.clone());
        if let Some(client) = &mut self.client {
            client.disconnected();
        }
    }

    /// Hosts from this member's copy, as a successor does: the World
    /// without its unconfirmed edits, the members it was sent, the same
    /// project ID, and IDs after the old host's.
    fn host(&mut self, recover: Option<Duration>) -> Hosting {
        let client = self.client.as_mut().expect("joined");
        let confirmed = client.confirmed(&mut self.world);
        let access = Access::from_list(client.members_list().expect("a may-host member has it"));
        let mut host = Host::new(confirmed.world)
            .with_graveyard(confirmed.graveyard)
            .with_session(client.session().unwrap().to_owned())
            .with_next_id(client.next_id());
        if let Some(wait) = recover {
            host = host.recover_from(confirmed.epoch.unwrap(), confirmed.seq, wait);
        }
        let me = client.member();
        let mut session = HostSession::start(
            host,
            access,
            None,
            &self.cert,
            self.address.parse().unwrap(),
            Some(self.address.clone()),
        )
        .unwrap();
        session.set_hosted_by(Some(me));
        let hosting = session.serve();
        let card = HostCard {
            member: Some(me),
            address: self.address.clone(),
            cert: self.cert.fingerprint(),
        };
        self.move_to(&card);
        hosting
    }
}

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

/// Everyone has the host's latest and nothing unconfirmed, and every copy
/// is the host's.
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
    for member in members.iter() {
        let copy = dump(&member.world, member.client().graveyard())
            + &crate::testing::dump_files(member.client().files());
        assert_eq!(copy, host, "{}'s copy differs", member.identity.name);
    }
}

fn has_part(world: &World, name: &str) -> bool {
    world
        .descendants(world.root())
        .into_iter()
        .any(|id| world.get(id).is_some_and(|i| i.name() == name))
}

/// Paul hosts (his own editor, joined as Owner); Sam and Kim join as
/// Editors, in that order.
fn three() -> (Hosting, Member, Member, Member) {
    let cert = HostCert::generate();
    let paul_identity = Arc::new(Identity::new("Paul", [9, 9, 9]));
    let access = Access::owned_by(paul_identity.public_key(), &paul_identity.name);
    let session = HostSession::start(
        Host::new(World::new()).with_session("0123456789abcdef"),
        access,
        None,
        &cert,
        "127.0.0.1:0".parse().unwrap(),
        None,
    )
    .unwrap();
    let hosting = session.serve();
    let loopback = hosting.lock().loopback();
    let fp = cert.fingerprint();
    let mut paul = Member::with(paul_identity, |id| Remote::join(loopback, fp, None, id));
    let invite = |hosting: &Hosting| {
        loopback_invite(&hosting.lock().invite(Role::Editor, INVITE_LIFETIME_SECONDS))
    };
    let sam_invite = invite(&hosting);
    let mut sam = Member::new("Sam", |id| Remote::join_invite(&sam_invite, id));
    until(&mut [&mut paul, &mut sam], "Sam", |m| {
        m.iter()
            .all(|m| m.client.as_ref().is_some_and(Client::is_joined))
    });
    let kim_invite = invite(&hosting);
    let mut kim = Member::new("Kim", |id| Remote::join_invite(&kim_invite, id));
    converge(&hosting, &mut [&mut paul, &mut sam, &mut kim]);
    let paul_id = paul.client().member();
    hosting.lock().set_hosted_by(Some(paul_id));
    (hosting, paul, sam, kim)
}

fn loopback_invite(invite: &Invite) -> Invite {
    let port = invite.host.rsplit(':').next().unwrap();
    Invite {
        host: format!("127.0.0.1:{port}"),
        ..invite.clone()
    }
}

#[test]
fn everyone_hears_the_same_successors_backup_first_then_longest_connected() {
    let (hosting, mut paul, mut sam, mut kim) = three();
    let (sam_id, kim_id) = (sam.client().member(), kim.client().member());
    until(&mut [&mut paul, &mut sam, &mut kim], "the list", |m| {
        m.iter().all(|m| m.client().successors().len() == 2)
    });
    let order = |m: &Member| -> Vec<_> {
        m.client()
            .successors()
            .iter()
            .map(|c| c.member.unwrap())
            .collect()
    };
    // Sam joined first; Paul hosts, so he's never on it.
    assert_eq!(order(&kim), [sam_id, kim_id]);
    assert_eq!(order(&paul), order(&sam));
    assert_eq!(kim.client().successors()[0].address, sam.address);
    // The Owner names Kim the backup host.
    hosting.lock().set_backup(Some(kim_id));
    until(
        &mut [&mut paul, &mut sam, &mut kim],
        "the backup first",
        |m| m.iter().all(|m| order(m) == [kim_id, sam_id]),
    );
    // A Viewer is never a successor.
    hosting.lock().set_role(kim_id, Role::Viewer);
    until(
        &mut [&mut paul, &mut sam, &mut kim],
        "Kim off the list",
        |m| m.iter().all(|m| order(m) == [sam_id]),
    );
    // Sam stops offering to host ("Let others host from my computer" off).
    sam.client.as_mut().unwrap().withdraw_card();
    until(
        &mut [&mut paul, &mut sam, &mut kim],
        "Sam off the list",
        |m| m.iter().all(|m| m.client().successors().is_empty()),
    );
}

/// Scenario A: the host leaves while others work. Sam takes over; nobody's
/// edits are lost, those made during the handoff included.
#[test]
fn the_host_leaves_and_the_successor_carries_on_with_everyones_edits() {
    let (hosting, mut paul, mut sam, mut kim) = three();
    sam.edit("BeforeSam");
    kim.edit("BeforeKim");
    converge(&hosting, &mut [&mut paul, &mut sam, &mut kim]);
    until(&mut [&mut paul, &mut sam, &mut kim], "the list", |m| {
        m.iter().all(|m| m.client().successors().len() == 2)
    });

    // Paul leaves. Everyone hears Sam hosts next.
    let to = hosting.hand_off().expect("a successor");
    assert_eq!(to.member, Some(sam.client().member()));
    until(&mut [&mut paul, &mut sam, &mut kim], "the handoff", |m| {
        m.iter().all(|m| m.client().handoff().is_some())
    });
    let last_seq = sam.client().handoff().unwrap().last_seq;
    assert!(
        sam.client().seq() >= last_seq,
        "Sam has everything Paul accepted"
    );
    // Edits made meanwhile wait, the successor's own too.
    kim.edit("DuringHandoff");
    sam.edit("SamDuringHandoff");
    let handoff = kim.client().handoff().unwrap().clone();
    assert_eq!(handoff.to.as_ref().unwrap().address, sam.address);

    let new_host = sam.host(None);
    let card = handoff.to.unwrap();
    kim.move_to(&card);
    paul.move_to(&card);
    sam.edit("AfterSam");
    converge(&new_host, &mut [&mut paul, &mut sam, &mut kim]);
    for name in [
        "BeforeSam",
        "BeforeKim",
        "DuringHandoff",
        "SamDuringHandoff",
        "AfterSam",
    ] {
        assert!(has_part(&paul.world, name), "{name} reached everyone");
    }
    // Paul is still the Owner; roles came with the members list.
    assert_eq!(paul.client().role(), Role::Owner);
    assert_eq!(kim.client().role(), Role::Editor);
    assert_eq!(paul.client().session(), Some("0123456789abcdef"));
    // A new part from anyone gets an ID nobody used.
    kim.edit("New");
    converge(&new_host, &mut [&mut paul, &mut sam, &mut kim]);
}

/// Scenario B: the host's computer goes off. After the grace period Sam
/// hosts; Kim had a transaction Sam's copy missed, and the new host gets
/// it from Kim before taking edits.
#[test]
fn a_lost_host_is_replaced_and_what_the_successor_missed_comes_from_another_member() {
    let (hosting, mut paul, mut sam, mut kim) = three();
    kim.edit("Early");
    converge(&hosting, &mut [&mut paul, &mut sam, &mut kim]);
    until(&mut [&mut paul, &mut sam, &mut kim], "the list", |m| {
        m.iter().all(|m| m.client().successors().len() == 2)
    });
    // Kim's last edit never reaches Sam: the host goes before it does.
    let sam_seq = sam.client().seq();
    sam.deaf = true;
    kim.edit("OnlyKimSawIt");
    until(
        &mut [&mut paul, &mut sam, &mut kim],
        "Kim's edit confirmed",
        |m| m[2].client().pending() == 0 && m[0].client().seq() == sam_seq + 1,
    );
    let successors = kim.client().successors().to_vec();

    // The host vanishes.
    hosting.stop_without_a_word();
    sam.deaf = false;
    assert_eq!(sam.client().seq(), sam_seq, "Sam's copy is one behind");
    let timing = Timing {
        grace: Duration::from_millis(50),
        per_successor: Duration::from_secs(5),
    };
    let me = |m: &Member| m.client().member();
    let after = Duration::from_millis(60);
    let during = Duration::from_millis(10);
    assert_eq!(
        election::after_loss(during, timing, &successors, me(&sam)),
        Choice::WaitForHost
    );
    assert_eq!(
        election::after_loss(after, timing, &successors, me(&sam)),
        Choice::Host
    );
    assert_eq!(
        election::after_loss(after, timing, &successors, me(&kim)),
        Choice::Join(0)
    );
    assert_eq!(
        election::after_loss(after, timing, &successors, me(&paul)),
        Choice::Join(0)
    );

    // Sam hosts from its copy, recovering what it missed.
    let new_host = sam.host(Some(Duration::from_secs(10)));
    assert!(new_host.lock().host().is_recovering());
    let card = successors[0].clone();
    kim.move_to(&card);
    paul.move_to(&card);
    until(&mut [&mut paul, &mut sam, &mut kim], "recovery", |_| {
        !new_host.lock().host().is_recovering()
    });
    converge(&new_host, &mut [&mut paul, &mut sam, &mut kim]);
    assert!(
        has_part(&sam.world, "OnlyKimSawIt"),
        "recovered from Kim's tail"
    );
    assert!(has_part(&paul.world, "Early"));
    assert_eq!(new_host.lock().host().unrecovered(), None);
}
