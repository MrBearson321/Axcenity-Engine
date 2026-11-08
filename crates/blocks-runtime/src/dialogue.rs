//! Dialogues (Phase 77): the nodes a `.dialogue.json` asset offers, for the
//! generic graph editor.
//!
//! A dialogue is its own asset, `assets/dialogue/<Name>.dialogue.json`, in
//! the same file shape as any [`NodeGraph`]. White exec wires say what
//! comes next: from Start, through Lines the player reads, Choices they
//! pick from, Conditions that branch and Actions that change the game, to
//! an End, whose name `DialogueService:Start` returns. Wires may loop back
//! (a Choice that goes back to ask again). Running and checking a dialogue
//! belong to the `script` crate (`script::dialogue`); the kinds, fields and
//! pins here must match its `DIALOGUE_NODE_KINDS`.

use crate::node_graph::*;

/// Where new dialogues go.
pub const DIALOGUES_FOLDER: &str = "assets/dialogue";
/// A dialogue's file ending.
pub const DIALOGUE_SUFFIX: &str = ".dialogue.json";

/// The project path of the dialogue called `name`.
pub fn dialogue_path(name: &str) -> String {
    format!("{DIALOGUES_FOLDER}/{name}{DIALOGUE_SUFFIX}")
}

/// Whether `path` is a dialogue's file.
pub fn is_dialogue(path: &str) -> bool {
    path.to_lowercase().ends_with(DIALOGUE_SUFFIX)
}

/// A dialogue's name: its file's name without the ending.
pub fn dialogue_name(path: &str) -> &str {
    let file = path.rsplit('/').next().unwrap_or(path);
    file.get(..file.len().saturating_sub(DIALOGUE_SUFFIX.len()))
        .filter(|_| is_dialogue(file))
        .unwrap_or(file)
}

const fn text(name: &'static str, default: &'static str) -> FieldSpec {
    FieldSpec {
        name,
        kind: FieldKind::Text,
        default,
    }
}

/// Whose attribute or tag: the player's character, or the speaker.
const fn who(name: &'static str) -> FieldSpec {
    FieldSpec {
        name,
        kind: FieldKind::Choice(&["Player", "Speaker"]),
        default: "Player",
    }
}

const IN: PinSpec = PinSpec::exec("");
const NEXT: &[PinSpec] = &[PinSpec::exec("")];
const BRANCHES: &[PinSpec] = &[PinSpec::exec("True"), PinSpec::exec("False")];

/// Every node a dialogue offers, in the node menu's order.
pub const DIALOGUE_NODES: &[NodeSpec] = &[
    NodeSpec {
        kind: "start",
        title: "Start",
        category: "Start",
        help: "Where the dialogue starts. One per dialogue.",
        fields: &[],
        inputs: &[],
        outputs: NEXT,
    },
    NodeSpec {
        kind: "line",
        title: "Line",
        category: "Talk",
        help: "Shows Text said by Speaker (empty: the speaker's Model's name) until the player \
               goes on. Text and Speaker can be keys from the string table, shown in the \
               player's language. Audio plays a sound file from the speaker; Animation plays \
               that state of the speaker's Animator. A Line wired straight to a Choice shows \
               the choices under it.",
        fields: &[
            text("Speaker", ""),
            text("Text", ""),
            text("Audio", ""),
            text("Animation", ""),
        ],
        inputs: &[IN],
        outputs: NEXT,
    },
    NodeSpec {
        kind: "choice",
        title: "Choice",
        category: "Talk",
        help: "Lets the player pick an option (empty options aren't shown), under the last \
               Line. Each option goes on from its pin. Options can be string table keys.",
        fields: &[
            text("Option 1", ""),
            text("Option 2", ""),
            text("Option 3", ""),
            text("Option 4", ""),
        ],
        inputs: &[IN],
        outputs: &[
            PinSpec::exec("1"),
            PinSpec::exec("2"),
            PinSpec::exec("3"),
            PinSpec::exec("4"),
        ],
    },
    NodeSpec {
        kind: "attribute_check",
        title: "Attribute?",
        category: "Conditions",
        help: "True when the player's character (or the speaker) has the attribute, and it's \
               not false or 0; with Equals, when it's that value.",
        fields: &[who("Of"), text("Attribute", ""), text("Equals", "")],
        inputs: &[IN],
        outputs: BRANCHES,
    },
    NodeSpec {
        kind: "tag_check",
        title: "Has Tag?",
        category: "Conditions",
        help: "True when the player's character (or the speaker) has the tag.",
        fields: &[who("Of"), text("Tag", "")],
        inputs: &[IN],
        outputs: BRANCHES,
    },
    NodeSpec {
        kind: "data_check",
        title: "Data Table?",
        category: "Conditions",
        help: "True when the data table's row Key has Column, and it's not false or 0; with \
               Equals, when it's that value.",
        fields: &[
            text("Table", ""),
            text("Key", ""),
            text("Column", ""),
            text("Equals", ""),
        ],
        inputs: &[IN],
        outputs: BRANCHES,
    },
    NodeSpec {
        kind: "quest_check",
        title: "Quest?",
        category: "Conditions",
        help: "True when the quest (its key in the quests data table) is in this state.",
        fields: &[
            text("Quest", ""),
            FieldSpec {
                name: "State",
                kind: FieldKind::Choice(&["NotStarted", "Active", "Completed"]),
                default: "Active",
            },
        ],
        inputs: &[IN],
        outputs: BRANCHES,
    },
    NodeSpec {
        kind: "lua_check",
        title: "Lua Condition",
        category: "Conditions",
        help: "True when the Lua expression is truthy, such as \
               `player.leaderstats.Coins.Value >= 10`. It knows `player`, `character` and \
               `speaker`.",
        fields: &[text("Code", "")],
        inputs: &[IN],
        outputs: BRANCHES,
    },
    NodeSpec {
        kind: "set_attribute",
        title: "Set Attribute",
        category: "Actions",
        help: "Sets an attribute of the player's character (or the speaker): a number, true \
               or false, or else text.",
        fields: &[who("On"), text("Attribute", ""), text("Value", "")],
        inputs: &[IN],
        outputs: NEXT,
    },
    NodeSpec {
        kind: "give_item",
        title: "Give Item",
        category: "Actions",
        help: "Puts a copy of the Tool called Item (from ServerStorage, else \
               ReplicatedStorage) in the player character's Inventory.",
        fields: &[text("Item", "")],
        inputs: &[IN],
        outputs: NEXT,
    },
    NodeSpec {
        kind: "start_quest",
        title: "Start Quest",
        category: "Actions",
        help: "Starts the quest (its key in the quests data table), unless it's active or done.",
        fields: &[text("Quest", "")],
        inputs: &[IN],
        outputs: NEXT,
    },
    NodeSpec {
        kind: "complete_quest",
        title: "Complete Quest",
        category: "Actions",
        help: "Completes the quest, whatever its objectives say.",
        fields: &[text("Quest", "")],
        inputs: &[IN],
        outputs: NEXT,
    },
    NodeSpec {
        kind: "run_lua",
        title: "Run Lua",
        category: "Actions",
        help: "Runs Lua, such as `speaker.Door:Destroy()`. It knows `player`, `character` and \
               `speaker`, and can't wait.",
        fields: &[text("Code", "")],
        inputs: &[IN],
        outputs: NEXT,
    },
    NodeSpec {
        kind: "end",
        title: "End",
        category: "Start",
        help: "Ends the dialogue. DialogueService:Start returns its Ending.",
        fields: &[text("Ending", "End")],
        inputs: &[IN],
        outputs: &[],
    },
];

