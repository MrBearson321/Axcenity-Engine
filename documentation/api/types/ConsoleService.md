---
description: "The game's own commands for the Developer Console (F9), from game:GetService(\"ConsoleService\")."
---

# ConsoleService

The game's own commands for the Developer Console (F9), from `game:GetService("ConsoleService")`. A LocalScript's commands run on that device, a Script's on the server.

### Properties

#### AllowedUserIds

```luau
ConsoleService.AllowedUserIds: {number}
```

On the server: the `UserId`s of the players who may run its commands from another device, such as `{123}`. The device that hosts, or plays offline, always may.

### Functions

#### RegisterCommand

```luau
ConsoleService:RegisterCommand(name: string, help: string, callback: (player: Player?, ...string) -> ()): ()
```

Adds the command `name` (one word), with `help` for the console's `help` list. Typed as `name arg1 arg2`, it calls `callback(player, "arg1", "arg2")` in a task of its own; `player` is who typed it. Registering a name again replaces it.

```lua
local ConsoleService = game:GetService("ConsoleService")
ConsoleService:RegisterCommand("heal", "Fills everyone's health", function(player)
	print(player.Name, "asked to heal")
end)
```

