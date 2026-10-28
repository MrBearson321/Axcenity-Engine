//! `StarterPlayer.StarterCharacter` (Phase 47): the prefab each player's
//! character is a copy of. Hosts read it into StarterPlayer, as a Model
//! named [`STARTER_CHARACTER`], before Play's scripts start; the script
//! runtime copies that Model for each spawn.

use std::path::Path;

use axcenity_core::{ClassName, InstanceId, STARTER_CHARACTER, Value, World};

/// Reads the prefab `StarterPlayer.StarterCharacter` names (a path in the
/// project `folder`) into StarterPlayer as a Model named
/// [`STARTER_CHARACTER`]: the prefab's Model itself, or a new Model
/// holding what it has when that's not one Model. `Ok(None)` when the
/// property is empty (the default character is used). `Err` says why the
/// prefab couldn't be read; the world is left as it was.
pub fn load_starter_character(
    folder: &Path,
    world: &mut World,
) -> Result<Option<InstanceId>, String> {
    let starter_player = world.starter_player();
    let path = world
        .get(starter_player)
        .and_then(|instance| instance.property("StarterCharacter"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_owned();
    if path.is_empty() {
        return Ok(None);
    }
    let roots = crate::Prefabs::in_folder(folder)
        .instantiate(&path, world, starter_player)
        .map_err(|err| match err {
            crate::PrefabError::Read { message, .. } => {
                format!("StarterCharacter `{path}` could not be read: {message}")
            }
            err => format!("StarterCharacter `{path}` is not a prefab: {err}"),
        })?;
    // Characters are copies made for Play, not linked copies to save.
    for &root in &roots {
        let _ = world.set_prefab(root, None);
    }
    let model = match roots.as_slice() {
        [model]
            if world
                .get(*model)
                .is_some_and(|instance| instance.class() == ClassName::Model) =>
        {
            *model
        }
        _ => {
            let model = world.create(ClassName::Model).expect("not a service");
            world
                .set_parent(model, Some(starter_player))
                .expect("both exist");
            for root in roots {
                world.set_parent(root, Some(model)).expect("both exist");
            }
            model
        }
    };
    world.set_name(model, STARTER_CHARACTER).expect("just made");
    Ok(Some(model))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_folder(name: &str) -> std::path::PathBuf {
        let folder = std::env::temp_dir().join(format!(
            "axcenity-starter-character-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(folder.join("assets/prefabs")).unwrap();
        folder
    }

    fn set_starter(world: &mut World, path: &str) {
        let starter = world.starter_player();
        world
            .set_property(starter, "StarterCharacter", Value::String(path.to_owned()))
            .unwrap();
    }

    #[test]
    fn the_prefab_goes_in_starter_player_as_starter_character() {
        let folder = temp_folder("model");
        std::fs::write(
            folder.join("assets/prefabs/Hero.prefab.json"),
            r#"{ "format_version": 1, "instances": [
                 { "class": "Model", "name": "Hero", "children": [
                   { "class": "Part", "name": "Body" },
                   { "class": "Humanoid" } ] } ] }"#,
        )
        .unwrap();
        let mut world = World::new();
        assert_eq!(load_starter_character(&folder, &mut world), Ok(None));
        set_starter(&mut world, "assets/prefabs/Hero.prefab.json");
        let model = load_starter_character(&folder, &mut world)
            .unwrap()
            .unwrap();
        let instance = world.get(model).unwrap();
        assert_eq!(instance.name(), STARTER_CHARACTER);
        assert_eq!(instance.parent(), Some(world.starter_player()));
        assert!(world.find_first_child(model, "Body").is_some());

        set_starter(&mut world, "assets/prefabs/Missing.prefab.json");
        let err = load_starter_character(&folder, &mut world).unwrap_err();
        assert!(err.contains("Missing.prefab.json"), "{err}");
        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn a_prefab_of_loose_parts_is_put_in_a_model() {
        let folder = temp_folder("parts");
        std::fs::write(
            folder.join("assets/prefabs/Blob.prefab.json"),
            r#"{ "format_version": 1, "instances": [
                 { "class": "Part", "name": "A" }, { "class": "Part", "name": "B" } ] }"#,
        )
        .unwrap();
        let mut world = World::new();
        set_starter(&mut world, "assets/prefabs/Blob.prefab.json");
        let model = load_starter_character(&folder, &mut world)
            .unwrap()
            .unwrap();
        assert_eq!(world.get(model).unwrap().class(), ClassName::Model);
        assert_eq!(world.get(model).unwrap().children().len(), 2);
        let _ = std::fs::remove_dir_all(folder);
    }
}
