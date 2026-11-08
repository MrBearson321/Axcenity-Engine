# `blocks`

- The block editor's side of block scripts: re-exports `blocks-runtime` (the data and its translation to Lua) and adds `from_lua`. The player never depends on this crate.
- `from_lua` (with the Luau reader in `lua.rs`) turns any Luau into blocks, checking each one writes the same code (`lua::same_code`); what no block can write stays word for word in grey Lua blocks. It only uses `blocks-runtime`'s public API.
- `luau_tests.rs` holds the acceptance scripts and sweeps every `.lua` in the repo. `tests.rs` and `tween_tests.rs` run translated scripts (and round trips through `from_lua`) in the real runtime; new block kinds need a row in `tests.rs`'s exhaustive match and in `docs/scripting/blocks.md`.
