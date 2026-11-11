---
description: "The service with a Player for everyone in the game."
---

# Players

```luau
game.Players
```

The service with a Player for everyone in the game.

It also has every member of [Instance](Instance.md).

### Properties

#### CharacterAutoLoads

```luau
Players.CharacterAutoLoads: boolean
```

If true, each player gets a character as they join (unless one is already in the Workspace, which becomes theirs) and a new one RespawnTime seconds after their character's Humanoid dies. If false, characters come only from `LoadCharacter`.

#### RespawnTime

```luau
Players.RespawnTime: number
```

Seconds between a character's Humanoid dying and its player getting a new character, while CharacterAutoLoads is on.

#### LeaderboardEnabled

```luau
Players.LeaderboardEnabled: boolean
```

If false, the default leaderboard isn't drawn, for a game that makes its own. It shows each Player's `leaderstats` Folder of Value objects, top right, and Tab shows and hides it.

#### MaxPlayers

```luau
Players.MaxPlayers: number
```

How many players a hosted game takes, the host's own player among them. A player who joins a full game is refused, and their `NetworkService.ConnectionFailed` says the game is full.

#### LocalPlayer

```luau
Players.LocalPlayer: Player
```

The player on this device. LocalScripts only.

### Events

#### PlayerAdded

```luau
Players.PlayerAdded: Signal
```

Fires with the Player when someone joins, after they're put on a team and before their first character spawns.

```lua
local Players = game:GetService("Players")
Players.PlayerAdded:Connect(function(player)
	print(player.DisplayName, "joined")
end)
```

#### PlayerRemoving

```luau
Players.PlayerRemoving: Signal
```

Fires with the Player when someone leaves a hosted game, just before their Player (and character) are destroyed.

### Functions

#### GetPlayers

```luau
Players:GetPlayers(): {Player}
```

Every Player in the game.

