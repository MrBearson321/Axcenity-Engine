//! Game saves: numbered slots and checkpoints.
//!
//! - [`Saves`] is what a running game keeps for saving: player data,
//!   settings and the active checkpoint, with what was captured there.
//! - A slot is one file, `slot-<n>.json`, in the player's data for the
//!   game ([`game_folder`]), never in the project folder. It holds player
//!   data, world state (`state`), the active checkpoint and settings (the
//!   world's game UI settings among them, as `UI/…`), and its save version
//!   ([`SAVE_VERSION`]).
//! - Loading reads and checks the whole file first, so a missing or damaged
//!   slot fails without changing anything. A slot from an older save version
//!   is moved up to this one as it's read ([`migrate`]); one from a newer
//!   version is refused, and never saved over.

mod identity;
mod migrate;
mod state;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use axcenity_core::{
    ClassName, InstanceId, MASTER_VOLUME_SETTING, UI_SETTING_PREFIX, Value, Vector3, World,
};
use serde::{Deserialize, Serialize};

pub use identity::{user_id, user_id_in};
pub use migrate::migrate;
pub use state::{
    Properties, SavedInstance, SavedParent, SavedValue, WorldState, restore, snapshot,
};

/// The version of the save files this engine writes. An older file is moved
/// up to it when it's read (see [`migrate`]).
pub const SAVE_VERSION: u32 = 2;

/// The highest slot number.
pub const MAX_SLOT: u32 = 999;

#[derive(Debug, thiserror::Error)]
pub enum SaveError {
    #[error("saving isn't set up in this game (it has no save folder)")]
    NoFolder,
    #[error("save slots are numbered 1 to {MAX_SLOT}, so {0} isn't one")]
    BadSlot(String),
    #[error(
        "save slot {0} doesn't exist yet; save to it first, or check with SaveService:HasSlot({0})"
    )]
    MissingSlot(u32),
    #[error("save slot {slot} can't be read ({message}), so nothing was loaded")]
    Damaged { slot: u32, message: String },
    #[error(
        "save slot {slot} was saved by a newer version of the game (save version {version}, this game reads up to {SAVE_VERSION}), so nothing was loaded"
    )]
    Newer { slot: u32, version: u32 },
    #[error(
        "save slot {slot} holds a save from a newer version of the game (save version {version}, this game reads up to {SAVE_VERSION}), so this game won't write over it"
    )]
    WontReplaceNewer { slot: u32, version: u32 },
    #[error("can't write save slot {slot}: {message}")]
    Write { slot: u32, message: String },
    #[error("no checkpoint is active; set one with SaveService:SetCheckpoint(checkpoint) first")]
    NoCheckpoint,
    #[error("the active checkpoint has been destroyed")]
    CheckpointGone,
    #[error("{0} is not a Checkpoint")]
    NotACheckpoint(String),
}

/// What a checkpoint holds: the player data and world state when it was set.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Capture {
    #[serde(default)]
    pub player_data: Properties,
    #[serde(default)]
    pub world: WorldState,
}

/// The active checkpoint in a slot file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedCheckpoint {
    /// Its entry in the slot's [`WorldState::instances`], so it is found
    /// again even beside checkpoints with the same name.
    pub instance: usize,
    #[serde(flatten)]
    pub capture: Capture,
}

/// The levels a game was in when a slot was saved (Phase 48): the
/// current level, and the ones added beside it, in the order they were
/// added.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SavedLevels {
    pub current: String,
    #[serde(default)]
    pub added: Vec<String>,
}

/// One slot file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SlotFile {
    pub version: u32,
    #[serde(default)]
    pub player_data: Properties,
    #[serde(default)]
    pub settings: Properties,
    #[serde(default)]
    pub world: WorldState,
    #[serde(default)]
    pub checkpoint: Option<SavedCheckpoint>,
    /// Left out by games without levels and by older slots, which load
    /// unchanged, so the save version stays the same.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub levels: Option<SavedLevels>,
}

