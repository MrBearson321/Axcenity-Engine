---
description: "Inside Lighting: shafts of light streaming from the sun past whatever is in front of it, such as bright sky between leaves or a window, worked out on screen from what's drawn (so they show only while the sun is in or near the view)."
---

# SunRaysEffect

```luau
Instance.new("SunRaysEffect")
```

Inside Lighting: shafts of light streaming from the sun past whatever is in front of it, such as bright sky between leaves or a window, worked out on screen from what's drawn (so they show only while the sun is in or near the view). The first enabled one in Lighting is used; off at Low graphics (UserGameSettings.SunRays).

It also has every member of [Instance](Instance.md).

### Properties

#### Enabled

```luau
SunRaysEffect.Enabled: boolean
```

Whether it's drawn. Off (or with no SunRaysEffect in Lighting) it costs nothing.

#### Intensity

```luau
SunRaysEffect.Intensity: number
```

How bright the shafts are, from 0 (none) to 1 (strong). 0.25 by default.

#### Spread

```luau
SunRaysEffect.Spread: number
```

How far the shafts reach from the sun across the picture, from 0 (a glow around it) to 1 (the whole way across). 0.5 by default.

