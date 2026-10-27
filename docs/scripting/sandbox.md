# Sandbox

What a script is allowed to call.

Each script gets its own restricted environment. Remove or replace `io`, `os` (keep only safe time functions), `debug`, `load`, `loadfile`, `dofile`, `require` (replaced by an engine module loader), and `collectgarbage` beyond safe options. Treat all script code as untrusted.

- Since Phase 4 the environment is built from an **allowlist** (`crates/script/src/sandbox.rs`): the safe base functions, per-script copies of `bit32`, `buffer`, `coroutine`, `math`, `string` (without `dump`), `table` and `utf8`, and `os.clock/date/difftime/time`. `bit32` and `buffer` only work on numbers and their own byte buffers, with every read and write bounds-checked. `typeof` is the engine's (`types::typeof_name`): every Instance is `"Instance"`, enum items `"EnumItem"`, `Enum.Material` is `"Enum"` and `Enum` `"Enums"`, as in Roblox; other engine values are named as scripts know them (`"Vector3"`, `"Vector2"`, `"Color3"`, `"UDim2"`, `"CFrame"`, `"Random"`, `"Signal"`, `"Connection"`, `"RunService"`, `"Camera"`, ...), and anything else is Luau's own (`"number"`, `"buffer"`, ...). `_G` is the script's own environment, and only source text loads (mode `t`), never bytecode: Luau doesn't verify bytecode, so loading it could corrupt memory. mlua adds `loadstring` and a file-system `require` to Luau's real globals; neither is on the allowlist.

### Hardened in Phase 15

- **String metatable**: every string shares one metatable, whose `__index` was the real `string` table. It's now a hidden copy without `dump`, and `getmetatable("")` returns `"The metatable is locked"` (as in Roblox). A script's `string.upper = ...` only changes its own copy.
- **`collectgarbage`** only takes `"count"` (memory in use, in KB). Anything else errors: the engine runs the collector for everyone.
- **`setmetatable`** refuses a metatable with `__gc`. Luau never runs `__gc` on tables, so the error says so instead of ignoring it silently.
- **`coroutine.resume/wrap/status/close`** are engine versions (`watchdog.rs`): errors come back as plain `path:line:` strings like `pcall`'s, `wrap` doesn't add a second location, and a coroutine the watchdog stopped looks dead and can't be resumed or closed.
- Engine objects' metatables (Instances, Vector3, ...) are locked by mlua: `getmetatable(workspace)` is `false`; `sandbox_tests.rs` checks this and every other escape attempt (missing `io`/`debug`/`load`/`package`/..., bytecode, `os` beyond the clock, cross-script and cross-side globals, `require` of paths and libraries).
- **Not covered** (out of scope for v1, noted for later): memory limits (a script can allocate until the process runs out), and C functions that run long by themselves (a huge `string.rep`, a pathological pattern in `string.find`): the watchdog only runs at Luau calls and loop iterations.