/// What a running game keeps for saving. Hosts make one per game run, with
/// the folder slots go in.
#[derive(Debug, Default)]
pub struct Saves {
    folder: Option<PathBuf>,
    /// Values scripts keep for the player, such as coins collected, by name.
    pub player_data: BTreeMap<String, Value>,
    /// The game's settings, such as volume, by name. The game UI settings
    /// aren't here: they're the world's (`World::ui_settings`), and a slot
    /// keeps them as settings named `UI/…`.
    pub settings: BTreeMap<String, Value>,
    checkpoint: Option<(InstanceId, Capture)>,
    /// The levels the game is in, which slots record. Hosts with levels
    /// keep it up to date; `None` for a game without them.
    pub levels: Option<SavedLevels>,
}

impl Saves {
    /// Saves that write slots in `folder`. `None` can't save or load.
    pub fn new(folder: Option<PathBuf>) -> Self {
        Self {
            folder,
            ..Self::default()
        }
    }

    /// Where slots are written, if anywhere.
    pub fn folder(&self) -> Option<&Path> {
        self.folder.as_deref()
    }

    pub fn set_folder(&mut self, folder: Option<PathBuf>) {
        self.folder = folder;
    }

    /// The file of slot `slot`.
    pub fn slot_path(&self, slot: u32) -> Result<PathBuf, SaveError> {
        check_slot(slot)?;
        let folder = self.folder.as_ref().ok_or(SaveError::NoFolder)?;
        Ok(folder.join(format!("slot-{slot}.json")))
    }

    /// True if slot `slot` has been saved.
    pub fn has_slot(&self, slot: u32) -> bool {
        self.slot_path(slot).is_ok_and(|path| path.is_file())
    }

    /// Makes `checkpoint` the active one, and captures the player data and
    /// world state now, for [`restore_checkpoint`](Self::restore_checkpoint).
    pub fn set_checkpoint(
        &mut self,
        world: &World,
        checkpoint: InstanceId,
    ) -> Result<(), SaveError> {
        let instance = world.get(checkpoint).ok_or(SaveError::CheckpointGone)?;
        if instance.class() != ClassName::Checkpoint {
            return Err(SaveError::NotACheckpoint(instance.class().to_string()));
        }
        let capture = Capture {
            player_data: to_properties(&self.player_data),
            world: snapshot(world),
        };
        self.checkpoint = Some((checkpoint, capture));
        Ok(())
    }

    /// The active checkpoint, while it's still in the world.
    pub fn checkpoint(&self, world: &World) -> Option<InstanceId> {
        self.checkpoint
            .as_ref()
            .map(|(id, _)| *id)
            .filter(|&id| world.contains(id))
    }

    /// Puts back the player data and world state captured at the active
    /// checkpoint, and returns where the character goes: the checkpoint's
    /// `Position` (see [`place_character`]).
    pub fn restore_checkpoint(&mut self, world: &mut World) -> Result<Vector3, SaveError> {
        let (id, capture) = self.checkpoint.as_ref().ok_or(SaveError::NoCheckpoint)?;
        if !world.contains(*id) {
            return Err(SaveError::CheckpointGone);
        }
        let (id, capture) = (*id, capture.clone());
        restore(world, &capture.world);
        self.player_data = from_properties(&capture.player_data);
        let position = world
            .get(id)
            .and_then(|checkpoint| checkpoint.property("Position"))
            .and_then(Value::as_vector3)
            .ok_or(SaveError::CheckpointGone)?;
        Ok(position)
    }

    /// Writes slot `slot`: player data, world state, the active checkpoint
    /// and settings, with the world's game UI settings. Returns the file it
    /// wrote. A slot saved by a newer
    /// version of the game is left as it is.
    pub fn save(&self, world: &World, slot: u32) -> Result<PathBuf, SaveError> {
        let path = self.slot_path(slot)?;
        if let Some(version) = fs::read_to_string(&path)
            .ok()
            .and_then(|text| file_version(&text))
            .filter(|&version| version > u64::from(SAVE_VERSION))
        {
            return Err(SaveError::WontReplaceNewer {
                slot,
                version: u32::try_from(version).unwrap_or(u32::MAX),
            });
        }
        let (state, ids) = state::snapshot_with_ids(world);
        let checkpoint = self.checkpoint.as_ref().and_then(|(id, capture)| {
            Some(SavedCheckpoint {
                instance: ids.iter().position(|saved| saved == id)?,
                capture: capture.clone(),
            })
        });
        let mut settings = to_properties(&self.settings);
        for (name, value) in world.ui_settings().to_settings() {
            settings.insert(name, SavedValue(value));
        }
        settings.insert(
            MASTER_VOLUME_SETTING.to_owned(),
            SavedValue(Value::Number(world.master_volume())),
        );
        let file = SlotFile {
            version: SAVE_VERSION,
            player_data: to_properties(&self.player_data),
            settings,
            world: state,
            checkpoint,
            levels: self.levels.clone(),
        };
        let text = serde_json::to_string_pretty(&file).expect("a slot file always serializes");
        let write_error = |err: std::io::Error| SaveError::Write {
            slot,
            message: err.to_string(),
        };
        let folder = path.parent().expect("a slot file is in a folder");
        fs::create_dir_all(folder).map_err(write_error)?;
        // Written beside it and renamed over it, so a crash halfway through
        // leaves the previous save as it was.
        let temporary = path.with_extension("json.tmp");
        fs::write(&temporary, text).map_err(write_error)?;
        fs::rename(&temporary, &path).map_err(write_error)?;
        Ok(path)
    }

