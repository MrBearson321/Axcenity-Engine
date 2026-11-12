//! A project hosted for a session, named and started the same way by the
//! hosting editor and by the self-hosted server (TC7), so a member can't
//! tell which one hosts: the session's name, where its members are kept,
//! and the World it starts from.

use std::path::{Path, PathBuf};

use axcenity_core::spline::is_generator;
use axcenity_core::{InstanceId, World};

use crate::identity::Fingerprint;

/// Names a hosted project the same way every time it's hosted from this
/// computer: its host certificate and its folder. Members keep their copy
/// of its files in a folder of this name.
pub fn session_id(cert: &Fingerprint, project: &Path) -> String {
    let folder = std::path::absolute(project).unwrap_or_else(|_| project.to_owned());
    let mut bytes = cert.0.to_vec();
    bytes.extend(folder.to_string_lossy().as_bytes());
    Fingerprint::of(&bytes).to_hex()[..16].to_owned()
}

/// The members file of the project in `project`, in the host's data
/// folder `folder` (never the project's), named by a hash of its path.
pub fn members_file(folder: &Path, project: &Path) -> PathBuf {
    let project = project
        .canonicalize()
        .unwrap_or_else(|_| project.to_owned());
    let hash = Fingerprint::of(project.to_string_lossy().as_bytes()).to_hex();
    folder
        .join("projects")
        .join(format!("{}.json", &hash[..16]))
}

/// `world` as a session starts from: without what SplineMeshes and
/// SplineScatters made (every copy makes its own).
pub fn session_world(mut world: World) -> World {
    world.stop_recording_edits();
    let generated: Vec<InstanceId> = world
        .descendants(world.root())
        .into_iter()
        .filter(|&id| world.get(id).is_some_and(|i| is_generator(i.class())))
        .flat_map(|id| {
            world
                .get(id)
                .map(|i| i.children().to_vec())
                .unwrap_or_default()
        })
        .collect();
    for id in generated {
        let _ = world.destroy(id);
    }
    world
}

/// The folder `name` inside `parent`, taking over the folder of the
/// feature's old name (`old`, from when Collaborate was called Team
/// Create) the first time, so identities, host certificates and members
/// lists carry over. When the old folder can't be moved it's used as it is.
pub fn renamed_folder(parent: &Path, name: &str, old: &str) -> PathBuf {
    let new = parent.join(name);
    let previous = parent.join(old);
    if new.exists() || !previous.is_dir() {
        return new;
    }
    match std::fs::rename(&previous, &new) {
        Ok(()) => new,
        Err(error) => {
            log::warn!(
                "Collaborate: couldn't rename `{}` to `{}` ({error}); using it as it is",
                previous.display(),
                new.display()
            );
            previous
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "axcenity-collab-renamed-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn the_old_folder_is_taken_over_once() {
        let parent = temp("old");
        std::fs::create_dir(parent.join("Team Create")).unwrap();
        std::fs::write(parent.join("Team Create").join("identity.json"), "{}").unwrap();
        let folder = renamed_folder(&parent, "Collaborate", "Team Create");
        assert_eq!(folder, parent.join("Collaborate"));
        assert!(folder.join("identity.json").exists());
        assert!(!parent.join("Team Create").exists());
        // Later calls just use it.
        assert_eq!(
            renamed_folder(&parent, "Collaborate", "Team Create"),
            folder
        );
        std::fs::remove_dir_all(parent).unwrap();
    }

    #[test]
    fn a_new_folder_wins_over_an_old_one() {
        let parent = temp("both");
        std::fs::create_dir(parent.join("Team Create")).unwrap();
        std::fs::create_dir(parent.join("Collaborate")).unwrap();
        assert_eq!(
            renamed_folder(&parent, "Collaborate", "Team Create"),
            parent.join("Collaborate")
        );
        assert!(parent.join("Team Create").exists(), "left alone");
        std::fs::remove_dir_all(parent).unwrap();
    }

    #[test]
    fn with_neither_folder_the_new_name_is_used() {
        let parent = temp("none");
        assert_eq!(
            renamed_folder(&parent, "Collaborate", "Team Create"),
            parent.join("Collaborate")
        );
        std::fs::remove_dir_all(parent).unwrap();
    }
}
