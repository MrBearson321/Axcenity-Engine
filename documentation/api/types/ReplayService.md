---
description: "Recording the game to a .replay file, and keeping the last few seconds for a kill cam, from game:GetService(\"ReplayService\")."
---

# ReplayService

Recording the game to a `.replay` file, and keeping the last few seconds for a kill cam, from `game:GetService("ReplayService")`. Each device records its own world as replication shows it; nothing is simulated again, so a replay looks the same each time. Watch a file with `axcenity-player --replay <file> <game>`. Both sides have it.

### Properties

#### IsRecording

```luau
ReplayService.IsRecording: boolean
```

Read-only: true between StartRecording and StopRecording.

#### BufferSeconds

```luau
ReplayService.BufferSeconds: number
```

Keeps the last this many seconds in memory (0, the default, keeps none; at most 60), for PlayBuffer.

### Functions

#### StartRecording

```luau
ReplayService:StartRecording(): ()
```

Starts recording this device's game (again, if one was going).

#### StopRecording

```luau
ReplayService:StopRecording(name: string): ()
```

Ends the recording and saves it as `<name>.replay` in the player's data folder, under `replays` (a server's beside its saves). `name` is 1 to 64 letters, digits, spaces, `-` or `_`.

#### PlayBuffer

```luau
ReplayService:PlayBuffer(player: Player?): ()
```

Shows the last `BufferSeconds` on this device's screen, through `player`'s character's eyes if given (a kill cam), then goes back to the game; Escape ends it early. The game goes on underneath. Set BufferSeconds first.

