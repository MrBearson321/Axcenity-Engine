//! `anim_edit`: keyed animation, as the Animation tab makes it.
//! `animate` is the editor's Animate (an Animator, a keyed clip file and an
//! Animation playing it, one undo step); `key` and `unkey` add, change or
//! take away one key in a clip's file; `info` shows a clip's tracks and
//! keys. Each change rewrites the `.anim.json` file at once, as one undo
//! step that puts the file back (`History::record_asset`), like
//! `material_edit`.

use axcenity_anim::{
    Applied, ClipFile, Interp, PropertyKeyframe, PropertyName, PropertyTrack, current_numbers,
    resolve_target,
};
use axcenity_core::{ClassName, InstanceId, Value, World};
use axcenity_mcp::{Args, Class, Code, Kind, Outcome, Param, Profile, ToolError, ToolSpec};

use super::material_edit::check_path;
use super::ops::checkpoint;
use super::refs::{id_text, label, quoted, resolve};
use super::{Host, ID};
use crate::animation::{animator_of, can_animate};
use crate::history::TextChange;

pub const SPEC: ToolSpec = ToolSpec {
    name: "anim_edit",
    title: "Edit keyed animation",
    summary: "Animate an object (Animator, clip, Animation), add or remove a key in its clip, or list a clip's keys.",
    profile: Profile::Standard,
    class: Class::Modify,
    idempotent: false,
    params: &[
        Param::required(
            "action",
            Kind::Enum(&["animate", "key", "unkey", "info"]),
            "",
        ),
        ID,
        Param::optional("clip", Kind::String, ".anim.json path; default id's"),
        Param::optional(
            "target",
            Kind::String,
            "path under the animated object; default .",
        ),
        Param::optional("property", Kind::Enum(PROPERTIES), ""),
        Param::optional("time", Kind::Number, "seconds"),
        Param::optional(
            "value",
            Kind::Vector3,
            "3 numbers; default target's value now",
        ),
        Param::optional("number", Kind::Number, "a 1-number property's value"),
        Param::optional(
            "interp",
            Kind::Enum(&["linear", "step", "ease"]),
            "to the next key",
        ),
        Param::optional("duration", Kind::Number, "seconds"),
        Param::optional("loop", Kind::Bool, ""),
    ],
};

/// What keys can drive here (bones are keyed in the tab).
const PROPERTIES: &[&str] = &[
    "Position",
    "Orientation",
    "Size",
    "Color",
    "Transparency",
    "BackgroundColor3",
    "BackgroundTransparency",
    "TextColor3",
    "TextSize",
    "ImageTransparency",
    "FieldOfView",
];

/// The most keys `info` lists.
const MAX_KEYS_SHOWN: usize = 40;

pub fn call(host: &mut Host, args: &Args) -> Outcome {
    let Some(folder) = host
        .app
        .project
        .as_ref()
        .map(|project| project.folder.clone())
    else {
        return Err(ToolError::not_found("no project is open"));
    };
    let action = args.str("action").unwrap_or_default();
    if action != "info" && host.app.playing() {
        return Err(ToolError::new(Code::Busy, "the game is playing")
            .with_fix("stop Play in the editor, then try again"));
    }
    if action == "animate" {
        return animate(host, &folder, args);
    }
    let (path, root) = clip_of(&host.app.world, args)?;
    check_path(&folder, &path)?;
    let before = std::fs::read_to_string(folder.join(&path))
        .map_err(|error| ToolError::internal(format!("could not read {path}: {error}")))?;
    let mut clip = ClipFile::from_json(&before).map_err(|error| {
        ToolError::new(Code::BadValue, format!("{path} won't load: {}", error.0))
    })?;
    if action == "info" {
        return Ok(info(&path, &clip));
    }
    if clip.link.is_some() {
        return Err(ToolError::new(
            Code::Unsupported,
            format!("{path} plays a take from a model file; only keyed clips take keys here"),
        ));
    }
    if host.app.animation.unwritten(&path) {
        return Err(ToolError::new(
            Code::Locked,
            format!("{path} has unsaved changes in the Animation tab"),
        )
        .with_fix("ask the user to save it first"));
    }
    let done = match action {
        "key" => key(&host.app.world, root, &mut clip, args)?,
        _ => unkey(&mut clip, args)?,
    };
    let mut done = done;
    if let Some(duration) = args.f64("duration") {
        if !(0.01..=3600.0).contains(&duration) {
            return Err(ToolError::new(
                Code::BadValue,
                "duration is 0.01 to 3,600 seconds",
            ));
        }
        clip.duration = duration;
        done.push_str(&format!(" · {duration} s long"));
    }
    if let Some(looping) = args.bool("loop") {
        clip.looping = looping;
    }
    let after = clip.to_json();
    checkpoint(host, &folder);
    axcenity_project::write_script(&folder, &path, &after)
        .map_err(|error| ToolError::internal(format!("could not write {path}: {error}")))?;
    host.app.disk_changed(std::slice::from_ref(&path));
    if !host.session.files.contains(&path) {
        host.session.files.push(path.clone());
    }
    let undo = format!("AI ({}): {done}", host.session.client);
    let app = &mut host.app;
    app.history.record_asset(
        &mut app.world,
        undo.clone(),
        vec![TextChange {
            path: path.clone(),
            before: Some(before),
            after: Some(after),
        }],
        Vec::new(),
    );
    app.history.seal();
    host.session.last_undo = Some(undo.clone());
    Ok(format!("{done} in {path} · undo {}", quoted(&undo)))
}

