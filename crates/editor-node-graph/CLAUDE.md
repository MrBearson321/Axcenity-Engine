# `editor-node-graph`

- The generic node-graph editor (nodes, typed pins, wires, comments, the node menu, variables) behind the UI Event Graph, Behavior Tree, Dialogue and Material graph tabs. Open `docs/scripting/ui-graphs.md`.
- It edits any `axcenity_blocks::node_graph::NodeGraph`; the graph is the caller's, and each finished edit comes back as one undo step. The editor imports this crate as `node_graph`.
- Must not depend on the editor crate.
