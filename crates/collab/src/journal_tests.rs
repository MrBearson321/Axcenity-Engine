//! The host's journal (TC6): what a host that stops without ending the
//! session accepted comes back, from its snapshot and the transactions
//! written after it.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use axcenity_core::{ClassName, Value, World};

use crate::host::Host;
use crate::journal::{self, SNAPSHOT_EVERY};
use crate::role::Role;
use crate::testing::*;

static FOLDERS: AtomicUsize = AtomicUsize::new(0);

/// A project folder that's deleted when the test ends.
struct Project(PathBuf);

impl Project {
    fn new() -> Project {
        let path = std::env::temp_dir().join(format!(
            "axcenity-journal-{}-{}",
            std::process::id(),
            FOLDERS.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Project(path)
    }

    fn journal(&self) -> PathBuf {
        self.0.join(journal::FOLDER).join("journal")
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A session with a journal in `project`: an owner and an editor, in a
/// world with one Part.
fn session(project: &Project) -> Net {
    let mut world = World::new();
    let part = world.create(ClassName::Part).unwrap();
    world.set_name(part, "Crate").unwrap();
    world.set_parent(part, Some(world.workspace())).unwrap();
    let mut host = Host::new(world);
    host.start_journal(&project.0).unwrap();
    Net::with_host(host, &[Role::Owner, Role::Editor])
}

/// Some edits from both members: a new Part, a rename, a property.
fn edit(net: &mut Net, round: usize) {
    let workspace = net.worlds[0].workspace();
    let id = net.new_id(0);
    net.submit(
        0,
        "Insert Part",
        vec![create(vec![part(id, workspace, &format!("Part{round}"))])],
    )
    .unwrap();
    net.settle();
    let crate_id = find(net, 1, "Crate").unwrap();
    net.submit(
        1,
        "Change Transparency",
        vec![set(
            crate_id,
            "Transparency",
            Value::Number(round as f64 / 10.0),
        )],
    )
    .unwrap();
    net.submit(1, "Rename", vec![rename(id, &format!("Renamed{round}"))])
        .unwrap();
    net.settle();
}

fn recovered(project: &Project) -> (String, usize) {
    let recovered = journal::recover(&project.0)
        .unwrap()
        .expect("a session to recover");
    (
        dump(&recovered.world, &recovered.graveyard),
        recovered.replayed,
    )
}

fn host_world(net: &Net) -> String {
    dump(&net.host.doc().world, &net.host.doc().graveyard)
}

#[test]
fn a_host_that_stops_without_ending_loses_nothing_it_accepted() {
    let project = Project::new();
    let mut net = session(&project);
    edit(&mut net, 1);
    edit(&mut net, 2);
    assert_eq!(net.host.seq(), 6);
    // The host stops here: no clean end.
    let (world, replayed) = recovered(&project);
    assert_eq!(world, host_world(&net));
    assert_eq!(replayed, 6);
    assert!(world.contains("Renamed2"));
}

#[test]
fn a_snapshot_empties_the_journal_and_recovery_starts_from_it() {
    let project = Project::new();
    let mut net = session(&project);
    edit(&mut net, 1);
    let before = std::fs::metadata(project.journal()).unwrap().len();
    assert!(before > 0);
    // Half a minute on, the host writes a snapshot.
    net.tick(Instant::now() + SNAPSHOT_EVERY + Duration::from_secs(1));
    assert_eq!(std::fs::metadata(project.journal()).unwrap().len(), 0);
    assert_eq!(recovered(&project), (host_world(&net), 0));
    edit(&mut net, 2);
    assert_eq!(recovered(&project), (host_world(&net), 3));
}

#[test]
fn entries_the_snapshot_already_has_are_skipped() {
    let project = Project::new();
    let mut net = session(&project);
    edit(&mut net, 1);
    // A host that stopped after writing a snapshot but before emptying
    // the journal: the snapshot has every entry, which mustn't apply twice.
    let journal = std::fs::read(project.journal()).unwrap();
    net.host.snapshot();
    std::fs::write(project.journal(), &journal).unwrap();
    assert_eq!(recovered(&project), (host_world(&net), 0));
}

#[test]
fn a_half_written_last_entry_is_left_out() {
    let project = Project::new();
    let mut net = session(&project);
    edit(&mut net, 1);
    let whole = host_world(&net);
    edit(&mut net, 2);
    // The host stopped while writing the last entry of round 2.
    let mut journal = std::fs::read(project.journal()).unwrap();
    let entries = 6;
    let mut at = 0;
    let mut starts = Vec::new();
    while at < journal.len() {
        starts.push(at);
        let length = u32::from_le_bytes(journal[at..at + 4].try_into().unwrap()) as usize;
        at += 4 + length;
    }
    assert_eq!(starts.len(), entries);
    journal.truncate(starts[5] + 7);
    std::fs::write(project.journal(), &journal).unwrap();
    let (world, replayed) = recovered(&project);
    assert_eq!(replayed, 5);
    assert_ne!(world, host_world(&net));
    assert_ne!(world, whole);
    assert!(world.contains("Part2"), "round 2's insert is in");
}

#[test]
fn ending_cleanly_leaves_nothing_to_recover() {
    let project = Project::new();
    let mut net = session(&project);
    edit(&mut net, 1);
    assert!(journal::has_recovery(&project.0));
    net.host.end_journal();
    assert!(!journal::has_recovery(&project.0));
    assert!(journal::recover(&project.0).unwrap().is_none());
    // Without a journal the session goes on.
    edit(&mut net, 2);
    net.assert_converged();
}

#[test]
fn unsent_edits_from_an_editor_that_stopped_are_sent_when_it_joins_again() {
    let mut world = World::new();
    let workspace = world.workspace();
    let a = world.create(ClassName::Part).unwrap();
    world.set_name(a, "A").unwrap();
    world.set_parent(a, Some(workspace)).unwrap();
    let mut net = Net::new(world, &[Role::Owner, Role::Editor]);
    let folder = Project::new();
    net.disconnect(1);
    let lamp = net.new_id(1);
    net.submit(
        1,
        "Add Lamp",
        vec![create(vec![part(lamp, workspace, "Lamp")])],
    )
    .unwrap();
    net.submit(
        1,
        "Dim",
        vec![set(lamp, "Transparency", Value::Number(0.25))],
    )
    .unwrap();
    net.submit(1, "Rename A", vec![rename(a, "Door")]).unwrap();
    journal::save_unsent(&folder.0, &net.clients[1].unsent()).unwrap();
    // The editor stops; the next run joins and finds the file.
    net.restart_editor(1);
    net.connect(1);
    net.settle();
    let unsent = journal::load_unsent(&folder.0).unwrap();
    assert_eq!(unsent.len(), 3);
    let count = net.with_client(1, |client, world| client.recover_unsent(world, unsent));
    assert_eq!(count, 3);
    net.settle();
    journal::discard_unsent(&folder.0).unwrap();
    assert!(journal::load_unsent(&folder.0).unwrap().is_empty());
    let lamp = find(&net, 0, "Lamp").expect("the Lamp reached the others");
    assert_eq!(
        net.host.world().get(lamp).unwrap().property("Transparency"),
        Some(&Value::Number(0.25))
    );
    assert_eq!(net.host.world().get(a).unwrap().name(), "Door");
    net.assert_converged();
}
