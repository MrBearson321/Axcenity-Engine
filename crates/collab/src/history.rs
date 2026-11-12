//! The host's checkpoints of a session (C9, design §8): the session's World
//! and its files as they were at a moment, kept in the project's
//! `.axcenity/teamcreate/history` on the host's computer (which never
//! syncs or exports), so a bad session can be rolled back for everyone.
//!
//! Checkpoints are made when a session starts, every
//! [`CHECKPOINT_EVERY`] while things change, when a member asks (Save
//! Checkpoint), and before a restore. Each is a folder named by its number
//! holding `info` (what it is, who asked, who changed things since the one
//! before, the project's files by hash) and `world` (the World, as a join
//! message writes it, so Instances keep their IDs). File contents are kept
//! once each, by hash, in `files/`: an hour of edits costs only the files
//! that changed. The newest [`AUTOMATIC_KEPT`] automatic checkpoints are
//! kept; ones someone saved and ones made before a restore stay until
//! removed by hand. Contents no checkpoint names any more are removed.
//!
//! [`restore_ops`] turns a checkpoint into one transaction that makes the
//! session's World and files what they were then: the host applies it like
//! any other, so everyone sees the restore as one step.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use axcenity_core::{InstanceId, Value, World};
use rustc_hash::{FxHashMap, FxHashSet};
use serde::{Deserialize, Serialize};

use crate::apply::{self, Doc, place_of};
use crate::files::{Bytes, FileHash, FileStore, Manifest};
use crate::journal;
use crate::message::{Snapshot, ToClient};
use crate::op::{NodeSpec, Op, remap_ids};
use crate::role::Role;
use crate::wire;

/// The folder, inside the journal's, that holds the checkpoints.
pub const FOLDER: &str = "history";
/// How often the host makes a checkpoint while the session changes.
pub const CHECKPOINT_EVERY: Duration = Duration::from_secs(60 * 60);
/// How many automatic checkpoints are kept (two days of hourly ones).
pub const AUTOMATIC_KEPT: usize = 48;
/// How many checkpoints members are told about, newest first.
pub const LISTED: usize = 100;
/// The longest label a member can give a checkpoint, in characters.
pub const LABEL_MOST: usize = 120;

const INFO_FILE: &str = "info";
const WORLD_FILE: &str = "world";
const FILES_FOLDER: &str = "files";

/// Why a checkpoint was made.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CheckpointKind {
    /// The session started.
    SessionStart,
    /// An hour of changes.
    Hourly,
    /// A member asked for it.
    Saved,
    /// Just before a restore, so the restore can be taken back.
    BeforeRestore,
}

impl CheckpointKind {
    /// Made by the host on its own (and removed when there are too many).
    pub fn automatic(self) -> bool {
        matches!(self, CheckpointKind::SessionStart | CheckpointKind::Hourly)
    }
}

/// One checkpoint, as members see it in the session's History.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Names it: milliseconds since 1970 when it was made (one more than
    /// the newest when two come in the same millisecond).
    pub id: u64,
    /// When, in seconds since 1970.
    pub at: u64,
    pub kind: CheckpointKind,
    /// What it's called: the member's label, or what the host made it for.
    pub label: String,
    /// Who asked for it, for one a member saved or a restore.
    pub by: Option<String>,
    /// Who changed the session since the checkpoint before it, as far as
    /// the host knows.
    pub edited_by: Vec<String>,
    /// The session's transaction number it's as of.
    pub seq: u64,
}

/// What `info` holds: the checkpoint, its files, and a digest of the
/// World and files to tell an unchanged project.
#[derive(Serialize, Deserialize)]
struct Info {
    checkpoint: Checkpoint,
    files: Manifest,
    digest: FileHash,
}

/// A project's checkpoints.
#[derive(Clone, Debug)]
pub struct History {
    folder: PathBuf,
}

impl History {
    /// The checkpoints of the project at `project` (none yet is fine).
    pub fn of(project: &Path) -> History {
        History {
            folder: project.join(journal::FOLDER).join(FOLDER),
        }
    }

    /// Every checkpoint, newest first. Ones that don't read are left out.
    pub fn list(&self) -> Vec<Checkpoint> {
        let mut found: Vec<Checkpoint> = self
            .ids()
            .into_iter()
            .filter_map(|id| self.info(id).ok())
            .map(|info| info.checkpoint)
            .collect();
        found.sort_by_key(|checkpoint| std::cmp::Reverse(checkpoint.id));
        found
    }

