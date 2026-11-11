//! What changed in the world, for `history {"action":"changes"}`: the
//! answer to "what happened since I last looked?" without reading the
//! scene again (`docs/research/mcp/architecture.md` §6.3).
//!
//! The host keeps the edit world's change log on and moves what it
//! records into a bounded [`ChangeLog`] around every call: changes from
//! before a call are someone else's (the user's in the live editor, or
//! another client's), changes during it are this client's. Each change
//! gets a number; a mark is the number the next change will get, so
//! `since: mark` asks for everything after it. (Not "cursor": that's the
//! paging token of cut results.)
//!
//! A step that swaps in a whole world (undoing a prefab Apply, opening a
//! level) isn't a list of changes. The log notices the world's own count
//! jumping and starts again; older marks then say "read it again".

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};

use axcenity_core::{Change, InstanceId, RecordedChange, World};

use super::refs::{id_text, in_tree, label, path, quoted};

/// The most changes kept. A bigger edit than this (thousands of parts)
/// pushes older marks out, and the answer says to read again.
const KEPT: usize = 20_000;

/// Why older marks can't be answered after a world was swapped in.
const REPLACED: &str =
    "the whole world was replaced (a level opened, or a whole-world step was undone or redone)";

/// The most objects a folded line names before `…`.
const NAMED: usize = 8;

/// Who made a change, as far as this session can tell.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum By {
    /// This client, during one of its calls.
    Client,
    /// Anyone else, between this client's calls: the user in the live
    /// editor, or another client.
    Others,
}

#[derive(Debug)]
struct Noted {
    number: u64,
    by: By,
    change: Change,
}

/// The changes this session has seen, numbered.
#[derive(Debug, Default)]
pub struct ChangeLog {
    kept: VecDeque<Noted>,
    /// The number the next change gets: the current mark.
    next: u64,
    /// No mark below this can be answered (changes dropped, or the
    /// world was replaced), and why.
    floor: u64,
    lost: &'static str,
    /// The world's number for its next change, as far as the log knows,
    /// to notice a world that was swapped in.
    world_next: Option<u64>,
    /// The mark the last answer gave: the default `since`.
    pub asked: u64,
}

impl ChangeLog {
    /// Moves what `world` recorded into the log as `by`'s, turning the
    /// world's recording on if it's off (a new world).
    pub fn take(&mut self, world: &mut World, by: By) {
        if !world.is_recording_changes() {
            world.record_changes();
            if self.world_next.take().is_some() {
                self.restart(REPLACED);
            }
        }
        let changes = world.take_changes();
        self.add(by, changes);
        let now = world.next_change_number();
        if self.world_next.is_some_and(|known| known != now) {
            self.restart(REPLACED);
        }
        self.world_next = Some(now);
    }

    /// Notes changes taken from the world, as `by`'s. A gap in the
    /// world's numbers means a world was swapped in between.
    pub fn add(&mut self, by: By, changes: Vec<RecordedChange>) {
        for recorded in changes {
            if self
                .world_next
                .is_some_and(|known| known != recorded.number)
            {
                self.restart(REPLACED);
            }
            self.world_next = Some(recorded.number + 1);
            self.kept.push_back(Noted {
                number: self.next,
                by,
                change: recorded.change,
            });
            self.next += 1;
            if self.kept.len() > KEPT
                && let Some(old) = self.kept.pop_front()
            {
                self.floor = old.number + 1;
                self.lost = "more changed than is kept";
            }
        }
    }

    fn restart(&mut self, why: &'static str) {
        self.kept.clear();
        self.floor = self.next;
        self.lost = why;
    }

