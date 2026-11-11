---
description: "Inside Lighting: makes the edges between colors crisper (useful with a render scale below 100%)."
---

# SharpenEffect

```luau
Instance.new("SharpenEffect")
```

Inside Lighting: makes the edges between colors crisper (useful with a render scale below 100%). The first enabled one in Lighting is used; off at Low graphics (UserGameSettings.Sharpen).

It also has every member of [Instance](Instance.md).

### Properties

#### Enabled

```luau
SharpenEffect.Enabled: boolean
```

Whether it's drawn. Off (or with no SharpenEffect in Lighting) it costs nothing.

#### Intensity

```luau
SharpenEffect.Intensity: number
```

How much crisper the edges between colors get, from 0 (no change) to 1 (strong). 0.5 by default.

