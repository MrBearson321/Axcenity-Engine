//! Live documents in a session (C7): members open a script, type in it at
//! once, the host checks who may type and what they send, writes settled
//! typing back to the file, and copies typed offline or across a host
//! restart merge when they're back. Every message goes through `wire`.

use std::time::{Duration, Instant};

use axcenity_core::World;

use crate::client::Notice;
use crate::files::{FileHash, MemoryStore};
use crate::host::Host;
use crate::live::{UPDATE_MOST, WRITE_BACK_AFTER};
use crate::message::{ToClient, ToHost};
use crate::role::Role;
use crate::testing::*;

const DOOR: &str = "scripts/Door.server.lua";
const START: &str = "local door = script.Parent\nprint('door')\n";

fn host_with(text: &str) -> Host {
    let (store, manifest) = MemoryStore::with(&[(DOOR, text.as_bytes())]);
    Host::new(World::new()).with_files(manifest, Box::new(store))
}

/// An owner, two editors and a viewer, in a project with one script that
/// everyone has open.
fn session() -> Net {
    let mut net = Net::with_host(
        host_with(START),
        &[Role::Owner, Role::Editor, Role::Editor, Role::Viewer],
    );
    for i in 0..4 {
        net.with_client(i, |client, _| client.open_doc(DOOR));
    }
    net.settle();
    net
}

fn text(net: &Net, i: usize) -> String {
    net.clients[i]
        .doc_text(DOOR)
        .expect("the document is ready")
}

fn type_in(net: &mut Net, i: usize, text: &str) {
    assert!(net.with_client(i, |client, _| client.edit_doc(DOOR, text)));
}

fn doc_refusals(net: &mut Net, i: usize) -> Vec<String> {
    net.clients[i]
        .take_notices()
        .into_iter()
        .filter_map(|n| match n {
            Notice::DocRefused { reason, .. } => Some(reason),
            _ => None,
        })
        .collect()
}

fn assert_same_text(net: &Net) -> String {
    let host = net.host.doc_text(DOOR).expect("open on the host");
    for i in 0..net.clients.len() {
        if net.is_connected(i) {
            assert_eq!(text(net, i), host, "member {i}");
        }
    }
    host
}

#[test]
fn everyone_gets_the_files_text_once() {
    let net = session();
    assert_eq!(assert_same_text(&net), START);
}

#[test]
fn two_people_typing_at_once_both_keep_their_text() {
    let mut net = session();
    // Both type before either hears of the other.
    type_in(&mut net, 1, &START.replace("door')", "door opened')"));
    type_in(
        &mut net,
        2,
        &format!("{START}door.Touched:Connect(print)\n"),
    );
    net.settle();
    let merged = assert_same_text(&net);
    assert_eq!(
        merged,
        "local door = script.Parent\nprint('door opened')\ndoor.Touched:Connect(print)\n"
    );
    let changed = net.clients[3].take_doc_changes();
    assert_eq!(
        changed,
        vec![DOOR.to_owned()],
        "the viewer is told to redraw"
    );
}

#[test]
fn a_viewer_cant_type_and_gets_the_text_back() {
    let mut net = session();
    type_in(&mut net, 3, "-- scribbled\n");
    net.settle();
    let refused = doc_refusals(&mut net, 3);
    assert_eq!(refused.len(), 1, "{refused:?}");
    assert_eq!(assert_same_text(&net), START, "the viewer's copy reopened");
}

#[test]
fn too_big_garbled_or_unopened_changes_are_refused_whole() {
    let mut net = session();
    let refused = |net: &mut Net, path: &str, update: Vec<u8>| {
        let out = net.host.receive(
            member(1),
            ToHost::DocUpdate {
                path: path.to_owned(),
                update,
            },
        );
        assert_eq!(out.len(), 1);
        match &out[0].message {
            ToClient::DocRefused { reason, .. } => reason.clone(),
            other => panic!("{other:?}"),
        }
    };
    let big = refused(&mut net, DOOR, vec![0; UPDATE_MOST + 1]);
    assert!(big.contains("too big"), "{big}");
    let garbled = refused(&mut net, DOOR, vec![0xff, 0x13, 0x07]);
    assert!(garbled.contains("couldn't be read"), "{garbled}");
    let unopened = refused(&mut net, "scripts/Other.server.lua", vec![0, 0]);
    assert!(unopened.contains("open"), "{unopened}");
    assert_eq!(assert_same_text(&net), START);

    // Only scripts and stylesheets are live; paths are checked as files.
    for path in ["levels/main.level.json", "../outside.lua"] {
        let out = net.host.receive(
            member(1),
            ToHost::DocOpen {
                path: path.to_owned(),
                state_vector: Vec::new(),
            },
        );
        assert!(
            matches!(&out[0].message, ToClient::DocRefused { .. }),
            "{path}"
        );
    }
}

