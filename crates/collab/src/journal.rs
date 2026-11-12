//! The host's journal (TC6, design §7.10 and §9): a snapshot of the
//! session's World, then every transaction accepted since, each written
//! before anyone hears of it. A host that stops without ending the session
//! (a crash, a killed process, a power cut after the OS wrote the file)
//! loses nothing it accepted: [`recover`] gives back the World as it was.
//!
//! Both files live in the project's `.axcenity/teamcreate` folder, which
//! never syncs. The snapshot is written again every [`SNAPSHOT_EVERY`]
//! with changes, which empties the journal, so recovery never replays
//! more than that much. Ending a session cleanly removes both.
//!
//! The snapshot is a join message ([`ToClient::Welcome`]) and each entry an
//! [`ToClient::Accepted`], as [`wire`](crate::wire) writes them, so
//! Instances keep their IDs. A file from another wire format isn't read
//! (recovery says so); a half-written last entry (the host stopped while
//! writing it) is left out.
//!
//! Beside them, `handled` keeps the newest transaction number the host
//! handled from each member, across runs of the host and clean ends (C5).
//! A member whose copy is from an earlier run sends its unconfirmed edits
//! again; the host's join message says which of them it already has (the
//! ones it wrote down just before it stopped), so they don't apply twice.
//!
//! A member keeps a recovery file the same way: their edits the host
//! hasn't confirmed ([`save_unsent`]), written while they're offline, so
//! an editor that stops before the connection comes back sends them the
//! next time it joins ([`Client::recover_unsent`](crate::Client::recover_unsent)).

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use axcenity_core::World;

use crate::apply::{self, Doc, Graveyard};
use crate::host::Logged;
use crate::message::{MemberId, Snapshot, ToClient, ToHost};
use crate::op::Transaction;
use crate::role::Role;
use crate::wire;

/// The folder, inside the project, that holds the journal and snapshot.
pub const FOLDER: &str = ".axcenity/teamcreate";
const SNAPSHOT_FILE: &str = "snapshot";
const JOURNAL_FILE: &str = "journal";
const HANDLED_FILE: &str = "handled";

/// The newest transaction number handled from each member.
pub type Handled = BTreeMap<MemberId, u64>;
const UNSENT_FILE: &str = "unsent";

/// How often the host writes a fresh snapshot while things change.
pub const SNAPSHOT_EVERY: Duration = Duration::from_secs(30);

/// An open journal: entries go to the end of its file.
#[derive(Debug)]
pub struct Journal {
    folder: PathBuf,
    file: File,
    /// Entries written since the last snapshot.
    since_snapshot: usize,
}

/// What [`recover`] found from a session that didn't end cleanly.
pub struct Recovered {
    /// The session's World as the host last had it.
    pub world: World,
    pub graveyard: Graveyard,
    /// The transactions replayed on top of the last snapshot.
    pub replayed: usize,
    /// The newest transaction in it.
    pub seq: u64,
    /// The newest transaction number handled from each member, the
    /// journal's included.
    pub handled: Handled,
}

impl Journal {
    /// Starts a journal for a session in the project at `project`: a
    /// snapshot of `world` (as of transaction `seq`) and an empty journal.
    pub fn start(
        project: &Path,
        world: &World,
        graveyard: &Graveyard,
        seq: u64,
        handled: &Handled,
    ) -> io::Result<Journal> {
        let folder = project.join(FOLDER);
        fs::create_dir_all(&folder)?;
        write_whole(&folder, HANDLED_FILE, &handled_text(handled))?;
        write_snapshot(&folder, world, graveyard, seq)?;
        let file = File::create(folder.join(JOURNAL_FILE))?;
        Ok(Journal {
            folder,
            file,
            since_snapshot: 0,
        })
    }

    /// Appends one accepted transaction. The file isn't buffered here, so
    /// the bytes are the OS's as soon as this returns.
    pub fn append(&mut self, logged: &Logged) -> io::Result<()> {
        let bytes = wire::encode_to_client(&accepted(logged));
        let mut record = Vec::with_capacity(bytes.len() + 4);
        record.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        record.extend_from_slice(&bytes);
        self.file.write_all(&record)?;
        self.since_snapshot += 1;
        Ok(())
    }

    /// Entries written since the last snapshot.
    pub fn since_snapshot(&self) -> usize {
        self.since_snapshot
    }

