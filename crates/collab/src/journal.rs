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

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use axcenity_core::World;

use crate::apply::{self, Doc, Graveyard};
use crate::host::Logged;
use crate::message::{Snapshot, ToClient};
use crate::role::Role;
use crate::wire;

/// The folder, inside the project, that holds the journal and snapshot.
pub const FOLDER: &str = ".axcenity/teamcreate";
const SNAPSHOT_FILE: &str = "snapshot";
const JOURNAL_FILE: &str = "journal";

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
}

impl Journal {
    /// Starts a journal for a session in the project at `project`: a
    /// snapshot of `world` (as of transaction `seq`) and an empty journal.
    pub fn start(
        project: &Path,
        world: &World,
        graveyard: &Graveyard,
        seq: u64,
    ) -> io::Result<Journal> {
        let folder = project.join(FOLDER);
        fs::create_dir_all(&folder)?;
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
    pub fn snapshot(&mut self, world: &World, graveyard: &Graveyard, seq: u64) -> io::Result<()> {
        write_snapshot(&self.folder, world, graveyard, seq)?;
        self.file = File::create(self.folder.join(JOURNAL_FILE))?;
        self.since_snapshot = 0;
        Ok(())
    }

    /// The session ended cleanly: nothing to recover.
    pub fn end(self) -> io::Result<()> {
        drop(self.file);
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
    let journal = match fs::read(folder.join(JOURNAL_FILE)) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => Vec::new(),
        Err(error) => return Err(error),
    };
    let mut replayed = 0;
    for entry in entries(&journal) {
        let Ok(ToClient::Accepted {
            seq: entry_seq,
            ops,
            ..
        }) = wire::decode_to_client(entry)
        else {
            log::warn!("Team Create: a journal entry couldn't be read; recovery stops there");
            break;
        };
        // Already in the snapshot (it was written after this entry).
        if entry_seq <= seq {
            continue;
        }
        // The ops as the host applied them, in the same order on the same
        // World: they apply the same way again. Every ID was handed out.
        if let Err(reason) = apply::apply(&mut doc.view(), &ops, &|_| true) {
            log::warn!("Team Create: journal transaction {entry_seq} didn't apply again: {reason}");
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
    }))
}

/// Removes what [`recover`] reads, once the recovered World is in the
/// editor (or the owner chose not to keep it).
pub fn discard(project: &Path) -> io::Result<()> {
    let folder = project.join(FOLDER);
    remove(&folder.join(JOURNAL_FILE))?;
    remove(&folder.join(SNAPSHOT_FILE))
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
    };
    let temporary = folder.join(format!("{SNAPSHOT_FILE}.tmp"));
    {
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&temporary)?;
        file.write_all(&wire::encode_to_client(&message))?;
        file.sync_all()?;
    }
    fs::rename(&temporary, folder.join(SNAPSHOT_FILE))
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