#[test]
fn settled_typing_is_written_to_the_file_by_whoever_typed() {
    let mut net = session();
    let start = Instant::now();
    net.tick(start);
    let typed = format!("{START}-- more\n");
    type_in(&mut net, 2, &typed);
    net.settle();
    // Still typing: the file waits.
    net.tick(start + WRITE_BACK_AFTER / 2);
    net.settle();
    assert_eq!(
        net.host.files().get(DOOR),
        Some(&FileHash::of(START.as_bytes()))
    );

    net.tick(start + WRITE_BACK_AFTER + Duration::from_millis(10));
    net.settle();
    let hash = FileHash::of(typed.as_bytes());
    assert_eq!(net.host.files().get(DOOR), Some(&hash));
    for i in 0..4 {
        assert_eq!(net.clients[i].files().get(DOOR), Some(&hash), "member {i}");
    }
    net.assert_converged();
    // A write-back isn't anyone's own edit: nothing is confirmed or
    // refused, and it isn't written twice.
    assert!(net.clients[2].take_notices().is_empty());
    let before = net.host.files().clone();
    net.tick(start + WRITE_BACK_AFTER * 3);
    net.settle();
    assert_eq!(net.host.files(), &before);
}

#[test]
fn a_save_from_elsewhere_moves_the_open_document() {
    let mut net = session();
    let saved = "print('saved by an older editor')\n";
    net.publish(1, "Save Door", &[(DOOR, Some(saved.as_bytes()))])
        .expect("published");
    net.settle();
    assert_eq!(assert_same_text(&net), saved);
}

#[test]
fn typing_offline_merges_when_back() {
    let mut net = session();
    net.disconnect(1);
    // Typed while away is kept.
    type_in(&mut net, 1, &format!("-- away\n{START}"));
    type_in(&mut net, 2, &format!("{START}-- here\n"));
    net.settle();
    net.connect(1);
    net.settle();
    assert_eq!(assert_same_text(&net), format!("-- away\n{START}-- here\n"));
}

#[test]
fn after_the_host_restarts_a_members_text_is_kept() {
    let mut net = session();
    type_in(&mut net, 1, &format!("{START}-- not written yet\n"));
    net.settle();
    // The new run starts from the file, which never got the typing.
    net.restart_host();
    net.host = host_with(START);
    for (i, role) in [Role::Owner, Role::Editor, Role::Editor, Role::Viewer]
        .into_iter()
        .enumerate()
    {
        net.host.add_member(member(i), role);
    }
    for i in 0..4 {
        net.connect(i);
    }
    net.settle();
    assert_eq!(
        assert_same_text(&net),
        format!("{START}-- not written yet\n")
    );
}

#[test]
fn a_member_who_joins_later_gets_the_typing_so_far() {
    let mut net = session();
    type_in(&mut net, 1, "print('typed')\n");
    net.settle();
    net.restart_editor(2);
    net.connect(2);
    net.with_client(2, |client, _| client.open_doc(DOOR));
    net.settle();
    assert_eq!(text(&net, 2), "print('typed')\n");
}

/// A project folder that's deleted when the test ends.
struct Project(std::path::PathBuf);

