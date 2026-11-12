//! C10's security checks: every message a member can send, from a Viewer
//! and from someone taken out of the project; each budget; the audit log.
//! The handshake's replay tests are in `net_tests.rs`, the wire decoder's
//! fuzzing in `wire_fuzz_tests.rs`.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use axcenity_core::World;

use crate::audit::AuditKind;
use crate::files::{FileHash, FolderStore, Piece};
use crate::host::Host;
use crate::limits::{self, Budget, Kind};
use crate::live::LiveDoc;
use crate::message::{ToClient, ToHost};
use crate::op::{Op, Transaction};
use crate::presence::Presence;
use crate::role::Role;
use crate::testing::*;

const SCRIPT: &str = "scripts/Door.server.lua";

/// A project folder that's deleted when the test ends.
struct Project(PathBuf);

impl Project {
    fn new(name: &str) -> Project {
        let path =
            std::env::temp_dir().join(format!("axcenity-security-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(path.join("scripts")).unwrap();
        std::fs::write(path.join(SCRIPT), "print('door')\n").unwrap();
        std::fs::write(path.join("logo.png"), [1, 2, 3, 4]).unwrap();
        Project(path)
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// An Owner, an Editor and a Viewer in a session of `project` with
/// checkpoints, a part and the script open as a live document.
fn session(project: &Project, host: impl FnOnce(Host) -> Host) -> (Net, axcenity_core::InstanceId) {
    let (store, manifest) = FolderStore::open(&project.0).unwrap();
    let host = host(Host::new(World::new()).with_files(manifest, Box::new(store)));
    let mut net = Net::with_host(host, &[Role::Owner, Role::Editor, Role::Viewer]);
    net.host.start_journal(&project.0).unwrap();
    let id = net.new_id(0);
    let workspace = net.world_mut(0).workspace();
    net.submit(
        0,
        "Add Crate",
        vec![create(vec![part(id, workspace, "Crate")])],
    )
    .unwrap();
    let out = net.host.receive(
        member(0),
        ToHost::DocOpen {
            path: SCRIPT.into(),
            state_vector: Vec::new(),
        },
    );
    net.script_state = out
        .iter()
        .find_map(|o| match &o.message {
            ToClient::DocState { update, .. } => Some(update.clone()),
            _ => None,
        })
        .expect("the script's state");
    net.route_out(out);
    net.with_client(0, |c, _| c.save_checkpoint("Start"))
        .unwrap();
    net.settle();
    assert!(!net.clients[0].checkpoints().is_empty());
    (net, id)
}

/// Everything the host keeps that a member's message could change.
fn state(net: &Net) -> String {
    let doc = net.host.doc();
    format!(
        "{}{}locks {:?}\nscript {:?}\nseq {}\ncheckpoints {}\n",
        dump(&doc.world, &doc.graveyard),
        dump_files(&doc.files),
        net.host.locks(),
        net.host.doc_text(SCRIPT),
        net.host.seq(),
        net.clients[0].checkpoints().len(),
    )
}

/// One of every message a member can send, aimed at what's in the session.
fn every_message(net: &Net, part: axcenity_core::InstanceId) -> Vec<ToHost> {
    let new_file = b"new".to_vec();
    let hash = FileHash::of(&new_file);
    let logo = net.host.doc().files.get("logo.png").copied().unwrap();
    let checkpoint = net.clients[0].checkpoints()[0].id;
    let messages = vec![
        ToHost::Hello { have: None },
        ToHost::Submit(Transaction {
            client_seq: 1_000,
            label: "Rename".into(),
            ops: vec![rename(part, "Changed")],
        }),
        ToHost::NeedIds,
        ToHost::Presence(Presence::default()),
        ToHost::Upload {
            hash,
            at: 0,
            total: new_file.len() as u64,
            bytes: Piece(new_file.into()),
        },
        ToHost::Submit(Transaction {
            client_seq: 1_001,
            label: "Save".into(),
            ops: vec![Op::FileVersion {
                path: "scripts/New.lua".into(),
                base: None,
                new: Some(hash),
            }],
        }),
        ToHost::Fetch(vec![logo]),
        ToHost::Lock {
            path: SCRIPT.into(),
        },
        ToHost::Unlock {
            path: SCRIPT.into(),
        },
        ToHost::DocOpen {
            path: SCRIPT.into(),
            state_vector: Vec::new(),
        },
        ToHost::DocUpdate {
            path: SCRIPT.into(),
            update: typed_update(net),
        },
        ToHost::Chat("hello".into()),
        ToHost::Checkpoint("mine".into()),
        ToHost::Restore(checkpoint),
        ToHost::AuditLog,
        ToHost::HostCard {
            address: "10.0.0.9:47810".into(),
            cert: crate::identity::Fingerprint([3; 32]),
        },
        // A tail nobody asked for, which would rename the part.
        ToHost::Tail(vec![crate::message::TailEntry {
            seq: 1,
            author: member(1),
            client_seq: 1,
            label: "Rename".into(),
            ops: vec![rename(part, "FromTail")],
        }]),
    ];
    // Every kind is here: a new one fails to compile until it's added.
    let mut kinds: Vec<&str> = messages.iter().map(name).collect();
    kinds.sort_unstable();
    kinds.dedup();
    assert_eq!(kinds.len(), 16);
    messages
}

fn name(message: &ToHost) -> &'static str {
    match message {
        ToHost::Hello { .. } => "Hello",
        ToHost::Submit(_) => "Submit",
        ToHost::NeedIds => "NeedIds",
        ToHost::Presence(_) => "Presence",
        ToHost::Upload { .. } => "Upload",
        ToHost::Fetch(_) => "Fetch",
        ToHost::Lock { .. } => "Lock",
        ToHost::Unlock { .. } => "Unlock",
        ToHost::DocOpen { .. } => "DocOpen",
        ToHost::DocUpdate { .. } => "DocUpdate",
        ToHost::Chat(_) => "Chat",
        ToHost::Checkpoint(_) => "Checkpoint",
        ToHost::Restore(_) => "Restore",
        ToHost::AuditLog => "AuditLog",
        ToHost::HostCard { .. } => "HostCard",
        ToHost::Tail(_) => "Tail",
    }
}

/// A real edit of the script's live document: text typed at its start,
/// made on a copy of the state the Owner was sent.
fn typed_update(net: &Net) -> Vec<u8> {
    let mut copy = LiveDoc::from_state(&net.script_state).unwrap();
    let text = copy.text();
    copy.set_text(&format!("-- typed\n{text}"))
        .expect("a change")
}

#[test]
fn a_viewer_changes_nothing_with_any_message() {
    let project = Project::new("viewer");
    let (mut net, part) = session(&project, |host| host);
    let viewer = member(2);
    for message in every_message(&net, part) {
        let what = name(&message);
        let before = state(&net);
        let out = net.host.receive(viewer, message);
        assert_eq!(state(&net), before, "a Viewer's {what} changed the session");
        for o in &out {
            assert!(
                o.to == viewer
                    || matches!(
                        o.message,
                        ToClient::Roster(_)
                            | ToClient::Presence { .. }
                            | ToClient::Chat(_)
                            // Who's online changed: so may who hosts next.
                            | ToClient::Successors { .. }
                    ),
                "a Viewer's {what} sent {:?} to someone else",
                o.message
            );
            assert!(
                !matches!(o.message, ToClient::Ids(_) | ToClient::AuditLog(_)),
                "a Viewer's {what} got {:?}",
                o.message
            );
        }
        net.route_out(out);
        net.settle();
    }
    // What they asked for and may not have is in the audit log.
    let denied: Vec<String> = net
        .host
        .audit_log()
        .into_iter()
        .filter(|e| e.kind == AuditKind::Denied && e.member == Some(viewer))
        .map(|e| e.text)
        .collect();
    for what in [
        "an edit",
        "new Instance IDs",
        "a file upload",
        "a document lock",
        "a live document edit",
        "a checkpoint",
        "a restore",
        "the audit log",
    ] {
        assert!(
            denied.iter().any(|text| text.contains(what)),
            "{what} isn't in {denied:#?}"
        );
    }

    // The same messages from the Editor do change it (the check above
    // isn't empty), except what only the Owner may do.
    let before = state(&net);
    let out = net.host.receive(
        member(1),
        ToHost::Submit(Transaction {
            client_seq: 1,
            label: "Rename".into(),
            ops: vec![rename(part, "Changed")],
        }),
    );
    assert!(matches!(out[0].message, ToClient::Accepted { .. }));
    assert_ne!(state(&net), before);
}

#[test]
fn someone_taken_out_of_the_project_is_ignored() {
    let project = Project::new("removed");
    let (mut net, part) = session(&project, |host| host);
    let messages = every_message(&net, part);
    let out = net.host.remove_member(member(1));
    net.route_out(out);
    net.settle();
    let before = state(&net);
    for message in messages {
        let what = name(&message);
        let out = net.host.receive(member(1), message);
        assert!(
            out.is_empty(),
            "a removed member's {what} was answered: {out:?}"
        );
        assert_eq!(
            state(&net),
            before,
            "a removed member's {what} changed the session"
        );
    }
    let removed = net.host.audit_log();
    assert!(
        removed
            .iter()
            .any(|e| e.kind == AuditKind::Removed && e.member == Some(member(1)))
    );
}

#[test]
fn a_flood_of_one_kind_is_dropped_and_written_down_once() {
    let project = Project::new("flood");
    let (mut net, _) = session(&project, |host| host);
    let editor = member(1);
    let burst = limits::budget(Kind::Lock).burst as usize;
    let mut answered = 0;
    for n in 0..burst * 3 {
        let message = if n % 2 == 0 {
            ToHost::Lock {
                path: SCRIPT.into(),
            }
        } else {
            ToHost::Unlock {
                path: SCRIPT.into(),
            }
        };
        if !net.host.receive(editor, message).is_empty() {
            answered += 1;
        }
    }
    assert!(answered <= burst, "{answered} of {} answered", burst * 3);
    assert!(answered >= burst - 2, "only {answered} answered");
    let limited = net
        .host
        .audit_log()
        .into_iter()
        .filter(|e| e.kind == AuditKind::Limited && e.member == Some(editor))
        .count();
    assert_eq!(limited, 1, "one line for the whole flood");
    // Other kinds aren't held up.
    let out = net.host.receive(editor, ToHost::Chat("still here".into()));
    assert!(!out.is_empty());
}

#[test]
fn hellos_past_their_budget_close_the_connection() {
    let project = Project::new("hellos");
    let (mut net, _) = session(&project, |host| host);
    let editor = member(1);
    let mut closed = false;
    for _ in 0..limits::budget(Kind::Hello).burst as usize + 5 {
        let _ = net.host.receive(editor, ToHost::Hello { have: None });
        closed |= net.host.take_closing().contains(&editor);
    }
    assert!(closed);
}

#[test]
fn checkpoints_asked_for_too_often_are_refused_with_a_reason() {
    let project = Project::new("checkpoints");
    let (mut net, _) = session(&project, |host| host);
    let before = net.clients[0].checkpoints().len();
    let mut notices = 0;
    for n in 0..10 {
        let out = net
            .host
            .receive(member(1), ToHost::Checkpoint(format!("mine {n}")));
        notices += out
            .iter()
            .filter(|o| matches!(&o.message, ToClient::HistoryNotice(t) if t.contains("too often")))
            .count();
        net.route_out(out);
        net.settle();
    }
    let burst = limits::budget(Kind::Checkpoint).burst as usize;
    assert_eq!(net.clients[0].checkpoints().len(), before + burst);
    assert_eq!(notices, 10 - burst);
}

/// Budgets small enough to see the download budget work.
fn small_downloads(kind: Kind) -> Budget {
    match kind {
        Kind::FetchBytes => Budget {
            per_second: 4.0,
            burst: 4.0,
        },
        other => limits::budget(other),
    }
}

#[test]
fn downloads_past_their_budget_wait_for_it() {
    let project = Project::new("downloads");
    // logo.png is 4 bytes: one fits the budget at a time.
    let (mut net, _) = session(&project, |host| host.with_budgets(small_downloads));
    let logo = net.host.doc().files.get("logo.png").copied().unwrap();
    let files = |out: &[crate::host::Outgoing]| {
        out.iter()
            .filter(|o| matches!(o.message, ToClient::File { .. }))
            .count()
    };
    let out = net
        .host
        .receive(member(2), ToHost::Fetch(vec![logo, logo, logo]));
    assert_eq!(files(&out), 1);
    // The rest go as the budget refills.
    let soon = Instant::now() + Duration::from_secs(1);
    assert_eq!(files(&net.host.tick(soon)), 1);
    assert_eq!(files(&net.host.tick(soon)), 0);
    assert_eq!(files(&net.host.tick(soon + Duration::from_secs(5))), 1);
    // Nothing waits for someone who left.
    let _ = net.host.receive(member(2), ToHost::Fetch(vec![logo, logo]));
    net.disconnect(2);
    assert_eq!(files(&net.host.tick(soon + Duration::from_secs(60))), 0);
}

#[test]
fn only_a_few_uploads_at_once() {
    let project = Project::new("uploads");
    let (mut net, _) = session(&project, |host| host);
    let editor = member(1);
    let files: Vec<Vec<u8>> = (0..limits::UPLOADS_AT_ONCE as u8 + 1)
        .map(|n| vec![n; 2])
        .collect();
    // Each started, none finished.
    for bytes in &files {
        let _ = net.host.receive(
            editor,
            ToHost::Upload {
                hash: FileHash::of(bytes),
                at: 0,
                total: 2,
                bytes: Piece(bytes[..1].to_vec().into()),
            },
        );
    }
    for bytes in &files {
        let _ = net.host.receive(
            editor,
            ToHost::Upload {
                hash: FileHash::of(bytes),
                at: 1,
                total: 2,
                bytes: Piece(bytes[1..].to_vec().into()),
            },
        );
    }
    let kept: Vec<bool> = files
        .iter()
        .map(|bytes| net.host.read_file(FileHash::of(bytes)).is_some())
        .collect();
    let mut expected = vec![true; limits::UPLOADS_AT_ONCE];
    expected.push(false);
    assert_eq!(kept, expected);
}

#[test]
fn the_owner_reads_the_audit_log() {
    let project = Project::new("audit");
    let (mut net, _) = session(&project, |host| host);
    net.set_role(1, Role::Viewer);
    net.settle();
    assert!(!net.with_client(1, |c, _| c.ask_for_audit_log()));
    assert!(net.with_client(0, |c, _| c.ask_for_audit_log()));
    net.settle();
    let log = net.clients[0].audit_log();
    assert!(
        log.iter()
            .any(|e| e.kind == AuditKind::Role && e.member == Some(member(1))),
        "{log:#?}"
    );
}
