//! Collaborate's self-hosted server (TC7, design §4): a project folder
//! hosted for editors to join, with no editor of its own, so a team's
//! project is there whenever someone wants to work on it.
//!
//! It hosts the way the hosting editor does ([`axcenity_collab::hosted`]):
//! the same session name, members file, certificate and journal, so a
//! member's editor can't tell the two apart. It writes the project's
//! `world.json` every [`SAVE_EVERY`] with changes, and when it stops; the
//! journal covers the time between (a crash, or a killed process, loses
//! nothing: the next start brings it back and saves it). Members, invites
//! and the certificate are kept in the server's data folder, never in the
//! project folder.
//!
//! The admin commands ([`Server::command`]) list and manage members and
//! invites, save, and stop. A project with no Owner yet gets an Owner
//! invite when the server starts.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use axcenity_collab::access::{Access, Invite};
use axcenity_collab::hosted::{members_file, session_id, session_world};
use axcenity_collab::session::{HostSession, Hosting};
use axcenity_collab::socket::{DEFAULT_PORT, HostCert};
use axcenity_collab::{FolderStore, Host, MemberId, Role, journal};

/// How often the server writes the project's World while it changes.
pub const SAVE_EVERY: Duration = Duration::from_secs(30);

/// How long an invite works unless the command says otherwise.
pub const INVITE_DAYS: u64 = 7;

const DAY: u64 = 24 * 60 * 60;

/// What to host, and where.
#[derive(Clone, Debug)]
pub struct Options {
    /// The project folder.
    pub project: PathBuf,
    /// Where members, invites and the certificate are kept.
    pub data: PathBuf,
    /// The port to listen on (0 picks a free one).
    pub port: u16,
    /// "host:port" as invites name the server; this computer's LAN address
    /// and the port when `None`.
    pub address: Option<String>,
    /// Without `address`: name this computer by its Tailscale name when
    /// Tailscale is connected (C3). Off with `--no-tailscale`.
    pub tailscale: bool,
}

impl Options {
    /// Hosts `project` on the default port, with the default data folder.
    pub fn new(project: impl Into<PathBuf>) -> Result<Options> {
        Ok(Options {
            project: project.into(),
            data: default_data_folder()
                .context("there's no data folder for this user: pass --data <folder>")?,
            port: DEFAULT_PORT,
            address: None,
            tailscale: true,
        })
    }
}

/// Where the server keeps members, invites and its certificate unless told:
/// this user's application data.
pub fn default_data_folder() -> Option<PathBuf> {
    let home = |var: &str| std::env::var_os(var).map(PathBuf::from);
    let base = if cfg!(windows) {
        home("APPDATA")
    } else if cfg!(target_os = "macos") {
        home("HOME").map(|h| h.join("Library").join("Application Support"))
    } else {
        home("XDG_DATA_HOME").or_else(|| home("HOME").map(|h| h.join(".local").join("share")))
    }?;
    Some(axcenity_collab::hosted::renamed_folder(
        &base.join("Axcenity"),
        "Collaborate Server",
        "Team Create Server",
    ))
}

/// What a command said, and whether the server should stop.
#[derive(Debug, PartialEq, Eq)]
pub struct Reply {
    pub text: String,
    pub quit: bool,
}

impl Reply {
    fn say(text: impl Into<String>) -> Reply {
        Reply {
            text: text.into(),
            quit: false,
        }
    }
}

/// A project hosted by the server.
pub struct Server {
    project: PathBuf,
    hosting: Option<Hosting>,
    port: u16,
    /// The newest transaction written to `world.json`.
    saved_seq: u64,
    /// When the server last looked whether to save.
    save_checked: Instant,
}

/// The commands, for `help`.
pub const HELP: &str = "\
status                       where it's hosted, who's here, what's saved
members                      everyone in the project
invite [editor|viewer|owner] [days]
                             a link that works once (editor, 7 days unless said)
revoke                       revokes every invite not used yet
role <member> <editor|viewer|owner>
                             changes a member's role
remove <member>              takes a member out of the project
save                         writes the project's World now
quit                         saves, tells everyone the session ended, and stops";

