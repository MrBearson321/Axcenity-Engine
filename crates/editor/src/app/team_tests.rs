//! Team Create in the editor (TC2): two headless editors in one session
//! stay identical through ordinary editing, undo, and edits made at the
//! same time.

use std::fmt::Write;

use axcenity_collab::Role;
use axcenity_core::{ClassName, PartType, Value};

use super::team::SharedHub;
use super::tests::example;
use super::*;
use crate::properties::Edit;

/// Two editors on hello-part: the first hosts, the second joins as an
/// Editor.
fn two_editors() -> (EditorApp, EditorApp, SharedHub) {
    let mut a = EditorApp::new(Some(&example("hello-part")));
    let hub = a.host_team_session();
    let mut b = EditorApp::new(Some(&example("hello-part")));
    let member = hub.lock().unwrap().add_member(Role::Editor);
    b.join_team_session(hub.clone(), member);
    (a, b, hub)
}

/// Runs both editors' frames until nothing more moves.
fn settle(a: &mut EditorApp, b: &mut EditorApp) {
    for _ in 0..4 {
        a.team_frame();
        b.team_frame();
    }
}

/// Everything about the tree that must match on every copy (what
/// SplineMeshes make, and Instances only this computer has, left out).
fn dump(world: &World) -> String {
    let mut out = String::new();
    dump_tree(world, world.root(), 0, &mut out);
    out
}

fn dump_tree(world: &World, id: InstanceId, depth: usize, out: &mut String) {
    let Some(instance) = world.get(id) else {
        return;
    };
    let _ = write!(
        out,
        "{:indent$}{} {:?} {:?}",
        "",
        id.to_bits(),
        instance.class(),
        instance.name(),
        indent = depth * 2
    );
    for (key, value) in instance.properties() {
        let _ = write!(out, " {key}={value:?}");
    }
    for (key, value) in instance.attributes() {
        let _ = write!(out, " @{key}={value:?}");
    }
    for tag in instance.tags() {
        let _ = write!(out, " #{tag}");
    }
    out.push('\n');
    if axcenity_core::spline::is_generator(instance.class()) {
        return;
    }
    for &child in instance.children() {
        if !child.is_local() {
            dump_tree(world, child, depth + 1, out);
        }
    }
}

fn assert_same(a: &EditorApp, b: &EditorApp, hub: &SharedHub) {
    let host = dump(hub.lock().unwrap().host().world());
    assert_eq!(
        dump(&a.world),
        host,
        "the host's editor differs from the session"
    );
    assert_eq!(
        dump(&b.world),
        host,
        "the joined editor differs from the session"
    );
    assert_eq!(a.team.as_ref().unwrap().client().pending(), 0);
    assert_eq!(b.team.as_ref().unwrap().client().pending(), 0);
}

fn rename(app: &mut EditorApp, id: InstanceId, name: &str) {
    app.change(
        id,
        Field::Name,
        format!("Rename to {name}"),
        vec![Edit::Name(name.into())],
    );
    app.history.seal();
}

fn warnings(app: &EditorApp) -> Vec<String> {
    app.output
        .lines()
        .iter()
        .map(|line| line.text.clone())
        .collect()
}

#[test]
fn joining_gives_the_same_world_and_both_editors_are_in_the_session() {
    let (a, b, hub) = two_editors();
    assert!(a.in_team_session() && b.in_team_session());
    assert!(a.world.is_recording_edits());
    assert_same(&a, &b, &hub);
}

#[test]
fn an_inserted_and_renamed_part_shows_in_the_other_editor() {
    let (mut a, mut b, hub) = two_editors();
    a.insert(Insertable::Part(PartType::Block), None);
    let part = a.selected.expect("the new part is selected");
    assert!(!part.is_local(), "it takes an ID from the editor's block");
    rename(&mut a, part, "Crate");
    settle(&mut a, &mut b);
    assert_same(&a, &b, &hub);
    assert_eq!(b.world.get(part).unwrap().name(), "Crate");
}

#[test]
fn a_drag_is_held_until_it_ends() {
    let (mut a, mut b, hub) = two_editors();
    let part = a.world.descendants(a.world.workspace())[0];
    let label = "Change Transparency".to_owned();
    for step in 1..=3 {
        a.change(
            part,
            Field::Property("Transparency"),
            label.clone(),
            vec![Edit::Property(
                "Transparency",
                Value::Number(f64::from(step) / 10.0),
            )],
        );
        settle(&mut a, &mut b);
        assert_eq!(
            hub.lock().unwrap().host().seq(),
            0,
            "nothing is sent while the gesture is open"
        );
    }
    a.history.seal();
    settle(&mut a, &mut b);
    assert_eq!(
        hub.lock().unwrap().host().seq(),
        1,
        "the drag is one transaction"
    );
    assert_same(&a, &b, &hub);
    assert_eq!(
        b.world.get(part).unwrap().property("Transparency"),
        Some(&Value::Number(0.3))
    );
}

