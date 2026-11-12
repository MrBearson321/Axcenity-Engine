//! A session's files (TC5): scripts, documents and assets, everything in
//! the project folder that isn't the World.
//!
//! Files are **content-addressed**. A new or changed file is uploaded once,
//! named by its SHA-256 ([`FileHash`]), and then an
//! [`Op::FileVersion`](crate::op::Op::FileVersion) in a transaction says
//! "`path` is now this hash" (or gone). The host takes it only if the
//! author's `base` is still the current version, so two saves of one file
//! never overwrite each other silently; the loser keeps their text as a
//! conflict copy ([`conflict_path`]).
//!
//! The project's files as the session sees them are a [`Manifest`]: path to
//! hash. The host keeps the bytes in a [`FileStore`]; each member's editor
//! keeps its own copy of the files in a folder ([`crate::mirror`]).
//!
//! Not synced ([`is_synced`]): the World's own files (`world.json`,
//! `levels/`; the World travels as operations), per-machine data
//! (`.axcenity/`: the cooked cache, local version history, editor layout),
//! `plugins/` (they run Lua with the editor's powers) and hidden files.

use std::collections::{BTreeMap, HashMap};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

/// A file's SHA-256.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct FileHash(pub [u8; 32]);

impl FileHash {
    pub fn of(bytes: &[u8]) -> FileHash {
        let digest = ring::digest::digest(&ring::digest::SHA256, bytes);
        let mut hash = [0; 32];
        hash.copy_from_slice(digest.as_ref());
        FileHash(hash)
    }

    pub fn hex(&self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }
}

impl std::fmt::Debug for FileHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "FileHash({})", &self.hex()[..12])
    }
}

/// A file's bytes, shared without copying.
pub type Bytes = Arc<[u8]>;

/// A piece of a file in a message. Its own type so `Debug` prints only
/// its size.
#[derive(Clone, PartialEq, Eq)]
pub struct Piece(pub Bytes);

impl std::fmt::Debug for Piece {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Piece({} bytes)", self.0.len())
    }
}

/// Every synced file of a project: its path (relative to the project
/// folder, with `/`) and its hash.
pub type Manifest = BTreeMap<String, FileHash>;

/// Files travel in pieces of this size, so one big import doesn't hold up
/// everyone's edits and no message gets near the socket's limit.
pub const CHUNK: usize = 1 << 20;

/// The largest file a session takes.
pub const MAX_FILE: u64 = 1 << 30;

/// Paths that are the World itself or per-machine.
const NOT_SYNCED: &[&str] = &[
    "world.json",
    "world.backup.json",
    "levels",
    "plugins",
    "thumbnail.png",
];

/// Each person's history (`team/history`, the teamwork crate's T4): in a
/// session only the host writes it, and the project's repository carries
/// it, so sharing it would only echo the host's own lines back.
const HISTORY: &str = "team/history";

/// True if `path` (relative, with `/`) is one of the session's files.
pub fn is_synced(path: &str) -> bool {
    let first = path.split('/').next().unwrap_or("");
    let name = path.rsplit('/').next().unwrap_or("");
    let history = path
        .strip_prefix(HISTORY)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'));
    !NOT_SYNCED.contains(&first)
        && !history
        && !matches!(name, "Thumbs.db" | "desktop.ini")
        && !path.split('/').any(|part| part.starts_with('.'))
        && !path.ends_with(".tmp")
}

/// Refuses a path that could reach outside the project folder or isn't
/// synced. The host checks every path a member sends.
pub fn check_path(path: &str) -> Result<(), String> {
    let bad = path.is_empty()
        || path.len() > 1024
        || path.starts_with('/')
        || path.contains('\\')
        || path.contains(':')
        || path.contains('\0')
        || path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..");
    if bad {
        return Err(format!("`{path}` isn't a path inside the project"));
    }
    if !is_synced(path) {
        return Err(format!("`{path}` isn't one of the session's files"));
    }
    Ok(())
}

