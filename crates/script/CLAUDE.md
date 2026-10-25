# `script`

- Luau runtime over `core` (`luau0-src` is pinned to match `mlua`; see `docs/scripting/language.md`). Open one file from the map in `docs/scripting.md` (API, sandbox, scheduler, or client/server). `workspace:Raycast` uses `physics`; hosts call `ScriptRuntime::set_assets`.
- `definitions.rs` writes the luau-lsp file `definitions/axcenity.d.luau` from `definitions/tables.rs` (Luau type syntax) and `definitions/docs.rs`.
- Build each script environment from the sandbox allowlist; never expose `lua.globals()`. Follow existing World handoff patterns and release borrows before re-entering Lua.
- For API changes, update descriptions and regenerate with `cargo run -p axcenity-script --bin lua-definitions`. New events also need core declarations and a host firing path.
- `debugger.rs` holds the workspace's only `unsafe` code (Luau's step hook and `read_frames`; `unsafe_code = "deny"`); keep new code safe. Every resume goes through `scheduler::resume`, so the debugger can hold it.
