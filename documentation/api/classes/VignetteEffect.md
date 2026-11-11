---
description: "Inside Lighting: darkens the picture toward its corners, or tints them with its Color."
---

# VignetteEffect

```luau
Instance.new("VignetteEffect")
```

Inside Lighting: darkens the picture toward its corners, or tints them with its Color. The first enabled one in Lighting is used; off at Low graphics (UserGameSettings.Vignette).

It also has every member of [Instance](Instance.md).

### Properties

#### Enabled

```luau
VignetteEffect.Enabled: boolean
```

Whether it's drawn. Off (or with no VignetteEffect in Lighting) it costs nothing.

#### Intensity

```luau
VignetteEffect.Intensity: number
```

How much of Color the corners take, from 0 to 1 (0.5 by default).

#### Size

```luau
VignetteEffect.Size: number
```

Where the darkening starts, from the middle (0) to the corners (1). 0.5 by default.

#### Color

```luau
VignetteEffect.Color: Color3
```

What the corners turn toward. Black by default.