impl Server {
    /// Loads the project in `options.project` and starts hosting it.
    /// Returns the server and what to tell whoever started it (an Owner
    /// invite if the project has no Owner yet, a recovered session).
    pub fn start(options: &Options) -> Result<(Server, Vec<String>)> {
        let project =
            std::path::absolute(&options.project).unwrap_or_else(|_| options.project.clone());
        let data = std::path::absolute(&options.data).unwrap_or_else(|_| options.data.clone());
        if is_inside(&data, &project) {
            bail!(
                "the data folder `{}` is inside the project: members and keys must never go \
                 in the project folder (it may be pushed somewhere public)",
                data.display()
            );
        }
        let mut said = Vec::new();
        let loaded = axcenity_project::load(&project)
            .with_context(|| format!("couldn't open the project in `{}`", project.display()))?;
        for problem in &loaded.problems {
            said.push(format!("warning: {problem}"));
        }
        let mut world = loaded.world;
        // A session this server (or an editor) hosted that stopped without
        // ending: its World, from the journal, is the project now.
        if journal::has_recovery(&project) {
            let recovered = journal::recover(&project).with_context(|| {
                format!(
                    "the last session stopped without ending and its journal (in `{}`) \
                     couldn't be read",
                    journal::FOLDER
                )
            })?;
            if let Some(recovered) = recovered {
                let strings = world.strings().clone();
                world = recovered.world;
                world.set_strings(strings);
                axcenity_project::save_world(&project, &world)
                    .context("couldn't save the recovered World")?;
                journal::discard(&project).context("couldn't remove the recovered journal")?;
                said.push(format!(
                    "The last session stopped without ending: its World is back and saved \
                     ({} change(s) after its last snapshot).",
                    recovered.replayed
                ));
            }
        }
        let cert = HostCert::load_or_make(&data.join("host-certificate.json"))
            .context("couldn't read or make the server's certificate")?;
        let members = members_file(&data, &project);
        let access = match Access::load(&members) {
            Ok(access) => access,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Access::empty(),
            Err(error) => return Err(error).context("couldn't read the members list"),
        };
        let (store, files) =
            FolderStore::open(&project).context("couldn't read the project's files")?;
        let mut host = Host::new(session_world(world))
            .with_files(files, Box::new(store))
            .with_session(session_id(&cert.fingerprint(), &project));
        host.start_journal(&project)
            .context("couldn't start the session's journal")?;
        let mut session = HostSession::start(
            host,
            access,
            Some(members),
            &cert,
            std::net::SocketAddr::from(([0, 0, 0, 0], options.port)),
            options.address.clone(),
        )
        .with_context(|| format!("couldn't listen on port {}", options.port))?;
        let port = session.local_address().port();
        if options.address.is_none() && options.tailscale {
            use axcenity_collab::tailscale::{State, advertise};
            said.push(match advertise(&mut session) {
                State::Running(net) => format!(
                    "Reachable over Tailscale as {} ({}). Members on other tailnets need this \
                     computer shared with them (Tailscale admin console: Machines › … › Share).",
                    session.address(),
                    net.login.as_deref().unwrap_or("signed in")
                ),
                other => format!(
                    "{} Invites name this computer's address on its network.",
                    other.describe()
                ),
            });
        }
        if !session
            .access()
            .members()
            .iter()
            .any(|m| m.role == Role::Owner)
        {
            let invite = session.invite(Role::Owner, INVITE_DAYS * DAY);
            said.push(format!(
                "This project has no Owner yet. Open this link in your editor (Join \
                 Collaborate Session…) to be its Owner; it works once, for {INVITE_DAYS} days:\n{}",
                invite.link()
            ));
        }
        let server = Server {
            project,
            hosting: Some(session.serve()),
            port,
            saved_seq: 0,
            save_checked: Instant::now(),
        };
        said.insert(0, server.status());
        Ok((server, said))
    }

    /// The port it listens on.
    pub fn port(&self) -> u16 {
        self.port
    }

    fn hosting(&self) -> &Hosting {
        self.hosting.as_ref().expect("running until stopped")
    }

