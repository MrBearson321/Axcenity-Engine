//! The server over real sockets on this computer: an Owner joins with the
//! invite it makes, edits, and the project is saved; stopping tells
//! members, and a crash loses nothing.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use axcenity_collab::access::Invite;
use axcenity_collab::identity::Identity;
use axcenity_collab::session::{Remote, RemoteEvent};
use axcenity_collab::{Client, Notice, Role, World, journal};

use super::*;

const PATIENCE: Duration = Duration::from_secs(20);

static FOLDERS: AtomicUsize = AtomicUsize::new(0);

/// A copy of `examples/hello-part` and a data folder beside it, removed
/// when the test ends.
struct Place(PathBuf);

impl Place {
    fn new() -> Place {
        let root = std::env::temp_dir().join(format!(
            "axcenity-collab-server-{}-{}",
            std::process::id(),
            FOLDERS.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&root);
        let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-part");
        copy_folder(&example, &root.join("project"));
        Place(root)
    }

    fn options(&self) -> Options {
        Options {
            project: self.0.join("project"),
            data: self.0.join("data"),
            port: 0,
            address: None,
            tailscale: false,
        }
    }

    fn project(&self) -> PathBuf {
        self.0.join("project")
    }
}

impl Drop for Place {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn copy_folder(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_folder(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// The link in what the server said.
fn link_in(lines: &[String]) -> Invite {
    let text = lines
        .iter()
        .flat_map(|line| line.lines())
        .find(|line| line.starts_with("axcenity://"))
        .expect("an invite link");
    loopback(&Invite::parse(text).unwrap())
}

/// The invite, naming the loopback address (the server names its LAN one).
fn loopback(invite: &Invite) -> Invite {
    let port = invite.host.rsplit(':').next().unwrap();
    Invite {
        host: format!("127.0.0.1:{port}"),
        ..invite.clone()
    }
}

/// A member's editor: its connection, client and World.
struct Member {
    remote: Remote,
    client: Option<Client>,
    world: World,
    notices: Vec<Notice>,
}

impl Member {
    fn join(invite: &Invite, identity: Arc<Identity>) -> Member {
        Member {
            remote: Remote::join_invite(invite, identity),
            client: None,
            world: World::new(),
            notices: Vec::new(),
        }
    }

    fn step(&mut self) {
        for event in self.remote.poll() {
            match event {
                RemoteEvent::Admitted { member, .. } => {
                    self.client
                        .get_or_insert_with(|| Client::new(member))
                        .connect();
                }
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
            self.notices.extend(client.take_notices());
        }
    }

    /// Steps until `done`, or fails after a while.
    fn until(&mut self, done: impl Fn(&Member) -> bool) {
        let start = Instant::now();
        while !done(self) {
            assert!(start.elapsed() < PATIENCE, "timed out");
            self.step();
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn caught_up(&self) -> bool {
        self.client
            .as_ref()
            .is_some_and(|c| c.is_joined() && c.is_online() && c.pending() == 0)
    }

    /// Renames the Workspace's Part `from` to `name`, as one step.
    fn rename(&mut self, from: &str, name: &str) {
        let block = self
            .world
            .find_first_descendant(self.world.workspace(), from)
            .expect("the Part is there");
        self.world.set_name(block, name).unwrap();
        let client = self.client.as_mut().unwrap();
        client.capture(&mut self.world, "Rename");
        self.remote.send(client.take_outbox());
    }
}

/// The names of the Workspace's children in the project's saved World.
fn saved_names(project: &Path) -> Vec<String> {
    let world = axcenity_project::load(project).unwrap().world;
    let workspace = world.workspace();
    world
        .get(workspace)
        .unwrap()
        .children()
        .iter()
        .map(|&id| world.get(id).unwrap().name().to_owned())
        .collect()
}

/// Whether any file under `folder` contains `text`.
fn mentions(folder: &Path, text: &str) -> bool {
    std::fs::read_dir(folder).unwrap().any(|entry| {
        let path = entry.unwrap().path();
        if path.is_dir() {
            mentions(&path, text)
        } else {
            std::fs::read(&path).is_ok_and(|bytes| String::from_utf8_lossy(&bytes).contains(text))
        }
    })
}

fn owner() -> Arc<Identity> {
    Arc::new(Identity::new("Ada Qzx", [200, 80, 40]))
}

#[test]
fn an_owner_joins_with_the_first_invite_and_their_edits_are_saved() {
    let place = Place::new();
    let (mut server, said) = Server::start(&place.options()).unwrap();
    let invite = link_in(&said);
    let mut ada = Member::join(&invite, owner());
    ada.until(Member::caught_up);
    assert_eq!(ada.client.as_ref().unwrap().role(), Role::Owner);
    assert!(server.command("members").text.contains("Ada"));
    ada.rename("Block", "Door");
    ada.until(Member::caught_up);
    assert!(
        server
            .command("status")
            .text
            .contains("1 change(s) since the last save")
    );
    assert_eq!(server.command("save").text, "Saved the World (change 1).");
    assert!(saved_names(&place.project()).contains(&"Door".to_owned()));
    // Members and keys stay out of the project.
    assert!(!mentions(&place.project(), "Ada Qzx"));
    // A second start finds the Owner: no new Owner invite.
    server.stop().unwrap();
    ada.until(|m| m.notices.contains(&Notice::HostLeft));
    let (server, said) = Server::start(&place.options()).unwrap();
    assert!(
        !said.iter().any(|line| line.contains("no Owner yet")),
        "{said:?}"
    );
    server.stop().unwrap();
    assert!(!journal::has_recovery(&place.project()));
}

#[test]
fn a_server_that_stops_without_ending_brings_its_changes_back() {
    let place = Place::new();
    let (server, said) = Server::start(&place.options()).unwrap();
    let mut ada = Member::join(&link_in(&said), owner());
    ada.until(Member::caught_up);
    ada.rename("Block", "Gate");
    ada.until(Member::caught_up);
    // Killed before saving: only the journal has it.
    let mut options = place.options();
    options.port = server.port();
    server.crash();
    assert!(!saved_names(&place.project()).contains(&"Gate".to_owned()));
    // Started again on the same port, where the member keeps trying.
    let start = Instant::now();
    let (server, said) = loop {
        match Server::start(&options) {
            Ok(started) => break started,
            Err(error) if start.elapsed() < PATIENCE => {
                eprintln!("{error:#}");
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(error) => panic!("{error:#}"),
        }
    };
    assert!(
        said.iter()
            .any(|line| line.contains("stopped without ending")),
        "{said:?}"
    );
    assert!(saved_names(&place.project()).contains(&"Gate".to_owned()));
    // The member comes back to the new run of the server and edits on.
    ada.until(|m| m.caught_up() && m.notices.contains(&Notice::Joined));
    ada.rename("Gate", "Fence");
    ada.until(Member::caught_up);
    server.stop().unwrap();
    assert!(saved_names(&place.project()).contains(&"Fence".to_owned()));
}

#[test]
fn admin_commands_manage_members_and_invites() {
    let place = Place::new();
    let (mut server, said) = Server::start(&place.options()).unwrap();
    let mut ada = Member::join(&link_in(&said), owner());
    ada.until(Member::caught_up);
    // A Viewer invite, then a role change.
    let reply = server.command("invite viewer 2");
    assert!(
        reply.text.contains("as Viewer, working once for 2 day(s)"),
        "{}",
        reply.text
    );
    let mut bo = Member::join(
        &link_in(&[reply.text]),
        Arc::new(Identity::new("Bo", [10, 10, 10])),
    );
    bo.until(Member::caught_up);
    assert_eq!(bo.client.as_ref().unwrap().role(), Role::Viewer);
    let bo_id = bo.client.as_ref().unwrap().member().0;
    assert_eq!(
        server.command(&format!("role {bo_id} editor")).text,
        format!("Member {bo_id} is now Editor.")
    );
    bo.until(|m| m.client.as_ref().unwrap().role() == Role::Editor);
    // Mistakes are explained.
    assert!(
        server
            .command("role 99 editor")
            .text
            .contains("no member 99")
    );
    assert!(
        server
            .command("role x editor")
            .text
            .contains("isn't a member number")
    );
    assert!(server.command("invite king").text.contains("isn't a role"));
    assert!(server.command("fly").text.contains("Unknown command"));
    assert!(server.command("help").text.contains("invite"));
    assert_eq!(
        server.command("revoke").text,
        "Revoked every invite not used yet."
    );
    server.command(&format!("remove {bo_id}"));
    assert!(!server.command("members").text.contains("Bo"));
    // The audit log says what happened, newest last.
    let audit = server.command("audit");
    assert!(audit.text.contains("Bo was taken out"), "{}", audit.text);
    assert!(audit.text.contains("Revoked"), "{}", audit.text);
    assert_eq!(server.command("audit 1").text.lines().count(), 1);
    assert!(server.command("audit x").text.contains("isn't a number"));
    assert!(server.command("quit").quit);
    server.stop().unwrap();
}

#[test]
fn the_data_folder_may_not_be_inside_the_project() {
    let place = Place::new();
    let mut options = place.options();
    options.data = place.project().join("team");
    let error = Server::start(&options).err().expect("refused");
    assert!(error.to_string().contains("inside the project"), "{error}");
}

/// The project written another way: in other letters' case (one folder on
/// Windows and macOS), through a link, or with `..`.
#[test]
fn the_data_folder_is_refused_inside_the_project_however_it_is_written() {
    let place = Place::new();
    let refused = |data: PathBuf| {
        let mut options = place.options();
        options.data = data;
        Server::start(&options).err().map(|error| error.to_string())
    };
    let inside = |error: Option<String>| error.is_some_and(|e| e.contains("inside the project"));

    let shouted = place.0.join("PROJECT");
    if shouted.exists() {
        // A file system that ignores case: it's the project.
        assert!(inside(refused(shouted.join("team"))));
        assert!(inside(refused(shouted.join("new").join("team"))));
    }
    assert!(inside(refused(
        place.0.join("data").join("..").join("project").join("team")
    )));
    #[cfg(unix)]
    {
        let link = place.0.join("link");
        std::os::unix::fs::symlink(place.project(), &link).unwrap();
        assert!(inside(refused(link.join("team"))));
    }
    // Beside the project is fine.
    assert!(!inside(refused(place.0.join("project-data"))));
}

#[test]
fn a_byte_order_mark_before_a_command_is_ignored() {
    let place = Place::new();
    let (mut server, _) = Server::start(&place.options()).unwrap();
    let reply = server.command("\u{feff}help");
    assert_eq!(reply.text, HELP);
}
