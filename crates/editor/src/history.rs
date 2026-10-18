//! Undo and redo. Every change to the world (from the Properties panel or a
//! gizmo) is recorded here as a [`Step`] holding each changed value before
//! and after, so undo puts the "before" values back and redo the "after" ones.
//!
//! One gesture is one step: dragging a gizmo or a number, or typing a name,
//! changes the world every frame, and each of those changes merges into the
//! step the gesture started until the app calls [`History::seal`] (when the
//! mouse is let go and no text box is being typed in).
//!
//! Saving doesn't touch the history. It only remembers which step was on top
//! when it saved, so undoing back to that point counts as saved again.

use axcenity_core::{InstanceId, Value, World};

/// Something about an Instance that can be changed: its name or one property.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Name,
    Property(&'static str),
}

/// Every field of an Instance with its value, as recorded before and after a change.
pub type Snapshot = Vec<(Field, Value)>;

/// The fields of `id` and their values right now (empty if it no longer exists).
pub fn snapshot(world: &World, id: InstanceId) -> Snapshot {
    let Some(instance) = world.get(id) else {
        return Vec::new();
    };
    let name = (Field::Name, Value::String(instance.name().to_string()));
    let properties = instance
        .properties()
        .map(|(name, value)| (Field::Property(name), value.clone()));
    std::iter::once(name).chain(properties).collect()
}

/// One field's value before and after a step.
#[derive(Clone, Debug, PartialEq)]
struct Change {
    field: Field,
    before: Value,
    after: Value,
}

/// One undoable action, such as "Move Pad".
#[derive(Debug)]
struct Step {
    /// What the Edit menu calls it.
    label: String,
    id: InstanceId,
    /// The field the gesture is editing. Later changes merge into this step
    /// only if they edit the same field of the same Instance.
    key: Field,
    /// Every field that changed, including side effects (setting `Material`
    /// also sets `Metalness` and `Roughness`).
    changes: Vec<Change>,
    /// A number no other step has had, to tell whether the world is as saved.
    number: u64,
}

#[derive(Debug, Default)]
pub struct History {
    undo: Vec<Step>,
    redo: Vec<Step>,
    /// The number of the step that the current gesture is still adding to.
    open: Option<u64>,
    /// Step numbers start at 1, so 0 stands for "no steps" in `saved_at`.
    last_number: u64,
    /// The number of the top undo step when the world was last saved (or
    /// opened), or 0 if there were no steps then.
    saved_at: u64,
}

