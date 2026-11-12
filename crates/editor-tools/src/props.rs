//! The asset browser's built-in props: common things made from parts, saved
//! as prefab text (the same layout as `assets/prefabs/*.prefab.json`), so
//! they can be placed in any project without files.
//!
//! Floor, Wall, Stairs, Ramp, Lamp and Table were taken out on request: each
//! was a part or two anyone builds faster from Insert, and they crowded the
//! browser. A world that already has one keeps it: placing a prop copies
//! its parts in, nothing points back here.
//!
//! Positions only say where each part is relative to the others: placing a
//! prop moves it, as a whole, onto the surface it's dropped on.

use crate::theme::icons;

/// A built-in prop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Prop {
    pub name: &'static str,
    pub icon: &'static str,
    pub hover: &'static str,
    /// Prefab file text.
    pub prefab: &'static str,
}

pub const PROPS: [Prop; 2] = [
    Prop {
        name: "Tree",
        icon: icons::TREE,
        hover: "A simple tree: a trunk and a ball of leaves.",
        prefab: TREE,
    },
    Prop {
        name: "Crate",
        icon: icons::PACKAGE,
        hover: "A 3 stud wooden crate.",
        prefab: CRATE,
    },
];

const TREE: &str = r#"{ "format_version": 1, "instances": [
  { "class": "Model", "name": "Tree", "children": [
    { "class": "Part", "name": "Trunk", "properties": { "Anchored": true, "Shape": "Cylinder", "Orientation": [0, 0, 90], "Size": [6, 1.2, 1.2], "Position": [0, 3, 0], "Color": [0.45, 0.3, 0.18] } },
    { "class": "Part", "name": "Leaves", "properties": { "Anchored": true, "Shape": "Ball", "Size": [6, 6, 6], "Position": [0, 7.5, 0], "Color": [0.29, 0.56, 0.25], "Roughness": 0.8 } }
  ] }
] }"#;

const CRATE: &str = r#"{ "format_version": 1, "instances": [
  { "class": "Part", "name": "Crate", "properties": {
      "Anchored": true, "Size": [3, 3, 3], "Position": [0, 1.5, 0], "Color": [0.66, 0.48, 0.28], "Roughness": 0.8 } }
] }"#;

#[cfg(test)]
mod tests {
    use axcenity_core::World;

    use super::PROPS;

    /// The base models taken out stay out; Tree and Crate stay in.
    #[test]
    fn only_the_tree_and_crate_are_built_in() {
        let names: Vec<&str> = PROPS.iter().map(|prop| prop.name).collect();
        assert_eq!(names, ["Tree", "Crate"]);
    }

    #[test]
    fn every_prop_is_a_valid_prefab() {
        for prop in PROPS {
            let mut world = World::new();
            let workspace = world.workspace();
            let added = axcenity_project::parse_prefab(prop.prefab, &mut world, workspace)
                .unwrap_or_else(|err| panic!("{}: {err}", prop.name));
            assert!(!added.is_empty(), "{} has no Instances", prop.name);
        }
    }
}
