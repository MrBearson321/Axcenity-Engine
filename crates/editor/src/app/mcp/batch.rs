//! `batch`: several object edits as one undo step, all or nothing.
//!
//! Each step is `{"op":"create"|"set"|"delete"|"duplicate", …}` with the
//! same arguments as that tool (`object_create`, …), plus `as` to name
//! what it makes. A later step names it `$name` in `id`, `ids`, `parent`
//! or a property value: `{"op":"create","kind":"Model","name":"Hut",
//! "as":"hut"}`, then `{"op":"create","kind":"Block","parent":"$hut"}`.
//!
//! The steps first run on a copy of the world, which finds any failing
//! step and what needs asking (a big change or delete) before anything
//! changes. Then they run for real inside one [`write`], so a failure
//! still puts everything back.

use std::collections::HashMap;

use axcenity_core::{InstanceId, World};
use axcenity_mcp::{Args, Class, Code, Kind, Outcome, Param, Profile, ToolError, ToolSpec};
use serde_json::{Map, Value as Json};

use super::ops::{confirm, write};
use super::refs::{id_text, label};
use super::{CONFIRM, Host};
use super::{object_create, object_delete, object_duplicate, object_set};

pub const SPEC: ToolSpec = ToolSpec {
    name: "batch",
    title: "Batch edits",
    summary: "Run create/set/delete/duplicate steps as one undo step, all or nothing; \"as\" names a result for later steps as \"$name\".",
    profile: Profile::Standard,
    class: Class::Modify,
    idempotent: false,
    params: &[
        Param::required("ops", Kind::Objects, "steps: {op, that tool's args, as}"),
        Param::optional("label", Kind::String, "undo step's name"),
        CONFIRM,
    ],
};

/// The most steps one batch runs.
const MAX_STEPS: usize = 100;
/// The most objects one batch touches.
const MAX_OBJECTS: usize = 2000;

#[derive(Clone, Copy, PartialEq)]
enum Op {
    Create,
    Set,
    Delete,
    Duplicate,
}

impl Op {
    const NAMES: [(&'static str, Op); 4] = [
        ("create", Op::Create),
        ("set", Op::Set),
        ("delete", Op::Delete),
        ("duplicate", Op::Duplicate),
    ];

    fn name(self) -> &'static str {
        Op::NAMES
            .iter()
            .find(|(_, op)| *op == self)
            .map_or("", |(name, _)| name)
    }

    fn spec(self) -> &'static ToolSpec {
        match self {
            Op::Create => &object_create::SPEC,
            Op::Set => &object_set::SPEC,
            Op::Delete => &object_delete::SPEC,
            Op::Duplicate => &object_duplicate::SPEC,
        }
    }
}

/// One step, read and checked.
struct Step {
    op: Op,
    args: Map<String, Json>,
    /// `as`: what later steps call what this one makes.
    name: Option<String>,
}

/// What running the steps did.
struct Report {
    lines: Vec<String>,
    /// Previews of the steps that ask first.
    asks: Vec<String>,
}

pub fn call(host: &mut Host, args: &Args) -> Outcome {
    if host.app.playing() {
        return Err(ToolError::new(Code::Busy, "the game is playing")
            .with_fix("stop Play in the editor, then try again"));
    }
    let steps = read_steps(args)?;
    // A trial run on a copy: nothing changes if a step fails or asks.
    let mut trial = host.app.world.clone();
    let report = run(&mut trial, &steps)?;
    drop(trial);
    if !report.asks.is_empty() {
        confirm(host, &SPEC, args, &report.asks.join("; "))?;
    }
    let undo_label = args
        .str("label")
        .map_or_else(|| format!("Batch ({})", steps.len()), str::to_owned);
    let written = write(host, &undo_label, |world| run(world, &steps))?;
    let mut out = format!(
        "ran {} step{}{}",
        steps.len(),
        if steps.len() == 1 { "" } else { "s" },
        written.undo_note()
    );
    for (number, line) in written.value.lines.iter().enumerate() {
        out.push_str(&format!("\n{}. {line}", number + 1));
    }
    Ok(out)
}

/// The steps in `args`, each checked against its tool's parameters.
fn read_steps(args: &Args) -> Result<Vec<Step>, ToolError> {
    let items = args
        .get("ops")
        .and_then(Json::as_array)
        .cloned()
        .unwrap_or_default();
    if items.is_empty() {
        return Err(ToolError::bad_args("ops is empty")
            .with_fix("give steps like {\"op\":\"create\",\"kind\":\"Block\"}"));
    }
    if items.len() > MAX_STEPS {
        return Err(ToolError::new(
            Code::TooLarge,
            format!("{} steps; at most {MAX_STEPS} in one batch", items.len()),
        ));
    }
    let mut steps = Vec::new();
    let mut named: Vec<String> = Vec::new();
    for (index, item) in items.into_iter().enumerate() {
        let number = index + 1;
        let Json::Object(mut map) = item else {
            return Err(ToolError::bad_args(format!(
                "step {number} isn't an object"
            )));
        };
        let op_name = map.remove("op");
        let op = op_name
            .as_ref()
            .and_then(Json::as_str)
            .and_then(|text| Op::NAMES.iter().find(|(name, _)| *name == text))
            .map(|(_, op)| *op)
            .ok_or_else(|| {
                ToolError::bad_args(format!("step {number} needs op"))
                    .with_fix("op is create, set, delete or duplicate")
            })?;
        let name = match map.remove("as") {
            None | Some(Json::Null) => None,
            Some(Json::String(name)) if !name.is_empty() && !name.starts_with('$') => {
                if named.contains(&name) {
                    return Err(ToolError::bad_args(format!(
                        "step {number}: another step is already \"as\":\"{name}\""
                    )));
                }
                named.push(name.clone());
                Some(name)
            }
            Some(_) => {
                return Err(ToolError::bad_args(format!(
                    "step {number}: as should be a name like \"hut\""
                )));
            }
        };
        if name.is_some() && matches!(op, Op::Set | Op::Delete) {
            return Err(ToolError::bad_args(format!(
                "step {number}: only create and duplicate make something to name with as"
            )));
        }
        if map.contains_key("confirm") {
            return Err(ToolError::bad_args(format!(
                "step {number}: confirm goes on the batch, not on a step"
            )));
        }
        Args::new(map.clone())
            .check(op.spec())
            .map_err(|error| in_step(error, number, op))?;
        steps.push(Step {
            op,
            args: map,
            name,
        });
    }
    Ok(steps)
}