impl History {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records that `id` changed from `before` to `after` (snapshots taken
    /// either side of the change). `key` is the field being edited and
    /// `label` names the step, such as "Move Pad". Does nothing if nothing
    /// actually changed.
    pub fn record(
        &mut self,
        id: InstanceId,
        key: Field,
        label: String,
        before: Snapshot,
        after: Snapshot,
    ) {
        let changes = differences(before, after);
        if changes.is_empty() {
            return;
        }
        let merging = self
            .undo
            .last()
            .is_some_and(|top| self.open == Some(top.number) && top.id == id && top.key == key);
        if !merging {
            self.redo.clear();
            let number = self.next_number();
            self.undo.push(Step {
                label,
                id,
                key,
                changes,
                number,
            });
            self.open = Some(number);
            return;
        }

        let number = self.next_number();
        let top = self.undo.last_mut().expect("checked by `merging`");
        for change in changes {
            // Keep the oldest "before" and the newest "after" of each field.
            match top.changes.iter_mut().find(|old| old.field == change.field) {
                Some(old) => old.after = change.after,
                None => top.changes.push(change),
            }
        }
        top.changes.retain(|change| change.before != change.after);
        if top.changes.is_empty() {
            // Dragged back to where it started: there's nothing left to undo.
            self.undo.pop();
            self.open = None;
        } else {
            // A new number, because the world is no longer as it was when
            // this step was last on top.
            top.number = number;
            self.open = Some(number);
        }
    }

    /// Ends the current gesture, so the next change starts a new step.
    pub fn seal(&mut self) {
        self.open = None;
    }

    /// What Undo would undo, such as "Move Pad".
    pub fn undo_label(&self) -> Option<&str> {
        self.undo.last().map(|step| step.label.as_str())
    }

    pub fn redo_label(&self) -> Option<&str> {
        self.redo.last().map(|step| step.label.as_str())
    }

    /// Puts the last step's "before" values back. Returns false if there
    /// was nothing to undo.
    pub fn undo(&mut self, world: &mut World) -> bool {
        self.seal();
        let Some(step) = self.undo.pop() else {
            return false;
        };
        restore(
            world,
            step.id,
            step.changes.iter().map(|c| (c.field, &c.before)),
        );
        self.redo.push(step);
        true
    }

    /// Puts the last undone step's "after" values back. Returns false if
    /// there was nothing to redo.
    pub fn redo(&mut self, world: &mut World) -> bool {
        self.seal();
        let Some(step) = self.redo.pop() else {
            return false;
        };
        restore(
            world,
            step.id,
            step.changes.iter().map(|c| (c.field, &c.after)),
        );
        self.undo.push(step);
        true
    }

    /// Remembers that the world as it is now is what's saved. The history
    /// itself stays, so saving never takes away undo.
    pub fn mark_saved(&mut self) {
        self.seal();
        self.saved_at = self.top_number();
    }

    /// True if the world is the same as when it was last saved (or opened).
    pub fn is_saved(&self) -> bool {
        self.top_number() == self.saved_at
    }

    fn top_number(&self) -> u64 {
        self.undo.last().map_or(0, |step| step.number)
    }

    fn next_number(&mut self) -> u64 {
        self.last_number += 1;
        self.last_number
    }
}

/// The fields whose values differ between two snapshots of one Instance.
fn differences(before: Snapshot, after: Snapshot) -> Vec<Change> {
    after
        .into_iter()
        .filter_map(|(field, after)| {
            let (_, before) = before.iter().find(|(old, _)| *old == field)?;
            (*before != after).then(|| Change {
                field,
                before: before.clone(),
                after,
            })
        })
        .collect()
}

/// Sets each field of `id` to its value. `Material` goes first, because
/// setting it resets `Metalness` and `Roughness`, which come after it and
/// put their own recorded values back. A missing Instance is skipped.
fn restore<'a>(
    world: &mut World,
    id: InstanceId,
    values: impl Iterator<Item = (Field, &'a Value)>,
) {
    // `'a` says the values are borrowed from the step for as long as this
    // call runs; they're cloned into the world one by one.
    let mut values: Vec<(Field, &Value)> = values.collect();
    // `sort_by_key` keeps the order otherwise; `false` sorts before `true`.
    values.sort_by_key(|(_, value)| value.as_material().is_none());
    for (field, value) in values {
        let restored = match (field, value) {
            (Field::Name, Value::String(name)) => world.set_name(id, name.clone()).is_ok(),
            (Field::Property(name), value) => world.set_property(id, name, value.clone()).is_ok(),
            // `snapshot` always records names as strings.
            (Field::Name, _) => false,
        };
        // Only possible once Instances can be deleted: there's nothing left
        // to put the value back on.
        if !restored {
            log::warn!("Could not put back {field:?} on {id:?}");
        }
    }
}

#[cfg(test)]
mod tests {
    use axcenity_core::{ClassName, Material, Vector3};

    use super::*;

    fn world_with_part() -> (World, InstanceId) {
        let mut world = World::new();
        let part = world.create(ClassName::Part).unwrap();
        world.set_parent(part, Some(world.workspace())).unwrap();
        (world, part)
    }

    /// Sets a property and records it, as the app does.
    fn set(
        history: &mut History,
        world: &mut World,
        id: InstanceId,
        name: &'static str,
        value: Value,
    ) {
        let before = snapshot(world, id);
        world.set_property(id, name, value).unwrap();
        let after = snapshot(world, id);
        let field = Field::Property(name);
        history.record(id, field, format!("Change {name}"), before, after);
    }

    fn position(world: &World, id: InstanceId) -> Value {
        world.get(id).unwrap().property("Position").unwrap().clone()
    }

    fn at(x: f32) -> Value {
        Value::Vector3(Vector3::new(x, 0.5, 0.0))
    }

    #[test]
    fn undo_and_redo_put_values_back_and_forth() {
        let (mut world, part) = world_with_part();
        let mut history = History::new();
        let start = position(&world, part);
        set(&mut history, &mut world, part, "Position", at(5.0));
        history.seal();
        assert_eq!(history.undo_label(), Some("Change Position"));

        assert!(history.undo(&mut world));
        assert_eq!(position(&world, part), start);
        assert_eq!(history.undo_label(), None);
        assert_eq!(history.redo_label(), Some("Change Position"));
        assert!(!history.undo(&mut world), "nothing more to undo");

        assert!(history.redo(&mut world));
        assert_eq!(position(&world, part), at(5.0));
        assert!(!history.redo(&mut world), "nothing more to redo");
    }

