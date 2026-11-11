---
description: "The game's levels, from game:GetService(\"LevelService\"): world.json is the start level, and each other level is levels/<name>.world.json."
---

# LevelService

The game's levels, from `game:GetService("LevelService")`: `world.json` is the start level, and each other level is `levels/<name>.world.json`. A level is its Workspace; ReplicatedStorage, Players, Lighting, StarterPlayer and Teams stay the same from level to level. Both sides share it.

### Properties

#### CurrentLevel

```luau
LevelService.CurrentLevel: string
```

The name of the level in the Workspace now: the start level until LoadLevel loads another (in the editor's Play, the open level).

#### LevelLoaded

```luau
LevelService.LevelLoaded: Signal
```

Fires with the level's name once LoadLevel (or LoadSlot, for a slot saved in another level) has put it in the Workspace: `function(name) ... end`. Connect again to things in the new level here.

### Functions

#### LoadLevel

```luau
LevelService:LoadLevel(name: string, spawnName: string?): ()
```

Puts the level `name` in place of the Workspace, such as `LevelService:LoadLevel("Castle", "Gate")`. The Workspace's ScreenGuis and the players' characters stay; everything else in it is replaced, and the levels added with AddLevel go. Characters stand on the Instance called `spawnName` in the new level, or on one of their SpawnLocations. The loading screen shows, and LevelLoaded fires once it's in.

```lua
local LevelService = game:GetService("LevelService")
LevelService:LoadLevel("Castle", "Gate")
```

#### AddLevel

```luau
LevelService:AddLevel(name: string): Model
```

Copies the level `name` into a new Model of that name in the Workspace, beside the current level, and returns the Model. An error if it's added already.

#### RemoveLevel

```luau
LevelService:RemoveLevel(name: string): ()
```

Takes the level `name`, added with AddLevel, out of the Workspace. An error if it isn't added.

