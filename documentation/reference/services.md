---
description: The globals every script has, the task library, and each service with what it's for, from input and tweens to saves, levels and the network.
icon: globe
samples: [hello-tween, hello-saves, hello-levels, hello-data]
---

# Services and globals

## Globals

| Global | What it is |
|---|---|
| `game` | The root of the tree (a DataModel). `game:GetService()` returns a service by name. |
| `workspace` | The Workspace: the 3D world. |
| `script` | The script that's running. |
| `require()` | Runs a ModuleScript once and returns its value. |
| `Instance.new()` | Makes a new Instance of a class. |
| `print()` | Writes to Output. |
| `task` | Waiting and scheduling: `task.wait()`, `task.spawn()`, `task.delay()`, `task.defer()`, `task.cancel()`. |
| `Enum` | Every enum, such as `Enum.KeyCode`. |
| Data type constructors | `Vector3`, `Vector2`, `CFrame`, `Color3`, `UDim2`, `Random`, `TweenInfo`, `RaycastParams`, `OverlapParams`, `PhysicalProperties`. See [Data types](data-types.md). |

The standard Luau libraries `math`, `string`, `table`, `coroutine`, `utf8`, `bit32` and `buffer` are there too, and `os.clock()` and `os.time()`.

## Services

Get one with `game:GetService("Name")`. Some are only for one side: input and the camera are on each player's device (LocalScripts), saving is on the server.

| Service | For | Main members |
|---|---|---|
| RunService | The frame loop | `RunService.Heartbeat`, `RunService.RenderStepped`, `RunService:IsServer()` |
| UserInputService | Keyboard, mouse, touch, controllers and input actions (LocalScripts) | `UserInputService.InputBegan`, `UserInputService:IsKeyDown()`, `UserInputService:IsActionPressed()` |
| ContextActionService | Binding a function to an input, with priorities | `ContextActionService:BindAction()` |
| TweenService | Animating properties smoothly | `TweenService:Create()` |
| CollectionService | Tags | `CollectionService:GetTagged()` |
| Debris | Destroying something later | `Debris:AddItem()` |
| PhysicsService | Collision groups | `PhysicsService:CollisionGroupSetCollidable()` |
| SoundService | Sound groups, occlusion | `SoundService.Occlusion` |
| Players | Everyone in the game | `Players.PlayerAdded`, `Players:GetPlayers()`, `Players.LocalPlayer` |
| Teams | Teams | — |
| SaveService | Save slots, checkpoints, per-player data and settings (server) | `SaveService:SaveSlot()`, `SaveService:LoadSlot()`, `SaveService:SetPlayerData()` |
| LevelService | Several levels in one game | `LevelService:LoadLevel()`, `LevelService.LevelLoaded` |
| DataService | Read-only data tables made in the editor | `DataService:GetTable()` |
| DialogueService, QuestService | Conversations and quests | `DialogueService:Start()`, `QuestService:Start()` |
| LocalizationService | The string table and the player's language | `LocalizationService:GetString()` |
| GuiService | Accessibility: text scale, high contrast, reduced motion | `GuiService.TextScale` |
| UserGameSettings | The player's graphics and volume settings | `UserGameSettings.GraphicsQuality`, `UserGameSettings.MasterVolume` |
| TextChatService | In-game chat | `TextChatService:SendMessage()` |
| NetworkService | Hosting and joining multiplayer games | `NetworkService:Host()`, `NetworkService:Join()`, `NetworkService:FindLanGames()` |
| ConsoleService | Commands for the Developer Console (F9) | `ConsoleService:RegisterCommand()` |
| ContentProvider | Loading assets before they're needed | `ContentProvider:PreloadAsync()` |
| ReplicatedFirst | Ending the loading screen | `ReplicatedFirst:RemoveDefaultLoadingScreen()` |
| HttpService | JSON and GUIDs (it doesn't reach the network) | `HttpService:JSONEncode()`, `HttpService:JSONDecode()` |

## A tween

```lua
local TweenService = game:GetService("TweenService")
local door = workspace.Door
local info = TweenInfo.new(1, Enum.EasingStyle.Quad, Enum.EasingDirection.Out)
local tween = TweenService:Create(door, info, { Position = door.Position + Vector3.new(0, 8, 0) })
tween:Play()
tween.Completed:Connect(function()
	print("The door is open")
end)
```

## Saving

```lua
local SaveService = game:GetService("SaveService")
SaveService:SaveSlot(1)
-- later
if SaveService:HasSlot(1) then
	SaveService:LoadSlot(1)
end
```

Save slots keep the world's state; `SaveService:SetPlayerData()` keeps a value for one player across games. The **hello-saves** sample shows both.
