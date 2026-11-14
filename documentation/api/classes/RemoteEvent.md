---
description: "Carries messages between Scripts and LocalScripts."
---

# RemoteEvent

```luau
Instance.new("RemoteEvent")
```

Carries messages between Scripts and LocalScripts. Keep it in ReplicatedStorage.

It also has every member of [Instance](Instance.md).

### Events

#### OnServerEvent

```luau
RemoteEvent.OnServerEvent: Signal
```

Fires when a LocalScript calls `FireServer`. The handler gets the Player who sent it, then the arguments: `function(player, ...) ... end`. Scripts only.

```lua
local shout = game.ReplicatedStorage.Shout
shout.OnServerEvent:Connect(function(player, message)
	print(player.Name, "says", message)
end)
```

#### OnClientEvent

```luau
RemoteEvent.OnClientEvent: Signal
```

Fires when a Script calls `FireClient` or `FireAllClients`. The handler gets the arguments. LocalScripts only.

```lua
-- In a LocalScript.
game.ReplicatedStorage.ShowMessage.OnClientEvent:Connect(function(text)
	print("The server says:", text)
end)
```

### Functions

#### FireServer

```luau
RemoteEvent:FireServer(...: any): ()
```

Sends the arguments to the server, where `OnServerEvent` fires with this player first. LocalScripts only.

```lua
-- In a LocalScript
local shout = game.ReplicatedStorage.Shout
shout:FireServer("Hello")
```

#### FireClient

```luau
RemoteEvent:FireClient(player: Player, ...: any): ()
```

Sends the arguments to one player, where `OnClientEvent` fires. Scripts only.

```lua
-- On the server: tell one player's device to show a message.
local message = game.ReplicatedStorage.ShowMessage
game:GetService("Players").PlayerAdded:Connect(function(player)
	message:FireClient(player, "Welcome!")
end)
```

#### FireAllClients

```luau
RemoteEvent:FireAllClients(...: any): ()
```

Sends the arguments to every player, where `OnClientEvent` fires. Scripts only.