    /// Keeps `doc` (its World and files) as a checkpoint described by
    /// `checkpoint` (its `id` is filled in). Each file's contents are read
    /// from `store` the first time a checkpoint names them. With
    /// `skip_unchanged`, nothing is kept when the newest checkpoint has the
    /// same World and files (`Ok(None)`).
    pub fn record(
        &self,
        doc: &Doc,
        store: &dyn FileStore,
        mut checkpoint: Checkpoint,
        skip_unchanged: bool,
    ) -> io::Result<Option<Checkpoint>> {
        let world = encode_world(&doc.world);
        let mut digested = world.clone();
        digested.extend(postcard::to_stdvec(&doc.files).map_err(invalid)?);
        let digest = FileHash::of(&digested);
        let ids = self.ids();
        if skip_unchanged
            && let Some(&newest) = ids.last()
            && self.info(newest).is_ok_and(|info| info.digest == digest)
        {
            return Ok(None);
        }
        let files = self.folder.join(FILES_FOLDER);
        fs::create_dir_all(&files)?;
        for hash in doc.files.values() {
            let path = files.join(hash.hex());
            if path.exists() {
                continue;
            }
            // A file the store can't give (the owner changed it on disk
            // since) is named but not kept: a restore leaves it as it is.
            if let Some(bytes) = store.read(*hash) {
                write_whole(&files, &hash.hex(), &bytes)?;
            }
        }
        checkpoint.id = checkpoint.id.max(ids.last().map_or(0, |newest| newest + 1));
        let folder = self.folder.join(checkpoint.id.to_string());
        fs::create_dir_all(&folder)?;
        write_whole(&folder, WORLD_FILE, &world)?;
        let info = Info {
            checkpoint: checkpoint.clone(),
            files: doc.files.clone(),
            digest,
        };
        let mut bytes = vec![wire::FORMAT];
        bytes.extend(postcard::to_stdvec(&info).map_err(invalid)?);
        // `info` last: a folder without it isn't a checkpoint yet.
        write_whole(&folder, INFO_FILE, &bytes)?;
        self.prune()?;
        Ok(Some(checkpoint))
    }

    /// The World and files of checkpoint `id`.
    pub fn load(&self, id: u64) -> io::Result<(World, Manifest)> {
        let info = self.info(id)?;
        let bytes = fs::read(self.folder.join(id.to_string()).join(WORLD_FILE))?;
        match wire::decode_to_client(&bytes) {
            Ok(ToClient::Welcome { world, .. }) => Ok((*world.0, info.files)),
            Ok(_) => Err(invalid("the checkpoint isn't a World")),
            Err(error) => Err(invalid(error)),
        }
    }

    /// The contents of a file some checkpoint kept.
    pub fn read_file(&self, hash: FileHash) -> Option<Bytes> {
        let bytes = fs::read(self.folder.join(FILES_FOLDER).join(hash.hex())).ok()?;
        (FileHash::of(&bytes) == hash).then(|| Bytes::from(bytes))
    }

    /// Checkpoint numbers, oldest first.
    fn ids(&self) -> Vec<u64> {
        let mut ids: Vec<u64> = fs::read_dir(&self.folder)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| entry.file_name().to_str()?.parse().ok())
            .filter(|id: &u64| self.folder.join(id.to_string()).join(INFO_FILE).is_file())
            .collect();
        ids.sort_unstable();
        ids
    }

    fn info(&self, id: u64) -> io::Result<Info> {
        let bytes = fs::read(self.folder.join(id.to_string()).join(INFO_FILE))?;
        match bytes.split_first() {
            Some((&wire::FORMAT, rest)) => postcard::from_bytes(rest).map_err(invalid),
            _ => Err(invalid("a checkpoint from another version")),
        }
    }

    /// Removes the oldest automatic checkpoints past [`AUTOMATIC_KEPT`],
    /// then contents no checkpoint names.
    fn prune(&self) -> io::Result<()> {
        let infos: Vec<Info> = self
            .ids()
            .into_iter()
            .filter_map(|id| self.info(id).ok())
            .collect();
        let automatic: Vec<u64> = infos
            .iter()
            .filter(|info| info.checkpoint.kind.automatic())
            .map(|info| info.checkpoint.id)
            .collect();
        let dropped: BTreeSet<u64> = automatic
            .iter()
            .take(automatic.len().saturating_sub(AUTOMATIC_KEPT))
            .copied()
            .collect();
        for id in &dropped {
            fs::remove_dir_all(self.folder.join(id.to_string()))?;
        }
        let named: FxHashSet<String> = infos
            .iter()
            .filter(|info| !dropped.contains(&info.checkpoint.id))
            .flat_map(|info| info.files.values().map(FileHash::hex))
            .collect();
        for entry in fs::read_dir(self.folder.join(FILES_FOLDER))?.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !named.contains(&name) {
                fs::remove_file(entry.path())?;
            }
        }
        Ok(())
    }
}

