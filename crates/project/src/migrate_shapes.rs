//! The format 5 → 6 step of a project upgrade (unified Phase 92): the file
//! shapes from before format 3 that the readers no longer take.
//!
//! - A clip (`*.anim.json`) or state machine (`*.statemachine.json`) still
//!   in animation format 1 is rewritten in the one shape. The format 3 step
//!   already did this for the files a project had then; this catches any
//!   copied in since. `axcenity_anim` now refuses format 1.
//! - A block script (`*.blocks.json`) whose condition slot is the first
//!   saved `{ left, operator, right }` comparison gets a compare block
//!   instead.
//!
//! The format 3 step (`migrate_animation.rs`) reads format 1 files through
//! [`read_clip`] and [`read_state_machine`] too.

use std::fs;
use std::path::Path;

use axcenity_anim::{ClipEvent, ClipFile, PropertyTrack, StateMachineFile, TakeLink};
use serde::Deserialize;
use serde_json::Value as Json;

const ANIM_FORMAT: u32 = axcenity_anim::files::FORMAT_VERSION;
/// Folders at the project root that aren't part of the game.
const SKIP_ROOT_DIRS: &[&str] = &[".axcenity", ".git", "target", "node_modules"];

/// Runs the step on the project in `folder`.
pub(crate) fn migrate(folder: &Path) -> Result<(), String> {
    let mut files = Vec::new();
    find(folder, folder, &mut files)?;
    for path in files {
        let text = fs::read_to_string(folder.join(&path))
            .map_err(|err| format!("could not read `{path}`: {err}"))?;
        let upgraded = if path.ends_with(axcenity_anim::CLIP_EXTENSION) {
            old_anim_format(&text)
                .then(|| read_clip(&text).map(|clip| clip.to_json()))
                .transpose()
        } else if path.ends_with(axcenity_anim::STATE_MACHINE_EXTENSION) {
            old_anim_format(&text)
                .then(|| read_state_machine(&text).map(|machine| machine.to_json()))
                .transpose()
        } else {
            upgrade_block_script(&text)
        }
        .map_err(|err| format!("could not upgrade `{path}`: {err}"))?;
        if let Some(upgraded) = upgraded {
            crate::migrate_animation::write(&folder.join(&path), &upgraded)?;
        }
    }
    Ok(())
}

/// Every clip, state machine and block script under `dir`, as paths from
/// `root` with `/`.
fn find(root: &Path, dir: &Path, files: &mut Vec<String>) -> Result<(), String> {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .map_err(|err| format!("could not read `{}`: {err}", dir.display()))?
        .flatten()
        .collect();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            if dir == root && SKIP_ROOT_DIRS.contains(&name.as_str()) {
                continue;
            }
            find(root, &entry.path(), files)?;
        } else if file_type.is_file()
            && [
                axcenity_anim::CLIP_EXTENSION,
                axcenity_anim::STATE_MACHINE_EXTENSION,
                ".blocks.json",
            ]
            .iter()
            .any(|ending| name.ends_with(ending))
            && let Ok(relative) = entry.path().strip_prefix(root)
        {
            files.push(relative.to_string_lossy().replace('\\', "/"));
        }
    }
    Ok(())
}

/// Whether an animation file says it's older than the current format.
/// A file that isn't JSON is left for the reader to report.
fn old_anim_format(text: &str) -> bool {
    serde_json::from_str::<Json>(text)
        .ok()
        .and_then(|value| value.get("format_version")?.as_u64())
        .is_some_and(|version| version < u64::from(ANIM_FORMAT))
}