impl Project {
    fn new(name: &str) -> Project {
        let path =
            std::env::temp_dir().join(format!("axcenity-live-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Project(path)
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const ROLES: [Role; 4] = [Role::Owner, Role::Editor, Role::Editor, Role::Viewer];

/// [`session`] with a journal in `project`; member 1 typed `-- a`, which
/// was written to the file (and kept beside the journal), then members 2
/// and 1 typed `-- b` and `-- c`, which weren't yet; then the host stopped
/// and started again with the file as `file_now`.
fn typed_then_crashed(project: &Project, file_now: &str) -> Net {
    let mut net = session();
    net.host.start_journal(&project.0).unwrap();
    let start = Instant::now();
    net.tick(start);
    type_in(&mut net, 1, &format!("{START}-- a\n"));
    net.settle();
    net.tick(start + WRITE_BACK_AFTER * 2);
    net.settle();
    type_in(&mut net, 2, &format!("-- b\n{START}-- a\n"));
    net.settle();
    type_in(&mut net, 1, &format!("-- b\n{START}-- a\n-- c\n"));
    net.settle();
    net.crash_host_into(&project.0, |world| {
        let (store, manifest) = MemoryStore::with(&[(DOOR, file_now.as_bytes())]);
        Host::new(world).with_files(manifest, Box::new(store))
    });
    for (i, role) in ROLES.into_iter().enumerate() {
        assert_eq!(net.host.role(member(i)), Some(role));
        net.connect(i);
    }
    net.settle();
    net
}

#[test]
fn a_host_that_crashes_carries_on_with_the_same_document() {
    let project = Project::new("crash");
    let net = typed_then_crashed(&project, &format!("{START}-- a\n"));
    // Everyone's copy merges back exactly: nothing typed twice or lost.
    assert_eq!(assert_same_text(&net), format!("-- b\n{START}-- a\n-- c\n"));
}

#[test]
fn a_file_saved_elsewhere_while_the_host_was_down_is_taken() {
    let project = Project::new("outside");
    let net = typed_then_crashed(&project, "print('saved outside')\n");
    let text = assert_same_text(&net);
    assert!(text.contains("print('saved outside')"), "{text}");
    assert!(!text.contains("-- a"), "{text}");
    assert_eq!(text.matches("-- b").count(), 1, "{text}");
    assert_eq!(text.matches("-- c").count(), 1, "{text}");
}

#[test]
fn ending_the_session_writes_typing_first_and_keeps_nothing() {
    let project = Project::new("end");
    let mut net = session();
    net.host.start_journal(&project.0).unwrap();
    let typed = format!("{START}-- last words\n");
    type_in(&mut net, 1, &typed);
    net.settle();
    let out = net.host.end();
    net.route_out(out);
    net.settle();
    let hash = FileHash::of(typed.as_bytes());
    assert_eq!(net.host.files().get(DOOR), Some(&hash));
    assert_eq!(net.clients[2].files().get(DOOR), Some(&hash));
    net.host.end_journal();
    assert!(crate::journal::saved_docs(&project.0).is_empty());
}

#[test]
fn undo_in_a_session_takes_back_only_your_own_typing() {
    let mut net = session();
    type_in(&mut net, 1, &format!("{START}-- mine\n"));
    net.settle();
    type_in(&mut net, 2, &format!("-- theirs\n{START}-- mine\n"));
    net.settle();
    assert!(net.with_client(1, |client, _| client.undo_doc(DOOR)));
    net.settle();
    assert_eq!(assert_same_text(&net), format!("-- theirs\n{START}"));
    assert!(net.with_client(1, |client, _| client.redo_doc(DOOR)));
    net.settle();
    assert_eq!(
        assert_same_text(&net),
        format!("-- theirs\n{START}-- mine\n")
    );
    // A viewer has nothing of their own to undo.
    assert!(!net.with_client(3, |client, _| client.undo_doc(DOOR)));
}

#[test]
fn document_messages_read_back_as_written() {
    use crate::message::MemberId;
    use crate::wire;
    let up = [
        ToHost::DocOpen {
            path: DOOR.into(),
            state_vector: vec![1, 2, 3],
        },
        ToHost::DocUpdate {
            path: DOOR.into(),
            update: vec![0; 300],
        },
    ];
    for message in up {
        let back = wire::decode_to_host(&wire::encode_to_host(&message)).unwrap();
        assert_eq!(format!("{back:?}"), format!("{message:?}"));
    }
    let down = [
        ToClient::DocState {
            path: DOOR.into(),
            update: vec![9; 40],
            state_vector: vec![4],
            origin: u64::MAX,
        },
        ToClient::DocUpdate {
            path: DOOR.into(),
            update: vec![5],
            author: MemberId(7),
        },
        ToClient::DocRefused {
            path: DOOR.into(),
            reason: "no".into(),
        },
    ];
    for message in down {
        let back = wire::decode_to_client(&wire::encode_to_client(&message)).unwrap();
        assert_eq!(format!("{back:?}"), format!("{message:?}"));
    }
}
