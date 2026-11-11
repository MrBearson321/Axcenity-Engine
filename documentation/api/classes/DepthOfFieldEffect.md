---
description: "Inside Lighting: blurs what's nearer or farther than the distance in focus, as a camera's lens does: sharp from FocusDistance - InFocusRadius to FocusDistance + InFocusRadius studs away, blurring toward the camera by NearIntensity and away from it by FarIntensity."
---

# DepthOfFieldEffect

```luau
Instance.new("DepthOfFieldEffect")
```

Inside Lighting: blurs what's nearer or farther than the distance in focus, as a camera's lens does: sharp from FocusDistance - InFocusRadius to FocusDistance + InFocusRadius studs away, blurring toward the camera by NearIntensity and away from it by FarIntensity. The first enabled one in Lighting is used. The player's graphics settings choose how finely it's drawn (UserGameSettings.DepthOfField); it's off at Low.

It also has every member of [Instance](Instance.md).

### Properties

#### Enabled

```luau
DepthOfFieldEffect.Enabled: boolean
```

Whether it blurs anything. Off (or with no DepthOfFieldEffect in Lighting) it costs nothing.

#### FocusDistance

```luau
DepthOfFieldEffect.FocusDistance: number
```

How far from the camera, in studs, the middle of what's sharp is (20 by default).

#### InFocusRadius

```luau
DepthOfFieldEffect.InFocusRadius: number
```

How far either side of FocusDistance stays sharp, in studs (10 by default).

#### NearIntensity

```luau
DepthOfFieldEffect.NearIntensity: number
```

How blurred what's nearer than the sharp range gets, from 0 (not at all) to 1, growing toward the camera. 0.75 by default.

#### FarIntensity

```luau
DepthOfFieldEffect.FarIntensity: number
```

How blurred what's farther than the sharp range gets, from 0 (not at all) to 1, reached at twice the sharp range's far edge, the sky included. 0.75 by default.

