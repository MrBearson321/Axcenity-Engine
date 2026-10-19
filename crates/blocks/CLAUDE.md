# `blocks`

- Stores Scratch-style script blocks and translates them to the same Lua API as typed scripts.
- `BlockScript` is the persisted model; `translate` emits readable Lua (no markers) and `block_lines`, each block's first line.
- Top-level blocks are event stacks or `Unattached` loose stacks (not translated); `position` places them on the canvas.
- Statement slots accept only statement blocks; event blocks are roots, never nested. New block kinds need a row in `docs/scripting.md`'s table and in `tests.rs`'s exhaustive match.
