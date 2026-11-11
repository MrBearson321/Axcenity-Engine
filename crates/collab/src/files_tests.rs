//! The session's files (TC5): saves by hash with compare-and-swap,
//! uploads and fetches in pieces, document locks, imports as one
//! transaction, and members' folders kept in step by [`Mirror`].

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use axcenity_core::{ClassName, World};

use crate::apply::Refusal;
use crate::client::Notice;
use crate::files::{self, CHUNK, FileHash, FolderStore, MemoryStore};
use crate::host::{Host, LOCK_GRACE};
use crate::message::ToHost;
use crate::mirror::{Change, Mirror};
use crate::op::Op;
use crate::role::Role;
use crate::testing::*;

const DOOR: &str = "scripts/Door.server.lua";

/// An owner, two editors and a viewer, in a project with one script.
fn session() -> Net {
    let (store, manifest) = MemoryStore::with(&[(DOOR, b"print('door')")]);
    let host = Host::new(World::new()).with_files(manifest, Box::new(store));
    Net::with_host(
        host,
        &[Role::Owner, Role::Editor, Role::Editor, Role::Viewer],
    )
}

fn hash(bytes: &[u8]) -> FileHash {
    FileHash::of(bytes)
}

fn refusals(net: &mut Net, i: usize) -> Vec<Refusal> {
    net.clients[i]
        .take_notices()
        .into_iter()
        .filter_map(|n| match n {
            Notice::Refused { reason, .. } => Some(reason),
            _ => None,
        })
        .collect()
}

/// Member `i` fetches `hash` and gets its bytes.
fn fetched(net: &mut Net, i: usize, hash: FileHash) -> Vec<u8> {
    net.with_client(i, |client, _| assert!(client.fetch(vec![hash])));
    net.settle();
    let files = net.clients[i].take_files();
    let (got, bytes) = files.into_iter().next().expect("a file arrived");
    assert_eq!(got, hash);
    bytes.to_vec()
}

#[test]
fn a_saved_file_reaches_everyone() {
    let mut net = session();
    assert_eq!(
        net.clients[2].files().get(DOOR),
        Some(&hash(b"print('door')"))
    );
    net.publish(1, "Save", &[(DOOR, Some(b"print('open')"))])
        .unwrap()
        .unwrap();
    net.publish(1, "New", &[("assets/note.txt", Some(b"hello"))])
        .unwrap()
        .unwrap();
    net.settle();
    net.assert_converged();
    assert_eq!(net.host.files().get(DOOR), Some(&hash(b"print('open')")));
    assert_eq!(fetched(&mut net, 2, hash(b"hello")), b"hello");
    // And deleting.
    net.publish(2, "Delete", &[("assets/note.txt", None)])
        .unwrap()
        .unwrap();
    net.settle();
    net.assert_converged();
    assert!(!net.host.files().contains_key("assets/note.txt"));
}

#[test]
fn the_second_of_two_saves_from_one_version_is_refused() {
    let mut net = session();
    net.publish(1, "Save", &[(DOOR, Some(b"one"))]).unwrap();
    net.publish(2, "Save", &[(DOOR, Some(b"two"))]).unwrap();
    net.settle();
    net.assert_converged();
    assert_eq!(net.host.files().get(DOOR), Some(&hash(b"one")));
    assert_eq!(
        refusals(&mut net, 2),
        vec![Refusal::FileChanged { path: DOOR.into() }]
    );
    // Saving again from the version they now have works.
    net.publish(2, "Save", &[(DOOR, Some(b"two"))]).unwrap();
    net.settle();
    net.assert_converged();
    assert_eq!(net.host.files().get(DOOR), Some(&hash(b"two")));
}

#[test]
fn a_save_whose_file_never_arrived_is_refused() {
    let mut net = session();
    let op = Op::FileVersion {
        path: DOOR.into(),
        base: Some(hash(b"print('door')")),
        new: Some(hash(b"never sent")),
    };
    net.submit(1, "Save", vec![op]).unwrap();
    net.settle();
    net.assert_converged();
    assert!(matches!(&refusals(&mut net, 1)[..], [Refusal::Invalid(_)]));
}

