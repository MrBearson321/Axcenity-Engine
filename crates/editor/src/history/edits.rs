//! Steps for edits made by Lua outside the editor's own tools: a plugin's
//! (Phase 54). The plugin changes the world directly, so the step is
//! worked out afterwards from the world before and after and the Instances
//! the world's change log names. Only Instances in the game's tree, before
//! or after, are recorded: a plugin's own UI (outside the tree) is never
//! undone.
//!
//! Every edit until the plugin's next waypoint is one step: later edits
//! merge into the open step, keeping each value's oldest "before".

use axcenity_core::{Change as WorldChange, RecordedChange};

use super::*;

/// What a plugin's edits did to the tree, fields, attributes and tags.
#[derive(Debug, Default)]
pub(super) struct Edits {
    pub(super) moves: Vec<Move>,
    pub(super) fields: Vec<(InstanceId, Vec<Change>)>,
    pub(super) annotations: Vec<(InstanceId, Annotations, Annotations)>,
}

impl Edits {
    fn is_empty(&self) -> bool {
        self.moves.is_empty() && self.fields.is_empty() && self.annotations.is_empty()
    }

    /// Returns how many changes were left alone (see `History::set_shared`).
    pub(super) fn undo(&self, world: &mut World, shared: bool) -> usize {
        let mut skipped = 0;
        for (id, changes) in &self.fields {
            skipped += restore(
                world,
                shared,
                *id,
                changes.iter().map(|c| (c.field, &c.before, &c.after)),
            );
        }
        for (id, before, _) in &self.annotations {
            put_annotations(world, *id, before);
        }
        // Backwards, so each Instance goes back to where it was at the time.
        for step in self.moves.iter().rev() {
            skipped += put_back(world, shared, step.id, step.before, step.after);
        }
        skipped
    }

    pub(super) fn redo(&self, world: &mut World, shared: bool) -> usize {
        let mut skipped = 0;
        for step in &self.moves {
            skipped += put_back(world, shared, step.id, step.after, step.before);
        }
        for (id, changes) in &self.fields {
            skipped += restore(
                world,
                shared,
                *id,
                changes.iter().map(|c| (c.field, &c.after, &c.before)),
            );
        }
        for (id, _, after) in &self.annotations {
            put_annotations(world, *id, after);
        }
        skipped
    }

    /// Adds `later` on top: the oldest "before" and the newest "after" of
    /// each value. What ends where it started is dropped.
    fn merge(&mut self, later: Edits) {
        for step in later.moves {
            match self.moves.iter_mut().find(|known| known.id == step.id) {
                Some(known) => known.after = step.after,
                None => self.moves.push(step),
            }
        }
        self.moves.retain(|step| step.before != step.after);
        for (id, changes) in later.fields {
            let Some((_, known)) = self.fields.iter_mut().find(|(known, _)| *known == id) else {
                self.fields.push((id, changes));
                continue;
            };
            for change in changes {
                match known.iter_mut().find(|c| c.field == change.field) {
                    Some(old) => old.after = change.after,
                    None => known.push(change),
                }
            }
            known.retain(|change| change.before != change.after);
        }
        self.fields.retain(|(_, changes)| !changes.is_empty());
        for (id, before, after) in later.annotations {
            match self
                .annotations
                .iter_mut()
                .find(|(known, _, _)| *known == id)
            {
                Some(known) => known.2 = after,
                None => self.annotations.push((id, before, after)),
            }
        }
        self.annotations
            .retain(|(_, before, after)| before != after);
    }
}

/// The Instances `changes` name, in the order they're first named.
pub fn changed_ids(changes: &[RecordedChange]) -> Vec<InstanceId> {
    let mut ids = Vec::new();
    for recorded in changes {
        let id = match &recorded.change {
            WorldChange::Property { instance, .. }
            | WorldChange::Attribute { instance, .. }
            | WorldChange::Tag { instance, .. } => *instance,
            WorldChange::ChildAdded { child, .. } | WorldChange::ChildRemoved { child, .. } => {
                *child
            }
        };
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    ids
}

fn in_game(world: &World, id: InstanceId) -> bool {
    world.is_descendant_of(id, world.root())
}

/// What happened to `ids` between `before` and `after`.
fn diff(before: &World, after: &World, ids: &[InstanceId]) -> Edits {
    let mut edits = Edits::default();
    for &id in ids {
        // Gone for good (destroyed outside the tree), or never in the game.
        if !after.contains(id) || (!in_game(before, id) && !in_game(after, id)) {
            continue;
        }
        let (from, to) = (place_of(before, id), place_of(after, id));
        if from != to {
            edits.moves.push(Move {
                id,
                before: from,
                after: to,
            });
        }
        // Just made: undo takes it out of the tree; its fields don't matter.
        if !before.contains(id) {
            continue;
        }
        let old = snapshot(before, id);
        let changes: Vec<Change> = snapshot(after, id)
            .into_iter()
            .filter_map(|(field, value)| {
                let previous = old.iter().find(|(f, _)| *f == field)?.1.clone();
                (previous != value).then_some(Change {
                    field,
                    before: previous,
                    after: value,
                })
            })
            .collect();
        if !changes.is_empty() {
            edits.fields.push((id, changes));
        }
        let (old, new) = (annotations(before, id), annotations(after, id));
        if old != new {
            edits.annotations.push((id, old, new));
        }
    }
    edits
}

impl History {
    /// Records what Lua did to `ids` (from the world's change log) from
    /// `before` to `after` (`None`: `world` as it is now), as a step named
    /// `label`. With `open` the number of the step still open on top, the
    /// edits merge into it instead. Returns the step's number now, or
    /// `None` when there's no such step (nothing in the game changed, and
    /// none was open).
    ///
    /// Several steps can be recorded one after the other from snapshots
    /// (a plugin's waypoints within one call), as long as the last one
    /// ends at `world`.
    pub fn record_edits(
        &mut self,
        world: &mut World,
        label: String,
        (before, after): (&World, Option<&World>),
        ids: &[InstanceId],
        open: Option<u64>,
    ) -> Option<u64> {
        let edits = diff(before, after.unwrap_or(world), ids);
        let on_top = open.filter(|&number| {
            self.undo
                .last()
                .is_some_and(|top| top.number == number && matches!(top.kind, StepKind::Edits(_)))
        });
        if let Some(open) = on_top {
            if edits.is_empty() {
                return Some(open);
            }
            let number = self.next_number();
            let top = self.undo.last_mut().expect("checked by `on_top`");
            let StepKind::Edits(known) = &mut top.kind else {
                unreachable!("checked by `on_top`");
            };
            known.merge(edits);
            if known.is_empty() {
                self.undo.pop();
                return None;
            }
            top.number = number;
            return Some(number);
        }
        if edits.is_empty() {
            return None;
        }
        self.seal();
        Some(self.push(world, label, StepKind::Edits(edits)))
    }

    /// Names the step `number`, if it's still on top.
    pub fn rename_top(&mut self, number: u64, label: String) {
        if let Some(top) = self.undo.last_mut().filter(|top| top.number == number) {
            top.label = label;
        }
    }
}