#[test]
fn delete_duplicate_and_undo_in_one_editor_reach_the_other() {
    let (mut a, mut b, hub) = two_editors();
    let part = b.world.descendants(b.world.workspace())[0];
    let copy = b.duplicate(part).expect("duplicated");
    settle(&mut a, &mut b);
    assert!(a.world.get(copy).is_some());
    b.delete(part);
    settle(&mut a, &mut b);
    assert_same(&a, &b, &hub);
    assert!(!a.world.is_descendant_of(part, a.world.root()));
    // Undoing the delete puts the very same Instance back for everyone.
    b.undo();
    settle(&mut a, &mut b);
    assert_same(&a, &b, &hub);
    assert!(a.world.is_descendant_of(part, a.world.root()));
    b.undo();
    settle(&mut a, &mut b);
    assert_same(&a, &b, &hub);
    assert!(!a.world.is_descendant_of(copy, a.world.root()));
}

#[test]
fn edits_made_at_the_same_time_end_the_same_in_both() {
    let (mut a, mut b, hub) = two_editors();
    let part = a.world.descendants(a.world.workspace())[0];
    rename(&mut a, part, "Mine");
    b.change(
        part,
        Field::Property("Transparency"),
        "Change Transparency".into(),
        vec![Edit::Property("Transparency", Value::Number(0.5))],
    );
    b.history.seal();
    rename(&mut b, part, "Theirs");
    settle(&mut a, &mut b);
    assert_same(&a, &b, &hub);
    let shown = a.world.get(part).unwrap();
    assert_eq!(
        shown.name(),
        "Theirs",
        "the later rename in the host's order"
    );
    assert_eq!(shown.property("Transparency"), Some(&Value::Number(0.5)));
}

#[test]
fn undo_leaves_alone_what_someone_else_changed_since() {
    let (mut a, mut b, hub) = two_editors();
    let part = a.world.descendants(a.world.workspace())[0];
    rename(&mut a, part, "Door");
    settle(&mut a, &mut b);
    rename(&mut b, part, "Gate");
    settle(&mut a, &mut b);
    a.undo();
    settle(&mut a, &mut b);
    assert_same(&a, &b, &hub);
    assert_eq!(a.world.get(part).unwrap().name(), "Gate");
    assert!(
        warnings(&a)
            .iter()
            .any(|w| w.contains("someone else changed 1 of them")),
        "{:?}",
        warnings(&a)
    );
}

#[test]
fn a_viewer_cannot_change_the_world() {
    let mut a = EditorApp::new(Some(&example("hello-part")));
    let hub = a.host_team_session();
    let mut b = EditorApp::new(Some(&example("hello-part")));
    let member = hub.lock().unwrap().add_member(Role::Viewer);
    b.join_team_session(hub.clone(), member);
    let part = b.world.descendants(b.world.workspace())[0];
    let name = b.world.get(part).unwrap().name().to_owned();
    rename(&mut b, part, "Mine");
    settle(&mut a, &mut b);
    assert_eq!(b.world.get(part).unwrap().name(), name, "taken back");
    assert_same(&a, &b, &hub);
}

#[test]
fn many_edits_from_both_editors_stay_identical() {
    let (mut a, mut b, hub) = two_editors();
    for round in 0..20 {
        let (me, other) = if round % 2 == 0 {
            (&mut a, &mut b)
        } else {
            (&mut b, &mut a)
        };
        me.insert(Insertable::Part(PartType::Block), None);
        let part = me.selected.unwrap();
        rename(me, part, &format!("P{round}"));
        if round % 3 == 0 {
            me.duplicate(part);
        }
        if round % 4 == 1 {
            let first = me.world.descendants(me.world.workspace())[0];
            me.delete(first);
        }
        if round % 5 == 2 {
            me.undo();
        }
        // Every other round the other editor hasn't caught up yet.
        if round % 2 == 0 {
            other.team_frame();
        }
        me.team_frame();
    }
    settle(&mut a, &mut b);
    assert_same(&a, &b, &hub);
    assert!(
        hub.lock().unwrap().host().seq() >= 20,
        "every round was sent"
    );
    // Inserting a part names the class; it's a ClassName::Part.
    assert!(
        a.world
            .descendants(a.world.workspace())
            .iter()
            .any(|&id| a.world.get(id).unwrap().class() == ClassName::Part)
    );
}
