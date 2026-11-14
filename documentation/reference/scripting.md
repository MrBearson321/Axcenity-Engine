---
description: Script kinds and where they run, how scripts are scheduled, the 3-second watchdog, the sandbox, modules, native code, errors and editor tooling.
icon: code
samples: [hello-script, hello-tasks, hello-modules, hello-remote]
---

# Scripting reference

The engine runs **Luau** (Luau 0.22, through mlua), not PUC Lua or LuaJIT. Every script, block script and graph runs in the same runtime.

## Script kinds

| Kind | File ending | Runs on | Starts in |
|---|---|---|---|
| Script | `.server.lua` (or `.server.blocks.json`) | The server, or the single-player game | ServerScriptService, or inside an object |
| LocalScript | `.client.lua` (or `.client.blocks.json`) | Each player's device | StarterPlayerScripts, or inside an object or a ScreenGui |
| ModuleScript | `.module.lua` | Wherever `require()` is called | ReplicatedStorage |

Scripts in ServerScriptService start in the order of their file paths. A script inside an object keeps its code in `objects/`, shared by every copy of the object, and each copy runs its own. Saving a script during Play runs it again in place.

## Scheduling

- A script's body and each event handler run as a **coroutine**. `task.wait()` gives the frame back and resumes the script when the time is up; `task.spawn()`, `task.delay()` and `task.defer()` start more.
- Handlers fired in the same frame run in a fixed order, so a game behaves the same each run.
- `task.cancel()` stops a task started with `task.spawn()` or `task.delay()`.

## The watchdog

A coroutine may run for **3 seconds** without yielding. After that it's stopped with an error that names the script and line, and the rest of the game carries on. A loop must `task.wait()` (or wait on an event) each time round.

## The sandbox

Scripts are untrusted code, so each gets a restricted environment:

- Available: the safe base functions, `math`, `string`, `table`, `coroutine`, `utf8`, `bit32`, `buffer`, and `os.clock`, `os.time`, `os.date` and `os.difftime`.
- Not available: files (`io`), the rest of `os`, `debug`, `load` and `loadstring`, `dofile`, loading bytecode, and `collectgarbage` beyond `"count"`.
- Standard libraries are read-only, as in Luau: replacing `math.floor` is an error.
- Scripts can't see each other's globals; share values through ModuleScripts, attributes or value Instances.

## Modules

```lua
-- ReplicatedStorage.Shared (Shared.module.lua)
local Shared = {}
function Shared.greet(name)
	return "Hello, " .. name
end
return Shared
```

```lua
local Shared = require(game.ReplicatedStorage.Shared)
print(Shared.greet("world"))
```

A module runs once per side; every `require()` of it gets the same table.

## Server and client

LocalScripts run on each player's device and handle input, the camera and UI; Scripts run the rules on the server. They talk through **RemoteEvents** (`RemoteEvent:FireServer()`, `RemoteEvent.OnServerEvent`, `RemoteEvent:FireClient()`, `RemoteEvent:FireAllClients()`, `RemoteEvent.OnClientEvent`) and **RemoteFunctions**, which wait for an answer. `RunService:IsServer()` and `RunService:IsClient()` say which side is running. The editor marks a script that uses an API from the wrong side with ⚠.

## Speed

- A script whose first line is the comment `--!native` is compiled to machine code (x86-64 and ARM64). Use it for heavy maths; it doesn't make engine calls faster.
- Prefer events to checking every frame, and `CollectionService:GetTagged()` to searching the whole tree.

## Errors

An error stops only the function it happens in. Output shows it as `path:line: message`, in plain words, and a click opens the script at that line. Wrap risky calls in `pcall()` to handle errors yourself. See [Debugging and profiling](debugging.md).

## Tooling

- The script tab highlights Lua, numbers lines and sets breakpoints. It offers completion (after `.` and `:`, or **Ctrl+Space**) and signature hints from the API description. It type-checks with luau-lsp, the Luau language server, which the editor ships with: it underlines errors and lints a moment after typing stops, and lists a saved script's errors in Output. An editor built from source offers to download the same version (a bar above the script, or **Settings > External tools**); `AXCENITY_LUAU_LSP` names another one.
- **File > Set Up VS Code Autocomplete** writes the engine's API definitions into the project for VS Code (with the Luau extension) or any editor that runs luau-lsp: completions, hovers and type checking.
- The definitions are generated from the engine's own API description, the same one the **API Reference** shows, so they always match the engine.

> **Known limits.** There's no per-script memory limit, and a single long C call (a huge `string.rep`) isn't interrupted by the watchdog. `task.defer` runs at the next step. A RemoteEvent message with no handler yet is dropped, not queued.
