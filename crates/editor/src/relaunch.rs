//! Relaunching into a rebuilt editor without losing the session.
//!
//! Before it closes, the editor writes a handoff folder in its data folder
//! (never the project): the open project, its unsaved world and script
//! tabs, the layout, the camera and the selection, and during Play the game
//! world as it is. The new editor reads the handoff and deletes it, opens
//! the project with those edits still unsaved, and resumes Play from the
//! saved game world.
//!
//! Instance IDs belong to one process, so the handoff names Instances by
//! [`Locator`]s, and the worlds go as world-file text. The new editor loads
//! the project from disk and then [`morph`]s that world into the saved one.
//! Instances that match keep their IDs, so scripts, tabs and documents that
//! point at them still do, and a resumed game's Instances have the same IDs
//! as the edit world's, as Play's copy always has.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use axcenity_blocks::BlockScript;
use axcenity_core::{ClassName, InstanceId, World};
use serde::{Deserialize, Serialize};

use crate::rebuild::EngineSource;

/// Bumped when the handoff's shape changes. A rebuilt editor only reads
/// one written by the same version.
pub const HANDOFF_VERSION: u32 = 1;
/// The handoff's own file, in its folder.
const HANDOFF_FILE: &str = "handoff.json";
/// The command-line flag that gives a new editor a handoff folder.
pub const RESUME_FLAG: &str = "--resume";

/// One step from the game's root down to an Instance: the child with this
/// class and name, the `nth` of that kind (counting from 0).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Step {
    pub class: String,
    pub name: String,
    pub nth: usize,
}

/// Where an Instance is in its world, the same in any process that loads
/// the same tree.
pub type Locator = Vec<Step>;

/// A camera's position and orientation, as `FlyCamera::view` gives them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct View {
    pub position: [f32; 3],
    pub orientation: [f32; 3],
}

impl View {
    pub fn new(position: axcenity_core::Vector3, orientation: axcenity_core::Vector3) -> View {
        View {
            position: [position.x, position.y, position.z],
            orientation: [orientation.x, orientation.y, orientation.z],
        }
    }

    pub fn position(&self) -> axcenity_core::Vector3 {
        let [x, y, z] = self.position;
        axcenity_core::Vector3::new(x, y, z)
    }

    pub fn orientation(&self) -> axcenity_core::Vector3 {
        let [x, y, z] = self.orientation;
        axcenity_core::Vector3::new(x, y, z)
    }
}

/// A world as world-file text, with each Terrain's landscape in a folder of
/// the handoff (by the Terrain's locator), since the world file doesn't
/// hold landscapes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorldHandoff {
    pub text: String,
    pub terrains: Vec<(Locator, String)>,
}

/// An open script or stylesheet tab.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TabHandoff {
    pub path: String,
    /// Its text, when it has unsaved changes.
    pub unsaved: Option<String>,
}

/// Everything a relaunched editor needs to carry on.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Handoff {
    pub version: u32,
    /// The open project's folder, if one is open.
    pub project: Option<PathBuf>,
    /// The world being edited, when it has unsaved changes.
    pub world: Option<WorldHandoff>,
    pub tabs: Vec<TabHandoff>,
    /// Block scripts with unsaved changes, by path.
    pub block_scripts: Vec<(String, BlockScript)>,
    /// The dock layout, as `Layout::to_text` writes it.
    pub layout: String,
    pub camera: View,
    /// The selection, in the world shown: the game's during Play.
    pub selection: Vec<Locator>,
    /// The game world as it is, during Play.
    pub play: Option<WorldHandoff>,
}

impl Handoff {
    /// Writes the handoff file into `folder`, which must exist.
    pub fn write(&self, folder: &Path) -> std::io::Result<()> {
        let text = serde_json::to_string(self).map_err(std::io::Error::other)?;
        std::fs::write(folder.join(HANDOFF_FILE), text)
    }

    /// Reads the handoff in `folder`. One from another version of the
    /// handoff is refused.
    pub fn read(folder: &Path) -> Result<Handoff, String> {
        let text = std::fs::read_to_string(folder.join(HANDOFF_FILE))
            .map_err(|err| format!("could not read it: {err}"))?;
        let handoff: Handoff =
            serde_json::from_str(&text).map_err(|err| format!("it isn't readable: {err}"))?;
        if handoff.version != HANDOFF_VERSION {
            return Err(format!(
                "it was written by another version of the editor (handoff {}, this one reads {HANDOFF_VERSION})",
                handoff.version
            ));
        }
        Ok(handoff)
    }
}

/// The folder handoffs are written in: the editor's data folder, beside its
/// settings. Never the project.
pub fn handoff_root() -> Option<PathBuf> {
    eframe::storage_dir(crate::APP_ID).map(|data| data.join("Relaunch"))
}

/// A new, empty folder for one handoff in `root`.
pub fn new_handoff_folder(root: &Path) -> std::io::Result<PathBuf> {
    let folder = root.join(format!("handoff-{}", std::process::id()));
    if folder.exists() {
        std::fs::remove_dir_all(&folder)?;
    }
    std::fs::create_dir_all(&folder)?;
    Ok(folder)
}

