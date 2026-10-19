//! The asset browser's built-in props: common things made from parts, saved
//! as prefab text (the same layout as `assets/prefabs/*.prefab.json`), so
//! they can be placed in any project without files.
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

pub const PROPS: [Prop; 8] = [
    Prop {
        name: "Floor",
        icon: icons::SQUARE,
        hover: "A 16 × 16 stud anchored floor plate.",
        prefab: FLOOR,
    },
    Prop {
        name: "Wall",
        icon: icons::WALL,
        hover: "A 16 stud wide, 8 stud high anchored wall.",
        prefab: WALL,
    },
    Prop {
        name: "Stairs",
        icon: icons::STEPS,
        hover: "Six steps, each 1 stud up, in a Model.",
        prefab: STAIRS,
    },
    Prop {
        name: "Ramp",
        icon: icons::TRIANGLE,
        hover: "A wedge 4 studs high and 8 long.",
        prefab: RAMP,
    },
    Prop {
        name: "Lamp",
        icon: icons::LAMP,
        hover: "A street lamp with a PointLight in its bulb.",
        prefab: LAMP,
    },
    Prop {
        name: "Table",
        icon: icons::TABLE,
        hover: "A wooden table: a top and four legs, in a Model.",
        prefab: TABLE,
    },
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

const FLOOR: &str = r#"{ "format_version": 1, "instances": [
  { "class": "Part", "name": "Floor", "properties": {
      "Anchored": true, "Size": [16, 1, 16], "Position": [0, 0.5, 0], "Color": [0.39, 0.6, 0.34] } }
] }"#;

const WALL: &str = r#"{ "format_version": 1, "instances": [
  { "class": "Part", "name": "Wall", "properties": {
      "Anchored": true, "Size": [16, 8, 1], "Position": [0, 4, 0], "Color": [0.82, 0.79, 0.72] } }
] }"#;

const STAIRS: &str = r#"{ "format_version": 1, "instances": [
  { "class": "Model", "name": "Stairs", "children": [
    { "class": "Part", "name": "Step", "properties": { "Anchored": true, "Size": [4, 1, 1.5], "Position": [0, 0.5, 0], "Color": [0.6, 0.6, 0.62] } },
    { "class": "Part", "name": "Step", "properties": { "Anchored": true, "Size": [4, 2, 1.5], "Position": [0, 1, -1.5], "Color": [0.6, 0.6, 0.62] } },
    { "class": "Part", "name": "Step", "properties": { "Anchored": true, "Size": [4, 3, 1.5], "Position": [0, 1.5, -3], "Color": [0.6, 0.6, 0.62] } },
    { "class": "Part", "name": "Step", "properties": { "Anchored": true, "Size": [4, 4, 1.5], "Position": [0, 2, -4.5], "Color": [0.6, 0.6, 0.62] } },
    { "class": "Part", "name": "Step", "properties": { "Anchored": true, "Size": [4, 5, 1.5], "Position": [0, 2.5, -6], "Color": [0.6, 0.6, 0.62] } },
    { "class": "Part", "name": "Step", "properties": { "Anchored": true, "Size": [4, 6, 1.5], "Position": [0, 3, -7.5], "Color": [0.6, 0.6, 0.62] } }
  ] }
] }"#;

const RAMP: &str = r#"{ "format_version": 1, "instances": [
  { "class": "Part", "name": "Ramp", "properties": {
      "Anchored": true, "Shape": "Wedge", "Size": [4, 4, 8], "Position": [0, 2, 0], "Color": [0.6, 0.6, 0.62] } }
] }"#;

const LAMP: &str = r#"{ "format_version": 1, "instances": [
  { "class": "Model", "name": "Lamp", "children": [
    { "class": "Part", "name": "Base", "properties": { "Anchored": true, "Size": [1.6, 0.4, 1.6], "Position": [0, 0.2, 0], "Color": [0.17, 0.17, 0.19], "Material": "Metal" } },
    { "class": "Part", "name": "Pole", "properties": { "Anchored": true, "Shape": "Cylinder", "Orientation": [0, 0, 90], "Size": [8, 0.4, 0.4], "Position": [0, 4.4, 0], "Color": [0.17, 0.17, 0.19], "Material": "Metal" } },
    { "class": "Part", "name": "Bulb", "properties": { "Anchored": true, "Shape": "Ball", "Size": [1.2, 1.2, 1.2], "Position": [0, 8.9, 0], "Color": [1, 0.86, 0.55], "Material": "Neon" },
      "children": [ { "class": "PointLight", "name": "PointLight", "properties": { "Color": [1, 0.86, 0.55], "Brightness": 1.5, "Range": 20 } } ] }
  ] }
] }"#;

const TABLE: &str = r#"{ "format_version": 1, "instances": [
  { "class": "Model", "name": "Table", "children": [
    { "class": "Part", "name": "Top", "properties": { "Anchored": true, "Size": [6, 0.4, 4], "Position": [0, 3.2, 0], "Color": [0.55, 0.38, 0.22] } },
    { "class": "Part", "name": "Leg", "properties": { "Anchored": true, "Size": [0.4, 3, 0.4], "Position": [2.6, 1.5, 1.6], "Color": [0.55, 0.38, 0.22] } },
    { "class": "Part", "name": "Leg", "properties": { "Anchored": true, "Size": [0.4, 3, 0.4], "Position": [-2.6, 1.5, 1.6], "Color": [0.55, 0.38, 0.22] } },
    { "class": "Part", "name": "Leg", "properties": { "Anchored": true, "Size": [0.4, 3, 0.4], "Position": [2.6, 1.5, -1.6], "Color": [0.55, 0.38, 0.22] } },
    { "class": "Part", "name": "Leg", "properties": { "Anchored": true, "Size": [0.4, 3, 0.4], "Position": [-2.6, 1.5, -1.6], "Color": [0.55, 0.38, 0.22] } }
  ] }
] }"#;

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
