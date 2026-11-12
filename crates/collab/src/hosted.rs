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