/// Where `id` is in `world`.
pub fn locate(world: &World, id: InstanceId) -> Option<Locator> {
    let mut steps = Vec::new();
    let mut at = id;
    while at != world.root() {
        let instance = world.get(at)?;
        let parent = instance.parent()?;
        let nth = world
            .get(parent)?
            .children()
            .iter()
            .take_while(|&&sibling| sibling != at)
            .filter(|&&sibling| {
                world.get(sibling).is_some_and(|other| {
                    other.class() == instance.class() && other.name() == instance.name()
                })
            })
            .count();
        steps.push(Step {
            class: instance.class().as_str().to_owned(),
            name: instance.name().to_owned(),
            nth,
        });
        at = parent;
    }
    steps.reverse();
    Some(steps)
}

/// The Instance `locator` names in `world`, if it's there.
pub fn find(world: &World, locator: &[Step]) -> Option<InstanceId> {
    let mut at = world.root();
    for step in locator {
        at = world
            .get(at)?
            .children()
            .iter()
            .copied()
            .filter(|&child| {
                world.get(child).is_some_and(|instance| {
                    instance.class().as_str() == step.class && instance.name() == step.name
                })
            })
            .nth(step.nth)?;
    }
    Some(at)
}

/// The children of `id` a world file holds: not scripts (they're files of
/// their own) and not Players.
fn saved_children(world: &World, id: InstanceId) -> Vec<InstanceId> {
    world.get(id).map_or_else(Vec::new, |instance| {
        instance
            .children()
            .iter()
            .copied()
            .filter(|&child| {
                world.get(child).is_some_and(|child| {
                    !child.class().is_script() && child.class() != ClassName::Player
                })
            })
            .collect()
    })
}

/// Changes `target` to match `source` in everything a world file holds:
/// the Workspace, ReplicatedStorage and ServerStorage (their properties and trees) and
/// Lighting's properties. A child of `target` that matches one of
/// `source`'s (same class and name, in order) stays, with its ID, and takes
/// its properties; the rest are destroyed, and `source`'s other children
/// are made. Scripts and Players aren't in world files, so they stay as
/// they are.
pub fn morph(target: &mut World, source: &World) {
    let services = [
        (target.workspace(), source.workspace()),
        (target.replicated_storage(), source.replicated_storage()),
        (target.server_storage(), source.server_storage()),
    ];
    for (to, from) in services {
        copy_properties(target, to, source, from);
        morph_children(target, to, source, from);
    }
    copy_properties(target, target.lighting(), source, source.lighting());
}

fn morph_children(target: &mut World, to: InstanceId, source: &World, from: InstanceId) {
    let mut unused = saved_children(target, to);
    let mut order = Vec::new();
    for child in saved_children(source, from) {
        let Some(wanted) = source.get(child) else {
            continue;
        };
        let (class, name) = (wanted.class(), wanted.name().to_owned());
        let matched = unused.iter().position(|&id| {
            target
                .get(id)
                .is_some_and(|instance| instance.class() == class && instance.name() == name)
        });
        let id = match matched {
            Some(index) => unused.remove(index),
            None => {
                let Ok(id) = target.create(class) else {
                    continue;
                };
                let _ = target.set_name(id, name);
                let _ = target.set_parent(id, Some(to));
                id
            }
        };
        copy_properties(target, id, source, child);
        morph_children(target, id, source, child);
        order.push(id);
    }
    for id in unused {
        let _ = target.destroy(id);
    }
    // In `source`'s order, ahead of the scripts.
    for (index, id) in order.into_iter().enumerate() {
        let _ = target.set_parent_at(id, to, index);
    }
}

/// Sets every property of `to` to `from`'s. In name order, so `Material`
/// (which resets `Metalness` and `Roughness`) comes before them.
fn copy_properties(target: &mut World, to: InstanceId, source: &World, from: InstanceId) {
    let Some(instance) = source.get(from) else {
        return;
    };
    for (name, value) in instance.properties() {
        let same = target
            .get(to)
            .and_then(|instance| instance.property(name))
            .is_some_and(|current| current == value);
        if !same {
            let _ = target.set_property(to, name, value.clone());
        }
    }
}

/// The world-file text of `world`, with its Terrains' landscapes written
/// into `folder` (as `<name>-<n>`).
pub fn write_world(world: &World, folder: &Path, name: &str) -> std::io::Result<WorldHandoff> {
    let mut terrains = Vec::new();
    for service in [
        world.workspace(),
        world.replicated_storage(),
        world.server_storage(),
    ] {
        for id in world.descendants(service) {
            let (Some(data), Some(locator)) = (world.terrain(id), locate(world, id)) else {
                continue;
            };
            let data_folder = format!("{name}-terrain-{}", terrains.len());
            let disk = folder.join(&data_folder);
            std::fs::create_dir_all(&disk)?;
            for (file, bytes) in axcenity_terrain::files::encode(data) {
                std::fs::write(disk.join(file), bytes)?;
            }
            terrains.push((locator, data_folder));
        }
    }
    Ok(WorldHandoff {
        text: axcenity_project::write_world(world),
        terrains,
    })
}