    /// Writes a fresh snapshot (the World as of transaction `seq`) and
    /// empties the journal. The snapshot goes in first, whole, so a stop
    /// partway leaves the old snapshot and journal, or the new snapshot
    /// with entries it already has (recovery skips those).
    /// `handled` is written first: the numbers only grow, and the
    /// journal's entries still count until the new snapshot replaces it.
    pub fn snapshot(
        &mut self,
        world: &World,
        graveyard: &Graveyard,
        seq: u64,
        handled: &Handled,
    ) -> io::Result<()> {
        write_whole(&self.folder, HANDLED_FILE, &handled_text(handled))?;
        write_snapshot(&self.folder, world, graveyard, seq)?;
        self.file = File::create(self.folder.join(JOURNAL_FILE))?;
        self.since_snapshot = 0;
        Ok(())
    }

    /// The session ended cleanly: nothing to recover.
    /// `handled` stays: a member's edits from this session may reach a
    /// later one.
    pub fn end(self, handled: &Handled) -> io::Result<()> {
        drop(self.file);
        write_whole(&self.folder, HANDLED_FILE, &handled_text(handled))?;
        remove(&self.folder.join(JOURNAL_FILE))?;
        remove(&self.folder.join(SNAPSHOT_FILE))
    }
}

/// Whether the project at `project` has a session to recover.
pub fn has_recovery(project: &Path) -> bool {
    project.join(FOLDER).join(SNAPSHOT_FILE).is_file()
}

/// The World of the session the project at `project` hosted, if it
/// didn't end cleanly: its last snapshot with the journal's transactions
/// after it applied. `Ok(None)` when there's nothing to recover.
pub fn recover(project: &Path) -> io::Result<Option<Recovered>> {
    let folder = project.join(FOLDER);
    let bytes = match fs::read(folder.join(SNAPSHOT_FILE)) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let (world, graveyard, mut seq) = match wire::decode_to_client(&bytes) {
        Ok(ToClient::Welcome {
            world,
            graveyard,
            seq,
            ..
        }) => (*world.0, graveyard, seq),
        Ok(_) => return Err(invalid("the snapshot isn't a World")),
        Err(error) => return Err(invalid(&error.to_string())),
    };
    let mut doc = Doc::new(world);
    doc.graveyard = graveyard;
    let journal = read_journal(&folder)?;
    let mut handled = handled(project);
    let mut replayed = 0;
    for entry in entries(&journal) {
        let Ok(ToClient::Accepted {
            seq: entry_seq,
            author,
            client_seq,
            ops,
            ..
        }) = wire::decode_to_client(entry)
        else {
            log::warn!("Collaborate: a journal entry couldn't be read; recovery stops there");
            break;
        };
        note_handled(&mut handled, author, client_seq);
        // Already in the snapshot (it was written after this entry).
        if entry_seq <= seq {
            continue;
        }
        // The ops as the host applied them, in the same order on the same
        // World: they apply the same way again. Every ID was handed out.
        if let Err(reason) = apply::apply(&mut doc.view(), &ops, &|_| true) {
            log::warn!("Collaborate: journal transaction {entry_seq} didn't apply again: {reason}");
            break;
        }
        seq = entry_seq;
        replayed += 1;
    }
    Ok(Some(Recovered {
        world: doc.world,
        graveyard: doc.graveyard,
        replayed,
        seq,
        handled,
    }))
}

/// The newest transaction number the host handled from each member, as
/// [`Journal::snapshot`] last wrote it (none when it never did).
pub fn handled(project: &Path) -> Handled {
    fs::read_to_string(project.join(FOLDER).join(HANDLED_FILE))
        .map(|text| read_handled(&text))
        .unwrap_or_default()
}

/// Adds that the host handled `member`'s transaction `client_seq`.
pub fn note_handled(handled: &mut Handled, member: MemberId, client_seq: u64) {
    let newest = handled.entry(member).or_default();
    *newest = (*newest).max(client_seq);
}

/// "member number" lines, one a member.
fn handled_text(handled: &Handled) -> Vec<u8> {
    let mut text = String::new();
    for (member, seq) in handled {
        text.push_str(&format!("{} {seq}\n", member.0));
    }
    text.into_bytes()
}