/// Reads a clip in any format this engine has written: format 1 as a take
/// clip (`source`, `take`, `start`, `end`, `speed`) or a property clip
/// (`"kind": "property"`), turned into the one shape. A take clip's speed
/// becomes its duration, and its events (seconds of the take from its
/// start) become seconds of the clip.
pub(crate) fn read_clip(text: &str) -> Result<ClipFile, String> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct TakeClip {
        #[serde(rename = "format_version")]
        _format_version: u32,
        source: String,
        take: String,
        start: f64,
        end: f64,
        #[serde(default = "one")]
        speed: f64,
        #[serde(rename = "loop", default = "yes")]
        looping: bool,
        #[serde(default)]
        root_motion: bool,
        #[serde(default)]
        events: Vec<ClipEvent>,
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct PropertyClip {
        #[serde(rename = "format_version")]
        _format_version: u32,
        kind: String,
        duration: f64,
        #[serde(rename = "loop", default = "yes")]
        looping: bool,
        #[serde(default)]
        tracks: Vec<PropertyTrack>,
        #[serde(default)]
        events: Vec<ClipEvent>,
    }
    fn one() -> f64 {
        1.0
    }
    fn yes() -> bool {
        true
    }

    if !old_anim_format(text) {
        return ClipFile::from_json(text).map_err(|err| err.0);
    }
    let value: Json = serde_json::from_str(text).map_err(|err| err.to_string())?;
    if value.get("kind").is_some() {
        let clip: PropertyClip = serde_json::from_value(value)
            .map_err(|err| format!("it isn't a valid property clip: {err}"))?;
        if clip.kind != "property" {
            return Err(format!(
                "unknown clip kind `{}` (expected `property` or a take clip)",
                clip.kind
            ));
        }
        return Ok(ClipFile {
            format_version: ANIM_FORMAT,
            duration: clip.duration,
            looping: clip.looping,
            link: None,
            tracks: clip.tracks,
            events: clip.events,
        });
    }
    let clip: TakeClip = serde_json::from_value(value)
        .map_err(|err| format!("it isn't a valid take clip: {err}"))?;
    if !(clip.start.is_finite() && clip.end.is_finite() && clip.speed.is_finite()) {
        return Err("its start, end and speed must be numbers".into());
    }
    let length = (clip.end - clip.start).max(0.0);
    let speed = clip.speed.abs().max(1e-3);
    let backwards = clip.speed < 0.0;
    let (start, end) = if backwards {
        (clip.end, clip.start)
    } else {
        (clip.start, clip.end)
    };
    let events = clip
        .events
        .into_iter()
        .map(|event| {
            let into = if backwards {
                length - event.time
            } else {
                event.time
            };
            ClipEvent {
                time: into.clamp(0.0, length) / speed,
                name: event.name,
            }
        })
        .collect();
    Ok(ClipFile {
        format_version: ANIM_FORMAT,
        duration: length / speed,
        looping: clip.looping,
        link: Some(TakeLink {
            source: clip.source,
            take: clip.take,
            start,
            end,
            root_motion: clip.root_motion,
        }),
        tracks: Vec::new(),
        events,
    })
}

/// Reads a state machine (or a format 1 animator controller, which had the
/// same shape) in the current format.
pub(crate) fn read_state_machine(text: &str) -> Result<StateMachineFile, String> {
    if !old_anim_format(text) {
        return StateMachineFile::from_json(text).map_err(|err| err.0);
    }
    let mut value: Json = serde_json::from_str(text).map_err(|err| err.to_string())?;
    value["format_version"] = Json::from(ANIM_FORMAT);
    serde_json::from_value(value).map_err(|err| format!("it isn't a valid state machine: {err}"))
}

/// The block script `text` with its first-format condition slots made
/// compare blocks, or `None` when it has none (or isn't JSON, which the
/// loader reports).
fn upgrade_block_script(text: &str) -> Result<Option<String>, String> {
    let Ok(mut script) = serde_json::from_str::<Json>(text) else {
        return Ok(None);
    };
    if !upgrade_slots(&mut script) {
        return Ok(None);
    }
    // Written the way the editor saves block scripts.
    let script: axcenity_blocks_runtime::BlockScript = serde_json::from_value(script)
        .map_err(|err| format!("it isn't a valid block script: {err}"))?;
    let mut text = serde_json::to_string_pretty(&script).map_err(|err| err.to_string())?;
    text.push('\n');
    Ok(Some(text))
}