/// Runs `steps` in order on `world`. Stops at the first that fails.
fn run(world: &mut World, steps: &[Step]) -> Result<Report, ToolError> {
    let mut names: HashMap<&str, InstanceId> = HashMap::new();
    let mut report = Report {
        lines: Vec::new(),
        asks: Vec::new(),
    };
    let mut touched = 0;
    for (index, step) in steps.iter().enumerate() {
        let number = index + 1;
        let fail = |error| in_step(error, number, step.op);
        let mut map = step.args.clone();
        bind(&mut map, &names).map_err(fail)?;
        let args = Args::new(map);
        let (line, made, count) = match step.op {
            Op::Create => {
                let id = object_create::step(world, &args).map_err(fail)?;
                let line = format!(
                    "created {} in {}",
                    label(world, id),
                    object_create::parent_path(world, id)
                );
                (line, vec![id], 1)
            }
            Op::Set => {
                let change = object_set::prepare(world, &args).map_err(fail)?;
                if let Some(preview) = object_set::ask(world, &change) {
                    report.asks.push(format!("step {number} {preview}"));
                }
                let changed = object_set::apply(world, &change).map_err(fail)?;
                let line = format!(
                    "changed {}: {}",
                    object_set::objects(world, &change.ids),
                    changed.join(", ")
                );
                (line, Vec::new(), change.ids.len())
            }
            Op::Delete => {
                let doomed = object_delete::prepare(world, &args).map_err(fail)?;
                if let Some(preview) = object_delete::ask(world, &doomed) {
                    report.asks.push(format!("step {number} {preview}"));
                }
                let line = object_delete::apply(world, &doomed).map_err(fail)?;
                (line, Vec::new(), doomed.all.len())
            }
            Op::Duplicate => {
                let copies = object_duplicate::step(world, &args).map_err(fail)?;
                let count = copies
                    .iter()
                    .map(|&id| 1 + world.descendants_iter(id).count())
                    .sum();
                (object_duplicate::result(world, &copies), copies, count)
            }
        };
        touched += count;
        if touched > MAX_OBJECTS {
            return Err(fail(ToolError::new(
                Code::TooLarge,
                format!("the batch touches over {MAX_OBJECTS} objects"),
            ))
            .with_fix("split it into smaller batches"));
        }
        if let (Some(name), Some(&first)) = (&step.name, made.first()) {
            names.insert(name, first);
        }
        report.lines.push(line);
    }
    Ok(report)
}

/// Puts the IDs of earlier steps' results where `map` says `$name`: in
/// `id`, `ids` and `parent` (where an unknown name is an error), and in
/// `props` values that name one (an object-valued property).
fn bind(map: &mut Map<String, Json>, names: &HashMap<&str, InstanceId>) -> Result<(), ToolError> {
    let lookup = |text: &str| -> Result<Option<String>, ToolError> {
        let Some(name) = text.strip_prefix('$') else {
            return Ok(None);
        };
        match names.get(name) {
            Some(&id) => Ok(Some(id_text(id))),
            None => Err(ToolError::new(
                Code::NotFound,
                format!("no earlier step is \"as\":\"{name}\""),
            )
            .with_fix("give the step that makes it \"as\" with that name")),
        }
    };
    for key in ["id", "parent"] {
        if let Some(Json::String(text)) = map.get_mut(key)
            && let Some(id) = lookup(text)?
        {
            *text = id;
        }
    }
    if let Some(Json::Array(items)) = map.get_mut("ids") {
        for item in items {
            if let Json::String(text) = item
                && let Some(id) = lookup(text)?
            {
                *text = id;
            }
        }
    }
    if let Some(Json::Object(props)) = map.get_mut("props") {
        for value in props.values_mut() {
            if let Json::String(text) = value
                && let Some(name) = text.strip_prefix('$')
                && let Some(&id) = names.get(name)
            {
                *text = id_text(id);
            }
        }
    }
    Ok(())
}

/// `error`, saying which step it came from and that nothing changed.
fn in_step(mut error: ToolError, number: usize, op: Op) -> ToolError {
    error.message = format!(
        "step {number} ({}): {}; nothing was changed",
        op.name(),
        error.message
    );
    error
}
