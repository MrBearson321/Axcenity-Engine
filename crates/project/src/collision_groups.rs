//! The collision groups, `collision-groups.json`: the project's groups and
//! which pairs don't collide (see [`CollisionGroups`]). It's its own
//! optional file, so a project without one has only the Default group and
//! [`FORMAT_VERSION`] doesn't change. The editor's Collision Groups window
//! writes it.
//!
//! ```json
//! {
//!   "groups": ["Players", "NPCs"],
//!   "notCollidable": [["NPCs", "Players"]]
//! }
//! ```
//!
//! `groups` leaves out Default, which every project has.

use axcenity_core::{CollisionGroups, DEFAULT_GROUP};

use super::*;

pub const COLLISION_GROUPS_FILE: &str = "collision-groups.json";

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct CollisionGroupsFile {
    #[serde(default)]
    groups: Vec<String>,
    #[serde(default)]
    not_collidable: Vec<(String, String)>,
}

/// Reads collision groups' text. A pair naming a group that isn't listed,
/// or a bad or repeated name, is an error.
pub fn parse_collision_groups(text: &str) -> Result<CollisionGroups, String> {
    let file: CollisionGroupsFile =
        serde_json::from_str(text).map_err(|error| error.to_string())?;
    let mut groups = CollisionGroups::default();
    for name in &file.groups {
        if groups.contains(name) {
            return Err(format!("the group \"{name}\" is listed twice"));
        }
        groups.register(name)?;
    }
    for (a, b) in &file.not_collidable {
        groups.set_collidable(a, b, false)?;
    }
    Ok(groups)
}

/// Collision groups' text, the same for the same groups.
pub fn write_collision_groups(groups: &CollisionGroups) -> String {
    let file = CollisionGroupsFile {
        groups: groups
            .names()
            .iter()
            .filter(|name| *name != DEFAULT_GROUP)
            .cloned()
            .collect(),
        not_collidable: groups
            .apart()
            .map(|(a, b)| (a.to_owned(), b.to_owned()))
            .collect(),
    };
    let mut text = serde_json::to_string_pretty(&file).expect("collision groups are plain strings");
    text.push('\n');
    text
}

/// Gives `world` the groups in `folder`'s `collision-groups.json`. No file
/// is only Default; a damaged one is too, and is added to `problems`.
pub(super) fn load_collision_groups(
    folder: &Path,
    world: &mut World,
    problems: &mut Vec<LoadError>,
) {
    let path = folder.join(COLLISION_GROUPS_FILE);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == ErrorKind::NotFound => return,
        Err(source) => {
            problems.push(LoadError::Read { path, source });
            return;
        }
    };
    match parse_collision_groups(&text) {
        Ok(groups) => world.set_collision_groups(groups),
        Err(message) => problems.push(LoadError::CollisionGroups { path, message }),
    }
}

/// Writes `groups` to `folder`'s `collision-groups.json`, the same way as
/// [`save_world`]. Only Default writes no file, unless there is one
/// already (it's emptied).
pub fn save_collision_groups(folder: &Path, groups: &CollisionGroups) -> Result<(), SaveError> {
    let path = folder.join(COLLISION_GROUPS_FILE);
    if *groups == CollisionGroups::default() && !path.exists() {
        return Ok(());
    }
    write_through_temporary(&path, &write_collision_groups(groups))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An empty folder of its own for one test.
    fn folder(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!(
            "axcenity-collision-groups-{name}-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(&folder).unwrap();
        folder
    }

    #[test]
    fn collision_groups_survive_saving_and_loading() {
        let folder = folder("round-trip");
        let mut groups = CollisionGroups::default();
        groups.register("Players").unwrap();
        groups.register("NPCs").unwrap();
        groups.set_collidable("Players", "NPCs", false).unwrap();
        save_collision_groups(&folder, &groups).unwrap();

        let mut world = World::new();
        let mut problems = Vec::new();
        load_collision_groups(&folder, &mut world, &mut problems);
        assert!(problems.is_empty());
        assert_eq!(*world.collision_groups(), groups);
        let text = fs::read_to_string(folder.join(COLLISION_GROUPS_FILE)).unwrap();
        assert!(text.contains("\"notCollidable\""), "{text}");
    }

    #[test]
    fn only_default_writes_no_file_and_a_bad_pair_is_a_problem() {
        let folder = folder("default");
        save_collision_groups(&folder, &CollisionGroups::default()).unwrap();
        assert!(!folder.join(COLLISION_GROUPS_FILE).exists());

        fs::write(
            folder.join(COLLISION_GROUPS_FILE),
            r#"{ "groups": ["A"], "notCollidable": [["A", "Ghosts"]] }"#,
        )
        .unwrap();
        let mut world = World::new();
        let mut problems = Vec::new();
        load_collision_groups(&folder, &mut world, &mut problems);
        assert_eq!(problems.len(), 1);
        assert_eq!(*world.collision_groups(), CollisionGroups::default());
    }
}
