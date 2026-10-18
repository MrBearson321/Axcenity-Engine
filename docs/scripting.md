# Scripting (Lua and blocks)

Read before working in the `script` or `blocks` crates, or adding anything to the Lua API.

## Language

- **Lua 5.4** via `mlua` (vendored). Not Luau, not LuaJIT.
- Types for tooling come from a generated **Lua Language Server definitions file** (`---@class`, `---@param` annotations). Every public API addition must update the generator.

## API style: Roblox-familiar, not Roblox-compatible

- Reuse familiar names and patterns: `Instance.new("Part")`, `part.Parent = workspace`, `part.Touched:Connect(fn)`, `Vector3.new`, `task.wait` / `task.spawn` / `task.delay`.
- Classes are PascalCase, properties are PascalCase, methods are called with `:`, events are `RBXScriptSignal`-like objects with `:Connect` (but don't name anything with `RBX`).
- Clean up Roblox quirks when it helps beginners. Don't chase full compatibility.
- Never use Roblox's name, logos or assets.

## Sandbox

Each script gets its own restricted environment. Remove or replace `io`, `os` (keep only safe time functions), `debug`, `load`, `loadfile`, `dofile`, `require` (replaced by an engine module loader), and `collectgarbage` beyond safe options. Treat all script code as untrusted.

## Execution model

- Every script body and every event handler runs as a **coroutine**.
- The **scheduler** owns all paused coroutines. `task.wait(t)` yields, and the scheduler resumes it once `t` has passed (frame order step 2).
- Event handlers fired in the same frame run in a deterministic order.
- **Watchdog**: an instruction-count hook aborts a coroutine that runs too long without yielding, and reports an error with the script name and line.
- Errors in one script never stop other scripts. Report them to Output with the script path and line.

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
