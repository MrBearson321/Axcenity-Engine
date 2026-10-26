//! UI Event Graphs: `ui/<ScreenGui>.uigraph.json`, each compiled to a
//! LocalScript called `EventGraph` under its ScreenGui (see
//! `axcenity_blocks::ui_graph`). Like block scripts, the Lua is made at
//! load and never saved, so the graph file is the only source.

use axcenity_blocks::node_graph::{NodeGraph, ObjectRoot};
use axcenity_blocks::ui_graph::{
    GraphError, GraphObject, UI_GRAPH_SCRIPT_NAME, UI_GRAPH_SUFFIX, UI_GRAPHS_FOLDER,
    compile_ui_graph, ui_graph_gui_name,
};

use super::*;

/// A UI Event Graph read from the project, with the LocalScript it compiles to.
pub struct UiGraphFile {
    /// Its LocalScript, under the ScreenGui; `None` when the graph
    /// couldn't compile (its errors are in `errors`) or has no ScreenGui.
    pub instance: Option<InstanceId>,
    pub path: String,
    pub graph: NodeGraph,
    /// Why it didn't compile, by node.
    pub errors: Vec<GraphError>,
}

/// The paths of every `ui/*.uigraph.json`, sorted.
pub fn find_ui_graphs(folder: &Path, problems: &mut Vec<LoadError>) -> Vec<String> {
    let dir = folder.join(UI_GRAPHS_FOLDER);
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == ErrorKind::NotFound => return Vec::new(),
        Err(source) => {
            problems.push(LoadError::Read { path: dir, source });
            return Vec::new();
        }
    };
    let mut found: Vec<String> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(UI_GRAPH_SUFFIX))
        .map(|name| format!("{UI_GRAPHS_FOLDER}/{name}"))
        .collect();
    found.sort();
    found
}

pub fn read_ui_graph(folder: &Path, path: &str) -> Result<NodeGraph, LoadError> {
    let full_path = folder.join(path);
    let text = read(&full_path)?;
    serde_json::from_str(&text).map_err(|err| LoadError::UiGraph {
        path: full_path,
        message: err.to_string(),
    })
}

/// Writes `graph` to `path`, through a temporary file like scripts.
pub fn write_ui_graph(folder: &Path, path: &str, graph: &NodeGraph) -> Result<(), SaveError> {
    let mut text = serde_json::to_string_pretty(graph).expect("a graph is plain data");
    text.push('\n');
    write_script(folder, path, &text)
}

/// The Workspace's ScreenGui called `name`, the first in tree order.
pub fn screen_gui_named(world: &World, name: &str) -> Option<InstanceId> {
    world
        .descendants(world.workspace())
        .into_iter()
        .find(|&id| {
            world.get(id).is_some_and(|instance| {
                instance.class() == ClassName::ScreenGui && instance.name() == name
            })
        })
}

/// The objects a graph for `gui` can name: what's in the ScreenGui (and
/// the ScreenGui itself, as an empty path), in the Workspace and in
/// ReplicatedStorage, with their paths of names joined by `/`.
pub fn graph_objects(world: &World, gui: InstanceId) -> Vec<GraphObject> {
    let mut objects = Vec::new();
    for (root, top) in [
        (ObjectRoot::Gui, gui),
        (ObjectRoot::Workspace, world.workspace()),
        (ObjectRoot::ReplicatedStorage, world.replicated_storage()),
    ] {
        if root == ObjectRoot::Gui
            && let Some(instance) = world.get(gui)
        {
            objects.push(GraphObject {
                root,
                path: String::new(),
                class: instance.class().as_str().to_owned(),
            });
        }
        for id in world.descendants(top) {
            let Some(instance) = world.get(id) else {
                continue;
            };
            if instance.class().is_script() {
                continue;
            }
            let mut names = vec![instance.name().to_owned()];
            let mut parent = instance.parent();
            while let Some(above) = parent.filter(|&above| above != top) {
                let Some(above) = world.get(above) else {
                    break;
                };
                names.push(above.name().to_owned());
                parent = above.parent();
            }
            names.reverse();
            objects.push(GraphObject {
                root,
                path: names.join("/"),
                class: instance.class().as_str().to_owned(),
            });
        }
    }
    objects
}

/// Compiles the graph at `path` for its ScreenGui in `world`.
pub fn compile_graph_for(
    world: &World,
    path: &str,
    graph: &NodeGraph,
) -> Result<String, Vec<GraphError>> {
    let name = ui_graph_gui_name(path);
    let Some(gui) = screen_gui_named(world, name) else {
        return Err(vec![GraphError {
            node: None,
            message: format!(
                "There's no ScreenGui called \"{name}\" in the Workspace for this graph."
            ),
        }]);
    };
    compile_ui_graph(graph, &graph_objects(world, gui)).map(|compiled| compiled.source)
}

/// Puts the graph's LocalScript (`EventGraph`) under the ScreenGui `gui`.
pub fn add_graph_script(world: &mut World, gui: InstanceId) -> InstanceId {
    let script = world
        .create(ClassName::LocalScript)
        .expect("a LocalScript is not a service");
    world
        .set_name(script, UI_GRAPH_SCRIPT_NAME)
        .expect("just created");
    world.set_parent(script, Some(gui)).expect("both exist");
    script
}

/// Reads every UI Event Graph and gives each that compiles its
/// LocalScript, after the project's scripts. One that can't be read, or
/// doesn't compile, is in `problems` and doesn't run.
pub(super) fn load_ui_graphs(
    folder: &Path,
    world: &mut World,
    scripts: &mut Vec<ScriptFile>,
    problems: &mut Vec<LoadError>,
) -> Vec<UiGraphFile> {
    let mut graphs = Vec::new();
    for path in find_ui_graphs(folder, problems) {
        let graph = match read_ui_graph(folder, &path) {
            Ok(graph) => graph,
            Err(err) => {
                problems.push(err);
                continue;
            }
        };
        let (instance, errors) = match compile_graph_for(world, &path, &graph) {
            Ok(source) => {
                let gui = screen_gui_named(world, ui_graph_gui_name(&path))
                    .expect("it compiled for this ScreenGui");
                let instance = add_graph_script(world, gui);
                scripts.push(ScriptFile {
                    instance,
                    path: path.clone(),
                    source,
                });
                (Some(instance), Vec::new())
            }
            Err(errors) => {
                problems.push(LoadError::UiGraph {
                    path: folder.join(&path),
                    message: errors
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join("; "),
                });
                (None, errors)
            }
        };
        graphs.push(UiGraphFile {
            instance,
            path,
            graph,
            errors,
        });
    }
    graphs
}
