//! The format 3 → 4 step of a project upgrade: physical properties
//! (Phase 34).
//!
//! Before format 4 every part weighed, gripped and bounced the same
//! (`PhysicalProperties::LEGACY`). Plastic, Glass and Neon still do, but
//! Metal is dense now. So each Metal Part in the world and the prefabs
//! that has no `CustomPhysicalProperties` gets the old values as its own,
//! and the world simulates as it did. Metal parts made after the upgrade
//! use Metal's.

use std::path::Path;

use axcenity_core::{Material, PhysicalProperties};
use serde_json::Value as Json;

/// Runs the step on the project in `folder`.
pub(crate) fn migrate(folder: &Path) -> Result<(), String> {
    crate::migrate_animation::upgrade_world_and_prefabs(folder, keep_old_physics)
}

/// Gives every Part node in `json` whose material's physical properties
/// changed the old ones. Returns true if it changed anything.
fn keep_old_physics(json: &mut Json) -> bool {
    let mut changed = false;
    match json {
        Json::Array(items) => {
            for item in items {
                changed |= keep_old_physics(item);
            }
        }
        Json::Object(fields) => {
            let is_part = fields.get("class").and_then(Json::as_str) == Some("Part");
            if is_part && let Some(Json::Object(properties)) = fields.get_mut("properties") {
                let material = properties
                    .get("Material")
                    .and_then(Json::as_str)
                    .and_then(Material::from_name)
                    .unwrap_or(Material::Plastic);
                if material.physical() != PhysicalProperties::LEGACY
                    && !properties.contains_key("CustomPhysicalProperties")
                {
                    let old = PhysicalProperties::LEGACY;
                    properties.insert(
                        "CustomPhysicalProperties".to_owned(),
                        serde_json::json!({
                            "Density": old.density,
                            "Friction": old.friction,
                            "Elasticity": old.elasticity,
                            "FrictionWeight": old.friction_weight,
                            "ElasticityWeight": old.elasticity_weight,
                        }),
                    );
                    changed = true;
                }
            }
            for value in fields.values_mut() {
                changed |= keep_old_physics(value);
            }
        }
        _ => {}
    }
    changed
}

#[cfg(test)]
mod tests {
    use std::fs;

    use axcenity_core::{Value, World};

    use crate::{MANIFEST_FILE, WORLD_FILE, load};

    use super::*;

    fn part<'w>(world: &'w World, name: &str) -> &'w axcenity_core::Instance {
        let id = world.find_first_child(world.workspace(), name).unwrap();
        world.get(id).unwrap()
    }

    #[test]
    fn an_upgrade_keeps_old_metal_parts_physics_and_leaves_the_rest() {
        let folder =
            std::env::temp_dir().join(format!("axcenity-migrate-physics-{}", std::process::id()));
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(folder.join("assets/prefabs")).unwrap();
        fs::write(
            folder.join(MANIFEST_FILE),
            "{\"name\": \"Old\", \"format_version\": 3}\n",
        )
        .unwrap();
        let world = r#"{
  "format_version": 1,
  "workspace": {
    "children": [
      { "class": "Part", "name": "Ball", "properties": { "Material": "Metal" } },
      { "class": "Part", "name": "Block", "properties": { "Material": "Glass" } },
      { "class": "Part", "name": "Tuned", "properties": {
        "Material": "Metal", "CustomPhysicalProperties": { "Density": 3 } } }
    ]
  }
}
"#;
        fs::write(folder.join(WORLD_FILE), world).unwrap();
        let prefab = r#"{ "format_version": 1, "instances": [
  { "class": "Part", "name": "Rail", "properties": { "Material": "Metal" } }
] }
"#;
        fs::write(folder.join("assets/prefabs/Rail.prefab.json"), prefab).unwrap();

        let project = load(&folder).unwrap();
        let prefab_after = fs::read_to_string(folder.join("assets/prefabs/Rail.prefab.json"));
        let again = load(&folder).unwrap();
        fs::remove_dir_all(&folder).unwrap();

        assert_eq!(project.migrated_from, Some(3));
        assert!(project.problems.is_empty(), "{:?}", project.problems);
        let world = &project.world;
        let custom = |name| {
            part(world, name)
                .property("CustomPhysicalProperties")
                .cloned()
        };
        assert_eq!(
            custom("Ball"),
            Some(Value::PhysicalProperties(Some(PhysicalProperties::LEGACY)))
        );
        // Glass has the old values anyway; a part's own are kept.
        assert_eq!(custom("Block"), Some(Value::PhysicalProperties(None)));
        let tuned = PhysicalProperties {
            density: 3.0,
            ..PhysicalProperties::LEGACY
        };
        assert_eq!(
            custom("Tuned"),
            Some(Value::PhysicalProperties(Some(tuned)))
        );
        assert!(
            prefab_after.unwrap().contains("\"FrictionWeight\": 1"),
            "the prefab's Metal part keeps the old values too"
        );
        // Upgraded once: opening it again changes nothing.
        assert_eq!(again.migrated_from, None);
        assert_eq!(
            again
                .world
                .get(
                    again
                        .world
                        .find_first_child(again.world.workspace(), "Ball")
                        .unwrap()
                )
                .unwrap()
                .property("CustomPhysicalProperties"),
            custom("Ball").as_ref()
        );
    }

    #[test]
    fn a_new_metal_part_is_dense() {
        let mut world = World::new();
        let id = world.create(axcenity_core::ClassName::Part).unwrap();
        world
            .set_property(id, "Material", Value::Material(Material::Metal))
            .unwrap();
        assert_eq!(
            world.get(id).unwrap().property("CustomPhysicalProperties"),
            Some(&Value::PhysicalProperties(None))
        );
        assert!(Material::Metal.physical().density > 7.0);
    }
}
