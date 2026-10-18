# Scripting (Lua and blocks)

Read before working in the `script` or `blocks` crates, or adding anything to the Lua API.

## Language

- **Lua 5.4** via `mlua` (vendored). Not Luau, not LuaJIT.
- Types for tooling come from a generated **Lua Language Server definitions file** (`---@class`, `---@param` annotations), checked in at `definitions/axcenity.lua`. The generator is `crates/script/src/definitions.rs`: classes, properties and enums come from `core`, while functions, globals and value types are described there by hand. Every public API addition must update it, then run `cargo run -p axcenity-script --bin lua-definitions`. Tests fail (so CI fails) if a property has no description, if a global or library function is missing from the descriptions, or if the checked-in file is out of date.

## API style: Roblox-familiar, not Roblox-compatible

- Reuse familiar names and patterns: `Instance.new("Part")`, `part.Parent = workspace`, `part.Touched:Connect(fn)`, `Vector3.new`, `task.wait` / `task.spawn` / `task.delay`.
- Classes are PascalCase, properties are PascalCase, methods are called with `:`, events are `RBXScriptSignal`-like objects with `:Connect` (but don't name anything with `RBX`).
- Clean up Roblox quirks when it helps beginners. Don't chase full compatibility.
- Never use Roblox's name, logos or assets.

## API so far (Phase 7)

- Globals: `game`, `workspace`, `script` (the running Script), `print`, `Instance.new(className)`, `Vector3.new(x, y, z)`, `Color3.new(r, g, b)`, `Color3.fromRGB(r, g, b)`, `Enum` and `task` (see Execution model).
- Instances: `Name`, `ClassName` (read-only), `Parent` (an Instance or `nil`), every property in `core`'s class table, and `:FindFirstChild(name, recursive?)`.
- **Children by name** (decided in Phase 4): `workspace.Block` returns the first child named `Block`. Methods and properties win over children with the same name. A name that's none of these errors with `X is not a valid member of Workspace "Workspace"`.
- **Enums** (decided in Phase 4): `Enum.Material.Neon` and `Enum.PartType.Ball`. Enum properties also accept the item's name as a string (`part.Material = "Neon"`); a wrong name errors and lists the choices. Reading always gives an enum item, which prints as `Enum.Material.Neon` and has `.Name`.
- `Vector3`: `.X .Y .Z .Magnitude .Unit`, `+ -`, `*` and `/` by a number or another Vector3, unary `-`, `==`. Prints as `1, 2, 3`.
- `Color3`: `.R .G .B`, `==`. `Color3.new` rejects channels outside 0 to 1 with a hint to use `fromRGB`, matching the world file's check (Roblox allows them silently).
- Numbers are Lua 5.4 numbers, so `print(part.Position.Y)` shows `5.0`.
- **Events** (decided in Phase 7): `part.Touched` is a Signal. `signal:Connect(fn)` returns a Connection with `:Disconnect()` (calling it twice is harmless) and `.Connected`. `Touched` handlers get the other part. Event names come from `ClassName::events()` in `core`; `instance.Key` checks them after properties and before children, and assigning to one explains how to connect instead. Connections to a destroyed Instance are dropped at the next step. `:Once` and `:Wait` aren't built yet (Phase 16 at the earliest).
- `Part.Anchored` (default `false`): unanchored parts fall under physics. Setting `Position` or `Orientation` on one teleports it and stops it; physics writes both back as the part moves. See Physics in `docs/architecture.md`.

## Errors

- Every message starts with `path:line:`, using the path from the project folder (`scripts/build.server.lua:3: Part.Size must be a Vector3, not a number`). Engine errors take the line from the Lua call stack.
- Scripts see the same plain strings through `pcall` and `xpcall`, which are thin Lua wrappers around the real ones, so pausing inside a `pcall` will still work.
- Output is a list of `OutputLine`s (`Print` or `Error`) that the host takes with `take_output()`: the player prints them, and the editor's Output panel will show them.

## Sandbox

Each script gets its own restricted environment. Remove or replace `io`, `os` (keep only safe time functions), `debug`, `load`, `loadfile`, `dofile`, `require` (replaced by an engine module loader), and `collectgarbage` beyond safe options. Treat all script code as untrusted.

- Since Phase 4 the environment is built from an **allowlist** (`crates/script/src/sandbox.rs`): the safe base functions, per-script copies of `coroutine`, `math`, `string` (without `dump`), `table` and `utf8`, and `os.clock/date/difftime/time`. `_G` is the script's own environment, and only source text loads, never bytecode.
- Still open for Phase 15: the string metatable is shared (`getmetatable("").__index`), the module `require`, and a limited `collectgarbage`.

## Execution model

- Every script body and every event handler runs as a **coroutine**.
- The **scheduler** (`crates/script/src/scheduler.rs`) owns all paused coroutines. `task.wait(t)` yields, and the scheduler resumes it once `t` has passed (frame order step 2, `ScriptRuntime::step`).
- **The clock is game time**, the sum of the frame times the host passes to `step`, not the computer's clock. The player caps a frame at 0.1 s, so after a stall waiting scripts don't all fire at once. Tests drive the clock exactly.
- `task` (decided in Phase 5, matching Roblox):
  - `task.wait(seconds?)` returns the seconds that really passed. Left out, 0, negative or NaN means "until the next step", so `while true do task.wait() end` runs once a frame. A task is never resumed in the step that scheduled it.
  - `task.spawn(fn, ...)` runs `fn` **right away** until it finishes or waits, then the caller continues. Returns the coroutine.
  - `task.delay(seconds, fn, ...)` starts `fn` once `seconds` have passed (at the next step at the earliest). Returns the coroutine.
  - Both take functions only, not coroutines. `task.cancel` and `task.defer` don't exist yet.
- **Resume order**: earliest due first; ties in the order they were scheduled. A task within 1 µs of due counts as due, so float rounding in summed frame times never costs a whole frame. A `task.wait(1)` loop still drifts by up to a frame per wait when frames are uneven (as in Roblox); use the returned time for exact timing.
- `task.wait` inside a script's own coroutine (`coroutine.wrap`) pauses just that coroutine and returns control to whoever resumed it; the scheduler resumes it later. It also works inside `pcall`.
- No legacy `wait`, `spawn` or `delay` globals (decided in Phase 5): only `task.*`.
- Event handlers fired in the same frame run in a deterministic order: touches in the order physics found them (sorted pairs per step), then for each pair the first part's handlers and then the other's, each in the order they were connected. Handlers run **immediately** when the event fires (step 4), each as a new coroutine, so a handler that calls `task.wait` pauses only itself and the scheduler resumes it later. A handler disconnected by an earlier handler in the same firing doesn't run. An error in a handler is reported and the handler stays connected, as in Roblox.
- **Watchdog**: an instruction-count hook aborts a coroutine that runs too long without yielding, and reports an error with the script name and line.
- Errors in one script never stop other scripts. Report them to Output with the script path and line. An error in a `task.spawn` or `task.delay` function is reported the same way and doesn't stop the script that started it.

## Server and client

- `Script` runs on the server side. `LocalScript` runs on the client side (input, camera, UI).
- `RemoteEvent` sends messages server ↔ client. v1 uses an **in-memory loopback transport**, so keep the transport behind a trait so a real network can replace it.
- Client-only APIs (input, camera, local UI) error in a `Script` with a helpful message, and vice versa.
- File naming: `*.server.lua` and `*.client.lua`.

## Blocks to Lua

- Blocks are Scratch-style (vertical snapping, shape-typed slots). There is no separate runtime: **blocks always translate to Lua**.
- Generated Lua must be **readable** (it's shown by "View as Lua") and carry **source-block tags**, so errors and highlighting map back to block IDs.
- One-way only. Don't build Lua-to-blocks conversion.
- Every block must correspond to a real Lua API. Don't add block-only features.