#[test]
fn viewers_and_bad_paths_change_no_files() {
    let mut net = session();
    assert_eq!(
        net.publish(3, "Save", &[(DOOR, Some(b"x"))]),
        Err(Refusal::Permission)
    );
    // A Viewer's client that sends anyway is refused by the host.
    let op = Op::FileVersion {
        path: DOOR.into(),
        base: Some(hash(b"print('door')")),
        new: None,
    };
    let out = net.host.receive(
        member(3),
        ToHost::Submit(crate::op::Transaction {
            client_seq: 99,
            label: "Delete".into(),
            ops: vec![op],
        }),
    );
    assert!(out.iter().any(|o| matches!(
        &o.message,
        crate::message::ToClient::Rejected {
            reason: Refusal::Permission,
            ..
        }
    )));
    for bad in [
        "../outside.txt",
        "/etc/passwd",
        "a\\b",
        "C:/x",
        ".axcenity/x",
        "world.json",
    ] {
        assert!(
            matches!(
                net.publish(1, "Save", &[(bad, Some(b"x"))]),
                Err(Refusal::Invalid(_))
            ),
            "{bad}"
        );
    }
    net.settle();
    net.assert_converged();
    assert_eq!(net.host.files().len(), 1);
}

#[test]
fn a_document_being_edited_is_saved_only_by_its_editor() {
    let mut net = session();
    net.with_client(1, |client, _| client.lock(DOOR));
    net.settle();
    for i in 0..4 {
        assert_eq!(
            net.clients[i].lock_holder(DOOR),
            Some(member(1)),
            "client {i}"
        );
    }
    // A second asker doesn't get it.
    net.with_client(2, |client, _| client.lock(DOOR));
    net.settle();
    assert_eq!(net.clients[2].lock_holder(DOOR), Some(member(1)));
    assert_eq!(
        net.publish(2, "Save", &[(DOOR, Some(b"two"))]),
        Err(Refusal::Locked {
            path: DOOR.into(),
            by: member(1)
        })
    );
    // The host refuses it too, should a save get there anyway.
    let out = net.host.receive(
        member(2),
        ToHost::Submit(crate::op::Transaction {
            client_seq: 50,
            label: "Save".into(),
            ops: vec![Op::FileVersion {
                path: DOOR.into(),
                base: Some(hash(b"print('door')")),
                new: None,
            }],
        }),
    );
    assert!(out.iter().any(|o| matches!(
        &o.message,
        crate::message::ToClient::Rejected {
            reason: Refusal::Locked { .. },
            ..
        }
    )));
    net.publish(1, "Save", &[(DOOR, Some(b"one"))]).unwrap();
    net.with_client(1, |client, _| client.unlock(DOOR));
    net.with_client(2, |client, _| client.unlock(DOOR));
    net.settle();
    assert_eq!(net.clients[2].lock_holder(DOOR), None);
    // (Member 2's client numbering was overtaken by the hand-made
    // transaction above, so the owner saves next.)
    net.publish(0, "Save", &[(DOOR, Some(b"two"))]).unwrap();
    net.settle();
    net.assert_converged();
    assert_eq!(net.host.files().get(DOOR), Some(&hash(b"two")));
}

#[test]
fn a_lock_outlasts_a_short_drop_but_not_a_long_one() {
    let mut net = session();
    let start = Instant::now();
    net.tick(start);
    net.with_client(1, |client, _| client.lock(DOOR));
    net.settle();
    net.disconnect(1);
    net.tick(start + LOCK_GRACE / 2);
    net.settle();
    assert_eq!(net.clients[2].lock_holder(DOOR), Some(member(1)));
    // Back in time: the lock stays theirs (and is asked for again).
    net.connect(1);
    net.settle();
    net.tick(start + LOCK_GRACE * 2);
    net.settle();
    assert_eq!(net.clients[2].lock_holder(DOOR), Some(member(1)));
    // Gone too long: it ends.
    net.disconnect(1);
    net.tick(start + LOCK_GRACE * 2 + Duration::from_secs(1));
    net.tick(start + LOCK_GRACE * 3 + Duration::from_secs(2));
    net.settle();
    assert_eq!(net.clients[2].lock_holder(DOOR), None);
    assert_eq!(net.host.locks().len(), 0);
}

