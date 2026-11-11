//! `object_delete`: the editor's Delete (objects leave the tree, and undo
//! puts them back), as one undo step. Up to [`ASK_OVER`] objects with no
//! script among them go at once; more, or any script, return a preview
//! first (what goes, which scripts, what still points at them) and a
//! confirm token. A script's file stays on disk.

use axcenity_core::{InstanceId, Value, World};
use axcenity_mcp::{Args, Class, Code, Outcome, Profile, ToolError, ToolSpec};

use super::object_set::targets;
use super::ops::{confirm, write};
use super::refs::{label, path};
use super::{CONFIRM, Host, ID, IDS};
use crate::tree;

pub const SPEC: ToolSpec = ToolSpec {
    name: "object_delete",
    title: "Delete objects",
    summary: "Delete objects and their contents (undoable); big or script deletes preview first.",
    profile: Profile::Lite,
    class: Class::Destructive,
    idempotent: false,
    params: &[ID, IDS, CONFIRM],
};

/// The most objects (counting what's inside) one call deletes without
/// asking first.
const ASK_OVER: usize = 5;

/// What a delete takes away.
struct Doomed {
    /// The objects named, without any inside another named one.
    roots: Vec<InstanceId>,
    /// Every object that goes, the roots included.
    all: Vec<InstanceId>,
    scripts: Vec<InstanceId>,
    /// Properties outside that point in: (holder, property).
    references: Vec<(InstanceId, &'static str)>,
}

pub fn call(host: &mut Host, args: &Args) -> Outcome {
    let world = &host.app.world;
    let ids = targets(world, args)?;
    for &id in &ids {
        tree::check_delete(world, id).map_err(|reason| {
            ToolError::new(
                Code::NotAllowed,
                format!("can't delete {}: {reason}", label(world, id)),
            )
        })?;
    }
    let doomed = doomed(world, &ids);
    let what = match doomed.roots.as_slice() {
        [one] => world
            .get(*one)
            .map_or_else(String::new, |i| i.name().to_owned()),
        many => format!("{} objects", many.len()),
    };
    if doomed.all.len() > ASK_OVER || !doomed.scripts.is_empty() {
        let preview = preview(world, &doomed);
        confirm(host, &SPEC, args, &preview)?;
    }
    let count = doomed.all.len();
    let scripts = doomed.scripts.len();
    let roots = doomed.roots.clone();
    let written = write(host, &format!("Delete {what}"), |world| {
        for &id in &roots {
            world
                .set_parent(id, None)
                .map_err(|error| ToolError::new(Code::NotAllowed, error.to_string()))?;
        }
        Ok(())
    })?;
    let mut text = format!(
        "deleted {count} object{}",
        if count == 1 { "" } else { "s" }
    );
    if scripts > 0 {
        text.push_str(&format!(
            " ({scripts} script{}; files stay on disk)",
            if scripts == 1 { "" } else { "s" }
        ));
    }
    text.push_str(&written.undo_note());
    Ok(text)
}

fn doomed(world: &World, ids: &[InstanceId]) -> Doomed {
    let roots: Vec<InstanceId> = ids
        .iter()
        .copied()
        .filter(|&id| {
            !ids.iter()
                .any(|&other| other != id && world.is_descendant_of(id, other))
        })
        .collect();
    let mut all = Vec::new();
    for &root in &roots {
        all.push(root);
        all.extend(world.descendants_iter(root));
    }
    let scripts = all
        .iter()
        .copied()
        .filter(|&id| {
            world
                .get(id)
                .is_some_and(|instance| instance.class().is_script())
        })
        .collect();
    // Looked up once per property of every object, so a set.
    let going: std::collections::HashSet<InstanceId> = all.iter().copied().collect();
    let mut references = Vec::new();
    for holder in world.descendants_iter(world.root()) {
        if going.contains(&holder) {
            continue;
        }
        let Some(instance) = world.get(holder) else {
            continue;
        };
        for (property, value) in instance.properties() {
            if let Value::Instance(Some(target)) = value
                && going.contains(target)
            {
                references.push((holder, property));
            }
        }
    }
    Doomed {
        roots,
        all,
        scripts,
        references,
    }
}

fn preview(world: &World, doomed: &Doomed) -> String {
    let mut text = String::from("deletes ");
    let named: Vec<String> = doomed
        .roots
        .iter()
        .take(5)
        .map(|&id| label(world, id))
        .collect();
    text.push_str(&named.join(", "));
    if doomed.roots.len() > 5 {
        text.push_str(&format!(" and {} more", doomed.roots.len() - 5));
    }
    let inside = doomed.all.len() - doomed.roots.len();
    if inside > 0 {
        text.push_str(&format!(
            " with {inside} inside ({} objects in all)",
            doomed.all.len()
        ));
    }
    if !doomed.scripts.is_empty() {
        let scripts: Vec<String> = doomed
            .scripts
            .iter()
            .take(5)
            .map(|&id| path(world, id))
            .collect();
        text.push_str(&format!(
            ". Scripts: {} (their files stay)",
            scripts.join(", ")
        ));
    }
    if !doomed.references.is_empty() {
        let shown: Vec<String> = doomed
            .references
            .iter()
            .take(5)
            .map(|&(holder, property)| format!("{}.{property}", path(world, holder)))
            .collect();
        text.push_str(&format!(
            ". {} reference{} will point at nothing: {}",
            doomed.references.len(),
            if doomed.references.len() == 1 {
                ""
            } else {
                "s"
            },
            shown.join(", ")
        ));
    }
    text
}
