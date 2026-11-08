# `blocks-runtime`

- Block-script and node-graph data and their one-way translation to Lua: what games need at runtime (`project` translates with it on load). Editor-only tools such as `from_lua` live in `blocks`. Docs: `docs/scripting/blocks.md`, `docs/scripting/graphs.md`, `docs/scripting/ui-graphs.md`.
- `BlockScript` is the persisted model; `translate` (in `translate.rs`) emits readable Lua (no markers) and `block_lines`, each block's first line. Top-level blocks are event stacks or `Unattached` loose stacks (not translated); `BlockScript::graph` is the Graph view's layout and never changes the Lua.
- Statement slots accept only statement blocks; event blocks are roots, never nested.
- `node_graph.rs` is generic graph data and edits; `ui_graph.rs` the UI Event Graph's nodes and compiler, errors by node; `behavior_tree.rs` only the nodes behavior trees offer (`nav` checks and runs them; keep both in step); `dialogue.rs` the same for dialogues (`script::dialogue` runs them; `NodeLibrary::exec_loops` lets their wires loop).
- No engine-crate dependencies: it must stay safe for the player to use.
