//! The ribbon's Solid Modeling group (Phase 50): Union, Negate, Intersect
//! and Separate, each one undo step that keeps the whole world
//! (`History::record_world`).
//!
//! Union and Intersect combine the selected parts with `mesh_edit` and save
//! the result as `assets/models/<name>.union.glb` (the file stays when the
//! step is undone, like a Fracture's pieces). The new UnionOperation takes
//! the first part's place, and the parts go inside it, out of the world,
//! for Separate. A boolean that fails (a part that isn't closed, nothing
//! left) says why in Output and changes nothing.

use axcenity_core::{ClassName, InstanceId, Value, World};
use axcenity_mesh_edit::Operation;

use super::EditorApp;
use crate::history::{self, Place};
use crate::theme::icons;
use crate::tree;

/// One of the Solid Modeling buttons.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SolidAction {
    Union,
    Negate,
    Intersect,
    Separate,
}

impl SolidAction {
    pub const ALL: [SolidAction; 4] = [
        SolidAction::Union,
        SolidAction::Negate,
        SolidAction::Intersect,
        SolidAction::Separate,
    ];

    pub fn label(self) -> &'static str {
        match self {
            SolidAction::Union => "Union",
            SolidAction::Negate => "Negate",
            SolidAction::Intersect => "Intersect",
            SolidAction::Separate => "Separate",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            SolidAction::Union => icons::UNITE,
            SolidAction::Negate => icons::SUBTRACT,
            SolidAction::Intersect => icons::INTERSECT_SQUARE,
            SolidAction::Separate => icons::ARROWS_SPLIT,
        }
    }

    pub fn hover(self) -> &'static str {
        match self {
            SolidAction::Union => {
                "Join the selected parts into one solid, a UnionOperation. Negated parts are cut \
                 out of it. Separate gives the parts back."
            }
            SolidAction::Negate => {
                "Make the selected Parts negative (pink and see-through): unioned with other \
                 parts, they cut themselves out. Negate again to turn them back."
            }
            SolidAction::Intersect => {
                "Keep only where the selected parts all overlap, as a UnionOperation. Negated \
                 parts are cut out of it."
            }
            SolidAction::Separate => {
                "Break the selected UnionOperations back into the parts they were made of."
            }
        }
    }

    /// What has to be selected for it.
    pub fn needs(self) -> &'static str {
        match self {
            SolidAction::Union | SolidAction::Intersect => "Select two or more parts.",
            SolidAction::Negate => "Select a Part or a negated part.",
            SolidAction::Separate => "Select a UnionOperation.",
        }
    }

    /// Whether the button does something for `parts` (the selected parts).
    pub fn usable(self, world: &World, parts: &[InstanceId]) -> bool {
        let class = |id: InstanceId| world.get(id).map(|instance| instance.class());
        match self {
            SolidAction::Union | SolidAction::Intersect => parts.len() >= 2,
            SolidAction::Negate => parts.iter().any(|&id| {
                matches!(
                    class(id),
                    Some(ClassName::Part | ClassName::NegateOperation)
                )
            }),
            SolidAction::Separate => parts
                .iter()
                .any(|&id| class(id) == Some(ClassName::UnionOperation)),
        }
    }
}

impl EditorApp {
    /// The selected parts (and the parts in selected Models), leaving out
    /// the ones kept inside a UnionOperation.
    pub(in crate::app) fn solid_parts(&self) -> Vec<InstanceId> {
        self.tool_parts()
            .into_iter()
            .filter(|&id| {
                self.world
                    .get(id)
                    .is_some_and(|instance| instance.class().is_base_part())
                    && !axcenity_core::csg::in_union(&self.world, id)
            })
            .collect()
    }

    pub(in crate::app) fn solid_action(&mut self, action: SolidAction) {
        if self.playing() || self.viewport.is_dragging() {
            return;
        }
        match action {
            SolidAction::Union => self.combine_selected(Operation::Union),
            SolidAction::Intersect => self.combine_selected(Operation::Intersect),
            SolidAction::Negate => self.negate_selected(),
            SolidAction::Separate => self.separate_selected(),
        }
    }