/// The editor's Animate on `id`.
fn animate(host: &mut Host, folder: &std::path::Path, args: &Args) -> Outcome {
    let Some(text) = args.str("id") else {
        return Err(ToolError::bad_args(
            "animate needs id: the object to animate",
        ));
    };
    let world = &host.app.world;
    let object = resolve(world, text)?;
    let class = world.get(object).map(|instance| instance.class());
    if !class.is_some_and(can_animate) {
        return Err(ToolError::new(
            Code::Unsupported,
            format!("{} can't be animated", label(world, object)),
        )
        .with_fix("animate a Part, MeshPart, Model, ScreenGui or UI element"));
    }
    if world.is_locked(object) {
        return Err(ToolError::new(
            Code::Locked,
            format!("{} is locked", label(world, object)),
        ));
    }
    checkpoint(host, folder);
    let app = &mut host.app;
    let revision = app.history.revision();
    let from = app.output.total();
    app.animate(object);
    if app.history.revision() == revision {
        let reason = app
            .output
            .since(from)
            .iter()
            .rev()
            .find(|line| line.kind == crate::output::OutputKind::Error)
            .map_or_else(|| "nothing changed".to_owned(), |line| line.text.clone());
        return Err(ToolError::new(Code::BadValue, reason));
    }
    let name = app
        .world
        .get(object)
        .map_or_else(String::new, |i| i.name().to_owned());
    let undo = format!("AI ({}): Animate {name}", host.session.client);
    app.history.relabel_top(undo.clone());
    app.history.seal();
    let world = &app.world;
    // Animate puts the new Animation last in the Animator.
    let animation = animator_of(world, object)
        .and_then(|animator| world.get(animator))
        .and_then(|animator| animator.children().last().copied());
    let clip = animation
        .and_then(|id| match world.get(id)?.property("Clip") {
            Some(Value::String(path)) => Some(path.clone()),
            _ => None,
        })
        .unwrap_or_default();
    if !clip.is_empty() && !host.session.files.contains(&clip) {
        host.session.files.push(clip.clone());
    }
    let out = format!(
        "animated {} · Animation {} plays {clip} (empty, 1.5 s); key it with action key, id {} · undo {}",
        label(world, object),
        animation.map_or_else(String::new, id_text),
        id_text(object),
        quoted(&undo)
    );
    host.session.last_undo = Some(undo);
    Ok(out)
}