fn read_handled(text: &str) -> Handled {
    let mut handled = Handled::new();
    for line in text.lines() {
        let mut words = line.split_whitespace();
        if let (Some(Ok(member)), Some(Ok(seq))) = (
            words.next().map(str::parse::<u32>),
            words.next().map(str::parse::<u64>),
        ) {
            note_handled(&mut handled, MemberId(member), seq);
        }
    }
    handled
}

fn read_journal(folder: &Path) -> io::Result<Vec<u8>> {
    match fs::read(folder.join(JOURNAL_FILE)) {
        Ok(bytes) => Ok(bytes),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(error),
    }
}

/// Removes what [`recover`] reads, once the recovered World is in the
/// editor (or the owner chose not to keep it).
///
/// What the journal handled from each member is kept (in `handled`), so
/// the next run of the host knows which re-sent edits it already has.
pub fn discard(project: &Path) -> io::Result<()> {
    let folder = project.join(FOLDER);
    let mut kept = handled(project);
    for entry in entries(&read_journal(&folder)?) {
        if let Ok(ToClient::Accepted {
            author, client_seq, ..
        }) = wire::decode_to_client(entry)
        {
            note_handled(&mut kept, author, client_seq);
        }
    }
    if !kept.is_empty() {
        write_whole(&folder, HANDLED_FILE, &handled_text(&kept))?;
    }
    remove(&folder.join(JOURNAL_FILE))?;
    remove(&folder.join(SNAPSHOT_FILE))
}

/// Keeps a member's unconfirmed edits in the folder `root` (their copy of
/// the session's files), replacing what was kept.
pub fn save_unsent(root: &Path, transactions: &[Transaction]) -> io::Result<()> {
    let folder = root.join(FOLDER);
    fs::create_dir_all(&folder)?;
    let mut bytes = Vec::new();
    for transaction in transactions {
        let entry = wire::encode_to_host(&ToHost::Submit(transaction.clone()));
        bytes.extend_from_slice(&(entry.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&entry);
    }
    write_whole(&folder, UNSENT_FILE, &bytes)
}

/// What [`save_unsent`] kept in `root`, oldest first (none if nothing was).
/// An entry that can't be read ends the list.
pub fn load_unsent(root: &Path) -> io::Result<Vec<Transaction>> {
    let bytes = match fs::read(root.join(FOLDER).join(UNSENT_FILE)) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut out = Vec::new();
    for entry in entries(&bytes) {
        match wire::decode_to_host(entry) {
            Ok(ToHost::Submit(transaction)) => out.push(transaction),
            _ => {
                log::warn!("Collaborate: an unsent edit couldn't be read; the rest are left out");
                break;
            }
        }
    }
    Ok(out)
}

/// Removes what [`save_unsent`] kept in `root`.
pub fn discard_unsent(root: &Path) -> io::Result<()> {
    remove(&root.join(FOLDER).join(UNSENT_FILE))
}

/// Writes `bytes` as `folder/name` whole: a stop partway leaves the old
/// file.
fn write_whole(folder: &Path, name: &str, bytes: &[u8]) -> io::Result<()> {
    let temporary = folder.join(format!("{name}.tmp"));
    {
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    fs::rename(&temporary, folder.join(name))
}

fn write_snapshot(folder: &Path, world: &World, graveyard: &Graveyard, seq: u64) -> io::Result<()> {
    let message = ToClient::Welcome {
        world: Snapshot(Box::new(world.clone())),
        graveyard: graveyard.clone(),
        seq,
        last_client_seq: 0,
        role: Role::Owner,
        ids: None,
        rejected: Vec::new(),
        files: Default::default(),
        session: String::new(),
        epoch: 0,
    };
    write_whole(folder, SNAPSHOT_FILE, &wire::encode_to_client(&message))
}

/// The journal's whole entries, in order; a cut-off last one is left out.
fn entries(bytes: &[u8]) -> impl Iterator<Item = &[u8]> {
    let mut rest = bytes;
    std::iter::from_fn(move || {
        let length = u32::from_le_bytes(rest.get(..4)?.try_into().ok()?) as usize;
        let entry = rest.get(4..4 + length)?;
        rest = &rest[4 + length..];
        Some(entry)
    })
}

fn accepted(logged: &Logged) -> ToClient {
    ToClient::Accepted {
        seq: logged.seq,
        author: logged.author,
        client_seq: logged.client_seq,
        label: logged.label.clone(),
        ops: logged.ops.clone(),
        notes: logged.notes.clone(),
    }
}

fn remove(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

fn invalid(why: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, why.to_owned())
}
