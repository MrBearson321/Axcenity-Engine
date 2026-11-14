---
description: Luau scripts, where they run (server, client, modules), the API's main ideas, events, waiting, talking between server and client, and finding API entries.
icon: code
samples: [hello-script, hello-events, hello-tasks, hello-modules, hello-remote]
---

# Scripting and the API

Games are scripted in **Luau**, a fast, typed dialect of Lua. The API will feel familiar if you know Roblox: `workspace`, `game:GetService()`, Instances with properties, events you `:Connect()` to, and `task.wait()`. Blocks and graphs (see [Blocks: visual programming](blocks.md)) become the same Lua, so there's one scripting runtime.

## Where scripts live

| Kind | File | Runs | Use it for |
|---|---|---|---|
| Script | `scripts/Name.server.lua` | On the server (or the single-player host) | Game rules, scores, spawning. |
| LocalScript | `scripts/Name.client.lua` | On each player's side | Input, the camera, UI. |
| ModuleScript | `scripts/Name.module.lua` | Wherever it's required | Code shared between scripts. |

Make one with the **Script**, **LocalScript** or **ModuleScript** button on the ribbon, or **Insert ▸ Script** on an Explorer row. A Script starts in **ServerScriptService**, a LocalScript in **StarterPlayerScripts** and a ModuleScript in **ReplicatedStorage**. With a part or model selected, a script can also live *inside* that object and travel with its copies. Double-click a script to open it in a tab.

The script tab highlights Lua, numbers its lines and stays editable during Play: saving a script while the game runs runs it again in place. A ⚠ in the Explorer marks a script that uses something from the wrong side (a LocalScript changing scores, say).

While you type, the tab helps:

- **Completion**: typing `.` or `:` lists what can follow, and **Ctrl+Space** lists what fits the word you're typing (keywords, your locals, globals). Up and Down pick, **Enter** or **Tab** puts it in, **Escape** closes the list. It follows types through chains such as `game:GetService("Players").LocalPlayer` and a part found by name in the Workspace, and through your own locals.
- **Signature hints**: typing `(` or `,` in an engine call shows its parameters, with the current one highlighted.
- **Errors and warnings**: each script is type-checked by **luau-lsp**, the Luau language server that comes with the editor, a moment after you stop typing. (An editor built from source asks to download it the first time you open a script.) Errors get a red underline and warnings, such as an unused local, an amber one; hover for the message. A line above the code counts them, and a saved script's errors also show in Output. Without luau-lsp there are no underlines.

To work in VS Code instead, **File > Set Up VS Code Autocomplete** sets the project up for VS Code (or any editor that runs luau-lsp) with the engine's API definitions; the editor picks up changes saved there.

## Instances, properties and events

Everything in a game is an **Instance**: parts, models, lights, sounds, UI. Find them by name through their parents, read and set their properties, and connect to their events.

```lua
local door = workspace.House.Door
door.Transparency = 0.5
door.Color = Color3.fromRGB(200, 60, 40)

local button = workspace.House:FindFirstChild("Button")
if button then
	button.Touched:Connect(function(other)
		door.CanCollide = false
	end)
end
```

New objects come from `Instance.new()`: set their properties, then their **Parent** to put them in the world.

```lua
local ball = Instance.new("Part")
ball.Shape = Enum.PartType.Ball
ball.Position = Vector3.new(0, 20, 0)
ball.Parent = workspace
```

## Waiting and timing

Never loop without waiting: a script that doesn't yield stops the game. Use the `task` library:

- `task.wait(seconds)` pauses this script (a frame with no number).
- `task.spawn()` runs a function alongside, and `task.delay()` runs one later.
- `RunService.Heartbeat` fires every frame with the time since the last.

```lua
while true do
	task.wait(1)
	print("A second passed")
end
```

## Services

Engine systems are **services**: `game:GetService("TweenService")`, `"UserInputService"`, `"SoundService"`, `"SaveService"` and more. See [Services and globals](../reference/services.md) for the list.

## Server and client

In a multiplayer game the server owns the world, and each player's LocalScripts handle their input and screen. They talk through **RemoteEvents** in ReplicatedStorage: a LocalScript calls `RemoteEvent:FireServer()` and a Script hears `RemoteEvent.OnServerEvent`, or the other way with `RemoteEvent:FireClient()` and `RemoteEvent.OnClientEvent`. In a single-player game the same code works unchanged. The **hello-remote** sample shows both sides.

## Finding your way in the API

- The **API Reference** (start screen, or **Help > Learn > API**) lists every class, property, method, event, data type and enum, with its description.
- The script tab's completion and signature hints, and VS Code after **File > Set Up VS Code Autocomplete**, come from the same definitions.
- **Ctrl+Shift+F** finds text in every script and block script.
- Names in `code` on these pages link to their API entry: click `Instance:FindFirstChild()` or `task.wait()` to look them up.

> **Familiar, not identical.** Axcenity uses Roblox-style names so tutorials and habits carry over, but it isn't a copy: some classes are Axcenity's own (Animator state machines, Effects, NavigationAgents), and some Roblox APIs don't exist. The API Reference lists exactly what's there.
