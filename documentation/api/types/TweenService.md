---
description: "Moves properties smoothly to goals over time, from game:GetService(\"TweenService\")."
---

# TweenService

Moves properties smoothly to goals over time, from `game:GetService("TweenService")`.

### Functions

#### Create

```luau
TweenService:Create(instance: Instance, tweenInfo: TweenInfo, goals: { [string]: any }): Tween
```

A Tween that moves each property in `goals` (a table such as `{Position = Vector3.new(0, 5, 0)}`) of `instance` from where it is to its goal, as `tweenInfo` says. Numbers, Vector3s, Color3s, UDim2s and booleans (which switch at the end) can be tweened. Call `Play` to start it.

```lua
local TweenService = game:GetService("TweenService")
local door = workspace.Door
local info = TweenInfo.new(1, Enum.EasingStyle.Quad, Enum.EasingDirection.Out)
local tween = TweenService:Create(door, info, { Position = door.Position + Vector3.new(0, 8, 0) })
tween:Play()
```