/// What a restore changes, and what it couldn't.
#[derive(Debug, Default)]
pub struct Restore {
    /// One transaction's ops: applied in order to the session they make
    /// its World and files what the checkpoint has.
    pub ops: Vec<Op>,
    /// Files whose contents the checkpoint didn't keep, left as they are.
    pub missing: Vec<String>,
}

/// The ops that turn `now` into the checkpoint's `then` World and
/// `then_files`. Instances `then` has that `now` doesn't have at all are
/// made again at IDs from `fresh` (so they can't clash with IDs handed out
/// since), and everything pointing at them follows. `has_file` says which
/// file contents the host can give.
///
/// The ops are worked out against a copy of `now`, applying each in turn,
/// so moves land at the right places whatever came before them.
pub fn restore_ops(
    now: &Doc,
    then: &World,
    then_files: &Manifest,
    fresh: &mut dyn FnMut() -> Option<InstanceId>,
    has_file: &dyn Fn(FileHash) -> bool,
) -> Result<Restore, String> {
    if then.root() != now.world.root() || then.workspace() != now.world.workspace() {
        return Err("The checkpoint is of another project".to_owned());
    }
    let mut scratch = now.clone();
    let mut restore = Restore::default();
    let then_tree: Vec<InstanceId> = then.descendants(then.root());
    let then_set: FxHashSet<InstanceId> = then_tree.iter().copied().collect();
    // Instances to make again, at new IDs. One `now` has with another class
    // is another Instance that took its ID: it goes, and this one is new.
    let mut map: FxHashMap<InstanceId, InstanceId> = FxHashMap::default();
    for &id in &then_tree {
        let same = match (then.get(id), now.world.get(id)) {
            (Some(was), Some(is)) => was.class() == is.class(),
            _ => false,
        };
        if !same {
            let id_now = fresh().ok_or("The session has run out of Instance IDs")?;
            map.insert(id, id_now);
        }
    }
    let step = |scratch: &mut Doc, mut op: Op, ops: &mut Vec<Op>| -> Result<(), String> {
        remap_ids(std::slice::from_mut(&mut op), &map);
        apply::apply(&mut scratch.view(), std::slice::from_ref(&op), &|_| true)
            .map_err(|reason| format!("{reason}"))?;
        ops.push(op);
        Ok(())
    };

    // What `then` doesn't have goes first (the tops of it).
    let now_tree = now.world.descendants(now.world.root());
    let gone: FxHashSet<InstanceId> = now_tree
        .iter()
        .copied()
        .filter(|id| !then_set.contains(id) || map.contains_key(id))
        .collect();
    for &id in &now_tree {
        let parent_gone = now
            .world
            .get(id)
            .and_then(|instance| instance.parent())
            .is_some_and(|parent| gone.contains(&parent));
        if gone.contains(&id) && !parent_gone {
            let op = Op::Reparent {
                id,
                parent: None,
                index: None,
                restore: false,
            };
            // Not remapped: it names the Instance there now.
            apply::apply(&mut scratch.view(), std::slice::from_ref(&op), &|_| true)
                .map_err(|reason| format!("{reason}"))?;
            restore.ops.push(op);
        }
    }

    // Then `then`'s tree, parents before children, each where it was.
    // Properties naming other Instances are set once everything is made.
    let mut links = Vec::new();
    for &id in &then_tree {
        let Some(was) = then.get(id) else { continue };
        let Some(place) = place_of(then, id) else {
            continue;
        };
        if map.contains_key(&id) {
            let (properties, linked): (Vec<_>, Vec<_>) = was
                .properties()
                .map(|(name, value)| (name.to_owned(), value.clone()))
                .partition(|(_, value)| !matches!(value, Value::Instance(Some(_))));
            if !linked.is_empty() {
                links.push(Op::SetProps {
                    id,
                    properties: linked,
                });
            }
            let node = NodeSpec {
                id,
                class: was.class(),
                parent: place.parent,
                index: Some(place.index),
                name: was.name().to_owned(),
                properties,
                attributes: was
                    .attributes()
                    .iter()
                    .map(|(name, value)| (name.clone(), value.clone()))
                    .collect(),
                tags: was.tags().to_vec(),
            };
            step(
                &mut scratch,
                Op::CreateTree { nodes: vec![node] },
                &mut restore.ops,
            )?;
            continue;
        }
        let parent = map.get(&place.parent).copied().unwrap_or(place.parent);
        if place_of(&scratch.world, id)
            != Some(apply::Place {
                parent,
                index: place.index,
            })
        {
            step(
                &mut scratch,
                Op::Reparent {
                    id,
                    parent: Some(place.parent),
                    index: Some(place.index),
                    restore: true,
                },
                &mut restore.ops,
            )?;
        }
        for op in field_ops(&scratch.world, then, id) {
            let op = match op {
                Op::SetProps { id, properties } => {
                    let (plain, linked): (Vec<_>, Vec<_>) = properties
                        .into_iter()
                        .partition(|(_, value)| !matches!(value, Value::Instance(Some(_))));
                    if !linked.is_empty() {
                        links.push(Op::SetProps {
                            id,
                            properties: linked,
                        });
                    }
                    if plain.is_empty() {
                        continue;
                    }
                    Op::SetProps {
                        id,
                        properties: plain,
                    }
                }
                op => op,
            };
            step(&mut scratch, op, &mut restore.ops)?;
        }
    }
    for op in links {
        step(&mut scratch, op, &mut restore.ops)?;
    }

    // Files.
    let paths: BTreeSet<&String> = scratch.files.keys().chain(then_files.keys()).collect();
    let mut file_ops = Vec::new();
    for path in paths {
        let is = scratch.files.get(path).copied();
        let was = then_files.get(path).copied();
        if is == was {
            continue;
        }
        if let Some(hash) = was
            && !has_file(hash)
        {
            restore.missing.push(path.clone());
            continue;
        }
        file_ops.push(Op::FileVersion {
            path: path.clone(),
            base: is,
            new: was,
        });
    }
    for op in file_ops {
        step(&mut scratch, op, &mut restore.ops)?;
    }
    Ok(restore)
}