/// The clip to change and the object its targets are under (`None` when
/// only `clip` was given).
fn clip_of(world: &World, args: &Args) -> Result<(String, Option<InstanceId>), ToolError> {
    let clip = args.str("clip").map(str::to_owned);
    let Some(text) = args.str("id") else {
        return clip.map(|clip| (clip, None)).ok_or_else(|| {
            ToolError::bad_args("give id (the animated object or its Animation) or clip")
        });
    };
    let id = resolve(world, text)?;
    let instance = world
        .get(id)
        .ok_or_else(|| ToolError::not_found("no such object"))?;
    // An Animation: its clip, under its Animator's parent (or its own).
    if instance.class() == ClassName::Animation {
        let parent = instance.parent();
        let root = match parent.and_then(|p| world.get(p)) {
            Some(p) if p.class() == ClassName::Animator => p.parent(),
            _ => parent,
        };
        let own = match instance.property("Clip") {
            Some(Value::String(path)) if !path.is_empty() => Some(path.clone()),
            _ => None,
        };
        return clip
            .or(own)
            .map(|clip| (clip, root))
            .ok_or_else(|| ToolError::not_found(format!("{} has no Clip", label(world, id))));
    }
    if let Some(clip) = clip {
        return Ok((clip, Some(id)));
    }
    // An animated object: its Animations' clips (the first keyed one).
    let animations: Vec<String> = animator_of(world, id)
        .into_iter()
        .chain(std::iter::once(id))
        .filter_map(|holder| world.get(holder))
        .flat_map(|holder| holder.children().to_vec())
        .filter_map(|child| {
            let animation = world.get(child)?;
            match (animation.class(), animation.property("Clip")) {
                (ClassName::Animation, Some(Value::String(path))) if !path.is_empty() => {
                    Some(path.clone())
                }
                _ => None,
            }
        })
        .collect();
    match animations.as_slice() {
        [] => Err(
            ToolError::not_found(format!("{} has no Animation", label(world, id)))
                .with_fix("action animate gives it one"),
        ),
        [one] => Ok((one.clone(), Some(id))),
        many => Err(ToolError::new(Code::Ambiguous, "it has several Animations")
            .with_fix(format!("give clip: {}", many.join(", ")))),
    }
}

fn property(args: &Args) -> Result<PropertyName, ToolError> {
    args.str("property")
        .and_then(PropertyName::from_name)
        .ok_or_else(|| {
            ToolError::bad_args("give property")
                .with_fix(format!("one of {}", PROPERTIES.join(", ")))
        })
}

fn time(args: &Args) -> Result<f64, ToolError> {
    match args.f64("time") {
        Some(time) if (0.0..=3600.0).contains(&time) => Ok(time),
        Some(_) => Err(ToolError::new(Code::BadValue, "time is 0 to 3,600 seconds")),
        None => Err(ToolError::bad_args("give time in seconds")),
    }
}

/// Sets one key, making its track if needed. Returns what it did.
fn key(
    world: &World,
    root: Option<InstanceId>,
    clip: &mut ClipFile,
    args: &Args,
) -> Result<String, ToolError> {
    let property = property(args)?;
    let time = time(args)?;
    let target = args.str("target").unwrap_or(".").trim().to_owned();
    if let Some(root) = root
        && resolve_target(world, root, &target).is_none()
    {
        return Err(ToolError::not_found(format!(
            "no {} under {}",
            quoted(&target),
            label(world, root)
        ))
        .with_fix("target is names below the animated object, like \"Hinge/Panel\", or \".\""));
    }
    let numbers: Vec<f32> = match (args.vector3("value"), args.f64("number")) {
        (Some(value), _) => value.iter().map(|&n| n as f32).collect(),
        (None, Some(number)) => vec![number as f32],
        (None, None) => root
            .and_then(|root| current_numbers(world, root, &target, property, &Applied::default()))
            .ok_or_else(|| {
                ToolError::bad_args("give value or number")
                    .with_fix("or give id, to key what the target is now")
            })?,
    };
    if !property.widths().contains(&numbers.len()) {
        return Err(ToolError::new(
            Code::BadValue,
            format!(
                "{} takes {} numbers, not {}",
                property.as_str(),
                property
                    .widths()
                    .iter()
                    .map(usize::to_string)
                    .collect::<Vec<_>>()
                    .join(" or "),
                numbers.len()
            ),
        ));
    }
    let interp = match args.str("interp") {
        Some("step") => Interp::Step,
        Some("ease") => Interp::Ease,
        _ => Interp::Linear,
    };
    let key_ids: Vec<String> = clip
        .tracks
        .iter()
        .flat_map(|track| track.keys.iter().map(|key| key.id.clone()))
        .collect();
    let index = match clip
        .tracks
        .iter()
        .position(|track| track.target == target && track.property == property)
    {
        Some(index) => index,
        None => {
            let ids: Vec<&str> = clip.tracks.iter().map(|track| track.id.as_str()).collect();
            let id = free_id(&property.as_str().to_lowercase(), &ids);
            clip.tracks.push(PropertyTrack {
                id,
                target: target.clone(),
                property,
                keys: Vec::new(),
            });
            clip.tracks.len() - 1
        }
    };
    let track = &mut clip.tracks[index];
    if let Some(first) = track.keys.first()
        && first.value.len() != numbers.len()
    {
        return Err(ToolError::new(
            Code::BadValue,
            format!("the track's keys have {} numbers", first.value.len()),
        ));
    }
    let shown = numbers
        .iter()
        .map(|n| format!("{n}"))
        .collect::<Vec<_>>()
        .join(", ");
    let changed = match track
        .keys
        .iter_mut()
        .find(|key| (key.time - time).abs() < 1e-6)
    {
        Some(key) => {
            key.value = numbers;
            key.interp = interp;
            true
        }
        None => {
            let ids: Vec<&str> = key_ids.iter().map(String::as_str).collect();
            let mut new = PropertyKeyframe::new(free_id("k", &ids), time, numbers);
            new.interp = interp;
            let at = track.keys.partition_point(|key| key.time < time);
            track.keys.insert(at, new);
            false
        }
    };
    let mut done = format!(
        "{} {} {} at {time} s = [{shown}]",
        if changed { "Change key" } else { "Key" },
        target,
        property.as_str()
    );
    if time > clip.duration {
        clip.duration = time;
        done.push_str(&format!(" · clip now {time} s long"));
    }
    Ok(done)
}

