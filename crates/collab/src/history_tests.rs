//! The host's checkpoints (C9): made when the session starts, every hour of
//! changes and when a member asks; kept by file hash; and restored for
//! everyone as one transaction. Every message goes through `wire`.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use axcenity_core::{ClassName, Value, World};

use crate::apply::Doc;
use crate::client::Notice;
use crate::files::{FileHash, FolderStore, MemoryStore};
use crate::history::{
    self, AUTOMATIC_KEPT, CHECKPOINT_EVERY, Checkpoint, CheckpointKind, History, restore_ops,
};
use crate::host::Host;
use crate::message::{ToClient, ToHost};
use crate::role::Role;
use crate::testing::*;
use crate::wire;

/// A project folder that's deleted when the test ends.
struct Project(PathBuf);

impl Project {
    fn new(name: &str) -> Project {
        let path =
            std::env::temp_dir().join(format!("axcenity-history-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(path.join("scripts")).unwrap();
        std::fs::write(path.join("scripts/Door.server.lua"), "print('door')\n").unwrap();
        std::fs::write(path.join("logo.png"), [1, 2, 3, 4]).unwrap();
        Project(path)
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// An owner, an editor and a viewer in a session of `project`, whose files
/// are read from its folder as the hosting editor does, with checkpoints.
fn session(project: &Project) -> Net {
    let (store, manifest) = FolderStore::open(&project.0).unwrap();
    let host = Host::new(World::new()).with_files(manifest, Box::new(store));
    let mut net = Net::with_host(host, &[Role::Owner, Role::Editor, Role::Viewer]);
    net.host.start_journal(&project.0).unwrap();
    net
}

/// The session's tree and files, as the host has them (deleted Instances
/// left out: a restore leaves them deleted, not as they were).
fn state(net: &Net) -> String {
    let doc = net.host.doc();
    dump(&doc.world, &Default::default()) + &dump_files(&doc.files)
}

fn checkpoints(net: &Net, i: usize) -> Vec<Checkpoint> {
    net.clients[i].checkpoints().to_vec()
}

fn history_notices(net: &mut Net, i: usize) -> Vec<String> {
    net.clients[i]
        .take_notices()
        .into_iter()
        .filter_map(|notice| match notice {
            Notice::History(text) => Some(text),
            _ => None,
        })
        .collect()
}

#[test]
fn a_restore_puts_the_world_and_files_back_for_everyone_and_can_be_taken_back() {
    let project = Project::new("restore");
    let mut net = session(&project);
    let workspace = net.host.world().workspace();

    // The doors go in, with a link, an attribute and a tag.
    let door = net.new_id(1);
    let frame = net.new_id(1);
    let link = net.new_id(1);
    let mut door_node = part(door, workspace, "Door");
    door_node.attributes = vec![("Locked".into(), Value::Bool(true))];
    door_node.tags = vec!["Doors".into()];
    let mut link_node = part(link, workspace, "Link");
    link_node.class = ClassName::ObjectValue;
    link_node.properties = vec![("Value".into(), Value::Instance(Some(door)))];
    net.submit(
        1,
        "Insert doors",
        vec![create(vec![
            door_node,
            part(frame, door, "Frame"),
            link_node,
        ])],
    )
    .unwrap();
    net.publish(
        1,
        "Save Door",
        &[("scripts/Door.server.lua", Some(b"print('open')\n"))],
    )
    .unwrap();
    net.settle();
    net.with_client(1, |client, _| client.save_checkpoint("  Doors in\n"))
        .unwrap();
    net.settle();
    let list = checkpoints(&net, 2);
    assert_eq!(list.len(), 2, "the session's start and this one");
    assert_eq!(list[0].label, "Doors in");
    assert_eq!(list[0].kind, CheckpointKind::Saved);
    assert_eq!(list[0].edited_by.len(), 1, "member 1 changed things since");
    assert_eq!(list[1].kind, CheckpointKind::SessionStart);
    let doors_in = list[0].id;
    let before = state(&net);

    // Then it goes wrong: the door is deleted, its frame moved out first,
    // a stray part added, the script changed and the logo deleted, on disk
    // too (as the owner's own editor would).
    net.submit(
        1,
        "Rearrange",
        vec![
            move_to(frame, workspace),
            delete(door),
            rename(frame, "Loose"),
        ],
    )
    .unwrap();
    let stray = net.new_id(1);
    net.submit(
        1,
        "Insert",
        vec![create(vec![part(stray, workspace, "Stray")])],
    )
    .unwrap();
    net.publish(
        1,
        "Save",
        &[
            ("scripts/Door.server.lua", Some(b"error('broken')\n")),
            ("logo.png", None),
        ],
    )
    .unwrap();
    net.settle();
    std::fs::remove_file(project.0.join("logo.png")).unwrap();
    std::fs::write(
        project.0.join("scripts/Door.server.lua"),
        "error('broken')\n",
    )
    .unwrap();
    let after = state(&net);
    assert_ne!(before, after);

    // Only the owner restores.
    assert!(
        net.with_client(2, |client, _| client.restore_checkpoint(doors_in))
            .is_err()
    );
    let out = net.host.receive(member(1), ToHost::Restore(doors_in));
    assert!(matches!(&out[..], [o] if matches!(o.message, ToClient::HistoryNotice(_))));
    net.route_out(out);
    net.settle();
    assert_eq!(history_notices(&mut net, 1).len(), 1);
    assert_eq!(state(&net), after);

    // The owner restores: one transaction, the same for everyone.
    let seq = net.host.seq();
    net.with_client(0, |client, _| client.restore_checkpoint(doors_in))
        .unwrap();
    net.settle();
    assert_eq!(net.host.seq(), seq + 1, "one transaction");
    assert_eq!(state(&net), before);
    net.assert_converged();
    let logged = net.host.log().last().unwrap();
    assert_eq!(logged.label, "Restore \u{201c}Doors in\u{201d}");
    // The logo's bytes came back from the checkpoint's own copy.
    let logo = net.host.files()["logo.png"];
    assert_eq!(
        net.host.read_file(logo).as_deref(),
        Some(&[1u8, 2, 3, 4][..])
    );
    // The link points at the door again (it was deleted, not destroyed,
    // so it keeps its ID).
    let world = net.host.world();
    assert_eq!(
        world.get(link).unwrap().property("Value"),
        Some(&Value::Instance(Some(door)))
    );

    // The session as it was before the restore was kept, and restoring it
    // takes the restore back.
    let list = checkpoints(&net, 2);
    assert_eq!(list[0].kind, CheckpointKind::BeforeRestore);
    assert_eq!(list[0].label, "Before restoring \u{201c}Doors in\u{201d}");
    assert_eq!(
        list[0].by.as_deref(),
        Some(net.host.roster()[0].profile.name.as_str())
    );
    let undo = list[0].id;
    net.with_client(0, |client, _| client.restore_checkpoint(undo))
        .unwrap();
    net.settle();
    assert_eq!(state(&net), after);
    net.assert_converged();
}

#[test]
fn instances_destroyed_since_come_back_at_new_ids_with_what_points_at_them() {
    let mut then = World::new();
    then.make_replica();
    let workspace = then.workspace();
    let base = then.next_unused_index();
    let id = |n: u32| axcenity_core::InstanceId::from_bits(u64::from(base + n));
    let now = Doc::new(then.clone());
    let mut then_doc = Doc::new(then);
    let mut link = part(id(1), workspace, "Link");
    link.class = ClassName::ObjectValue;
    link.properties = vec![("Value".into(), Value::Instance(Some(id(0))))];
    crate::apply::apply(
        &mut then_doc.view(),
        &[create(vec![
            part(id(0), workspace, "Door"),
            part(id(2), id(0), "Frame"),
            link,
        ])],
        &|_| true,
    )
    .unwrap();
    // `now` never had them (a World from before they were made).
    let mut next = 5000;
    let mut fresh = || {
        next += 1;
        axcenity_core::InstanceId::from_bits(next)
    };
    let restore = restore_ops(
        &now,
        &then_doc.world,
        &Default::default(),
        &mut fresh,
        &|_| true,
    )
    .unwrap();
    let mut restored = now.clone();
    crate::apply::apply(&mut restored.view(), &restore.ops, &|_| true).unwrap();
    let world = &restored.world;
    let door = world.find_first_child(workspace, "Door").unwrap();
    assert!(door.to_bits() > 5000, "a new ID");
    assert!(world.find_first_child(door, "Frame").is_some());
    let link = world.find_first_child(workspace, "Link").unwrap();
    assert_eq!(
        world.get(link).unwrap().property("Value"),
        Some(&Value::Instance(Some(door)))
    );
    // The same tree, IDs aside.
    let names = |world: &World| {
        world
            .descendants(world.workspace())
            .into_iter()
            .map(|id| world.get(id).unwrap().name().to_owned())
            .collect::<Vec<_>>()
    };
    assert_eq!(names(world), names(&then_doc.world));
}

#[test]
fn an_hour_of_changes_makes_a_checkpoint_and_an_idle_hour_doesnt() {
    let project = Project::new("hourly");
    let mut net = session(&project);
    let start = Instant::now();
    net.tick(start + CHECKPOINT_EVERY + Duration::from_secs(1));
    net.settle();
    assert!(
        checkpoints(&net, 1).is_empty(),
        "nothing changed: nothing new to list"
    );
    let id = net.new_id(1);
    let workspace = net.host.world().workspace();
    net.submit(1, "Insert", vec![create(vec![part(id, workspace, "A")])])
        .unwrap();
    net.settle();
    net.tick(start + CHECKPOINT_EVERY * 2 + Duration::from_secs(2));
    net.settle();
    let list = checkpoints(&net, 1);
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].kind, CheckpointKind::Hourly);
    // A member joining later is told about them.
    net.disconnect(2);
    net.restart_editor(2);
    net.connect(2);
    net.settle();
    assert_eq!(checkpoints(&net, 2), list);
}

#[test]
fn viewers_cant_save_checkpoints_and_labels_are_one_short_line() {
    let project = Project::new("labels");
    let mut net = session(&project);
    assert!(
        net.with_client(2, |c, _| c.save_checkpoint("mine"))
            .is_err()
    );
    let out = net
        .host
        .receive(member(2), ToHost::Checkpoint("mine".into()));
    assert!(matches!(&out[..], [o] if matches!(o.message, ToClient::HistoryNotice(_))));
    let long = format!("a\u{7}b{}", "x".repeat(500));
    net.with_client(1, |c, _| c.save_checkpoint(&long)).unwrap();
    net.with_client(0, |c, _| c.save_checkpoint("   ")).unwrap();
    net.settle();
    let list = checkpoints(&net, 2);
    assert!(list.iter().any(|c| c.label == "Checkpoint"));
    let long = list.iter().find(|c| c.label.starts_with("a b")).unwrap();
    assert_eq!(long.label.chars().count(), history::LABEL_MOST);
}

#[test]
fn contents_are_kept_once_by_hash_and_old_automatic_checkpoints_go() {
    let project = Project::new("prune");
    let history = History::of(&project.0);
    let (store, files) = MemoryStore::with(&[("a.txt", b"same"), ("b.txt", b"same")]);
    let mut doc = Doc::new(World::new());
    doc.files = files;
    let checkpoint = |kind, n: u64| Checkpoint {
        id: n,
        at: n,
        kind,
        label: format!("{n}"),
        by: None,
        edited_by: Vec::new(),
        seq: n,
    };
    let kept = history
        .record(&doc, &store, checkpoint(CheckpointKind::Saved, 1), true)
        .unwrap();
    assert!(kept.is_some());
    // Unchanged: not kept again.
    assert!(
        history
            .record(&doc, &store, checkpoint(CheckpointKind::Hourly, 2), true)
            .unwrap()
            .is_none()
    );
    for n in 0..AUTOMATIC_KEPT as u64 + 3 {
        let workspace = doc.world.workspace();
        doc.world
            .set_name(workspace, format!("Workspace{n}"))
            .unwrap();
        history
            .record(
                &doc,
                &store,
                checkpoint(CheckpointKind::Hourly, 10 + n),
                true,
            )
            .unwrap()
            .unwrap();
    }
    let list = history.list();
    assert_eq!(list.len(), AUTOMATIC_KEPT + 1, "the saved one stays");
    assert_eq!(list.last().unwrap().kind, CheckpointKind::Saved);
    let blobs = std::fs::read_dir(project.0.join(".axcenity/teamcreate/history/files"))
        .unwrap()
        .count();
    assert_eq!(blobs, 1, "two files with the same contents, kept once");
    assert!(history.read_file(FileHash::of(b"same")).is_some());
    let (_, files) = history.load(list[0].id).unwrap();
    assert_eq!(files, doc.files);
}

#[test]
fn history_messages_cross_the_wire() {
    let up = [
        ToHost::Checkpoint("Before the boss".into()),
        ToHost::Restore(42),
    ];
    for message in up {
        let back = wire::decode_to_host(&wire::encode_to_host(&message)).unwrap();
        assert_eq!(format!("{back:?}"), format!("{message:?}"));
    }
    let list = vec![Checkpoint {
        id: 1,
        at: 2,
        kind: CheckpointKind::BeforeRestore,
        label: "x".into(),
        by: Some("Paul".into()),
        edited_by: vec!["Sam".into()],
        seq: 3,
    }];
    let down = [
        ToClient::Checkpoints(list),
        ToClient::HistoryNotice("no".into()),
    ];
    for message in down {
        let back = wire::decode_to_client(&wire::encode_to_client(&message)).unwrap();
        assert_eq!(format!("{back:?}"), format!("{message:?}"));
    }
}