    #[test]
    fn changes_in_one_gesture_are_one_step() {
        let (mut world, part) = world_with_part();
        let mut history = History::new();
        let start = position(&world, part);
        // A drag: a new position every frame, then the mouse is let go.
        for x in 1..=10 {
            set(&mut history, &mut world, part, "Position", at(x as f32));
        }
        history.seal();
        set(&mut history, &mut world, part, "Position", at(20.0));
        history.seal();

        history.undo(&mut world);
        assert_eq!(position(&world, part), at(10.0));
        history.undo(&mut world);
        assert_eq!(position(&world, part), start);
        assert!(!history.undo(&mut world), "two drags are two steps");
    }

    #[test]
    fn a_gesture_on_another_field_or_part_starts_a_new_step() {
        let (mut world, part) = world_with_part();
        let mut history = History::new();
        set(&mut history, &mut world, part, "Position", at(1.0));
        // No seal between them, but a different field.
        set(
            &mut history,
            &mut world,
            part,
            "Anchored",
            Value::Bool(true),
        );
        history.undo(&mut world);
        assert_eq!(position(&world, part), at(1.0));
        history.undo(&mut world);
        assert_ne!(position(&world, part), at(1.0));
    }

    #[test]
    fn a_drag_that_ends_where_it_started_leaves_nothing_to_undo() {
        let (mut world, part) = world_with_part();
        let mut history = History::new();
        let start = position(&world, part);
        set(&mut history, &mut world, part, "Position", at(3.0));
        set(&mut history, &mut world, part, "Position", start.clone());
        history.seal();
        assert_eq!(history.undo_label(), None);
        assert!(history.is_saved());
    }

    #[test]
    fn a_new_change_after_undo_clears_redo() {
        let (mut world, part) = world_with_part();
        let mut history = History::new();
        set(&mut history, &mut world, part, "Position", at(1.0));
        history.undo(&mut world);
        set(&mut history, &mut world, part, "Position", at(2.0));
        assert_eq!(history.redo_label(), None);
        assert!(!history.redo(&mut world));
    }

    #[test]
    fn undoing_a_material_change_brings_back_metalness_and_roughness() {
        let (mut world, part) = world_with_part();
        let mut history = History::new();
        // A custom roughness, then a material that resets it to its preset.
        set(
            &mut history,
            &mut world,
            part,
            "Roughness",
            Value::Number(0.9),
        );
        history.seal();
        set(
            &mut history,
            &mut world,
            part,
            "Material",
            Value::Material(Material::Metal),
        );
        history.seal();
        let roughness = |world: &World| world.get(part).unwrap().property("Roughness").cloned();
        assert_eq!(roughness(&world), Some(Value::Number(0.3)));

        history.undo(&mut world);
        assert_eq!(roughness(&world), Some(Value::Number(0.9)));
        let material = world.get(part).unwrap().property("Material").cloned();
        assert_eq!(material, Some(Value::Material(Material::Plastic)));
        history.redo(&mut world);
        assert_eq!(roughness(&world), Some(Value::Number(0.3)));
    }

    #[test]
    fn renames_undo_too() {
        let (mut world, part) = world_with_part();
        let mut history = History::new();
        let before = snapshot(&world, part);
        world.set_name(part, "Door").unwrap();
        history.record(
            part,
            Field::Name,
            "Rename".into(),
            before,
            snapshot(&world, part),
        );
        history.undo(&mut world);
        assert_eq!(world.get(part).unwrap().name(), "Part");
    }

    #[test]
    fn saving_keeps_the_history_and_undoing_back_to_it_counts_as_saved() {
        let (mut world, part) = world_with_part();
        let mut history = History::new();
        assert!(history.is_saved());
        set(&mut history, &mut world, part, "Position", at(1.0));
        set(&mut history, &mut world, part, "Position", at(2.0));
        assert!(!history.is_saved());
        history.mark_saved();
        assert!(history.is_saved());
        // A gesture still going when saving doesn't merge into the saved step.
        set(&mut history, &mut world, part, "Position", at(3.0));
        assert!(!history.is_saved());

        assert!(history.undo(&mut world));
        assert_eq!(position(&world, part), at(2.0));
        assert!(history.is_saved());
        assert!(history.undo(&mut world), "saving didn't clear the history");
        assert!(!history.is_saved());
        history.redo(&mut world);
        assert!(history.is_saved());
    }
}