    /// Reads slot `slot` and puts the game back as it was saved: player
    /// data, world state, the active checkpoint and settings, with the game
    /// UI settings going back to the world (one the slot doesn't have stays
    /// as it is). A missing, damaged or newer slot changes nothing.
    pub fn load(&mut self, world: &mut World, slot: u32) -> Result<(), SaveError> {
        let file = self.read(slot)?;
        self.apply(world, file);
        Ok(())
    }

    /// Puts the game back as `file` (read with [`read`](Self::read)) was
    /// saved, as [`load`](Self::load) does. Its levels are only noted in
    /// [`levels`](Self::levels): a host with levels loads them into `world`
    /// before this.
    pub fn apply(&mut self, world: &mut World, file: SlotFile) {
        let restored = restore(world, &file.world);
        self.player_data = from_properties(&file.player_data);
        let mut settings = from_properties(&file.settings);
        let ui = world
            .ui_settings()
            .with_settings(settings.iter().map(|(name, value)| (name.as_str(), value)));
        world.set_ui_settings(ui);
        if let Some(volume) = settings
            .remove(MASTER_VOLUME_SETTING)
            .and_then(|value| value.as_number())
        {
            world.set_master_volume(volume);
        }
        settings.retain(|name, _| !name.starts_with(UI_SETTING_PREFIX));
        self.settings = settings;
        self.checkpoint = file.checkpoint.and_then(|saved| {
            let id = (*restored.get(saved.instance)?)?;
            (world.get(id)?.class() == ClassName::Checkpoint).then_some((id, saved.capture))
        });
        if file.levels.is_some() {
            self.levels = file.levels;
        }
    }

    /// Reads and checks slot `slot`, moving an older save version up to
    /// this one. Changes nothing, the file included.
    pub fn read(&self, slot: u32) -> Result<SlotFile, SaveError> {
        let path = self.slot_path(slot)?;
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Err(SaveError::MissingSlot(slot));
            }
            Err(err) => {
                return Err(SaveError::Damaged {
                    slot,
                    message: err.to_string(),
                });
            }
        };
        let damaged = |message: String| SaveError::Damaged { slot, message };
        // The version first, so a newer file is refused for that reason
        // even if the rest has changed shape.
        let mut json: serde_json::Value =
            serde_json::from_str(&text).map_err(|err| damaged(err.to_string()))?;
        let version = json
            .get("version")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| damaged("it has no save version".to_owned()))?;
        if version > u64::from(SAVE_VERSION) {
            return Err(SaveError::Newer {
                slot,
                version: u32::try_from(version).unwrap_or(u32::MAX),
            });
        }
        migrate(&mut json, version as u32).map_err(damaged)?;
        serde_json::from_value(json).map_err(|err| damaged(err.to_string()))
    }
}

/// The save version of a slot file's text, if it has one.
fn file_version(text: &str) -> Option<u64> {
    serde_json::from_str::<serde_json::Value>(text)
        .ok()?
        .get("version")?
        .as_u64()
}

/// Checks a slot number.
fn check_slot(slot: u32) -> Result<(), SaveError> {
    if (1..=MAX_SLOT).contains(&slot) {
        Ok(())
    } else {
        Err(SaveError::BadSlot(slot.to_string()))
    }
}

