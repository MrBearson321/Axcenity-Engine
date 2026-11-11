---
description: "The service that holds the SoundGroups, the game's audio buses."
---

# SoundService

```luau
game.SoundService
```

The service that holds the SoundGroups, the game's audio buses. Its groups' volumes are saved with the world in save slots. When a group is UserAdjustable, the game has an AudioSettings ScreenGui in the Workspace (made when it starts, hidden until a script sets its Enabled) with a slider for UserGameSettings.MasterVolume and one for each of those groups.

It also has every member of [Instance](Instance.md).

### Properties

#### Occlusion

```luau
SoundService.Occlusion: boolean
```

If true (the default), solid parts between a spatial sound and the camera muffle it: each wall, up to three, halves it and takes off more of the highs.

