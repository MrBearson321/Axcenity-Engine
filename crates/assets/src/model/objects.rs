//! The objects inside a model file, and how a `MeshId` names some of them.
//!
//! A model file such as a props pack holds many objects (nodes with a
//! mesh). Each has a path: its name and its parents' names from the top,
//! with `/` between (`Street/Bench`). Two siblings with the same name are
//! told apart by a number in brackets on the second and later ones
//! (`Cube`, `Cube[2]`). Characters with a meaning in paths (`%`, `/`, `,`,
//! `#`, `[`, `]`) are written as `%XX` in a name.
//!
//! A `MeshId` with `#` after the file names objects in it, with `,`
//! between: `assets/models/props.fbx#Street/Bench,Lamp`. Only those
//! objects' own meshes are loaded (not the objects inside them), each where
//! it is in the file. Without `#`, the whole file is loaded.

use std::collections::{HashMap, HashSet};

/// One object in a model file's tree, for choosing what to import. Groups
/// (nodes without a mesh of their own) are listed when something inside
/// them has a mesh, so the tree can be shown as it is in the file.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelObject {
    /// Its path (see the module docs), as it goes in a `MeshId`.
    pub path: String,
    /// Its name in the file, as it's shown.
    pub name: String,
    /// How deep it is: 0 at the top, 1 inside one of those, and so on.
    /// Objects are listed parents first, so the ones right after an object
    /// with a bigger depth are inside it.
    pub depth: usize,
    /// Triangles in its own mesh. 0 for a group.
    pub triangles: usize,
    /// The names of the materials its own mesh uses.
    pub materials: Vec<String>,
}

impl ModelObject {
    /// True if it has a mesh of its own, so importing it makes a MeshPart.
    pub fn has_mesh(&self) -> bool {
        self.triangles > 0
    }
}

/// What a `MeshId` loads from its file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pick {
    /// Every object, as one model.
    Whole,
    /// Only the objects at these paths, as one model.
    Objects(Vec<String>),
}

impl Pick {
    /// Splits a `MeshId` into its file and what it picks from it.
    pub fn of_mesh_id(mesh_id: &str) -> (&str, Pick) {
        match mesh_id.split_once('#') {
            Some((file, objects)) => {
                let paths = objects
                    .split(',')
                    .filter(|path| !path.is_empty())
                    .map(str::to_owned)
                    .collect();
                (file, Pick::Objects(paths))
            }
            None => (mesh_id, Pick::Whole),
        }
    }

    /// The `MeshId` that picks this from the model at `file` (a `MeshId`
    /// without `#`).
    pub fn mesh_id(&self, file: &str) -> String {
        match self {
            Pick::Whole => file.to_owned(),
            Pick::Objects(paths) => format!("{file}#{}", paths.join(",")),
        }
    }

    /// The paths as a set, or `None` for every object.
    pub(super) fn paths(&self) -> Option<HashSet<String>> {
        match self {
            Pick::Whole => None,
            Pick::Objects(paths) => Some(paths.iter().cloned().collect()),
        }
    }
}

/// Gives the children of one parent their path segments, numbering
/// repeated names.
#[derive(Default)]
pub(super) struct Siblings {
    seen: HashMap<String, usize>,
}

impl Siblings {
    /// The path of the next child, named `name`, of the object at `parent`
    /// (`""` at the top).
    pub(super) fn path(&mut self, parent: &str, name: &str) -> String {
        let count = self.seen.entry(name.to_owned()).or_insert(0);
        *count += 1;
        let mut segment = escape(name);
        if *count > 1 {
            segment.push_str(&format!("[{count}]"));
        }
        if parent.is_empty() {
            segment
        } else {
            format!("{parent}/{segment}")
        }
    }
}

/// `name` with the characters that mean something in a path written as `%XX`.
fn escape(name: &str) -> String {
    let mut escaped = String::with_capacity(name.len());
    for c in name.chars() {
        match c {
            '%' | '/' | ',' | '#' | '[' | ']' => escaped.push_str(&format!("%{:02X}", c as u32)),
            c => escaped.push(c),
        }
    }
    escaped
}

/// A node in a file's tree while it's being listed.
pub(super) struct Listed {
    pub(super) object: ModelObject,
    pub(super) children: Vec<Listed>,
}

/// The tree as a flat list, parents first, leaving out groups with no mesh
/// anywhere inside them.
pub(super) fn flatten(nodes: Vec<Listed>) -> Vec<ModelObject> {
    fn walk(node: Listed, into: &mut Vec<ModelObject>) -> bool {
        let at = into.len();
        into.push(node.object);
        let mut any = into[at].has_mesh();
        for child in node.children {
            any |= walk(child, into);
        }
        if !any {
            into.truncate(at);
        }
        any
    }
    let mut objects = Vec::new();
    for node in nodes {
        walk(node, &mut objects);
    }
    objects
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_number_repeated_names_and_escape_special_characters() {
        let mut top = Siblings::default();
        assert_eq!(top.path("", "Street"), "Street");
        assert_eq!(top.path("", "Cube"), "Cube");
        assert_eq!(top.path("", "Cube"), "Cube[2]");
        let mut inside = Siblings::default();
        assert_eq!(inside.path("Street", "Bench/Seat"), "Street/Bench%2FSeat");
        assert_eq!(inside.path("Street", "a,b#c"), "Street/a%2Cb%23c");
    }

    #[test]
    fn mesh_ids_split_into_file_and_objects() {
        assert_eq!(
            Pick::of_mesh_id("assets/models/a.fbx"),
            ("assets/models/a.fbx", Pick::Whole)
        );
        let pick = Pick::Objects(vec!["Street/Bench".into(), "Lamp".into()]);
        let mesh_id = pick.mesh_id("assets/models/a.fbx");
        assert_eq!(mesh_id, "assets/models/a.fbx#Street/Bench,Lamp");
        assert_eq!(Pick::of_mesh_id(&mesh_id), ("assets/models/a.fbx", pick));
    }

    #[test]
    fn groups_without_meshes_are_left_out() {
        let object = |name: &str, depth, triangles| ModelObject {
            path: name.into(),
            name: name.into(),
            depth,
            triangles,
            materials: Vec::new(),
        };
        let tree = vec![
            Listed {
                object: object("Street", 0, 0),
                children: vec![
                    Listed {
                        object: object("Bench", 1, 12),
                        children: Vec::new(),
                    },
                    Listed {
                        object: object("Empty", 1, 0),
                        children: Vec::new(),
                    },
                ],
            },
            Listed {
                object: object("Camera", 0, 0),
                children: Vec::new(),
            },
        ];
        let names: Vec<String> = flatten(tree).into_iter().map(|o| o.name).collect();
        assert_eq!(names, ["Street", "Bench"]);
    }
}
