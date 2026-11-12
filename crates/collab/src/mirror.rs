//! A member's copy of the session's files in a folder on their computer
//! (TC5). The editor works on the folder as on any project; the mirror
//! keeps it and the session in step.
//!
//! For each file the mirror remembers the version the folder and the
//! session last agreed on (`known`). Each [`Mirror::sync`] compares three
//! versions of every file: the folder's, the session's
//! ([`Client::files`]) and that one.
//!
//! | Folder | Session | What happens |
//! |---|---|---|
//! | as known | as known | nothing |
//! | changed | as known | published (unless someone else is editing it: then it waits) |
//! | as known | changed | the session's version is written (fetched first) |
//! | changed | changed the same way | nothing (now known) |
//! | changed | changed differently | the folder's version is kept as a conflict copy, which is published, and the session's is written |
//!
//! A save the host refuses (someone else saved first) puts `known` back to
//! what the save was based on, so the next sync finds both sides changed
//! and keeps the member's text as a conflict copy. Nothing is ever
//! overwritten that the session doesn't have.
//!
//! What the mirror knows is kept in `.axcenity/teamcreate/mirror.json`, so
//! files changed while the editor was away from the session are published
//! when it rejoins.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use axcenity_core::World;
use serde::{Deserialize, Serialize};

use crate::apply::Refusal;
use crate::client::Client;
use crate::files::{self, Bytes, FileHash, Manifest};

/// How long a fetch may go unanswered before it's asked again.
const ASK_AGAIN: Duration = Duration::from_secs(5);

/// Something the mirror did to the folder or the session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    /// Wrote the session's version of a file.
    Wrote(String),
    /// Deleted a file the session deleted.
    Removed(String),
    /// Published the folder's version (in transaction `client_seq`).
    Published { path: String, client_seq: u64 },
    /// Both sides changed `path`: the folder's version is now `copy`.
    Conflict { path: String, copy: String },
    /// The session didn't take a file.
    Failed { path: String, reason: Refusal },
}

#[derive(Default, Serialize, Deserialize)]
struct Saved {
    session: String,
    known: BTreeMap<String, String>,
}

/// One member's folder, kept in step with the session's files.
pub struct Mirror {
    root: PathBuf,
    /// Names this session, so a folder reused for another one starts
    /// afresh.
    session: String,
    /// Who this member is, for conflict copies' names.
    name: String,
    /// Each file's version when the folder and the session last agreed.
    known: Manifest,
    /// True until the first sync after joining, when a mirror with nothing
    /// remembered learns which of the folder's files the session has.
    fresh: bool,
    /// What the folder held at the last look, so unchanged files aren't
    /// read again: path to (length, modified, hash).
    seen: HashMap<String, (u64, Option<SystemTime>, FileHash)>,
    /// Published and not confirmed yet: path to (the version it was based
    /// on, the transaction).
    in_flight: HashMap<String, (Option<FileHash>, u64)>,
    /// Files asked for, and when.
    asked: HashMap<FileHash, Instant>,
    /// Fetched files not written yet.
    fetched: HashMap<FileHash, Bytes>,
    dirty: bool,
}