    /// Saves the World if it changed and [`SAVE_EVERY`] has passed. Call it
    /// a few times a second. Returns what to report.
    pub fn tick(&mut self) -> Option<String> {
        if self.save_checked.elapsed() < SAVE_EVERY {
            return None;
        }
        self.save_checked = Instant::now();
        match self.save() {
            Ok(None) => None,
            Ok(Some(text)) => Some(text),
            Err(error) => Some(format!("error: {error:#}")),
        }
    }

    /// Writes the session's World to the project's `world.json` if it
    /// changed since the last save; what happened, if anything.
    pub fn save(&mut self) -> Result<Option<String>> {
        let (world, seq) = {
            let session = self.hosting().lock();
            let host = session.host();
            if host.seq() == self.saved_seq {
                return Ok(None);
            }
            (host.world().clone(), host.seq())
        };
        axcenity_project::save_world(&self.project, &world)
            .context("couldn't save the project's World")?;
        self.saved_seq = seq;
        Ok(Some(format!("Saved the World (change {seq}).")))
    }

    /// Saves, ends the session cleanly (its journal goes) and tells every
    /// member it ended.
    pub fn stop(mut self) -> Result<()> {
        self.save()?;
        if let Some(hosting) = self.hosting.take() {
            hosting.lock().end_cleanly();
            // Dropping it tells everyone and closes every connection.
            drop(hosting);
        }
        Ok(())
    }

    /// Stops as a crash or a killed process would: nothing saved, nobody
    /// told, the journal left for the next start (tests).
    #[cfg(test)]
    fn crash(mut self) {
        if let Some(hosting) = self.hosting.take() {
            hosting.stop_without_a_word();
        }
    }

    /// Runs one admin command (see [`HELP`]).
    pub fn command(&mut self, line: &str) -> Reply {
        // Windows PowerShell starts what it pipes in with a byte-order mark.
        let line = line.trim_start_matches('\u{feff}');
        let words: Vec<&str> = line.split_whitespace().collect();
        match words.as_slice() {
            [] => Reply::say(""),
            ["help" | "?"] => Reply::say(HELP),
            ["status"] => Reply::say(self.status()),
            ["members"] => Reply::say(self.members()),
            ["invite", rest @ ..] => match invite_terms(rest) {
                Ok((role, days)) => {
                    let link = self.invite(role, days).link();
                    Reply::say(format!(
                        "An invite to join as {role:?}, working once for {days} day(s):\n{link}"
                    ))
                }
                Err(why) => Reply::say(why),
            },
            ["revoke"] => {
                self.hosting().lock().revoke_invites();
                Reply::say("Revoked every invite not used yet.")
            }
            ["role", member, role] => match (self.member(member), parse_role(role)) {
                (Err(why), _) | (_, Err(why)) => Reply::say(why),
                (Ok(member), Ok(role)) => {
                    self.hosting().lock().set_role(member, role);
                    Reply::say(format!("Member {} is now {role:?}.", member.0))
                }
            },
            ["remove", member] => match self.member(member) {
                Err(why) => Reply::say(why),
                Ok(member) => {
                    self.hosting().lock().remove(member);
                    Reply::say(format!("Took member {} out of the project.", member.0))
                }
            },
            ["save"] => Reply::say(match self.save() {
                Ok(Some(text)) => text,
                Ok(None) => "Nothing changed since the last save.".to_owned(),
                Err(error) => format!("error: {error:#}"),
            }),
            ["quit" | "stop" | "exit"] => Reply {
                text: "Stopping…".to_owned(),
                quit: true,
            },
            _ => Reply::say(format!(
                "Unknown command `{line}`. Type `help` for the commands."
            )),
        }
    }

    /// A new invite for `role`, working once for `days` days.
    pub fn invite(&mut self, role: Role, days: u64) -> Invite {
        self.hosting().lock().invite(role, days * DAY)
    }

    fn status(&self) -> String {
        let session = self.hosting().lock();
        let online = session.connected().len();
        let host = session.host();
        let unsaved = if host.seq() == self.saved_seq {
            "everything saved".to_owned()
        } else {
            format!(
                "{} change(s) since the last save",
                host.seq() - self.saved_seq
            )
        };
        format!(
            "Hosting `{}` on port {} (invites name {}): {online} connected, {unsaved}.",
            self.project.display(),
            self.port,
            session.address(),
        )
    }

