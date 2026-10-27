# `blocks`

- The block editor's side of block scripts: re-exports `blocks-runtime` (the data and its translation to Lua) and adds `from_lua`. The player never depends on this crate.
- `from_lua` (with the Luau reader in `lua.rs`) turns code back into blocks; whatever has no block, including Luau type annotations, stays word for word in grey Lua blocks. It only uses `blocks-runtime`'s public API.
- `tests.rs` and `tween_tests.rs` run translated scripts (and round trips through `from_lua`) in the real runtime; new block kinds need a row in `tests.rs`'s exhaustive match and in `docs/scripting/blocks.md`.
