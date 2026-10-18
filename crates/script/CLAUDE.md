# script

Runs Lua 5.4 (mlua 0.12, vendored) against the `core` World. Depends only on `core`. API rules: `docs/scripting.md`.

- `lib.rs`: `ScriptRuntime::run_script(world, script_id, path, source)` and `take_output()`. The World is **moved into Lua app data** while a script runs and moved back after, because Lua callbacks can't hold a `&mut World`. Callbacks use `instance::world` / `world_mut`; drop the guard before calling back into Lua.
- `sandbox.rs`: each script's environment is a fresh table built from an **allowlist** (`SAFE_FUNCTIONS`, copied library tables, clock-only `os`). Add new globals there or in `api.rs`; never expose `lua.globals()`.
- `api.rs`: `print`, `game`, `workspace`, `script`, `Instance`, `Vector3`, `Color3`, `Enum`. Created per script, so one script can't break another's.
- `instance.rs`: `LuaInstance(InstanceId)`. `__index` order: mlua checks methods first, then `Name`/`ClassName`/`Parent`, properties, children by name.
- `types.rs`: newtypes (orphan rule) for `Vector3`, `Color3`, enum items; `value_from_lua` converts by the property's existing kind. Use `f32_to_lua` so `0.1f32` reads as `0.1`.
- `errors.rs`: messages start `path:line:`. Rust callback errors take the location from mlua's traceback, skipping `[C]`, `[engine]` and `__mlua*` frames. `pcall`/`xpcall` are Lua wrappers (so yields still work) that turn mlua error objects into plain strings.
- Gotchas: mlua's `Value::type_name` says `integer` for whole numbers; show `number`. Lua's `tostring(1.0)` is `1.0`, and `print` uses the real `tostring` to match.
- Not yet: coroutines and `task.*` (Phase 5), events, the watchdog and the shared string metatable (Phase 15), the Lua definitions-file generator.
