# Graphs

The Graph view: a block script shown as Unreal Blueprint-style nodes and wires. It's a view of the same blocks, not a third language, so a script switches between Code, Blocks and Graph at any time and runs the same (one Lua runtime; see [blocks.md](blocks.md) for the Lua each block becomes).

## The view

- The bar above a Script or LocalScript's tab picks **Code**, **Blocks** or **Graph**. Code ↔ Blocks or Graph replaces the file (`.server.lua` ↔ `.server.blocks.json`), as before. Blocks ↔ Graph is the same file: the file remembers which view it was in (`graph.shown`), saved straight away unless the script has other unsaved changes.
- Each block is a node, titled in Unreal style (`Move To`, `Branch`, `Play Sound`). Its header has its block category's colour; a node that isn't wired to an event (a loose stack's blocks, a loose value node) has a greyed header and doesn't run, like loose blocks.
- **Exec wires** (white, dark grey on the light theme) say what runs next. An event has one exec output: the first block of its stack. A block's output is the block after it. C-shaped blocks have one output per mouth and then **Completed** for the blocks after them: Repeat and Repeat Until have **Loop Body**, Forever has only **Loop Body**, if and if/else are one **Branch** node with **True**, **False** and **Completed**. Wiring False gives an if an else; breaking it (leaving the else empty) makes it an if again. Stop This Script has no output.
- **Value nodes** are the reporters (`Math`, `Compare`, `Get Property`, `Get score` for a variable, ...). Their output wires into an input pin. A value typed into a pin (a number, text, true or false, a Vector3, a colour) stays on the pin as an inline box, as Unreal shows defaults. Menus (objects, properties, keys, operators, Animator names) are rows on the node. Pin colours: green numbers, pink text, red true or false, gold vectors, blue colours, grey for any value.
- Any Luau script becomes real nodes ([blocks.md](blocks.md)). Top-level statements are one chain from a **Script Runs** node, and a top-level `Connect` is its own **On Signal** event node. A method call's node is named after the method. A name such as `row.Binding.Text` is typed on the pin, not a node of its own.
- A function passed as a value (`task.delay(1, function() … end)`, `local f = function() … end`) is a **Function** exec output on the node it's in, with its parameters on the node; the blocks inside it run from that wire.
- The little Lua with no blocks (a type definition, say) shows as grey **Lua** nodes (switch to Code to change it).
- **View as Lua** shows the Lua beside the graph. A runtime error outlines its block's node in red.

## Editing

- Right-click the canvas, or **Add node**, for the searchable node menu (every palette block and reporter, by category). Dropping a wire on the canvas opens it filtered to nodes that fit the wire, and the new node is wired to it.
- Drag from a pin to another to wire them; it works from either end. Alt-click a pin, or right-click it > Break link, to break its wires.
- An exec input takes one wire: wiring an output to a node moves that node and the blocks after it there (they're a stack), and leaves what was wired to the output before loose. A wire that would loop back into its own chain is refused, with a note at the bottom of the canvas; use a loop node instead.
- A value node feeds one input. Wiring a loose value node moves it into the input; wiring one that's already wired copies it. That runs the same as Unreal's fan-out, since a value node is worked out again wherever it's used. A wire into an input that doesn't fit is refused (a condition needs a red true-or-false value).
- Right-click a call, local, return or table-of-arguments node for **Add Input** and **Remove Last Input**.
- Click a node to select it (Shift or Ctrl adds), left-drag the background for a box, drag a node to move the selection. Delete or Backspace deletes the selected nodes, Ctrl+D duplicates them; right-click a node for Duplicate and Delete. Deleting a node keeps what it was wired to, loose.
- Right- or middle-drag (or the wheel) pans; Ctrl + wheel or pinching zooms. **Clean up** lays every node out again.

## Layout and file

- Before a node is placed, it's laid out (`layout.rs`): exec flows left to right in a row, a block's value nodes sit to its left below its exec pins, and what runs in a mouth is a row below. Nodes don't overlap.
- The first edit in the graph stores every node's place, so rewiring never moves nodes. Places are in `graph.nodes`, by key: a block's id (`"12"`), or a value node's owner id and slots (`"12.0.1"`). Value nodes wired to nothing are in `graph.loose`, each with its own id from the blocks' ids. None of this changes the Lua. A block script never shown as a graph has no `graph` key, so older files and saves are unchanged.

## Code

- Data: `axcenity_blocks::Graph` and `LooseValue` (`crates/blocks-runtime/src/lib.rs`).
- Editor: `crates/editor/src/block_editor/graph.rs` (drawing and the pointer), `graph/edit.rs` (what wiring does to the blocks), `graph/nodes.rs` (titles, pin names, colours), `graph/layout.rs`, `graph/menu.rs`. A new block kind needs a title and pin names in `nodes.rs` (`every_block_and_reporter_has_a_node_with_every_pin_named` checks them).
- Switching: `EditorApp::switch_script_view` (`crates/editor/src/app/files/script_files.rs`).
