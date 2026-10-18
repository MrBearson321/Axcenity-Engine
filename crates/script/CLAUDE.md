# `script`

- Lua 5.4 runtime over `core`; see `docs/scripting.md` for API, sandbox, scheduling and event semantics.
- Build each script environment from the sandbox allowlist; never expose `lua.globals()`. Follow existing World handoff patterns and release borrows before re-entering Lua.
- For API changes, update descriptions and regenerate with `cargo run -p axcenity-script --bin lua-definitions`. New events also need core declarations and a host firing path.
