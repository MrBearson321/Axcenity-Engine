//! Moving a slot file from an older save version to [`SAVE_VERSION`].
//!
//! Each step works on the file's JSON, so an older shape needs no Rust type
//! of its own, and moves it up one version. Loading runs the steps from the
//! file's version up; it never writes the file, so the slot keeps its old
//! version until the game saves over it.
//!
//! - 1 to 2: the active checkpoint was found by its names from the Workspace
//!   (`"path": ["Checkpoints", "Cave"]`), so two checkpoints with the same
//!   name were one. Version 2 names its entry in the slot's world state
//!   (`"instance": 5`).

use serde_json::{Value as Json, json};

use crate::SAVE_VERSION;

type Step = fn(&mut Json) -> Result<(), String>;

/// `STEPS[n]` moves a file from version `n + 1` to `n + 2`.
const STEPS: [Step; SAVE_VERSION as usize - 1] = [checkpoint_by_entry];

/// Moves `file`, a slot file of save version `version`, up to
/// [`SAVE_VERSION`], and returns how many steps that took (0 for a current
/// file). Newer files are refused by the caller, with their own error.
pub fn migrate(file: &mut Json, version: u32) -> Result<usize, String> {
    if version == 0 || version > SAVE_VERSION {
        return Err(format!("save version {version} isn't one this game knows"));
    }
    let mut steps = 0;
    for (from, step) in (version..SAVE_VERSION).zip(&STEPS[version as usize - 1..]) {
        step(file).map_err(|message| {
            format!("it can't be moved up from save version {from} ({message})")
        })?;
        file["version"] = json!(from + 1);
        steps += 1;
    }
    Ok(steps)
}

/// 1 to 2: the checkpoint's `path` becomes `instance`, its entry in the
/// slot's `world.instances`. Each name is the first saved Instance with that
/// name in the one before it, as loading a version 1 slot found it. A path
/// that doesn't lead to a saved Checkpoint drops the checkpoint, as loading
/// did then.
fn checkpoint_by_entry(file: &mut Json) -> Result<(), String> {
    let Some(checkpoint) = file.get("checkpoint").filter(|c| !c.is_null()) else {
        return Ok(());
    };
    let path = checkpoint
        .get("path")
        .and_then(Json::as_array)
        .ok_or("its checkpoint has no path")?;
    let instances = file
        .pointer("/world/instances")
        .and_then(Json::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let mut parent = json!("Workspace");
    let mut found = None;
    for name in path {
        found = instances.iter().position(|entry| {
            entry.get("parent") == Some(&parent) && entry.get("name") == Some(name)
        });
        let Some(index) = found else {
            break;
        };
        parent = json!(index);
    }
    let found = found.filter(|&index| instances[index].get("class") == Some(&json!("Checkpoint")));

    let checkpoint = file
        .get_mut("checkpoint")
        .and_then(Json::as_object_mut)
        .ok_or("its checkpoint isn't an object")?;
    match found {
        Some(index) => {
            checkpoint.remove("path");
            checkpoint.insert("instance".to_owned(), json!(index));
        }
        None => file["checkpoint"] = Json::Null,
    }
    Ok(())
}