impl Mirror {
    /// The mirror of session `session` in `root`, with what it remembers
    /// from earlier visits to the same session.
    pub fn open(root: &Path, session: &str, name: &str) -> Mirror {
        let saved = std::fs::read(state_path(root))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Saved>(&bytes).ok())
            .filter(|saved| saved.session == session);
        let known: Manifest = saved
            .map(|saved| {
                saved
                    .known
                    .into_iter()
                    .filter_map(|(path, hex)| Some((path, from_hex(&hex)?)))
                    .collect()
            })
            .unwrap_or_default();
        Mirror {
            root: root.to_path_buf(),
            session: session.to_owned(),
            name: name.to_owned(),
            fresh: known.is_empty(),
            known,
            seen: HashMap::new(),
            in_flight: HashMap::new(),
            asked: HashMap::new(),
            fetched: HashMap::new(),
            dirty: false,
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn set_name(&mut self, name: &str) {
        self.name = name.to_owned();
    }

    /// True while files the session has aren't in the folder yet.
    pub fn is_catching_up(&self, client: &Client) -> bool {
        client.files().iter().any(|(path, hash)| {
            self.known.get(path) != Some(hash) && !self.in_flight.contains_key(path)
        })
    }

    /// Brings the folder and the session into step as far as it can now
    /// (fetched files arrive later; call it again). World edits not
    /// captured yet go with what it publishes, so call it where the
    /// editor would hand the client a message, not in the middle of a
    /// drag.
    pub fn sync(&mut self, client: &mut Client, world: &mut World) -> io::Result<Vec<Change>> {
        self.sync_as(client, world, None)
    }

    /// [`Mirror::sync`], naming the step that publishes what changed
    /// `label` (an import: its files and the model placed from them go as
    /// one step).
    pub fn sync_as(
        &mut self,
        client: &mut Client,
        world: &mut World,
        label: Option<&str>,
    ) -> io::Result<Vec<Change>> {
        if !client.is_joined() {
            return Ok(Vec::new());
        }
        for (hash, bytes) in client.take_files() {
            self.asked.remove(&hash);
            self.fetched.insert(hash, bytes);
        }
        for hash in client.take_missing() {
            self.asked.remove(&hash);
        }
        // Saves the host has answered.
        let answered: Vec<String> = self
            .in_flight
            .iter()
            .filter(|(_, (_, seq))| !client.is_pending(*seq))
            .map(|(path, _)| path.clone())
            .collect();
        for path in answered {
            let (base, seq) = self.in_flight.remove(&path).expect("listed above");
            if client.was_refused(seq) {
                set(&mut self.known, &path, base);
                self.dirty = true;
            }
        }

        let disk = self.look()?;
        let session = client.files().clone();
        if self.fresh {
            self.fresh = false;
            for (path, hash) in &disk {
                if session.contains_key(path) {
                    self.known.insert(path.clone(), *hash);
                }
            }
            self.dirty = true;
        }

        let mut changes = Vec::new();
        let mut publish: Vec<(String, Option<Bytes>)> = Vec::new();
        let mut want = Vec::new();
        let paths: BTreeSet<String> = disk
            .keys()
            .chain(session.keys())
            .chain(self.known.keys())
            .cloned()
            .collect();
        for path in paths {
            if self.in_flight.contains_key(&path) {
                continue;
            }
            let here = disk.get(&path).copied();
            let there = session.get(&path).copied();
            let known = self.known.get(&path).copied();
            match (here != known, there != known) {
                (false, false) => {}
                (true, false) => {
                    if !client.can_change_file(&path) {
                        // Someone else is editing it (their save will make
                        // this a conflict, or the lock ends and it goes),
                        // or this member's role doesn't change files.
                        continue;
                    }
                    let bytes = match here {
                        Some(_) => Some(Bytes::from(std::fs::read(self.root.join(&path))?)),
                        None => None,
                    };
                    publish.push((path, bytes));
                }
                (false, true) => {
                    if let Some(change) = self.take_theirs(&path, there, &mut want)? {
                        changes.push(change);
                    }
                }
                (true, true) if here == there => {
                    set(&mut self.known, &path, here);
                    self.dirty = true;
                }
                (true, true) => {
                    if here.is_some() {
                        let taken = |p: &str| {
                            disk.contains_key(p)
                                || session.contains_key(p)
                                || self.root.join(p).exists()
                        };
                        let copy = files::conflict_path(&path, &self.name, &taken);
                        std::fs::rename(self.root.join(&path), self.root.join(&copy))?;
                        self.seen.remove(&path);
                        let bytes = Bytes::from(std::fs::read(self.root.join(&copy))?);
                        publish.push((copy.clone(), Some(bytes)));
                        changes.push(Change::Conflict {
                            path: path.clone(),
                            copy,
                        });
                    }
                    // Deleted here but changed there: theirs comes back.
                    if let Some(change) = self.take_theirs(&path, there, &mut want)? {
                        changes.push(change);
                    }
                }
            }
        }

        if !publish.is_empty() {
            changes.extend(self.publish(client, world, publish, label));
        }
        let now = Instant::now();
        want.retain(|hash| {
            self.asked
                .get(hash)
                .is_none_or(|&at| now.duration_since(at) >= ASK_AGAIN)
        });
        want.sort();
        want.dedup();
        if client.fetch(want.clone()) {
            for hash in want {
                self.asked.insert(hash, now);
            }
        }
        // What was written is no longer needed in memory.
        let needed: BTreeSet<FileHash> = session.values().copied().collect();
        self.fetched.retain(|hash, _| needed.contains(hash));
        if self.dirty {
            self.save();
        }
        Ok(changes)
    }

    /// Writes the session's version of `path` (or deletes it), if its
    /// bytes are here; otherwise adds it to `want`.
    fn take_theirs(
        &mut self,
        path: &str,
        there: Option<FileHash>,
        want: &mut Vec<FileHash>,
    ) -> io::Result<Option<Change>> {
        let full = self.root.join(path);
        let Some(hash) = there else {
            match std::fs::remove_file(&full) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
            self.seen.remove(path);
            self.known.remove(path);
            self.dirty = true;
            return Ok(Some(Change::Removed(path.to_owned())));
        };
        let Some(bytes) = self.fetched.get(&hash) else {
            want.push(hash);
            return Ok(None);
        };
        if let Some(folder) = full.parent() {
            std::fs::create_dir_all(folder)?;
        }
        let temporary = full.with_file_name(format!(
            ".{}.teamcreate",
            full.file_name().and_then(|n| n.to_str()).unwrap_or("file")
        ));
        std::fs::write(&temporary, bytes)?;
        std::fs::rename(&temporary, &full)?;
        let meta = std::fs::metadata(&full)?;
        self.seen
            .insert(path.to_owned(), (meta.len(), meta.modified().ok(), hash));
        self.known.insert(path.to_owned(), hash);
        self.dirty = true;
        Ok(Some(Change::Wrote(path.to_owned())))
    }

    fn publish(
        &mut self,
        client: &mut Client,
        world: &mut World,
        publish: Vec<(String, Option<Bytes>)>,
        label: Option<&str>,
    ) -> Vec<Change> {
        let label = match (label, publish.as_slice()) {
            (Some(label), _) => label.to_owned(),
            (None, [(path, _)]) => format!("Save {path}"),
            (None, list) => format!("Save {} files", list.len()),
        };
        let versions: Vec<(String, Option<FileHash>)> = publish
            .iter()
            .map(|(path, bytes)| (path.clone(), bytes.as_ref().map(|b| FileHash::of(b))))
            .collect();
        let bases: Vec<Option<FileHash>> = versions
            .iter()
            .map(|(path, _)| client.files().get(path).copied())
            .collect();
        match client.publish(world, label, publish) {
            Ok(Some(client_seq)) => versions
                .into_iter()
                .zip(bases)
                .map(|((path, hash), base)| {
                    self.in_flight.insert(path.clone(), (base, client_seq));
                    set(&mut self.known, &path, hash);
                    self.dirty = true;
                    Change::Published { path, client_seq }
                })
                .collect(),
            Ok(None) => Vec::new(),
            Err(reason) => versions
                .into_iter()
                .map(|(path, _)| Change::Failed {
                    path,
                    reason: reason.clone(),
                })
                .collect(),
        }
    }

    /// The folder's synced files and their hashes, reading only files whose
    /// length or time changed since the last look.
    fn look(&mut self) -> io::Result<Manifest> {
        let mut out = Manifest::new();
        let mut seen = HashMap::new();
        for path in files::list(&self.root)? {
            let meta = match std::fs::metadata(self.root.join(&path)) {
                Ok(meta) => meta,
                // Gone since the listing.
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error),
            };
            let (len, modified) = (meta.len(), meta.modified().ok());
            let hash = match self.seen.get(&path) {
                Some(&(l, m, hash)) if l == len && m == modified && modified.is_some() => hash,
                _ => FileHash::of(&std::fs::read(self.root.join(&path))?),
            };
            seen.insert(path.clone(), (len, modified, hash));
            out.insert(path, hash);
        }
        self.seen = seen;
        Ok(out)
    }

    fn save(&mut self) {
        let saved = Saved {
            session: self.session.clone(),
            known: self
                .known
                .iter()
                .map(|(path, hash)| (path.clone(), hash.hex()))
                .collect(),
        };
        let path = state_path(&self.root);
        let written = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| {
                std::fs::write(&path, serde_json::to_vec(&saved).expect("JSON of strings"))
            });
        match written {
            Ok(()) => self.dirty = false,
            Err(error) => log::warn!("Collaborate: couldn't save the mirror's state: {error}"),
        }
    }
}

fn state_path(root: &Path) -> PathBuf {
    root.join(".axcenity")
        .join("teamcreate")
        .join("mirror.json")
}

fn set(manifest: &mut Manifest, path: &str, hash: Option<FileHash>) {
    match hash {
        Some(hash) => manifest.insert(path.to_owned(), hash),
        None => manifest.remove(path),
    };
}

fn from_hex(hex: &str) -> Option<FileHash> {
    if hex.len() != 64 {
        return None;
    }
    let mut out = [0; 32];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(hex.get(i * 2..i * 2 + 2)?, 16).ok()?;
    }
    Some(FileHash(out))
}
