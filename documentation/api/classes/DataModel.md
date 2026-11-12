---
description: "The root of the game, called game in scripts."
---

# DataModel

```luau
game
```

The root of the game, called `game` in scripts.

It also has every member of [Instance](Instance.md).

### Properties

#### Workspace

```luau
DataModel.Workspace: Workspace
```

The service that holds the 3D world. Parts inside it are drawn.

#### Players

```luau
DataModel.Players: Players
```

The service with a Player for everyone in the game.

#### Lighting

```luau
DataModel.Lighting: Lighting
```

The service for the world's lighting. It sets sun shadows, the sky colors that metal parts reflect, and post-processing (exposure, tone mapping and bloom).

#### ReplicatedStorage

```luau
DataModel.ReplicatedStorage: ReplicatedStorage
```

The service for things Scripts and LocalScripts share, such as RemoteEvents and ModuleScripts.

#### ServerScriptService

```luau
DataModel.ServerScriptService: ServerScriptService
```

The service that holds server scripts (`scripts/*.server.lua`). Players' devices never get what's inside.

#### ServerStorage

```luau
DataModel.ServerStorage: ServerStorage
```

The service for things only the server uses, such as Models a Script clones into the Workspace. It's saved with the world, and players' devices never get what's inside: to them it's empty.

#### StarterPlayer

```luau
DataModel.StarterPlayer: StarterPlayer
```

The service with what each player starts with: StarterPlayerScripts, and the StarterCharacter each character is a copy of.

#### Teams

```luau
DataModel.Teams: Teams
```

The service that holds the Teams players can be on.

#### SoundService

```luau
DataModel.SoundService: SoundService
```

The service that holds the SoundGroups, the game's audio buses. Its groups' volumes are saved with the world in save slots. When a group is UserAdjustable, the game has an AudioSettings ScreenGui in the Workspace (made when it starts, hidden until a script sets its Enabled) with a slider for UserGameSettings.MasterVolume and one for each of those groups.

#### TextChatService

```luau
DataModel.TextChatService: TextChatService
```

The default chat (Phase 67): a chat window at the bottom left that `/` opens, styled with `TextChatService` rules (`TextChatService::key` styles the names). SendMessage sends from a LocalScript; on the server, OnIncomingMessage sees every message and returns its text, changed or not, or nil to drop it; DisplaySystemMessage shows a message from the game. MessageReceived fires on each device as a message arrives. ChatEnabled off hides the window.

#### Debris

```luau
DataModel.Debris: Debris
```

Destroys Instances later (same as `game:GetService("Debris")`).

#### HttpService

```luau
DataModel.HttpService: HttpService
```

JSON and GUIDs (same as `game:GetService("HttpService")`).

#### ContentProvider

```luau
DataModel.ContentProvider: ContentProvider
```

Loads files ahead of time (same as `game:GetService("ContentProvider")`).

#### DataService

```luau
DataModel.DataService: DataService
```

The project's data tables (same as `game:GetService("DataService")`).

#### UserInputService

```luau
DataModel.UserInputService: UserInputService
```

The player's keyboard, mouse and controllers (same as `game:GetService("UserInputService")`). LocalScripts only.

#### ContextActionService

```luau
DataModel.ContextActionService: ContextActionService
```

Functions bound to inputs by priority (same as `game:GetService("ContextActionService")`). LocalScripts only.

#### RunService

```luau
DataModel.RunService: RunService
```

The game's frame loop, with `Stepped`, `Heartbeat` and `RenderStepped` (same as `game:GetService("RunService")`).

#### TweenService

```luau
DataModel.TweenService: TweenService
```

Moves properties smoothly to goals (same as `game:GetService("TweenService")`).

#### CollectionService

```luau
DataModel.CollectionService: CollectionService
```

Tags on Instances (same as `game:GetService("CollectionService")`).

#### SaveService

```luau
DataModel.SaveService: SaveService
```

Save slots, player data, settings and checkpoints (same as `game:GetService("SaveService")`).

#### LevelService

```luau
DataModel.LevelService: LevelService
```

The game's levels (same as `game:GetService("LevelService")`).

#### ConsoleService

```luau
DataModel.ConsoleService: ConsoleService
```

The game's Developer Console commands (same as `game:GetService("ConsoleService")`).

#### NetworkService

```luau
DataModel.NetworkService: NetworkService
```

Hosting and joining games (same as `game:GetService("NetworkService")`).

#### ReplayService

```luau
DataModel.ReplayService: ReplayService
```

Replays and kill cams (same as `game:GetService("ReplayService")`).

#### LocalizationService

```luau
DataModel.LocalizationService: LocalizationService
```

The language UI text shows (same as `game:GetService("LocalizationService")`).

#### GuiService

```luau
DataModel.GuiService: GuiService
```

The player's game UI settings (same as `game:GetService("GuiService")`).

#### UserGameSettings

```luau
DataModel.UserGameSettings: UserGameSettings
```

This computer's graphics settings (same as `game:GetService("UserGameSettings")`). LocalScripts only.

#### ReplicatedFirst

```luau
DataModel.ReplicatedFirst: ReplicatedFirst
```

What loads first on the player's side, with `RemoveDefaultLoadingScreen` (same as `game:GetService("ReplicatedFirst")`).

### Functions

#### GetService

```luau
DataModel:GetService(className: string): Instance
```

The service with this class name, such as `"Players"`, `"ReplicatedFirst"`, `"RunService"`, `"SaveService"`, `"LevelService"`, `"NetworkService"`, `"ReplayService"`, `"LocalizationService"`, `"GuiService"`, `"Debris"`, `"HttpService"`, `"ContentProvider"`, `"DataService"`, `"DialogueService"`, `"QuestService"`, `"ConsoleService"`, `"PhysicsService"`, `"TweenService"`, `"CollectionService"`, `"UserGameSettings"`, `"UserInputService"` or `"ContextActionService"`.

```lua
local TweenService = game:GetService("TweenService")
local Players = game:GetService("Players")
```

#### FindService

```luau
DataModel:FindService(className: string): Instance?
```

Like GetService, but nil when there's no service with this name, instead of an error: `if game:FindService("QuestService") then ... end`.

