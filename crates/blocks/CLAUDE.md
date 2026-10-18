# `blocks`

- Stores Scratch-style script blocks and translates them to the same Lua API as typed scripts.
- `BlockScript` is the persisted model; `translate` emits readable Lua and block-to-line locations.
- Statement slots accept only statement blocks; event blocks are roots, never nested.
