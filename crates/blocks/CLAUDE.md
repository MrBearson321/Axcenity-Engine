# `blocks`

- Stores Scratch-style script blocks and translates them to the same Lua API as typed scripts.
- `BlockScript` is the persisted model; `translate` (in `translate.rs`) emits readable Lua (no markers) and `block_lines`, each block's first line.
- Top-level blocks are event stacks or `Unattached` loose stacks (not translated); `position` places them on the canvas. `BlockScript::graph` is the Graph view's layout and loose value nodes (`docs/scripting/graphs.md`); it never changes the Lua.
- Statement slots accept only statement blocks; event blocks are roots, never nested. New block kinds need a row in `docs/scripting/blocks.md` and in `tests.rs`'s exhaustive match.
- `from_lua` (with the Luau reader in `lua.rs`) turns code back into blocks; whatever has no block, including Luau type annotations, stays word for word in grey Lua blocks.
