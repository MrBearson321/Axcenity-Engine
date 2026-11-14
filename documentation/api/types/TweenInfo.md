---
description: "How a tween plays, from TweenInfo.new."
---

# TweenInfo

How a tween plays, from `TweenInfo.new`. Read-only.

### Properties

#### Time

```luau
TweenInfo.Time: number
```

Seconds to reach the goals.

#### EasingStyle

```luau
TweenInfo.EasingStyle: EnumEasingStyle
```

The shape of the motion: Linear, or one of the clip key easings (Sine, Quad, Cubic, Back, Bounce, Elastic).

#### EasingDirection

```luau
TweenInfo.EasingDirection: EnumEasingDirection
```

Which end of the motion is eased: In (the start), Out (the end) or InOut (both).

#### RepeatCount

```luau
TweenInfo.RepeatCount: number
```

How many more times it plays after the first; -1 repeats forever.

#### Reverses

```luau
TweenInfo.Reverses: boolean
```

Whether each play goes back to the start after reaching the goals.

#### DelayTime

```luau
TweenInfo.DelayTime: number
```

Seconds to wait after Play before it starts.

### Functions

#### new

```luau
TweenInfo.new(time: number?, easingStyle: EnumEasingStyle?, easingDirection: EnumEasingDirection?, repeatCount: number?, reverses: boolean?, delayTime: number?): TweenInfo
```

How a tween plays. Left out: 1 second, `Enum.EasingStyle.Quad`, `Enum.EasingDirection.Out`, no repeats, no reversing and no delay. The style and direction can also be item names, such as `"Sine"`.

```lua
local TweenService = game:GetService("TweenService")
local info = TweenInfo.new(2, Enum.EasingStyle.Quad, Enum.EasingDirection.Out, -1, true)
TweenService:Create(workspace.Platform, info, { Position = Vector3.new(0, 20, 0) }):Play()
```