/// Takes the key at `time` off a track (and the track, once empty).
fn unkey(clip: &mut ClipFile, args: &Args) -> Result<String, ToolError> {
    let property = property(args)?;
    let time = time(args)?;
    let target = args.str("target").unwrap_or(".").trim().to_owned();
    let Some(index) = clip
        .tracks
        .iter()
        .position(|track| track.target == target && track.property == property)
    else {
        return Err(ToolError::not_found(format!(
            "no {} track for {}",
            property.as_str(),
            quoted(&target)
        ))
        .with_fix("action info lists the tracks"));
    };
    let track = &mut clip.tracks[index];
    let Some(at) = track
        .keys
        .iter()
        .position(|key| (key.time - time).abs() < 1e-3)
    else {
        let times: Vec<String> = track
            .keys
            .iter()
            .map(|key| format!("{}", key.time))
            .collect();
        return Err(ToolError::not_found(format!("no key at {time} s"))
            .with_fix(format!("its keys are at {}", times.join(", "))));
    };
    track.keys.remove(at);
    if track.keys.is_empty() {
        clip.tracks.remove(index);
    }
    Ok(format!(
        "Remove key {target} {} at {time} s",
        property.as_str()
    ))
}

/// `prefix`, or `prefix2`, `prefix3`… — the first `taken` doesn't hold.
fn free_id(prefix: &str, taken: &[&str]) -> String {
    (1..)
        .map(|n| {
            if n == 1 {
                prefix.to_owned()
            } else {
                format!("{prefix}{n}")
            }
        })
        .find(|id| !taken.contains(&id.as_str()))
        .expect("an unused id")
}

/// The clip's length, loop, tracks and keys, one track a line.
fn info(path: &str, clip: &ClipFile) -> String {
    let mut out = format!(
        "{path} · {} s · {} · {} track{}",
        clip.duration,
        if clip.looping { "loops" } else { "plays once" },
        clip.tracks.len(),
        if clip.tracks.len() == 1 { "" } else { "s" }
    );
    if let Some(link) = &clip.link {
        out.push_str(&format!(" · plays take {:?} of {}", link.take, link.source));
    }
    let mut shown = 0;
    for track in &clip.tracks {
        out.push_str(&format!("\n{} {}:", track.target, track.property.as_str()));
        for key in &track.keys {
            if shown == MAX_KEYS_SHOWN {
                out.push_str(" …");
                break;
            }
            shown += 1;
            let numbers = key
                .value
                .iter()
                .map(|n| format!("{n}"))
                .collect::<Vec<_>>()
                .join(", ");
            let interp = match key.interp {
                Interp::Step => " step",
                Interp::Ease => " ease",
                Interp::Bezier => " bezier",
                Interp::Linear => "",
            };
            out.push_str(&format!(" {} [{numbers}]{interp};", key.time));
        }
    }
    for event in &clip.events {
        out.push_str(&format!("\nevent {:?} at {} s", event.name, event.time));
    }
    out
}