/// Scripts and block scripts need [`crate::Capability::EditScripts`];
/// other files [`crate::Capability::EditAssets`].
pub fn is_script(path: &str) -> bool {
    path.ends_with(".lua") || path.ends_with(".blocks.json")
}

/// Where a member's losing save of `path` is kept: `<name>.conflict-<who>`
/// before the last extension, so `Door.server.lua` becomes
/// `Door.server.conflict-sam.lua`, which no loader takes for a script.
/// `taken` says whether a path exists already; a number follows if so.
pub fn conflict_path(path: &str, who: &str, taken: &dyn Fn(&str) -> bool) -> String {
    let who: String = who
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let who = who.trim_matches('-');
    let who = if who.is_empty() { "member" } else { who };
    let (folder, name) = match path.rfind('/') {
        Some(at) => (&path[..=at], &path[at + 1..]),
        None => ("", path),
    };
    let (stem, extension) = match name.rfind('.') {
        Some(at) if at > 0 => (&name[..at], &name[at..]),
        _ => (name, ""),
    };
    let mut candidate = format!("{folder}{stem}.conflict-{who}{extension}");
    let mut n = 2;
    while taken(&candidate) {
        candidate = format!("{folder}{stem}.conflict-{who}-{n}{extension}");
        n += 1;
    }
    candidate
}

/// The synced files under `root`, hashed.
pub fn scan(root: &Path) -> io::Result<Manifest> {
    let mut manifest = Manifest::new();
    for path in list(root)? {
        let bytes = std::fs::read(root.join(&path))?;
        manifest.insert(path, FileHash::of(&bytes));
    }
    Ok(manifest)
}

/// The synced files under `root`, as relative paths with `/`.
pub fn list(root: &Path) -> io::Result<Vec<String>> {
    let mut out = Vec::new();
    let mut folders = vec![(root.to_path_buf(), String::new())];
    while let Some((folder, prefix)) = folders.pop() {
        let entries = match std::fs::read_dir(&folder) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound && !prefix.is_empty() => {
                continue;
            }
            Err(error) => return Err(error),
        };
        for entry in entries {
            let entry = entry?;
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            let path = format!("{prefix}{name}");
            if !is_synced(&path) {
                continue;
            }
            let kind = entry.file_type()?;
            if kind.is_dir() {
                folders.push((entry.path(), format!("{path}/")));
            } else if kind.is_file() {
                out.push(path);
            }
        }
    }
    out.sort();
    Ok(out)
}

/// Where the host keeps file bytes.
pub trait FileStore: Send {
    /// The bytes with this hash, if the store has them.
    fn read(&self, hash: FileHash) -> Option<Bytes>;
    /// Keeps an uploaded file (its hash already checked).
    fn keep(&mut self, hash: FileHash, bytes: Bytes);
    fn has(&self, hash: FileHash) -> bool;
}

/// Files in memory: for tests and the in-editor test hub.
#[derive(Default)]
pub struct MemoryStore {
    files: HashMap<FileHash, Bytes>,
}

impl MemoryStore {
    /// A store holding `files`, with the manifest that names them.
    pub fn with(files: &[(&str, &[u8])]) -> (MemoryStore, Manifest) {
        let mut store = MemoryStore::default();
        let mut manifest = Manifest::new();
        for (path, bytes) in files {
            let hash = FileHash::of(bytes);
            store.files.insert(hash, Arc::from(*bytes));
            manifest.insert((*path).to_owned(), hash);
        }
        (store, manifest)
    }
}

impl FileStore for MemoryStore {
    fn read(&self, hash: FileHash) -> Option<Bytes> {
        self.files.get(&hash).cloned()
    }

    fn keep(&mut self, hash: FileHash, bytes: Bytes) {
        self.files.insert(hash, bytes);
    }

    fn has(&self, hash: FileHash) -> bool {
        self.files.contains_key(&hash)
    }
}

/// The host's files when the project folder is on this computer.
///
/// Files that were there when the session started are read from the
/// project folder (and checked against their hash, since the owner's own
/// editor keeps working in that folder). Every file uploaded during the
/// session is kept under `.axcenity/teamcreate/files/`, by hash, which is
/// never synced. The host never writes the project folder itself: the
/// owner's editor keeps it, like every member keeps their own copy.
pub struct FolderStore {
    root: PathBuf,
    kept: PathBuf,
    /// For each hash found at the start, a path that had it.
    found: HashMap<FileHash, String>,
}

