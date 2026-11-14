---
description: "Asks the server and waits for the answer: a LocalScript's InvokeServer(...) pauses until the server's OnServerInvoke returns, and returns what it returned."
---

# RemoteFunction

```luau
Instance.new("RemoteFunction")
```

Asks the server and waits for the answer: a LocalScript's `InvokeServer(...)` pauses until the server's `OnServerInvoke` returns, and returns what it returned. An error there is raised in the LocalScript. Keep it in ReplicatedStorage.

It also has every member of [Instance](Instance.md).

### Properties

#### OnServerInvoke

```luau
RemoteFunction.OnServerInvoke: (Player, ...any) -> ...any
```

The function that answers `InvokeServer`: `remote.OnServerInvoke = function(player, ...) return ... end`. Setting it replaces the one before; it can be set, not read. Questions asked before it's set wait for it. Scripts only.

```lua
game.ReplicatedStorage.GetCoins.OnServerInvoke = function(player)
	return player:GetAttribute("Coins") or 0
end
```

#### OnClientInvoke

```luau
RemoteFunction.OnClientInvoke: (...any) -> ...any
```

The function that answers a Script's `InvokeClient`: `remote.OnClientInvoke = function(...) return ... end`. Setting it replaces the one before; it can be set, not read. Questions asked before it's set wait for it. LocalScripts only.

### Functions

#### InvokeServer

```luau
RemoteFunction:InvokeServer(...: any): ...any
```

Asks the server: pauses until its `OnServerInvoke` returns (it gets this player first, then the arguments), and returns what it returned. An error there is raised here, with the server script's line. LocalScripts only. LocalScripts only.

```lua
-- In a LocalScript: ask the server and wait for its answer.
local coins = game.ReplicatedStorage.GetCoins:InvokeServer()
print("I have", coins, "coins")
```

#### InvokeClient

```luau
RemoteFunction:InvokeClient(player: Player, ...: any): ...any
```

Asks one player's device: pauses until its `OnClientInvoke` returns (it gets the arguments), and returns what it returned. An error there is raised here, with the LocalScript's line. A player who leaves first never answers, so the Script waits. Scripts only.