#[test]
fn an_import_is_one_transaction_with_its_files() {
    let mut net = session();
    let world = net.world_mut(1);
    let workspace = world.workspace();
    let model = world.create(ClassName::Model).unwrap();
    world.set_name(model, "Tree").unwrap();
    world.set_parent(model, Some(workspace)).unwrap();
    let seq = net
        .publish(1, "Import Tree", &[("assets/tree.glb", Some(b"glTF"))])
        .unwrap()
        .unwrap();
    net.settle();
    net.assert_converged();
    let logged: Vec<_> = net.host.log().filter(|l| l.client_seq == seq).collect();
    assert_eq!(logged.len(), 1);
    assert!(
        logged[0]
            .ops
            .iter()
            .any(|op| matches!(op, Op::FileVersion { .. }))
    );
    assert!(
        logged[0]
            .ops
            .iter()
            .any(|op| matches!(op, Op::CreateTree { .. }))
    );
    assert!(net.worlds[2].get(model).is_some());
}

#[test]
fn saves_made_offline_are_uploaded_again_on_reconnect() {
    let mut net = session();
    net.disconnect(1);
    net.publish(1, "Save", &[(DOOR, Some(b"offline"))]).unwrap();
    assert_eq!(net.clients[1].pending(), 1);
    net.connect(1);
    net.settle();
    net.assert_converged();
    assert_eq!(net.host.files().get(DOOR), Some(&hash(b"offline")));
    assert_eq!(fetched(&mut net, 2, hash(b"offline")), b"offline");
}

#[test]
fn big_files_travel_in_pieces() {
    let mut net = session();
    let big: Vec<u8> = (0..CHUNK * 2 + 1234).map(|i| (i * 7 % 251) as u8).collect();
    net.publish(1, "Import", &[("assets/big.bin", Some(&big))])
        .unwrap();
    net.settle();
    net.assert_converged();
    assert_eq!(fetched(&mut net, 3, hash(&big)), big);
    // An empty file is a file too.
    net.publish(1, "New", &[("assets/empty.txt", Some(b""))])
        .unwrap();
    net.settle();
    assert_eq!(fetched(&mut net, 2, hash(b"")), b"");
}

#[test]
fn a_file_the_host_no_longer_has_is_reported_missing() {
    let mut net = session();
    net.with_client(2, |client, _| client.fetch(vec![hash(b"nothing")]));
    net.settle();
    assert_eq!(net.clients[2].take_missing(), vec![hash(b"nothing")]);
}

#[test]
fn conflict_copies_keep_the_script_from_loading() {
    let none = |_: &str| false;
    assert_eq!(
        files::conflict_path(DOOR, "Sam Lee", &none),
        "scripts/Door.server.conflict-sam-lee.lua"
    );
    assert_eq!(
        files::conflict_path("README", "sam", &none),
        "README.conflict-sam"
    );
    let taken = |p: &str| p == "a.conflict-sam.txt";
    assert_eq!(
        files::conflict_path("a.txt", "sam", &taken),
        "a.conflict-sam-2.txt"
    );
    assert!(files::is_synced("scripts/a.lua"));
    for not in [
        ".axcenity/cache",
        "levels/two.world.json",
        "world.json",
        "plugins/p.lua",
        "a/.DS_Store",
    ] {
        assert!(!files::is_synced(not), "{not}");
    }
}

// Mirrors: each member's folder.

static FOLDERS: AtomicUsize = AtomicUsize::new(0);

/// A folder that's deleted when the test ends.
struct Folder(PathBuf);

