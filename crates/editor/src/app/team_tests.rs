//! Collaborate in the editor (TC2): two headless editors in one session
//! stay identical through ordinary editing, undo, and edits made at the
//! same time.

use std::fmt::Write;

use axcenity_collab::Role;
use axcenity_core::{ClassName, PartType, Value};

use super::team::SharedHub;
use super::tests::{copy_of_example, example};
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

/// A folder for one test's Collaborate files (identity, certificate,
/// members), so each editor is its own computer.
fn team_folder(name: &str) -> std::path::PathBuf {
    // A counter, not the time: names must be valid on Windows too.
    static FOLDERS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let folder = std::env::temp_dir().join(format!(
        "axcenity-team-{name}-{}-{}",
        std::process::id(),
        FOLDERS.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&folder);
    folder
}

/// Runs both editors' frames until `done`, or fails after 20 s.
fn settle_net(a: &mut EditorApp, b: &mut EditorApp, done: impl Fn(&EditorApp, &EditorApp) -> bool) {
    let start = std::time::Instant::now();
    while !done(a, b) {
        assert!(
            start.elapsed().as_secs() < 20,
            "timed out: {:?}",
            warnings(b)
        );
        a.team_frame();
        b.team_frame();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

/// The session's newest transaction, from `host`'s editor.
fn host_seq(host: &EditorApp) -> u64 {
    let hosting = host
        .team
        .as_ref()
        .and_then(|t| t.hosting())
        .expect("hosting");
    hosting.lock().host().seq()
}

/// Joined, with everything up to `seq` and nothing unconfirmed.
fn caught_up(app: &EditorApp, seq: u64) -> bool {
    team_client(app).is_some_and(|c| c.is_online() && c.pending() == 0 && c.seq() == seq)
}

fn team_client(app: &EditorApp) -> Option<&axcenity_collab::Client> {
    let team = app.team.as_ref()?;
    team.is_joined().then(|| team.client())
}

#[test]
fn two_editors_share_a_session_over_the_network() {
    let project = copy_of_example("hello-touched", "team-net-host");
    let mut a = EditorApp::new(Some(&project));
    let invite = a
        .host_network_session(Some(&team_folder("host")), 0)
        .expect("hosting");
    // The project as it was is kept in Version History first (C5).
    assert!(
        axcenity_project::list_versions(&project)
            .iter()
            .any(|v| v.label == super::team::SESSION_CHECKPOINT)
    );
    // The invite names this computer's LAN address; join over loopback.
    let port = invite.host.rsplit(':').next().unwrap();
    let invite = axcenity_collab::access::Invite {
        host: format!("127.0.0.1:{port}"),
        ..invite
    };
    let mut b = EditorApp::new(Some(&copy_of_example("hello-touched", "team-net-joiner")));
    b.join_network_session(&invite, Some(&team_folder("joiner")))
        .expect("joining");
    settle_net(&mut a, &mut b, |a, b| {
        let seq = host_seq(a);
        caught_up(a, seq) && caught_up(b, seq)
    });
    assert_eq!(team_client(&a).unwrap().role(), Role::Owner);
    assert_eq!(team_client(&b).unwrap().role(), Role::Editor);

    b.insert(Insertable::Part(PartType::Block), None);
    let part = b.selected.expect("the new part is selected");
    rename(&mut b, part, "Crate");
    settle_net(&mut a, &mut b, |a, b| {
        let seq = host_seq(a);
        caught_up(a, seq)
            && caught_up(b, seq)
            && a.world.get(part).is_some_and(|p| p.name() == "Crate")
    });
    let host = {
        let team = a.team.as_ref().unwrap();
        let hosting = team.hosting().unwrap();
        dump(hosting.lock().host().world())
    };
    assert_eq!(dump(&a.world), host);
    assert_eq!(dump(&b.world), host);
}

#[test]
fn the_command_line_asks_for_a_session() {
    use super::team::TeamStart;
    let args = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert_eq!(TeamStart::from_args(&args(&[])), Ok(None));
    assert_eq!(
        TeamStart::from_args(&args(&["--team-host"])),
        Ok(Some(TeamStart::Host))
    );
    let link = "axcenity://teamcreate/join?host=192.168.1.20:47810&cert=\
                00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff&\
                invite=00112233445566778899aabbccddeeff";
    let Ok(Some(TeamStart::Join(invite))) = TeamStart::from_args(&args(&["--team-join", link]))
    else {
        panic!("a join")
    };
    assert_eq!(invite.host, "192.168.1.20:47810");
    assert!(TeamStart::from_args(&args(&["--team-join"])).is_err());
    assert!(TeamStart::from_args(&args(&["--team-join", "https://example.com"])).is_err());
    // The new names, and a bare link as the OS passes one it opens.
    assert_eq!(
        TeamStart::from_args(&args(&["--collab-host"])),
        Ok(Some(TeamStart::Host))
    );
    let new_link = link.replace("teamcreate", "collaborate");
    for given in [
        args(&["--collab-join", &new_link]),
        args(&[&new_link]),
        args(&[link]),
    ] {
        let Ok(Some(TeamStart::Join(found))) = TeamStart::from_args(&given) else {
            panic!("a join from {given:?}")
        };
        assert_eq!(found, invite);
    }
}

#[test]
fn what_one_editor_selects_shows_in_the_others_session() {
    let (mut a, mut b, _hub) = two_editors();
    let part = a.world.descendants(a.world.workspace())[0];
    a.selection = vec![part];
    a.selected = Some(part);
    // Presence goes out at most 10 times a second.
    std::thread::sleep(std::time::Duration::from_millis(110));
    settle(&mut a, &mut b);
    let me = a.team.as_ref().unwrap().client().member();
    let client = b.team.as_ref().unwrap().client();
    let seen = client.presences().get(&me).expect("a's presence");
    assert_eq!(seen.selection, [part]);
    assert!(seen.camera.is_some(), "and where a's camera is");
    assert_eq!(client.roster().len(), 2);
    // What the viewport, Explorer and Properties show of it.
    let shown = b.team_selections();
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].selection, [part]);
    assert_eq!(shown[0].name, "Member 1", "the in-process hub names nobody");
    // Drawing the panels with it doesn't trip over anything.
    b.selected = Some(part);
    b.selection = vec![part];
    let ctx = theme::test_context();
    for _ in 0..2 {
        let _ = theme::run(&ctx, Default::default(), |ui| {
            b.explorer_pane(ui);
            b.properties_pane(ui);
        });
    }
    assert_eq!(b.explorer.others.get(&part).map(Vec::len), Some(1));

    // Dragging a field shows as a bar over the other's Properties (C6).
    a.team_panel.editing_field = Some("Transparency".to_owned());
    std::thread::sleep(std::time::Duration::from_millis(110));
    settle(&mut a, &mut b);
    assert_eq!(
        b.team_selections()[0].editing.as_deref(),
        Some("Transparency")
    );
    let mut drawn = String::new();
    for _ in 0..2 {
        let output = theme::run(&ctx, Default::default(), |ui| b.properties_pane(ui));
        drawn = format!("{:?}", output.shapes);
    }
    assert!(drawn.contains("Member 1 is changing Transparency"));
}

/// Draws the Collaborate panel 260 points wide (the narrowest column), and
/// saves it as `name` when `AXCENITY_SNAPSHOTS` names a folder.
fn draw_panel(app: &mut EditorApp, name: &str) {
    let mut draw = |ui: &mut egui::Ui| app.team_create_pane(ui);
    match std::env::var_os("AXCENITY_SNAPSHOTS") {
        Some(folder) => crate::snapshot::snapshot(
            &std::path::Path::new(&folder).join(format!("{name}.png")),
            egui::vec2(260.0, 560.0),
            vec![Vec::new(); 3],
            &mut draw,
        ),
        None => {
            let ctx = theme::test_context();
            for _ in 0..3 {
                let _ = theme::run(&ctx, Default::default(), |ui| draw(ui));
            }
        }
    }
}

#[test]
fn the_panel_offers_share_and_join_then_shows_the_session() {
    let mut a = EditorApp::new(Some(&copy_of_example("hello-touched", "team-panel-host")));
    draw_panel(&mut a, "team-create-start");
    assert!(!a.in_team_session());

    // A bad link says why, and nothing starts.
    a.team_panel.link = "https://example.com".into();
    a.team_action_for_test(super::team_panel::TestAction::Join);
    assert!(a.team_panel.problem.is_some());
    assert!(!a.in_team_session());

    let invite = a
        .host_network_session(Some(&team_folder("panel-host")), 0)
        .expect("hosting");
    let port = invite.host.rsplit(':').next().unwrap();
    let invite = axcenity_collab::access::Invite {
        host: format!("127.0.0.1:{port}"),
        ..invite
    };
    let mut b = EditorApp::new(Some(&copy_of_example("hello-touched", "team-panel-joiner")));
    b.join_network_session(&invite, Some(&team_folder("panel-joiner")))
        .expect("joining");
    settle_net(&mut a, &mut b, |a, b| {
        let seq = host_seq(a);
        caught_up(a, seq)
            && caught_up(b, seq)
            && b.team()
                .unwrap()
                .roster()
                .iter()
                .filter(|c| c.online)
                .count()
                == 2
    });
    assert_eq!(
        super::team_panel::status_text(&a.team().unwrap().status()),
        "Connected"
    );
    draw_panel(&mut a, "team-create-host");
    draw_panel(&mut b, "team-create-member");

    // The owner makes the joiner a Viewer; the joiner's editor hears it.
    let member = b.team().unwrap().me().unwrap();
    a.set_team_role(member, Role::Viewer);
    settle_net(&mut a, &mut b, |_, b| {
        b.team().unwrap().client_role() == Some(Role::Viewer)
    });
    // A new invite replaces the link shown.
    let link = a.make_team_invite(Role::Editor).expect("hosting");
    assert_eq!(a.team().unwrap().invite(), Some(link.as_str()));

    // Leaving keeps the World and stops syncing; ending drops everyone.
    b.leave_team_session();
    assert!(!b.in_team_session() && !b.world.is_recording_edits());
    a.leave_team_session();
    assert!(!a.in_team_session());
}

/// `invite`, naming the loopback address (the host names its LAN one).
fn over_loopback(invite: axcenity_collab::access::Invite) -> axcenity_collab::access::Invite {
    let port = invite.host.rsplit(':').next().unwrap().to_owned();
    axcenity_collab::access::Invite {
        host: format!("127.0.0.1:{port}"),
        ..invite
    }
}

/// The project's scripts: path, Instance and code.
fn scripts(app: &EditorApp) -> Vec<(String, InstanceId, String)> {
    let mut list: Vec<_> = app
        .project
        .as_ref()
        .unwrap()
        .scripts
        .iter()
        .map(|s| (s.path.clone(), s.instance, s.source.clone()))
        .collect();
    list.sort();
    list
}

#[test]
fn the_start_screen_joins_a_session_and_opens_its_files() {
    use crate::start::JoinProgress;
    let project = copy_of_example("hello-touched", "team-start-host");
    let mut a = EditorApp::new(Some(&project));
    let invite = over_loopback(
        a.host_network_session(Some(&team_folder("start-host")), 0)
            .expect("hosting"),
    );
    // From the start screen: the Join dialog waits and says how it goes.
    let mut b = EditorApp::new(None);
    let joiner = team_folder("start-joiner");
    b.join_from_start(&invite, Some(&joiner));
    b.follow_start_join();
    assert!(matches!(
        b.start.join_dialog(),
        Some((true, None, Some(JoinProgress::Connecting { .. })))
    ));
    settle_net(&mut a, &mut b, |_, b| b.project.is_some());
    assert!(b.project.as_ref().unwrap().folder.starts_with(&joiner));
    assert!(b.in_team_session());
    settle_net(&mut a, &mut b, |a, b| {
        let seq = host_seq(a);
        caught_up(a, seq) && caught_up(b, seq)
    });
    assert_eq!(dump(&a.world), dump(&b.world));

    // The invite worked once: another computer with it is refused, and
    // the dialog says why and asks for a link again.
    let mut c = EditorApp::new(None);
    c.join_from_start(&invite, Some(&team_folder("start-refused")));
    let start = std::time::Instant::now();
    while c.team.is_some() {
        assert!(start.elapsed().as_secs() < 20, "never refused");
        a.team_frame();
        c.team_frame();
        c.follow_start_join();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let (waiting, error, _) = c.start.join_dialog().expect("the Join dialog");
    assert!(!waiting);
    assert!(error.is_some_and(|e| !e.is_empty()), "{error:?}");
    assert!(c.project.is_none());

    // Closing the session's project leaves the session.
    b.finish_closing_project();
    assert!(!b.in_team_session());
}

#[test]
fn document_tabs_lock_while_edited_and_reload_what_a_teammate_saves() {
    let project = copy_of_example("hello-touched", "team-docs-host");
    let mut a = EditorApp::new(Some(&project));
    let invite = over_loopback(
        a.host_network_session(Some(&team_folder("docs-host")), 0)
            .expect("hosting"),
    );
    let mut b = EditorApp::new(None);
    b.join_network_session(&invite, Some(&team_folder("docs-joiner")))
        .expect("joining");
    settle_net(&mut a, &mut b, |_, b| b.project.is_some());

    // The joiner makes a behavior tree and saves it: it reaches the host's
    // folder, where the host's editor opens it.
    let tree = "assets/Guard.behaviortree.json";
    b.behavior.create(tree);
    assert!(b.save());
    settle_net(&mut a, &mut b, |_, _| project.join(tree).exists());
    a.behavior.open(&project, tree).expect("the tree opens");
    let saved = a.behavior.document(tree).unwrap().graph.clone();

    // The joiner edits it: the host's tab is read-only, by the joiner.
    b.behavior.document_mut(tree).unwrap().graph.nodes[0].position[0] += 120.0;
    settle_net(&mut a, &mut b, |a, _| {
        a.team
            .as_ref()
            .is_some_and(|team| team.editing_elsewhere(tree).is_some())
    });
    assert_eq!(a.behavior.document(tree).unwrap().graph, saved);

    // Saving frees it, and the host's editor shows the new version.
    assert!(b.save());
    settle_net(&mut a, &mut b, |a, b| {
        a.behavior.document(tree).map(|doc| &doc.graph)
            == b.behavior.document(tree).map(|doc| &doc.graph)
            && a.team
                .as_ref()
                .is_some_and(|team| team.editing_elsewhere(tree).is_none())
    });
    assert!(!a.behavior.unsaved());

    // The string table and the input map are read again too.
    let mut strings = (**b.world.strings()).clone();
    strings.set("Greeting", "en", "Hello, team");
    std::fs::write(
        b.project
            .as_ref()
            .unwrap()
            .folder
            .join(axcenity_project::STRINGS_FILE),
        axcenity_project::write_strings(&strings),
    )
    .unwrap();
    settle_net(&mut a, &mut b, |a, _| {
        a.world.strings().get("Greeting", "en") == Some("Hello, team")
    });
}

#[test]
fn a_ui_graph_a_teammate_saves_runs_its_new_version_here() {
    let project = copy_of_example("hello-ui-graph", "team-graph-host");
    let mut a = EditorApp::new(Some(&project));
    let invite = over_loopback(
        a.host_network_session(Some(&team_folder("graph-host")), 0)
            .expect("hosting"),
    );
    let mut b = EditorApp::new(None);
    b.join_network_session(&invite, Some(&team_folder("graph-joiner")))
        .expect("joining");
    settle_net(&mut a, &mut b, |_, b| b.project.is_some());
    let path = "ui/Menu.uigraph.json";
    let graph = |app: &EditorApp| {
        app.project
            .as_ref()
            .unwrap()
            .ui_graphs
            .iter()
            .find(|doc| doc.saved_path.as_deref() == Some(path))
            .map(|doc| doc.graph.clone())
            .expect("the Menu graph")
    };
    assert_eq!(graph(&a), graph(&b));

    // The joiner moves a node: the host's Event Graph is locked meanwhile.
    {
        let doc = b
            .project
            .as_mut()
            .unwrap()
            .ui_graphs
            .iter_mut()
            .find(|doc| doc.saved_path.as_deref() == Some(path))
            .unwrap();
        doc.graph.nodes[0].position[1] += 80.0;
    }
    settle_net(&mut a, &mut b, |a, _| {
        a.team
            .as_ref()
            .is_some_and(|team| team.editing_elsewhere(path).is_some())
    });
    assert!(b.save());
    settle_net(&mut a, &mut b, |a, b| graph(a) == graph(b));
    // Still one EventGraph script, with the same code on both.
    settle_net(&mut a, &mut b, |a, b| {
        let seq = host_seq(a);
        caught_up(a, seq) && caught_up(b, seq)
    });
    assert_eq!(scripts(&a), scripts(&b));
    assert_eq!(dump(&a.world), dump(&b.world));
}

#[test]
fn a_joining_editor_gets_the_files_and_types_in_scripts_with_the_host() {
    let project = copy_of_example("hello-touched", "team-files-host");
    let mut a = EditorApp::new(Some(&project));
    let invite = a
        .host_network_session(Some(&team_folder("files-host")), 0)
        .expect("hosting");
    // The joiner starts with no project: the session's opens.
    let mut b = EditorApp::new(None);
    let joiner = team_folder("files-joiner");
    b.join_network_session(&over_loopback(invite), Some(&joiner))
        .expect("joining");
    settle_net(&mut a, &mut b, |a, b| {
        let seq = host_seq(a);
        caught_up(a, seq)
            && caught_up(b, seq)
            && b.project
                .as_ref()
                .is_some_and(|p| p.folder.starts_with(&joiner))
    });
    let mirror = b.project.as_ref().unwrap().folder.clone();
    assert_eq!(
        std::fs::read_to_string(mirror.join("axcenity.json")).unwrap(),
        std::fs::read_to_string(project.join("axcenity.json")).unwrap()
    );
    // The same scripts, on the same Instances, and the same World.
    assert!(!scripts(&a).is_empty());
    assert_eq!(scripts(&a), scripts(&b));
    assert_eq!(dump(&a.world), dump(&b.world));

    // Scripts are live (C7): both editors type in the same script at
    // once, neither tab locks, and each sees the other's typing.
    let (path, _, source) = scripts(&b).remove(0);
    b.tabs.open(&path, &source, None);
    a.tabs.open(&path, &source, None);
    settle_net(&mut a, &mut b, |a, b| {
        a.live_doc_ready(&path) && b.live_doc_ready(&path)
    });
    b.tabs.get_mut(&path).unwrap().text = format!("{source}-- from the joiner\n");
    a.tabs.get_mut(&path).unwrap().text = format!("-- from the host\n{source}");
    let both = format!("-- from the host\n{source}-- from the joiner\n");
    settle_net(&mut a, &mut b, |a, b| {
        a.tabs.get(&path).unwrap().text == both && b.tabs.get(&path).unwrap().text == both
    });
    for app in [&a, &b] {
        let tab = app.tabs.get(&path).unwrap();
        assert!(tab.live && tab.editing_elsewhere.is_none());
    }
    // Once the typing settles, the host writes it to its file, and the
    // tabs have nothing left to save.
    settle_net(&mut a, &mut b, |a, b| {
        std::fs::read_to_string(project.join(&path)).is_ok_and(|text| text == both)
            && !a.tabs.get(&path).unwrap().unsaved()
            && !b.tabs.get(&path).unwrap().unsaved()
    });
    // Undo in the joiner's tab takes back only the joiner's line.
    b.tabs.get_mut(&path).unwrap().live_undo = Some(true);
    let hosts_only = format!("-- from the host\n{source}");
    settle_net(&mut a, &mut b, |a, b| {
        a.tabs.get(&path).unwrap().text == hosts_only
            && b.tabs.get(&path).unwrap().text == hosts_only
    });
    settle_net(&mut a, &mut b, |a, b| scripts(a) == scripts(b));

    // Others' cursors (C8): the joiner's caret shows in the host's tab, and
    // stays on the same text when the host types before it.
    b.tabs.activate(b.tabs.get(&path).unwrap().id);
    b.tabs.get_mut(&path).unwrap().cursor = Some((3, 3));
    let joiner_name = team_client(&b)
        .and_then(|client| {
            client
                .roster()
                .iter()
                .find(|c| c.member == client.member())
                .map(|c| c.profile.name.clone())
        })
        .expect("the joiner is in the roster");
    let caret = |a: &EditorApp| {
        a.tabs
            .get(&path)
            .unwrap()
            .others
            .iter()
            .map(|other| (other.name.clone(), other.anchor, other.head))
            .collect::<Vec<_>>()
    };
    settle_net(&mut a, &mut b, |a, _| {
        caret(a) == [(joiner_name.clone(), 3, 3)]
    });
    a.tabs.get_mut(&path).unwrap().text = format!("-- 1\n{hosts_only}");
    settle_net(&mut a, &mut b, |a, b| {
        b.tabs.get(&path).unwrap().text.starts_with("-- 1\n")
            && caret(a) == [(joiner_name.clone(), 8, 8)]
    });
    // Its own cursor never shows in its own tab.
    assert!(b.tabs.get(&path).unwrap().others.is_empty());

    // A new script from the host's editor reaches the joiner on the same
    // Instance, without a second Script.
    let new = "scripts/Extra.server.lua";
    std::fs::write(project.join(new), "print('extra')\n").unwrap();
    a.disk_changed(&[new.to_owned()]);
    settle_net(&mut a, &mut b, |a, b| {
        let seq = host_seq(a);
        caught_up(a, seq) && caught_up(b, seq) && scripts(b).iter().any(|s| s.0 == new)
    });
    assert_eq!(scripts(&a), scripts(&b));
    assert_eq!(dump(&a.world), dump(&b.world));
}

#[test]
fn an_import_reaches_everyone_as_one_step_with_its_files() {
    let project = copy_of_example("hello-touched", "team-import-host");
    let mut a = EditorApp::new(Some(&project));
    let invite = a
        .host_network_session(Some(&team_folder("import-host")), 0)
        .expect("hosting");
    let mut b = EditorApp::new(None);
    let joiner = team_folder("import-joiner");
    b.join_network_session(&over_loopback(invite), Some(&joiner))
        .expect("joining");
    settle_net(&mut a, &mut b, |a, b| {
        let seq = host_seq(a);
        caught_up(a, seq) && caught_up(b, seq) && b.project.is_some()
    });

    a.import_model(&super::tests::example("hello-mesh").join("assets/models/pyramid.gltf"));
    let dialog = a.importing.take().expect("the dialog is open");
    a.import_chosen(&dialog);
    let part = a.selected.expect("the new MeshPart is selected");
    let file = "assets/models/pyramid/pyramid.gltf";
    settle_net(&mut a, &mut b, |a, b| {
        let seq = host_seq(a);
        caught_up(a, seq)
            && caught_up(b, seq)
            && b.world.get(part).is_some()
            && b.project
                .as_ref()
                .is_some_and(|p| p.folder.join(file).exists())
    });
    // One transaction made the MeshPart and published its files.
    let hosting = a.team().unwrap().hosting().unwrap().lock();
    let step = hosting
        .host()
        .log()
        .find(|logged| logged.label == "Import pyramid")
        .expect("the import's step");
    assert!(
        step.ops
            .iter()
            .any(|op| matches!(op, axcenity_collab::Op::CreateTree { .. }))
    );
    assert!(
        step.ops
            .iter()
            .any(|op| matches!(op, axcenity_collab::Op::FileVersion { path, .. } if path == file))
    );
}

/// Whether `world` has an Instance named `name` under the Workspace.
fn has_named(world: &axcenity_core::World, name: &str) -> bool {
    world
        .descendants(world.workspace())
        .into_iter()
        .any(|id| world.get(id).is_some_and(|i| i.name() == name))
}

/// A hosted session from `project` with a joiner who has made a Part
/// called "Crate", every copy caught up.
fn session_with_a_crate(project: &std::path::Path, name: &str) -> (EditorApp, EditorApp) {
    let mut a = EditorApp::new(Some(project));
    let invite = over_loopback(
        a.host_network_session(Some(&team_folder(&format!("{name}-host"))), 0)
            .expect("hosting"),
    );
    let mut b = EditorApp::new(None);
    b.join_network_session(&invite, Some(&team_folder(&format!("{name}-joiner"))))
        .expect("joining");
    settle_net(&mut a, &mut b, |_, b| b.project.is_some());
    b.insert(Insertable::Part(PartType::Block), None);
    let part = b.selected.expect("the new part is selected");
    rename(&mut b, part, "Crate");
    settle_net(&mut a, &mut b, |a, b| {
        let seq = host_seq(a);
        caught_up(a, seq) && caught_up(b, seq) && has_named(&a.world, "Crate")
    });
    (a, b)
}

/// Hosts a session from `project` and stops the host's editor without
/// ending it (a crash). Returns how many scripts the project had.
#[inline(never)]
fn crash_a_session(project: &std::path::Path) -> usize {
    let (a, b) = session_with_a_crate(project, "recover");
    // The joiner's Part is a change to save in the host's project.
    assert!(!a.history.is_saved());
    let scripts = scripts(&a).len();
    drop(b);
    drop(a);
    scripts
}

/// Runs `test` on a thread with room for several editors on its stack
/// (an unoptimized `EditorApp` is large; the default is 2 MiB).
fn with_big_stack(test: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(16 << 20)
        .spawn(test)
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn a_hosting_editor_that_stops_without_ending_gets_the_session_back() {
    with_big_stack(recover_after_a_crash);
}

fn recover_after_a_crash() {
    let project = copy_of_example("hello-touched", "team-recover");
    let scripts_before = crash_a_session(&project);
    assert!(axcenity_collab::journal::has_recovery(&project));

    let c = EditorApp::new(Some(&project));
    assert!(has_named(&c.world, "Crate"), "the session's World is back");
    assert!(!c.history.is_saved(), "as changes to save");
    assert!(!axcenity_collab::journal::has_recovery(&project));
    // The project's scripts point at the recovered World's Instances.
    let recovered = scripts(&c);
    assert_eq!(recovered.len(), scripts_before);
    for (path, id, _) in &recovered {
        assert!(c.world.get(*id).is_some(), "{path}");
    }
    assert!(
        warnings(&c)
            .iter()
            .any(|w| w.contains("stopped without ending"))
    );
}

#[test]
fn a_session_ended_with_its_changes_saved_leaves_nothing_to_recover() {
    let project = copy_of_example("hello-touched", "team-quit-saved");
    let (mut a, b) = session_with_a_crate(&project, "quit-saved");
    drop(b);
    assert!(a.save());
    a.end_team_for_quit();
    assert!(!axcenity_collab::journal::has_recovery(&project));
}

#[test]
fn a_session_ended_with_changes_to_save_keeps_them_until_a_save() {
    with_big_stack(keep_until_a_save);
}

fn keep_until_a_save() {
    let project = copy_of_example("hello-touched", "team-end-unsaved");
    end_unsaved_then_save(&project);
    let reopened = EditorApp::new(Some(&project));
    assert!(has_named(&reopened.world, "Crate"), "it was saved");
}

#[inline(never)]
fn end_unsaved_then_save(project: &std::path::Path) {
    let (mut a, b) = session_with_a_crate(project, "end-unsaved");
    drop(b);
    a.leave_team_session();
    assert!(axcenity_collab::journal::has_recovery(project));
    assert!(a.save());
    assert!(!axcenity_collab::journal::has_recovery(project));
}

#[test]
fn when_the_host_ends_the_session_the_joiner_stays_open_read_only() {
    with_big_stack(host_ends_the_session);
}

fn host_ends_the_session() {
    let project = copy_of_example("hello-part", "team-host-leaves");
    let (mut a, mut b) = session_with_a_crate(&project, "host-leaves");
    a.leave_team_session();
    let start = std::time::Instant::now();
    while !matches!(
        b.team().map(|t| t.status()),
        Some(super::team::TeamStatus::HostLeft { .. })
    ) {
        assert!(start.elapsed().as_secs() < 20, "{:?}", warnings(&b));
        b.team_frame();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(super::team_panel::status_text(&b.team().unwrap().status()).contains("host left"));
    // The copy is still there, and an edit is taken back.
    let part = b
        .world
        .descendants(b.world.workspace())
        .into_iter()
        .find(|&id| b.world.get(id).is_some_and(|i| i.name() == "Crate"))
        .expect("the copy stays open");
    rename(&mut b, part, "Box");
    b.team_frame();
    assert_eq!(b.world.get(part).unwrap().name(), "Crate");
    assert!(
        warnings(&b).iter().any(|w| w.contains("read-only")),
        "{:?}",
        warnings(&b)
    );
}

#[test]
fn a_joiner_that_stops_while_offline_sends_its_edits_when_it_joins_again() {
    with_big_stack(stop_offline_then_join_again);
}

fn stop_offline_then_join_again() {
    let project = copy_of_example("hello-part", "team-unsent");
    let mut a = EditorApp::new(Some(&project));
    let invite = over_loopback(
        a.host_network_session(Some(&team_folder("unsent-host")), 0)
            .expect("hosting"),
    );
    // The joiner's computer: its identity and folders, kept across runs.
    let joiner = team_folder("unsent-joiner");
    let mut b = EditorApp::new(None);
    b.join_network_session(&invite, Some(&joiner))
        .expect("joining");
    settle_net(&mut a, &mut b, |_, b| b.project.is_some());
    b.insert(Insertable::Part(PartType::Block), None);
    let part = b.selected.expect("the new part is selected");
    rename(&mut b, part, "Crate");
    settle_net(&mut a, &mut b, |a, b| {
        let seq = host_seq(a);
        caught_up(a, seq) && caught_up(b, seq) && has_named(&a.world, "Crate")
    });
    // The cable is cut: the joiner adds a Lamp and renames the Crate,
    // then the editor stops before the connection is back.
    b.drop_team_connection();
    b.insert(Insertable::Part(PartType::Block), None);
    let lamp = b.selected.expect("the new part is selected");
    rename(&mut b, lamp, "Lamp");
    b.team_frame();
    rename(&mut b, part, "Box");
    b.team_frame();
    // The Lamp (inserted and named in one frame, so one step) and the rename.
    assert_eq!(team_client(&b).unwrap().pending(), 2);
    let kept = b.team().unwrap().files_folder().unwrap().to_owned();
    drop(b);
    // Meanwhile the host's editor deletes the Crate.
    let host_crate = a
        .world
        .descendants(a.world.workspace())
        .into_iter()
        .find(|&id| a.world.get(id).is_some_and(|i| i.name() == "Crate"))
        .unwrap();
    a.select_from_viewport(Some(host_crate));
    a.delete_selected();
    // The joiner's editor starts again and joins.
    let mut c = EditorApp::new(None);
    c.join_network_session(&invite, Some(&joiner))
        .expect("joining again");
    settle_net(&mut a, &mut c, |a, c| {
        let seq = host_seq(a);
        caught_up(a, seq) && caught_up(c, seq) && has_named(&a.world, "Lamp")
    });
    // The Lamp arrived; renaming the deleted Crate couldn't apply, and the
    // Recovered Changes window says so.
    assert!(!has_named(&a.world, "Box") && !has_named(&a.world, "Crate"));
    let recovered: Vec<&str> = c
        .team_panel
        .recovered
        .iter()
        .map(|r| r.label.as_str())
        .collect();
    assert_eq!(recovered, ["Rename to Box"], "{:?}", warnings(&c));
    // The copy from before they were sent is a checkpoint to compare with
    // (C5); the window points at it.
    assert!(c.team_panel.offline_checkpoint);
    let mirror = c.team.as_ref().unwrap().files_folder().unwrap().to_owned();
    assert!(
        axcenity_project::list_versions(&mirror)
            .iter()
            .any(|v| v.label == super::team::OFFLINE_CHECKPOINT && v.pinned)
    );
    // Everything is confirmed: the recovery file is gone.
    c.team_frame();
    assert!(
        axcenity_collab::journal::load_unsent(&kept)
            .unwrap()
            .is_empty()
    );
    assert_eq!(dump(&a.world), dump(&c.world));
}

#[test]
fn following_a_member_moves_the_camera_with_theirs_until_you_move_it() {
    let (mut a, mut b, _hub) = two_editors();
    let b_member = b.team().unwrap().me().unwrap();
    // Presence goes at most 10 times a second.
    std::thread::sleep(std::time::Duration::from_millis(120));
    b.viewport.look_at(
        axcenity_render::glam::Vec3::new(10.0, 5.0, 20.0),
        axcenity_render::glam::Vec3::ZERO,
    );
    settle(&mut a, &mut b);
    a.team_panel.following = Some(b_member);
    a.team_frame();
    let here = a.viewport.render_camera().position;
    assert!(here.distance(axcenity_render::glam::Vec3::new(10.0, 5.0, 20.0)) < 1e-3);
    // b moves on; a follows.
    std::thread::sleep(std::time::Duration::from_millis(120));
    b.viewport.look_at(
        axcenity_render::glam::Vec3::new(-4.0, 8.0, 2.0),
        axcenity_render::glam::Vec3::ZERO,
    );
    settle(&mut a, &mut b);
    let here = a.viewport.render_camera().position;
    assert!(here.distance(axcenity_render::glam::Vec3::new(-4.0, 8.0, 2.0)) < 1e-3);
    // a moves the camera: following stops.
    a.viewport.look_at(
        axcenity_render::glam::Vec3::new(0.0, 50.0, 0.0),
        axcenity_render::glam::Vec3::ZERO,
    );
    a.team_frame();
    assert_eq!(a.team_panel.following, None);
}

#[test]
fn invite_defaults_are_kept_between_runs() {
    let mut panel = super::team_panel::TeamPanel::default();
    assert_eq!(panel.to_text(), "editor week");
    panel.read_text("viewer month");
    assert_eq!(panel.to_text(), "viewer month");
    assert_eq!(panel.invite_lifetime.seconds(), 30 * 24 * 60 * 60);
    panel.read_text("garbled");
    assert_eq!(panel.to_text(), "viewer month");
}

// Chat (C8).

#[test]
fn chat_reaches_the_other_editor_with_an_unread_count_and_instance_links() {
    let (mut a, mut b, _hub) = two_editors();
    settle(&mut a, &mut b);
    let part = a.world.get(a.world.workspace()).unwrap().children()[0];
    let path = a.world.full_name(part).unwrap();
    a.team_panel.chat_draft = format!("look at {path}, please");
    a.send_team_chat();
    assert!(a.team_panel.chat_draft.is_empty());
    assert_eq!(a.team_panel.chat_problem, None);
    settle(&mut a, &mut b);
    let line = |app: &EditorApp| {
        app.team()
            .unwrap()
            .client()
            .chat()
            .iter()
            .map(|line| line.text.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(line(&a), [format!("look at {path}, please")]);
    assert_eq!(line(&b), line(&a));
    // Unread for the other editor only, until its chat shows.
    assert_eq!(a.team_panel.chat_unread, 0);
    assert_eq!(b.team_panel.chat_unread, 1);
    draw_panel(&mut b, "collaborate-chat");
    assert_eq!(b.team_panel.chat_unread, 0);
    // The path is a link to the Instance.
    let text = &line(&b)[0];
    let pieces = super::team_chat::pieces(text, &b.world);
    assert!(pieces.contains(&super::team_chat::Piece::Instance(&path, part)));

    // A blank message doesn't go, and says why.
    b.team_panel.chat_draft = "   ".to_owned();
    b.send_team_chat();
    assert!(b.team_panel.chat_problem.is_some());
}

#[test]
fn only_paths_of_instances_become_links() {
    let (a, _b, _hub) = two_editors();
    let part = a.world.get(a.world.workspace()).unwrap().children()[0];
    let path = a.world.full_name(part).unwrap();
    use super::team_chat::{Piece, pieces};
    let text = format!("e.g. ({path}). and Workspace.Nothing");
    let found = pieces(&text, &a.world);
    assert_eq!(
        found,
        [
            Piece::Text("e.g. ("),
            Piece::Instance(&path, part),
            Piece::Text("). and Workspace.Nothing"),
        ]
    );
    assert_eq!(pieces("plain", &a.world), [Piece::Text("plain")]);
}
