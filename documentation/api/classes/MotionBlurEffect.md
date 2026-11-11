---
description: "Inside Lighting: streaks what moves across the view along its motion, as a camera's shutter does: parts that move, posed characters, and everything when the camera moves."
---

# MotionBlurEffect

```luau
Instance.new("MotionBlurEffect")
```

Inside Lighting: streaks what moves across the view along its motion, as a camera's shutter does: parts that move, posed characters, and everything when the camera moves. What moves with the camera stays sharp. The first enabled one in Lighting is used. The player's graphics settings choose how finely it's drawn (UserGameSettings.MotionBlur); it's off at Low.

It also has every member of [Instance](Instance.md).

### Properties

#### Enabled

```luau
MotionBlurEffect.Enabled: boolean
```

Whether it's drawn. Off (or with no MotionBlurEffect in Lighting) it costs nothing.

#### Intensity

```luau
MotionBlurEffect.Intensity: number
```

How much of the way something moved across the view in a frame it streaks over, from 0 (none) to 1 (all of it, a shutter open the whole frame). 0.5 by default. Streaks are at most 1/18 of the view's height long.