    fn members(&self) -> String {
        let session = self.hosting().lock();
        let connected = session.connected();
        let members = session.access().members();
        if members.is_empty() {
            return "No members yet: make an invite with `invite owner`.".to_owned();
        }
        members
            .iter()
            .map(|m| {
                format!(
                    "{:>3}  {:<20} {:<7} {:<8} {}",
                    m.id.0,
                    m.name,
                    format!("{:?}", m.role),
                    if connected.contains(&m.id) {
                        "online"
                    } else {
                        "offline"
                    },
                    m.key.fingerprint().short()
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The member a command names by number.
    fn member(&self, text: &str) -> std::result::Result<MemberId, String> {
        let id: u32 = text
            .parse()
            .map_err(|_| format!("`{text}` isn't a member number: see `members`"))?;
        let session = self.hosting().lock();
        session
            .access()
            .members()
            .iter()
            .find(|m| m.id.0 == id)
            .map(|m| m.id)
            .ok_or_else(|| format!("There's no member {id}: see `members`"))
    }
}

fn parse_role(text: &str) -> std::result::Result<Role, String> {
    match text {
        "editor" => Ok(Role::Editor),
        "viewer" => Ok(Role::Viewer),
        "owner" => Ok(Role::Owner),
        _ => Err(format!("`{text}` isn't a role: editor, viewer or owner")),
    }
}

/// `invite`'s role and days, either way round.
fn invite_terms(words: &[&str]) -> std::result::Result<(Role, u64), String> {
    let mut role = Role::Editor;
    let mut days = INVITE_DAYS;
    for word in words {
        match word.parse::<u64>() {
            Ok(0) => return Err("An invite has to work for at least a day.".to_owned()),
            Ok(n) => days = n.min(365),
            Err(_) => role = parse_role(word)?,
        }
    }
    Ok((role, days))
}

/// Whether `path` is the project folder of a project.
pub fn is_project(path: &Path) -> bool {
    axcenity_project::read_manifest(path).is_some()
}

#[cfg(test)]
mod tests;

/// Whether `folder` is `project` or inside it, however either is written:
/// another letter case, a Windows short (8.3) name such as `PAULJA~1`, a
/// link, `..`. Comparing the paths as text missed those, and a data folder
/// given that way put the host's private key in the project.
fn is_inside(folder: &Path, project: &Path) -> bool {
    let Ok(project) = std::fs::canonicalize(project) else {
        return folder.starts_with(project);
    };
    // The nearest folder that exists, made canonical (on Windows that gives
    // the long name in the case on disk), then the rest added back.
    let mut existing = folder;
    let mut rest = Vec::new();
    let mut resolved = loop {
        if let Ok(path) = std::fs::canonicalize(existing) {
            break path;
        }
        match (existing.parent(), existing.components().next_back()) {
            (Some(parent), Some(last)) => {
                rest.push(last);
                existing = parent;
            }
            _ => return folder.starts_with(&project),
        }
    };
    for part in rest.into_iter().rev() {
        match part {
            std::path::Component::ParentDir => {
                resolved.pop();
            }
            std::path::Component::CurDir => {}
            part => resolved.push(part),
        }
    }
    resolved.starts_with(&project)
        || resolved
            .ancestors()
            .any(|folder| same_folder(folder, &project))
}

/// Whether `a` and `b` are the same folder on disk. On macOS a folder can
/// be named in any letter case and canonicalizing keeps the case given, so
/// this compares the folders themselves.
#[cfg(unix)]
fn same_folder(a: &Path, b: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    match (std::fs::metadata(a), std::fs::metadata(b)) {
        (Ok(a), Ok(b)) => a.dev() == b.dev() && a.ino() == b.ino(),
        _ => false,
    }
}

/// On Windows canonicalizing already gives one spelling per folder.
#[cfg(not(unix))]
fn same_folder(_: &Path, _: &Path) -> bool {
    false
}