/// A slot number from a script's number: a whole number from 1 to [`MAX_SLOT`].
pub fn slot_number(number: f64) -> Result<u32, SaveError> {
    if number.fract() == 0.0 && (1.0..=f64::from(MAX_SLOT)).contains(&number) {
        Ok(number as u32)
    } else {
        Err(SaveError::BadSlot(number.to_string()))
    }
}

fn to_properties(values: &BTreeMap<String, Value>) -> Properties {
    values
        .iter()
        .map(|(name, value)| (name.clone(), SavedValue(value.clone())))
        .collect()
}

fn from_properties(properties: &Properties) -> BTreeMap<String, Value> {
    properties
        .iter()
        .map(|(name, value)| (name.clone(), value.0.clone()))
        .collect()
}

/// Moves the character made of `parts` so that the middle of its feet is at
/// `position`: the bottom of the box around the parts, centred across it.
/// Only `Position` changes, so it keeps facing the way it faced.
pub fn place_character(world: &mut World, parts: &[InstanceId], position: Vector3) {
    let mut min = Vector3::new(f32::INFINITY, f32::INFINITY, f32::INFINITY);
    let mut max = Vector3::new(f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);
    let mut placed = Vec::new();
    for &part in parts {
        let Some(instance) = world.get(part) else {
            continue;
        };
        let (Some(at), Some(size)) = (
            instance.property("Position").and_then(Value::as_vector3),
            instance.property("Size").and_then(Value::as_vector3),
        ) else {
            continue;
        };
        let half = Vector3::new(size.x.abs() / 2.0, size.y.abs() / 2.0, size.z.abs() / 2.0);
        min = Vector3::new(
            min.x.min(at.x - half.x),
            min.y.min(at.y - half.y),
            min.z.min(at.z - half.z),
        );
        max = Vector3::new(
            max.x.max(at.x + half.x),
            max.y.max(at.y + half.y),
            max.z.max(at.z + half.z),
        );
        placed.push((part, at));
    }
    if placed.is_empty() {
        return;
    }
    let shift = Vector3::new(
        position.x - (min.x + max.x) / 2.0,
        position.y - min.y,
        position.z - (min.z + max.z) / 2.0,
    );
    for (part, at) in placed {
        let moved = Vector3::new(at.x + shift.x, at.y + shift.y, at.z + shift.z);
        let _ = world.set_property(part, "Position", Value::Vector3(moved));
    }
}

/// The folder a game's save slots go in: the player's data for the game,
/// by its name. `AXCENITY_SAVE_DIR`, when set, is used in place of the
/// player's data folder (for tests and portable installs).
///
/// - Windows: `%APPDATA%\Axcenity\Saves\<game>`
/// - macOS: `~/Library/Application Support/Axcenity/Saves/<game>`
/// - Linux and others: `$XDG_DATA_HOME/axcenity/saves/<game>`, or
///   `~/.local/share/axcenity/saves/<game>`
///
/// `None` when none of those can be found.
pub fn game_folder(game: &str) -> Option<PathBuf> {
    Some(saves_folder()?.join(folder_name(game)))
}

/// The folder every game's save folder is in (see [`game_folder`]). The
/// engine also keeps this install's player identity there (Phase 64).
pub fn saves_folder() -> Option<PathBuf> {
    let env = |name| {
        std::env::var_os(name)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    };
    let base = if let Some(folder) = env("AXCENITY_SAVE_DIR") {
        folder
    } else if cfg!(windows) {
        env("APPDATA")?.join("Axcenity").join("Saves")
    } else if cfg!(target_os = "macos") {
        env("HOME")?
            .join("Library/Application Support")
            .join("Axcenity")
            .join("Saves")
    } else {
        env("XDG_DATA_HOME")
            .or_else(|| Some(env("HOME")?.join(".local/share")))?
            .join("axcenity")
            .join("saves")
    };
    Some(base)
}

/// `game` as a folder name: letters, digits, spaces, `-`, `_` and `.` stay,
/// anything else becomes `_`.
fn folder_name(game: &str) -> String {
    let name: String = game
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let name = name.trim().trim_matches('.');
    if name.is_empty() {
        "Untitled".to_owned()
    } else {
        name.to_owned()
    }
}

#[cfg(test)]
mod tests;