    /// What changed since the mark `since` (default: the last answer's),
    /// one line per object or per fold of alike objects, at most `limit`
    /// lines. The answer's mark becomes the next default.
    pub fn summary(&mut self, world: &World, since: Option<u64>, limit: usize) -> String {
        let since = since.unwrap_or(self.asked);
        let cursor = self.next;
        self.asked = cursor;
        if since > cursor {
            return format!("mark {cursor} · {since} is ahead of it; use {cursor}");
        }
        if since < self.floor {
            return format!(
                "mark {cursor} · can't list changes since {since}: {}; read the scene again (scene_inspect)",
                self.lost
            );
        }
        let objects = Objects::gather(self.kept.iter().filter(|noted| noted.number >= since));
        let lines = objects.lines(world);
        if lines.is_empty() {
            return format!("mark {cursor} · nothing changed since {since}");
        }
        let mut text = format!("mark {cursor} · since {since}:");
        let several_sides = lines.iter().any(|(by, _)| *by == By::Others)
            && lines.iter().any(|(by, _)| *by == By::Client);
        let mut side = None;
        for (shown, (by, line)) in lines.iter().enumerate() {
            if shown == limit {
                text.push_str(&format!("\n… {} more lines", lines.len() - limit));
                break;
            }
            if several_sides && side != Some(*by) {
                text.push_str(match by {
                    By::Client => "\nby you:",
                    By::Others => "\nby others (the user or another client):",
                });
                side = Some(*by);
            }
            text.push_str(&format!("\n{line}"));
        }
        if !several_sides && lines[0].0 == By::Others {
            text.push_str("\n(all by others: the user or another client)");
        }
        text
    }
}

/// What happened to one object over the changes asked about.
#[derive(Debug, Default)]
struct Seen {
    by: BTreeSet<By>,
    /// Whether its first move in the tree was being added (it wasn't in
    /// the tree before) or taken out.
    first_added: Option<bool>,
    props: BTreeSet<&'static str>,
    attrs: BTreeSet<String>,
    tags: BTreeMap<String, bool>,
}

/// The objects the changes name, in the order they first changed.
#[derive(Default)]
struct Objects {
    order: Vec<InstanceId>,
    seen: HashMap<InstanceId, Seen>,
}

