# script

Runs Lua 5.4 (mlua 0.12, vendored) against the `core` World. Depends only on `core`. API rules: `docs/scripting.md`.

- `lib.rs`: `ScriptRuntime::start_script(world, script_id, path, source)` (runs the body as a coroutine until it finishes or waits), `step(world, seconds)` (frame step 2) and `take_output()`. The World is **moved into Lua app data** (`with_world`) while scripts run and moved back after, because Lua callbacks can't hold a `&mut World`. Callbacks use `instance::world` / `world_mut`; drop the guard before calling back into Lua.
- `sandbox.rs`: each script's environment is a fresh table built from an **allowlist** (`SAFE_FUNCTIONS`, copied library tables, clock-only `os`). Add new globals there or in `api.rs`; never expose `lua.globals()`.
- `api.rs`: `print`, `game`, `workspace`, `script`, `Instance`, `Vector3`, `Color3`, `Enum`. Created per script, so one script can't break another's.
- `instance.rs`: `LuaInstance(InstanceId)`. `__index` order: mlua checks methods first, then `Name`/`ClassName`/`Parent`, properties, children by name.
- `types.rs`: newtypes (orphan rule) for `Vector3`, `Color3`, enum items; `value_from_lua` converts by the property's existing kind. Use `f32_to_lua` so `0.1f32` reads as `0.1`.
- `errors.rs`: messages start `path:line:`. Rust callback errors take the location from mlua's traceback, skipping `[C]`, `[engine]` and `__mlua*` frames. `pcall`/`xpcall` are Lua wrappers (so yields still work) that turn mlua error objects into plain strings.
- `definitions.rs`: generates `definitions/axcenity.lua` for Lua Language Server. **Any API change** needs its description there (new properties: `PROPERTY_DOCS`), then `cargo run -p axcenity-script --bin lua-definitions`. Tests catch anything missed.
- Gotchas: mlua's `Value::type_name` says `integer` for whole numbers; show `number`. Lua's `tostring(1.0)` is `1.0`, and `print` uses the real `tostring` to match.
- `scheduler.rs`: `Scheduler` (in Lua app data) holds waiting coroutines with a due time and an order number; `step` resumes due ones, `resume` reports errors. Drop the `scheduler_mut` guard before resuming anything.
- `task.rs`: `task.wait` is Lua (only Lua can yield; mlua Rust functions can't) calling a Rust `schedule`; `spawn`/`delay` are Rust.
- Not yet: events (Phase 7), the watchdog and the shared string metatable (Phase 15).
