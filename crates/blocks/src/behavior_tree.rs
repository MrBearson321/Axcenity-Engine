//! Behavior trees: the nodes a NavigationAgent's AI graph offers, for the
//! generic graph editor.
//!
//! A tree is its own asset, `assets/ai/<Name>.behaviortree.json`, in the
//! same file shape as any [`NodeGraph`]. Wires go from a parent's child
//! pin to a child's plain input, so the white exec pins are the tree's
//! branches. What each node does, and checking a tree, belong to the `nav`
//! crate, which runs trees during Play (`nav::BehaviorTree`); the kinds,
//! fields and pins here must match it.

use crate::node_graph::*;

/// Where new behavior trees go.
pub const BEHAVIOR_TREES_FOLDER: &str = "assets/ai";
/// A behavior tree's file ending.
pub const BEHAVIOR_TREE_SUFFIX: &str = ".behaviortree.json";

/// The project path of the behavior tree called `name`.
pub fn behavior_tree_path(name: &str) -> String {
    format!("{BEHAVIOR_TREES_FOLDER}/{name}{BEHAVIOR_TREE_SUFFIX}")
}

/// Whether `path` is a behavior tree's file.
pub fn is_behavior_tree(path: &str) -> bool {
    path.to_lowercase().ends_with(BEHAVIOR_TREE_SUFFIX)
}

/// A tree's name: its file's name without the ending.
pub fn behavior_tree_name(path: &str) -> &str {
    let file = path.rsplit('/').next().unwrap_or(path);
    file.get(..file.len().saturating_sub(BEHAVIOR_TREE_SUFFIX.len()))
        .filter(|_| is_behavior_tree(file))
        .unwrap_or(file)
}

const fn number(name: &'static str, default: &'static str) -> FieldSpec {
    FieldSpec {
        name,
        kind: FieldKind::Text,
        default,
    }
}

const IN: PinSpec = PinSpec::exec("");
/// A composite's children, tried in this order.
const CHILDREN: &[PinSpec] = &[
    PinSpec::exec("1"),
    PinSpec::exec("2"),
    PinSpec::exec("3"),
    PinSpec::exec("4"),
];
const POINT: &[FieldSpec] = &[number("X", "0"), number("Y", "0"), number("Z", "0")];

/// Every node a behavior tree offers, in the node menu's order.
pub const BEHAVIOR_NODES: &[NodeSpec] = &[
    NodeSpec {
        kind: "root",
        title: "Root",
        category: "Root",
        help: "Where the tree starts. Every frame of Play, it runs what's wired to it.",
        fields: &[],
        inputs: &[],
        outputs: &[PinSpec::exec("")],
    },
    NodeSpec {
        kind: "selector",
        title: "Selector",
        category: "Composites",
        help: "Tries 1, 2, 3 and 4 in turn, from 1 every frame, and stops at the first that \
               succeeds or is still running. Fails when they all fail. So an earlier branch \
               that starts to succeed (Sees Player?) takes over from a later one.",
        fields: &[],
        inputs: &[IN],
        outputs: CHILDREN,
    },
    NodeSpec {
        kind: "sequence",
        title: "Sequence",
        category: "Composites",
        help: "Runs 1, 2, 3 and 4 in turn, each once the one before succeeds, and succeeds \
               when they all have. Fails at the first that fails. Its conditions (the ? \
               nodes) are checked again every frame, so one turning false stops it.",
        fields: &[],
        inputs: &[IN],
        outputs: CHILDREN,
    },
    NodeSpec {
        kind: "sees_player",
        title: "Sees Player?",
        category: "Conditions",
        help: "Succeeds while the agent sees the player character (its SeesPlayer), and fails \
               otherwise.",
        fields: &[],
        inputs: &[IN],
        outputs: &[],
    },
    NodeSpec {
        kind: "hears_player",
        title: "Hears Player?",
        category: "Conditions",
        help: "Succeeds while the agent hears a Sound playing in the player character (its \
               HearsPlayer), and fails otherwise.",
        fields: &[],
        inputs: &[IN],
        outputs: &[],
    },
    NodeSpec {
        kind: "chase_player",
        title: "Chase Player",
        category: "Actions",
        help: "Walks to the player character, following them as they move. Succeeds once it's \
               beside them; fails when there's no player character or no way to them.",
        fields: &[],
        inputs: &[IN],
        outputs: &[],
    },
    NodeSpec {
        kind: "move_to",
        title: "Move To",
        category: "Actions",
        help: "Sets the agent's Destination to this point and walks there. Succeeds when it \
               arrives; fails when it can't reach it or is blocked.",
        fields: POINT,
        inputs: &[IN],
        outputs: &[],
    },
    NodeSpec {
        kind: "set_destination",
        title: "Set Destination",
        category: "Actions",
        help: "Sets the agent's Destination to this point and succeeds at once, without \
               waiting for it to get there.",
        fields: POINT,
        inputs: &[IN],
        outputs: &[],
    },
    NodeSpec {
        kind: "wait",
        title: "Wait",
        category: "Actions",
        help: "Keeps running for this many seconds, then succeeds.",
        fields: &[number("Seconds", "1")],
        inputs: &[IN],
        outputs: &[],
    },
];

/// The behavior tree's nodes, as a library for the graph editor.
pub struct BehaviorNodes;

impl NodeLibrary for BehaviorNodes {
    fn specs(&self) -> &[NodeSpec] {
        BEHAVIOR_NODES
    }
}

/// A new tree: a Root and nothing under it yet.
pub fn new_behavior_tree() -> NodeGraph {
    let mut graph = NodeGraph::default();
    graph.add_node(&BEHAVIOR_NODES[0], [40.0, 120.0]);
    graph
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tree_is_named_after_its_file() {
        let path = behavior_tree_path("Guard");
        assert_eq!(path, "assets/ai/Guard.behaviortree.json");
        assert!(is_behavior_tree(&path));
        assert_eq!(behavior_tree_name(&path), "Guard");
        assert!(!is_behavior_tree(
            "assets/animations/Hero.statemachine.json"
        ));
        assert_eq!(
            behavior_tree_name("Hero.statemachine.json"),
            "Hero.statemachine.json"
        );
    }

    #[test]
    fn a_new_tree_has_one_root_and_children_wire_from_composites() {
        let mut graph = new_behavior_tree();
        assert_eq!(graph.nodes.len(), 1);
        assert_eq!(graph.nodes[0].kind, "root");
        let library = BehaviorNodes;
        let selector = graph.add_node(library.spec("selector").unwrap(), [0.0; 2]);
        let sees = graph.add_node(library.spec("sees_player").unwrap(), [0.0; 2]);
        let root = graph.nodes[0].id;
        graph
            .connect(&library, PinRef::new(root, ""), PinRef::new(selector, ""))
            .unwrap();
        graph
            .connect(&library, PinRef::new(selector, "1"), PinRef::new(sees, ""))
            .unwrap();
        // A child can't be wired back to its own parent.
        assert!(
            graph
                .connect(&library, PinRef::new(selector, "2"), PinRef::new(root, ""))
                .is_err()
        );
        assert_eq!(graph.wires.len(), 2);
    }
}