/// Changes `target` to match the world in `handoff` (see [`morph`]),
/// giving its Terrains the landscapes in `folder`. Problems are returned.
pub fn read_world(target: &mut World, handoff: &WorldHandoff, folder: &Path) -> Vec<String> {
    let mut problems = Vec::new();
    match axcenity_project::parse_world(&handoff.text) {
        Ok(source) => morph(target, &source),
        Err(err) => problems.push(format!("the world isn't readable: {err}")),
    }
    for (locator, data_folder) in &handoff.terrains {
        let Some(id) = find(target, locator) else {
            continue;
        };
        let disk = folder.join(data_folder);
        match axcenity_terrain::files::decode(|file| std::fs::read(disk.join(file)).ok()) {
            Ok(data) => {
                target.set_terrain(id, Arc::new(data));
            }
            Err(err) => problems.push(format!("a terrain isn't readable: {err}")),
        }
    }
    problems
}

/// Starts the editor `source` just built, giving it the handoff in
/// `handoff`. The build's program is copied first (into `run/` beside it),
/// so the next rebuild can write over it while the copy runs; older copies
/// are deleted when they're no longer running.
pub fn launch(source: &EngineSource, handoff: &Path) -> std::io::Result<()> {
    let built = source.built_editor();
    let run = source.target_dir().join("run");
    std::fs::create_dir_all(&run)?;
    if let Ok(entries) = std::fs::read_dir(&run) {
        for entry in entries.flatten() {
            // A copy that's still running can't be deleted on Windows: fine.
            let _ = std::fs::remove_file(entry.path());
        }
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis());
    let copy = run.join(format!(
        "axcenity-editor-{stamp}{}",
        std::env::consts::EXE_SUFFIX
    ));
    std::fs::copy(&built, &copy)?;
    std::process::Command::new(&copy)
        .current_dir(std::env::current_dir().unwrap_or_else(|_| source.workspace.clone()))
        .arg(RESUME_FLAG)
        .arg(handoff)
        .spawn()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axcenity_core::{Value, Vector3};

    fn part(world: &mut World, name: &str, parent: InstanceId) -> InstanceId {
        let id = world.create(ClassName::Part).unwrap();
        world.set_name(id, name).unwrap();
        world.set_parent(id, Some(parent)).unwrap();
        id
    }

    #[test]
    fn a_locator_finds_the_same_instance_among_others_with_its_name() {
        let mut world = World::new();
        let workspace = world.workspace();
        let first = part(&mut world, "Crate", workspace);
        let model = world.create(ClassName::Model).unwrap();
        world.set_parent(model, Some(workspace)).unwrap();
        let second = part(&mut world, "Crate", workspace);
        let inner = part(&mut world, "Crate", model);
        for id in [first, second, inner, workspace] {
            let locator = locate(&world, id).unwrap();
            assert_eq!(find(&world, &locator), Some(id));
        }
        assert_eq!(locate(&world, second).unwrap()[1].nth, 1);
    }

    #[test]
    fn morphing_keeps_matching_instances_and_their_ids_and_leaves_scripts_alone() {
        let mut target = World::new();
        let workspace = target.workspace();
        let floor = part(&mut target, "Floor", workspace);
        let gone = part(&mut target, "Gone", workspace);
        let script = target.create(ClassName::Script).unwrap();
        target
            .set_parent(script, Some(target.server_script_service()))
            .unwrap();

        let mut source = target.clone();
        source
            .set_property(
                floor,
                "Position",
                Value::Vector3(Vector3::new(1.0, 2.0, 3.0)),
            )
            .unwrap();
        source.destroy(gone).unwrap();
        let workspace = source.workspace();
        let new = part(&mut source, "New", workspace);
        let model = source.create(ClassName::Model).unwrap();
        source.set_parent(model, Some(source.workspace())).unwrap();
        source.set_parent(new, Some(model)).unwrap();
        // Through world-file text, so the IDs mean nothing to `target`.
        let source =
            axcenity_project::parse_world(&axcenity_project::write_world(&source)).unwrap();

        morph(&mut target, &source);
        assert_eq!(
            target.get(floor).unwrap().property("Position"),
            Some(&Value::Vector3(Vector3::new(1.0, 2.0, 3.0)))
        );
        assert!(target.get(gone).is_none());
        assert!(target.get(script).is_some());
        let step = |class: &str, name: &str| Step {
            class: class.into(),
            name: name.into(),
            nth: 0,
        };
        let new = [
            step("Workspace", "Workspace"),
            step("Model", "Model"),
            step("Part", "New"),
        ];
        assert!(find(&target, &new).is_some());
        assert_eq!(
            axcenity_project::write_world(&target),
            axcenity_project::write_world(&source)
        );
    }
}