impl Objects {
    fn gather<'a>(changes: impl Iterator<Item = &'a Noted>) -> Self {
        let mut objects = Objects::default();
        for noted in changes {
            let id = match &noted.change {
                // Moves show as added, removed or moved.
                Change::Property { name: "Parent", .. } => continue,
                Change::Property { instance, .. }
                | Change::Attribute { instance, .. }
                | Change::Tag { instance, .. } => *instance,
                Change::ChildAdded { child, .. } | Change::ChildRemoved { child, .. } => *child,
            };
            let seen = objects.seen.entry(id).or_insert_with(|| {
                objects.order.push(id);
                Seen::default()
            });
            seen.by.insert(noted.by);
            match &noted.change {
                Change::Property { name, .. } => {
                    seen.props.insert(name);
                }
                Change::ChildAdded { .. } => {
                    seen.first_added.get_or_insert(true);
                }
                Change::ChildRemoved { .. } => {
                    seen.first_added.get_or_insert(false);
                }
                Change::Attribute { name, .. } => {
                    seen.attrs.insert(name.clone());
                }
                Change::Tag { tag, added, .. } => {
                    // Added and then removed again is no change.
                    if seen.tags.get(tag) == Some(&!added) {
                        seen.tags.remove(tag);
                    } else {
                        seen.tags.insert(tag.clone(), *added);
                    }
                }
            }
        }
        objects
    }

    /// One line per object (or fold of alike ones), with who did it:
    /// this client's first.
    fn lines(&self, world: &World) -> Vec<(By, String)> {
        let added: BTreeSet<InstanceId> = self
            .order
            .iter()
            .copied()
            .filter(|id| self.seen[id].first_added == Some(true) && in_tree(world, *id))
            .collect();
        // Inside something added: it came with it.
        let came_with = |id: InstanceId| {
            let mut at = world.get(id).and_then(|instance| instance.parent());
            while let Some(parent) = at {
                if added.contains(&parent) {
                    return true;
                }
                at = world.get(parent).and_then(|instance| instance.parent());
            }
            false
        };
        // (who, what was done, where or what to) → the objects.
        let mut folds: Vec<((By, String, String), Vec<InstanceId>)> = Vec::new();
        for &id in &self.order {
            let seen = &self.seen[&id];
            let now = in_tree(world, id);
            let (verb, detail) = match (seen.first_added, now) {
                // Added and taken out again, or changed outside the tree.
                (Some(true), false) | (None, false) => continue,
                (Some(true), true) => {
                    if came_with(id) {
                        continue;
                    }
                    ("added", format!("in {}", parent_path(world, id)))
                }
                (Some(false), false) => ("removed", String::new()),
                (Some(false), true) => {
                    let mut detail = format!("to {}", parent_path(world, id));
                    let set = settings(seen);
                    if !set.is_empty() {
                        detail.push_str(&format!("; set {set}"));
                    }
                    ("moved", detail)
                }
                (None, true) => {
                    if came_with(id) {
                        continue;
                    }
                    let set = settings(seen);
                    if set.is_empty() {
                        continue;
                    }
                    ("set", set)
                }
            };
            // Both sides changed it: say it once, as someone else's.
            let by = seen.by.last().copied().unwrap_or(By::Client);
            let key = (by, verb.to_owned(), detail);
            match folds.iter_mut().find(|(known, _)| *known == key) {
                Some((_, ids)) => ids.push(id),
                None => folds.push((key, vec![id])),
            }
        }
        folds.sort_by_key(|((by, _, _), _)| *by);
        folds
            .into_iter()
            .map(|((by, verb, detail), ids)| {
                let line = if let [id] = ids[..] {
                    let separator = if verb == "set" { ": " } else { " " };
                    format!(
                        "{verb} {}{}{detail}",
                        label(world, id),
                        if detail.is_empty() { "" } else { separator }
                    )
                } else {
                    let mut named: Vec<String> =
                        ids.iter().take(NAMED).map(|&id| id_text(id)).collect();
                    if ids.len() > NAMED {
                        named.push("…".to_owned());
                    }
                    let separator = if verb == "set" { ": " } else { " " };
                    // `4× Part "Crate"` when they're alike, else `4 objects`.
                    let kind = |id: InstanceId| {
                        world
                            .get(id)
                            .map(|instance| (instance.class(), instance.name().to_owned()))
                    };
                    let what = match kind(ids[0]) {
                        Some((class, name))
                            if ids
                                .iter()
                                .all(|&id| kind(id).as_ref() == Some(&(class, name.clone()))) =>
                        {
                            format!("{}× {} {}", ids.len(), class.as_str(), quoted(&name))
                        }
                        _ => format!("{} objects", ids.len()),
                    };
                    format!(
                        "{verb} {what}{}{detail} ({})",
                        if detail.is_empty() { "" } else { separator },
                        named.join(", ")
                    )
                };
                (by, line)
            })
            .collect()
    }
}

/// Where an object is now: its parent's path.
fn parent_path(world: &World, id: InstanceId) -> String {
    world
        .get(id)
        .and_then(|instance| instance.parent())
        .map_or_else(|| "game".to_owned(), |parent| path(world, parent))
}

/// `Position, Color; attrs Health; tags +Enemy -Old`, or empty.
fn settings(seen: &Seen) -> String {
    let mut parts = Vec::new();
    if !seen.props.is_empty() {
        parts.push(seen.props.iter().copied().collect::<Vec<_>>().join(", "));
    }
    if !seen.attrs.is_empty() {
        let attrs: Vec<&str> = seen.attrs.iter().map(String::as_str).collect();
        parts.push(format!("attrs {}", attrs.join(", ")));
    }
    if !seen.tags.is_empty() {
        let tags: Vec<String> = seen
            .tags
            .iter()
            .map(|(tag, added)| format!("{}{tag}", if *added { '+' } else { '-' }))
            .collect();
        parts.push(format!("tags {}", tags.join(" ")));
    }
    parts.join("; ")
}