/// A dialogue's nodes, as a library for the graph editor.
pub struct DialogueNodes;

impl NodeLibrary for DialogueNodes {
    fn specs(&self) -> &[NodeSpec] {
        DIALOGUE_NODES
    }

    fn exec_loops(&self) -> bool {
        true
    }
}

/// A new dialogue: a Start wired to one Line, then an End.
pub fn new_dialogue() -> NodeGraph {
    let library = DialogueNodes;
    let mut graph = NodeGraph::default();
    let start = graph.add_node(&DIALOGUE_NODES[0], [40.0, 120.0]);
    let line = graph.add_node(library.spec("line").expect("a node"), [220.0, 120.0]);
    let end = graph.add_node(library.spec("end").expect("a node"), [520.0, 120.0]);
    if let Some(node) = graph.node_mut(line) {
        node.values.insert("Text".to_owned(), "Hello!".to_owned());
    }
    for (from, to) in [(start, line), (line, end)] {
        graph
            .connect(&library, PinRef::new(from, ""), PinRef::new(to, ""))
            .expect("a new dialogue's wires fit");
    }
    graph
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dialogue_is_named_after_its_file() {
        let path = dialogue_path("Farmer");
        assert_eq!(path, "assets/dialogue/Farmer.dialogue.json");
        assert!(is_dialogue(&path));
        assert_eq!(dialogue_name(&path), "Farmer");
        assert!(!is_dialogue("assets/ai/Guard.behaviortree.json"));
    }

    #[test]
    fn a_choice_can_go_back_to_an_earlier_line() {
        let mut graph = new_dialogue();
        let library = DialogueNodes;
        let line = graph.nodes[1].id;
        let choice = graph.add_node(library.spec("choice").unwrap(), [0.0; 2]);
        graph
            .connect(&library, PinRef::new(line, ""), PinRef::new(choice, ""))
            .unwrap();
        // Back to the line that leads to it: a loop, which dialogues allow.
        graph
            .connect(&library, PinRef::new(choice, "1"), PinRef::new(line, ""))
            .unwrap();
        // Behavior trees still refuse one.
        let mut tree = crate::behavior_tree::new_behavior_tree();
        let trees = crate::behavior_tree::BehaviorNodes;
        let root = tree.nodes[0].id;
        let sequence = tree.add_node(trees.spec("sequence").unwrap(), [0.0; 2]);
        tree.connect(&trees, PinRef::new(root, ""), PinRef::new(sequence, ""))
            .unwrap();
        assert!(
            tree.connect(&trees, PinRef::new(sequence, "1"), PinRef::new(root, ""))
                .is_err()
        );
    }

    #[test]
    fn a_new_dialogue_says_hello_and_ends() {
        let graph = new_dialogue();
        let kinds: Vec<&str> = graph.nodes.iter().map(|node| node.kind.as_str()).collect();
        assert_eq!(kinds, ["start", "line", "end"]);
        assert_eq!(graph.wires.len(), 2);
    }
}