    /// Union or Intersect the selected parts into a new UnionOperation.
    fn combine_selected(&mut self, operation: Operation) {
        let verb = match operation {
            Operation::Intersect => "intersect",
            _ => "union",
        };
        let parts = self.solid_parts();
        if parts.len() < 2 {
            self.output
                .warning(format!("Select two or more parts to {verb}."));
            return;
        }
        let Some(folder) = self.project.as_ref().map(|project| project.folder.clone()) else {
            self.output
                .warning("Open or save a project first: the solid is saved in it.");
            return;
        };
        let combined =
            match axcenity_mesh_edit::combine_parts(&self.world, &self.assets, &parts, operation) {
                Ok(combined) => combined,
                Err(problem) => {
                    self.output.warning(format!(
                        "Could not {verb} the parts: {problem}. Nothing was changed."
                    ));
                    return;
                }
            };
        let first = parts[0];
        let Some(place) = history::place_of(&self.world, first) else {
            return;
        };
        let name = tree::unique_name(&self.world, place.parent, "Union");
        let path = match axcenity_mesh_edit::save_union(&folder, &self.assets, &name, &combined) {
            Ok(path) => path,
            Err(problem) => {
                self.output.error(problem);
                return;
            }
        };

        let before = self.world.clone();
        let union = self
            .world
            .create(ClassName::UnionOperation)
            .expect("a UnionOperation can be made");
        let _ = self.world.set_name(union, name);
        let size = combined.size;
        let position = combined.position;
        let mut properties = vec![
            ("MeshId", Value::String(path.clone())),
            (
                "Position",
                Value::Vector3(axcenity_core::Vector3::new(
                    position.x, position.y, position.z,
                )),
            ),
            ("Orientation", Value::Vector3(combined.orientation())),
            (
                "Size",
                Value::Vector3(axcenity_core::Vector3::new(size.x, size.y, size.z)),
            ),
        ];
        // The rest as the first part has them.
        if let Some(instance) = self.world.get(first) {
            for name in [
                "Anchored",
                "Color",
                "CastShadow",
                "Transparency",
                "CanCollide",
                "CanTouch",
                "CanQuery",
                "CollisionGroup",
                "CustomPhysicalProperties",
            ] {
                if let Some(value) = instance.property(name) {
                    properties.push((name, value.clone()));
                }
            }
        }
        for (name, value) in properties {
            let _ = self.world.set_property(union, name, value);
        }
        let _ = history::put(&mut self.world, union, Some(place));
        for &part in &parts {
            let _ = self.world.set_parent(part, Some(union));
        }
        let label = format!("{} {} parts", capitalized(verb), parts.len());
        self.history.record_world(&mut self.world, label, before);
        self.select_from_viewport(Some(union));
        self.output.info(format!("Saved the solid in {path}."));
    }

    /// Turns each selected Part into a NegateOperation, and each negated
    /// part back into a Part.
    fn negate_selected(&mut self) {
        let parts: Vec<InstanceId> = self
            .solid_parts()
            .into_iter()
            .filter(|&id| {
                matches!(
                    self.world.get(id).map(|instance| instance.class()),
                    Some(ClassName::Part | ClassName::NegateOperation)
                )
            })
            .collect();
        if parts.is_empty() {
            self.output
                .warning("Select a Part or a negated part to negate.");
            return;
        }
        let before = self.world.clone();
        let mut made = Vec::new();
        for part in parts {
            let class = match self.world.get(part).map(|instance| instance.class()) {
                Some(ClassName::NegateOperation) => ClassName::Part,
                _ => ClassName::NegateOperation,
            };
            if let Some(copy) = replace_class(&mut self.world, part, class) {
                made.push(copy);
            }
        }
        let label = format!("Negate {} parts", made.len());
        self.history.record_world(&mut self.world, label, before);
        self.select_many(made);
    }

    /// Puts each selected UnionOperation's parts back where it is, and
    /// takes it out.
    fn separate_selected(&mut self) {
        let unions: Vec<InstanceId> = self
            .solid_parts()
            .into_iter()
            .filter(|&id| {
                self.world.get(id).map(|instance| instance.class())
                    == Some(ClassName::UnionOperation)
            })
            .collect();
        if unions.is_empty() {
            self.output.warning("Select a UnionOperation to separate.");
            return;
        }
        let before = self.world.clone();
        let mut parts = Vec::new();
        for union in unions {
            let Some(place) = history::place_of(&self.world, union) else {
                continue;
            };
            let inside = self
                .world
                .get(union)
                .map(|instance| instance.children().to_vec())
                .unwrap_or_default();
            for (offset, child) in inside.into_iter().enumerate() {
                let at = Place {
                    parent: place.parent,
                    index: place.index + 1 + offset,
                };
                if history::put(&mut self.world, child, Some(at)).is_ok() {
                    parts.push(child);
                }
            }
            let _ = self.world.set_parent(union, None);
        }
        let label = format!("Separate into {} parts", parts.len());
        self.history.record_world(&mut self.world, label, before);
        self.select_many(parts);
    }
}

fn capitalized(word: &str) -> String {
    let mut chars = word.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// Puts a `class` copy of `id` in its place: the same name, the properties
/// both classes have, attributes, tags and children. `id` leaves the tree.
fn replace_class(world: &mut World, id: InstanceId, class: ClassName) -> Option<InstanceId> {
    let place = history::place_of(world, id)?;
    let copy = world.create(class).ok()?;
    let instance = world.get(id)?;
    let name = instance.name().to_owned();
    let properties: Vec<(&'static str, Value)> = instance
        .properties()
        .map(|(name, value)| (name, value.clone()))
        .collect();
    let attributes = instance.attributes().clone();
    let tags = instance.tags().to_vec();
    let children = instance.children().to_vec();
    let _ = world.set_name(copy, name);
    for (name, value) in properties {
        let _ = world.set_property(copy, name, value);
    }
    for (name, value) in attributes {
        let _ = world.set_attribute(copy, &name, Some(value));
    }
    for tag in tags {
        let _ = world.add_tag(copy, &tag);
    }
    for child in children {
        let _ = world.set_parent(child, Some(copy));
    }
    world.set_parent(id, None).ok()?;
    history::put(world, copy, Some(place)).ok()?;
    Some(copy)
}
