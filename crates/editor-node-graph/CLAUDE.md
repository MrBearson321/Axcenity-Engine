# `editor-node-graph`

- The generic node-graph editor (nodes, typed pins, wires, comments, the node menu, variables) behind the UI Event Graph, Behavior Tree, Dialogue and Material graph tabs. Open `docs/scripting/ui-graphs.md`.
- It edits any `axcenity_blocks::node_graph::NodeGraph`; the graph is the caller's, and each finished edit comes back as one undo step. The editor imports this crate as `node_graph`.
- `canvas.rs` (unified Phase 91) is the canvas every graph view shares, this crate's and the editor's block Graph view and Animation state machine: pan and zoom, box select, the shared keys, the searchable menu, pins, wires, the minimap and notices. Put new canvas behaviour there, not in one view; `graph_keys_tests.rs` in the editor checks all five views.
- Must not depend on the editor crate.