/// Turns every `{ left, operator, right }` object (no `type`) under `json`
/// into `{ "type": "compare", op, left, right }`. True if it changed any.
fn upgrade_slots(json: &mut Json) -> bool {
    let mut changed = false;
    match json {
        Json::Array(items) => {
            for item in items {
                changed |= upgrade_slots(item);
            }
        }
        Json::Object(object) => {
            for value in object.values_mut() {
                changed |= upgrade_slots(value);
            }
            let first_format = object.len() == 3
                && ["left", "operator", "right"]
                    .iter()
                    .all(|key| object.contains_key(*key));
            if first_format {
                let operator = object.remove("operator").expect("checked above");
                object.insert("type".into(), Json::from("compare"));
                object.insert("op".into(), operator);
                changed = true;
            }
        }
        _ => {}
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_1_clips_and_controllers_read_as_the_one_shape() {
        let take = read_clip(
            r#"{ "format_version": 1, "source": "a.fbx", "take": "T", "start": 1, "end": 3,
                 "speed": 2, "loop": false, "root_motion": true,
                 "events": [ { "time": 1.0, "name": "Step" } ] }"#,
        )
        .unwrap();
        let link = take.link.as_ref().unwrap();
        assert_eq!((link.source.as_str(), link.take.as_str()), ("a.fbx", "T"));
        assert_eq!((link.start, link.end, link.root_motion), (1.0, 3.0, true));
        // Twice as fast: 2 seconds of the take in 1 of the clip.
        assert_eq!(take.duration, 1.0);
        assert_eq!(take.speed(), 2.0);
        assert!(!take.looping);
        assert_eq!(take.events[0].time, 0.5);
        // Backwards: the link runs from the end to the start.
        let back = read_clip(
            r#"{ "format_version": 1, "source": "a.fbx", "take": "T", "start": 0, "end": 2,
                 "speed": -1, "events": [ { "time": 0.5, "name": "Step" } ] }"#,
        )
        .unwrap();
        let link = back.link.as_ref().unwrap();
        assert_eq!((link.start, link.end), (2.0, 0.0));
        assert_eq!(back.speed(), -1.0);
        assert_eq!(back.events[0].time, 1.5);
        let property = read_clip(
            r#"{ "format_version": 1, "kind": "property", "duration": 1.5, "loop": false,
                 "tracks": [ { "id": "p", "target": ".", "property": "Position",
                   "keys": [ { "id": "k", "time": 0, "value": [0, 2, 0] } ] } ] }"#,
        )
        .unwrap();
        assert!(property.link.is_none());
        assert_eq!(property.tracks.len(), 1);
        assert_eq!(property.format_version, ANIM_FORMAT);
        // What it reads, the one reader reads back the same.
        assert_eq!(ClipFile::from_json(&property.to_json()).unwrap(), property);
        // Left-out fields take their defaults.
        let plain = read_clip(
            r#"{ "format_version": 1, "source": "a.fbx", "take": "T", "start": 0, "end": 2 }"#,
        )
        .unwrap();
        assert_eq!(plain.speed(), 1.0);
        assert!(plain.looping);
        let controller = read_state_machine(r#"{ "format_version": 1 }"#).unwrap();
        assert!(controller.states.is_empty());
        assert_eq!(controller.format_version, ANIM_FORMAT);
    }

    #[test]
    fn the_step_rewrites_old_clips_and_first_format_block_conditions_only() {
        let folder = std::env::temp_dir().join("axcenity-migrate-shapes");
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(folder.join("assets/animations/Door")).unwrap();
        fs::create_dir_all(folder.join("scripts")).unwrap();
        let old_clip = r#"{ "format_version": 1, "kind": "property", "duration": 1.5 }"#;
        fs::write(
            folder.join("assets/animations/Door/Open.anim.json"),
            old_clip,
        )
        .unwrap();
        let current = ClipFile::new(2.0).to_json();
        fs::write(
            folder.join("assets/animations/Door/Close.anim.json"),
            &current,
        )
        .unwrap();
        let old_blocks = r#"{ "blocks": [ { "id": 1, "kind": { "type": "when_started", "body": [
            { "id": 2, "kind": { "type": "if_else",
              "condition": { "left": { "type": "number", "value": 1 }, "operator": "less",
                             "right": { "type": "variable", "name": "x" } },
              "then_body": [], "else_body": [] } } ] } } ] }"#;
        fs::write(folder.join("scripts/Check.server.blocks.json"), old_blocks).unwrap();
        let new_blocks = r#"{ "blocks": [] }"#;
        fs::write(folder.join("scripts/Empty.server.blocks.json"), new_blocks).unwrap();

        migrate(&folder).unwrap();

        let open =
            fs::read_to_string(folder.join("assets/animations/Door/Open.anim.json")).unwrap();
        let open = ClipFile::from_json(&open).unwrap();
        assert_eq!((open.duration, open.format_version), (1.5, ANIM_FORMAT));
        // Files already current are left byte for byte.
        let close =
            fs::read_to_string(folder.join("assets/animations/Door/Close.anim.json")).unwrap();
        assert_eq!(close, current);
        let empty = fs::read_to_string(folder.join("scripts/Empty.server.blocks.json")).unwrap();
        assert_eq!(empty, new_blocks);
        let check = fs::read_to_string(folder.join("scripts/Check.server.blocks.json")).unwrap();
        let script: axcenity_blocks_runtime::BlockScript = serde_json::from_str(&check).unwrap();
        let lua = axcenity_blocks_runtime::translate(&script).unwrap().source;
        assert!(lua.contains("if 1 < x then"), "{lua}");
        let _ = fs::remove_dir_all(&folder);
    }

    /// Copies `from` into `to`, leaving out files over 256 KB (models,
    /// pictures, sounds): the upgrade never reads them.
    fn copy_small_files(from: &Path, to: &Path) {
        fs::create_dir_all(to).unwrap();
        for entry in fs::read_dir(from).unwrap().flatten() {
            let path = entry.path();
            let target = to.join(entry.file_name());
            if path.is_dir() {
                copy_small_files(&path, &target);
            } else if entry.metadata().unwrap().len() <= 256 * 1024 {
                fs::copy(&path, &target).unwrap();
            }
        }
    }

    /// The world as saved and every script's path and source.
    fn loaded(folder: &Path) -> (Option<u32>, String, Vec<(String, String)>) {
        let project = crate::load(folder).unwrap();
        let mut scripts: Vec<(String, String)> = project
            .scripts
            .iter()
            .map(|script| (script.path.clone(), script.source.clone()))
            .collect();
        scripts.sort();
        (
            project.migrated_from,
            crate::write_world(&project.world),
            scripts,
        )
    }

    #[test]
    fn every_example_saved_by_0_2_0_alpha_1_upgrades_once_and_opens_the_same() {
        // 0.2.0-alpha.1 wrote format 5 (`releases/history`); the 0.1.0
        // alphas wrote format 2, which `migrate_animation`'s tests take
        // from there to now.
        let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
        let mut names: Vec<_> = fs::read_dir(&examples)
            .unwrap()
            .flatten()
            .filter(|entry| entry.path().join(crate::MANIFEST_FILE).is_file())
            .map(|entry| entry.file_name())
            .collect();
        names.sort();
        assert!(names.len() > 50, "{names:?}");
        let root = std::env::temp_dir().join("axcenity-examples-at-format-5");
        let _ = fs::remove_dir_all(&root);
        for name in names {
            let folder = root.join(&name);
            copy_small_files(&examples.join(&name), &folder);
            let (now, world, scripts) = loaded(&folder);
            assert_eq!(now, None, "{name:?} is at the current format");

            let manifest = folder.join(crate::MANIFEST_FILE);
            let text = fs::read_to_string(&manifest).unwrap();
            let current = format!("\"format_version\": {}", crate::FORMAT_VERSION);
            assert!(text.contains(&current), "{name:?}: {text}");
            fs::write(&manifest, text.replace(&current, "\"format_version\": 5")).unwrap();

            let upgraded = loaded(&folder);
            assert_eq!(upgraded.0, Some(5), "{name:?}");
            assert_eq!(upgraded.1, world, "{name:?}'s world");
            assert_eq!(upgraded.2, scripts, "{name:?}'s scripts");
            assert!(folder.join(crate::PREVIOUS_FORMAT_DIR).join("5").is_dir());
            assert_eq!(loaded(&folder).0, None, "{name:?} upgrades once");
            fs::remove_dir_all(&folder).unwrap();
        }
        let _ = fs::remove_dir_all(&root);
    }
}
