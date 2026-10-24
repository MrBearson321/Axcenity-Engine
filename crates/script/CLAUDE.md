# `script`

- Lua 5.4 runtime over `core`. Open one file from the map in `docs/scripting.md` (API, sandbox, scheduler, or client/server).
- Build each script environment from the sandbox allowlist; never expose `lua.globals()`. Follow existing World handoff patterns and release borrows before re-entering Lua.
- For API changes, update descriptions and regenerate with `cargo run -p axcenity-script --bin lua-definitions`. New events also need core declarations and a host firing path.
- `debugger::read_frames` is the workspace's only `unsafe` code (`unsafe_code = "deny"`); keep new code safe. Every resume goes through `scheduler::resume`, so the debugger can hold it.
