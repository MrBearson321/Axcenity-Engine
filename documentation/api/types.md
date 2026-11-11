---
description: "Values that aren't Instances, such as Vector3, CFrame and Signal."
---

# Data types

Values that aren't Instances, such as Vector3, CFrame and Signal.

| Name | What it is |
|---|---|
| [Vector3](types/Vector3.md) | A position, size or direction in studs. |
| [Vector2](types/Vector2.md) | A point or size on the screen, in pixels, such as a ScrollingFrame's CanvasPosition. |
| [Color3](types/Color3.md) | A color, with each channel from 0 to 1. |
| [CFrame](types/CFrame.md) | A position and a rotation together. |
| [Random](types/Random.md) | A random number generator from `Random.new(seed)`. |
| [PhysicalProperties](types/PhysicalProperties.md) | How a part weighs, grips and bounces. |
| [UDim](types/UDim.md) | One UI axis as a scale and pixel offset. |
| [UDim2](types/UDim2.md) | A UI position or size with scale and pixel-offset components. |
| [EnumItem](types/EnumItem.md) | One choice of an enum, such as `Enum.Material.Neon`. |
| [UserInputService](types/UserInputService.md) | The player's keyboard, mouse, touch screen and game controllers, and the input map's actions, from `game:GetService("UserInputService")`. |
| [ContextActionService](types/ContextActionService.md) | Functions bound to inputs, from `game:GetService("ContextActionService")`. |
| [RunService](types/RunService.md) | The game's frame loop, from `game:GetService("RunService")`. |
| [Debris](types/Debris.md) | Destroys Instances later without waiting, from `game:GetService("Debris")`. |
| [HttpService](types/HttpService.md) | JSON and GUIDs, from `game:GetService("HttpService")`. |
| [PhysicsService](types/PhysicsService.md) | Collision groups, from `game:GetService("PhysicsService")`. |
| [RaycastParams](types/RaycastParams.md) | Which parts `workspace:Raycast`, `Spherecast`, `Blockcast` and `Shapecast` find, from `RaycastParams.new()`. |
| [OverlapParams](types/OverlapParams.md) | Which parts `workspace:GetPartBoundsInBox`, `GetPartBoundsInRadius` and `GetPartsInPart` list, from `OverlapParams.new()`. |
| [ContentProvider](types/ContentProvider.md) | Loads files ahead of time, so they don't load the moment they're first needed, from `game:GetService("ContentProvider")`. |
| [DataService](types/DataService.md) | The project's data tables (`assets/data/*.data.json`, made in the Data Table tab), from `game:GetService("DataService")`. |
| [DialogueService](types/DialogueService.md) | Dialogues (`assets/dialogue/*.dialogue.json`, made in the Dialogue tab), from `game:GetService("DialogueService")`. |
| [QuestService](types/QuestService.md) | Quests, from `game:GetService("QuestService")`: rows of the data table `quests` (Title, Description), with objectives from `quest_objectives` (Quest, Kind: Count, Reach or Talk, Target, Count, Text). |
| [ConsoleService](types/ConsoleService.md) | The game's own commands for the Developer Console (F9), from `game:GetService("ConsoleService")`. |
| [DataTable](types/DataTable.md) | A data table, read-only. |
| [DataRow](types/DataRow.md) | One row of a data table, read by column name: `row.Damage`. |
| [ReplicatedFirst](types/ReplicatedFirst.md) | What loads first on the player's side, from `game:GetService("ReplicatedFirst")`. |
| [TweenService](types/TweenService.md) | Moves properties smoothly to goals over time, from `game:GetService("TweenService")`. |
| [CollectionService](types/CollectionService.md) | Tags on Instances, from `game:GetService("CollectionService")`. |
| [SaveService](types/SaveService.md) | Save slots, player data, settings and checkpoints, from `game:GetService("SaveService")`. |
| [LevelService](types/LevelService.md) | The game's levels, from `game:GetService("LevelService")`: `world.json` is the start level, and each other level is `levels/<name>.world.json`. |
| [NetworkService](types/NetworkService.md) | Hosting and joining games, from `game:GetService("NetworkService")`, so a menu can do them. |
| [LocalizationService](types/LocalizationService.md) | The language UI text shows, from `game:GetService("LocalizationService")`. |
| [GuiService](types/GuiService.md) | The player's game UI settings, from `game:GetService("GuiService")`, for a settings menu. |
| [UserGameSettings](types/UserGameSettings.md) | This computer's graphics settings, from `game:GetService("UserGameSettings")`, for a settings menu. |
| [Tween](types/Tween.md) | Properties moving to goals, made by `TweenService:Create`. |
| [TweenInfo](types/TweenInfo.md) | How a tween plays, from `TweenInfo.new`. |
| [InputObject](types/InputObject.md) | One input, as `UserInputService`'s events and `ContextActionService`'s actions get it. |
| [RaycastResult](types/RaycastResult.md) | Where `workspace:Raycast` (or a shape cast) first met a collider, or nil when it missed. |
| [Ray](types/Ray.md) | A half-line from `Origin` along `Direction`, from `Camera:ScreenPointToRay`. |
| [Mouse](types/Mouse.md) | The mouse in the world, from `Players.LocalPlayer:GetMouse()`. |
| [Signal](types/Signal.md) | An event, such as `part.Touched`. |
| [Connection](types/Connection.md) | A function connected to an event, returned by `Connect`. |