impl Folder {
    fn new() -> Folder {
        let path = std::env::temp_dir().join(format!(
            "axcenity-mirror-{}-{}",
            std::process::id(),
            FOLDERS.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Folder(path)
    }

    fn write(&self, path: &str, text: &str) {
        let full = self.0.join(path);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(full, text).unwrap();
    }

    fn read(&self, path: &str) -> Option<String> {
        std::fs::read_to_string(self.0.join(path)).ok()
    }
}

impl Drop for Folder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The owner's project folder (hosted from), and a session with the owner
/// and an editor, each with a mirror (the owner's is the project folder).
fn mirrored() -> (Net, Folder, Folder, Vec<Mirror>) {
    let project = Folder::new();
    project.write(DOOR, "print('door')");
    project.write("assets/notes.txt", "notes");
    project.write("world.json", "{}");
    let (store, manifest) = FolderStore::open(&project.0).unwrap();
    let host = Host::new(World::new()).with_files(manifest, Box::new(store));
    let net = Net::with_host(host, &[Role::Owner, Role::Editor]);
    let theirs = Folder::new();
    let mirrors = vec![
        Mirror::open(&project.0, "test", "Owner"),
        Mirror::open(&theirs.0, "test", "Sam"),
    ];
    (net, project, theirs, mirrors)
}

/// Syncs every mirror and delivers everything, until nothing changes.
fn sync_all(net: &mut Net, mirrors: &mut [Mirror]) -> Vec<Change> {
    let mut all = Vec::new();
    for _ in 0..6 {
        for (i, mirror) in mirrors.iter_mut().enumerate() {
            let changes = net.with_client(i, |client, world| mirror.sync(client, world));
            all.extend(changes.unwrap());
        }
        net.settle();
    }
    all
}

fn touch_later() {
    // File times can be coarse; make sure a rewrite looks changed.
    std::thread::sleep(Duration::from_millis(20));
}

#[test]
fn a_new_member_gets_the_files_and_their_saves_come_back() {
    let (mut net, project, theirs, mut mirrors) = mirrored();
    sync_all(&mut net, &mut mirrors);
    assert_eq!(theirs.read(DOOR).as_deref(), Some("print('door')"));
    assert_eq!(theirs.read("assets/notes.txt").as_deref(), Some("notes"));
    // The World's own file isn't one of the session's files.
    assert_eq!(theirs.read("world.json"), None);
    assert!(!mirrors[1].is_catching_up(&net.clients[1]));

    touch_later();
    theirs.write(DOOR, "print('open')");
    theirs.write("scripts/New.module.lua", "return {}");
    std::fs::remove_file(theirs.0.join("assets/notes.txt")).unwrap();
    sync_all(&mut net, &mut mirrors);
    assert_eq!(project.read(DOOR).as_deref(), Some("print('open')"));
    assert_eq!(
        project.read("scripts/New.module.lua").as_deref(),
        Some("return {}")
    );
    assert_eq!(project.read("assets/notes.txt"), None);
    net.assert_converged();
}

#[test]
fn a_lost_save_is_kept_as_a_conflict_copy() {
    let (mut net, project, theirs, mut mirrors) = mirrored();
    sync_all(&mut net, &mut mirrors);
    touch_later();
    project.write(DOOR, "owner's");
    theirs.write(DOOR, "sam's");
    // Both publish before either hears of the other.
    for (i, mirror) in mirrors.iter_mut().enumerate() {
        net.with_client(i, |client, world| mirror.sync(client, world))
            .unwrap();
    }
    let changes = sync_all(&mut net, &mut mirrors);
    let copy = "scripts/Door.server.conflict-sam.lua";
    assert!(
        changes.contains(&Change::Conflict {
            path: DOOR.into(),
            copy: copy.into()
        }),
        "{changes:?}"
    );
    for folder in [&project, &theirs] {
        assert_eq!(folder.read(DOOR).as_deref(), Some("owner's"));
        assert_eq!(folder.read(copy).as_deref(), Some("sam's"));
    }
    net.assert_converged();
}

#[test]
fn a_mirror_publishes_what_changed_while_it_was_away() {
    let (mut net, project, theirs, mut mirrors) = mirrored();
    sync_all(&mut net, &mut mirrors);
    // Sam's editor closes, Sam edits, and the editor opens the same
    // mirror again.
    net.disconnect(1);
    drop(mirrors.pop());
    touch_later();
    theirs.write("assets/notes.txt", "more notes");
    net.connect(1);
    net.settle();
    mirrors.push(Mirror::open(&theirs.0, "test", "Sam"));
    sync_all(&mut net, &mut mirrors);
    assert_eq!(
        project.read("assets/notes.txt").as_deref(),
        Some("more notes")
    );
    // A mirror of another session starts afresh and takes the session's.
    let mut other = Mirror::open(&theirs.0, "another", "Sam");
    let changes = net.with_client(1, |client, world| other.sync(client, world));
    assert!(changes.unwrap().is_empty());
}

#[test]
fn a_file_being_edited_elsewhere_waits() {
    let (mut net, project, theirs, mut mirrors) = mirrored();
    sync_all(&mut net, &mut mirrors);
    net.with_client(0, |client, _| client.lock(DOOR));
    net.settle();
    touch_later();
    theirs.write(DOOR, "sam's");
    sync_all(&mut net, &mut mirrors);
    assert_eq!(project.read(DOOR).as_deref(), Some("print('door')"));
    net.with_client(0, |client, _| client.unlock(DOOR));
    net.settle();
    sync_all(&mut net, &mut mirrors);
    assert_eq!(project.read(DOOR).as_deref(), Some("sam's"));
}
