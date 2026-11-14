---
description: "Save slots, player data, settings and checkpoints, from game:GetService(\"SaveService\")."
---

# SaveService

Save slots, player data, settings and checkpoints, from `game:GetService("SaveService")`. Slots are files in the player's data for this game, never in the project, so saving during Play doesn't change the project. Both sides share the same data.

### Functions

#### SaveSlot

```luau
SaveService:SaveSlot(slot: number): ()
```

Writes save slot `slot` (1 to 999): the player data, the world state (every part and model in the Workspace, and Lighting; not scripts or ScreenGuis), the active checkpoint and the settings. Replaces what the slot held.

```lua
local SaveService = game:GetService("SaveService")
SaveService:SaveSlot(1)
if SaveService:HasSlot(1) then
	SaveService:LoadSlot(1)
end
```

#### LoadSlot

```luau
SaveService:LoadSlot(slot: number): ()
```

Puts the game back as slot `slot` was saved: player data, world state, the active checkpoint and settings. Parts and models still in the world keep being the same Instances. A slot that doesn't exist, or can't be read, is an error and changes nothing.

#### HasSlot

```luau
SaveService:HasSlot(slot: number): boolean
```

True if slot `slot` has been saved.

#### SetPlayerData

```luau
SaveService:SetPlayerData(name: string, value: any): ()
```

Keeps `value` for the player under `name`, such as `SaveService:SetPlayerData("Coins", 10)`: a boolean, number, string, Vector3, Vector2, Color3, UDim2 or EnumItem. nil removes it. Saved in slots and captured at checkpoints.

```lua
local SaveService = game:GetService("SaveService")
game:GetService("Players").PlayerRemoving:Connect(function(player)
	SaveService:SetPlayerData(player, "Coins", player:GetAttribute("Coins") or 0)
end)
```

#### GetPlayerData

```luau
SaveService:GetPlayerData(name: string): any
```

The player data kept under `name`, or nil.

#### SetSetting

```luau
SaveService:SetSetting(name: string, value: any): ()
```

Keeps a game setting under `name`, such as `SaveService:SetSetting("Volume", 0.5)`, with the same values as SetPlayerData. Saved in slots, but not captured at checkpoints. The `UI/` names are the game UI settings (see GuiService), and `Audio/MasterVolume` is `UserGameSettings.MasterVolume`.

#### GetSetting

```luau
SaveService:GetSetting(name: string): any
```

The setting kept under `name`, or nil.

#### SetCheckpoint

```luau
SaveService:SetCheckpoint(checkpoint: Checkpoint): ()
```

Makes `checkpoint` the active Checkpoint, and captures the player data and world state now, for RestoreCheckpoint.

#### GetCheckpoint

```luau
SaveService:GetCheckpoint(): Checkpoint?
```

The active Checkpoint, or nil.

#### RestoreCheckpoint

```luau
SaveService:RestoreCheckpoint(): ()
```

Puts back the player data and world state captured at the active checkpoint, and stands the player character (the one whose controller owns the camera) at its Position. An error if no checkpoint is active.

