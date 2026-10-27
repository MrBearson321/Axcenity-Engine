//! Play edits to keep: values changed in Properties during Play that Stop
//! copies into the edit world, instead of throwing them away with the game.
//!
//! Each value edited during Play gets a Keep pin in Properties. An
//! object's right-click **Keep changes to <name>** keeps the whole object:
//! every value of it that differs from the edit world at Stop, whatever
//! changed it (Properties, a script or physics).

use std::collections::BTreeSet;

use axcenity_core::{InstanceId, Value, World};

use crate::history::{self, Field};

#[derive(Debug, Default)]
pub struct PlayEdits {
    /// The fields edited in Properties during Play, in the order first
    /// edited, and whether each is kept.
    edited: Vec<(InstanceId, Field, bool)>,
    /// Objects kept whole with Keep changes to <name>.
    whole: BTreeSet<InstanceId>,
}

/// What Stop copies into one object of the edit world.
#[derive(Debug, PartialEq)]
pub struct Kept {
    pub id: InstanceId,
    pub values: Vec<(Field, Value)>,
}

impl PlayEdits {
    /// Notes that `field` of `id` was edited, not kept until pinned (or
    /// kept already, when the whole object is).
    pub fn edited(&mut self, id: InstanceId, field: Field) {
        if !self.edited.iter().any(|(i, f, _)| *i == id && *f == field) {
            let kept = self.whole.contains(&id);
            self.edited.push((id, field, kept));
        }
    }

    /// The fields of `id` edited during Play, and whether each is kept.
    pub fn fields(&self, id: InstanceId) -> Vec<(Field, bool)> {
        self.edited
            .iter()
            .filter(|(i, _, _)| *i == id)
            .map(|(_, field, kept)| (*field, *kept))
            .collect()
    }

    /// Pins or unpins one edited value. Unpinning one value of an object
    /// kept whole keeps only its pinned values from then on.
    pub fn set_kept(&mut self, id: InstanceId, field: Field, kept: bool) {
        if let Some(entry) = self
            .edited
            .iter_mut()
            .find(|(i, f, _)| *i == id && *f == field)
        {
            entry.2 = kept;
        }
        if !kept {
            self.whole.remove(&id);
        }
    }

    /// Keeps every change to `id` (Keep changes to <name>), and pins its
    /// edited values.
    pub fn keep_whole(&mut self, id: InstanceId) {
        self.whole.insert(id);
        for (i, _, kept) in &mut self.edited {
            if *i == id {
                *kept = true;
            }
        }
    }

    /// The values Stop copies from the `game` world into the `edit` world:
    /// pinned values, and for objects kept whole every value that differs.
    /// Only values that differ from the edit world are listed; objects
    /// made during Play (not in `edit`) are left out.
    pub fn values(&self, game: &World, edit: &World) -> Vec<Kept> {
        let mut ids: Vec<InstanceId> = Vec::new();
        for id in self
            .edited
            .iter()
            .filter(|(_, _, kept)| *kept)
            .map(|(id, _, _)| *id)
            .chain(self.whole.iter().copied())
        {
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
        ids.into_iter()
            .filter(|id| edit.contains(*id))
            .filter_map(|id| {
                let now = history::snapshot(game, id);
                let before = history::snapshot(edit, id);
                let whole = self.whole.contains(&id);
                let values: Vec<(Field, Value)> = now
                    .into_iter()
                    .filter(|(field, value)| {
                        let pinned = whole
                            || self
                                .edited
                                .iter()
                                .any(|(i, f, kept)| *i == id && f == field && *kept);
                        pinned && !before.contains(&(*field, value.clone()))
                    })
                    .collect();
                (!values.is_empty()).then_some(Kept { id, values })
            })
            .collect()
    }

    /// How many kept objects aren't in `edit`, because they were made
    /// during Play.
    pub fn made_in_play(&self, edit: &World) -> usize {
        let mut ids: BTreeSet<InstanceId> = self.whole.clone();
        ids.extend(
            self.edited
                .iter()
                .filter(|(_, _, kept)| *kept)
                .map(|(id, _, _)| *id),
        );
        ids.into_iter().filter(|id| !edit.contains(*id)).count()
    }
}

#[cfg(test)]
mod tests {
    use axcenity_core::{ClassName, Vector3};

    use super::*;

    fn worlds() -> (World, World, InstanceId) {
        let mut edit = World::new();
        let pad = edit.create(ClassName::Part).unwrap();
        edit.set_name(pad, "Pad").unwrap();
        edit.set_parent(pad, Some(edit.workspace())).unwrap();
        let game = edit.clone();
        (edit, game, pad)
    }

    #[test]
    fn only_pinned_values_are_kept() {
        let (edit, mut game, pad) = worlds();
        let up = Value::Vector3(Vector3::new(0.0, 5.0, 0.0));
        game.set_property(pad, "Position", up.clone()).unwrap();
        game.set_property(pad, "Anchored", Value::Bool(false))
            .unwrap();
        let mut edits = PlayEdits::default();
        edits.edited(pad, Field::Property("Position"));
        edits.edited(pad, Field::Property("Anchored"));
        assert!(edits.values(&game, &edit).is_empty());

        edits.set_kept(pad, Field::Property("Position"), true);
        assert_eq!(
            edits.values(&game, &edit),
            [Kept {
                id: pad,
                values: vec![(Field::Property("Position"), up)]
            }]
        );
        assert_eq!(
            edits.fields(pad),
            [
                (Field::Property("Position"), true),
                (Field::Property("Anchored"), false)
            ]
        );
    }

    #[test]
    fn keeping_an_object_keeps_every_value_that_changed() {
        let (edit, mut game, pad) = worlds();
        game.set_name(pad, "Lift").unwrap();
        let up = Value::Vector3(Vector3::new(0.0, 5.0, 0.0));
        game.set_property(pad, "Position", up.clone()).unwrap();
        let mut edits = PlayEdits::default();
        edits.keep_whole(pad);
        edits.edited(pad, Field::Name);
        assert_eq!(edits.fields(pad), [(Field::Name, true)]);
        let kept = edits.values(&game, &edit);
        assert_eq!(kept.len(), 1);
        assert!(
            kept[0]
                .values
                .contains(&(Field::Name, Value::String("Lift".into())))
        );
        assert!(kept[0].values.contains(&(Field::Property("Position"), up)));

        // Unpinning one value goes back to keeping only the pinned ones.
        edits.set_kept(pad, Field::Name, false);
        assert!(edits.values(&game, &edit).is_empty());
    }

    #[test]
    fn objects_made_during_play_are_left_out() {
        let (edit, mut game, _) = worlds();
        let ball = game.create(ClassName::Part).unwrap();
        game.set_parent(ball, Some(game.workspace())).unwrap();
        let mut edits = PlayEdits::default();
        edits.keep_whole(ball);
        assert!(edits.values(&game, &edit).is_empty());
        assert_eq!(edits.made_in_play(&edit), 1);
    }
}
