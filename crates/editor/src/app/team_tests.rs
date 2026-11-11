//! Team Create in the editor (TC2): two headless editors in one session
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

/// A folder for one test's Team Create files (identity, certificate,
/// members), so each editor is its own computer.
fn team_folder(name: &str) -> std::path::PathBuf {
    let folder = std::env::temp_dir().join(format!(
        "axcenity-team-{name}-{}-{:?}",
        std::process::id(),
        std::time::Instant::now()
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
    let mut a = EditorApp::new(Some(&copy_of_example("hello-touched", "team-net-host")));
    let invite = a
        .host_network_session(Some(&team_folder("host")), 0)
        .expect("hosting");
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
}

/// Draws the Team Create panel 260 points wide (the narrowest column), and
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
fn a_joining_editor_gets_the_files_and_its_saves_reach_the_host() {
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

    // The joiner edits a script: the host's editor shows it read-only.
    let (path, _, source) = scripts(&b).remove(0);
    b.tabs.open(&path, &source, None);
    a.tabs.open(&path, &source, None);
    b.tabs.get_mut(&path).unwrap().text = format!("{source}\n-- edited by the joiner\n");
    settle_net(&mut a, &mut b, |a, _| {
        a.tabs
            .get(&path)
            .is_some_and(|tab| tab.editing_elsewhere.is_some())
    });
    // Saving sends it; the host's editor reads it in and the tab frees up.
    assert!(b.save_tab(&path));
    settle_net(&mut a, &mut b, |a, _| {
        std::fs::read_to_string(project.join(&path))
            .is_ok_and(|text| text.ends_with("-- edited by the joiner\n"))
            && a.tabs
                .get(&path)
                .is_some_and(|tab| tab.editing_elsewhere.is_none())
    });
    settle_net(&mut a, &mut b, |a, b| scripts(a) == scripts(b));
    assert!(
        a.tabs
            .get(&path)
            .unwrap()
            .text
            .ends_with("-- edited by the joiner\n")
    );

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