impl FolderStore {
    /// A store for the project at `root`, and its files as they are now.
    pub fn open(root: &Path) -> io::Result<(FolderStore, Manifest)> {
        let manifest = scan(root)?;
        let found = manifest
            .iter()
            .map(|(path, &hash)| (hash, path.clone()))
            .collect();
        let kept = root.join(".axcenity").join("teamcreate").join("files");
        Ok((
            FolderStore {
                root: root.to_path_buf(),
                kept,
                found,
            },
            manifest,
        ))
    }

    fn kept_path(&self, hash: FileHash) -> PathBuf {
        self.kept.join(hash.hex())
    }
}

impl FileStore for FolderStore {
    fn read(&self, hash: FileHash) -> Option<Bytes> {
        if let Ok(bytes) = std::fs::read(self.kept_path(hash))
            && FileHash::of(&bytes) == hash
        {
            return Some(Arc::from(bytes));
        }
        let path = self.found.get(&hash)?;
        let bytes = std::fs::read(self.root.join(path)).ok()?;
        // The owner may have changed it since; then it isn't this version.
        (FileHash::of(&bytes) == hash).then(|| Arc::from(bytes))
    }

    fn keep(&mut self, hash: FileHash, bytes: Bytes) {
        let path = self.kept_path(hash);
        if path.exists() {
            return;
        }
        let written = std::fs::create_dir_all(&self.kept).and_then(|()| {
            let temporary = path.with_extension("part");
            std::fs::write(&temporary, &bytes)?;
            std::fs::rename(&temporary, &path)
        });
        if let Err(error) = written {
            log::warn!("Collaborate: couldn't keep an uploaded file: {error}");
        }
    }

    fn has(&self, hash: FileHash) -> bool {
        self.kept_path(hash).exists() || self.found.contains_key(&hash)
    }
}

/// Puts a file back together from its pieces.
#[derive(Debug)]
pub(crate) struct Assembly {
    bytes: Vec<u8>,
    total: u64,
}

impl Assembly {
    /// Adds the piece at `at`. Returns the whole file once it's complete,
    /// or `Err` if the pieces don't fit (they come in order on one
    /// connection, so a gap means something's wrong).
    pub(crate) fn add(
        assemblies: &mut HashMap<FileHash, Assembly>,
        hash: FileHash,
        at: u64,
        total: u64,
        piece: &[u8],
    ) -> Result<Option<Bytes>, ()> {
        if total > MAX_FILE {
            return Err(());
        }
        if at == 0 {
            assemblies.insert(
                hash,
                Assembly {
                    bytes: Vec::with_capacity(total.min(CHUNK as u64 * 4) as usize),
                    total,
                },
            );
        }
        let Some(assembly) = assemblies.get_mut(&hash) else {
            return Err(());
        };
        if assembly.total != total
            || assembly.bytes.len() as u64 != at
            || at + piece.len() as u64 > total
        {
            assemblies.remove(&hash);
            return Err(());
        }
        assembly.bytes.extend_from_slice(piece);
        if (assembly.bytes.len() as u64) < total {
            return Ok(None);
        }
        let assembly = assemblies.remove(&hash).expect("found above");
        if FileHash::of(&assembly.bytes) != hash {
            return Err(());
        }
        Ok(Some(Arc::from(assembly.bytes)))
    }
}

/// `bytes` in pieces: (offset, piece). An empty file is one empty piece.
pub(crate) fn pieces(bytes: &Bytes) -> impl Iterator<Item = (u64, Bytes)> + '_ {
    let count = bytes.len().div_ceil(CHUNK).max(1);
    (0..count).map(move |i| {
        let start = i * CHUNK;
        let end = (start + CHUNK).min(bytes.len());
        (start as u64, Arc::from(&bytes[start..end]))
    })
}
