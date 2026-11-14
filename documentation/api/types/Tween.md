---
description: "Properties moving to goals, made by TweenService:Create."
---

# Tween

Properties moving to goals, made by `TweenService:Create`. Each frame it sets them, so their change events fire; a newer tween on the same property of the same Instance cancels it.

### Properties

#### PlaybackState

```luau
Tween.PlaybackState: EnumPlaybackState
```

Where it is: Begin, Delayed, Playing, Paused, Completed or Cancelled (read-only).

#### Completed

```luau
Tween.Completed: Signal
```

Fires when it reaches its goals, or is cancelled, with how it ended: `function(playbackState) ... end`.

```lua
local TweenService = game:GetService("TweenService")
local tween = TweenService:Create(workspace.Door, TweenInfo.new(1), { Transparency = 1 })
tween.Completed:Connect(function()
	workspace.Door.CanCollide = false
end)
tween:Play()
```

#### Instance

```luau
Tween.Instance: Instance
```

The Instance it moves (read-only).

#### TweenInfo

```luau
Tween.TweenInfo: TweenInfo
```

How it plays (read-only).

### Functions

#### Play

```luau
Tween:Play(): ()
```

Starts it from where the properties are now (after its DelayTime), or carries on after Pause.

#### Pause

```luau
Tween:Pause(): ()
```

Holds it where it is; Play carries on from there.

#### Cancel

```luau
Tween:Cancel(): ()
```

Stops it where it is, and fires Completed with `Enum.PlaybackState.Cancelled`. Playing it again starts over from there.

