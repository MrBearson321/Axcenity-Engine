//! The project's collision groups: named sets of parts, and which groups
//! collide with each other.
//!
//! Every part is in one group, named by its `CollisionGroup` property.
//! [`DEFAULT_GROUP`] always exists, and a part whose group isn't
//! registered counts as in it. Any two groups collide until they're set
//! not to. The editor's Collision Groups window and scripts'
//! `PhysicsService` change them; the `project` crate saves them as
//! `collision-groups.json`.

use std::collections::BTreeSet;

/// The group every part starts in. It can't be removed or renamed.
pub const DEFAULT_GROUP: &str = "Default";

/// The most groups a project can have, [`DEFAULT_GROUP`] included: physics
/// gives each group one bit of a 32-bit mask.
pub const MAX_COLLISION_GROUPS: usize = 32;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CollisionGroups {
    /// The groups in the order they were registered, [`DEFAULT_GROUP`] first.
    names: Vec<String>,
    /// The pairs of groups that don't collide, each with the smaller name
    /// first. A group can be set not to collide with itself.
    apart: BTreeSet<(String, String)>,
    /// Each group's [`mask`](Self::mask), by index, worked out after every
    /// change: physics reads one for every part every frame.
    masks: Vec<u32>,
}

impl Default for CollisionGroups {
    fn default() -> Self {
        Self {
            names: vec![DEFAULT_GROUP.to_owned()],
            apart: BTreeSet::new(),
            masks: vec![1],
        }
    }
}

/// Both names, the smaller first, so a pair has one spelling.
fn pair(a: &str, b: &str) -> (String, String) {
    if a <= b {
        (a.to_owned(), b.to_owned())
    } else {
        (b.to_owned(), a.to_owned())
    }
}

impl CollisionGroups {
    /// Every group, [`DEFAULT_GROUP`] first.
    pub fn names(&self) -> &[String] {
        &self.names
    }

    pub fn contains(&self, name: &str) -> bool {
        self.names.iter().any(|known| known == name)
    }

    /// The group's place in [`names`](Self::names), or the Default group's
    /// (0) for a name that isn't registered.
    pub fn index(&self, name: &str) -> usize {
        self.names
            .iter()
            .position(|known| known == name)
            .unwrap_or(0)
    }

    /// Adds a group. Registering one that exists does nothing.
    pub fn register(&mut self, name: &str) -> Result<(), String> {
        if self.contains(name) {
            return Ok(());
        }
        check_name(name)?;
        if self.names.len() >= MAX_COLLISION_GROUPS {
            return Err(format!(
                "a project can have at most {MAX_COLLISION_GROUPS} collision groups"
            ));
        }
        self.names.push(name.to_owned());
        self.update_masks();
        Ok(())
    }

    /// Removes a group and its pairs. Parts in it count as in Default.
    pub fn remove(&mut self, name: &str) -> Result<(), String> {
        if name == DEFAULT_GROUP {
            return Err("the Default collision group can't be removed".to_owned());
        }
        let index = self.position(name)?;
        self.names.remove(index);
        self.apart.retain(|(a, b)| a != name && b != name);
        self.update_masks();
        Ok(())
    }

    /// Sets whether parts in `a` collide with parts in `b`.
    pub fn set_collidable(&mut self, a: &str, b: &str, collidable: bool) -> Result<(), String> {
        self.position(a)?;
        self.position(b)?;
        if collidable {
            self.apart.remove(&pair(a, b));
        } else {
            self.apart.insert(pair(a, b));
        }
        self.update_masks();
        Ok(())
    }

    /// True if parts in `a` collide with parts in `b`. Unregistered names
    /// count as Default.
    pub fn collidable(&self, a: &str, b: &str) -> bool {
        let a = if self.contains(a) { a } else { DEFAULT_GROUP };
        let b = if self.contains(b) { b } else { DEFAULT_GROUP };
        !self.apart.contains(&pair(a, b))
    }

    /// The pairs that don't collide, each with the smaller name first.
    pub fn apart(&self) -> impl Iterator<Item = (&str, &str)> {
        self.apart.iter().map(|(a, b)| (a.as_str(), b.as_str()))
    }

    /// One bit per group a part in `name` collides with, by index, for
    /// physics.
    pub fn mask(&self, name: &str) -> u32 {
        self.masks[self.index(name)]
    }

    fn update_masks(&mut self) {
        self.masks = self
            .names
            .iter()
            .map(|own| {
                self.names
                    .iter()
                    .enumerate()
                    .filter(|(_, other)| self.collidable(own, other))
                    .fold(0, |mask, (bit, _)| mask | (1 << bit))
            })
            .collect();
    }

    fn position(&self, name: &str) -> Result<usize, String> {
        self.names
            .iter()
            .position(|known| known == name)
            .ok_or_else(|| format!("there's no collision group named \"{name}\""))
    }
}

fn check_name(name: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("a collision group needs a name".to_owned());
    }
    if name.len() > 100 {
        return Err("a collision group's name can be at most 100 characters".to_owned());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_collide_until_set_apart_and_unknown_names_are_default() {
        let mut groups = CollisionGroups::default();
        groups.register("Players").unwrap();
        groups.register("NPCs").unwrap();
        assert!(groups.collidable("Players", "NPCs"));
        groups.set_collidable("NPCs", "Players", false).unwrap();
        assert!(!groups.collidable("Players", "NPCs"));
        assert!(groups.collidable("Players", "Default"));
        assert!(groups.collidable("Nobody", "Players"));
        assert_eq!(groups.mask("Players"), 0b011);
        assert_eq!(groups.mask("NPCs"), 0b101);
        assert!(groups.set_collidable("Players", "Ghosts", false).is_err());
    }

    #[test]
    fn removing_a_group_drops_its_pairs_and_there_are_at_most_32() {
        let mut groups = CollisionGroups::default();
        groups.register("A").unwrap();
        groups.register("B").unwrap();
        groups.set_collidable("A", "B", false).unwrap();
        groups.remove("A").unwrap();
        assert_eq!(groups.apart().count(), 0);
        assert!(groups.remove(DEFAULT_GROUP).is_err());
        for i in 0..30 {
            groups.register(&format!("G{i}")).unwrap();
        }
        assert!(groups.register("One too many").is_err());
    }
}