/// SetProps, SetAttribute and Tag ops that make `id` in `world` what it
/// was in `then` (Material first, as setting it resets Metalness and
/// Roughness).
fn field_ops(world: &World, then: &World, id: InstanceId) -> Vec<Op> {
    let (Some(is), Some(was)) = (world.get(id), then.get(id)) else {
        return Vec::new();
    };
    let mut ops = Vec::new();
    let now: BTreeMap<&str, &Value> = is.properties().collect();
    let mut properties: Vec<(String, Value)> = Vec::new();
    if is.name() != was.name() {
        properties.push(("Name".to_owned(), Value::String(was.name().to_owned())));
    }
    let material_changed = now.get("Material").copied()
        != was
            .properties()
            .find(|(k, _)| *k == "Material")
            .map(|(_, v)| v);
    for (name, value) in was.properties() {
        if now.get(name) != Some(&value)
            || (material_changed && matches!(name, "Metalness" | "Roughness"))
        {
            properties.push((name.to_owned(), value.clone()));
        }
    }
    properties.sort_by_key(|(name, _)| name != "Name" && name != "Material");
    if !properties.is_empty() {
        ops.push(Op::SetProps { id, properties });
    }
    for (name, value) in was.attributes() {
        if is.attribute(name) != Some(value) {
            ops.push(Op::SetAttribute {
                id,
                name: name.clone(),
                value: Some(value.clone()),
            });
        }
    }
    for name in is.attributes().keys() {
        if was.attribute(name).is_none() {
            ops.push(Op::SetAttribute {
                id,
                name: name.clone(),
                value: None,
            });
        }
    }
    for tag in was.tags() {
        if !is.tags().contains(tag) {
            ops.push(Op::Tag {
                id,
                tag: tag.clone(),
                added: true,
            });
        }
    }
    for tag in is.tags() {
        if !was.tags().contains(tag) {
            ops.push(Op::Tag {
                id,
                tag: tag.clone(),
                added: false,
            });
        }
    }
    ops
}

/// `world` as the journal writes a snapshot (a join message), so a
/// checkpoint reads back with the same IDs.
fn encode_world(world: &World) -> Vec<u8> {
    wire::encode_to_client(&ToClient::Welcome {
        world: Snapshot(Box::new(world.clone())),
        graveyard: Default::default(),
        seq: 0,
        last_client_seq: 0,
        role: Role::Owner,
        ids: None,
        rejected: Vec::new(),
        files: Default::default(),
        session: String::new(),
        epoch: 0,
    })
}

fn write_whole(folder: &Path, name: &str, bytes: &[u8]) -> io::Result<()> {
    let temporary = folder.join(format!("{name}.tmp"));
    fs::write(&temporary, bytes)?;
    fs::rename(&temporary, folder.join(name))
}

fn invalid(error: impl ToString) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error.to_string())
}
