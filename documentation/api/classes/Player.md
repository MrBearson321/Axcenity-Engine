---
description: "Someone playing the game, in Players."
---

# Player

Someone playing the game, in Players. Made when they join.

It also has every member of [Instance](Instance.md).

### Properties

#### Character

```luau
Player.Character: Instance?
```

The character Model the player plays as, or nil. Set by spawning and `LoadCharacter`; a character placed in the Workspace before Play becomes the player's.

#### Team

```luau
Player.Team: Instance?
```

The Team the player is on, or nil. Setting it also sets Neutral (false on a team, true on nil). Players join the AutoAssignable Team with the fewest players.

#### Neutral

```luau
Player.Neutral: boolean
```

If true, the player is on no team, and spawns only on Neutral SpawnLocations.

#### UserId

```luau
Player.UserId: number
```

Who plays: a number that stays the same each time they play on one install of Axcenity, so a server can recognize them when they join again. Read-only.

#### DisplayName

```luau
Player.DisplayName: string
```

The name the player joined with. `Name` is the same, but numbered when someone in the game already has it (`Player2`). Read-only.

### Events

#### CharacterAdded

```luau
Player.CharacterAdded: Signal
```

Fires with the character Model once it's in the Workspace: when the player first gets one, after each respawn and after `LoadCharacter`.

```lua
local Players = game:GetService("Players")
Players.PlayerAdded:Connect(function(player)
	player.CharacterAdded:Connect(function(character)
		print(player.Name, "spawned as", character.Name)
	end)
end)
```

#### CharacterRemoving

```luau
Player.CharacterRemoving: Signal
```

Fires with the old character Model just before it's destroyed to make way for a new one. It's already out of the Workspace (its Parent is nil).

### Functions

#### GetMouse

```luau
Player:GetMouse(): Mouse
```

LocalScripts only: the mouse in the world, with the part under it (`Target`). LocalScripts only.

#### Kick

```luau
Player:Kick(reason: string?): ()
```

Scripts only (Phase 67): takes the player out of the game. Their device's NetworkService.ConnectionFailed fires with `reason` ("You were kicked from the game." without one), and PlayerRemoving fires here. Kicking the player on this device ends the game, with the reason. Scripts only.

#### RequestStreamAroundAsync

```luau
Player:RequestStreamAroundAsync(position: Vector3): ()
```

Loads what's around `position` for this player and waits until it's there, before a teleport (world streaming, Phase 80). With streaming off it carries on at once. What's loaded stays for 10 seconds, so teleport soon after: `player:RequestStreamAroundAsync(spot) character:PivotTo(CFrame.new(spot))`.

#### LoadCharacter

```luau
Player:LoadCharacter(): ()
```

Scripts only: gives the player a new character. The old one (if any) leaves the Workspace, fires CharacterRemoving and is destroyed; a copy of the StarterCharacter stands on one of the player's team's SpawnLocations (or a Neutral one), chosen at random, with a ForceField for its Duration, and CharacterAdded fires. With no SpawnLocation it stands at the origin and Output warns. Scripts only.

